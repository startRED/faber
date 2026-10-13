//! System keep-awake management: keeps the system awake while CLI work runs.
//!
//! Windows execution state uses `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)`.
//! The display is never kept awake (`ES_DISPLAY_REQUIRED` is never set).
//! A single counted guard tracks active runs and ensures that overlapping runs
//! cannot release early or leak.
#![allow(unsafe_code)]

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub trait PowerKeepAwake: Send + Sync {
    fn set_awake(&self, awake: bool);
}

#[cfg(windows)]
pub struct WindowsPower;

#[cfg(windows)]
impl PowerKeepAwake for WindowsPower {
    fn set_awake(&self, awake: bool) {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn SetThreadExecutionState(esFlags: u32) -> u32;
        }
        const ES_CONTINUOUS: u32 = 0x8000_0000;
        const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;

        // SAFETY: SetThreadExecutionState is a thread-safe Windows power API.
        unsafe {
            if awake {
                SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
            } else {
                SetThreadExecutionState(ES_CONTINUOUS);
            }
        }
    }
}

pub struct NoopPower;

impl PowerKeepAwake for NoopPower {
    fn set_awake(&self, _awake: bool) {}
}

/// Fake power implementation for unit tests and automated runs.
#[allow(dead_code)]
pub struct FakePower {
    pub awake: AtomicBool,
    pub calls: AtomicUsize,
}

#[allow(dead_code)]
impl FakePower {
    pub fn new() -> Self {
        Self { awake: AtomicBool::new(false), calls: AtomicUsize::new(0) }
    }

    pub fn is_awake(&self) -> bool {
        self.awake.load(Ordering::SeqCst)
    }

    pub fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Default for FakePower {
    fn default() -> Self {
        Self::new()
    }
}

impl PowerKeepAwake for FakePower {
    fn set_awake(&self, awake: bool) {
        self.awake.store(awake, Ordering::SeqCst);
        self.calls.fetch_add(1, Ordering::SeqCst);
    }
}

/// A counted guard that tracks active runs and ensures the OS stays awake as long as at least
/// one run is active and the setting is enabled.
pub struct CountedAwake {
    power: Box<dyn PowerKeepAwake>,
    enabled: bool,
    active_runs: HashSet<String>,
    is_awake: bool,
}

impl CountedAwake {
    pub fn new(power: Box<dyn PowerKeepAwake>, enabled: bool) -> Self {
        Self { power, enabled, active_runs: HashSet::new(), is_awake: false }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;
            self.apply();
        }
    }

    #[allow(dead_code)]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    #[allow(dead_code)]
    pub fn start_run(&mut self, run_id: impl Into<String>) {
        self.active_runs.insert(run_id.into());
        self.apply();
    }

    #[allow(dead_code)]
    pub fn end_run(&mut self, run_id: &str) {
        self.active_runs.remove(run_id);
        self.apply();
    }

    pub fn sync_runs(&mut self, current_run_ids: HashSet<String>) {
        self.active_runs = current_run_ids;
        self.apply();
    }

    pub fn shutdown(&mut self) {
        self.active_runs.clear();
        self.apply();
    }

    #[allow(dead_code)]
    pub fn active_count(&self) -> usize {
        self.active_runs.len()
    }

    #[allow(dead_code)]
    pub fn is_awake(&self) -> bool {
        self.is_awake
    }

    fn apply(&mut self) {
        let should_be_awake = self.enabled && !self.active_runs.is_empty();
        if should_be_awake != self.is_awake {
            self.is_awake = should_be_awake;
            self.power.set_awake(should_be_awake);
        }
    }
}

impl Drop for CountedAwake {
    fn drop(&mut self) {
        if self.is_awake {
            self.power.set_awake(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct SharedFake(Arc<FakePower>);

    impl PowerKeepAwake for SharedFake {
        fn set_awake(&self, awake: bool) {
            self.0.set_awake(awake);
        }
    }

    #[test]
    fn single_run_acquires_and_releases_awake() {
        let fake = Arc::new(FakePower::new());
        let mut guard = CountedAwake::new(Box::new(SharedFake(fake.clone())), true);
        assert!(!guard.is_awake());
        assert!(!fake.is_awake());

        guard.start_run("turn-1");
        assert!(guard.is_awake());
        assert!(fake.is_awake());
        assert_eq!(fake.call_count(), 1);

        guard.end_run("turn-1");
        assert!(!guard.is_awake());
        assert!(!fake.is_awake());
        assert_eq!(fake.call_count(), 2);
    }

    #[test]
    fn overlapping_runs_keep_system_awake_until_last_run_ends() {
        let fake = Arc::new(FakePower::new());
        let mut guard = CountedAwake::new(Box::new(SharedFake(fake.clone())), true);

        guard.start_run("turn-1");
        assert_eq!(fake.call_count(), 1);
        assert!(fake.is_awake());

        // Overlapping run starts
        guard.start_run("turn-2");
        assert_eq!(fake.call_count(), 1, "does not redundantly call when already awake");
        assert!(fake.is_awake());

        // First run ends: second run keeps it awake!
        guard.end_run("turn-1");
        assert!(guard.is_awake());
        assert!(fake.is_awake());
        assert_eq!(fake.call_count(), 1, "does not release early while turn-2 is active");

        // Second run ends: now released
        guard.end_run("turn-2");
        assert!(!guard.is_awake());
        assert!(!fake.is_awake());
        assert_eq!(fake.call_count(), 2);
    }

    #[test]
    fn turning_off_setting_releases_awake_immediately_even_with_active_runs() {
        let fake = Arc::new(FakePower::new());
        let mut guard = CountedAwake::new(Box::new(SharedFake(fake.clone())), true);

        guard.start_run("turn-1");
        assert!(fake.is_awake());

        // User turns off toggle in Settings
        guard.set_enabled(false);
        assert!(!guard.is_awake());
        assert!(!fake.is_awake());

        // New runs while disabled do not awake
        guard.start_run("turn-2");
        assert!(!fake.is_awake());

        // User turns toggle back on while runs are active
        guard.set_enabled(true);
        assert!(fake.is_awake());
    }

    #[test]
    fn shutdown_and_drop_release_awake() {
        let fake = Arc::new(FakePower::new());
        {
            let mut guard = CountedAwake::new(Box::new(SharedFake(fake.clone())), true);
            guard.start_run("turn-1");
            assert!(fake.is_awake());
        }
        // Dropped
        assert!(!fake.is_awake());
    }

    #[test]
    fn crash_or_sync_runs_drops_vanished_runs() {
        let fake = Arc::new(FakePower::new());
        let mut guard = CountedAwake::new(Box::new(SharedFake(fake.clone())), true);

        let mut runs = HashSet::new();
        runs.insert("turn-1".to_string());
        guard.sync_runs(runs);
        assert!(fake.is_awake());

        // Run crashes: engine clears running items
        guard.sync_runs(HashSet::new());
        assert!(!fake.is_awake());
    }
}

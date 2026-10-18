//! System keep-awake management: keeps the system awake while CLI work runs.
//!
//! Windows execution state uses `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)`.
//! The display is never kept awake (`ES_DISPLAY_REQUIRED` is never set).
//! A single counted guard tracks active runs and ensures that overlapping runs
//! cannot release early or leak.
#![allow(unsafe_code)]

use std::collections::HashSet;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub trait PowerKeepAwake: Send + Sync {
    fn set_awake(&self, awake: bool) -> Result<(), String>;
}

#[cfg(windows)]
pub struct WindowsPower;

#[cfg(windows)]
impl PowerKeepAwake for WindowsPower {
    fn set_awake(&self, awake: bool) -> Result<(), String> {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn SetThreadExecutionState(esFlags: u32) -> u32;
        }
        const ES_CONTINUOUS: u32 = 0x8000_0000;
        const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;

        // SAFETY: No pointers; the guard owns the calling thread for the request lifetime.
        let flags = if awake { ES_CONTINUOUS | ES_SYSTEM_REQUIRED } else { ES_CONTINUOUS };
        if unsafe { SetThreadExecutionState(flags) } == 0 { Err("SetThreadExecutionState failed".into()) } else { Ok(()) }
    }
}

pub struct NoopPower;

impl PowerKeepAwake for NoopPower {
    fn set_awake(&self, _awake: bool) -> Result<(), String> {
        Ok(())
    }
}

/// Fake power implementation for unit tests.
#[cfg(test)]
pub struct FakePower {
    pub awake: AtomicBool,
    pub calls: AtomicUsize,
}

#[cfg(test)]
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

#[cfg(test)]
impl Default for FakePower {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl PowerKeepAwake for FakePower {
    fn set_awake(&self, awake: bool) -> Result<(), String> {
        self.awake.store(awake, Ordering::SeqCst);
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

type PowerCommand = (bool, std::sync::mpsc::Sender<Result<(), String>>);

/// A counted guard that holds execution state on one thread until the last run ends.
pub struct CountedAwake {
    power: std::sync::Arc<dyn PowerKeepAwake>,
    commands: Option<std::sync::mpsc::Sender<PowerCommand>>,
    worker: Option<std::thread::JoinHandle<()>>,
    stopped: bool,
    enabled: bool,
    active_runs: HashSet<String>,
    is_awake: bool,
}

impl CountedAwake {
    pub fn new(power: Box<dyn PowerKeepAwake>, enabled: bool) -> std::io::Result<Self> {
        let mut guard = Self {
            power: power.into(),
            commands: None,
            worker: None,
            stopped: false,
            enabled,
            active_runs: HashSet::new(),
            is_awake: false,
        };
        guard.start_worker()?;
        Ok(guard)
    }

    fn start_worker(&mut self) -> std::io::Result<()> {
        let (commands, updates) = std::sync::mpsc::channel::<PowerCommand>();
        let power = self.power.clone();
        // Execution state is thread-local, so acquisition, release and unwind cleanup share one owner.
        let worker = std::thread::Builder::new().name("faber-power".into()).stack_size(64 * 1024).spawn(move || {
            struct Release(std::sync::Arc<dyn PowerKeepAwake>);
            impl Drop for Release {
                fn drop(&mut self) {
                    let _ = self.0.set_awake(false);
                }
            }
            let power = Release(power);
            for (awake, answer) in updates {
                let _ = answer.send(power.0.set_awake(awake));
            }
        })?;
        self.commands = Some(commands);
        self.worker = Some(worker);
        Ok(())
    }

    fn end_worker(&mut self) {
        self.commands.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;
            self.apply();
        }
    }

    #[cfg(test)]
    pub fn start_run(&mut self, run_id: impl Into<String>) {
        self.active_runs.insert(run_id.into());
        self.apply();
    }

    #[cfg(test)]
    pub fn end_run(&mut self, run_id: &str) {
        self.active_runs.remove(run_id);
        self.apply();
    }

    pub fn sync_runs(&mut self, current_run_ids: HashSet<String>) {
        self.active_runs = current_run_ids;
        self.apply();
    }

    pub fn shutdown(&mut self) {
        self.stopped = true;
        self.active_runs.clear();
        self.apply();
    }

    #[cfg(any(test, feature = "test-hooks"))]
    pub fn is_awake(&self) -> bool {
        self.is_awake
    }

    fn apply(&mut self) {
        let should_be_awake = !self.stopped && self.enabled && !self.active_runs.is_empty();
        if should_be_awake != self.is_awake {
            if self.commands.is_none()
                && let Err(error) = self.start_worker()
            {
                eprintln!("faber: the system power owner could not start: {error}");
                return;
            }
            let (answer, result) = std::sync::mpsc::channel();
            let applied = self.commands.as_ref().is_some_and(|commands| commands.send((should_be_awake, answer)).is_ok())
                && result.recv().is_ok_and(|result| result.is_ok());
            if applied {
                self.is_awake = should_be_awake;
            } else {
                eprintln!("faber: the system power request failed");
                // Ending the owner clears its thread-local request even if the explicit clear failed.
                self.end_worker();
                self.is_awake = false;
            }
        }
    }
}

impl Drop for CountedAwake {
    fn drop(&mut self) {
        self.shutdown();
        self.end_worker();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn acquisition_and_release_use_the_same_thread_across_callers() {
        struct ThreadPower(Arc<std::sync::Mutex<Vec<std::thread::ThreadId>>>);
        impl PowerKeepAwake for ThreadPower {
            fn set_awake(&self, _awake: bool) -> Result<(), String> {
                self.0.lock().unwrap().push(std::thread::current().id());
                Ok(())
            }
        }
        let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let guard = Arc::new(std::sync::Mutex::new(CountedAwake::new(Box::new(ThreadPower(calls.clone())), true).unwrap()));
        let start = guard.clone();
        std::thread::spawn(move || start.lock().unwrap().start_run("chat")).join().unwrap();
        guard.lock().unwrap().end_run("chat");
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], calls[1], "execution state belongs to its calling thread");
    }

    struct SharedFake(Arc<FakePower>);

    impl PowerKeepAwake for SharedFake {
        fn set_awake(&self, awake: bool) -> Result<(), String> {
            self.0.set_awake(awake)
        }
    }

    #[test]
    fn single_run_acquires_and_releases_awake() {
        let fake = Arc::new(FakePower::new());
        let mut guard = CountedAwake::new(Box::new(SharedFake(fake.clone())), true).unwrap();
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
        let mut guard = CountedAwake::new(Box::new(SharedFake(fake.clone())), true).unwrap();

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
        let mut guard = CountedAwake::new(Box::new(SharedFake(fake.clone())), true).unwrap();

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
            let mut guard = CountedAwake::new(Box::new(SharedFake(fake.clone())), true).unwrap();
            guard.start_run("turn-1");
            assert!(fake.is_awake());
        }
        // Dropped
        assert!(!fake.is_awake());
    }

    #[test]
    fn crash_or_sync_runs_drops_vanished_runs() {
        let fake = Arc::new(FakePower::new());
        let mut guard = CountedAwake::new(Box::new(SharedFake(fake.clone())), true).unwrap();

        let mut runs = HashSet::new();
        runs.insert("turn-1".to_string());
        guard.sync_runs(runs);
        assert!(fake.is_awake());

        // Run crashes: engine clears running items
        guard.sync_runs(HashSet::new());
        assert!(!fake.is_awake());
    }

    #[test]
    fn shutdown_cannot_be_reacquired_by_late_engine_events() {
        let fake = Arc::new(FakePower::new());
        let mut guard = CountedAwake::new(Box::new(SharedFake(fake.clone())), true).unwrap();
        guard.start_run("chat");
        guard.shutdown();
        guard.sync_runs(HashSet::from(["project".into()]));
        guard.set_enabled(false);
        guard.set_enabled(true);
        assert!(!fake.is_awake());
    }

    #[test]
    fn unwinding_a_counted_guard_releases_the_request() {
        let fake = Arc::new(FakePower::new());
        let calls = fake.clone();
        let result = std::panic::catch_unwind(move || {
            let mut guard = CountedAwake::new(Box::new(SharedFake(calls)), true).unwrap();
            guard.start_run("chat");
            panic!("simulated owner panic");
        });
        assert!(result.is_err());
        assert!(!fake.is_awake());
    }

    #[test]
    fn a_failed_power_request_is_not_counted_as_applied_and_can_be_retried() {
        struct RefusesOnce(Arc<FakePower>);
        impl PowerKeepAwake for RefusesOnce {
            fn set_awake(&self, awake: bool) -> Result<(), String> {
                if self.0.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    return Err("simulated power failure".into());
                }
                self.0.awake.store(awake, Ordering::SeqCst);
                Ok(())
            }
        }
        let fake = Arc::new(FakePower::new());
        let mut guard = CountedAwake::new(Box::new(RefusesOnce(fake.clone())), true).unwrap();
        guard.start_run("chat");
        assert!(!guard.is_awake());
        guard.sync_runs(HashSet::from(["chat".into()]));
        assert!(fake.is_awake());
        guard.end_run("chat");
        assert!(!fake.is_awake());
    }

    #[test]
    fn a_failed_release_ends_the_owner_and_the_next_run_can_acquire_again() {
        struct RefusesReleaseOnce(Arc<FakePower>, AtomicBool);
        impl PowerKeepAwake for RefusesReleaseOnce {
            fn set_awake(&self, awake: bool) -> Result<(), String> {
                if !awake && !self.1.swap(true, Ordering::SeqCst) {
                    return Err("simulated release failure".into());
                }
                self.0.set_awake(awake)
            }
        }
        let fake = Arc::new(FakePower::new());
        let power = RefusesReleaseOnce(fake.clone(), AtomicBool::new(false));
        let mut guard = CountedAwake::new(Box::new(power), true).unwrap();
        guard.start_run("chat");
        guard.end_run("chat");
        assert!(!fake.is_awake(), "a failed release must close its owner thread");
        guard.start_run("project");
        assert!(fake.is_awake());
        guard.end_run("project");
        assert!(!fake.is_awake());
    }
}

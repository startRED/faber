//! When the PC should stay awake: while a turn runs, if the setting is on. This is only the policy; asking the system to stay
//! awake (an execution state on Windows, a power-save blocker in another shell) is the shell's, so every shell keeps the
//! PC awake under the same rules. The display is never kept awake.

use std::collections::HashSet;

/// The runs that count and the switches that can end them all: awake is wanted only while something runs, the setting is
/// on and the program is not closing.
#[derive(Debug, Clone)]
pub struct Policy {
    enabled: bool,
    /// The program is closing: nothing keeps the PC awake again, however late an event or a setting arrives.
    stopped: bool,
    runs: HashSet<String>,
}

impl Policy {
    pub fn new(enabled: bool) -> Self {
        Self { enabled, stopped: false, runs: HashSet::new() }
    }

    /// The "keep the PC awake" setting as the policy has it.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn start_run(&mut self, run_id: impl Into<String>) {
        self.runs.insert(run_id.into());
    }

    pub fn end_run(&mut self, run_id: &str) {
        self.runs.remove(run_id);
    }

    /// The runs that are going on now, which replace the ones counted so far: a run that vanished (a crash clears the
    /// engine's running list) stops counting.
    pub fn sync_runs(&mut self, current_run_ids: HashSet<String>) {
        self.runs = current_run_ids;
    }

    pub fn shutdown(&mut self) {
        self.stopped = true;
        self.runs.clear();
    }

    /// Whether the PC should be kept awake now.
    pub fn wanted(&self) -> bool {
        !self.stopped && self.enabled && !self.runs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn awake_is_wanted_while_any_run_goes_on_and_not_before_or_after() {
        let mut policy = Policy::new(true);
        assert!(!policy.wanted());
        policy.start_run("turn-1");
        policy.start_run("turn-2");
        assert!(policy.wanted());
        policy.end_run("turn-1");
        assert!(policy.wanted(), "turn-2 still goes on");
        policy.end_run("turn-2");
        assert!(!policy.wanted());
    }

    #[test]
    fn the_setting_ends_the_request_at_once_and_brings_it_back_with_runs_going() {
        let mut policy = Policy::new(true);
        policy.start_run("turn-1");
        policy.set_enabled(false);
        assert!(!policy.wanted());
        policy.start_run("turn-2");
        assert!(!policy.wanted(), "a run that starts while the setting is off does not keep the PC awake");
        policy.set_enabled(true);
        assert!(policy.wanted());
        assert!(!Policy::new(false).wanted());
    }

    #[test]
    fn a_run_that_vanished_from_the_list_stops_counting() {
        let mut policy = Policy::new(true);
        policy.sync_runs(HashSet::from(["turn-1".to_string()]));
        assert!(policy.wanted());
        policy.sync_runs(HashSet::new());
        assert!(!policy.wanted());
    }

    #[test]
    fn nothing_keeps_the_pc_awake_after_shutdown() {
        let mut policy = Policy::new(true);
        policy.start_run("chat");
        policy.shutdown();
        assert!(!policy.wanted());
        policy.sync_runs(HashSet::from(["project".to_string()]));
        policy.set_enabled(false);
        policy.set_enabled(true);
        assert!(!policy.wanted(), "late events cannot bring it back");
    }
}

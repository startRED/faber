//! A subagent that seems stuck (R161 step 6). The engine follows each subagent of the running reply through the steps it
//! reports and marks its call only when it matters: one waiting on the person's answer is "waiting for you", never stuck,
//! and the signs of a stuck one are the same step started again and again, the same check failing after a retry with
//! nothing changed in between, or no step at all for longer than its kind of work allows. Neither CLI lets Faber speak to
//! the main agent while it waits for its subagent, so the main agent's own chance is counted in the numbers: a failing
//! check shows only once its retry also failed. Pure: the engine feeds the steps and the time, and reads the sign.

use crate::model::ToolUse;
use serde::Serialize;
use std::collections::HashMap;

/// What a subagent's row says beside "working".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Sign {
    /// It asked the person something and waits for the answer.
    Waiting,
    /// It started the same step, with the same input, this many times in a row.
    Repeat,
    /// The same step failed, was run again with no file changed, and failed again.
    Failing,
    /// No step started or ended for longer than its work allows.
    Silent,
}

impl Sign {
    /// A sign that the person should look: everything but waiting for them.
    pub fn stuck(self) -> bool {
        self != Sign::Waiting
    }
}

/// The same step started this many times in a row. A subagent that polls (a build, a page) does so a few times; six with
/// nothing else between is a loop.
pub const REPEATS: u32 = 6;
/// The same step failing this many times with no file changed between: failed twice, retried once, failed again.
pub const FAILS: u32 = 3;
/// Seconds with no step while nothing runs: thinking, or writing its report.
pub const QUIET: f64 = 600.0;
/// Seconds with no step while a command runs: builds and test suites take long and say nothing until they end.
pub const QUIET_COMMAND: f64 = 1800.0;

/// One subagent at work, as far as its steps tell.
#[derive(Debug, Clone)]
pub struct Child {
    /// When it last started or ended a step, or the person last answered it.
    last: f64,
    /// The step it started last, and how many times in a row.
    action: Option<String>,
    repeats: u32,
    /// How many times each step failed since a file last changed or the step last passed.
    failed: HashMap<String, u32>,
    /// The steps still running: their key, and whether it is a command.
    open: HashMap<String, (String, bool)>,
}

fn key(tool: &ToolUse) -> String {
    format!("{}\u{0}{}", tool.name, tool.input)
}

fn is_command(name: &str) -> bool {
    matches!(name, "Bash" | "PowerShell" | "commandExecution")
}

impl Child {
    pub fn new(now: f64) -> Self {
        Child { last: now, action: None, repeats: 0, failed: HashMap::new(), open: HashMap::new() }
    }

    pub fn started(&mut self, tool: &ToolUse, now: f64) {
        self.last = now;
        let key = key(tool);
        if self.action.as_ref() == Some(&key) {
            self.repeats += 1;
        } else {
            self.action = Some(key.clone());
            self.repeats = 1;
        }
        self.open.insert(tool.id.clone(), (key, is_command(&tool.name)));
    }

    /// A step ended; `tool` carries its outcome. A refused step is the person's answer, not a failure of the work.
    pub fn ended(&mut self, tool: &ToolUse, now: f64) {
        self.last = now;
        let key = self.open.remove(&tool.id).map_or_else(|| key(tool), |(key, _)| key);
        if tool.declined {
            return;
        }
        if tool.ok == Some(false) {
            *self.failed.entry(key).or_default() += 1;
        } else {
            self.failed.remove(&key);
            if !crate::nudge::edited(tool).is_empty() {
                self.failed.clear();
            }
        }
    }

    /// The time the person takes to answer is not the subagent's: its clock starts again.
    pub fn asked(&mut self, now: f64) {
        self.last = now;
    }

    /// What its steps say now, the person's questions aside.
    pub fn sign(&self, now: f64) -> Option<Sign> {
        if self.repeats >= REPEATS {
            return Some(Sign::Repeat);
        }
        if self.failed.values().any(|n| *n >= FAILS) {
            return Some(Sign::Failing);
        }
        let quiet = if self.open.values().any(|(_, command)| *command) { QUIET_COMMAND } else { QUIET };
        (now - self.last > quiet).then_some(Sign::Silent)
    }
}

/// The agent call a step belongs to: its parent, followed up while that is itself a step of a subagent (a subagent's own
/// helper counts for the call the chain started from).
pub fn root<'a>(tools: &'a [ToolUse], tool: &'a ToolUse) -> Option<&'a str> {
    let mut id = tool.parent.as_deref()?;
    for _ in 0..tools.len() {
        match tools.iter().find(|t| t.id == id).and_then(|t| t.parent.as_deref()) {
            Some(up) => id = up,
            None => break,
        }
    }
    Some(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn step(id: &str, name: &str, input: Value) -> ToolUse {
        ToolUse {
            id: id.into(),
            name: name.into(),
            input,
            ok: None,
            output: None,
            declined: false,
            parent: Some("call".into()),
            sign: None,
            facts: None,
            told: vec![],
            words: vec![],
            words_cut: false,
            words_chars: None,
            transcript: None,
            check: false,
            started: None,
            ended: None,
        }
    }

    fn ended(mut tool: ToolUse, ok: bool) -> ToolUse {
        tool.ok = Some(ok);
        tool
    }

    #[test]
    fn the_same_step_six_times_in_a_row_is_a_repeat_and_another_step_ends_it() {
        let mut child = Child::new(0.0);
        let read = |n: u32| step(&format!("r{n}"), "Read", json!({ "file_path": "a.rs" }));
        for n in 1..REPEATS {
            child.started(&read(n), f64::from(n));
            child.ended(&ended(read(n), true), f64::from(n));
        }
        assert_eq!(child.sign(10.0), None, "five reads of one file are not a loop");
        child.started(&read(REPEATS), 10.0);
        assert_eq!(child.sign(10.0), Some(Sign::Repeat));
        child.started(&step("g", "Grep", json!({ "pattern": "x" })), 11.0);
        assert_eq!(child.sign(11.0), None);
    }

    #[test]
    fn a_check_fails_three_times_unchanged_but_an_edit_between_is_work() {
        let test = |id: &str| step(id, "Bash", json!({ "command": "cargo test" }));
        let mut child = Child::new(0.0);
        for id in ["t1", "t2"] {
            child.started(&test(id), 1.0);
            child.ended(&ended(test(id), false), 2.0);
        }
        assert_eq!(child.sign(2.0), None, "a failure and its first retry are the work going on");
        let edit = step("e", "Edit", json!({ "file_path": "src/lib.rs" }));
        child.started(&edit, 3.0);
        child.ended(&ended(edit, true), 3.0);
        child.started(&test("t3"), 4.0);
        child.ended(&ended(test("t3"), false), 5.0);
        assert_eq!(child.sign(5.0), None, "a change came between, so it is a new attempt");
        for id in ["t4", "t5"] {
            child.started(&test(id), 6.0);
            child.ended(&ended(test(id), false), 7.0);
        }
        assert_eq!(child.sign(7.0), Some(Sign::Failing));
        child.started(&test("t6"), 8.0);
        child.ended(&ended(test("t6"), true), 9.0);
        assert_eq!(child.sign(9.0), None, "the check passed");
    }

    #[test]
    fn a_refused_step_is_never_a_failure() {
        let mut child = Child::new(0.0);
        for id in ["a", "b", "c"] {
            let mut tool = ended(step(id, "Bash", json!({ "command": "rm -rf build" })), false);
            tool.declined = true;
            child.ended(&tool, 1.0);
        }
        assert_eq!(child.sign(1.0), None);
    }

    #[test]
    fn silence_counts_longer_while_a_command_runs_and_an_answer_restarts_the_clock() {
        let mut child = Child::new(0.0);
        assert_eq!(child.sign(QUIET), None);
        assert_eq!(child.sign(QUIET + 1.0), Some(Sign::Silent));
        child.asked(QUIET + 1.0);
        assert_eq!(child.sign(QUIET + 2.0), None);
        let build = step("b", "commandExecution", json!("cargo build"));
        child.started(&build, 1000.0);
        assert_eq!(child.sign(1000.0 + QUIET + 1.0), None, "a build says nothing until it ends");
        assert_eq!(child.sign(1000.0 + QUIET_COMMAND + 1.0), Some(Sign::Silent));
        child.ended(&ended(build, true), 1000.0 + QUIET_COMMAND + 2.0);
        assert_eq!(child.sign(1000.0 + QUIET_COMMAND + 3.0), None);
    }

    #[test]
    fn a_step_of_a_subagents_own_helper_counts_for_the_first_call() {
        let mut call = step("call", "Agent", json!({}));
        call.parent = None;
        let mut helper = step("h", "Helper", json!({}));
        helper.parent = Some("call".into());
        let mut deep = step("d", "Read", json!({}));
        deep.parent = Some("h".into());
        let tools = vec![call.clone(), helper, deep.clone()];
        assert_eq!(root(&tools, &deep), Some("call"));
        assert_eq!(root(&tools, &call), None);
    }
}

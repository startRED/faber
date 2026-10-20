//! Subagents the agent leaves running in the background answer after the turn that started them. Each one comes back as a
//! turn the CLI starts by itself, with a `result` of its own that names the subagent's run (`origin.runId`), and the answer
//! the person waits for is in the last of them (`tests/fixtures/claude/background.jsonl`). So a reply ends at the first
//! `result` that no background subagent still owes a turn to, and the turns before it count as parts of the same reply.

use crate::events::EngineEvent;
use crate::proc::ProcessIdentity;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Disk writers are separate from reply aggregation: a background command need not owe an answer.
#[derive(Default)]
pub(super) struct Writers {
    calls: BTreeSet<String>,
    background_calls: BTreeSet<String>,
    tasks: BTreeMap<String, Vec<String>>,
    interrupted: bool,
    lost: bool,
    tree_members: Vec<ProcessIdentity>,
    tree_unknown: bool,
}

impl Writers {
    pub fn observe_tree(&mut self, members: std::io::Result<Vec<ProcessIdentity>>) {
        match members {
            Ok(members) => {
                self.tree_members = members;
                self.tree_unknown = false;
            }
            Err(_) => self.tree_unknown = true,
        }
        if !self.tree_pending() && self.calls.is_empty() && self.tasks.is_empty() {
            self.interrupted = false;
        }
    }

    pub fn tree_pending(&self) -> bool {
        self.tree_unknown || !self.tree_members.is_empty()
    }

    pub fn observe(&mut self, msg: &Value) {
        match msg["type"].as_str() {
            Some("assistant") => {
                for call in msg["message"]["content"].as_array().into_iter().flatten() {
                    if call["type"] == "tool_use" && matches!(call["name"].as_str(), Some("Bash" | "Agent" | "Task")) {
                        if let Some(id) = call["id"].as_str().filter(|id| !id.is_empty()) {
                            self.calls.insert(id.into());
                            if call["input"]["run_in_background"] == true {
                                self.background_calls.insert(id.into());
                            }
                        } else {
                            self.lost = true;
                        }
                    }
                }
            }
            Some("user") => {
                for result in msg["message"]["content"].as_array().into_iter().flatten() {
                    if result["type"] == "tool_result"
                        && let Some(id) = result["tool_use_id"].as_str()
                        && (!self.background_calls.contains(id)
                            || self.tasks.values().any(|aliases| aliases.iter().any(|alias| alias == id))
                            || result["is_error"] == true)
                    {
                        self.calls.remove(id);
                        self.background_calls.remove(id);
                    }
                }
            }
            Some("system")
                if msg["subtype"] == "task_started" && matches!(msg["task_type"].as_str(), Some("local_agent" | "local_bash")) =>
            {
                let id = msg["run_id"].as_str().or(msg["task_id"].as_str()).or(msg["tool_use_id"].as_str());
                if let Some(id) = id.filter(|id| !id.is_empty()) {
                    let call = msg["tool_use_id"].as_str().map(String::from);
                    if let Some(call) = &call {
                        self.calls.remove(call);
                        self.background_calls.remove(call);
                    }
                    let aliases = [msg["run_id"].as_str(), msg["task_id"].as_str(), call.as_deref()]
                        .into_iter()
                        .flatten()
                        .map(String::from)
                        .collect();
                    self.tasks.insert(id.into(), aliases);
                } else {
                    self.lost = true;
                }
            }
            Some("system")
                if msg["subtype"] == "task_notification"
                    && matches!(msg["status"].as_str(), Some("completed" | "failed" | "stopped" | "cancelled")) =>
            {
                let ids = [msg["run_id"].as_str(), msg["task_id"].as_str(), msg["tool_use_id"].as_str()];
                self.tasks.retain(|_, aliases| {
                    if ids.into_iter().flatten().any(|id| aliases.iter().any(|alias| alias == id)) {
                        for alias in aliases {
                            self.calls.remove(alias);
                            self.background_calls.remove(alias);
                        }
                        false
                    } else {
                        true
                    }
                });
                for id in ids.into_iter().flatten() {
                    self.calls.remove(id);
                    self.background_calls.remove(id);
                }
            }
            Some("result") => {
                if let Some(run) = msg["origin"]["runId"].as_str()
                    && let Some(aliases) = self.tasks.remove(run)
                {
                    for alias in aliases {
                        self.calls.remove(&alias);
                        self.background_calls.remove(&alias);
                    }
                }
            }
            _ => {}
        }
        if self.calls.is_empty() && self.tasks.is_empty() {
            self.interrupted = false;
        }
    }

    pub fn busy(&self) -> bool {
        self.lost || self.tree_pending() || !self.calls.is_empty() || !self.tasks.is_empty()
    }

    pub fn interrupt(&mut self) {
        self.interrupted = self.busy();
    }

    pub fn lost(&mut self, active_turn: bool) {
        self.lost |= active_turn || self.busy();
    }

    pub fn shutdown(&mut self, confirmed: bool, active_turn: bool) {
        if confirmed {
            *self = Self::default();
        } else {
            self.lost(active_turn);
        }
    }

    pub fn snapshot(&self) -> EngineEvent {
        EngineEvent::Writers {
            active: self
                .calls
                .iter()
                .map(|id| format!("tool:{id}"))
                .chain(self.tasks.keys().map(|id| format!("task:{id}")))
                .chain(self.tree_members.iter().map(|member| format!("process-tree:{}", member.pid)))
                .collect(),
            unknown: self.lost || self.interrupted || self.tree_pending(),
        }
    }

    pub fn should_interrupt(&self, active_turn: bool, stopping: bool) -> bool {
        !stopping && (active_turn || self.busy())
    }
}

#[derive(Default)]
pub(super) struct Background {
    /// Runs of the background subagents the main agent started whose turn has not come back yet.
    owed: BTreeSet<String>,
    /// What the turns already held back counted, added to the reply's own `Done`.
    held: Option<EngineEvent>,
}

impl Background {
    /// Reads one line of the CLI's output; true when it is a `result` that ends the reply. A stopped reply ends at its next
    /// result: what is still out is not waited for.
    pub fn ends_reply(&mut self, msg: &Value, stopping: bool) -> bool {
        // A subagent's own subagents answer to it, never to the main agent.
        if !msg["parent_tool_use_id"].is_null() {
            return false;
        }
        match msg["type"].as_str() {
            // Only agents: a command left running (a dev server) may never end, and its turn is not the answer.
            Some("system") if msg["subtype"] == "task_started" && msg["is_backgrounded"] == true && msg["task_type"] == "local_agent" => {
                if let Some(run) = msg["run_id"].as_str() {
                    self.owed.insert(run.into());
                }
                false
            }
            Some("result") => {
                if let Some(run) = msg["origin"]["runId"].as_str() {
                    self.owed.remove(run);
                }
                if stopping {
                    self.owed.clear();
                }
                self.owed.is_empty()
            }
            _ => false,
        }
    }

    /// Whether a background subagent still owes the reply a turn: until it comes back, the main thread ending its message
    /// does not finish the reply.
    pub fn owes(&self) -> bool {
        !self.owed.is_empty()
    }

    /// Keeps the counters of a turn that is not the end of the reply.
    pub fn hold(&mut self, done: EngineEvent) {
        self.held = Some(match self.held.take() {
            Some(before) => add(before, done),
            None => done,
        });
    }

    /// The reply's own `Done`, with what the turns before it counted. The CLI's cost is already the session's total.
    pub fn close(&mut self, done: EngineEvent) -> EngineEvent {
        match self.held.take() {
            Some(before) => add(before, done),
            None => done,
        }
    }
}

fn add(before: EngineEvent, after: EngineEvent) -> EngineEvent {
    match (before, after) {
        (
            EngineEvent::Done {
                input_tokens: i,
                output_tokens: o,
                cache_read_tokens: r,
                cache_write_tokens: w,
                cache_write_1h_tokens: h,
                incomplete: x,
                ..
            },
            EngineEvent::Done {
                ok,
                reason,
                cost_total_usd,
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_write_tokens,
                cache_write_1h_tokens,
                incomplete,
            },
        ) => EngineEvent::Done {
            ok,
            reason,
            cost_total_usd,
            input_tokens: i + input_tokens,
            output_tokens: o + output_tokens,
            cache_read_tokens: r + cache_read_tokens,
            cache_write_tokens: w + cache_write_tokens,
            cache_write_1h_tokens: h + cache_write_1h_tokens,
            incomplete: x || incomplete,
        },
        (_, after) => after,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn started(run: &str, background: bool, kind: &str) -> Value {
        json!({ "type": "system", "subtype": "task_started", "run_id": run, "is_backgrounded": background, "task_type": kind,
            "parent_tool_use_id": null })
    }
    fn result(run: Option<&str>) -> Value {
        json!({ "type": "result", "parent_tool_use_id": null, "origin": run.map(|r| json!({ "kind": "task-notification", "runId": r })) })
    }

    #[test]
    fn only_background_agents_of_the_main_agent_keep_a_reply_open() {
        let mut b = Background::default();
        b.ends_reply(&started("fg", false, "local_agent"), false);
        b.ends_reply(&started("sh", true, "local_bash"), false);
        let mut nested = started("deep", true, "local_agent");
        nested["parent_tool_use_id"] = json!("toolu_1");
        b.ends_reply(&nested, false);
        assert!(b.ends_reply(&result(None), false));
    }

    #[test]
    fn the_reply_ends_when_every_background_agent_has_had_its_turn_or_at_once_when_stopped() {
        let mut b = Background::default();
        b.ends_reply(&started("a", true, "local_agent"), false);
        b.ends_reply(&started("b", true, "local_agent"), false);
        assert!(!b.ends_reply(&result(None), false));
        assert!(!b.ends_reply(&result(Some("a")), false));
        assert!(b.ends_reply(&result(Some("b")), false));
        b.ends_reply(&started("c", true, "local_agent"), false);
        assert!(b.ends_reply(&result(None), true), "an interrupt does not wait for what is still out");
        assert!(b.ends_reply(&result(None), false), "and leaves nothing owed for the next reply");
    }

    #[test]
    fn a_background_agent_still_out_keeps_the_main_threads_end_from_finishing_the_reply() {
        let mut b = Background::default();
        assert!(!b.owes());
        b.ends_reply(&started("a", true, "local_agent"), false);
        assert!(b.owes(), "its turn is still to come");
        b.ends_reply(&result(Some("a")), false);
        assert!(!b.owes());
    }

    fn call(id: &str, tool: &str, background: bool) -> Value {
        json!({ "type": "assistant", "message": { "content": [{ "type": "tool_use", "id": id,
            "name": tool, "input": { "run_in_background": background } }] } })
    }

    fn tool_result(id: &str) -> Value {
        json!({ "type": "user", "message": { "content": [{ "type": "tool_result", "tool_use_id": id }] } })
    }

    #[test]
    fn rewind_tracks_commands_before_launch_and_after_the_main_reply_ends() {
        let mut writers = Writers::default();
        writers.observe(&call("bash", "Bash", true));
        assert!(writers.busy(), "the tool call reserves a writer before execution");
        writers.observe(&tool_result("bash"));
        writers.observe(&result(None));
        assert!(writers.busy(), "a delayed task-start notification cannot create an idle gap");
        let mut task = started("command", true, "local_bash");
        task["tool_use_id"] = json!("bash");
        writers.observe(&task);
        assert!(matches!(writers.snapshot(), EngineEvent::Writers { active, unknown: false } if active == ["task:command"]));
        writers.observe(&json!({ "type": "system", "subtype": "task_notification", "task_id": "command", "status": "running" }));
        assert!(writers.busy(), "progress never proves completion");
        writers.observe(&json!({ "type": "system", "subtype": "task_notification", "task_id": "command", "status": "completed" }));
        assert!(!writers.busy());
    }

    #[test]
    fn rewind_stop_with_no_active_turn_reaches_writers_and_waits_for_confirmed_idle() {
        let mut writers = Writers::default();
        assert!(!writers.should_interrupt(false, false));
        writers.observe(&started("helper", true, "local_agent"));
        assert!(writers.should_interrupt(false, false), "Stop reaches helpers without an active main turn");
        writers.interrupt();
        writers.observe(&result(None));
        assert!(matches!(writers.snapshot(), EngineEvent::Writers { unknown: true, .. }));
        assert!(!writers.should_interrupt(false, true), "do not duplicate an outstanding interrupt");
        writers.observe(&result(Some("helper")));
        assert!(matches!(writers.snapshot(), EngineEvent::Writers { active, unknown: false } if active.is_empty()));
        writers.observe(&started("command", true, "local_bash"));
        assert!(writers.should_interrupt(false, false), "Stop also reaches commands");
    }

    #[test]
    fn rewind_a_crash_does_not_confirm_writers_stopped_and_nested_writers_count() {
        let mut writers = Writers::default();
        let mut nested = call("nested", "Bash", false);
        nested["parent_tool_use_id"] = json!("helper");
        writers.observe(&nested);
        assert!(writers.busy());
        writers.lost(false);
        writers.observe(&tool_result("nested"));
        assert!(matches!(writers.snapshot(), EngineEvent::Writers { active, unknown: true } if active.is_empty()));
        assert!(writers.busy(), "a crash never establishes that descendants ended");
    }

    #[test]
    fn rewind_only_confirmed_shutdown_releases_a_sessions_background_writers() {
        let mut writers = Writers::default();
        writers.observe(&started("command", true, "local_bash"));
        writers.interrupt();
        writers.shutdown(false, false);
        assert!(matches!(writers.snapshot(), EngineEvent::Writers { active, unknown: true } if active == ["task:command"]));
        writers.shutdown(true, false);
        assert!(matches!(writers.snapshot(), EngineEvent::Writers { active, unknown: false } if active.is_empty()));
    }

    #[test]
    fn rewind_os_membership_counts_unreported_hooks_and_clears_after_the_last_child_exits() {
        let mut writers = Writers::default();
        // A hook has no Bash, Agent or Task message at all.
        writers.observe_tree(Ok(vec![ProcessIdentity { pid: 99, created: 100 }]));
        assert!(matches!(writers.snapshot(), EngineEvent::Writers { active, unknown: true } if active == ["process-tree:99"]));
        writers.observe(&result(None));
        assert!(writers.busy(), "the main result says nothing about a hook's process");
        writers.observe_tree(Err(std::io::Error::other("membership unavailable")));
        assert!(writers.busy());
        writers.observe_tree(Ok(Vec::new()));
        assert!(!writers.busy(), "a successful OS query clears a completed hook without closing the CLI");
        writers.observe_tree(Ok(vec![ProcessIdentity { pid: 100, created: 101 }]));
        writers.shutdown(false, false);
        assert!(writers.busy());
        writers.shutdown(true, false);
        assert!(!writers.busy());
    }
}

//! Subagents the agent leaves running in the background answer after the turn that started them. Each one comes back as a
//! turn the CLI starts by itself, with a `result` of its own that names the subagent's run (`origin.runId`), and the answer
//! the person waits for is in the last of them (`tests/fixtures/claude/background.jsonl`). So a reply ends at the first
//! `result` that no background subagent still owes a turn to, and the turns before it count as parts of the same reply.

use crate::events::EngineEvent;
use serde_json::Value;
use std::collections::BTreeSet;

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
}

//! Shell tasks use task ids, never native process ids. Output is read only on the CLI's task events.
use crate::background::{MAX_COMMANDS, State, Update};
use serde_json::Value;

#[derive(Default)]
pub(super) struct Commands {
    calls: Vec<(String, String, bool)>,
    tasks: Vec<String>,
}

impl Commands {
    pub fn contains(&self, task: &str) -> bool {
        self.tasks.iter().any(|id| id == task)
    }

    pub fn end_turn(&mut self) {
        for (_, _, ended) in &mut self.calls {
            *ended = true;
        }
    }

    pub fn observe(&mut self, msg: &Value) -> Vec<Update> {
        match msg["type"].as_str() {
            Some("assistant") => {
                for call in msg["message"]["content"].as_array().into_iter().flatten() {
                    if call["type"] == "tool_use"
                        && call["name"] == "Bash"
                        && let Some((id, command)) = call["id"].as_str().zip(call["input"]["command"].as_str())
                    {
                        if self.calls.len() == MAX_COMMANDS {
                            self.calls.remove(0);
                        }
                        self.calls.push((id.into(), command.into(), false));
                    }
                }
            }
            Some("system") if msg["subtype"] == "task_started" && msg["task_type"] == "local_bash" => {
                let Some((task, call)) = msg["task_id"].as_str().zip(msg["tool_use_id"].as_str()) else { return vec![] };
                if task.is_empty() || self.contains(task) || self.tasks.len() == MAX_COMMANDS {
                    return vec![];
                }
                let Some(at) = self.calls.iter().position(|(id, _, _)| id == call) else { return vec![] };
                let (_, command, turn_ended) = self.calls.remove(at);
                self.tasks.push(task.into());
                return vec![Update::Start { id: task.into(), command, stoppable: true, turn_ended }];
            }
            Some("system") if msg["subtype"] == "task_notification" => {
                let Some(task) = msg["task_id"].as_str() else { return vec![] };
                if !self.contains(task) {
                    return vec![];
                }
                let state = match msg["status"].as_str() {
                    Some("completed") => State::Done,
                    Some("stopped" | "cancelled") => State::Stopped,
                    Some("failed") => State::Failed,
                    _ => return vec![],
                };
                self.tasks.retain(|id| id != task);
                return vec![Update::End { id: task.into(), state }];
            }
            _ => (),
        }
        vec![]
    }

    pub fn read_event(&self, msg: &Value) -> Option<String> {
        if msg["type"] != "system" || !matches!(msg["subtype"].as_str(), Some("task_started" | "task_progress" | "task_notification")) {
            return None;
        }
        let task = msg["task_id"].as_str()?;
        self.contains(task).then(|| task.into())
    }
}

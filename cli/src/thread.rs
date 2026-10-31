//! A conversation as the events tell it. A streaming reply carries only the steps that changed since the last update
//! (`toolsFrom`, and `{"id", "same": true}` for a step as it was), the way the screen hears it; this puts each reply back
//! together, and says when it cannot, so the caller opens the conversation again.

use serde_json::Value;
use std::collections::HashMap;

#[derive(Default)]
pub struct Thread {
    /// Each message by its id, whole.
    messages: HashMap<String, Value>,
    pub order: Vec<String>,
}

/// An update came for a reply this thread never saw whole: the conversation has to be opened again.
#[derive(Debug, PartialEq)]
pub struct Unknown;

impl Thread {
    /// From `open`'s answer.
    pub fn from_conversation(conversation: &Value) -> Thread {
        let mut thread = Thread::default();
        for message in conversation["messages"].as_array().into_iter().flatten() {
            thread.put(message.clone());
        }
        thread
    }

    fn put(&mut self, message: Value) {
        let id = message["id"].as_str().unwrap_or_default().to_string();
        if !self.messages.contains_key(&id) {
            self.order.push(id.clone());
        }
        self.messages.insert(id, message);
    }

    pub fn get(&self, id: &str) -> Option<&Value> {
        self.messages.get(id)
    }

    pub fn last(&self) -> Option<&Value> {
        self.order.last().and_then(|id| self.messages.get(id))
    }

    /// Takes one `message` event's message, and gives back the whole message it now is.
    pub fn take(&mut self, mut message: Value) -> Result<&Value, Unknown> {
        let id = message["id"].as_str().unwrap_or_default().to_string();
        if let Some(from) = message.get("toolsFrom").and_then(Value::as_u64) {
            let before = self.messages.get(&id).ok_or(Unknown)?;
            let old: Vec<Value> = before["tools"].as_array().cloned().unwrap_or_default();
            let from = usize::try_from(from).map_err(|_| Unknown)?;
            if from > old.len() {
                return Err(Unknown);
            }
            let by_id: HashMap<&str, &Value> = old.iter().filter_map(|t| Some((t["id"].as_str()?, t))).collect();
            let mut tools: Vec<Value> = old[..from].to_vec();
            for step in message["tools"].as_array().cloned().unwrap_or_default() {
                if step.get("same").and_then(Value::as_bool) == Some(true) {
                    let id = step["id"].as_str().unwrap_or_default();
                    tools.push((*by_id.get(id).ok_or(Unknown)?).clone());
                } else {
                    tools.push(step);
                }
            }
            let object = message.as_object_mut().ok_or(Unknown)?;
            object.remove("toolsFrom");
            object.insert("tools".into(), Value::Array(tools));
        }
        self.put(message);
        Ok(&self.messages[&id])
    }
}

/// The one-line form of a step: its tool and what it worked on.
pub fn step_line(tool: &Value) -> String {
    let name = tool["name"].as_str().unwrap_or("?");
    let input = &tool["input"];
    let on = ["file_path", "path", "command", "pattern", "url", "query", "description", "cmd"]
        .iter()
        .find_map(|key| input.get(key).and_then(Value::as_str))
        .or_else(|| input.get("command").and_then(Value::as_array).and_then(|parts| parts.last()?.as_str()))
        .unwrap_or("");
    let on: String = on.lines().next().unwrap_or("").chars().take(80).collect();
    if on.is_empty() { name.to_string() } else { format!("{name} {on}") }
}

/// How a step ended: ran, failed, declined, or still running.
pub fn step_state(tool: &Value) -> StepState {
    if tool["declined"].as_bool() == Some(true) {
        StepState::Declined
    } else {
        match tool["ok"].as_bool() {
            Some(true) => StepState::Done,
            Some(false) => StepState::Failed,
            None => StepState::Running,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    Running,
    Done,
    Failed,
    Declined,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_reply_comes_back_whole_from_its_changed_steps() {
        let mut thread = Thread::default();
        thread.take(json!({"id": "m1", "text": "a", "tools": [{"id": "t1", "ok": true}, {"id": "t2"}]})).unwrap();
        let whole = thread
            .take(json!({"id": "m1", "text": "ab", "toolsFrom": 1, "tools": [{"id": "t2", "ok": true}, {"id": "t3"}]}))
            .unwrap()
            .clone();
        assert_eq!(whole["tools"], json!([{"id": "t1", "ok": true}, {"id": "t2", "ok": true}, {"id": "t3"}]));
        assert!(whole.get("toolsFrom").is_none());
        let same = thread.take(json!({"id": "m1", "text": "abc", "toolsFrom": 0, "tools": [{"id": "t1", "same": true}]})).unwrap();
        assert_eq!(same["tools"], json!([{"id": "t1", "ok": true}]));
    }

    #[test]
    fn an_update_for_a_reply_never_seen_whole_says_so() {
        let mut thread = Thread::default();
        assert_eq!(thread.take(json!({"id": "m9", "toolsFrom": 2, "tools": []})).err(), Some(Unknown));
        thread.take(json!({"id": "m1", "tools": [{"id": "t1"}]})).unwrap();
        assert_eq!(thread.take(json!({"id": "m1", "toolsFrom": 0, "tools": [{"id": "zz", "same": true}]})).err(), Some(Unknown));
    }

    #[test]
    fn a_step_reads_as_its_tool_and_what_it_touched() {
        assert_eq!(step_line(&json!({"name": "Read", "input": {"file_path": "src/a.rs"}})), "Read src/a.rs");
        assert_eq!(step_line(&json!({"name": "Bash", "input": {"command": "cargo test\nmore"}})), "Bash cargo test");
        assert_eq!(step_line(&json!({"name": "TodoWrite", "input": {}})), "TodoWrite");
        assert_eq!(step_state(&json!({"ok": false, "declined": true})), StepState::Declined);
        assert_eq!(step_state(&json!({})), StepState::Running);
    }
}

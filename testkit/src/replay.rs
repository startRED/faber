//! A recorded session replayed line by line: what the app writes is checked against the next recorded input, and the
//! recorded outputs that follow it are sent back. The same rule as the engine's own tests (`engine/tests/support`).

use serde_json::Value;
use std::io;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Entry {
    pub input: bool,
    pub msg: Value,
}

/// Reads a `*.jsonl` recording: one `{"dir": "in" | "out", "msg": ...}` object per line.
pub fn load(path: &Path) -> io::Result<Vec<Entry>> {
    let text = std::fs::read_to_string(path).map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", path.display())))?;
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).map_err(|e| io::Error::other(format!("{}: {e}", path.display())))?;
            Ok(Entry { input: v["dir"] == "in", msg: v["msg"].clone() })
        })
        .collect()
}

/// What must match between what the app sends and what was recorded.
pub fn key(msg: &Value) -> String {
    let method = msg["method"].as_str();
    let id = &msg["id"];
    // Codex app-server speaks JSON-RPC: requests and notifications have a method, answers have an id.
    if method == Some("turn/start") {
        return format!("turn/start {} {}", id, msg["params"]["input"]);
    }
    if let Some(m) = method {
        return if id.is_null() { m.to_string() } else { format!("{m} {id}") };
    }
    if !id.is_null() {
        return format!("answer {} {}", id, msg["result"]["decision"].as_str().unwrap_or(""));
    }
    match msg["type"].as_str().unwrap_or_default() {
        "user" => format!("user {}", msg["message"]["content"]),
        "control_request" => format!("control_request {}", msg["request"]["subtype"].as_str().unwrap_or("")),
        "control_response" => {
            let r = &msg["response"];
            format!("control_response {} {}", r["request_id"].as_str().unwrap_or(""), r["response"]["behavior"].as_str().unwrap_or("ok"))
        }
        other => other.to_string(),
    }
}

pub struct Replay {
    entries: Vec<Entry>,
    at: usize,
    pub mismatches: Vec<String>,
}

impl Replay {
    pub fn new(entries: Vec<Entry>) -> Replay {
        Replay { entries, at: 0, mismatches: Vec::new() }
    }

    /// One line the app wrote. Returns the lines to send back: the recorded outputs up to the next recorded input.
    pub fn feed(&mut self, line: &str) -> io::Result<Vec<String>> {
        let msg: Value = serde_json::from_str(line).map_err(|e| io::Error::other(format!("not JSON ({e}): {line}")))?;
        let sent = key(&msg);
        let expected = match self.entries.get(self.at) {
            Some(entry) if entry.input => key(&entry.msg),
            _ => "(no input expected)".to_string(),
        };
        if sent == expected {
            self.at += 1;
        } else {
            self.mismatches.push(format!("sent {sent}, recorded {expected}"));
        }
        let mut out = Vec::new();
        while let Some(entry) = self.entries.get(self.at).filter(|e| !e.input) {
            out.push(entry.msg.to_string());
            self.at += 1;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(input: bool, msg: Value) -> Entry {
        Entry { input, msg }
    }

    #[test]
    fn the_key_ignores_what_may_vary_and_keeps_what_must_match() {
        assert_eq!(
            key(&json!({"type": "user", "message": {"content": "hi"}, "session_id": "a"})),
            key(&json!({"type": "user", "message": {"content": "hi"}, "session_id": "b"}))
        );
        assert_ne!(
            key(&json!({"type": "user", "message": {"content": "hi"}})),
            key(&json!({"type": "user", "message": {"content": "bye"}}))
        );
        assert_eq!(key(&json!({"method": "initialized"})), "initialized");
        assert_eq!(key(&json!({"method": "thread/start", "id": 2, "params": {"cwd": "x"}})), "thread/start 2");
        assert_eq!(key(&json!({"id": 7, "result": {"decision": "accept"}})), "answer 7 accept");
        assert_eq!(
            key(&json!({"type": "control_response", "response": {"request_id": "r1", "response": {"behavior": "allow"}}})),
            "control_response r1 allow"
        );
    }

    #[test]
    fn the_outputs_after_a_matching_input_come_back_and_a_wrong_input_is_named() {
        let mut replay = Replay::new(vec![
            entry(true, json!({"type": "user", "message": {"content": "hi"}})),
            entry(false, json!({"type": "a"})),
            entry(false, json!({"type": "b"})),
            entry(true, json!({"type": "user", "message": {"content": "again"}})),
            entry(false, json!({"type": "c"})),
        ]);
        assert_eq!(replay.feed(r#"{"type":"user","message":{"content":"hi"}}"#).unwrap(), [r#"{"type":"a"}"#, r#"{"type":"b"}"#]);
        assert!(replay.mismatches.is_empty());
        assert!(replay.feed(r#"{"type":"user","message":{"content":"other"}}"#).unwrap().is_empty());
        assert_eq!(replay.mismatches.len(), 1);
        assert!(replay.mismatches[0].contains("sent user \"other\""), "{:?}", replay.mismatches);
        assert_eq!(replay.feed(r#"{"type":"user","message":{"content":"again"}}"#).unwrap(), [r#"{"type":"c"}"#]);
        assert!(replay.feed("not json").is_err());
    }
}

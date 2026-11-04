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

/// The person's own text of a first input: the line that points to the guide, which Faber puts ahead of it, is not part of it.
fn person_text(content: &Value) -> Value {
    let strip = |text: &str| {
        let rest = faber_engine::guide::without_note(text);
        (rest.len() != text.len()).then(|| rest.to_string())
    };
    match content {
        Value::String(text) => strip(text).map_or_else(|| content.clone(), Value::from),
        Value::Array(parts) => Value::Array(
            parts
                .iter()
                .map(|part| {
                    let mut part = part.clone();
                    if let Some(text) = part["text"].as_str().and_then(strip) {
                        part["text"] = text.into();
                    }
                    part
                })
                .collect(),
        ),
        _ => content.clone(),
    }
}

/// What must match between what the app sends and what was recorded.
pub fn key(msg: &Value) -> String {
    let method = msg["method"].as_str();
    let id = &msg["id"];
    // Codex app-server speaks JSON-RPC: requests and notifications have a method, answers have an id.
    if method == Some("turn/start") {
        return format!("turn/start {} {}", id, person_text(&msg["params"]["input"]));
    }
    if method == Some("turn/steer") {
        return format!("turn/steer {} {}", id, msg["params"]["input"]);
    }
    if let Some(m) = method {
        return if id.is_null() { m.to_string() } else { format!("{m} {id}") };
    }
    if !id.is_null() {
        return format!("answer {} {}", id, msg["result"]["decision"].as_str().unwrap_or(""));
    }
    match msg["type"].as_str().unwrap_or_default() {
        "user" => format!("user {}", person_text(&msg["message"]["content"])),
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
        if matches!(msg["method"].as_str(), Some("skills/list" | "thread/name/set"))
            && self.entries.get(self.at).is_none_or(|entry| entry.msg["method"] != msg["method"])
        {
            return Ok(vec![]);
        }
        if msg["method"] == "config/read" && self.entries.get(self.at).is_none_or(|entry| entry.msg["method"] != "config/read") {
            return Ok(vec![serde_json::json!({"id":msg["id"],"result":{"config":{}}}).to_string()]);
        }
        if msg["method"] == "thread/unsubscribe"
            && self.entries.get(self.at).is_none_or(|entry| entry.msg["method"] != "thread/unsubscribe")
        {
            return Ok(vec![serde_json::json!({"id":msg["id"],"result":{"status":"notLoaded"}}).to_string()]);
        }
        let sent = key(&msg);
        let expected = match self.entries.get(self.at) {
            Some(entry) if entry.input => key(&entry.msg),
            _ => "(no input expected)".to_string(),
        };
        let report_matches = self.entries.get(self.at).and_then(|entry| entry.msg["_faberReportContains"].as_str()).is_none_or(|text| {
            msg["result"]["contentItems"][0]["text"]
                .as_str()
                .or_else(|| msg["response"]["response"]["mcp_response"]["result"]["content"][0]["text"].as_str())
                .is_some_and(|report| report.contains(text))
        });
        if sent == expected && report_matches {
            self.at += 1;
        } else if !report_matches {
            self.mismatches.push("helper report did not contain the recorded text".into());
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
    fn a_recorded_helper_report_must_reach_the_agent_before_its_answer() {
        for (expected, wrong, right) in [
            (
                json!({"id":0,"result":{},"_faberReportContains":"found 3"}),
                json!({"id":0,"result":{"contentItems":[{"text":"wrong"}]}}),
                json!({"id":0,"result":{"contentItems":[{"text":"found 3"}]}}),
            ),
            (
                json!({"type":"control_response","response":{"request_id":"r"},"_faberReportContains":"found 3"}),
                json!({"type":"control_response","response":{"request_id":"r","response":{"mcp_response":{"result":{"content":[{"text":"wrong"}]}}}}}),
                json!({"type":"control_response","response":{"request_id":"r","response":{"mcp_response":{"result":{"content":[{"text":"found 3"}]}}}}}),
            ),
        ] {
            let mut replay = Replay::new(vec![entry(true, expected), entry(false, json!({"answer":true}))]);
            assert!(replay.feed(&wrong.to_string()).unwrap().is_empty());
            assert_eq!(replay.feed(&right.to_string()).unwrap().len(), 1);
        }
    }

    #[test]
    fn the_line_to_the_guide_ahead_of_a_first_input_is_not_part_of_the_persons_text() {
        let line = "Faber's own guide: when the person asks how Faber itself works, read \"guide/SKILL.md\" and answer from it. The screen is shown in English.";
        let user = |content: Value| key(&json!({"type": "user", "message": {"content": content}}));
        let sent = format!("{line}\n\nhi");
        assert_eq!(user(json!(sent)), user(json!("hi")));
        assert_eq!(user(json!([{"type": "text", "text": sent}])), user(json!([{"type": "text", "text": "hi"}])));
        assert_ne!(user(json!("hello")), user(json!("hi")), "only the line is taken off");
        let wrong = format!("Current catalog resources for this session:\nsomething\n\n{line}\n\nhi");
        assert_ne!(user(json!(wrong)), user(json!("hi")), "anything else ahead of the words still fails a test");
        assert_ne!(user(json!(format!("{line}\nhi"))), user(json!("hi")), "the line ends at its blank line");
    }

    #[test]
    fn slash_skill_discovery_can_be_recorded_or_absent_from_an_older_fixture() {
        let request = json!({"id":"faber-commands-1","method":"skills/list","params":{"cwds":["project"]}});
        let turn = json!({"id":3,"method":"turn/start"});
        let mut old = Replay::new(vec![entry(true, turn.clone())]);
        assert!(old.feed(&request.to_string()).unwrap().is_empty());
        old.feed(&turn.to_string()).unwrap();
        assert!(old.mismatches.is_empty());
        let answer = json!({"id":"faber-commands-1","result":{"data":[]}});
        let mut current = Replay::new(vec![entry(true, request.clone()), entry(false, answer.clone())]);
        assert_eq!(current.feed(&request.to_string()).unwrap(), [answer.to_string()]);
        assert!(current.mismatches.is_empty());
    }

    #[test]
    fn thread_naming_can_be_recorded_or_absent_from_an_older_fixture() {
        let request = json!({"id":"faber-name-1","method":"thread/name/set","params":{"threadId":"t","name":"My conversation"}});
        let turn = json!({"id":3,"method":"turn/start"});
        let mut old = Replay::new(vec![entry(true, turn.clone())]);
        assert!(old.feed(&request.to_string()).unwrap().is_empty());
        old.feed(&turn.to_string()).unwrap();
        assert!(old.mismatches.is_empty());
        let answer = json!({"id":"faber-name-1","result":{}});
        let mut current = Replay::new(vec![entry(true, request.clone()), entry(false, answer.clone())]);
        assert_eq!(current.feed(&request.to_string()).unwrap(), [answer.to_string()]);
        assert!(current.mismatches.is_empty());
    }

    #[test]
    fn identity_refresh_can_be_recorded_or_absent_from_an_older_fixture() {
        let request = json!({"id":"faber-unsubscribe-1","method":"thread/unsubscribe","params":{"threadId":"plan-thread"}});
        let resume = json!({"id":2,"method":"thread/resume","params":{"threadId":"plan-thread"}});
        let mut old = Replay::new(vec![entry(true, resume.clone())]);
        let answer: Value = serde_json::from_str(&old.feed(&request.to_string()).unwrap()[0]).unwrap();
        assert_eq!(answer, json!({"id":request["id"],"result":{"status":"notLoaded"}}));
        old.feed(&resume.to_string()).unwrap();
        assert!(old.mismatches.is_empty(), "identity refresh leaves the recorded resume untouched");
        let answer = json!({"id":request["id"],"result":{"status":"unsubscribed"}});
        let mut current = Replay::new(vec![entry(true, request.clone()), entry(false, answer.clone()), entry(true, resume.clone())]);
        assert_eq!(current.feed(&request.to_string()).unwrap(), [answer.to_string()]);
        current.feed(&resume.to_string()).unwrap();
        assert!(current.mismatches.is_empty());

        let entries = load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/codex/approve-plan-run.jsonl")).unwrap();
        let inputs: Vec<_> = entries.iter().filter(|entry| entry.input).map(|entry| entry.msg.clone()).collect();
        let mut approved = Replay::new(entries);
        let mut outputs = Vec::new();
        for input in inputs {
            if input["method"] == "thread/resume" {
                approved.feed(&request.to_string()).unwrap();
            }
            outputs.extend(approved.feed(&input.to_string()).unwrap());
        }
        assert!(approved.mismatches.is_empty(), "{:?}", approved.mismatches);
        assert!(outputs.iter().any(|line| line.contains("Started the approved work.")));
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

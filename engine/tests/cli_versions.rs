//! Offline parser evidence for recorded CLI versions, never a verdict about an untested binary.

use faber_engine::claude::parse_claude_message;
use faber_engine::codex::{CodexTurnState, parse_codex_notification};
use faber_engine::events::EngineEvent;
use serde_json::{Value, json};
use std::collections::BTreeSet;

const CLAUDE: &str = include_str!("../../tests/fixtures/claude/chat.jsonl");
const CODEX: &str = include_str!("../../tests/fixtures/codex/chat.jsonl");

fn check(provider: &str, version: &str, recording: &str, required: &[&str]) -> Result<BTreeSet<&'static str>, String> {
    if !["claude", "codex"].contains(&provider) || !faber_engine::install::valid_version(version) {
        return Err("invalid provider or version".into());
    }
    let mut state = CodexTurnState::default();
    let mut version_seen = false;
    let mut shapes = BTreeSet::new();
    for (line, raw) in recording.lines().enumerate().filter(|(_, raw)| !raw.trim().is_empty()) {
        let entry: Value = serde_json::from_str(raw).map_err(|e| format!("line {}: {e}", line + 1))?;
        if !matches!(entry["dir"].as_str(), Some("in" | "out")) || !entry["msg"].is_object() {
            return Err(format!("line {}: expected an in/out message", line + 1));
        }
        if entry["dir"] == "in" {
            continue;
        }
        let msg = &entry["msg"];
        let recorded = if provider == "claude" { &msg["claude_code_version"] } else { &msg["result"]["thread"]["cliVersion"] };
        if !recorded.is_null() {
            if recorded.as_str() != Some(version) {
                return Err(format!("line {}: recorded version {recorded} does not match {version}", line + 1));
            }
            version_seen = true;
        }
        let events = if provider == "claude" { parse_claude_message(msg) } else { parse_codex_notification(msg, &mut state) };
        for event in events {
            let shape = match event {
                EngineEvent::Started { session_id, model } if !session_id.is_empty() && !model.is_empty() => "started",
                EngineEvent::Text(text) if !text.is_empty() => "text",
                EngineEvent::Tool { id, name, .. } if !id.is_empty() && !name.is_empty() => "tool",
                EngineEvent::ToolResult { id, .. } if !id.is_empty() => "tool_result",
                EngineEvent::Permission { request_id, tool, .. } if !request_id.is_empty() && !tool.is_empty() => "permission",
                EngineEvent::Done { ok: true, .. } => "done",
                EngineEvent::Error { message, .. } => return Err(message),
                _ => continue,
            };
            shapes.insert(shape);
        }
    }
    if !version_seen {
        return Err("no recorded version; cannot attribute this fixture".into());
    }
    let missing: Vec<_> = required.iter().filter(|shape| !shapes.contains(**shape)).collect();
    if !missing.is_empty() {
        return Err(format!("missing parsed shapes: {missing:?}"));
    }
    Ok(shapes)
}

fn report(provider: &str, version: &str, cases: &[(&str, &str, &[&str])]) {
    let mut failures = Vec::new();
    for (fixture, recording, required) in cases {
        let result = check(provider, version, recording, required);
        println!(
            "{}",
            json!({"kind":"recorded_parser_check", "provider":provider, "version":version, "fixture":fixture,
                "status":if result.is_ok() { "green" } else { "red" },
                "shapes":result.as_ref().ok(), "error":result.as_ref().err()})
        );
        if let Err(error) = result {
            failures.push(format!("{provider} {version} {fixture}: {error}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn recorded_claude_version_shapes() {
    report(
        "claude",
        "2.1.296",
        &[
            ("chat", CLAUDE, &["started", "text", "done"]),
            (
                "tools",
                include_str!("../../tests/fixtures/claude/tools.jsonl"),
                &["started", "text", "tool", "tool_result", "permission", "done"],
            ),
            ("resume", include_str!("../../tests/fixtures/claude/resume.jsonl"), &["started", "text", "done"]),
        ],
    );
}

#[test]
fn recorded_codex_version_shapes() {
    // Startup replies and approval requests belong to the session driver, not its notification parser.
    report(
        "codex",
        "0.161.0",
        &[
            ("chat", CODEX, &["text", "done"]),
            ("tools", include_str!("../../tests/fixtures/codex/tools.jsonl"), &["text", "tool", "tool_result", "done"]),
            ("resume", include_str!("../../tests/fixtures/codex/resume.jsonl"), &["text", "done"]),
        ],
    );
}

#[test]
fn an_unrecorded_version_cannot_inherit_a_green_result() {
    for (provider, recording) in [("claude", CLAUDE), ("codex", CODEX)] {
        assert!(check(provider, "999.0.0", recording, &["text", "done"]).unwrap_err().contains("does not match"));
    }
}

#[test]
fn missing_shapes_and_unattributed_or_broken_recordings_are_red() {
    assert!(check("claude", "2.1.296", CLAUDE, &["tool"]).unwrap_err().contains("missing parsed shapes"));
    for recording in ["", "not json", r#"{"dir":"sideways","msg":{}}"#, r#"{"dir":"out","msg":{}}"#] {
        assert!(check("claude", "2.1.296", recording, &["text", "done"]).is_err());
    }
    // A recording whose version line is gone cannot be attributed to any version.
    for (provider, version, recording, line) in
        [("claude", "2.1.296", CLAUDE, r#""claude_code_version":"2.1.296","#), ("codex", "0.161.0", CODEX, r#""cliVersion":"0.161.0","#)]
    {
        let unattributed = recording.replace(line, "");
        assert!(check(provider, version, &unattributed, &["text", "done"]).unwrap_err().contains("no recorded version"));
    }
    let no_done = CODEX.replace("turn/completed", "turn/unknown");
    assert!(check("codex", "0.161.0", &no_done, &["text", "done"]).unwrap_err().contains("done"));
}

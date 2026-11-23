//! The reader against agy's own recordings (`tests/fixtures/agy/acp-*.jsonl`). The event lists are checked in beside this
//! file (`expected/<name>.events`); `FABER_AGY_WRITE=1` writes them anew, to be read before they are committed.

use super::*;
use std::path::{Path, PathBuf};

fn recordings() -> Vec<(String, Vec<Value>)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/agy");
    let mut found: Vec<(String, Vec<Value>)> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|f| f.unwrap().path())
        .filter(|p| p.file_name().unwrap().to_str().unwrap().starts_with("acp-"))
        .map(|p| {
            let name = p.file_stem().unwrap().to_str().unwrap().to_string();
            let lines = std::fs::read_to_string(&p).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
            (name, lines)
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(found.len() >= 20, "only {} recordings", found.len());
    found
}

/// Plays one recording: what the client wrote is noted, what agy printed is read, and each process starts a new reader.
fn play(lines: &[Value], reader: &mut AcpReader) -> Vec<EngineEvent> {
    let mut events = Vec::new();
    for line in lines {
        match line["dir"].as_str() {
            Some("in") => reader.sent(&line["msg"]),
            Some("stdout") => events.extend(reader.read(line["msg"].as_str().unwrap())),
            Some("exit") => *reader = AcpReader::new(),
            _ => {}
        }
    }
    events
}

fn expected(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/agy/expected").join(format!("{name}.events"))
}

#[test]
fn every_recording_reads_as_the_checked_in_events() {
    let write = std::env::var_os("FABER_AGY_WRITE").is_some();
    let mut wrong = Vec::new();
    for (name, lines) in recordings() {
        let got: String = play(&lines, &mut AcpReader::new()).iter().map(|e| format!("{e:?}\n")).collect();
        if write {
            std::fs::create_dir_all(expected(&name).parent().unwrap()).unwrap();
            std::fs::write(expected(&name), &got).unwrap();
        } else if std::fs::read_to_string(expected(&name)).ok().as_deref().map(|t| t.replace("\r\n", "\n")) != Some(got) {
            wrong.push(name);
        }
    }
    assert!(wrong.is_empty(), "the events differ from expected/*.events for {wrong:?}");
}

#[test]
fn any_prefix_of_any_recording_reads_without_a_panic() {
    for (_, lines) in recordings() {
        for end in 0..lines.len() {
            let mut reader = AcpReader::new();
            play(&lines[..end], &mut reader);
            if let Some(cut) = lines[end]["msg"].as_str() {
                let bounds: Vec<usize> = cut.char_indices().map(|(i, _)| i).collect();
                for i in bounds.iter().step_by(bounds.len() / 7 + 1) {
                    reader.read(&cut[..*i]);
                }
            }
            reader.read_value(&json!({ "id": 4, "result": "not an object" }));
            reader.read_value(&json!({ "method": "session/update", "params": { "update": { "sessionUpdate": "tool_call_update" } } }));
            reader.cancel_pending();
        }
    }
}

fn launch(server: &Value) -> Launch {
    Launch {
        name: text(&server["name"]),
        command: PathBuf::from(text(&server["command"])),
        args: server["args"].as_array().unwrap().iter().map(text).collect(),
        env: server["env"].as_array().unwrap().iter().map(|e| (text(&e["name"]), text(&e["value"]))).collect(),
    }
}

/// What the builders make for a message the recorder wrote, when one of them makes it.
fn rebuilt(msg: &Value) -> Option<(&'static str, Value)> {
    let p = &msg["params"];
    let id = msg["id"].as_u64();
    let session = p["sessionId"].as_str().unwrap_or_default();
    let servers = || p["mcpServers"].as_array().unwrap().iter().map(launch).collect::<Vec<_>>();
    let only = |keys: &[&str]| p.as_object().is_some_and(|o| o.keys().all(|k| keys.contains(&k.as_str())));
    Some(match msg["method"].as_str() {
        // The recorder declared forms only where questions were tried; the builder always does.
        Some("initialize") if p["clientCapabilities"]["elicitation"].is_object() => {
            ("initialize", initialize(id?, p["clientInfo"]["version"].as_str()?))
        }
        Some("session/new") if only(&["cwd", "mcpServers"]) => ("session/new", session_new(id?, p["cwd"].as_str()?, &servers())),
        Some("session/load") => ("session/load", session_load(id?, session, p["cwd"].as_str()?, &servers())),
        Some("session/set_model") => ("set_model", set_model(id?, session, p["modelId"].as_str()?)),
        Some("session/set_config_option") => {
            ("set_config_option", set_config_option(id?, session, p["configId"].as_str()?, p["value"].as_str()?))
        }
        Some("session/set_mode") => ("set_mode", set_mode(id?, session, p["modeId"].as_str()?)),
        Some("session/prompt") => ("prompt", prompt(id?, session, p["prompt"][0]["text"].as_str()?)),
        Some("session/cancel") => ("cancel", cancel(session)),
        Some(_) => return None,
        None if msg["result"]["outcome"].is_object() => {
            ("permission_answer", permission_answer(&msg["id"], msg["result"]["outcome"]["optionId"].as_str()))
        }
        None => ("elicitation_answer", elicitation_answer(&msg["id"], msg["result"].get("content").cloned())),
    })
}

#[test]
fn the_builders_write_what_the_recorder_wrote() {
    let mut used = std::collections::BTreeSet::new();
    for (name, lines) in recordings() {
        for line in lines.iter().filter(|l| l["dir"] == "in") {
            if let Some((builder, built)) = rebuilt(&line["msg"]) {
                assert_eq!(built, line["msg"], "{name}: {builder}");
                used.insert(builder);
            }
        }
    }
    let all = [
        "cancel",
        "elicitation_answer",
        "initialize",
        "permission_answer",
        "prompt",
        "session/load",
        "session/new",
        "set_config_option",
        "set_mode",
        "set_model",
    ];
    assert_eq!(used.into_iter().collect::<Vec<_>>(), all);
}

/// The answer the reader builds for a card is the one agy took in the recording.
fn answers(name: &str, answer: impl Fn(&mut AcpReader, &str) -> Option<Value>) {
    let (_, lines) = recordings().into_iter().find(|(n, _)| n == name).unwrap();
    let mut reader = AcpReader::new();
    let mut checked = 0;
    for line in &lines {
        match line["dir"].as_str() {
            Some("stdout") => {
                for event in reader.read(line["msg"].as_str().unwrap()) {
                    if let EngineEvent::Permission { request_id, .. } = event {
                        let recorded = lines.iter().find(|l| l["dir"] == "in" && l["msg"]["id"] == request_id.as_str()).unwrap();
                        assert_eq!(answer(&mut reader, &request_id).as_ref(), Some(&recorded["msg"]), "{name}");
                        checked += 1;
                    }
                }
            }
            Some("in") => reader.sent(&line["msg"]),
            _ => {}
        }
    }
    assert_eq!(checked, 1, "{name}");
}

#[test]
fn a_card_answer_is_the_one_agy_took() {
    answers("acp-approval-allow", |r, id| r.answer_permission(id, true));
    answers("acp-approval-deny", |r, id| r.answer_permission(id, false));
    answers("acp-question-elicitation", |r, id| r.answer_question(id, Some(&json!({ "Which color do you prefer?": "blue" }))));
    answers("acp-question-declined", |r, id| r.answer_question(id, None));
}

#[test]
fn a_free_text_answer_goes_in_the_other_field_and_a_card_is_answered_once() {
    let mut r = AcpReader::new();
    let ask = json!({ "id": "7", "method": "elicitation/create", "params": { "requestedSchema": { "properties": {
        "q1": { "title": "Which?", "oneOf": [{ "const": "1", "title": "red" }] },
        "q1_other": { "type": "string" },
    } } } });
    assert!(matches!(&r.read_value(&ask)[..], [EngineEvent::Permission { tool, .. }] if tool == "AskUserQuestion"));
    let answer = r.answer_question("7", Some(&json!({ "Which?": "green" }))).unwrap();
    assert_eq!(answer["result"], json!({ "action": "accept", "content": { "q1_other": "green" } }));
    assert_eq!(r.answer_question("7", None), None);
}

#[test]
fn a_long_command_output_is_cut_at_the_preview_limit() {
    let mut r = AcpReader::new();
    let out = "x".repeat(TOOL_OUTPUT_CHARS + 10);
    let update = json!({ "method": "session/update", "params": { "update": { "sessionUpdate": "tool_call", "toolCallId": "c",
        "name": "run_command", "kind": "execute", "status": "completed", "rawInput": { "CommandLine": "dir" },
        "rawOutput": { "output": format!("Command exited with code 0.\n\nCommand output:\n{out}"), "exit_code": 0 } } } });
    let events = r.read_value(&update);
    let [EngineEvent::Tool { name, .. }, EngineEvent::ToolResult { ok: true, exit_code: Some(0), text, .. }] = &events[..] else {
        panic!("{events:?}")
    };
    assert_eq!((name.as_str(), text.chars().count()), ("Bash", TOOL_OUTPUT_CHARS));
}

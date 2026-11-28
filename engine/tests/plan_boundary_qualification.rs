//! Task 500 qualification evidence, not an execution gate or a live security probe.
use crate::support;
use faber_engine::claude::parse_claude_message;
use faber_engine::codex::{CodexTurnState, codex_thread, parse_codex_notification};
use faber_engine::events::EngineEvent;
use faber_engine::model::{Area, Permissions};

#[test]
fn size_boundary_qualification_recorded_approvals_have_matching_answers_before_results() {
    for vendor in ["claude", "codex"] {
        let rows = support::fixture(vendor, "tools");
        let requests: Vec<_> = rows
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                !e.input && (e.msg["request"]["subtype"] == "can_use_tool" || e.msg["method"] == "item/commandExecution/requestApproval")
            })
            .collect();
        assert_eq!(requests.len(), 2, "{vendor}: one allowed and one refused call");
        for (at, request) in requests {
            let (answer, result) = if vendor == "codex" {
                let answer = rows
                    .iter()
                    .enumerate()
                    .skip(at + 1)
                    .find(|(_, e)| e.input && e.msg["id"] == request.msg["id"] && e.msg.get("result").is_some())
                    .unwrap()
                    .0;
                let result = rows
                    .iter()
                    .enumerate()
                    .skip(at + 1)
                    .find(|(_, e)| {
                        !e.input && e.msg["method"] == "item/completed" && e.msg["params"]["item"]["id"] == request.msg["params"]["itemId"]
                    })
                    .unwrap()
                    .0;
                (answer, result)
            } else {
                assert!(parse_claude_message(&request.msg).iter().any(|e| matches!(e, EngineEvent::Permission { .. })));
                let answer = rows
                    .iter()
                    .enumerate()
                    .skip(at + 1)
                    .find(|(_, e)| e.input && e.msg["response"]["request_id"] == request.msg["request_id"])
                    .unwrap()
                    .0;
                let result = rows
                    .iter()
                    .enumerate()
                    .skip(at + 1)
                    .find(|(_, e)| {
                        !e.input
                            && e.msg["message"]["content"].as_array().is_some_and(|parts| {
                                parts.iter().any(|part| {
                                    part["type"] == "tool_result" && part["tool_use_id"] == request.msg["request"]["tool_use_id"]
                                })
                            })
                    })
                    .unwrap()
                    .0;
                (answer, result)
            };
            assert!(at < answer && answer < result, "{vendor}: only an answered request establishes this gate");
        }
    }
}

#[test]
fn size_boundary_qualification_native_notifications_do_not_qualify_as_universal_gates() {
    let rows = support::fixture("codex", "helper-run");
    let started = rows
        .iter()
        .find(|e| !e.input && e.msg["method"] == "item/started" && e.msg["params"]["item"]["type"] == "commandExecution")
        .unwrap();
    assert!(started.msg.get("id").is_none(), "a notification has no response to withhold");
    assert!(started.msg["params"]["item"]["processId"].is_string());
    assert!(!rows.iter().any(|e| !e.input && e.msg["method"] == "item/commandExecution/requestApproval"));
    let mut state = CodexTurnState::default();
    let events: Vec<_> = rows.iter().filter(|e| !e.input).flat_map(|e| parse_codex_notification(&e.msg, &mut state)).collect();
    let id = started.msg["params"]["item"]["id"].as_str().unwrap();
    assert!(events.iter().any(|e| matches!(e, EngineEvent::ToolResult { id: result, ok: true, .. } if result == id)));
    assert!(!events.iter().any(|e| matches!(e, EngineEvent::Permission { .. })));

    let rows = support::fixture("claude", "queued-midturn");
    assert!(rows.iter().any(|e| e.msg["subtype"] == "init" && e.msg["permissionMode"] == "auto"));
    assert!(!rows.iter().any(|e| !e.input && e.msg["request"]["subtype"] == "can_use_tool"));
    let events: Vec<_> = rows.iter().filter(|e| !e.input).flat_map(|e| parse_claude_message(&e.msg)).collect();
    let id = events
        .iter()
        .find_map(|e| match e {
            EngineEvent::Tool { id, name, .. } if name == "Bash" => Some(id),
            _ => None,
        })
        .unwrap();
    assert!(events.iter().any(|e| matches!(e, EngineEvent::ToolResult { id: result, ok: true, .. } if result == id)));
    assert!(!events.iter().any(|e| matches!(e, EngineEvent::Permission { .. })));
}

#[test]
fn size_boundary_qualification_changing_notification_text_cannot_hold_a_boundary_call() {
    // Synthetic substitutions in recorded notifications prove parser behavior, never execution of these commands.
    let codex = support::fixture("codex", "helper-run")
        .into_iter()
        .find(|e| !e.input && e.msg["method"] == "item/started" && e.msg["params"]["item"]["type"] == "commandExecution")
        .unwrap()
        .msg;
    let claude = support::fixture("claude", "queued-midturn")
        .into_iter()
        .find(|e| !e.input && e.msg["type"] == "assistant" && e.msg["message"]["content"][0]["name"] == "Bash")
        .unwrap()
        .msg;
    for command in [
        "cat ../../Faber/settings.json", // Data outside the project.
        "cat ~/.ssh/id_ed25519",         // Credentials, including reads.
        "cat ../outside.txt",            // Leaves the project.
        "git push origin HEAD",          // Publishes.
        "rm -rf ../original",            // Deletes outside a copy.
        "echo $(unknown)",               // Unsupported shell expansion.
        "cat README.md",                 // Plain local read control.
    ] {
        let mut notification = codex.clone();
        notification["params"]["item"]["command"] = command.into();
        let codex_events = parse_codex_notification(&notification, &mut CodexTurnState::default());
        let mut notification = claude.clone();
        notification["message"]["content"][0]["input"]["command"] = command.into();
        let claude_events = parse_claude_message(&notification);
        for events in [codex_events, claude_events] {
            assert!(events.iter().any(|e| matches!(e, EngineEvent::Tool { input, .. } if input.to_string().contains(command))));
            assert!(!events.iter().any(|e| matches!(e, EngineEvent::Permission { .. })), "{command}: observing cannot block");
        }
    }
    let full = codex_thread(Area::Project, None, Permissions::FullAccess);
    assert_eq!(full["approvalPolicy"], "never");
    assert_eq!(full["sandbox"], "danger-full-access");
    assert!(full.get("dynamicTools").is_none());
    assert_eq!(faber_engine::claude::permission_mode(Permissions::FullAccess), "bypassPermissions");
}

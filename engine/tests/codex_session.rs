use crate::support;

use faber_engine::codex::{
    CodexServer, CodexServerOptions, CodexThread, CodexTurnState, codex_args, codex_thread, list_codex_models, parse_codex_notification,
    read_codex_limits, thread_params,
};
use faber_engine::events::{Delivery, EngineEvent, Session, UserTurn};
use faber_engine::model::{Area, Permissions};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use support::{Entry, drive, fake, fixture, next_permission, text_of};

async fn run(
    entries: Vec<Entry>,
    first: &str,
    resume: Option<&str>,
    react: impl FnMut(&EngineEvent, &Session) -> bool,
) -> (Vec<EngineEvent>, Vec<String>) {
    let thread = CodexThread {
        cwd: ".".into(),
        model: None,
        effort: None,
        resume: resume.map(String::from),
        instructions: None,
        user_name: None,
        permissions: Permissions::Ask,
    };
    drive(
        entries,
        |spawn, sink| {
            let opts = CodexServerOptions {
                permissions: Permissions::Ask,
                cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
                cwd: ".".into(),
                area: Area::Chat,
                user_hooks: false,
                windows: false,
                spawn,
            };
            CodexServer::start(opts).open(thread, sink)
        },
        first,
        react,
    )
    .await
}

fn recorded_answers(entries: &[Entry]) -> String {
    entries
        .iter()
        .filter(|e| e.msg["method"] == "item/completed" && e.msg["params"]["item"]["type"] == "agentMessage")
        .map(|e| e.msg["params"]["item"]["text"].as_str().unwrap_or_default().to_string())
        .collect()
}

async fn check_chat(entries: Vec<Entry>) {
    let expected = recorded_answers(&entries);
    let mut dones = 0;
    let (events, mismatches) = run(entries, "In one sentence: why is the sky blue?", None, |e, s| {
        if matches!(e, EngineEvent::Done { .. }) {
            dones += 1;
            if dones == 1 {
                s.send(UserTurn::text("Reply with just the word: pong"));
            }
        }
        dones == 2
    })
    .await;
    assert_eq!(mismatches, Vec::<String>::new());
    assert_eq!(events[0], EngineEvent::Started { session_id: "01a12513-0e42-7b63-9a21-c8698f522c12".into(), model: "gpt-6-luna".into() });
    assert_eq!(text_of(&events), expected);
    assert!(
        events.iter().any(|e| matches!(e, EngineEvent::Quota { window, used, .. } if window == "seven_day" && (*used - 0.47).abs() < 1e-9))
    );
    let done: Vec<&EngineEvent> = events.iter().filter(|e| matches!(e, EngineEvent::Done { .. })).collect();
    assert!(
        done.iter().all(
            |d| matches!(d, EngineEvent::Done { ok: true, input_tokens, output_tokens, .. } if *input_tokens > 0 && *output_tokens > 0)
        )
    );
}

#[tokio::test]
async fn codex_chat_two_turns_with_text_model_quota_and_tokens() {
    check_chat(fixture("codex", "chat")).await;
}

#[tokio::test]
#[should_panic]
async fn codex_a_renamed_field_in_a_recorded_session_turns_the_check_red() {
    let entries = fixture("codex", "chat")
        .into_iter()
        .map(|mut e| {
            if e.msg["method"] == "item/agentMessage/delta" {
                let delta = e.msg["params"]["delta"].take();
                e.msg["params"]["text"] = delta;
                e.msg["params"].as_object_mut().unwrap().remove("delta");
            }
            e
        })
        .collect();
    check_chat(entries).await;
}

#[tokio::test]
async fn codex_permissions_the_first_command_is_allowed_and_the_second_declined() {
    let mut asks = Vec::new();
    let (events, mismatches) = run(
        fixture("codex", "tools"),
        "Create a.txt containing one. Then create b.txt containing two. Make each file in a separate step.",
        None,
        |e, s| {
            if let EngineEvent::Permission { request_id, tool, .. } = e {
                asks.push(tool.clone());
                drop(s.answer(request_id, asks.len() == 1));
            }
            matches!(e, EngineEvent::Done { .. })
        },
    )
    .await;
    assert_eq!(mismatches, Vec::<String>::new());
    assert_eq!(asks, ["command", "command"]);
    let results: Vec<(bool, bool)> = events
        .iter()
        .filter_map(|e| if let EngineEvent::ToolResult { ok, declined, .. } = e { Some((*ok, *declined)) } else { None })
        .collect();
    assert_eq!(results, [(true, false), (false, true)], "a refused approval is declined, not a failure");
}

/// R124b, item 5: the request names only `auth.json`; the command's folder is what says it is the sign-in. The driver refuses it
/// itself (a refusal is always safe, so nothing races a mode switch) and the person is asked only about the other command.
#[tokio::test]
async fn r124b_a_command_run_inside_a_credential_folder_is_refused_without_a_card() {
    let mut entries = fixture("codex", "tools");
    let at = entries.iter().position(|e| !e.input && e.msg["method"] == "item/commandExecution/requestApproval").unwrap();
    entries[at].msg["params"]["cwd"] = "C:\\Users\\me\\.codex".into();
    entries[at].msg["params"]["command"] = "type auth.json".into();
    entries[at + 1].msg["result"]["decision"] = "decline".into();
    let mut asks = Vec::new();
    let (_, mismatches) =
        run(entries, "Create a.txt containing one. Then create b.txt containing two. Make each file in a separate step.", None, |e, s| {
            if let EngineEvent::Permission { request_id, input, .. } = e {
                asks.push(input.to_string());
                drop(s.answer(request_id, false));
            }
            matches!(e, EngineEvent::Done { .. })
        })
        .await;
    assert_eq!(mismatches, Vec::<String>::new());
    assert_eq!(asks.len(), 1, "only the second command is asked: {asks:?}");
    assert!(!asks[0].contains("auth.json"));
}

#[tokio::test]
async fn codex_an_answer_that_fails_to_write_keeps_the_approval_and_the_retry_is_written_once() {
    let f = fake(fixture("codex", "tools"));
    let opts = CodexServerOptions {
        permissions: Permissions::Ask,
        cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
        cwd: ".".into(),
        area: Area::Chat,
        user_hooks: false,
        windows: false,
        spawn: f.spawn.clone(),
    };
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let thread = CodexThread {
        cwd: ".".into(),
        model: None,
        effort: None,
        resume: None,
        instructions: None,
        user_name: None,
        permissions: Permissions::Ask,
    };
    let session = CodexServer::start(opts).open(thread, Arc::new(move |e| drop(tx.send(e))));
    session.send(UserTurn::text("Create a.txt containing one. Then create b.txt containing two. Make each file in a separate step."));
    let request = next_permission(&mut rx).await;
    assert_eq!(request, "0", "Codex numbers its requests from 0 in each server");
    let written = || f.wire.lines_with("\"decision\"");

    f.wire.refuse_next(1);
    assert_eq!(session.answer(&request, true).await, Ok(Delivery::WriteFailed));
    assert_eq!(written(), 0, "the refused write left nothing on the wire");
    // The thread still holds the approval, so the same answer can be given again.
    assert_eq!(session.answer(&request, true).await, Ok(Delivery::Delivered));
    assert_eq!(written(), 1);
    // An approval the thread never held, and one it has already delivered, write nothing.
    assert_eq!(session.answer("7", true).await, Ok(Delivery::Unknown));
    assert_eq!(session.answer(&request, true).await, Ok(Delivery::Unknown));
    assert_eq!(written(), 1);
    session.close();
    assert_eq!(*f.mismatches.lock().unwrap(), Vec::<String>::new());
}

#[tokio::test]
async fn codex_an_answer_that_may_be_half_written_is_never_written_again() {
    let f = fake(fixture("codex", "tools"));
    let opts = CodexServerOptions {
        permissions: Permissions::Ask,
        cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
        cwd: ".".into(),
        area: Area::Chat,
        user_hooks: false,
        windows: false,
        spawn: f.spawn.clone(),
    };
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let thread = CodexThread {
        cwd: ".".into(),
        model: None,
        effort: None,
        resume: None,
        instructions: None,
        user_name: None,
        permissions: Permissions::Ask,
    };
    let session = CodexServer::start(opts).open(thread, Arc::new(move |e| drop(tx.send(e))));
    session.send(UserTurn::text("Create a.txt containing one. Then create b.txt containing two. Make each file in a separate step."));
    let request = next_permission(&mut rx).await;
    assert_eq!(request, "0", "Codex numbers its requests from 0 in each server");
    let before = f.wire.len();

    f.wire.tear_next(10);
    assert_eq!(session.answer(&request, true).await, Ok(Delivery::WriteFailed));
    let after = f.wire.len();
    assert!(after > before, "the first attempt put bytes on the wire");

    // The same answer again must not write a byte: the shared process is ending, for every thread in it.
    assert_ne!(session.answer(&request, true).await, Ok(Delivery::Delivered));
    assert_eq!(f.wire.len(), after, "nothing was written again");
    let ended = loop {
        match tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.expect("the session did not end") {
            Some(EngineEvent::Error { fatal: true, .. }) | None => break true,
            Some(_) => {}
        }
    };
    assert!(ended);
}

#[tokio::test]
async fn codex_interrupt_stops_the_turn_and_the_thread_takes_the_next_one() {
    let (mut deltas, mut dones) = (0, 0);
    let (events, mismatches) = run(fixture("codex", "interrupt"), "Write a 600-word essay about the history of bread.", None, |e, s| {
        if matches!(e, EngineEvent::Text(_)) {
            deltas += 1;
            if deltas == 5 {
                s.interrupt();
            }
        }
        if matches!(e, EngineEvent::Done { .. }) {
            dones += 1;
            if dones == 1 {
                s.send(UserTurn::text("Reply with just the word: pong"));
            }
        }
        dones == 2
    })
    .await;
    assert_eq!(mismatches, Vec::<String>::new());
    let done: Vec<(bool, String)> = events
        .iter()
        .filter_map(|e| if let EngineEvent::Done { ok, reason, .. } = e { Some((*ok, reason.clone())) } else { None })
        .collect();
    assert_eq!(done[0], (false, "interrupted".to_string()));
    assert!(done[1].0);
}

#[tokio::test]
async fn codex_resume_a_new_process_continues_the_same_thread() {
    let id = "01a1251a-252b-7031-bcb9-dd0b0adfc7ce";
    let (events, mismatches) =
        run(fixture("codex", "resume"), "What single word did you reply with last time? Answer with only that word.", Some(id), |e, _| {
            matches!(e, EngineEvent::Done { .. })
        })
        .await;
    assert_eq!(mismatches, Vec::<String>::new());
    assert!(matches!(&events[0], EngineEvent::Started { session_id, .. } if session_id == id));
    assert_eq!(text_of(&events).trim(), "pong");
}

#[test]
fn codex_a_message_that_arrives_whole_without_deltas_is_still_shown_once() {
    let mut state = CodexTurnState::default();
    let done = |id: &str| json!({ "method": "item/completed", "params": { "item": { "type": "agentMessage", "id": id, "text": "hi" } } });
    assert_eq!(parse_codex_notification(&done("a"), &mut state), [EngineEvent::Text("hi".into())]);
    parse_codex_notification(&json!({ "method": "item/agentMessage/delta", "params": { "itemId": "b", "delta": "hi" } }), &mut state);
    assert_eq!(parse_codex_notification(&done("b"), &mut state), []);
}

#[test]
fn codex_chat_is_lean_and_sandboxed_projects_keep_the_user_setup_without_hooks() {
    let chat = codex_args(Area::Chat, false, false, Permissions::Ask).join(" ");
    assert!(chat.contains("--disable plugins") && chat.contains("mcp_servers={}") && chat.contains("project_doc_max_bytes=0"));
    assert!(chat.contains("--disable hooks") && !chat.contains("windows.sandbox"));
    let project = codex_args(Area::Project, true, false, Permissions::Ask).join(" ");
    assert!(project.contains("windows.sandbox=\"unelevated\"") && project.contains("--disable hooks"));
    assert!(!project.contains("plugins") && !project.contains("mcp_servers"));
    assert!(!codex_args(Area::Project, false, true, Permissions::Ask).join(" ").contains("hooks"));
    for area in [Area::Chat, Area::Project] {
        assert_eq!(codex_thread(area, None, Permissions::Ask)["sandbox"], "workspace-write");
        // "untrusted" asks before every command that is not a plain read and before every file change, inside the folder too
        // (checked against the installed Codex: "on-request" let sandboxed commands in the folder run unasked).
        assert_eq!(codex_thread(area, None, Permissions::Ask)["approvalPolicy"], "untrusted");
    }
    assert!(codex_thread(Area::Chat, None, Permissions::Ask).contains_key("baseInstructions"));
}

#[tokio::test]
async fn review3_user_mcp_servers_are_disabled_before_start_and_resume_without_copying_values() {
    for area in [Area::Chat, Area::Project] {
        for resume in [None, Some("thread".into())] {
            let method = if resume.is_some() { "thread/resume" } else { "thread/start" };
            let entries = vec![
                Entry { input: true, msg: json!({"id":1,"method":"initialize"}) },
                Entry { input: false, msg: json!({"id":1,"result":{}}) },
                Entry { input: true, msg: json!({"method":"initialized"}) },
                Entry { input: true, msg: json!({"id":"faber-config-1","method":"config/read"}) },
                Entry {
                    input: false,
                    msg: json!({"id":"faber-config-1","result":{"config":{"mcp_servers":{"github.push":{"env":{"TOKEN":"synthetic-do-not-copy"}}}}}}),
                },
                Entry { input: true, msg: json!({"id":2,"method":method}) },
                Entry { input: false, msg: json!({"id":2,"result":{"thread":{"id":"thread","model":"model"}}}) },
            ];
            let f = fake(entries);
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let server = CodexServer::start(CodexServerOptions {
                permissions: Permissions::Isolated,
                cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
                cwd: ".".into(),
                area,
                user_hooks: false,
                windows: true,
                spawn: f.spawn.clone(),
            });
            let session = server.open(
                CodexThread {
                    cwd: ".".into(),
                    model: None,
                    effort: None,
                    resume,
                    instructions: None,
                    user_name: None,
                    permissions: Permissions::Isolated,
                },
                Arc::new(move |e| drop(tx.send(e))),
            );
            tokio::time::timeout(Duration::from_secs(3), async { while !matches!(rx.recv().await, Some(EngineEvent::Started { .. })) {} })
                .await
                .unwrap();
            let messages = f.wire.messages();
            let thread = messages.iter().find(|msg| msg["method"] == method).unwrap();
            assert_eq!(thread["params"]["config"]["mcp_servers"]["github.push"], json!({"enabled":false}));
            assert!(!serde_json::to_string(&messages).unwrap().contains("synthetic-do-not-copy"));
            assert!(f.mismatches.lock().unwrap().is_empty());
            session.close();
            server.retire();
        }
    }
}

#[tokio::test]
async fn the_chatgpt_model_list_comes_from_the_account_without_hidden_models() {
    let f = fake(fixture("codex", "models"));
    let models = list_codex_models("codex", ".".as_ref(), false, f.spawn.clone(), Duration::from_secs(3)).await;
    assert_eq!(*f.mismatches.lock().unwrap(), Vec::<String>::new());
    let got: Vec<(String, String, bool)> = models.into_iter().map(|m| (m.id, m.label, m.is_default)).collect();
    assert_eq!(got, [("gpt-6.1-sol".into(), "GPT-6.1-Sol".into(), true), ("gpt-6-astra".into(), "GPT-6-Astra".into(), false)]);
}

#[tokio::test]
async fn a_cli_that_never_answers_gives_an_empty_list_instead_of_hanging() {
    let f = fake(vec![]);
    let models = list_codex_models("codex", ".".as_ref(), false, f.spawn.clone(), Duration::from_millis(50)).await;
    assert!(models.is_empty());
}

#[tokio::test]
async fn one_process_serves_two_conversations_at_once_and_keeps_their_answers_apart() {
    let f = fake(fixture("codex", "two"));
    let opts = CodexServerOptions {
        permissions: Permissions::Ask,
        cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
        cwd: ".".into(),
        area: Area::Chat,
        user_hooks: false,
        windows: false,
        spawn: f.spawn.clone(),
    };
    let server = CodexServer::start(opts);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let open = |name: &'static str| {
        let tx = tx.clone();
        let thread = CodexThread {
            cwd: ".".into(),
            model: None,
            effort: Some("medium".into()),
            resume: None,
            instructions: None,
            user_name: None,
            permissions: Permissions::Ask,
        };
        server.open(thread, Arc::new(move |e| drop(tx.send((name, e)))))
    };
    let (a, b) = (open("a"), open("b"));
    a.send(UserTurn::text("In one word: what color is the sky on a clear day?"));
    b.send(UserTurn::text("In one word: what color is fresh grass?"));
    let (mut events_a, mut events_b) = (vec![], vec![]);
    let done = |events: &Vec<EngineEvent>| events.iter().any(|e| matches!(e, EngineEvent::Done { .. }));
    while !(done(&events_a) && done(&events_b)) {
        let (name, e) = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.expect("stalled").expect("server gone");
        if name == "a" { events_a.push(e) } else { events_b.push(e) }
    }
    a.close();
    b.close();
    assert_eq!(*f.mismatches.lock().unwrap(), Vec::<String>::new());
    assert_eq!(f.args.lock().unwrap().len(), 1, "both conversations share one process");
    assert_eq!(text_of(&events_a).trim(), "Blue");
    assert_eq!(text_of(&events_b).trim(), "Green");
    let started = |events: &[EngineEvent]| {
        events.iter().find_map(|e| if let EngineEvent::Started { session_id, .. } = e { Some(session_id.clone()) } else { None })
    };
    assert_ne!(started(&events_a), started(&events_b));
}

#[test]
fn a_side_question_thread_has_its_own_instructions_restricted_reads_no_writes_and_is_not_saved() {
    let thread = CodexThread {
        cwd: ".".into(),
        model: Some("gpt-6-luna".into()),
        effort: None,
        resume: None,
        instructions: Some("Route.".into()),
        user_name: None,
        permissions: Permissions::Ask,
    };
    let (method, params) = thread_params(Area::Chat, &thread);
    assert_eq!(method, "thread/start");
    assert_eq!((&params["baseInstructions"], &params["approvalPolicy"]), (&"Route.".into(), &"never".into()));
    assert_eq!(params["model"], "gpt-6-luna");
    assert_eq!(params["ephemeral"], true, "it stays out of the user's Codex history");
}

#[test]
fn permissions_keep_the_workspace_sandbox_and_disable_project_network_without_approvals() {
    for area in [Area::Chat, Area::Project] {
        for permissions in [Permissions::Isolated, Permissions::Ask, Permissions::FullAccess] {
            for resume in [None, Some("thread-id".into())] {
                let t = CodexThread {
                    cwd: "project".into(),
                    model: Some("model".into()),
                    effort: None,
                    resume,
                    instructions: None,
                    user_name: None,
                    permissions,
                };
                let (method, params) = thread_params(area, &t);
                assert_eq!(method, if t.resume.is_some() { "thread/resume" } else { "thread/start" });
                assert_eq!(params["cwd"], "project");
                assert_eq!(params["model"], "model");
                if permissions == Permissions::Isolated {
                    assert!(params.get("sandbox").is_none());
                    assert_eq!(params["approvalPolicy"], "never");
                    assert_eq!(params["config"]["sandbox_workspace_write.network_access"], false);
                } else if permissions == Permissions::FullAccess {
                    assert_eq!((&params["sandbox"], &params["approvalPolicy"]), (&"danger-full-access".into(), &"never".into()));
                    assert!(params.get("config").is_none() && params.get("dynamicTools").is_none());
                } else {
                    assert_eq!(params["sandbox"], "workspace-write");
                    assert_eq!(params["approvalPolicy"], "untrusted");
                    assert!(params.get("config").is_none());
                }
                assert_eq!(params.get("baseInstructions").is_some(), area == Area::Chat);
                let (_, side) = thread_params(area, &CodexThread { instructions: Some("Route.".into()), ..t });
                assert!(side.get("sandbox").is_none());
                assert_eq!(side["approvalPolicy"], "never");
                let id = side["config"]["default_permissions"].as_str().unwrap();
                assert_eq!(side["config"][format!("permissions.{id}")]["filesystem"][":workspace_roots"]["."], "read");
            }
        }
    }
}

#[tokio::test]
async fn l2_serialized_start_and_resume_carry_the_no_approval_offline_policy() {
    for area in [Area::Chat, Area::Project] {
        for resume in [false, true] {
            let f = fake(fixture("codex", if resume { "resume" } else { "chat" }));
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let server = CodexServer::start(CodexServerOptions {
                permissions: Permissions::Ask,
                cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
                cwd: ".".into(),
                area,
                user_hooks: false,
                windows: false,
                spawn: f.spawn.clone(),
            });
            let session = server.open(
                CodexThread {
                    cwd: ".".into(),
                    model: None,
                    effort: None,
                    resume: resume.then(|| "thread-id".into()),
                    instructions: None,
                    user_name: None,
                    permissions: Permissions::Isolated,
                },
                Arc::new(move |e| {
                    let _ = tx.send(e);
                }),
            );
            loop {
                let e = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.unwrap().unwrap();
                if matches!(e, EngineEvent::Started { .. }) {
                    break;
                }
            }
            let messages = f.wire.messages();
            let message = messages.iter().find(|m| m["method"] == if resume { "thread/resume" } else { "thread/start" }).unwrap();
            assert_eq!(message["params"]["approvalPolicy"], "never");
            assert!(message["params"].get("sandbox").is_none());
            let id = message["params"]["config"]["default_permissions"].as_str().unwrap();
            assert_eq!(message["params"]["config"][format!("permissions.{id}")]["filesystem"][":root"], "deny");
            assert_eq!(message["params"]["config"]["sandbox_workspace_write.network_access"], false);
            assert_eq!(message["params"]["config"]["shell_environment_policy.set"]["GIT_ALLOW_PROTOCOL"], "");
            session.close();
        }
    }
}

#[tokio::test]
async fn a_project_with_no_approvals_declines_a_residual_request_without_emitting_a_card() {
    for area in [Area::Chat, Area::Project] {
        let entries = fixture("codex", "tools")
            .into_iter()
            .map(|mut e| {
                if e.input && e.msg.get("result").is_some() && e.msg["result"].get("decision").is_some() {
                    e.msg["result"]["decision"] = "decline".into();
                }
                e
            })
            .collect();
        let (events, mismatches) = drive(
            entries,
            |spawn, sink| {
                let server = CodexServer::start(CodexServerOptions {
                    permissions: Permissions::Ask,
                    cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
                    cwd: ".".into(),
                    area,
                    user_hooks: false,
                    windows: false,
                    spawn,
                });
                server.open(
                    CodexThread {
                        cwd: ".".into(),
                        model: None,
                        effort: None,
                        resume: None,
                        instructions: None,
                        user_name: None,
                        permissions: Permissions::Isolated,
                    },
                    sink,
                )
            },
            "Create a.txt containing one. Then create b.txt containing two. Make each file in a separate step.",
            |event, _| matches!(event, EngineEvent::Done { .. }),
        )
        .await;
        assert!(mismatches.is_empty(), "{mismatches:?}");
        assert!(!events.iter().any(|e| matches!(e, EngineEvent::Permission { .. })));
    }
}

#[test]
fn the_name_is_a_developer_instruction_that_resume_repeats_and_a_side_question_never_gets() {
    let thread = |resume: Option<&str>, instructions: Option<&str>, name: Option<&str>| CodexThread {
        cwd: ".".into(),
        model: None,
        effort: None,
        resume: resume.map(String::from),
        instructions: instructions.map(String::from),
        user_name: name.map(String::from),
        permissions: Permissions::Ask,
    };
    let (method, start) = thread_params(Area::Project, &thread(None, None, Some("Erick")));
    assert_eq!(method, "thread/start");
    assert!(start["developerInstructions"].as_str().unwrap().contains("called Erick."));
    assert!(start.get("baseInstructions").is_none(), "a project keeps Codex's own prompt");
    let (method, resume) = thread_params(Area::Project, &thread(Some("t1"), None, Some("Erick")));
    assert_eq!((method, &resume["developerInstructions"]), ("thread/resume", &start["developerInstructions"]));
    let (_, chat) = thread_params(Area::Chat, &thread(None, None, Some("Erick")));
    assert!(chat["baseInstructions"].as_str().unwrap().starts_with("You are a helpful"));
    assert_eq!(chat["developerInstructions"], start["developerInstructions"]);
    let (_, none) = thread_params(Area::Chat, &thread(None, None, None));
    assert!(none.get("developerInstructions").is_none());
    let (_, side) = thread_params(Area::Chat, &thread(None, Some("Route."), Some("Erick")));
    assert!(side.get("developerInstructions").is_none());
}

const COUNT: &str = "Count from 1 to 30, one number per line, each followed by a five-word comment.";
const STEER: &str = "stop at 10";

/// Sends a steer when the first words of the answer arrive and runs the session to the end of the turn.
async fn steer_after_first_words(fixture_name: &str, until: fn(&EngineEvent) -> bool) -> (Vec<EngineEvent>, Vec<String>) {
    let mut steered = false;
    run(fixture("codex", fixture_name), COUNT, None, |e, s| {
        if matches!(e, EngineEvent::Text(_)) && !steered {
            steered = true;
            s.steer(UserTurn::text(STEER), "faber-probe-1");
        }
        until(e)
    })
    .await
}

#[tokio::test]
async fn a_steered_message_reaches_the_turn_and_the_driver_says_when_codex_took_it_in() {
    let (events, mismatches) = steer_after_first_words("steer", |e| matches!(e, EngineEvent::Done { .. })).await;
    assert_eq!(mismatches, Vec::<String>::new());
    // Live capture (codex-cli 0.162.1): Codex echoes the client id on the steered user message, between the answer it
    // cut short and the new answer, which it begins with a new agent message item.
    let injected: Vec<_> = events.iter().enumerate().filter(|(_, e)| matches!(e, EngineEvent::Injected { .. })).collect();
    assert_eq!(injected.len(), 1);
    assert_eq!(injected[0].1, &EngineEvent::Injected { tag: Some("faber-probe-1".into()) });
    let before = text_of(&events[..injected[0].0]);
    let after = text_of(&events[injected[0].0..]);
    assert!(before.starts_with("1 #"), "{before:?}");
    assert!(after.contains("Counting starts") && after.trim_end().ends_with("as you asked."), "{after:?}");
    assert!(!events.iter().any(|e| matches!(e, EngineEvent::SteerFailed { .. } | EngineEvent::Error { .. })));
}

#[tokio::test]
async fn a_steer_codex_refuses_is_reported_as_not_delivered_and_is_no_error() {
    for name in ["steer-rejected", "steer-late"] {
        let (events, mismatches) = steer_after_first_words(name, |e| matches!(e, EngineEvent::SteerFailed { .. })).await;
        assert_eq!(mismatches, Vec::<String>::new(), "{name}");
        assert!(events.contains(&EngineEvent::SteerFailed { tag: "faber-probe-1".into() }), "{name}: {events:?}");
        assert!(!events.iter().any(|e| matches!(e, EngineEvent::Injected { .. } | EngineEvent::Error { .. })), "{name}");
    }
}

#[tokio::test]
async fn a_steer_before_the_turn_has_an_id_is_not_delivered() {
    // The thread exists but no turn/started came yet, so there is nothing to name in expectedTurnId.
    let entries: Vec<Entry> = fixture("codex", "steer").into_iter().take(8).collect();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let f = fake(entries);
    let thread = CodexThread {
        cwd: ".".into(),
        model: None,
        effort: None,
        resume: None,
        instructions: None,
        user_name: None,
        permissions: Permissions::Ask,
    };
    let opts = CodexServerOptions {
        permissions: Permissions::Ask,
        cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
        cwd: ".".into(),
        area: Area::Chat,
        user_hooks: false,
        windows: false,
        spawn: f.spawn.clone(),
    };
    let session = CodexServer::start(opts).open(thread, Arc::new(move |e| drop(tx.send(e))));
    session.steer(UserTurn::text("early"), "q9");
    let seen = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(EngineEvent::SteerFailed { tag }) = rx.recv().await {
                return tag;
            }
        }
    })
    .await
    .expect("the driver never said the steer was dropped");
    assert_eq!(seen, "q9");
    session.close();
}

#[test]
fn review3_dynamic_command_failures_keep_their_output_and_failed_status() {
    let events = parse_codex_notification(
        &json!({"method":"item/completed","params":{"item":{
            "id":"tool","type":"dynamicToolCall","status":"completed","success":false,
            "contentItems":[{"type":"inputText","text":"sandbox refused"}]
        }}}),
        &mut CodexTurnState::default(),
    );
    assert_eq!(events, vec![EngineEvent::ToolResult { id: "tool".into(), ok: false, declined: false, text: "sandbox refused".into() }]);
}

#[tokio::test]
async fn review3_command_gate_declines_git_mutations_and_runs_builds_only_in_the_sandbox() {
    for area in [Area::Chat, Area::Project] {
        let root = tempfile::tempdir().unwrap();
        let mut entries = vec![
            Entry { input: true, msg: json!({"id":1,"method":"initialize"}) },
            Entry { input: false, msg: json!({"id":1,"result":{}}) },
            Entry { input: true, msg: json!({"method":"initialized"}) },
            Entry { input: true, msg: json!({"id":2,"method":"thread/start"}) },
            Entry { input: false, msg: json!({"id":2,"result":{"thread":{"id":"thread","model":"model"}}}) },
        ];
        for (i, command) in ["git reset --hard HEAD~1", "git clean -fdx", "git rebase", "git push", "cargo test"].into_iter().enumerate() {
            entries.push(Entry{input:false,msg:json!({"id":format!("call-{i}"),"method":"item/tool/call","params":{"threadId":"thread","tool":"run_command","arguments":{"command":command}}})});
            entries.push(Entry { input: true, msg: json!({"id":format!("call-{i}"),"result":{"success":i==4}}) });
        }
        let f = fake(entries);
        let argv = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (start, capture) = (f.spawn.clone(), argv.clone());
        let spawn: faber_engine::proc::Spawner = Arc::new(move |bin, args, cwd| {
            if args.first().is_some_and(|arg| arg == "app-server") {
                return start(bin, args, cwd);
            }
            capture.lock().unwrap().push(args.to_vec());
            Ok(faber_engine::proc::Proc {
                stdin: Box::new(tokio::io::sink()),
                stdout: Box::new(std::io::Cursor::new(b"build completed".to_vec())),
                stderr: None,
                child: None,
                contained: None,
            })
        });
        let server = CodexServer::start(CodexServerOptions {
            cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
            cwd: root.path().into(),
            area,
            user_hooks: false,
            windows: true,
            permissions: Permissions::Isolated,
            spawn,
        });
        let session = server.open(
            CodexThread {
                cwd: root.path().into(),
                model: None,
                effort: None,
                resume: None,
                instructions: None,
                user_name: None,
                permissions: Permissions::Isolated,
            },
            Arc::new(|_| {}),
        );
        tokio::time::timeout(Duration::from_secs(3), async {
            while f.wire.lines_with("\"id\":\"call-") < 5 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let messages = f.wire.messages();
        for i in 0..5 {
            let reply = messages.iter().find(|msg| msg["id"] == format!("call-{i}")).unwrap();
            assert_eq!(reply["result"]["success"], i == 4);
        }
        let argv = argv.lock().unwrap();
        assert_eq!(argv.len(), 1, "only the build may start a child");
        assert!(argv[0].iter().any(|arg| arg == "sandbox"));
        assert!(argv[0].ends_with(&["cargo".into(), "test".into()]));
        assert!(f.mismatches.lock().unwrap().is_empty());
        session.close();
        server.retire();
    }
}

#[test]
fn only_a_user_message_after_the_first_of_a_turn_counts_as_taken_in() {
    let started = |client: serde_json::Value| json!({ "method": "item/started", "params": { "item": { "type": "userMessage", "id": "u", "clientId": client, "content": [] } } });
    let mut state = CodexTurnState::default();
    assert!(parse_codex_notification(&started(json!(null)), &mut state).is_empty(), "the turn's own prompt");
    let again = parse_codex_notification(&started(json!(null)), &mut state);
    assert_eq!(again, vec![EngineEvent::Injected { tag: None }]);
    // Our own tag, echoed back, is enough even if the first message was not seen.
    let mut fresh = CodexTurnState::default();
    let tagged = parse_codex_notification(&started(json!("faber-probe-1")), &mut fresh);
    assert_eq!(tagged, vec![EngineEvent::Injected { tag: Some("faber-probe-1".into()) }]);
    // A new turn starts counting again.
    parse_codex_notification(&json!({ "method": "turn/started", "params": { "turn": { "id": "t2" } } }), &mut state);
    assert!(parse_codex_notification(&started(json!(null)), &mut state).is_empty());
}

#[test]
fn r121_full_access_starts_codex_as_the_person_would_and_isolated_keeps_every_restriction() {
    for area in [Area::Chat, Area::Project] {
        for user_hooks in [false, true] {
            // Nothing disabled, no sandbox choice, no emptied MCP list, no cut-off project instructions.
            assert_eq!(codex_args(area, true, user_hooks, Permissions::FullAccess), ["app-server"], "{area:?}");
        }
        let isolated = codex_args(area, true, true, Permissions::Isolated);
        for feature in ["apps", "plugins", "computer_use", "browser_use", "hooks", "skill_mcp_dependency_install"] {
            assert!(isolated.windows(2).any(|pair| pair == ["--disable", feature]), "{area:?} {feature}");
        }
        assert!(isolated.contains(&"mcp_servers={}".into()) && isolated.contains(&"windows.sandbox=\"mxc\"".into()));
    }
}

/// Codex's only filesystem deny rules live in a permission profile. On Windows a profile that grants the whole disk and denies
/// the credential folders is refused ("MXC cannot verify it does not alias a denied object" when a drive letter is unreachable,
/// "requires the elevated Windows sandbox" otherwise), so full access sends no profile and no credential block exists for Codex.
/// This test states that limit; docs/security-r97-review.md and the Settings text say it to the person.
#[test]
fn r121_codex_full_access_has_no_sandbox_no_approvals_and_no_host_command_tool() {
    for area in [Area::Chat, Area::Project] {
        for resume in [None, Some("thread".into())] {
            let t = CodexThread {
                cwd: "project".into(),
                model: None,
                effort: None,
                resume,
                instructions: None,
                user_name: None,
                permissions: Permissions::FullAccess,
            };
            let (_, params) = thread_params(area, &t);
            assert_eq!((&params["sandbox"], &params["approvalPolicy"]), (&"danger-full-access".into(), &"never".into()), "{area:?}");
            for key in ["config", "dynamicTools", "runtimeWorkspaceRoots"] {
                assert!(params.get(key).is_none(), "{area:?} {key}: full access sends no profile, no tool override and no workspace roots");
            }
        }
    }
}

#[test]
fn r121_a_side_question_stays_isolated_whatever_mode_the_person_chose() {
    for permissions in [Permissions::FullAccess, Permissions::Ask, Permissions::Isolated] {
        let t = CodexThread {
            cwd: ".".into(),
            model: None,
            effort: None,
            resume: None,
            instructions: Some("Route.".into()),
            user_name: None,
            permissions,
        };
        let (_, params) = thread_params(Area::Chat, &t);
        assert!(params.get("sandbox").is_none() && params.get("dynamicTools").is_none(), "{permissions:?}");
        assert_eq!(params["approvalPolicy"], "never");
        assert_eq!(params["ephemeral"], true);
        assert_eq!(params["config"]["features.shell_tool"], false, "{permissions:?}");
        let id = params["config"]["default_permissions"].as_str().unwrap();
        let profile = &params["config"][format!("permissions.{id}")];
        assert_eq!(profile["network"]["enabled"], false, "{permissions:?}");
        assert_eq!(profile["filesystem"][":workspace_roots"]["."], "read");
        assert_eq!(profile["filesystem"]["~/.codex"], "deny");
    }
}

#[tokio::test]
async fn r121_the_model_list_and_the_limits_reading_start_an_isolated_server() {
    let f = fake(vec![]);
    let _ = list_codex_models("codex", ".".as_ref(), true, f.spawn.clone(), Duration::from_millis(50)).await;
    let _ = read_codex_limits("codex", ".".as_ref(), true, f.spawn.clone(), Duration::from_millis(50)).await;
    let args = f.args.lock().unwrap();
    assert_eq!(args.len(), 2);
    for started in args.iter() {
        assert_eq!(*started, codex_args(Area::Chat, true, false, Permissions::Isolated));
        assert!(started.contains(&"mcp_servers={}".into()) && started.contains(&"windows.sandbox=\"mxc\"".into()));
    }
}

#[tokio::test]
async fn r121_a_full_access_thread_keeps_the_persons_mcp_servers_and_starts_without_a_sandbox() {
    for resume in [None, Some("thread".to_string())] {
        let method = if resume.is_some() { "thread/resume" } else { "thread/start" };
        let entries = vec![
            Entry { input: true, msg: json!({"id":1,"method":"initialize"}) },
            Entry { input: false, msg: json!({"id":1,"result":{}}) },
            Entry { input: true, msg: json!({"method":"initialized"}) },
            Entry { input: true, msg: json!({"id":2,"method":method}) },
            Entry { input: false, msg: json!({"id":2,"result":{"thread":{"id":"thread","model":"model"}}}) },
        ];
        let f = fake(entries);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let server = CodexServer::start(CodexServerOptions {
            permissions: Permissions::FullAccess,
            cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
            cwd: ".".into(),
            area: Area::Chat,
            user_hooks: false,
            windows: true,
            spawn: f.spawn.clone(),
        });
        let session = server.open(
            CodexThread {
                cwd: ".".into(),
                model: None,
                effort: None,
                resume,
                instructions: None,
                user_name: None,
                permissions: Permissions::FullAccess,
            },
            Arc::new(move |e| drop(tx.send(e))),
        );
        tokio::time::timeout(Duration::from_secs(3), async { while !matches!(rx.recv().await, Some(EngineEvent::Started { .. })) {} })
            .await
            .unwrap();
        assert_eq!(f.wire.lines_with("config/read"), 0, "the MCP servers are not listed in order to be switched off");
        assert_eq!(f.args.lock().unwrap()[0], ["app-server"]);
        let messages = f.wire.messages();
        let thread = messages.iter().find(|msg| msg["method"] == method).unwrap();
        assert_eq!(thread["params"]["sandbox"], "danger-full-access");
        assert!(thread["params"].get("config").is_none());
        assert!(f.mismatches.lock().unwrap().is_empty());
        session.close();
        server.retire();
    }
}

#[tokio::test]
async fn r124_a_full_access_thread_hands_a_residual_approval_to_the_engine_and_never_answers_it_alone() {
    for area in [Area::Chat, Area::Project] {
        let entries = fixture("codex", "tools")
            .into_iter()
            .map(|mut e| {
                if e.input && e.msg.get("result").is_some() && e.msg["result"].get("decision").is_some() {
                    e.msg["result"]["decision"] = "accept".into();
                }
                e
            })
            .collect();
        let (events, mismatches) = drive(
            entries,
            |spawn, sink| {
                let server = CodexServer::start(CodexServerOptions {
                    permissions: Permissions::FullAccess,
                    cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
                    cwd: ".".into(),
                    area,
                    user_hooks: false,
                    windows: false,
                    spawn,
                });
                server.open(
                    CodexThread {
                        cwd: ".".into(),
                        model: None,
                        effort: None,
                        resume: None,
                        instructions: None,
                        user_name: None,
                        permissions: Permissions::FullAccess,
                    },
                    sink,
                )
            },
            "Create a.txt containing one. Then create b.txt containing two. Make each file in a separate step.",
            |event, session| {
                // What the engine does for full access: it answers yes at once. The driver itself writes nothing before.
                if let EngineEvent::Permission { request_id, .. } = event {
                    drop(session.answer(request_id, true));
                }
                matches!(event, EngineEvent::Done { .. })
            },
        )
        .await;
        assert!(mismatches.is_empty(), "{mismatches:?}");
        assert!(events.iter().any(|e| matches!(e, EngineEvent::Permission { .. })), "the request reaches the engine, which decides");
    }
}

#[test]
fn codex_work_log_items_from_a_recorded_turn_keep_their_raw_shape_for_the_screen_to_tidy() {
    let mut state = CodexTurnState::default();
    let events: Vec<EngineEvent> = fixture("codex", "worklog").iter().flat_map(|e| parse_codex_notification(&e.msg, &mut state)).collect();
    let thinking: String = events.iter().filter_map(|e| if let EngineEvent::Thinking(t) = e { Some(t.as_str()) } else { None }).collect();
    assert_eq!(thinking, "**Minimizing output verbosity****Assessing time budget and test approach****Preparing for final retry**");
    let tool = events.iter().find_map(|e| if let EngineEvent::Tool { name, input, .. } = e { Some((name, input)) } else { None });
    let (name, input) = tool.expect("the command item becomes a tool step");
    assert_eq!(name, "commandExecution");
    let command = input.as_str().expect("a command is a string");
    assert!(command.starts_with("\"C:\\\\WINDOWS\\\\System32\\\\WindowsPowerShell"), "{command}");
    assert!(command.contains("Set-Content -LiteralPath 'a.txt'"), "{command}");
    let text: String = events.iter().filter_map(|e| if let EngineEvent::Text(t) = e { Some(t.as_str()) } else { None }).collect();
    assert_eq!(text.matches("The window control did not connect.").count(), 2, "the engine keeps both; the screen shows it once");
}

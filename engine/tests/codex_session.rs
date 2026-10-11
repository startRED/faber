mod support;

use faber_engine::codex::{
    CodexServer, CodexServerOptions, CodexThread, CodexTurnState, codex_args, codex_thread, list_codex_models, parse_codex_notification,
    thread_params,
};
use faber_engine::events::{EngineEvent, Session, UserTurn};
use faber_engine::model::Area;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use support::{Entry, drive, fake, fixture, text_of};

async fn run(
    entries: Vec<Entry>,
    first: &str,
    resume: Option<&str>,
    react: impl FnMut(&EngineEvent, &Session) -> bool,
) -> (Vec<EngineEvent>, Vec<String>) {
    let thread = CodexThread { cwd: ".".into(), model: None, effort: None, resume: resume.map(String::from), instructions: None };
    drive(
        entries,
        |spawn, sink| {
            let opts =
                CodexServerOptions { bin: "codex".into(), cwd: ".".into(), area: Area::Chat, user_hooks: false, windows: false, spawn };
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
                s.answer(request_id, asks.len() == 1);
            }
            matches!(e, EngineEvent::Done { .. })
        },
    )
    .await;
    assert_eq!(mismatches, Vec::<String>::new());
    assert_eq!(asks, ["command", "command"]);
    let results: Vec<bool> =
        events.iter().filter_map(|e| if let EngineEvent::ToolResult { ok, .. } = e { Some(*ok) } else { None }).collect();
    assert_eq!(results, [true, false]);
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
    let chat = codex_args(Area::Chat, false, false).join(" ");
    assert!(chat.contains("--disable plugins") && chat.contains("mcp_servers={}") && chat.contains("project_doc_max_bytes=0"));
    assert!(chat.contains("--disable hooks") && !chat.contains("windows.sandbox"));
    let project = codex_args(Area::Project, true, false).join(" ");
    assert!(project.contains("windows.sandbox=\"unelevated\"") && project.contains("--disable hooks"));
    assert!(!project.contains("plugins") && !project.contains("mcp_servers"));
    assert!(!codex_args(Area::Project, false, true).join(" ").contains("hooks"));
    for area in [Area::Chat, Area::Project] {
        assert_eq!(codex_thread(area, None)["sandbox"], "workspace-write");
        assert_eq!(codex_thread(area, None)["approvalPolicy"], "on-request");
    }
    assert!(codex_thread(Area::Chat, None).contains_key("baseInstructions"));
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
        bin: "codex".into(),
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
        let thread = CodexThread { cwd: ".".into(), model: None, effort: Some("medium".into()), resume: None, instructions: None };
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
fn a_side_question_thread_has_its_own_instructions_reads_and_runs_nothing_and_is_not_saved() {
    let thread =
        CodexThread { cwd: ".".into(), model: Some("gpt-6-luna".into()), effort: None, resume: None, instructions: Some("Route.".into()) };
    let (method, params) = thread_params(Area::Chat, &thread);
    assert_eq!(method, "thread/start");
    assert_eq!(
        (&params["baseInstructions"], &params["sandbox"], &params["approvalPolicy"]),
        (&"Route.".into(), &"read-only".into(), &"never".into())
    );
    assert_eq!(params["model"], "gpt-6-luna");
    assert_eq!(params["ephemeral"], true, "it stays out of the user's Codex history");
}

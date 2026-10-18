//! "Keep going" (R199): a project conversation that goes on by itself while the person is away. Each test drives a Claude
//! process by hand (`pipes`) and checks what Faber sends it when a turn ends.

use crate::support;

use faber_engine::cli::Cli;
use faber_engine::model::{Area, Conversation, Role, Status, Vendor};
use faber_engine::router::Choice;
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use support::{Pipe, next_pipe, pipes};
use tokio::sync::mpsc::UnboundedReceiver;

fn engine(dir: &Path, spawn: faber_engine::proc::Spawner) -> (Engine, UnboundedReceiver<AppEvent>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let opts = EngineOptions {
        data_dir: dir.into(),
        clis: BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]),
        spawn,
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e))));
    // Faber goes on by itself only on a known subscription.
    let plan = faber_engine::signals::Signals { billing: Some(faber_engine::budget::Billing::Subscription), ..Default::default() };
    engine.note_account_events(Vendor::Claude, vec![faber_engine::events::EngineEvent::Signals(plan)]);
    (engine, rx)
}

/// A project conversation whose first turn runs on a hand-driven Claude process.
async fn started(dir: &Path, folder: &Path) -> (Engine, UnboundedReceiver<AppEvent>, Pipe, String) {
    let (spawn, made) = pipes();
    let (engine, rx) = engine(dir, spawn);
    let project = engine.add_project(&folder.to_string_lossy()).unwrap();
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
    let request = SendRequest {
        conversation_id: None,
        area: Area::Project,
        project_id: Some(project.id),
        text: "Fix the login page, then the rest of the list".into(),
        images: vec![],
        files: vec![],
        choice,
    };
    let id = engine.send(request).unwrap();
    let mut pipe = next_pipe(&made).await;
    // A real CLI says nothing before it has the turn; a line before it would be read as the end of an earlier turn.
    until_turns(&pipe, 1).await;
    pipe.say(json!({ "type": "system", "subtype": "init", "session_id": "s", "model": "claude-haiku-5-5" })).await;
    (engine, rx, pipe, id)
}

/// A project folder Faber accepts: the build's own folder, or the system's temporary one where the build sits in a folder
/// Faber refuses (a worktree under `.claude`).
fn folder() -> tempfile::TempDir {
    let build = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/test-workspaces");
    std::fs::create_dir_all(&build).unwrap();
    [build, std::env::temp_dir()]
        .into_iter()
        .map(|root| tempfile::tempdir_in(root).unwrap())
        .find(|dir| faber_engine::permissions::checked_folder(dir.path()).is_some())
        .expect("a folder Faber works in")
}

/// The end of a turn on a subscription whose weekly window is `week` used, with six of its seven days still ahead.
async fn answers_at(pipe: &mut Pipe, text: &str, ok: bool, week: f64) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let windows = json!({ "five_hour": { "utilization": 0.05, "resetsAt": now + 4 * 3600 },
        "seven_day": { "utilization": week, "resetsAt": now + 6 * 86_400 } });
    pipe.say(json!({ "type": "rate_limit_event", "rate_limit_info": { "status": "allowed", "resetsAt": now + 4 * 3600,
        "rateLimitType": "five_hour", "unifiedWindows": windows } }))
        .await;
    answers(pipe, text, ok).await;
}

async fn answers(pipe: &mut Pipe, text: &str, ok: bool) {
    if ok {
        let block = |event: Value| json!({ "type": "stream_event", "event": event, "parent_tool_use_id": null });
        pipe.say(block(json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "text", "text": "" } }))).await;
        pipe.say(block(json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": text } }))).await;
        pipe.say(json!({ "type": "assistant", "message": { "role": "assistant", "content": [{ "type": "text", "text": text }] } })).await;
    }
    pipe.say(json!({ "type": "result", "subtype": if ok { "success" } else { "error_during_execution" }, "is_error": !ok,
        "total_cost_usd": 0.0, "usage": {} }))
        .await;
}

/// The CLI takes in the user line `n` it was given (`--replay-user-messages`).
async fn echo(pipe: &mut Pipe, n: usize) {
    let content = pipe.lines.lock().unwrap().iter().filter(|m| m["type"] == "user").nth(n).unwrap()["message"]["content"].clone();
    pipe.say(json!({ "type": "user", "message": { "role": "user", "content": content }, "isReplay": true })).await;
}

/// The turns the CLI was given so far, as the text of each user line.
fn turns(pipe: &Pipe) -> Vec<String> {
    pipe.lines.lock().unwrap().iter().filter(|m| m["type"] == "user").map(|m| text_of(&m["message"]["content"])).collect()
}

fn text_of(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join(""),
        _ => String::new(),
    }
}

async fn until_turns(pipe: &Pipe, n: usize) {
    for _ in 0..300 {
        if turns(pipe).len() >= n {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the CLI got {} turns, not {n}: {:?}", turns(pipe).len(), turns(pipe));
}

async fn until(engine: &Engine, id: &str, done: impl Fn(&Conversation) -> bool) -> Conversation {
    for _ in 0..300 {
        let c = engine.open(id).unwrap();
        if done(&c) {
            return c;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the conversation never got there: {:?}", engine.open(id).unwrap());
}

fn idle(c: &Conversation) -> bool {
    c.messages.last().is_some_and(|m| m.role == Role::Assistant && m.status != Some(Status::Streaming))
}

fn halts(rx: &mut UnboundedReceiver<AppEvent>) -> Vec<String> {
    let mut seen = vec![];
    while let Ok(e) = rx.try_recv() {
        if let AppEvent::Halted { why, .. } = e {
            seen.push(why);
        }
    }
    seen
}

#[tokio::test]
async fn keep_going_sends_the_next_turn_by_itself_until_the_agent_says_nothing_is_left() {
    let (dir, project) = (tempfile::tempdir().unwrap(), folder());
    let (engine, mut rx, mut pipe, id) = started(dir.path(), project.path()).await;
    // Turned on while the person's own turn runs: it acts when that turn ends.
    engine.keep_going(&id, true).unwrap();
    assert!(engine.state().conversations.iter().any(|c| c.id == id && c.keep_going), "the list says it runs on its own");
    answers_at(&mut pipe, "The login page is fixed.", true, 0.1).await;
    until_turns(&pipe, 2).await;
    assert!(turns(&pipe)[1].contains("keep going on your own"), "{:?}", turns(&pipe));
    let c = until(&engine, &id, |c| c.messages.len() == 4).await;
    assert_eq!(c.messages[2].wake.as_ref().map(|w| w.why.as_str()), Some("wake.keepGoing"), "Faber's own message says so");
    assert_eq!(c.keep_going.as_ref().map(|k| k.turns), Some(1));
    // The next item ends with the stop line: nothing more is sent, and the line leaves the answer.
    answers_at(&mut pipe, "The list is done.\n\n[keep going: stop]", true, 0.1).await;
    let c = until(&engine, &id, |c| idle(c) && c.keep_going.is_none()).await;
    let reply = c.messages.last().unwrap();
    assert_eq!(reply.halt.as_deref(), Some("reason.keepGoing.done"));
    assert_eq!(reply.text, "The list is done.");
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(turns(&pipe).len(), 2, "no turn after nothing was left");
    assert_eq!(halts(&mut rx), vec!["reason.keepGoing.done".to_string()], "the tray is told once");
    engine.shutdown();
}

#[tokio::test]
async fn keep_going_tries_a_failed_turn_once_and_stops_after_the_second() {
    let (dir, project) = (tempfile::tempdir().unwrap(), folder());
    let (engine, mut rx, mut pipe, id) = started(dir.path(), project.path()).await;
    answers_at(&mut pipe, "Started.", true, 0.1).await;
    until(&engine, &id, idle).await;
    // Turned on after a finished reply, the next turn goes at once.
    engine.keep_going(&id, true).unwrap();
    until_turns(&pipe, 2).await;
    answers_at(&mut pipe, "", false, 0.1).await;
    until_turns(&pipe, 3).await;
    answers_at(&mut pipe, "", false, 0.1).await;
    let c = until(&engine, &id, |c| idle(c) && c.keep_going.is_none()).await;
    assert_eq!(c.messages.last().unwrap().halt.as_deref(), Some("reason.keepGoing.failed"));
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(turns(&pipe).len(), 3);
    assert_eq!(halts(&mut rx), vec!["reason.keepGoing.failed".to_string()]);
    // Turned on again after the failed reply, it goes on at once.
    engine.keep_going(&id, true).unwrap();
    until_turns(&pipe, 4).await;
    engine.shutdown();
}

#[tokio::test]
async fn the_persons_stop_ends_keep_going_without_a_notice() {
    let (dir, project) = (tempfile::tempdir().unwrap(), folder());
    let (engine, mut rx, mut pipe, id) = started(dir.path(), project.path()).await;
    engine.keep_going(&id, true).unwrap();
    answers_at(&mut pipe, "One done.", true, 0.1).await;
    until_turns(&pipe, 2).await;
    engine.interrupt(&id);
    answers_at(&mut pipe, "", false, 0.1).await;
    let c = until(&engine, &id, |c| idle(c) && c.messages.len() == 4).await;
    assert!(c.keep_going.is_none());
    assert_eq!(c.messages.last().unwrap().halt, None, "a stop by the person is not a stop to explain");
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(turns(&pipe).len(), 2);
    assert_eq!(halts(&mut rx), Vec::<String>::new());
    engine.shutdown();
}

#[tokio::test]
async fn keep_going_stops_when_the_agent_asked_the_person_something() {
    let (dir, project) = (tempfile::tempdir().unwrap(), folder());
    let (engine, mut rx, mut pipe, id) = started(dir.path(), project.path()).await;
    engine.keep_going(&id, true).unwrap();
    answers_at(&mut pipe, "One done.", true, 0.1).await;
    until_turns(&pipe, 2).await;
    let question = json!({ "questions": [{ "question": "Which stack?", "header": "Stack", "multiSelect": false, "options": [] }] });
    pipe.say(json!({ "type": "assistant", "message": { "role": "assistant",
        "content": [{ "type": "tool_use", "id": "toolu_q", "name": "AskUserQuestion", "input": question }] } }))
        .await;
    answers_at(&mut pipe, "I asked which stack to use.", true, 0.1).await;
    let c = until(&engine, &id, |c| idle(c) && c.keep_going.is_none()).await;
    assert_eq!(c.messages.last().unwrap().halt.as_deref(), Some("reason.keepGoing.asked"));
    assert_eq!(halts(&mut rx), vec!["reason.keepGoing.asked".to_string()]);
    assert!(engine.keep_going(&c.id, true).is_ok(), "the person can turn it on again");
    engine.shutdown();
}

#[tokio::test]
async fn keep_going_is_for_project_conversations() {
    let dir = tempfile::tempdir().unwrap();
    let (spawn, made) = pipes();
    let (engine, _rx) = engine(dir.path(), spawn);
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
    let id = engine
        .send(SendRequest {
            conversation_id: None,
            area: Area::Chat,
            project_id: None,
            text: "hi".into(),
            images: vec![],
            files: vec![],
            choice,
        })
        .unwrap();
    drop(next_pipe(&made).await);
    assert_eq!(engine.keep_going(&id, true), Err("error.keepGoing".to_string()));
    engine.shutdown();
}

#[tokio::test]
async fn keep_going_goes_on_when_auto_s_pacing_says_that_ai_spends_too_fast() {
    let (dir, project) = (tempfile::tempdir().unwrap(), folder());
    let (engine, mut rx, mut pipe, id) = started(dir.path(), project.path()).await;
    engine.keep_going(&id, true).unwrap();
    // Most of the week spent with six of its seven days still ahead: Auto calls that spending fast, and still the person
    // who turned it on is trusting Faber to go on (R212).
    answers_at(&mut pipe, "One done.", true, 0.8).await;
    until_turns(&pipe, 2).await;
    assert!(engine.open(&id).unwrap().keep_going.is_some());
    assert!(halts(&mut rx).is_empty());
    engine.shutdown();
}

#[tokio::test]
async fn keep_going_ends_with_the_run_that_started_it() {
    let (dir, project) = (tempfile::tempdir().unwrap(), folder());
    let (first, _rx, mut pipe, id) = started(dir.path(), project.path()).await;
    first.keep_going(&id, true).unwrap();
    answers_at(&mut pipe, "One done.", true, 0.1).await;
    until_turns(&pipe, 2).await;
    first.shutdown();
    // Faber restarts: nothing runs the conversation, so it does not say it goes on by itself.
    let (spawn, made) = pipes();
    let (reopened, _rx) = engine(dir.path(), spawn);
    let c = reopened.open(&id).unwrap();
    assert!(c.keep_going.is_none());
    assert!(reopened.state().conversations.iter().any(|s| s.id == id && !s.keep_going), "the list agrees");
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert!(made.lock().unwrap().iter().all(|p| turns(p).is_empty()), "no turn by itself after a restart");
    reopened.shutdown();
}

#[tokio::test]
async fn a_message_the_person_queued_goes_first_and_the_stop_line_still_counts() {
    let (dir, project) = (tempfile::tempdir().unwrap(), folder());
    let (engine, mut rx, mut pipe, id) = started(dir.path(), project.path()).await;
    engine.keep_going(&id, true).unwrap();
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
    let status = SendRequest {
        conversation_id: Some(id.clone()),
        area: Area::Project,
        project_id: None,
        text: "How is it going?".into(),
        images: vec![],
        files: vec![],
        choice,
    };
    engine.send(status).unwrap();
    answers_at(&mut pipe, "All of it is done.\n[keep going: stop]", true, 0.1).await;
    let c = until(&engine, &id, |c| c.keep_going.is_none()).await;
    let reply = c.messages.iter().find(|m| m.text.starts_with("All of it")).unwrap();
    assert_eq!(reply.text, "All of it is done.", "the line leaves the answer even with a message waiting");
    assert_eq!(reply.halt.as_deref(), Some("reason.keepGoing.done"));
    until_turns(&pipe, 2).await;
    assert!(turns(&pipe)[1].contains("How is it going?"), "the person's message still goes: {:?}", turns(&pipe));
    echo(&mut pipe, 1).await;
    answers_at(&mut pipe, "Fine.", true, 0.1).await;
    until(&engine, &id, |c| idle(c) && c.messages.len() == 4).await;
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(turns(&pipe).len(), 2, "nothing by itself after the stop");
    assert_eq!(halts(&mut rx), vec!["reason.keepGoing.done".to_string()]);
    engine.shutdown();
}

#[tokio::test]
async fn a_turn_out_of_quota_stops_keep_going_and_says_so() {
    let (dir, project) = (tempfile::tempdir().unwrap(), folder());
    let (engine, mut rx, mut pipe, id) = started(dir.path(), project.path()).await;
    engine.keep_going(&id, true).unwrap();
    // The model the person picked ran out of its plan: no turn ends normally, and nothing would end it again.
    pipe.say(json!({ "type": "result", "subtype": "error_during_execution", "is_error": true,
        "errors": ["You've hit your usage limit"], "total_cost_usd": 0.0, "usage": {} }))
        .await;
    let c = until(&engine, &id, |c| c.keep_going.is_none()).await;
    let last = c.messages.last().unwrap();
    assert_eq!(last.halt.as_deref(), Some("reason.keepGoing.quota"), "{:?} {:?}", last.status, last.error);
    assert_eq!(halts(&mut rx), vec!["reason.keepGoing.quota".to_string()], "the tray is told");
    engine.shutdown();
}

#[tokio::test]
async fn after_a_failed_turn_the_message_the_person_left_is_the_next_one() {
    let (dir, project) = (tempfile::tempdir().unwrap(), folder());
    let (engine, mut rx, mut pipe, id) = started(dir.path(), project.path()).await;
    engine.keep_going(&id, true).unwrap();
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
    let left = SendRequest {
        conversation_id: Some(id.clone()),
        area: Area::Project,
        project_id: None,
        text: "Then update the docs".into(),
        images: vec![],
        files: vec![],
        choice,
    };
    engine.send(left).unwrap();
    answers_at(&mut pipe, "", false, 0.1).await;
    // The person's words went into the turn, and the CLI answers them after the failure: they are the retry.
    until_turns(&pipe, 2).await;
    assert!(turns(&pipe)[1].contains("Then update the docs"), "{:?}", turns(&pipe));
    echo(&mut pipe, 1).await;
    answers_at(&mut pipe, "Docs updated.", true, 0.1).await;
    until_turns(&pipe, 3).await;
    assert!(turns(&pipe)[2].contains("keep going on your own"), "then Faber goes on: {:?}", turns(&pipe));
    engine.keep_going(&id, false).unwrap();
    answers_at(&mut pipe, "Next done.", true, 0.1).await;
    until(&engine, &id, |c| idle(c) && c.messages.len() == 6).await;
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(turns(&pipe).len(), 3, "turned off, nothing more goes");
    assert_eq!(halts(&mut rx), Vec::<String>::new());
    engine.shutdown();
}

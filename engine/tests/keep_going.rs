//! "Keep going" (R199): a project conversation that goes on by itself while the person is away. Each test drives a Claude
//! process by hand (`pipes`) and checks what Faber sends it when a turn ends.

use crate::support;

use faber_engine::cli::Cli;
use faber_engine::model::{Area, Conversation, Role, Status, Vendor};
use faber_engine::router::Choice;
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use support::{Pipe, next_pipe, pipes};
use tokio::sync::mpsc::UnboundedReceiver;

fn engine(dir: &Path, spawn: faber_engine::proc::Spawner) -> (Engine, UnboundedReceiver<AppEvent>) {
    engine_with(dir, spawn, &[Vendor::Claude])
}

/// The engine with these AIs connected, all on a known subscription. Its fixed waits (a rest, a retry, the question timer)
/// run a thousand times faster than they do for a person.
fn engine_with(dir: &Path, spawn: faber_engine::proc::Spawner, vendors: &[Vendor]) -> (Engine, UnboundedReceiver<AppEvent>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let clis = vendors.iter().map(|v| (*v, Cli { command: v.name().into(), prefix: vec![] })).collect();
    let opts = EngineOptions {
        data_dir: dir.into(),
        clis,
        spawn,
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e))));
    engine.set_keep_going_pace(0.001);
    // Faber goes on by itself only on a known subscription.
    for vendor in vendors {
        let plan = faber_engine::signals::Signals { billing: Some(faber_engine::budget::Billing::Subscription), ..Default::default() };
        engine.note_account_events(*vendor, vec![faber_engine::events::EngineEvent::Signals(plan)]);
    }
    if vendors.contains(&Vendor::Codex) {
        engine.set_codex_models(vec!["gpt-6.1-sol".into(), "gpt-6-luna".into()], Some("gpt-6.1-sol".into()));
    }
    (engine, rx)
}

/// A project conversation whose first turn runs on a hand-driven Claude process.
async fn started(dir: &Path, folder: &Path) -> (Engine, UnboundedReceiver<AppEvent>, Pipe, String) {
    let (engine, rx, pipe, id, _) = started_on(dir, folder, &[Vendor::Claude]).await;
    (engine, rx, pipe, id)
}

/// The CLIs the engine started, by name, in order.
type Bins = Arc<std::sync::Mutex<Vec<String>>>;

/// Like [`started`] with these AIs connected; the first turn runs on Claude. Also says which CLIs the engine starts.
async fn started_on(dir: &Path, folder: &Path, vendors: &[Vendor]) -> (Engine, UnboundedReceiver<AppEvent>, Pipe, String, Bins) {
    started_with(dir, folder, vendors, &|_| {}).await
}

/// Like [`started_on`], after `prepare` set the engine up (settings that a running process does not take up later).
async fn started_with(
    dir: &Path,
    folder: &Path,
    vendors: &[Vendor],
    prepare: &dyn Fn(&Engine),
) -> (Engine, UnboundedReceiver<AppEvent>, Pipe, String, Bins) {
    let (inner, made) = pipes();
    let bins = Bins::default();
    let seen = bins.clone();
    let spawn: faber_engine::proc::Spawner = Arc::new(move |bin, argv, cwd| {
        seen.lock().unwrap().push(bin.to_string());
        inner(bin, argv, cwd)
    });
    let (engine, rx) = engine_with(dir, spawn, vendors);
    prepare(&engine);
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
    (engine, rx, pipe, id, bins)
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
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
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
    // The next item ends with the stop line, which leaves the answer. The first stop is not believed: Faber asks the agent to
    // reread the original request word for word.
    answers_at(&mut pipe, "The list is done.\n\n[keep going: stop]", true, 0.1).await;
    until_turns(&pipe, 3).await;
    assert!(turns(&pipe)[2].contains("original request word for word"), "{:?}", turns(&pipe));
    let c = until(&engine, &id, |c| c.messages.len() == 6).await;
    assert_eq!(c.messages[3].text, "The list is done.", "the line leaves the answer");
    assert!(c.keep_going.is_some() && c.messages[3].halt.is_none(), "one stop is not the end");
    assert!(halts(&mut rx).is_empty());
    // It found something left: it goes on, and the next stop is checked again.
    answers_at(&mut pipe, "Not yet: the docs are left, and now they are done.", true, 0.1).await;
    until_turns(&pipe, 4).await;
    assert!(turns(&pipe)[3].contains("keep going on your own"), "back to the plain prompt: {:?}", turns(&pipe));
    answers_at(&mut pipe, "All done.\n[keep going: stop]", true, 0.1).await;
    until_turns(&pipe, 5).await;
    assert!(turns(&pipe)[4].contains("original request word for word"));
    // The second stop is believed: nothing more is sent.
    answers_at(&mut pipe, "Nothing is left.\n[keep going: stop]", true, 0.1).await;
    let c = until(&engine, &id, |c| idle(c) && c.keep_going.is_none()).await;
    let reply = c.messages.last().unwrap();
    assert_eq!(reply.halt.as_deref(), Some("reason.keepGoing.done"));
    assert_eq!(reply.text, "Nothing is left.");
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(turns(&pipe).len(), 5, "no turn after nothing was left");
    assert_eq!(halts(&mut rx), vec!["reason.keepGoing.done".to_string()], "the tray is told once");
    engine.shutdown();
}

#[tokio::test]
async fn keep_going_rests_after_a_failed_turn_and_stops_only_after_six_in_a_row() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(dir.path(), project.path()).await;
    // Its rests (one, five, then fifteen minutes) run faster here: the first lasts 600 ms, so it can be seen.
    engine.set_keep_going_pace(0.01);
    answers_at(&mut pipe, "Started.", true, 0.1).await;
    until(&engine, &id, idle).await;
    // Turned on after a finished reply, the next turn goes at once.
    engine.keep_going(&id, true).unwrap();
    until_turns(&pipe, 2).await;
    // The first failure waits, and says so on the conversation, then the turn goes again.
    answers_at(&mut pipe, "", false, 0.1).await;
    let c = until(&engine, &id, |c| c.keep_going.as_ref().is_some_and(|k| k.waiting.is_some())).await;
    let wait = c.keep_going.as_ref().unwrap().waiting.clone().unwrap();
    assert_eq!(wait.why, "retry");
    assert_eq!(c.keep_going.as_ref().unwrap().failed, 1);
    engine.set_keep_going_pace(0.0005);
    until_turns(&pipe, 3).await;
    // Five more in a row, each after a longer rest: the sixth ends it.
    for n in 3..=7 {
        answers_at(&mut pipe, "", false, 0.1).await;
        if n < 7 {
            until_turns(&pipe, n + 1).await;
        }
    }
    let c = until(&engine, &id, |c| idle(c) && c.keep_going.is_none()).await;
    assert_eq!(c.messages.last().unwrap().halt.as_deref(), Some("reason.keepGoing.failed"));
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(turns(&pipe).len(), 7);
    assert_eq!(halts(&mut rx), vec!["reason.keepGoing.failed".to_string()]);
    // Turned on again after the failed reply, it goes on at once.
    engine.keep_going(&id, true).unwrap();
    until_turns(&pipe, 8).await;
    engine.shutdown();
}

#[tokio::test]
async fn a_turn_that_ends_well_after_failures_starts_the_count_again() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
    let (engine, _rx, mut pipe, id) = started(dir.path(), project.path()).await;
    engine.set_keep_going_pace(0.0005);
    answers_at(&mut pipe, "Started.", true, 0.1).await;
    until(&engine, &id, idle).await;
    engine.keep_going(&id, true).unwrap();
    for n in 2..=5 {
        until_turns(&pipe, n).await;
        answers_at(&mut pipe, "", false, 0.1).await;
    }
    until_turns(&pipe, 6).await;
    answers_at(&mut pipe, "Back.", true, 0.1).await;
    let c = until(&engine, &id, |c| c.keep_going.as_ref().is_some_and(|k| k.failed == 0 && k.turns >= 5)).await;
    assert!(c.keep_going.is_some(), "four failures in a row, then a good turn: it never stopped");
    until_turns(&pipe, 7).await;
    engine.shutdown();
}

#[tokio::test]
async fn the_persons_stop_ends_keep_going_without_a_notice() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
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
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
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
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
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
async fn keep_going_survives_a_restart_and_goes_on_by_itself() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
    let (first, _rx, mut pipe, id) = started(dir.path(), project.path()).await;
    first.keep_going(&id, true).unwrap();
    answers_at(&mut pipe, "One done.", true, 0.1).await;
    until_turns(&pipe, 2).await;
    first.shutdown();
    // Faber restarts in the middle of the night: the conversation is still on its own, and it goes on once the accounts are read.
    let (spawn, made) = pipes();
    let (reopened, _rx) = engine(dir.path(), spawn);
    assert!(reopened.state().conversations.iter().any(|s| s.id == id && s.keep_going), "the list agrees");
    reopened.resume_keep_going();
    let c = reopened.open(&id).unwrap();
    assert!(c.keep_going.is_some(), "still on");
    assert_eq!(c.keep_going.as_ref().and_then(|k| k.waiting.as_ref()).map(|w| w.why.as_str()), Some("start"));
    assert!(reopened.is_working(), "an update never restarts Faber under work that goes on by itself");
    let pipe = next_pipe(&made).await;
    until_turns(&pipe, 1).await;
    assert!(turns(&pipe)[0].contains("keep going on your own"), "the next turn was sent by Faber: {:?}", turns(&pipe));
    reopened.shutdown();
}

#[tokio::test]
async fn an_account_not_read_yet_rests_again_each_time_and_goes_on_once_it_is_read() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(dir.path(), project.path()).await;
    answers_at(&mut pipe, "One done.", true, 0.1).await;
    until(&engine, &id, idle).await;
    // After a restart the account is not read yet: no turn can go, and that is a rest like a failure, never an end.
    let unknown = faber_engine::signals::Signals { billing: Some(faber_engine::budget::Billing::Unknown), ..Default::default() };
    engine.note_account_events(Vendor::Claude, vec![faber_engine::events::EngineEvent::Signals(unknown)]);
    engine.set_keep_going_pace(0.005);
    engine.keep_going(&id, true).unwrap();
    // The rest ends, the door still says no, and it rests again (the second failure is counted too).
    until(&engine, &id, |c| c.keep_going.as_ref().is_some_and(|k| k.failed >= 2 && k.waiting.is_some())).await;
    assert_eq!(turns(&pipe).len(), 1, "nothing was sent while the account was unknown");
    let known = faber_engine::signals::Signals { billing: Some(faber_engine::budget::Billing::Subscription), ..Default::default() };
    engine.note_account_events(Vendor::Claude, vec![faber_engine::events::EngineEvent::Signals(known)]);
    until_turns(&pipe, 2).await;
    assert!(halts(&mut rx).is_empty());
    engine.shutdown();
}

#[tokio::test]
async fn a_rest_after_an_error_is_not_cut_short_by_a_restart() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
    let (first, _rx, mut pipe, id) = started(dir.path(), project.path()).await;
    first.keep_going(&id, true).unwrap();
    // The turn fails and Faber rests a quarter of an hour (the scale is the real one here); then Faber closes and opens again.
    first.set_keep_going_pace(15.0);
    answers_at(&mut pipe, "", false, 0.1).await;
    let c = until(&first, &id, |c| c.keep_going.as_ref().is_some_and(|k| k.waiting.is_some())).await;
    let saved = c.keep_going.unwrap().waiting.unwrap();
    assert_eq!(saved.why, "retry");
    first.shutdown();
    let (spawn, _made) = pipes();
    let (reopened, _rx) = engine(dir.path(), spawn);
    reopened.resume_keep_going();
    let wait = reopened.open(&id).unwrap().keep_going.unwrap().waiting.unwrap();
    assert_eq!(wait.why, "retry", "it is still the same rest, not the short wait of a start");
    assert!(wait.until > saved.until - 5.0, "{} vs {}", wait.until, saved.until);
    reopened.shutdown();
}

#[tokio::test]
async fn a_rest_after_an_error_keeps_its_row_in_the_running_list_and_faber_working() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
    let (engine, _rx, mut pipe, id) = started(dir.path(), project.path()).await;
    engine.keep_going(&id, true).unwrap();
    // The turn fails and Faber rests before it tries again (here half as long), and the update must not install under it.
    engine.set_keep_going_pace(0.5);
    answers_at(&mut pipe, "", false, 0.1).await;
    let c = until(&engine, &id, |c| c.keep_going.as_ref().is_some_and(|k| k.waiting.is_some())).await;
    let wait = c.keep_going.unwrap().waiting.unwrap();
    assert_eq!(wait.why, "retry");
    assert!(wait.until > unix_now() + 20.0, "it rests about half a minute: {}", wait.until - unix_now());
    assert!(engine.is_working());
    let state = engine.state();
    let row = state.running.items.iter().find(|i| i.conversation_id == id).expect("its row stays in the running list");
    assert!(row.keep_going && row.keep_wait.is_some());
    let progress = row.keep_progress.as_ref().expect("and how far it has come");
    assert_eq!(progress.turns, 0);
    assert!(progress.since > 0, "since it was turned on");
    engine.keep_going(&id, false).unwrap();
    assert!(!engine.is_working(), "off, it is not work");
    engine.shutdown();
}

fn unix_now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64()
}

#[tokio::test]
async fn a_message_the_person_queued_goes_first_and_the_stop_line_still_counts() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
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
    // The line leaves the answer even with a message waiting; it is a first stop, so it is not the end yet.
    let c = until(&engine, &id, |c| c.messages.iter().any(|m| m.text == "All of it is done.")).await;
    assert!(c.keep_going.as_ref().is_some_and(|k| k.checked), "the stop is remembered across the person's turn");
    until_turns(&pipe, 2).await;
    assert!(turns(&pipe)[1].contains("How is it going?"), "the person's message still goes: {:?}", turns(&pipe));
    echo(&mut pipe, 1).await;
    answers_at(&mut pipe, "Fine.", true, 0.1).await;
    // Then Faber asks the agent to check against the original request, and a second stop ends it.
    until_turns(&pipe, 3).await;
    assert!(turns(&pipe)[2].contains("original request word for word"), "{:?}", turns(&pipe));
    answers_at(&mut pipe, "Checked, nothing left.\n[keep going: stop]", true, 0.1).await;
    let c = until(&engine, &id, |c| idle(c) && c.keep_going.is_none()).await;
    assert_eq!(c.messages.last().unwrap().halt.as_deref(), Some("reason.keepGoing.done"));
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(turns(&pipe).len(), 3, "nothing by itself after the stop");
    assert_eq!(halts(&mut rx), vec!["reason.keepGoing.done".to_string()]);
    engine.shutdown();
}

// Task 015: the person asking "is it done?" between Faber's turns never keeps it going without a word. A stop after a stop
// ends it, whoever sent the turn, and the tray says so.
#[tokio::test]
async fn a_second_stop_in_the_persons_own_turn_ends_it_with_a_notice() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(dir.path(), project.path()).await;
    engine.keep_going(&id, true).unwrap();
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
    let status = SendRequest {
        conversation_id: Some(id.clone()),
        area: Area::Project,
        project_id: None,
        text: "Is all of it done?".into(),
        images: vec![],
        files: vec![],
        choice,
    };
    engine.send(status).unwrap();
    answers_at(
        &mut pipe,
        "All of it is done.
[keep going: stop]",
        true,
        0.1,
    )
    .await;
    until_turns(&pipe, 2).await;
    echo(&mut pipe, 1).await;
    answers_at(
        &mut pipe,
        "Yes, all of it.
[keep going: stop]",
        true,
        0.1,
    )
    .await;
    let c = until(&engine, &id, |c| idle(c) && c.keep_going.is_none()).await;
    let reply = c.messages.last().unwrap();
    assert_eq!((reply.text.as_str(), reply.halt.as_deref()), ("Yes, all of it.", Some("reason.keepGoing.done")));
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(turns(&pipe).len(), 2, "no check turn after the second stop");
    assert_eq!(halts(&mut rx), vec!["reason.keepGoing.done".to_string()]);
    engine.shutdown();
}

// The stop line is never words for the person: it leaves a reply with Keep going off too, and from the middle of one, such as
// before the answer to a hook.
#[tokio::test]
async fn the_stop_line_leaves_every_reply_wherever_it_is() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(dir.path(), project.path()).await;
    answers_at(
        &mut pipe,
        "Checked it.
**[keep going: stop]**
The hook said ok.",
        true,
        0.1,
    )
    .await;
    let c = until(&engine, &id, idle).await;
    assert_eq!(
        c.messages.last().unwrap().text,
        "Checked it.
The hook said ok."
    );
    assert!(halts(&mut rx).is_empty(), "with Keep going off there is nothing to end");
    engine.shutdown();
}

#[tokio::test]
async fn a_turn_cut_at_the_limit_waits_for_quota_and_a_reading_with_room_sends_the_next_turn() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(dir.path(), project.path()).await;
    engine.set_keep_going_pace(1.0);
    engine.keep_going(&id, true).unwrap();
    // The model the person picked ran out of its plan, and the CLI did not say when it comes back: keep going does not
    // stop, it looks again within ten minutes.
    pipe.say(json!({ "type": "result", "subtype": "error_during_execution", "is_error": true,
        "errors": ["You've hit your usage limit"], "total_cost_usd": 0.0, "usage": {} }))
        .await;
    let c = until(&engine, &id, |c| c.keep_going.as_ref().is_some_and(|k| k.wait_until.is_some())).await;
    let at = c.keep_going.unwrap().wait_until.unwrap();
    assert!(at > unix_now() + 500.0 && at < unix_now() + 700.0, "it looks again in ten minutes: {at} vs {}", unix_now());
    assert!(c.messages.last().unwrap().halt.is_none());
    assert!(halts(&mut rx).is_empty(), "nothing stopped, so the tray says nothing");
    // A reading says the limit is gone: the next turn goes now, without waiting for the ten minutes.
    let clear = faber_engine::signals::Signals { reach: Some(faber_engine::budget::Reach::Clear), ..Default::default() };
    engine.note_account_events(Vendor::Claude, vec![faber_engine::events::EngineEvent::Signals(clear)]);
    let c = until(&engine, &id, |c| c.messages.iter().any(|m| m.wake.as_ref().is_some_and(|w| w.why == "wake.keepGoing"))).await;
    assert!(c.keep_going.as_ref().is_some_and(|k| k.wait_until.is_none()), "the wait is over: {:?}", c.keep_going);
    engine.shutdown();
}

#[tokio::test]
async fn with_no_reset_known_it_tries_the_cli_again_after_the_wait_and_never_stops_by_itself() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
    let (engine, mut rx, mut pipe, id, bins) = started_on(dir.path(), project.path(), &[Vendor::Claude]).await;
    engine.keep_going(&id, true).unwrap();
    pipe.say(json!({ "type": "result", "subtype": "error_during_execution", "is_error": true,
        "errors": ["You've hit your usage limit"], "total_cost_usd": 0.0, "usage": {} }))
        .await;
    until(&engine, &id, |c| c.keep_going.as_ref().is_some_and(|k| k.wait_until.is_some())).await;
    let started = bins.lock().unwrap().len();
    // Ten minutes (here a thousandth of that) later the limit is not taken for granted: a turn goes to the CLI to find out.
    for _ in 0..300 {
        if bins.lock().unwrap().len() > started {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(bins.lock().unwrap().len() > started, "the CLI was tried again: {:?}", bins.lock().unwrap());
    assert!(engine.open(&id).unwrap().keep_going.is_some());
    assert!(halts(&mut rx).is_empty(), "the tray is not woken: nothing needs the person");
    engine.shutdown();
}

#[tokio::test]
async fn with_every_ai_out_it_waits_for_the_reset_and_then_goes_on() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(dir.path(), project.path()).await;
    answers(&mut pipe, "One done.", true).await;
    until(&engine, &id, idle).await;
    // The window is out and says it is back in a second and a half.
    engine.note_account_events(
        Vendor::Claude,
        vec![faber_engine::events::EngineEvent::Quota { window: "five_hour".into(), used: 1.0, resets_at: unix_now() + 1.5 }],
    );
    engine.keep_going(&id, true).unwrap();
    let c = until(&engine, &id, |c| c.keep_going.as_ref().is_some_and(|k| k.wait_until.is_some())).await;
    assert!(c.keep_going.unwrap().wait_until.is_some());
    assert_eq!(turns(&pipe).len(), 1, "nothing is sent while the quota is out");
    until_turns(&pipe, 2).await;
    assert!(turns(&pipe)[1].contains("keep going on your own"));
    assert!(halts(&mut rx).is_empty());
    engine.shutdown();
}

#[tokio::test]
async fn with_the_picked_ai_out_the_next_turn_goes_to_another_ai_that_has_room() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
    let (engine, mut rx, mut pipe, id, bins) = started_on(dir.path(), project.path(), &[Vendor::Claude, Vendor::Codex]).await;
    answers(&mut pipe, "One done.", true).await;
    until(&engine, &id, idle).await;
    engine.note_account_events(
        Vendor::Claude,
        vec![faber_engine::events::EngineEvent::Quota { window: "five_hour".into(), used: 1.0, resets_at: unix_now() + 3600.0 }],
    );
    engine.keep_going(&id, true).unwrap();
    // Claude is out for an hour and Codex is not: the work goes on there, with no wait.
    let c = until(&engine, &id, |c| c.messages.last().is_some_and(|m| m.vendor == Some(Vendor::Codex))).await;
    assert_eq!(c.keep_going.as_ref().map(|k| k.switches), Some(1));
    for _ in 0..300 {
        if bins.lock().unwrap().iter().any(|b| b == "codex") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(bins.lock().unwrap().iter().any(|b| b == "codex"), "Codex was started: {:?}", bins.lock().unwrap());
    assert!(c.keep_going.as_ref().is_some_and(|k| k.waiting.is_none()));
    assert!(halts(&mut rx).is_empty());
    engine.shutdown();
}

#[tokio::test]
async fn an_ai_billed_per_use_is_never_the_one_it_moves_to() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
    let (engine, mut rx, mut pipe, id, bins) = started_on(dir.path(), project.path(), &[Vendor::Claude, Vendor::Codex]).await;
    // Codex is billed per use here: unattended work is never spent on it.
    let billed = faber_engine::signals::Signals { billing: Some(faber_engine::budget::Billing::UsageBilled), ..Default::default() };
    engine.note_account_events(Vendor::Codex, vec![faber_engine::events::EngineEvent::Signals(billed)]);
    answers(&mut pipe, "One done.", true).await;
    until(&engine, &id, idle).await;
    engine.note_account_events(
        Vendor::Claude,
        vec![faber_engine::events::EngineEvent::Quota { window: "five_hour".into(), used: 1.0, resets_at: unix_now() + 3600.0 }],
    );
    engine.keep_going(&id, true).unwrap();
    let c = until(&engine, &id, |c| c.keep_going.as_ref().is_some_and(|k| k.wait_until.is_some())).await;
    assert!(c.keep_going.unwrap().wait_until.is_some(), "it waits for Claude instead");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!bins.lock().unwrap().iter().any(|b| b == "codex"));
    assert!(halts(&mut rx).is_empty());
    engine.shutdown();
}

#[tokio::test]
async fn a_first_turn_picked_by_hand_moves_to_the_other_ai_when_keep_going_is_on() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
    let (engine, _rx, mut pipe, id, bins) = started_on(dir.path(), project.path(), &[Vendor::Claude, Vendor::Codex]).await;
    engine.keep_going(&id, true).unwrap();
    // The first turn, picked by hand, hits the plan limit in the middle of its work.
    pipe.say(json!({ "type": "result", "subtype": "error_during_execution", "is_error": true,
        "errors": ["You've hit your usage limit"], "total_cost_usd": 0.0, "usage": {} }))
        .await;
    for _ in 0..300 {
        if bins.lock().unwrap().iter().any(|b| b == "codex") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(bins.lock().unwrap().iter().any(|b| b == "codex"), "the work moved to Codex: {:?}", bins.lock().unwrap());
    assert_eq!(engine.open(&id).unwrap().keep_going.map(|k| k.switches), Some(1));
    engine.shutdown();
}

#[tokio::test]
async fn a_busy_faber_tries_the_turn_again_soon() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(dir.path(), project.path()).await;
    answers_at(&mut pipe, "Started.", true, 0.1).await;
    until(&engine, &id, idle).await;
    // An update being installed holds Faber for a moment: the turn is refused with its key.
    engine.set_keep_going_pace(0.02);
    let install = engine.install_admission().unwrap();
    engine.keep_going(&id, true).unwrap();
    let c = until(&engine, &id, |c| c.keep_going.as_ref().is_some_and(|k| k.waiting.is_some())).await;
    assert_eq!(c.keep_going.as_ref().unwrap().waiting.as_ref().unwrap().why, "busy");
    assert_eq!(c.keep_going.unwrap().failed, 0, "waiting out an install is not a failure");
    drop(install);
    until_turns(&pipe, 2).await;
    assert!(halts(&mut rx).is_empty());
    engine.shutdown();
}

#[tokio::test]
async fn a_turn_that_only_waited_rests_before_the_next_and_does_not_count_toward_the_cap() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
    let (engine, _rx, mut pipe, id) = started(dir.path(), project.path()).await;
    engine.set_keep_going_pace(0.02);
    engine.keep_going(&id, true).unwrap();
    // A quick answer with no tool ("waiting for CI"): two to five minutes pass before Faber asks again (here 3.6 s).
    answers_at(&mut pipe, "Waiting for CI.", true, 0.1).await;
    let c = until(&engine, &id, |c| c.keep_going.as_ref().is_some_and(|k| k.waiting.is_some())).await;
    let kg = c.keep_going.unwrap();
    assert_eq!((kg.waiting.unwrap().why.as_str(), kg.work, kg.idle), ("rest", 0, 1), "it did not count");
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(turns(&pipe).len(), 1, "it rests before the next turn");
    // The same turn with a tool is work: it counts, and the next goes at once.
    engine.set_keep_going_pace(0.0);
    engine.keep_going(&id, false).unwrap();
    engine.keep_going(&id, true).unwrap();
    until_turns(&pipe, 2).await;
    pipe.say(json!({ "type": "assistant", "message": { "role": "assistant",
        "content": [{ "type": "tool_use", "id": "toolu_b", "name": "Bash", "input": { "command": "cargo test" } }] } }))
        .await;
    answers_at(&mut pipe, "Tests pass.", true, 0.1).await;
    until_turns(&pipe, 3).await;
    let kg = engine.open(&id).unwrap().keep_going.unwrap();
    assert_eq!((kg.work, kg.idle), (1, 0));
    engine.shutdown();
}

#[tokio::test]
async fn a_question_already_answered_does_not_stop_it() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(dir.path(), project.path()).await;
    engine.keep_going(&id, true).unwrap();
    let question = json!({ "questions": [{ "question": "Which stack?", "header": "Stack", "multiSelect": false, "options": [] }] });
    pipe.say(json!({ "type": "assistant", "message": { "role": "assistant",
        "content": [{ "type": "tool_use", "id": "toolu_q", "name": "AskUserQuestion", "input": question }] } }))
        .await;
    // The person answered: the call has its result.
    pipe.say(json!({ "type": "user", "message": { "role": "user",
        "content": [{ "type": "tool_result", "tool_use_id": "toolu_q", "content": "Rust", "is_error": false }] } }))
        .await;
    answers_at(&mut pipe, "I used Rust.", true, 0.1).await;
    until_turns(&pipe, 2).await;
    assert!(engine.open(&id).unwrap().keep_going.is_some());
    assert!(halts(&mut rx).is_empty());
    engine.shutdown();
}

#[tokio::test]
async fn a_question_nobody_answers_gets_its_first_choice_and_the_reply_says_so() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(dir.path(), project.path()).await;
    engine.keep_going(&id, true).unwrap();
    let question = json!({ "questions": [{ "question": "Which stack?", "header": "Stack", "multiSelect": false,
        "options": [{ "label": "Rust", "description": "recommended" }, { "label": "Go", "description": "" }] }] });
    pipe.say(json!({ "type": "control_request", "request_id": "asked",
        "request": { "subtype": "can_use_tool", "tool_name": "AskUserQuestion", "input": question } }))
        .await;
    until(&engine, &id, |c| c.messages.last().is_some_and(|m| m.permissions.as_ref().is_some_and(|p| !p.is_empty()))).await;
    // Ten minutes pass (here 600 ms) and nobody answers: the first choice goes to the agent.
    for _ in 0..300 {
        if !pipe.answers().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(pipe.answers(), vec![("asked".to_string(), "allow".to_string())]);
    let sent = pipe.lines.lock().unwrap().iter().find(|m| m["type"] == "control_response").cloned().unwrap();
    assert_eq!(sent["response"]["response"]["updatedInput"]["answers"], json!({ "Which stack?": "Rust" }));
    let c = until(&engine, &id, |c| c.messages.last().is_some_and(|m| m.auto_answers.is_some())).await;
    let chosen = c.messages.last().unwrap().auto_answers.clone().unwrap();
    assert_eq!((chosen[0].question.as_str(), chosen[0].answer.as_str()), ("Which stack?", "Rust"));
    assert_eq!(c.keep_going.unwrap().answered, 1);
    assert!(halts(&mut rx).is_empty(), "the tray is not woken: Faber decided");
    engine.shutdown();
}

#[tokio::test]
async fn a_permission_card_is_never_answered_by_keep_going() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
    let ask = |engine: &Engine| {
        let settings = faber_engine::model::Settings { permissions: Some(faber_engine::model::Permissions::Ask), ..Default::default() };
        engine.save_settings(settings).unwrap();
    };
    let (engine, _rx, mut pipe, id, _) = started_with(dir.path(), project.path(), &[Vendor::Claude], &ask).await;
    engine.keep_going(&id, true).unwrap();
    pipe.say(json!({ "type": "control_request", "request_id": "write",
        "request": { "subtype": "can_use_tool", "tool_name": "Write", "input": { "file_path": "notes.txt", "content": "x" } } }))
        .await;
    until(&engine, &id, |c| c.messages.last().is_some_and(|m| m.permissions.as_ref().is_some_and(|p| !p.is_empty()))).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(pipe.answers().is_empty(), "only the person allows a command or a file change");
    engine.shutdown();
}

#[tokio::test]
async fn after_a_failed_turn_the_message_the_person_left_is_the_next_one() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
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

#[tokio::test]
async fn every_ai_out_of_quota_with_a_known_reset_waits_on_screen_instead_of_stopping() {
    let (dir, project) = (tempfile::tempdir().unwrap(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(dir.path(), project.path()).await;
    engine.keep_going(&id, true).unwrap();
    // The week is all but used up (past the share Faber still sends at) and resets in six days: the only AI is out, so
    // Faber waits for it rather than ending.
    answers_at(&mut pipe, "One done.", true, 0.985).await;
    let c = until(&engine, &id, |c| idle(c) && c.keep_going.as_ref().is_some_and(|k| k.wait_until.is_some())).await;
    let at = c.keep_going.unwrap().wait_until.unwrap();
    assert!(at > unix_now() + 5.0 * 86_400.0 && at < unix_now() + 7.0 * 86_400.0, "it waits for the reset: {at} vs {}", unix_now());
    let row = engine.state().running.items.into_iter().find(|r| r.conversation_id == id).expect("the running list keeps it");
    assert_eq!(row.wait_until, Some(at), "and says until when");
    assert!(row.keep_going);
    assert!(engine.is_working(), "so no idle update restarts Faber under it");
    assert!(halts(&mut rx).is_empty(), "nothing stopped");
    assert_eq!(turns(&pipe).len(), 1, "and no turn went to an AI with no quota");
    // Turned off while it waits, the list lets it go.
    engine.keep_going(&id, false).unwrap();
    assert!(engine.state().running.items.iter().all(|r| r.conversation_id != id));
    engine.shutdown();
}

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
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::UnboundedReceiver;

fn engine(dir: &crate::support::DataFolder, spawn: faber_engine::proc::Spawner) -> (Engine, UnboundedReceiver<AppEvent>) {
    engine_with(dir, spawn, &[Vendor::Claude])
}

/// The engine with these AIs connected, all on a known subscription. Its fixed waits (a rest, a retry, the question timer)
/// run a thousand times faster than they do for a person.
fn engine_with(
    dir: &crate::support::DataFolder,
    spawn: faber_engine::proc::Spawner,
    vendors: &[Vendor],
) -> (Engine, UnboundedReceiver<AppEvent>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let clis = vendors.iter().map(|v| (*v, Cli { command: v.name().into(), prefix: vec![] })).collect();
    let opts = EngineOptions {
        data_dir: dir.path().into(),
        #[cfg(windows)]
        chat_dir: dir.chat_path().into(),
        clis,
        spawn,
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e)))).unwrap();
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
async fn started(dir: &crate::support::DataFolder, folder: &Path) -> (Engine, UnboundedReceiver<AppEvent>, Pipe, String) {
    let (engine, rx, pipe, id, _) = started_on(dir, folder, &[Vendor::Claude]).await;
    (engine, rx, pipe, id)
}

/// The CLIs the engine started, by name, in order.
type Bins = Arc<std::sync::Mutex<Vec<String>>>;

/// Like [`started`] with these AIs connected; the first turn runs on Claude. Also says which CLIs the engine starts.
async fn started_on(
    dir: &crate::support::DataFolder,
    folder: &Path,
    vendors: &[Vendor],
) -> (Engine, UnboundedReceiver<AppEvent>, Pipe, String, Bins) {
    let (engine, rx, pipe, id, bins, _) = started_with(dir, folder, vendors, &|_| {}).await;
    (engine, rx, pipe, id, bins)
}

/// Like [`started_on`], after `prepare` set the engine up (settings that a running process does not take up later).
async fn started_with(
    dir: &crate::support::DataFolder,
    folder: &Path,
    vendors: &[Vendor],
    prepare: &dyn Fn(&Engine),
) -> (Engine, UnboundedReceiver<AppEvent>, Pipe, String, Bins, Arc<std::sync::Mutex<Vec<Pipe>>>) {
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
    (engine, rx, pipe, id, bins, made)
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
    support::until(&format!("the CLI never got {n} turns"), || turns(pipe).len() >= n).await;
}

async fn until(engine: &Engine, id: &str, done: impl Fn(&Conversation) -> bool) -> Conversation {
    support::until("the conversation never got there", || done(&engine.open(id).unwrap())).await;
    engine.open(id).unwrap()
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
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(&dir, project.path()).await;
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
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(&dir, project.path()).await;
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
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, _rx, mut pipe, id) = started(&dir, project.path()).await;
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
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(&dir, project.path()).await;
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
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(&dir, project.path()).await;
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
    let dir = crate::support::DataFolder::new();
    let (spawn, made) = pipes();
    let (engine, _rx) = engine(&dir, spawn);
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
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(&dir, project.path()).await;
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
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (first, _rx, mut pipe, id) = started(&dir, project.path()).await;
    first.keep_going(&id, true).unwrap();
    answers_at(&mut pipe, "One done.", true, 0.1).await;
    until_turns(&pipe, 2).await;
    first.shutdown();
    // Faber restarts in the middle of the night: the conversation is still on its own, and it goes on once the accounts are read.
    let (spawn, made) = pipes();
    let (reopened, _rx) = engine(&dir, spawn);
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
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(&dir, project.path()).await;
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
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (first, _rx, mut pipe, id) = started(&dir, project.path()).await;
    first.keep_going(&id, true).unwrap();
    // The turn fails and Faber rests a quarter of an hour (the scale is the real one here); then Faber closes and opens again.
    first.set_keep_going_pace(15.0);
    answers_at(&mut pipe, "", false, 0.1).await;
    let c = until(&first, &id, |c| c.keep_going.as_ref().is_some_and(|k| k.waiting.is_some())).await;
    let saved = c.keep_going.unwrap().waiting.unwrap();
    assert_eq!(saved.why, "retry");
    first.shutdown();
    let (spawn, _made) = pipes();
    let (reopened, _rx) = engine(&dir, spawn);
    reopened.resume_keep_going();
    let wait = reopened.open(&id).unwrap().keep_going.unwrap().waiting.unwrap();
    assert_eq!(wait.why, "retry", "it is still the same rest, not the short wait of a start");
    assert!(wait.until > saved.until - 5.0, "{} vs {}", wait.until, saved.until);
    reopened.shutdown();
}

#[tokio::test]
async fn a_rest_after_an_error_keeps_its_row_in_the_running_list_and_faber_working() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, _rx, mut pipe, id) = started(&dir, project.path()).await;
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
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(&dir, project.path()).await;
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
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(&dir, project.path()).await;
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
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(&dir, project.path()).await;
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
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(&dir, project.path()).await;
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
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id, bins) = started_on(&dir, project.path(), &[Vendor::Claude]).await;
    engine.keep_going(&id, true).unwrap();
    pipe.say(json!({ "type": "result", "subtype": "error_during_execution", "is_error": true,
        "errors": ["You've hit your usage limit"], "total_cost_usd": 0.0, "usage": {} }))
        .await;
    until(&engine, &id, |c| c.keep_going.as_ref().is_some_and(|k| k.wait_until.is_some())).await;
    let started = bins.lock().unwrap().len();
    // Ten minutes (here a thousandth of that) later the limit is not taken for granted: a turn goes to the CLI to find out.
    support::until("the CLI was never tried again", || bins.lock().unwrap().len() > started).await;
    assert!(bins.lock().unwrap().len() > started, "the CLI was tried again: {:?}", bins.lock().unwrap());
    assert!(engine.open(&id).unwrap().keep_going.is_some());
    assert!(halts(&mut rx).is_empty(), "the tray is not woken: nothing needs the person");
    engine.shutdown();
}

#[tokio::test]
async fn with_every_ai_out_it_waits_for_the_reset_and_then_goes_on() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(&dir, project.path()).await;
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
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id, bins) = started_on(&dir, project.path(), &[Vendor::Claude, Vendor::Codex]).await;
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
    support::until("Codex was never started", || bins.lock().unwrap().iter().any(|b| b == "codex")).await;
    assert!(bins.lock().unwrap().iter().any(|b| b == "codex"), "Codex was started: {:?}", bins.lock().unwrap());
    assert!(c.keep_going.as_ref().is_some_and(|k| k.waiting.is_none()));
    assert!(halts(&mut rx).is_empty());
    engine.shutdown();
}

#[tokio::test]
async fn an_ai_billed_per_use_is_never_the_one_it_moves_to() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id, bins) = started_on(&dir, project.path(), &[Vendor::Claude, Vendor::Codex]).await;
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
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, _rx, mut pipe, id, bins) = started_on(&dir, project.path(), &[Vendor::Claude, Vendor::Codex]).await;
    engine.keep_going(&id, true).unwrap();
    // The first turn, picked by hand, hits the plan limit in the middle of its work.
    pipe.say(json!({ "type": "result", "subtype": "error_during_execution", "is_error": true,
        "errors": ["You've hit your usage limit"], "total_cost_usd": 0.0, "usage": {} }))
        .await;
    support::until("the work never moved to Codex", || bins.lock().unwrap().iter().any(|b| b == "codex")).await;
    assert!(bins.lock().unwrap().iter().any(|b| b == "codex"), "the work moved to Codex: {:?}", bins.lock().unwrap());
    assert_eq!(engine.open(&id).unwrap().keep_going.map(|k| k.switches), Some(1));
    engine.shutdown();
}

#[tokio::test]
async fn a_busy_faber_tries_the_turn_again_soon() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(&dir, project.path()).await;
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
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, _rx, mut pipe, id) = started(&dir, project.path()).await;
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
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(&dir, project.path()).await;
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
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(&dir, project.path()).await;
    engine.keep_going(&id, true).unwrap();
    let question = json!({ "questions": [{ "question": "Which stack?", "header": "Stack", "multiSelect": false,
        "options": [{ "label": "Rust", "description": "recommended" }, { "label": "Go", "description": "" }] }] });
    pipe.say(json!({ "type": "control_request", "request_id": "asked",
        "request": { "subtype": "can_use_tool", "tool_name": "AskUserQuestion", "input": question } }))
        .await;
    until(&engine, &id, |c| c.messages.last().is_some_and(|m| m.permissions.as_ref().is_some_and(|p| !p.is_empty()))).await;
    // Ten minutes pass (here 600 ms) and nobody answers: the first choice goes to the agent.
    support::until("the unanswered question never got its first choice", || !pipe.answers().is_empty()).await;
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
async fn a_bigger_offer_nobody_answers_gets_the_request_as_asked_and_the_reply_says_so() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(&dir, project.path()).await;
    let offer = json!({ "question": "Keep the app free of crashes?", "asked": "Fix this crash", "bigger": "No more crashes" });
    pipe.say(json!({ "type": "control_request", "request_id": "rpc",
        "request": { "subtype": "mcp_message", "server_name": "faber", "message": { "jsonrpc": "2.0", "id": 7, "method": "tools/call",
            "params": { "name": "offer_bigger", "arguments": offer, "_meta": { "claudecode/toolUseId": "toolu_b" } } } } }))
        .await;
    until(&engine, &id, |c| c.messages.last().is_some_and(|m| m.permissions.as_ref().is_some_and(|p| !p.is_empty()))).await;
    // The person goes away with the card open: ten minutes pass (here 600 ms) and the first choice, the request as asked, is theirs.
    engine.keep_going(&id, true).unwrap();
    let reply = loop {
        if let Some(m) = pipe.lines.lock().unwrap().iter().find(|m| m["response"]["request_id"] == "rpc") {
            break m.clone();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    let result = &reply["response"]["response"]["mcp_response"]["result"];
    assert_eq!(result["content"][0]["text"], "The person chose the request as asked. Do exactly that.");
    assert_eq!(result["isError"], false);
    let c = until(&engine, &id, |c| c.messages.last().is_some_and(|m| m.auto_answers.is_some())).await;
    let chosen = c.messages.last().unwrap().auto_answers.clone().unwrap();
    assert_eq!((chosen[0].question.as_str(), chosen[0].answer.as_str()), ("Keep the app free of crashes?", "Fix this crash"));
    assert_eq!(chosen[0].others, vec!["No more crashes".to_string()]);
    assert_eq!(c.keep_going.unwrap().answered, 1);
    let heard = pipe.lines.lock().unwrap().iter().any(|m| m["response"]["request_id"] == "toolu_b");
    assert!(!heard, "no control response went to the CLI for the card");
    assert!(halts(&mut rx).is_empty(), "the tray is not woken: Faber decided");
    engine.shutdown();
}

#[tokio::test]
async fn a_permission_card_is_never_answered_by_keep_going() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let ask = |engine: &Engine| {
        let settings = faber_engine::model::Settings { permissions: Some(faber_engine::model::Permissions::Ask), ..Default::default() };
        engine.save_settings(settings).unwrap();
    };
    let (engine, _rx, mut pipe, id, _, _) = started_with(&dir, project.path(), &[Vendor::Claude], &ask).await;
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
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(&dir, project.path()).await;
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
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(&dir, project.path()).await;
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

/// The agent calls one of Faber's own tools (`plan`, `status`, `wrap_up`) and gets Faber's answer back: its text and whether
/// it was an error.
async fn calls(pipe: &mut Pipe, request: &str, tool: &str, input: Value) -> (String, bool) {
    let call = json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": { "name": tool, "arguments": input } });
    pipe.say(json!({ "type": "control_request", "request_id": request,
        "request": { "subtype": "mcp_message", "server_name": "faber", "message": call } }))
        .await;
    support::until(&format!("Faber never answered the {tool} call"), || {
        pipe.lines.lock().unwrap().iter().any(|m| m["response"]["request_id"] == request)
    })
    .await;
    let a = pipe.lines.lock().unwrap().iter().find(|m| m["response"]["request_id"] == request).cloned().unwrap();
    let result = &a["response"]["response"]["mcp_response"]["result"];
    (result["content"][0]["text"].as_str().unwrap_or_default().to_string(), result["isError"] == true)
}

/// Task 017: a project agent keeps its plan, says what it is doing now and closes with a box, through Faber's own tools.
/// The plan belongs to the conversation (and the list's row counts it); the now line and the box belong to the reply.
#[tokio::test]
async fn the_agent_keeps_a_plan_says_what_it_does_now_and_closes_with_a_box() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(&dir, project.path()).await;
    let steps =
        json!({ "steps": [{ "text": "Fix the login page", "state": "in_progress" }, { "text": "Fix the rest", "state": "pending" }] });
    let (said, error) = calls(&mut pipe, "p1", "plan", steps).await;
    assert!(!error && said.contains("0 of 2"), "{said}");
    let (said, _) = calls(&mut pipe, "s1", "status", json!({ "text": "Reading the login page" })).await;
    assert!(!said.is_empty());
    let c = until(&engine, &id, |c| c.messages.last().is_some_and(|m| m.now.is_some())).await;
    assert_eq!(c.messages.last().unwrap().now.as_deref(), Some("Reading the login page"));
    let row = engine.state().running.items.into_iter().find(|r| r.conversation_id == id).expect("it runs");
    assert_eq!((row.now.as_deref(), row.plan.map(|p| (p.done, p.total))), (Some("Reading the login page"), Some((0, 2))));
    let (said, _) = calls(&mut pipe, "p2", "plan", json!({ "advance": true })).await;
    assert!(said.contains("1 of 2"), "{said}");
    // A wrong call is answered as an error the agent can read, and changes nothing.
    let (said, error) = calls(&mut pipe, "w0", "wrap_up", json!({ "ask": "Ship it?" })).await;
    assert!(error && said.contains("recap"), "{said}");
    let box_ =
        json!({ "recap": "The login page works. The rest is not checked yet.", "ask": "Check the rest now?", "actions": ["Yes", "Later"] });
    let (said, error) = calls(&mut pipe, "w1", "wrap_up", box_).await;
    assert!(!error && said.contains("End your turn"), "{said}");
    answers(&mut pipe, "Done for now.", true).await;
    let c = until(&engine, &id, idle).await;
    let plan = c.plan.clone().expect("the plan stays with the conversation");
    let states: Vec<_> = plan.steps.iter().map(|s| format!("{:?}", s.state)).collect();
    assert_eq!(states, ["Done", "Active"]);
    let reply = c.messages.last().unwrap();
    let wrap = reply.wrap_up.clone().expect("the reply keeps its box");
    assert_eq!(
        (wrap.ask.as_deref(), wrap.actions.clone(), wrap.done),
        (Some("Check the rest now?"), vec!["Yes".to_string(), "Later".into()], false)
    );
    assert_eq!(engine.state().conversations.iter().find(|s| s.id == id).and_then(|s| s.plan.map(|p| (p.done, p.total))), Some((1, 2)));
    let mut plans = 0;
    while let Ok(e) = rx.try_recv() {
        if matches!(e, AppEvent::Plan { .. }) {
            plans += 1;
        }
    }
    assert_eq!(plans, 2, "the screen hears of each change of the plan");
    // Saved and read back the same.
    assert_eq!(engine.open(&id).unwrap().plan, Some(plan));
    engine.shutdown();
}

/// Rule 7: a turn ends with this running total of the conversation's cost, as Claude Code reports it.
async fn ends_costing(pipe: &mut Pipe, total: f64) {
    pipe.say(json!({ "type": "result", "subtype": "success", "is_error": false, "total_cost_usd": total, "usage": {} })).await;
}

/// Rule 7: a project agent gives an estimate for big work; when the job has cost half as much again, Faber warns once and
/// switches to economy mode (the plan's answer says so, no conversation starts), and a later turn over the limit says nothing more.
#[tokio::test]
async fn a_job_going_half_over_its_estimate_warns_once_and_switches_to_economy_mode() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(&dir, project.path()).await;
    let steps =
        json!({ "steps": [{ "text": "Build it", "state": "in_progress" }, { "text": "Check it", "state": "pending" }], "estimate": 1.0 });
    let (said, error) = calls(&mut pipe, "p1", "plan", steps).await;
    assert!(!error && said.contains("0 of 2") && !said.contains("estimate"), "{said}");
    // A turn that costs US$ 1.60 passes an estimate of US$ 1.00 by more than half.
    ends_costing(&mut pipe, 1.6).await;
    let c = until(&engine, &id, economy_on).await;
    assert_eq!(c.estimate.map(|e| (e.cents, e.base)), Some((100, 0)));
    let events = |rx: &mut UnboundedReceiver<AppEvent>| {
        let mut plans = 0;
        while let Ok(e) = rx.try_recv() {
            plans += usize::from(matches!(e, AppEvent::Plan { .. }));
        }
        plans
    };
    assert_eq!(events(&mut rx), 2, "the plan, then the one warning");
    let (said, error) = calls(&mut pipe, "p2", "plan", json!({ "advance": true })).await;
    assert!(!error && said.contains("1 of 2") && said.contains("half as much again"), "{said}");
    let (said, error) = calls(&mut pipe, "c1", "conversation", json!({ "title": "Docs", "brief": "Write the docs." })).await;
    assert!(error && said.contains("passed its estimate"), "{said}");
    assert_eq!(events(&mut rx), 1, "the advance is the only plan event");
    // The next turn is over the limit too: nothing more is said, nothing is stopped.
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
    let more = SendRequest {
        conversation_id: Some(id.clone()),
        area: Area::Project,
        project_id: None,
        text: "Then the rest".into(),
        images: vec![],
        files: vec![],
        choice,
    };
    engine.send(more).unwrap();
    until_turns(&pipe, 2).await;
    echo(&mut pipe, 1).await;
    ends_costing(&mut pipe, 3.2).await;
    until(&engine, &id, |c| idle(c) && c.messages.len() == 4).await;
    assert_eq!(events(&mut rx), 0, "the warning fires once");
    engine.shutdown();
}

/// The processes a test's engine starts, in order, apart from the first agent's.
type Made = Arc<std::sync::Mutex<Vec<Pipe>>>;

/// Like [`started_on`], also giving the queue of the other processes the engine starts (a helper's, a conversation an agent starts).
async fn started_made(dir: &crate::support::DataFolder, folder: &Path, vendors: &[Vendor]) -> (Engine, Pipe, String, Made) {
    let (spawn, made) = pipes();
    let (engine, _rx) = engine_with(dir, spawn, vendors);
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
    until_turns(&pipe, 1).await;
    pipe.say(json!({ "type": "system", "subtype": "init", "session_id": "s", "model": "claude-haiku-5-5" })).await;
    (engine, pipe, id, made)
}

fn steps_with(estimate: f64) -> Value {
    json!({ "steps": [{ "text": "Build it", "state": "in_progress" }, { "text": "Check it", "state": "pending" }], "estimate": estimate })
}

fn economy_on(c: &Conversation) -> bool {
    c.estimate.is_some_and(|e| e.economy)
}

/// Rule 7: the estimate and economy mode belong to the job. They go when the agent's closing box says the work is done, and a
/// plan sent after that starts with none.
#[tokio::test]
async fn the_estimate_and_economy_mode_end_with_the_job() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(&dir, project.path()).await;
    calls(&mut pipe, "p1", "plan", steps_with(1.0)).await;
    ends_costing(&mut pipe, 1.6).await;
    until(&engine, &id, economy_on).await;
    while rx.try_recv().is_ok() {}
    // The closing box belongs to a reply, so the agent closes the job in its next turn.
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
    let more = SendRequest {
        conversation_id: Some(id.clone()),
        area: Area::Project,
        project_id: None,
        text: "Then the rest".into(),
        images: vec![],
        files: vec![],
        choice,
    };
    engine.send(more).unwrap();
    until_turns(&pipe, 2).await;
    echo(&mut pipe, 1).await;
    // Every step done, an empty list, a list again: none of them ends the job, only the closing box does.
    let all_done = json!({ "steps": [{ "text": "Build it", "state": "done" }, { "text": "Check it", "state": "done" }] });
    for (call, input) in
        [("p3", all_done), ("p4", json!({ "steps": [] })), ("p5", json!({ "steps": [{ "text": "More", "state": "pending" }] }))]
    {
        let (said, error) = calls(&mut pipe, call, "plan", input).await;
        assert!(!error && said.contains("half as much again"), "{call}: {said}");
        assert!(economy_on(&engine.open(&id).unwrap()), "{call} left the job's estimate alone");
    }
    while rx.try_recv().is_ok() {}
    let done = json!({ "recap": "The login page and the rest are fixed. Both were checked.", "done": true });
    let (said, error) = calls(&mut pipe, "w1", "wrap_up", done).await;
    assert!(!error, "{said}");
    let c = until(&engine, &id, |c| c.estimate.is_none()).await;
    assert!(!economy_on(&c));
    let mut plans = 0;
    while let Ok(e) = rx.try_recv() {
        plans += usize::from(matches!(e, AppEvent::Plan { plan: Some(p), .. } if p.estimate.is_none()));
    }
    assert_eq!(plans, 1, "the screen hears that economy mode ended");
    let later = json!({ "steps": [{ "text": "Next job", "state": "in_progress" }] });
    let (said, error) = calls(&mut pipe, "p2", "plan", later).await;
    assert!(!error && !said.contains("half as much again"), "{said}");
    assert!(until(&engine, &id, |c| c.plan.as_ref().is_some_and(|p| p.steps[0].text == "Next job")).await.estimate.is_none());
    engine.shutdown();
}

/// Rule 7: the job's spend counts the conversations the agent started, not only the replies of its own.
#[tokio::test]
async fn the_spend_of_a_job_counts_the_conversations_the_agent_started() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut pipe, id, made) = started_made(&dir, project.path(), &[Vendor::Claude]).await;
    calls(&mut pipe, "p1", "plan", steps_with(1.0)).await;
    let (said, error) = calls(&mut pipe, "c1", "conversation", json!({ "title": "Docs", "brief": "Write the docs." })).await;
    assert!(!error, "{said}");
    let mut child = next_pipe(&made).await;
    until_turns(&child, 1).await;
    child.say(json!({ "type": "system", "subtype": "init", "session_id": "c", "model": "claude-haiku-5-5" })).await;
    ends_costing(&mut child, 1.6).await;
    let started = engine.state().conversations.into_iter().find(|c| c.title == "Docs").unwrap().id;
    until(&engine, &started, |c| c.messages.last().is_some_and(|m| m.cost_usd == Some(1.6))).await;
    let parent = engine.open(&id).unwrap();
    assert!((parent.spent_usd - 1.6).abs() < 1e-9, "the child's turn is added to the parent's total: {}", parent.spent_usd);
    assert!(!economy_on(&parent), "nothing is checked before the agent's own turn ends");
    // The agent's own turn cost nothing known, and the job is past its estimate all the same.
    ends_costing(&mut pipe, 0.0).await;
    until(&engine, &id, economy_on).await;
    engine.shutdown();
}

/// The model and effort of the helper that a Claude process asks for, as the Codex app-server it starts receives them. The
/// app-server is answered by hand: the model comes with the thread, the effort with the turn.
async fn helper_models(claude: &mut Pipe, made: &Made) -> (Option<String>, Option<String>) {
    let call = json!({ "jsonrpc": "2.0", "id": 8, "method": "tools/call",
        "params": { "name": "helper", "arguments": { "title": "Check", "brief": "What is the answer?" } } });
    claude
        .say(json!({ "type": "control_request", "request_id": "h1",
            "request": { "subtype": "mcp_message", "server_name": "faber", "message": call } }))
        .await;
    let mut codex = next_pipe(made).await;
    let (mut answered, mut seen) = (0, (None, None));
    // The loop answers the app-server with .await inside, which a closure cannot hold.
    for _ in 0..600 {
        let lines = codex.lines.lock().unwrap().clone();
        for m in lines.iter().skip(answered) {
            let result = match m["method"].as_str() {
                Some("thread/start") => {
                    seen.0 = m["params"]["model"].as_str().map(String::from);
                    json!({ "thread": { "id": "t1", "model": "gpt-6.1-sol" } })
                }
                Some("turn/start") => {
                    seen.1 = m["params"]["effort"].as_str().map(String::from);
                    json!({ "turn": { "id": "u1", "status": "inProgress" } })
                }
                _ => json!({}),
            };
            if m["method"].is_string() && !m["id"].is_null() {
                codex.say(json!({ "id": m["id"], "result": result })).await;
            }
        }
        answered = lines.len();
        if seen.1.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    seen
}

/// Rule 7: a helper the agent calls in economy mode runs one rung cheaper than usual, on the other AI's own model list.
#[tokio::test]
async fn a_helper_started_in_economy_mode_runs_one_rung_cheaper() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut pipe, id, made) = started_made(&dir, project.path(), &[Vendor::Claude, Vendor::Codex]).await;
    calls(&mut pipe, "p1", "plan", steps_with(1.0)).await;
    ends_costing(&mut pipe, 1.6).await;
    until(&engine, &id, economy_on).await;
    let seen = helper_models(&mut pipe, &made).await;
    assert_eq!(seen, (Some("gpt-6.1-sol".to_string()), Some("low".to_string())), "Sol at low effort, not medium");
    engine.shutdown();
}

/// Rule 7: the job is the lead's, so a helper of a conversation the lead started runs cheaper too once the lead's job is in
/// economy mode.
#[tokio::test]
async fn a_helper_of_a_conversation_the_lead_started_follows_the_leads_economy_mode() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut pipe, id, made) = started_made(&dir, project.path(), &[Vendor::Claude, Vendor::Codex]).await;
    calls(&mut pipe, "p1", "plan", steps_with(1.0)).await;
    let (said, error) = calls(&mut pipe, "c1", "conversation", json!({ "title": "Docs", "brief": "Write the docs." })).await;
    assert!(!error, "{said}");
    let mut child = next_pipe(&made).await;
    until_turns(&child, 1).await;
    child.say(json!({ "type": "system", "subtype": "init", "session_id": "c", "model": "claude-haiku-5-5" })).await;
    ends_costing(&mut child, 1.6).await;
    // The job is the lead's: its turn ends with the child's spend in the total, and economy mode starts.
    ends_costing(&mut pipe, 0.0).await;
    until(&engine, &id, economy_on).await;
    let seen = helper_models(&mut child, &made).await;
    assert_eq!(seen, (Some("gpt-6.1-sol".to_string()), Some("low".to_string())), "the child's helper runs cheaper as well");
    engine.shutdown();
}

/// Rule 7: the running total is saved with the conversation, so a restart in the middle of a job loses nothing: a turn of a
/// conversation the agent started, ended after the restart, is still added to the lead's total.
#[tokio::test]
async fn a_restart_mid_job_still_counts_the_conversations_the_agent_started() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut pipe, id, made) = started_made(&dir, project.path(), &[Vendor::Claude]).await;
    calls(&mut pipe, "p1", "plan", steps_with(1.0)).await;
    let (said, error) = calls(&mut pipe, "c1", "conversation", json!({ "title": "Docs", "brief": "Write the docs." })).await;
    assert!(!error, "{said}");
    let mut child = next_pipe(&made).await;
    until_turns(&child, 1).await;
    child.say(json!({ "type": "system", "subtype": "init", "session_id": "c", "model": "claude-haiku-5-5" })).await;
    ends_costing(&mut child, 0.8).await;
    let started = engine.state().conversations.into_iter().find(|c| c.title == "Docs").unwrap().id;
    until(&engine, &started, |c| c.messages.last().is_some_and(|m| m.cost_usd == Some(0.8))).await;
    assert!((engine.open(&id).unwrap().spent_usd - 0.8).abs() < 1e-9);
    engine.shutdown();

    // A new engine reads everything from disk; the child goes on with a turn of the person's.
    let (spawn, made) = pipes();
    let (again, _rx) = engine_with(&dir, spawn, &[Vendor::Claude]);
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
    let more = SendRequest {
        conversation_id: Some(started.clone()),
        area: Area::Project,
        project_id: None,
        text: "And the changelog".into(),
        images: vec![],
        files: vec![],
        choice,
    };
    again.send(more).unwrap();
    let mut child = next_pipe(&made).await;
    until_turns(&child, 1).await;
    child.say(json!({ "type": "system", "subtype": "init", "session_id": "c2", "model": "claude-haiku-5-5" })).await;
    ends_costing(&mut child, 0.9).await;
    let c = until(&again, &started, |c| idle(c) && c.messages.len() == 4).await;
    let paid: f64 = c.messages.iter().filter_map(|m| m.cost_usd).sum();
    assert!(paid > 0.8 + 1e-9, "the second turn cost something: {paid}");
    let lead = again.open(&id).unwrap();
    assert!((lead.spent_usd - paid).abs() < 1e-9, "the lead counts both turns of the child: {} against {paid}", lead.spent_usd);
    assert!((c.spent_usd - paid).abs() < 1e-9, "and the child its own: {}", c.spent_usd);
    again.shutdown();
}

/// Rule 7: a quick question on the side is no reply, yet its cost joins the job's spend and can tip it over the limit.
#[tokio::test]
async fn a_quick_question_on_the_side_tips_the_spend_of_the_job() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut pipe, id, made) = started_made(&dir, project.path(), &[Vendor::Claude]).await;
    calls(&mut pipe, "p1", "plan", steps_with(1.0)).await;
    pipe.say(json!({"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Reading."}}})).await;
    until(&engine, &id, |c| c.resumable(Vendor::Claude).is_some()).await;
    let aside = engine.ask_aside(&id, "why that file?", None).unwrap();
    let mut side = next_pipe(&made).await;
    side.say(json!({"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"It holds the parser."}}})).await;
    side.say(json!({"type":"result","subtype":"success","is_error":false,"total_cost_usd":1.6,"usage":{}})).await;
    support::until("the side question never finished", || engine.asides(&id).iter().any(|a| a.id == aside && a.state == "done")).await;
    let c = engine.open(&id).unwrap();
    assert!((c.spent_usd - 1.6).abs() < 1e-9, "the question's cost is in the total: {}", c.spent_usd);
    assert!(!economy_on(&c), "nothing is checked before the agent's own turn ends");
    // The agent's own turn cost nothing known: the side question alone passed the estimate.
    ends_costing(&mut pipe, 0.0).await;
    until(&engine, &id, economy_on).await;
    engine.shutdown();
}

/// Rule 7: a conversation saved before estimates existed loads without one, and an estimate saved before economy mode loads with it off.
#[tokio::test]
async fn a_conversation_saved_before_the_estimate_loads_without_it() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, _rx, mut pipe, id) = started(&dir, project.path()).await;
    calls(&mut pipe, "p1", "plan", steps_with(1.0)).await;
    engine.shutdown();
    let file = dir.path().join("conversations").join(format!("{id}.json"));
    let mut saved: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert!(saved["plan"].get("estimate").is_none(), "the list carries none: the estimate belongs to the job");
    let estimate = saved["estimate"].as_object_mut().expect("the estimate is saved on the conversation");
    assert!(estimate.remove("economy").is_some());
    std::fs::write(&file, saved.to_string()).unwrap();
    let (again, _rx) = engine_with(&dir, pipes().0, &[Vendor::Claude]);
    let e = again.open(&id).unwrap().estimate.expect("the estimate loads");
    assert_eq!((e.cents, e.base, e.economy), (100, 0, false), "an estimate with no economy flag has it off");
    again.shutdown();
    saved.as_object_mut().unwrap().remove("estimate");
    std::fs::write(&file, saved.to_string()).unwrap();
    let (again, _rx) = engine_with(&dir, pipes().0, &[Vendor::Claude]);
    let c = again.open(&id).unwrap();
    assert_eq!(
        (c.plan.expect("the plan loads").steps.len(), c.estimate, c.spent_usd),
        (2, None, 0.0),
        "a conversation with no estimate loads as before"
    );
    again.shutdown();
}

/// Task 017: under Keep going, a closing box that says the request is done ends it like the stop line, on the second time
/// (the first is checked against the original request).
#[tokio::test]
async fn keep_going_ends_when_the_closing_box_says_the_request_is_done() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut rx, mut pipe, id) = started(&dir, project.path()).await;
    engine.keep_going(&id, true).unwrap();
    let done = json!({ "recap": "The login page and the rest are fixed. Both were checked.", "done": true });
    calls(&mut pipe, "d1", "wrap_up", done.clone()).await;
    answers_at(&mut pipe, "All fixed.", true, 0.1).await;
    until_turns(&pipe, 2).await;
    assert!(turns(&pipe)[1].contains("original request word for word"), "{:?}", turns(&pipe));
    calls(&mut pipe, "d2", "wrap_up", done).await;
    answers_at(&mut pipe, "Nothing is left.", true, 0.1).await;
    let c = until(&engine, &id, |c| idle(c) && c.keep_going.is_none()).await;
    assert_eq!(c.messages.last().unwrap().halt.as_deref(), Some("reason.keepGoing.done"));
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(turns(&pipe).len(), 2, "no turn after nothing was left");
    assert_eq!(halts(&mut rx), vec!["reason.keepGoing.done".to_string()]);
    engine.shutdown();
}

/// Task 017: under Keep going, a closing box with a question waits for the person; when nobody answers, Faber sends the
/// recommended reply (the first button), says so to the agent, and the box shows which reply it chose.
#[tokio::test]
async fn keep_going_answers_a_closing_question_nobody_answered_with_its_first_reply() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, _rx, mut pipe, id) = started(&dir, project.path()).await;
    engine.keep_going(&id, true).unwrap();
    let ask = json!({ "recap": "The login page works. The docs are not written.", "ask": "Write the docs now?", "actions": ["Yes, write them", "Skip"] });
    calls(&mut pipe, "q1", "wrap_up", ask).await;
    answers_at(&mut pipe, "Login fixed.", true, 0.1).await;
    let c = until(&engine, &id, |c| c.keep_going.as_ref().is_some_and(|k| k.waiting.is_some())).await;
    assert_eq!(c.keep_going.unwrap().waiting.map(|w| w.why), Some("question".to_string()));
    until_turns(&pipe, 2).await;
    let sent = &turns(&pipe)[1];
    assert!(sent.contains("did not answer your question in time") && sent.contains("\"Yes, write them\""), "{sent}");
    let c = until(&engine, &id, |c| c.messages.len() >= 4).await;
    let reply = c.messages.iter().rev().find(|m| m.wrap_up.is_some()).unwrap();
    assert_eq!(reply.wrap_up.as_ref().unwrap().answered.as_deref(), Some("Yes, write them"));
    assert_eq!(c.keep_going.unwrap().answered, 1);
    engine.shutdown();
}

/// Task 017, review: Keep going turned on after a closing question already waits gives the person the same ten minutes
/// before Faber answers it for them.
#[tokio::test]
async fn keep_going_turned_on_after_a_closing_question_still_waits_before_answering_it() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, _rx, mut pipe, id) = started(&dir, project.path()).await;
    let ask = json!({ "recap": "The login page works.", "ask": "Write the docs now?", "actions": ["Yes", "Skip"] });
    calls(&mut pipe, "q1", "wrap_up", ask).await;
    answers_at(&mut pipe, "Login fixed.", true, 0.1).await;
    until(&engine, &id, idle).await;
    engine.keep_going(&id, true).unwrap();
    let c = until(&engine, &id, |c| c.keep_going.as_ref().is_some_and(|k| k.waiting.is_some())).await;
    assert_eq!(c.keep_going.unwrap().waiting.map(|w| w.why), Some("question".to_string()));
    assert_eq!(turns(&pipe).len(), 1, "nothing is sent while the person may still answer");
    until_turns(&pipe, 2).await;
    assert!(turns(&pipe)[1].contains("\"Yes\""), "{:?}", turns(&pipe));
    engine.shutdown();
}

#[tokio::test]
async fn failed_turns_drain_sibling_questions_in_order_without_retrying_or_sending_human_messages() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut agent, id, made) = started_made(&dir, project.path(), &[Vendor::Claude]).await;
    let (said, error) = calls(&mut agent, "start", "conversation", json!({ "title": "Questions", "brief": "Check the docs." })).await;
    assert!(!error, "{said}");
    // These checks exercise queued wakes after failure, not messages already handed into a safe step.
    agent.say(json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_hold","name":"Read","input":{"file_path":"synthetic.txt"}}]}})).await;
    until(&engine, &id, |c| c.messages.last().is_some_and(|m| m.tools.iter().flatten().any(|t| t.id == "toolu_hold"))).await;
    let mut sibling = next_pipe(&made).await;
    until_turns(&sibling, 1).await;
    sibling.say(json!({ "type": "system", "subtype": "init", "session_id": "questions", "model": "claude-haiku-5-5" })).await;
    for n in 0..3 {
        let (said, error) = calls(
            &mut sibling,
            &format!("ask{n}"),
            "conversation",
            json!({ "action": "ask", "id": id, "message": format!("Question {n}") }),
        )
        .await;
        assert!(!error, "{said}");
        if n == 0 {
            engine
                .send(SendRequest {
                    conversation_id: Some(id.clone()),
                    area: Area::Project,
                    project_id: None,
                    text: "Also the footer".into(),
                    images: vec![],
                    files: vec![],
                    choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
                })
                .unwrap();
        }
    }
    assert_eq!(engine.open(&id).unwrap().queued.len(), 4);
    let mut finished = vec![];
    for n in 0..3 {
        // EOF is a fatal Error; an unsuccessful result is Stopped and deliberately holds the queue.
        agent.out.shutdown().await.unwrap();
        finished.push(agent);
        agent = next_pipe(&made).await;
        until_turns(&agent, 1).await;
        agent.say(json!({ "type": "system", "subtype": "init", "session_id": format!("wake{n}"), "model": "claude-haiku-5-5" })).await;
        assert!(turns(&agent)[0].contains(&format!("Question {n}")), "{:?}", turns(&agent));
        let c = engine.open(&id).unwrap();
        assert_eq!(c.queued.len(), 3 - n, "each wake is removed exactly once");
        assert!(c.queued.iter().any(|q| q.wake.is_none() && q.text == "Also the footer"));
    }
    agent.out.shutdown().await.unwrap();
    let c = until(&engine, &id, idle).await;
    assert_eq!(c.messages.last().unwrap().status, Some(Status::Error));
    assert_eq!(c.queued.len(), 1);
    assert_eq!(c.queued[0].text, "Also the footer");
    tokio::time::sleep(Duration::from_millis(100)).await;
    finished.push(agent);
    assert!(finished.iter().all(|pipe| turns(pipe).len() == 1), "failed wake turns are never retried");
    assert!(made.lock().unwrap().is_empty(), "no extra process retries a failed wake");
    engine.shutdown();
}

#[tokio::test]
async fn a_new_sibling_question_after_an_error_waits_behind_older_questions() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut agent, id, made) = started_made(&dir, project.path(), &[Vendor::Claude]).await;
    let (said, error) = calls(&mut agent, "start", "conversation", json!({ "title": "Questions", "brief": "Check the docs." })).await;
    assert!(!error, "{said}");
    let mut sibling = next_pipe(&made).await;
    until_turns(&sibling, 1).await;
    sibling.say(json!({ "type": "system", "subtype": "init", "session_id": "questions", "model": "claude-haiku-5-5" })).await;
    for n in 0..3 {
        let (said, error) = calls(
            &mut sibling,
            &format!("ask{n}"),
            "conversation",
            json!({ "action": "ask", "id": id, "message": format!("Question {n}") }),
        )
        .await;
        assert!(!error, "{said}");
    }
    let mut saved = engine.open(&id).unwrap();
    let mut questions = engine.open(&titled(&engine, "Questions")).unwrap();
    engine.shutdown();
    // Reproduce an existing Error with older asks still saved, as in a conversation left by an earlier version.
    saved.messages.last_mut().unwrap().status = Some(Status::Error);
    let file = dir.path().join("conversations").join(format!("{id}.json"));
    std::fs::write(file, serde_json::to_vec(&saved).unwrap()).unwrap();
    // Only the asks wait here: "Questions" is saved as finished, since the report a member cut off by the close owes
    // (engine.rs, `a_team_member_cut_off_by_faber_closing_is_reported_to_its_leader_once`) would wake this one first.
    questions.messages.last_mut().unwrap().status = Some(Status::Done);
    let file = dir.path().join("conversations").join(format!("{}.json", questions.id));
    std::fs::write(file, serde_json::to_vec(&questions).unwrap()).unwrap();
    let (spawn, made) = pipes();
    let (again, _rx) = engine_with(&dir, spawn, &[Vendor::Claude]);
    again
        .send(SendRequest {
            conversation_id: None,
            area: Area::Project,
            project_id: saved.project_id,
            text: "Ask the other agent".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
        })
        .unwrap();
    let mut sibling = next_pipe(&made).await;
    until_turns(&sibling, 1).await;
    sibling.say(json!({ "type": "system", "subtype": "init", "session_id": "new-questions", "model": "claude-haiku-5-5" })).await;
    let (said, error) =
        calls(&mut sibling, "newest", "conversation", json!({ "action": "ask", "id": id, "message": "Newest question" })).await;
    assert!(!error, "{said}");
    let mut target = next_pipe(&made).await;
    until_turns(&target, 1).await;
    target.say(json!({ "type": "system", "subtype": "init", "session_id": "resumed", "model": "claude-haiku-5-5" })).await;
    assert!(turns(&target)[0].contains("Question 0"), "{:?}", turns(&target));
    let mut finished = vec![];
    for n in 1..3 {
        target.out.shutdown().await.unwrap();
        finished.push(target);
        target = next_pipe(&made).await;
        until_turns(&target, 1).await;
        target.say(json!({ "type": "system", "subtype": "init", "session_id": format!("resumed{n}"), "model": "claude-haiku-5-5" })).await;
        assert!(turns(&target)[0].contains(&format!("Question {n}")), "{:?}", turns(&target));
    }
    target.out.shutdown().await.unwrap();
    finished.push(target);
    let mut target = next_pipe(&made).await;
    until_turns(&target, 1).await;
    assert!(turns(&target)[0].contains("Newest question"));
    target.say(json!({ "type": "system", "subtype": "init", "session_id": "newest", "model": "claude-haiku-5-5" })).await;
    target.out.shutdown().await.unwrap();
    let c = until(&again, &id, idle).await;
    assert_eq!(c.messages.last().unwrap().status, Some(Status::Error));
    tokio::time::sleep(Duration::from_millis(100)).await;
    finished.push(target);
    assert!(finished.iter().all(|pipe| turns(pipe).len() == 1), "each question is sent once");
    assert!(made.lock().unwrap().is_empty(), "no extra process retries a failed wake");
    again.shutdown();
}

#[tokio::test]
async fn wakes_that_cannot_start_fail_once_and_drain_the_remaining_wakes() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, mut agent, id, made) = started_made(&dir, project.path(), &[Vendor::Claude]).await;
    let (said, error) = calls(&mut agent, "start", "conversation", json!({ "title": "Questions", "brief": "Check the docs." })).await;
    assert!(!error, "{said}");
    // The billing change must precede delivery, so keep the initial turn inside a step.
    agent.say(json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_hold","name":"Read","input":{"file_path":"synthetic.txt"}}]}})).await;
    until(&engine, &id, |c| c.messages.last().is_some_and(|m| m.tools.iter().flatten().any(|t| t.id == "toolu_hold"))).await;
    let mut sibling = next_pipe(&made).await;
    until_turns(&sibling, 1).await;
    sibling.say(json!({ "type": "system", "subtype": "init", "session_id": "questions", "model": "claude-haiku-5-5" })).await;
    for n in 0..3 {
        let (said, error) = calls(
            &mut sibling,
            &format!("ask{n}"),
            "conversation",
            json!({ "action": "ask", "id": id, "message": format!("Question {n}") }),
        )
        .await;
        assert!(!error, "{said}");
    }
    let billing = faber_engine::signals::Signals { billing: Some(faber_engine::budget::Billing::UsageBilled), ..Default::default() };
    engine.note_account_events(Vendor::Claude, vec![faber_engine::events::EngineEvent::Signals(billing)]);
    agent.out.shutdown().await.unwrap();
    let c = until(&engine, &id, |c| idle(c) && c.queued.is_empty()).await;
    // Three refused questions, then the report of the conversation it started (a wake too): one failed reply each.
    assert_eq!(c.messages.len(), 10, "each refused wake has one failed reply");
    for n in 0..3 {
        assert!(c.messages[2 * n + 2].text.contains(&format!("Question {n}")));
        assert_eq!(c.messages[2 * n + 3].status, Some(Status::Error));
    }
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert_eq!(turns(&agent).len(), 1, "the billing gate starts no wake turn");
    assert_eq!(engine.open(&id).unwrap().messages.len(), 10, "no failed reply retries itself");
    engine.shutdown();
}

#[tokio::test]
async fn two_silent_keep_going_turns_in_a_row_halt_on_the_last_reply_with_content() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, _rx, mut pipe, id) = started(&dir, project.path()).await;
    engine.keep_going(&id, true).unwrap();
    answers(&mut pipe, "All three items are waiting on your decision.", true).await;
    until(&engine, &id, |c| c.messages.len() == 4).await;
    answers(&mut pipe, "", true).await;
    until(&engine, &id, |c| c.messages.len() == 6).await;
    answers(&mut pipe, "", true).await;
    let c = until(&engine, &id, |c| c.keep_going.is_none()).await;
    assert_eq!(c.messages[1].halt.as_deref(), Some("reason.keepGoing.done"));
    assert_eq!(c.messages.last().unwrap().halt, None);
    engine.shutdown();
}

/// The agent calls the conversation tool's `start` with these arguments; Faber's answer.
async fn start_team_member(pipe: &mut Pipe, rpc: u64, args: Value) -> Value {
    pipe.say(json!({"type":"control_request","request_id":format!("r{rpc}"),"request":{"subtype":"mcp_message","server_name":"faber",
        "message":{"jsonrpc":"2.0","id":rpc,"method":"tools/call","params":{"name":"conversation","arguments":args,
        "_meta":{"claudecode/toolUseId":format!("toolu_{rpc}")}}}}}))
        .await;
    support::until(&format!("r{rpc} was never answered"), || {
        pipe.lines.lock().unwrap().iter().any(|m| m["response"]["request_id"] == format!("r{rpc}"))
    })
    .await;
    let lines = pipe.lines.lock().unwrap();
    let m = lines.iter().find(|m| m["response"]["request_id"] == format!("r{rpc}")).unwrap();
    m["response"]["response"]["mcp_response"]["result"].clone()
}

fn said(result: &Value) -> String {
    result["content"][0]["text"].as_str().unwrap_or_default().to_string()
}

fn titled(engine: &Engine, title: &str) -> String {
    engine.state().conversations.into_iter().find(|c| c.title == title).expect("the new conversation is listed").id
}

/// A parent pinned to Claude spreads its team: a named AI runs the new conversation (the person's pin stays on the parent),
/// "auto" leaves it to the router, and an AI that cannot run work nobody sent by hand is refused with the reason.
#[tokio::test]
async fn the_agent_picks_the_ai_of_a_team_conversation_and_one_that_cannot_run_is_refused() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, _rx, mut pipe, parent, bins) = started_on(&dir, project.path(), &[Vendor::Claude, Vendor::Codex]).await;
    let review = start_team_member(&mut pipe, 1, json!({"title":"Review","brief":"Review the login fix.","ai":"codex"})).await;
    assert_eq!(review["isError"], false, "{}", said(&review));
    let choice = |id: &str| engine.open(id).unwrap().choice.map(|c| c.vendor);
    assert_eq!(choice(&titled(&engine, "Review")).as_deref(), Some("codex"));
    assert_eq!(choice(&parent).as_deref(), Some("claude"), "never the conversation that asked");
    support::until("the first turn never started on Codex", || bins.lock().unwrap().iter().any(|b| b == "codex")).await;
    assert!(bins.lock().unwrap().iter().any(|b| b == "codex"), "its first turn runs on Codex: {:?}", bins.lock().unwrap());
    let docs = start_team_member(&mut pipe, 2, json!({"title":"Docs","brief":"Write the docs.","ai":"auto"})).await;
    assert_eq!(docs["isError"], false, "{}", said(&docs));
    assert_eq!(choice(&titled(&engine, "Docs")).as_deref(), Some("auto"), "the router picks, whatever the parent is pinned to");
    let billed = faber_engine::signals::Signals { billing: Some(faber_engine::budget::Billing::UsageBilled), ..Default::default() };
    engine.note_account_events(Vendor::Codex, vec![faber_engine::events::EngineEvent::Signals(billed)]);
    let refused = start_team_member(&mut pipe, 3, json!({"title":"More","brief":"Do more.","ai":"codex"})).await;
    assert!(refused["isError"] == true && said(&refused).contains("billed per use"), "{}", said(&refused));
    assert!(engine.state().conversations.iter().all(|c| c.title != "More"), "a refused start leaves nothing behind");
    engine.shutdown();

    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, _rx, mut pipe, _, _) = started_on(&dir, project.path(), &[Vendor::Claude]).await;
    let refused = start_team_member(&mut pipe, 1, json!({"title":"Review","brief":"Review it.","ai":"codex"})).await;
    assert!(said(&refused).contains("not connected"), "{}", said(&refused));
    engine.shutdown();
}

/// A team member given an ordered list goes through it with Keep going, and its parent hears once, when the list ends.
#[tokio::test]
async fn a_team_member_given_a_list_goes_through_it_and_reports_once_when_it_ends() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, _rx, mut pipe, parent, _, made) = started_with(&dir, project.path(), &[Vendor::Claude], &|_| {}).await;
    let started = start_team_member(&mut pipe, 1, json!({"title":"List","brief":"1. Fix a. 2. Fix b. 3. Fix c.","keep_going":true})).await;
    assert_eq!(started["isError"], false, "{}", said(&started));
    let child = titled(&engine, "List");
    let mut member = next_pipe(&made).await;
    until_turns(&member, 1).await;
    member.say(json!({ "type": "system", "subtype": "init", "session_id": "m", "model": "claude-haiku-5-5" })).await;
    assert!(engine.open(&child).unwrap().keep_going.is_some(), "Keep going is on for it");
    let reports = |c: &Conversation| c.queued.iter().filter(|q| q.wake.as_ref().is_some_and(|w| w.why == "wake.childDone")).count();
    for (turn, text) in ["Fixed a.", "Fixed b.", "Fixed c.\n[keep going: stop]"].into_iter().enumerate() {
        answers_at(&mut member, text, true, 0.1).await;
        until_turns(&member, turn + 2).await;
        assert_eq!(reports(&engine.open(&parent).unwrap()), 0, "no report while the list goes on");
    }
    answers_at(&mut member, "Nothing is left.\n[keep going: stop]", true, 0.1).await;
    until(&engine, &child, |c| idle(c) && c.keep_going.is_none()).await;
    until(&engine, &parent, |c| reports(c) == 1).await;
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(reports(&engine.open(&parent).unwrap()), 1, "one report, when the list ended");
    engine.shutdown();
}

/// Method 2, slice 6: the project's local count (`method/<project id>.jsonl`) takes a done closing box, and the person's next
/// message after it as a correction. A box that is not done, or a project that is gone, leaves nothing.
#[tokio::test]
async fn a_message_after_a_done_closing_box_is_counted_as_a_correction_and_a_first_close_is_not() {
    let (dir, project) = (support::DataFolder::new(), support::work_folder());
    let (engine, _rx, mut pipe, id) = started(&dir, project.path()).await;
    let project_id = engine.open(&id).unwrap().project_id.unwrap();
    let count = || faber_engine::method_log::counts(dir.path(), &project_id);
    let wait = |want: faber_engine::method_log::Counts| async move {
        for _ in 0..300 {
            if count() == want {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the count is {:?}, not {want:?}", count());
    };
    // A box that is not done is no close, and the message after it is no correction.
    calls(&mut pipe, "w0", "wrap_up", json!({ "recap": "The login page works. The rest is not done." })).await;
    answers(&mut pipe, "Login fixed.", true).await;
    until(&engine, &id, idle).await;
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
    let follow = |text: &str| SendRequest {
        conversation_id: Some(id.clone()),
        area: Area::Project,
        project_id: None,
        text: text.into(),
        images: vec![],
        files: vec![],
        choice: choice.clone(),
    };
    engine.send(follow("Now the rest")).unwrap();
    until_turns(&pipe, 2).await;
    echo(&mut pipe, 1).await;
    // A done box is a close, and no correction before the person writes again.
    calls(&mut pipe, "w1", "wrap_up", json!({ "recap": "Everything is fixed and checked.", "done": true })).await;
    answers(&mut pipe, "All fixed.", true).await;
    until(&engine, &id, |c| idle(c) && c.messages.len() == 4).await;
    for _ in 0..300 {
        if count().closed == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!((count().closed, count().corrected), (1, 0));
    // A send the engine refuses (a file it will not take) joins nothing, so it is no correction, however often it is retried.
    for _ in 0..2 {
        let refused = SendRequest {
            files: vec![faber_engine::workspace::Attachment { name: ".npmrc".into(), base64: "c3ludGhldGlj".into() }],
            ..follow("The button is still wrong")
        };
        assert_eq!(engine.send(refused).unwrap_err(), "error.projectSandbox");
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(count().corrected, 0);
    engine.send(follow("The button is still wrong")).unwrap();
    until_turns(&pipe, 3).await;
    wait(faber_engine::method_log::Counts { closed: 1, corrected: 1, close_secs: count().close_secs }).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(count().corrected, 1, "one message, one correction");
    engine.shutdown();
}

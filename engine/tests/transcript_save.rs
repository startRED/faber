//! A conversation record is written at most every two seconds while helpers work, and at once at every point that needs it.
//! The pages of a helper's transcript are written as they arrive, so what a crash leaves out of the record is found again.

use crate::support::{self, Pipe, next_pipe, pipes};
use faber_engine::cli::Cli;
use faber_engine::model::{Area, Permissions, Settings, Vendor};
use faber_engine::router::Choice;
use faber_engine::{Engine, EngineOptions, SendRequest};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const CALL: &str = "toolu_A";

fn engine(dir: &Path) -> (Engine, Arc<std::sync::Mutex<Vec<Pipe>>>) {
    let (spawn, made) = pipes();
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
    (Engine::new(opts, Arc::new(|_| ())).unwrap(), made)
}

fn send(engine: &Engine, id: Option<String>) -> String {
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
    engine
        .send(SendRequest {
            conversation_id: id,
            area: Area::Chat,
            project_id: None,
            text: "go".into(),
            images: vec![],
            files: vec![],
            choice,
        })
        .unwrap()
}

fn child_line(content: Value) -> Value {
    json!({ "type": "assistant", "message": { "role": "assistant", "content": content }, "parent_tool_use_id": CALL })
}

/// One helper step: some words, then a tool it runs.
fn child_pair(n: usize) -> [Value; 2] {
    [
        child_line(json!([{ "type": "text", "text": format!("Step {n} looks first.") }])),
        child_line(json!([{ "type": "tool_use", "id": format!("step{n}"), "name": "Grep", "input": { "pattern": format!("x{n}") } }])),
    ]
}

/// Starts a turn whose agent calls a helper, and waits until the engine holds the call.
async fn start(dir: &Path, id: Option<String>) -> (Engine, Pipe, String) {
    let (engine, made) = engine(dir);
    let id = send(&engine, id);
    let mut pipe = next_pipe(&made).await;
    pipe.say(json!({ "type": "system", "subtype": "init", "session_id": "s", "model": "haiku" })).await;
    pipe.say(json!({
        "type": "assistant", "parent_tool_use_id": null,
        "message": { "role": "assistant", "content": [{ "type": "tool_use", "id": CALL, "name": "Agent",
            "input": { "description": "d", "subagent_type": "Explore", "prompt": "p", "run_in_background": false } }] }
    }))
    .await;
    wait(|| helper_pages(&engine, &id).is_some()).await;
    // Whatever the start of the turn writes has been written by now.
    tokio::time::sleep(Duration::from_millis(100)).await;
    (engine, pipe, id)
}

async fn wait(mut ready: impl FnMut() -> bool) {
    let started = Instant::now();
    while !ready() {
        assert!(started.elapsed() < Duration::from_secs(10), "the engine never got there");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// The page count the engine holds in memory for the helper call.
fn helper_pages(engine: &Engine, id: &str) -> Option<u64> {
    engine.open(id)?.messages.iter().flat_map(|m| m.tools.iter().flatten()).find(|t| t.id == CALL)?.transcript.as_ref().map(|t| t.pages)
}

async fn feed(engine: &Engine, pipe: &mut Pipe, id: &str, steps: usize) {
    for n in 0..steps {
        for line in child_pair(n) {
            pipe.say(line).await;
        }
    }
    wait(|| helper_pages(engine, id) == Some(2 * steps as u64)).await;
}

fn record_path(dir: &Path, id: &str) -> PathBuf {
    dir.join("conversations").join(format!("{id}.json"))
}

/// The page count in the conversation file itself, as written: none when the file does not hold the call.
fn written_pages(dir: &Path, id: &str) -> Option<u64> {
    let record: Value = serde_json::from_slice(&std::fs::read(record_path(dir, id)).ok()?).ok()?;
    record["messages"].as_array()?.iter().flat_map(|m| m["tools"].as_array().into_iter().flatten()).find(|t| t["id"] == CALL)?["transcript"]
        ["pages"]
        .as_u64()
}

/// The page count and steps a fresh start finds for the helper call.
fn reloaded(dir: &Path, id: &str) -> Option<(u64, usize)> {
    let saved = faber_engine::store::Store::new(dir).get(id)?;
    let tools: Vec<_> = saved.messages.iter().flat_map(|m| m.tools.iter().flatten()).collect();
    let pages = tools.iter().find(|t| t.id == CALL)?.transcript.as_ref()?.pages;
    Some((pages, tools.iter().filter(|t| t.parent.as_deref() == Some(CALL)).count()))
}

#[tokio::test]
async fn many_helper_events_inside_the_window_write_the_conversation_once_and_not_once_each() {
    let dir = support::tempdir();
    let (engine, mut pipe, id) = start(dir.path(), None).await;
    let before = engine.conversation_writes();
    feed(&engine, &mut pipe, &id, 40).await;
    assert_eq!(
        engine.conversation_writes(),
        before,
        "80 events inside the window wrote the record {} times",
        engine.conversation_writes() - before
    );
    tokio::time::sleep(Duration::from_millis(2300)).await;
    assert_eq!(engine.conversation_writes(), before + 1, "the timer writes it once");
    assert_eq!(written_pages(dir.path(), &id), Some(80), "and the record then holds every page");
    tokio::time::sleep(Duration::from_millis(2300)).await;
    assert_eq!(engine.conversation_writes(), before + 1, "nothing is dirty, so nothing is written again");
    engine.shutdown();
}

#[tokio::test]
async fn a_timed_write_that_failed_is_tried_again_without_waiting_for_another_event() {
    let dir = support::tempdir();
    let (engine, mut pipe, id) = start(dir.path(), None).await;
    // A directory where the temp file goes makes the write fail while the old file stays valid.
    let blocker = dir.path().join("conversations").join(format!("{id}.json.tmp"));
    std::fs::create_dir(&blocker).unwrap();
    feed(&engine, &mut pipe, &id, 3).await;
    tokio::time::sleep(Duration::from_millis(2300)).await;
    assert_eq!(written_pages(dir.path(), &id), None, "the timed write failed");
    std::fs::remove_dir(&blocker).unwrap();
    // No helper event comes after this: the record still gets written.
    wait(|| written_pages(dir.path(), &id) == Some(6)).await;
    engine.shutdown();
}

#[tokio::test]
async fn a_crash_inside_the_window_loses_no_page_and_no_helper_step_even_from_the_first_page_of_a_call() {
    let dir = support::tempdir();
    let (engine, mut pipe, id) = start(dir.path(), None).await;
    feed(&engine, &mut pipe, &id, 5).await;
    assert_eq!(written_pages(dir.path(), &id), None, "the record has not seen the helper call at all (the crash window)");
    // A new start reads the folder as the crash left it, while the first engine still holds its unwritten state.
    assert_eq!(reloaded(dir.path(), &id), Some((10, 5)), "every page and every step is found again, the call too");
    let pages = faber_engine::transcript::TranscriptStore::new(dir.path().join("child-transcripts"));
    assert_eq!(pages.read_page(&id, CALL, 9).unwrap().unwrap()[0].tool.as_deref(), Some("step4"));
    engine.shutdown();
}

#[tokio::test]
async fn a_crash_after_part_of_the_pages_were_recorded_finds_the_rest() {
    let dir = support::tempdir();
    let (engine, mut pipe, id) = start(dir.path(), None).await;
    feed(&engine, &mut pipe, &id, 3).await;
    tokio::time::sleep(Duration::from_millis(2300)).await;
    assert_eq!(written_pages(dir.path(), &id), Some(6));
    for n in 3..7 {
        for line in child_pair(n) {
            pipe.say(line).await;
        }
    }
    wait(|| helper_pages(&engine, &id) == Some(14)).await;
    assert_eq!(written_pages(dir.path(), &id), Some(6), "the record is behind");
    assert_eq!(reloaded(dir.path(), &id), Some((14, 7)));
    engine.shutdown();
}

#[tokio::test]
async fn closing_faber_writes_the_record_at_once() {
    let dir = support::tempdir();
    let (engine, mut pipe, id) = start(dir.path(), None).await;
    feed(&engine, &mut pipe, &id, 10).await;
    assert_ne!(written_pages(dir.path(), &id), Some(20));
    engine.shutdown();
    assert_eq!(written_pages(dir.path(), &id), Some(20));
}

#[tokio::test]
async fn renaming_writes_the_record_at_once() {
    let dir = support::tempdir();
    let (engine, mut pipe, id) = start(dir.path(), None).await;
    feed(&engine, &mut pipe, &id, 10).await;
    assert_ne!(written_pages(dir.path(), &id), Some(20));
    engine.rename(&id, "Renamed");
    assert_eq!(written_pages(dir.path(), &id), Some(20));
    engine.shutdown();
}

#[tokio::test]
async fn the_end_of_the_turn_writes_the_record_at_once() {
    let dir = support::tempdir();
    let (engine, mut pipe, id) = start(dir.path(), None).await;
    feed(&engine, &mut pipe, &id, 10).await;
    assert_ne!(written_pages(dir.path(), &id), Some(20));
    pipe.say(json!({ "type": "user", "parent_tool_use_id": null,
        "message": { "role": "user", "content": [{ "type": "tool_result", "tool_use_id": CALL, "content": "report" }] } }))
        .await;
    pipe.say(json!({ "type": "assistant", "message": { "role": "assistant", "content": [{ "type": "text", "text": "answer" }] } })).await;
    pipe.say(json!({ "type": "result", "subtype": "success", "is_error": false, "total_cost_usd": 0.0, "usage": {} })).await;
    wait(|| written_pages(dir.path(), &id) == Some(21)).await;
    engine.shutdown();
}

#[tokio::test]
async fn a_question_for_the_person_writes_the_record_at_once() {
    let dir = support::tempdir();
    let (engine, made) = engine(dir.path());
    engine.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
    let id = send(&engine, None);
    let mut pipe = next_pipe(&made).await;
    pipe.say(json!({ "type": "system", "subtype": "init", "session_id": "s", "model": "haiku" })).await;
    pipe.say(json!({
        "type": "assistant", "parent_tool_use_id": null,
        "message": { "role": "assistant", "content": [{ "type": "tool_use", "id": CALL, "name": "Agent",
            "input": { "description": "d", "subagent_type": "Explore", "prompt": "p", "run_in_background": false } }] }
    }))
    .await;
    wait(|| helper_pages(&engine, &id).is_some()).await;
    feed(&engine, &mut pipe, &id, 10).await;
    assert_ne!(written_pages(dir.path(), &id), Some(20));
    pipe.say(json!({ "type": "control_request", "request_id": "q", "request": { "subtype": "can_use_tool", "tool_name": "Write",
        "input": { "file_path": "a.txt", "content": "x" } } }))
        .await;
    wait(|| written_pages(dir.path(), &id) == Some(20)).await;
    let record: Value = serde_json::from_slice(&std::fs::read(record_path(dir.path(), &id)).unwrap()).unwrap();
    assert!(record["messages"].as_array().unwrap().iter().any(|m| m["permissions"].as_array().is_some_and(|p| !p.is_empty())));
    engine.shutdown();
}

#[tokio::test]
async fn deleting_a_conversation_leaves_nothing_for_the_timer_to_write_back() {
    let dir = support::tempdir();
    let (engine, mut pipe, id) = start(dir.path(), None).await;
    feed(&engine, &mut pipe, &id, 10).await;
    engine.remove(&id);
    let writes = engine.conversation_writes();
    tokio::time::sleep(Duration::from_millis(2300)).await;
    assert_eq!(engine.conversation_writes(), writes);
    assert!(!record_path(dir.path(), &id).exists(), "the record stays deleted");
    assert!(!dir.path().join("child-transcripts").join(&id).exists());
    engine.shutdown();
}

/// Not a check: prints what 500 helper events cost on a conversation of about 20 MB. Run with `--ignored --nocapture`.
/// Measured 2026-10-29 in a debug build: 500 writes and 390 s when the record was written at every event; 0 writes inside
/// the window and about 2 s (the pages themselves) with the timer.
#[tokio::test]
#[ignore]
async fn measure_500_helper_events_on_a_20_mb_conversation() {
    let dir = support::tempdir();
    let fixture: Value = serde_json::from_str(include_str!("../../tests/fixtures/conversation-changes.json")).unwrap();
    let mut conversation = fixture["conversation"].clone();
    conversation["id"] = json!("big");
    conversation["area"] = json!("chat");
    conversation["projectId"] = Value::Null;
    let text = "Some earlier words of the conversation, long enough to matter. ".repeat(1600);
    let mut messages = vec![];
    for n in 0..100 {
        messages.push(json!({ "id": format!("u{n}"), "role": "user", "text": text, "at": 1792000000000u64 + n }));
        messages.push(json!({ "id": format!("a{n}"), "role": "assistant", "text": text, "at": 1792000000001u64 + n, "status": "done", "vendor": "claude" }));
    }
    conversation["messages"] = Value::Array(messages);
    let conversation: faber_engine::model::Conversation = serde_json::from_value(conversation).unwrap();
    faber_engine::store::Store::new(dir.path()).save_checked(&conversation).unwrap();
    let bytes = std::fs::metadata(record_path(dir.path(), "big")).unwrap().len();
    let (engine, mut pipe, id) = start(dir.path(), Some("big".into())).await;
    let (writes, started) = (engine.conversation_writes(), Instant::now());
    feed(&engine, &mut pipe, &id, 250).await;
    let events = started.elapsed();
    println!(
        "record {} MB, 500 events: {} writes, {} ms until all were kept",
        bytes / 1_000_000,
        engine.conversation_writes() - writes,
        events.as_millis()
    );
    engine.shutdown();
}

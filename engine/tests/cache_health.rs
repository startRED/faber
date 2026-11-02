//! A warm turn that read little of its prompt from cache leaves one line in the local cache-health log.
use crate::support;

use faber_engine::cli::Cli;
use faber_engine::model::{Area, Role, Status, Vendor};
use faber_engine::router::Choice;
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use support::Entry;
use tokio::sync::mpsc::UnboundedReceiver;

fn engine(dir: &crate::support::DataFolder, entries: Vec<Entry>) -> (Engine, UnboundedReceiver<AppEvent>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let opts = EngineOptions {
        data_dir: dir.path().into(),
        #[cfg(windows)]
        chat_dir: dir.chat_path().into(),
        clis: BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]),
        spawn: support::fake(entries).spawn,
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    (Engine::new(opts, Arc::new(move |e| drop(tx.send(e)))).unwrap(), rx)
}

fn user(text: &str) -> Entry {
    Entry {
        input: true,
        msg: json!({ "type": "user", "message": { "role": "user", "content": text }, "parent_tool_use_id": null, "session_id": "" }),
    }
}

fn init() -> Entry {
    Entry { input: false, msg: json!({ "type": "system", "subtype": "init", "session_id": "warm", "model": "claude-haiku-5-5" }) }
}

/// A finished turn whose prompt held `read` tokens read from cache out of `read + 4100`.
fn result(read: u64) -> Entry {
    let usage = json!({ "input_tokens": 100, "cache_read_input_tokens": read, "cache_creation_input_tokens": 4000, "output_tokens": 10 });
    Entry {
        input: false,
        msg: json!({ "type": "result", "subtype": "success", "is_error": false, "total_cost_usd": 0.01, "usage": usage }),
    }
}

async fn turn(engine: &Engine, rx: &mut UnboundedReceiver<AppEvent>, id: Option<String>, text: &str) -> String {
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
    let request =
        SendRequest { conversation_id: id, area: Area::Chat, project_id: None, text: text.into(), images: vec![], files: vec![], choice };
    let id = engine.send(request).unwrap();
    loop {
        let e = tokio::time::timeout(crate::support::DEADLINE, rx.recv()).await.expect("no finished answer in time").expect("engine gone");
        if let AppEvent::Message { message, .. } = e
            && message.role == Role::Assistant
            && message.status != Some(Status::Streaming)
        {
            return id;
        }
    }
}

fn logged(dir: &Path) -> Vec<Value> {
    let text = std::fs::read_to_string(dir.join("cache-health.jsonl")).unwrap_or_default();
    text.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

#[tokio::test]
async fn a_low_share_warm_turn_is_logged_and_a_cold_or_healthy_one_is_not() {
    let dir = crate::support::DataFolder::new();
    // 300 of 4400 read (cold, never logged), 300 of 4400 (warm, logged), 9000 of 13100 is 69% (warm, logged), then 30000 of
    // 34100 is 88% (warm, healthy).
    let entries =
        vec![user("one"), init(), result(300), user("two"), result(300), user("three"), result(9000), user("four"), result(30_000)];
    let (engine, mut rx) = engine(&dir, entries);
    let id = turn(&engine, &mut rx, None, "one").await;
    assert!(logged(dir.path()).is_empty(), "the first turn of a fresh process has an empty cache");
    for text in ["two", "three", "four"] {
        turn(&engine, &mut rx, Some(id.clone()), text).await;
    }
    engine.shutdown();
    let lines = logged(dir.path());
    assert_eq!(lines.len(), 2, "{lines:?}");
    let first = &lines[0];
    assert_eq!(
        (first["conv"].as_str(), first["vendor"].as_str(), first["model"].as_str()),
        (Some(id.as_str()), Some("claude"), Some("claude-haiku-5-5"))
    );
    assert_eq!(
        (first["input"].as_u64(), first["read"].as_u64(), first["written"].as_u64(), first["resumed"].as_bool()),
        (Some(4400), Some(300), Some(4000), Some(false))
    );
    assert_eq!(first["effort"], "medium");
    assert!((first["share"].as_f64().unwrap() - 0.068).abs() < 1e-9);
    assert!(first["at"].as_u64().unwrap() > 0);
    assert_eq!(lines[1]["read"].as_u64(), Some(9000));
}

#[tokio::test]
async fn the_first_turn_after_a_resume_is_logged_as_resumed_when_its_share_is_low() {
    let dir = crate::support::DataFolder::new();
    let (first, mut rx) = engine(&dir, vec![user("one"), init(), result(300)]);
    let id = turn(&first, &mut rx, None, "one").await;
    first.shutdown();
    assert!(logged(dir.path()).is_empty());
    // A new process continues the saved session: its cache may have expired while nobody was typing.
    let (second, mut rx) = engine(&dir, vec![user("two"), init(), result(300), user("three"), result(30_000)]);
    turn(&second, &mut rx, Some(id), "two").await;
    let lines = logged(dir.path());
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["resumed"], true);
    turn(&second, &mut rx, lines[0]["conv"].as_str().map(String::from), "three").await;
    second.shutdown();
    assert_eq!(logged(dir.path()).len(), 1, "a healthy warm turn adds nothing");
}

/// Two turns on one process, the second warm and low (the case that is logged).
async fn low_warm_turn(dir: &crate::support::DataFolder) {
    let (engine, mut rx) = engine(dir, vec![user("one"), init(), result(300), user("two"), result(300)]);
    let id = turn(&engine, &mut rx, None, "one").await;
    turn(&engine, &mut rx, Some(id), "two").await;
    engine.shutdown();
}

#[tokio::test]
async fn a_log_that_cannot_be_written_never_breaks_the_turn() {
    let dir = crate::support::DataFolder::new();
    // A folder sits where the file would go, so every append fails.
    std::fs::create_dir(dir.path().join("cache-health.jsonl")).unwrap();
    low_warm_turn(&dir).await;
    assert!(dir.path().join("cache-health.jsonl").is_dir());
}

#[tokio::test]
async fn a_log_at_its_size_cap_is_not_appended_to() {
    let dir = crate::support::DataFolder::new();
    let file = dir.path().join("cache-health.jsonl");
    std::fs::write(&file, vec![b'x'; 1_000_000]).unwrap();
    low_warm_turn(&dir).await;
    assert_eq!(std::fs::metadata(&file).unwrap().len(), 1_000_000, "a capped file stays as it was");
    // Just under the cap, one more line still fits (and the cap is checked before the write, so it may pass it once).
    std::fs::write(&file, vec![b'x'; 999_999]).unwrap();
    low_warm_turn(&dir).await;
    assert!(std::fs::metadata(&file).unwrap().len() > 1_000_000);
}

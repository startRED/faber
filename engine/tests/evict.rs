//! Conversations nothing uses leave memory (R461, `engine/src/engine/evict.rs`): the engine kept every conversation it
//! ever opened until Faber closed, 707 MB after three hours of real use. The one the screen shows, and any the engine is
//! still working on, stay.
use crate::support;

use faber_engine::{Engine, EngineOptions};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

/// A finished Chat conversation `id`, its person's message written at `at`.
fn conversation(id: &str, at: u64) -> Value {
    let recorded: Value = serde_json::from_slice(
        &std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/conversation-changes.json")).unwrap(),
    )
    .unwrap();
    let mut c = recorded["conversation"].clone();
    c["id"] = json!(id);
    c["title"] = json!(format!("Conversation {id}"));
    c["area"] = json!("chat");
    c.as_object_mut().unwrap().remove("projectId");
    c["messages"][0]["at"] = json!(at);
    c["messages"][1]["at"] = json!(at + 1000);
    c
}

fn boot(dir: &Path, conversations: &[Value]) -> Engine {
    let folder = dir.join("data").join("conversations");
    std::fs::create_dir_all(&folder).unwrap();
    for c in conversations {
        std::fs::write(folder.join(format!("{}.json", c["id"].as_str().unwrap())), c.to_string()).unwrap();
    }
    let opts = EngineOptions {
        data_dir: dir.join("data"),
        clis: BTreeMap::new(),
        spawn: support::pipes().0,
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    Engine::new(opts, Arc::new(|_| {}))
}

const HOUR: u64 = 3600 * 1000;

#[tokio::test]
async fn r_next_opening_another_conversation_lets_the_idle_one_leave_memory() {
    let dir = support::tempdir();
    let engine = boot(dir.path(), &[conversation("a1", now() - HOUR), conversation("b2", now() - HOUR)]);
    engine.open_for_screen("a1").expect("a1 opens");
    assert_eq!(engine.in_memory(), vec!["a1"]);
    engine.open_for_screen("b2").expect("b2 opens");
    assert_eq!(engine.in_memory(), vec!["b2"], "a1 is idle and no longer shown, so it leaves memory");
    let again = engine.open_for_screen("a1").expect("a1 opens again from its file");
    assert_eq!(again.messages.len(), 2, "a conversation read again has every message it had");
    assert_eq!(engine.in_memory(), vec!["a1"]);
}

#[tokio::test]
async fn r_next_a_conversation_with_a_queue_or_a_recent_message_stays_in_memory() {
    let dir = support::tempdir();
    let mut queued = conversation("q1", now() - HOUR);
    queued["queued"] = json!([{ "id": "turn-1", "text": "next, please", "choice": { "vendor": "claude" }, "at": now() - HOUR }]);
    let engine = boot(dir.path(), &[queued, conversation("r2", now()), conversation("s3", now() - HOUR)]);
    engine.open_for_screen("q1").expect("q1 opens");
    engine.open_for_screen("r2").expect("r2 opens");
    engine.open_for_screen("s3").expect("s3 opens");
    assert_eq!(
        engine.in_memory(),
        vec!["q1", "r2", "s3"],
        "a queued turn still has to be sent, and the person wrote in r2 under 15 minutes ago (a mission waits for them)"
    );
}

#[tokio::test]
async fn r_next_a_file_changed_by_something_else_is_not_overwritten_when_its_conversation_goes() {
    let dir = support::tempdir();
    let engine = boot(dir.path(), &[conversation("a1", now() - HOUR), conversation("b2", now() - HOUR)]);
    engine.open_for_screen("a1").expect("a1 opens");
    let file = dir.path().join("data").join("conversations").join("a1.json");
    let mut outside = conversation("a1", now() - HOUR);
    outside["title"] = json!("Written by another program meanwhile");
    std::fs::write(&file, outside.to_string()).unwrap();
    engine.open_for_screen("b2").expect("b2 opens");
    assert_eq!(engine.in_memory(), vec!["a1", "b2"], "a1 stays in memory rather than overwrite a file it did not write");
    let on_disk: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(on_disk["title"], "Written by another program meanwhile");
}

//! A huge conversation opens with its newest messages only (task newest-messages-first): the screen draws 40 at a time, and
//! sending the whole record moved about 100 MB through the bridge to draw them. Older ones come by request.
use crate::support;

use faber_engine::model::{Conversation, Message, Role, Status};
use faber_engine::store::Store;
use faber_engine::{Engine, EngineOptions};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

const MESSAGES: usize = 2_000;
/// 2,000 messages of 50 KB: the shape of the owner's 102 MB conversation.
const TEXT: usize = 50_000;
const WINDOW: usize = 40;

fn boot(dir: &Path) -> Engine {
    let opts = EngineOptions {
        data_dir: dir.join("data"),
        #[cfg(windows)]
        chat_dir: dir.join("work"),
        clis: BTreeMap::new(),
        spawn: support::pipes().0,
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    Engine::new(opts, Arc::new(|_| {})).unwrap()
}

/// The fastest of three runs of `open`, which also turns the result into what the shell sends.
fn fastest(mut open: impl FnMut() -> Vec<u8>) -> (Duration, usize) {
    (0..3)
        .map(|_| {
            let start = Instant::now();
            let bytes = open().len();
            (start.elapsed(), bytes)
        })
        .min()
        .unwrap()
}

#[tokio::test]
async fn a_huge_conversation_opens_with_its_newest_messages_and_walks_back_without_gaps() {
    let dir = support::tempdir();
    let mut c: Conversation = serde_json::from_value(json!({
        "id": "huge", "title": "Huge", "area": "chat", "createdAt": 1, "updatedAt": 1, "messages": []
    }))
    .unwrap();
    for i in 0..MESSAGES {
        let role = if i % 2 == 0 { Role::User } else { Role::Assistant };
        let mut m = Message::new(format!("m{i}"), role, format!("{i} {}", "x".repeat(TEXT)), i as u64 + 1);
        if role == Role::Assistant {
            m.status = Some(Status::Done);
        }
        c.messages.push(m);
    }
    let ids: Vec<String> = c.messages.iter().map(|m| m.id.clone()).collect();
    Store::new(&dir.path().join("data")).save(&c);
    drop(c);
    let engine = boot(dir.path());

    let window = engine.open_for_screen_window("huge", WINDOW).unwrap();
    assert_eq!(window.earlier, MESSAGES - WINDOW);
    assert_eq!(window.messages.iter().map(|m| &m.id).collect::<Vec<_>>(), ids[MESSAGES - WINDOW..].iter().collect::<Vec<_>>());
    let whole = engine.open_for_screen("huge").unwrap();
    assert_eq!((whole.messages.len(), whole.earlier), (MESSAGES, 0), "the whole opening stays whole");
    assert_eq!(serde_json::to_value(&whole).unwrap().get("earlier"), None, "absent when nothing came before");
    drop(whole);

    let (all_time, all_bytes) = fastest(|| serde_json::to_vec(&engine.open_for_screen("huge").unwrap()).unwrap());
    let (window_time, window_bytes) = fastest(|| serde_json::to_vec(&engine.open_for_screen_window("huge", WINDOW).unwrap()).unwrap());
    println!("big_conversation: whole {all_bytes} bytes in {all_time:?}, newest {WINDOW} {window_bytes} bytes in {window_time:?}");
    assert!(window_time * 10 < all_time, "the newest {WINDOW} took {window_time:?}, the whole {all_time:?}");

    // Walking back from the first shown message reaches the first one, each message once, in order.
    let mut seen: Vec<String> = window.messages.iter().map(|m| m.id.clone()).collect();
    while let Some(older) = engine.messages_before("huge", &seen[0], WINDOW).filter(|o| !o.is_empty()) {
        assert!(older.len() <= WINDOW);
        seen.splice(0..0, older.into_iter().map(|m| m.id));
    }
    assert_eq!(seen, ids);
    assert_eq!(engine.messages_before("huge", "gone", WINDOW), None, "a message that is gone has nothing before it");
    engine.shutdown();
}

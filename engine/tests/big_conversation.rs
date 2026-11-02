//! A stand-in for the owner's biggest conversation (102 MB) measures what opening and saving it costs, so the storage tasks
//! can prove the time or size they promise. Loose bounds only: it must come back equal; the numbers are printed. It also opens
//! with its newest messages only (newest-messages-first): the screen draws 40 at a time, and older ones come by request.
use crate::support;
use faber_engine::cli::Cli;
use faber_engine::model::{Conversation, Message, Role, Status, Vendor};
use faber_engine::store::Store;
use faber_engine::{Engine, EngineOptions};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

const MESSAGES: usize = 2_000;
/// 2,000 messages of 50 KB: the shape of the owner's 102 MB conversation.
const TEXT: usize = 50_000;
const WINDOW: usize = 40;

fn engine(dir: &support::DataFolder) -> Engine {
    Engine::new(
        EngineOptions {
            data_dir: dir.path().into(),
            #[cfg(windows)]
            chat_dir: dir.chat_path().into(),
            clis: BTreeMap::<Vendor, Cli>::new(),
            spawn: support::fake(vec![]).spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 0,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    )
    .unwrap()
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

/// Builds the case, then times a cold read, a save and an opening for the screen, and prints them with the file size.
async fn measure(megabytes: usize) {
    let dir = support::DataFolder::new();
    let written = support::big_conversation(dir.path(), megabytes * 1_000_000);
    let id = written.id.clone();
    let file = dir.path().join("conversations").join(format!("{id}.json"));
    let size = std::fs::metadata(&file).unwrap().len();
    let store = Store::new(dir.path());

    let start = Instant::now();
    let read = store.get(&id).expect("the big conversation reads back");
    let get = start.elapsed();
    assert!(read == written, "the {megabytes} MB conversation came back different from what was written");
    drop(written);

    let start = Instant::now();
    store.save_checked(&read).unwrap();
    let save = start.elapsed();
    drop(read);

    let engine = engine(&dir);
    let start = Instant::now();
    let shown = engine.open_for_screen(&id).expect("the big conversation opens");
    let open = start.elapsed();
    let tools: usize = shown.messages.iter().map(|m| m.tools.as_ref().map_or(0, Vec::len)).sum();
    eprintln!(
        "big conversation {megabytes} MB: file {:.1} MB, {} messages, {tools} tool records; get {} ms, save {} ms, open for screen {} ms",
        size as f64 / 1e6,
        shown.messages.len(),
        get.as_millis(),
        save.as_millis(),
        open.as_millis(),
    );
    assert!(size as usize >= megabytes * 900_000, "the case is {size} bytes, short of {megabytes} MB");
}

#[tokio::test]
async fn a_20_mb_conversation_reads_saves_and_opens() {
    measure(20).await;
}

#[tokio::test]
async fn a_100_mb_conversation_reads_saves_and_opens_equal_to_what_was_written() {
    measure(100).await;
}

#[tokio::test]
async fn a_huge_conversation_opens_with_its_newest_messages_and_walks_back_without_gaps() {
    let dir = support::DataFolder::new();
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
    Store::new(dir.path()).save(&c);
    drop(c);
    let engine = engine(&dir);

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

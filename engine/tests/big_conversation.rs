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
async fn a_300_message_conversation_measures_whole_and_newest_screen_delivery() {
    let dir = support::DataFolder::new();
    let mut c: Conversation = serde_json::from_value(json!({
        "id": "paged-300", "title": "Paged conversation", "area": "chat", "createdAt": 1, "updatedAt": 1, "messages": []
    }))
    .unwrap();
    for i in 0..300 {
        c.messages.push(Message::new(format!("m{i}"), if i % 2 == 0 { Role::User } else { Role::Assistant }, "x".repeat(TEXT), i + 1));
    }
    Store::new(dir.path()).save(&c);
    drop(c);
    let engine = engine(&dir);
    let start = Instant::now();
    let window = engine.open_for_screen_window("paged-300", WINDOW).unwrap();
    let cold_window = start.elapsed();
    assert_eq!((window.earlier, window.messages.len()), (260, WINDOW));
    let (whole_time, whole_bytes) = fastest(|| serde_json::to_vec(&engine.open_for_screen("paged-300").unwrap()).unwrap());
    let (window_time, window_bytes) = fastest(|| serde_json::to_vec(&engine.open_for_screen_window("paged-300", WINDOW).unwrap()).unwrap());
    let measurement = format!(
        "300-message delivery: whole {whole_bytes} bytes in {whole_time:?}; newest 40 {window_bytes} bytes in {window_time:?}; cold newest {cold_window:?}"
    );
    eprintln!("{measurement}");
    // Keep the measurement available when the runner captures successful test output.
    let report = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/faber-300-message-delivery.txt");
    std::fs::create_dir_all(report.parent().unwrap()).unwrap();
    std::fs::write(report, measurement).unwrap();
    assert!(window_bytes * 7 < whole_bytes, "the screen receives only its requested window");
    let mut seen = window.messages;
    while let Some(older) = engine.messages_before("paged-300", &seen[0].id, WINDOW).filter(|page| !page.is_empty()) {
        seen.splice(0..0, older);
    }
    assert_eq!(seen.len(), 300);
    assert_eq!(seen[4].id, "m4");
    engine.shutdown();
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

/// Models new calls with the same inputs as the old fixture, without migrating existing records.
#[test]
fn big_conversation_100mb_new_inputs_are_paged() {
    let dir = support::DataFolder::new();
    let mut conversation = support::big_conversation(dir.path(), 100_000_000);
    let file = dir.path().join("conversations").join(format!("{}.json", conversation.id));
    let before = std::fs::metadata(&file).unwrap().len();
    let store = Store::new(dir.path());
    let pages = faber_engine::transcript::TranscriptStore::new(dir.path().join("child-transcripts"));
    let start = Instant::now();
    let mut paged = 0;
    for tool in conversation.messages.iter_mut().flat_map(|m| m.tools.iter_mut().flatten()) {
        tool.change = faber_engine::change::of(&tool.name, &tool.input);
        tool.detail = pages.keep_input(&conversation.id, &tool.id, &tool.name, &mut tool.input);
        paged += usize::from(tool.detail.is_some());
    }
    let elapsed = start.elapsed();
    store.save_checked(&conversation).unwrap();
    let after = std::fs::metadata(&file).unwrap().len();
    let report = format!("before {before} bytes, after {after} bytes, {paged} calls paged in {} ms", elapsed.as_millis());
    eprintln!("{report}");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/faber-big-input-measurement.txt");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, report).unwrap();
    assert!(paged > 0 && after < before, "the new record must be smaller");
}

#[tokio::test]
async fn old_inputs_are_paged_after_open_and_remain_readable() {
    let dir = support::DataFolder::new();
    let conversation = support::big_conversation(dir.path(), 1_000_000);
    let mut unopened = conversation.clone();
    unopened.id = "not-opened".into();
    Store::new(dir.path()).save_checked(&unopened).unwrap();
    let unopened_file = dir.path().join("conversations/not-opened.json");
    let unopened_bytes = std::fs::read(&unopened_file).unwrap();
    let id = conversation.id.clone();
    let inputs: Vec<_> =
        conversation.messages.iter().flat_map(|m| m.tools.iter().flatten()).map(|tool| (tool.id.clone(), tool.input.clone())).collect();
    let file = dir.path().join("conversations").join(format!("{id}.json"));
    let before = std::fs::metadata(&file).unwrap().len();
    let app = engine(&dir);
    app.open_for_screen(&id).unwrap();
    assert_eq!(std::fs::metadata(&file).unwrap().len(), before, "opening does not write the record");
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if std::fs::metadata(&file).unwrap().len() < before {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the background save finishes");
    let after = std::fs::metadata(&file).unwrap().len();
    assert_eq!(std::fs::read(&unopened_file).unwrap(), unopened_bytes);
    eprintln!("old inputs: before {before} bytes, after {after} bytes, saved {} bytes", before - after);
    let saved = Store::new(dir.path()).get(&id).unwrap();
    for tool in saved.messages.iter().flat_map(|m| m.tools.iter().flatten()) {
        let original = inputs.iter().find(|(call, _)| call == &tool.id).unwrap();
        if original.1.to_string().len() > 4096 {
            assert!(tool.detail.is_some(), "large input {} was not paged", tool.id);
        }
    }
    for (call, input) in &inputs {
        assert_eq!(&app.tool_detail(&id, call).unwrap(), input, "input {call}");
    }
    app.shutdown();
    let reopened = engine(&dir);
    for (call, input) in inputs {
        assert_eq!(reopened.tool_detail(&id, &call).unwrap(), input, "reopened input {call}");
    }
    reopened.shutdown();
}

#[test]
fn failed_record_replacement_after_pages_keeps_old_inputs() {
    use faber_engine::store::{Disk, RealDisk};
    struct FailRecord;
    impl Disk for FailRecord {
        fn write(&self, path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
            RealDisk.write(path, bytes)
        }
        fn rename(&self, from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
            if to.parent().is_some_and(|p| p.ends_with("conversations")) {
                Err(std::io::ErrorKind::StorageFull.into())
            } else {
                RealDisk.rename(from, to)
            }
        }
    }
    let dir = support::DataFolder::new();
    let old = support::big_conversation(dir.path(), 1_000_000);
    let disk = Arc::new(FailRecord);
    let store = Store::with_disk(dir.path(), disk.clone());
    let pages = faber_engine::transcript::TranscriptStore::with_disk(dir.path().join("child-transcripts"), disk);
    let mut changed = store.get(&old.id).unwrap();
    let mut paged = 0;
    for tool in changed.messages.iter_mut().flat_map(|m| m.tools.iter_mut().flatten()) {
        tool.detail = pages.keep_input(&changed.id, &tool.id, &tool.name, &mut tool.input);
        paged += usize::from(tool.detail.is_some());
    }
    assert!(paged > 0);
    assert!(store.save_checked(&changed).is_err());
    assert_eq!(store.get(&old.id).unwrap(), old);
}

#[test]
fn concurrent_different_inputs_never_share_the_wrong_page() {
    use faber_engine::transcript::TranscriptStore;
    use sha2::{Digest, Sha256};
    for round in 0..40 {
        let dir = support::DataFolder::new();
        let pages = TranscriptStore::new(dir.path().join("child-transcripts"));
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let handles: Vec<_> = ["a", "b"]
            .into_iter()
            .map(|letter| {
                let pages = pages.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let original = json!(letter.repeat(16000));
                    let mut input = original.clone();
                    barrier.wait();
                    let detail = pages.keep_input("conversation", "same-call", "tool", &mut input);
                    (original, input, detail)
                })
            })
            .collect();
        let mut saved = 0;
        for handle in handles {
            let (original, input, detail) = handle.join().unwrap();
            if let Some(detail) = detail {
                saved += 1;
                let text = pages.read_detail("conversation", "same-call", detail.pages, detail.bytes).unwrap();
                assert_eq!(text, original.to_string(), "round {round}");
                assert_eq!(faber_engine::hex(&Sha256::digest(text.as_bytes())), detail.hash);
            } else {
                assert_eq!(input, original);
            }
        }
        assert_eq!(saved, 1, "exactly one input must be safely paged");
    }
}

#[tokio::test]
async fn deleting_during_old_input_migration_leaves_no_pages() {
    let dir = support::DataFolder::new();
    let conversation = support::big_conversation(dir.path(), 20_000_000);
    let id = conversation.id;
    let root = dir.path().join("child-transcripts").join(&id);
    let app = engine(&dir);
    app.open_for_screen(&id).unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        while !root.exists() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("migration started");
    app.remove(&id);
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(!root.exists(), "migration recreated the deleted conversation's pages");
    app.shutdown();
}

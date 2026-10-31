//! A stand-in for the owner's biggest conversation (102 MB) measures what opening and saving it costs, so the storage tasks
//! can prove the time or size they promise. Loose bounds only: it must come back equal; the numbers are printed.
use crate::support;
use faber_engine::cli::Cli;
use faber_engine::model::Vendor;
use faber_engine::store::Store;
use faber_engine::{Engine, EngineOptions};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

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

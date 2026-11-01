//! The sidebar's list reads only the conversation files that changed (R461): reading one decodes the whole conversation,
//! tool outputs included, to take a few fields, and the list was read twice a turn (202 MB and 420 ms for 32 real ones).
use crate::support;

use faber_engine::store::Store;
use serde_json::{Value, json};
use std::path::Path;

fn conversation(id: &str, title: &str) -> Value {
    let recorded: Value = serde_json::from_slice(
        &std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/conversation-changes.json")).unwrap(),
    )
    .unwrap();
    let mut c = recorded["conversation"].clone();
    c["id"] = json!(id);
    c["title"] = json!(title);
    c
}

fn titles(store: &Store) -> Vec<String> {
    store.list().into_iter().map(|s| s.title).collect()
}

#[test]
fn r_next_a_listed_file_that_did_not_change_is_not_read_again() {
    let dir = support::tempdir();
    let store = Store::new(dir.path());
    let file = dir.path().join("conversations").join("c1.json");
    let text = conversation("c1", "First title").to_string();
    std::fs::write(&file, &text).unwrap();
    assert_eq!(titles(&store), ["First title"]);

    // Same length and same time: the file was not written since, so the list keeps what it read.
    let when = std::fs::metadata(&file).unwrap().modified().unwrap();
    std::fs::write(&file, " ".repeat(text.len())).unwrap();
    std::fs::File::options().write(true).open(&file).unwrap().set_modified(when).unwrap();
    assert_eq!(titles(&store), ["First title"], "an unchanged file is not decoded again");

    // Any real change is seen: another length here.
    std::fs::write(&file, conversation("c1", "Second title").to_string()).unwrap();
    assert_eq!(titles(&store), ["Second title"]);
}

#[test]
fn r_next_a_save_and_a_removal_show_in_the_next_list() {
    let dir = support::tempdir();
    let store = Store::new(dir.path());
    let folder = dir.path().join("conversations");
    std::fs::write(folder.join("c1.json"), conversation("c1", "Kept").to_string()).unwrap();
    std::fs::write(folder.join("c2.json"), conversation("c2", "Removed").to_string()).unwrap();
    assert_eq!(titles(&store).len(), 2);

    let mut c = store.get("c1").expect("c1 reads");
    c.title = "Renamed".into();
    store.save(&c);
    store.remove("c2");
    assert_eq!(titles(&store), ["Renamed"]);
}

#[test]
fn a_conversation_file_is_written_compact_and_an_old_pretty_one_still_loads() {
    let dir = support::tempdir();
    let store = Store::new(dir.path());
    let file = dir.path().join("conversations").join("c1.json");
    // A file from before, written pretty, loads and is rewritten compact on its next save.
    std::fs::write(&file, serde_json::to_string_pretty(&conversation("c1", "Old")).unwrap()).unwrap();
    let mut c = store.get("c1").expect("a pretty file reads");
    assert_eq!(c.title, "Old");
    c.title = "Saved".into();
    store.save(&c);
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(!text.contains('\n') && !text.contains("\": "), "no line break or indent between fields: {}", &text[..200]);
    assert_eq!(store.get("c1").unwrap(), c, "loads equal");
}

/// The same 20 MB of tool records written both ways: the numbers of the commit that made conversation files compact.
#[test]
fn a_big_conversation_file_is_smaller_compact() {
    let dir = support::tempdir();
    let store = Store::new(dir.path());
    let mut c = conversation("big", "Big");
    let tool = json!({ "id": "t", "name": "Bash", "input": { "command": "x".repeat(100) }, "ok": true, "output": "y".repeat(2_000) });
    let message = json!({ "id": "m", "role": "assistant", "text": "z".repeat(10_000), "at": 1, "status": "done", "tools": vec![tool; 35] });
    c["messages"] = Value::Array(
        (0..230)
            .map(|i| {
                let mut m = message.clone();
                m["id"] = json!(format!("m{i}"));
                m
            })
            .collect(),
    );
    let c: faber_engine::model::Conversation = serde_json::from_value(c).unwrap();
    let file = dir.path().join("conversations").join("big.json");
    // Serialised and written the same way, so the numbers differ only by the format.
    let timed = |bytes: &dyn Fn() -> Vec<u8>| {
        let start = std::time::Instant::now();
        std::fs::write(&file, bytes()).unwrap();
        (start.elapsed(), std::fs::metadata(&file).unwrap().len())
    };
    let (pretty_time, pretty) = timed(&|| serde_json::to_vec_pretty(&c).unwrap());
    let (compact_time, compact) = timed(&|| serde_json::to_vec(&c).unwrap());
    store.save(&c);
    assert_eq!(std::fs::metadata(&file).unwrap().len(), compact, "the store writes the compact form");
    println!("store: pretty {pretty} bytes in {pretty_time:?}, compact {compact} bytes in {compact_time:?}");
    assert!(compact < pretty, "compact {compact} bytes, pretty {pretty}");
}

//! A conversation keeps its tabs in Faber's browser (R431): the page the person or the agent left comes back after the
//! pane closed, another conversation and a restart, and never leaves Faber in an export.
use crate::support;
use faber_engine::cli::Cli;
use faber_engine::export::Words;
use faber_engine::model::{Conversation, Vendor};
use faber_engine::store::Store;
use faber_engine::{Engine, EngineOptions};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

fn engine(dir: &Path) -> Engine {
    Engine::new(
        EngineOptions {
            data_dir: dir.into(),
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
}

fn saved(dir: &Path) -> Conversation {
    let mut conversation: Conversation = serde_json::from_str(include_str!("../../tests/fixtures/conversation-export.json")).unwrap();
    conversation.piece_values.clear();
    conversation.browser = None;
    Store::new(dir).save_checked(&conversation).unwrap();
    conversation
}

#[tokio::test]
async fn the_last_page_of_a_conversation_comes_back_after_a_restart_and_never_in_its_export() {
    let dir = support::tempdir();
    let id = saved(dir.path()).id;
    let first = engine(dir.path());
    assert_eq!(first.browser_tabs(&id), None, "a conversation that never had a page has no tabs");
    first.browser_seen(&id, None, "https://docs.example.org/start?q=secret-word", None);
    first.browser_seen(&id, None, "https://docs.example.org/start?q=secret-word", Some("Getting started"));
    // Anything the pane refuses never becomes a tab.
    first.browser_seen(&id, None, "file:///C:/Users/me/notes.txt", Some("Notes"));
    first.browser_seen("gone", None, "https://example.org/", None);
    drop(first);

    let again = engine(dir.path());
    let tabs = again.browser_tabs(&id).expect("kept in the conversation's file");
    let front = tabs.front_tab().expect("a tab in front");
    assert_eq!((front.url.as_str(), front.title.as_str()), ("https://docs.example.org/start?q=secret-word", "Getting started"));
    assert_eq!(tabs.tabs.len(), 1);
    assert_eq!(again.browser_tabs("gone"), None);

    let words = Words { you: "You".into(), auto: "Auto".into(), attachments: "Attachments".into(), files: "Files changed".into() };
    let (_, exported) = again.export_conversation(&id, &words).unwrap();
    assert!(!exported.contains("docs.example.org") && !exported.contains("Getting started"), "{exported}");

    again.remove(&id);
    assert_eq!(again.browser_tabs(&id), None, "the tabs leave with the conversation");
}

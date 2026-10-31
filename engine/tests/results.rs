//! What a conversation delivered and cost reaches the people who ask: the conversation list carries it, the engine answers it
//! for one conversation and for a project, and an empty conversation stays out (R-next, results ledger).
use crate::support;

use faber_engine::model::{Area, Conversation, Message, Role, Status, Vendor};
use faber_engine::store::Store;
use faber_engine::{Engine, EngineOptions};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

fn conversation(id: &str, project: Option<&str>, messages: Vec<Message>) -> Conversation {
    let mut c: Conversation = serde_json::from_value(json!({
        "id": id, "title": format!("title {id}"), "area": "project", "createdAt": 1, "updatedAt": 1, "messages": []
    }))
    .unwrap();
    c.project_id = project.map(Into::into);
    c.area = if project.is_some() { Area::Project } else { Area::Chat };
    c.messages = messages;
    c
}

fn edit_reply(at: u64, usd: f64) -> Message {
    let mut m = Message::new(format!("r{at}"), Role::Assistant, "Done.".into(), at);
    m.status = Some(Status::Done);
    m.vendor = Some(Vendor::Claude);
    m.took = Some(2000);
    m.cost_usd = Some(usd);
    m.tools =
        Some(vec![serde_json::from_value(json!({ "id": "e", "name": "Edit", "input": { "file_path": "a.rs" }, "ok": true })).unwrap()]);
    m
}

fn engine(dir: &support::DataFolder) -> Engine {
    let opts = EngineOptions {
        data_dir: dir.path().into(),
        #[cfg(windows)]
        chat_dir: dir.chat_path().into(),
        clis: BTreeMap::new(),
        spawn: Arc::new(|_, _, _| Err(std::io::Error::other("no CLI in this test"))),
        windows: false,
        claude_env_billed: false,
        max_warm: 1,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    Engine::new(opts, Arc::new(|_| {})).unwrap()
}

#[tokio::test]
async fn the_list_and_the_commands_carry_what_each_conversation_delivered() {
    let dir = support::DataFolder::new();
    let store = Store::new(dir.path());
    let user = Message::new("u1".into(), Role::User, "Fix it".into(), 1);
    store.save(&conversation("big", Some("p"), vec![user.clone(), edit_reply(2, 3.0)]));
    store.save(&conversation("small", Some("p"), vec![user.clone(), edit_reply(2, 0.5)]));
    store.save(&conversation("empty", Some("p"), vec![]));
    store.save(&conversation("elsewhere", Some("q"), vec![user, edit_reply(2, 9.0)]));

    let listed = store.list();
    let big = listed.iter().find(|s| s.id == "big").unwrap();
    assert_eq!(big.results.as_ref().map(|r| r.delivered.files), Some(1));
    assert!(listed.iter().find(|s| s.id == "empty").unwrap().results.is_none(), "nothing yet, nothing to show");

    let engine = engine(&dir);
    let one = engine.results("small").unwrap();
    assert!((one.cost.usd - 0.5).abs() < 1e-9);
    let project = engine.project_results("p");
    assert_eq!(project.conversations.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["big", "small"], "most spent first");
    assert!((project.total.cost.usd - 3.5).abs() < 1e-9, "another project's spend is not in it");
    assert_eq!(project.total.delivered.edits, 2);
    engine.shutdown();
}

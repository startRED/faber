//! The channel over Faber's real command table, with an engine that has no CLI.

use faber_channel::guard::Guard;
use faber_channel::server::connection;
use faber_channel::{Client, Hello};
use faber_engine::engine::EngineOptions;
use faber_engine::proc::Spawner;
use faber_engine::{Engine, model::Vendor};
use faber_host::{Shell, Wiring};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

struct NoWindow;
impl Shell for NoWindow {}

async fn client(host: &Arc<faber_host::Host>, depth: u32) -> Client {
    let (ours, theirs) = tokio::io::duplex(1 << 16);
    tokio::spawn(connection(theirs, Arc::new(host.clone()), Guard::watching(host.subscribe())));
    Client::over(ours, Hello { client: "test".into(), agent_depth: depth }).await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn faber_s_own_table_answers_with_the_screen_s_names_and_keeps_its_refusals() {
    let data = tempfile::tempdir().unwrap();
    let wiring = Wiring::new(Arc::new(NoWindow));
    let spawn: Spawner = Arc::new(|_, _, _| Err(std::io::Error::other("no CLI in this test")));
    let opts = EngineOptions {
        data_dir: data.path().into(),
        clis: BTreeMap::<Vendor, _>::new(),
        spawn,
        windows: cfg!(windows),
        claude_env_billed: false,
        max_warm: 1,
        idle: Duration::from_secs(60),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    let engine = Engine::new(opts, wiring.emitter()).unwrap();
    let host = Arc::new(wiring.host(engine));

    let person = client(&host, 0).await;
    let answer = person.offer["commands"].as_array().unwrap().iter().find(|c| c["name"] == "answer").unwrap().clone();
    assert_eq!(answer["args"][1], json!(["turnId", "String"]), "arguments are named as the screen sends them");
    assert_eq!(person.call("no_such_command", json!({})).await.unwrap(), Err("error.host.unknownCommand".into()));
    assert_eq!(person.call("open", json!(1)).await.unwrap(), Err("error.host.badArgs".into()));

    let deep = client(&host, faber_channel::server::MAX_AGENT_DEPTH).await;
    let started = deep.call("new_group_chat", json!({ "groupId": "g" })).await.unwrap();
    assert_eq!(started, Err("error.channel.agentDepth".into()), "a turn the table marks as starting one is held");
}

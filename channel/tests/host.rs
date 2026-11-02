//! The channel over Faber's real command table, with an engine that has no CLI.

use faber_channel::guard::Guard;
use faber_channel::server::connection_at;
use faber_channel::{Client, Hello};
use faber_engine::engine::EngineOptions;
use faber_engine::proc::Spawner;
use faber_engine::{Engine, model::Vendor};
use faber_host::{Shell, Wiring};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

fn test_secret() -> Arc<faber_channel::secret::Secret> {
    Arc::new(faber_channel::secret::Secret::from_hex(&"11".repeat(32)).unwrap())
}

struct NoWindow;
impl Shell for NoWindow {}

async fn client(host: &Arc<faber_host::Host>, depth: u32) -> Client {
    let (ours, theirs) = tokio::io::duplex(1 << 16);
    tokio::spawn(connection_at(theirs, Arc::new(host.clone()), Guard::watching(host.subscribe()), test_secret(), 0));
    Client::over(ours, Hello { client: "test".into(), agent_depth: depth, secret: test_secret().hex(), ..Default::default() })
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn refusal_detail_and_faber_s_own_table_keep_their_refusals() {
    let data = tempfile::tempdir().unwrap();
    let data_dir = data.path().join("data");
    let wiring = Wiring::new(Arc::new(NoWindow));
    let spawn: Spawner = Arc::new(|_, _, _| Err(std::io::Error::other("no CLI in this test")));
    let opts = EngineOptions {
        data_dir: data_dir.clone(),
        #[cfg(windows)]
        chat_dir: data.path().join("work"),
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
    let host = Arc::new(wiring.host(engine, data_dir, true, tokio::runtime::Handle::current()));

    let person = client(&host, 0).await;
    let refusal = person.call_detailed("save_settings", json!({"patch": {"appearance": {"base": "nowhere"}}})).await.unwrap().unwrap_err();
    assert_eq!(refusal.key, "error.look.fileInvalid");
    assert_eq!(refusal.detail.as_deref(), Some("base"));
    let answer = person.offer["commands"].as_array().unwrap().iter().find(|c| c["name"] == "answer").unwrap().clone();
    assert_eq!(answer["args"][1], json!(["turnId", "String"]), "arguments are named as the screen sends them");
    assert!(person.offer["commands"].as_array().unwrap().iter().all(|c| c["name"] != "state" && c["name"] != "screen_ready"));
    assert_eq!(person.call("no_such_command", json!({})).await.unwrap(), Err("error.host.unknownCommand".into()));
    assert_eq!(person.call("answer", json!({})).await.unwrap(), Err("error.reach.person".into()));
    assert_eq!(person.call("state", json!({})).await.unwrap(), Err("error.reach.screen".into()));
    let group = person.call("create_chat_group", json!({"name":"test"})).await.unwrap().unwrap();
    let conversation = person.call("new_group_chat", json!({"groupId":group["id"]})).await.unwrap().unwrap();
    let agent = client(&host, 1).await;
    assert_eq!(agent.call("interrupt", json!({"id": conversation["id"]})).await.unwrap(), Err("error.reach.ask".into()));
    assert!(person.call("conversations", json!({})).await.unwrap().unwrap().is_array());

    let deep = client(&host, faber_channel::server::MAX_AGENT_DEPTH).await;
    let started = deep.call("new_group_chat", json!({ "groupId": "g" })).await.unwrap();
    assert_eq!(started, Err("error.channel.agentDepth".into()), "a turn the table marks as starting one is held");

    let (ours, theirs) = tokio::io::duplex(1 << 16);
    tokio::spawn(connection_at(theirs, Arc::new(host.clone()), Guard::watching(host.subscribe()), test_secret(), 0));
    let named = Client::over(
        ours,
        Hello { client: "test".into(), secret: test_secret().hex(), token: Some(test_secret().token("c1")), ..Default::default() },
    )
    .await
    .unwrap();
    let args = json!({"patch": {"language": "en"}});
    let failure = named.call_detailed("save_settings", args.clone()).await.unwrap().unwrap_err();
    assert_eq!(failure.key, "error.reach.ask");
    let id = failure.data.unwrap()["request"].as_str().unwrap().to_string();
    assert_eq!(named.call("asks", json!({})).await.unwrap().unwrap()[0]["state"], "pending");
    assert_eq!(host.dispatch_as(&faber_host::Caller::Screen, "answer_ask", json!({"id": id, "allow": true})).await.unwrap(), true);
    assert_eq!(named.call("asks", json!({})).await.unwrap().unwrap()[0]["state"], "allowed");
    assert!(named.call("save_settings", args.clone()).await.unwrap().is_ok());
    assert_eq!(named.call("save_settings", args).await.unwrap(), Err("error.reach.ask".into()));
}

#[tokio::test(flavor = "multi_thread")]
async fn engine_token_is_accepted_by_the_running_host_channel() {
    let data = tempfile::tempdir().unwrap();
    let data_dir = data.path().join("data");
    let wiring = Wiring::new(Arc::new(NoWindow));
    let spawn: Spawner = Arc::new(|_, _, _| Err(std::io::Error::other("no CLI")));
    let engine = Engine::new(
        EngineOptions {
            data_dir: data_dir.clone(),
            #[cfg(windows)]
            chat_dir: data.path().join("work"),
            clis: BTreeMap::new(),
            spawn,
            windows: cfg!(windows),
            claude_env_billed: false,
            max_warm: 1,
            idle: Duration::from_secs(60),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        wiring.emitter(),
    )
    .unwrap();
    let host = Arc::new(wiring.host(engine, data_dir.clone(), true, tokio::runtime::Handle::current()));
    let server_data = data_dir.clone();
    let server = tokio::spawn(async move { faber_channel::host::serve(&server_data, host).await });
    let client = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(token) = faber_engine::proc::conversation_token("conversation-1") {
                let hello = Hello { client: "test".into(), agent_depth: 1, token: Some(token), ..Default::default() };
                if let Ok(client) = Client::connect(&data_dir, hello).await {
                    break client;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(client.offer["caller"], json!({"kind":"agent", "conversation":"conversation-1"}));
    server.abort();
}

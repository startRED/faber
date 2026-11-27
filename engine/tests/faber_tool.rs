//! The `faber` tool (R585): a conversation's agent finds and runs Faber's own commands through the table the front end gives
//! the engine (`Engine::set_commands`). The table here is a stand-in; the host's own is tested in `host/src/tests.rs`.
use crate::support;
use faber_engine::cli::Cli;
use faber_engine::commands::{Commands, Scope};
use faber_engine::model::{Area, Permissions, Settings, Vendor};
use faber_engine::{Engine, EngineOptions};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Every find and run it was asked for: the words or the conversation, command and arguments, each with its scope.
#[derive(Default)]
struct Table {
    finds: Mutex<Vec<(String, Scope)>>,
    runs: Mutex<Vec<(String, String, Value, Scope)>>,
}

impl Commands for Table {
    fn find(&self, words: &str, scope: Scope) -> Vec<Value> {
        self.finds.lock().unwrap().push((words.into(), scope));
        if words.contains("rename") { vec![json!({"name": "rename", "kind": "write"})] } else { vec![] }
    }

    fn run(
        &self,
        conversation: String,
        command: String,
        args: Value,
        scope: Scope,
    ) -> Pin<Box<dyn Future<Output = Result<Value, String>> + Send>> {
        self.runs.lock().unwrap().push((conversation, command.clone(), args, scope));
        Box::pin(async move { if command == "rename" { Ok(json!(true)) } else { Err("\"error.reach.person\"".into()) } })
    }
}

/// A Chat conversation on a hand-driven Codex, with its thread started: the engine, its id, the process and `thread/start`.
async fn chat(table: Option<Arc<Table>>, mode: Permissions) -> (Engine, String, support::Pipe, Value, support::DataFolder) {
    let dir = support::DataFolder::new();
    let (spawn, made) = support::pipes();
    let app = Engine::new(
        EngineOptions {
            data_dir: dir.path().into(),
            #[cfg(windows)]
            chat_dir: dir.chat_path().into(),
            clis: BTreeMap::from([(Vendor::Codex, Cli { command: "codex".into(), prefix: vec![] })]),
            spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 3,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    )
    .unwrap();
    if let Some(table) = table {
        app.set_commands(table);
    }
    app.save_settings(Settings { permissions: Some(mode), ..Settings::default() }).unwrap();
    let id = app
        .send(faber_engine::SendRequest {
            conversation_id: None,
            area: Area::Chat,
            project_id: None,
            text: "Rename this conversation".into(),
            images: vec![],
            files: vec![],
            choice: faber_engine::router::Choice { vendor: "codex".into(), model: None, effort: None },
        })
        .unwrap();
    let mut pipe = support::next_pipe(&made).await;
    let init = wire(&pipe, |m| m["method"] == "initialize").await;
    pipe.say(json!({"id": init["id"], "result": {}})).await;
    let start = wire(&pipe, |m| m["method"] == "thread/start").await;
    pipe.say(json!({"id": start["id"], "result": {"thread": {"id": "main"}}})).await;
    let turn = wire(&pipe, |m| m["method"] == "turn/start").await;
    pipe.say(json!({"id": turn["id"], "result": {"turn": {"id": "turn-main"}}})).await;
    (app, id, pipe, start, dir)
}

async fn wire(pipe: &support::Pipe, ready: impl Fn(&Value) -> bool) -> Value {
    tokio::time::timeout(support::DEADLINE, async {
        loop {
            if let Some(m) = pipe.lines.lock().unwrap().iter().find(|m| ready(m)).cloned() {
                return m;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("missing line on the fake wire: {:?}", pipe.lines.lock().unwrap()))
}

async fn call(pipe: &mut support::Pipe, id: u64, arguments: Value) -> (String, bool) {
    pipe.say(json!({"id": id, "method": "item/tool/call", "params": {"threadId": "main", "turnId": "turn-main",
        "callId": format!("call-{id}"), "tool": "faber", "arguments": arguments}}))
        .await;
    let result = wire(pipe, |m| m["id"] == id && m.get("result").is_some()).await["result"].clone();
    (result["contentItems"][0]["text"].as_str().unwrap_or_default().to_string(), result["success"] == true)
}

fn offers_faber(start: &Value) -> bool {
    start["params"]["dynamicTools"].as_array().into_iter().flatten().any(|tool| tool["name"] == "faber")
}

#[tokio::test]
async fn a_chat_offers_the_tool_only_where_the_engine_holds_the_table() {
    let (app, _, mut pipe, start, _dir) = chat(None, Permissions::Ask).await;
    assert!(!offers_faber(&start), "no table, no tool: {start}");
    let (said, ok) = call(&mut pipe, 700, json!({"action": "find", "words": "rename"})).await;
    assert!(!ok && said.contains("not available"), "{said}");
    app.shutdown();
    let (app, _, _pipe, start, _dir) = chat(Some(Arc::default()), Permissions::Ask).await;
    assert!(offers_faber(&start), "{start}");
    app.shutdown();
}

#[tokio::test]
async fn the_agent_finds_and_runs_a_command_as_its_own_conversation() {
    let table = Arc::new(Table::default());
    let (app, id, mut pipe, _, _dir) = chat(Some(table.clone()), Permissions::Ask).await;
    let (said, ok) = call(&mut pipe, 701, json!({"action": "find", "words": "rename it"})).await;
    assert!(ok && said.contains("\"rename\""), "{said}");
    let (said, ok) = call(&mut pipe, 702, json!({"action": "find", "words": "nothing"})).await;
    assert!(ok && said.contains("No Faber command"), "{said}");
    let (said, ok) = call(&mut pipe, 703, json!({"action": "run", "command": "rename", "args": {"id": id, "title": "New"}})).await;
    assert!(ok && said == "true", "{said}");
    let (said, ok) = call(&mut pipe, 704, json!({"action": "run", "command": "setup_sign_in", "args": {"vendor": "claude"}})).await;
    assert!(!ok && said.contains("error.reach.person"), "{said}");
    let (said, ok) = call(&mut pipe, 705, json!({"action": "run", "command": "rename", "args": "c1"})).await;
    assert!(!ok && said.contains("args must be an object"), "{said}");
    let runs = table.runs.lock().unwrap().clone();
    assert_eq!(runs.len(), 2, "a call that cannot be read never reaches the table");
    let chat = Scope { read_only: false, project: false };
    assert_eq!(runs[0], (id.clone(), "rename".into(), json!({"id": id, "title": "New"}), chat));
    assert_eq!(table.finds.lock().unwrap()[0], ("rename it".to_string(), chat), "a Chat never reaches a project's commands");
    app.shutdown();
}

#[tokio::test]
async fn plan_runs_only_reads() {
    let table = Arc::new(Table::default());
    let (app, id, mut pipe, _, _dir) = chat(Some(table.clone()), Permissions::Plan).await;
    call(&mut pipe, 710, json!({"action": "run", "command": "rename", "args": {"id": id, "title": "New"}})).await;
    assert!(table.runs.lock().unwrap()[0].3.read_only, "Plan changes nothing: the host is told only reads may run");
    app.shutdown();
}

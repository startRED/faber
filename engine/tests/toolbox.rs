//! The agent's read-only toolbox (`quota`, `catalog`): offered to a project's own agent in both CLIs and never in Chat, and
//! answered from what Faber holds, with no request, process or download.
use crate::support::{self, Pipe, next_pipe, pipes};
use faber_engine::cli::Cli;
use faber_engine::events::EngineEvent;
use faber_engine::model::{Area, Settings, Vendor};
use faber_engine::router::Choice;
use faber_engine::{Engine, EngineOptions, SendRequest};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

fn engine(dir: &std::path::Path, vendor: Vendor, spawn: faber_engine::proc::Spawner) -> Engine {
    let app = Engine::new(
        EngineOptions {
            data_dir: dir.join("data"),
            clis: BTreeMap::from([(vendor, Cli { command: vendor.name().into(), prefix: vec![] })]),
            spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 3,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    );
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();
    let signals = faber_engine::signals::Signals { billing: Some(faber_engine::budget::Billing::Subscription), ..Default::default() };
    let windows = [("five_hour", 0.31, 2.0 * 3600.0), ("seven_day", 0.30, 5.0 * 86_400.0)];
    let mut events: Vec<_> = windows
        .iter()
        .map(|(window, used, ahead)| EngineEvent::Quota { window: window.to_string(), used: *used, resets_at: now + ahead })
        .collect();
    events.push(EngineEvent::Signals(signals));
    app.note_account_events(vendor, events);
    app
}

async fn wire(pipe: &Pipe, ready: impl Fn(&Value) -> bool) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
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

fn request(vendor: Vendor, area: Area, project: Option<String>) -> SendRequest {
    SendRequest {
        conversation_id: None,
        area,
        project_id: project,
        text: "Plan the week's work".into(),
        images: vec![],
        files: vec![],
        choice: Choice { vendor: vendor.name().into(), model: None, effort: None },
    }
}

/// What the agent reads back from Faber: the text of the answer and whether it is an error.
fn said(result: &Value) -> (String, bool) {
    (result["content"][0]["text"].as_str().unwrap_or_default().to_string(), result["isError"] == true)
}

// ---- Claude Code: an MCP server the host answers over stdio ----

async fn claude_call(pipe: &mut Pipe, request: &str, tool: &str, arguments: Value) -> (String, bool) {
    let call = json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": { "name": tool, "arguments": arguments } });
    pipe.say(json!({ "type": "control_request", "request_id": request,
        "request": { "subtype": "mcp_message", "server_name": "faber", "message": call } }))
        .await;
    let answer = wire(pipe, |m| m["response"]["request_id"] == request).await;
    said(&answer["response"]["response"]["mcp_response"]["result"])
}

async fn claude_list(pipe: &mut Pipe, request: &str) -> Value {
    pipe.say(json!({ "type": "control_request", "request_id": request,
        "request": { "subtype": "mcp_message", "server_name": "faber", "message": { "jsonrpc": "2.0", "id": 1, "method": "tools/list" } } }))
        .await;
    wire(pipe, |m| m["response"]["request_id"] == request).await["response"]["response"]["mcp_response"].clone()
}

fn names(list: &Value) -> Vec<String> {
    list["result"]["tools"].as_array().map(|t| t.iter().map(|t| t["name"].as_str().unwrap().to_string()).collect()).unwrap_or_default()
}

async fn claude_agent(area: Area) -> (Engine, Pipe, tempfile::TempDir, tempfile::TempDir) {
    let (dir, folder) = (support::canonical_tempdir().unwrap(), support::work_folder());
    let (spawn, made) = pipes();
    let app = engine(dir.path(), Vendor::Claude, spawn);
    let project = (area == Area::Project).then(|| app.add_project(&folder.path().to_string_lossy()).unwrap().id);
    app.send(request(Vendor::Claude, area, project)).unwrap();
    let mut pipe = next_pipe(&made).await;
    wire(&pipe, |m| m["type"] == "user").await;
    pipe.say(json!({ "type": "system", "subtype": "init", "session_id": "s1", "model": "claude-haiku-5-5" })).await;
    (app, pipe, dir, folder)
}

#[tokio::test]
async fn claude_code_reads_the_limits_and_searches_the_catalog_in_any_permission_mode() {
    use faber_engine::model::Permissions as P;
    for mode in [P::Ask, P::FullAccess, P::Plan] {
        let (dir, folder) = (support::canonical_tempdir().unwrap(), support::work_folder());
        let (spawn, made) = pipes();
        let app = engine(dir.path(), Vendor::Claude, spawn);
        app.save_settings(Settings { permissions: Some(mode), ..Settings::default() }).unwrap();
        let project = app.add_project(&folder.path().to_string_lossy()).unwrap().id;
        let id = app.send(request(Vendor::Claude, Area::Project, Some(project))).unwrap();
        let mut pipe = next_pipe(&made).await;
        wire(&pipe, |m| m["type"] == "user").await;
        pipe.say(json!({ "type": "system", "subtype": "init", "session_id": "s1", "model": "claude-haiku-5-5" })).await;

        let tools = names(&claude_list(&mut pipe, "list").await);
        assert!(tools.contains(&"quota".to_string()) && tools.contains(&"catalog".to_string()), "{mode:?}: {tools:?}");

        // The step is a row of the reply, named Faber's way, and the tool's own permission asks nothing.
        pipe.say(json!({"type":"assistant","parent_tool_use_id":null,"message":{"content":[
            {"type":"tool_use","id":"toolu_q","name":"mcp__faber__quota","input":{}},
            {"type":"tool_use","id":"toolu_c","name":"mcp__faber__catalog","input":{"query":"bug"}}]}}))
            .await;
        pipe.say(json!({"type":"control_request","request_id":"perm","request":{"subtype":"can_use_tool","tool_name":"mcp__faber__quota",
            "input":{},"tool_use_id":"toolu_q"}}))
            .await;
        let allowed = wire(&pipe, |m| m["response"]["request_id"] == "perm").await;
        assert_eq!(allowed["response"]["response"]["behavior"], "allow", "{mode:?}");

        let (quota, error) = claude_call(&mut pipe, "quota", "quota", json!({})).await;
        assert!(!error, "{quota}");
        assert!(quota.contains("Claude Code (subscription"), "{quota}");
        let line = |name: &str| quota.lines().find(|l| l.trim_start().starts_with(name)).unwrap_or_default().to_string();
        let (week, hours) = (line("weekly"), line("5-hour"));
        assert!(week.contains("30% used") && (week.contains("renews in 4 d 23 h") || week.contains("renews in 5 d 0 h")), "{quota}");
        assert!(hours.contains("31% used") && (hours.contains("renews in 1 h 59 min") || hours.contains("renews in 2 h 0 min")), "{quota}");
        assert!(!quota.contains('@') && !quota.contains("token"), "no account detail: {quota}");

        let (found, error) = claude_call(&mut pipe, "catalog", "catalog", json!({ "query": "debugging a bug" })).await;
        assert!(!error, "{found}");
        let first = found.lines().next().unwrap();
        assert!(
            first.starts_with("systematic-debugging — Systematic debugging (skill): Finds the cause of a bug")
                && first.contains("Licence: MIT.")
                && first.contains("Source: github.com/obra/superpowers.")
                && first.ends_with("not installed."),
            "{first}"
        );
        assert!(found.lines().count() <= 21, "at most 20 results");

        let reply = app.open(&id).unwrap().messages.last().cloned().unwrap();
        let rows: Vec<_> = reply.tools.unwrap().iter().map(|t| t.name.clone()).collect();
        assert!(rows.contains(&"Quota".to_string()) && rows.contains(&"Catalog".to_string()), "{rows:?}");
        app.shutdown();
    }
}

#[tokio::test]
async fn claude_code_in_chat_is_not_offered_the_limits_or_the_catalog() {
    let (app, mut pipe, _dir, _folder) = claude_agent(Area::Chat).await;
    let list = claude_list(&mut pipe, "list").await;
    assert!(names(&list).is_empty(), "{list}");
    assert_eq!(list["error"]["code"], -32601, "{list}");
    app.shutdown();
}

// ---- Codex: dynamic tools registered with the thread ----

async fn codex_boot(pipe: &mut Pipe) -> Value {
    let init = wire(pipe, |m| m["method"] == "initialize").await;
    pipe.say(json!({"id":init["id"],"result":{}})).await;
    let start = wire(pipe, |m| m["method"] == "thread/start").await;
    pipe.say(json!({"id":start["id"],"result":{"thread":{"id":"main"}}})).await;
    let turn = wire(pipe, |m| m["method"] == "turn/start").await;
    pipe.say(json!({"id":turn["id"],"result":{"turn":{"id":"turn-main"}}})).await;
    start
}

async fn codex_call(pipe: &mut Pipe, id: u64, tool: &str, arguments: Value) -> (String, bool) {
    pipe.say(json!({"id":id,"method":"item/tool/call","params":{"threadId":"main","turnId":"turn-main","callId":format!("call-{id}"),
        "tool":tool,"arguments":arguments}}))
        .await;
    let result = wire(pipe, |m| m["id"] == id && m.get("result").is_some()).await["result"].clone();
    (result["contentItems"][0]["text"].as_str().unwrap_or_default().to_string(), result["success"] != true)
}

#[tokio::test]
async fn codex_reads_the_limits_and_searches_the_catalog_through_its_dynamic_tools() {
    let (dir, folder) = (support::canonical_tempdir().unwrap(), support::work_folder());
    let (spawn, made) = pipes();
    let app = engine(dir.path(), Vendor::Codex, spawn);
    let project = app.add_project(&folder.path().to_string_lossy()).unwrap().id;
    let id = app.send(request(Vendor::Codex, Area::Project, Some(project))).unwrap();
    let mut pipe = next_pipe(&made).await;
    let start = codex_boot(&mut pipe).await;
    let tools: Vec<String> =
        start["params"]["dynamicTools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().into()).collect();
    assert!(tools.contains(&"quota".to_string()) && tools.contains(&"catalog".to_string()), "{tools:?}");

    pipe.say(json!({"method":"item/started","params":{"threadId":"main","turnId":"turn-main","item":{"id":"c1","type":"dynamicToolCall",
        "tool":"catalog","arguments":{"query":"bug"}}}}))
        .await;
    let (quota, failed) = codex_call(&mut pipe, 900, "quota", json!({})).await;
    assert!(!failed, "{quota}");
    assert!(quota.contains("Codex (subscription") && quota.contains("weekly: 30% used") && quota.contains("renews in"), "{quota}");
    let (found, failed) = codex_call(&mut pipe, 901, "catalog", json!({ "query": "debugging a bug" })).await;
    assert!(!failed, "{found}");
    assert!(found.starts_with("systematic-debugging — Systematic debugging (skill)") && found.lines().count() <= 21, "{found}");

    let reply = app.open(&id).unwrap().messages.last().cloned().unwrap();
    assert!(reply.tools.unwrap().iter().any(|t| t.name == "Catalog"), "the step reads as Faber's own");
    app.shutdown();
}

#[tokio::test]
async fn codex_in_chat_gets_neither_tool() {
    let dir = support::canonical_tempdir().unwrap();
    let (spawn, made) = pipes();
    let app = engine(dir.path(), Vendor::Codex, spawn);
    app.send(request(Vendor::Codex, Area::Chat, None)).unwrap();
    let mut pipe = next_pipe(&made).await;
    let start = codex_boot(&mut pipe).await;
    let tools: Vec<String> = start["params"]["dynamicTools"]
        .as_array()
        .map(|t| t.iter().map(|t| t["name"].as_str().unwrap().into()).collect())
        .unwrap_or_default();
    assert!(!tools.contains(&"quota".to_string()) && !tools.contains(&"catalog".to_string()), "{tools:?}");
    let (refused, failed) = codex_call(&mut pipe, 902, "quota", json!({})).await;
    assert!(failed && refused.contains("not available"), "{refused}");
    app.shutdown();
}

/// A side question never gets either tool, whatever it is asked to offer (R250); a helper gets them only when the engine says so.
#[test]
fn a_side_question_lists_neither_tool_in_either_cli() {
    use faber_engine::claude::{ClaudeLaunch, claude_args};
    use faber_engine::codex::{CodexThread, thread_params};
    use faber_engine::helper::{CLAUDE_MCP_CONFIG, Helper};
    let thread = CodexThread {
        helper: Helper::Aside,
        instructions: Some("Record.".into()),
        progress: true,
        leave_plan: false,
        cwd: "project".into(),
        model: None,
        effort: None,
        resume: None,
        research: None,
        user_name: None,
        group_instructions: None,
        browse: false,
        conversations: false,
        verify: false,
        permissions: faber_engine::model::Permissions::FullAccess,
        tuning: Default::default(),
    };
    assert!(thread_params(Area::Project, &thread).1.get("dynamicTools").is_none());
    let launch = ClaudeLaunch {
        helper: Helper::Aside,
        browse: false,
        conversations: false,
        verify: false,
        progress: true,
        area: Area::Project,
        model: Some("unlisted-model".into()),
        effort: None,
        resume: None,
        user_hooks: false,
        instructions: Some("Record.".into()),
        research: None,
        user_name: None,
        group_instructions: None,
        permissions: faber_engine::model::Permissions::FullAccess,
        flags: Default::default(),
        tuning: Default::default(),
        tools: Default::default(),
    };
    assert!(!claude_args(&launch).iter().any(|a| a == CLAUDE_MCP_CONFIG), "no server of Faber's, so no tool");
}

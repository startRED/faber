//! Codex driver: one `codex app-server` process per conversation, spoken to over JSON-RPC on stdio.

use crate::claude::EventSink;
use crate::events::{EngineEvent, Session, SessionCommand, UserTurn};
use crate::model::Area;
use crate::proc::{self, Spawner};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};

const CHAT_INSTRUCTIONS: &str = "You are a helpful, precise assistant in a chat app. Answer in the language of the user. \
Use Markdown when it helps. Your working folder is a private workspace for this conversation: run code and create files \
there when it helps, and mention the files you create.";

/// Arguments for `codex app-server`. Projects keep the user's own setup, like a direct session, except hooks.
/// Chat stays lean (no plugins, apps, MCP servers or AGENTS.md) and works inside its own workspace folder.
/// On Windows the elevated sandbox fails to start commands in Codex 0.161 ("apply deny-read ACLs"),
/// so Faber uses the unelevated one.
pub fn codex_args(area: Area, windows: bool, user_hooks: bool) -> Vec<String> {
    let mut args = vec!["app-server".to_string()];
    let mut push = |a: &[&str]| args.extend(a.iter().map(|s| s.to_string()));
    if !user_hooks {
        push(&["--disable", "hooks"]);
    }
    if windows {
        push(&["-c", "windows.sandbox=\"unelevated\""]);
    }
    if area == Area::Chat {
        for feature in ["apps", "plugins", "computer_use", "browser_use"] {
            push(&["--disable", feature]);
        }
        push(&["-c", "mcp_servers={}", "-c", "project_doc_max_bytes=0"]);
    }
    args
}

/// Parameters for thread/start and thread/resume. The sandbox keeps writes inside the working folder;
/// anything beyond it comes back as an approval request.
pub fn codex_thread(area: Area, model: Option<&str>) -> Map<String, Value> {
    let mut thread = Map::new();
    if let Some(model) = model {
        thread.insert("model".into(), model.into());
    }
    thread.insert("sandbox".into(), "workspace-write".into());
    thread.insert("approvalPolicy".into(), "on-request".into());
    if area == Area::Chat {
        thread.insert("baseInstructions".into(), CHAT_INSTRUCTIONS.into());
    }
    thread
}

/// What parsing remembers across notifications: the latest usage, which Codex sends before turn/completed,
/// and which messages already streamed, so a message that arrives whole is still shown.
#[derive(Debug, Default)]
pub struct CodexTurnState {
    pub input: u64,
    pub output: u64,
    pub streamed: HashSet<String>,
}

const NOT_TOOLS: [&str; 3] = ["userMessage", "agentMessage", "reasoning"];

// Codex reports a limit window by its length; Faber names the ones it knows.
fn window_name(mins: i64) -> String {
    match mins {
        300 => "five_hour".into(),
        10080 => "seven_day".into(),
        _ => format!("{mins}_minutes"),
    }
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

/// Turns one `codex app-server` notification into engine events.
pub fn parse_codex_notification(msg: &Value, state: &mut CodexTurnState) -> Vec<EngineEvent> {
    let p = &msg["params"];
    match msg["method"].as_str().unwrap_or_default() {
        "item/agentMessage/delta" => {
            state.streamed.insert(s(&p["itemId"]));
            match p["delta"].as_str() {
                Some(delta) => vec![EngineEvent::Text(delta.into())],
                None => vec![EngineEvent::Error { message: "Codex sent a message delta without text.".into(), fatal: false }],
            }
        }
        "item/reasoning/summaryTextDelta" => vec![EngineEvent::Thinking(s(&p["delta"]))],
        "item/started" => {
            let item = &p["item"];
            let kind = item["type"].as_str().unwrap_or_default();
            if kind == "agentMessage" {
                return vec![EngineEvent::Break];
            }
            if NOT_TOOLS.contains(&kind) {
                return vec![];
            }
            let input = [&item["command"], &item["query"], &item["changes"]].into_iter().find(|v| !v.is_null()).unwrap_or(item).clone();
            vec![EngineEvent::Tool { id: s(&item["id"]), name: kind.into(), input }]
        }
        "item/completed" => {
            let item = &p["item"];
            let kind = item["type"].as_str().unwrap_or_default();
            let id = s(&item["id"]);
            if kind == "agentMessage" {
                let text = s(&item["text"]);
                return if !text.is_empty() && !state.streamed.contains(&id) { vec![EngineEvent::Text(text)] } else { vec![] };
            }
            if NOT_TOOLS.contains(&kind) {
                return vec![];
            }
            let ok = item["status"] == "completed" && (item["exitCode"].is_null() || item["exitCode"] == 0);
            let text = item["aggregatedOutput"].as_str().or(item["status"].as_str()).unwrap_or_default().into();
            vec![EngineEvent::ToolResult { id, ok, text }]
        }
        "thread/tokenUsage/updated" => {
            state.input = p["tokenUsage"]["last"]["inputTokens"].as_u64().unwrap_or(0);
            state.output = p["tokenUsage"]["last"]["outputTokens"].as_u64().unwrap_or(0);
            vec![]
        }
        "account/rateLimits/updated" => [&p["rateLimits"]["primary"], &p["rateLimits"]["secondary"]]
            .into_iter()
            .filter(|w| w.is_object())
            .map(|w| EngineEvent::Quota {
                window: window_name(w["windowDurationMins"].as_i64().unwrap_or(0)),
                used: w["usedPercent"].as_f64().unwrap_or(0.0) / 100.0,
                resets_at: w["resetsAt"].as_f64().unwrap_or(0.0),
            })
            .collect(),
        "turn/completed" => {
            let status = s(&p["turn"]["status"]);
            vec![EngineEvent::Done {
                ok: status == "completed",
                reason: status,
                cost_usd: None,
                input_tokens: state.input,
                output_tokens: state.output,
            }]
        }
        "error" => {
            let message = p["error"]["message"].as_str().map(String::from).unwrap_or_else(|| p.to_string().chars().take(300).collect());
            vec![EngineEvent::Error { message, fatal: false }]
        }
        _ => vec![],
    }
}

#[derive(Debug, Clone)]
pub struct CodexLaunch {
    pub area: Area,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub resume: Option<String>,
    pub user_hooks: bool,
}

pub struct CodexOptions {
    pub bin: String,
    pub cwd: PathBuf,
    pub launch: CodexLaunch,
    pub windows: bool,
    pub spawn: Spawner,
}

// What an answer from Codex is for.
enum Waiting {
    Initialize,
    Thread,
    Other(&'static str),
}

pub fn start_codex(opts: CodexOptions, on_event: EventSink) -> Session {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let CodexLaunch { area, model, effort, resume, user_hooks } = opts.launch;
    // The process starts right away, so a warmed-up session is already booting while the user types.
    let started = (opts.spawn)(&opts.bin, &codex_args(area, opts.windows, user_hooks), &opts.cwd);
    tokio::spawn(async move {
        let mut p = match started {
            Ok(p) => p,
            Err(err) => return on_event(EngineEvent::Error { message: format!("Codex could not start: {err}"), fatal: true }),
        };
        let tail = proc::stderr_tail(p.stderr.take());
        let mut lines = BufReader::new(p.stdout).lines();
        let mut stdin = p.stdin;
        let mut child = p.child;
        let mut waiting: HashMap<u64, Waiting> = HashMap::new();
        let mut approvals: HashMap<String, Value> = HashMap::new();
        let mut queued: Vec<UserTurn> = Vec::new();
        let mut state = CodexTurnState::default();
        let mut next_id = 0u64;
        let mut thread_id: Option<String> = None;
        let mut turn_id: Option<String> = None;
        let mut closed = false;

        macro_rules! request {
            ($method:expr, $params:expr, $kind:expr) => {{
                next_id += 1;
                waiting.insert(next_id, $kind);
                proc::write_line(&mut *stdin, &json!({ "id": next_id, "method": $method, "params": $params })).await;
            }};
        }
        let turn_params = |thread: &str, turn: &UserTurn| {
            let mut input: Vec<Value> = turn
                .images
                .iter()
                .map(|i| json!({ "type": "image", "url": format!("data:{};base64,{}", i.media_type, i.base64) }))
                .collect();
            input.push(json!({ "type": "text", "text": turn.text, "text_elements": [] }));
            let mut params = json!({ "threadId": thread, "input": input });
            if let Some(effort) = &effort {
                params["effort"] = effort.clone().into();
            }
            params
        };

        let init = json!({ "clientInfo": { "name": "faber", "title": "Faber", "version": "0.0.0" }, "capabilities": { "experimentalApi": false, "requestAttestation": false } });
        request!("initialize", init, Waiting::Initialize);

        loop {
            tokio::select! {
                line = lines.next_line() => {
                    let Ok(Some(line)) = line else {
                        if !closed {
                            let code = proc::exit_code(&mut child).await;
                            let tail = tail.lock().unwrap().clone();
                            on_event(EngineEvent::Error { message: format!("Codex stopped (exit {code}). {tail}").trim().into(), fatal: true });
                        }
                        break;
                    };
                    if line.trim().is_empty() {
                        continue;
                    }
                    let Ok(msg) = serde_json::from_str::<Value>(&line) else {
                        on_event(EngineEvent::Error { message: format!("Unexpected output from Codex: {}", line.chars().take(200).collect::<String>()), fatal: false });
                        continue;
                    };
                    let method = msg["method"].as_str();
                    match (method, msg.get("id")) {
                        // An answer to one of Faber's requests.
                        (None, Some(id)) => {
                            let Some(kind) = id.as_u64().and_then(|id| waiting.remove(&id)) else { continue };
                            if let Some(error) = msg.get("error") {
                                let what = match kind { Waiting::Initialize => "initialize", Waiting::Thread => "thread", Waiting::Other(m) => m };
                                let detail = error["message"].as_str().map(String::from).unwrap_or_else(|| error.to_string());
                                on_event(EngineEvent::Error { message: format!("Codex {what} failed: {detail}"), fatal: false });
                                continue;
                            }
                            match kind {
                                Waiting::Initialize => {
                                    proc::write_line(&mut *stdin, &json!({ "method": "initialized" })).await;
                                    let mut params = codex_thread(area, model.as_deref());
                                    let method = match &resume {
                                        Some(id) => { params.insert("threadId".into(), id.clone().into()); "thread/resume" }
                                        None => { params.insert("cwd".into(), opts.cwd.to_string_lossy().into_owned().into()); "thread/start" }
                                    };
                                    request!(method, Value::Object(params), Waiting::Thread);
                                }
                                Waiting::Thread => {
                                    let thread = &msg["result"]["thread"];
                                    let id = s(&thread["id"]);
                                    let shown = thread["model"].as_str().map(String::from).or_else(|| model.clone()).unwrap_or_else(|| "codex".into());
                                    on_event(EngineEvent::Started { session_id: id.clone(), model: shown });
                                    for turn in std::mem::take(&mut queued) {
                                        request!("turn/start", turn_params(&id, &turn), Waiting::Other("turn/start"));
                                    }
                                    thread_id = Some(id);
                                }
                                Waiting::Other(_) => {}
                            }
                        }
                        // A request from Codex: approvals reach the user; anything else is declined politely.
                        (Some(method), Some(id)) => {
                            if method.ends_with("requestApproval") {
                                let key = match id { Value::String(s) => s.clone(), other => other.to_string() };
                                approvals.insert(key.clone(), id.clone());
                                let p = &msg["params"];
                                let tool = p["kind"].as_str().unwrap_or(method).to_string();
                                let input = [&p["command"], &p["changes"]].into_iter().find(|v| !v.is_null()).unwrap_or(p).clone();
                                on_event(EngineEvent::Permission { request_id: key, tool, input });
                            } else {
                                let error = json!({ "id": id, "error": { "code": -32601, "message": format!("Faber does not handle {method} yet") } });
                                proc::write_line(&mut *stdin, &error).await;
                            }
                        }
                        _ => {
                            if method == Some("turn/started") {
                                turn_id = msg["params"]["turn"]["id"].as_str().map(String::from);
                            }
                            for event in parse_codex_notification(&msg, &mut state) {
                                on_event(event);
                            }
                        }
                    }
                }
                cmd = rx.recv(), if !closed => match cmd {
                    Some(SessionCommand::Send(turn)) => match &thread_id {
                        Some(id) => { let params = turn_params(id, &turn); request!("turn/start", params, Waiting::Other("turn/start")) }
                        None => queued.push(turn),
                    },
                    Some(SessionCommand::Answer { request_id, allow }) => {
                        if let Some(id) = approvals.remove(&request_id) {
                            let decision = if allow { "accept" } else { "decline" };
                            proc::write_line(&mut *stdin, &json!({ "id": id, "result": { "decision": decision } })).await;
                        }
                    }
                    Some(SessionCommand::Interrupt) => {
                        if let (Some(thread), Some(turn)) = (thread_id.clone(), turn_id.clone()) {
                            request!("turn/interrupt", json!({ "threadId": thread, "turnId": turn }), Waiting::Other("turn/interrupt"));
                        }
                    }
                    Some(SessionCommand::Close) | None => {
                        closed = true;
                        proc::stop(&mut *stdin, &mut child).await;
                    }
                }
            }
        }
    });
    Session::new(tx)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub is_default: bool,
}

/// Asks a short-lived `codex app-server` which models this account can use, so the picker shows the real list.
/// Gives an empty list on any failure: the picker then offers just the account's default.
pub async fn list_codex_models(bin: &str, cwd: &Path, windows: bool, spawn: Spawner, timeout: Duration) -> Vec<ModelInfo> {
    let Ok(p) = spawn(bin, &codex_args(Area::Chat, windows, false), cwd) else { return vec![] };
    let mut stdin = p.stdin;
    let mut child = p.child;
    let mut lines = BufReader::new(p.stdout).lines();
    let ask = async {
        let init = json!({ "id": 1, "method": "initialize", "params": { "clientInfo": { "name": "faber", "title": "Faber", "version": "0.0.0" } } });
        proc::write_line(&mut *stdin, &init).await;
        while let Ok(Some(line)) = lines.next_line().await {
            let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
            if msg["id"] == 1 {
                proc::write_line(&mut *stdin, &json!({ "method": "initialized" })).await;
                proc::write_line(&mut *stdin, &json!({ "id": 2, "method": "model/list", "params": {} })).await;
            } else if msg["id"] == 2 {
                return msg["result"]["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|m| m["hidden"] != true)
                    .map(|m| ModelInfo {
                        id: s(&m["id"]),
                        label: m["displayName"].as_str().map(String::from).unwrap_or_else(|| s(&m["id"])),
                        description: m["description"].as_str().map(String::from),
                        is_default: m["isDefault"] == true,
                    })
                    .collect();
            }
        }
        vec![]
    };
    let models = tokio::time::timeout(timeout, ask).await.unwrap_or_default();
    proc::stop(&mut *stdin, &mut child).await;
    models
}

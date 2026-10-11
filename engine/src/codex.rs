//! Codex driver: one shared `codex app-server` process, spoken to over JSON-RPC on stdio, with each conversation as a
//! thread in it.

use crate::claude::EventSink;
use crate::events::{EngineEvent, Session, SessionCommand, UserTurn};
use crate::model::Area;
use crate::proc::{self, Spawner};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

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

/// What needs a process of its own: Chat and Projects start `codex app-server` with different flags.
#[derive(Clone)]
pub struct CodexServerOptions {
    pub bin: String,
    /// Where the process itself runs; each thread works in its own folder.
    pub cwd: PathBuf,
    pub area: Area,
    pub user_hooks: bool,
    pub windows: bool,
    pub spawn: Spawner,
}

/// One conversation inside the shared process.
#[derive(Debug, Clone)]
pub struct CodexThread {
    pub cwd: PathBuf,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub resume: Option<String>,
    /// A side question (such as routing) with its own instructions: it reads nothing and runs nothing.
    pub instructions: Option<String>,
}

enum ServerCommand {
    Open { key: u64, thread: CodexThread, sink: EventSink },
    Session(u64, SessionCommand),
    Retire,
}

// What an answer from Codex is for.
enum Waiting {
    Initialize,
    Thread(u64),
    Other(u64, &'static str),
}

/// A conversation's side of the shared process.
struct Thread {
    sink: EventSink,
    params: CodexThread,
    id: Option<String>,
    turn: Option<String>,
    queued: Vec<UserTurn>,
    state: CodexTurnState,
    approvals: HashMap<String, Value>,
}

fn turn_params(thread: &str, effort: Option<&str>, turn: &UserTurn) -> Value {
    let mut input: Vec<Value> =
        turn.images.iter().map(|i| json!({ "type": "image", "url": format!("data:{};base64,{}", i.media_type, i.base64) })).collect();
    input.push(json!({ "type": "text", "text": turn.text, "text_elements": [] }));
    let mut params = json!({ "threadId": thread, "input": input });
    if let Some(effort) = effort {
        params["effort"] = effort.into();
    }
    params
}

pub fn thread_params(area: Area, t: &CodexThread) -> (&'static str, Value) {
    let mut params = codex_thread(area, t.model.as_deref());
    if let Some(instructions) = &t.instructions {
        params.insert("baseInstructions".into(), instructions.clone().into());
        params.insert("sandbox".into(), "read-only".into());
        params.insert("approvalPolicy".into(), "never".into());
        // Nothing to resume later: it stays out of the user's Codex history.
        params.insert("ephemeral".into(), true.into());
    }
    params.insert("cwd".into(), t.cwd.to_string_lossy().into_owned().into());
    let method = match &t.resume {
        Some(id) => {
            params.insert("threadId".into(), id.clone().into());
            "thread/resume"
        }
        None => "thread/start",
    };
    (method, Value::Object(params))
}

/// One `codex app-server` serves every conversation with the same settings, each as a thread of its own, so open
/// conversations cost one process instead of one each (measured: about 50 MB for three threads).
#[derive(Clone)]
pub struct CodexServer {
    tx: UnboundedSender<ServerCommand>,
    next: Arc<AtomicU64>,
}

impl CodexServer {
    /// The process starts right away, so a warmed-up conversation is already booting while the user types.
    pub fn start(opts: CodexServerOptions) -> CodexServer {
        let (tx, rx) = unbounded_channel();
        let started = (opts.spawn)(&opts.bin, &codex_args(opts.area, opts.windows, opts.user_hooks), &opts.cwd);
        tokio::spawn(serve(started, opts.area, rx));
        CodexServer { tx, next: Arc::new(AtomicU64::new(1)) }
    }

    /// False once the process has stopped; the engine then starts a new one.
    pub fn is_alive(&self) -> bool {
        !self.tx.is_closed()
    }

    /// Opens a conversation as a thread. The session behaves as if it had the process to itself.
    pub fn open(&self, thread: CodexThread, sink: EventSink) -> Session {
        let key = self.next.fetch_add(1, Ordering::Relaxed);
        if let Err(SendError(ServerCommand::Open { sink, .. })) = self.tx.send(ServerCommand::Open { key, thread, sink }) {
            sink(EngineEvent::Error { message: "Codex stopped.".into(), fatal: true });
        }
        // Straight into the server's queue, so commands from all conversations keep the order they were given in.
        let server = self.tx.clone();
        Session::from_fn(move |cmd| drop(server.send(ServerCommand::Session(key, cmd))))
    }

    /// Stops the process once its last conversation closes; new conversations go to a server with the new settings.
    pub fn retire(&self) {
        let _ = self.tx.send(ServerCommand::Retire);
    }
}

async fn serve(started: std::io::Result<proc::Proc>, area: Area, mut rx: UnboundedReceiver<ServerCommand>) {
    let mut p = match started {
        Ok(p) => p,
        Err(err) => {
            // Every conversation already waiting hears why; the next one starts a fresh process.
            rx.close();
            while let Some(cmd) = rx.recv().await {
                if let ServerCommand::Open { sink, .. } = cmd {
                    sink(EngineEvent::Error { message: format!("Codex could not start: {err}"), fatal: true });
                }
            }
            return;
        }
    };
    let tail = proc::stderr_tail(p.stderr.take());
    let mut lines = BufReader::new(p.stdout).lines();
    let mut stdin = p.stdin;
    let mut child = p.child;
    let mut threads: HashMap<u64, Thread> = HashMap::new();
    let mut by_id: HashMap<String, u64> = HashMap::new();
    // Opened before Codex finished starting, in order, so request ids stay predictable.
    let mut pending: Vec<u64> = Vec::new();
    let mut waiting: HashMap<u64, Waiting> = HashMap::new();
    let mut next_id = 0u64;
    let (mut ready, mut retired, mut stopping) = (false, false, false);

    macro_rules! request {
        ($method:expr, $params:expr, $kind:expr) => {{
            next_id += 1;
            waiting.insert(next_id, $kind);
            proc::write_line(&mut *stdin, &json!({ "id": next_id, "method": $method, "params": $params })).await;
        }};
    }
    macro_rules! stop {
        () => {{
            stopping = true;
            proc::stop(&mut *stdin, &mut child).await;
        }};
    }

    let init = json!({ "clientInfo": { "name": "faber", "title": "Faber", "version": "0.0.0" }, "capabilities": { "experimentalApi": false, "requestAttestation": false } });
    request!("initialize", init, Waiting::Initialize);

    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Ok(Some(line)) = line else {
                    if !stopping {
                        let code = proc::exit_code(&mut child).await;
                        let message = format!("Codex stopped (exit {code}). {}", tail.lock().unwrap()).trim().to_string();
                        for t in threads.values() {
                            (t.sink)(EngineEvent::Error { message: message.clone(), fatal: true });
                        }
                    }
                    break;
                };
                if line.trim().is_empty() {
                    continue;
                }
                let Ok(msg) = serde_json::from_str::<Value>(&line) else {
                    let message = format!("Unexpected output from Codex: {}", line.chars().take(200).collect::<String>());
                    for t in threads.values() {
                        (t.sink)(EngineEvent::Error { message: message.clone(), fatal: false });
                    }
                    continue;
                };
                let method = msg["method"].as_str();
                let params = &msg["params"];
                match (method, msg.get("id")) {
                    // An answer to one of Faber's requests.
                    (None, Some(id)) => {
                        let Some(kind) = id.as_u64().and_then(|id| waiting.remove(&id)) else { continue };
                        if let Some(error) = msg.get("error") {
                            let detail = error["message"].as_str().map(String::from).unwrap_or_else(|| error.to_string());
                            let (keys, what, fatal): (Vec<u64>, &str, bool) = match kind {
                                Waiting::Initialize => (threads.keys().copied().collect(), "initialize", true),
                                Waiting::Thread(key) => (vec![key], "thread", true),
                                Waiting::Other(key, m) => (vec![key], m, false),
                            };
                            for t in keys.iter().filter_map(|k| threads.get(k)) {
                                (t.sink)(EngineEvent::Error { message: format!("Codex {what} failed: {detail}"), fatal });
                            }
                            continue;
                        }
                        match kind {
                            Waiting::Initialize => {
                                proc::write_line(&mut *stdin, &json!({ "method": "initialized" })).await;
                                ready = true;
                                for key in std::mem::take(&mut pending) {
                                    let Some(t) = threads.get(&key) else { continue };
                                    let (method, params) = thread_params(area, &t.params);
                                    request!(method, params, Waiting::Thread(key));
                                }
                            }
                            Waiting::Thread(key) => {
                                let Some(t) = threads.get_mut(&key) else { continue };
                                let thread = &msg["result"]["thread"];
                                let id = s(&thread["id"]);
                                let shown = thread["model"].as_str().map(String::from).or_else(|| t.params.model.clone()).unwrap_or_else(|| "codex".into());
                                (t.sink)(EngineEvent::Started { session_id: id.clone(), model: shown });
                                by_id.insert(id.clone(), key);
                                t.id = Some(id.clone());
                                for turn in std::mem::take(&mut t.queued) {
                                    let params = turn_params(&id, t.params.effort.as_deref(), &turn);
                                    request!("turn/start", params, Waiting::Other(key, "turn/start"));
                                }
                            }
                            Waiting::Other(..) => {}
                        }
                    }
                    // A request from Codex: approvals reach the user; anything else is declined politely.
                    (Some(method), Some(id)) => {
                        let thread = params["threadId"].as_str().and_then(|t| by_id.get(t)).and_then(|k| threads.get_mut(k));
                        match (method.ends_with("requestApproval"), thread) {
                            (true, Some(t)) => {
                                let key = match id { Value::String(s) => s.clone(), other => other.to_string() };
                                t.approvals.insert(key.clone(), id.clone());
                                let tool = params["kind"].as_str().unwrap_or(method).to_string();
                                let input = [&params["command"], &params["changes"]].into_iter().find(|v| !v.is_null()).unwrap_or(params).clone();
                                (t.sink)(EngineEvent::Permission { request_id: key, tool, input });
                            }
                            // The conversation is gone, so nobody can say yes.
                            (true, None) => proc::write_line(&mut *stdin, &json!({ "id": id, "result": { "decision": "decline" } })).await,
                            (false, _) => {
                                let error = json!({ "id": id, "error": { "code": -32601, "message": format!("Faber does not handle {method} yet") } });
                                proc::write_line(&mut *stdin, &error).await;
                            }
                        }
                    }
                    _ => {
                        let thread_id = params["threadId"].as_str().or_else(|| params["thread"]["id"].as_str());
                        match thread_id.and_then(|t| by_id.get(t)).and_then(|k| threads.get_mut(k)) {
                            Some(t) => {
                                if method == Some("turn/started") {
                                    t.turn = params["turn"]["id"].as_str().map(String::from);
                                }
                                for event in parse_codex_notification(&msg, &mut t.state) {
                                    (t.sink)(event);
                                }
                            }
                            // Account-wide news, such as quota: one conversation passes it on.
                            None if thread_id.is_none() => {
                                if let Some(t) = threads.values().next() {
                                    for event in parse_codex_notification(&msg, &mut CodexTurnState::default()) {
                                        (t.sink)(event);
                                    }
                                }
                            }
                            None => {}
                        }
                    }
                }
            }
            cmd = rx.recv(), if !stopping => match cmd {
                Some(ServerCommand::Open { key, thread, sink }) => {
                    let t = Thread { sink, params: thread, id: None, turn: None, queued: vec![], state: CodexTurnState::default(), approvals: HashMap::new() };
                    if ready {
                        let (method, params) = thread_params(area, &t.params);
                        request!(method, params, Waiting::Thread(key));
                    } else {
                        pending.push(key);
                    }
                    threads.insert(key, t);
                }
                Some(ServerCommand::Session(key, cmd)) => match cmd {
                    SessionCommand::Send(turn) => {
                        let Some(t) = threads.get_mut(&key) else { continue };
                        match t.id.clone() {
                            Some(id) => request!("turn/start", turn_params(&id, t.params.effort.as_deref(), &turn), Waiting::Other(key, "turn/start")),
                            None => t.queued.push(turn),
                        }
                    }
                    SessionCommand::Answer { request_id, allow } => {
                        if let Some(id) = threads.get_mut(&key).and_then(|t| t.approvals.remove(&request_id)) {
                            let decision = if allow { "accept" } else { "decline" };
                            proc::write_line(&mut *stdin, &json!({ "id": id, "result": { "decision": decision } })).await;
                        }
                    }
                    SessionCommand::Interrupt => {
                        if let Some((thread, turn)) = threads.get(&key).and_then(|t| Some((t.id.clone()?, t.turn.clone()?))) {
                            request!("turn/interrupt", json!({ "threadId": thread, "turnId": turn }), Waiting::Other(key, "turn/interrupt"));
                        }
                    }
                    // The thread stays loaded in Codex (cheap); Faber stops listening, and stops any work in flight.
                    SessionCommand::Close => {
                        pending.retain(|k| *k != key);
                        if let Some(t) = threads.remove(&key)
                            && let Some(id) = &t.id
                        {
                            by_id.remove(id);
                            if let Some(turn) = &t.turn {
                                request!("turn/interrupt", json!({ "threadId": id, "turnId": turn }), Waiting::Other(key, "turn/interrupt"));
                            }
                        }
                        if retired && threads.is_empty() {
                            stop!();
                        }
                    }
                },
                Some(ServerCommand::Retire) => {
                    retired = true;
                    if threads.is_empty() {
                        stop!();
                    }
                }
                None => stop!(),
            }
        }
    }
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

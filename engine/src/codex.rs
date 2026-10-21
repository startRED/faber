//! Codex driver: one shared `codex app-server` process, spoken to over JSON-RPC on stdio, with each conversation as a
//! thread in it.

use crate::claude::EventSink;
use crate::events::{Delivery, EngineEvent, Session, SessionCommand, UserTurn};
use crate::model::{Area, CliSession, Permissions, ReasoningSummary, ReplyStyle, Tuning, Vendor, WebSearch};
use crate::persona::{PersonaInput, persona};
use crate::proc::{self, Spawner};
use crate::signals::{codex_account, codex_signals};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

const CHAT_INSTRUCTIONS: &str = "You are a helpful, precise assistant in a chat app. Answer in the language of the user's \
latest message, even when files or tools use another. Use Markdown when it helps. Your working folder belongs to this conversation: run code and create files \
there when it helps, and mention the files you create.";

/// Every guarded mode (Ask, Accept edits, Plan, Auto) preserves the existing Ask setup. Isolated servers disable integrations
/// and require MXC on Windows. Full access ("Do not ask", R121) starts the CLI as the person would: their MCP servers, plugins
/// and hooks stay on, and no sandbox is configured.
pub fn codex_args(area: Area, windows: bool, user_hooks: bool, permissions: Permissions) -> Vec<String> {
    let mut args = vec!["app-server".to_string()];
    let mut push = |a: &[&str]| args.extend(a.iter().map(|s| s.to_string()));
    let isolated = permissions == Permissions::Isolated;
    if isolated || (permissions.guarded() && !user_hooks) {
        push(&["--disable", "hooks"]);
    }
    if windows && permissions != Permissions::FullAccess {
        push(&["-c", if isolated { "windows.sandbox=\"mxc\"" } else { "windows.sandbox=\"unelevated\"" }]);
    }
    if isolated || (area == Area::Chat && permissions.guarded()) {
        for feature in ["apps", "plugins", "computer_use", "browser_use"] {
            push(&["--disable", feature]);
        }
        push(&["-c", "mcp_servers={}"]);
    }
    if area == Area::Chat && permissions != Permissions::FullAccess {
        push(&["-c", "project_doc_max_bytes=0"]);
    }
    if isolated {
        push(&["--disable", "skill_mcp_dependency_install"]);
    }
    args
}

/// Parameters for thread/start and thread/resume. Ask keeps writes inside the working folder and asks before every command that
/// is not a plain read and before every file change, inside that folder too. The other guarded modes (R140) keep that sandbox and
/// ask only for what leaves it: Accept edits and Auto let `on-request` decide, Plan makes the folder read-only, and Auto sends
/// the requests that remain to Codex's own reviewer (`auto_review`) instead of the person. The reviewer is named in every
/// guarded mode, so a person's own config file cannot pick a looser one.
/// Full access runs without Codex's sandbox and never asks. Isolated runs Faber's own sandbox and fails what it does not allow.
///
/// Full access cannot keep Codex away from the credential folders: Codex's only filesystem deny rules live in a permission
/// profile, and on Windows a profile that grants the whole disk and denies a few folders is refused when any drive letter is
/// unreachable ("MXC cannot verify it does not alias a denied object") or needs the elevated sandbox. The same limit is
/// written in docs/security-r97-review.md.
pub fn codex_thread(area: Area, model: Option<&str>, permissions: Permissions) -> Map<String, Value> {
    let mut thread = Map::new();
    if let Some(model) = model {
        thread.insert("model".into(), model.into());
    }
    let (sandbox, approval, reviewer) = match permissions {
        // "untrusted": everything but a short list of plain reads asks first, file changes inside the working folder included.
        Permissions::Ask => ("workspace-write", "untrusted", Some("user")),
        Permissions::AcceptEdits => ("workspace-write", "on-request", Some("user")),
        Permissions::Plan => ("read-only", "on-request", Some("user")),
        Permissions::Auto => ("workspace-write", "on-request", Some("auto_review")),
        Permissions::FullAccess => ("danger-full-access", "never", None),
        Permissions::Isolated => ("workspace-write", "never", None),
    };
    thread.insert("sandbox".into(), sandbox.into());
    thread.insert("approvalPolicy".into(), approval.into());
    if let Some(reviewer) = reviewer {
        thread.insert("approvalsReviewer".into(), reviewer.into());
    }
    if permissions == Permissions::Isolated {
        thread.remove("sandbox");
        thread.insert("config".into(), crate::permissions::sandbox_config(Path::new("."), true).into());
        thread["config"]["features.shell_tool"] = false.into();
        thread.insert("dynamicTools".into(), json!([{
            "name":"run_command", "description":"Run one literal command in the working folder through Faber's sandbox. Supports read-only git inspection, local deletion, builds, tests and local scripts; rejects shell expansion, compound commands and git mutations. Use rg for text reads and local script files for code execution.",
            "inputSchema":{"type":"object","properties":{"command":{"type":"string"}},"required":["command"],"additionalProperties":false}
        }]));
    }
    if area == Area::Chat {
        thread.insert("baseInstructions".into(), CHAT_INSTRUCTIONS.into());
    }
    thread
}

/// The thread's token counts, as `thread/tokenUsage/updated` reports them in `total`.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Totals {
    pub input: u64,
    pub cached: u64,
    pub written: Option<u64>,
    pub output: u64,
}

impl Totals {
    fn since(self, base: Totals) -> Totals {
        Totals {
            input: self.input.saturating_sub(base.input),
            cached: self.cached.saturating_sub(base.cached),
            written: self.written.zip(base.written).and_then(|(now, before)| now.checked_sub(before)),
            output: self.output.saturating_sub(base.output),
        }
    }
}

/// What parsing remembers across notifications: the thread's usage so far and its value when the turn started, and which
/// messages already streamed, so a message that arrives whole is still shown.
#[derive(Debug, Default)]
pub struct CodexTurnState {
    pub streamed: HashSet<String>,
    /// The thread's usage so far, as the latest update said. None until one arrives (a resumed thread gets one before its
    /// first turn).
    pub total: Option<Totals>,
    /// `total` when the current turn started.
    pub base: Option<Totals>,
    pub model: String,
    cost: f64,
    priced: bool,
    price_incomplete: bool,
    prompt: Option<u64>,
    /// User messages seen in the current turn: the first is the turn's own prompt, any later one was steered in.
    users: u32,
    plan: Option<(String, String)>,
    /// What the turn's subagents used, which Codex counts in their own threads.
    subagents: Totals,
    subagents_cost: Option<f64>,
    subagents_incomplete: bool,
}

impl CodexTurnState {
    /// A subagent of the turn is still at work, so the turn's usage misses what it used.
    pub fn subagent_running(&mut self) {
        self.subagents_incomplete = true;
    }

    /// A subagent's turn ended: what it used counts for the turn of the conversation that started it.
    pub fn add_subagent(&mut self, done: &EngineEvent) {
        let EngineEvent::Done { cost_total_usd, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, incomplete, .. } = done
        else {
            return;
        };
        let sub = &mut self.subagents;
        sub.input += input_tokens;
        sub.output += output_tokens;
        sub.cached += cache_read_tokens;
        sub.written = Some(sub.written.unwrap_or(0) + cache_write_tokens);
        match cost_total_usd {
            Some(cost) => *self.subagents_cost.get_or_insert(0.0) += cost,
            None => self.subagents_incomplete = true,
        }
        self.subagents_incomplete |= incomplete;
    }

    /// A thread that starts empty has used nothing yet; a resumed one learns its total from the update that comes first.
    pub fn new(resumed: bool) -> Self {
        CodexTurnState { total: (!resumed).then(|| Totals { written: Some(0), ..Totals::default() }), ..Default::default() }
    }
}

const NOT_TOOLS: [&str; 3] = ["userMessage", "agentMessage", "reasoning"];

/// The name a subagent's call takes on screen, the same as Claude Code's.
const SUBAGENT: &str = "Agent";
/// A stop sent to a subagent's turn, whose answer nobody waits for.
const SUBAGENT_STOP: &str = "subagent/interrupt";

/// The few words for a subagent: the last part of the path Codex gives it (`/root/read_docs` reads "read docs").
fn subagent_title(path: &str) -> String {
    path.rsplit('/').next().unwrap_or_default().replace(['_', '-'], " ").trim().to_string()
}

/// Codex's own subagents (its `multi_agent` feature, R161). Each works in a thread of its own that Faber never opened: the
/// thread that started it reports it as `subAgentActivity` items, and its own notifications name its thread.
#[derive(Default)]
pub struct Subagents(HashMap<String, Subagent>);

#[derive(Default)]
struct NativeCommands {
    /// Terminal handles are opaque; CLI completion also needs the server's process-tree proof.
    items: HashMap<String, Option<String>>,
    unknown: bool,
}

fn reported_terminal(item: &Value) -> Option<String> {
    item["processId"].as_str().filter(|p| !p.is_empty()).map(String::from).or_else(|| item["processId"].as_u64().map(|pid| pid.to_string()))
}

impl NativeCommands {
    fn track(&mut self, msg: &Value) {
        let item = &msg["params"]["item"];
        if item["type"] != "commandExecution" {
            return;
        }
        let Some(id) = item["id"].as_str().filter(|id| !id.is_empty()) else {
            self.unknown = true;
            return;
        };
        let process = reported_terminal(item);
        match msg["method"].as_str() {
            Some("item/completed") if item["exitCode"].as_i64().is_some() => {
                self.items.remove(id);
            }
            Some("item/started" | "item/completed") if process.is_some() => {
                self.items.insert(id.into(), process);
            }
            Some("item/started") => {
                self.items.insert(id.into(), None);
            }
            Some("item/completed") if !item["processId"].is_null() => {
                self.unknown = true;
            }
            Some("item/completed") if self.items.get(id).is_none_or(Option::is_none) => {
                self.items.remove(id);
            }
            _ => (),
        }
    }

    fn unknown(&self) -> bool {
        self.unknown || self.items.values().any(Option::is_some)
    }

    fn stops(&self, thread: &str) -> Vec<(String, String, String)> {
        self.items.iter().filter_map(|(item, process)| Some((thread.into(), item.clone(), process.clone()?))).collect()
    }

    fn confirm_gone(&mut self, item: &str, process: &str) {
        if self.items.get(item).and_then(Option::as_deref) == Some(process) {
            self.items.remove(item);
        }
    }
}

struct Subagent {
    /// The conversation it works for.
    key: u64,
    /// The call that started it, under which its tools show.
    call: String,
    /// Its latest words, which become its report.
    report: String,
    state: CodexTurnState,
    /// The conversation's turn it works for. Its approvals and usage belong to that turn only: once the turn is over they
    /// reach no other reply.
    turn: Option<String>,
    /// Its own turn, while one is under way.
    own: Option<String>,
    /// The child was announced, but has not confirmed whether its turn is running.
    pending: bool,
    commands: NativeCommands,
}

/// What one notification of a subagent's thread means for its conversation.
pub struct ChildNews {
    pub key: u64,
    /// The conversation's turn the subagent serves.
    pub turn: Option<String>,
    pub events: Vec<EngineEvent>,
    /// Its own turn ended with this usage (a `Done`), which counts for `turn`.
    pub used: Option<EngineEvent>,
}

impl Subagents {
    /// The conversation a subagent's thread works for, and the turn of it.
    pub fn owner(&self, thread: &str) -> Option<(u64, Option<String>)> {
        self.0.get(thread).map(|sub| (sub.key, sub.turn.clone()))
    }

    /// Whether a subagent of conversation `key` is still at work for its turn `turn`.
    pub fn running(&self, key: u64, turn: &str) -> bool {
        self.0.values().any(|sub| sub.key == key && sub.own.is_some() && sub.turn.as_deref() == Some(turn))
    }

    /// The subagents of conversation `key` at work now, as their thread and turn, so a stop reaches them too.
    pub fn at_work(&self, key: u64) -> Vec<(String, String)> {
        self.0.iter().filter(|(_, sub)| sub.key == key).filter_map(|(thread, sub)| Some((thread.clone(), sub.own.clone()?))).collect()
    }

    fn writers(&self, key: u64) -> (Vec<String>, bool) {
        let mut active = Vec::new();
        let mut unknown = false;
        for (thread, sub) in &self.0 {
            if sub.key == key {
                unknown |= sub.pending || sub.commands.unknown();
                if sub.own.is_some() || sub.pending || !sub.commands.items.is_empty() {
                    active.push(format!("helper:{thread}"));
                }
            }
        }
        (active, unknown)
    }

    /// Follows the subagents a notification of conversation `key`, sent for its turn `turn`, starts and ends. One that ends
    /// settles the call that started it, with its report.
    pub fn track(&mut self, key: u64, turn: Option<&str>, msg: &Value) -> Option<EngineEvent> {
        let item = &msg["params"]["item"];
        if item["type"] != "subAgentActivity" {
            return None;
        }
        let thread = s(&item["agentThreadId"]);
        match (msg["method"].as_str(), item["kind"].as_str()) {
            (Some("item/started"), Some("started")) if !thread.is_empty() => {
                // A subagent's thread starts empty; its model prices what it uses.
                let state = CodexTurnState { model: s(&item["model"]), ..CodexTurnState::new(false) };
                let turn = turn.map(String::from);
                self.0.entry(thread).or_insert_with(|| Subagent {
                    key,
                    call: s(&item["id"]),
                    report: String::new(),
                    state,
                    turn,
                    own: None,
                    pending: true,
                    commands: Default::default(),
                });
                None
            }
            (Some("item/completed"), Some(end @ ("completed" | "interrupted"))) => {
                let sub = self.0.get_mut(&thread).filter(|sub| sub.key == key)?;
                sub.pending = false;
                sub.own = None;
                Some(EngineEvent::ToolResult { id: sub.call.clone(), ok: end == "completed", declined: false, text: sub.report.clone() })
            }
            _ => None,
        }
    }

    /// A notification from a subagent's own thread: the tools it ran, under the call that started it, for its conversation.
    /// Its words are kept as its report and never join the answer. None when the thread is no subagent's.
    pub fn child(&mut self, msg: &Value) -> Option<ChildNews> {
        let sub = self.0.get_mut(msg["params"]["threadId"].as_str()?)?;
        sub.commands.track(msg);
        match msg["method"].as_str() {
            // Each turn of a subagent names the conversation's turn it serves (a later message to it starts a new one).
            Some("turn/started") => {
                sub.own = msg["params"]["turn"]["id"].as_str().map(String::from);
                sub.pending = sub.own.is_none();
                sub.report.clear();
                if let Some(root) = msg["params"]["turn"]["rootTurnId"].as_str() {
                    sub.turn = Some(root.to_string());
                }
            }
            Some("turn/completed") => {
                sub.own = None;
                sub.pending = false;
            }
            _ => {}
        }
        let (key, call, turn) = (sub.key, sub.call.clone(), sub.turn.clone());
        let item = &msg["params"]["item"];
        if msg["method"] == "item/completed" && item["type"] == "agentMessage" && item["text"].as_str().is_some_and(|t| !t.is_empty()) {
            sub.report = s(&item["text"]);
        }
        let mut used = None;
        let mut events: Vec<EngineEvent> = parse_codex_notification(msg, &mut sub.state)
            .into_iter()
            .filter_map(|event| match event {
                done @ EngineEvent::Done { .. } => {
                    used = Some(done);
                    None
                }
                EngineEvent::Tool { id, name, input, parent } => {
                    Some(EngineEvent::Tool { id, name, input, parent: parent.or(Some(call.clone())) })
                }
                result @ (EngineEvent::ToolResult { .. } | EngineEvent::Background(_)) => Some(result),
                _ => None,
            })
            .collect();
        // A subagent's own subagent works for the same conversation and folds into the step it started from.
        events.extend(self.track(key, turn.as_deref(), msg));
        Some(ChildNews { key, turn, events, used })
    }

    /// The conversation closed: its subagents' threads are no longer followed.
    pub fn forget(&mut self, key: u64) {
        self.0.retain(|_, sub| sub.key != key);
    }
}

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
    // The thread's context after a request: what the last request held (`last.totalTokens`, which after a compaction is the
    // size of what is left), against the model's window.
    let usage = &msg["params"]["tokenUsage"];
    let context = (msg["method"] == "thread/tokenUsage/updated")
        .then(|| usage["last"]["totalTokens"].as_u64().or(usage["last"]["inputTokens"].as_u64()))
        .flatten()
        .map(|used| EngineEvent::Context { used: Some(used), window: usage["modelContextWindow"].as_u64() });
    context.into_iter().chain(parse_notification(msg, state)).chain(command_news(msg)).collect()
}

fn command_key(thread: &str, item: &str) -> String {
    json!([thread, item]).to_string()
}

fn command_news(msg: &Value) -> Vec<EngineEvent> {
    use crate::background::{State, Update};
    let p = &msg["params"];
    let Some(thread) = p["threadId"].as_str() else { return vec![] };
    if msg["method"] == "item/commandExecution/outputDelta" {
        return p["itemId"]
            .as_str()
            .zip(p["delta"].as_str())
            .map(|(item, text)| {
                EngineEvent::Background(Update::Output { id: command_key(thread, item), text: text.into(), replace: false })
            })
            .into_iter()
            .collect();
    }
    let item = &p["item"];
    if item["type"] != "commandExecution" || !matches!(msg["method"].as_str(), Some("item/started" | "item/completed")) {
        return vec![];
    }
    let Some(id) = item["id"].as_str() else { return vec![] };
    let id = command_key(thread, id);
    let mut events = Vec::new();
    if let Some(command) = item["command"].as_str() {
        // 0.162.1 reports an opaque PTY handle, not a PID; its native items have no published scoped stop request.
        events.push(EngineEvent::Background(Update::Start {
            id: id.clone(),
            command: command.into(),
            stoppable: false,
            turn_ended: false,
        }));
    }
    if let Some(text) = item["aggregatedOutput"].as_str() {
        events.push(EngineEvent::Background(Update::Output { id: id.clone(), text: text.into(), replace: true }));
    }
    if msg["method"] == "item/completed" && (item["exitCode"].is_number() || reported_terminal(item).is_none()) {
        let state = match item["status"].as_str() {
            Some("interrupted") => State::Stopped,
            Some("completed") if item["exitCode"].is_null() || item["exitCode"] == 0 => State::Done,
            _ => State::Failed,
        };
        events.push(EngineEvent::Background(Update::End { id, state }));
    }
    events
}

fn parse_notification(msg: &Value, state: &mut CodexTurnState) -> Vec<EngineEvent> {
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
            if kind == "plan" {
                return vec![];
            }
            if kind == "agentMessage" {
                return vec![EngineEvent::Break { final_answer: item["phase"] == "final_answer" }];
            }
            if kind == "contextCompaction" {
                return vec![];
            }
            if kind == "userMessage" {
                state.users += 1;
                // Our own tag echoed back says it for sure; without one, anything after the turn's first message does.
                let tag = item["clientId"].as_str().filter(|t| !t.is_empty()).map(String::from);
                return if tag.is_some() || state.users > 1 { vec![EngineEvent::Injected { tag }] } else { vec![] };
            }
            if NOT_TOOLS.contains(&kind) || kind == "collabAgentToolCall" {
                return vec![];
            }
            // A subagent the agent started reads as Claude Code's `Agent` call, so the screen shows one step for both CLIs.
            if kind == "subAgentActivity" {
                return match item["kind"].as_str() {
                    Some("started") => vec![EngineEvent::Tool {
                        id: s(&item["id"]),
                        name: SUBAGENT.into(),
                        input: json!({ "description": subagent_title(&s(&item["agentPath"])) }),
                        parent: None,
                    }],
                    _ => vec![],
                };
            }
            let input = [&item["command"], &item["query"], &item["changes"], &item["arguments"]]
                .into_iter()
                .find(|v| !v.is_null())
                .unwrap_or(item)
                .clone();
            let name = match (kind, item["tool"].as_str()) {
                ("dynamicToolCall", Some("run_command")) => "Bash",
                ("dynamicToolCall", Some(crate::helper::NAME)) => crate::helper::TOOL,
                ("dynamicToolCall", Some(crate::browser::NAME)) => crate::browser::TOOL,
                ("dynamicToolCall", Some(crate::sibling::NAME)) => crate::sibling::TOOL,
                ("dynamicToolCall", Some(crate::verify::NAME)) => crate::verify::TOOL,
                ("dynamicToolCall", Some(crate::map::NAME)) => crate::map::TOOL,
                ("dynamicToolCall", Some(name)) if let Some(tool) = crate::progress::tool_of(name) => tool,
                _ => kind,
            };
            vec![EngineEvent::Tool { id: s(&item["id"]), name: name.into(), input, parent: None }]
        }
        "item/completed" => {
            let item = &p["item"];
            let kind = item["type"].as_str().unwrap_or_default();
            let id = s(&item["id"]);
            // Codex compacted the thread, whoever asked (a `thread/compact/start` of Faber's or its own limit).
            if kind == "contextCompaction" {
                return vec![EngineEvent::Compacted { after: None }];
            }
            if kind == "plan" {
                let text = s(&item["text"]);
                if !text.trim().is_empty() {
                    state.plan = Some((id, text.clone()));
                    return vec![EngineEvent::Break { final_answer: true }, EngineEvent::Text(text)];
                }
                return vec![];
            }
            if kind == "agentMessage" {
                let text = s(&item["text"]);
                return if !text.is_empty() && !state.streamed.contains(&id) { vec![EngineEvent::Text(text)] } else { vec![] };
            }
            // A subagent's end settles its call with its report (`Subagents`); the agent's waits on it are bookkeeping.
            if NOT_TOOLS.contains(&kind) || kind == "collabAgentToolCall" || kind == "subAgentActivity" {
                return vec![];
            }
            let ok = item["status"] == "completed" && item["success"] != false && (item["exitCode"].is_null() || item["exitCode"] == 0);
            let text = if kind == "dynamicToolCall" {
                item["contentItems"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|item| item["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                item["aggregatedOutput"].as_str().or(item["status"].as_str()).unwrap_or_default().into()
            };
            vec![EngineEvent::ToolResult { id, ok, declined: item["status"] == "declined", text }]
        }
        "turn/started" => {
            state.plan = None;
            state.base = state.total;
            state.cost = 0.0;
            state.priced = false;
            state.price_incomplete = false;
            state.prompt = None;
            state.users = 0;
            state.subagents = Totals::default();
            state.subagents_cost = None;
            state.subagents_incomplete = false;
            vec![]
        }
        "thread/tokenUsage/updated" => {
            let before = state.total;
            let t = &p["tokenUsage"]["total"];
            let (Some(input), Some(cached), Some(output)) =
                (t["inputTokens"].as_u64(), t["cachedInputTokens"].as_u64(), t["outputTokens"].as_u64())
            else {
                state.price_incomplete = true;
                return vec![];
            };
            if before.is_some_and(|previous| {
                input < previous.input
                    || cached < previous.cached
                    || output < previous.output
                    || previous.written.zip(t["cacheWriteInputTokens"].as_u64()).is_some_and(|(old, new)| new < old)
            }) {
                state.price_incomplete = true;
                return vec![];
            }
            state.total = Some(Totals { input, cached, written: t["cacheWriteInputTokens"].as_u64(), output });
            if state.base.is_some()
                && let Some(before) = before
            {
                let total = state.total.expect("just set");
                let delta = total.since(before);
                if delta.input > 0 || delta.output > 0 {
                    let last = p["tokenUsage"]["last"]["inputTokens"].as_u64();
                    if delta.input > 0 {
                        state.prompt =
                            last.filter(|input| *input >= delta.input).or_else(|| (delta.input <= 272_000).then_some(delta.input));
                    } else if last.is_some() {
                        state.prompt = last;
                    }
                    let context = state.prompt.unwrap_or(0);
                    // Without a request-sized reading, standard short-context rates are only a lower bound.
                    state.price_incomplete |= context == 0 && (delta.input > 272_000 || delta.input == 0);
                    let cost = delta.written.and_then(|write| {
                        crate::usage::api_price_for_prompt(&state.model, delta.input, delta.output, delta.cached, write, 0, context)
                    });
                    if let Some(cost) = cost {
                        state.cost += cost;
                        state.priced = true;
                    } else {
                        state.price_incomplete |= !state.model.is_empty();
                    }
                }
            }
            vec![]
        }
        "account/rateLimits/updated" => [&p["rateLimits"]["primary"], &p["rateLimits"]["secondary"]]
            .into_iter()
            .filter_map(|w| {
                Some(EngineEvent::Quota {
                    window: window_name(w["windowDurationMins"].as_i64()?),
                    used: w["usedPercent"].as_f64()? / 100.0,
                    resets_at: w["resetsAt"].as_f64()?,
                })
            })
            .chain(Some(codex_signals(msg)).filter(|s| !s.is_empty()).map(EngineEvent::Signals))
            .collect(),
        "account/updated" => Some(codex_account(msg)).filter(|s| !s.is_empty()).map(EngineEvent::Signals).into_iter().collect(),
        "turn/completed" => {
            let status = s(&p["turn"]["status"]);
            // The turn's usage is the thread's total now minus its total when the turn started. With no start to
            // measure from, the turn reports no usage (and says so) rather than the whole thread.
            let used = state.total.zip(state.base).map(|(total, base)| total.since(base));
            let u = used.unwrap_or_default();
            let sub = state.subagents;
            let cost = state.priced.then(|| state.cost + state.subagents_cost.unwrap_or(0.0));
            let approval = state.plan.take().filter(|_| status == "completed").map(|(id, _)| EngineEvent::Permission {
                request_id: format!("plan:{id}"),
                tool: "ExitPlanMode".into(),
                input: json!({}),
            });
            approval
                .into_iter()
                .chain([EngineEvent::Done {
                    ok: status == "completed",
                    reason: p["turn"]["error"]["message"].as_str().map(String::from).unwrap_or(status),
                    cost_total_usd: cost,
                    input_tokens: u.input + sub.input,
                    output_tokens: u.output + sub.output,
                    cache_read_tokens: u.cached + sub.cached,
                    cache_write_tokens: u.written.unwrap_or(0) + sub.written.unwrap_or(0),
                    cache_write_1h_tokens: 0,
                    incomplete: used.is_none() || u.written.is_none() || state.price_incomplete || state.subagents_incomplete,
                }])
                .collect()
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
    pub permissions: Permissions,
    pub cli: crate::cli::Cli,
    /// Where the process itself runs; each thread works in its own folder.
    pub cwd: PathBuf,
    pub area: Area,
    pub user_hooks: bool,
    pub windows: bool,
    pub spawn: Spawner,
    /// What Faber's tool store hands over (`crate::tools`), for a project's server outside Isolated: its skill root and servers.
    pub tools: crate::tools::Delivery,
}

/// The settings that go into the thread's `config` (R140): web search always, and the two Codex-only choices when the person
/// made one. Only the person's own Chat and project sessions get them: an internal question never does.
fn apply_tuning(params: &mut Map<String, Value>, tuning: &Tuning) {
    let web = match tuning.web {
        WebSearch::Off => "disabled",
        WebSearch::Cached => "cached",
        WebSearch::Live => "live",
    };
    let mut config = json!({ "web_search": web });
    match tuning.summary {
        ReasoningSummary::Default => {}
        ReasoningSummary::Auto => config["model_reasoning_summary"] = "auto".into(),
        ReasoningSummary::Concise => config["model_reasoning_summary"] = "concise".into(),
        ReasoningSummary::Detailed => config["model_reasoning_summary"] = "detailed".into(),
        ReasoningSummary::Hidden => config["model_reasoning_summary"] = "none".into(),
    }
    match tuning.reply_style {
        ReplyStyle::Default | ReplyStyle::Own => {}
        ReplyStyle::Short => config["model_verbosity"] = "low".into(),
        ReplyStyle::Explained => config["model_verbosity"] = "high".into(),
    }
    params.insert("config".into(), config);
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
    /// How the user wants to be called; see `ClaudeLaunch::user_name`.
    pub user_name: Option<String>,
    /// Fixed research instruction alongside the ordinary persona.
    pub research: Option<crate::persona::Research>,
    /// The person's Chat group words, passed unchanged beside Faber's persona.
    pub group_instructions: Option<String>,
    pub permissions: Permissions,
    /// Reset the app-server's sticky Plan collaboration mode once after resuming a thread approved to run.
    pub leave_plan: bool,
    pub tuning: Tuning,
    /// Faber's helper tool (R161), as for `ClaudeLaunch::helper`.
    pub helper: crate::helper::Helper,
    /// Faber's own browser (R161 step 7), as for `ClaudeLaunch::browse`.
    pub browse: bool,
    /// Faber's conversation tool, as for `ClaudeLaunch::conversations`.
    pub conversations: bool,
    /// Faber's checks, as for `ClaudeLaunch::verify`.
    pub verify: bool,
    /// The plan, the status and the closing box, as for `ClaudeLaunch::progress`.
    pub progress: bool,
}

enum ServerCommand {
    Open { key: u64, thread: CodexThread, sink: EventSink },
    Session(u64, SessionCommand),
    Tools(crate::tools::Delivery),
    Retire,
}

// What an answer from Codex is for.
enum Waiting {
    Initialize,
    Thread(u64),
    Other(u64, &'static str),
    /// A `turn/steer`; its tag tells the engine which message it was.
    Steer(u64, String),
    NativeStop(u64, String, String, String),
    NativeGone(u64, String, String, String, usize),
}

fn skill_commands(result: &Value, cwd: &std::path::Path) -> Option<Vec<crate::model::Command>> {
    let entry = result["data"].as_array()?.iter().find(|entry| entry["cwd"].as_str().is_some_and(|s| std::path::Path::new(s) == cwd))?;
    let skills = entry["skills"].as_array()?;
    Some(
        skills
            .iter()
            .filter_map(|skill| {
                if skill["enabled"].as_bool() != Some(true) {
                    return None;
                }
                let name = skill["name"].as_str().filter(|name| !name.is_empty())?;
                let note = skill["shortDescription"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
                    .or_else(|| skill["description"].as_str().map(|s| s.split_inclusive(['.', '!', '?']).next().unwrap_or(s)))
                    .map(|s| s.trim().chars().take(120).collect::<String>())
                    .filter(|s| !s.is_empty());
                Some(crate::model::Command { name: name.into(), skill: true, note })
            })
            .collect(),
    )
}

/// A conversation's side of the shared process.
struct Thread {
    early_completion: Vec<Value>,
    sink: EventSink,
    params: CodexThread,
    id: Option<String>,
    turn: Option<String>,
    queued: Vec<UserTurn>,
    /// A compaction asked for before the thread was open: it goes out once it is.
    compact: bool,
    state: CodexTurnState,
    approvals: HashMap<String, Value>,
    /// Helper calls (R161) waiting for their report: the tool call id to the id of Codex's request.
    helpers: HashMap<String, Value>,
    watch: proc::TurnWatch,
    interrupt_pending: bool,
    blocked: bool,
    closing: bool,
    stop_children: bool,
    commands: HashMap<String, (Option<tokio::sync::oneshot::Sender<()>>, tokio::task::JoinHandle<()>)>,
    unconfirmed_commands: HashMap<String, proc::OwnedTree>,
    owns_server: bool,
    server_writers: bool,
    native_commands: NativeCommands,
    stop_reported: bool,
}

type CommandNotice = (u64, Value, proc::CommandOutput);
fn refresh_native(threads: &mut HashMap<u64, Thread>, key: u64, server: Option<&proc::OwnedTree>, no_process: bool) -> Vec<u64> {
    let mut clear = no_process;
    if !no_process && let Some(server) = server {
        let original_empty = server.descendants().is_ok_and(|children| children.is_empty());
        #[cfg(unix)]
        let outside_empty = server.external_descendants().is_ok_and(|children| children.is_empty());
        #[cfg(windows)]
        let outside_empty = true;
        clear = original_empty && outside_empty;
    }
    threads
        .iter_mut()
        .filter_map(|(&owner, t)| {
            // Unix terminals have separate groups. Their descendants cannot safely be attributed to one conversation.
            if (cfg!(unix) || owner == key) && t.server_writers == clear {
                t.server_writers = !clear;
                Some(owner)
            } else {
                None
            }
        })
        .collect()
}

impl Thread {
    fn native_pending(&self, key: u64, subagents: &Subagents) -> bool {
        !self.native_commands.items.is_empty()
            || self.native_commands.unknown()
            || subagents.0.values().any(|sub| sub.key == key && (!sub.commands.items.is_empty() || sub.commands.unknown()))
    }

    fn report_stop(&mut self, key: u64, subagents: &Subagents) {
        self.stop_reported = true;
        self.publish_writers(key, subagents);
        if self.blocked || self.server_writers || self.native_commands.unknown() || subagents.writers(key).1 {
            (self.sink)(EngineEvent::Error { message: "error.rewind.stopUnconfirmed".into(), fatal: false });
        }
    }

    fn command_finished(&mut self, id: &Value, confirmed: bool, unresolved: Option<proc::OwnedTree>) {
        if confirmed {
            self.commands.remove(&id.to_string());
            self.unconfirmed_commands.remove(&id.to_string());
        } else if let Some(tree) = unresolved {
            self.unconfirmed_commands.insert(id.to_string(), tree);
        } else {
            self.blocked = true;
        }
    }

    fn retry_commands(&mut self, key: u64, tx: &UnboundedSender<CommandNotice>) {
        for (command, tree) in &self.unconfirmed_commands {
            if self.commands.get(command).is_some_and(|(_, job)| !job.is_finished()) {
                continue;
            }
            let (Ok(id), tree, tx) = (serde_json::from_str::<Value>(command), tree.clone(), tx.clone()) else { continue };
            let retry = tokio::spawn(async move {
                let confirmed = tree.retire().await.is_ok();
                let _ = tx.send((
                    key,
                    id,
                    proc::CommandOutput {
                        success: false,
                        text: "error.cli.stopUnconfirmed".into(),
                        confirmed,
                        unresolved: (!confirmed).then_some(tree),
                    },
                ));
            });
            if let Some((_, job)) = self.commands.get_mut(command) {
                *job = retry;
            }
        }
    }

    fn drained(&self, key: u64, subagents: &Subagents) -> bool {
        let (helpers, unknown) = subagents.writers(key);
        self.turn.is_none()
            && !self.watch.active()
            && !self.interrupt_pending
            && !self.blocked
            && self.commands.is_empty()
            && self.native_commands.items.is_empty()
            && !self.native_commands.unknown()
            && helpers.is_empty()
            && !unknown
    }

    fn publish_writers(&self, key: u64, subagents: &Subagents) {
        let (mut active, mut unknown) = subagents.writers(key);
        for id in self.commands.keys() {
            active.push(format!("command:{id}"));
        }
        for id in self.native_commands.items.keys() {
            active.push(format!("native-command:{id}"));
        }
        if self.owns_server && self.server_writers {
            active.push("process-tree".into());
            unknown = true;
        }
        if let Some(turn) = &self.turn {
            active.push(format!("turn:{turn}"));
        }
        unknown |= self.blocked
            || !self.unconfirmed_commands.is_empty()
            || self.native_commands.unknown()
            || (self.watch.active() && self.turn.is_none());
        active.sort_unstable();
        (self.sink)(EngineEvent::Writers { active, unknown });
    }

    fn interrupt(&mut self) -> Option<(String, String)> {
        for (cancel, _) in self.commands.values_mut() {
            if let Some(cancel) = cancel.take() {
                let _ = cancel.send(());
            }
        }
        if self.blocked || self.watch.stopping() {
            return None;
        }
        if !self.watch.active() && self.turn.is_none() {
            return None;
        }
        self.watch.interrupt();
        let ids = self.id.clone().zip(self.turn.clone());
        // turn/start may be on the wire before either response supplies its id.
        self.interrupt_pending = ids.is_none();
        ids
    }
}

// Only server names are retained. Ignore config values (which can include private integration settings).
#[derive(Deserialize)]
struct ConfigReply {
    id: Option<String>,
    result: Option<ConfigResult>,
}
#[derive(Deserialize)]
struct ConfigResult {
    config: McpNames,
}
#[derive(Deserialize)]
struct McpNames {
    #[serde(default)]
    mcp_servers: HashMap<String, serde::de::IgnoredAny>,
}

/// `expectedTurnId` makes Codex refuse the message if that turn is no longer the active one, so a steer never lands in
/// a turn the person did not mean. `clientUserMessageId` comes back as the `clientId` of the user message item.
pub fn steer_params(thread: &str, expected_turn: &str, turn: &UserTurn, tag: &str) -> Value {
    let mut params = turn_params(thread, None, turn);
    params["expectedTurnId"] = expected_turn.into();
    params["clientUserMessageId"] = tag.into();
    params
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

/// Collaboration mode selects Codex's actual planning prompt; a read-only sandbox alone is not Plan mode.
fn start_params(thread: &str, settings: &mut CodexThread, model: &str, turn: &UserTurn) -> Value {
    let mut params = turn_params(thread, settings.effort.as_deref(), turn);
    let leave_plan = settings.leave_plan && !model.is_empty();
    if leave_plan {
        settings.leave_plan = false;
    }
    // The app-server keeps its current collaboration mode. Send Plan only when entering a Plan turn; ordinary turns omit it.
    // Its settings must use the model actually returned by thread/start or thread/resume, never a guessed model name.
    if (settings.permissions == Permissions::Plan || leave_plan) && settings.instructions.is_none() && !model.is_empty() {
        params["collaborationMode"] = json!({
            "mode": if settings.permissions == Permissions::Plan { "plan" } else { "default" },
            "settings": { "model": model,
                "reasoning_effort": settings.effort, "developer_instructions": null }
        });
    }
    // thread/start or thread/resume already sets this session's sandbox and approval policy from the same mode.
    params
}

/// The catalog's servers (`crate::tools`) beside the person's own, each under its own key so no other server is replaced.
pub fn add_servers(params: &mut Value, servers: &[crate::tools::Launch]) {
    for s in servers {
        params["config"][format!("mcp_servers.{}", s.name)] = json!({ "command": s.command, "args": s.args, "env": s.env });
    }
}

pub fn thread_params(area: Area, t: &CodexThread) -> (&'static str, Value) {
    let mut params = codex_thread(area, t.model.as_deref(), t.permissions);
    if let Some(instructions) = &t.instructions {
        params.insert("baseInstructions".into(), instructions.clone().into());
        // Legacy read-only still permits disk-wide reads. Internal questions keep the same credential/network
        // boundary as user turns, with their existing working folder read-only and no workspace writes.
        params.remove("sandbox");
        params.insert("config".into(), crate::permissions::sandbox_config(&t.cwd, false).into());
        // A quick question on the side (R250) too: on Windows the shell cannot start under that read-only profile ("MXC launcher:
        // failed to create the process security environment", Codex 0.162), so it answers from Faber's record alone.
        params["config"]["features.shell_tool"] = false.into();
        params.remove("dynamicTools");
        params.insert("approvalPolicy".into(), "never".into());
        // Nothing to resume later: it stays out of the user's Codex history.
        params.insert("ephemeral".into(), true.into());
    } else {
        // Developer instructions sit beside Codex's own, so a project keeps its default prompt.
        let mut words = persona(&PersonaInput { name: t.user_name.as_deref(), tuning: &t.tuning, research: t.research });
        if let Some(instructions) = &t.group_instructions {
            words.push_str("\n\nThe person's instructions for this chat group:\n");
            words.push_str(instructions);
        }
        params.insert("developerInstructions".into(), words.into());
        if t.permissions != Permissions::Isolated {
            apply_tuning(&mut params, &t.tuning);
        }
        // R161: the helper tool beside any tool the mode already gave; a helper itself leaves no history to resume.
        match t.helper {
            crate::helper::Helper::Offers => {
                let tools = params.entry("dynamicTools").or_insert_with(|| json!([]));
                if let Some(tools) = tools.as_array_mut() {
                    tools.push(crate::helper::codex_tool());
                }
            }
            crate::helper::Helper::Is => {
                params.insert("ephemeral".into(), true.into());
            }
            crate::helper::Helper::Off | crate::helper::Helper::Aside => {}
        }
        // R161 step 7: Faber's own browser, where it is offered.
        if t.browse {
            let tools = params.entry("dynamicTools").or_insert_with(|| json!([]));
            if let Some(tools) = tools.as_array_mut() {
                tools.push(crate::browser::codex_tool());
            }
        }
        if t.conversations {
            let tools = params.entry("dynamicTools").or_insert_with(|| json!([]));
            if let Some(tools) = tools.as_array_mut() {
                tools.push(crate::sibling::codex_tool());
            }
        }
        if t.verify {
            let tools = params.entry("dynamicTools").or_insert_with(|| json!([]));
            if let Some(tools) = tools.as_array_mut() {
                tools.push(crate::verify::codex_tool());
                tools.push(crate::map::codex_tool());
            }
        }
        if t.progress {
            let tools = params.entry("dynamicTools").or_insert_with(|| json!([]));
            if let Some(tools) = tools.as_array_mut() {
                tools.extend(crate::progress::codex_tools());
            }
        }
    }
    if t.permissions == Permissions::Isolated && t.instructions.is_none() {
        params.insert("config".into(), crate::permissions::sandbox_config(&t.cwd, true).into());
        params["config"]["features.shell_tool"] = false.into();
    }
    if (t.permissions == Permissions::Isolated || t.instructions.is_some()) && t.cwd.is_absolute() {
        params.insert("runtimeWorkspaceRoots".into(), json!([t.cwd]));
    }
    params.insert("cwd".into(), t.cwd.to_string_lossy().into_owned().into());
    let method = match &t.resume {
        Some(id) => {
            // Dynamic tools persist with the thread; this API only accepts their definition on start.
            params.remove("dynamicTools");
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
    alive: Arc<AtomicBool>,
}

impl CodexServer {
    /// The process starts right away, so a warmed-up conversation is already booting while the user types.
    pub fn start(opts: CodexServerOptions) -> CodexServer {
        let (tx, rx) = unbounded_channel();
        let args = codex_args(opts.area, opts.windows, opts.user_hooks, opts.permissions);
        let started = (opts.spawn)(&opts.cli.command, &args, &opts.cwd);
        let alive = Arc::new(AtomicBool::new(true));
        tokio::spawn(serve(started, opts, rx, alive.clone()));
        CodexServer { tx, next: Arc::new(AtomicU64::new(1)), alive }
    }

    /// False once the process has stopped; the engine then starts a new one.
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire) && !self.tx.is_closed()
    }

    /// Opens a conversation as a thread. The session behaves as if it had the process to itself.
    pub fn open(&self, thread: CodexThread, sink: EventSink) -> Session {
        let key = self.next.fetch_add(1, Ordering::Relaxed);
        if let Err(SendError(ServerCommand::Open { sink, .. })) = self.tx.send(ServerCommand::Open { key, thread, sink }) {
            sink(EngineEvent::Error { message: "Codex stopped.".into(), fatal: true });
            sink(EngineEvent::WriterExit { confirmed: true });
        }
        // Straight into the server's queue, so commands from all conversations keep the order they were given in.
        let server = self.tx.clone();
        Session::from_fn(move |cmd| drop(server.send(ServerCommand::Session(key, cmd))))
    }

    /// The catalog's tools as the store sees them now (`crate::tools`): a store that could not be made clean takes them away
    /// from a server already running. The skill root is sent again only when it changes; servers go with each new thread.
    pub fn tools(&self, tools: crate::tools::Delivery) {
        let _ = self.tx.send(ServerCommand::Tools(tools));
    }

    /// Stops the process once its last conversation closes; new conversations go to a server with the new settings.
    pub fn retire(&self) {
        let _ = self.tx.send(ServerCommand::Retire);
    }
}

async fn serve(
    started: std::io::Result<proc::Proc>,
    opts: CodexServerOptions,
    mut rx: UnboundedReceiver<ServerCommand>,
    alive: Arc<AtomicBool>,
) {
    let area = opts.area;
    let mut p = match started {
        Ok(p) => p,
        Err(err) => {
            alive.store(false, Ordering::Release);
            // Every conversation already waiting hears why; the next one starts a fresh process.
            rx.close();
            while let Some(cmd) = rx.recv().await {
                if let ServerCommand::Open { sink, .. } = cmd {
                    sink(EngineEvent::Error { message: format!("Codex could not start: {err}"), fatal: true });
                    sink(EngineEvent::WriterExit { confirmed: true });
                }
            }
            return;
        }
    };
    let tail = proc::stderr_tail(p.stderr.take());
    let mut lines = BufReader::new(p.stdout).lines();
    let mut stdin = p.stdin;
    let mut child = p.child;
    let mut job = p.contained;
    let no_process = child.is_none() && job.is_none();
    let server_tree = job.as_ref().and_then(proc::Containment::owned_tree);
    let mut shutdown_confirmed = false;
    let mut threads: HashMap<u64, Thread> = HashMap::new();
    let mut by_id: HashMap<String, u64> = HashMap::new();
    let mut subagents = Subagents::default();
    // Opened before Codex finished starting, in order, so request ids stay predictable.
    let mut pending: Vec<u64> = Vec::new();
    let mut waiting: HashMap<u64, Waiting> = HashMap::new();
    let mut exposure_pending = HashSet::new();
    let mut confirmed = VecDeque::new();
    let mut next_id = 0u64;
    let (mut ready, mut retired, mut stopping) = (false, false, false);
    let mut tools = opts.tools.clone();
    let (command_tx, mut command_rx) = unbounded_channel::<CommandNotice>();

    macro_rules! request {
        ($method:expr, $params:expr, $kind:expr) => {{
            next_id += 1;
            waiting.insert(next_id, $kind);
            let _ = proc::write_line(&mut *stdin, &json!({ "id": next_id, "method": $method, "params": $params })).await;
        }};
    }
    macro_rules! stop {
        () => {{
            stopping = true;
            alive.store(false, Ordering::Release);
            shutdown_confirmed |= proc::stop(&mut *stdin, &mut child, &mut job).await;
        }};
    }
    macro_rules! refresh_writers {
        ($key:expr) => {{
            for key in refresh_native(&mut threads, $key, server_tree.as_ref(), no_process) {
                if let Some(t) = threads.get(&key) {
                    t.publish_writers(key, &subagents);
                }
            }
        }};
    }
    macro_rules! stop_native {
        ($key:expr) => {{
            let key = $key;
            refresh_writers!(key);
            if let Some(t) = threads.get_mut(&key) {
                t.retry_commands(key, &command_tx);
            }
            let mut commands = Vec::new();
            if let Some(t) = threads.get(&key) && let Some(id) = &t.id {
                commands.extend(t.native_commands.stops(id));
            }
            for (id, sub) in &subagents.0 {
                if sub.key == key { commands.extend(sub.commands.stops(id)); }
            }
            for (thread, item, process) in commands {
                if waiting.values().any(|w| matches!(w,
                    Waiting::NativeStop(k, t, i, p) | Waiting::NativeGone(k, t, i, p, _)
                    if *k == key && t == &thread && i == &item && p == &process)) { continue; }
                let params = json!({"threadId":thread,"processId":process});
                request!("thread/backgroundTerminals/terminate", params,
                    Waiting::NativeStop(key, thread, item, process));
            }
        }};
    }
    macro_rules! open_thread {
        ($key:expr, $t:expr) => {{
            if $t.params.permissions == Permissions::Isolated || $t.params.instructions.is_some() {
                exposure_pending.insert($key);
                if proc::write_line(&mut *stdin, &json!({"id":format!("faber-config-{}", $key), "method":"config/read",
                    "params":{"includeLayers":false,"cwd":$t.params.cwd}})).await.is_err() {
                    stop!();
                }
            } else {
                let (method, mut params) = thread_params(area, &$t.params);
                add_servers(&mut params, &tools.servers);
                request!(method, params, Waiting::Thread($key));
            }
        }};
    }

    let init = json!({ "clientInfo": { "name": "faber", "title": "Faber", "version": "0.0.0" }, "capabilities": { "experimentalApi": true, "requestAttestation": false } });
    request!("initialize", init, Waiting::Initialize);

    let mut native_poll_at = None;
    loop {
        let report: Vec<_> = threads
            .iter()
            .filter(|(key, t)| {
                !no_process
                    && t.stop_children
                    && !t.stop_reported
                    && t.turn.is_none()
                    && !t.watch.active()
                    && t.commands.is_empty()
                    && !waiting.values().any(|w| matches!(w, Waiting::NativeStop(k, ..) | Waiting::NativeGone(k, ..) if *k == **key))
            })
            .map(|(&key, _)| key)
            .collect();
        for key in report {
            refresh_writers!(key);
            if let Some(t) = threads.get_mut(&key) {
                t.report_stop(key, &subagents);
            }
        }
        let closed: Vec<u64> =
            threads.iter().filter(|(key, t)| no_process && t.closing && t.drained(**key, &subagents)).map(|(&key, _)| key).collect();
        for key in closed {
            subagents.forget(key);
            if let Some(t) = threads.remove(&key) {
                (t.sink)(EngineEvent::WriterExit { confirmed: true });
                if let Some(id) = t.id {
                    // A replacement may already have resumed this same native thread.
                    if by_id.get(&id) == Some(&key) {
                        by_id.remove(&id);
                    }
                }
            }
        }
        if !stopping
            && ((retired && threads.is_empty())
                || (!threads.is_empty()
                    && threads.iter().all(|(&key, t)| {
                        t.closing
                            && (t.stop_children || (t.drained(key, &subagents) && !t.server_writers))
                            && !t.native_pending(key, &subagents)
                            && ((!t.watch.active() && t.turn.is_none() && !t.interrupt_pending) || (!no_process && t.blocked))
                    })))
        {
            stop!();
        }
        let deadline = threads.values().filter_map(|t| t.watch.deadline()).min();
        let idle_native: Vec<_> = threads
            .iter()
            .filter(|(_, t)| !no_process && t.turn.is_none() && !t.watch.active() && t.server_writers)
            .map(|(&key, _)| key)
            .collect();
        if idle_native.is_empty() {
            native_poll_at = None;
        } else {
            native_poll_at.get_or_insert_with(|| tokio::time::Instant::now() + Duration::from_secs(1));
        }
        tokio::select! {
            () = proc::wait_deadline(native_poll_at) => {
                for key in idle_native.into_iter().take(if cfg!(unix) { 1 } else { usize::MAX }) {
                    refresh_writers!(key);
                    if let Some(t) = threads.get(&key) { t.publish_writers(key, &subagents); }
                }
                native_poll_at = None;
            }
            Some((key, id, result)) = command_rx.recv() => {
                let retry = threads.get(&key).is_some_and(|t| t.unconfirmed_commands.contains_key(&id.to_string()));
                if let Some(t) = threads.get_mut(&key) {
                    t.command_finished(&id, result.confirmed, result.unresolved);
                    t.publish_writers(key, &subagents);
                    if !result.confirmed { (t.sink)(EngineEvent::Error { message: "error.rewind.stopUnconfirmed".into(), fatal: false }); }
                }
                if !retry {
                    let _ = proc::write_line(&mut *stdin, &json!({"id":id,"result":{"success":result.success,"contentItems":[{"type":"inputText","text":result.text}]}})).await;
                }
            }
            () = proc::wait_deadline(deadline) => {
                let expired: Vec<u64> = threads.iter().filter(|(_, t)| t.watch.deadline().is_some_and(|d| d <= tokio::time::Instant::now())).map(|(&k, _)| k).collect();
                for key in expired {
                    let Some(t) = threads.get_mut(&key) else { continue };
                    if t.watch.stopping() || t.turn.is_none() {
                        // A shared process cannot be killed for one task. Retain its identity until completion/recovery.
                        t.blocked = true;
                        t.watch.finish();
                        t.publish_writers(key, &subagents);
                        (t.sink)(EngineEvent::Error { message: "error.cli.stopUnconfirmed".into(), fatal: true });
                    } else if let Some((thread, turn)) = t.id.clone().zip(t.turn.clone()) {
                        t.watch.interrupt();
                        request!("turn/interrupt", json!({ "threadId": thread, "turnId": turn }), Waiting::Other(key, "turn/interrupt"));
                    }
                }
            }
            line = async {
                if let Some(msg) = confirmed.pop_front() { Ok(Some(Value::to_string(&msg))) }
                else { lines.next_line().await }
            } => {
                let Ok(Some(line)) = line else {
                    alive.store(false, Ordering::Release);
                    shutdown_confirmed |= proc::stop(&mut *stdin, &mut child, &mut job).await;
                    if !stopping {
                        let code = tokio::time::timeout(Duration::from_secs(1), proc::exit_code(&mut child))
                            .await.unwrap_or_else(|_| "unknown".into());
                        let message = format!("Codex stopped (exit {code}). {}", tail.lock().unwrap()).trim().to_string();
                        for t in threads.values() {
                            (t.sink)(EngineEvent::Writers { active: vec![], unknown: true });
                            (t.sink)(EngineEvent::Error { message: message.clone(), fatal: true });
                        }
                    }
                    break;
                };
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(reply) = serde_json::from_str::<ConfigReply>(&line)
                    && let Some(key) = reply.id.as_deref().and_then(|id| id.strip_prefix("faber-config-")).and_then(|key| key.parse::<u64>().ok())
                {
                    if !exposure_pending.remove(&key) { continue; }
                    let Some(t) = threads.get(&key) else { continue };
                    let Some(result) = reply.result else {
                        (t.sink)(EngineEvent::Error { message: "error.projectSandbox".into(), fatal: true });
                        continue;
                    };
                    let (method, mut params) = thread_params(area, &t.params);
                    let disabled: Map<String, Value> = result.config.mcp_servers.into_keys().map(|name| (name, json!({"enabled":false}))).collect();
                    params["config"]["mcp_servers"] = disabled.into();
                    request!(method, params, Waiting::Thread(key));
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
                if let Some(key) = msg["id"].as_str().and_then(|id| id.strip_prefix("faber-commands-")).and_then(|key| key.parse::<u64>().ok()) {
                    if let Some(t) = threads.get(&key) && let Some(commands) = skill_commands(&msg["result"], &t.params.cwd) {
                        (t.sink)(EngineEvent::Commands(commands));
                    }
                    continue;
                }
                match (method, msg.get("id")) {
                    // An answer to one of Faber's requests.
                    (None, Some(id)) => {
                        let Some(kind) = id.as_u64().and_then(|id| waiting.remove(&id)) else { continue };
                        if let Some(error) = msg.get("error") {
                            if matches!(kind, Waiting::Other(_, SUBAGENT_STOP)) {
                                continue;
                            }
                            // The turn ended, or cannot take a message: not an error of the conversation. The engine keeps the
                            // message for the next turn.
                            if let Waiting::Steer(key, tag) = kind {
                                if let Some(t) = threads.get(&key) {
                                    (t.sink)(EngineEvent::SteerFailed { tag });
                                }
                                continue;
                            }
                            let detail = error["message"].as_str().map(String::from).unwrap_or_else(|| error.to_string());
                            let (keys, what, fatal): (Vec<u64>, &str, bool) = match kind {
                                Waiting::Initialize => (threads.keys().copied().collect(), "initialize", true),
                                Waiting::Thread(key) => (vec![key], "thread", true),
                                Waiting::Other(key, m) => (vec![key], m, false),
                                Waiting::Steer(..) => unreachable!("handled above"),
                                Waiting::NativeStop(key, ..) | Waiting::NativeGone(key, ..) => (vec![key], "thread/backgroundTerminals/terminate", false),
                            };
                            for t in keys.iter().filter_map(|k| threads.get(k)) {
                                if matches!(kind, Waiting::NativeStop(..) | Waiting::NativeGone(..)) {
                                    if no_process { (t.sink)(EngineEvent::Error { message: "error.rewind.stopUnconfirmed".into(), fatal: false }); }
                                } else {
                                    (t.sink)(EngineEvent::Error { message: format!("Codex {what} failed: {detail}"), fatal });
                                }
                            }
                            if let Waiting::Other(key,"turn/start" | "thread/compact/start") = kind && let Some(t) = threads.get_mut(&key) {
                                t.watch.finish();
                                t.interrupt_pending = false;
                                t.turn = None;
                                t.early_completion.clear();
                                t.publish_writers(key, &subagents);
                            }
                            continue;
                        }
                        match kind {
                            Waiting::NativeStop(key, thread, item, process) => {
                                // A cancellation acknowledgement alone does not prove the writer has exited.
                                let params = json!({"threadId":thread});
                                request!("thread/backgroundTerminals/list", params,
                                    Waiting::NativeGone(key, thread, item, process, 100));
                            }
                            Waiting::NativeGone(key, thread, item, process, pages) => {
                                let result = &msg["result"];
                                let Some(data) = result["data"].as_array() else { continue; };
                                if data.iter().any(|entry| reported_terminal(entry).is_none())
                                    || !(result["nextCursor"].is_null() || result["nextCursor"].is_string()) { continue; }
                                if data.iter().any(|entry| reported_terminal(entry).as_deref() == Some(&process)) { continue; }
                                if let Some(cursor) = result["nextCursor"].as_str() {
                                    if pages > 0 {
                                        let params = json!({"threadId":thread,"cursor":cursor});
                                        request!("thread/backgroundTerminals/list", params,
                                            Waiting::NativeGone(key, thread, item, process, pages - 1));
                                    }
                                    continue;
                                }
                                refresh_writers!(key);
                                if let Some(t) = threads.get_mut(&key) {
                                    if t.id.as_deref() == Some(&thread) { t.native_commands.confirm_gone(&item, &process); }
                                    else if let Some(sub) = subagents.0.get_mut(&thread).filter(|sub| sub.key == key) {
                                        sub.commands.confirm_gone(&item, &process);
                                    }
                                    t.publish_writers(key, &subagents);
                                }
                            }
                            Waiting::Initialize => {
                                let _ = proc::write_line(&mut *stdin, &json!({ "method": "initialized" })).await;
                                // The catalog's skills (`crate::tools`), before any thread opens; its answer needs nothing.
                                if let Some(root) = &tools.skills {
                                    let set = json!({ "id": "faber-skills", "method": "skills/extraRoots/set", "params": { "extraRoots": [root] } });
                                    let _ = proc::write_line(&mut *stdin, &set).await;
                                }
                                ready = true;
                                for key in std::mem::take(&mut pending) {
                                    let Some(t) = threads.get(&key) else { continue };
                                    open_thread!(key, t);
                                }
                            }
                            Waiting::Thread(key) => {
                                let Some(t) = threads.get_mut(&key) else { continue };
                                let thread = &msg["result"]["thread"];
                                let id = s(&thread["id"]);
                                let actual_model = msg["result"]["model"].as_str().or_else(|| thread["model"].as_str()).unwrap_or_default().to_string();
                                let shown = if actual_model.is_empty() { "codex".into() } else { actual_model.clone() };
                                t.state.model = actual_model;
                                (t.sink)(EngineEvent::Started { session_id: id.clone(), model: shown });
                                by_id.insert(id.clone(), key);
                                t.id = Some(id.clone());
                                let _ = proc::write_line(&mut *stdin, &json!({"id":format!("faber-commands-{key}"), "method":"skills/list",
                                    "params":{"cwds":[t.params.cwd]}})).await;
                                if std::mem::take(&mut t.compact) {
                                    t.watch.begin();
                                    t.publish_writers(key, &subagents);
                                    t.state.base = t.state.total;
                                    request!("thread/compact/start", json!({ "threadId": id }), Waiting::Other(key, "thread/compact/start"));
                                }
                                for turn in std::mem::take(&mut t.queued) {
                                    t.watch.begin();
                                    t.publish_writers(key, &subagents);
                                    t.state.base = t.state.total;
                                    let params = start_params(&id, &mut t.params, &t.state.model, &turn);
                                    request!("turn/start", params, Waiting::Other(key, "turn/start"));
                                }
                            }
                            Waiting::Other(key, "turn/start") => {
                                if let Some(t) = threads.get_mut(&key) {
                                    t.turn = msg["result"]["turn"]["id"].as_str().map(String::from).or(t.turn.take());
                                    if t.interrupt_pending && let Some((thread, turn)) = t.id.clone().zip(t.turn.clone()) {
                                        t.interrupt_pending = false;
                                        request!("turn/interrupt", json!({ "threadId": thread, "turnId": turn }), Waiting::Other(key, "turn/interrupt"));
                                    }
                                    t.publish_writers(key, &subagents);
                                    if let Some(completed) = std::mem::take(&mut t.early_completion).into_iter().find(|msg|msg["params"]["turn"]["id"].as_str() == t.turn.as_deref()) {
                                        confirmed.push_back(completed);
                                    }
                                }
                            }
                            // Accepted: the message is in the turn once Codex reports it as a user message (`Injected`).
                            Waiting::Steer(..) | Waiting::Other(..) => {}
                        }
                    }
                    // A request from Codex: approvals reach the user; anything else is declined politely.
                    (Some(method), Some(id)) => {
                        let top = params["threadId"].as_str().and_then(|t| by_id.get(t)).copied();
                        let thread = top.and_then(|k| threads.get_mut(&k));
                        if method == "item/tool/call" && params["tool"] == crate::helper::NAME {
                            // R161: the agent asked for a helper; the engine answers once the helper is done.
                            match thread.filter(|t| t.params.helper == crate::helper::Helper::Offers && !t.blocked && !t.closing) {
                                Some(t) => {
                                    let call = s(&params["callId"]);
                                    t.helpers.insert(call.clone(), id.clone());
                                    (t.sink)(EngineEvent::HelperCall { id: call, input: params["arguments"].clone() });
                                }
                                None => {
                                    let _ = proc::write_line(&mut *stdin, &json!({"id":id,"result":{"success":false,"contentItems":[{"type":"inputText","text":"No helper is available here."}]}})).await;
                                }
                            }
                            continue;
                        }
                        if method == "item/tool/call" && params["tool"] == crate::sibling::NAME {
                            // The agent starts a conversation of its own or reads one; the engine answers at once.
                            match thread.filter(|t| t.params.conversations && !t.blocked && !t.closing) {
                                Some(t) => {
                                    let call = s(&params["callId"]);
                                    t.helpers.insert(call.clone(), id.clone());
                                    (t.sink)(EngineEvent::ConversationCall { id: call, input: params["arguments"].clone() });
                                }
                                None => {
                                    let _ = proc::write_line(&mut *stdin, &json!({"id":id,"result":{"success":false,"contentItems":[{"type":"inputText","text":"Starting a conversation is not available here."}]}})).await;
                                }
                            }
                            continue;
                        }
                        if method == "item/tool/call" && params["tool"] == crate::map::NAME {
                            // The agent reads the project's map; the engine answers once it has read the folder.
                            match thread.filter(|t| t.params.verify && !t.blocked && !t.closing) {
                                Some(t) => {
                                    let call = s(&params["callId"]);
                                    t.helpers.insert(call.clone(), id.clone());
                                    (t.sink)(EngineEvent::MapCall { id: call, input: params["arguments"].clone() });
                                }
                                None => {
                                    let _ = proc::write_line(&mut *stdin, &json!({"id":id,"result":{"success":false,"contentItems":[{"type":"inputText","text":"Faber's project map is not available here."}]}})).await;
                                }
                            }
                            continue;
                        }
                        if method == "item/tool/call" && let Some(tool) = params["tool"].as_str().and_then(crate::progress::tool_of) {
                            // The plan, the status or the closing box: the engine writes it down and answers at once.
                            match thread.filter(|t| t.params.progress && !t.blocked && !t.closing) {
                                Some(t) => {
                                    let call = s(&params["callId"]);
                                    t.helpers.insert(call.clone(), id.clone());
                                    (t.sink)(EngineEvent::ProgressCall { id: call, tool, input: params["arguments"].clone() });
                                }
                                None => {
                                    let _ = proc::write_line(&mut *stdin, &json!({"id":id,"result":{"success":false,"contentItems":[{"type":"inputText","text":"This tool is not available here."}]}})).await;
                                }
                            }
                            continue;
                        }
                        if method == "item/tool/call" && params["tool"] == crate::verify::NAME {
                            // The agent asks Faber to run the project's checks; the engine answers once they ended.
                            match thread.filter(|t| t.params.verify && !t.blocked && !t.closing) {
                                Some(t) => {
                                    let call = s(&params["callId"]);
                                    t.helpers.insert(call.clone(), id.clone());
                                    (t.sink)(EngineEvent::VerifyCall { id: call, input: params["arguments"].clone() });
                                }
                                None => {
                                    let _ = proc::write_line(&mut *stdin, &json!({"id":id,"result":{"success":false,"contentItems":[{"type":"inputText","text":"Faber's checks are not available here."}]}})).await;
                                }
                            }
                            continue;
                        }
                        if method == "item/tool/call" && params["tool"] == crate::browser::NAME {
                            // R161 step 7: the agent opens a page in Faber's browser; the engine answers once it has loaded.
                            match thread.filter(|t| t.params.browse && !t.blocked && !t.closing) {
                                Some(t) => {
                                    let call = s(&params["callId"]);
                                    t.helpers.insert(call.clone(), id.clone());
                                    (t.sink)(EngineEvent::BrowseCall { id: call, input: params["arguments"].clone() });
                                }
                                None => {
                                    let _ = proc::write_line(&mut *stdin, &json!({"id":id,"result":{"success":false,"contentItems":[{"type":"inputText","text":"Faber's browser is not available here."}]}})).await;
                                }
                            }
                            continue;
                        }
                        if method == "item/tool/call" {
                            let mut accepted = false;
                            if let Some(t) = thread && t.params.permissions == Permissions::Isolated && t.params.instructions.is_none() && params["tool"] == "run_command" {
                                let input = &params["arguments"];
                                if let Some(argv) = crate::permissions::command_argv(&t.params.cwd, "Bash", input, Some(&opts.cli), &crate::cli::find_cli) {
                                    let key = top.expect("registered command thread");
                                    let (spawn, cwd, tx, id) = (opts.spawn.clone(), t.params.cwd.clone(), command_tx.clone(), id.clone());
                                    let command = id.to_string();
                                    let (cancel, cancelled) = tokio::sync::oneshot::channel();
                                    // The task cannot start until its writer is visible to the engine.
                                    let (admit, admitted) = tokio::sync::oneshot::channel();
                                    let job = tokio::spawn(async move {
                                        if admitted.await.is_err() { return; }
                                        let result = match spawn(&argv[0], &argv[1..], &cwd) {
                                            Ok(p) => proc::command_output(p, cancelled).await,
                                            Err(_) => proc::CommandOutput { success: false, text: "error.projectSandbox".into(), confirmed: true, unresolved: None },
                                        };
                                        let _ = tx.send((key, id, result));
                                    });
                                    t.commands.insert(command, (Some(cancel), job));
                                    t.publish_writers(key, &subagents);
                                    let _ = admit.send(());
                                    accepted = true;
                                }
                            }
                            if !accepted {
                                let _ = proc::write_line(&mut *stdin, &json!({"id":id,"result":{"success":false,"contentItems":[{"type":"inputText","text":"The working-folder policy declined this command."}]}})).await;
                            }
                            continue;
                        }
                        // A subagent's approval is its conversation's: the person answers it on the same card, by the same rules.
                        let thread = match top {
                            Some(_) => thread,
                            // Only while the turn it serves is under way: a late request reaches no other reply, and is refused.
                            None => params["threadId"].as_str().and_then(|t| subagents.owner(t)).and_then(|(k, turn)| {
                                threads.get_mut(&k).filter(|t| turn.is_some() && t.turn == turn)
                            }),
                        };
                        match (method.ends_with("requestApproval"), thread) {
                            // Full access never shows a card: Codex does not ask under the "never" policy, and if it still does, the
                            // engine answers (yes, unless the request touches a credential folder) with the mode in force at that
                            // moment, so a switch to Ask is never raced. Ask shows the card.
                            (true, Some(t))
                                if t.params.instructions.is_none()
                                    && (t.params.permissions.guarded()
                                        || (t.params.permissions == Permissions::FullAccess
                                            && ["item/commandExecution/requestApproval", "item/fileChange/requestApproval"].contains(&method))) =>
                            {
                                if t.blocked || t.closing {
                                    let _ = proc::write_line(&mut *stdin, &json!({ "id": id, "result": { "decision": "decline" } })).await;
                                    continue;
                                }
                                let key = match id { Value::String(s) => s.clone(), other => other.to_string() };
                                t.approvals.insert(key.clone(), id.clone());
                                let tool = params["kind"].as_str().unwrap_or(method).to_string();
                                let input = [&params["command"], &params["changes"]].into_iter().find(|v| !v.is_null()).unwrap_or(params).clone();
                                // The request carries the folder the command runs in; `auth.json` alone says nothing without it. A
                                // refusal is always safe, so it is written here instead of racing the engine's mode.
                                if crate::permissions::touches_credentials(&input, params["cwd"].as_str(), &|name| std::env::var(name).ok()) {
                                    t.approvals.remove(&key);
                                    let _ = proc::write_line(&mut *stdin, &json!({ "id": id, "result": { "decision": "decline" } })).await;
                                    continue;
                                }
                                (t.sink)(EngineEvent::Permission { request_id: key, tool, input });
                            }
                            // No live conversation or approvals disabled: fail closed without a card.
                            (true, _) => {
                                let _ = proc::write_line(&mut *stdin, &json!({ "id": id, "result": { "decision": "decline" } })).await;
                            }
                            (false, _) => {
                                let error = json!({ "id": id, "error": { "code": -32601, "message": format!("Faber does not handle {method} yet") } });
                                let _ = proc::write_line(&mut *stdin, &error).await;
                            }
                        }
                    }
                    _ => {
                        // Observe the parent chain before a completion can forget its terminal.
                        #[cfg(unix)]
                        if params["item"]["type"] == "commandExecution" { refresh_writers!(0); }
                        // A subagent's own thread speaks for the conversation that started it.
                        if let Some(ChildNews { key, turn, events, used }) = subagents.child(&msg) {
                            if method == Some("turn/completed") { refresh_writers!(key); }
                            if let Some(t) = threads.get_mut(&key) {
                                t.publish_writers(key, &subagents);
                                if t.stop_children && method == Some("turn/started") {
                                    for (thread, turn) in subagents.at_work(key) {
                                        request!("turn/interrupt", json!({ "threadId": thread, "turnId": turn }), Waiting::Other(key, SUBAGENT_STOP));
                                    }
                                }
                                // A subagent belongs to the reply it serves: once that reply is over, what it still does or uses
                                // reaches no other reply.
                                let current = turn.is_some() && turn == t.turn;
                                if let Some(done) = used && current { t.state.add_subagent(&done); }
                                for mut event in events {
                                    // A child's original root turn can have ended even while another turn runs.
                                    if let EngineEvent::Background(crate::background::Update::Start { turn_ended, .. }) = &mut event {
                                        *turn_ended = !current;
                                    }
                                    if matches!(event, EngineEvent::Background(_)) || (current && !t.blocked) { (t.sink)(event); }
                                }
                            }
                            if threads.get(&key).is_some_and(|t| t.stop_children) { stop_native!(key); }
                            continue;
                        }
                        let thread_id = params["threadId"].as_str().or_else(|| params["thread"]["id"].as_str());
                        // Resuming a thread changes its session key, but an earlier command still belongs to its original owner.
                        let retained = thread_id.and_then(|id| threads.iter().find_map(|(&key, t)| {
                            (t.closing && t.id.as_deref() == Some(id)
                                && params["item"]["type"] == "commandExecution"
                                && params["item"]["id"].as_str().is_some_and(|item| t.native_commands.items.contains_key(item)))
                            .then_some(key)
                        }));
                        let key = retained.or_else(|| thread_id.and_then(|t| by_id.get(t)).copied());
                        if method == Some("turn/completed") && let Some(key) = key {
                            refresh_writers!(key);
                        }
                        match key.and_then(|k| threads.get_mut(&k).map(|t| (k, t))) {
                            Some((key, t)) => {
                                t.native_commands.track(&msg);
                                if method == Some("turn/completed") && params["turn"]["id"].as_str() != t.turn.as_deref() {
                                    if t.turn.is_none() && waiting.values().any(|w| matches!(w,Waiting::Other(key,"turn/start") if Some(key) == thread_id.and_then(|id|by_id.get(id)))) {
                                        if t.early_completion.len() < 2 { t.early_completion.push(msg); }
                                        else {
                                            t.blocked = true;
                                            t.watch.finish();
                                            t.publish_writers(key, &subagents);
                                            (t.sink)(EngineEvent::Error { message:"error.cli.stopUnconfirmed".into(),fatal:true });
                                        }
                                    }
                                    continue;
                                }
                                if method == Some("turn/started") {
                                    t.turn = params["turn"]["id"].as_str().map(String::from);
                                    if t.interrupt_pending && let Some((thread, turn)) = t.id.clone().zip(t.turn.clone()) {
                                        t.interrupt_pending = false;
                                        request!("turn/interrupt", json!({ "threadId": thread, "turnId": turn }), Waiting::Other(key, "turn/interrupt"));
                                    }
                                    t.publish_writers(key, &subagents);
                                    if !t.blocked {
                                        (t.sink)(EngineEvent::TurnStarted);
                                    }
                                }
                                if method == Some("turn/completed") {
                                    (t.sink)(EngineEvent::Cursor { entry: None, turn: params["turn"]["id"].as_str().map(String::from) });
                                    // A subagent still at work has not reported what it used: the reply's usage says so.
                                    if t.turn.as_deref().is_some_and(|turn| subagents.running(key, turn)) {
                                        t.state.subagent_running();
                                    }
                                    // Only a subagent can still be waiting on an approval once the turn is over; its card goes with
                                    // the reply, so the request is refused instead of hanging unseen.
                                    for (_, id) in t.approvals.drain() {
                                        let _ = proc::write_line(&mut *stdin, &json!({ "id": id, "result": { "decision": "decline" } })).await;
                                    }
                                    t.turn = None;
                                    t.watch.finish();
                                    t.interrupt_pending = false;
                                }
                                let tracked = subagents.track(key, params["turnId"].as_str(), &msg);
                                if method != Some("turn/completed") { t.publish_writers(key, &subagents); }
                                for mut event in parse_codex_notification(&msg, &mut t.state).into_iter().chain(tracked) {
                                    if let EngineEvent::Background(crate::background::Update::Start { turn_ended, .. }) = &mut event {
                                        *turn_ended = params["turnId"].as_str().is_some_and(|turn| t.turn.as_deref() != Some(turn));
                                    }
                                    if !t.blocked { (t.sink)(event); }
                                }
                                t.publish_writers(key, &subagents);
                                if method == Some("turn/completed") && !t.blocked && !t.closing && let Some(id) = t.id.clone() {
                                    for turn in std::mem::take(&mut t.queued) {
                                        t.watch.begin();
                                        t.stop_children = false;
                                        t.publish_writers(key, &subagents);
                                        t.state.base = t.state.total;
                                        request!("turn/start", start_params(&id, &mut t.params, &t.state.model, &turn), Waiting::Other(*by_id.get(&id).expect("registered thread"), "turn/start"));
                                    }
                                }
                                if threads.get(&key).is_some_and(|t| t.stop_children) { stop_native!(key); }
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
                    let state = CodexTurnState::new(thread.resume.is_some());
                    let t = Thread { sink, params: thread, id: None, turn: None, queued: vec![], compact: false, state, approvals: HashMap::new(), helpers: HashMap::new(), watch: proc::TurnWatch::default(), interrupt_pending: false, blocked: false, closing: false, stop_children: false, early_completion: vec![], commands: HashMap::new(), unconfirmed_commands: HashMap::new(), owns_server: !no_process, server_writers: false, native_commands: Default::default(), stop_reported: false };
                    if ready {
                        open_thread!(key, &t);
                    } else {
                        pending.push(key);
                    }
                    threads.insert(key, t);
                    #[cfg(unix)]
                    refresh_writers!(key);
                }
                Some(ServerCommand::Session(key, cmd)) => match cmd {
                    SessionCommand::StopBackground { ack, .. } => { let _ = ack.send(Delivery::Unknown); }
                    SessionCommand::Send(turn) => {
                        let Some(t) = threads.get_mut(&key) else { continue };
                        if t.blocked || t.closing { continue; }
                        if t.watch.stopping() { t.queued.push(turn); continue; }
                        t.stop_children = false;
                        match t.id.clone() {
                            Some(id) => {
                                t.watch.begin();
                                t.publish_writers(key, &subagents);
                                t.state.base = t.state.total;
                                request!("turn/start", start_params(&id, &mut t.params, &t.state.model, &turn), Waiting::Other(key, "turn/start"));
                            }
                            None => t.queued.push(turn),
                        }
                    }
                    // Compaction runs as a turn of its own (`turn/started` ... `turn/completed`), so the turn machinery follows it.
                    SessionCommand::Compact => {
                        let Some(t) = threads.get_mut(&key) else { continue };
                        if t.blocked || t.closing || t.watch.stopping() { continue; }
                        match t.id.clone() {
                            Some(id) => {
                                t.watch.begin();
                                t.publish_writers(key, &subagents);
                                t.state.base = t.state.total;
                                request!("thread/compact/start", json!({ "threadId": id }), Waiting::Other(key, "thread/compact/start"));
                            }
                            // A thread still opening (a conversation reopened only to compact): once it is open.
                            None => t.compact = true,
                        }
                    }
                    // Codex reports only the whole: there are no parts to count.
                    SessionCommand::ContextUsage => {}
                    SessionCommand::Steer { turn, tag } => {
                        let Some(t) = threads.get_mut(&key) else { continue };
                        match (t.id.clone(), t.turn.clone()) {
                            (Some(id), Some(turn_id)) => {
                                let params = steer_params(&id, &turn_id, &turn, &tag);
                                request!("turn/steer", params, Waiting::Steer(key, tag));
                            }
                            // No turn to name yet, so Codex could not tell which one this is for.
                            _ => (t.sink)(EngineEvent::SteerFailed { tag }),
                        }
                    }
                    SessionCommand::Answer { request_id, allow, ack, .. } => {
                        // The approval stays pending until the write worked, so a failed write can be answered again.
                        let delivery = match threads.get(&key).and_then(|t| t.approvals.get(&request_id)).cloned() {
                            None => Delivery::Unknown,
                            Some(id) if threads.get(&key).is_some_and(|t| !t.blocked && !t.closing) => {
                                let decision = if allow { "accept" } else { "decline" };
                                match proc::write_line(&mut *stdin, &json!({ "id": id, "result": { "decision": decision } })).await {
                                    Ok(()) => {
                                        if let Some(t) = threads.get_mut(&key) {
                                            t.approvals.remove(&request_id);
                                        }
                                        Delivery::Delivered
                                    }
                                    Err(e) => {
                                        // A half-written or unflushed line cannot be sent again: end the process, and the
                                        // end of its output tells the engine.
                                        if e.torn {
                                            shutdown_confirmed |= proc::stop(&mut *stdin, &mut child, &mut job).await;
                                        }
                                        Delivery::WriteFailed
                                    }
                                }
                            }
                            Some(_) => Delivery::Unknown,
                        };
                        let _ = ack.send(delivery);
                    }
                    SessionCommand::HelperReply { id, ok, text, pictures } => {
                        let Some(t) = threads.get_mut(&key) else { continue };
                        let Some(request) = t.helpers.remove(&id) else { continue };
                        let _ = proc::write_line(&mut *stdin, &json!({"id":request,"result":crate::helper::dynamic_result(ok, &text, &pictures)})).await;
                    }
                    SessionCommand::Interrupt => {
                        if let Some(t) = threads.get_mut(&key) {
                            let queued = !t.queued.is_empty();
                            t.queued.clear();
                            if queued && !t.blocked && !t.watch.stopping() && !t.watch.active() && t.turn.is_none() {
                                // No turn is on the wire while the thread opens: discard it without starting work.
                                (t.sink)(EngineEvent::Done { ok: false, reason: "interrupted".into(), cost_total_usd: Some(0.0),
                                    input_tokens: 0, output_tokens: 0, cache_read_tokens: 0, cache_write_tokens: 0,
                                    cache_write_1h_tokens: 0, incomplete: false });
                            }
                        }
                        // A compaction still waiting for its thread never starts; its end is said here, as Codex would.
                        if let Some(t) = threads.get_mut(&key) && std::mem::take(&mut t.compact) {
                            (t.sink)(EngineEvent::Done { ok: false, reason: "interrupted".into(), cost_total_usd: None, input_tokens: 0, output_tokens: 0, cache_read_tokens: 0, cache_write_tokens: 0, cache_write_1h_tokens: 0, incomplete: true });
                        }
                        let idle = threads.get(&key).is_some_and(|t| t.turn.is_none() && !t.watch.active());
                        let busy_peer = threads.iter().any(|(&owner, t)| owner != key && (!t.drained(owner, &subagents)
                            || t.server_writers || t.id.is_none() || !t.queued.is_empty()));
                        let native_pending = threads.get(&key).is_some_and(|t| t.native_pending(key, &subagents));
                        if !no_process && idle && !busy_peer && !native_pending {
                            // With no other writer, the owned server container is the smallest complete stop boundary.
                            alive.store(false, Ordering::Release);
                            shutdown_confirmed |= proc::stop(&mut *stdin, &mut child, &mut job).await;
                            break;
                        }
                        if let Some(t) = threads.get_mut(&key) { t.stop_children = true; t.stop_reported = false; }
                        if let Some((thread, turn)) = threads.get_mut(&key).and_then(Thread::interrupt) {
                            request!("turn/interrupt", json!({ "threadId": thread, "turnId": turn }), Waiting::Other(key, "turn/interrupt"));
                        }
                        // Stop reaches the conversation's subagents too; one that just ended answers with an error nobody needs.
                        for (thread, turn) in subagents.at_work(key) {
                            request!("turn/interrupt", json!({ "threadId": thread, "turnId": turn }), Waiting::Other(key, SUBAGENT_STOP));
                        }
                        stop_native!(key);
                    }
                    SessionCommand::Retire => {
                        if let Some(t) = threads.get_mut(&key) {
                            t.closing = true;
                            t.publish_writers(key, &subagents);
                        }
                    }
                    // Keep an active thread until completion acknowledges the stop; its peers keep running.
                    SessionCommand::Close => {
                        pending.retain(|k| *k != key);
                        // A subagent can outlive its reply; closing the conversation stops it either way.
                        for (thread, turn) in subagents.at_work(key) {
                            request!("turn/interrupt", json!({ "threadId": thread, "turnId": turn }), Waiting::Other(key, SUBAGENT_STOP));
                        }
                        if let Some(t) = threads.get_mut(&key) {
                            t.queued.clear();
                            t.compact = false;
                            t.closing = true;
                            t.stop_children = true;
                            t.stop_reported = false;
                            let interrupted = t.interrupt();
                            if let Some((id, turn)) = interrupted {
                                request!("turn/interrupt", json!({ "threadId": id, "turnId": turn }), Waiting::Other(key, "turn/interrupt"));
                            }
                            if (t.watch.active() || t.turn.is_some()) && !t.blocked && !t.watch.stopping() {
                                t.watch.interrupt();
                            }
                            t.publish_writers(key, &subagents);
                        }
                        stop_native!(key);
                    }
                },
                Some(ServerCommand::Tools(next)) => {
                    let changed = next.skills != tools.skills;
                    tools = next;
                    // Before `initialized`, the first set carries the new value by itself.
                    if changed && ready {
                        let set = json!({ "id": "faber-skills", "method": "skills/extraRoots/set", "params": { "extraRoots": tools.skills.iter().collect::<Vec<_>>() } });
                        let _ = proc::write_line(&mut *stdin, &set).await;
                    }
                }
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
    // Dynamic commands own separate containers; server exit alone cannot retire their writers.
    for t in threads.values_mut() {
        for (cancel, _) in t.commands.values_mut() {
            if let Some(cancel) = cancel.take() {
                let _ = cancel.send(());
            }
        }
    }
    let mut announced = HashSet::new();
    loop {
        #[cfg(unix)]
        refresh_writers!(0);
        let finished: Vec<_> = threads
            .iter()
            .filter(|(key, t)| {
                t.commands.is_empty()
                    // Unix also checks sampled descendants outside the original group after server exit.
                    && ((shutdown_confirmed && (cfg!(windows) || !t.server_writers))
                        || (no_process && !t.blocked && !t.native_pending(**key, &subagents)))
            })
            .map(|(&key, _)| key)
            .collect();
        for key in finished {
            if let Some(t) = threads.remove(&key) {
                (t.sink)(EngineEvent::WriterExit { confirmed: true });
            }
        }
        for (&key, t) in &threads {
            if announced.insert(key) {
                (t.sink)(EngineEvent::WriterExit { confirmed: false });
            }
        }
        if threads.is_empty() {
            break;
        }
        tokio::select! {
            () = tokio::time::sleep(Duration::from_secs(1)), if cfg!(unix) && shutdown_confirmed => {}
            Some((key, id, result)) = command_rx.recv() => {
                if let Some(t) = threads.get_mut(&key) {
                    t.command_finished(&id, result.confirmed, result.unresolved);
                    t.publish_writers(key, &subagents);
                }
            }
            cmd = rx.recv() => match cmd {
                Some(ServerCommand::Session(key, SessionCommand::Interrupt | SessionCommand::Close)) => {
                    if !shutdown_confirmed && job.is_some() {
                        shutdown_confirmed = proc::stop(&mut *stdin, &mut child, &mut job).await;
                    }
                    if let Some(t) = threads.get_mut(&key) { t.retry_commands(key, &command_tx); }
                }
                Some(ServerCommand::Open { sink, .. }) => {
                    sink(EngineEvent::Error { message: "error.driverStopped".into(), fatal: true });
                    sink(EngineEvent::WriterExit { confirmed: true });
                }
                None => break,
                _ => {}
            }
        }
    }
}

/// Asks a short-lived `codex app-server` for the account's quota windows and plan, so the first reading does not wait for a
/// turn. The answers are dressed as the notifications the server sends later and go through the same parser; the rest of
/// them (the account's email and other details) is dropped unread. A failed reply does not discard the other reply's evidence.
pub async fn read_codex_limits(bin: &str, cwd: &Path, windows: bool, spawn: Spawner, timeout: Duration) -> Vec<EngineEvent> {
    let args = codex_args(Area::Chat, windows, false, Permissions::Isolated);
    let Ok(p) = proc::spawn_background(&spawn, bin, &args, cwd) else { return vec![] };
    let mut stdin = p.stdin;
    let mut child = p.child;
    let mut job = p.contained;
    let mut lines = BufReader::new(p.stdout).lines();
    let mut events = vec![];
    let ask = async {
        let init = json!({ "id": 1, "method": "initialize", "params": { "clientInfo": { "name": "faber", "title": "Faber", "version": "0.0.0" } } });
        let _ = proc::write_line(&mut *stdin, &init).await;
        let mut answered = 0;
        while let Ok(Some(line)) = lines.next_line().await {
            let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
            match msg["id"].as_i64() {
                Some(1) => {
                    if !msg["error"].is_null() {
                        return;
                    }
                    let _ = proc::write_line(&mut *stdin, &json!({ "method": "initialized" })).await;
                    let _ = proc::write_line(&mut *stdin, &json!({ "id": 2, "method": "account/rateLimits/read" })).await;
                    let _ =
                        proc::write_line(&mut *stdin, &json!({ "id": 3, "method": "account/read", "params": { "refreshToken": false } }))
                            .await;
                }
                Some(2) if answered & 1 == 0 => {
                    if msg["error"].is_null() {
                        let note =
                            json!({ "method": "account/rateLimits/updated", "params": { "rateLimits": msg["result"]["rateLimits"] } });
                        events.extend(parse_codex_notification(&note, &mut CodexTurnState::default()));
                    }
                    answered |= 1;
                }
                Some(3) if answered & 2 == 0 => {
                    if msg["error"].is_null() {
                        let account = &msg["result"]["account"];
                        // `chatgpt`, `apiKey`, ...: the same mode names `account/updated` uses, in lower case.
                        let mode = account["type"].as_str().map(str::to_ascii_lowercase);
                        let note = json!({ "method": "account/updated", "params": { "authMode": mode, "planType": account["planType"] } });
                        events.extend(parse_codex_notification(&note, &mut CodexTurnState::default()));
                    }
                    answered |= 2;
                }
                _ => {}
            }
            if answered == 3 {
                break;
            }
        }
    };
    let _ = tokio::time::timeout(timeout, ask).await;
    proc::stop(&mut *stdin, &mut child, &mut job).await;
    events
}

/// Asks a short-lived `codex app-server` for the sessions started in a terminal in `folder`, newest first. Faber's own
/// sessions come from the app server and are left out. Gives an empty list on any failure.
pub async fn list_codex_threads(bin: &str, folder: &Path, windows: bool, spawn: Spawner, timeout: Duration) -> Vec<CliSession> {
    let Ok(p) = spawn(bin, &codex_args(Area::Chat, windows, false, Permissions::Isolated), folder) else { return vec![] };
    let mut stdin = p.stdin;
    let mut child = p.child;
    let mut job = p.contained;
    let mut lines = BufReader::new(p.stdout).lines();
    let ask = async {
        let init = json!({ "id": 1, "method": "initialize", "params": { "clientInfo": { "name": "faber", "title": "Faber", "version": "0.0.0" } } });
        let _ = proc::write_line(&mut *stdin, &init).await;
        while let Ok(Some(line)) = lines.next_line().await {
            let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
            if msg["id"] == 1 {
                let _ = proc::write_line(&mut *stdin, &json!({ "method": "initialized" })).await;
                let params = json!({ "cwd": folder.to_string_lossy(), "limit": 30, "sourceKinds": ["cli"] });
                let _ = proc::write_line(&mut *stdin, &json!({ "id": 2, "method": "thread/list", "params": params })).await;
            } else if msg["id"] == 2 {
                return msg["result"]["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|t| t["ephemeral"] != true && t["id"].is_string())
                    .map(|t| CliSession {
                        vendor: Vendor::Codex,
                        id: s(&t["id"]),
                        title: [&t["name"], &t["preview"]]
                            .into_iter()
                            .filter_map(|v| v.as_str())
                            .map(|v| v.split_whitespace().collect::<Vec<_>>().join(" "))
                            .find(|v| !v.is_empty())
                            .unwrap_or_default(),
                        model: t["model"].as_str().map(String::from),
                        updated_at: t["updatedAt"].as_u64().unwrap_or(0) * 1000,
                    })
                    .collect();
            }
        }
        vec![]
    };
    let threads = tokio::time::timeout(timeout, ask).await.unwrap_or_default();
    proc::stop(&mut *stdin, &mut child, &mut job).await;
    threads
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
    let Ok(p) = spawn(bin, &codex_args(Area::Chat, windows, false, Permissions::Isolated), cwd) else { return vec![] };
    let mut stdin = p.stdin;
    let mut child = p.child;
    let mut job = p.contained;
    let mut lines = BufReader::new(p.stdout).lines();
    let ask = async {
        let init = json!({ "id": 1, "method": "initialize", "params": { "clientInfo": { "name": "faber", "title": "Faber", "version": "0.0.0" } } });
        let _ = proc::write_line(&mut *stdin, &init).await;
        while let Ok(Some(line)) = lines.next_line().await {
            let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
            if msg["id"] == 1 {
                let _ = proc::write_line(&mut *stdin, &json!({ "method": "initialized" })).await;
                let _ = proc::write_line(&mut *stdin, &json!({ "id": 2, "method": "model/list", "params": {} })).await;
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
    proc::stop(&mut *stdin, &mut child, &mut job).await;
    models
}

#[cfg(test)]
mod rewind_tests {
    use super::*;

    fn thread(sink: EventSink) -> Thread {
        Thread {
            sink,
            params: CodexThread {
                cwd: PathBuf::new(),
                model: None,
                effort: None,
                resume: None,
                instructions: None,
                research: None,
                group_instructions: None,
                user_name: None,
                permissions: Permissions::Ask,
                leave_plan: false,
                tuning: Default::default(),
                helper: crate::helper::Helper::Off,
                browse: false,
                conversations: false,
                verify: false,
                progress: false,
            },
            id: Some("owner".into()),
            turn: None,
            queued: vec![],
            compact: false,
            state: Default::default(),
            approvals: HashMap::new(),
            helpers: HashMap::new(),
            watch: Default::default(),
            interrupt_pending: false,
            blocked: false,
            closing: false,
            stop_children: false,
            early_completion: vec![],
            commands: HashMap::new(),
            unconfirmed_commands: HashMap::new(),
            owns_server: false,
            server_writers: false,
            native_commands: Default::default(),
            stop_reported: false,
        }
    }

    #[tokio::test]
    async fn plan_stop_before_the_turn_id_also_cancels_commands_without_forgetting_their_writers() {
        let mut owner = thread(Arc::new(|_| {}));
        owner.watch.begin();
        let (cancel, cancelled) = tokio::sync::oneshot::channel();
        owner.commands.insert("command".into(), (Some(cancel), tokio::spawn(async {})));
        assert!(owner.interrupt().is_none());
        assert!(owner.interrupt_pending && owner.watch.stopping());
        assert!(cancelled.await.is_ok());
        assert!(owner.commands.contains_key("command"));
        assert!(!owner.drained(1, &Subagents::default()));
    }

    #[tokio::test]
    async fn rewind_an_unconfirmed_command_keeps_its_owned_tree_and_stop_can_retry_it() {
        let empty = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let query = empty.clone();
        let contained = proc::Containment::scripted(
            move || {
                if query.load(Ordering::Acquire) { Ok(true) } else { Err(std::io::Error::other("query unavailable")) }
            },
            || {},
        );
        let tree = contained.owned_tree().unwrap();
        let mut owner = thread(Arc::new(|_| {}));
        let id = json!("command");
        let job = tokio::spawn(async {});
        tokio::task::yield_now().await;
        owner.commands.insert(id.to_string(), (None, job));
        owner.command_finished(&id, false, Some(tree));
        assert!(owner.commands.contains_key(&id.to_string()));
        assert!(owner.unconfirmed_commands.contains_key(&id.to_string()));
        assert!(!owner.drained(1, &Subagents::default()));
        empty.store(true, Ordering::Release);
        let (sent, mut completed) = unbounded_channel();
        owner.retry_commands(1, &sent);
        let (key, retry_id, result) = completed.recv().await.unwrap();
        assert_eq!((key, &retry_id), (1, &id));
        assert!(result.confirmed);
        owner.command_finished(&retry_id, result.confirmed, result.unresolved);
        assert!(owner.commands.is_empty() && owner.unconfirmed_commands.is_empty());
        assert!(owner.drained(1, &Subagents::default()));
    }

    #[test]
    fn rewind_empty_server_group_never_confirms_a_reported_terminal() {
        let root = proc::ProcessIdentity { pid: 100, created: 1 };
        let child = proc::ProcessIdentity { pid: 101, created: 2 };
        let members = Arc::new(std::sync::Mutex::new(vec![root]));
        let query = members.clone();
        let failed = Arc::new(AtomicBool::new(false));
        let fail_query = failed.clone();
        let container = proc::Containment::scripted_descendants(
            root,
            move || {
                if fail_query.load(Ordering::Acquire) {
                    Err(std::io::Error::other("query unavailable"))
                } else {
                    Ok(query.lock().unwrap().clone())
                }
            },
            || {},
        );
        let server = container.owned_tree().unwrap();
        let (sent, mut events) = unbounded_channel();
        let mut owner = thread(Arc::new(move |event| drop(sent.send(event))));
        owner.owns_server = true;
        owner.server_writers = true;
        owner.native_commands.track(&json!({"method":"item/completed", "params":{"item":{
            "type":"commandExecution", "id":"command", "processId":"0020000", "exitCode":null}}}));
        assert_eq!(
            owner.native_commands.stops("owner"),
            [("owner".into(), "command".into(), "0020000".into())],
            "even numeric-looking terminal handles are preserved verbatim"
        );
        owner.publish_writers(1, &Subagents::default());
        assert!(
            matches!(events.try_recv(), Ok(EngineEvent::Writers { active, unknown: true }) if active == ["native-command:command", "process-tree"])
        );
        let mut threads = HashMap::from([(1, owner)]);
        let helpers = Subagents::default();
        let _ = refresh_native(&mut threads, 1, Some(&server), false);
        threads[&1].publish_writers(1, &helpers);
        assert!(
            matches!(events.try_recv(), Ok(EngineEvent::Writers { active, unknown: true }) if active == ["native-command:command"]),
            "a reported terminal can still write from outside the original group"
        );
        assert!(!threads[&1].drained(1, &helpers));
        threads.get_mut(&1).unwrap().native_commands.track(&json!({"method":"item/completed", "params":{"item":{
            "type":"commandExecution", "id":"command", "processId":"0020000", "exitCode":0}}}));
        threads[&1].publish_writers(1, &helpers);
        assert!(
            matches!(events.try_recv(), Ok(EngineEvent::Writers { active, unknown:false }) if active.is_empty()),
            "CLI completion releases the terminal after the process-table proof is clear"
        );
        members.lock().unwrap().push(child);
        let _ = refresh_native(&mut threads, 1, Some(&server), false);
        threads[&1].publish_writers(1, &helpers);
        assert!(
            matches!(events.try_recv(), Ok(EngineEvent::Writers { active, unknown: true }) if active == ["process-tree"]),
            "an unreported hook child is a writer without any command item"
        );
        failed.store(true, Ordering::Release);
        let _ = refresh_native(&mut threads, 1, Some(&server), false);
        threads.get_mut(&1).unwrap().report_stop(1, &helpers);
        assert!(matches!(events.try_recv(), Ok(EngineEvent::Writers { unknown: true, .. })));
        assert!(matches!(events.try_recv(), Ok(EngineEvent::Error { message, fatal: false }) if message == "error.rewind.stopUnconfirmed"));
    }

    #[cfg(unix)]
    #[test]
    fn rewind_cli_exit_and_missing_terminal_need_the_shared_descendant_proof() {
        for listed_gone in [false, true] {
            let root = proc::ProcessIdentity { pid: 100, created: 1 };
            let child = proc::ProcessIdentity { pid: 102, created: 3 };
            let outside = Arc::new(std::sync::Mutex::new(vec![child]));
            let query = outside.clone();
            let contained =
                proc::Containment::scripted_external_descendants(root, move || Ok(vec![root]), move || Ok(query.lock().unwrap().clone()));
            let server = contained.owned_tree().unwrap();
            let sink: EventSink = Arc::new(|_| {});
            let mut owner = thread(sink.clone());
            owner.owns_server = true;
            owner.native_commands.track(&json!({"method":"item/completed","params":{"item":{
                "type":"commandExecution","id":"cmd","processId":"opaque-terminal","exitCode":null}}}));
            let mut peer = thread(sink);
            peer.owns_server = true;
            let mut threads = HashMap::from([(1, owner), (2, peer)]);
            let _ = refresh_native(&mut threads, 1, Some(&server), false);
            if listed_gone {
                threads.get_mut(&1).unwrap().native_commands.confirm_gone("cmd", "opaque-terminal");
            } else {
                threads.get_mut(&1).unwrap().native_commands.track(&json!({"method":"item/completed","params":{"item":{
                    "type":"commandExecution","id":"cmd","processId":"opaque-terminal","exitCode":0}}}));
            }
            let _ = refresh_native(&mut threads, 1, Some(&server), false);
            assert!(
                threads.values().all(|thread| thread.server_writers),
                "neither CLI completion nor listing absence releases the surviving child or its idle peer"
            );
            outside.lock().unwrap().clear();
            let _ = refresh_native(&mut threads, 1, Some(&server), false);
            assert!(threads.values().all(|thread| !thread.server_writers));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn rewind_an_idle_unannounced_child_can_finish_without_another_event_or_stop() {
        use tokio::io::AsyncWriteExt;
        async fn read(lines: &mut tokio::io::Lines<BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>>) -> Value {
            loop {
                let msg: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
                if msg["method"] != "skills/list" {
                    return msg;
                }
            }
        }
        async fn write(pipe: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>, msg: Value) {
            pipe.write_all(format!("{msg}\n").as_bytes()).await.unwrap();
        }
        tokio::time::timeout(Duration::from_secs(3), async {
            let root = proc::ProcessIdentity { pid: 100, created: 1 };
            let child = proc::ProcessIdentity { pid: 101, created: 2 };
            let members = Arc::new(std::sync::Mutex::new(vec![root, child]));
            let query = members.clone();
            let contained = proc::Containment::scripted_descendants(root, move || Ok(query.lock().unwrap().clone()), || {});
            let (driver, script) = tokio::io::duplex(65536);
            let (stdout, stdin) = tokio::io::split(driver);
            let (read_pipe, mut input) = tokio::io::split(script);
            let mut output = BufReader::new(read_pipe).lines();
            let (tx, rx) = unbounded_channel();
            let (sent, mut events) = unbounded_channel();
            let sink: EventSink = Arc::new(move |event| drop(sent.send(event)));
            let opts = CodexServerOptions {
                permissions: Permissions::FullAccess,
                cli: crate::cli::Cli { command: "unused".into(), prefix: vec![] },
                cwd: PathBuf::new(),
                area: Area::Project,
                user_hooks: false,
                windows: false,
                spawn: Arc::new(|_, _, _| panic!("scripted driver must not spawn a process")),
                tools: Default::default(),
            };
            let p = proc::Proc { stdin: Box::new(stdin), stdout: Box::new(stdout), stderr: None, child: None, contained: Some(contained) };
            let server = tokio::spawn(serve(Ok(p), opts, rx, Arc::new(AtomicBool::new(true))));
            let init = read(&mut output).await;
            write(&mut input, json!({"id":init["id"],"result":{}})).await;
            assert_eq!(read(&mut output).await["method"], "initialized");
            tx.send(ServerCommand::Open { key: 1, thread: thread(sink.clone()).params, sink }).unwrap();
            let open = read(&mut output).await;
            write(&mut input, json!({"id":open["id"],"result":{"thread":{"id":"owner"}}})).await;
            while !matches!(events.recv().await.unwrap(), EngineEvent::Started { .. }) {}
            write(&mut input, json!({"method":"turn/started","params":{"threadId":"owner","turn":{"id":"first"}}})).await;
            write(&mut input, json!({"method":"turn/completed","params":{"threadId":"owner","turn":{"id":"first","status":"completed"}}}))
                .await;
            while !matches!(events.recv().await.unwrap(), EngineEvent::Done { .. }) {}
            assert!(matches!(events.recv().await.unwrap(), EngineEvent::Writers { active, unknown: true } if active == ["process-tree"]));
            members.lock().unwrap().retain(|member| *member == root);
            // No command or notification follows; only the owned OS membership changed.
            tokio::time::advance(Duration::from_secs(1)).await;
            while !matches!(events.recv().await.unwrap(), EngineEvent::Writers { active, unknown: false } if active.is_empty()) {}
            server.abort();
        })
        .await
        .expect("natural worker exit publishes idle without Check again or Stop supplying another event");
    }

    #[tokio::test]
    async fn rewind_stop_without_main_turn_cancels_commands_until_completion_is_confirmed() {
        let (sent, mut events) = unbounded_channel();
        let mut owner = thread(Arc::new(move |event| {
            let _ = sent.send(event);
        }));
        let (cancel, stopped) = tokio::sync::oneshot::channel();
        let (confirmed, completed) = tokio::sync::oneshot::channel();
        let job = tokio::spawn(async move {
            stopped.await.unwrap();
            let _ = confirmed.send(());
        });
        owner.commands.insert("command".into(), (Some(cancel), job));
        let (peer_cancel, mut peer_stopped) = tokio::sync::oneshot::channel();
        let mut peer = thread(Arc::new(|_| {}));
        peer.commands.insert("peer".into(), (Some(peer_cancel), tokio::spawn(std::future::pending())));
        assert!(owner.interrupt().is_none(), "there is no main turn to interrupt");
        completed.await.unwrap();
        assert!(peer_stopped.try_recv().is_err(), "the shared server's other conversation keeps working");
        owner.publish_writers(1, &Subagents::default());
        assert!(matches!(events.try_recv(), Ok(EngineEvent::Writers { active, unknown: false }) if active == ["command:command"]));
        owner.commands.remove("command");
        owner.publish_writers(1, &Subagents::default());
        assert!(matches!(events.try_recv(), Ok(EngineEvent::Writers { active, unknown: false }) if active.is_empty()));
        for (_, job) in peer.commands.values() {
            job.abort();
        }
    }

    #[test]
    fn rewind_native_background_command_stays_unknown_until_the_cli_reports_its_exit() {
        let (sent, mut events) = unbounded_channel();
        let mut owner = thread(Arc::new(move |event| {
            let _ = sent.send(event);
        }));
        owner.native_commands.track(&json!({"method":"item/started","params":{"item":{"type":"commandExecution","id":"cmd"}}}));
        owner.native_commands.track(&json!({"method":"item/completed","params":{"item":{
            "type":"commandExecution","id":"cmd","processId":"terminal","exitCode":null,"status":"completed"}}}));
        owner.publish_writers(1, &Subagents::default());
        assert!(matches!(events.try_recv(), Ok(EngineEvent::Writers { active, unknown: true }) if active == ["native-command:cmd"]));
        assert_eq!(owner.native_commands.stops("owner"), [("owner".into(), "cmd".into(), "terminal".into())]);
        owner.native_commands.confirm_gone("cmd", "different-process");
        assert!(owner.native_commands.unknown(), "a stale confirmation never clears a different writer");
        owner.native_commands.track(&json!({"method":"item/completed","params":{"item":{
            "type":"commandExecution","id":"cmd","processId":"terminal","exitCode":0,"status":"completed"}}}));
        owner.publish_writers(1, &Subagents::default());
        assert!(
            matches!(events.try_recv(), Ok(EngineEvent::Writers { active, unknown: false }) if active.is_empty()),
            "the CLI explicitly reported this command ended"
        );
        owner.native_commands.confirm_gone("cmd", "terminal");
        owner.publish_writers(1, &Subagents::default());
        assert!(matches!(events.try_recv(), Ok(EngineEvent::Writers { active, unknown: false }) if active.is_empty()));
    }

    // Only in-memory pipes and channels: keep the hang guard independent of host load.
    #[tokio::test(start_paused = true)]
    async fn rewind_stop_cancels_native_commands_after_turn_and_confirms_exit_without_touching_peers() {
        use tokio::io::AsyncWriteExt;
        async fn read(lines: &mut tokio::io::Lines<BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>>) -> Value {
            loop {
                let msg: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
                if msg["method"] != "skills/list" {
                    return msg;
                }
            }
        }
        async fn write(pipe: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>, msg: Value) {
            pipe.write_all(format!("{msg}\n").as_bytes()).await.unwrap();
        }
        tokio::time::timeout(Duration::from_secs(3), async {
            let (driver, script) = tokio::io::duplex(65536);
            let (stdout, stdin) = tokio::io::split(driver);
            let (read_pipe, mut input) = tokio::io::split(script);
            let mut output = BufReader::new(read_pipe).lines();
            let (tx, rx) = unbounded_channel();
            let (sent, mut events) = unbounded_channel();
            let sink: EventSink = Arc::new(move |event| { let _ = sent.send(event); });
            let opts = CodexServerOptions {
                permissions: Permissions::FullAccess,
                cli: crate::cli::Cli { command: "unused".into(), prefix: vec![] },
                cwd: PathBuf::new(), area: Area::Project, user_hooks: false, windows: false,
                spawn: Arc::new(|_, _, _| panic!("scripted driver must not spawn a process")), tools: Default::default(),
            };
            let p = proc::Proc { stdin: Box::new(stdin), stdout: Box::new(stdout), stderr: None, child: None, contained: None };
            let server = tokio::spawn(serve(Ok(p), opts, rx, Arc::new(AtomicBool::new(true))));
            let init = read(&mut output).await;
            write(&mut input, json!({"id":init["id"],"result":{}})).await;
            assert_eq!(read(&mut output).await["method"], "initialized");
            for (key, id) in [(1, "owner"), (2, "peer")] {
                tx.send(ServerCommand::Open { key, thread: thread(sink.clone()).params, sink: sink.clone() }).unwrap();
                let open = read(&mut output).await;
                write(&mut input, json!({"id":open["id"],"result":{"thread":{"id":id}}})).await;
                while !matches!(events.recv().await.unwrap(), EngineEvent::Started { .. }) {}
            }
            // Both commands outlive their turns; their process ids belong to separate threads.
            for id in ["owner", "peer"] {
                write(&mut input, json!({"method":"item/completed","params":{"threadId":id,"item":{
                    "type":"commandExecution","id":"cmd","processId":"terminal","exitCode":null}}})).await;
            }
            while !matches!(events.recv().await.unwrap(), EngineEvent::Writers { active, unknown: true } if active == ["native-command:cmd"]) {}
            tx.send(ServerCommand::Session(1, SessionCommand::Interrupt)).unwrap();
            let stop = read(&mut output).await;
            assert_eq!(stop["method"], "thread/backgroundTerminals/terminate");
            assert_eq!(stop["params"], json!({"threadId":"owner","processId":"terminal"}));
            write(&mut input, json!({"id":stop["id"],"result":{"terminated":true}})).await;
            let list = read(&mut output).await;
            assert_eq!(list["method"], "thread/backgroundTerminals/list");
            assert_eq!(list["params"]["threadId"], "owner");
            write(&mut input, json!({"id":list["id"],"result":{"data":[{"processId":"terminal"}]}})).await;
            // A later notification confirms the list was consumed; a retry must not overtake its answer in select!.
            write(&mut input, json!({"method":"turn/started","params":{"threadId":"peer","turn":{"id":"barrier"}}})).await;
            write(&mut input, json!({"method":"turn/completed","params":{"threadId":"peer","turn":{"id":"barrier","status":"completed"}}})).await;
            while !matches!(events.recv().await.unwrap(), EngineEvent::Done { .. }) {}
            // An acknowledged cancellation with a still-listed process must not publish idle.
            tx.send(ServerCommand::Session(1, SessionCommand::Interrupt)).unwrap();
            let retry = read(&mut output).await;
            assert_eq!(retry["method"], "thread/backgroundTerminals/terminate");
            write(&mut input, json!({"id":retry["id"],"error":{"message":"not stopped"}})).await;
            while !matches!(events.recv().await.unwrap(), EngineEvent::Error { fatal: false, .. }) {}
            assert!(!events.try_recv().is_ok_and(|e| matches!(e, EngineEvent::Writers { active, unknown: false } if active.is_empty())));
            tx.send(ServerCommand::Session(1, SessionCommand::Interrupt)).unwrap();
            let stop = read(&mut output).await;
            write(&mut input, json!({"id":stop["id"],"result":{"terminated":false}})).await;
            let list = read(&mut output).await;
            write(&mut input, json!({"id":list["id"],"result":{"data":[],"nextCursor":"page2"}})).await;
            let page = read(&mut output).await;
            assert_eq!(page["params"], json!({"threadId":"owner","cursor":"page2"}));
            write(&mut input, json!({"id":page["id"],"result":{"data":[],"nextCursor":null}})).await;
            assert!(matches!(events.recv().await.unwrap(), EngineEvent::Writers { active, unknown: false } if active.is_empty()));
            tx.send(ServerCommand::Session(2, SessionCommand::Interrupt)).unwrap();
            let peer = read(&mut output).await;
            assert_eq!(peer["params"], json!({"threadId":"peer","processId":"terminal"}), "the peer remained live throughout");
            server.abort();
        }).await.unwrap();
    }

    #[test]
    fn rewind_late_native_helpers_remain_writers_and_stop_is_scoped_to_the_owner() {
        let mut helpers = Subagents::default();
        for (key, thread) in [(1, "child"), (2, "peer")] {
            helpers.track(
                key,
                Some("earlier"),
                &json!({"method":"item/started", "params":{"item":{
                "type":"subAgentActivity", "kind":"started", "agentThreadId":thread, "id":thread}}}),
            );
        }
        assert_eq!(helpers.writers(1), (vec!["helper:child".into()], true), "an announced helper is unknown until its turn is known");
        for thread in ["child", "peer"] {
            helpers.child(&json!({"method":"turn/started", "params":{"threadId":thread,"turn":{"id":"work"}}}));
        }
        assert_eq!(helpers.at_work(1), [("child".into(), "work".into())]);
        assert_eq!(helpers.writers(1), (vec!["helper:child".into()], false));
        // Its owner has no current turn; child completion still settles the writer independently.
        helpers.child(&json!({"method":"turn/completed", "params":{"threadId":"child","turn":{"id":"work","status":"completed"}}}));
        assert_eq!(helpers.writers(1), (vec![], false));
        assert_eq!(helpers.at_work(2), [("peer".into(), "work".into())]);
    }

    #[tokio::test(start_paused = true)]
    async fn rewind_replacing_a_session_keeps_its_native_command_until_a_late_exit() {
        use tokio::io::AsyncWriteExt;
        async fn read(lines: &mut tokio::io::Lines<BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>>) -> Value {
            loop {
                let msg: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
                if msg["method"] != "skills/list" {
                    return msg;
                }
            }
        }
        async fn write(pipe: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>, msg: Value) {
            pipe.write_all(format!("{msg}\n").as_bytes()).await.unwrap();
        }
        tokio::time::timeout(Duration::from_secs(3), async {
            // A replacement can open a new native thread or resume the same one.
            for replacement in ["replacement", "owner"] {
                let (driver, script) = tokio::io::duplex(65536);
                let (stdout, stdin) = tokio::io::split(driver);
                let (read_pipe, mut input) = tokio::io::split(script);
                let mut output = BufReader::new(read_pipe).lines();
                let (tx, rx) = unbounded_channel();
                let (sent, mut events) = unbounded_channel();
                let sink = |key| {
                    let sent = sent.clone();
                    Arc::new(move |event| { let _ = sent.send((key, event)); }) as EventSink
                };
                let opts = CodexServerOptions {
                    permissions: Permissions::FullAccess,
                    cli: crate::cli::Cli { command: "unused".into(), prefix: vec![] },
                    cwd: PathBuf::new(), area: Area::Project, user_hooks: false, windows: false,
                    spawn: Arc::new(|_, _, _| panic!("scripted driver must not spawn a process")), tools: Default::default(),
                };
                let p = proc::Proc { stdin: Box::new(stdin), stdout: Box::new(stdout), stderr: None, child: None, contained: None };
                let server = tokio::spawn(serve(Ok(p), opts, rx, Arc::new(AtomicBool::new(true))));
                let init = read(&mut output).await;
                write(&mut input, json!({"id":init["id"],"result":{}})).await;
                assert_eq!(read(&mut output).await["method"], "initialized");
                for (key, id) in [(1, "owner"), (2, "peer")] {
                    tx.send(ServerCommand::Open { key, thread: thread(sink(key)).params, sink: sink(key) }).unwrap();
                    let open = read(&mut output).await;
                    write(&mut input, json!({"id":open["id"],"result":{"thread":{"id":id}}})).await;
                    while !matches!(events.recv().await.unwrap(), (k, EngineEvent::Started { .. }) if k == key) {}
                }
                for (key, id) in [(1, "owner"), (2, "peer")] {
                    write(&mut input, json!({"method":"turn/started","params":{"threadId":id,"turn":{"id":"first"}}})).await;
                    write(&mut input, json!({"method":"item/completed","params":{"threadId":id,"item":{
                        "type":"commandExecution","id":"cmd","processId":"terminal","exitCode":null}}})).await;
                    write(&mut input, json!({"method":"turn/completed","params":{"threadId":id,"turn":{"id":"first","status":"completed"}}})).await;
                    while !matches!(events.recv().await.unwrap(), (k, EngineEvent::Writers { active, unknown: true }) if k == key && active == ["native-command:cmd"]) {}
                }
                tx.send(ServerCommand::Session(1, SessionCommand::Close)).unwrap();
                let stop = read(&mut output).await;
                assert_eq!(stop["method"], "thread/backgroundTerminals/terminate");
                assert_eq!(stop["params"], json!({"threadId":"owner","processId":"terminal"}));
                write(&mut input, json!({"id":stop["id"],"error":{"message":"still running"}})).await;
                while !matches!(events.recv().await.unwrap(), (1, EngineEvent::Error { .. })) {}
                let mut next = thread(sink(3)).params;
                next.effort = Some("high".into());
                tx.send(ServerCommand::Open { key: 3, thread: next, sink: sink(3) }).unwrap();
                let open = read(&mut output).await;
                write(&mut input, json!({"id":open["id"],"result":{"thread":{"id":replacement}}})).await;
                while !matches!(events.recv().await.unwrap(), (3, EngineEvent::Started { .. })) {}
                write(&mut input, json!({"method":"turn/started","params":{"threadId":replacement,"turn":{"id":"second"}}})).await;
                write(&mut input, json!({"method":"turn/completed","params":{"threadId":replacement,"turn":{"id":"second","status":"completed"}}})).await;
                while !matches!(events.recv().await.unwrap(), (3, EngineEvent::Writers { active, unknown: false }) if active.is_empty()) {}
                // Only the earlier session can confirm this command's exit, even after native-thread reuse.
                write(&mut input, json!({"method":"item/completed","params":{"threadId":"owner","item":{
                    "type":"commandExecution","id":"cmd","processId":"terminal","exitCode":0}}})).await;
                while !matches!(events.recv().await.unwrap(), (1, EngineEvent::Writers { active, unknown: false }) if active.is_empty()) {}
                write(&mut input, json!({"method":"turn/started","params":{"threadId":replacement,"turn":{"id":"third"}}})).await;
                while !matches!(events.recv().await.unwrap(), (3, EngineEvent::TurnStarted)) {}
                tx.send(ServerCommand::Session(2, SessionCommand::Interrupt)).unwrap();
                let peer = read(&mut output).await;
                assert_eq!(peer["params"], json!({"threadId":"peer","processId":"terminal"}), "the peer stayed live and retained its own owner");
                server.abort();
            }
        }).await.expect("the old owner reports exit and the new session remains routable");
    }

    #[tokio::test]
    async fn rewind_driver_keeps_late_helpers_open_and_stop_reaches_them_after_done() {
        use tokio::io::AsyncWriteExt;
        async fn read(lines: &mut tokio::io::Lines<BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>>) -> Value {
            loop {
                let msg: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
                if msg["method"] != "skills/list" {
                    return msg;
                }
            }
        }
        async fn write(pipe: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>, msg: Value) {
            pipe.write_all(format!("{msg}\n").as_bytes()).await.unwrap();
        }
        tokio::time::timeout(Duration::from_secs(3), async {
            let (driver, script) = tokio::io::duplex(65536);
            let (stdout, stdin) = tokio::io::split(driver);
            let (read_pipe, mut input) = tokio::io::split(script);
            let mut output = BufReader::new(read_pipe).lines();
            let (tx, rx) = unbounded_channel();
            let (sent, mut events) = unbounded_channel();
            let sink: EventSink = Arc::new(move |event| {
                let _ = sent.send(event);
            });
            let opts = CodexServerOptions {
                permissions: Permissions::FullAccess,
                cli: crate::cli::Cli { command: "unused".into(), prefix: vec![] },
                cwd: PathBuf::new(),
                area: Area::Project,
                user_hooks: false,
                windows: false,
                spawn: Arc::new(|_, _, _| panic!("scripted driver must not spawn a process")),
                tools: Default::default(),
            };
            let p = proc::Proc { stdin: Box::new(stdin), stdout: Box::new(stdout), stderr: None, child: None, contained: None };
            let server = tokio::spawn(serve(Ok(p), opts, rx, Arc::new(AtomicBool::new(true))));
            let init = read(&mut output).await;
            assert_eq!(init["method"], "initialize");
            write(&mut input, json!({"id":init["id"],"result":{}})).await;
            assert_eq!(read(&mut output).await["method"], "initialized");
            tx.send(ServerCommand::Open { key: 1, thread: thread(sink.clone()).params, sink }).unwrap();
            let open = read(&mut output).await;
            write(&mut input, json!({"id":open["id"],"result":{"thread":{"id":"owner"}}})).await;
            while !matches!(events.recv().await.unwrap(), EngineEvent::Started { .. }) {}
            tx.send(ServerCommand::Session(1, SessionCommand::Send(UserTurn::text("work")))).unwrap();
            let start = read(&mut output).await;
            assert_eq!(start["method"], "turn/start");
            write(&mut input, json!({"id":start["id"],"result":{"turn":{"id":"main"}}})).await;
            write(&mut input, json!({"method":"turn/started","params":{"threadId":"owner","turn":{"id":"main"}}})).await;
            write(
                &mut input,
                json!({"method":"item/started","params":{"threadId":"owner","turnId":"main","item":{
                "type":"subAgentActivity","kind":"started","id":"helper-call","agentThreadId":"child"}}}),
            )
            .await;
            write(&mut input, json!({"method":"turn/started","params":{"threadId":"child","turn":{"id":"late","rootTurnId":"main"}}}))
                .await;
            write(&mut input, json!({"method":"turn/completed","params":{"threadId":"owner","turn":{"id":"main","status":"completed"}}}))
                .await;
            let mut cursor = false;
            loop {
                match events.recv().await.unwrap() {
                    EngineEvent::Cursor { turn, .. } => {
                        assert_eq!(turn.as_deref(), Some("main"));
                        cursor = true;
                    }
                    EngineEvent::Done { .. } => {
                        assert!(cursor, "the saved cursor precedes completion");
                        break;
                    }
                    _ => (),
                }
            }
            assert!(matches!(events.recv().await.unwrap(), EngineEvent::Writers { active, unknown: false } if active == ["helper:child"]));
            tx.send(ServerCommand::Session(1, SessionCommand::Interrupt)).unwrap();
            let stop = read(&mut output).await;
            assert_eq!(stop["method"], "turn/interrupt");
            assert_eq!(stop["params"], json!({"threadId":"child","turnId":"late"}));
            write(
                &mut input,
                json!({"method":"item/completed","params":{"threadId":"child","item":{
                "type":"commandExecution","id":"child-cmd","processId":"child-terminal","exitCode":null}}}),
            )
            .await;
            let native = read(&mut output).await;
            assert_eq!(native["method"], "thread/backgroundTerminals/terminate");
            assert_eq!(native["params"], json!({"threadId":"child","processId":"child-terminal"}));
            write(&mut input, json!({"method":"turn/completed","params":{"threadId":"child","turn":{"id":"late","status":"interrupted"}}}))
                .await;
            while !matches!(events.recv().await.unwrap(), EngineEvent::Writers { active, unknown: true } if active == ["helper:child"]) {}
            write(&mut input, json!({"id":native["id"],"result":{"terminated":true}})).await;
            let list = read(&mut output).await;
            assert_eq!(list["params"]["threadId"], "child");
            write(&mut input, json!({"id":list["id"],"result":{"data":[]}})).await;
            // The helper's turn ending did not settle its command; only the scoped process check does.
            loop {
                if matches!(events.recv().await.unwrap(), EngineEvent::Writers { active, unknown: false } if active.is_empty()) {
                    break;
                }
            }
            server.abort();
        })
        .await
        .unwrap();
    }
}

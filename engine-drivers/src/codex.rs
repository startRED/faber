//! Codex driver: `codex app-server` processes spoken to over JSON-RPC on stdio.

use crate::claude::EventSink;
use crate::events::{
    CompactOutcome, Delivery, EngineEvent, IdleThreadRelease, MAX_FAILURE_SCAN_BYTES, Session, SessionCommand, TOOL_OUTPUT_CHARS, UserTurn,
};
use crate::model::{Area, CliSession, Permissions, ReasoningSummary, ReplyStyle, Tuning, Vendor, WebSearch};
use crate::persona::{PersonaInput, persona};
use crate::proc::{self, Spawner};
use crate::rewind::NativePoint;
use crate::signals::{codex_account, codex_signals};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::runtime::Handle;
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
        push(&["-c", "allow_login_shell=false"]);
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

/// What the credential and data-folder guards read: the command or the changes alone, never the explanation beside them.
fn approval_action(params: &Value) -> Value {
    [&params["command"], &params["changes"]].into_iter().find(|v| !v.is_null()).unwrap_or(params).clone()
}

fn approval_details(method: &str, params: &Value) -> (String, Value) {
    let tool = match method {
        "item/commandExecution/requestApproval" => "command",
        "item/fileChange/requestApproval" => "fileChange",
        _ => return (params["kind"].as_str().unwrap_or(method).to_string(), params.clone()),
    };
    // Keep the request's explanation and folder beside the action, without carrying routing identifiers into the card.
    let mut input = Map::new();
    for key in ["command", "changes", "cwd", "reason", "grantRoot", "networkApprovalContext"] {
        if let Some(value) = params.get(key).filter(|value| !value.is_null()) {
            input.insert(key.to_string(), value.clone());
        }
    }
    (tool.into(), Value::Object(input))
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
            "name":"run_command", "description":"Run one literal command in the working folder through Faber's sandbox. Supports read-only git inspection, local deletion, builds, tests and local scripts; rejects shell expansion, compound commands and git mutations.",
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
    probe_tools: HashMap<String, ProbeTool>,
    native_commands: NativeCommands,
    /// What the turn's subagents used, which Codex counts in their own threads.
    subagents: Totals,
    subagents_cost: Option<f64>,
    subagents_incomplete: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum ProbeTool {
    Mcp,
    Command,
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

#[derive(Debug, Default)]
struct NativeCommands {
    /// Terminal handles are opaque; CLI completion also needs the server's process-tree proof.
    items: HashMap<String, Option<String>>,
    unknown: bool,
}

fn reported_terminal(item: &Value) -> Option<String> {
    item["processId"].as_str().filter(|p| !p.is_empty()).map(String::from).or_else(|| item["processId"].as_u64().map(|pid| pid.to_string()))
}

impl NativeCommands {
    fn pending_exit(&self, item: &Value) -> bool {
        item["type"] == "commandExecution"
            && item["exitCode"].as_i64().is_none()
            && (!item["processId"].is_null() || item["id"].as_str().is_some_and(|id| self.items.get(id).is_some_and(Option::is_some)))
    }

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
                unknown |= sub.pending || sub.state.native_commands.unknown();
                if sub.own.is_some() || sub.pending || !sub.state.native_commands.items.is_empty() {
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
                });
                None
            }
            (Some("item/completed"), Some(end @ ("completed" | "interrupted"))) => {
                let sub = self.0.get_mut(&thread).filter(|sub| sub.key == key)?;
                sub.pending = false;
                sub.own = None;
                Some(EngineEvent::ToolResult {
                    id: sub.call.clone(),
                    ok: end == "completed",
                    declined: false,
                    exit_code: None,
                    text: sub.report.clone(),
                })
            }
            _ => None,
        }
    }

    /// A notification from a subagent's own thread: the tools it ran, under the call that started it, for its conversation.
    /// Its words are kept as its report and never join the answer. None when the thread is no subagent's.
    pub fn child(&mut self, msg: &Value) -> Option<ChildNews> {
        let sub = self.0.get_mut(msg["params"]["threadId"].as_str()?)?;
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
        // Each finished message of the subagent is words of its own, kept on its call in order with its commands. Its final answer
        // (`phase`) is its report, which the call's result carries, so it is not kept twice.
        if msg["method"] == "item/completed"
            && item["type"] == "agentMessage"
            && item["phase"] != "final_answer"
            && item["text"].as_str().is_some_and(|t| !t.trim().is_empty())
        {
            events.push(EngineEvent::ChildWords { parent: call, text: s(&item["text"]) });
        }
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

/// Codex's question to the person (`item/tool/requestUserInput`, which Plan mode offers; checked on 0.162.1) as the card
/// Claude Code's `AskUserQuestion` makes, with each shown question's id and text for the answer. A secret question is left
/// out: Faber takes no passwords or keys, so it goes back unanswered. The card answers by question text, so a text asked twice
/// gets a number ("Which one? (2)") and each question keeps its own answer. `None` when nothing is left to show.
pub fn question_card(params: &Value) -> Option<(Value, Vec<(String, String)>)> {
    let mut asked = Vec::new();
    let mut questions = Vec::new();
    for q in params["questions"].as_array().into_iter().flatten() {
        let (Some(id), Some(asked_text)) = (q["id"].as_str(), q["question"].as_str()) else { continue };
        if q["isSecret"] == true {
            continue;
        }
        let mut text = asked_text.to_string();
        for n in 2.. {
            if !asked.iter().any(|(_, t)| *t == text) {
                break;
            }
            text = format!("{asked_text} ({n})");
        }
        let options: Vec<Value> = q["options"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|o| Some(json!({ "label": o["label"].as_str()?, "description": s(&o["description"]) })))
            .collect();
        questions.push(json!({ "question": text, "header": s(&q["header"]), "multiSelect": false, "options": options }));
        asked.push((id.to_string(), text));
    }
    (!asked.is_empty()).then(|| (json!({ "questions": questions }), asked))
}

/// The answer Codex reads for a card from `question_card`: each question id to what the person chose or wrote, from the
/// card's reply (question text to answer). Skip, or a question left empty, sends no answer for it, and Codex goes on.
pub fn question_answers(asked: &[(String, String)], reply: Option<&Value>) -> Value {
    let answers: Map<String, Value> = asked
        .iter()
        .filter_map(|(id, text)| {
            let answer = reply?.get(text)?.as_str().filter(|a| !a.trim().is_empty())?;
            Some((id.clone(), json!({ "answers": [answer] })))
        })
        .collect();
    json!({ "answers": answers })
}

/// Turns one `codex app-server` notification into engine events.
pub fn parse_codex_notification(msg: &Value, state: &mut CodexTurnState) -> Vec<EngineEvent> {
    state.native_commands.track(msg);
    let pending_exit = state.native_commands.pending_exit(&msg["params"]["item"]);
    // The thread's context after a request: what the last request held (`last.totalTokens`, which after a compaction is the
    // size of what is left), against the model's window.
    let usage = &msg["params"]["tokenUsage"];
    let context = (msg["method"] == "thread/tokenUsage/updated")
        .then(|| usage["last"]["totalTokens"].as_u64().or(usage["last"]["inputTokens"].as_u64()))
        .flatten()
        .map(|used| EngineEvent::Context { used: Some(used), window: usage["modelContextWindow"].as_u64() });
    context.into_iter().chain(parse_notification(msg, state, pending_exit)).chain(command_news(msg, pending_exit)).collect()
}

fn command_key(thread: &str, item: &str) -> String {
    json!([thread, item]).to_string()
}

fn command_news(msg: &Value, pending_exit: bool) -> Vec<EngineEvent> {
    use crate::background::{Capture, State, Update};
    let p = &msg["params"];
    let Some(thread) = p["threadId"].as_str() else { return vec![] };
    if msg["method"] == "item/commandExecution/outputDelta" {
        return p["itemId"]
            .as_str()
            .zip(p["delta"].as_str())
            .map(|(item, text)| {
                EngineEvent::Background(Update::Output {
                    id: command_key(thread, item),
                    text: text.into(),
                    capture: Capture::Delta,
                    source_truncated: false,
                })
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
    let terminal = msg["method"] == "item/completed" && !pending_exit;
    if let Some(text) = item["aggregatedOutput"].as_str() {
        let capture = if terminal { Capture::Terminal } else { Capture::Snapshot };
        events.push(EngineEvent::Background(Update::Output { id: id.clone(), text: text.into(), capture, source_truncated: false }));
    }
    if terminal {
        let state = match item["status"].as_str() {
            Some("interrupted") => State::Stopped,
            Some("completed") if item["exitCode"].is_null() || item["exitCode"] == 0 => State::Done,
            _ => State::Failed,
        };
        events.push(EngineEvent::Background(Update::End { id, state }));
    }
    events
}

fn mcp_failure_text(item: &Value, fallback: &str) -> String {
    if let Some(error) = item["error"]["message"].as_str().filter(|text| !text.trim().is_empty()) {
        return if error.len() <= MAX_FAILURE_SCAN_BYTES { error.into() } else { error.chars().take(TOOL_OUTPUT_CHARS).collect() };
    }
    let parts = item["result"]["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|part| part["type"] == "text")
        .filter_map(|part| part["text"].as_str().filter(|text| !text.trim().is_empty()));
    let mut text = String::new();
    for part in parts {
        let separator = (!text.is_empty()).then_some('\n');
        let bytes = text.len() + usize::from(separator.is_some());
        if bytes.saturating_add(part.len()) > MAX_FAILURE_SCAN_BYTES {
            return text.chars().chain(separator).chain(part.chars()).take(TOOL_OUTPUT_CHARS).collect();
        }
        text.extend(separator);
        text.push_str(part);
    }
    if text.is_empty() { fallback.into() } else { text }
}

fn web_request_parts<'a>(parts: impl Iterator<Item = &'a Value>) -> Option<String> {
    let mut parts = parts.filter_map(Value::as_str).filter(|part| !part.trim().is_empty());
    Some(parts.next()?.chars().chain(parts.flat_map(|part| std::iter::once('\n').chain(part.chars()))).take(TOOL_OUTPUT_CHARS).collect())
}

fn web_search_text(item: &Value, fallback: &str) -> String {
    let action = &item["action"];
    match action["type"].as_str() {
        Some("search") => web_request_parts(action["queries"].as_array().into_iter().flatten())
            .or_else(|| web_request_parts(std::iter::once(&action["query"]))),
        Some("openPage") => web_request_parts(std::iter::once(&action["url"])),
        Some("findInPage") => web_request_parts([&action["url"], &action["pattern"]].into_iter()),
        _ => None,
    }
    .or_else(|| web_request_parts(std::iter::once(&item["query"])))
    .unwrap_or_else(|| fallback.chars().take(TOOL_OUTPUT_CHARS).collect())
}

fn parse_notification(msg: &Value, state: &mut CodexTurnState, pending_exit: bool) -> Vec<EngineEvent> {
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
        "thread/compacted" => vec![EngineEvent::Compacted { after: None }],
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
            let input = if kind == "webSearch" {
                Value::String(web_search_text(item, ""))
            } else {
                [&item["command"], &item["query"], &item["changes"], &item["arguments"]]
                    .into_iter()
                    .find(|v| !v.is_null())
                    .unwrap_or(item)
                    .clone()
            };
            let name = match (kind, item["tool"].as_str()) {
                ("dynamicToolCall", Some("run_command")) => "Bash",
                ("dynamicToolCall", Some(crate::helper::NAME)) => crate::helper::TOOL,
                ("dynamicToolCall", Some(crate::review::NAME)) => crate::review::TOOL,
                ("dynamicToolCall", Some(crate::browser::NAME)) => crate::browser::TOOL,
                ("dynamicToolCall", Some(crate::sibling::NAME)) => crate::sibling::TOOL,
                ("dynamicToolCall", Some(crate::verify::NAME)) => crate::verify::TOOL,
                ("dynamicToolCall", Some(crate::map::NAME)) => crate::map::TOOL,
                ("dynamicToolCall", Some(crate::faber_tool::NAME)) => crate::faber_tool::TOOL,
                ("dynamicToolCall", Some(name)) if let Some(tool) = crate::progress::tool_of(name) => tool,
                ("dynamicToolCall", Some(name)) if let Some(tool) = crate::toolbox::tool_of(name) => tool,
                _ => kind,
            };
            vec![EngineEvent::Tool { id: s(&item["id"]), name: name.into(), input, parent: None }]
        }
        "item/completed" => {
            let item = &p["item"];
            let kind = item["type"].as_str().unwrap_or_default();
            let id = s(&item["id"]);
            if pending_exit {
                return vec![EngineEvent::ToolDetached { id }];
            }
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
            // An item with no status of its own (Codex's `imageView`) ended when it completes: it was shown as a failed step.
            let finished = item["status"] == "completed" || item["status"].is_null();
            let ok = finished && item["success"] != false && (item["exitCode"].is_null() || item["exitCode"] == 0);
            let status = item["status"].as_str().unwrap_or_default();
            let declined = status == "declined";
            let output = item["aggregatedOutput"].as_str();
            let contents = item["contentItems"].as_array().map(Vec::as_slice).unwrap_or_default();
            // Only Faber adds the second item to run_command. The program writes only the first item's text.
            let isolated_code = match contents {
                [out, code]
                    if kind == "dynamicToolCall"
                        && item["tool"] == "run_command"
                        && !declined
                        && out["type"] == "inputText"
                        && code["type"] == "inputText" =>
                {
                    code["text"].as_str().and_then(|text| text.strip_prefix(COMMAND_EXIT_PREFIX)).and_then(|code| code.parse().ok())
                }
                _ => None,
            };
            let text = match kind {
                "dynamicToolCall" => contents
                    .iter()
                    .take(if isolated_code.is_some() { 1 } else { contents.len() })
                    .filter_map(|item| item["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
                "mcpToolCall" if !ok => mcp_failure_text(item, status),
                // A native search may have no query at its start. The completion does not carry a success status.
                "webSearch" => web_search_text(item, if ok { "" } else { status }),
                "commandExecution" if !ok && output.is_none_or(|text| text.trim().is_empty()) => match item["exitCode"].as_i64() {
                    Some(code) if !status.is_empty() => format!("{status} ({code})"),
                    Some(code) => code.to_string(),
                    None => status.into(),
                },
                _ => output.unwrap_or(status).into(),
            };
            let exit_code = if kind == "commandExecution" && !declined {
                item["exitCode"].as_i64().and_then(|code| i32::try_from(code).ok())
            } else {
                isolated_code
            };
            vec![EngineEvent::ToolResult { id, ok, declined, exit_code, text }]
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
            // Why a turn that did not complete ended, from the turn's own status, as a key (rule 6); a generic one says the kind
            // of failure the turn's error names (`codexErrorInfo`, else its message).
            let error = &p["turn"]["error"];
            let kind = crate::failure::from_codex(&error["codexErrorInfo"])
                .or_else(|| error["message"].as_str().and_then(crate::failure::from_text));
            let why = (status != "completed").then(|| crate::failure::reason(crate::events::failure_reason(Some(&status), None), kind));
            let mut reason = error["message"].as_str().map(String::from).unwrap_or_else(|| status.clone());
            // A plan limit Codex names in its own word goes to the quota recovery, whatever its message says.
            if kind == Some(crate::failure::Failure::Limit) && error["codexErrorInfo"] == "usageLimitExceeded" {
                reason = format!("usage_limit_reached: {reason}");
            }
            approval
                .into_iter()
                .chain(why.map(EngineEvent::Reason))
                .chain([EngineEvent::Done {
                    ok: status == "completed",
                    reason,
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
        // A reconnect keeps this turn active. Its eventual completion reports whether the connection recovered.
        "error" if p["willRetry"] == true => vec![],
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
    /// User-requested temporary Chat: the CLI must not persist its thread.
    pub temporary: bool,
    pub cwd: PathBuf,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub resume: Option<String>,
    /// The person's visible conversation title; internal questions leave it empty.
    pub title: Option<String>,
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
    /// Told when to lead a small team, as for `ClaudeLaunch::leads`.
    pub leads: bool,
    /// Faber's checks, as for `ClaudeLaunch::verify`.
    pub verify: bool,
    /// The plan, the status and the closing box, as for `ClaudeLaunch::progress`.
    pub progress: bool,
    /// Faber's own commands, as for `ClaudeLaunch::faber`.
    pub faber: bool,
    /// The models of Codex's own subagents, as for `ClaudeLaunch::subagents`.
    pub subagents: Option<crate::helper::Subagents>,
}

enum ServerCommand {
    Open {
        key: u64,
        thread: Box<CodexThread>,
        sink: EventSink,
        tools: Option<crate::tools::Delivery>,
        identity: Option<ThreadIdentity>,
        before_turn: Option<String>,
        native_fork: Option<NativePoint>,
    },
    Session(u64, SessionCommand),
    Tools(crate::tools::Delivery),
    Retire,
}

pub struct ThreadIdentity {
    pub identity: Arc<proc::ConversationIdentity>,
    pub spawn: Spawner,
    pub native_goal: NativeGoalOpen,
}

pub struct CodexOpen {
    pub thread: CodexThread,
    pub sink: EventSink,
    pub tools: Option<crate::tools::Delivery>,
    pub identity: Option<ThreadIdentity>,
    pub before_turn: Option<String>,
    pub native_fork: Option<NativePoint>,
}

impl CodexOpen {
    pub fn new(thread: CodexThread, sink: EventSink) -> Self {
        Self { thread, sink, tools: None, identity: None, before_turn: None, native_fork: None }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NativeGoalOpen {
    Ordinary,
    CheckStopped,
    Rearm,
}
impl Drop for ThreadIdentity {
    fn drop(&mut self) {
        self.identity.revoke();
    }
}

// What an answer from Codex is for.
enum Waiting {
    Initialize,
    Unsubscribe(u64, tokio::time::Instant),
    Release(u64, bool, Option<tokio::time::Instant>),
    Goal(u64, tokio::time::Instant),
    IdleGoal(u64, tokio::time::Instant, IdleThreadRelease),
    Thread(u64, Option<tokio::time::Instant>),
    Revert(u64, Option<tokio::time::Instant>),
    ForkTail(u64, tokio::time::Instant),
    TurnStart(u64),
    Other(u64, &'static str),
    /// A `turn/steer`; its tag tells the engine which message it was.
    Steer(u64, String),
    McpStatus {
        thread: String,
        reply: tokio::sync::oneshot::Sender<Option<Vec<crate::mcp_servers::Reported>>>,
        rows: Vec<crate::mcp_servers::Reported>,
        cursors: HashSet<String>,
    },
    NativeStop(u64, String, String, String),
    NativeGone(u64, String, String, String, usize),
}

fn turn_start_pending(waiting: &HashMap<String, Waiting>, key: u64) -> bool {
    waiting.values().any(|w| matches!(w, Waiting::TurnStart(owner) if *owner == key))
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
struct ForkOpen {
    point: NativePoint,
    verified: bool,
    reply: Option<Value>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TurnNotice {
    /// No authoritative turn/started for this turn yet. A turn/start response may supply a provisional ID.
    Pending,
    Visible,
    Suppressed,
    /// An empty compact RPC response cannot identify the turn. Its first item can.
    Classifying,
    Compacting {
        applied: bool,
    },
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum CompactRequest {
    #[default]
    None,
    Queued,
    /// The request has no turn ID. After an ambiguous end, give the next turn time to identify it.
    Sent(Option<tokio::time::Instant>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GoalResume {
    Allow,
    Check,
    Hold,
    Rearm { confirmed_goal: bool },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NativeRelease {
    Keep,
    UnconfirmedOpen,
    ReleaseLateOpen,
    OnClose,
    OnIdle,
    Pending { recheck: Option<tokio::time::Instant>, idle: bool },
    Settled,
    Unloaded,
}

struct Thread {
    native_fork: Option<ForkOpen>,
    before_turn: Option<String>,
    early_completion: Vec<Value>,
    sink: EventSink,
    params: CodexThread,
    tools: Option<crate::tools::Delivery>,
    identity: Option<ThreadIdentity>,
    unsubscribed: bool,
    native_release: NativeRelease,
    retried_closing_resume: bool,
    goal_resume: GoalResume,
    id: Option<String>,
    turn: Option<String>,
    turn_notice: TurnNotice,
    queued: Vec<UserTurn>,
    compact: CompactRequest,
    state: CodexTurnState,
    approvals: HashMap<String, Value>,
    /// The approvals that are questions to the person (`question_card`): each shown question's id and text.
    questions: HashMap<String, Vec<(String, String)>>,
    /// Helper calls (R161) waiting for their report: the tool call id to the id of Codex's request.
    helpers: HashMap<String, Value>,
    watch: proc::TurnWatch,
    stop_completion_grace: Option<(tokio::time::Instant, tokio::time::Instant)>,
    interrupt_pending: bool,
    blocked: bool,
    closing: bool,
    stop_children: bool,
    commands: HashMap<String, (Option<tokio::sync::oneshot::Sender<()>>, tokio::task::JoinHandle<()>)>,
    unconfirmed_commands: HashMap<String, proc::OwnedTree>,
    owns_server: bool,
    server_writers: bool,
    stop_reported: bool,
    /// A tool server already used by this runtime may keep state between turns.
    mcp_used: bool,
    /// Local commands or hooks may leave a child beyond the terminal the service reported.
    process_tools_used: bool,
}

const COMMAND_EXIT_PREFIX: &str = "Process exit code: ";
type CommandNotice = (u64, Value, proc::CommandOutput);
fn native_tree_empty(server: &proc::OwnedTree) -> bool {
    let original_empty = server.descendants().is_ok_and(|children| children.is_empty());
    #[cfg(unix)]
    let outside_empty = server.external_descendants().is_ok_and(|children| children.is_empty());
    #[cfg(windows)]
    let outside_empty = true;
    original_empty && outside_empty
}

fn refresh_native(threads: &mut HashMap<u64, Thread>, key: Option<u64>, server: Option<&proc::OwnedTree>, no_process: bool) -> Vec<u64> {
    let clear = no_process || server.is_some_and(native_tree_empty);
    threads
        .iter_mut()
        .filter_map(|(&owner, t)| {
            // Unix terminals have separate groups, so every thread shares uncertainty. A Windows timer samples the
            // same job once for all waiting threads; a turn or stop refreshes only its owner.
            if (cfg!(unix) || key.map_or_else(|| t.waiting_on_native_tree(), |key| owner == key)) && t.server_writers == clear {
                t.server_writers = !clear;
                Some(owner)
            } else {
                None
            }
        })
        .collect()
}

impl Thread {
    fn stop_deadline(&self) -> Option<tokio::time::Instant> {
        self.watch.deadline().map(|deadline| {
            self.stop_completion_grace.filter(|&(original, _)| original == deadline).map_or(deadline, |(_, extended)| extended)
        })
    }

    fn waiting_on_native_tree(&self) -> bool {
        self.server_writers && self.turn.is_none() && !self.watch.active()
    }

    fn rearming_goal(&self) -> bool {
        self.identity.as_ref().is_some_and(|i| i.native_goal == NativeGoalOpen::Rearm)
    }

    fn reveal_overlapping_turn(&mut self) {
        if self.blocked || self.closing || self.stop_children || self.watch.stopping() {
            self.turn_notice = TurnNotice::Suppressed;
        } else {
            self.turn_notice = TurnNotice::Visible;
            (self.sink)(EngineEvent::OwnTurn);
            (self.sink)(EngineEvent::TurnStarted);
        }
    }

    fn open_params(&self, area: Area, supports_thread_options: bool) -> (&'static str, Value) {
        let (mut method, mut params) = thread_params(area, &self.params);
        if supports_thread_options {
            // A protected resume waits for this runtime to release its old identity.
            params["config"]["thread_unload_delay_secs"] = 2.into();
            if method == "thread/resume" {
                // Faber has its own transcript and only needs the native ID and model from this reply.
                params["excludeTurns"] = true.into();
            }
        }
        match self.goal_resume {
            GoalResume::Hold => params["config"]["features.goals"] = false.into(),
            GoalResume::Rearm { confirmed_goal: true } => params["config"]["features.goals"] = true.into(),
            _ => {}
        }
        // The person's "never touch" rules (decided 2026-11-13): a guarded sandbox keeps them read-only, for commands too.
        if self.params.permissions.guarded()
            && self.params.instructions.is_none()
            && let Some(config) = params["config"].as_object_mut()
        {
            crate::permissions::keep_read_only(config, &self.params.tuning.protected);
        }
        if let Some(fork) = &self.native_fork {
            method = "thread/fork";
            params["threadId"] = fork.point.session.clone().into();
            params["lastTurnId"] = fork.point.cursor.turn.clone().expect("validated fork").into();
            params["excludeTurns"] = true.into();
            params.as_object_mut().expect("thread object").remove("dynamicTools");
        }
        if let Some(identity) = &self.identity
            && let Some(token) = &identity.identity.token
        {
            // Merge with the sandbox's environment (HOME/TEMP), rather than replacing it.
            let config = params["config"].as_object_mut().expect("thread config");
            let env = config.entry("shell_environment_policy.set").or_insert_with(|| json!({}));
            env["FABER_AGENT_TOKEN"] = token.clone().into();
            env["FABER_AGENT_AUTH"] = token.clone().into();
            env["FABER_CONVERSATION"] = identity.identity.id.clone().into();
            env[proc::AGENT_MARK] = "1".into();
        }
        (method, params)
    }

    fn native_pending(&self, key: u64, subagents: &Subagents) -> bool {
        !self.state.native_commands.items.is_empty()
            || self.state.native_commands.unknown()
            || subagents
                .0
                .values()
                .any(|sub| sub.key == key && (!sub.state.native_commands.items.is_empty() || sub.state.native_commands.unknown()))
    }

    fn report_stop(&mut self, key: u64, subagents: &Subagents) {
        self.stop_reported = true;
        self.publish_writers(key, subagents);
        if self.blocked || self.server_writers || self.state.native_commands.unknown() || subagents.writers(key).1 {
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
                        exit_code: None,
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
            && self.unconfirmed_commands.is_empty()
            && self.helpers.is_empty()
            && self.state.native_commands.items.is_empty()
            && !self.state.native_commands.unknown()
            && helpers.is_empty()
            && !unknown
    }

    fn can_release(&self, key: u64, subagents: &Subagents, no_process: bool, has_tree: bool) -> bool {
        self.closing
            && !matches!(
                self.native_release,
                NativeRelease::UnconfirmedOpen | NativeRelease::ReleaseLateOpen | NativeRelease::OnIdle | NativeRelease::Pending { .. }
            )
            && self.drained(key, subagents)
            && (no_process || (has_tree && self.id.is_some() && !self.server_writers))
    }

    fn ready_to_release_idle(&self, key: u64, subagents: &Subagents, id: &str) -> bool {
        !self.closing
            && self.native_release == NativeRelease::Keep
            && !self.mcp_used
            && !(self.server_writers && self.process_tools_used)
            && self.id.as_deref() == Some(id)
            && self.queued.is_empty()
            && self.compact == CompactRequest::None
            && self.approvals.is_empty()
            && self.questions.is_empty()
            && self.drained(key, subagents)
    }

    fn publish_writers(&self, key: u64, subagents: &Subagents) {
        let (mut active, mut unknown) = subagents.writers(key);
        for id in self.commands.keys() {
            active.push(format!("command:{id}"));
        }
        for id in self.state.native_commands.items.keys() {
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
            || matches!(
                self.native_release,
                NativeRelease::UnconfirmedOpen
                    | NativeRelease::ReleaseLateOpen
                    | NativeRelease::OnClose
                    | NativeRelease::OnIdle
                    | NativeRelease::Pending { .. }
            )
            || !self.unconfirmed_commands.is_empty()
            || self.state.native_commands.unknown()
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
    error: Option<serde::de::IgnoredAny>,
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
    // Rule 2 of docs/product/models.md: every turn names its effort, never the account's own.
    let effort = settings.effort.clone().filter(|e| !e.is_empty()).unwrap_or_else(|| crate::router::EFFORT.into());
    let mut params = turn_params(thread, Some(&effort), turn);
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
                "reasoning_effort": effort, "developer_instructions": null }
        });
    }
    // thread/start or thread/resume already sets this session's sandbox and approval policy from the same mode.
    params
}

/// The catalog's servers (`crate::tools`) beside the person's own, each under its own key so no other server is replaced.
pub fn add_servers(params: &mut Value, servers: &[crate::tools::Launch]) {
    for s in servers {
        // Codex starts MCP with its safe baseline; inherit no additional shell variables from user config.
        params["config"][format!("mcp_servers.{}", s.name)] =
            json!({ "command": s.command, "args": s.args, "env": s.env, "env_vars": [], "enabled": true });
    }
}

pub fn thread_params(area: Area, t: &CodexThread) -> (&'static str, Value) {
    // Rule 2 of docs/product/models.md: a thread always names its model. The engine resolves its choice first, from the
    // account's list; this is the last resort for a caller that chose none and knows no account.
    let model = t.model.clone().filter(|m| !m.is_empty()).unwrap_or_else(|| crate::router::default_model(Vendor::Codex, None, &[]));
    let mut params = codex_thread(area, Some(&model), t.permissions);
    if t.temporary {
        params.insert("ephemeral".into(), true.into());
    }
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
        if !t.tuning.bench_persona_off {
            let mut words = persona(&PersonaInput { name: t.user_name.as_deref(), tuning: &t.tuning, research: t.research });
            if area == Area::Project {
                words.push_str(&crate::persona::project_details(&t.tuning));
                if matches!(t.helper, crate::helper::Helper::Is | crate::helper::Helper::Reviewer) {
                    words.push_str(" As a helper, return an ordinary report without a faber-wrap-up block.");
                }
                if t.helper == crate::helper::Helper::Offers {
                    words.push_str(crate::helper::HELPERS);
                }
                if t.progress {
                    words.push_str(crate::persona::BIGGER_OFFER);
                }
                if t.progress && t.tuning.assumptions {
                    words.push_str(crate::persona::ASSUMPTIONS);
                }
                if t.leads {
                    words.push_str(crate::persona::LEADS);
                }
            }
            if let Some(instructions) = &t.group_instructions {
                words.push_str("\n\nThe person's instructions for this chat group:\n");
                words.push_str(instructions);
            }
            if t.permissions == Permissions::Isolated {
                words.push_str("\n\nIn run_command, search files with rg PATTERN . or rg PATTERN FILE. Use -t TYPE to filter file types. With no path, rg can read the empty input stream. If Git exclude metadata cannot be read, retry rg with --no-ignore-exclude.");
            }
            params.insert("developerInstructions".into(), words.into());
        }
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
            crate::helper::Helper::Reviewer => {
                params.insert("ephemeral".into(), true.into());
            }
            crate::helper::Helper::Is => {
                params.insert("ephemeral".into(), true.into());
                params.entry("dynamicTools").or_insert_with(|| json!([])).as_array_mut().unwrap().push(crate::helper::team_codex_tool());
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
                if !t.tuning.bench_checks_off {
                    tools.push(crate::verify::codex_tool());
                }
                tools.push(crate::map::codex_tool());
            }
        }
        if t.progress {
            let tools = params.entry("dynamicTools").or_insert_with(|| json!([]));
            if let Some(tools) = tools.as_array_mut() {
                if !t.tuning.bench_review_off {
                    tools.push(crate::review::codex_tool());
                }
                tools.extend(crate::progress::codex_tools());
                tools.extend(
                    crate::toolbox::codex_tools()
                        .into_iter()
                        .filter(|tool| !t.tuning.bench_creative_off || tool["name"] != crate::toolbox::CREATIVE),
                );
            }
        }
        if t.faber {
            params.entry("dynamicTools").or_insert_with(|| json!([])).as_array_mut().unwrap().push(crate::faber_tool::codex_tool());
        }
    }
    if t.permissions == Permissions::Isolated && t.instructions.is_none() {
        params.insert("config".into(), crate::permissions::sandbox_config(&t.cwd, true).into());
        params["config"]["features.shell_tool"] = false.into();
    }
    if t.permissions.guarded() && t.instructions.is_none() {
        params.remove("sandbox");
        let config = params.entry("config").or_insert_with(|| json!({})).as_object_mut().unwrap();
        config.extend(crate::permissions::data_sandbox_config(&t.cwd, t.permissions != Permissions::Plan));
    }
    // Codex's own subagents (`helper::Subagents`): its documented defaults for every agent it spawns without naming a model.
    if let Some(team) =
        t.subagents.as_ref().filter(|_| area == Area::Project && t.helper == crate::helper::Helper::Offers && t.instructions.is_none())
    {
        let config = params.entry("config").or_insert_with(|| json!({}));
        config["agents.default_subagent_model"] = team.build.model.clone().into();
        if let Some(effort) = &team.build.effort {
            config["agents.default_subagent_reasoning_effort"] = effort.clone().into();
        }
    }
    if (t.permissions != Permissions::FullAccess || t.instructions.is_some()) && t.cwd.is_absolute() {
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

/// A shared Codex app-server; identity belongs to each thread, never its process.
#[derive(Clone)]
pub struct CodexServer {
    tx: UnboundedSender<ServerCommand>,
    next: Arc<AtomicU64>,
    alive: Arc<AtomicBool>,
    opening: Arc<Mutex<()>>,
    tree: Option<proc::OwnedTree>,
    runtime: Handle,
}

pub const SHARED_SERVICE_STOPPED: &str = "reason.sharedServiceStopped";
pub const SERVICE_STARTUP_TIMEOUT: &str = "reason.codexStartupTimeout";
/// Internal: this protected resume stayed queued and the driver never sent its turn.
pub const IDLE_RESUME_UNCONFIRMED: &str = "reason.codexIdleResumeUnconfirmed";
pub const TURN_START_FAILURE: &str = "Codex turn/start failed:";
const HEALTH_GRACE: Duration = Duration::from_secs(5);
const STARTUP_GRACE: Duration = Duration::from_secs(30);
const THREAD_OPEN_GRACE: Duration = Duration::from_secs(30);
const THREAD_RESTORE_GRACE: Duration = Duration::from_secs(90);
const MCP_HEALTH_QUIET: Duration = Duration::from_secs(15 * 60);
const RELEASE_FIRST_RECHECK: Duration = Duration::from_secs(15);
const RELEASE_RECHECK: Duration = Duration::from_secs(75);
const RELEASE_RESUME_GRACE: Duration = Duration::from_secs(90);
const RELEASE_RESUME_DELAY: Duration = Duration::from_millis(100);
const COMPACT_CONFIRM_GRACE: Duration = Duration::from_secs(5);
const HEALTH_REPLY_ID: &str = "faber-shared-service-health";

/// The greeting's first field is `faber/<CLI version>`, using initialize's client name.
fn cli_version(greeting: &Value) -> Option<(u32, u32, u32)> {
    let version = greeting["userAgent"].as_str()?.split_ascii_whitespace().next()?.strip_prefix("faber/")?;
    let mut parts = version.split('.').map(|part| {
        (part.bytes().all(|byte| byte.is_ascii_digit()) && (part.len() == 1 || !part.starts_with('0')))
            .then(|| part.parse::<u32>().ok())
            .flatten()
    });
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(Some(major)), Some(Some(minor)), Some(Some(patch)), None) => Some((major, minor, patch)),
        _ => None,
    }
}

impl CodexServer {
    /// The process starts right away, so a warmed-up conversation is already booting while the user types.
    pub fn start(opts: CodexServerOptions) -> CodexServer {
        let runtime = Handle::current();
        let (tx, rx) = unbounded_channel();
        let args = codex_args(opts.area, opts.windows, opts.user_hooks, opts.permissions);
        let started = (opts.spawn)(&opts.cli.command, &args, &opts.cwd);
        let tree = started.as_ref().ok().and_then(|p| p.contained.as_ref()).and_then(proc::Containment::owned_tree);
        let alive = Arc::new(AtomicBool::new(true));
        let opening = Arc::new(Mutex::new(()));
        runtime.spawn(serve(started, opts, rx, alive.clone(), opening.clone()));
        CodexServer { tx, next: Arc::new(AtomicU64::new(1)), alive, opening, tree, runtime }
    }

    pub fn owned_tree(&self) -> Option<proc::OwnedTree> {
        self.tree.clone()
    }

    /// False once the process has stopped; the engine then starts a new one.
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire) && !self.tx.is_closed()
    }

    /// Opens a conversation as a thread. The session behaves as if it had the process to itself.
    pub fn open(&self, thread: CodexThread, sink: EventSink) -> Session {
        self.open_request(CodexOpen::new(thread, sink))
    }

    /// Uses the project's catalog choices for this launch, leaving already open conversations alone.
    pub fn open_with_tools(&self, thread: CodexThread, sink: EventSink, tools: crate::tools::Delivery) -> Session {
        self.open_request(CodexOpen { tools: Some(tools), ..CodexOpen::new(thread, sink) })
    }

    pub fn open_identified(
        &self,
        thread: CodexThread,
        sink: EventSink,
        tools: crate::tools::Delivery,
        identity: ThreadIdentity,
        before_turn: Option<String>,
        native_fork: Option<NativePoint>,
    ) -> Session {
        self.open_request(CodexOpen { thread, sink, tools: Some(tools), identity: Some(identity), before_turn, native_fork })
    }

    pub fn open_fork(&self, thread: CodexThread, point: NativePoint, sink: EventSink) -> Session {
        self.open_request(CodexOpen { native_fork: Some(point), ..CodexOpen::new(thread, sink) })
    }

    /// An idle shutdown competes with admission, not with a conversation already in the queue.
    /// Return the untouched launch only when no Open was sent.
    pub fn try_open(&self, open: CodexOpen) -> Result<Session, Box<CodexOpen>> {
        let admission = self.opening.lock().unwrap();
        if !self.is_alive() {
            return Err(Box::new(open));
        }
        Ok(self.open_inner(open, Some(admission)))
    }

    /// Only one replacement is attempted, so the second service reports its own failure normally.
    pub fn open_request(&self, open: CodexOpen) -> Session {
        self.open_inner(open, None)
    }

    fn open_inner(&self, open: CodexOpen, admission: Option<MutexGuard<'_, ()>>) -> Session {
        let CodexOpen { thread, sink, tools, identity, before_turn, native_fork } = open;
        let session_identity = identity.as_ref().map(|i| i.identity.clone());
        let sink =
            identity.as_ref().and_then(|i| i.identity.token.clone()).map_or_else(|| sink.clone(), |token| redact_sink(sink.clone(), token));
        let key = self.next.fetch_add(1, Ordering::Relaxed);
        let sent = self.tx.send(ServerCommand::Open { key, thread: Box::new(thread), sink, tools, identity, before_turn, native_fork });
        drop(admission);
        if let Err(SendError(ServerCommand::Open { sink, .. })) = sent {
            // The caller can hold the engine lock, which the callback acquires.
            self.runtime.spawn(async move {
                sink(EngineEvent::Error { message: "error.driverStopped".into(), fatal: true });
                sink(EngineEvent::WriterExit { confirmed: true });
            });
        }
        // Straight into the server's queue, so commands from all conversations keep the order they were given in.
        let server = self.tx.clone();
        let session = Session::from_fn(move |cmd| drop(server.send(ServerCommand::Session(key, cmd)))).with_tree(self.tree.clone());
        if let Some(identity) = session_identity { session.with_identity(identity) } else { session }
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

pub fn redact_sink(sink: EventSink, token: String) -> EventSink {
    use crate::background::{Capture, Update};
    let pending = std::sync::Mutex::new((String::new(), String::new(), HashMap::<String, String>::new()));
    let original = sink;
    Arc::new(move |mut event| {
        match &mut event {
            EngineEvent::Tool { input, .. }
            | EngineEvent::BrowseCall { input, .. }
            | EngineEvent::ConversationCall { input, .. }
            | EngineEvent::VerifyCall { input, .. }
            | EngineEvent::MapCall { input, .. }
            | EngineEvent::FaberCall { input, .. }
            | EngineEvent::ToolboxCall { input, .. }
            | EngineEvent::ProgressCall { input, .. }
            | EngineEvent::TeamCall { input, .. }
            | EngineEvent::HelperCall { input, .. }
            | EngineEvent::ReviewCall { input, .. }
            | EngineEvent::Permission { input, .. }
            | EngineEvent::Parked { input, .. } => redact_value(input, &token),
            EngineEvent::Background(crate::background::Update::Start { command, .. }) => *command = command.replace(&token, "[redacted]"),
            _ => {}
        }
        let mut pending = pending.lock().unwrap_or_else(|e| e.into_inner());
        match event {
            EngineEvent::Text(delta) => {
                let safe = redact_delta(&mut pending.0, &delta, &token);
                if !safe.is_empty() {
                    original(EngineEvent::Text(safe));
                }
            }
            EngineEvent::Thinking(delta) => {
                let safe = redact_delta(&mut pending.1, &delta, &token);
                if !safe.is_empty() {
                    original(EngineEvent::Thinking(safe));
                }
            }
            EngineEvent::Background(Update::Output { id, text, capture: Capture::Terminal, source_truncated }) => {
                pending.2.remove(&id);
                original(EngineEvent::Background(Update::Output {
                    id,
                    text: text.replace(&token, "[redacted]"),
                    capture: Capture::Terminal,
                    source_truncated,
                }));
            }
            EngineEvent::Background(Update::Output { id, text, capture, source_truncated }) => {
                let buffer = pending.2.entry(id.clone()).or_default();
                let replace = capture != Capture::Delta;
                if replace || source_truncated {
                    buffer.clear();
                }
                let safe = redact_delta(buffer, &text, &token);
                if replace || source_truncated || !safe.is_empty() {
                    original(EngineEvent::Background(Update::Output { id, text: safe, capture, source_truncated }));
                }
            }
            EngineEvent::Background(Update::End { id, state }) => {
                if let Some(words) = pending.2.remove(&id).filter(|words| !words.is_empty()) {
                    original(EngineEvent::Background(Update::Output {
                        id: id.clone(),
                        text: words,
                        capture: Capture::Delta,
                        source_truncated: false,
                    }));
                }
                original(EngineEvent::Background(Update::End { id, state }));
            }
            EngineEvent::Error { message, fatal } => {
                original(EngineEvent::Error { message: message.replace(&token, "[redacted]"), fatal });
            }
            EngineEvent::ToolResult { id, ok, declined, exit_code, text } => {
                original(EngineEvent::ToolResult { id, ok, declined, exit_code, text: text.replace(&token, "[redacted]") });
            }
            EngineEvent::HelperDone { id, ok, text } => {
                if let Some(words) = pending.2.remove(&format!("child:{id}")).filter(|words| !words.is_empty()) {
                    original(EngineEvent::ChildWords { parent: id.clone(), text: words });
                }
                original(EngineEvent::HelperDone { id, ok, text: text.replace(&token, "[redacted]") });
            }
            EngineEvent::ChildWords { parent, text } => {
                let buffer = pending.2.entry(format!("child:{parent}")).or_default();
                let safe = redact_delta(buffer, &text, &token);
                if !safe.is_empty() {
                    original(EngineEvent::ChildWords { parent, text: safe });
                }
            }
            other => {
                if matches!(&other, EngineEvent::Break { .. } | EngineEvent::Done { .. } | EngineEvent::WriterExit { .. }) {
                    if !pending.0.is_empty() {
                        original(EngineEvent::Text(std::mem::take(&mut pending.0)));
                    }
                    if !pending.1.is_empty() {
                        original(EngineEvent::Thinking(std::mem::take(&mut pending.1)));
                    }
                }
                original(other);
            }
        }
    })
}

fn redact_value(value: &mut Value, token: &str) {
    match value {
        Value::String(text) => *text = text.replace(token, "[redacted]"),
        Value::Array(values) => values.iter_mut().for_each(|value| redact_value(value, token)),
        Value::Object(values) => {
            let original = std::mem::take(values);
            for (key, mut value) in original {
                redact_value(&mut value, token);
                values.insert(key.replace(token, "[redacted]"), value);
            }
        }
        _ => {}
    }
}

fn redact_delta(pending: &mut String, delta: &str, token: &str) -> String {
    pending.push_str(delta);
    let mut safe = pending.replace(token, "[redacted]");
    let keep = safe.char_indices().map(|(index, _)| index).find(|&index| token.starts_with(&safe[index..])).unwrap_or(safe.len());
    *pending = safe.split_off(keep);
    safe
}

#[derive(Default)]
struct WireTokens {
    values: HashSet<String>,
    has_other_format: bool,
}

impl WireTokens {
    // The channel currently mints this prefix. Other formats still use exact matching.
    const PREFIX: &str = "fa1.";

    fn insert(&mut self, token: String) {
        self.has_other_format |= !token.starts_with(Self::PREFIX);
        self.values.insert(token);
    }

    fn redact(&self, mut line: String) -> String {
        if self.values.is_empty() || (!self.has_other_format && !line.contains(Self::PREFIX)) {
            return line;
        }
        for token in &self.values {
            if line.contains(token) {
                line = line.replace(token, "[redacted]");
            }
        }
        line
    }
}

async fn serve(
    started: std::io::Result<proc::Proc>,
    opts: CodexServerOptions,
    rx: UnboundedReceiver<ServerCommand>,
    alive: Arc<AtomicBool>,
    opening: Arc<Mutex<()>>,
) {
    let mut rx = rx;
    let area = opts.area;
    let mut p = match started {
        Ok(p) => p,
        Err(err) => {
            alive.store(false, Ordering::Release);
            // Every conversation already waiting hears why; the next one starts a fresh process.
            rx.close();
            while let Some(cmd) = rx.recv().await {
                if let ServerCommand::Open { sink, .. } = cmd {
                    let message = format!("Codex could not start: {err}");
                    sink(EngineEvent::Error { message, fatal: true });
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
    let begin_idle_stop = |rx: &UnboundedReceiver<ServerCommand>| {
        let _admission = opening.lock().unwrap();
        if !rx.is_empty() {
            return false;
        }
        alive.store(false, Ordering::Release);
        true
    };
    // A torn line poisons the whole process stream, including writes without an acknowledgement.
    macro_rules! write_checked {
        ($msg:expr) => {{
            let result = proc::write_line(&mut *stdin, $msg).await;
            if result.is_err_and(|e| e.torn) {
                shutdown_confirmed |= proc::stop(&mut *stdin, &mut child, &mut job).await;
            }
            result
        }};
    }
    let mut threads: HashMap<u64, Thread> = HashMap::new();
    // Late output from a closed thread must not expose its revoked token to a peer.
    let mut tokens = WireTokens::default();
    let mut by_id: HashMap<String, u64> = HashMap::new();
    let mut subagents = Subagents::default();
    // Opened before Codex finished starting, in order, so request ids stay predictable.
    let mut pending: Vec<u64> = Vec::new();
    let mut resuming = HashMap::<u64, tokio::time::Instant>::new();
    let mut reopening = HashMap::<u64, tokio::time::Instant>::new();
    let mut waiting: HashMap<String, Waiting> = HashMap::new();
    let mut exposure_pending = HashMap::<u64, tokio::time::Instant>::new();
    let mut confirmed = VecDeque::new();
    let mut next_id = 0u64;
    let (mut ready, mut retired, mut stopping) = (false, false, false);
    let mut received_open = false;
    let mut guarded_mxc = false;
    let mut release_subscription = false;
    let mut tools = opts.tools.clone();
    let (command_tx, mut command_rx) = unbounded_channel::<CommandNotice>();

    macro_rules! request {
        ($method:expr, $params:expr, $kind:expr) => {{
            next_id += 1;
            waiting.insert(next_id.to_string(), $kind);
            let _ = write_checked!(&json!({ "id": next_id, "method": $method, "params": $params }));
        }};
    }
    macro_rules! start_turn {
        ($key:expr, $thread:expr, $id:expr, $turn:expr) => {{
            let t = $thread;
            let key = $key;
            if !t.watch.active() && t.turn.is_none() {
                // An old ACK can arrive after its turn completed and a new one started.
                waiting.retain(|_, w| !matches!(w, Waiting::TurnStart(old) if *old == key));
                t.state.base = t.state.total;
            }
            t.watch.begin();
            t.publish_writers(key, &subagents);
            request!("turn/start", start_params($id, &mut t.params, &t.state.model, $turn), Waiting::TurnStart(key));
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
    macro_rules! settle_release {
        ($key:expr, $state:expr) => {{
            if let Some(t) = threads.get_mut(&$key).filter(|t| matches!(t.native_release, NativeRelease::Pending { .. })) {
                t.native_release = $state;
                let id = t.id.clone();
                waiting.retain(|_, w| !matches!(w, Waiting::Release(key, ..) if *key == $key));
                refresh_writers!(Some($key));
                threads[&$key].publish_writers($key, &subagents);
                if let Some(id) = id {
                    let soon = tokio::time::Instant::now() + RELEASE_RESUME_DELAY;
                    for (key, at) in &mut reopening {
                        if threads.get(key).is_some_and(|t| t.params.resume.as_ref() == Some(&id)) {
                            *at = (*at).min(soon);
                        }
                    }
                }
            }
        }};
    }
    macro_rules! stop_native {
        ($key:expr) => {{
            let key = $key;
            refresh_writers!(Some(key));
            if let Some(t) = threads.get_mut(&key) {
                t.retry_commands(key, &command_tx);
            }
            let mut commands = Vec::new();
            if let Some(t) = threads.get(&key) && let Some(id) = &t.id {
                commands.extend(t.state.native_commands.stops(id));
            }
            for (id, sub) in &subagents.0 {
                if sub.key == key { commands.extend(sub.state.native_commands.stops(id)); }
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
    macro_rules! fail_before_open {
        ($key:expr, $reason:expr) => {{
            if let Some(t) = threads.remove(&$key) {
                if let Some(identity) = &t.identity {
                    identity.identity.revoke();
                }
                // Even after Stop settled its reply, the engine must retire this handle before another send can use it.
                if !t.closing {
                    (t.sink)(EngineEvent::Error { message: $reason.into(), fatal: true });
                }
                // No thread or turn was sent, even if another conversation still uses this service.
                (t.sink)(EngineEvent::WriterExit { confirmed: true });
            }
        }};
    }
    macro_rules! abandon_thread_open {
        ($key:expr, $message:expr) => {{
            if let Some(t) = threads.get_mut(&$key) {
                let report = !t.closing;
                t.closing = true;
                t.stop_children = true;
                t.queued.clear();
                t.compact = CompactRequest::None;
                t.native_release = NativeRelease::UnconfirmedOpen;
                if let Some(identity) = &t.identity {
                    identity.identity.revoke();
                }
                // The provider may have opened a runtime even though it never took a user turn.
                t.publish_writers($key, &subagents);
                if report {
                    (t.sink)(EngineEvent::Error { message: $message.into(), fatal: true });
                }
            }
        }};
    }
    macro_rules! cleanup_late_open {
        ($key:expr, $native:expr) => {{
            if let Some(t) = threads.get_mut(&$key) {
                if let Some(id) = $native.filter(|id| !by_id.contains_key(*id) && !subagents.0.contains_key(*id)) {
                    by_id.insert(id.into(), $key);
                    t.id = Some(id.into());
                    t.native_release = if release_subscription { NativeRelease::ReleaseLateOpen } else { NativeRelease::UnconfirmedOpen };
                    stop_native!($key);
                } else {
                    t.native_release = NativeRelease::UnconfirmedOpen;
                    t.publish_writers($key, &subagents);
                }
            }
        }};
    }
    macro_rules! discard_fork_tail {
        ($key:expr) => {{
            let opened = threads.get_mut(&$key).and_then(|t| t.native_fork.as_mut()).and_then(|fork| fork.reply.take());
            abandon_thread_open!($key, "reason.codexThreadOpenTimeout");
            cleanup_late_open!($key, opened.as_ref().and_then(|reply| reply["thread"]["id"].as_str()));
        }};
    }
    macro_rules! send_thread_open {
        ($method:expr, $params:expr, $key:expr) => {{
            let grace = if $method == "thread/start" { THREAD_OPEN_GRACE } else { THREAD_RESTORE_GRACE };
            request!($method, $params, Waiting::Thread($key, Some(tokio::time::Instant::now() + grace)));
        }};
    }
    macro_rules! open_thread {
        ($key:expr, $t:expr) => {{
            if $t.native_fork.is_none() && !$t.unsubscribed
                && $t.identity.as_ref().is_some_and(|i| i.identity.token.is_some() || i.native_goal != NativeGoalOpen::Ordinary)
                && let Some(id) = &$t.params.resume {
                let attached = threads.iter().any(|(&key, old)| key != $key && old.id.as_ref() == Some(id) && old.native_release != NativeRelease::Unloaded);
                let opening = || waiting.values().filter_map(|w| match w {
                    Waiting::Unsubscribe(owner, _) | Waiting::Goal(owner, _) | Waiting::Thread(owner, _) | Waiting::Revert(owner, _) => Some(owner),
                    _ => None,
                }).chain(exposure_pending.keys()).chain(reopening.keys()).any(|&owner| {
                    owner != $key && threads.get(&owner).is_some_and(|old| old.params.resume.as_ref() == Some(id))
                });
                if attached || opening() {
                    // Reloading a live runtime kills its terminals. Once unload is confirmed, a shared process-tree
                    // lease can stay with the retired owner while the successor loads the saved thread again.
                    resuming.entry($key).or_insert_with(|| tokio::time::Instant::now() + THREAD_RESTORE_GRACE);
                } else {
                    resuming.remove(&$key);
                    // A subscribed loaded thread ignores resume overrides, including a newly minted token.
                    let request_id = format!("faber-unsubscribe-{}", $key);
                    waiting.insert(request_id.clone(), Waiting::Unsubscribe($key, tokio::time::Instant::now() + THREAD_OPEN_GRACE));
                    if write_checked!(&json!({"id":request_id,"method":"thread/unsubscribe","params":{"threadId":id}})).is_err() {
                        stop!();
                    }
                }
            } else if $t.goal_resume == GoalResume::Check && let Some(id) = &$t.params.resume {
                request!("thread/goal/get", json!({ "threadId": id }), Waiting::Goal($key, tokio::time::Instant::now() + HEALTH_GRACE));
            } else if $t.params.permissions == Permissions::Isolated || $t.params.instructions.is_some() {
                exposure_pending.insert($key, tokio::time::Instant::now() + STARTUP_GRACE);
                if write_checked!(&json!({"id":format!("faber-config-{}", $key), "method":"config/read",
                    "params":{"includeLayers":false,"cwd":$t.params.cwd}})).is_err() {
                    stop!();
                }
            } else {
                let (method, mut params) = $t.open_params(area, release_subscription);
                if guarded_mxc && $t.params.permissions.guarded() {
                    params["config"]["windows.sandbox"] = "mxc".into();
                }
                let delivery = $t.tools.as_ref().unwrap_or(&tools);
                if area == Area::Project && $t.params.instructions.is_none() && let Some(card) = &delivery.start_card {
                    let words = params["developerInstructions"].as_str().unwrap_or_default();
                    params["developerInstructions"] = format!("{words}\n\n{card}").into();
                }
                // extraRoots is process-wide; discovery and bounded instructions belong only to this thread.
                // Faber's own guide (R18): its line goes into a new thread only; a resumed one keeps what it was started with, and the
                // first input carries the line when the thread has not had it.
                let guide = delivery.guide.note().filter(|_| method == "thread/start");
                let instructions = [delivery.project_skill_descriptions(Vendor::Codex), delivery.skill_instructions(), guide]
                    .into_iter().flatten().collect::<Vec<_>>().join("\n\n");
                if !instructions.is_empty() {
                    let words = params["developerInstructions"].as_str().unwrap_or_default();
                    params["developerInstructions"] = format!("{words}\n\n{instructions}").into();
                }
                add_servers(&mut params, &delivery.servers);
                for (key, value) in delivery.codex_agent_overrides() {
                    params["config"][key] = value;
                }
                for name in &delivery.disabled_servers {
                    if let Some(server) = params["config"].get_mut(format!("mcp_servers.{name}")) {
                        server["enabled"] = false.into();
                    }
                }
                send_thread_open!(method, params, $key);
            }
        }};
    }
    macro_rules! start_compaction {
        ($key:expr, $t:expr, $id:expr) => {{
            $t.compact = CompactRequest::Sent(None);
            $t.watch.begin();
            $t.publish_writers($key, &subagents);
            request!("thread/compact/start", json!({ "threadId": $id }), Waiting::Other($key, "thread/compact/start"));
        }};
    }

    let init = json!({ "clientInfo": { "name": "faber", "title": "Faber", "version": "0.0.0" }, "capabilities": { "experimentalApi": true, "requestAttestation": false } });
    request!("initialize", init, Waiting::Initialize);

    let mut native_poll_at = None;
    let mut last_wire = tokio::time::Instant::now();
    let initialize_by = last_wire + STARTUP_GRACE;
    let mut active_since = None;
    let mut health_until = None;
    let mut service_failure = None;
    let mut last_health_check = None;
    macro_rules! heard_wire {
        () => {{
            last_wire = tokio::time::Instant::now();
            if health_until.take().is_some()
                && let Some(sent) = last_health_check
            {
                for t in threads.values_mut().filter(|t| !t.closing && !t.blocked && t.turn.is_some()) {
                    if let Some(d) = t.watch.deadline().filter(|d| *d <= sent)
                        && t.stop_completion_grace.is_none_or(|(original, _)| original != d)
                    {
                        // The service answered. Allow the separate completion notice to follow, once per Stop.
                        t.stop_completion_grace = Some((d, last_wire + HEALTH_GRACE));
                    }
                }
            }
        }};
    }
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
            refresh_writers!(Some(key));
            if let Some(t) = threads.get_mut(&key) {
                t.report_stop(key, &subagents);
            }
        }
        let mut release_deadline = None;
        if release_subscription && !stopping {
            let now = tokio::time::Instant::now();
            let has_peer = threads.values().any(|t| !t.closing);
            let releases: Vec<_> = threads
                .iter()
                .filter_map(|(&key, t)| {
                    let (first, idle) = match t.native_release {
                        NativeRelease::OnIdle => (true, true),
                        NativeRelease::OnClose | NativeRelease::ReleaseLateOpen if has_peer => (true, false),
                        NativeRelease::Pending { recheck: Some(at), idle } if idle || has_peer => {
                            if at > now {
                                release_deadline = Some(release_deadline.map_or(at, |other: tokio::time::Instant| other.min(at)));
                                return None;
                            }
                            (false, idle)
                        }
                        _ => return None,
                    };
                    let id = t.id.as_ref()?;
                    if !t.drained(key, &subagents)
                        || by_id.get(id) != Some(&key)
                        || threads.iter().any(|(other, peer)| {
                            peer.id.is_none()
                                && !resuming.contains_key(other)
                                && !reopening.contains_key(other)
                                && (peer.params.resume.as_ref() == Some(id)
                                    || peer.native_fork.as_ref().is_some_and(|fork| &fork.point.session == id))
                        })
                    {
                        return None;
                    }
                    Some((key, id.clone(), first, idle, first && t.native_release == NativeRelease::OnClose))
                })
                .collect();
            for (key, id, first, idle, allow_unsupported) in releases {
                // Shutdown can start a hook. Keep the process-tree check and wait for shutdown to finish.
                // If the close notification is lost, the same request can establish that the runtime is gone.
                let recheck = if first { Some(now + RELEASE_FIRST_RECHECK) } else { (!idle).then(|| now + RELEASE_RECHECK) };
                threads.get_mut(&key).unwrap().native_release = NativeRelease::Pending { recheck, idle };
                if let Some(at) = recheck {
                    release_deadline = Some(release_deadline.map_or(at, |other| other.min(at)));
                }
                waiting.retain(|_, w| !matches!(w, Waiting::Release(owner, ..) if *owner == key));
                next_id += 1;
                let request_id = format!("faber-release-{next_id}");
                match write_checked!(&json!({"id":request_id,"method":"thread/unsubscribe","params":{"threadId":id}})) {
                    Ok(()) => {
                        let expires = (idle && !first).then(|| now + HEALTH_GRACE);
                        waiting.insert(request_id, Waiting::Release(key, allow_unsupported, expires));
                    }
                    Err(e) if !e.torn => {
                        if allow_unsupported {
                            settle_release!(key, NativeRelease::Settled);
                        }
                    }
                    Err(_) => break,
                }
            }
        }
        // Close/Retire and completion refresh the native tree; unresolved descendants use the existing one-second poll.
        let closed: Vec<u64> = threads
            .iter()
            .filter(|(key, t)| {
                t.can_release(**key, &subagents, no_process, server_tree.is_some())
                    && (t.id.is_some()
                        || !waiting
                            .values()
                            .any(|w| matches!(w, Waiting::Thread(owner, Some(_)) | Waiting::ForkTail(owner, _) if owner == *key)))
            })
            .map(|(&key, _)| key)
            .collect();
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
        let resumes: Vec<_> = resuming.iter().map(|(&key, &at)| (at, key)).collect();
        let now = tokio::time::Instant::now();
        for (at, key) in resumes {
            if at <= now {
                resuming.remove(&key);
                let parked = threads
                    .get(&key)
                    .and_then(|t| t.params.resume.as_ref())
                    .and_then(|id| by_id.get(id))
                    .and_then(|owner| threads.get(owner))
                    .is_some_and(|owner| matches!(owner.native_release, NativeRelease::OnIdle | NativeRelease::Pending { idle: true, .. }));
                // Nothing reached the old history or took the new turn. Let the engine recover from its own saved transcript.
                fail_before_open!(key, if parked { IDLE_RESUME_UNCONFIRMED } else { "reason.codexThreadOpenTimeout" });
            } else if let Some(t) = threads.get(&key).filter(|t| !t.closing) {
                open_thread!(key, t);
            }
        }
        let mut empty_idle_job = None;
        if !stopping
            && ((received_open || retired) && threads.is_empty()
                || (!threads.is_empty()
                    && threads.iter().all(|(&key, t)| {
                        t.closing
                            && (t.stop_children || (t.drained(key, &subagents) && !t.server_writers))
                            && match t.native_release {
                                NativeRelease::OnIdle => false,
                                NativeRelease::Pending { idle: true, recheck } => {
                                    recheck.is_none()
                                        && !waiting.values().any(|w| matches!(w, Waiting::Release(owner, ..) if *owner == key))
                                        && *empty_idle_job.get_or_insert_with(|| server_tree.as_ref().is_some_and(native_tree_empty))
                                }
                                _ => true,
                            }
                            && !t.native_pending(key, &subagents)
                            && ((!t.watch.active() && t.turn.is_none() && !t.interrupt_pending) || (!no_process && t.blocked))
                    })))
            && begin_idle_stop(&rx)
        {
            stop!();
        }
        let interrupt_deadline = threads.values().filter_map(Thread::stop_deadline).min();
        let compact_deadline = threads
            .values()
            .filter_map(|t| match t.compact {
                CompactRequest::Sent(deadline) if !t.blocked => deadline,
                _ => None,
            })
            .min();
        let mut active = threads.values().filter(|t| {
            !t.closing && !t.blocked && t.approvals.is_empty() && t.questions.is_empty() && (t.turn.is_some() || t.watch.active())
        });
        let first = active.next();
        let shared_work = ready && active.next().is_some();
        let quiet_for = if !ready || threads.values().any(|t| !t.approvals.is_empty() || !t.questions.is_empty()) {
            None
        } else if shared_work {
            Some(Duration::from_secs_f64(crate::stuck::QUIET))
        } else {
            first.filter(|t| t.turn.is_some() && !t.state.probe_tools.is_empty()).map(|t| {
                if t.state.probe_tools.values().any(|kind| *kind == ProbeTool::Command) {
                    Duration::from_secs_f64(crate::stuck::QUIET_COMMAND)
                } else {
                    MCP_HEALTH_QUIET
                }
            })
        };
        let quiet_deadline = if let Some(quiet_for) = quiet_for {
            let since = *active_since.get_or_insert_with(tokio::time::Instant::now);
            Some(last_wire.max(since) + quiet_for)
        } else {
            active_since = None;
            None
        };
        let deadline = health_until.or_else(|| {
            interrupt_deadline.into_iter().chain(quiet_deadline).chain(compact_deadline).chain((!ready).then_some(initialize_by)).min()
        });
        let goal_deadline = waiting
            .values()
            .filter_map(|w| match w {
                Waiting::Goal(_, at) | Waiting::IdleGoal(_, at, _) => Some(*at),
                _ => None,
            })
            .min();
        let release_deadline = waiting
            .values()
            .filter_map(|w| match w {
                Waiting::Release(_, _, expires) => *expires,
                _ => None,
            })
            .chain(release_deadline)
            .min();
        let config_deadline = exposure_pending.values().copied().min();
        let open_deadline = waiting
            .values()
            .filter_map(|w| match w {
                Waiting::Thread(_, at) | Waiting::Revert(_, at) => *at,
                Waiting::Unsubscribe(_, at) | Waiting::ForkTail(_, at) => Some(*at),
                _ => None,
            })
            .chain(resuming.values().copied())
            .min();
        let reopen_deadline = reopening.values().copied().min();
        if !no_process && threads.values().any(Thread::waiting_on_native_tree) {
            native_poll_at.get_or_insert_with(|| tokio::time::Instant::now() + Duration::from_secs(1));
        } else {
            native_poll_at = None;
        }
        tokio::select! {
            () = proc::wait_deadline(release_deadline) => {
                let now = tokio::time::Instant::now();
                waiting.retain(|_, w| !matches!(w, Waiting::Release(_, _, Some(at)) if *at <= now));
            }
            () = proc::wait_deadline(reopen_deadline) => {
                let now = tokio::time::Instant::now();
                let due: Vec<_> = reopening.extract_if(|_, at| *at <= now).map(|(key, _)| key).collect();
                for key in due {
                    if let Some(t) = threads.get(&key).filter(|t| !t.closing) {
                        open_thread!(key, t);
                    }
                }
            }
            () = proc::wait_deadline(goal_deadline) => {
                let now = tokio::time::Instant::now();
                for (_, w) in waiting.extract_if(|_, w| matches!(w, Waiting::IdleGoal(_, at, _) if *at <= now)) {
                    if let Waiting::IdleGoal(_, _, release) = w { release.retry(); }
                }
                let overdue: Vec<_> = waiting.extract_if(|_, w| matches!(w, Waiting::Goal(_, at) if *at <= now))
                    .filter_map(|(_, w)| if let Waiting::Goal(key, _) = w { Some(key) } else { None })
                    .collect();
                for key in overdue {
                    if let Some(t) = threads.get_mut(&key).filter(|t| !t.closing) {
                        t.goal_resume = if t.rearming_goal() { GoalResume::Rearm { confirmed_goal: false } } else { GoalResume::Hold };
                    }
                    if let Some(t) = threads.get(&key).filter(|t| !t.closing) {
                        open_thread!(key, t);
                    }
                }
            }
            () = proc::wait_deadline(config_deadline) => {
                let now = tokio::time::Instant::now();
                for (key, _) in exposure_pending.extract_if(|_, at| *at <= now) {
                    fail_before_open!(key, "reason.codexConfigTimeout");
                }
            }
            () = proc::wait_deadline(open_deadline) => {
                let now = tokio::time::Instant::now();
                let mut abandoned_forks = Vec::new();
                waiting.retain(|_, w| {
                    match w {
                        Waiting::Thread(key, at) | Waiting::Revert(key, at) if at.is_some_and(|at| at <= now) => {
                            *at = None;
                            abandon_thread_open!(*key, "reason.codexThreadOpenTimeout");
                        }
                        Waiting::Unsubscribe(key, at) if *at <= now => {
                            fail_before_open!(*key, "reason.codexThreadOpenTimeout");
                            return false;
                        }
                        Waiting::ForkTail(key, at) if *at <= now => {
                            abandoned_forks.push(*key);
                            return false;
                        }
                        _ => {}
                    }
                    true
                });
                for key in abandoned_forks {
                    discard_fork_tail!(key);
                }
            }
            () = proc::wait_deadline(native_poll_at) => {
                refresh_writers!(None);
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
                    let mut contents = vec![json!({"type":"inputText","text":result.text})];
                    if let Some(code) = result.exit_code {
                        contents.push(json!({"type":"inputText","text":format!("{COMMAND_EXIT_PREFIX}{code}")}));
                    }
                    let _ = write_checked!(&json!({"id":id,"result":{"success":result.success,"contentItems":contents}}));
                }
            }
            () = proc::wait_deadline(deadline) => {
                let now = tokio::time::Instant::now();
                if health_until.is_some() {
                    service_failure = Some(SHARED_SERVICE_STOPPED);
                    break;
                }
                // No turn was sent before initialize, so the engine can keep using the ordinary queue and retry path.
                if !ready && now >= initialize_by {
                    service_failure = Some(SERVICE_STARTUP_TIMEOUT);
                    break;
                }
                for (&key, t) in threads.iter_mut().filter(|(_, t)| {
                    !t.blocked && t.turn.is_none() && matches!(t.compact, CompactRequest::Sent(end) if
                        end.is_some_and(|d| d <= now) || t.watch.deadline().is_some_and(|d| d <= now))
                }) {
                    // A known turn ended, but an empty ACK cannot prove whether the compact op ran.
                    // Close admission and retain the obligation to stop any ID that arrives late.
                    t.blocked = true;
                    t.watch.finish();
                    t.publish_writers(key, &subagents);
                    (t.sink)(EngineEvent::CompactFinished(CompactOutcome::Unconfirmed));
                }
                let expired: Vec<u64> = threads.iter().filter(|(_, t)| t.stop_deadline().is_some_and(|d| d <= now)).map(|(&k, _)| k).collect();
                let stop_due = ready && expired.iter().any(|key| {
                    let Some(t) = threads.get(key).filter(|t| !t.closing && !t.blocked) else { return false };
                    let Some(deadline) = t.watch.deadline().filter(|d| last_health_check.is_none_or(|at| *d > at)) else { return false };
                    // Acknowledgements and status updates can precede turn/completed. Check once if the end is missing,
                    // protecting questions and approvals on other conversations, not on the turn the person stopped.
                    if threads.iter().any(|(other, t)| other != key && (!t.approvals.is_empty() || !t.questions.is_empty())) { return false; }
                    if t.turn.is_some() { return true; }
                    // Without a known turn, leave late starts with the existing shared-work check.
                    shared_work && last_wire <= deadline - HEALTH_GRACE && waiting.values().any(|w| match w {
                            Waiting::Other(k, "turn/interrupt") | Waiting::TurnStart(k) => k == key,
                            _ => false,
                        })
                });
                if quiet_deadline.is_some_and(|d| d <= now) || stop_due {
                    // A loaded-thread list is local and sends no model request. Any protocol message after the probe
                    // protects peers, even when the CLI version responds with an unknown-method error.
                    let probe = json!({"id":HEALTH_REPLY_ID,"method":"thread/loaded/list","params":{"limit":1}});
                    if proc::write_line(&mut *stdin, &probe).await.is_err() {
                        service_failure = Some(SHARED_SERVICE_STOPPED);
                        break;
                    }
                    let sent = tokio::time::Instant::now();
                    last_health_check = Some(sent);
                    health_until = Some(sent + HEALTH_GRACE);
                    continue;
                }
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
                if let Some(msg) = confirmed.pop_front() { Ok(Some((Value::to_string(&msg), false))) }
                else { lines.next_line().await.map(|line| line.map(|text| (text, true))) }
            } => {
                let Ok(Some((line, from_wire))) = line else {
                    alive.store(false, Ordering::Release);
                    shutdown_confirmed |= proc::stop(&mut *stdin, &mut child, &mut job).await;
                    if !stopping {
                        let code = tokio::time::timeout(Duration::from_secs(1), proc::exit_code(&mut child))
                            .await.unwrap_or_else(|_| "unknown".into());
                        let tail = if tokens.values.is_empty() { tail.lock().unwrap().clone() } else { String::new() };
                        let message = format!("Codex stopped (exit {code}). {tail}").trim().to_string();
                        #[cfg(unix)]
                        refresh_writers!(None);
                        for t in threads.values() {
                            let confirmed = shutdown_confirmed && (cfg!(windows) || !t.server_writers)
                                && t.commands.is_empty() && t.unconfirmed_commands.is_empty();
                            let detail = crate::helper::writer_exit_detail(t.params.helper, confirmed);
                            (t.sink)(EngineEvent::Writers { active: vec![], unknown: true });
                            (t.sink)(EngineEvent::Error { message: format!("{detail}{message}"), fatal: true });
                        }
                    }
                    break;
                };
                let line = tokens.redact(line);
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(reply) = serde_json::from_str::<ConfigReply>(&line)
                    && let Some(key) = reply.id.as_deref().and_then(|id| id.strip_prefix("faber-config-")).and_then(|key| key.parse::<u64>().ok())
                {
                    if from_wire && (reply.result.is_some() || reply.error.is_some()) {
                        heard_wire!();
                    }
                    if exposure_pending.remove(&key).is_none() { continue; }
                    let Some(result) = reply.result.filter(|_| reply.error.is_none()) else {
                        fail_before_open!(key, "error.projectSandbox");
                        continue;
                    };
                    let Some(t) = threads.get(&key) else { continue };
                    let (method, mut params) = t.open_params(area, release_subscription);
                    if area == Area::Project && t.params.instructions.is_none()
                        && let Some(card) = t.tools.as_ref().and_then(|delivery| delivery.start_card.as_deref()) {
                        let words = params["developerInstructions"].as_str().unwrap_or_default();
                        params["developerInstructions"] = format!("{words}\n\n{card}").into();
                    }
                    let disabled: Map<String, Value> = result.config.mcp_servers.into_keys().map(|name| (name, json!({"enabled":false}))).collect();
                    params["config"]["mcp_servers"] = disabled.into();
                    send_thread_open!(method, params, key);
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
                if from_wire
                    && (method.is_some()
                        || msg.get("id").is_some_and(|id| id.is_number() || id.is_string())
                            && (msg.get("result").is_some() || msg["error"].is_object()))
                {
                    heard_wire!();
                }
                let params = &msg["params"];
                if method == Some("thread/closed") {
                    if let Some(key) = params["threadId"].as_str().and_then(|id| by_id.get(id)).copied() {
                        settle_release!(key, NativeRelease::Unloaded);
                    }
                    continue;
                }
                // This is also before server requests such as a dynamic tool call, which need a reply to belong to.
                if method.is_some_and(|m| m.starts_with("item/") || m == "thread/compacted")
                    && let Some(key) = params["threadId"].as_str().and_then(|id| by_id.get(id))
                    && let Some(t) = threads.get_mut(key)
                    && params["turnId"].as_str().is_some_and(|id| t.turn.as_deref() == Some(id))
                    && matches!(t.turn_notice, TurnNotice::Classifying | TurnNotice::Compacting { .. })
                {
                    if params["item"]["type"] == "contextCompaction" || method == Some("thread/compacted") {
                        let applied = matches!(method, Some("item/completed" | "thread/compacted"))
                            || t.turn_notice == (TurnNotice::Compacting { applied: true });
                        if t.turn_notice == TurnNotice::Classifying && !t.blocked {
                            (t.sink)(EngineEvent::TurnStarted);
                        }
                        t.turn_notice = TurnNotice::Compacting { applied };
                    } else {
                        // A goal can compact inline before its first words; later regular items belong to its reply.
                        t.reveal_overlapping_turn();
                    }
                }
                if let Some(key) = msg["id"].as_str().and_then(|id| id.strip_prefix("faber-commands-")).and_then(|key| key.parse::<u64>().ok()) {
                    if let Some(t) = threads.get(&key) && let Some(commands) = skill_commands(&msg["result"], &t.params.cwd) {
                        (t.sink)(EngineEvent::Commands(commands));
                    }
                    continue;
                }
                match (method, msg.get("id")) {
                    // An answer to one of Faber's requests.
                    (None, Some(id)) => {
                        let Some(kind) = waiting.remove(&id.as_str().map_or_else(|| id.to_string(), str::to_owned)) else { continue };
                        if let Waiting::Thread(key, at) = kind {
                            if at.is_some_and(|at| at <= tokio::time::Instant::now()) {
                                abandon_thread_open!(key, "reason.codexThreadOpenTimeout");
                            }
                            if let Some(t) = threads.get(&key).filter(|t| t.closing) {
                                // These protocol errors reject the call before running the opening method.
                                if msg.get("result").is_none() && matches!(msg["error"]["code"].as_i64(), Some(-32601 | -32602)) {
                                    let t = threads.remove(&key).expect("known opening");
                                    (t.sink)(EngineEvent::WriterExit { confirmed: true });
                                    continue;
                                }
                                // A late open can only give us an ID for cleanup. It cannot claim a peer's runtime,
                                // validate a fork, apply a rewind, or start any of the held work.
                                let thread = msg["result"]["thread"]["id"].as_str().filter(|id| {
                                    msg["error"].is_null()
                                        && !id.is_empty()
                                        && t.native_fork.as_ref().is_none_or(|fork| fork.point.session != *id)
                                        && (t.before_turn.is_none() || t.params.resume.as_deref() == Some(*id))
                                });
                                cleanup_late_open!(key, thread);
                                continue;
                            }
                        }
                        if let Waiting::Revert(key, at) = kind {
                            let Some(t) = threads.get(&key) else { continue };
                            let original = t.params.resume.clone();
                            let returned = msg["result"]["thread"]["id"].as_str();
                            let valid = returned.is_some() && returned == original.as_deref() && msg["error"].is_null();
                            let rejected = msg.get("result").is_none() && msg["error"].is_object();
                            if t.closing || at.is_some_and(|at| at <= tokio::time::Instant::now()) || !valid {
                                let message = if t.closing || at.is_some_and(|at| at <= tokio::time::Instant::now()) {
                                    "reason.codexThreadOpenTimeout".into()
                                } else if let Some(detail) = msg["error"]["message"].as_str() {
                                    format!("Codex thread failed: {detail}")
                                } else {
                                    "error.sessionLost".into()
                                };
                                abandon_thread_open!(key, message);
                                // Revert can still rewrite and reload this runtime after cancellation. Unsubscribe
                                // only after its reply, and only from the original thread if no peer owns it.
                                cleanup_late_open!(key, original.as_deref().filter(|_| valid || rejected));
                                continue;
                            }
                        }
                        if let Waiting::ForkTail(key, at) = kind
                            && (at <= tokio::time::Instant::now() || threads.get(&key).is_some_and(|t| t.closing))
                        {
                            discard_fork_tail!(key);
                            continue;
                        }
                        if let Waiting::Release(key, allow_unsupported, _) = kind {
                            if msg["result"]["status"] == "notLoaded" {
                                settle_release!(key, NativeRelease::Unloaded);
                            } else if allow_unsupported && msg["error"]["code"] == -32601 {
                                settle_release!(key, NativeRelease::Settled);
                            }
                            continue;
                        }
                        if let Waiting::Unsubscribe(key, at) = kind {
                            if at <= tokio::time::Instant::now() {
                                fail_before_open!(key, "reason.codexThreadOpenTimeout");
                            } else if msg["error"].is_null()
                                && matches!(msg["result"]["status"].as_str(), Some("unsubscribed" | "notSubscribed" | "notLoaded"))
                            {
                                if let Some(t) = threads.get_mut(&key) { t.unsubscribed = true; }
                            } else {
                                fail_before_open!(key, "error.channel.token");
                            }
                            if let Some(t) = threads.get(&key).filter(|t| t.unsubscribed && !t.closing) {
                                open_thread!(key, t);
                            }
                            continue;
                        }
                        if let Waiting::IdleGoal(key, at, release) = kind {
                            let safe = if msg["error"].is_null() {
                                msg["result"].get("goal").and_then(|goal| {
                                    if goal.is_null() { Some(true) } else { goal["status"].as_str().map(|status| status == "complete") }
                                })
                            } else if msg.get("result").is_none() && msg["error"]["code"] == -32600
                                && msg["error"]["message"] == "goals feature is disabled"
                            {
                                Some(true)
                            } else {
                                None
                            };
                            if safe.is_none() || at <= tokio::time::Instant::now() {
                                release.retry();
                                continue;
                            }
                            refresh_writers!(Some(key));
                            if safe == Some(true)
                                && by_id.get(&release.thread) == Some(&key)
                                && let Some(t) = threads.get_mut(&key).filter(|t| t.ready_to_release_idle(key, &subagents, &release.thread))
                                && release.accept()
                            {
                                t.closing = true;
                                t.native_release = NativeRelease::OnIdle;
                                if let Some(identity) = &t.identity { identity.identity.revoke(); }
                                t.publish_writers(key, &subagents);
                            }
                            continue;
                        }
                        if let Waiting::Goal(key, _) = kind {
                            if let Some(t) = threads.get_mut(&key).filter(|t| !t.closing) {
                                // A missing or finished goal must not prevent the next one from starting. On an
                                // unconfirmed state, keep the hold without modifying the stored goal.
                                let goal = msg["result"].get("goal");
                                let safe = goal.is_some_and(|goal| {
                                    goal.is_null() || goal["status"] == "complete"
                                });
                                t.goal_resume = if safe {
                                    GoalResume::Allow
                                } else if !t.rearming_goal() {
                                    GoalResume::Hold
                                } else {
                                    GoalResume::Rearm {
                                        confirmed_goal: goal.is_some_and(|g| matches!(g["status"].as_str(), Some("active" | "paused" | "budgetLimited" | "usageLimited"))),
                                    }
                                };
                                if safe && let Some(session_id) = t.params.resume.clone() {
                                    (t.sink)(EngineEvent::NativeGoalHoldCleared { session_id, after_rearm: false });
                                }
                            }
                            if let Some(t) = threads.get(&key).filter(|t| !t.closing) {
                                open_thread!(key, t);
                            }
                            continue;
                        }
                        if let Waiting::McpStatus { thread, reply, mut rows, mut cursors } = kind {
                            if reply.is_closed() { continue; }
                            let result = &msg["result"];
                            let Some(data) = result["data"].as_array().filter(|data| msg["error"].is_null() && rows.len() + data.len() <= 256) else {
                                let _ = reply.send(None);
                                continue;
                            };
                            rows.extend(crate::mcp_servers::reported(data, "runtimeStatus"));
                            if let Some(cursor) = result["nextCursor"].as_str() {
                                if cursor.len() > 1024 || cursors.len() >= 16 || !cursors.insert(cursor.into()) {
                                    let _ = reply.send(None);
                                } else {
                                    request!("mcpServerStatus/list", json!({"threadId":thread,"detail":"toolsAndAuthOnly","limit":100,"cursor":cursor}), Waiting::McpStatus { thread:thread.clone(), reply, rows, cursors });
                                }
                            } else if result["nextCursor"].is_null() {
                                let _ = reply.send(Some(rows));
                            } else {
                                let _ = reply.send(None);
                            }
                            continue;
                        }
                        if let Some(error) = msg.get("error").filter(|error| !error.is_null()) {
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
                            if release_subscription && error["code"] == -32600
                                && let Waiting::Thread(key, _) = kind
                                && let Some((id, retry)) = detail.strip_prefix("thread ").and_then(|text| text.split_once(" is closing; retry "))
                                && matches!(retry, "thread/resume after the thread is closed" | "after the thread is closed")
                            {
                                let unloading = by_id.get(id).and_then(|old| threads.get(old))
                                    .is_some_and(|t| matches!(t.native_release, NativeRelease::Pending { .. }));
                                if let Some(t) = threads.get_mut(&key).filter(|t| !t.closing && !t.retried_closing_resume
                                    && t.native_fork.is_none() && t.params.resume.as_deref() == Some(id))
                                {
                                    // The server rejected the open before accepting any input. Retry it once.
                                    t.retried_closing_resume = true;
                                    let wait = if unloading { RELEASE_RESUME_GRACE } else { RELEASE_RESUME_DELAY };
                                    reopening.insert(key, tokio::time::Instant::now() + wait);
                                    continue;
                                }
                            }
                            if let Waiting::Thread(key, _) | Waiting::ForkTail(key, _) = kind
                                && threads.get(&key).is_some_and(|t| t.before_turn.is_some() || t.native_fork.is_some())
                                && let Some(t) = threads.remove(&key)
                            {
                                // No turn was sent: release this unopened owner without stopping the shared server.
                                (t.sink)(EngineEvent::Error { message: format!("Codex thread failed: {detail}"), fatal: true });
                                (t.sink)(EngineEvent::WriterExit { confirmed: true });
                                continue;
                            }
                            if let Waiting::Other(key, "thread/compact/start") = kind {
                                if let Some(t) = threads.get_mut(&key).filter(|t| matches!(t.compact, CompactRequest::Sent(_))) {
                                    t.compact = CompactRequest::None;
                                    if t.turn_notice == TurnNotice::Classifying {
                                        t.reveal_overlapping_turn();
                                    }
                                    if t.turn.is_none() && !turn_start_pending(&waiting, key) {
                                        t.watch.finish();
                                        t.interrupt_pending = false;
                                    }
                                    t.publish_writers(key, &subagents);
                                    (t.sink)(EngineEvent::CompactFinished(CompactOutcome::Failed));
                                }
                                continue;
                            }
                            if let Waiting::TurnStart(key) = kind {
                                if let Some(t) = threads.get_mut(&key).filter(|t| t.watch.active()) {
                                    let no_turn = t.turn.is_none() && t.turn_notice == TurnNotice::Pending
                                        && !turn_start_pending(&waiting, key);
                                    let refused = no_turn && t.compact == CompactRequest::None;
                                    if refused {
                                        t.watch.finish();
                                        t.interrupt_pending = false;
                                        t.state.probe_tools.clear();
                                        t.early_completion.clear();
                                        t.publish_writers(key, &subagents);
                                    }
                                    if !t.closing && !t.blocked {
                                        (t.sink)(EngineEvent::Error { message: format!("{TURN_START_FAILURE} {detail}"), fatal: refused });
                                        if no_turn && !refused {
                                            // The request ended. The compaction can still hold the session.
                                            (t.sink)(EngineEvent::Done { ok: false, reason: "start refused".into(), cost_total_usd: Some(0.0),
                                                input_tokens: 0, output_tokens: 0, cache_read_tokens: 0, cache_write_tokens: 0,
                                                cache_write_1h_tokens: 0, incomplete: false });
                                        }
                                        if no_turn && t.compact == CompactRequest::Queued && let Some(id) = t.id.clone() {
                                            // No completion will arrive to advance the queued compaction.
                                            start_compaction!(key, t, id);
                                        }
                                    }
                                }
                                continue;
                            }
                            let (keys, what, fatal): (Vec<u64>, &str, bool) = match kind {
                                Waiting::Initialize => (threads.keys().copied().collect(), "initialize", true),
                                Waiting::Thread(key, _) | Waiting::ForkTail(key, _) => (vec![key], "thread", true),
                                Waiting::Revert(..) => unreachable!("handled above"),
                                Waiting::Other(key, m) => (vec![key], m, false),
                                Waiting::TurnStart(_) => unreachable!("handled above"),
                                Waiting::Steer(..) => unreachable!("handled above"),
                                Waiting::McpStatus { .. } => unreachable!("handled above"),
                                Waiting::Unsubscribe(..) => unreachable!("handled above"),
                                Waiting::Release(..) => unreachable!("handled above"),
                                Waiting::Goal(..) => unreachable!("handled above"),
                                Waiting::IdleGoal(..) => unreachable!("handled above"),
                                Waiting::NativeStop(key, ..) | Waiting::NativeGone(key, ..) => (vec![key], "thread/backgroundTerminals/terminate", false),
                            };
                            for t in keys.iter().filter_map(|k| threads.get(k)) {
                                if matches!(kind, Waiting::NativeStop(..) | Waiting::NativeGone(..)) {
                                    if no_process { (t.sink)(EngineEvent::Error { message: "error.rewind.stopUnconfirmed".into(), fatal: false }); }
                                } else {
                                    (t.sink)(EngineEvent::Error { message: format!("Codex {what} failed: {detail}"), fatal });
                                }
                            }
                            continue;
                        }
                        if let Waiting::ForkTail(key, _) = kind {
                            let valid = threads.get(&key).and_then(|t| t.native_fork.as_ref()).is_some_and(|fork| {
                                msg["result"]["data"].as_array().is_some_and(|tail| tail.len() == 1
                                    && tail[0]["id"].as_str() == fork.point.cursor.turn.as_deref() && tail[0]["status"] != "inProgress")
                            });
                            if !valid {
                                if let Some(t) = threads.remove(&key) {
                                    (t.sink)(EngineEvent::Error { message: "error.sessionLost".into(), fatal: true });
                                    (t.sink)(EngineEvent::WriterExit { confirmed: true });
                                }
                            } else if let Some(fork) = threads.get_mut(&key).and_then(|t| t.native_fork.as_mut()) {
                                fork.verified = true;
                                let id = format!("faber-fork-verified-{key}");
                                waiting.insert(id.clone(), Waiting::Thread(key, None));
                                confirmed.push_back(json!({"id":id,"result":fork.reply.take().expect("pending fork metadata")}));
                            }
                            continue;
                        }
                        match kind {
                            Waiting::Unsubscribe(..) => unreachable!("handled above"),
                            Waiting::Release(..) => unreachable!("handled above"),
                            Waiting::Goal(..) => unreachable!("handled above"),
                            Waiting::IdleGoal(..) => unreachable!("handled above"),
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
                                refresh_writers!(Some(key));
                                if let Some(t) = threads.get_mut(&key) {
                                    if t.id.as_deref() == Some(&thread) { t.state.native_commands.confirm_gone(&item, &process); }
                                    else if let Some(sub) = subagents.0.get_mut(&thread).filter(|sub| sub.key == key) {
                                        sub.state.native_commands.confirm_gone(&item, &process);
                                    }
                                    t.publish_writers(key, &subagents);
                                }
                            }
                            Waiting::Initialize => {
                                let version = cli_version(&msg["result"]);
                                // 0.166.0 is the first release checked with the protected writable and Plan profiles.
                                guarded_mxc = opts.windows && version.is_some_and(|v| v >= (0, 166, 0));
                                // 0.166.0 is verified to notify when an unsubscribed runtime finishes shutting down.
                                release_subscription = version.is_some_and(|v| v >= (0, 166, 0));
                                let _ = write_checked!(&json!({ "method": "initialized" }));
                                // The catalog's skills (`crate::tools`), before any thread opens; its answer needs nothing.
                                if let Some(root) = &tools.skills {
                                    let set = json!({ "id": "faber-skills", "method": "skills/extraRoots/set", "params": { "extraRoots": [root] } });
                                    let _ = write_checked!(&set);
                                }
                                ready = true;
                                for key in std::mem::take(&mut pending) {
                                    let Some(t) = threads.get(&key) else { continue };
                                    open_thread!(key, t);
                                }
                            }
                            Waiting::Thread(key, _) | Waiting::Revert(key, _) | Waiting::ForkTail(key, _) => {
                                let thread = &msg["result"]["thread"];
                                let id = s(&thread["id"]);
                                if threads.get(&key).is_some_and(|t| t.before_turn.is_some() && (id.is_empty() || t.params.resume.as_deref() != Some(id.as_str())))
                                    && let Some(t) = threads.remove(&key)
                                {
                                    (t.sink)(EngineEvent::Error { message: "error.sessionLost".into(), fatal: true });
                                    (t.sink)(EngineEvent::WriterExit { confirmed: true });
                                    continue;
                                }
                                let Some(t) = threads.get_mut(&key) else { continue };
                                if let Some(fork) = &mut t.native_fork {
                                    if id.is_empty() || id == fork.point.session {
                                        (t.sink)(EngineEvent::Error { message: "error.sessionLost".into(), fatal: true });
                                        (t.sink)(EngineEvent::WriterExit { confirmed: true });
                                        threads.remove(&key);
                                        continue;
                                    }
                                    if !fork.verified {
                                        // Retain metadata only while checking one turn summary, never the copied transcript.
                                        fork.reply = Some(json!({"thread":{"id":id,"model":thread["model"]},"model":msg["result"]["model"]}));
                                        request!("thread/turns/list", json!({"threadId":id,"limit":1,"sortDirection":"desc","itemsView":"summary"}),
                                            Waiting::ForkTail(key, tokio::time::Instant::now() + THREAD_RESTORE_GRACE));
                                        continue;
                                    }
                                }
                                if matches!(kind, Waiting::Thread(..)) && let Some(before_turn) = &t.before_turn {
                                    t.state.model = msg["result"]["model"].as_str().unwrap_or_default().to_string();
                                    request!("thread/revert", json!({"threadId":id,"beforeTurnId":before_turn}),
                                        Waiting::Revert(key, Some(tokio::time::Instant::now() + THREAD_RESTORE_GRACE)));
                                    continue;
                                }
                                let actual_model = msg["result"]["model"].as_str().or_else(|| thread["model"].as_str()).unwrap_or_default().to_string();
                                let actual_model = if actual_model.is_empty() { t.state.model.clone() } else { actual_model };
                                t.before_turn = None;
                                let shown = if actual_model.is_empty() { "codex".into() } else { actual_model.clone() };
                                t.state.model = actual_model;
                                (t.sink)(EngineEvent::Started { session_id: id.clone(), model: shown });
                                let replaced = by_id.insert(id.clone(), key);
                                t.id = Some(id.clone());
                                if matches!(t.goal_resume, GoalResume::Rearm { .. }) && t.params.resume.as_ref() == Some(&id) {
                                    (t.sink)(EngineEvent::NativeGoalHoldCleared { session_id: id.clone(), after_rearm: true });
                                }
                                if t.params.instructions.is_none() && let Some(title) = t.params.title.take() {
                                    // An untracked reply keeps older app-servers' unsupported-method errors silent.
                                    let _ = write_checked!(&json!({"id":format!("faber-name-{key}"), "method":"thread/name/set",
                                        "params":{"threadId":id,"name":title.chars().take(60).collect::<String>()}}));
                                }
                                let _ = write_checked!(&json!({"id":format!("faber-commands-{key}"), "method":"skills/list",
                                    "params":{"cwds":[t.params.cwd]}}));
                                if t.compact == CompactRequest::Queued {
                                    start_compaction!(key, t, id);
                                }
                                for turn in std::mem::take(&mut t.queued) {
                                    start_turn!(key, &mut *t, &id, &turn);
                                }
                                if let Some(old) = replaced.filter(|old| *old != key) {
                                    // The loaded runtime is subscribed again, or this is a new runtime after unload.
                                    settle_release!(old, NativeRelease::Settled);
                                }
                            }
                            Waiting::TurnStart(key) => {
                                if let Some(t) = threads.get_mut(&key)
                                    .filter(|t| (t.watch.active() || t.interrupt_pending) && t.turn_notice == TurnNotice::Pending)
                                {
                                    // When input joins a running goal turn, older servers return a submission ID.
                                    // Once turn/started names the running turn, an RPC reply cannot replace it.
                                    if let Some(turn) = msg["result"]["turn"]["id"].as_str() {
                                        t.turn = Some(turn.into());
                                    }
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
                            Waiting::McpStatus { .. } => unreachable!("handled above"),
                        }
                    }
                    // A request from Codex: approvals reach the user; anything else is declined politely.
                    (Some(method), Some(id)) => {
                        let top = params["threadId"].as_str().and_then(|t| by_id.get(t)).copied();
                        let accepts = |t: &&mut Thread| !t.blocked && !t.closing && !t.watch.stopping() && t.turn_notice != TurnNotice::Suppressed;
                        let thread = top.and_then(|k| threads.get_mut(&k)).filter(accepts);
                        if method == "item/tool/call" && params["tool"] == crate::review::NAME {
                            match thread.filter(|t| t.params.progress && !t.blocked && !t.closing) {
                                Some(t) => {
                                    let call = s(&params["callId"]);
                                    t.helpers.insert(call.clone(), id.clone());
                                    (t.sink)(EngineEvent::ReviewCall { id: call, input: params["arguments"].clone() });
                                }
                                None => { let _ = write_checked!(&json!({"id":id,"result":crate::helper::dynamic_result(false, "Review is not available here.", &[])})); }
                            }
                            continue;
                        }
                        if method == "item/tool/call" && params["tool"] == crate::helper::TEAM_NAME {
                            let refusal = if thread.as_ref().is_some_and(|t| t.params.helper == crate::helper::Helper::Reviewer) {
                                "Nobody can answer: decide yourself."
                            } else {
                                "Team is available only to helpers."
                            };
                            match thread.filter(|t| t.params.helper == crate::helper::Helper::Is && !t.blocked && !t.closing) {
                                Some(t) => {
                                    let call = s(&params["callId"]);
                                    t.helpers.insert(call.clone(), id.clone());
                                    (t.sink)(EngineEvent::TeamCall { id: call, input: params["arguments"].clone() });
                                }
                                None => { let _ = write_checked!(&json!({"id":id,"result":crate::helper::dynamic_result(false, refusal, &[])})); }
                            }
                            continue;
                        }
                        if method == "item/tool/call" && params["tool"] == crate::helper::NAME {
                            // R161: the agent asked for a helper; the engine answers once the helper is done.
                            match thread.filter(|t| t.params.helper == crate::helper::Helper::Offers && !t.blocked && !t.closing) {
                                Some(t) => {
                                    let call = s(&params["callId"]);
                                    t.helpers.insert(call.clone(), id.clone());
                                    (t.sink)(EngineEvent::HelperCall { id: call, input: params["arguments"].clone() });
                                }
                                None => {
                                    let _ = write_checked!(&json!({"id":id,"result":{"success":false,"contentItems":[{"type":"inputText","text":"No helper is available here."}]}}));
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
                                    let _ = write_checked!(&json!({"id":id,"result":{"success":false,"contentItems":[{"type":"inputText","text":"Starting a conversation is not available here."}]}}));
                                }
                            }
                            continue;
                        }
                        if method == "item/tool/call" && params["tool"] == crate::faber_tool::NAME {
                            // One of Faber's own commands: the host checks its reach and the engine answers once it ran.
                            match thread.filter(|t| t.params.faber && !t.blocked && !t.closing) {
                                Some(t) => {
                                    let call = s(&params["callId"]);
                                    t.helpers.insert(call.clone(), id.clone());
                                    (t.sink)(EngineEvent::FaberCall { id: call, input: params["arguments"].clone() });
                                }
                                None => {
                                    let _ = write_checked!(&json!({"id":id,"result":{"success":false,"contentItems":[{"type":"inputText","text":"Faber's commands are not available here."}]}}));
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
                                    let _ = write_checked!(&json!({"id":id,"result":{"success":false,"contentItems":[{"type":"inputText","text":"Faber's project map is not available here."}]}}));
                                }
                            }
                            continue;
                        }
                        if method == "item/tool/call" && let Some(tool) = params["tool"].as_str().and_then(crate::toolbox::tool_of) {
                            // `quota` and `catalog` read only what Faber keeps; the engine answers at once.
                            match thread.filter(|t| t.params.progress && !t.blocked && !t.closing) {
                                Some(t) => {
                                    let call = s(&params["callId"]);
                                    t.helpers.insert(call.clone(), id.clone());
                                    (t.sink)(EngineEvent::ToolboxCall { id: call, tool, input: params["arguments"].clone() });
                                }
                                None => {
                                    let _ = write_checked!(&json!({"id":id,"result":{"success":false,"contentItems":[{"type":"inputText","text":"This tool is not available here."}]}}));
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
                                    let _ = write_checked!(&json!({"id":id,"result":{"success":false,"contentItems":[{"type":"inputText","text":"This tool is not available here."}]}}));
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
                                    let _ = write_checked!(&json!({"id":id,"result":{"success":false,"contentItems":[{"type":"inputText","text":"Faber's checks are not available here."}]}}));
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
                                    let _ = write_checked!(&json!({"id":id,"result":{"success":false,"contentItems":[{"type":"inputText","text":"Faber's browser is not available here."}]}}));
                                }
                            }
                            continue;
                        }
                        if method == "item/tool/call" {
                            let mut accepted = false;
                            if let Some(t) = thread && t.params.permissions == Permissions::Isolated && t.params.instructions.is_none() && params["tool"] == "run_command" {
                                let input = &params["arguments"];
                                let own_command = proc::cli_binary().and_then(|path| crate::permissions::faber_command(input, &path));
                                let prepare_temp = opts.windows && own_command.is_none();
                                if let Some(argv) = own_command.or_else(|| crate::permissions::command_argv(&t.params.cwd, "Bash", input, Some(&opts.cli), &crate::cli::find_cli)) {
                                    let key = top.expect("registered command thread");
                                    let (spawn, cwd, tx, id) = (t.identity.as_ref().map_or_else(|| opts.spawn.clone(), |i| i.spawn.clone()), t.params.cwd.clone(), command_tx.clone(), id.clone());
                                    let identity = t.identity.as_ref().map(|i| i.identity.clone());
                                    let command = id.to_string();
                                    let (cancel, cancelled) = tokio::sync::oneshot::channel();
                                    // The task cannot start until its writer is visible to the engine.
                                    let (admit, admitted) = tokio::sync::oneshot::channel();
                                    let job = tokio::spawn(async move {
                                        if admitted.await.is_err() { return; }
                                        let run = || spawn(&argv[0], &argv[1..], &cwd);
                                        // MXC needs the configured TMP path to exist before the sandbox starts.
                                        let started = if prepare_temp && !crate::permissions::prepare_tmp(&cwd) {
                                            Err(std::io::Error::other("could not prepare the working-folder temporary directory"))
                                        } else if let Some(identity) = &identity {
                                            proc::as_conversation(identity, run)
                                        } else {
                                            run()
                                        };
                                        let mut result = match started {
                                            Ok(p) => proc::command_output(p, cancelled).await,
                                            Err(_) => proc::CommandOutput { success: false, exit_code: None, text: "error.projectSandbox".into(), confirmed: true, unresolved: None },
                                        };
                                        if let Some(token) = identity.as_ref().and_then(|identity| identity.token.as_deref()) {
                                            result.text = result.text.replace(token, "[redacted]");
                                        }
                                        let _ = tx.send((key, id, result));
                                    });
                                    t.commands.insert(command, (Some(cancel), job));
                                    t.publish_writers(key, &subagents);
                                    let _ = admit.send(());
                                    accepted = true;
                                }
                            }
                            if !accepted {
                                let _ = write_checked!(&json!({"id":id,"result":{"success":false,"contentItems":[{"type":"inputText","text":"The working-folder policy declined this command."}]}}));
                            }
                            continue;
                        }
                        // A subagent's approval is its conversation's: the person answers it on the same card, by the same rules.
                        let thread = match top {
                            Some(_) => thread,
                            // Only while the turn it serves is under way: a late request reaches no other reply, and is refused.
                            None => params["threadId"].as_str().and_then(|t| subagents.owner(t)).and_then(|(k, turn)| {
                                threads.get_mut(&k).filter(accepts).filter(|t| turn.is_some() && t.turn == turn)
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
                                    let _ = write_checked!(&json!({ "id": id, "result": { "decision": "decline" } }));
                                    continue;
                                }
                                let key = match id { Value::String(s) => s.clone(), other => other.to_string() };
                                t.approvals.insert(key.clone(), id.clone());
                                let (tool, input) = approval_details(method, params);
                                // The guards read the action alone, as before: the card's explanation never decides a refusal.
                                let action = approval_action(params);
                                // The request carries the folder the command runs in; `auth.json` alone says nothing without it. A
                                // refusal is always safe, so it is written here instead of racing the engine's mode.
                                if crate::permissions::touches_credentials(&action, params["cwd"].as_str(), &|name| std::env::var(name).ok())
                                    || crate::permissions::names_protected(method, &action, params["cwd"].as_str().or_else(|| t.params.cwd.to_str()), &t.params.tuning.protected)
                                    || crate::permissions::denies_data_write(
                                        t.params.permissions,
                                        method,
                                        &action,
                                        params["cwd"].as_str().or_else(|| t.params.cwd.to_str()),
                                    )
                                {
                                    t.approvals.remove(&key);
                                    let _ = write_checked!(&json!({ "id": id, "result": { "decision": "decline" } }));
                                    continue;
                                }
                                let input = if let Some(cwd) = params["cwd"].as_str().filter(|cwd| Some(Path::new(cwd)) != Some(t.params.cwd.as_path())) {
                                    if input.is_string() { json!({"command": input, "cwd": cwd}) }
                                    else if let Some(mut fields) = input.as_object().cloned() { fields.insert("cwd".into(), cwd.into()); Value::Object(fields) }
                                    else if input.is_array() { json!({"changes": input, "cwd": cwd}) }
                                    else { input }
                                } else { input };
                                (t.sink)(EngineEvent::Permission { request_id: key, tool, input });
                            }
                            // A question to the person is a card in every mode, like Claude Code's `AskUserQuestion`; the
                            // answer goes back once the person sends or skips it.
                            (false, Some(t))
                                if method == "item/tool/requestUserInput"
                                    && t.params.instructions.is_none()
                                    && !t.blocked
                                    && !t.closing =>
                            {
                                let Some((card, asked)) = question_card(params) else {
                                    let _ = write_checked!(&json!({ "id": id, "result": question_answers(&[], None) }));
                                    continue;
                                };
                                let key = match id { Value::String(s) => s.clone(), other => other.to_string() };
                                t.approvals.insert(key.clone(), id.clone());
                                t.questions.insert(key.clone(), asked);
                                (t.sink)(EngineEvent::Permission { request_id: key, tool: "AskUserQuestion".into(), input: card });
                            }
                            // Nobody can answer it (a run with its own instructions, a reply that is over): it goes on unanswered.
                            (false, _) if method == "item/tool/requestUserInput" => {
                                let _ = write_checked!(&json!({ "id": id, "result": question_answers(&[], None) }));
                            }
                            // No live conversation or approvals disabled: fail closed without a card.
                            (true, _) => {
                                let _ = write_checked!(&json!({ "id": id, "result": { "decision": "decline" } }));
                            }
                            (false, _) => {
                                let error = json!({ "id": id, "error": { "code": -32601, "message": format!("Faber does not handle {method} yet") } });
                                let _ = write_checked!(&error);
                            }
                        }
                    }
                    _ => {
                        let uses_mcp = matches!(method, Some("item/started" | "item/completed")) && params["item"]["type"] == "mcpToolCall";
                        let may_spawn_local_process = matches!(method, Some("hook/started" | "hook/completed"))
                            || (matches!(method, Some("item/started" | "item/completed")) && (params["item"]["type"] == "commandExecution"
                                || (params["item"]["type"] == "dynamicToolCall" && params["item"]["tool"] == "run_command")));
                        // Observe the parent chain before a completion can forget its terminal.
                        #[cfg(unix)]
                        if params["item"]["type"] == "commandExecution" { refresh_writers!(None); }
                        // A subagent's own thread speaks for the conversation that started it.
                        if let Some(ChildNews { key, turn, events, used }) = subagents.child(&msg) {
                            if method == Some("turn/completed") { refresh_writers!(Some(key)); }
                            if let Some(t) = threads.get_mut(&key) {
                                t.mcp_used |= uses_mcp;
                                t.process_tools_used |= may_spawn_local_process;
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
                                    if matches!(event, EngineEvent::Background(_)) || (current && !t.blocked && t.turn_notice != TurnNotice::Suppressed) { (t.sink)(event); }
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
                                && params["item"]["id"].as_str().is_some_and(|item| t.state.native_commands.items.contains_key(item)))
                            .then_some(key)
                        }));
                        let key = retained.or_else(|| thread_id.and_then(|t| by_id.get(t)).copied());
                        if method == Some("turn/completed") && let Some(key) = key {
                            refresh_writers!(Some(key));
                        }
                        match key.and_then(|k| threads.get_mut(&k).map(|t| (k, t))) {
                            Some((key, t)) => {
                                t.mcp_used |= uses_mcp;
                                t.process_tools_used |= may_spawn_local_process;
                                if method == Some("turn/completed") && params["turn"]["id"].as_str() != t.turn.as_deref() {
                                    if t.turn.is_none() && turn_start_pending(&waiting, key) {
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
                                    let Some(turn) = params["turn"]["id"].as_str().filter(|id| !id.is_empty()) else { continue };
                                    let same = t.turn.as_deref() == Some(turn);
                                    if same && t.turn_notice != TurnNotice::Pending { continue; }
                                    if !same { t.state.probe_tools.clear(); }
                                    let classifying = matches!(t.compact, CompactRequest::Sent(_)) && t.turn.is_none();
                                    if classifying {
                                        t.compact = CompactRequest::Sent(None);
                                    }
                                    let independent = !t.watch.active() && t.turn.is_none() && !t.interrupt_pending;
                                    if independent {
                                        waiting.retain(|_, w| !matches!(w, Waiting::TurnStart(old) if *old == key));
                                    }
                                    let refused = independent && (t.closing || t.stop_children || t.blocked || t.params.instructions.is_some());
                                    // Expiry quarantines the thread but leaves the stop owed to its real turn.
                                    let correct_interrupt = !same && (t.watch.stopping() || t.blocked);
                                    t.turn = Some(turn.into());
                                    t.turn_notice = if classifying {
                                        TurnNotice::Classifying
                                    } else if refused {
                                        TurnNotice::Suppressed
                                    } else {
                                        TurnNotice::Visible
                                    };
                                    t.watch.begin();
                                    if refused { t.watch.interrupt(); }
                                    if (refused || t.interrupt_pending || correct_interrupt) && let Some(thread) = t.id.clone() {
                                        t.interrupt_pending = false;
                                        request!("turn/interrupt", json!({ "threadId": thread, "turnId": turn }), Waiting::Other(key, "turn/interrupt"));
                                    }
                                    t.publish_writers(key, &subagents);
                                    if !t.blocked && !refused && !classifying {
                                        if independent { (t.sink)(EngineEvent::OwnTurn); }
                                        (t.sink)(EngineEvent::TurnStarted);
                                    }
                                }
                                let notice = t.turn_notice;
                                let stopped = t.watch.stopping();
                                let compact_ended = method == Some("turn/completed")
                                    && matches!(notice, TurnNotice::Compacting { .. })
                                    // Codex interrupts the old turn before it starts the requested compaction, even if
                                    // an inline compaction of the old turn managed to finish.
                                    && (stopped || params["turn"]["status"] != "interrupted");
                                if method == Some("turn/completed") {
                                    t.state.probe_tools.clear();
                                    if !matches!(notice, TurnNotice::Classifying | TurnNotice::Compacting { .. }) {
                                        (t.sink)(EngineEvent::Cursor { entry: None, turn: params["turn"]["id"].as_str().map(String::from) });
                                    }
                                    // A subagent still at work has not reported what it used: the reply's usage says so.
                                    if t.turn.as_deref().is_some_and(|turn| subagents.running(key, turn)) {
                                        t.state.subagent_running();
                                    }
                                    // Only a subagent can still be waiting on an approval once the turn is over; its card goes with
                                    // the reply, so the request is refused instead of hanging unseen.
                                    for (key, id) in t.approvals.drain() {
                                        let result = match t.questions.remove(&key) {
                                            Some(_) => question_answers(&[], None),
                                            None => json!({ "decision": "decline" }),
                                        };
                                        let _ = write_checked!(&json!({ "id": id, "result": result }));
                                    }
                                    t.turn = None;
                                    t.turn_notice = TurnNotice::Pending;
                                    if compact_ended {
                                        t.compact = CompactRequest::None;
                                    }
                                    // The compact operation is still on the wire after an overlapping goal turn ends.
                                    if matches!(t.compact, CompactRequest::Sent(_)) {
                                        t.compact = CompactRequest::Sent(Some(tokio::time::Instant::now() + COMPACT_CONFIRM_GRACE));
                                    } else if !compact_ended || !turn_start_pending(&waiting, key) {
                                        t.watch.finish();
                                        t.interrupt_pending = false;
                                    }
                                }
                                if let Some(turn) = t.turn.as_deref()
                                    && params["turnId"] == turn
                                    && let Some(id) = params["item"]["id"].as_str().filter(|id| !id.is_empty())
                                {
                                    let item = &params["item"];
                                    match method {
                                        Some("item/started") if item["status"] == "inProgress" => {
                                            let kind = match item["type"].as_str() {
                                                Some("mcpToolCall") => Some(ProbeTool::Mcp),
                                                Some("commandExecution") => Some(ProbeTool::Command),
                                                _ => None,
                                            };
                                            if let Some(kind) = kind { t.state.probe_tools.insert(id.into(), kind); }
                                        }
                                        Some("item/completed") => { t.state.probe_tools.remove(id); }
                                        _ => {}
                                    }
                                }
                                let tracked = subagents.track(key, params["turnId"].as_str(), &msg);
                                if method != Some("turn/completed") { t.publish_writers(key, &subagents); }
                                for mut event in parse_codex_notification(&msg, &mut t.state).into_iter().chain(tracked) {
                                    if let EngineEvent::Background(crate::background::Update::Start { turn_ended, .. }) = &mut event {
                                        *turn_ended = params["turnId"].as_str().is_some_and(|turn| t.turn.as_deref() != Some(turn));
                                    }
                                    if t.blocked { continue; }
                                    match (notice, event) {
                                        (TurnNotice::Compacting { applied }, EngineEvent::Done { ok, .. }) if compact_ended => {
                                            let outcome = if applied && ok && !stopped { CompactOutcome::Applied } else { CompactOutcome::Failed };
                                            (t.sink)(EngineEvent::CompactFinished(outcome));
                                        }
                                        (TurnNotice::Compacting { .. } | TurnNotice::Classifying, EngineEvent::Error { fatal: false, .. }) => {}
                                        (TurnNotice::Compacting { .. } | TurnNotice::Classifying | TurnNotice::Suppressed,
                                            EngineEvent::Done { .. } | EngineEvent::Reason(_)) => {}
                                        (TurnNotice::Suppressed, event) if !matches!(event, EngineEvent::Background(_)) => {}
                                        (_, event) => (t.sink)(event),
                                    }
                                }
                                t.publish_writers(key, &subagents);
                                if method == Some("turn/completed") && !t.blocked && !t.closing && let Some(id) = t.id.clone() {
                                    if t.compact == CompactRequest::Queued {
                                        start_compaction!(key, t, id);
                                    } else if t.compact == CompactRequest::None {
                                        for turn in std::mem::take(&mut t.queued) {
                                            t.stop_children = false;
                                            start_turn!(key, &mut *t, &id, &turn);
                                        }
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
                Some(ServerCommand::Open { key, thread, sink, tools, identity, before_turn, native_fork }) => {
                    // A cancelled or rejected opening can leave the service empty without reaching the writer release loop.
                    received_open = true;
                    if native_fork.as_ref().is_some_and(|point| !point.valid(crate::model::Vendor::Codex) || before_turn.is_some() || thread.instructions.is_some() || matches!(thread.helper, crate::helper::Helper::Is | crate::helper::Helper::Reviewer | crate::helper::Helper::Aside)) {
                        sink(EngineEvent::Error { message: "error.sessionLost".into(), fatal: true });
                        sink(EngineEvent::WriterExit { confirmed: true });
                        continue;
                    }
                    let source = native_fork.as_ref().map(|point| &point.session).or(thread.resume.as_ref());
                    if let Some(id) = source
                        && (threads.values().any(|old| {
                            let same_source = old.params.resume.as_ref() == Some(id);
                            (same_source && old.before_turn.is_some())
                                || (before_turn.is_some() && !old.closing && (same_source || old.id.as_ref() == Some(id)))
                        }) || (before_turn.is_some() && subagents.0.contains_key(id)))
                    {
                        // A queued replacement must not reopen native history while its previous change may still commit.
                        sink(EngineEvent::Error { message: "error.sessionLost".into(), fatal: true });
                        sink(EngineEvent::WriterExit { confirmed: true });
                        continue;
                    }
                    // A send can ask for proof again after the quiet recheck budget ran out.
                    if let Some(old) = thread.resume.as_ref().and_then(|id| by_id.get(id)).and_then(|key| threads.get_mut(key))
                        && let NativeRelease::Pending { recheck: recheck @ None, idle: true } = &mut old.native_release
                    {
                        *recheck = Some(tokio::time::Instant::now());
                    }
                    let state = CodexTurnState::new(thread.resume.is_some() || native_fork.is_some());
                    if let Some(token) = identity.as_ref().and_then(|i| i.identity.token.clone()) { tokens.insert(token); }
                    let goal_resume = if identity.as_ref().is_some_and(|i| i.native_goal != NativeGoalOpen::Ordinary) { GoalResume::Check } else { GoalResume::Allow };
                    let t = Thread { native_fork: native_fork.map(|point| ForkOpen { point, verified: false, reply: None }), before_turn, sink, params: *thread, tools, identity, unsubscribed: false, native_release: NativeRelease::Keep, retried_closing_resume: false, goal_resume, id: None, turn: None, turn_notice: TurnNotice::Pending, queued: vec![], compact: CompactRequest::None, state, approvals: HashMap::new(), questions: HashMap::new(), helpers: HashMap::new(), watch: proc::TurnWatch::default(), stop_completion_grace: None, interrupt_pending: false, blocked: false, closing: false, stop_children: false, early_completion: vec![], commands: HashMap::new(), unconfirmed_commands: HashMap::new(), owns_server: !no_process, server_writers: false, stop_reported: false, mcp_used: false, process_tools_used: false };
                    if ready {
                        open_thread!(key, &t);
                    } else {
                        pending.push(key);
                    }
                    threads.insert(key, t);
                    #[cfg(unix)]
                    refresh_writers!(Some(key));
                }
                Some(ServerCommand::Session(key, SessionCommand::Close | SessionCommand::Retire))
                    if let Some(Waiting::Revert(_, at)) = waiting.values_mut().find(|w| matches!(w, Waiting::Revert(owner, _) if *owner == key)) => {
                    *at = None;
                    if let Some(t) = threads.get_mut(&key) { t.closing = true; }
                    abandon_thread_open!(key, "reason.codexThreadOpenTimeout");
                }
                Some(ServerCommand::Session(key, SessionCommand::Close | SessionCommand::Retire))
                    if waiting.values().any(|w| matches!(w, Waiting::ForkTail(owner, _) if *owner == key)) => {
                    if let Some(t) = threads.get_mut(&key) { t.closing = true; }
                    waiting.retain(|_, w| !matches!(w, Waiting::ForkTail(owner, _) if *owner == key));
                    discard_fork_tail!(key);
                }
                Some(ServerCommand::Session(key, SessionCommand::Close | SessionCommand::Retire))
                    if exposure_pending.remove(&key).is_some() || resuming.remove(&key).is_some() || reopening.remove(&key).is_some()
                        || waiting.values().any(|w| matches!(w, Waiting::Unsubscribe(k, _) | Waiting::Goal(k, _) if *k == key)) => {
                    // No native thread was opened for this replacement, so it owns no writer to stop.
                    waiting.retain(|_, w| !matches!(w, Waiting::Unsubscribe(k, _) | Waiting::Goal(k, _) if *k == key));
                    if let Some(t) = threads.remove(&key) { (t.sink)(EngineEvent::WriterExit { confirmed: true }); }
                }
                Some(ServerCommand::Session(key, cmd)) => match cmd {
                    SessionCommand::ReleaseIdle(release) => {
                        refresh_writers!(Some(key));
                        if release_subscription && !opts.user_hooks && by_id.get(&release.thread) == Some(&key)
                            && threads.get(&key).is_some_and(|t| t.ready_to_release_idle(key, &subagents, &release.thread))
                            && !waiting.values().any(|w| matches!(w, Waiting::IdleGoal(owner, ..) if *owner == key))
                        {
                            next_id += 1;
                            let id = next_id;
                            let params = json!({"threadId":release.thread});
                            match write_checked!(&json!({"id":id,"method":"thread/goal/get","params":params})) {
                                Ok(()) => {
                                    waiting.insert(id.to_string(), Waiting::IdleGoal(key, tokio::time::Instant::now() + HEALTH_GRACE, release));
                                }
                                Err(e) if e.torn => break,
                                Err(_) => release.retry(),
                            }
                        }
                    }
                    SessionCommand::McpStatus { reply } => {
                        waiting.retain(|_, pending| !matches!(pending, Waiting::McpStatus { reply, .. } if reply.is_closed()));
                        if let Some(thread) = threads.get(&key).filter(|t| !t.closing && !t.blocked).and_then(|t| t.id.clone()) {
                            request!("mcpServerStatus/list", json!({"threadId":thread,"detail":"toolsAndAuthOnly","limit":100}), Waiting::McpStatus { thread:thread.clone(), reply, rows:vec![], cursors:HashSet::new() });
                        } else {
                            let _ = reply.send(None);
                        }
                    }
                    SessionCommand::StopBackground { ack, .. } => { let _ = ack.send(Delivery::Unknown); }
                    SessionCommand::Send(turn) => {
                        let Some(t) = threads.get_mut(&key) else { continue };
                        if t.blocked || t.closing { continue; }
                        if t.watch.stopping() { t.queued.push(turn); continue; }
                        t.stop_children = false;
                        match t.id.clone() {
                            Some(id) => {
                                start_turn!(key, t, &id, &turn);
                            }
                            None => t.queued.push(turn),
                        }
                    }
                    SessionCommand::Compact => {
                        let Some(t) = threads.get_mut(&key) else { continue };
                        if t.blocked || t.closing || t.watch.stopping() {
                            (t.sink)(EngineEvent::CompactFinished(CompactOutcome::Failed));
                        } else if t.compact == CompactRequest::None && !matches!(t.turn_notice, TurnNotice::Compacting { .. }) {
                            // An active goal turn would be interrupted by the compact RPC. Wait if we already know it
                            // is running; its notifications can also cross a request already on the wire.
                            if !t.watch.active() && let Some(id) = t.id.clone() {
                                start_compaction!(key, t, id);
                            } else {
                                t.compact = CompactRequest::Queued;
                            }
                        }
                    }
                    // Codex reports only the whole: there are no parts to count.
                    SessionCommand::ContextUsage | SessionCommand::InterruptQuietSearch(_) => {}
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
                    SessionCommand::Answer { request_id, allow, reply, ack } => {
                        // The approval stays pending until the write worked, so a failed write can be answered again.
                        let delivery = match threads.get(&key).and_then(|t| t.approvals.get(&request_id)).cloned() {
                            None => Delivery::Unknown,
                            Some(id) if threads.get(&key).is_some_and(|t| !t.blocked && !t.closing) => {
                                let result = match threads.get(&key).and_then(|t| t.questions.get(&request_id)) {
                                    Some(asked) => question_answers(asked, reply.as_ref().filter(|_| allow)),
                                    None => json!({ "decision": if allow { "accept" } else { "decline" } }),
                                };
                                match write_checked!(&json!({ "id": id, "result": result })) {
                                    Ok(()) => {
                                        if let Some(t) = threads.get_mut(&key) {
                                            t.approvals.remove(&request_id);
                                            t.questions.remove(&request_id);
                                        }
                                        Delivery::Delivered
                                    }
                                    Err(_) => Delivery::WriteFailed,
                                }
                            }
                            Some(_) => Delivery::Unknown,
                        };
                        let _ = ack.send(delivery);
                    }
                    SessionCommand::HelperReply { id, ok, text, pictures, mut ack } => {
                        // A call no longer pending drops `ack`: the engine then resumes the caller another way.
                        if ack.as_ref().is_some_and(|a| a.is_closed()) { continue; }
                        let Some(t) = threads.get_mut(&key) else { continue };
                        let Some(request) = t.helpers.remove(&id) else { continue };
                        let msg = json!({"id":request,"result":crate::helper::dynamic_result(ok, &text, &pictures)});
                        let written = tokio::select! {
                            biased;
                            _ = async { match ack.as_mut() { Some(a) => a.closed().await, None => std::future::pending().await } } => {
                                shutdown_confirmed |= proc::stop(&mut *stdin, &mut child, &mut job).await;
                                Err(proc::WriteError { torn: true })
                            },
                            written = async { write_checked!(&msg) } => written,
                        };
                        if let (Some(ack), Ok(())) = (ack, written) {
                            let _ = ack.send(Delivery::Delivered);
                        }
                    }
                    SessionCommand::Interrupt => {
                        if let Some(t) = threads.get_mut(&key) {
                            if t.goal_resume == GoalResume::Check && let Some(identity) = t.identity.as_mut()
                                && identity.native_goal == NativeGoalOpen::Rearm
                            {
                                // A later Send resets worker-stop bookkeeping, but does not rearm the old goal.
                                identity.native_goal = NativeGoalOpen::CheckStopped;
                            }
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
                        if let Some(t) = threads.get_mut(&key) && t.compact == CompactRequest::Queued {
                            t.compact = CompactRequest::None;
                            (t.sink)(EngineEvent::CompactFinished(CompactOutcome::Failed));
                        }
                        let idle = threads.get(&key).is_some_and(|t| t.turn.is_none() && !t.watch.active());
                        let busy_peer = threads.iter().any(|(&owner, t)| owner != key && (!t.drained(owner, &subagents)
                            || t.server_writers || t.id.is_none() || !t.queued.is_empty()));
                        let native_pending = threads.get(&key).is_some_and(|t| t.native_pending(key, &subagents));
                        if !no_process && idle && !busy_peer && !native_pending && begin_idle_stop(&rx) {
                            // With no other writer, the owned server container is the smallest complete stop boundary.
                            if exposure_pending.remove(&key).is_some() {
                                fail_before_open!(key, "error.driverStopped");
                            }
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
                        refresh_writers!(Some(key));
                        if let Some(t) = threads.get_mut(&key) {
                            t.closing = true;
                            t.publish_writers(key, &subagents);
                        }
                    }
                    // Keep an active thread until completion acknowledges the stop; its peers keep running.
                    SessionCommand::Close => {
                        pending.retain(|k| *k != key);
                        resuming.remove(&key);
                        // A subagent can outlive its reply; closing the conversation stops it either way.
                        for (thread, turn) in subagents.at_work(key) {
                            request!("turn/interrupt", json!({ "threadId": thread, "turnId": turn }), Waiting::Other(key, SUBAGENT_STOP));
                        }
                        if let Some(t) = threads.get_mut(&key) {
                            t.queued.clear();
                            if t.compact == CompactRequest::Queued { t.compact = CompactRequest::None; }
                            t.closing = true;
                            t.stop_children = true;
                            if release_subscription && t.native_release == NativeRelease::Keep {
                                t.native_release = NativeRelease::OnClose;
                            }
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
                        let _ = write_checked!(&set);
                    }
                }
                Some(ServerCommand::Retire) => retired = true,
                None => stop!(),
            }
        }
    }
    if service_failure.is_some() {
        alive.store(false, Ordering::Release);
        shutdown_confirmed |= proc::stop(&mut *stdin, &mut child, &mut job).await;
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
        refresh_writers!(None);
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
                if let Some(reason) = service_failure {
                    (t.sink)(EngineEvent::Error { message: reason.into(), fatal: true });
                }
            }
        }
        for (&key, t) in &threads {
            if announced.insert(key) {
                (t.sink)(EngineEvent::WriterExit { confirmed: false });
                if service_failure.is_some() {
                    (t.sink)(EngineEvent::Error { message: "error.rewind.stopUnconfirmed".into(), fatal: false });
                }
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
    rx.close();
    while let Ok(cmd) = rx.try_recv() {
        if let ServerCommand::Open { sink, .. } = cmd {
            sink(EngineEvent::Error { message: "error.driverStopped".into(), fatal: true });
            sink(EngineEvent::WriterExit { confirmed: true });
        }
    }
}

/// Asks a short-lived `codex app-server` for the account's quota windows and plan, so the first reading does not wait for a
/// turn. The answers are dressed as the notifications the server sends later and go through the same parser; the rest of
/// them (the account's email and other details) is dropped unread. A failed reply does not discard the other reply's evidence.
pub async fn read_codex_limits(bin: &str, cwd: &Path, windows: bool, spawn: Spawner, timeout: Duration) -> Vec<EngineEvent> {
    let mut events = vec![];
    query_codex_control(bin, cwd, windows, spawn, timeout, Some(|read| events = read), false).await;
    events
}

async fn query_codex_control(
    bin: &str,
    cwd: &Path,
    windows: bool,
    spawn: Spawner,
    timeout: Duration,
    mut on_account: Option<impl FnOnce(Vec<EngineEvent>) + Send>,
    want_models: bool,
) -> Vec<ModelInfo> {
    let args = codex_args(Area::Chat, windows, false, Permissions::Isolated);
    let want_account = on_account.is_some();
    let process = if want_account { proc::spawn_background(&spawn, bin, &args, cwd) } else { spawn(bin, &args, cwd) };
    let Ok(p) = process else {
        if let Some(publish) = on_account {
            publish(vec![]);
        }
        return vec![];
    };
    let mut stdin = p.stdin;
    let mut child = p.child;
    let mut job = p.contained;
    let mut lines = BufReader::new(p.stdout).lines();
    let mut events = vec![];
    let mut models = vec![];
    let ask = async {
        let init = json!({ "id": 1, "method": "initialize", "params": { "clientInfo": { "name": "faber", "title": "Faber", "version": "0.0.0" } } });
        let _ = proc::write_line(&mut *stdin, &init).await;
        let mut initialized = false;
        let mut pending_account = if want_account { 3 } else { 0 };
        let model_id = if want_account { 4 } else { 2 };
        let mut pending_models = want_models;
        while let Ok(Some(line)) = lines.next_line().await {
            let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
            let id = msg["id"].as_i64();
            if !initialized {
                if id != Some(1) {
                    continue;
                }
                if !msg["error"].is_null() {
                    break;
                }
                initialized = true;
                let _ = proc::write_line(&mut *stdin, &json!({ "method": "initialized" })).await;
                if want_account {
                    let _ = proc::write_line(&mut *stdin, &json!({ "id": 2, "method": "account/rateLimits/read" })).await;
                    let _ =
                        proc::write_line(&mut *stdin, &json!({ "id": 3, "method": "account/read", "params": { "refreshToken": false } }))
                            .await;
                }
                if want_models {
                    let _ = proc::write_line(&mut *stdin, &json!({ "id": model_id, "method": "model/list", "params": {} })).await;
                }
                continue;
            }
            match id {
                Some(2) if pending_account & 1 != 0 => {
                    if msg["error"].is_null() {
                        let note =
                            json!({ "method": "account/rateLimits/updated", "params": { "rateLimits": msg["result"]["rateLimits"] } });
                        events.extend(parse_codex_notification(&note, &mut CodexTurnState::default()));
                    }
                    pending_account &= !1;
                }
                Some(3) if pending_account & 2 != 0 => {
                    if msg["error"].is_null() {
                        let account = &msg["result"]["account"];
                        // `chatgpt`, `apiKey`, ...: the same mode names `account/updated` uses, in lower case.
                        let mode = account["type"].as_str().map(str::to_ascii_lowercase);
                        let note = json!({ "method": "account/updated", "params": { "authMode": mode, "planType": account["planType"] } });
                        events.extend(parse_codex_notification(&note, &mut CodexTurnState::default()));
                    }
                    pending_account &= !2;
                }
                Some(id) if pending_models && id == model_id => {
                    models = msg["result"]["data"]
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
                    pending_models = false;
                }
                _ => {}
            }
            if pending_account == 0 {
                if let Some(publish) = on_account.take() {
                    publish(std::mem::take(&mut events));
                }
                if !pending_models {
                    break;
                }
            }
        }
    };
    let _ = tokio::time::timeout(timeout, ask).await;
    if let Some(publish) = on_account {
        publish(events);
    }
    proc::stop(&mut *stdin, &mut child, &mut job).await;
    models
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

/// Reads the account and available models from one isolated server, without starting a conversation. Publishes account
/// evidence once the two account requests settle (or on timeout), even if the model list is still pending.
pub async fn read_codex_account_models(
    bin: &str,
    cwd: &Path,
    windows: bool,
    spawn: Spawner,
    timeout: Duration,
    on_account: impl FnOnce(Vec<EngineEvent>) + Send,
) -> Vec<ModelInfo> {
    query_codex_control(bin, cwd, windows, spawn, timeout, Some(on_account), true).await
}

/// Asks a short-lived `codex app-server` which models this account can use, so the picker shows the real list.
/// Gives an empty list on any failure: the picker then offers just the account's default.
pub async fn list_codex_models(bin: &str, cwd: &Path, windows: bool, spawn: Spawner, timeout: Duration) -> Vec<ModelInfo> {
    query_codex_control(bin, cwd, windows, spawn, timeout, None::<fn(Vec<EngineEvent>)>, true).await
}

#[cfg(test)]
mod rewind_tests {
    use super::*;

    #[test]
    fn shared_wire_keeps_ordinary_lines_and_hides_old_tokens() {
        let mut tokens = WireTokens::default();
        tokens.insert("fa1.first-fixture".into());
        let ordinary = r#"{"text":"cópia em uso"}"#.to_string();
        let buffer = ordinary.as_ptr();
        let safe = tokens.redact(ordinary);
        assert!(std::ptr::eq(safe.as_ptr(), buffer));
        assert_eq!(safe, r#"{"text":"cópia em uso"}"#);
        tokens.insert("fa1.later-fixture".into());
        assert_eq!(tokens.redact("fa1.later-fixture and fa1.first-fixture".into()), "[redacted] and [redacted]");
        assert_eq!(tokens.redact("unknown fa1.example".into()), "unknown fa1.example");
        tokens.insert("other-fixture".into());
        assert_eq!(tokens.redact("old other-fixture".into()), "old [redacted]");
        assert_eq!(tokens.redact("fa1.first-fixture or other-fixture".into()), "[redacted] or [redacted]");
    }

    #[test]
    fn streamed_secret_is_redacted_across_deltas() {
        let mut pending = String::new();
        let mut output = redact_delta(&mut pending, "answer fa", "fa1.secret");
        output.push_str(&redact_delta(&mut pending, "1.sec", "fa1.secret"));
        output.push_str(&redact_delta(&mut pending, "ret done", "fa1.secret"));
        output.push_str(&pending);
        assert_eq!(output, "answer [redacted] done");
    }

    #[test]
    fn only_a_completed_command_with_a_snapshot_has_terminal_output() {
        use crate::background::{Capture, Update};
        for (method, pending_exit, output, expected) in [
            ("item/started", false, Some("working"), Some(Capture::Snapshot)),
            ("item/completed", true, Some("working"), Some(Capture::Snapshot)),
            ("item/completed", false, None, None),
            ("item/completed", false, Some(""), Some(Capture::Terminal)),
        ] {
            let msg = json!({"method":method,"params":{"threadId":"thread","item":{
                "type":"commandExecution","id":"cmd","command":"echo ready","status":"completed","aggregatedOutput":output,
            }}});
            let events = command_news(&msg, pending_exit);
            let capture = events.iter().find_map(|event| match event {
                EngineEvent::Background(Update::Output { capture, .. }) => Some(*capture),
                _ => None,
            });
            assert_eq!(capture, expected);
            assert_eq!(
                events.iter().any(|event| matches!(event, EngineEvent::Background(Update::End { .. }))),
                method == "item/completed" && !pending_exit
            );
        }
    }

    #[test]
    fn a_redacted_terminal_snapshot_replaces_pending_command_deltas() {
        use crate::background::{Capture, State, Update};
        let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
        let capture = recorded.clone();
        let sink = redact_sink(Arc::new(move |event| capture.lock().unwrap().push(event)), "fa1.secret".into());
        for (text, capture) in [("fa1.", Capture::Delta), ("fa1.secret and fa1.", Capture::Terminal)] {
            sink(EngineEvent::Background(Update::Output { id: "cmd".into(), text: text.into(), capture, source_truncated: false }));
        }
        sink(EngineEvent::Background(Update::End { id: "cmd".into(), state: State::Done }));
        assert!(matches!(&recorded.lock().unwrap()[..], [
            EngineEvent::Background(Update::Output { text, capture: Capture::Terminal, .. }),
            EngineEvent::Background(Update::End { .. }),
        ] if text == "[redacted] and fa1."));
    }

    #[test]
    fn streamed_secrets_remain_redacted_across_other_events_and_background_output() {
        let recorded = Arc::new(std::sync::Mutex::new(Vec::new()));
        let capture = recorded.clone();
        let sink = redact_sink(Arc::new(move |event| capture.lock().unwrap().push(event)), "fa1.secret".into());
        sink(EngineEvent::Text("fa1.".into()));
        sink(EngineEvent::Quota { window: "test".into(), used: 0.0, resets_at: 0.0 });
        sink(EngineEvent::Text("secret".into()));
        for text in ["fa1.", "secret"] {
            sink(EngineEvent::Background(crate::background::Update::Output {
                id: "cmd".into(),
                text: text.into(),
                capture: crate::background::Capture::Delta,
                source_truncated: false,
            }));
        }
        sink(EngineEvent::Permission {
            request_id: "r".into(),
            tool: "Bash".into(),
            input: json!({"command":"echo fa1.secret", "fa1.secret":"x"}),
        });
        sink(EngineEvent::ToolResult { id: "r".into(), ok: true, declined: false, exit_code: None, text: "fa1.secret".into() });
        sink(EngineEvent::Text("off".into()));
        sink(EngineEvent::Break { final_answer: false });
        sink(EngineEvent::Text("next".into()));
        let events = recorded.lock().unwrap();
        assert!(events.iter().any(|e| matches!(e, EngineEvent::Text(text) if text == "[redacted]")));
        assert!(events.iter().any(
            |e| matches!(e, EngineEvent::Permission { input, .. } if input["command"] == "echo [redacted]" && input["[redacted]"] == "x")
        ));
        assert!(events.iter().any(|e| matches!(e, EngineEvent::ToolResult { text, .. } if text == "[redacted]")));
        let words: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                EngineEvent::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(words, ["[redacted]", "of", "f", "next"]);
        let boundary = events.iter().position(|event| matches!(event, EngineEvent::Break { .. })).unwrap();
        assert!(matches!(&events[boundary - 1], EngineEvent::Text(text) if text == "f"));
        assert!(matches!(&events[boundary + 1], EngineEvent::Text(text) if text == "next"));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, EngineEvent::Background(crate::background::Update::Output { text, .. }) if text == "[redacted]"))
        );
    }

    #[test]
    fn permission_details_keep_the_reason_and_working_folder() {
        let params = json!({ "command": ["pwsh", "-Command", "Get-ChildItem"], "cwd": "C:/work", "reason": "Read the build files", "threadId": "private" });
        let (tool, input) = approval_details("item/commandExecution/requestApproval", &params);
        assert_eq!(tool, "command");
        assert_eq!(input["command"], params["command"]);
        assert_eq!(input["cwd"], params["cwd"]);
        assert_eq!(input["reason"], params["reason"]);
        assert!(input.get("threadId").is_none());
        let (tool, input) = approval_details(
            "item/fileChange/requestApproval",
            &json!({ "changes": { "app.ts": { "add": { "content": "hello" } } }, "reason": "Add the view" }),
        );
        assert_eq!(tool, "fileChange");
        assert!(input["changes"]["app.ts"].is_object());
        assert_eq!(input["reason"], "Add the view");
    }

    #[test]
    fn the_guards_read_the_action_not_the_explanation() {
        let params = json!({"command":"git status --short","cwd":"C:/work","reason":"Check status without reading .npmrc"});
        assert_eq!(approval_action(&params), json!("git status --short"));
        assert!(!crate::permissions::touches_credentials(&approval_action(&params), Some("C:/work"), &|_| None));
    }

    #[test]
    fn permission_details_keep_network_destination_without_command() {
        let context = json!({"host":"upload.example.com", "protocol":"https"});
        let (_, input) =
            approval_details("item/commandExecution/requestApproval", &json!({"command":null,"networkApprovalContext":context}));
        assert_eq!(input["networkApprovalContext"], context);
    }

    #[tokio::test]
    async fn closed_shared_server_does_not_reenter_the_engine_during_open() {
        let server = CodexServer::start(CodexServerOptions {
            permissions: Permissions::FullAccess,
            cli: crate::cli::Cli { command: "unused".into(), prefix: vec![] },
            cwd: PathBuf::new(),
            area: Area::Project,
            user_hooks: false,
            windows: false,
            spawn: Arc::new(|_, _, _| Err(std::io::Error::other("already stopped"))),
            tools: Default::default(),
        });
        tokio::time::timeout(Duration::from_secs(3), server.tx.closed()).await.unwrap();

        let (sent, mut events) = unbounded_channel();
        let state = Arc::new(std::sync::Mutex::new(()));
        let engine = state.clone();
        let sink: EventSink = Arc::new(move |event| {
            let _lock = engine.try_lock().expect("opening a closed service reentered the locked engine");
            sent.send(event).unwrap();
        });
        let _session = std::thread::spawn(move || {
            let _lock = state.lock().unwrap();
            server.open(thread(sink.clone()).params, sink)
        })
        .join()
        .unwrap();

        let error = tokio::time::timeout(Duration::from_secs(3), events.recv()).await.unwrap().unwrap();
        assert!(matches!(error, EngineEvent::Error { message, fatal: true } if message == "error.driverStopped"));
        assert!(matches!(events.recv().await, Some(EngineEvent::WriterExit { confirmed: true })));
        assert!(events.recv().await.is_none());
    }

    #[tokio::test]
    async fn closing_shared_server_checks_queued_opens_and_still_stops_after_tools_updates() {
        use tokio::io::AsyncWriteExt;

        for (with_open, interrupt, waiting_for_config) in
            [(true, false, false), (false, false, false), (true, true, false), (false, true, true)]
        {
            tokio::time::timeout(Duration::from_secs(3), async {
                let root = proc::ProcessIdentity { pid: 100, created: 1 };
                let child = proc::ProcessIdentity { pid: 101, created: 2 };
                let members = if with_open { vec![root, child] } else { vec![root] };
                let ended = Arc::new(AtomicBool::new(false));
                let stopped = ended.clone();
                let contained = proc::Containment::scripted_descendants(
                    root,
                    move || Ok(if stopped.load(Ordering::Acquire) { vec![] } else { members.clone() }),
                    move || ended.store(true, Ordering::Release),
                );
                let (driver, script) = tokio::io::duplex(65536);
                let (stdout, stdin) = tokio::io::split(driver);
                let (read, mut input) = tokio::io::split(script);
                let mut output = BufReader::new(read).lines();
                let (tx, rx) = unbounded_channel();
                let (sent, mut events) = unbounded_channel();
                let open = |key| {
                    let sent = sent.clone();
                    let sink: EventSink = Arc::new(move |event| drop(sent.send((key, event))));
                    let mut params = thread(sink.clone()).params;
                    if waiting_for_config {
                        params.permissions = Permissions::Isolated;
                    }
                    ServerCommand::Open {
                        key,
                        thread: Box::new(params),
                        sink,
                        tools: None,
                        identity: None,
                        before_turn: None,
                        native_fork: None,
                    }
                };
                let opts = CodexServerOptions {
                    permissions: if waiting_for_config { Permissions::Isolated } else { Permissions::FullAccess },
                    cli: crate::cli::Cli { command: "unused".into(), prefix: vec![] },
                    cwd: PathBuf::new(),
                    area: Area::Project,
                    user_hooks: false,
                    windows: false,
                    spawn: Arc::new(|_, _, _| panic!("scripted driver must not spawn")),
                    tools: Default::default(),
                };
                let p =
                    proc::Proc { stdin: Box::new(stdin), stdout: Box::new(stdout), stderr: None, child: None, contained: Some(contained) };
                let server = tokio::spawn(serve(Ok(p), opts, rx, Arc::new(AtomicBool::new(true)), Arc::new(Mutex::new(()))));
                let init: Value = serde_json::from_str(&output.next_line().await.unwrap().unwrap()).unwrap();
                input.write_all(format!("{}\n", json!({"id":init["id"],"result":{}})).as_bytes()).await.unwrap();
                tx.send(open(1)).unwrap();
                loop {
                    let request: Value = serde_json::from_str(&output.next_line().await.unwrap().expect("first open")).unwrap();
                    if waiting_for_config && request["method"] == "config/read" {
                        tx.send(ServerCommand::Session(1, SessionCommand::Send(UserTurn::text("Stop before opening")))).unwrap();
                        break;
                    }
                    if request["method"] == "thread/start" {
                        input
                            .write_all(format!("{}\n", json!({"id":request["id"],"result":{"thread":{"id":"first"}}})).as_bytes())
                            .await
                            .unwrap();
                        break;
                    }
                }
                if !waiting_for_config {
                    while !matches!(events.recv().await.unwrap(), (1, EngineEvent::Started { .. })) {}
                }

                tx.send(ServerCommand::Session(1, if interrupt { SessionCommand::Interrupt } else { SessionCommand::Close })).unwrap();
                if !waiting_for_config {
                    tx.send(ServerCommand::Tools(Default::default())).unwrap();
                }
                if with_open {
                    tx.send(open(2)).unwrap();
                    loop {
                        let request: Value =
                            serde_json::from_str(&output.next_line().await.unwrap().expect("queued open was lost")).unwrap();
                        if request["method"] == "thread/start" {
                            input
                                .write_all(format!("{}\n", json!({"id":request["id"],"result":{"thread":{"id":"second"}}})).as_bytes())
                                .await
                                .unwrap();
                            break;
                        }
                    }
                    loop {
                        match events.recv().await.unwrap() {
                            (2, EngineEvent::Started { .. }) => break,
                            (2, event) => assert!(!matches!(event, EngineEvent::Error { fatal: true, .. })),
                            _ => {}
                        }
                    }
                    if interrupt {
                        tx.send(ServerCommand::Session(1, SessionCommand::Close)).unwrap();
                    }
                    tx.send(ServerCommand::Session(2, SessionCommand::Close)).unwrap();
                }
                while output.next_line().await.unwrap().is_some() {}
                input.shutdown().await.unwrap();
                server.await.unwrap();
                if waiting_for_config {
                    let events: Vec<_> = std::iter::from_fn(|| events.try_recv().ok()).map(|(_, event)| event).collect();
                    let stopped = events.iter().position(|e| matches!(e, EngineEvent::Done { reason, .. } if reason == "interrupted"));
                    let retired = events
                        .iter()
                        .position(|e| matches!(e, EngineEvent::Error { message, fatal: true } if message == "error.driverStopped"));
                    assert!(
                        stopped.zip(retired).is_some_and(|(stop, retire)| stop < retire),
                        "Stop must settle the reply before retiring the dead unopened session: {events:?}"
                    );
                    assert!(events.iter().any(|e| matches!(e, EngineEvent::WriterExit { confirmed: true })));
                }
            })
            .await
            .expect("the shared service stays open for queued conversations and closes after its last conversation");
        }
    }

    #[tokio::test]
    async fn shared_server_leaves_an_open_untouched_after_choosing_to_stop() {
        use tokio::io::AsyncWriteExt;

        tokio::time::timeout(Duration::from_secs(3), async {
            let start_service = || {
                let (driver, script) = tokio::io::duplex(65536);
                let (stdout, stdin) = tokio::io::split(driver);
                let (read, input) = tokio::io::split(script);
                let process = Mutex::new(Some(proc::Proc {
                    stdin: Box::new(stdin),
                    stdout: Box::new(stdout),
                    stderr: None,
                    child: None,
                    contained: None,
                }));
                let server = CodexServer::start(CodexServerOptions {
                    permissions: Permissions::FullAccess,
                    cli: crate::cli::Cli { command: "unused".into(), prefix: vec![] },
                    cwd: PathBuf::new(),
                    area: Area::Project,
                    user_hooks: false,
                    windows: false,
                    spawn: Arc::new(move |_, _, _| Ok(process.lock().unwrap().take().unwrap())),
                    tools: Default::default(),
                });
                (server, BufReader::new(read).lines(), input)
            };
            let (server, mut output, mut input) = start_service();
            // The engine has selected this service but has not submitted the new conversation yet.
            assert!(server.is_alive());
            server.retire();
            while output.next_line().await.unwrap().is_some() {}
            assert!(!server.tx.is_closed(), "the service is still waiting for its stream to end");
            let (sent, mut events) = unbounded_channel();
            let sink: EventSink = Arc::new(move |event| drop(sent.send(event)));
            let mut request = CodexOpen::new(thread(sink.clone()).params, sink);
            request.thread.resume = Some("selected-thread".into());
            let Err(request) = server.try_open(request) else { panic!("open went to the service already closing") };
            input.shutdown().await.unwrap();
            server.tx.closed().await;
            assert!(matches!(events.try_recv(), Err(tokio::sync::mpsc::error::TryRecvError::Empty)));

            let (replacement, mut output, mut input) = start_service();
            let session = replacement.open_request(*request);
            let parse = |line: String| serde_json::from_str::<Value>(&line).unwrap();
            let init = parse(output.next_line().await.unwrap().unwrap());
            input.write_all(format!("{}\n", json!({"id":init["id"],"result":{}})).as_bytes()).await.unwrap();
            assert_eq!(parse(output.next_line().await.unwrap().unwrap())["method"], "initialized");
            let open = parse(output.next_line().await.unwrap().unwrap());
            assert_eq!(open["method"], "thread/resume");
            assert_eq!(open["params"]["threadId"], "selected-thread");
            input
                .write_all(format!("{}\n", json!({"id":open["id"],"result":{"thread":{"id":"selected-thread"}}})).as_bytes())
                .await
                .unwrap();
            loop {
                match events.recv().await.unwrap() {
                    EngineEvent::Started { .. } => break,
                    EngineEvent::Error { fatal: true, message } => panic!("the replacement failed: {message}"),
                    _ => {}
                }
            }
            session.close();
            while let Some(line) = output.next_line().await.unwrap() {
                assert_ne!(parse(line)["method"], "thread/resume");
            }
            input.shutdown().await.unwrap();
            replacement.tx.closed().await;
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn rewind_failed_native_open_releases_its_writer_without_stopping_a_real_servers_peer() {
        use tokio::io::AsyncWriteExt;

        for failure in ["thread/resume", "thread/revert"] {
            tokio::time::timeout(Duration::from_secs(5), async {
                let root = proc::ProcessIdentity { pid: 100, created: 1 };
                let stopped = Arc::new(AtomicBool::new(false));
                let ended = stopped.clone();
                let contained =
                    proc::Containment::scripted_descendants(root, move || Ok(vec![root]), move || ended.store(true, Ordering::Release));
                let (driver, script) = tokio::io::duplex(65536);
                let (stdout, stdin) = tokio::io::split(driver);
                let (read, mut input) = tokio::io::split(script);
                let mut output = BufReader::new(read).lines();
                let (tx, rx) = unbounded_channel();
                let (sent, mut events) = unbounded_channel();
                let sink = |key| {
                    let sent = sent.clone();
                    Arc::new(move |event| drop(sent.send((key, event)))) as EventSink
                };
                let alive = Arc::new(AtomicBool::new(true));
                let opts = CodexServerOptions {
                    permissions: Permissions::FullAccess,
                    cli: crate::cli::Cli { command: "unused".into(), prefix: vec![] },
                    cwd: PathBuf::new(),
                    area: Area::Chat,
                    user_hooks: false,
                    windows: false,
                    spawn: Arc::new(|_, _, _| panic!("scripted driver must not spawn")),
                    tools: Default::default(),
                };
                let process =
                    proc::Proc { stdin: Box::new(stdin), stdout: Box::new(stdout), stderr: None, child: None, contained: Some(contained) };
                let server = tokio::spawn(serve(Ok(process), opts, rx, alive.clone(), Arc::new(Mutex::new(()))));
                let read = |line: String| serde_json::from_str::<Value>(&line).unwrap();
                let init = read(output.next_line().await.unwrap().unwrap());
                input.write_all(format!("{}\n", json!({"id":init["id"],"result":{"userAgent":"faber/0.166.0"}})).as_bytes()).await.unwrap();
                assert_eq!(read(output.next_line().await.unwrap().unwrap())["method"], "initialized");
                tx.send(ServerCommand::Open {
                    key: 2,
                    thread: Box::new(thread(sink(2)).params),
                    sink: sink(2),
                    tools: None,
                    identity: None,
                    before_turn: None,
                    native_fork: None,
                })
                .unwrap();
                let open = read(output.next_line().await.unwrap().unwrap());
                input.write_all(format!("{}\n", json!({"id":open["id"],"result":{"thread":{"id":"peer"}}})).as_bytes()).await.unwrap();
                while !matches!(events.recv().await.unwrap(), (2, EngineEvent::Started { .. })) {}
                // A peer holds the real server open while the replacement fails before its first turn.
                input
                    .write_all(
                        format!("{}\n", json!({"method":"turn/started","params":{"threadId":"peer","turn":{"id":"peer-turn"}}})).as_bytes(),
                    )
                    .await
                    .unwrap();
                let mut params = thread(sink(1)).params;
                params.resume = Some("original".into());
                tx.send(ServerCommand::Open {
                    key: 1,
                    thread: Box::new(params),
                    sink: sink(1),
                    tools: None,
                    identity: None,
                    before_turn: Some("cut".into()),
                    native_fork: None,
                })
                .unwrap();
                tx.send(ServerCommand::Session(1, SessionCommand::Send(UserTurn::text("pending")))).unwrap();
                loop {
                    let request = read(output.next_line().await.unwrap().unwrap());
                    if request["method"] == "skills/list" {
                        continue;
                    }
                    if request["method"] == failure {
                        input
                            .write_all(format!("{}\n", json!({"id":request["id"],"error":{"message":"injected failure"}})).as_bytes())
                            .await
                            .unwrap();
                        break;
                    }
                    assert_eq!(request["method"], "thread/resume");
                    input
                        .write_all(format!("{}\n", json!({"id":request["id"],"result":{"thread":{"id":"original"}}})).as_bytes())
                        .await
                        .unwrap();
                }
                while !matches!(events.recv().await.unwrap(), (1, EngineEvent::Error { fatal: true, .. })) {}
                if failure == "thread/revert" {
                    let release = read(output.next_line().await.unwrap().unwrap());
                    assert_eq!(release["method"], "thread/unsubscribe");
                    assert_eq!(release["params"]["threadId"], "original");
                    assert!(
                        !std::iter::from_fn(|| events.try_recv().ok()).any(|event| matches!(event, (1, EngineEvent::WriterExit { .. })))
                    );
                    input.write_all(format!("{}\n", json!({"id":release["id"],"result":{"status":"notLoaded"}})).as_bytes()).await.unwrap();
                }
                while !matches!(events.recv().await.unwrap(), (1, EngineEvent::WriterExit { confirmed: true })) {}
                assert!(alive.load(Ordering::Acquire) && !stopped.load(Ordering::Acquire));
                input
                    .write_all(
                        format!(
                            "{}\n",
                            json!({"method":"turn/completed","params":{"threadId":"peer","turn":{"id":"peer-turn","status":"completed"}}})
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
                while !matches!(events.recv().await.unwrap(), (2, EngineEvent::Done { ok: true, .. })) {}
                assert!(!stopped.load(Ordering::Acquire), "the peer finishes without its server being stopped");
                server.abort();
            })
            .await
            .expect("failed native startup releases its owner in a real shared server");
        }
    }

    #[tokio::test]
    async fn shared_threads_keep_separate_identity_and_refresh_it_before_resume() {
        use tokio::io::AsyncWriteExt;
        tokio::time::timeout(Duration::from_secs(5), async {
            let (driver, script) = tokio::io::duplex(65536);
            let (stdout, stdin) = tokio::io::split(driver);
            let (read, mut input) = tokio::io::split(script);
            let mut output = BufReader::new(read).lines();
            let (tx, rx) = unbounded_channel();
            let spawn: Spawner = Arc::new(|_, _, _| panic!("threads must reuse the shared process"));
            let opts = CodexServerOptions {
                permissions: Permissions::Ask,
                cli: crate::cli::Cli { command: "codex".into(), prefix: vec![] },
                cwd: PathBuf::new(),
                area: Area::Chat,
                user_hooks: false,
                windows: false,
                spawn: spawn.clone(),
                tools: Default::default(),
            };
            let root = proc::ProcessIdentity { pid: 100, created: 1 };
            let stopped = Arc::new(AtomicBool::new(false));
            let query = stopped.clone();
            let stop = stopped.clone();
            let container = proc::Containment::scripted_descendants(root,
                move || Ok(if query.load(Ordering::Acquire) { vec![] } else { vec![root] }),
                move || { stop.store(true, Ordering::Release); });
            let p = proc::Proc { stdin: Box::new(stdin), stdout: Box::new(stdout), stderr: None, child: None, contained: Some(container) };
            let server = tokio::spawn(serve(Ok(p), opts, rx, Arc::new(AtomicBool::new(true)), Arc::new(Mutex::new(()))));
            let init: Value = serde_json::from_str(&output.next_line().await.unwrap().unwrap()).unwrap();
            input.write_all(format!("{}\n", json!({"id":init["id"],"result":{}})).as_bytes()).await.unwrap();
            let initialized: Value = serde_json::from_str(&output.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(initialized["method"], "initialized");
            for (key, conversation, token, resume) in
                [(1, "one", "key-one", None), (2, "two", "key-two", None), (3, "one", "key-new", Some("native-one"))]
            {
                let sink: EventSink = Arc::new(|_| {});
                let mut params = thread(sink.clone()).params;
                params.resume = resume.map(String::from);
                let identity = Some(ThreadIdentity {
                    native_goal: NativeGoalOpen::Ordinary,
                    identity: Arc::new(proc::ConversationIdentity { id: conversation.into(), token: Some(token.into()) }),
                    spawn: spawn.clone(),
                });
                if resume.is_some() {
                    tx.send(ServerCommand::Session(1, SessionCommand::Retire)).unwrap();
                    for (cancel_key, cancel) in [(5, SessionCommand::Close), (6, SessionCommand::Retire)] {
                        let (events, mut received) = unbounded_channel();
                        let sink: EventSink = Arc::new(move |event| { let _ = events.send(event); });
                        let identity = Some(ThreadIdentity {
                            native_goal: NativeGoalOpen::Ordinary,
                            identity: Arc::new(proc::ConversationIdentity { id: "one".into(), token: Some(format!("key-cancel-{cancel_key}")) }),
                            spawn: spawn.clone(),
                        });
                        tx.send(ServerCommand::Open { key: cancel_key, thread: Box::new(params.clone()), sink, tools: None, identity, before_turn: None, native_fork: None }).unwrap();
                        assert!(tokio::time::timeout(Duration::from_millis(20), output.next_line()).await.is_err());
                        tx.send(ServerCommand::Session(cancel_key, cancel)).unwrap();
                        loop {
                            if let EngineEvent::WriterExit { confirmed } = received.recv().await.unwrap() { assert!(confirmed); break; }
                        }
                    }
                }
                tx.send(ServerCommand::Open { key, thread: Box::new(params), sink, tools: None, identity, before_turn: None, native_fork: None }).unwrap();
                if resume.is_some() {
                    assert!(tokio::time::timeout(Duration::from_millis(20), output.next_line()).await.is_err(), "a native writer must survive token rotation until it finishes");
                    input.write_all(format!("{}\n",json!({"method":"item/completed","params":{"threadId":"native-one","item":{"id":"background","type":"commandExecution","processId":"terminal-one","exitCode":0}}})).as_bytes()).await.unwrap();
                }
                let mut open: Value = serde_json::from_str(&output.next_line().await.unwrap().unwrap()).unwrap();
                if let Some(id) = resume {
                    assert_eq!(open["method"], "thread/unsubscribe");
                    assert_eq!(open["id"], format!("faber-unsubscribe-{key}"));
                    assert_eq!(open["params"]["threadId"], id);
                    assert!(
                        tokio::time::timeout(Duration::from_millis(20), output.next_line()).await.is_err(),
                        "resume must wait for unsubscribe acknowledgement"
                    );
                    input.write_all(format!("{}\n", json!({"id":open["id"],"result":{"status":"unsubscribed"}})).as_bytes()).await.unwrap();
                    open = serde_json::from_str(&output.next_line().await.unwrap().unwrap()).unwrap();
                }
                assert_eq!(open["method"], if resume.is_some() { "thread/resume" } else { "thread/start" });
                let env = &open["params"]["config"]["shell_environment_policy.set"];
                assert_eq!(env["FABER_AGENT_TOKEN"], token);
                assert_eq!(env["FABER_AGENT_AUTH"], token);
                assert_eq!(env["FABER_CONVERSATION"], conversation);
                assert_eq!(env[proc::AGENT_MARK], "1");
                for other in ["key-one", "key-two", "key-new"].into_iter().filter(|other| *other != token) {
                    assert!(!open.to_string().contains(other));
                }
                if key == 1 {
                    input.write_all(format!("{}\n", json!({"id":open["id"],"result":{"thread":{"id":"native-one"}}})).as_bytes()).await.unwrap();
                    let skills: Value = serde_json::from_str(&output.next_line().await.unwrap().unwrap()).unwrap();
                    assert_eq!(skills["method"], "skills/list");
                    input.write_all(format!("{}\n",json!({"method":"item/started","params":{"threadId":"native-one","item":{"id":"background","type":"commandExecution","processId":"terminal-one"}}})).as_bytes()).await.unwrap();
                } else if key == 3 {
                    // End the previous resume before testing an independent preflight rejection for this ID.
                    input.write_all(format!("{}\n", json!({"id":open["id"],"error":{"code":-32602,"message":"test resume refused"}})).as_bytes()).await.unwrap();
                }
            }
            let (events, mut received) = unbounded_channel();
            let sink: EventSink = Arc::new(move |event| {
                let _ = events.send(event);
            });
            let mut params = thread(sink.clone()).params;
            params.resume = Some("native-one".into());
            let identity = Some(ThreadIdentity {
                native_goal: NativeGoalOpen::Ordinary,
                identity: Arc::new(proc::ConversationIdentity { id: "one".into(), token: Some("key-failed".into()) }),
                spawn,
            });
            tx.send(ServerCommand::Open { key: 4, thread: Box::new(params), sink, tools: None, identity, before_turn: None, native_fork: None }).unwrap();
            let unsubscribe: Value = serde_json::from_str(&output.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(unsubscribe["method"], "thread/unsubscribe");
            input.write_all(format!("{}\n", json!({"id":unsubscribe["id"],"error":{"message":"unsupported"}})).as_bytes()).await.unwrap();
            loop {
                if let EngineEvent::Error { fatal, .. } = received.recv().await.unwrap() {
                    assert!(fatal);
                    break;
                }
            }
            assert!(
                tokio::time::timeout(Duration::from_millis(20), output.next_line()).await.is_err(),
                "a rejected unsubscribe cannot resume with stale identity"
            );
            server.abort();
        })
        .await
        .expect("shared identity handshake stalled");
    }

    /// Item 10 (decided 2026-11-13): a guarded thread's sandbox keeps the person's "never touch" paths read-only, for commands
    /// too; Full access has no sandbox to hold them (its approvals, when Codex asks, are refused by `names_protected`).
    #[test]
    fn a_guarded_thread_keeps_the_never_touch_paths_read_only() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().canonicalize().unwrap();
        for permissions in [Permissions::Auto, Permissions::AcceptEdits, Permissions::Ask, Permissions::FullAccess] {
            let mut t = thread(Arc::new(|_| {}));
            t.params.cwd = root.clone();
            t.params.permissions = permissions;
            t.params.tuning.protected = vec![root.join("legacy")];
            let (_, params) = t.open_params(Area::Project, false);
            let config = &params["config"];
            let profile = config["default_permissions"].as_str().map(|id| &config[&format!("permissions.{id}")]["filesystem"]);
            let kept = root.join("legacy").to_string_lossy().replace('\\', "/");
            let kept = kept.strip_prefix("//?/").unwrap_or(&kept).to_string();
            match permissions {
                Permissions::FullAccess => assert!(profile.is_none(), "{params}"),
                _ => assert_eq!(profile.unwrap()[&kept], "read", "{permissions:?} {params}"),
            }
        }
    }

    #[test]
    fn thread_identity_preserves_sandbox_environment_on_start_and_resume() {
        for resume in [None, Some("native-one".into())] {
            let mut t = thread(Arc::new(|_| {}));
            t.params.cwd = PathBuf::from("project");
            t.params.permissions = Permissions::Isolated;
            t.params.resume = resume;
            t.identity = Some(ThreadIdentity {
                native_goal: NativeGoalOpen::Ordinary,
                identity: Arc::new(proc::ConversationIdentity { id: "one".into(), token: Some("key-one".into()) }),
                spawn: Arc::new(|_, _, _| unreachable!()),
            });
            let (_, params) = t.open_params(Area::Project, false);
            let env = &params["config"]["shell_environment_policy.set"];
            for (key, value) in crate::permissions::child_env(&t.params.cwd).as_object().unwrap() {
                assert_eq!(&env[key], value, "sandbox variable {key} was replaced");
            }
            assert_eq!(env["FABER_AGENT_TOKEN"], "key-one");
            assert_eq!(env["FABER_CONVERSATION"], "one");
        }
    }

    #[tokio::test]
    async fn isolated_fake_server_prepares_the_sandbox_and_passes_identity() {
        use tokio::io::AsyncWriteExt;
        async fn read(lines: &mut tokio::io::Lines<BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>>) -> Value {
            loop {
                let message: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
                if message["method"] != "skills/list" {
                    return message;
                }
            }
        }
        async fn write(pipe: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>, value: Value) {
            pipe.write_all(format!("{value}\n").as_bytes()).await.unwrap();
        }
        tokio::time::timeout(Duration::from_secs(5), async {
            let root = tempfile::tempdir().unwrap();
            let (driver, script) = tokio::io::duplex(65536);
            let (stdout, stdin) = tokio::io::split(driver);
            let (read_pipe, mut input) = tokio::io::split(script);
            let mut output = BufReader::new(read_pipe).lines();
            let (tx, rx) = unbounded_channel();
            let (captured, mut commands) = unbounded_channel();
            let spawn: Spawner = Arc::new(move |bin, args, cwd| {
                let command = proc::marked(bin, args, cwd);
                let env = command.as_std().get_envs().filter_map(|(k,v)| v.map(|v| (k.to_owned(),v.to_owned())))
                    .collect::<HashMap<_,_>>();
                captured.send((bin.to_owned(), args.to_vec(), env, cwd.join(".faber/tmp").is_dir())).unwrap();
                Err(std::io::Error::other("fake"))
            });
            let opts = CodexServerOptions {
                permissions: Permissions::Isolated,
                cli: crate::cli::Cli { command: "codex".into(), prefix: vec![] },
                cwd: root.path().into(), area: Area::Chat, user_hooks: false, windows: true,
                spawn: spawn.clone(), tools: Default::default(),
            };
            let identity = Arc::new(proc::ConversationIdentity { id: "one".into(), token: Some("test-key".into()) });
            let p = proc::Proc { stdin: Box::new(stdin), stdout: Box::new(stdout), stderr: None, child: None, contained: None };
            let server = tokio::spawn(serve(Ok(p), opts, rx, Arc::new(AtomicBool::new(true)), Arc::new(Mutex::new(()))));
            let init = read(&mut output).await;
            write(&mut input, json!({"id":init["id"],"result":{}})).await;
            assert_eq!(read(&mut output).await["method"], "initialized");
            let sink: EventSink = Arc::new(|_| {});
            let mut params = thread(sink.clone()).params;
            params.cwd = root.path().into();
            params.permissions = Permissions::Isolated;
            params.resume = Some("previous".into());
            let (method, resume) = thread_params(Area::Chat, &params);
            assert_eq!(method, "thread/resume");
            assert!(resume["dynamicTools"].is_null(), "a resumed conversation keeps its original tool description");
            assert!(resume["developerInstructions"].as_str().is_some_and(|words| words.contains("rg PATTERN .")));
            params.resume = None;
            tx.send(ServerCommand::Open { key: 1, thread: Box::new(params), sink, tools: None, identity: Some(ThreadIdentity { identity, spawn, native_goal: NativeGoalOpen::Ordinary }), before_turn: None, native_fork: None }).unwrap();
            let config = read(&mut output).await;
            assert_eq!(config["method"], "config/read");
            write(&mut input, json!({"id":config["id"],"result":{"config":{"mcp_servers":{}}}})).await;
            let open = read(&mut output).await;
            assert_eq!(open["method"], "thread/start");
            let instructions = open["params"]["developerInstructions"].as_str().unwrap();
            assert!(instructions.contains("rg PATTERN .") && instructions.contains("--no-ignore-exclude"), "{instructions}");
            write(&mut input, json!({"id":open["id"],"result":{"thread":{"id":"owner","model":"test"}}})).await;
            write(&mut input, json!({"id":"call","method":"item/tool/call","params":{"threadId":"owner","tool":"run_command","arguments":{"command":"cargo test"}}})).await;
            let (bin, args, env, temp_ready) = commands.recv().await.unwrap();
            assert!(temp_ready, "the sandbox receives a temporary path that must exist before launch");
            assert_eq!(bin, "codex");
            assert!(args.iter().all(|arg| !arg.contains("test-key")));
            assert!(args.iter().any(|arg| arg == "shell_environment_policy.ignore_default_excludes=false"));
            assert_eq!(env.get(std::ffi::OsStr::new("FABER_AGENT_TOKEN")).map(|value| value.as_os_str()), Some(std::ffi::OsStr::new("test-key")));
            assert_eq!(env.get(std::ffi::OsStr::new("FABER_AGENT_AUTH")).map(|value| value.as_os_str()), Some(std::ffi::OsStr::new("test-key")));
            assert_eq!(read(&mut output).await["id"], "call");
            let temp = root.path().join(".faber/tmp");
            std::fs::remove_dir(&temp).unwrap();
            write(&mut input, json!({"id":"again","method":"item/tool/call","params":{"threadId":"owner","tool":"run_command","arguments":{"command":"cargo test"}}})).await;
            assert!(commands.recv().await.unwrap().3, "recreate the directory if the earlier reply removed it");
            assert_eq!(read(&mut output).await["id"], "again");
            std::fs::remove_dir(&temp).unwrap();
            std::fs::write(&temp, "ordinary file").unwrap();
            write(&mut input, json!({"id":"blocked","method":"item/tool/call","params":{"threadId":"owner","tool":"run_command","arguments":{"command":"cargo test"}}})).await;
            let blocked = read(&mut output).await;
            assert_eq!(blocked["id"], "blocked");
            assert_eq!(blocked["result"]["success"], false);
            assert_eq!(blocked["result"]["contentItems"][0]["text"], "error.projectSandbox");
            assert!(commands.try_recv().is_err(), "do not launch when preparing the temporary directory fails");
            std::fs::remove_file(temp).unwrap();
            write(&mut input, json!({"id":"restored","method":"item/tool/call","params":{"threadId":"owner","tool":"run_command","arguments":{"command":"cargo test"}}})).await;
            assert!(commands.recv().await.unwrap().3, "the failed launch does not block later commands");
            assert_eq!(read(&mut output).await["id"], "restored");
            server.abort();
        }).await.expect("the isolated fake command was never dispatched");
    }

    #[tokio::test]
    async fn shared_server_does_not_receive_a_conversations_key() {
        let recorded = Arc::new(std::sync::Mutex::new(None));
        let capture = recorded.clone();
        let spawn: Spawner = Arc::new(move |bin, args, cwd| {
            let command = proc::marked(bin, args, cwd);
            let env = command.as_std().get_envs().filter_map(|(k, v)| v.map(|v| (k.to_owned(), v.to_owned()))).collect::<HashMap<_, _>>();
            *capture.lock().unwrap() = Some((args.to_vec(), env));
            Err(std::io::Error::other("fake"))
        });
        let opts = CodexServerOptions {
            permissions: Permissions::Ask,
            cli: crate::cli::Cli { command: "fake".into(), prefix: vec![] },
            cwd: PathBuf::new(),
            area: Area::Chat,
            user_hooks: false,
            windows: false,
            spawn: spawn.clone(),
            tools: Default::default(),
        };
        let identity = Arc::new(proc::ConversationIdentity { id: "one".into(), token: Some("test-key".into()) });
        let server = CodexServer::start(opts);
        let _session = server.open_identified(
            thread(Arc::new(|_| {})).params,
            Arc::new(|_| {}),
            Default::default(),
            ThreadIdentity { identity, spawn, native_goal: NativeGoalOpen::Ordinary },
            None,
            None,
        );
        let recorded = recorded.lock().unwrap();
        let (args, env) = recorded.as_ref().unwrap();
        assert!(args.iter().all(|arg| !arg.contains("test-key")));
        for name in ["FABER_AGENT_TOKEN", "FABER_AGENT_AUTH", "FABER_CONVERSATION"] {
            assert!(!env.contains_key(std::ffi::OsStr::new(name)));
        }
    }

    #[test]
    fn native_branch_fork_keeps_the_new_identity_and_current_thread_policy() {
        let mut owner = thread(Arc::new(|_| {}));
        let data = tempfile::tempdir().unwrap();
        let _store = crate::store::Store::new(data.path());
        // A runner's temporary folder can be spelled with 8.3 short names; the policy names the resolved long path.
        let protected = std::fs::canonicalize(data.path()).unwrap().to_string_lossy().trim_start_matches(r"\\?\").replace('\\', "/");
        owner.params.resume = Some("source".into());
        owner.identity = Some(ThreadIdentity {
            native_goal: NativeGoalOpen::Ordinary,
            identity: Arc::new(proc::ConversationIdentity { id: "branch".into(), token: Some("synthetic-branch-key".into()) }),
            spawn: Arc::new(|_, _, _| Err(std::io::Error::other("not spawned by this metadata test"))),
        });
        let (_, ordinary) = owner.open_params(Area::Chat, false);
        owner.native_fork = Some(ForkOpen {
            point: NativePoint { session: "source".into(), cursor: crate::rewind::Cursor { entry: None, turn: Some("kept".into()) } },
            verified: false,
            reply: None,
        });
        let (method, fork) = owner.open_params(Area::Chat, false);
        assert_eq!(method, "thread/fork");
        assert_eq!(fork["threadId"], "source");
        assert_eq!(fork["lastTurnId"], "kept");
        for key in ["approvalPolicy", "sandbox", "developerInstructions", "cwd"] {
            assert_eq!(fork[key], ordinary[key], "fork must preserve {key}");
        }
        // Each launch names its profile freshly, and parallel tests can open or close other stores in between.
        let policy = |params: &Value| {
            let mut config = params["config"].as_object().expect("thread config").clone();
            let selected = config.remove("default_permissions").expect("selected profile");
            let mut profile = config.remove(&format!("permissions.{}", selected.as_str().expect("profile name"))).expect("profile policy");
            let filesystem = profile["filesystem"].as_object_mut().expect("filesystem policy");
            assert_eq!(filesystem[&protected], "read");
            filesystem.retain(|path, access| {
                // The fixture's own store must stay protected. Other registered stores can only add read-only paths.
                path.starts_with(':') || path == &protected || access != "read"
            });
            (config, profile)
        };
        assert_eq!(policy(&fork), policy(&ordinary), "fork must preserve all config and the selected policy");
        assert_eq!(fork["config"]["shell_environment_policy.set"]["FABER_CONVERSATION"], "branch");
        assert_eq!(fork["config"]["shell_environment_policy.set"]["FABER_AGENT_TOKEN"], "synthetic-branch-key");
        assert!(fork.get("dynamicTools").is_none());
    }

    fn thread(sink: EventSink) -> Thread {
        Thread {
            before_turn: None,
            native_fork: None,
            sink,
            identity: None,
            unsubscribed: false,
            native_release: NativeRelease::Keep,
            retried_closing_resume: false,
            params: CodexThread {
                subagents: None,
                temporary: false,
                title: None,
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
                leads: false,
                verify: false,
                progress: false,
                faber: false,
            },
            id: Some("owner".into()),
            turn: None,
            turn_notice: TurnNotice::Pending,
            goal_resume: GoalResume::Allow,
            queued: vec![],
            compact: CompactRequest::None,
            state: Default::default(),
            approvals: HashMap::new(),
            questions: HashMap::new(),
            helpers: HashMap::new(),
            watch: Default::default(),
            stop_completion_grace: None,
            interrupt_pending: false,
            blocked: false,
            closing: false,
            stop_children: false,
            early_completion: vec![],
            commands: HashMap::new(),
            unconfirmed_commands: HashMap::new(),
            owns_server: false,
            server_writers: false,
            stop_reported: false,
            mcp_used: false,
            process_tools_used: false,
            tools: None,
        }
    }

    #[tokio::test(start_paused = true)]
    async fn stopped_codex_helper_reports_what_its_writer_exit_proved_in_the_fatal_error() {
        use tokio::io::AsyncWriteExt;

        for confirmed in [true, false] {
            let root = proc::ProcessIdentity { pid: 100, created: 1 };
            let stopped = Arc::new(AtomicBool::new(false));
            let query = stopped.clone();
            let stop = stopped.clone();
            let contained = proc::Containment::scripted_descendants(
                root,
                move || Ok(if confirmed && query.load(Ordering::Acquire) { vec![] } else { vec![root] }),
                move || stop.store(true, Ordering::Release),
            );
            let (driver, script) = tokio::io::duplex(65536);
            let (stdout, stdin) = tokio::io::split(driver);
            let (read_pipe, mut input) = tokio::io::split(script);
            let mut output = BufReader::new(read_pipe).lines();
            let (tx, rx) = unbounded_channel();
            let (sent, mut events) = unbounded_channel();
            let sink: EventSink = Arc::new(move |e| drop(sent.send(e)));
            let opts = CodexServerOptions {
                permissions: Permissions::Plan,
                cli: crate::cli::Cli { command: "unused".into(), prefix: vec![] },
                cwd: PathBuf::new(),
                area: Area::Project,
                user_hooks: false,
                windows: cfg!(windows),
                spawn: Arc::new(|_, _, _| panic!("scripted driver cannot start processes")),
                tools: Default::default(),
            };
            let p = proc::Proc { stdin: Box::new(stdin), stdout: Box::new(stdout), stderr: None, child: None, contained: Some(contained) };
            let server = tokio::spawn(serve(Ok(p), opts, rx, Arc::new(AtomicBool::new(true)), Arc::new(Mutex::new(()))));
            macro_rules! read {
                () => {
                    serde_json::from_str::<Value>(&output.next_line().await.unwrap().unwrap()).unwrap()
                };
            }
            macro_rules! send {
                ($value:expr) => {
                    input.write_all(format!("{}\n", $value).as_bytes()).await.unwrap()
                };
            }
            let init = read!();
            send!(json!({"id":init["id"],"result":{}}));
            assert_eq!(read!()["method"], "initialized");
            let mut params = thread(sink.clone()).params;
            params.permissions = Permissions::Plan;
            params.helper = crate::helper::Helper::Is;
            tx.send(ServerCommand::Open {
                key: 1,
                thread: Box::new(params),
                sink,
                tools: None,
                identity: None,
                before_turn: None,
                native_fork: None,
            })
            .unwrap();
            let start = read!();
            assert_eq!(start["method"], "thread/start");
            send!(json!({"id":start["id"],"result":{"thread":{"id":"helper","model":"model"}}}));
            let skills = read!();
            assert_eq!(skills["method"], "skills/list");
            send!(json!({"id":skills["id"],"result":{"data":[]}}));
            tx.send(ServerCommand::Session(1, SessionCommand::Send(UserTurn::text("Inspect the copy")))).unwrap();
            let start = read!();
            assert_eq!(start["method"], "turn/start");
            send!(json!({"id":start["id"],"result":{"turn":{"id":"helper-turn"}}}));
            input.shutdown().await.unwrap();
            let text = tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    if let EngineEvent::Error { message, fatal: true } = events.recv().await.expect("helper events closed") {
                        break message;
                    }
                }
            })
            .await
            .expect("the fatal report must reach the worker's caller");
            assert!(text.contains("Codex stopped"), "{text}");
            let evidence = if confirmed { "writer exit was confirmed" } else { "writer exit is unconfirmed" };
            assert!(text.contains(evidence), "{evidence}: {text}");
            assert_eq!(text.to_ascii_lowercase().contains("stop cause is unknown"), confirmed);
            assert!(stopped.load(Ordering::Acquire));
            server.abort();
        }
    }

    #[tokio::test]
    async fn worker_memory_codex_threads_share_the_actual_spawned_container() {
        let root = proc::ProcessIdentity { pid: 42, created: 10 };
        let container = proc::Containment::scripted_descendants(root, move || Ok(vec![root]), || {});
        let expected = container.owned_tree().unwrap();
        let process = std::sync::Mutex::new(Some(proc::Proc {
            stdin: Box::new(tokio::io::sink()),
            stdout: Box::new(tokio::io::empty()),
            stderr: None,
            child: None,
            contained: Some(container),
        }));
        let server = CodexServer::start(CodexServerOptions {
            permissions: Permissions::Ask,
            cli: crate::cli::Cli { command: "scripted".into(), prefix: vec![] },
            cwd: PathBuf::new(),
            area: Area::Project,
            user_hooks: false,
            windows: false,
            spawn: Arc::new(move |_, _, _| Ok(process.lock().unwrap().take().unwrap())),
            tools: Default::default(),
        });
        let sink: EventSink = Arc::new(|_| {});
        let one = server.open(thread(sink.clone()).params, sink.clone());
        let two = server.open(thread(sink.clone()).params, sink);
        assert!(expected.same_tree(&server.owned_tree().unwrap()));
        assert!(expected.same_tree(&one.owned_tree().unwrap()));
        assert!(expected.same_tree(&two.owned_tree().unwrap()));
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
    fn a_closed_thread_releases_its_kit_only_after_the_native_tree_and_every_writer_are_clear() {
        let root = proc::ProcessIdentity { pid: 100, created: 1 };
        let child = proc::ProcessIdentity { pid: 101, created: 2 };
        let members = Arc::new(std::sync::Mutex::new(vec![root, child]));
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
        let mut owner = thread(Arc::new(|_| {}));
        owner.closing = true;
        let mut peer = thread(Arc::new(|_| {}));
        peer.watch.begin();
        let mut threads = HashMap::from([(1, owner), (2, peer)]);
        let helpers = Subagents::default();
        refresh_native(&mut threads, Some(1), Some(&server), false);
        assert!(!threads[&1].can_release(1, &helpers, false, true), "a native descendant still leases the kit");
        members.lock().unwrap().clear();
        failed.store(true, Ordering::Release);
        refresh_native(&mut threads, Some(1), Some(&server), false);
        assert!(!threads[&1].can_release(1, &helpers, false, true), "an unknown process tree cannot release anything");
        failed.store(false, Ordering::Release);
        members.lock().unwrap().push(root);
        refresh_native(&mut threads, Some(1), Some(&server), false);
        assert!(!threads[&1].can_release(1, &helpers, false, false), "a real process without a containment proof stays leased");
        threads.get_mut(&1).unwrap().helpers.insert("helper".into(), json!(1));
        assert!(!threads[&1].can_release(1, &helpers, false, true), "the helper may still need its owner's resources");
        threads.get_mut(&1).unwrap().helpers.clear();
        threads.get_mut(&1).unwrap().state.native_commands.track(&json!({"method":"item/started", "params":{"item":{
            "type":"commandExecution", "id":"native", "processId":"terminal"}}}));
        assert!(!threads[&1].can_release(1, &helpers, false, true));
        threads.get_mut(&1).unwrap().state.native_commands.track(&json!({"method":"item/completed", "params":{"item":{
            "type":"commandExecution", "id":"native", "processId":"terminal", "exitCode":0}}}));
        assert!(threads[&1].can_release(1, &helpers, false, true), "the idle owner releases while the peer keeps running");
        assert!(!threads[&2].can_release(2, &helpers, false, true));
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
        owner.state.native_commands.track(&json!({"method":"item/completed", "params":{"item":{
            "type":"commandExecution", "id":"command", "processId":"0020000", "exitCode":null}}}));
        assert_eq!(
            owner.state.native_commands.stops("owner"),
            [("owner".into(), "command".into(), "0020000".into())],
            "even numeric-looking terminal handles are preserved verbatim"
        );
        owner.publish_writers(1, &Subagents::default());
        assert!(
            matches!(events.try_recv(), Ok(EngineEvent::Writers { active, unknown: true }) if active == ["native-command:command", "process-tree"])
        );
        let mut threads = HashMap::from([(1, owner)]);
        let helpers = Subagents::default();
        let _ = refresh_native(&mut threads, Some(1), Some(&server), false);
        threads[&1].publish_writers(1, &helpers);
        assert!(
            matches!(events.try_recv(), Ok(EngineEvent::Writers { active, unknown: true }) if active == ["native-command:command"]),
            "a reported terminal can still write from outside the original group"
        );
        assert!(!threads[&1].drained(1, &helpers));
        threads.get_mut(&1).unwrap().state.native_commands.track(&json!({"method":"item/completed", "params":{"item":{
            "type":"commandExecution", "id":"command", "processId":"0020000", "exitCode":0}}}));
        threads[&1].publish_writers(1, &helpers);
        assert!(
            matches!(events.try_recv(), Ok(EngineEvent::Writers { active, unknown:false }) if active.is_empty()),
            "CLI completion releases the terminal after the process-table proof is clear"
        );
        members.lock().unwrap().push(child);
        let _ = refresh_native(&mut threads, Some(1), Some(&server), false);
        threads[&1].publish_writers(1, &helpers);
        assert!(
            matches!(events.try_recv(), Ok(EngineEvent::Writers { active, unknown: true }) if active == ["process-tree"]),
            "an unreported hook child is a writer without any command item"
        );
        failed.store(true, Ordering::Release);
        let _ = refresh_native(&mut threads, Some(1), Some(&server), false);
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
            owner.state.native_commands.track(&json!({"method":"item/completed","params":{"item":{
                "type":"commandExecution","id":"cmd","processId":"opaque-terminal","exitCode":null}}}));
            let mut peer = thread(sink);
            peer.owns_server = true;
            let mut threads = HashMap::from([(1, owner), (2, peer)]);
            let _ = refresh_native(&mut threads, Some(1), Some(&server), false);
            if listed_gone {
                threads.get_mut(&1).unwrap().state.native_commands.confirm_gone("cmd", "opaque-terminal");
            } else {
                threads.get_mut(&1).unwrap().state.native_commands.track(&json!({"method":"item/completed","params":{"item":{
                    "type":"commandExecution","id":"cmd","processId":"opaque-terminal","exitCode":0}}}));
            }
            let _ = refresh_native(&mut threads, Some(1), Some(&server), false);
            assert!(
                threads.values().all(|thread| thread.server_writers),
                "neither CLI completion nor listing absence releases the surviving child or its idle peer"
            );
            outside.lock().unwrap().clear();
            let _ = refresh_native(&mut threads, Some(1), Some(&server), false);
            assert!(threads.values().all(|thread| !thread.server_writers));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn rewind_idle_threads_share_one_native_tree_check_for_an_unannounced_child() {
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
            let checks = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let count = checks.clone();
            let contained = proc::Containment::scripted_descendants(root, move || {
                count.fetch_add(1, Ordering::Relaxed);
                Ok(query.lock().unwrap().clone())
            }, || {});
            let (driver, script) = tokio::io::duplex(65536);
            let (stdout, stdin) = tokio::io::split(driver);
            let (read_pipe, mut input) = tokio::io::split(script);
            let mut output = BufReader::new(read_pipe).lines();
            let (tx, rx) = unbounded_channel();
            let (sent, mut events) = unbounded_channel();
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
            let server = tokio::spawn(serve(Ok(p), opts, rx, Arc::new(AtomicBool::new(true)), Arc::new(Mutex::new(()))));
            let init = read(&mut output).await;
            write(&mut input, json!({"id":init["id"],"result":{}})).await;
            assert_eq!(read(&mut output).await["method"], "initialized");
            for (key, id) in [(1, "owner"), (2, "peer"), (3, "other")] {
                let send = sent.clone();
                let sink: EventSink = Arc::new(move |event| drop(send.send((key, event))));
                tx.send(ServerCommand::Open {
                    key,
                    thread: Box::new(thread(sink.clone()).params),
                    sink,
                    tools: None,
                    identity: None,
                    before_turn: None,
                    native_fork: None,
                }).unwrap();
                let open = read(&mut output).await;
                write(&mut input, json!({"id":open["id"],"result":{"thread":{"id":id}}})).await;
                while !matches!(events.recv().await.unwrap(), (k, EngineEvent::Started { .. }) if k == key) {}
                write(&mut input, json!({"method":"turn/started","params":{"threadId":id,"turn":{"id":"first"}}})).await;
                write(&mut input, json!({"method":"turn/completed","params":{"threadId":id,"turn":{"id":"first","status":"completed"}}}))
                    .await;
                while !matches!(events.recv().await.unwrap(), (k, EngineEvent::Done { .. }) if k == key) {}
                assert!(matches!(events.recv().await.unwrap(), (k, EngineEvent::Writers { active, unknown: true }) if k == key && active == ["process-tree"]));
            }
            members.lock().unwrap().retain(|member| *member == root);
            // No command or notification follows; only the owned OS membership changed.
            let before = checks.load(Ordering::Relaxed);
            tokio::time::advance(Duration::from_secs(1)).await;
            let mut idle = HashSet::new();
            while idle.len() < 3 {
                if let (key, EngineEvent::Writers { active, unknown: false }) = events.recv().await.unwrap() && active.is_empty() {
                    idle.insert(key);
                }
            }
            assert_eq!(checks.load(Ordering::Relaxed) - before, 1, "all idle threads share the same owned process tree");
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
        owner.state.native_commands.track(&json!({"method":"item/started","params":{"item":{"type":"commandExecution","id":"cmd"}}}));
        owner.state.native_commands.track(&json!({"method":"item/completed","params":{"item":{
            "type":"commandExecution","id":"cmd","processId":"terminal","exitCode":null,"status":"completed"}}}));
        owner.publish_writers(1, &Subagents::default());
        assert!(matches!(events.try_recv(), Ok(EngineEvent::Writers { active, unknown: true }) if active == ["native-command:cmd"]));
        assert_eq!(owner.state.native_commands.stops("owner"), [("owner".into(), "cmd".into(), "terminal".into())]);
        owner.state.native_commands.confirm_gone("cmd", "different-process");
        assert!(owner.state.native_commands.unknown(), "a stale confirmation never clears a different writer");
        owner.state.native_commands.track(&json!({"method":"item/completed","params":{"item":{
            "type":"commandExecution","id":"cmd","processId":"terminal","exitCode":0,"status":"completed"}}}));
        owner.publish_writers(1, &Subagents::default());
        assert!(
            matches!(events.try_recv(), Ok(EngineEvent::Writers { active, unknown: false }) if active.is_empty()),
            "the CLI explicitly reported this command ended"
        );
        owner.state.native_commands.confirm_gone("cmd", "terminal");
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
            let server = tokio::spawn(serve(Ok(p), opts, rx, Arc::new(AtomicBool::new(true)), Arc::new(Mutex::new(()))));
            let init = read(&mut output).await;
            write(&mut input, json!({"id":init["id"],"result":{}})).await;
            assert_eq!(read(&mut output).await["method"], "initialized");
            for (key, id) in [(1, "owner"), (2, "peer")] {
                tx.send(ServerCommand::Open { key, thread: Box::new(thread(sink.clone()).params), sink: sink.clone(), tools: None, identity: None, before_turn: None, native_fork: None }).unwrap();
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
    async fn closing_an_idle_thread_requests_unload_before_the_shared_tree_empties() {
        use tokio::io::AsyncWriteExt;
        let (driver, script) = tokio::io::duplex(65536);
        let (stdout, stdin) = tokio::io::split(driver);
        let (read_pipe, mut input) = tokio::io::split(script);
        let mut output = BufReader::new(read_pipe).lines();
        let (tx, rx) = unbounded_channel();
        let (sent, mut events) = unbounded_channel();
        let sink = |key| {
            let sent = sent.clone();
            Arc::new(move |event| {
                let _ = sent.send((key, event));
            }) as EventSink
        };
        let spawn: Spawner = Arc::new(|_, _, _| panic!("scripted driver must not spawn a process"));
        let opts = CodexServerOptions {
            permissions: Permissions::FullAccess,
            cli: crate::cli::Cli { command: "unused".into(), prefix: vec![] },
            cwd: PathBuf::new(),
            area: Area::Project,
            user_hooks: true,
            windows: false,
            spawn: spawn.clone(),
            tools: Default::default(),
        };
        let root = proc::ProcessIdentity { pid: 100, created: 1 };
        let child = proc::ProcessIdentity { pid: 101, created: 2 };
        let child_present = Arc::new(AtomicBool::new(true));
        let query = child_present.clone();
        let contained = proc::Containment::scripted_descendants(
            root,
            move || Ok(if query.load(Ordering::Acquire) { vec![root, child] } else { vec![root] }),
            || {},
        );
        let p = proc::Proc { stdin: Box::new(stdin), stdout: Box::new(stdout), stderr: None, child: None, contained: Some(contained) };
        let server = tokio::spawn(serve(Ok(p), opts, rx, Arc::new(AtomicBool::new(true)), Arc::new(Mutex::new(()))));
        macro_rules! read {
            () => {{
                loop {
                    let message: Value = serde_json::from_str(&output.next_line().await.unwrap().unwrap()).unwrap();
                    if message["method"] != "skills/list" {
                        break message;
                    }
                }
            }};
        }
        macro_rules! write {
            ($message:expr) => {
                input.write_all(format!("{}\n", $message).as_bytes()).await.unwrap()
            };
        }
        tokio::time::timeout(RELEASE_FIRST_RECHECK + THREAD_RESTORE_GRACE + Duration::from_secs(10), async {
            let init = read!();
            write!(json!({"id":init["id"],"result":{"userAgent":"faber/0.166.0"}}));
            assert_eq!(read!()["method"], "initialized");
            for (key, id) in [(1, "owner"), (2, "peer")] {
                tx.send(ServerCommand::Open {
                    key,
                    thread: Box::new(thread(sink(key)).params),
                    sink: sink(key),
                    tools: None,
                    identity: None,
                    before_turn: None,
                    native_fork: None,
                })
                .unwrap();
                let open = read!();
                assert_eq!(open["params"]["config"]["thread_unload_delay_secs"], 2);
                write!(json!({"id":open["id"],"result":{"thread":{"id":id}}}));
                while !matches!(events.recv().await.unwrap(), (k, EngineEvent::Started { .. }) if k == key) {}
            }
            tx.send(ServerCommand::Session(1, SessionCommand::Close)).unwrap();
            let unload = read!();
            assert_eq!(unload["method"], "thread/unsubscribe");
            assert_eq!(unload["params"], json!({"threadId":"owner"}));
            let mut next = thread(sink(3)).params;
            next.resume = Some("owner".into());
            tx.send(ServerCommand::Open {
                key: 3,
                thread: Box::new(next),
                sink: sink(3),
                tools: None,
                identity: None,
                before_turn: None,
                native_fork: None,
            })
            .unwrap();
            let resume = read!();
            assert_eq!(resume["method"], "thread/resume");
            write!(json!({"id":resume["id"],"error":{"code":-32600,
                "message":"thread owner is closing; retry thread/resume after the thread is closed"}}));
            child_present.store(false, Ordering::Release);
            tokio::time::advance(Duration::from_secs(2)).await;
            write!(json!({"method":"turn/started","params":{"threadId":"peer","turn":{"id":"work"}}}));
            loop {
                match events.recv().await.unwrap() {
                    (1, EngineEvent::WriterExit { .. }) => panic!("unload can still run shutdown hooks"),
                    (3, EngineEvent::Error { .. }) => panic!("opening while unloading must retry"),
                    (2, EngineEvent::TurnStarted) => break,
                    _ => {}
                }
            }
            child_present.store(true, Ordering::Release);
            write!(json!({"method":"thread/closed","params":{"threadId":"owner"}}));
            write!(json!({"id":unload["id"],"result":{"status":"unsubscribed"}}));
            let retry = read!();
            assert_eq!(retry["method"], "thread/resume");
            assert_eq!(retry["params"]["threadId"], "owner");
            write!(json!({"id":retry["id"],"result":{"thread":{"id":"owner"}}}));
            loop {
                match events.recv().await.unwrap() {
                    (1, EngineEvent::WriterExit { .. }) => panic!("the native tree still has a descendant"),
                    (3, EngineEvent::Started { .. }) => break,
                    _ => {}
                }
            }
            child_present.store(false, Ordering::Release);
            while !matches!(events.recv().await.unwrap(), (1, EngineEvent::WriterExit { confirmed: true })) {}
            child_present.store(true, Ordering::Release);
            tx.send(ServerCommand::Session(3, SessionCommand::Close)).unwrap();
            let unload = read!();
            assert_eq!(unload["method"], "thread/unsubscribe");
            write!(json!({"id":unload["id"],"result":{"status":"notSubscribed"}}));
            let mut next = thread(sink(4)).params;
            next.resume = Some("owner".into());
            let identity = Some(ThreadIdentity {
                identity: Arc::new(proc::ConversationIdentity { id: "owner".into(), token: Some("synthetic-proof".into()) }),
                spawn: spawn.clone(),
                native_goal: NativeGoalOpen::Ordinary,
            });
            tx.send(ServerCommand::Open {
                key: 4,
                thread: Box::new(next),
                sink: sink(4),
                tools: None,
                identity,
                before_turn: None,
                native_fork: None,
            })
            .unwrap();
            assert!(tokio::time::timeout(Duration::from_millis(20), output.next_line()).await.is_err());
            let probe = read!();
            assert_eq!(probe["method"], "thread/unsubscribe");
            assert_eq!(probe["params"], unload["params"]);
            assert_ne!(probe["id"], unload["id"]);
            while let Ok(event) = events.try_recv() {
                assert!(!matches!(event, (3, EngineEvent::WriterExit { .. })), "notSubscribed can still leave a live runtime");
            }
            write!(json!({"id":probe["id"],"result":{"status":"notLoaded"}}));
            let admission = tokio::time::timeout(Duration::from_secs(1), async { read!() })
                .await
                .expect("unload confirmation allows resume while the shared tree still has a descendant");
            assert_eq!(admission["method"], "thread/unsubscribe", "the named successor may now prepare its new identity");
            let mut contender = thread(sink(5)).params;
            contender.resume = Some("owner".into());
            tx.send(ServerCommand::Open {
                key: 5,
                thread: Box::new(contender),
                sink: sink(5),
                tools: None,
                identity: Some(ThreadIdentity {
                    identity: Arc::new(proc::ConversationIdentity { id: "owner".into(), token: Some("synthetic-contender".into()) }),
                    spawn,
                    native_goal: NativeGoalOpen::Ordinary,
                }),
                before_turn: None,
                native_fork: None,
            })
            .unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(20), output.next_line()).await.is_err(),
                "a contender cannot unsubscribe the pending successor"
            );
            write!(json!({"id":admission["id"],"result":{"status":"notLoaded"}}));
            let next = read!();
            assert_eq!(next["method"], "thread/resume");
            assert_eq!(next["params"]["config"]["thread_unload_delay_secs"], 2);
            assert!(
                tokio::time::timeout(Duration::from_millis(20), output.next_line()).await.is_err(),
                "the contender also waits for the resume reply"
            );
            write!(json!({"id":next["id"],"result":{"thread":{"id":"owner"}}}));
            while !matches!(events.recv().await.unwrap(), (4, EngineEvent::Started { .. })) {}
            assert!(
                tokio::time::timeout(Duration::from_secs(2), async {
                    while !matches!(events.recv().await.unwrap(), (3, EngineEvent::WriterExit { .. })) {}
                })
                .await
                .is_err(),
                "the physical descendant still holds the old writer lease"
            );
            child_present.store(false, Ordering::Release);
            while !matches!(events.recv().await.unwrap(), (3, EngineEvent::WriterExit { confirmed: true })) {}
            let mut failed = false;
            loop {
                match events.recv().await.unwrap() {
                    (5, EngineEvent::Error { message, fatal: true }) => {
                        assert_eq!(message, "reason.codexThreadOpenTimeout");
                        failed = true;
                    }
                    (5, EngineEvent::WriterExit { confirmed: true }) => {
                        assert!(failed, "a blocked contender is told why it could not open");
                        break;
                    }
                    (4, EngineEvent::WriterExit { .. }) | (2, EngineEvent::WriterExit { .. }) => {
                        panic!("an unopened contender cannot stop either owner")
                    }
                    _ => {}
                }
            }
            write!(json!({"method":"turn/completed","params":{"threadId":"peer","turn":{"id":"work","status":"completed"}}}));
            while !matches!(events.recv().await.unwrap(), (2, EngineEvent::Done { ok: true, .. })) {}
        })
        .await
        .expect("the owner unloads and the peer keeps working");
        server.abort();
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
            for (replacement, version, pending_resume) in [
                ("replacement", "0.166.0", false),
                ("owner", "0.166.0", false),
                ("owner", "0.166.0", true),
                ("replacement", "0.120.0", false),
            ] {
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
                let server = tokio::spawn(serve(Ok(p), opts, rx, Arc::new(AtomicBool::new(true)), Arc::new(Mutex::new(()))));
                let init = read(&mut output).await;
                write(&mut input, json!({"id":init["id"],"result":{"userAgent":format!("faber/{version}")}})).await;
                assert_eq!(read(&mut output).await["method"], "initialized");
                for (key, id) in [(1, "owner"), (2, "peer")] {
                    tx.send(ServerCommand::Open { key, thread: Box::new(thread(sink(key)).params), sink: sink(key), tools: None, identity: None, before_turn: None, native_fork: None }).unwrap();
                    let open = read(&mut output).await;
                    assert_eq!(open["params"]["config"]["thread_unload_delay_secs"],
                        if version == "0.166.0" { json!(2) } else { Value::Null });
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
                if replacement == "owner" { next.resume = Some("owner".into()); }
                tx.send(ServerCommand::Open { key: 3, thread: Box::new(next), sink: sink(3), tools: None, identity: None, before_turn: None, native_fork: None }).unwrap();
                let open = read(&mut output).await;
                let method = if replacement == "owner" { "thread/resume" } else { "thread/start" };
                assert_eq!(open["method"], method, "the owner is still subscribed while its command can write");
                let late = json!({"method":"item/completed","params":{"threadId":"owner","item":{
                    "type":"commandExecution","id":"cmd","processId":"terminal","exitCode":0}}});
                if pending_resume {
                    write(&mut input, late.clone()).await;
                    while !matches!(events.recv().await.unwrap(), (1, EngineEvent::WriterExit { confirmed: true })) {}
                }
                write(&mut input, json!({"id":open["id"],"result":{"thread":{"id":replacement}}})).await;
                while !matches!(events.recv().await.unwrap(), (3, EngineEvent::Started { .. })) {}
                write(&mut input, json!({"method":"turn/started","params":{"threadId":replacement,"turn":{"id":"second"}}})).await;
                write(&mut input, json!({"method":"turn/completed","params":{"threadId":replacement,"turn":{"id":"second","status":"completed"}}})).await;
                while !matches!(events.recv().await.unwrap(), (3, EngineEvent::Writers { active, unknown: false }) if active.is_empty()) {}
                // Only the earlier session can confirm this command's exit, even after native-thread reuse.
                if !pending_resume {
                    write(&mut input, late).await;
                    while !matches!(events.recv().await.unwrap(), (1, EngineEvent::Writers { active, .. }) if active.is_empty()) {}
                }
                write(&mut input, json!({"method":"turn/started","params":{"threadId":replacement,"turn":{"id":"third"}}})).await;
                while !matches!(events.recv().await.unwrap(), (3, EngineEvent::TurnStarted)) {}
                tx.send(ServerCommand::Session(2, SessionCommand::Interrupt)).unwrap();
                if replacement != "owner" && version == "0.166.0" {
                    let unsubscribe = read(&mut output).await;
                    assert_eq!(unsubscribe["method"], "thread/unsubscribe", "closing lets the shared server unload the drained thread");
                    assert_eq!(unsubscribe["params"], json!({"threadId":"owner"}));
                    write(&mut input, json!({"id":unsubscribe["id"],"error":{"code":-32601,"message":"unsupported"}})).await;
                    while !matches!(events.recv().await.unwrap(), (1, EngineEvent::WriterExit { confirmed: true })) {}
                }
                let peer = read(&mut output).await;
                assert_eq!(peer["params"], json!({"threadId":"peer","processId":"terminal"}), "the peer stayed live and retained its own owner");
                write(&mut input, json!({"method":"turn/completed","params":{"threadId":replacement,"turn":{"id":"third","status":"completed"}}})).await;
                while !matches!(events.recv().await.unwrap(), (3, EngineEvent::Done { ok: true, .. })) {}
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
            let server = tokio::spawn(serve(Ok(p), opts, rx, Arc::new(AtomicBool::new(true)), Arc::new(Mutex::new(()))));
            let init = read(&mut output).await;
            assert_eq!(init["method"], "initialize");
            write(&mut input, json!({"id":init["id"],"result":{}})).await;
            assert_eq!(read(&mut output).await["method"], "initialized");
            tx.send(ServerCommand::Open {
                key: 1,
                thread: Box::new(thread(sink.clone()).params),
                sink,
                tools: None,
                identity: None,
                before_turn: None,
                native_fork: None,
            })
            .unwrap();
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

#[cfg(test)]
mod tool_result_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_picture_the_agent_looked_at_is_a_finished_step_and_a_failed_command_still_fails() {
        let mut state = CodexTurnState::default();
        let done = |item: Value, state: &mut CodexTurnState| {
            parse_codex_notification(&json!({"method":"item/completed","params":{"threadId":"t","item":item}}), state).into_iter().find_map(
                |e| match e {
                    EngineEvent::ToolResult { ok, .. } => Some(ok),
                    _ => None,
                },
            )
        };
        let view = json!({"id":"v","type":"imageView","path":"look/phone.png","startedAtMs":1,"completedAtMs":2});
        assert_eq!(done(view, &mut state), Some(true));
        let failed = json!({"id":"c","type":"commandExecution","command":"x","status":"failed","exitCode":1,"aggregatedOutput":"failed"});
        assert_eq!(done(failed, &mut state), Some(false));
    }

    #[test]
    fn tool_completions_preserve_observed_search_and_command_details() {
        let cases = [
            (json!({"type":"mcpToolCall", "status":"failed", "error":{"message":"Search timed out"}}), false, "Search timed out"),
            (
                json!({"type":"mcpToolCall", "status":"failed", "result":{"content":[
                    {"type":"image", "data":"excluded"}, {"type":"text", "text":"Page unavailable"}, {"type":"text", "text":"Try a different source"}
                ]}}),
                false,
                "Page unavailable\nTry a different source",
            ),
            (json!({"type":"webSearch", "query":"shipping docs", "action":{"type":"search"}}), true, "shipping docs"),
            (
                json!({"type":"webSearch", "query":"shipping docs ...", "action":{"type":"search", "queries":["shipping docs", "version notes"]},
                    "results":[{"text":"unchecked result"}]}),
                true,
                "shipping docs\nversion notes",
            ),
            (
                json!({"type":"webSearch", "query":"", "action":{"type":"search", "queries":[" ", null], "query":"single term"}}),
                true,
                "single term",
            ),
            (json!({"type":"webSearch", "query":"", "results":null}), true, ""),
            (json!({"type":"webSearch", "query":"", "status":"completed"}), true, ""),
            (
                json!({"type":"webSearch", "query":"https://example.test/docs", "action":{"type":"openPage","url":"https://example.test/docs"}}),
                true,
                "https://example.test/docs",
            ),
            (
                json!({"type":"webSearch", "query":"", "action":{"type":"openPage","url":"https://example.test/docs"}}),
                true,
                "https://example.test/docs",
            ),
            (
                json!({"type":"webSearch", "query":"connector docs", "action":{"type":"findInPage","url":"https://example.test/docs", "pattern":"quota"}}),
                true,
                "https://example.test/docs\nquota",
            ),
            (json!({"type":"mcpToolCall", "status":"failed", "result":{"content":[{"type":"image", "text":"ignored"}]}}), false, "failed"),
            (json!({"type":"commandExecution", "status":"failed", "exitCode":23, "aggregatedOutput":""}), false, "failed (23)"),
            (
                json!({"type":"commandExecution", "status":"failed", "exitCode":23, "aggregatedOutput":"Actual stderr"}),
                false,
                "Actual stderr",
            ),
        ];
        for (mut item, expected_ok, expected_text) in cases {
            item["id"] = json!("step");
            let msg = json!({"method":"item/completed", "params":{"threadId":"thread", "item":item}});
            let events = parse_codex_notification(&msg, &mut CodexTurnState::default());
            let result = events.iter().find_map(|event| match event {
                EngineEvent::ToolResult { ok, text, .. } => Some((*ok, text.as_str())),
                _ => None,
            });
            assert_eq!(result, Some((expected_ok, expected_text)), "{item}");
        }
    }

    #[test]
    fn long_connector_failures_keep_the_cause_for_the_engine_to_mask() {
        let prefix = "é".repeat(5000);
        let cause = "The account has no access to the requested collection.";
        let text = format!("{prefix}\n{cause}");
        let error = json!({"error":{"message":text}});
        let result = json!({"result":{"content":[{"type":"text","text":prefix}, {"type":"text","text":cause}]}});
        assert!(mcp_failure_text(&error, "failed") == text, "the provider error kept its last line");
        assert!(mcp_failure_text(&result, "failed") == text, "several text parts kept their final cause");

        let exact = format!("{}{}", "x".repeat(512 * 1024 - cause.len()), cause);
        let error = json!({"error":{"message":exact}});
        assert!(mcp_failure_text(&error, "failed") == exact, "the exact byte limit keeps the whole error");
        let over_limit = json!({"result":{"content":[{"type":"text","text":exact}, {"type":"text","text":cause}]}});
        assert_eq!(mcp_failure_text(&over_limit, "failed"), "x".repeat(4000));
    }

    #[test]
    fn a_native_web_start_only_keeps_the_request_fields() {
        let item = json!({"id":"step", "type":"webSearch", "action":{"type":"openPage","url":"https://example.test/docs"},
            "results":[{"text":"page contents"}]});
        let msg = json!({"method":"item/started", "params":{"threadId":"thread", "item":item}});
        let events = parse_codex_notification(&msg, &mut CodexTurnState::default());
        assert!(events.iter().any(|event| matches!(event, EngineEvent::Tool { input, .. } if input == "https://example.test/docs")));
    }

    #[test]
    fn a_background_command_without_an_exit_code_cannot_be_proof() {
        let mut state = CodexTurnState::default();
        let note = |exit, process| {
            json!({"method":"item/completed", "params":{
                "threadId":"thread", "item":{"id":"cmd", "type":"commandExecution", "status":"completed", "processId":process, "exitCode":exit}
            }})
        };
        for process in [json!("pty"), Value::Null] {
            let events = parse_codex_notification(&note(Value::Null, process), &mut state);
            assert!(events.iter().any(|event| matches!(event, EngineEvent::ToolDetached { id } if id == "cmd")));
            assert!(!events.iter().any(|event| matches!(
                event,
                EngineEvent::ToolResult { .. } | EngineEvent::Background(crate::background::Update::End { .. })
            )));
        }
        let events = parse_codex_notification(&note(json!(23), Value::Null), &mut state);
        assert!(events.iter().any(|event| matches!(event, EngineEvent::ToolResult { ok: false, text, .. } if text == "completed (23)")));
        assert!(events.iter().any(|event| matches!(
            event,
            EngineEvent::Background(crate::background::Update::End { state: crate::background::State::Failed, .. })
        )));
    }

    #[test]
    fn command_exit_code_does_not_displace_its_output() {
        for (status, code, expected) in [("failed", json!(23), Some(23)), ("completed", json!(0), Some(0)), ("declined", json!(0), None)] {
            let item =
                json!({"id":"cmd", "type":"commandExecution", "status":status, "exitCode":code, "aggregatedOutput":"Command output"});
            let event = json!({"method":"item/completed", "params":{"threadId":"thread", "item":item}});
            let events = parse_codex_notification(&event, &mut CodexTurnState::default());
            assert!(events.iter().any(|event| matches!(event,
                EngineEvent::ToolResult { exit_code, text, .. } if *exit_code == expected && text == "Command output"
            )));
        }
    }

    #[test]
    fn isolated_command_exit_code_is_separate_from_its_output() {
        for (tool, trailing, exit_code, expected_text) in [
            ("run_command", Some("Process exit code: 23"), Some(23), "Process exit code: 0"),
            ("run_command", None, None, "Process exit code: 0"),
            ("other", Some("Process exit code: 23"), None, "Process exit code: 0\nProcess exit code: 23"),
        ] {
            let mut content_items = vec![json!({"type":"inputText","text":"Process exit code: 0"})];
            if let Some(text) = trailing {
                content_items.push(json!({"type":"inputText","text":text}));
            }
            let item = json!({"id":"cmd", "type":"dynamicToolCall", "tool":tool, "status":"completed", "success":false, "contentItems":content_items});
            let note = json!({"method":"item/completed", "params":{"threadId":"thread", "item":item}});
            let events = parse_codex_notification(&note, &mut CodexTurnState::default());
            assert!(
                events.iter().any(|event| matches!(event,
                    EngineEvent::ToolResult { ok: false, exit_code: code, text, .. } if *code == exit_code && text == expected_text
                )),
                "{tool} {trailing:?}: {events:?}"
            );
        }
    }
}

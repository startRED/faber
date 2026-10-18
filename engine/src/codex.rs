//! Codex driver: one shared `codex app-server` process, spoken to over JSON-RPC on stdio, with each conversation as a
//! thread in it.

use crate::claude::EventSink;
use crate::events::{Delivery, EngineEvent, Session, SessionCommand, UserTurn};
use crate::model::{Area, Permissions, ReasoningSummary, Tuning, Verbosity, WebSearch};
use crate::persona::{PersonaInput, persona, worker_contract};
use crate::proc::{self, Spawner};
use crate::signals::{codex_account, codex_signals};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

const CHAT_INSTRUCTIONS: &str = "You are a helpful, precise assistant in a chat app. Answer in the language of the user. \
Use Markdown when it helps. Your working folder belongs to this conversation: run code and create files \
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

/// Retain names only: configuration values are ignored, never copied into requests or errors.
pub fn thread_exposure(params: &mut Value, reply: &str) -> Result<(), crate::project::policy::Blocker> {
    #[derive(serde::Deserialize)]
    struct Reply {
        result: Config,
    }
    #[derive(serde::Deserialize)]
    struct Config {
        config: Names,
    }
    #[derive(serde::Deserialize)]
    struct Names {
        #[serde(default)]
        mcp_servers: HashMap<String, serde::de::IgnoredAny>,
    }
    let reply: Reply = serde_json::from_str(reply).map_err(|_| crate::project::policy::Blocker::Restrictions)?;
    if reply.result.config.mcp_servers.len() > 1000
        || reply.result.config.mcp_servers.keys().any(|name| name.is_empty() || name.len() > 256)
    {
        return Err(crate::project::policy::Blocker::Restrictions);
    }
    params["config"]["mcp_servers"] =
        Value::Object(reply.result.config.mcp_servers.into_keys().map(|name| (name, json!({"enabled":false}))).collect());
    Ok(())
}

/// Automatic roles always start isolated, whatever mode the person chose: no hooks, plugins, apps or MCP servers, Faber's own
/// sandbox on Windows, and no native shell or subagents.
pub fn project_codex_args(windows: bool) -> Vec<String> {
    let mut args = codex_args(Area::Chat, windows, false, Permissions::Isolated);
    args.extend(["-c".into(), "model_provider=\"openai\"".into()]);
    args.extend(["-c".into(), format!("model_catalog_json={}", json!(project_catalog_path().to_string_lossy()))]);
    for setting in [
        "developer_instructions=\"\"",
        "skills.include_instructions=false",
        "skills.bundled.enabled=false",
        "tools.update_plan.enabled=false",
        "tools.experimental_request_user_input.enabled=false",
    ] {
        args.extend(["-c".into(), setting.into()]);
    }
    for feature in [
        "shell_tool",
        "multi_agent",
        "multi_agent_v2",
        "view_image",
        "request_permissions_tool",
        "code_mode",
        "code_mode_only",
        "code_mode_host",
        "code_mode_tool_search",
        "token_budget",
        "deferred_executor",
        "send_message_to_user_async",
        "current_time_reminder",
        "sleep_tool",
        "image_generation",
        "imagegenext",
    ] {
        args.extend(["--disable".into(), feature.into()]);
    }
    args
}

fn project_catalog_path() -> &'static Path {
    static PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    PATH.get_or_init(|| std::env::temp_dir().join(format!("faber-role-model-{}.json", uuid::Uuid::new_v4())))
}

/// Original client metadata for the qualified MVP model; it grants no native tool or model delegation.
/// The installed startup-only model_catalog_json control consumes ModelInfo from the published protocol.
pub fn project_codex_catalog() -> Value {
    json!({"models":[{
        "slug":"gpt-6-luna", "display_name":"Project worker", "description":null,
        "default_reasoning_level":"medium",
        "supported_reasoning_levels":[
            {"effort":"low","description":"Low"}, {"effort":"medium","description":"Medium"},
            {"effort":"high","description":"High"}, {"effort":"xhigh","description":"Extra high"},
            {"effort":"max","description":"Maximum"}],
        "shell_type":"disabled", "visibility":"list", "supported_in_api":true, "priority":0,
        "support_verbosity":false, "apply_patch_tool_type":null,
        "model_messages":{"instructions_template":""},
        "truncation_policy":{"mode":"tokens","limit":10000},
        "context_window":1050000, "max_context_window":1050000,
        "experimental_supported_tools":[], "input_modalities":["text"],
        "include_skills_usage_instructions":false, "include_apps_usage_instructions":false,
        "include_plugin_usage_instructions":false, "node_repl_disabled":true,
        "supports_search_tool":false, "supports_experimental_context":false,
        "tool_mode":"direct", "multi_agent_version":null
    }]})
}

/// Writes only a generated temporary control file, never the CLI's configuration or credentials.
#[cfg(windows)]
pub fn prepare_project_codex_catalog() -> std::io::Result<()> {
    use std::io::Write;
    use std::os::windows::fs::OpenOptionsExt;
    static FILE: std::sync::OnceLock<std::io::Result<std::fs::File>> = std::sync::OnceLock::new();
    FILE.get_or_init(|| {
        // The held handle denies writes; Windows removes this exact created file on process exit.
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .share_mode(0x0000_0001 | 0x0000_0004)
            .custom_flags(0x0400_0000)
            .open(project_catalog_path())?;
        file.write_all(&serde_json::to_vec(&project_codex_catalog())?)?;
        file.sync_all()?;
        Ok(file)
    })
    .as_ref()
    .map(|_| ())
    .map_err(std::io::Error::other)
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
                });
                None
            }
            (Some("item/completed"), Some(end @ ("completed" | "interrupted"))) => {
                let sub = self.0.get(&thread).filter(|sub| sub.key == key)?;
                Some(EngineEvent::ToolResult { id: sub.call.clone(), ok: end == "completed", declined: false, text: sub.report.clone() })
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
                sub.report.clear();
                if let Some(root) = msg["params"]["turn"]["rootTurnId"].as_str() {
                    sub.turn = Some(root.to_string());
                }
            }
            Some("turn/completed") => sub.own = None,
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
                result @ EngineEvent::ToolResult { .. } => Some(result),
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
                return vec![EngineEvent::Break { final_answer: item["phase"] == "final_answer" }];
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
                _ => kind,
            };
            vec![EngineEvent::Tool { id: s(&item["id"]), name: name.into(), input, parent: None }]
        }
        "item/completed" => {
            let item = &p["item"];
            let kind = item["type"].as_str().unwrap_or_default();
            let id = s(&item["id"]);
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
            vec![EngineEvent::Done {
                ok: status == "completed",
                reason: p["turn"]["error"]["message"].as_str().map(String::from).unwrap_or(status),
                cost_total_usd: cost,
                input_tokens: u.input + sub.input,
                output_tokens: u.output + sub.output,
                cache_read_tokens: u.cached + sub.cached,
                cache_write_tokens: u.written.unwrap_or(0) + sub.written.unwrap_or(0),
                cache_write_1h_tokens: 0,
                incomplete: used.is_none() || u.written.is_none() || state.price_incomplete || state.subagents_incomplete,
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
    pub permissions: Permissions,
    pub cli: crate::cli::Cli,
    /// Where the process itself runs; each thread works in its own folder.
    pub cwd: PathBuf,
    pub area: Area,
    pub user_hooks: bool,
    pub windows: bool,
    pub spawn: Spawner,
}

/// The settings that go into the thread's `config` (R140): web search always, and the two Codex-only choices when the person
/// made one. Only the person's own Chat and project sessions get them: an internal question and a role never do.
fn apply_tuning(params: &mut Map<String, Value>, tuning: Tuning) {
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
    match tuning.verbosity {
        Verbosity::Default => {}
        Verbosity::Low => config["model_verbosity"] = "low".into(),
        Verbosity::Medium => config["model_verbosity"] = "medium".into(),
        Verbosity::High => config["model_verbosity"] = "high".into(),
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
    pub permissions: Permissions,
    pub tuning: Tuning,
    /// Faber's helper tool (R161), as for `ClaudeLaunch::helper`.
    pub helper: crate::helper::Helper,
    /// Faber's own browser (R161 step 7), as for `ClaudeLaunch::browse`.
    pub browse: bool,
    /// Faber's conversation tool, as for `ClaudeLaunch::conversations`.
    pub conversations: bool,
    /// Faber's checks, as for `ClaudeLaunch::verify`.
    pub verify: bool,
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
    /// A `turn/steer`; its tag tells the engine which message it was.
    Steer(u64, String),
}

/// A conversation's side of the shared process.
struct Thread {
    early_completion: Vec<Value>,
    sink: EventSink,
    params: CodexThread,
    id: Option<String>,
    turn: Option<String>,
    queued: Vec<UserTurn>,
    state: CodexTurnState,
    approvals: HashMap<String, Value>,
    /// Helper calls (R161) waiting for their report: the tool call id to the id of Codex's request.
    helpers: HashMap<String, Value>,
    watch: proc::TurnWatch,
    blocked: bool,
    closing: bool,
    commands: Vec<tokio::task::JoinHandle<()>>,
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

pub fn thread_params(area: Area, t: &CodexThread) -> (&'static str, Value) {
    let mut params = codex_thread(area, t.model.as_deref(), t.permissions);
    if let Some(instructions) = &t.instructions {
        params.insert("baseInstructions".into(), instructions.clone().into());
        // Legacy read-only still permits disk-wide reads. Internal questions keep the same credential/network
        // boundary as user turns, with their existing working folder read-only and no workspace writes.
        params.remove("sandbox");
        params.insert("config".into(), crate::permissions::sandbox_config(&t.cwd, false).into());
        params["config"]["features.shell_tool"] = false.into();
        params.remove("dynamicTools");
        params.insert("approvalPolicy".into(), "never".into());
        // Nothing to resume later: it stays out of the user's Codex history.
        params.insert("ephemeral".into(), true.into());
    } else {
        // Developer instructions sit beside Codex's own, so a project keeps its default prompt.
        params.insert("developerInstructions".into(), persona(&PersonaInput { name: t.user_name.as_deref() }).into());
        if t.permissions != Permissions::Isolated {
            apply_tuning(&mut params, t.tuning);
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
            crate::helper::Helper::Off => {}
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

/// The instruction files a started role thread declares against the stored receipt. A missing inventory, an
/// unreadable receipt, an added file or a removed one all refuse.
pub(crate) fn project_sources_match(result: &Value) -> Result<(), &'static str> {
    let key = crate::project::policy::Blocker::Restrictions.key();
    let declared: Vec<String> =
        result["instructionSources"].as_array().and_then(|list| list.iter().map(|s| s.as_str().map(String::from)).collect()).ok_or(key)?;
    let recorded = crate::project::qualification::recorded_sources(crate::model::Vendor::Codex).map_err(|_| key)?;
    crate::project::qualification::session_sources(&declared, &recorded).map_err(|_| key)
}

/// Candidate controls, not a claim that native command/read tools are disabled by a sandbox.
pub fn project_thread_params(
    t: &CodexThread,
    role: crate::project::Role,
) -> Result<(&'static str, Value), crate::project::policy::Blocker> {
    use crate::project::policy;
    policy::invocation(crate::model::Vendor::Codex, t.model.as_deref(), t.effort.as_deref(), t.instructions.is_some())?;
    // A role never inherits the person's mode: it always gets Faber's own permission profile (credential folders and
    // temporary folders denied, no network), and only the writer may write in its working folder.
    let isolated = CodexThread { permissions: Permissions::Isolated, tuning: Tuning::default(), ..t.clone() };
    let (method, mut params) = thread_params(Area::Project, &isolated);
    params["approvalPolicy"] = "never".into();
    params["baseInstructions"] = worker_contract(role).into();
    let config = params["config"].as_object_mut().expect("isolated thread config");
    if role != crate::project::Role::Writer {
        for (key, profile) in config.iter_mut() {
            if key.starts_with("permissions.") {
                profile["filesystem"][":workspace_roots"]["."] = "read".into();
            }
        }
    }
    config.insert("model_reasoning_effort".into(), t.effort.clone().into());
    config.insert("web_search".into(), "disabled".into());
    config.insert("service_tier".into(), "default".into());
    config.insert("approvals_reviewer".into(), "user".into());
    let fields = params.as_object_mut().expect("thread parameters");
    // Roles never execute anything: no dynamic tools, and the profile above replaces Codex's own sandbox modes.
    for key in ["developerInstructions", "dynamicTools", "sandbox"] {
        fields.remove(key);
    }
    Ok((method, params))
}

/// What every project role is launched with: the server arguments (the per-process model catalog file by its content) and
/// each role's thread parameters, with fixed stand-ins for the per-turn values (folder, model, effort). A receipt is bound
/// to its hash (`qualification::launch_policy`), so a change to any of them asks for a new qualification run.
pub(crate) fn project_launch_policy(roles: &[crate::project::Role]) -> Value {
    let catalog = project_catalog_path().to_string_lossy().into_owned();
    let server: Vec<String> =
        project_codex_args(cfg!(windows)).into_iter().map(|a| a.replace(&json!(catalog).to_string(), "<catalog>")).collect();
    let thread = CodexThread {
        helper: Default::default(),
        browse: false,
        conversations: false,
        verify: false,
        cwd: PathBuf::from(if cfg!(windows) { r"C:\faber-role" } else { "/faber-role" }),
        model: Some("gpt-6-luna".into()),
        effort: Some("medium".into()),
        resume: None,
        instructions: None,
        user_name: None,
        permissions: Permissions::Isolated,
        tuning: Default::default(),
    };
    // Each thread names its permission profile with a fresh id; the profile's content is what counts.
    let threads: Vec<Value> = roles
        .iter()
        .map(|role| {
            let Ok((_, params)) = project_thread_params(&thread, *role) else { return Value::Null };
            let text = params.to_string();
            let text = match params["config"]["default_permissions"].as_str() {
                Some(profile) => text.replace(profile, "<profile>"),
                None => text,
            };
            serde_json::from_str(&text).unwrap_or(Value::Null)
        })
        .collect();
    json!({"server": server, "catalog": project_codex_catalog(), "threads": threads})
}

/// The thread a conversation opens: a role's fixed policy, or the person's own settings.
fn open_params(area: Area, role: Option<crate::project::Role>, t: &CodexThread) -> (&'static str, Value) {
    match role {
        Some(role) => project_thread_params(t, role).expect("validated role"),
        None => thread_params(area, t),
    }
}

/// One `codex app-server` serves every conversation with the same settings, each as a thread of its own, so open
/// conversations cost one process instead of one each (measured: about 50 MB for three threads).
#[derive(Clone)]
pub struct CodexServer {
    tx: UnboundedSender<ServerCommand>,
    next: Arc<AtomicU64>,
    tree: Option<proc::OwnedTree>,
}

impl CodexServer {
    /// Dedicated role entry point: deny before starting a server, never share an automatic role with Chat threads.
    pub fn start_project(
        opts: CodexServerOptions,
        thread: CodexThread,
        role: crate::project::Role,
        sink: EventSink,
    ) -> Result<Session, crate::project::policy::Blocker> {
        if opts.area != Area::Project {
            return Err(crate::project::policy::Blocker::Invocation);
        }
        let opts = CodexServerOptions { permissions: Permissions::Isolated, ..opts };
        let thread = CodexThread { permissions: Permissions::Isolated, ..thread };
        project_thread_params(&thread, role)?;
        // The pin is held until the started process was checked and resumed.
        let pinned = crate::project::policy::admit_launch(crate::model::Vendor::Codex, &opts.cli.command)?;
        #[cfg(windows)]
        prepare_project_codex_catalog().map_err(|_| crate::project::policy::Blocker::Restrictions)?;
        let server = proc::spawn_expecting(&pinned.sha256, || Self::start_with_policy(opts, Some(role)));
        let session = server.open(thread, sink);
        server.retire();
        Ok(session.for_project(role))
    }
    /// The process starts right away, so a warmed-up conversation is already booting while the user types.
    pub fn start(opts: CodexServerOptions) -> CodexServer {
        Self::start_with_policy(opts, None)
    }

    fn start_with_policy(opts: CodexServerOptions, role: Option<crate::project::Role>) -> CodexServer {
        let (tx, rx) = unbounded_channel();
        let args = if role.is_some() {
            project_codex_args(opts.windows)
        } else {
            codex_args(opts.area, opts.windows, opts.user_hooks, opts.permissions)
        };
        let started = (opts.spawn)(&opts.cli.command, &args, &opts.cwd);
        let tree = started.as_ref().ok().and_then(|p| p.contained.as_ref()).and_then(proc::Containment::owned_tree);
        tokio::spawn(serve(started, opts, rx, role));
        CodexServer { tx, next: Arc::new(AtomicU64::new(1)), tree }
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
        Session::from_fn(move |cmd| drop(server.send(ServerCommand::Session(key, cmd)))).with_tree(self.tree.clone())
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
    role: Option<crate::project::Role>,
) {
    let area = opts.area;
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
    let mut job = p.contained;
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
    let (command_tx, mut command_rx) = unbounded_channel::<(Value, bool, String)>();

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
            proc::stop(&mut *stdin, &mut child, &mut job).await;
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
                let (method, params) = open_params(area, role, &$t.params);
                request!(method, params, Waiting::Thread($key));
            }
        }};
    }

    let init = json!({ "clientInfo": { "name": "faber", "title": "Faber", "version": "0.0.0" }, "capabilities": { "experimentalApi": true, "requestAttestation": false } });
    request!("initialize", init, Waiting::Initialize);

    loop {
        let deadline = threads.values().filter_map(|t| t.watch.deadline()).min();
        tokio::select! {
            Some((id, success, text)) = command_rx.recv() => {
                let _ = proc::write_line(&mut *stdin, &json!({"id":id,"result":{"success":success,"contentItems":[{"type":"inputText","text":text}]}})).await;
            }
            () = proc::wait_deadline(deadline) => {
                let expired: Vec<u64> = threads.iter().filter(|(_, t)| t.watch.deadline().is_some_and(|d| d <= tokio::time::Instant::now())).map(|(&k, _)| k).collect();
                for key in expired {
                    let Some(t) = threads.get_mut(&key) else { continue };
                    if t.watch.stopping() || t.turn.is_none() {
                        // A shared process cannot be killed for one task. Retain its identity until completion/recovery.
                        t.blocked = true;
                        t.watch.finish();
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
                // Parse the identity before any config values; malformed role output never reaches raw diagnostics.
                #[derive(serde::Deserialize)]
                struct Header {
                    id: Option<Value>,
                    #[serde(flatten)]
                    other: HashMap<String, serde::de::IgnoredAny>,
                }
                if let Some(role) = role {
                    let header = serde_json::from_str::<Header>(&line);
                    if line.len() > 4 * 1024 * 1024 || header.is_err() {
                        for t in threads.values() { (t.sink)(EngineEvent::Error {message:crate::project::policy::Blocker::Restrictions.key().into(),fatal:true}); }
                        stop!();
                        continue;
                    }
                    if let Some(suffix) = header.ok().filter(|h| !h.other.contains_key("method")).and_then(|h| h.id).and_then(|id| id.as_str()
                        .and_then(|id| id.strip_prefix("faber-config-")).map(String::from)) {
                        let Ok(key) = suffix.parse::<u64>() else { continue; };
                        if !exposure_pending.remove(&key) { continue; }
                        let Some(t) = threads.get(&key) else { continue; };
                        let (method, mut params) = project_thread_params(&t.params, role).expect("validated role");
                        if let Err(blocker) = thread_exposure(&mut params, &line) {
                            (t.sink)(EngineEvent::Error {message:blocker.key().into(),fatal:true});
                            stop!();
                            continue;
                        }
                        request!(method, params, Waiting::Thread(key));
                        continue;
                    }
                } else if let Ok(reply) = serde_json::from_str::<ConfigReply>(&line)
                    && let Some(key) = reply.id.as_deref().and_then(|id| id.strip_prefix("faber-config-")).and_then(|key| key.parse::<u64>().ok())
                {
                    if !exposure_pending.remove(&key) { continue; }
                    let Some(t) = threads.get(&key) else { continue };
                    let Some(result) = reply.result else {
                        (t.sink)(EngineEvent::Error { message: "error.projectSandbox".into(), fatal: true });
                        continue;
                    };
                    let (method, mut params) = open_params(area, role, &t.params);
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
                if role.is_some() {
                    match crate::project::policy::reject_native(&mut *stdin, crate::model::Vendor::Codex, &msg).await {
                        Ok(true) => continue,
                        Err(_) => {
                            for t in threads.values() { (t.sink)(EngineEvent::Error {message:crate::project::policy::Blocker::Restrictions.key().into(),fatal:true}); }
                            proc::stop(&mut *stdin, &mut child, &mut job).await;
                            break;
                        }
                        Ok(false) => {}
                    }
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
                            };
                            for t in keys.iter().filter_map(|k| threads.get(k)) {
                                (t.sink)(EngineEvent::Error { message: format!("Codex {what} failed: {detail}"), fatal });
                            }
                            if let Waiting::Other(key,"turn/start") = kind && let Some(t) = threads.get_mut(&key) {
                                t.watch.finish();
                                t.turn = None;
                                t.early_completion.clear();
                            }
                            continue;
                        }
                        match kind {
                            Waiting::Initialize => {
                                let _ = proc::write_line(&mut *stdin, &json!({ "method": "initialized" })).await;
                                ready = true;
                                for key in std::mem::take(&mut pending) {
                                    let Some(t) = threads.get(&key) else { continue };
                                    open_thread!(key, t);
                                }
                            }
                            Waiting::Thread(key) => {
                                let Some(t) = threads.get_mut(&key) else { continue };
                                // A role's own inventory of instruction files must be the very set its receipt recorded.
                                if role.is_some() && let Err(key) = project_sources_match(&msg["result"]) {
                                    (t.sink)(EngineEvent::Error { message: key.into(), fatal: true });
                                    t.blocked = true;
                                    t.queued.clear();
                                    continue;
                                }
                                let thread = &msg["result"]["thread"];
                                let id = s(&thread["id"]);
                                let shown = thread["model"].as_str().map(String::from).or_else(|| t.params.model.clone()).unwrap_or_else(|| "codex".into());
                                t.state.model = shown.clone();
                                (t.sink)(EngineEvent::Started { session_id: id.clone(), model: shown });
                                by_id.insert(id.clone(), key);
                                t.id = Some(id.clone());
                                for turn in std::mem::take(&mut t.queued) {
                                    t.watch.begin(role.is_some());
                                    t.state.base = t.state.total;
                                    let params = turn_params(&id, t.params.effort.as_deref(), &turn);
                                    request!("turn/start", params, Waiting::Other(key, "turn/start"));
                                }
                            }
                            Waiting::Other(key, "turn/start") => {
                                if let Some(t) = threads.get_mut(&key) {
                                    t.turn = msg["result"]["turn"]["id"].as_str().map(String::from).or(t.turn.take());
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
                                    t.watch.pause();
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
                                    t.watch.pause();
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
                                    t.watch.pause();
                                    (t.sink)(EngineEvent::MapCall { id: call, input: params["arguments"].clone() });
                                }
                                None => {
                                    let _ = proc::write_line(&mut *stdin, &json!({"id":id,"result":{"success":false,"contentItems":[{"type":"inputText","text":"Faber's project map is not available here."}]}})).await;
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
                                    t.watch.pause();
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
                                    t.watch.pause();
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
                            if let Some(t) = thread && role.is_none() && t.params.permissions == Permissions::Isolated && t.params.instructions.is_none() && params["tool"] == "run_command" {
                                let input = &params["arguments"];
                                if let Some(argv) = crate::permissions::command_argv(&t.params.cwd, "Bash", input, Some(&opts.cli), &crate::cli::find_cli) {
                                    let (spawn, cwd, tx, id) = (opts.spawn.clone(), t.params.cwd.clone(), command_tx.clone(), id.clone());
                                    t.commands.retain(|job| !job.is_finished());
                                    t.commands.push(tokio::spawn(async move {
                                        let result = match spawn(&argv[0], &argv[1..], &cwd) {
                                            Ok(p) => proc::command_output(p).await,
                                            Err(_) => (false, "error.projectSandbox".into()),
                                        };
                                        let _ = tx.send((id, result.0, result.1));
                                    }));
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
                                t.watch.pause();
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
                        // A subagent's own thread speaks for the conversation that started it.
                        if let Some(ChildNews { key, turn, events, used }) = subagents.child(&msg) {
                            if let Some(t) = threads.get_mut(&key) {
                                t.watch.activity();
                                // A subagent belongs to the reply it serves: once that reply is over, what it still does or uses
                                // reaches no other reply.
                                let current = turn.is_some() && turn == t.turn;
                                if let Some(done) = used && current { t.state.add_subagent(&done); }
                                if current && !t.blocked {
                                    for event in events { (t.sink)(event); }
                                }
                            }
                            continue;
                        }
                        let thread_id = params["threadId"].as_str().or_else(|| params["thread"]["id"].as_str());
                        let key = thread_id.and_then(|t| by_id.get(t)).copied();
                        match key.and_then(|k| threads.get_mut(&k).map(|t| (k, t))) {
                            Some((key, t)) => {
                                if method == Some("turn/completed") && params["turn"]["id"].as_str() != t.turn.as_deref() {
                                    if t.turn.is_none() && waiting.values().any(|w| matches!(w,Waiting::Other(key,"turn/start") if Some(key) == thread_id.and_then(|id|by_id.get(id)))) {
                                        if t.early_completion.len() < 2 { t.early_completion.push(msg); }
                                        else {
                                            t.blocked = true;
                                            t.watch.finish();
                                            (t.sink)(EngineEvent::Error { message:"error.cli.stopUnconfirmed".into(),fatal:true });
                                        }
                                    }
                                    continue;
                                }
                                t.watch.activity();
                                if method == Some("turn/started") {
                                    t.turn = params["turn"]["id"].as_str().map(String::from);
                                    if !t.blocked {
                                        (t.sink)(EngineEvent::TurnStarted);
                                    }
                                }
                                if method == Some("turn/completed") {
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
                                }
                                for event in parse_codex_notification(&msg, &mut t.state).into_iter().chain(subagents.track(key, params["turnId"].as_str(), &msg)) {
                                    if !t.blocked { (t.sink)(event); }
                                }
                                if method == Some("turn/completed") && !t.blocked && !t.closing && let Some(id) = t.id.clone() {
                                    for turn in std::mem::take(&mut t.queued) {
                                        t.watch.begin(role.is_some());
                                        t.state.base = t.state.total;
                                        request!("turn/start", turn_params(&id, t.params.effort.as_deref(), &turn), Waiting::Other(*by_id.get(&id).expect("registered thread"), "turn/start"));
                                    }
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
                        let closed: Vec<u64> = threads.iter().filter(|(_, t)| t.closing && t.turn.is_none() && !t.watch.active()).map(|(&k, _)| k).collect();
                        for key in closed {
                            subagents.forget(key);
                            if let Some(t) = threads.remove(&key) && let Some(id) = t.id { by_id.remove(&id); }
                        }
                        if retired && threads.is_empty() { stop!(); }
                    }
                }
            }
            cmd = rx.recv(), if !stopping => match cmd {
                Some(ServerCommand::Open { key, thread, sink }) => {
                    let state = CodexTurnState::new(thread.resume.is_some());
                    let t = Thread { sink, params: thread, id: None, turn: None, queued: vec![], state, approvals: HashMap::new(), helpers: HashMap::new(), watch: proc::TurnWatch::default(), blocked: false, closing: false, early_completion: vec![], commands: vec![] };
                    if ready {
                        open_thread!(key, &t);
                    } else {
                        pending.push(key);
                    }
                    threads.insert(key, t);
                }
                Some(ServerCommand::Session(key, cmd)) => match cmd {
                    SessionCommand::Send(turn) => {
                        let Some(t) = threads.get_mut(&key) else { continue };
                        if t.blocked || t.closing { continue; }
                        if t.watch.stopping() { t.queued.push(turn); continue; }
                        match t.id.clone() {
                            Some(id) => {
                                t.watch.begin(role.is_some());
                                t.state.base = t.state.total;
                                request!("turn/start", turn_params(&id, t.params.effort.as_deref(), &turn), Waiting::Other(key, "turn/start"));
                            }
                            None => t.queued.push(turn),
                        }
                    }
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
                                            if t.approvals.is_empty() { t.watch.resume(); }
                                        }
                                        Delivery::Delivered
                                    }
                                    Err(e) => {
                                        // A half-written or unflushed line cannot be sent again: end the process, and the
                                        // end of its output tells the engine.
                                        if e.torn {
                                            proc::stop(&mut *stdin, &mut child, &mut job).await;
                                        }
                                        Delivery::WriteFailed
                                    }
                                }
                            }
                            Some(_) => Delivery::Unknown,
                        };
                        let _ = ack.send(delivery);
                    }
                    SessionCommand::HelperReply { id, ok, text } => {
                        let Some(t) = threads.get_mut(&key) else { continue };
                        let Some(request) = t.helpers.remove(&id) else { continue };
                        if t.helpers.is_empty() && t.approvals.is_empty() { t.watch.resume(); }
                        let _ = proc::write_line(&mut *stdin, &json!({"id":request,"result":{"success":ok,"contentItems":[{"type":"inputText","text":text}]}})).await;
                    }
                    SessionCommand::Interrupt => {
                        if let Some(t) = threads.get_mut(&key) { for job in t.commands.drain(..) { job.abort(); } }
                        if let Some(t) = threads.get_mut(&key)
                            && !t.blocked && !t.watch.stopping()
                            && let Some((thread, turn)) = t.id.clone().zip(t.turn.clone()) {
                            t.watch.interrupt();
                            request!("turn/interrupt", json!({ "threadId": thread, "turnId": turn }), Waiting::Other(key, "turn/interrupt"));
                        }
                        // Stop reaches the conversation's subagents too; one that just ended answers with an error nobody needs.
                        for (thread, turn) in subagents.at_work(key) {
                            request!("turn/interrupt", json!({ "threadId": thread, "turnId": turn }), Waiting::Other(key, SUBAGENT_STOP));
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
                            for job in t.commands.drain(..) { job.abort(); }
                            if t.watch.active() || t.turn.is_some() {
                                t.closing = true;
                                if !t.blocked && !t.watch.stopping() {
                                    t.watch.interrupt();
                                    if let Some((id, turn)) = t.id.clone().zip(t.turn.clone()) {
                                        request!("turn/interrupt", json!({ "threadId": id, "turnId": turn }), Waiting::Other(key, "turn/interrupt"));
                                    }
                                }
                            } else if let Some(t) = threads.remove(&key) && let Some(id) = t.id {
                                subagents.forget(key);
                                by_id.remove(&id);
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
    for t in threads.values() {
        for job in &t.commands {
            job.abort();
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
mod role_tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn t13_every_role_disables_inherited_mcp_before_start_and_resume() {
        use crate::project::Role;
        for role in [Role::Lead, Role::Research, Role::TestProposal, Role::Writer, Role::Reviewer] {
            for resume in [None, Some("thread".into())] {
                let (stdin, input) = tokio::io::duplex(65536);
                let (mut output, stdout) = tokio::io::duplex(65536);
                let fake = Arc::new(std::sync::Mutex::new(Some(proc::Proc {
                    stdin: Box::new(stdin),
                    stdout: Box::new(stdout),
                    stderr: None,
                    child: None,
                    contained: None,
                })));
                let spawn: Spawner = Arc::new(move |_, _, _| Ok(fake.lock().unwrap().take().unwrap()));
                let server = CodexServer::start_with_policy(
                    CodexServerOptions {
                        permissions: Permissions::Isolated,
                        cli: crate::cli::Cli { command: "fake".into(), prefix: vec![] },
                        cwd: ".".into(),
                        area: Area::Project,
                        user_hooks: false,
                        windows: false,
                        spawn,
                    },
                    Some(role),
                );
                let mut lines = BufReader::new(input).lines();
                lines.next_line().await.unwrap().unwrap();
                if resume.is_some() {
                    output.write_all(b"{\"id\":1,\"result\":{}}\n").await.unwrap();
                    lines.next_line().await.unwrap().unwrap();
                }
                let session = server.open(
                    CodexThread {
                        helper: Default::default(),
                        browse: false,
                        conversations: false,
                        verify: false,
                        cwd: ".".into(),
                        model: Some("gpt-6-luna".into()),
                        effort: Some("low".into()),
                        resume: resume.clone(),
                        instructions: None,
                        user_name: None,
                        permissions: Permissions::Isolated,
                        tuning: Default::default(),
                    },
                    Arc::new(|_| {}),
                );
                if resume.is_none() {
                    output.write_all(b"{\"id\":1,\"result\":{}}\n").await.unwrap();
                    lines.next_line().await.unwrap().unwrap();
                }
                let query: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
                assert_eq!(query["method"], "config/read");
                assert_eq!(query["params"]["includeLayers"], false);
                let fixture: Value =
                    serde_json::from_str(include_str!("../../tests/fixtures/projects/inherited-mcp.jsonl").trim()).unwrap();
                output.write_all(format!("{}\n", fixture["msg"]).as_bytes()).await.unwrap();
                let start: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
                assert_eq!(start["method"], if resume.is_some() { "thread/resume" } else { "thread/start" });
                for name in ["Roblox_Studio", "node_repl", "rea", "ytlab"] {
                    assert_eq!(start["params"]["config"]["mcp_servers"][name], json!({"enabled":false}));
                }
                assert!(!start.to_string().contains("SYNTHETIC"));
                assert_eq!(start["params"]["config"]["web_search"], "disabled");
                assert!(start["params"].get("sandbox").is_none(), "{start}");
                assert_eq!(start["params"]["approvalPolicy"], "never");
                assert_eq!(start["params"]["config"]["approvals_reviewer"], "user");
                session.close();
                server.retire();
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn role_loop_declines_every_native_request_and_disarms_a_failed_start() {
        role_loop_rejects(false, false).await;
    }

    #[tokio::test(start_paused = true)]
    async fn role_loop_stops_an_unknown_permission_request_immediately() {
        role_loop_rejects(true, false).await;
    }

    #[tokio::test(start_paused = true)]
    async fn role_loop_does_not_confuse_an_unknown_request_with_a_configuration_response() {
        role_loop_rejects(true, true).await;
    }

    /// A role thread whose own inventory differs from the receipt (a file added, or no inventory at all) never gets a turn.
    #[tokio::test(start_paused = true)]
    async fn a_role_thread_with_another_instruction_inventory_than_its_receipt_never_starts() {
        for reply in [
            r#"{"id":2,"result":{"instructionSources":["C:/Users/x/.codex/AGENTS.md"],"thread":{"id":"t"}}}"#,
            r#"{"id":2,"result":{"thread":{"id":"t"}}}"#,
            r#"{"id":2,"result":{"instructionSources":[7],"thread":{"id":"t"}}}"#,
        ] {
            *crate::project::qualification::TEST_RECORDED.lock().unwrap() = Some(vec![]);
            let (stdin, input) = tokio::io::duplex(65536);
            let (mut output, stdout) = tokio::io::duplex(65536);
            let fake = Arc::new(std::sync::Mutex::new(Some(proc::Proc {
                stdin: Box::new(stdin),
                stdout: Box::new(stdout),
                stderr: None,
                child: None,
                contained: None,
            })));
            let spawn: Spawner = Arc::new(move |_, _, _| Ok(fake.lock().unwrap().take().unwrap()));
            let server = CodexServer::start_with_policy(
                CodexServerOptions {
                    permissions: Permissions::Isolated,
                    cli: crate::cli::Cli { command: "fake".into(), prefix: vec![] },
                    cwd: ".".into(),
                    area: Area::Project,
                    user_hooks: false,
                    windows: false,
                    spawn,
                },
                Some(crate::project::Role::Reviewer),
            );
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let session = server.open(
                CodexThread {
                    helper: Default::default(),
                    browse: false,
                    conversations: false,
                    verify: false,
                    cwd: ".".into(),
                    model: Some("gpt-6-luna".into()),
                    effort: Some("low".into()),
                    resume: None,
                    instructions: None,
                    user_name: None,
                    permissions: Permissions::Isolated,
                    tuning: Default::default(),
                },
                Arc::new(move |e| drop(tx.send(e))),
            );
            session.send(UserTurn::text("review"));
            let mut lines = BufReader::new(input).lines();
            lines.next_line().await.unwrap().unwrap();
            output.write_all(b"{\"id\":1,\"result\":{}}\n").await.unwrap();
            lines.next_line().await.unwrap().unwrap();
            lines.next_line().await.unwrap().unwrap();
            output.write_all(b"{\"id\":\"faber-config-1\",\"result\":{\"config\":{\"mcp_servers\":{}}}}\n").await.unwrap();
            lines.next_line().await.unwrap().unwrap();
            output.write_all(format!("{reply}\n").as_bytes()).await.unwrap();
            let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv()).await;
            assert!(
                matches!(event, Ok(Some(EngineEvent::Error { fatal: true, ref message })) if message == crate::project::policy::Blocker::Restrictions.key()),
                "{reply}: {event:?}"
            );
            let turn = tokio::time::timeout(std::time::Duration::from_millis(200), lines.next_line()).await;
            assert!(turn.is_err() || !matches!(turn, Ok(Ok(Some(ref l))) if l.contains("turn/start")), "{reply}: a turn was sent");
            session.close();
            server.retire();
        }
    }

    async fn role_loop_rejects(unknown: bool, config_id: bool) {
        *crate::project::qualification::TEST_RECORDED.lock().unwrap() = Some(vec![]);
        let (stdin, input) = tokio::io::duplex(65536);
        let (mut output, stdout) = tokio::io::duplex(65536);
        let fake = Arc::new(std::sync::Mutex::new(Some(proc::Proc {
            stdin: Box::new(stdin),
            stdout: Box::new(stdout),
            stderr: None,
            child: None,
            contained: None,
        })));
        let spawn: Spawner = Arc::new(move |_, _, _| Ok(fake.lock().unwrap().take().unwrap()));
        let server = CodexServer::start_with_policy(
            CodexServerOptions {
                permissions: Permissions::Isolated,
                cli: crate::cli::Cli { command: "fake".into(), prefix: vec![] },
                cwd: ".".into(),
                area: Area::Project,
                user_hooks: false,
                windows: false,
                spawn,
            },
            Some(crate::project::Role::Writer),
        );
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let session = server.open(
            CodexThread {
                helper: Default::default(),
                browse: false,
                conversations: false,
                verify: false,
                cwd: ".".into(),
                model: Some("gpt-6-luna".into()),
                effort: Some("low".into()),
                resume: None,
                instructions: None,
                user_name: None,
                permissions: Permissions::Isolated,
                tuning: Default::default(),
            },
            Arc::new(move |e| drop(tx.send(e))),
        );
        let mut lines = BufReader::new(input).lines();
        lines.next_line().await.unwrap().unwrap();
        output.write_all(b"{\"id\":1,\"result\":{}}\n").await.unwrap();
        lines.next_line().await.unwrap().unwrap();
        // An isolated thread reads the MCP server names first, to switch each one off.
        let read: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(read["method"], "config/read");
        output.write_all(format!("{}\n", json!({"id":read["id"],"result":{"config":{"mcp_servers":{}}}})).as_bytes()).await.unwrap();
        let start: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(start["method"], "thread/start");
        assert!(start["params"].get("dynamicTools").is_none() && start["params"].get("sandbox").is_none(), "{start}");
        output.write_all(b"{\"id\":2,\"result\":{\"instructionSources\":[],\"thread\":{\"id\":\"t\"}}}\n").await.unwrap();
        assert!(matches!(rx.recv().await, Some(EngineEvent::Started { .. })));
        let mut denied = 0;
        for line in include_str!("../../tests/fixtures/projects/policy-escapes.jsonl").lines() {
            let entry: Value = serde_json::from_str(line).unwrap();
            if entry["msg"]["type"] != "control_request" {
                continue;
            }
            let request = json!({"id":entry["msg"]["request_id"],"method":"item/commandExecution/requestApproval","params":{"threadId":"t","input":entry["msg"]["request"]}});
            output.write_all(format!("{request}\n").as_bytes()).await.unwrap();
            let reply: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(reply["id"], request["id"]);
            assert_eq!(reply["result"]["decision"], "decline");
            denied += 1;
        }
        assert_eq!(denied, 9);
        session.send(UserTurn::text("failed start"));
        let start: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        output.write_all(format!("{}\n", json!({"id":start["id"],"error":{"code":-1,"message":"rejected"}})).as_bytes()).await.unwrap();
        assert!(matches!(rx.recv().await, Some(EngineEvent::Error { fatal: false, .. })));
        tokio::time::advance(std::time::Duration::from_secs(130)).await;
        tokio::task::yield_now().await;
        assert!(rx.try_recv().is_err(), "failed start left a deadline");
        session.send(UserTurn::text("next"));
        let next: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(next["method"], "turn/start");
        let line = if unknown {
            include_str!("../../tests/fixtures/projects/unknown-permissions.jsonl").lines().next().unwrap()
        } else {
            include_str!("../../tests/fixtures/projects/native-file-change.jsonl").trim()
        };
        let mut escaped: Value = serde_json::from_str(line).unwrap();
        if config_id {
            escaped["msg"]["id"] = "faber-config-future".into();
        }
        output.write_all(format!("{}\n", escaped["msg"]).as_bytes()).await.unwrap();
        let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv()).await;
        assert!(
            matches!(event, Ok(Some(EngineEvent::Error { fatal: true, ref message })) if message == crate::project::policy::Blocker::Restrictions.key()),
            "completed native file change was accepted: {event:?}"
        );
        session.close();
        server.retire();
    }
}

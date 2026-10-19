//! Claude Code driver: one long-lived `claude -p` process per conversation, spoken to in stream-json over stdio.

mod background;

use crate::events::{Delivery, EngineEvent, Session, SessionCommand, UserTurn};
use crate::model::{Area, Permissions, Tuning, WebSearch};
use crate::persona::{PersonaInput, persona};
use crate::proc::{self, Spawner};
use crate::signals::claude_signals;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};

#[derive(Debug, Clone)]
pub struct ClaudeLaunch {
    pub area: Area,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub resume: Option<String>,
    pub user_hooks: bool,
    pub permissions: Permissions,
    /// A side question (such as routing) with its own instructions: no tools, no MCP servers.
    pub instructions: Option<String>,
    /// How the user wants to be called, told to the model once, when the conversation starts (and again, unchanged, each time
    /// it resumes). Ignored for a side question, which has instructions of its own.
    pub user_name: Option<String>,
    /// What the installed CLI is known to accept (see `ClaudeFlags`); the default passes none of the newer flags.
    pub flags: ClaudeFlags,
    /// Web search from the settings (R140). Chat and the person's own project sessions only: a side question never reads it.
    pub tuning: Tuning,
    /// Faber's helper tool (R161): offered to a project's own agent, never to a side question.
    pub helper: crate::helper::Helper,
    /// Faber's own browser (R161 step 7): offered to a project's own agent in full access, where the shell has one.
    pub browse: bool,
    /// Faber's conversation tool (`crate::sibling`): a project's own agent that no other agent started.
    pub conversations: bool,
    /// Faber's checks (`crate::verify`): a project's agent in full access, where the shell gives a runner.
    pub verify: bool,
    /// The skills installed from Faber's catalog (`crate::tools`), for a project's agent outside Isolated.
    pub tools: crate::tools::Delivery,
}

/// The tools Faber's MCP server holds for a launch: the helper for a project's own agent, the browser where it is offered.
fn mcp_tools(launch: &ClaudeLaunch) -> crate::helper::Tools {
    crate::helper::Tools {
        helper: launch.helper == crate::helper::Helper::Offers,
        browse: launch.browse,
        conversation: launch.conversations,
        verify: launch.verify,
        // The map only reads files: it goes where the checks go.
        map: launch.verify,
    }
}

/// Where the catalog's skills go (`crate::tools`): a project's own agent, in every mode but Isolated, which turns every
/// customization off. Chat keeps its lean tool list, which has no skill tool.
pub fn offers_tools(area: Area, permissions: Permissions) -> bool {
    area == Area::Project && permissions != Permissions::Isolated
}

/// Flags that only newer CLIs take, read once from `claude --help` (`from_help`). A flag the CLI does not list is never
/// passed, so an older CLI starts exactly as before: the default (nothing known) is the safe one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClaudeFlags {
    /// `--autocompact <auto|tokens>`: where the CLI compacts a long conversation (checked on 2.1.296).
    pub autocompact: bool,
    /// `--permission-prompts none`: a request that would prompt is denied at once instead of waiting for an answer (2.1.296).
    pub permission_prompts: bool,
    /// `auto` among the `--permission-mode` choices: the CLI's own reviewer approves the safe actions (R140).
    pub auto_mode: bool,
    /// `--plugin-dir <path>`: a plugin loaded from a folder for this session only (2.1.302), how the catalog's skills arrive.
    pub plugin_dir: bool,
}

impl ClaudeFlags {
    /// Only an option line counts: indented, starting with the flag (after an optional short alias) and followed by its value
    /// placeholder or the gap before its description. The flag named in a sentence, an error or a description is no support.
    pub fn from_help(help: &str) -> Self {
        let declaring = |line: &str, flag: &str| {
            if !line.starts_with([' ', '\t']) {
                return false;
            }
            let mut rest = line.trim_start();
            if rest.starts_with('-') && !rest.starts_with("--") {
                rest = rest.split_once(", ").map_or(rest, |(_, after)| after.trim_start());
            }
            rest.strip_prefix(flag).is_some_and(|tail| {
                tail.is_empty() || tail.starts_with("  ") || tail.starts_with('\t') || tail.trim_start().starts_with(['<', '['])
            })
        };
        let declares = |flag: &str| help.lines().any(|line| declaring(line, flag));
        // The choices of `--permission-mode` are quoted words on the option's own lines, up to the next option.
        let mut block = help.lines().skip_while(|line| !declaring(line, "--permission-mode"));
        let mut modes = block.next().unwrap_or_default().to_string();
        for line in block.take_while(|line| !line.trim_start().starts_with('-')) {
            modes.push(' ');
            modes.push_str(line);
        }
        ClaudeFlags {
            autocompact: declares("--autocompact"),
            permission_prompts: declares("--permission-prompts"),
            auto_mode: modes.contains("\"auto\""),
            plugin_dir: declares("--plugin-dir"),
        }
    }
}

// Chat stays lean: a fixed list of built-in tools. Ask keeps Chat restricted with no MCP servers; Isolated also drops every
// startup customization; full access leaves the person's own integrations on.
const CHAT_TOOLS: &str = "Read,Write,Edit,Glob,Grep,Bash,WebSearch,WebFetch,AskUserQuestion";

/// The empty integration loadout of a side question: no MCP servers, no settings files, no hooks.
fn claude_exposure_args() -> Vec<String> {
    [
        "--strict-mcp-config",
        "--mcp-config",
        "{\"mcpServers\":{}}",
        "--safe-mode",
        "--setting-sources",
        "",
        "--settings",
        "{\"disableAllHooks\":true}",
    ]
    .map(String::from)
    .to_vec()
}

// Deny rules apply in bypassPermissions too; command patterns are not an OS sandbox, so a command can still reach what the
// file tools cannot. Read rules also stop Edit and Write on the same path in recent CLIs; the Edit rules say it for older ones.
// https://code.claude.com/docs/en/permissions#read-and-edit
const CREDENTIAL_DIRS: [&str; 10] = [
    "~/.claude",
    "~/.codex",
    "~/.gemini",
    "~/.ssh",
    "~/.aws",
    "~/.docker",
    "~/.azure",
    "~/.kube",
    "~/.config/gh",
    "~/AppData/Roaming/GitHub CLI",
];
const CREDENTIAL_FILES: [&str; 4] = ["~/.claude.json*", "~/.git-credentials", "~/.npmrc", "//**/*updater.key*"];

/// Every deny rule of the isolated and full-access modes, one string per rule. They travel in the settings JSON, an array
/// that keeps each rule whole (a rule with a space, such as the GitHub CLI folder, cannot be split there). Where a documented
/// variable (CLAUDE_CONFIG_DIR and the like) moved a sign-in folder, the new place is denied as well.
pub fn tool_deny(var: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let mut rules = vec!["Edit(//**/.git/**)".to_string()];
    let moved = crate::permissions::relocated_credentials(var);
    let dirs = CREDENTIAL_DIRS.iter().map(|d| format!("{d}/**"));
    let files = CREDENTIAL_FILES.iter().map(|f| (*f).to_string());
    // A network path is written with two slashes as well: how the CLI spells one in a rule could not be checked.
    let moved = moved.into_iter().flat_map(|(p, file)| {
        let forms = if p.starts_with("//") {
            vec![p.clone(), crate::permissions::claude_rule_path(&p)]
        } else {
            vec![crate::permissions::claude_rule_path(&p)]
        };
        forms.into_iter().map(move |rule| if file { rule } else { format!("{rule}/**") })
    });
    for path in dirs.chain(files).chain(moved) {
        rules.push(format!("Read({path})"));
        rules.push(format!("Edit({path})"));
    }
    rules
}

/// The deny rules the web setting adds (R140): Off drops both web tools; Cached has no equivalent in Claude Code, so it keeps
/// searching and stops fetching pages.
fn web_deny(web: WebSearch) -> Vec<String> {
    match web {
        WebSearch::Live => vec![],
        WebSearch::Cached => vec!["WebFetch".into()],
        WebSearch::Off => vec!["WebSearch".into(), "WebFetch".into()],
    }
}

/// The tools a guarded Chat session may use without a card (R140).
fn web_allowed(web: WebSearch) -> Option<&'static str> {
    match web {
        WebSearch::Live => Some("WebSearch,WebFetch"),
        WebSearch::Cached => Some("WebSearch"),
        WebSearch::Off => None,
    }
}

/// The answer that lets a request through. Leaving plan mode (the plan was approved) also names the mode the session goes on in:
/// the CLI returns to the mode it was in before plan, which for a session that began in plan is `default` (read in the CLI),
/// and naming it keeps the person's own `defaultMode` from ever being the one that takes over.
fn allow_response(tool: &str, input: &Value, reply: Option<&Value>) -> Value {
    let mut response = json!({ "behavior": "allow", "updatedInput": input });
    // A question the agent asked comes back with the user's choices: the question list as it was, plus `answers`.
    if let (true, Some(answers)) = (tool == "AskUserQuestion", reply) {
        response["updatedInput"]["answers"] = answers.clone();
    }
    if tool == "ExitPlanMode" {
        response["updatedPermissions"] = json!([{ "type": "setMode", "mode": "default", "destination": "session" }]);
    }
    response
}

/// How full a conversation gets before Claude Code compacts it. All three models hold 1M and the CLI compacts near 967K,
/// deep into context rot. Haiku thrashes at 100K (the base context eats the room, measured), so it compacts at 150K (R158);
/// Opus and Sonnet are starting points to measure, not proven optima. Unknown models keep the CLI's own.
fn autocompact_window(model: &str) -> Option<String> {
    autocompact_tokens(model).map(|tokens| format!("{}k", tokens / 1000))
}

/// The same point in tokens: the size Faber's context meter counts the conversation against when it passes `--autocompact`.
pub fn autocompact_tokens(model: &str) -> Option<u64> {
    let model = model.to_ascii_lowercase();
    [("haiku", 150_000), ("sonnet", 300_000), ("opus", 400_000)]
        .into_iter()
        .find(|(name, _)| model.contains(name))
        .map(|(_, tokens)| tokens)
}

/// Arguments for one long-lived Claude Code process that takes many turns over stdin.
/// Every guarded mode names its permission mode (Ask: `default`, ask before file changes and commands that are not plain
/// reading; Accept edits: `acceptEdits`; Plan: `plan`; Auto: `auto`, or accept edits when the CLI does not list it), so neither
/// the person's CLI settings nor an earlier session can pick a looser one. Full access ("Do not ask") bypasses approvals, keeps the
/// person's integrations and only denies the credential folders. Isolated disables startup integrations and routes tools to
/// Faber's working-folder policy. A side question (router, pulse) always gets the isolated treatment.
pub fn claude_args(launch: &ClaudeLaunch) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--exclude-dynamic-system-prompt-sections",
    ]
    .map(String::from)
    .to_vec();
    // The host always answers the CLI's permission requests over stdio, Full access included: the engine denies and parks them there, as
    // `--permission-prompts none` did, except a question the agent asks the person (`AskUserQuestion`), a card in every mode.
    args.extend(["--permission-prompt-tool".into(), "stdio".into()]);
    let env = |name: &str| std::env::var(name).ok();
    if let Some(instructions) = &launch.instructions {
        // Whatever mode the person chose, Faber's own requests run without the person's hooks, settings files, MCP servers and
        // plugins: a hook that fired on every background routing turn would run the person's commands unasked.
        for a in ["--system-prompt", instructions, "--tools", "", "--no-session-persistence"] {
            args.push(a.into());
        }
        args.extend(claude_exposure_args());
    } else {
        let mut settings = json!({});
        if launch.area == Area::Chat {
            args.extend(["--tools".into(), CHAT_TOOLS.into()]);
            match launch.permissions {
                mode if mode.guarded() => {
                    args.extend(["--strict-mcp-config".into(), "--restricted".into()]);
                    if let Some(allowed) = web_allowed(launch.tuning.web) {
                        args.extend(["--allowedTools".into(), allowed.into()]);
                    }
                }
                Permissions::Isolated => args.extend(["--strict-mcp-config".into(), "--safe-mode".into()]),
                // The person's own MCP servers, plugins, skills and hooks load as they do in the CLI itself.
                _ => {}
            }
        }
        let web = web_deny(launch.tuning.web);
        let deny = || tool_deny(&env).into_iter().chain(web.iter().cloned()).collect::<Vec<_>>();
        match launch.permissions.for_claude(launch.flags) {
            Permissions::Isolated => {
                if launch.area == Area::Project {
                    args.extend(["--safe-mode".into(), "--strict-mcp-config".into()]);
                }
                // Restricted mode refuses bypassPermissions. Chat disables customizations with safe mode and
                // keeps its lean tools/MCP setup, with the same host policy and sandbox as Projects.
                args.extend(["--setting-sources".into(), "user".into()]);
                args.extend(["--permission-mode".into(), "bypassPermissions".into()]);
                settings["permissions"] = json!({ "blockReadsOutsideWorkingDirectories": true, "deny": tool_deny(&env),
                    "ask": ["Read(//**)", "Edit(//**)", "Bash", "PowerShell", "*"] });
                settings["sandbox"] = json!({ "autoAllowBashIfSandboxed": false });
                settings["env"] = crate::permissions::child_env(std::path::Path::new("."));
            }
            // Full access (R121): no sandbox, no ask rules, network on. Deny rules still apply in bypassPermissions.
            Permissions::FullAccess => {
                args.extend(["--permission-mode".into(), "bypassPermissions".into()]);
                settings["permissions"] = json!({ "deny": deny() });
            }
            // Every guarded mode carries the same credential denies as full access: a person's own rule or an approval card
            // must not open them.
            mode => {
                let name = match mode {
                    Permissions::AcceptEdits => "acceptEdits",
                    Permissions::Plan => "plan",
                    Permissions::Auto => "auto",
                    _ => "default",
                };
                args.extend(["--permission-mode".into(), name.into()]);
                settings["permissions"] = json!({ "deny": deny() });
            }
        }
        let hooks_off = match launch.permissions {
            Permissions::Isolated => true,
            Permissions::FullAccess => false,
            _ => launch.area == Area::Project && !launch.user_hooks,
        };
        if hooks_off {
            settings["disableAllHooks"] = true.into();
        }
        if settings.as_object().is_some_and(|s| !s.is_empty()) {
            args.push("--settings".into());
            args.push(settings.to_string());
        }
    }
    if launch.instructions.is_none() {
        args.push("--append-system-prompt".into());
        args.push(persona(&PersonaInput { name: launch.user_name.as_deref() }));
        // R161: Faber's own MCP server, answered by this driver over stdio, holds the helper tool and the browser.
        if mcp_tools(launch).any() {
            args.extend(["--mcp-config".into(), crate::helper::CLAUDE_MCP_CONFIG.into()]);
        }
        // The catalog's skills, as one plugin from Faber's store, where Faber offers them.
        if let Some(plugin) =
            launch.tools.plugin.as_ref().filter(|_| offers_tools(launch.area, launch.permissions) && launch.flags.plugin_dir)
        {
            args.extend(["--plugin-dir".into(), plugin.to_string_lossy().into_owned()]);
        }
        // The CLI echoes each line it is given when it takes it in, so a line the person wrote while it works is known to be
        // read, and by which turn. A helper keeps no history.
        args.push("--replay-user-messages".into());
        if launch.helper == crate::helper::Helper::Is {
            args.push("--no-session-persistence".into());
        }
    }
    // Side questions keep the CLI default, and an older CLI that does not list the flag would refuse to start with it.
    let compacts = launch.flags.autocompact && launch.instructions.is_none();
    if let Some(window) = compacts.then(|| launch.model.as_deref().and_then(autocompact_window)).flatten() {
        args.extend(["--autocompact".into(), window]);
    }
    for (flag, value) in [("--model", &launch.model), ("--effort", &launch.effort), ("--resume", &launch.resume)] {
        if let Some(v) = value {
            args.push(flag.into());
            args.push(v.clone());
        }
    }
    args
}

fn block_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items.iter().filter_map(|c| c["text"].as_str()).collect(),
        _ => String::new(),
    }
}

/// How a refused approval comes back as the tool's error: the text Faber answers with, and the older wording of the recordings.
const DECLINED: [&str; 2] = ["The user declined.", "The user denied"];

fn s(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

/// Turns one stream-json line into engine events. Claude Code's JSON is loosely typed, so fields are read
/// defensively; lines that carry nothing the app shows give no events.
pub fn parse_claude_message(msg: &Value) -> Vec<EngineEvent> {
    match msg["type"].as_str().unwrap_or_default() {
        "system" if msg["subtype"] == "init" => match (msg["session_id"].as_str(), msg["model"].as_str()) {
            (Some(id), Some(model)) => vec![EngineEvent::Started { session_id: id.into(), model: model.into() }],
            _ => vec![EngineEvent::Error { message: "Claude Code started without a session id.".into(), fatal: false }],
        },
        // A subagent's own words stream too (`parent_tool_use_id` names the agent call that started it): they are its work, never
        // the answer, so only its tool calls are shown, inside its step.
        "stream_event" if !msg["parent_tool_use_id"].is_null() => vec![],
        "stream_event" => {
            let event = &msg["event"];
            if event["type"] == "message_delta" && event["delta"]["stop_reason"] == "end_turn" {
                return vec![EngineEvent::Settled];
            }
            if event["type"] == "content_block_start" && event["content_block"]["type"] == "text" {
                return vec![EngineEvent::Break { final_answer: false }];
            }
            if event["type"] != "content_block_delta" {
                return vec![];
            }
            let delta = &event["delta"];
            match delta["type"].as_str() {
                Some("text_delta") => vec![EngineEvent::Text(s(&delta["text"]))],
                Some("thinking_delta") => vec![EngineEvent::Thinking(s(&delta["thinking"]))],
                _ => vec![],
            }
        }
        // Text already arrived as deltas; only finished tool calls are new here. A subagent's calls name the agent call that
        // started it.
        "assistant" => {
            let parent = msg["parent_tool_use_id"].as_str().map(String::from);
            // What the main thread holds in its context: the prompt of its latest response (a subagent's own never counts). A
            // response with no prompt is a synthetic one, not a reading.
            let context = parent
                .is_none()
                .then(|| {
                    let u = &msg["message"]["usage"];
                    ["input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"]
                        .iter()
                        .map(|k| u[*k].as_u64().unwrap_or(0))
                        .sum::<u64>()
                })
                .filter(|used| *used > 0)
                .map(|used| EngineEvent::Context { used: Some(used), window: None });
            context
                .into_iter()
                .chain(items(&msg["message"]["content"]).filter(|c| c["type"] == "tool_use").map(|c| {
                    let name = match c["name"].as_str() {
                        Some(crate::helper::CLAUDE_NAME) => crate::helper::TOOL.to_string(),
                        Some(crate::browser::CLAUDE_NAME) => crate::browser::TOOL.to_string(),
                        Some(crate::sibling::CLAUDE_NAME) => crate::sibling::TOOL.to_string(),
                        Some(crate::verify::CLAUDE_NAME) => crate::verify::TOOL.to_string(),
                        Some(crate::map::CLAUDE_NAME) => crate::map::TOOL.to_string(),
                        _ => s(&c["name"]),
                    };
                    EngineEvent::Tool { id: s(&c["id"]), name, input: c["input"].clone(), parent: parent.clone() }
                }))
                .collect()
        }
        // The conversation was compacted (by Faber, by the agent or by the CLI's own limit); the summary's size is reported.
        "system" if msg["subtype"] == "compact_boundary" && msg["parent_tool_use_id"].is_null() => {
            vec![EngineEvent::Compacted { after: msg["compact_metadata"]["post_tokens"].as_u64() }]
        }
        "user" => items(&msg["message"]["content"])
            .filter(|c| c["type"] == "tool_result")
            .map(|c| {
                let (ok, text) = (c["is_error"] != true, block_text(&c["content"]));
                EngineEvent::ToolResult {
                    id: s(&c["tool_use_id"]),
                    ok,
                    declined: !ok && DECLINED.iter().any(|d| text.starts_with(d)),
                    text,
                }
            })
            .collect(),
        "system" if msg["subtype"] == "task_notification" && msg["tool_use_id"].is_string() => {
            vec![EngineEvent::HelperDone { id: s(&msg["tool_use_id"]), ok: msg["status"] == "completed", text: s(&msg["summary"]) }]
        }
        "control_request" if msg["request"]["subtype"] == "can_use_tool" => vec![EngineEvent::Permission {
            request_id: s(&msg["request_id"]),
            tool: s(&msg["request"]["tool_name"]),
            input: msg["request"]["input"].clone(),
        }],
        "rate_limit_event" => {
            let windows = msg["rate_limit_info"]["unifiedWindows"].as_object().into_iter().flatten().filter_map(|(window, w)| {
                Some(EngineEvent::Quota { window: window.clone(), used: w["utilization"].as_f64()?, resets_at: w["resetsAt"].as_f64()? })
            });
            windows.chain(Some(claude_signals(msg)).filter(|s| !s.is_empty()).map(EngineEvent::Signals)).collect()
        }
        "result" => result_events(msg),
        _ => vec![],
    }
}

/// A finished turn. `usage` describes the main response only, while `modelUsage` adds the sub-model work of the same turn
/// (web fetches and the like), so a turn whose `modelUsage` prompt is larger is marked incomplete; its cost stays valid.
fn result_events(msg: &Value) -> Vec<EngineEvent> {
    let u = &msg["usage"];
    let n = |k: &str| u[k].as_u64().unwrap_or(0);
    let (fresh, read, write) = (n("input_tokens"), n("cache_read_input_tokens"), n("cache_creation_input_tokens"));
    let prompt = fresh + read + write;
    // `usage` describes the main response only; `modelUsage` adds the sub-model work of the same turn.
    let whole_turn: u64 = msg["modelUsage"]
        .as_object()
        .map(|models| {
            models
                .values()
                .map(|m| {
                    ["inputTokens", "cacheReadInputTokens", "cacheCreationInputTokens"]
                        .iter()
                        .map(|k| m[*k].as_u64().unwrap_or(0))
                        .sum::<u64>()
                })
                .sum()
        })
        .unwrap_or(0);
    // An aborted turn reports a total of 0 and an empty usage: no counters, which is not a cost of 0.
    let total = msg["total_cost_usd"].as_f64();
    let counters = total.is_some_and(|t| t > 0.0) || prompt + n("output_tokens") > 0;
    let warm = cache_warm_secs(u).map(|secs| EngineEvent::CacheWarm { secs, started_ms: None });
    // A request the CLI denied for want of anyone to answer it; each is kept for the person, never allowed on its own.
    let parked = items(&msg["permission_denials"]).map(|d| EngineEvent::Parked {
        tool_use_id: s(&d["tool_use_id"]),
        tool: s(&d["tool_name"]),
        input: d["tool_input"].clone(),
    });
    // The window of the model that did most of the turn's work: the main thread's, not a sub-model's.
    let window = msg["modelUsage"]
        .as_object()
        .into_iter()
        .flat_map(|models| models.values())
        .max_by_key(|m| {
            ["inputTokens", "cacheReadInputTokens", "cacheCreationInputTokens"].iter().map(|k| m[*k].as_u64().unwrap_or(0)).sum::<u64>()
        })
        .and_then(|m| m["contextWindow"].as_u64())
        .map(|window| EngineEvent::Context { used: None, window: Some(window) });
    window
        .into_iter()
        .chain(warm)
        .chain(parked)
        .chain([EngineEvent::Done {
            ok: msg["subtype"] == "success" && msg["is_error"] != true,
            reason: msg["errors"]
                .as_array()
                .filter(|errors| !errors.is_empty())
                .map(|errors| errors.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "))
                .unwrap_or_else(|| msg["terminal_reason"].as_str().or(msg["subtype"].as_str()).unwrap_or_default().into()),
            cost_total_usd: total.filter(|_| counters),
            input_tokens: prompt,
            output_tokens: n("output_tokens"),
            cache_read_tokens: read,
            cache_write_tokens: write,
            cache_write_1h_tokens: u["cache_creation"]["ephemeral_1h_input_tokens"].as_u64().unwrap_or(0),
            incomplete: whole_turn > prompt,
        }])
        .collect()
}

/// How long this turn's cache writes stay warm: an hour when the 1-hour bucket was written (a subscription writes every entry
/// that way), else five minutes when only the 5-minute bucket was. None when neither is named (an older CLI, or a turn that
/// wrote nothing), which the screen shows as nothing. Mixed buckets count as the long one: the main conversation wrote it.
pub fn cache_warm_secs(usage: &Value) -> Option<u64> {
    let bucket = |k: &str| usage["cache_creation"][k].as_u64().unwrap_or(0);
    if bucket("ephemeral_1h_input_tokens") > 0 {
        Some(3600)
    } else if bucket("ephemeral_5m_input_tokens") > 0 {
        Some(300)
    } else {
        None
    }
}
fn items(v: &Value) -> impl Iterator<Item = &Value> {
    v.as_array().into_iter().flatten()
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

pub type EventSink = Arc<dyn Fn(EngineEvent) + Send + Sync>;

pub struct ClaudeOptions {
    pub bin: String,
    pub cwd: PathBuf,
    pub launch: ClaudeLaunch,
    pub spawn: Spawner,
    pub sandbox: Option<crate::cli::Cli>,
}

pub fn start_claude(opts: ClaudeOptions, on_event: EventSink) -> Session {
    let args = claude_args(&opts.launch);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    // The process starts right away, so a warmed-up session is already booting while the user types.
    // A denial in the result only means "nobody could answer" when the CLI ran with no prompts; with the host answering, a denial is
    // the person's own refusal (or the engine's), which is no request to keep.
    let parking = args.windows(2).any(|a| a == ["--permission-prompts", "none"]);
    let started = (opts.spawn)(&opts.bin, &args, &opts.cwd);
    tokio::spawn(async move {
        let mut p = match started {
            Ok(p) => p,
            Err(err) => return on_event(EngineEvent::Error { message: format!("Claude Code could not start: {err}"), fatal: true }),
        };
        let tail = proc::stderr_tail(p.stderr.take());
        let mut lines = BufReader::new(p.stdout).lines();
        let mut stdin = p.stdin;
        let mut child = p.child;
        let mut job = p.contained;
        let mut pending: HashMap<String, (String, Value)> = HashMap::new();
        // Helper calls waiting for their report: tool call id to the control request and the JSON-RPC id that carried it.
        let mut helpers: HashMap<String, (String, Value)> = HashMap::new();
        let mut closed = false;
        let mut control_id = 0u32;
        let mut watch = proc::TurnWatch::default();
        let mut queued: Vec<UserTurn> = vec![];
        // Lines not yet echoed back, in order, with their words: the tag of one the person wrote while a turn ran, none for a
        // turn's own. An echo is matched by its words, so a recording made before the echo existed reads the same.
        let replay = opts.launch.instructions.is_none();
        let mut unread: VecDeque<(Option<String>, String)> = VecDeque::new();
        // When the latest request of the running turn began: the cache lifetime counts from it, not from the result.
        let mut request_began: Option<u64> = None;
        // When the running turn was handed to the CLI: its request goes out at that moment. Set once per turn and never moved by
        // the lines that follow (status, retries), so later requests of a tool turn count from the first: never more warmth than there is.
        let mut turn_sent_ms = unix_ms();
        let mut background = background::Background::default();
        loop {
            let deadline = watch.deadline();
            tokio::select! {
                () = proc::wait_deadline(deadline) => {
                    if watch.stopping() {
                        proc::stop(&mut *stdin, &mut child, &mut job).await;
                        on_event(EngineEvent::Error { message: "error.cli.interruptTimeout".into(), fatal: true });
                        break;
                    }
                    watch.interrupt();
                    control_id += 1;
                    let msg = json!({ "type": "control_request", "request_id": format!("faber-{control_id}"), "request": { "subtype": "interrupt" } });
                    let _ = proc::write_line(&mut *stdin, &msg).await;
                }
                line = lines.next_line() => {
                    let Ok(Some(line)) = line else {
                        if !closed {
                            let code = proc::exit_code(&mut child).await;
                            let tail = tail.lock().unwrap().clone();
                            on_event(EngineEvent::Error { message: format!("Claude Code stopped (exit {code}). {tail}").trim().into(), fatal: true });
                        }
                        break;
                    };
                    if line.trim().is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<Value>(&line) {
                        Ok(msg) => {
                            if msg["type"] == "stream_event" && msg["event"]["type"] == "message_start" && msg["parent_tool_use_id"].is_null() {
                                request_began = Some(turn_sent_ms);
                            }
                            let finished = background.ends_reply(&msg, watch.stopping());
                            if finished { watch.finish(); }
                            if replay && msg["type"] == "user" && msg["isReplay"] == true {
                                let said = words(&msg["message"]["content"]);
                                if let Some((Some(tag), _)) = unread.iter().position(|(_, w)| *w == said).and_then(|at| unread.remove(at)) {
                                    on_event(EngineEvent::Injected { tag: Some(tag) });
                                }
                                continue;
                            }
                            // R161: Faber's MCP server. Only a session that offers one of its tools has it; the CLI waits for each answer.
                            if msg["type"] == "control_request" && msg["request"]["subtype"] == "mcp_message" {
                                let tools = mcp_tools(&opts.launch);
                                let offered = tools.any() && msg["request"]["server_name"] == "faber";
                                let step = if offered { crate::helper::mcp(&msg["request"]["message"], tools) } else {
                                    crate::helper::Mcp::Answer(json!({"jsonrpc":"2.0","id":msg["request"]["message"]["id"],"error":{"code":-32601,"message":"No such server"}}))
                                };
                                match step {
                                    crate::helper::Mcp::Answer(answer) => {
                                        let reply = json!({ "type": "control_response", "response": { "subtype": "success", "request_id": msg["request_id"],
                                            "response": { "mcp_response": answer } } });
                                        let _ = proc::write_line(&mut *stdin, &reply).await;
                                    }
                                    crate::helper::Mcp::Call { tool, id, rpc, input } => {
                                        // Both answers come back as a `HelperReply` for the call's id.
                                        helpers.insert(id.clone(), (s(&msg["request_id"]), rpc));
                                        on_event(match tool {
                                            crate::browser::NAME => EngineEvent::BrowseCall { id, input },
                                            crate::sibling::NAME => EngineEvent::ConversationCall { id, input },
                                            crate::verify::NAME => EngineEvent::VerifyCall { id, input },
                                            crate::map::NAME => EngineEvent::MapCall { id, input },
                                            _ => EngineEvent::HelperCall { id, input },
                                        });
                                    }
                                }
                                continue;
                            }
                            for mut event in parse_claude_message(&msg) {
                                if matches!(event, EngineEvent::Settled) && background.owes() {
                                    continue;
                                }
                                if matches!(event, EngineEvent::Done { .. }) {
                                    if !finished {
                                        background.hold(event);
                                        continue;
                                    }
                                    event = background.close(event);
                                }
                                if matches!(event, EngineEvent::Parked { .. }) && !parking {
                                    continue;
                                }
                                if let EngineEvent::CacheWarm { secs, .. } = event {
                                    // A turn whose request start was never seen has no trustworthy expiry: nothing is shown.
                                    let Some(began) = request_began.take() else { continue };
                                    event = EngineEvent::CacheWarm { secs, started_ms: Some(began) };
                                }
                                if let EngineEvent::Permission { request_id, tool, input } = &event {
                                    // Calling the helper changes nothing by itself: what the helper does is held to the same mode. The
                                    // browser is offered only in full access, which asks nothing, and the engine checks every address.
                                    if (tool == crate::helper::CLAUDE_NAME && opts.launch.helper == crate::helper::Helper::Offers)
                                        || (tool == crate::browser::CLAUDE_NAME && opts.launch.browse)
                                        || (tool == crate::sibling::CLAUDE_NAME && opts.launch.conversations)
                                        || ((tool == crate::verify::CLAUDE_NAME || tool == crate::map::CLAUDE_NAME) && opts.launch.verify)
                                    {
                                        let msg = json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id,
                                            "response": { "behavior": "allow", "updatedInput": input } } });
                                        let _ = proc::write_line(&mut *stdin, &msg).await;
                                        continue;
                                    }
                                    // Only the isolated mode answers for itself. Full access hands the request to the engine, which
                                    // decides with the mode in force at that moment (see Core::handle), so a switch to Ask is never raced.
                                    if opts.launch.permissions == Permissions::Isolated {
                                        let resolve = |name: &str| crate::cli::Cli::find_in(name, &std::env::var("PATH").unwrap_or_default(), cfg!(windows),
                                            &|key| std::env::var(key).ok(), &|path| std::path::Path::new(path).is_file(),
                                            &|path| std::fs::read_to_string(path).unwrap_or_default());
                                        let response = if let Some(updated) = crate::permissions::checked_input(&opts.cwd, tool, input, opts.sandbox.as_ref(), &resolve) {
                                            json!({ "behavior": "allow", "updatedInput": updated })
                                        } else {
                                            json!({ "behavior": "deny", "message": "This action is outside the working-folder policy, or its native sandbox runner is unavailable. Use sandboxed tools; never retry unsandboxed." })
                                        };
                                        let msg = json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id,
                                            "response": response } });
                                        if proc::write_line(&mut *stdin, &msg).await.is_err() {
                                            on_event(EngineEvent::Error { message: "error.permission.undelivered".into(), fatal: true });
                                            proc::stop(&mut *stdin, &mut child, &mut job).await;
                                            return;
                                        }
                                        continue;
                                    }
                                    pending.insert(request_id.clone(), (tool.clone(), input.clone()));
                                }
                                on_event(event);
                            }
                            if closed && !watch.active() {
                                proc::stop(&mut *stdin, &mut child, &mut job).await;
                                break;
                            }
                            if finished {
                                // A turn's own line is echoed before its result. A line the person wrote that is still unread is
                                // a turn of its own, and it starts now, after a stop too (fixtures `steer*`, `helper-told-later`).
                                unread.retain(|(tag, _)| tag.is_some());
                                if !unread.is_empty() { watch.begin(); }
                                for turn in std::mem::take(&mut queued) {
                                    watch.begin();
                                    turn_sent_ms = unix_ms();
                                    if replay { unread.push_back((None, turn.text.clone())); }
                                    let _ = proc::write_line(&mut *stdin, &user_message(&turn)).await;
                                }
                            }
                        }
                        Err(_) => on_event(EngineEvent::Error { message: format!("Unexpected output from Claude Code: {}", line.chars().take(200).collect::<String>()), fatal: false }),
                    }
                }
                cmd = rx.recv(), if !closed => match cmd {
                    Some(SessionCommand::Send(turn)) => {
                        if watch.stopping() { queued.push(turn); continue; }
                        watch.begin();
                        request_began = None;
                        turn_sent_ms = unix_ms();
                        if replay { unread.push_back((None, turn.text.clone())); }
                        let _ = proc::write_line(&mut *stdin, &user_message(&turn)).await;
                    }
                    // The CLI's own command, as a line of its own: it compacts and answers with a result like any turn.
                    Some(SessionCommand::Compact) => {
                        if watch.stopping() { continue; }
                        let turn = UserTurn::text("/compact");
                        watch.begin();
                        request_began = None;
                        turn_sent_ms = unix_ms();
                        if replay { unread.push_back((None, turn.text.clone())); }
                        let _ = proc::write_line(&mut *stdin, &user_message(&turn)).await;
                    }
                    // A line written mid-turn is folded into a turn that uses tools at its next step, and becomes a turn of its
                    // own after a plain reply or a stop (fixtures `steer*`, `queued*`, `helper-told*`). Its echo says which turn
                    // took it in.
                    Some(SessionCommand::Steer { turn, tag }) => {
                        if replay && watch.active() && !watch.stopping() && proc::write_line(&mut *stdin, &user_message(&turn)).await.is_ok() {
                            unread.push_back((Some(tag), turn.text.clone()));
                        } else {
                            on_event(EngineEvent::SteerFailed { tag });
                        }
                    }
                    Some(SessionCommand::Answer { request_id, allow, reply, ack }) => {
                        // The request stays pending until the write worked, so a failed write can be answered again.
                        let delivery = match pending.get(&request_id) {
                            None => Delivery::Unknown,
                            Some((tool, input)) => {
                                let response = if allow { allow_response(tool, input, reply.as_ref()) } else { json!({ "behavior": "deny", "message": DECLINED[0] }) };
                                let msg = json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id, "response": response } });
                                match proc::write_line(&mut *stdin, &msg).await {
                                    Ok(()) => {
                                        pending.remove(&request_id);
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
                        };
                        let _ = ack.send(delivery);
                    }
                    Some(SessionCommand::HelperReply { id, ok, text, pictures }) => {
                        let Some((request_id, rpc)) = helpers.remove(&id) else { continue };
                        let msg = json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id,
                            "response": { "mcp_response": crate::helper::mcp_result(&rpc, ok, &text, &pictures) } } });
                        let _ = proc::write_line(&mut *stdin, &msg).await;
                    }
                    Some(SessionCommand::Interrupt) => {
                        if !watch.active() || watch.stopping() { continue; }
                        watch.interrupt();
                        control_id += 1;
                        let msg = json!({ "type": "control_request", "request_id": format!("faber-{control_id}"), "request": { "subtype": "interrupt" } });
                        let _ = proc::write_line(&mut *stdin, &msg).await;
                    }
                    Some(SessionCommand::Close) | None => {
                        closed = true;
                        if watch.active() {
                            if !watch.stopping() {
                                watch.interrupt();
                                control_id += 1;
                                let msg = json!({ "type": "control_request", "request_id": format!("faber-{control_id}"), "request": { "subtype": "interrupt" } });
                                let _ = proc::write_line(&mut *stdin, &msg).await;
                            }
                        } else {
                            proc::stop(&mut *stdin, &mut child, &mut job).await;
                            break;
                        }
                    }
                }
            }
        }
    });
    Session::new(tx)
}

/// The words of a user line's content, plain or in blocks: what an echo is matched by.
fn words(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks.iter().filter_map(|b| b["text"].as_str()).collect(),
        _ => String::new(),
    }
}

fn user_message(turn: &UserTurn) -> Value {
    let content = if turn.images.is_empty() {
        Value::String(turn.text.clone())
    } else {
        let mut blocks: Vec<Value> = turn
            .images
            .iter()
            .map(|i| json!({ "type": "image", "source": { "type": "base64", "media_type": i.media_type, "data": i.base64 } }))
            .collect();
        blocks.push(json!({ "type": "text", "text": turn.text }));
        Value::Array(blocks)
    };
    json!({ "type": "user", "message": { "role": "user", "content": content }, "parent_tool_use_id": null, "session_id": "" })
}

#[cfg(test)]
mod allow_tests {
    use super::*;

    #[test]
    fn the_main_threads_end_turn_settles_the_reply_before_the_result_comes() {
        let delta = |parent: Value, reason: Value| {
            parse_claude_message(&json!({ "type": "stream_event", "parent_tool_use_id": parent,
                "event": { "type": "message_delta", "delta": { "stop_reason": reason } } }))
        };
        assert!(matches!(delta(Value::Null, json!("end_turn")).as_slice(), [EngineEvent::Settled]));
        assert!(delta(Value::Null, json!("tool_use")).is_empty(), "a message that calls a tool is not the last one");
        assert!(delta(json!("agent1"), json!("end_turn")).is_empty(), "a subagent ending says nothing of the main thread");
    }

    #[test]
    fn a_subagents_tool_calls_name_the_agent_call_and_its_words_never_join_the_answer() {
        let call = |parent: Value| {
            parse_claude_message(&json!({ "type": "assistant", "parent_tool_use_id": parent,
                "message": { "content": [{ "type": "tool_use", "id": "t2", "name": "Grep", "input": { "pattern": "x" } }] } }))
        };
        let parent_of = |events: Vec<EngineEvent>| match events.as_slice() {
            [EngineEvent::Tool { parent, .. }] => parent.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(parent_of(call(json!("agent1"))), Some("agent1".into()));
        assert_eq!(parent_of(call(Value::Null)), None);
        let words = |parent: Value| {
            parse_claude_message(&json!({ "type": "stream_event", "parent_tool_use_id": parent,
                "event": { "type": "content_block_delta", "delta": { "type": "text_delta", "text": "inner" } } }))
        };
        assert!(words(json!("agent1")).is_empty(), "a subagent's text is not the answer");
        assert!(matches!(words(Value::Null).as_slice(), [EngineEvent::Text(t)] if t == "inner"));
        let done = parse_claude_message(&json!({ "type": "system", "subtype": "task_notification", "tool_use_id": "agent1",
            "status": "failed", "summary": "Gave up" }));
        assert!(matches!(done.as_slice(), [EngineEvent::HelperDone { id, ok: false, text }] if id == "agent1" && text == "Gave up"));
    }

    #[test]
    fn a_question_the_person_answered_goes_back_with_the_questions_and_the_answers() {
        let input = json!({ "questions": [{ "question": "Which stack?", "header": "Stack", "multiSelect": false, "options": [{ "label": "Rust", "description": "" }] }] });
        let answers = json!({ "Which stack?": "Rust" });
        let response = allow_response("AskUserQuestion", &input, Some(&answers));
        assert_eq!(response["behavior"], "allow");
        assert_eq!(response["updatedInput"]["questions"], input["questions"]);
        assert_eq!(response["updatedInput"]["answers"], answers);
        assert_eq!(
            allow_response("Bash", &input, Some(&answers)),
            json!({ "behavior": "allow", "updatedInput": input }),
            "only a question carries answers"
        );
    }

    #[test]
    fn r140_leaving_plan_names_the_mode_default_for_the_session_and_no_other_request_names_one() {
        let input = json!({ "plan": "do it" });
        let leaving = allow_response("ExitPlanMode", &input, None);
        assert_eq!(leaving["behavior"], "allow");
        assert_eq!(leaving["updatedInput"], input);
        assert_eq!(leaving["updatedPermissions"], json!([{ "type": "setMode", "mode": "default", "destination": "session" }]));
        for tool in ["Write", "Bash", "Edit", "WebFetch"] {
            let plain = allow_response(tool, &input, None);
            assert_eq!(plain, json!({ "behavior": "allow", "updatedInput": input }), "{tool}");
        }
    }
}

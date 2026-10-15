//! Claude Code driver: one long-lived `claude -p` process per conversation, spoken to in stream-json over stdio.

use crate::events::{Delivery, EngineEvent, Session, SessionCommand, UserTurn};
use crate::model::{Area, Permissions, name_instructions};
use crate::proc::{self, Spawner};
use crate::signals::claude_signals;
use serde_json::{Value, json};
use std::collections::HashMap;
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
}

/// Flags that only newer CLIs take, read once from `claude --help` (`from_help`). A flag the CLI does not list is never
/// passed, so an older CLI starts exactly as before: the default (nothing known) is the safe one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClaudeFlags {
    /// `--autocompact <auto|tokens>`: where the CLI compacts a long conversation (checked on 2.1.296).
    pub autocompact: bool,
    /// `--permission-prompts none`: a request that would prompt is denied at once instead of waiting for an answer (2.1.296).
    pub permission_prompts: bool,
}

impl ClaudeFlags {
    pub fn from_help(help: &str) -> Self {
        let lists = |flag: &str| help.split(|c: char| c.is_whitespace() || c == ',' || c == '<').any(|word| word == flag);
        ClaudeFlags { autocompact: lists("--autocompact"), permission_prompts: lists("--permission-prompts") }
    }
}

/// Where Haiku conversations are compacted: under the 100K step above which its price rises fivefold. 100k is the lowest the
/// CLI accepts.
const HAIKU_AUTOCOMPACT: &str = "100k";

// Chat stays lean: a fixed list of built-in tools. Ask keeps Chat restricted with no MCP servers; Isolated also drops every
// startup customization; full access leaves the person's own integrations on.
const CHAT_TOOLS: &str = "Read,Write,Edit,Glob,Grep,Bash,WebSearch,WebFetch";

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

/// Arguments for one long-lived Claude Code process that takes many turns over stdin.
/// Ask names its permission mode (`default`: ask before file changes and commands that are not plain reading), so neither the
/// person's CLI settings nor an earlier session can pick a looser one. Full access ("Do not ask") bypasses approvals, keeps the
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
    // Full access on a CLI that has the flag runs with nobody to answer: a request that would prompt is denied at once, comes
    // back as a denial in the turn's result and is parked for the person (`EngineEvent::Parked`). Everything else (Ask, the
    // isolated mode, a CLI without the flag) keeps the host answering over stdio.
    let no_prompts = launch.instructions.is_none() && launch.permissions == Permissions::FullAccess && launch.flags.permission_prompts;
    if no_prompts {
        args.extend(["--permission-prompts".into(), "none".into()]);
    } else {
        args.extend(["--permission-prompt-tool".into(), "stdio".into()]);
    }
    let env = |name: &str| std::env::var(name).ok();
    if let Some(instructions) = &launch.instructions {
        // Whatever mode the person chose, Faber's own requests run without the person's hooks, settings files, MCP servers and
        // plugins: a hook that fired on every background routing turn would run the person's commands unasked.
        for a in [
            "--system-prompt",
            instructions,
            "--tools",
            "",
            "--strict-mcp-config",
            "--safe-mode",
            "--setting-sources",
            "",
            "--settings",
            r#"{"disableAllHooks":true}"#,
            "--no-session-persistence",
        ] {
            args.push(a.into());
        }
    } else {
        let mut settings = json!({});
        if launch.area == Area::Chat {
            args.extend(["--tools".into(), CHAT_TOOLS.into()]);
            match launch.permissions {
                Permissions::Ask => {
                    args.extend(["--strict-mcp-config".into(), "--restricted".into(), "--allowedTools".into(), "WebSearch,WebFetch".into()])
                }
                Permissions::Isolated => args.extend(["--strict-mcp-config".into(), "--safe-mode".into()]),
                // The person's own MCP servers, plugins, skills and hooks load as they do in the CLI itself.
                Permissions::FullAccess => {}
            }
        }
        match launch.permissions {
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
                settings["permissions"] = json!({ "deny": tool_deny(&env) });
            }
            // Ask carries the same credential denies as full access: a person's own rule or an approval card must not open them.
            Permissions::Ask => {
                args.extend(["--permission-mode".into(), "default".into()]);
                settings["permissions"] = json!({ "deny": tool_deny(&env) });
            }
        }
        let hooks_off = match launch.permissions {
            Permissions::Isolated => true,
            Permissions::FullAccess => false,
            Permissions::Ask => launch.area == Area::Project && !launch.user_hooks,
        };
        if hooks_off {
            settings["disableAllHooks"] = true.into();
        }
        if settings.as_object().is_some_and(|s| !s.is_empty()) {
            args.push("--settings".into());
            args.push(settings.to_string());
        }
    }
    if let (None, Some(name)) = (&launch.instructions, &launch.user_name) {
        args.push("--append-system-prompt".into());
        args.push(name_instructions(name));
    }
    for (flag, value) in [("--model", &launch.model), ("--effort", &launch.effort), ("--resume", &launch.resume)] {
        if let Some(v) = value {
            args.push(flag.into());
            args.push(v.clone());
        }
    }
    // Faber's own Haiku workers and roles (the isolated mode and side questions) compact early; every other model, and every
    // conversation the person chose a model for, keeps the CLI's own default.
    let internal = launch.instructions.is_some() || launch.permissions == Permissions::Isolated;
    if launch.flags.autocompact && internal && launch.model.as_deref().is_some_and(|m| m.to_ascii_lowercase().contains("haiku")) {
        args.extend(["--autocompact".into(), HAIKU_AUTOCOMPACT.into()]);
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
        "stream_event" => {
            let event = &msg["event"];
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
        // Text already arrived as deltas; only finished tool calls are new here.
        "assistant" => items(&msg["message"]["content"])
            .filter(|c| c["type"] == "tool_use")
            .map(|c| EngineEvent::Tool { id: s(&c["id"]), name: s(&c["name"]), input: c["input"].clone() })
            .collect(),
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
    let warm = cache_warm_secs(u).map(|secs| EngineEvent::CacheWarm { secs });
    // A request the CLI denied for want of anyone to answer it; each is kept for the person, never allowed on its own.
    let parked = items(&msg["permission_denials"]).map(|d| EngineEvent::Parked {
        tool_use_id: s(&d["tool_use_id"]),
        tool: s(&d["tool_name"]),
        input: d["tool_input"].clone(),
    });
    warm.into_iter()
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

pub type EventSink = Arc<dyn Fn(EngineEvent) + Send + Sync>;

pub struct ClaudeOptions {
    pub bin: String,
    pub cwd: PathBuf,
    pub launch: ClaudeLaunch,
    pub spawn: Spawner,
    pub sandbox: Option<crate::cli::Cli>,
}

pub fn start_claude(opts: ClaudeOptions, on_event: EventSink) -> Session {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    // The process starts right away, so a warmed-up session is already booting while the user types.
    let args = claude_args(&opts.launch);
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
        let mut pending: HashMap<String, Value> = HashMap::new();
        let mut closed = false;
        let mut control_id = 0u32;
        let mut quota_seen = false;
        loop {
            tokio::select! {
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
                            quota_seen |= msg["type"] == "rate_limit_event";
                            let finished = msg["type"] == "result";
                            for event in parse_claude_message(&msg) {
                                if matches!(event, EngineEvent::Parked { .. }) && !parking {
                                    continue;
                                }
                                if let EngineEvent::Permission { request_id, tool, input } = &event {
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
                                    pending.insert(request_id.clone(), input.clone());
                                }
                                on_event(event);
                            }
                            if finished && !quota_seen {
                                on_event(EngineEvent::Signals(crate::signals::Signals { billing: Some(crate::budget::Billing::Unknown), ..Default::default() }));
                            }
                        }
                        Err(_) => on_event(EngineEvent::Error { message: format!("Unexpected output from Claude Code: {}", line.chars().take(200).collect::<String>()), fatal: false }),
                    }
                }
                cmd = rx.recv(), if !closed => match cmd {
                    Some(SessionCommand::Send(turn)) => {
                        quota_seen = false;
                        let _ = proc::write_line(&mut *stdin, &user_message(&turn)).await;
                    }
                    // A line written mid-turn is folded into a turn that uses tools (one result for both) and becomes a turn of
                    // its own after a plain reply (see tests/fixtures/claude/queued*.jsonl). Faber cannot tell which ahead of
                    // time, so it holds the message itself and never steers this CLI; if asked anyway, it was not delivered.
                    Some(SessionCommand::Steer { tag, .. }) => on_event(EngineEvent::SteerFailed { tag }),
                    Some(SessionCommand::Answer { request_id, allow, ack }) => {
                        // The request stays pending until the write worked, so a failed write can be answered again.
                        let delivery = match pending.get(&request_id) {
                            None => Delivery::Unknown,
                            Some(input) => {
                                let response = if allow { json!({ "behavior": "allow", "updatedInput": input }) } else { json!({ "behavior": "deny", "message": DECLINED[0] }) };
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
                    Some(SessionCommand::Interrupt) => {
                        control_id += 1;
                        let msg = json!({ "type": "control_request", "request_id": format!("faber-{control_id}"), "request": { "subtype": "interrupt" } });
                        let _ = proc::write_line(&mut *stdin, &msg).await;
                    }
                    Some(SessionCommand::Close) | None => {
                        closed = true;
                        proc::stop(&mut *stdin, &mut child, &mut job).await;
                    }
                }
            }
        }
    });
    Session::new(tx)
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

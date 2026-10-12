//! Claude Code driver: one long-lived `claude -p` process per conversation, spoken to in stream-json over stdio.

use crate::events::{Delivery, EngineEvent, Session, SessionCommand, UserTurn};
use crate::model::{Area, name_instructions};
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
    /// A side question (such as routing) with its own instructions: no tools, no MCP servers.
    pub instructions: Option<String>,
    /// How the user wants to be called, told to the model once, when the conversation starts (and again, unchanged, each time
    /// it resumes). Ignored for a side question, which has instructions of its own.
    pub user_name: Option<String>,
}

// Chat stays lean (about 3k tokens of setup instead of the user's full configuration) and runs in --restricted mode
// inside its own workspace folder: file tools cannot leave it, edits there are accepted, and any command asks first.
const CHAT_TOOLS: &str = "Read,Write,Edit,Glob,Grep,Bash,WebSearch,WebFetch";

/// Arguments for one long-lived Claude Code process that takes many turns over stdin.
/// Projects get the user's own setup (skills, MCP servers, plugins), like a direct session; the user's hooks stay
/// off unless they turn them on, because hooks can rewrite answers. Permission prompts come back to Faber over stdio.
pub fn claude_args(launch: &ClaudeLaunch) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--permission-prompt-tool",
        "stdio",
        "--exclude-dynamic-system-prompt-sections",
    ]
    .map(String::from)
    .to_vec();
    if let Some(instructions) = &launch.instructions {
        for a in ["--system-prompt", instructions, "--tools", "", "--strict-mcp-config", "--no-session-persistence"] {
            args.push(a.into());
        }
    } else if launch.area == Area::Chat {
        // Searching and reading pages never asks, like claude.ai.
        for a in [
            "--restricted",
            "--strict-mcp-config",
            "--tools",
            CHAT_TOOLS,
            "--permission-mode",
            "acceptEdits",
            "--allowedTools",
            "WebSearch,WebFetch",
        ] {
            args.push(a.into());
        }
    } else if !launch.user_hooks {
        args.push("--settings".into());
        args.push(json!({ "disableAllHooks": true }).to_string());
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
    args
}

/// Candidate controls, for inspection only. Help output does not prove enforcement; use the gated constructor.
pub fn project_claude_args(launch: &ClaudeLaunch, role: crate::project::Role) -> Result<Vec<String>, crate::project::policy::Blocker> {
    use crate::project::policy;
    policy::invocation(
        crate::model::Vendor::Claude,
        launch.model.as_deref(),
        launch.effort.as_deref(),
        launch.instructions.is_some() || launch.area != Area::Project,
    )?;
    let mut restricted = launch.clone();
    restricted.instructions = Some(policy::instructions(role).into());
    restricted.user_name = None;
    let mut args = claude_args(&restricted);
    // Unlike side questions, mission sessions may resume with the same fixed policy.
    args.retain(|a| a != "--no-session-persistence");
    for a in [
        "--permission-mode",
        "dontAsk",
        "--disable-slash-commands",
        "--disallowedTools",
        "Bash,Write,Edit,Read,Glob,Grep,Agent,Task,WebSearch,WebFetch,Bash(git push *),Bash(git reset *),Bash(git update-ref *)",
        "--settings",
        "{\"disableAllHooks\":true}",
    ] {
        args.push(a.into());
    }
    Ok(args)
}

/// Automatic missions must never fall back to the legacy project session or its user's configuration.
pub fn start_project_claude(
    opts: ClaudeOptions,
    role: crate::project::Role,
    on_event: EventSink,
) -> Result<Session, crate::project::policy::Blocker> {
    project_claude_args(&opts.launch, role)?;
    crate::project::policy::admit(crate::model::Vendor::Claude)?;
    // Admission stays closed until this launch uses a verified owned-tree spawner and capability handshake.
    let _ = on_event;
    Err(crate::project::policy::Blocker::ProcessTree)
}

fn block_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items.iter().filter_map(|c| c["text"].as_str()).collect(),
        _ => String::new(),
    }
}

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
                return vec![EngineEvent::Break];
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
            .map(|c| EngineEvent::ToolResult { id: s(&c["tool_use_id"]), ok: c["is_error"] != true, text: block_text(&c["content"]) })
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
    vec![EngineEvent::Done {
        ok: msg["subtype"] == "success" && msg["is_error"] != true,
        reason: msg["terminal_reason"].as_str().or(msg["subtype"].as_str()).unwrap_or_default().into(),
        cost_total_usd: total.filter(|_| counters),
        input_tokens: prompt,
        output_tokens: n("output_tokens"),
        cache_read_tokens: read,
        cache_write_tokens: write,
        cache_write_1h_tokens: u["cache_creation"]["ephemeral_1h_input_tokens"].as_u64().unwrap_or(0),
        incomplete: whole_turn > prompt,
    }]
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
}

pub fn start_claude(opts: ClaudeOptions, on_event: EventSink) -> Session {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    // The process starts right away, so a warmed-up session is already booting while the user types.
    let started = (opts.spawn)(&opts.bin, &claude_args(&opts.launch), &opts.cwd);
    tokio::spawn(async move {
        let mut p = match started {
            Ok(p) => p,
            Err(err) => return on_event(EngineEvent::Error { message: format!("Claude Code could not start: {err}"), fatal: true }),
        };
        let tail = proc::stderr_tail(p.stderr.take());
        let mut lines = BufReader::new(p.stdout).lines();
        let mut stdin = p.stdin;
        let mut child = p.child;
        let mut pending: HashMap<String, Value> = HashMap::new();
        let mut closed = false;
        let mut control_id = 0u32;
        let mut watch = proc::TurnWatch::default();
        loop {
            let deadline = watch.deadline();
            tokio::select! {
                () = proc::wait_deadline(deadline) => {
                    if watch.stopping() {
                        proc::stop(&mut *stdin, &mut child).await;
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
                            watch.activity();
                            if msg["type"] == "result" { watch.finish(); }
                            for event in parse_claude_message(&msg) {
                                if let EngineEvent::Permission { request_id, input, .. } = &event {
                                    pending.insert(request_id.clone(), input.clone());
                                    watch.pause();
                                }
                                on_event(event);
                            }
                            if closed && !watch.active() {
                                proc::stop(&mut *stdin, &mut child).await;
                                break;
                            }
                        }
                        Err(_) => on_event(EngineEvent::Error { message: format!("Unexpected output from Claude Code: {}", line.chars().take(200).collect::<String>()), fatal: false }),
                    }
                }
                cmd = rx.recv(), if !closed => match cmd {
                    Some(SessionCommand::Send(turn)) => {
                        if watch.stopping() { continue; }
                        watch.begin();
                        let _ = proc::write_line(&mut *stdin, &user_message(&turn)).await;
                    }
                    Some(SessionCommand::Answer { request_id, allow, ack }) => {
                        // The request stays pending until the write worked, so a failed write can be answered again.
                        let delivery = match pending.get(&request_id) {
                            None => Delivery::Unknown,
                            Some(input) => {
                                let response = if allow { json!({ "behavior": "allow", "updatedInput": input }) } else { json!({ "behavior": "deny", "message": "The user declined." }) };
                                let msg = json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id, "response": response } });
                                match proc::write_line(&mut *stdin, &msg).await {
                                    Ok(()) => {
                                        pending.remove(&request_id);
                                        if pending.is_empty() { watch.resume(); }
                                        Delivery::Delivered
                                    }
                                    Err(e) => {
                                        // A half-written or unflushed line cannot be sent again: end the process, and the
                                        // end of its output tells the engine.
                                        if e.torn {
                                            proc::stop(&mut *stdin, &mut child).await;
                                        }
                                        Delivery::WriteFailed
                                    }
                                }
                            }
                        };
                        let _ = ack.send(delivery);
                    }
                    Some(SessionCommand::Interrupt) => {
                        if watch.stopping() { continue; }
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
                            proc::stop(&mut *stdin, &mut child).await;
                            break;
                        }
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

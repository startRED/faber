//! `faber mcp`: Faber as an MCP server on stdio, so Claude Code, Codex or any MCP client can use it as tools
//! (`claude mcp add faber-app -- faber mcp`). Its tools are a few short ones over the same command table, and `faber_call`
//! for every other command.

use crate::args::Args;
use crate::connect;
use crate::out::{DONE, UNREACHED};
use crate::thread::{self, Thread};
use faber_channel::Client;
use faber_channel::wire;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

/// How long `faber_chat` waits for an answer before it hands back the conversation to follow later.
const CHAT_WAIT: Duration = Duration::from_secs(600);

/// The MCP version this server speaks when the client names none it knows.
const MCP_VERSION: &str = "2025-06-18";

fn tools() -> Value {
    let text = |description: &str| json!({ "type": "string", "description": description });
    json!([
        {
            "name": "faber_status",
            "description": "Faber's state: its version, whether each AI CLI (Claude Code, Codex) is installed and signed in, quota windows, what is running and waiting for the person.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "faber_list",
            "description": "Faber's conversations, newest first: id, title, area (chat or project), project id, last update.",
            "inputSchema": { "type": "object", "properties": {
                "projectId": text("Only this project's conversations."),
                "archived": { "type": "boolean", "description": "Archived conversations instead of the others." },
                "limit": { "type": "integer", "description": "At most this many (default 20)." }
            } }
        },
        {
            "name": "faber_show",
            "description": "One conversation: its messages as text, each step of the AI on one line.",
            "inputSchema": { "type": "object", "properties": { "id": text("The conversation id.") }, "required": ["id"] }
        },
        {
            "name": "faber_chat",
            "description": "Sends a message in Faber (a new chat, or an existing conversation) and waits for the answer. If the AI asks the person something, returns the waiting card instead: only the person answers it, in Faber. Call faber_chat with the same id and no text to keep following once they have.",
            "inputSchema": { "type": "object", "properties": {
                "text": text("The message. Empty to follow the turn already running in `id`."),
                "id": text("The conversation to continue; none starts a new chat."),
                "projectId": text("Start the new conversation in this project."),
                "vendor": text("claude, codex or auto (default)."),
                "model": text("A model of that vendor."),
                "effort": text("low, medium or high.")
            } }
        },
        {
            "name": "faber_commands",
            "description": "Every command Faber's screen can call, with its arguments and kind (read, write, startsTurn, window).",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "faber_call",
            "description": "Calls any command of Faber's table by its name with its arguments (camelCase, as faber_commands lists them), and returns its answer.",
            "inputSchema": { "type": "object", "properties": {
                "command": text("The command name, e.g. state, open, rename."),
                "args": { "type": "object", "description": "The command's arguments." }
            }, "required": ["command"] }
        }
    ])
}

pub async fn run(args: &Args) -> i32 {
    let client = match connect::connect(args, "faber mcp").await {
        Ok(client) => Arc::new(client),
        Err(why) => {
            eprintln!("{}", why.key());
            return UNREACHED;
        }
    };
    // One subscription feeds every faber_chat; each takes the events of its own conversation.
    let events = match client.subscribe().await {
        Ok(events) => Arc::new(tokio::sync::Mutex::new(events)),
        Err(_) => return UNREACHED,
    };
    let (tx, mut rx) = mpsc::channel::<String>(64);
    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(line) = rx.recv().await {
            if stdout.write_all(line.as_bytes()).await.is_err() || stdout.write_all(b"\n").await.is_err() {
                break;
            }
            let _ = stdout.flush().await;
        }
    });
    let mut reader = BufReader::new(tokio::io::stdin());
    let mut buffer = Vec::new();
    let mut running = tokio::task::JoinSet::new();
    // Past this many requests in flight, stdin waits: a stalled stdout cannot pile up tasks without end.
    let in_flight = Arc::new(tokio::sync::Semaphore::new(faber_channel::server::MAX_IN_FLIGHT));
    while let Ok(Some(line)) = wire::read_line(&mut reader, &mut buffer).await {
        // Answers already written leave the set, so a long session keeps only what is in flight.
        while running.try_join_next().is_some() {}
        let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
        let Some(id) = message.get("id").cloned() else { continue };
        let method = message["method"].as_str().unwrap_or("").to_string();
        let params = message["params"].clone();
        let Ok(place) = in_flight.clone().acquire_owned().await else { break };
        let (client, events, tx) = (client.clone(), events.clone(), tx.clone());
        running.spawn(async move {
            let answer = match method.as_str() {
                "initialize" => Ok(json!({
                    "protocolVersion": params["protocolVersion"].as_str().unwrap_or(MCP_VERSION),
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "faber", "version": env!("CARGO_PKG_VERSION") }
                })),
                "ping" => Ok(json!({})),
                "tools/list" => Ok(json!({ "tools": tools() })),
                "tools/call" => Ok(call(&client, &events, &params["name"], &params["arguments"]).await),
                _ => Err(json!({ "code": -32601, "message": "Faber has no such method" })),
            };
            let line = match answer {
                Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
                Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
            };
            let _ = tx.send(line.to_string()).await;
            drop(place);
        });
    }
    while running.join_next().await.is_some() {}
    drop(tx);
    let _ = writer.await;
    DONE
}

fn text_result(value: &Value, error: bool) -> Value {
    let text = match value {
        Value::String(text) => text.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    };
    json!({ "content": [{ "type": "text", "text": text }], "isError": error })
}

type Events = Arc<tokio::sync::Mutex<mpsc::Receiver<Value>>>;

async fn ask(client: &Client, cmd: &str, args: Value) -> Result<Value, Value> {
    match client.call(cmd, args).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(key)) => Err(json!({ "error": key })),
        Err(_) => Err(json!({ "error": "cli.error.channel" })),
    }
}

async fn call(client: &Client, events: &Events, name: &Value, args: &Value) -> Value {
    let done = match name.as_str().unwrap_or("") {
        "faber_status" => status(client).await,
        "faber_list" => list(client, args).await,
        "faber_show" => show(client, args).await,
        "faber_chat" => chat(client, events, args).await,
        "faber_commands" => Ok(client.offer["commands"].clone()),
        "faber_call" => {
            let command = args["command"].as_str().unwrap_or("");
            ask(client, command, args.get("args").cloned().unwrap_or(Value::Null)).await
        }
        other => Err(json!({ "error": "cli.error.verb", "tool": other })),
    };
    match done {
        Ok(value) => text_result(&value, false),
        Err(error) => text_result(&error, true),
    }
}

fn inner(state: Value) -> Value {
    match state.get("state") {
        Some(inner) => inner.clone(),
        None => state,
    }
}

async fn status(client: &Client) -> Result<Value, Value> {
    let state = inner(ask(client, "state", Value::Null).await?);
    let mut accounts = Vec::new();
    for vendor in ["claude", "codex"] {
        accounts.push(
            ask(client, "setup_check", json!({ "vendor": vendor }))
                .await
                .unwrap_or_else(|e| json!({ "vendor": vendor, "error": e["error"] })),
        );
    }
    Ok(json!({
        "version": client.offer["version"],
        "accounts": accounts,
        "quotas": state["quotas"],
        "running": state["running"]["items"],
        "conversations": state["conversations"].as_array().map_or(0, Vec::len),
        "projects": state["projects"],
    }))
}

async fn list(client: &Client, args: &Value) -> Result<Value, Value> {
    let state = inner(ask(client, "state", Value::Null).await?);
    let archived = args["archived"].as_bool().unwrap_or(false);
    let limit = args["limit"].as_u64().unwrap_or(20) as usize;
    let mut rows: Vec<Value> = state["conversations"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|c| c["archived"].as_bool().unwrap_or(false) == archived)
        .filter(|c| args["projectId"].as_str().is_none_or(|p| c["projectId"].as_str() == Some(p)))
        .collect();
    rows.sort_by_key(|c| std::cmp::Reverse(c["updatedAt"].as_u64().unwrap_or(0)));
    rows.truncate(limit);
    Ok(Value::Array(rows))
}

/// A conversation as text: the whole one would be megabytes of JSON for a model to read.
fn as_text(conversation: &Value) -> String {
    let mut text = format!("# {}\n", conversation["title"].as_str().unwrap_or(""));
    for message in conversation["messages"].as_array().into_iter().flatten() {
        let role = message["role"].as_str().unwrap_or("?");
        text.push_str(&format!("\n## {role}\n"));
        for tool in message["tools"].as_array().into_iter().flatten() {
            text.push_str(&format!("- {} ({:?})\n", thread::step_line(tool), thread::step_state(tool)));
        }
        text.push_str(message["text"].as_str().unwrap_or(""));
        text.push('\n');
    }
    text
}

async fn show(client: &Client, args: &Value) -> Result<Value, Value> {
    let conversation = ask(client, "open", json!({ "id": args["id"] })).await?;
    if conversation.is_null() {
        return Err(json!({ "error": "cli.error.noConversation" }));
    }
    Ok(Value::String(as_text(&conversation)))
}

async fn chat(client: &Client, events: &Events, args: &Value) -> Result<Value, Value> {
    // One chat waits at a time: they share the event stream.
    let mut events = events.lock().await;
    while events.try_recv().is_ok() {}
    let text = args["text"].as_str().unwrap_or("").trim().to_string();
    let id = args["id"].as_str().map(str::to_string);
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    let conversation = if text.is_empty() {
        id.ok_or_else(|| json!({ "error": "cli.error.usage" }))?
    } else {
        let state = inner(ask(client, "state", Value::Null).await?);
        let known = id.as_deref().and_then(|id| state["conversations"].as_array()?.iter().find(|c| c["id"].as_str() == Some(id)).cloned());
        let project = args["projectId"].as_str().map(str::to_string).or_else(|| known.as_ref()?["projectId"].as_str().map(str::to_string));
        let area = known.as_ref().and_then(|c| c["area"].as_str()).unwrap_or(if project.is_some() { "project" } else { "chat" });
        let mut choice = json!({ "vendor": args["vendor"].as_str().unwrap_or("auto") });
        for key in ["model", "effort"] {
            if let Some(value) = args[key].as_str() {
                choice[key] = json!(value);
            }
        }
        let req = json!({ "conversationId": id, "area": area, "projectId": project, "text": text, "choice": choice });
        let sent = ask(client, "send", json!({ "req": req })).await?;
        sent.as_str().map(str::to_string).or(id).unwrap_or_default()
    };
    let mut thread = Thread::default();
    let since = if text.is_empty() { 0 } else { since };
    // A turn that already ended or already waits, before or while this call began, is answered from what Faber holds.
    if let Some(done) = look(client, &conversation, &mut thread, since).await? {
        return Ok(done);
    }
    let deadline = tokio::time::Instant::now() + CHAT_WAIT;
    loop {
        let event = match tokio::time::timeout_at(deadline, events.recv()).await {
            Ok(Some(event)) => event,
            Ok(None) => return Err(json!({ "error": "cli.error.gone" })),
            Err(_) => {
                return Ok(
                    json!({ "conversationId": conversation, "status": "streaming", "note": "Still working: call faber_chat with this id and no text to keep following." }),
                );
            }
        };
        match event["type"].as_str() {
            Some("message") if event["conversationId"].as_str() == Some(&conversation) => {
                let whole = match thread.take(event["message"].clone()) {
                    Ok(whole) => whole.clone(),
                    Err(thread::Unknown) => {
                        thread = Thread::from_conversation(&ask(client, "open", json!({ "id": conversation })).await?);
                        continue;
                    }
                };
                if let Some(done) = finished(&conversation, &whole, since) {
                    return Ok(done);
                }
            }
            Some("running") => {
                if let Some(waiting) = needs_you(&conversation, event["items"].as_array().map_or(&[][..], Vec::as_slice)) {
                    return Ok(waiting);
                }
            }
            // Events were dropped while this call was slow: the end of the turn or a card may be among them.
            Some("lagged") => {
                if let Some(done) = look(client, &conversation, &mut thread, since).await? {
                    return Ok(done);
                }
            }
            _ => {}
        }
    }
}

/// Reads the conversation and what runs again; the answer when the turn already ended or waits for the person.
async fn look(client: &Client, conversation: &str, thread: &mut Thread, since: u64) -> Result<Option<Value>, Value> {
    *thread = Thread::from_conversation(&ask(client, "open", json!({ "id": conversation })).await?);
    if let Some(done) = thread.last().and_then(|last| finished(conversation, last, since)) {
        return Ok(Some(done));
    }
    let state = inner(ask(client, "state", Value::Null).await?);
    Ok(needs_you(conversation, state["running"]["items"].as_array().map_or(&[][..], Vec::as_slice)))
}

/// The result of an answer that ended since `since`.
fn finished(conversation: &str, whole: &Value, since: u64) -> Option<Value> {
    let fresh = whole["at"].as_u64().unwrap_or(0) + 2000 >= since;
    (whole["role"] == "assistant" && fresh && whole["status"].as_str().is_some_and(|s| s != "streaming")).then(|| {
        json!({
            "conversationId": conversation,
            "status": whole["status"],
            "error": whole["error"],
            "text": whole["text"],
            "steps": whole["tools"].as_array().into_iter().flatten().map(thread::step_line).collect::<Vec<_>>(),
            "vendor": whole["vendor"],
            "model": whole["model"],
        })
    })
}

/// The cards of this conversation waiting for the person, among what runs.
fn needs_you(conversation: &str, items: &[Value]) -> Option<Value> {
    let item = items
        .iter()
        .find(|i| i["conversationId"].as_str() == Some(conversation) && i["permissions"].as_array().is_some_and(|p| !p.is_empty()))?;
    Some(json!({ "conversationId": conversation, "status": "needsYou", "turnId": item["turnId"], "cards": item["permissions"] }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_name_a_description_and_an_object_schema() {
        for tool in tools().as_array().unwrap() {
            assert!(tool["name"].as_str().unwrap().starts_with("faber_"));
            assert!(!tool["description"].as_str().unwrap().is_empty());
            assert_eq!(tool["inputSchema"]["type"], "object");
        }
    }

    #[test]
    fn a_conversation_reads_as_text_with_one_line_per_step() {
        let text = as_text(&json!({ "title": "T", "messages": [
            { "role": "user", "text": "hi" },
            { "role": "assistant", "text": "done", "tools": [{ "name": "Read", "input": { "file_path": "a.rs" }, "ok": true }] }
        ] }));
        assert!(text.contains("# T") && text.contains("- Read a.rs (Done)") && text.ends_with("done\n"), "{text}");
    }
}

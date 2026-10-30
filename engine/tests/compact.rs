//! The context meter and the agent compacting itself (conversation tool: `context`, `compact`). Claude Code is driven by hand
//! (`pipes`), Codex by a small app-server that speaks the notifications recorded from Codex 0.162.1 and Claude Code 2.1.302.

use crate::support::{self, Pipe, next_pipe, pipes};
use faber_engine::cli::Cli;
use faber_engine::model::{Area, ContextFill, Conversation, Role, RunState, Settings, Status, Vendor};
use faber_engine::proc::{Proc, Spawner};
use faber_engine::router::{Choice, Effort};
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest, WarmRequest};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::UnboundedReceiver;

// The hand-driven Claude project conversation of keep_going.rs, written again here so the two lanes touch different files.
fn new_engine(dir: &crate::support::DataFolder, spawn: faber_engine::proc::Spawner) -> (Engine, UnboundedReceiver<AppEvent>) {
    new_engine_with(dir, spawn, Duration::from_secs(120), 3)
}

fn new_engine_with(
    dir: &crate::support::DataFolder,
    spawn: faber_engine::proc::Spawner,
    idle: Duration,
    max_warm: usize,
) -> (Engine, UnboundedReceiver<AppEvent>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let opts = EngineOptions {
        data_dir: dir.path().into(),
        #[cfg(windows)]
        chat_dir: dir.chat_path().into(),
        clis: BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]),
        spawn,
        windows: false,
        claude_env_billed: false,
        max_warm,
        idle,
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e)))).unwrap();
    // Faber goes on by itself only on a known subscription.
    let plan = faber_engine::signals::Signals { billing: Some(faber_engine::budget::Billing::Subscription), ..Default::default() };
    engine.note_account_events(Vendor::Claude, vec![faber_engine::events::EngineEvent::Signals(plan)]);
    (engine, rx)
}

async fn started(dir: &crate::support::DataFolder, folder: &Path) -> (Engine, UnboundedReceiver<AppEvent>, Pipe, String) {
    let (engine, rx, pipe, id, _made) = started_with(dir, folder, Duration::from_secs(120), 3).await;
    (engine, rx, pipe, id)
}

type Made = Arc<Mutex<Vec<Pipe>>>;

async fn started_with(
    dir: &crate::support::DataFolder,
    folder: &Path,
    idle: Duration,
    max_warm: usize,
) -> (Engine, UnboundedReceiver<AppEvent>, Pipe, String, Made) {
    let (spawn, made) = pipes();
    let (engine, rx) = new_engine_with(dir, spawn, idle, max_warm);
    let project = engine.add_project(&folder.to_string_lossy()).unwrap();
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
    let request = SendRequest {
        conversation_id: None,
        area: Area::Project,
        project_id: Some(project.id),
        text: "Fix the login page, then the rest of the list".into(),
        images: vec![],
        files: vec![],
        choice,
    };
    let id = engine.send(request).unwrap();
    let mut pipe = next_pipe(&made).await;
    // A real CLI says nothing before it has the turn; a line before it would be read as the end of an earlier turn.
    until_turns(&pipe, 1).await;
    pipe.say(json!({ "type": "system", "subtype": "init", "session_id": "s", "model": "claude-haiku-5-5" })).await;
    (engine, rx, pipe, id, made)
}

async fn answers(pipe: &mut Pipe, text: &str, ok: bool) {
    if ok {
        let block = |event: Value| json!({ "type": "stream_event", "event": event, "parent_tool_use_id": null });
        pipe.say(block(json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "text", "text": "" } }))).await;
        pipe.say(block(json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": text } }))).await;
        pipe.say(json!({ "type": "assistant", "message": { "role": "assistant", "content": [{ "type": "text", "text": text }] } })).await;
    }
    pipe.say(json!({ "type": "result", "subtype": if ok { "success" } else { "error_during_execution" }, "is_error": !ok,
        "total_cost_usd": 0.0, "usage": {} }))
        .await;
}

fn turns(pipe: &Pipe) -> Vec<String> {
    pipe.lines.lock().unwrap().iter().filter(|m| m["type"] == "user").map(|m| text_of(&m["message"]["content"])).collect()
}

fn text_of(content: &Value) -> String {
    match content {
        Value::String(s) => crate::support::person_string(s),
        Value::Array(parts) => parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join(""),
        _ => String::new(),
    }
}

async fn until_turns(pipe: &Pipe, n: usize) {
    support::until(&format!("the CLI never got {n} turns"), || turns(pipe).len() >= n).await;
}

async fn until(engine: &Engine, id: &str, done: impl Fn(&Conversation) -> bool) -> Conversation {
    support::until("the conversation never got there", || done(&engine.open(id).unwrap())).await;
    engine.open(id).unwrap()
}

fn idle(c: &Conversation) -> bool {
    c.messages.last().is_some_and(|m| m.role == Role::Assistant && m.status != Some(Status::Streaming))
}

/// The main thread's response, with the prompt it was given.
fn response(parent: Value, fresh: u64, read: u64, wrote: u64) -> Value {
    json!({ "type": "assistant", "parent_tool_use_id": parent, "message": { "role": "assistant", "content": [{ "type": "text", "text": "ok" }],
        "usage": { "input_tokens": fresh, "cache_read_input_tokens": read, "cache_creation_input_tokens": wrote, "output_tokens": 4 } } })
}

/// The end of a turn, naming the model's window as the CLI does.
fn result(window: u64) -> Value {
    json!({ "type": "result", "subtype": "success", "is_error": false, "total_cost_usd": 0.0, "usage": {},
        "modelUsage": { "claude-haiku-5-5": { "inputTokens": 2, "cacheReadInputTokens": 9000, "cacheCreationInputTokens": 100, "contextWindow": window },
                        "claude-other": { "inputTokens": 1, "contextWindow": 200_000 } } })
}

fn context_events(rx: &mut UnboundedReceiver<AppEvent>) -> Vec<Option<ContextFill>> {
    let mut seen = vec![];
    while let Ok(e) = rx.try_recv() {
        if let AppEvent::Context { context, .. } = e {
            seen.push(context);
        }
    }
    seen
}

/// The agent's call of Faber's conversation tool, as Claude Code sends it, and the text Faber answers with.
async fn tool(agent: &mut Pipe, n: u64, args: Value) -> (bool, String) {
    let request = format!("r{n}");
    let message = json!({ "jsonrpc": "2.0", "id": n, "method": "tools/call",
        "params": { "name": "conversation", "arguments": args, "_meta": { "claudecode/toolUseId": format!("toolu_{n}") } } });
    agent.say(json!({ "type": "assistant", "parent_tool_use_id": null, "message": { "content": [{ "type": "tool_use", "id": format!("toolu_{n}"),
        "name": "mcp__faber__conversation", "input": args }] } })).await;
    agent.say(json!({ "type": "control_request", "request_id": request, "request": { "subtype": "mcp_message", "server_name": "faber", "message": message } })).await;
    let mut found = None;
    support::until(&format!("{request} was never answered"), || {
        found = agent.lines.lock().unwrap().iter().find(|m| m["response"]["request_id"] == request.as_str()).cloned();
        found.is_some()
    })
    .await;
    let m = found.unwrap();
    let r = &m["response"]["response"]["mcp_response"]["result"];
    (r["isError"] == false, r["content"][0]["text"].as_str().unwrap_or_default().to_string())
}

/// The line Faber sends to compact: the CLI's command with what the summary must keep.
fn compact_line() -> String {
    format!("/compact {}", faber_engine::claude::COMPACT_FOCUS)
}

/// What Claude Code says when the conversation was compacted on request (`/compact`), recorded from 2.1.302.
async fn compacts(agent: &mut Pipe, after: u64) {
    agent.say(json!({ "type": "system", "subtype": "status", "status": "compacting" })).await;
    agent.say(json!({ "type": "system", "subtype": "init", "session_id": "s", "model": "claude-haiku-5-5" })).await;
    agent.say(json!({ "type": "system", "subtype": "compact_boundary", "compact_metadata": { "trigger": "manual", "pre_tokens": 23212, "post_tokens": after } })).await;
    agent
        .say(json!({ "type": "result", "subtype": "success", "is_error": false, "num_turns": 0, "result": "", "local_command": "compact",
        "total_cost_usd": 0.0, "usage": { "input_tokens": 0 } }))
        .await;
}

#[tokio::test]
async fn the_meter_counts_the_main_agents_latest_prompt_against_its_models_window() {
    let (dir, project) = (crate::support::DataFolder::new(), crate::support::work_folder());
    let (engine, mut rx, mut agent, id) = started(&dir, project.path()).await;
    // Nothing is known before the CLI says it.
    assert_eq!(engine.open(&id).unwrap().context, None);
    agent.say(response(Value::Null, 2, 10_000, 5_000)).await;
    // A subagent's own prompt is never the conversation's.
    agent.say(response(json!("agent1"), 900_000, 0, 0)).await;
    agent.say(result(1_000_000)).await;
    let c = until(&engine, &id, |c| idle(c) && c.context.is_some()).await;
    assert_eq!(
        c.context,
        Some(ContextFill { used: 15_002, window: 1_000_000 }),
        "the main thread's prompt, against the main model's window"
    );
    let seen = context_events(&mut rx);
    assert_eq!(seen.last(), Some(&c.context), "the screen is told: {seen:?}");
    assert!(!seen.iter().flatten().any(|c| c.used == 900_000), "{seen:?}");

    // It was kept with the conversation, so a reopened one shows it before anything runs.
    // The agent can ask for it too, as words.
    let (ok, text) = tool(&mut agent, 1, json!({ "action": "context" })).await;
    assert!(ok && text == "Your context holds 15002 of 1000000 tokens (1%).", "{text}");
    engine.shutdown();
    let (again, _rx) = new_engine(&dir, pipes().0);
    assert_eq!(again.open(&id).unwrap().context, Some(ContextFill { used: 15_002, window: 1_000_000 }));

    again.shutdown();
}

#[tokio::test]
async fn a_compaction_the_cli_reports_resets_the_meter_to_the_summary() {
    let (dir, project) = (crate::support::DataFolder::new(), crate::support::work_folder());
    let (engine, _rx, mut agent, id) = started(&dir, project.path()).await;
    agent.say(response(Value::Null, 2, 200_000, 5_000)).await;
    agent.say(result(1_000_000)).await;
    until(&engine, &id, |c| idle(c) && c.context.is_some_and(|c| c.used == 205_002)).await;
    agent.say(json!({ "type": "system", "subtype": "compact_boundary", "compact_metadata": { "trigger": "auto", "pre_tokens": 205_002, "post_tokens": 1_200 } })).await;
    until(&engine, &id, |c| c.context == Some(ContextFill { used: 1_200, window: 1_000_000 })).await;
    engine.shutdown();
}

#[tokio::test]
async fn the_agent_compacts_itself_when_its_turn_ends_and_gets_its_own_next_word_for_word() {
    let (dir, project) = (crate::support::DataFolder::new(), crate::support::work_folder());
    let (engine, _rx, mut agent, id) = started(&dir, project.path()).await;
    agent.say(response(Value::Null, 2, 150_000, 5_000)).await;
    let (ok, text) = tool(&mut agent, 1, json!({ "action": "context" })).await;
    assert!(ok && text.contains("not known yet"), "the window comes with the first result: {text}");

    let next = "Finish the docs page: the parser section is written, the lexer section is left (the notes are in the page itself).";
    let (ok, text) = tool(&mut agent, 2, json!({ "action": "compact", "next": next })).await;
    assert!(ok && text.contains("end your current turn"), "{text}");
    // The turn is not cut: nothing is sent while it runs.
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert_eq!(turns(&agent), vec!["Fix the login page, then the rest of the list".to_string()]);

    agent.say(response(Value::Null, 2, 150_000, 5_000)).await;
    agent.say(result(1_000_000)).await;
    until_turns(&agent, 2).await;
    assert_eq!(turns(&agent)[1], compact_line(), "the CLI is asked once the turn ended");
    // While it compacts, the conversation is at work: the person's message waits behind it.
    let queued = SendRequest {
        conversation_id: Some(id.clone()),
        area: Area::Project,
        project_id: None,
        text: "Also the footer".into(),
        images: vec![],
        files: vec![],
        choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
    };
    engine.send(queued).unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(turns(&agent).len(), 2, "nothing goes out before the compaction ended");

    compacts(&mut agent, 1_166).await;
    until_turns(&agent, 3).await;
    assert_eq!(turns(&agent)[2], "Also the footer", "the person's waiting message goes first");
    let c = engine.open(&id).unwrap();
    assert_eq!(c.queued.len(), 1);
    assert_eq!(c.queued[0].wake.as_ref().map(|w| w.why.as_str()), Some("wake.compacted"));
    assert_eq!(c.context.map(|c| c.used), Some(1_166), "the meter shows what is left");
    answers(&mut agent, "The footer is fixed.", true).await;
    until_turns(&agent, 4).await;
    assert_eq!(turns(&agent)[3], format!("The conversation was compacted. Continue: {next}"));
    let c = engine.open(&id).unwrap();
    let woke = c
        .messages
        .iter()
        .find(|m| m.wake.as_ref().is_some_and(|w| w.why == "wake.compacted"))
        .expect("a quiet line says Faber compacted it");
    assert!(woke.text.ends_with(next));
    assert!(c.queued.is_empty(), "the continuation follows the waiting message");
    answers(&mut agent, "", false).await;
    until(&engine, &id, idle).await;
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert_eq!(turns(&agent).len(), 4, "a failed continuation neither retries nor compacts again");
    engine.shutdown();
}

#[tokio::test]
async fn a_compaction_without_next_sends_no_turn_and_a_failed_turn_compacts_nothing() {
    let (dir, project) = (crate::support::DataFolder::new(), crate::support::work_folder());
    let (engine, _rx, mut agent, id) = started(&dir, project.path()).await;
    let (ok, _) = tool(&mut agent, 1, json!({ "action": "compact" })).await;
    assert!(ok);
    answers(&mut agent, "One topic is done.", true).await;
    until_turns(&agent, 2).await;
    assert_eq!(turns(&agent)[1], compact_line());
    compacts(&mut agent, 900).await;
    until(&engine, &id, |c| !c.compacting).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(turns(&agent).len(), 2, "with no next, the conversation waits for the person");
    assert!(engine.open(&id).unwrap().messages.iter().all(|m| m.wake.is_none()));

    // A turn that fails after the request compacts nothing: the request ended with it.
    engine
        .send(SendRequest {
            conversation_id: Some(id.clone()),
            area: Area::Project,
            project_id: None,
            text: "again".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
        })
        .unwrap();
    until_turns(&agent, 3).await;
    let (ok, _) = tool(&mut agent, 2, json!({ "action": "compact", "next": "x" })).await;
    assert!(ok);
    answers(&mut agent, "", false).await;
    until(&engine, &id, idle).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(turns(&agent).len(), 3, "no /compact after a failed turn");
    engine.shutdown();
}

/// A compaction is the CLI at work without a turn of Faber's, so neither the idle timer nor the limit of warm processes
/// may take its process away: its end would never be heard and the conversation would wait on it for good.
#[tokio::test]
async fn a_process_that_compacts_is_neither_closed_as_idle_nor_evicted_for_a_newer_conversation() {
    let (dir, project) = (crate::support::DataFolder::new(), crate::support::work_folder());
    let (engine, _rx, mut agent, _id, made) = started_with(&dir, project.path(), Duration::from_millis(150), 1).await;
    let (ok, _) = tool(&mut agent, 1, json!({ "action": "compact", "next": "Go on with the footer." })).await;
    assert!(ok);
    answers(&mut agent, "The header is done.", true).await;
    until_turns(&agent, 2).await;
    // The idle time passes while the CLI compacts.
    tokio::time::sleep(Duration::from_millis(450)).await;
    assert!(!agent.closed.load(std::sync::atomic::Ordering::SeqCst), "the idle timer closed the process that was compacting");
    // A newer conversation takes a process beyond the limit of one.
    engine
        .send(SendRequest {
            conversation_id: None,
            area: Area::Chat,
            project_id: None,
            text: "Something else".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
        })
        .unwrap();
    let _other = next_pipe(&made).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!agent.closed.load(std::sync::atomic::Ordering::SeqCst), "the newer conversation evicted the process that was compacting");
    // A change of settings that retires idle project processes leaves this one be.
    engine.save_settings(Settings { user_hooks: Some(true), ..Settings::default() }).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!agent.closed.load(std::sync::atomic::Ordering::SeqCst), "a change of settings closed the process that was compacting");
    compacts(&mut agent, 900).await;
    until_turns(&agent, 3).await;
    assert_eq!(turns(&agent)[2], "The conversation was compacted. Continue: Go on with the footer.");
    engine.shutdown();
}

/// A compaction is work: the running list has it (so an update waits and the screen offers Stop) until the CLI ends it.
#[tokio::test]
async fn a_compaction_is_work_in_the_running_list_until_the_cli_ends_it() {
    let (dir, project) = (crate::support::DataFolder::new(), crate::support::work_folder());
    let (engine, _rx, mut agent, id) = started(&dir, project.path()).await;
    let (ok, _) = tool(&mut agent, 1, json!({ "action": "compact" })).await;
    assert!(ok);
    answers(&mut agent, "The header is done.", true).await;
    until_turns(&agent, 2).await;
    until(&engine, &id, |c| c.compacting).await;
    assert!(engine.is_working(), "nothing says the engine is at work while the CLI compacts");
    let item = engine.state().running.items.into_iter().find(|i| i.conversation_id == id).expect("a row in the running list");
    assert_eq!(item.state, RunState::Working);
    compacts(&mut agent, 900).await;
    until(&engine, &id, |c| !c.compacting).await;
    assert!(!engine.is_working(), "and nothing keeps saying it after the compaction ended");
    engine.shutdown();
}

/// Stop during a compaction reaches the CLI, and what the person had written stays in the queue: nothing goes on by itself
/// after a Stop, as after a stopped turn.
#[tokio::test]
async fn stop_during_a_compaction_interrupts_it_and_nothing_goes_out_after_it() {
    let (dir, project) = (crate::support::DataFolder::new(), crate::support::work_folder());
    let (engine, _rx, mut agent, id) = started(&dir, project.path()).await;
    let (ok, _) = tool(&mut agent, 1, json!({ "action": "compact", "next": "Go on with the footer." })).await;
    assert!(ok);
    answers(&mut agent, "The header is done.", true).await;
    until_turns(&agent, 2).await;
    engine
        .send(SendRequest {
            conversation_id: Some(id.clone()),
            area: Area::Project,
            project_id: None,
            text: "Also the sidebar".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
        })
        .unwrap();
    engine.interrupt(&id);
    support::until("the CLI was never told to stop", || agent.lines.lock().unwrap().iter().any(|m| m["request"]["subtype"] == "interrupt"))
        .await;
    assert!(agent.lines.lock().unwrap().iter().any(|m| m["request"]["subtype"] == "interrupt"), "the CLI was never told to stop");
    compacts(&mut agent, 900).await;
    until(&engine, &id, |c| !c.compacting).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(turns(&agent).len(), 2, "nothing is sent after Stop: {:?}", turns(&agent));
    let c = engine.open(&id).unwrap();
    assert_eq!(c.queued.len(), 1, "the person's message still waits: {:?}", c.queued);
    assert!(c.messages.iter().all(|m| m.wake.is_none()), "and the agent's own words are not sent either");
    engine.shutdown();
}

/// A Codex app-server that answers like the recorded one: a turn that calls the conversation tool once, a compaction that runs
/// as a turn of its own, and a context update after every request.
fn codex(seen: Arc<Mutex<Vec<Value>>>) -> Spawner {
    codex_with(seen, true)
}

/// The small app-server; `asks` makes the first turn call `compact` itself.
fn codex_with(seen: Arc<Mutex<Vec<Value>>>, asks: bool) -> Spawner {
    Arc::new(move |_bin, _argv, _cwd| {
        let (driver_in, fake_in) = tokio::io::duplex(1 << 20);
        let (mut out, driver_out) = tokio::io::duplex(1 << 20);
        let seen = seen.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(fake_in).lines();
            let mut turns = 0;
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
                seen.lock().unwrap().push(msg.clone());
                let (id, params) = (msg["id"].clone(), &msg["params"]);
                let thread = params["threadId"].as_str().unwrap_or("thread-1").to_string();
                let usage = |last: u64| {
                    json!({ "method": "thread/tokenUsage/updated", "params": { "threadId": thread, "turnId": "t", "tokenUsage": {
                        "total": { "totalTokens": last, "inputTokens": last, "cachedInputTokens": 0, "outputTokens": 0, "reasoningOutputTokens": 0 },
                        "last": { "totalTokens": last, "inputTokens": last - 5, "cachedInputTokens": 0, "outputTokens": 5, "reasoningOutputTokens": 0 },
                        "modelContextWindow": 258_400 } } })
                };
                let finish = |turn: &str, item: Value| {
                    let done = json!({ "id": turn, "items": [], "status": "completed" });
                    vec![
                        json!({ "method": "item/completed", "params": { "threadId": thread, "turnId": turn, "item": item } }),
                        json!({ "method": "turn/completed", "params": { "threadId": thread, "turn": done } }),
                    ]
                };
                let started = |turn: &str| {
                    let t = json!({ "id": turn, "items": [], "status": "inProgress" });
                    json!({ "method": "turn/started", "params": { "threadId": thread, "turn": t } })
                };
                let message = json!({ "type": "agentMessage", "id": "m", "text": "done", "phase": "final_answer" });
                let mut say = vec![];
                match msg["method"].as_str() {
                    Some("initialize") => say.push(json!({ "id": id, "result": {} })),
                    Some("config/read") => say.push(json!({ "id": id, "result": { "config": {} } })),
                    Some("thread/start" | "thread/resume") => {
                        say.push(json!({ "id": id, "result": { "thread": { "id": "thread-1", "model": "gpt-6-luna" } } }))
                    }
                    Some("turn/start") => {
                        turns += 1;
                        let turn = format!("turn-{turns}");
                        say.push(json!({ "id": id, "result": { "turn": { "id": turn, "items": [], "status": "inProgress" } } }));
                        say.push(started(&turn));
                        if turns == 1 && asks {
                            // The agent asks to be compacted; the turn goes on when Faber answers the call.
                            say.push(usage(40_000));
                            say.push(json!({ "id": 900, "method": "item/tool/call", "params": { "threadId": thread, "turnId": turn, "callId": "c1", "tool": "conversation",
                                "arguments": { "action": "compact", "next": "Write the lexer section." } } }));
                        } else {
                            say.extend(finish(&turn, message.clone()));
                        }
                    }
                    Some("thread/compact/start") => {
                        say.push(json!({ "id": id, "result": {} }));
                        say.push(started("turn-compact"));
                        say.push(json!({ "method": "item/started", "params": { "threadId": thread, "turnId": "turn-compact", "item": { "type": "contextCompaction", "id": "k" } } }));
                        say.push(usage(13_808));
                        say.extend(finish("turn-compact", json!({ "type": "contextCompaction", "id": "k" })));
                    }
                    None if id == 900 => say.extend(finish("turn-1", message.clone())),
                    _ => {}
                }
                for m in say {
                    if out.write_all(format!("{m}\n").as_bytes()).await.is_err() {
                        return;
                    }
                }
            }
        });
        Ok(Proc { stdin: Box::new(driver_in), stdout: Box::new(driver_out), stderr: None, child: None, contained: None })
    })
}

#[tokio::test]
async fn codex_reports_its_context_compacts_on_request_and_continues_with_next() {
    let (dir, project) = (crate::support::DataFolder::new(), crate::support::work_folder());
    let seen = Arc::new(Mutex::new(vec![]));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let opts = EngineOptions {
        data_dir: dir.path().into(),
        #[cfg(windows)]
        chat_dir: dir.chat_path().into(),
        clis: BTreeMap::from([(Vendor::Codex, Cli { command: "codex".into(), prefix: vec![] })]),
        spawn: codex(seen.clone()),
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e)))).unwrap();
    let plan = faber_engine::signals::Signals { billing: Some(faber_engine::budget::Billing::Subscription), ..Default::default() };
    engine.note_account_events(Vendor::Codex, vec![faber_engine::events::EngineEvent::Signals(plan)]);
    let p = engine.add_project(&project.path().to_string_lossy()).unwrap();
    let id = engine
        .send(SendRequest {
            conversation_id: None,
            area: Area::Project,
            project_id: Some(p.id),
            text: "Write the docs".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "codex".into(), model: None, effort: Some(Effort::High) },
        })
        .unwrap();
    // The follow-up is Faber's own message, which Codex receives as the second turn.
    let c = until(&engine, &id, |c| c.messages.len() >= 4 && idle(c)).await;
    let methods: Vec<String> = seen.lock().unwrap().iter().filter_map(|m| m["method"].as_str().map(String::from)).collect();
    let at = |name: &str| methods.iter().position(|m| m == name).unwrap_or_else(|| panic!("{name} missing in {methods:?}"));
    assert!(at("turn/start") < at("thread/compact/start"), "the compaction waits for the turn: {methods:?}");
    let compact = seen.lock().unwrap().iter().find(|m| m["method"] == "thread/compact/start").cloned().unwrap();
    assert_eq!(compact["params"]["threadId"], "thread-1");
    let second = seen.lock().unwrap().iter().filter(|m| m["method"] == "turn/start").nth(1).cloned().expect("a turn after the compaction");
    assert_eq!(second["params"]["input"][0]["text"], "The conversation was compacted. Continue: Write the lexer section.");
    assert_eq!(c.messages[2].wake.as_ref().map(|w| w.why.as_str()), Some("wake.compacted"));
    // The meter followed: 40000 before, what was left after, against the window Codex names.
    let seen = context_events(&mut rx);
    assert!(seen.contains(&Some(ContextFill { used: 40_000, window: 258_400 })), "{seen:?}");
    assert!(seen.contains(&Some(ContextFill { used: 13_808, window: 258_400 })), "{seen:?}");
    assert_eq!(c.context, Some(ContextFill { used: 13_808, window: 258_400 }));
    engine.shutdown();
}

fn codex_engine(dir: &crate::support::DataFolder, seen: Arc<Mutex<Vec<Value>>>) -> Engine {
    let opts = EngineOptions {
        data_dir: dir.path().into(),
        #[cfg(windows)]
        chat_dir: dir.chat_path().into(),
        clis: BTreeMap::from([(Vendor::Codex, Cli { command: "codex".into(), prefix: vec![] })]),
        spawn: codex_with(seen, false),
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    let engine = Engine::new(opts, Arc::new(|_| {})).unwrap();
    let plan = faber_engine::signals::Signals { billing: Some(faber_engine::budget::Billing::Subscription), ..Default::default() };
    engine.note_account_events(Vendor::Codex, vec![faber_engine::events::EngineEvent::Signals(plan)]);
    engine
}

/// "Compact now" in the context meter (task 190): Claude Code gets its own `/compact`, and nothing is sent after it, since
/// the person said nothing more.
#[tokio::test]
async fn compact_now_sends_claude_code_its_own_compact_and_nothing_after_it() {
    let (dir, project) = (crate::support::DataFolder::new(), crate::support::work_folder());
    let (engine, _rx, mut agent, id) = started(&dir, project.path()).await;
    // While the agent works, it is not the moment: the compaction would cut its turn.
    assert_eq!(engine.compact_now(&id), Err("error.compact.busy".to_string()));
    answers(&mut agent, "The login page works.", true).await;
    until(&engine, &id, idle).await;
    engine.compact_now(&id).unwrap();
    until_turns(&agent, 2).await;
    assert!(turns(&agent)[1].starts_with("/compact "), "{:?}", turns(&agent));
    assert!(until(&engine, &id, |c| c.compacting).await.compacting);
    compacts(&mut agent, 900).await;
    let c = until(&engine, &id, |c| !c.compacting).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(turns(&agent).len(), 2, "nothing goes out after a compaction the person asked for");
    assert!(c.messages.iter().all(|m| m.wake.is_none()));
    engine.shutdown();
}

/// Claude Code closes an idle process: "Compact now" starts one on the conversation's own session for the compaction.
#[tokio::test]
async fn compact_now_starts_claude_code_again_when_its_idle_process_was_closed() {
    let (dir, project) = (crate::support::DataFolder::new(), crate::support::work_folder());
    let (engine, _rx, mut agent, id, made) = started_with(&dir, project.path(), Duration::from_millis(100), 3).await;
    answers(&mut agent, "The login page works.", true).await;
    until(&engine, &id, idle).await;
    support::until("the idle process stayed open", || agent.closed.load(std::sync::atomic::Ordering::SeqCst)).await;
    assert!(agent.closed.load(std::sync::atomic::Ordering::SeqCst), "the idle process stays open");
    engine.compact_now(&id).unwrap();
    let mut again = next_pipe(&made).await;
    until_turns(&again, 1).await;
    assert!(turns(&again).len() == 1 && turns(&again)[0].starts_with("/compact "), "{:?}", turns(&again));
    // The person types meanwhile with another model picked: the warm-up waits, since a new process would end the compaction.
    let other = Choice { vendor: "claude".into(), model: Some("opus".into()), effort: Some(Effort::High) };
    engine.warm(WarmRequest {
        conversation_id: Some(id.clone()),
        area: Area::Project,
        project_id: engine.open(&id).unwrap().project_id,
        choice: other,
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(made.lock().unwrap().is_empty(), "a warm-up replaced the compacting process");
    compacts(&mut again, 700).await;
    until(&engine, &id, |c| !c.compacting).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(turns(&again).len(), 1, "nothing goes out after it");
    engine.shutdown();
}

/// Opening the meter asks Claude Code what fills the context, without a turn; the answer comes back in Faber's parts.
#[tokio::test]
async fn the_meter_asks_claude_code_what_fills_the_context_without_a_turn() {
    let (dir, project) = (crate::support::DataFolder::new(), crate::support::work_folder());
    let (engine, mut rx, mut agent, id) = started(&dir, project.path()).await;
    answers(&mut agent, "The login page works.", true).await;
    until(&engine, &id, idle).await;
    assert!(engine.context_parts(&id));
    let mut request = None;
    support::until("no get_context_usage request", || {
        request = agent.lines.lock().unwrap().iter().find(|m| m["request"]["subtype"] == "get_context_usage").cloned();
        request.is_some()
    })
    .await;
    let request = request.unwrap();
    assert_eq!(request["request"]["detail"], "summary", "a summary sends no token-count request");
    assert_eq!(turns(&agent).len(), 1, "no turn for it");
    // The shape of Claude Code's answer (SDKControlGetContextUsageResponse), cut to what Faber reads.
    let categories = json!([
        { "name": "System prompt", "tokens": 3000, "color": "x", "kind": "used" },
        { "name": "System tools", "tokens": 12000, "color": "x", "kind": "used" },
        { "name": "MCP tools", "tokens": 2000, "color": "x", "kind": "used" },
        { "name": "MCP tools (deferred)", "tokens": 9000, "color": "x", "kind": "deferred", "isDeferred": true },
        { "name": "Memory files", "tokens": 1500, "color": "x", "kind": "used" },
        { "name": "Skills", "tokens": 500, "color": "x", "kind": "used" },
        { "name": "Messages", "tokens": 40000, "color": "x", "kind": "used" },
        { "name": "Free space", "tokens": 900000, "color": "x", "kind": "free" },
        { "name": "Autocompact buffer", "tokens": 33000, "color": "x", "kind": "buffer" },
    ]);
    agent
        .say(json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request["request_id"],
            "response": { "categories": categories, "totalTokens": 59000, "maxTokens": 1_000_000, "percentage": 5.9 } } }))
        .await;
    let mut parts = None;
    support::until("no parts", || {
        while let Ok(e) = rx.try_recv() {
            if let AppEvent::ContextParts { conversation_id, parts: p } = e {
                assert_eq!(conversation_id, id);
                parts = Some(p);
            }
        }
        parts.is_some()
    })
    .await;
    let parts: Vec<(String, u64)> = parts.unwrap().into_iter().map(|p| (p.key, p.tokens)).collect();
    assert_eq!(
        parts,
        vec![
            ("ctx.part.system".to_string(), 3500),
            ("ctx.part.tools".to_string(), 14000),
            ("ctx.part.files".to_string(), 1500),
            ("ctx.part.conversation".to_string(), 40000),
        ]
    );
    engine.shutdown();
}

/// Codex gets its own compaction request, also for a conversation reopened after Faber started again, whose thread is still
/// opening when the person asks; and it counts no parts.
#[tokio::test]
async fn compact_now_sends_codex_its_own_request_also_while_its_thread_opens() {
    let (dir, project) = (crate::support::DataFolder::new(), crate::support::work_folder());
    let seen = Arc::new(Mutex::new(vec![]));
    let engine = codex_engine(&dir, seen.clone());
    let p = engine.add_project(&project.path().to_string_lossy()).unwrap();
    let request = SendRequest {
        conversation_id: None,
        area: Area::Project,
        project_id: Some(p.id),
        text: "Write the docs".into(),
        images: vec![],
        files: vec![],
        choice: Choice { vendor: "codex".into(), model: None, effort: Some(Effort::High) },
    };
    let id = engine.send(request).unwrap();
    until(&engine, &id, |c| c.messages.len() >= 2 && idle(c)).await;
    assert!(!engine.context_parts(&id), "Codex reports only the whole");
    engine.shutdown();

    let seen = Arc::new(Mutex::new(vec![]));
    let engine = codex_engine(&dir, seen.clone());
    // The person opens it again after Faber started, and asks.
    engine.open(&id).unwrap();
    engine.compact_now(&id).unwrap();
    until(&engine, &id, |c| !c.compacting).await;
    let methods: Vec<String> = seen.lock().unwrap().iter().filter_map(|m| m["method"].as_str().map(String::from)).collect();
    let at = |name: &str| methods.iter().position(|m| m == name).unwrap_or_else(|| panic!("{name} missing in {methods:?}"));
    assert!(at("thread/resume") < at("thread/compact/start"), "{methods:?}");
    assert!(!methods.iter().any(|m| m == "turn/start"), "no turn for a compaction: {methods:?}");
    assert_eq!(engine.open(&id).unwrap().context.map(|f| f.used), Some(13_808));
    engine.shutdown();
}

/// The agent calls one of Faber's progress tools (`wrap_up` here) and Faber answers it.
async fn progress(agent: &mut Pipe, request: &str, tool: &str, input: Value) {
    let call = json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": { "name": tool, "arguments": input } });
    agent
        .say(json!({ "type": "control_request", "request_id": request,
        "request": { "subtype": "mcp_message", "server_name": "faber", "message": call } }))
        .await;
    support::until(&format!("Faber never answered the {tool} call"), || {
        agent.lines.lock().unwrap().iter().any(|m| m["response"]["request_id"] == request)
    })
    .await;
}

/// A Keep going turn whose main thread held `read` tokens, closing a task with a box or not.
async fn kept_turn(agent: &mut Pipe, n: usize, read: u64, closes: bool) {
    agent.say(response(Value::Null, 2, read, 0)).await;
    if closes {
        progress(agent, &format!("w{n}"), "wrap_up", json!({ "recap": "The login page is fixed and checked." })).await;
    }
    agent.say(result(1_000_000)).await;
}

/// Under Keep going Faber compacts by itself between tasks, like a careful person: after a reply that closed a task with a
/// context of a quarter of its window, it asks the CLI to compact before it sends the next turn, and the turn follows the
/// summary. A small context, or a reply that closed nothing below three fifths, goes straight on. A compaction that failed
/// is not asked again before the next turn.
#[tokio::test]
async fn keep_going_compacts_a_large_context_between_tasks_before_its_next_turn() {
    let (dir, project) = (crate::support::DataFolder::new(), crate::support::work_folder());
    let (engine, _rx, mut agent, id) = started(&dir, project.path()).await;
    engine.set_keep_going_pace(0.001);
    engine.keep_going(&id, true).unwrap();
    let keep_going = |t: &str| t.starts_with("The person is away");

    // Small: on at once.
    kept_turn(&mut agent, 1, 200_000, true).await;
    until_turns(&agent, 2).await;
    assert!(keep_going(&turns(&agent)[1]), "{:?}", turns(&agent));
    // Large, but the reply closed nothing: on at once too.
    kept_turn(&mut agent, 2, 400_000, false).await;
    until_turns(&agent, 3).await;
    assert!(keep_going(&turns(&agent)[2]), "{:?}", turns(&agent));

    // A task closed with a quarter of the window held: compacted first, then the next turn.
    kept_turn(&mut agent, 3, 300_000, true).await;
    until_turns(&agent, 4).await;
    assert_eq!(turns(&agent)[3], compact_line());
    compacts(&mut agent, 9_000).await;
    until_turns(&agent, 5).await;
    assert!(keep_going(&turns(&agent)[4]), "{:?}", turns(&agent));
    assert_eq!(engine.open(&id).unwrap().context.map(|c| c.used), Some(9_000));

    // A compaction that fails goes on with the turn, and is not asked again in a loop.
    kept_turn(&mut agent, 4, 700_000, false).await;
    until_turns(&agent, 6).await;
    assert_eq!(turns(&agent)[5], compact_line());
    agent.say(json!({ "type": "result", "subtype": "error_during_execution", "is_error": true, "total_cost_usd": 0.0, "usage": {} })).await;
    until_turns(&agent, 7).await;
    assert!(keep_going(&turns(&agent)[6]), "{:?}", turns(&agent));
    engine.shutdown();
}

#[tokio::test]
async fn the_agent_clears_its_context_and_a_new_session_starts_from_the_persons_requests_and_its_note() {
    let (dir, project) = (crate::support::DataFolder::new(), crate::support::work_folder());
    let (engine, mut rx, mut agent, id, made) = started_with(&dir, project.path(), Duration::from_secs(120), 3).await;
    agent.say(response(Value::Null, 2, 150_000, 5_000)).await;
    let (ok, text) = tool(&mut agent, 1, json!({ "action": "clear" })).await;
    assert!(!ok && text.contains("handoff note"), "a fresh context starts from the note alone, so one is needed: {text}");
    let next = "The login page is fixed and checked (auth.rs, cargo t --filter login). Next: the rest of the list, from item 2.";
    let (ok, text) = tool(&mut agent, 2, json!({ "action": "clear", "next": next })).await;
    assert!(ok && text.contains("fresh context is scheduled"), "{text}");
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(turns(&agent).len(), 1, "the turn is not cut");

    agent.say(response(Value::Null, 2, 150_000, 5_000)).await;
    agent.say(result(1_000_000)).await;
    let fresh = next_pipe(&made).await;
    until_turns(&fresh, 1).await;
    assert!(agent.closed.load(std::sync::atomic::Ordering::SeqCst), "the old session ends");
    assert_eq!(turns(&agent).len(), 1, "nothing more goes to the old session, not even a /compact");
    let first = &turns(&fresh)[0];
    assert!(first.starts_with("Faber started your context fresh"), "{first}");
    assert!(first.contains("Fix the login page, then the rest of the list"), "the person's request, word for word: {first}");
    assert!(first.ends_with(&format!("Your handoff note: {next}")), "{first}");
    assert!(!first.contains("Earlier in this conversation"), "the old conversation is never sent again: {first}");

    let c = engine.open(&id).unwrap();
    let old_reply = &c.messages[1];
    assert_eq!(c.fresh_after.as_deref(), Some(old_reply.id.as_str()));
    assert!(c.sessions.is_empty() && c.context.is_none(), "{:?} {:?}", c.sessions, c.context);
    assert!(c.messages.iter().any(|m| m.wake.as_ref().is_some_and(|w| w.why == "wake.cleared")), "a quiet line says so");
    assert!(context_events(&mut rx).contains(&None), "the meter empties");
    assert_eq!(c.messages.len(), 4, "the person still sees the whole conversation");
    engine.shutdown();
}

#[tokio::test]
async fn a_failed_compaction_continues_after_messages_already_waiting() {
    let (dir, project) = (crate::support::DataFolder::new(), crate::support::work_folder());
    let (engine, _rx, mut agent, id) = started(&dir, project.path()).await;
    let (ok, text) = tool(&mut agent, 1, json!({ "action": "compact", "next": "Finish the docs." })).await;
    assert!(ok, "{text}");
    answers(&mut agent, "Ready to compact.", true).await;
    until_turns(&agent, 2).await;
    engine
        .send(SendRequest {
            conversation_id: Some(id.clone()),
            area: Area::Project,
            project_id: None,
            text: "Also the footer".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
        })
        .unwrap();
    answers(&mut agent, "", false).await;
    until_turns(&agent, 3).await;
    assert_eq!(turns(&agent)[2], "Also the footer");
    let c = engine.open(&id).unwrap();
    assert_eq!(c.queued.len(), 1);
    assert_eq!(c.queued[0].wake.as_ref().map(|w| w.why.as_str()), Some("wake.compactFailed"));
    answers(&mut agent, "Footer fixed.", true).await;
    until_turns(&agent, 4).await;
    assert!(turns(&agent)[3].ends_with("Finish the docs."));
    engine.shutdown();
}

#[tokio::test]
async fn clearing_context_delivers_waiting_messages_before_the_handoff() {
    let (dir, project) = (crate::support::DataFolder::new(), crate::support::work_folder());
    let (engine, _rx, mut agent, id, made) = started_with(&dir, project.path(), Duration::from_secs(120), 3).await;
    let (ok, text) = tool(&mut agent, 1, json!({ "action": "clear", "next": "Finish the docs." })).await;
    assert!(ok, "{text}");
    engine
        .send(SendRequest {
            conversation_id: Some(id.clone()),
            area: Area::Project,
            project_id: None,
            text: "Also the footer".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
        })
        .unwrap();
    agent.say(result(1_000_000)).await;
    let mut fresh = next_pipe(&made).await;
    until_turns(&fresh, 1).await;
    assert_eq!(turns(&fresh)[0], "Also the footer");
    assert!(!turns(&fresh)[0].contains("Earlier in this conversation"));
    fresh.say(json!({ "type": "system", "subtype": "init", "session_id": "fresh", "model": "claude-haiku-5-5" })).await;
    answers(&mut fresh, "Footer fixed.", true).await;
    until_turns(&fresh, 2).await;
    let text = &turns(&fresh)[1];
    assert!(text.contains("Fix the login page, then the rest of the list"), "{text}");
    assert!(text.ends_with("Your handoff note: Finish the docs."), "{text}");
    assert_eq!(engine.open(&id).unwrap().messages[4].wake.as_ref().map(|w| w.why.as_str()), Some("wake.cleared"));
    engine.shutdown();
}

/// Task 110: a compaction the CLI never ends held the queue and Keep going for ever with nothing said. Past five minutes the
/// reply it follows says so, with Stop; at the deadline Faber stops it like a compaction that failed, so the context stays as
/// it was and the message that waited goes out.
#[tokio::test]
async fn a_compaction_that_takes_too_long_is_told_then_stopped_and_the_queue_goes_on() {
    use faber_engine::model::StallKind;
    let (dir, project) = (crate::support::DataFolder::new(), crate::support::work_folder());
    let (engine, _rx, mut agent, id) = started(&dir, project.path()).await;
    let clock = Arc::new(Mutex::new(1_000_000.0));
    let time = clock.clone();
    engine.set_time_source(Arc::new(move || *time.lock().unwrap()));
    let (ok, _) = tool(&mut agent, 1, json!({ "action": "compact", "next": "Go on with the footer." })).await;
    assert!(ok);
    answers(&mut agent, "The header is done.", true).await;
    until_turns(&agent, 2).await;
    until(&engine, &id, |c| c.compacting).await;
    engine
        .send(SendRequest {
            conversation_id: Some(id.clone()),
            area: Area::Project,
            project_id: None,
            text: "Also the sidebar".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
        })
        .unwrap();
    // The timer's looks, one every half minute, up to `secs` after the compaction started.
    let start = *clock.lock().unwrap();
    let walk = |secs: f64| {
        let mut now = *clock.lock().unwrap();
        while now < start + secs {
            now = (now + 30.0).min(start + secs);
            *clock.lock().unwrap() = now;
            engine.check_compactions();
        }
    };
    let stall = || engine.open(&id).unwrap().messages.last().unwrap().stall.map(|s| (s.kind, s.minutes));
    let interrupted = || agent.lines.lock().unwrap().iter().any(|m| m["request"]["subtype"] == "interrupt");
    walk(299.0);
    assert_eq!(stall(), None, "a few minutes are a normal compaction");
    walk(330.0);
    assert_eq!(stall(), Some((StallKind::Compacting, Some(5))));
    let item = engine.state().running.items.into_iter().find(|i| i.conversation_id == id).unwrap();
    assert_eq!((item.state, item.stall.map(|s| s.kind)), (RunState::NeedsYou, Some(StallKind::Compacting)));
    walk(899.0);
    assert!(!interrupted(), "told, not stopped, before the deadline");
    walk(900.0);
    support::until("the CLI was never told to stop at the deadline", interrupted).await;
    // The CLI ends it as interrupted: a compaction that failed. The context is the one before, and the queue goes on.
    answers(&mut agent, "", false).await;
    until_turns(&agent, 3).await;
    assert_eq!(turns(&agent)[2], "Also the sidebar");
    let c = engine.open(&id).unwrap();
    assert!(!c.compacting);
    assert_eq!(c.queued.first().and_then(|q| q.wake.as_ref()).map(|w| w.why.as_str()), Some("wake.compactFailed"));
    assert!(c.messages.iter().all(|m| m.stall.is_none() || m.stall.is_some_and(|s| s.kind != StallKind::Compacting)), "the notice went");
    engine.shutdown();
}

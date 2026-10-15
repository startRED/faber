//! A CLI saves a session only after its first turn. These tests run the engine against a Codex app-server that behaves so:
//! `thread/resume` of a thread that never ran a turn fails with "no rollout found", as the real one does.

use faber_engine::cli::Cli;
use faber_engine::model::{Area, Role, Status, Vendor};
use faber_engine::proc::{Proc, Spawner};
use faber_engine::router::{Choice, Effort};
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest, WarmRequest};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::UnboundedReceiver;

/// What the app-server keeps on disk (shared by every process, like `~/.codex`) and what it was asked.
#[derive(Default)]
struct Disk {
    /// Threads that had a turn: the only ones with a rollout file.
    ran: HashSet<String>,
    started: usize,
    /// Every thread request, as "method thread-id" (a start has no id yet).
    asked: Vec<String>,
    /// The text of every turn, in order.
    turns: Vec<String>,
}

fn codex(disk: Arc<Mutex<Disk>>) -> Spawner {
    Arc::new(move |_bin, _argv, _cwd| {
        let (driver_in, fake_in) = tokio::io::duplex(1 << 20);
        let (mut out, driver_out) = tokio::io::duplex(1 << 20);
        let disk = disk.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(fake_in).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
                let (id, params) = (msg["id"].clone(), &msg["params"]);
                let mut say = vec![];
                match msg["method"].as_str() {
                    Some("initialize") => say.push(json!({ "id": id, "result": {} })),
                    Some("config/read") => say.push(json!({ "id": id, "result": { "config": {} } })),
                    Some(m @ ("thread/start" | "thread/resume")) => {
                        let mut d = disk.lock().unwrap();
                        let thread = if m == "thread/start" {
                            d.started += 1;
                            d.asked.push(m.into());
                            format!("thread-{}", d.started)
                        } else {
                            let t = params["threadId"].as_str().unwrap_or_default().to_string();
                            d.asked.push(format!("{m} {t}"));
                            t
                        };
                        if m == "thread/resume" && !d.ran.contains(&thread) {
                            let error = format!("no rollout found for thread id {thread}");
                            say.push(json!({ "id": id, "error": { "code": -32600, "message": error } }));
                        } else {
                            say.push(json!({ "id": id, "result": { "thread": { "id": thread, "model": "gpt-6-luna" } } }));
                        }
                    }
                    Some("turn/start") => {
                        let thread = params["threadId"].as_str().unwrap_or_default().to_string();
                        let mut d = disk.lock().unwrap();
                        d.ran.insert(thread.clone());
                        d.turns.push(params["input"][0]["text"].as_str().unwrap_or_default().into());
                        let turn = json!({ "id": "turn-1", "items": [], "status": "inProgress" });
                        let item = json!({ "type": "agentMessage", "id": "m1", "text": "pong", "phase": "final_answer" });
                        say.push(json!({ "id": id, "result": { "turn": turn } }));
                        say.push(json!({ "method": "turn/started", "params": { "threadId": thread, "turn": turn } }));
                        say.push(json!({ "method": "item/completed", "params": { "threadId": thread, "turnId": "turn-1", "item": item } }));
                        let done = json!({ "id": "turn-1", "items": [item], "status": "completed" });
                        say.push(json!({ "method": "turn/completed", "params": { "threadId": thread, "turn": done } }));
                    }
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

fn start(dir: &Path, disk: &Arc<Mutex<Disk>>) -> (Engine, UnboundedReceiver<AppEvent>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let opts = EngineOptions {
        data_dir: dir.into(),
        clis: BTreeMap::from([(Vendor::Codex, Cli { command: "codex".into(), prefix: vec![] })]),
        spawn: codex(disk.clone()),
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    (Engine::new(opts, Arc::new(move |e| drop(tx.send(e)))), rx)
}

fn choice(effort: Effort) -> Choice {
    Choice { vendor: "codex".into(), model: None, effort: Some(effort) }
}

fn send(id: Option<&str>, text: &str, effort: Effort) -> SendRequest {
    SendRequest {
        conversation_id: id.map(String::from),
        area: Area::Chat,
        project_id: None,
        text: text.into(),
        images: vec![],
        files: vec![],
        choice: choice(effort),
    }
}

async fn finished(rx: &mut UnboundedReceiver<AppEvent>) -> faber_engine::model::Message {
    loop {
        let e = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.expect("no finished answer in time").expect("engine gone");
        if let AppEvent::Message { message, .. } = e
            && message.role == Role::Assistant
            && message.status != Some(Status::Streaming)
        {
            return *message;
        }
    }
}

async fn until(disk: &Arc<Mutex<Disk>>, ready: impl Fn(&Disk) -> bool) {
    for _ in 0..300 {
        if ready(&disk.lock().unwrap()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the app-server never saw the request");
}

/// The first message after a warm-up, with a model or effort the warm-up did not use (Auto's choice), is the case the owner hit.
#[tokio::test]
async fn a_warmed_thread_that_never_ran_is_not_resumed_when_the_first_turn_needs_another_launch() {
    let (dir, disk) = (tempfile::tempdir().unwrap(), Arc::<Mutex<Disk>>::default());
    let (engine, mut rx) = start(dir.path(), &disk);
    engine.warm(WarmRequest { conversation_id: None, area: Area::Chat, project_id: None, choice: choice(Effort::Low) });
    until(&disk, |d| d.started == 1).await;
    // Give the warm-up's session id time to reach the engine before the message does.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let id = engine.send(send(None, "first", Effort::High)).unwrap();
    let reply = finished(&mut rx).await;
    assert_eq!((reply.status, reply.text.as_str()), (Some(Status::Done), "pong"), "{:?}", reply.error);
    let d = disk.lock().unwrap();
    assert_eq!(d.asked, vec!["thread/start", "thread/start"], "the unused warm-up thread was resumed");
    assert_eq!(engine.open(&id).unwrap().sessions[&Vendor::Codex], "thread-2", "only the thread that ran is resumable");
    drop(d);
    engine.shutdown();
}

/// A conversation saved by an older Faber holds the id of a thread that never ran, and its only reply failed.
#[tokio::test]
async fn a_lost_thread_with_no_turns_starts_a_new_one_and_runs_the_message_once() {
    let (dir, disk) = (tempfile::tempdir().unwrap(), Arc::<Mutex<Disk>>::default());
    let (engine, mut rx) = start(dir.path(), &disk);
    let id = engine.send(send(None, "first", Effort::High)).unwrap();
    finished(&mut rx).await;
    engine.shutdown();
    let path = dir.path().join("conversations").join(format!("{id}.json"));
    let mut saved = engine.open(&id).unwrap();
    saved.messages.pop();
    saved.messages.pop();
    saved.sessions.insert(Vendor::Codex, "never-ran".into());
    std::fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
    disk.lock().unwrap().turns.clear();
    disk.lock().unwrap().ran.clear();
    let (engine, mut rx) = start(dir.path(), &disk);
    engine.send(send(Some(&id), "second", Effort::High)).unwrap();
    let reply = finished(&mut rx).await;
    assert_eq!((reply.status, reply.text.as_str()), (Some(Status::Done), "pong"), "{:?}", reply.error);
    let d = disk.lock().unwrap();
    assert_eq!(d.asked, vec!["thread/start", "thread/resume never-ran", "thread/start"]);
    assert_eq!(d.turns, vec!["second"], "the message ran once, on the new thread");
    assert_eq!(engine.open(&id).unwrap().sessions[&Vendor::Codex], "thread-2");
    drop(d);
    engine.shutdown();
}

#[tokio::test]
async fn a_lost_thread_that_had_turns_shows_an_error_and_never_drops_the_context() {
    let (dir, disk) = (tempfile::tempdir().unwrap(), Arc::<Mutex<Disk>>::default());
    let (engine, mut rx) = start(dir.path(), &disk);
    let id = engine.send(send(None, "first", Effort::High)).unwrap();
    finished(&mut rx).await;
    engine.shutdown();
    disk.lock().unwrap().ran.clear(); // Codex lost the rollout file
    let (engine, mut rx) = start(dir.path(), &disk);
    engine.send(send(Some(&id), "second", Effort::High)).unwrap();
    let reply = finished(&mut rx).await;
    assert_eq!((reply.status, reply.error.as_deref()), (Some(Status::Error), Some("error.sessionLost")));
    assert_eq!(disk.lock().unwrap().turns, vec!["first"], "the message was not run on a thread without the earlier turns");
    let c = engine.open(&id).unwrap();
    assert_eq!(c.messages.len(), 4, "the history is kept on screen");
    // The next message starts a new thread and is given the visible history, so nothing is lost silently.
    engine.send(send(Some(&id), "third", Effort::High)).unwrap();
    assert_eq!(finished(&mut rx).await.status, Some(Status::Done));
    let d = disk.lock().unwrap();
    assert_eq!(d.turns.len(), 2);
    assert!(d.turns[1].contains("first") && d.turns[1].ends_with("third"), "{:?}", d.turns[1]);
    drop(d);
    engine.shutdown();
}

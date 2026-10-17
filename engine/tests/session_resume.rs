//! A CLI saves a session only after its first turn. These tests run the engine against a Codex app-server that behaves so:
//! `thread/resume` of a thread that never ran a turn fails with "no rollout found", as the real one does.

use faber_engine::cli::Cli;
use faber_engine::model::{Area, Message, Role, Status, Vendor};
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

/// What the app-server keeps on disk (shared by every process, like `~/.codex`), what it was asked, and how it misbehaves.
#[derive(Default)]
struct Disk {
    /// Threads that had a turn: the only ones with a rollout file.
    ran: HashSet<String>,
    started: usize,
    /// Every thread request, as "method thread-id" (a start has no id yet).
    asked: Vec<String>,
    /// The text of every turn, in order.
    turns: Vec<String>,
    /// A new thread cannot start either: the same error comes back from a fresh launch.
    fail_starts: bool,
    /// A turn is accepted and then nothing more is said (the test treats this as the app dying).
    hang_turns: bool,
    /// A turn streams some text, then the process dies with the lost-session message on stderr.
    die_after_partial: bool,
    /// Runs when a resume fails, before the engine hears of it.
    on_lost: Option<Arc<dyn Fn() + Send + Sync>>,
}

type Shared = Arc<Mutex<Disk>>;

fn codex(disk: Shared) -> Spawner {
    Arc::new(move |_bin, _argv, _cwd| {
        let (driver_in, fake_in) = tokio::io::duplex(1 << 20);
        let (mut out, driver_out) = tokio::io::duplex(1 << 20);
        let (mut err, driver_err) = tokio::io::duplex(1 << 16);
        let disk = disk.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(fake_in).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
                let (id, params) = (msg["id"].clone(), &msg["params"]);
                let mut say = vec![];
                let mut dies = false;
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
                        let lost = if m == "thread/resume" { !d.ran.contains(&thread) } else { d.fail_starts };
                        if lost {
                            if let Some(hook) = d.on_lost.clone() {
                                hook();
                            }
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
                        say.push(json!({ "id": id, "result": { "turn": turn } }));
                        say.push(json!({ "method": "turn/started", "params": { "threadId": thread, "turn": turn } }));
                        if d.die_after_partial {
                            let begun = json!({ "type": "agentMessage", "id": "m1", "text": "", "phase": "final_answer" });
                            say.push(
                                json!({ "method": "item/started", "params": { "threadId": thread, "turnId": "turn-1", "item": begun } }),
                            );
                            let delta = json!({ "threadId": thread, "turnId": "turn-1", "itemId": "m1", "delta": "partial" });
                            say.push(json!({ "method": "item/agentMessage/delta", "params": delta }));
                            dies = true;
                        } else if !d.hang_turns {
                            let item = json!({ "type": "agentMessage", "id": "m1", "text": "pong", "phase": "final_answer" });
                            say.push(
                                json!({ "method": "item/completed", "params": { "threadId": thread, "turnId": "turn-1", "item": item } }),
                            );
                            let done = json!({ "id": "turn-1", "items": [item], "status": "completed" });
                            say.push(json!({ "method": "turn/completed", "params": { "threadId": thread, "turn": done } }));
                        }
                    }
                    _ => {}
                }
                for m in say {
                    if out.write_all(format!("{m}\n").as_bytes()).await.is_err() {
                        return;
                    }
                }
                if dies {
                    let _ = err.write_all(b"fatal: no rollout found for thread id").await;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    return;
                }
            }
        });
        Ok(Proc {
            stdin: Box::new(driver_in),
            stdout: Box::new(driver_out),
            stderr: Some(Box::new(driver_err)),
            child: None,
            contained: None,
        })
    })
}

fn start(dir: &Path, disk: &Shared) -> (Engine, UnboundedReceiver<AppEvent>) {
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

async fn finished(rx: &mut UnboundedReceiver<AppEvent>) -> Message {
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

async fn until(disk: &Shared, ready: impl Fn(&Disk) -> bool) {
    for _ in 0..300 {
        if ready(&disk.lock().unwrap()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the app-server never saw the request");
}

/// A conversation with one finished Codex turn, left on disk the way a closed Faber leaves it.
async fn after_one_turn(dir: &Path, disk: &Shared, text: &str) -> String {
    let (engine, mut rx) = start(dir, disk);
    let id = engine.send(send(None, text, Effort::High)).unwrap();
    finished(&mut rx).await;
    engine.shutdown();
    id
}

/// What an older Faber left after the bug: the only reply failed, and the conversation names a thread that never ran.
fn leave_a_stale_thread(dir: &Path, id: &str, engine: &Engine) {
    let mut saved = engine.open(id).unwrap();
    saved.messages.clear();
    saved.sessions.insert(Vendor::Codex, "never-ran".into());
    std::fs::write(dir.join("conversations").join(format!("{id}.json")), serde_json::to_vec(&saved).unwrap()).unwrap();
}

/// The first message after a warm-up, with a model or effort the warm-up did not use (Auto's choice), is the case the owner hit.
#[tokio::test]
async fn a_warmed_thread_that_never_ran_is_not_resumed_when_the_first_turn_needs_another_launch() {
    let (dir, disk) = (tempfile::tempdir().unwrap(), Shared::default());
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

#[tokio::test]
async fn a_lost_thread_with_no_turns_starts_a_new_one_and_runs_the_message_once() {
    let (dir, disk) = (tempfile::tempdir().unwrap(), Shared::default());
    let id = after_one_turn(dir.path(), &disk, "first").await;
    let (engine, _rx) = start(dir.path(), &disk);
    leave_a_stale_thread(dir.path(), &id, &engine);
    engine.shutdown();
    let (engine, mut rx2) = start(dir.path(), &disk);
    disk.lock().unwrap().turns.clear();
    disk.lock().unwrap().ran.clear();
    engine.send(send(Some(&id), "second", Effort::High)).unwrap();
    let reply = finished(&mut rx2).await;
    assert_eq!((reply.status, reply.text.as_str()), (Some(Status::Done), "pong"), "{:?}", reply.error);
    let d = disk.lock().unwrap();
    assert_eq!(d.asked, vec!["thread/start", "thread/resume never-ran", "thread/start"]);
    assert_eq!(d.turns, vec!["second"], "the message ran once, on the new thread");
    assert_eq!(engine.open(&id).unwrap().sessions[&Vendor::Codex], "thread-2");
    drop(d);
    engine.shutdown();
}

#[tokio::test]
async fn a_lost_thread_that_had_turns_shows_an_error_and_the_next_message_carries_the_whole_history() {
    let (dir, disk) = (tempfile::tempdir().unwrap(), Shared::default());
    // A first message longer than the 20,000 characters the history used to be cut to.
    let first = format!("START {}", "x".repeat(30_000));
    let id = after_one_turn(dir.path(), &disk, &first).await;
    disk.lock().unwrap().ran.clear(); // Codex lost the rollout file
    let (engine, mut rx) = start(dir.path(), &disk);
    engine.send(send(Some(&id), "second", Effort::High)).unwrap();
    let reply = finished(&mut rx).await;
    assert_eq!((reply.status, reply.error.as_deref()), (Some(Status::Error), Some("error.sessionLost")));
    assert_eq!(disk.lock().unwrap().turns.len(), 1, "the message was not run on a thread without the earlier turns");
    assert_eq!(engine.open(&id).unwrap().messages.len(), 4, "the history is kept on screen");
    // The next message starts a new thread and is given the visible history, so nothing is lost silently.
    engine.send(send(Some(&id), "third", Effort::High)).unwrap();
    assert_eq!(finished(&mut rx).await.status, Some(Status::Done));
    let d = disk.lock().unwrap();
    assert_eq!(d.turns.len(), 2);
    assert!(d.turns[1].contains(&first) && d.turns[1].ends_with("third"), "the history was cut: {} characters", d.turns[1].len());
    drop(d);
    engine.shutdown();
}

/// Work already done in the reply (here, partial text before the process died) is never replayed from the request.
#[tokio::test]
async fn a_reply_that_already_holds_work_is_never_replayed_on_a_new_thread() {
    let (dir, disk) = (tempfile::tempdir().unwrap(), Shared::default());
    let id = after_one_turn(dir.path(), &disk, "first").await;
    let (engine, _rx) = start(dir.path(), &disk);
    leave_a_stale_thread(dir.path(), &id, &engine);
    engine.shutdown();
    let (engine, mut rx) = start(dir.path(), &disk);
    {
        let mut d = disk.lock().unwrap();
        d.ran.insert("never-ran".into()); // the thread resumes; its turn streams "partial" and the process dies
        d.die_after_partial = true;
        d.turns.clear();
    }
    engine.send(send(Some(&id), "second", Effort::High)).unwrap();
    let reply = finished(&mut rx).await;
    assert_eq!((reply.status, reply.error.as_deref()), (Some(Status::Error), Some("error.sessionLost")));
    assert_eq!(reply.text, "partial", "what was already answered stays");
    let d = disk.lock().unwrap();
    assert_eq!(d.started, 1, "a new thread replayed the request over the work already done");
    assert_eq!(d.turns, vec!["second"]);
    drop(d);
    engine.shutdown();
}

/// Codex accepted the turn and Faber died before any output: the thread is remembered apart from the resumable ones.
#[tokio::test]
async fn a_crash_after_codex_accepts_a_turn_resumes_that_thread_and_never_starts_a_blank_one() {
    let (dir, disk) = (tempfile::tempdir().unwrap(), Shared::default());
    disk.lock().unwrap().hang_turns = true;
    let (engine, _rx) = start(dir.path(), &disk);
    let id = engine.send(send(None, "first", Effort::High)).unwrap();
    until(&disk, |d| d.turns.len() == 1).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let saved = engine.open(&id).unwrap();
    assert_eq!(saved.tentative.get(&Vendor::Codex).map(String::as_str), Some("thread-1"));
    assert!(saved.sessions.is_empty(), "no output has shown the session was saved");
    let on_disk: Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("conversations").join(format!("{id}.json"))).unwrap()).unwrap();
    assert_eq!(on_disk["tentative"]["codex"], "thread-1", "the tentative id must reach the disk before the crash");
    engine.shutdown();
    // After the restart the same thread continues; its rollout exists because its turn started.
    disk.lock().unwrap().hang_turns = false;
    let (engine, mut rx) = start(dir.path(), &disk);
    engine.send(send(Some(&id), "second", Effort::High)).unwrap();
    let reply = finished(&mut rx).await;
    assert_eq!((reply.status, reply.text.as_str()), (Some(Status::Done), "pong"), "{:?}", reply.error);
    let d = disk.lock().unwrap();
    assert_eq!(d.asked, vec!["thread/start", "thread/resume thread-1"]);
    assert_eq!(d.turns.last().map(String::as_str), Some("second"), "no history text: the thread still has the context");
    drop(d);
    let c = engine.open(&id).unwrap();
    assert_eq!((c.sessions[&Vendor::Codex].as_str(), c.tentative.is_empty()), ("thread-1", true));
    engine.shutdown();
}

#[tokio::test]
async fn a_cut_off_thread_that_cannot_be_resumed_is_reported_not_replaced_silently() {
    let (dir, disk) = (tempfile::tempdir().unwrap(), Shared::default());
    disk.lock().unwrap().hang_turns = true;
    let (engine, _rx) = start(dir.path(), &disk);
    let id = engine.send(send(None, "first", Effort::High)).unwrap();
    until(&disk, |d| d.turns.len() == 1).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    engine.shutdown();
    {
        let mut d = disk.lock().unwrap();
        d.hang_turns = false;
        d.ran.clear(); // the rollout never reached the disk
    }
    let (engine, mut rx) = start(dir.path(), &disk);
    engine.send(send(Some(&id), "second", Effort::High)).unwrap();
    let reply = finished(&mut rx).await;
    assert_eq!((reply.status, reply.error.as_deref()), (Some(Status::Error), Some("error.sessionLost")));
    assert_eq!(disk.lock().unwrap().started, 1, "a blank thread ran a turn whose first attempt Codex had accepted");
    engine.shutdown();
}

/// The fresh launch is tried once: when the same error comes back, the reply fails instead of looping.
#[tokio::test]
async fn the_fresh_launch_runs_at_most_once() {
    let (dir, disk) = (tempfile::tempdir().unwrap(), Shared::default());
    let id = after_one_turn(dir.path(), &disk, "first").await;
    let (engine, _rx) = start(dir.path(), &disk);
    leave_a_stale_thread(dir.path(), &id, &engine);
    engine.shutdown();
    let (engine, mut rx) = start(dir.path(), &disk);
    disk.lock().unwrap().fail_starts = true;
    engine.send(send(Some(&id), "second", Effort::High)).unwrap();
    let reply = finished(&mut rx).await;
    assert_eq!(reply.status, Some(Status::Error));
    assert!(reply.error.as_deref().is_some_and(|e| e.contains("no rollout found")), "{:?}", reply.error);
    assert_eq!(disk.lock().unwrap().asked, vec!["thread/start", "thread/resume never-ran", "thread/start"]);
    engine.shutdown();
}

/// The fresh launch can fail before it reaches the CLI (here, the project is gone): the reply must say so, not stay Streaming.
#[tokio::test]
async fn a_fresh_launch_that_fails_at_once_fails_the_reply() {
    let (dir, disk) = (tempfile::tempdir().unwrap(), Shared::default());
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/test-workspaces");
    std::fs::create_dir_all(&root).unwrap();
    let folder = tempfile::tempdir_in(root).unwrap();
    let (engine, mut rx) = start(dir.path(), &disk);
    let project = engine.add_project(&folder.path().to_string_lossy()).unwrap().id;
    let ask = |text: &str, id: Option<&str>| SendRequest {
        area: Area::Project,
        project_id: Some(project.clone()),
        ..send(id, text, Effort::High)
    };
    let id = engine.send(ask("first", None)).unwrap();
    finished(&mut rx).await;
    leave_a_stale_thread(dir.path(), &id, &engine);
    engine.shutdown();
    let (engine, mut rx) = start(dir.path(), &disk);
    // The project leaves the list behind Faber's back: from the screen a busy project cannot be removed (error.project.busy).
    let list = dir.path().join("projects.json");
    disk.lock().unwrap().on_lost = Some(Arc::new(move || std::fs::write(&list, "[]").expect("the project leaves the list")));
    engine.send(ask("second", Some(&id))).unwrap();
    let reply = finished(&mut rx).await;
    assert_eq!((reply.status, reply.error.as_deref()), (Some(Status::Error), Some("error.projectGone")));
    engine.shutdown();
}

/// Earlier images cannot go as text and files go by reference: the recovered reply says plainly that the history was resent.
#[tokio::test]
async fn recovery_carries_attachment_references_and_tells_the_user_that_images_were_left_out() {
    let (dir, disk) = (tempfile::tempdir().unwrap(), Shared::default());
    let (engine, mut rx) = start(dir.path(), &disk);
    let first = SendRequest {
        images: vec![faber_engine::model::Image { media_type: "image/png".into(), base64: "AAAA".into() }],
        files: vec![faber_engine::workspace::Attachment { name: "notes.pdf".into(), base64: "JVBERi0xLjQ=".into() }],
        ..send(None, "look at these", Effort::High)
    };
    let id = engine.send(first).unwrap();
    assert_eq!(finished(&mut rx).await.note, None, "a normal turn carries no note");
    engine.shutdown();
    disk.lock().unwrap().ran.clear(); // Codex lost the rollout file
    let (engine, mut rx) = start(dir.path(), &disk);
    engine.send(send(Some(&id), "second", Effort::High)).unwrap();
    assert_eq!(finished(&mut rx).await.error.as_deref(), Some("error.sessionLost"));
    engine.send(send(Some(&id), "third", Effort::High)).unwrap();
    let reply = finished(&mut rx).await;
    assert_eq!((reply.status, reply.note.as_deref()), (Some(Status::Done), Some("note.historyResent")), "{:?}", reply.error);
    let d = disk.lock().unwrap();
    let text = d.turns.last().unwrap();
    assert!(text.contains("[attached file: attachments/notes.pdf]"), "{text}");
    assert!(text.contains("[1 image(s) sent here are not included]"), "{text}");
    drop(d);
    engine.shutdown();
}

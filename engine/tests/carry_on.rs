//! A project turn that stops short of the job with no real question for the person is sent on by Faber (`carry_on.rs`).
//! The closing boxes are the agent's own from the bench run of 2026-11-12 (`v11-faber`, an explainer film, Opus 5.5):
//! it stopped twice to ask leave to go on and needed a person to say "continue". The CLI is a fake driven by hand: no
//! process runs.
use crate::support;

use faber_engine::cli::Cli;
use faber_engine::events::EngineEvent;
use faber_engine::model::{Area, Conversation, Role, Status, Vendor};
use faber_engine::proc::Spawner;
use faber_engine::router::Choice;
use faber_engine::signals::Signals;
use faber_engine::{Engine, EngineOptions, SendRequest};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use support::{next_pipe, pipes};

/// The request of the bench run, cut short.
const ASK: &str = "Make the best explainer film you can: a 30-second video that teaches a curious viewer why the sky is blue.";
/// What Faber tells the agent when it sends it on.
const SENT_ON: &str = "Faber sends this, not the person";

fn boot(dir: &support::DataFolder, spawn: Spawner) -> Engine {
    let opts = EngineOptions {
        data_dir: dir.path().into(),
        #[cfg(windows)]
        chat_dir: dir.chat_path().into(),
        clis: BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]),
        spawn,
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    let engine = Engine::new(opts, Arc::new(|_| {})).unwrap();
    // A subscription: Faber never spends a turn of its own on an account billed per use.
    let plan = Signals { billing: Some(faber_engine::budget::Billing::Subscription), ..Default::default() };
    engine.note_account_events(Vendor::Claude, vec![EngineEvent::Signals(plan)]);
    engine
}

fn ask(project: &str) -> SendRequest {
    SendRequest {
        conversation_id: None,
        area: Area::Project,
        project_id: Some(project.into()),
        text: ASK.into(),
        images: vec![],
        files: vec![],
        choice: Choice { vendor: "claude".into(), model: Some("opus".into()), effort: None },
    }
}

fn init() -> Value {
    json!({ "type": "system", "subtype": "init", "session_id": "s", "model": "opus" })
}

fn said(text: &str) -> Value {
    json!({ "type": "assistant", "message": { "role": "assistant", "content": [{ "type": "text", "text": text }] } })
}

/// The agent writes a file: work done in the turn.
async fn writes(pipe: &mut support::Pipe, id: &str, path: &str) {
    pipe.say(json!({ "type": "assistant", "message": { "role": "assistant", "content": [
        { "type": "tool_use", "id": id, "name": "Write", "input": { "file_path": path, "content": "<html></html>" } }] } }))
        .await;
    pipe.say(json!({ "type": "user", "message": { "role": "user", "content": [
        { "type": "tool_result", "tool_use_id": id, "content": "File written", "is_error": false }] } }))
        .await;
}

fn done() -> Value {
    json!({ "type": "result", "subtype": "success", "is_error": false, "total_cost_usd": 0.0, "usage": {} })
}

/// The closing call, as the agent made it.
async fn closes(engine: &Engine, id: &str, pipe: &mut support::Pipe, call: u64, arguments: Value) {
    pipe.say(json!({"type":"control_request","request_id":format!("closing-{call}"),
        "request":{"subtype":"mcp_message","server_name":"faber","message":{
            "jsonrpc":"2.0","id":call,"method":"tools/call","params":{"name":"wrap_up","arguments":arguments}}}}))
        .await;
    until(engine, id, "the closing box arrives", |c| c.messages.last().is_some_and(|m| m.wrap_up.is_some())).await;
}

/// v11's first stop: the audio is done, the film is not, and the only question is whether to go on.
fn checkpoint() -> Value {
    json!({"outcome":"needs_you","did":"The music, sound effects and voice mix are finished and measured; the draft render is in progress.",
        "result":"The finished audio measures -14.0 LUFS with a -1.3 dBTP true peak, and the voice is 15-20 dB above the music.",
        "not_done":"Draft render; independent review, fixes and final render of video.mp4 still to do.",
        "verified":"Loudness and peak measurements from the sound designer.",
        "ask":"Should I continue to the final film?","actions":["Continue","Wait"]})
}

async fn until(engine: &Engine, id: &str, what: &str, ok: impl Fn(&Conversation) -> bool) -> Conversation {
    let mut found = None;
    support::until(what, || {
        let c = engine.open(id).unwrap();
        let is_ok = ok(&c);
        found = Some(c);
        is_ok
    })
    .await;
    found.unwrap()
}

fn ended(c: &Conversation) -> bool {
    c.messages.last().is_some_and(|m| m.role == Role::Assistant && m.status == Some(Status::Done))
}

/// What Faber wrote to the CLI as the person's side of a turn.
fn turns(pipe: &support::Pipe) -> Vec<String> {
    pipe.lines.lock().unwrap().iter().filter(|m| m["type"] == "user").map(|m| m["message"]["content"].to_string()).collect()
}

async fn start(engine: &Engine, made: &Arc<Mutex<Vec<support::Pipe>>>, dir: &support::DataFolder) -> (String, support::Pipe) {
    let project = match engine.add_project(&dir.path().to_string_lossy()) {
        Ok(project) => project.id,
        Err(_) => engine.state().projects[0].id.clone(),
    };
    let id = engine.send(ask(&project)).unwrap();
    let mut pipe = next_pipe(made).await;
    pipe.say(init()).await;
    (id, pipe)
}

#[tokio::test]
async fn a_turn_that_stops_to_ask_leave_to_go_on_is_sent_on_until_the_job_is_done() {
    let dir = support::DataFolder::new();
    let (spawn, made) = pipes();
    let engine = boot(&dir, spawn);
    let (id, mut pipe) = start(&engine, &made, &dir).await;
    writes(&mut pipe, "w1", "index.html").await;
    pipe.say(said("The sound designer is done. Rendering a draft for the independent review now.")).await;
    closes(&engine, &id, &mut pipe, 1, checkpoint()).await;
    pipe.say(done()).await;

    // Faber sends it on instead of leaving "Should I continue?" for the person.
    support::until("Faber sends the turn on", || turns(&pipe).len() == 2).await;
    let sent = turns(&pipe)[1].clone();
    assert!(sent.contains(SENT_ON) && sent.contains("final render of video.mp4"), "{sent}");
    let c = until(&engine, &id, "the turn on is in the conversation", |c| c.messages.len() == 4).await;
    assert_eq!(c.messages[2].wake.as_ref().map(|w| w.why.as_str()), Some("wake.carryOn"), "the screen says who spoke");

    // It goes on, finishes and says so: nothing more is sent.
    writes(&mut pipe, "w2", "video.mp4").await;
    pipe.say(said("The film is rendered and reviewed.")).await;
    closes(
        &engine,
        &id,
        &mut pipe,
        2,
        json!({"outcome":"done","did":"Rendered the film.","result":"video.mp4 is 30 s.","verified":"Watched every frame sheet."}),
    )
    .await;
    pipe.say(done()).await;
    let c = until(&engine, &id, "the job ends", |c| c.messages.len() == 4 && ended(c)).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    engine.shutdown();
    assert_eq!(turns(&pipe).len(), 2, "a finished job is not sent on");
    assert!(!c.messages[3].needs_you);
}

#[tokio::test]
async fn a_box_that_says_done_with_work_left_is_sent_on() {
    let dir = support::DataFolder::new();
    let (spawn, made) = pipes();
    let engine = boot(&dir, spawn);
    let (id, mut pipe) = start(&engine, &made, &dir).await;
    writes(&mut pipe, "w1", "index.html").await;
    pipe.say(said("The scenes are built. When the render finishes I'll review it and fix what it finds.")).await;
    closes(
        &engine,
        &id,
        &mut pipe,
        1,
        json!({"outcome":"done","did":"Built the five scenes.","result":"The composition lints clean.",
        "not_done":"The review of the draft render and the final render.","verified":"hyperframes lint"}),
    )
    .await;
    pipe.say(done()).await;
    support::until("Faber sends the turn on", || turns(&pipe).len() == 2).await;
    engine.shutdown();
    assert!(turns(&pipe)[1].contains("The review of the draft render"));
}

#[tokio::test]
async fn a_real_question_said_again_reaches_the_person() {
    let dir = support::DataFolder::new();
    let (spawn, made) = pipes();
    let engine = boot(&dir, spawn);
    let (id, mut pipe) = start(&engine, &made, &dir).await;
    writes(&mut pipe, "w1", "index.html").await;
    closes(&engine, &id, &mut pipe, 1, checkpoint()).await;
    pipe.say(done()).await;
    support::until("Faber sends the turn on", || turns(&pipe).len() == 2).await;
    // The agent changes nothing and asks again: it meant it, so the person gets the question.
    until(&engine, &id, "the turn on runs", |c| c.messages.len() == 4).await;
    pipe.say(said("I looked again: the question stands.")).await;
    closes(&engine, &id, &mut pipe, 2, checkpoint()).await;
    pipe.say(done()).await;
    let c = until(&engine, &id, "the question waits", |c| c.messages.len() == 4 && ended(c)).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(turns(&pipe).len(), 2, "one look again, never a loop");
    assert!(engine.open(&id).unwrap().messages.last().unwrap().needs_you, "{:?}", c.messages.last().map(|m| &m.wrap_up));
    engine.shutdown();
}

#[tokio::test]
async fn a_pure_decision_is_never_sent_on() {
    let dir = support::DataFolder::new();
    let (spawn, made) = pipes();
    let engine = boot(&dir, spawn);
    // A decision only the person can make, with nothing left undone, goes to them at once.
    let (id, mut pipe) = start(&engine, &made, &dir).await;
    writes(&mut pipe, "w1", "index.html").await;
    closes(
        &engine,
        &id,
        &mut pipe,
        1,
        json!({"outcome":"needs_you","did":"Drafted two voices.","result":"Both are in work/voices.",
        "verified":"Listened to both.","ask":"Which narrator voice do you want?","actions":["Warm","Bright"]}),
    )
    .await;
    pipe.say(done()).await;
    until(&engine, &id, "the question waits", ended).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    engine.shutdown();
    assert_eq!(turns(&pipe).len(), 1, "a real choice is the person's");
}

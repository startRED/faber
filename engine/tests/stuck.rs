//! R161 step 6: a subagent that seems stuck shows on its step, and only when it matters. The recorded subagent turns must
//! never show one (no false amber), and a subagent driven by hand shows each sign, waits for the person while its question
//! is open, and leaves no sign behind once its reply ends.
use crate::support;

use faber_engine::cli::Cli;
use faber_engine::model::{Area, Message, Permissions, RunState, Settings, Vendor};
use faber_engine::proc::Spawner;
use faber_engine::router::Choice;
use faber_engine::stuck::{QUIET, Sign};
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use support::{fake, fixture, next_pipe, pipes};
use tokio::sync::mpsc::UnboundedReceiver;

fn engine(dir: &Path, vendor: Vendor, spawn: Spawner) -> (Engine, UnboundedReceiver<AppEvent>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let opts = EngineOptions {
        data_dir: dir.into(),
        clis: BTreeMap::from([(vendor, Cli { command: vendor.name().into(), prefix: vec![] })]),
        spawn,
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e))));
    engine.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
    (engine, rx)
}

fn send(engine: &Engine, vendor: &str, model: &str, text: &str) -> String {
    let choice = Choice { vendor: vendor.into(), model: Some(model.into()), effort: None };
    engine
        .send(SendRequest {
            conversation_id: None,
            area: Area::Chat,
            project_id: None,
            text: text.into(),
            images: vec![],
            files: vec![],
            choice,
        })
        .unwrap()
}

/// Every sign any subagent call carried in the replies the screen was sent, in order, without repeats.
fn signs(rx: &mut UnboundedReceiver<AppEvent>, seen: &mut Vec<Sign>) {
    while let Ok(event) = rx.try_recv() {
        if let AppEvent::Message { message, .. } = event {
            for sign in message.tools.iter().flatten().filter_map(|t| t.sign) {
                if seen.last() != Some(&sign) {
                    seen.push(sign);
                }
            }
        }
    }
}

async fn reply_until(engine: &Engine, id: &str, what: &str, done: impl Fn(&Message) -> bool) -> Message {
    for _ in 0..300 {
        if let Some(m) = engine.open(id).and_then(|c| c.messages.last().cloned()).filter(|m| done(m)) {
            return m;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("{what}: {:?}", engine.open(id).and_then(|c| c.messages.last().cloned()));
}

fn call_sign(m: &Message) -> Option<Sign> {
    m.tools.iter().flatten().find(|t| t.id == "call").and_then(|t| t.sign)
}

/// The recorded turns with a subagent or a helper (Claude Code and Codex): none of them ever shows a stuck sign. The Codex
/// subagent asks for a command in Ask mode, so its step reads "waiting for you" while the card is open.
#[tokio::test]
async fn r161_no_recorded_subagent_turn_ever_looks_stuck() {
    const CLAUDE: &str = "Use the Agent tool exactly once (subagent_type Explore) to find which file in this folder defines load_config. Then answer in one short sentence.";
    const CODEX: &str = "Spawn exactly one subagent named copier. Its task: create the file c.txt containing the word gamma using a shell command, then reply with the word done. Wait for it, then answer with its reply. Do not do the work yourself.";
    let cases = [
        ("claude", "subagent", "haiku", CLAUDE),
        ("claude", "subagent-background", "haiku", "Start a background helper to audit the docs, then tell me it is running."),
        ("codex", "subagent", "gpt-6-luna", CODEX),
    ];
    for (vendor, name, model, text) in cases {
        let dir = tempfile::tempdir().unwrap();
        let f = fake(fixture(vendor, name));
        let v = if vendor == "claude" { Vendor::Claude } else { Vendor::Codex };
        let (engine, mut rx) = engine(dir.path(), v, f.spawn.clone());
        let id = send(&engine, vendor, model, text);
        let (mut seen, mut answered) = (vec![], vec![]);
        for _ in 0..500 {
            signs(&mut rx, &mut seen);
            let c = engine.open(&id).unwrap();
            let reply = c.messages.last().unwrap();
            let new: Vec<String> =
                reply.permissions.iter().flatten().map(|p| p.request_id.clone()).filter(|r| !answered.contains(r)).collect();
            for request in new {
                // A replayed server never confirms the answer, so it reads as unconfirmed; the recording checks it arrived.
                drop(engine.answer(&id, &reply.id, &request, true, false).await);
                answered.push(request);
            }
            if reply.status != Some(faber_engine::model::Status::Streaming) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        signs(&mut rx, &mut seen);
        engine.shutdown();
        assert!(
            f.mismatches.lock().unwrap().iter().all(|m| m.contains("turn/interrupt")),
            "{vendor} {name}: {:?}",
            f.mismatches.lock().unwrap()
        );
        assert!(seen.iter().all(|s| !s.stuck()), "{vendor} {name}: {seen:?}");
        let expected: &[Sign] = if (vendor, name) == ("codex", "subagent") { &[Sign::Waiting] } else { &[] };
        assert_eq!(seen, expected, "{vendor} {name}");
        let reply = engine.open(&id).unwrap().messages.pop().unwrap();
        assert!(reply.tools.iter().flatten().all(|t| t.sign.is_none()), "no sign outlives its reply: {reply:?}");
    }
}

fn step(id: &str, name: &str, input: Value, parent: Option<&str>) -> Value {
    json!({ "type": "assistant", "parent_tool_use_id": parent,
        "message": { "role": "assistant", "content": [{ "type": "tool_use", "id": id, "name": name, "input": input }] } })
}

fn result(id: &str, ok: bool, parent: Option<&str>) -> Value {
    json!({ "type": "user", "parent_tool_use_id": parent,
        "message": { "role": "user", "content": [{ "type": "tool_result", "tool_use_id": id, "content": if ok { "ok" } else { "1 test failed" }, "is_error": !ok }] } })
}

/// A subagent that runs the same failing test three times with nothing changed is stuck, and so is one that goes quiet; an
/// edit and a passing run are work again, its question makes it wait for the person, and the end of the reply clears it.
#[tokio::test]
async fn r161_a_stuck_subagent_shows_on_its_step_and_needs_the_person() {
    let dir = tempfile::tempdir().unwrap();
    let (spawn, made) = pipes();
    let (engine, _rx) = engine(dir.path(), Vendor::Claude, spawn);
    let clock = Arc::new(AtomicU64::new(1_000_000));
    let time = clock.clone();
    engine.set_time_source(Arc::new(move || time.load(Ordering::SeqCst) as f64));
    let id = send(&engine, "claude", "haiku", "fix the tests with a subagent");
    let mut pipe = next_pipe(&made).await;
    pipe.say(json!({ "type": "system", "subtype": "init", "session_id": "s", "model": "haiku" })).await;
    pipe.say(step("call", "Agent", json!({ "description": "Fix the tests", "prompt": "make cargo test pass" }), None)).await;
    let test = json!({ "command": "cargo test" });
    for n in 1..=2 {
        pipe.say(step(&format!("t{n}"), "Bash", test.clone(), Some("call"))).await;
        pipe.say(result(&format!("t{n}"), false, Some("call"))).await;
    }
    let reply = reply_until(&engine, &id, "two failures", |m| m.tools.as_ref().is_some_and(|t| t.len() == 3 && t[2].ok.is_some())).await;
    assert_eq!(call_sign(&reply), None, "a failure and its retry are the work going on");
    pipe.say(step("t3", "Bash", test.clone(), Some("call"))).await;
    pipe.say(result("t3", false, Some("call"))).await;
    let reply = reply_until(&engine, &id, "the third failure", |m| call_sign(m) == Some(Sign::Failing)).await;
    let item = engine.state().running.items.into_iter().find(|i| i.conversation_id == id).unwrap();
    assert_eq!((item.state, item.stuck), (RunState::NeedsYou, vec!["call".to_string()]), "a stuck subagent needs the person");
    assert!(reply.permissions.is_none());

    pipe.say(step("e1", "Edit", json!({ "file_path": "src/lib.rs", "old_string": "a", "new_string": "b" }), Some("call"))).await;
    pipe.say(result("e1", true, Some("call"))).await;
    reply_until(&engine, &id, "an edit is work", |m| {
        m.tools.as_ref().is_some_and(|t| t.len() == 5 && t[4].ok.is_some()) && call_sign(m).is_none()
    })
    .await;
    let item = engine.state().running.items.into_iter().find(|i| i.conversation_id == id).unwrap();
    assert_eq!((item.state, item.stuck), (RunState::Working, vec![]));

    // Ten quiet minutes while nothing runs.
    clock.fetch_add(QUIET as u64 + 1, Ordering::SeqCst);
    engine.weigh_subagents();
    reply_until(&engine, &id, "silence", |m| call_sign(m) == Some(Sign::Silent)).await;

    // Its own question: it waits for the person, and the time it waits counts for nothing.
    let install = json!({ "command": "npm install" });
    pipe.say(step("t4", "Bash", install.clone(), Some("call"))).await;
    pipe.say(json!({ "type": "control_request", "request_id": "q1", "request": { "subtype": "can_use_tool", "tool_name": "Bash", "input": install } })).await;
    let reply = reply_until(&engine, &id, "waiting", |m| call_sign(m) == Some(Sign::Waiting)).await;
    clock.fetch_add(3 * QUIET as u64, Ordering::SeqCst);
    engine.weigh_subagents();
    assert_eq!(call_sign(&engine.open(&id).unwrap().messages.pop().unwrap()), Some(Sign::Waiting));
    let item = engine.state().running.items.into_iter().find(|i| i.conversation_id == id).unwrap();
    assert!(item.stuck.is_empty() && item.state == RunState::NeedsYou, "the question needs the person, not a stuck step: {item:?}");
    engine.answer(&id, &reply.id, "q1", true, false).await.unwrap();
    reply_until(&engine, &id, "answered", |m| call_sign(m).is_none()).await;

    // The subagent ends, then the reply: no sign is left anywhere.
    pipe.say(result("t4", true, Some("call"))).await;
    pipe.say(result("call", true, None)).await;
    pipe.say(json!({ "type": "result", "subtype": "success", "is_error": false, "total_cost_usd": 0.0, "usage": {} })).await;
    let reply = reply_until(&engine, &id, "the end", |m| m.status != Some(faber_engine::model::Status::Streaming)).await;
    engine.shutdown();
    assert!(reply.tools.iter().flatten().all(|t| t.sign.is_none()), "{reply:?}");
    // R161 step 4: its call keeps the facts of what it did, not a verdict.
    let facts = reply.tools.iter().flatten().find(|t| t.id == "call").and_then(|t| t.facts.clone()).expect("the ended call has facts");
    assert_eq!(facts.edited, ["src/lib.rs"]);
    assert_eq!((facts.checks, facts.failed), (3, 3), "three cargo test runs, all failed; npm install is no check");
    assert_eq!(facts.unnamed, ["src/lib.rs"], "the brief never named the file it changed");
}

/// The same step started six times in a row is a loop; the sign goes away when the subagent does something else.
#[tokio::test]
async fn r161_a_subagent_that_repeats_one_step_is_stuck_until_it_does_another() {
    let dir = tempfile::tempdir().unwrap();
    let (spawn, made) = pipes();
    let (engine, _rx) = engine(dir.path(), Vendor::Claude, spawn);
    let id = send(&engine, "claude", "haiku", "look around with a subagent");
    let mut pipe = next_pipe(&made).await;
    pipe.say(json!({ "type": "system", "subtype": "init", "session_id": "s", "model": "haiku" })).await;
    pipe.say(step("call", "Agent", json!({ "description": "Look around", "prompt": "read the config" }), None)).await;
    for n in 1..=6 {
        pipe.say(step(&format!("r{n}"), "Read", json!({ "file_path": "config.toml" }), Some("call"))).await;
        pipe.say(result(&format!("r{n}"), true, Some("call"))).await;
    }
    reply_until(&engine, &id, "a loop", |m| call_sign(m) == Some(Sign::Repeat)).await;
    pipe.say(step("g", "Grep", json!({ "pattern": "port" }), Some("call"))).await;
    reply_until(&engine, &id, "another step", |m| m.tools.as_ref().is_some_and(|t| t.len() == 8) && call_sign(m).is_none()).await;
    engine.shutdown();
}

//! Auxiliary work through the real drivers and in-memory fake CLIs, never a model service.
use crate::support::{self, fake, fixture};
use faber_engine::cli::Cli;
use faber_engine::events::EngineEvent;
use faber_engine::model::{Area, Vendor};
use faber_engine::proc::Spawner;
use faber_engine::router::Choice;
use faber_engine::{Engine, EngineOptions, SendRequest, WarmRequest};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

const QUESTION: &str = "In one sentence: why is the sky blue?";

fn engine(dir: &support::DataFolder, spawn: Spawner) -> Engine {
    let engine = Engine::new(
        EngineOptions {
            data_dir: dir.path().into(),
            #[cfg(windows)]
            chat_dir: dir.chat_path().into(),
            clis: BTreeMap::from([(Vendor::Codex, Cli { command: "fake".into(), prefix: vec![] })]),
            spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 3,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    )
    .unwrap();
    engine.note_account_events(
        Vendor::Codex,
        vec![EngineEvent::Signals(faber_engine::signals::Signals {
            billing: Some(faber_engine::budget::Billing::Subscription),
            ..Default::default()
        })],
    );
    engine.set_codex_models(vec!["gpt-6-luna".into(), "gpt-6.1-sol".into()], Some("gpt-6.1-sol".into()));
    engine
}

fn choice() -> Choice {
    Choice { vendor: "auto".into(), model: None, effort: None }
}

fn send(engine: &Engine) -> String {
    engine
        .send(SendRequest {
            conversation_id: None,
            area: Area::Chat,
            project_id: None,
            text: QUESTION.into(),
            images: vec![],
            files: vec![],
            choice: choice(),
        })
        .unwrap()
}

#[tokio::test]
async fn router_cold_send_starts_at_once_without_an_auxiliary_request() {
    let dir = support::DataFolder::new();
    let f = fake(fixture("codex", "chat"));
    let engine = engine(&dir, f.spawn.clone());
    let start = Instant::now();
    let id = send(&engine);
    engine.rename(&id, "Router test");
    support::until("the default never started", || engine.open(&id).unwrap().messages[1].model.is_some()).await;
    assert!(start.elapsed() < Duration::from_millis(500), "a cold router must not hold the first send: {:?}", start.elapsed());
    let reply = engine.open(&id).unwrap().messages[1].clone();
    assert_eq!(reply.acts[0].why.as_deref(), Some("reason.autoImmediate"));
    assert!(!f.wire.messages().iter().any(|m| m["params"]["baseInstructions"].as_str().is_some_and(|s| s.contains("You classify"))));
    assert!(engine.ledger().codex.unmetered.is_empty());
    engine.shutdown();
}

#[tokio::test]
async fn router_pending_draft_does_not_hold_the_send() {
    let dir = support::DataFolder::new();
    let mut recording = fixture("codex", "chat");
    recording.insert(0, support::Entry { input: false, msg: serde_json::json!({"faber_hold": 3200}) });
    let f = fake(recording);
    let engine = engine(&dir, f.spawn.clone());
    engine.route_draft(WarmRequest { conversation_id: None, area: Area::Chat, project_id: None, choice: choice() }, QUESTION.into());
    let start = Instant::now();
    let id = send(&engine);
    engine.rename(&id, "Router test");
    support::until("the pending draft held the send", || engine.open(&id).unwrap().messages[1].model.is_some()).await;
    assert!(start.elapsed() < Duration::from_millis(500), "{:?}", start.elapsed());
    assert_eq!(engine.open(&id).unwrap().messages[1].acts[0].why.as_deref(), Some("reason.autoImmediate"));
    engine.shutdown();
}

#[tokio::test]
async fn router_draft_can_finish_after_the_old_send_deadline() {
    let dir = support::DataFolder::new();
    let verdict = r#"{"kind":"chat","difficulty":"easy","check":"glance","confidence":0.95}"#;
    let mut recording = fixture("codex", "chat");
    let end = recording.iter().position(|e| e.msg["method"] == "turn/completed").unwrap();
    recording.truncate(end + 1);
    let mut first = true;
    for entry in &mut recording {
        if entry.msg["method"] == "item/agentMessage/delta" {
            entry.msg["params"]["delta"] = if std::mem::take(&mut first) { verdict } else { "" }.into();
        }
        if entry.msg["method"] == "item/completed" && entry.msg["params"]["item"]["type"] == "agentMessage" {
            entry.msg["params"]["item"]["text"] = verdict.into();
        }
    }
    let at = recording.iter().position(|e| e.msg["method"] == "item/agentMessage/delta").unwrap();
    recording.insert(at, support::Entry { input: false, msg: serde_json::json!({"faber_hold": 3200}) });
    let router = fake(recording);
    let answer = fake(fixture("codex", "chat"));
    let (r, a) = (router.spawn.clone(), answer.spawn.clone());
    let engine = engine(
        &dir,
        Arc::new(move |bin, args, cwd| if args.iter().any(|a| a == "mcp_servers={}") { r(bin, args, cwd) } else { a(bin, args, cwd) }),
    );
    engine.route_draft(WarmRequest { conversation_id: None, area: Area::Chat, project_id: None, choice: choice() }, QUESTION.into());
    support::until("the draft question was not sent", || router.wire.lines_with("turn/start") == 1).await;
    tokio::time::sleep(Duration::from_millis(3500)).await;
    let id = send(&engine);
    engine.rename(&id, "Router test");
    support::until("the routed draft never started", || engine.open(&id).unwrap().messages[1].model.is_some()).await;
    let reply = engine.open(&id).unwrap().messages[1].clone();
    assert_eq!(reply.reason.as_deref(), Some("reason.route.chat.easy"));
    assert_eq!(reply.model.as_deref(), Some("gpt-6-luna"));
    assert!(engine.ledger().codex.unmetered.is_empty());
    assert!(router.mismatches.lock().unwrap().is_empty(), "{:?}", router.mismatches.lock().unwrap());
    engine.shutdown();
}

//! What the engine hands to the learning: the span of every request Faber makes, and the weights learned from them.

use crate::support;

use faber_engine::budget::Billing;
use faber_engine::cli::Cli;
use faber_engine::engine::Ask;
use faber_engine::events::EngineEvent;
use faber_engine::model::{Area, Message, Role, Status, Vendor};
use faber_engine::proc::Spawner;
use faber_engine::route::question;
use faber_engine::router::Choice;
use faber_engine::signals::Signals;
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest, WarmRequest};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use support::{Entry, fake, fixture};
use tokio::sync::mpsc::UnboundedReceiver;

const SKY: &str = "In one sentence: why is the sky blue?";
/// The recorded five-hour window resets at this time.
const RESETS: u64 = 1_791_627_000;
const START: u64 = 1_791_620_000;

/// A clock that moves one second every time it is read, from a base the test sets: a turn then takes time, as it does.
struct Ticking {
    base: AtomicU64,
    ticks: AtomicU64,
}

fn start(dir: &crate::support::DataFolder, spawn: Spawner, ask: Option<Ask>) -> (Engine, UnboundedReceiver<AppEvent>, Arc<Ticking>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
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
        ask,
    };
    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e)))).unwrap();
    let clock = Arc::new(Ticking { base: AtomicU64::new(START), ticks: AtomicU64::new(0) });
    let c = clock.clone();
    engine.set_time_source(Arc::new(move || (c.base.load(Ordering::SeqCst) + c.ticks.fetch_add(1, Ordering::SeqCst)) as f64));
    (engine, rx, clock)
}

fn by_hand() -> SendRequest {
    SendRequest {
        conversation_id: None,
        area: Area::Chat,
        project_id: None,
        text: SKY.into(),
        images: vec![],
        files: vec![],
        choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
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

fn reading(used: f64) -> EngineEvent {
    EngineEvent::Quota { window: "five_hour".into(), used, resets_at: RESETS as f64 }
}

#[tokio::test]
async fn a_user_turn_leaves_its_span_with_the_model_and_the_cost() {
    let dir = support::DataFolder::new();
    let (engine, mut rx, _) = start(&dir, fake(fixture("claude", "chat")).spawn, None);
    engine.send(by_hand()).unwrap();
    let reply = finished(&mut rx).await;
    let spans = engine.ledger().claude.spans;
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].model, "claude-haiku-5-5");
    // The ledger keeps a millionth of a dollar.
    let (kept, shown) = (spans[0].cost.unwrap(), reply.cost_usd.unwrap());
    assert!((kept - shown).abs() < 1e-6, "the cost of the turn, not the running total: {kept} vs {shown}");
    assert!(spans[0].cost.is_some_and(|c| c > 0.0));
    assert!(spans[0].start >= START as f64 && spans[0].end > spans[0].start, "{spans:?}");
    engine.shutdown();
}

#[tokio::test]
async fn eight_measured_turns_teach_the_weights_and_the_stretches_are_kept() {
    let dir = support::DataFolder::new();
    let (engine, mut rx, clock) = start(&dir, fake(fixture("claude", "chat")).spawn, None);
    for i in 0..8 {
        clock.base.store(START + i * 600, Ordering::SeqCst);
        clock.ticks.store(0, Ordering::SeqCst);
        // Three points below what the recording will report, so each turn moves the window by three points.
        engine.note_account_events(Vendor::Claude, vec![reading(0.04)]);
        engine.send(by_hand()).unwrap();
        finished(&mut rx).await;
    }
    let claude = engine.ledger().claude;
    assert_eq!(claude.spans.len(), 8);
    assert_eq!(claude.stretches.len(), 8);
    assert_eq!(claude.stretches_since, 8);
    assert!(claude.weights.capacity.is_some_and(|c| c > 0.0), "{:?}", claude.weights);
    assert!(claude.weights.w.contains_key("claude-haiku-5-5"));
    engine.shutdown();
    let saved: faber_engine::ledger::Ledger =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("budget.json")).unwrap()).unwrap();
    assert_eq!(saved.claude.stretches.len(), 8, "what was learned survives a restart");
}

#[tokio::test]
async fn a_router_question_is_a_span_too_and_has_its_own_cost() {
    let dir = support::DataFolder::new();
    // The router's session is the Claude chat recording, with the question it will be asked as the recorded input.
    let text = "Plan a trip to Lisbon";
    let entries: Vec<Entry> = fixture("claude", "chat")
        .into_iter()
        .map(|mut e| {
            if e.input {
                e.msg["message"]["content"] = json!(question(text, 0));
            }
            e
        })
        .collect();
    let f = fake(entries);
    let (engine, _rx, _) = start(&dir, f.spawn.clone(), None);
    // A fresh reading and a seen plan, so that no pulse is due and the gate lets the router ask.
    let plan = EngineEvent::Signals(Signals { plan: Some("max".into()), billing: Some(Billing::Subscription), ..Signals::default() });
    engine.note_account_events(Vendor::Claude, vec![reading(0.05), plan]);
    let typing = WarmRequest {
        conversation_id: None,
        area: Area::Chat,
        project_id: None,
        choice: Choice { vendor: "auto".into(), model: None, effort: None },
    };
    engine.route_draft(typing, text.into());
    support::until("the question never produced a span", || !engine.ledger().claude.spans.is_empty()).await;
    let spans = engine.ledger().claude.spans;
    assert_eq!(spans.len(), 1, "one question, one span");
    assert!(spans[0].cost.is_some_and(|cost| cost > 0.0));
    assert_eq!(f.args.lock().unwrap().len(), 1, "no pulse: the reading was fresh");
    engine.shutdown();
}

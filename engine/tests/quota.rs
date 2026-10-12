mod support;

use faber_engine::budget::{Billing, Reach};
use faber_engine::cli::Cli;
use faber_engine::codex::read_codex_limits;
use faber_engine::events::EngineEvent;
use faber_engine::ledger::Ledger;
use faber_engine::model::{Area, Message, Role, Status, Vendor};
use faber_engine::proc::Spawner;
use faber_engine::router::Choice;
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use support::{Entry, fake, fixture};
use tokio::sync::mpsc::UnboundedReceiver;

const SKY: &str = "In one sentence: why is the sky blue?";
/// A moment after the recordings were made and before their windows reset (the Claude five-hour window resets at
/// 1791627000).
const WHEN: f64 = 1_791_620_000.0;

fn start(dir: &Path, vendor: Vendor, spawn: Spawner) -> (Engine, UnboundedReceiver<AppEvent>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let opts = EngineOptions {
        data_dir: dir.into(),
        clis: BTreeMap::from([(vendor, Cli { command: vendor.name().into(), prefix: vec![] })]),
        spawn,
        windows: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e))));
    engine.set_time_source(Arc::new(|| WHEN));
    (engine, rx)
}

fn ask(vendor: &str) -> SendRequest {
    SendRequest {
        conversation_id: None,
        area: Area::Chat,
        project_id: None,
        text: SKY.into(),
        images: vec![],
        files: vec![],
        choice: Choice { vendor: vendor.into(), model: (vendor == "claude").then(|| "haiku".into()) },
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

#[tokio::test]
async fn a_claude_reading_reaches_the_ledger_is_saved_and_is_still_there_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, mut rx) = start(dir.path(), Vendor::Claude, fake(fixture("claude", "chat")).spawn);
    engine.send(ask("claude")).unwrap();
    finished(&mut rx).await;
    let claude = engine.ledger().claude;
    assert_eq!(claude.readings["five_hour"].used, 0.07);
    assert_eq!(claude.readings["seven_day"].resets_at, 1_791_878_400.0);
    assert_eq!(claude.reach, Reach::Clear);
    assert_eq!(claude.paid_fallback, Some(false), "the recording says extra usage is rejected");
    assert!(!claude.overage_active);
    engine.shutdown();

    let saved: Ledger = serde_json::from_str(&std::fs::read_to_string(dir.path().join("budget.json")).unwrap()).unwrap();
    assert_eq!(saved.claude.readings["five_hour"].used, 0.07);
    assert_eq!(saved.claude.billing, Billing::Unknown, "what is true only now is not written");

    let (again, _rx) = start(dir.path(), Vendor::Claude, fake(fixture("claude", "chat")).spawn);
    let state = again.state();
    let windows: Vec<&str> = state.quotas.iter().map(|q| q.window.as_str()).collect();
    assert_eq!(windows, ["five_hour", "seven_day"], "Settings has numbers before the first answer");
}

#[tokio::test]
async fn a_codex_session_tells_the_ledger_the_plan_and_that_it_is_a_subscription() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, mut rx) = start(dir.path(), Vendor::Codex, fake(fixture("codex", "chat")).spawn);
    engine.send(ask("codex")).unwrap();
    finished(&mut rx).await;
    let codex = engine.ledger().codex;
    assert_eq!(codex.plan.as_deref(), Some("pro"));
    assert_eq!(codex.billing, Billing::Subscription);
    assert_eq!(codex.readings["seven_day"].used, 0.47);
    assert!(codex.plan_ok(WHEN), "a plan seen a moment ago");
    assert_eq!(codex.paid_fallback, Some(false), "no credits");
    engine.shutdown();
}

#[tokio::test]
async fn a_budget_file_that_is_damaged_old_or_odd_never_stops_the_app() {
    for (name, content) in [
        ("garbage", "not json at all"),
        ("empty", ""),
        ("from an older version", "{}"),
        ("wrong types", r#"{"claude": {"readings": 5}}"#),
        (
            "nonsense inside",
            r#"{"claude": {"readings": {"five_hour": {"used": -3.0, "resets_at": 1791627000.0, "read_at": 1.0}},
                "learned": {"mass": [1.0], "covered": 5.0}, "weights": {"w": {"a": 99.0}, "capacity": null}}}"#,
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("budget.json"), content).unwrap();
        let (engine, mut rx) = start(dir.path(), Vendor::Claude, fake(fixture("claude", "chat")).spawn);
        assert!(engine.ledger().claude.readings.is_empty(), "{name}: nothing believable to start from");
        assert!(engine.state().quotas.is_empty(), "{name}");
        engine.send(ask("claude")).unwrap();
        assert_eq!(finished(&mut rx).await.status, Some(Status::Done), "{name}");
        assert_eq!(engine.ledger().claude.readings["five_hour"].used, 0.07, "{name}: the next reading is taken as usual");
        engine.shutdown();
    }
}

#[tokio::test]
async fn a_reading_that_cannot_be_real_is_neither_shown_nor_remembered() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, _rx) = start(dir.path(), Vendor::Claude, fake(vec![]).spawn);
    let quota = |window: &str, used: f64, resets_at: f64| EngineEvent::Quota { window: window.into(), used, resets_at };
    engine.note_account_events(
        Vendor::Claude,
        vec![
            quota("five_hour", 7.0, 1_791_627_000.0),
            quota("five_hour", -0.2, 1_791_627_000.0),
            quota("seven_day", 0.3, 0.0),
            quota("weird window", 0.3, 1_791_627_000.0),
        ],
    );
    let ledger = engine.ledger().claude;
    assert_eq!(ledger.readings.keys().collect::<Vec<_>>(), ["weird window"], "an odd name is kept, and prices as no information");
    assert_eq!(ledger.snap(WHEN).windows[0].1.length, 0.0);
    assert_eq!(engine.state().quotas.len(), 1);
}

#[tokio::test]
async fn limits_are_read_without_a_turn_and_the_account_is_not_kept() {
    let entries = vec![
        Entry {
            input: true,
            msg: json!({ "id": 1, "method": "initialize", "params": { "clientInfo": { "name": "faber", "title": "Faber", "version": "0.0.0" } } }),
        },
        Entry { input: false, msg: json!({ "id": 1, "result": {} }) },
        Entry { input: true, msg: json!({ "method": "initialized" }) },
        Entry { input: true, msg: json!({ "id": 2, "method": "account/rateLimits/read" }) },
        Entry { input: true, msg: json!({ "id": 3, "method": "account/read", "params": { "refreshToken": false } }) },
        Entry {
            input: false,
            msg: json!({ "id": 3, "result": { "account": { "type": "chatgpt", "email": "someone@example.com", "planType": "plus" }, "requiresOpenaiAuth": true } }),
        },
        Entry {
            input: false,
            msg: json!({ "id": 2, "result": { "rateLimits": {
                "primary": { "usedPercent": 12, "windowDurationMins": 300, "resetsAt": 1791627000 },
                "secondary": { "usedPercent": 30, "windowDurationMins": 10080, "resetsAt": 1791878400 },
                "credits": { "hasCredits": false, "unlimited": false, "balance": "0" }, "planType": "plus", "rateLimitReachedType": null } } }),
        },
    ];
    let f = fake(entries);
    let events = read_codex_limits("codex", ".".as_ref(), false, f.spawn.clone(), Duration::from_secs(3)).await;
    assert_eq!(*f.mismatches.lock().unwrap(), Vec::<String>::new());
    let windows: Vec<(&str, f64)> = events
        .iter()
        .filter_map(|e| if let EngineEvent::Quota { window, used, .. } = e { Some((window.as_str(), *used)) } else { None })
        .collect();
    assert_eq!(windows, [("five_hour", 0.12), ("seven_day", 0.3)]);
    let plan = events.iter().find_map(|e| if let EngineEvent::Signals(s) = e { s.billing } else { None });
    assert_eq!(plan, Some(Billing::Subscription));
    assert!(!format!("{events:?}").contains("someone@example.com"), "the email is dropped unread");

    let dir = tempfile::tempdir().unwrap();
    let (engine, _rx) = start(dir.path(), Vendor::Codex, fake(vec![]).spawn);
    engine.note_account_events(Vendor::Codex, events);
    let codex = engine.ledger().codex;
    assert_eq!((codex.plan.as_deref(), codex.billing), (Some("plus"), Billing::Subscription));
    assert_eq!(codex.readings.len(), 2);
}

#[tokio::test]
async fn a_cli_that_never_answers_gives_no_limits() {
    let f = fake(vec![]);
    let events = read_codex_limits("codex", ".".as_ref(), false, f.spawn.clone(), Duration::from_millis(50)).await;
    assert!(events.is_empty());
}

#[tokio::test]
async fn an_account_update_with_no_billing_clears_an_earlier_subscription_verdict() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, _rx) = start(dir.path(), Vendor::Codex, fake(fixture("codex", "chat")).spawn);
    let mut state = faber_engine::codex::CodexTurnState::default();
    for mode in [json!("chatgpt"), serde_json::Value::Null] {
        let events =
            faber_engine::codex::parse_codex_notification(&json!({"method":"account/updated","params":{"authMode":mode}}), &mut state);
        engine.note_account_events(Vendor::Codex, events);
        assert_eq!(engine.ledger().codex.billing, if mode.is_null() { Billing::Unknown } else { Billing::Subscription });
    }
}

#[test]
fn incomplete_window_payloads_do_not_fabricate_an_empty_quota() {
    let claude = faber_engine::claude::parse_claude_message(
        &json!({"type":"rate_limit_event","rate_limit_info":{"unifiedWindows":{"five_hour":{"resetsAt":1791627000}}}}),
    );
    let codex = faber_engine::codex::parse_codex_notification(
        &json!({"method":"account/rateLimits/updated","params":{"rateLimits":{"primary":{"windowDurationMins":300,"resetsAt":1791627000}}}}),
        &mut faber_engine::codex::CodexTurnState::default(),
    );
    for events in [claude, codex] {
        assert!(!events.iter().any(|e| matches!(e, EngineEvent::Quota { .. })));
    }
}

#[tokio::test]
async fn an_account_read_error_returns_no_replacement_evidence() {
    let entries = vec![
        Entry { input: true, msg: json!({"id":1,"method":"initialize"}) },
        Entry { input: false, msg: json!({"id":1,"result":{}}) },
        Entry { input: true, msg: json!({"method":"initialized"}) },
        Entry { input: true, msg: json!({"id":2,"method":"account/rateLimits/read"}) },
        Entry { input: true, msg: json!({"id":3,"method":"account/read"}) },
        Entry { input: false, msg: json!({"id":2,"error":{"code":-1,"message":"unavailable"}}) },
        Entry { input: false, msg: json!({"id":3,"result":{"account":{"type":"chatgpt","planType":"pro"}}}) },
    ];
    let f = fake(entries);
    let dir = tempfile::tempdir().unwrap();
    assert!(read_codex_limits("codex", dir.path(), false, f.spawn, Duration::from_secs(1)).await.is_empty());
}

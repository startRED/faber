//! Plan detection, the billing verdict and the quota pulse. No real CLI runs: status commands and the pulse are replayed.

mod support;

use faber_engine::budget::Billing;
use faber_engine::cli::Cli;
use faber_engine::engine::Ask;
use faber_engine::events::EngineEvent;
use faber_engine::model::{Area, Vendor};
use faber_engine::proc::{Proc, Spawner};
use faber_engine::router::Choice;
use faber_engine::signals::Signals;
use faber_engine::{AppEvent, Engine, EngineOptions, WarmRequest};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use support::{Entry, fake, fixture};
use tokio::io::{AsyncRead, AsyncWriteExt, ReadBuf};
use tokio::sync::mpsc::UnboundedReceiver;

/// A moment before the recorded windows reset (the Claude five-hour window resets at 1791627000).
const WHEN: u64 = 1_791_620_000;
type Calls = Arc<Mutex<Vec<(String, Vec<String>)>>>;
type Time = Arc<AtomicU64>;

struct Pending;

impl AsyncRead for Pending {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Pending
    }
}

/// A status command that prints `stdout` and ends, or never answers when `stdout` is None.
fn status(stdout: Option<&'static str>) -> (Spawner, Calls) {
    let calls: Calls = Arc::default();
    let recorded = calls.clone();
    let spawn: Spawner = Arc::new(move |bin, args, _cwd| {
        recorded.lock().unwrap().push((bin.into(), args.to_vec()));
        let Some(stdout) = stdout else {
            return Ok(Proc { stdin: Box::new(tokio::io::sink()), stdout: Box::new(Pending), stderr: None, child: None });
        };
        let (mut out, reader) = tokio::io::duplex(4096);
        tokio::spawn(async move { drop(out.write_all(stdout.as_bytes()).await) });
        Ok(Proc { stdin: Box::new(tokio::io::sink()), stdout: Box::new(reader), stderr: None, child: None })
    });
    (spawn, calls)
}

fn start(dir: &Path, spawn: Spawner, ask: Option<Ask>) -> (Engine, UnboundedReceiver<AppEvent>, Time) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let opts = EngineOptions {
        data_dir: dir.into(),
        clis: BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]),
        spawn,
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask,
    };
    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e))));
    let time: Time = Arc::new(AtomicU64::new(WHEN));
    let clock = time.clone();
    engine.set_time_source(Arc::new(move || clock.load(Ordering::SeqCst) as f64));
    (engine, rx, time)
}

const FIVE: Duration = Duration::from_millis(500);

#[tokio::test]
async fn the_plan_comes_from_the_status_command_and_nothing_else_of_it_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let (spawn, calls) = status(Some(r#"{"loggedIn":true,"email":"someone@example.com","orgName":"Acme","subscriptionType":"max"}"#));
    let (engine, mut events, _) = start(dir.path(), spawn, None);
    engine.check_plans(FIVE).await;
    let claude = engine.ledger().claude;
    assert_eq!((claude.plan.as_deref(), claude.billing), (Some("max"), Billing::Subscription));
    assert!(claude.plan_ok(WHEN as f64));
    assert_eq!(*calls.lock().unwrap(), [("claude".to_string(), vec!["auth".to_string(), "status".to_string()])]);
    engine.shutdown();
    let saved = std::fs::read_to_string(dir.path().join("budget.json")).unwrap();
    assert!(!saved.contains("someone@example.com") && !saved.contains("Acme"));
    while let Ok(event) = events.try_recv() {
        assert!(matches!(event, AppEvent::Usage { .. }), "only the sanitized usage view reaches the screen");
        let shown = format!("{event:?}");
        assert!(!shown.contains("someone@example.com") && !shown.contains("Acme"));
    }
}

#[tokio::test]
async fn a_login_billed_per_use_is_known_as_such_and_needs_no_plan() {
    let dir = tempfile::tempdir().unwrap();
    let (spawn, _) = status(Some(r#"{"loggedIn":true,"authMethod":"api_key"}"#));
    let (engine, _, _) = start(dir.path(), spawn, None);
    engine.check_plans(FIVE).await;
    let claude = engine.ledger().claude;
    assert_eq!((claude.billing, claude.plan.as_deref()), (Billing::UsageBilled, None));
    assert!(!claude.ask_for_plan(), "there is no plan to ask about");
}

#[tokio::test]
async fn without_a_plan_in_the_answer_the_user_is_asked_and_the_answer_never_authorizes_billing() {
    let dir = tempfile::tempdir().unwrap();
    let (spawn, _) = status(Some(r#"{"loggedIn":true,"authMethod":"claude.ai"}"#));
    let (engine, _, _) = start(dir.path(), spawn, None);
    engine.check_plans(FIVE).await;
    assert!(engine.ledger().claude.ask_for_plan());
    engine.answer_plan(Vendor::Claude, "  Max 20x! ");
    let claude = engine.ledger().claude;
    assert_eq!(claude.typed_plan.as_deref(), Some("max20x"));
    assert!(claude.plan_ok(WHEN as f64 + 3600.0 * 5.0), "a typed plan needs no fresh detection");
    assert!(!claude.ask_for_plan());
    assert_eq!(claude.billing, Billing::Unknown, "naming a plan proves nothing about how the login is billed");
    engine.answer_plan(Vendor::Claude, "   ");
    assert_eq!(engine.ledger().claude.typed_plan.as_deref(), Some("max20x"), "an empty answer changes nothing");
}

#[tokio::test]
async fn one_failed_check_asks_nothing_three_in_a_row_ask_and_a_failure_never_refreshes_the_plan() {
    let dir = tempfile::tempdir().unwrap();
    let (good, _) = status(Some(r#"{"loggedIn":true,"subscriptionType":"pro"}"#));
    let (engine, _, time) = start(dir.path(), good, None);
    engine.check_plans(FIVE).await;
    assert!(engine.ledger().claude.plan_ok(WHEN as f64));
    engine.shutdown();

    // The same data folder, a CLI that no longer answers.
    let (hung, _) = status(None);
    let (engine, _, time2) = start(dir.path(), hung, None);
    time2.store(WHEN + 31 * 60, Ordering::SeqCst);
    drop(time);
    engine.check_plans(Duration::from_millis(30)).await;
    assert!(!engine.ledger().claude.plan_ok((WHEN + 31 * 60) as f64), "the plan is as old as the last good check");
    assert!(!engine.ledger().claude.ask_for_plan());
    engine.check_plans(Duration::from_millis(30)).await;
    engine.check_plans(Duration::from_millis(30)).await;
    // A plan was known from before, so nothing is asked even after three failures; discounts simply wait.
    assert!(!engine.ledger().claude.ask_for_plan());
    assert_eq!(engine.ledger().claude.plan.as_deref(), Some("pro"));

    let fresh = tempfile::tempdir().unwrap();
    let (hung, _) = status(None);
    let (engine, _, _) = start(fresh.path(), hung, None);
    for _ in 0..2 {
        engine.check_plans(Duration::from_millis(30)).await;
    }
    assert!(!engine.ledger().claude.ask_for_plan(), "a single failed or slow attempt asks nothing");
    engine.check_plans(Duration::from_millis(30)).await;
    assert!(engine.ledger().claude.ask_for_plan());
}

#[tokio::test]
async fn a_changed_plan_name_starts_the_learning_again() {
    let dir = tempfile::tempdir().unwrap();
    let (spawn, _) = status(Some(r#"{"loggedIn":true,"subscriptionType":"pro"}"#));
    let (engine, _, _) = start(dir.path(), spawn, None);
    engine.check_plans(FIVE).await;
    assert!(!engine.ledger().claude.relearning(WHEN as f64));
    engine.shutdown();

    // The plan is remembered across runs, so the next run notices that it is a different one.
    let (spawn, _) = status(Some(r#"{"loggedIn":true,"subscriptionType":"max"}"#));
    let (engine, _, _) = start(dir.path(), spawn, None);
    engine.check_plans(FIVE).await;
    let claude = engine.ledger().claude;
    assert_eq!(claude.plan.as_deref(), Some("max"));
    assert!(claude.relearning(WHEN as f64 + 60.0), "no discount until it has learned the new plan");
}

#[tokio::test]
async fn watching_the_accounts_checks_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let (spawn, calls) = status(Some(r#"{"loggedIn":true,"subscriptionType":"max"}"#));
    let (engine, _, _) = start(dir.path(), spawn, None);
    engine.set_utc_offset(0);
    engine.watch_accounts();
    for _ in 0..50 {
        if engine.ledger().claude.plan.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(engine.ledger().claude.plan.as_deref(), Some("max"));
    assert_eq!(calls.lock().unwrap().len(), 1, "once now, then every quarter of an hour");
    engine.shutdown();
}

#[tokio::test]
async fn startup_account_readings_wait_for_the_local_offset_and_do_not_spend() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = faber_engine::ledger::Ledger::default();
    ledger.codex.record("five_hour", 0.10, (WHEN + 7000) as f64, (WHEN - 60) as f64, -10800);
    faber_engine::store::Store::new(dir.path()).save_ledger(&ledger).unwrap();
    let f = fake(vec![
        Entry { input: true, msg: json!({"id":1,"method":"initialize"}) },
        Entry { input: false, msg: json!({"id":1,"result":{}}) },
        Entry { input: true, msg: json!({"method":"initialized"}) },
        Entry { input: true, msg: json!({"id":2,"method":"account/rateLimits/read"}) },
        Entry { input: true, msg: json!({"id":3,"method":"account/read","params":{"refreshToken":false}}) },
        Entry { input: false, msg: json!({"id":3,"result":{"account":{"type":"chatgpt","planType":"plus"}}}) },
        Entry {
            input: false,
            msg: json!({"id":2,"result":{"rateLimits":{"primary":{"usedPercent":13,"windowDurationMins":300,"resetsAt":WHEN + 7000}}}}),
        },
    ]);
    let (engine, _, _) = start(dir.path(), f.spawn.clone(), None);
    engine.replace_clis(BTreeMap::from([(Vendor::Codex, Cli { command: "codex".into(), prefix: vec![] })]));
    engine.watch_accounts();
    tokio::time::sleep(Duration::from_millis(80)).await;
    let before_clock = f.args.lock().unwrap().len();
    assert!(engine.ledger().codex.own_spend.is_empty());
    engine.set_utc_offset(-10800);
    for _ in 0..50 {
        if engine.ledger().codex.readings["five_hour"].used == 0.13 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let account = engine.ledger().codex;
    let local_bin = faber_engine::budget::bin((WHEN - 60) as f64, -10800);
    assert_eq!(account.learned.mass[local_bin], 3.0, "the startup drop belongs to the local weekday and hour");
    assert_eq!(before_clock, 0, "the watcher waits until the screen supplies the offset");
    assert!(account.own_spend.is_empty() && account.spans.is_empty(), "a free account check never meters a turn");
    assert_eq!(f.wire.lines_with("turn/start"), 0);
    assert!(f.mismatches.lock().unwrap().is_empty());
    engine.set_utc_offset(-10800);
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(f.args.lock().unwrap().len(), 1, "clock updates do not duplicate the watcher");
    engine.shutdown();
}

/// The recording of a Claude chat, with the one-word question a pulse asks in place of the recorded one.
fn pulse_recording() -> Vec<Entry> {
    fixture("claude", "chat")
        .into_iter()
        .map(|mut e| {
            if e.input && e.msg["type"] == "user" {
                e.msg["message"]["content"] = json!("ok");
            }
            e
        })
        .collect()
}

fn typing() -> WarmRequest {
    WarmRequest { conversation_id: None, area: Area::Chat, project_id: None, choice: Choice { vendor: "auto".into(), model: None } }
}

fn silent() -> Ask {
    Arc::new(|_| Box::pin(async { None }))
}

async fn settle_for(engine: &Engine, window: &str) -> bool {
    for _ in 0..100 {
        if engine.ledger().claude.readings.contains_key(window) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

#[tokio::test]
async fn typing_in_auto_asks_for_a_fresh_reading_only_when_the_one_held_is_old() {
    let dir = tempfile::tempdir().unwrap();
    let f = fake(pulse_recording());
    let (engine, _, time) = start(dir.path(), f.spawn.clone(), Some(silent()));
    engine.note_account_events(
        Vendor::Claude,
        vec![EngineEvent::Signals(Signals { billing: Some(Billing::Subscription), ..Signals::default() })],
    );
    engine.route_draft(typing(), "Plan a trip".into());
    assert!(settle_for(&engine, "five_hour").await, "the pulse brought a reading");
    let args = f.args.lock().unwrap().clone();
    assert_eq!(args.len(), 1);
    assert!(args[0].contains(&"claude-haiku-5-5".to_string()) && args[0].contains(&"--system-prompt".to_string()));
    assert_eq!(*f.mismatches.lock().unwrap(), Vec::<String>::new());
    assert_eq!(engine.ledger().claude.spans.len(), 1, "the pulse is Faber's own request, so the learning knows when it ran");

    engine.route_draft(typing(), "Plan a trip to Lisbon".into());
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(f.args.lock().unwrap().len(), 1, "the reading is fresh");

    time.store(WHEN + 21 * 60, Ordering::SeqCst);
    engine.route_draft(typing(), "Plan a trip to Porto".into());
    for _ in 0..50 {
        if f.args.lock().unwrap().len() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(f.args.lock().unwrap().len(), 2, "twenty minutes later the reading is old");
    engine.shutdown();
}

#[tokio::test]
async fn a_pulse_is_a_request_like_any_other_and_waits_at_the_gate() {
    let dir = tempfile::tempdir().unwrap();
    let f = fake(pulse_recording());
    let (engine, _, time) = start(dir.path(), f.spawn.clone(), Some(silent()));
    // Nearly out, and the CLI may switch to paid use by itself: the pulse must not be the request that does it.
    let quota = EngineEvent::Quota { window: "five_hour".into(), used: 0.95, resets_at: (WHEN + 3600) as f64 };
    let paid = EngineEvent::Signals(Signals { paid_fallback: Some(true), billing: Some(Billing::Subscription), ..Signals::default() });
    engine.note_account_events(Vendor::Claude, vec![quota, paid]);
    time.store(WHEN + 21 * 60, Ordering::SeqCst);
    engine.route_draft(typing(), "Plan a trip".into());
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(f.args.lock().unwrap().is_empty(), "denied: a window above 90% with paid use possible");
    engine.shutdown();
}

#[tokio::test]
async fn a_typed_plan_never_allows_a_pulse_with_unknown_billing() {
    let dir = tempfile::tempdir().unwrap();
    let f = fake(pulse_recording());
    let (engine, _, _) = start(dir.path(), f.spawn.clone(), Some(silent()));
    engine.answer_plan(Vendor::Claude, "max");
    engine.route_draft(typing(), "Plan a trip".into());
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(engine.ledger().claude.billing, Billing::Unknown);
    assert!(f.args.lock().unwrap().is_empty());
    engine.shutdown();
}

#[tokio::test]
async fn signing_out_clears_a_previously_confirmed_subscription() {
    let dir = tempfile::tempdir().unwrap();
    let (connected, _) = status(Some(r#"{"loggedIn":true,"subscriptionType":"max"}"#));
    let (disconnected, _) = status(Some(r#"{"loggedIn":false}"#));
    let calls = Arc::new(AtomicU64::new(0));
    let spawn: Spawner =
        Arc::new(
            move |bin, args, cwd| {
                if calls.fetch_add(1, Ordering::SeqCst) == 0 { connected(bin, args, cwd) } else { disconnected(bin, args, cwd) }
            },
        );
    let (engine, _, _) = start(dir.path(), spawn, None);
    engine.check_plans(FIVE).await;
    assert_eq!(engine.ledger().claude.billing, Billing::Subscription);
    engine.check_plans(FIVE).await;
    assert_eq!(engine.ledger().claude.billing, Billing::Unknown);
    engine.shutdown();
}

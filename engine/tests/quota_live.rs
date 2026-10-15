//! Quota readings that stay fresh at no model cost: when they are taken (fake time), what they say while Faber's own recent
//! work is not in them yet, and that the decisions treat such a reading as a floor.

use crate::support::{self, Entry, fake, fixture};
use faber_engine::budget::{Billing, Purpose, gate};
use faber_engine::cadence::{AFTER_TURN_GAP, Cadence, FOCUS_GAP, Reason, TICK};
use faber_engine::cli::Cli;
use faber_engine::ledger::{LAG_CLAUDE, Ledger, VendorLedger, Work};
use faber_engine::model::{Area, Vendor};
use faber_engine::proc::{Proc, Spawner};
use faber_engine::router::Choice;
use faber_engine::signals::claude_usage_windows;
use faber_engine::store::Store;
use faber_engine::{Engine, EngineOptions, SendRequest};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncWriteExt;

const T0: f64 = 1_791_620_000.0;
const BOTH: [Vendor; 2] = [Vendor::Claude, Vendor::Codex];

// ---- when readings are taken -------------------------------------------------------------------------------------------

/// Looks at the clock every TICK from `from` (exclusive) to `to` (inclusive), as the loop does, and gathers who was read.
fn ticks(c: &mut Cadence, from: f64, to: f64) -> Vec<(f64, Vendor, Reason)> {
    let mut read = vec![];
    let mut t = from + TICK;
    while t <= to {
        read.extend(c.tick(t, &BOTH).into_iter().map(|(v, r)| (t, v, r)));
        t += TICK;
    }
    read
}

#[test]
fn the_first_look_reads_everyone_and_a_visible_window_reads_every_five_minutes() {
    let mut c = Cadence::default();
    assert_eq!(c.tick(T0, &BOTH), vec![(Vendor::Claude, Reason::Start), (Vendor::Codex, Reason::Start)]);
    assert!(ticks(&mut c, T0, T0 + 5.0 * 60.0 - 1.0).is_empty(), "nothing in the first five minutes");
    let later = ticks(&mut c, T0 + 5.0 * 60.0 - TICK, T0 + 5.0 * 60.0 + TICK);
    assert_eq!(later, vec![(T0 + 300.0, Vendor::Claude, Reason::Timer), (T0 + 300.0, Vendor::Codex, Reason::Timer)]);
}

#[test]
fn a_hidden_window_reads_every_quarter_of_an_hour_and_never_later() {
    let mut c = Cadence::default();
    c.tick(T0, &BOTH);
    assert!(c.attention(T0 + 10.0, &BOTH, false, false).is_empty(), "hiding reads nothing");
    // The owner wants a reading at most 15 minutes old while hidden: the numbers are written out, not read from the constants.
    assert!(ticks(&mut c, T0, T0 + 15.0 * 60.0 - 1.0).is_empty(), "a quiet quarter of an hour");
    let at = ticks(&mut c, T0 + 15.0 * 60.0 - TICK, T0 + 15.0 * 60.0 + TICK);
    assert_eq!(at.iter().map(|r| r.0).collect::<Vec<_>>(), vec![T0 + 15.0 * 60.0; 2]);
}

#[test]
fn the_end_of_a_request_reads_soon_but_not_after_every_one() {
    let mut c = Cadence::default();
    c.tick(T0, &BOTH);
    assert!(c.event(T0 + 10.0, &BOTH, Reason::TurnEnded).is_empty(), "a reading was just taken");
    assert!(ticks(&mut c, T0, T0 + AFTER_TURN_GAP - TICK).is_empty());
    assert_eq!(c.event(T0 + AFTER_TURN_GAP, &BOTH, Reason::TurnEnded), BOTH.to_vec());
    assert!(c.event(T0 + AFTER_TURN_GAP + 5.0, &BOTH, Reason::TurnEnded).is_empty());
    // The timer counts from the last reading, whatever caused it.
    let next = ticks(&mut c, T0 + AFTER_TURN_GAP - TICK, T0 + AFTER_TURN_GAP + 5.0 * 60.0 + TICK);
    assert_eq!(next.first().map(|r| r.0), Some(T0 + AFTER_TURN_GAP + 5.0 * 60.0));
}

#[test]
fn gaining_focus_reads_at_once_but_a_hidden_window_gaining_nothing_does_not() {
    let mut c = Cadence::default();
    c.tick(T0, &BOTH);
    assert!(c.attention(T0 + 5.0, &BOTH, true, true).is_empty(), "read a moment ago");
    assert_eq!(c.attention(T0 + FOCUS_GAP, &BOTH, true, true), BOTH.to_vec());
    assert!(c.attention(T0 + 2.0 * FOCUS_GAP + 1.0, &BOTH, false, false).is_empty());
    assert!(c.attention(T0 + 4.0 * FOCUS_GAP, &BOTH, false, true).is_empty(), "focused but not visible reads nothing");
}

#[test]
fn a_pc_that_woke_reads_at_once() {
    let mut c = Cadence::default();
    c.tick(T0, &BOTH);
    c.tick(T0 + TICK, &BOTH);
    // The loop was not heard from for two minutes: the PC slept, so the counters have moved on without Faber.
    let woke = c.tick(T0 + TICK + 120.0, &BOTH);
    assert_eq!(woke, vec![(Vendor::Claude, Reason::Wake), (Vendor::Codex, Reason::Wake)]);
    assert!(c.tick(T0 + 2.0 * TICK + 120.0, &BOTH).is_empty());
    assert!(c.tick(T0 + TICK + 120.0 - 3600.0, &BOTH).len() == 2, "a clock set back counts as a wake too");
}

#[test]
fn a_vendor_that_is_not_installed_is_never_read() {
    let mut c = Cadence::default();
    assert_eq!(c.tick(T0, &[Vendor::Codex]), vec![(Vendor::Codex, Reason::Start)]);
    assert!(c.event(T0 + 1000.0, &[], Reason::Focus).is_empty());
}

// ---- what a reading says while Faber's own work is not in it ------------------------------------------------------------

fn calibrated(now: f64) -> VendorLedger {
    let mut v = VendorLedger {
        lag: LAG_CLAUDE,
        billing: Billing::Subscription,
        paid_fallback: Some(false),
        plan_seen_at: Some(now),
        ..Default::default()
    };
    v.weights.raw.insert("m".into(), 20.0); // 20 points of the five-hour window per API-equivalent dollar
    v.record("five_hour", 0.10, now + 10_000.0, now, 0);
    v.record("seven_day", 0.30, now + 400_000.0, now, 0);
    v
}

fn work(v: &mut VendorLedger, start: f64, finished: f64, model: &str, cost: Option<f64>) {
    v.worked(Work { start, finished, model: model.into(), cost, incomplete: false }, finished);
}

fn used(v: &VendorLedger, window: &str, now: f64) -> f64 {
    v.snap(now).windows.into_iter().find(|(n, _)| n == window).unwrap().1.used
}

#[test]
fn a_reading_with_no_recent_work_of_ours_is_final() {
    let v = calibrated(T0);
    assert!(!v.counting(T0 + 60.0));
    assert_eq!(v.uncounted("five_hour", T0 + 60.0), None);
    assert_eq!(used(&v, "five_hour", T0 + 60.0), 0.10);
}

#[test]
fn recent_work_marks_the_reading_and_adds_what_the_conversion_says_it_cost() {
    let mut v = calibrated(T0);
    work(&mut v, T0 + 20.0, T0 + 60.0, "m", Some(0.5)); // 0.5 dollars x 20 points = 10 points
    let now = T0 + 120.0;
    assert!(v.counting(now));
    let extra = v.uncounted("five_hour", now).unwrap();
    assert!((extra - 0.10).abs() < 1e-9, "{extra}");
    assert!((used(&v, "five_hour", now) - 0.20).abs() < 1e-9, "the decision sees the floor: 10% read + 10% not counted");
    assert_eq!(v.uncounted("seven_day", now), None, "no conversion for the weekly window: no number made up");
    assert_eq!(used(&v, "seven_day", now), 0.30);
    assert!(v.snap(now).counting);
}

#[test]
fn what_the_reading_already_rose_by_is_not_counted_twice() {
    let mut v = calibrated(T0);
    work(&mut v, T0 + 20.0, T0 + 60.0, "m", Some(0.5));
    v.record("five_hour", 0.16, T0 + 10_000.0, T0 + 300.0, 0); // the vendor has counted 6 of the 10 points
    let extra = v.uncounted("five_hour", T0 + 301.0).unwrap();
    assert!((extra - 0.04).abs() < 1e-9, "{extra}");
    v.record("five_hour", 0.25, T0 + 10_000.0, T0 + 600.0, 0); // and then some more, from elsewhere
    assert_eq!(v.uncounted("five_hour", T0 + 601.0), Some(0.0));
    assert!(v.counting(T0 + 601.0), "still marked: the lag has not passed");
}

#[test]
fn a_model_without_a_conversion_gives_only_the_marker() {
    let mut v = calibrated(T0);
    work(&mut v, T0 + 20.0, T0 + 60.0, "never-measured", Some(0.5));
    assert!(v.counting(T0 + 120.0));
    assert_eq!(v.uncounted("five_hour", T0 + 120.0), None);
    assert_eq!(used(&v, "five_hour", T0 + 120.0), 0.10, "never invent a number");
    let mut w = calibrated(T0);
    work(&mut w, T0 + 20.0, T0 + 60.0, "m", None);
    assert_eq!(w.uncounted("five_hour", T0 + 120.0), None, "a request with no cost cannot be converted");
}

#[test]
fn after_the_lag_the_reading_is_final_again() {
    let mut v = calibrated(T0);
    work(&mut v, T0 + 20.0, T0 + 60.0, "m", Some(0.5));
    assert!(v.counting(T0 + 60.0 + LAG_CLAUDE - 1.0));
    assert!(!v.counting(T0 + 60.0 + LAG_CLAUDE + 1.0));
    assert_eq!(used(&v, "five_hour", T0 + 60.0 + LAG_CLAUDE + 1.0), 0.10);
}

#[test]
fn a_reading_that_may_still_rise_is_a_floor_for_every_decision() {
    let mut v = calibrated(T0);
    v.record("five_hour", 0.90, T0 + 10_000.0, T0 + 5.0, 0);
    let now = T0 + 120.0;
    assert!(gate(&v.snap(now), None, Purpose::UserTurn { by_hand: false }, false, now).is_ok(), "90% with nothing pending");
    work(&mut v, T0 + 20.0, T0 + 60.0, "m", Some(0.5)); // 10 more points, not in the 90% yet
    assert!(
        gate(&v.snap(now), None, Purpose::UserTurn { by_hand: false }, false, now).is_err(),
        "90% + 10% not counted is the limit: no request is admitted on the stale number"
    );
}

#[test]
fn a_discount_is_not_believed_while_the_reading_may_still_rise() {
    let mut v = calibrated(T0);
    assert!(v.snap(T0 + 60.0).discount_ok());
    work(&mut v, T0 + 20.0, T0 + 60.0, "never-measured", Some(0.01));
    assert!(!v.snap(T0 + 120.0).discount_ok(), "even with no number to add, a rising reading buys no discount");
}

#[test]
fn the_same_reset_printed_to_the_minute_and_to_the_second_is_one_window() {
    let mut v = VendorLedger::default();
    v.record("five_hour", 0.10, T0 + 7_000.0, T0, 0);
    v.record("five_hour", 0.12, T0 + 7_000.0 - 59.0, T0 + 60.0, 0);
    v.record("five_hour", 0.14, T0 + 7_000.0 + 60.0, T0 + 120.0, 0);
    assert_eq!(v.readings["five_hour"].resets_at, T0 + 7_000.0);
    assert!(v.history["five_hour"].iter().all(|p| p.resets_at == T0 + 7_000.0), "the history keeps comparing like with like");
    v.record("five_hour", 0.01, T0 + 7_000.0 + 18_000.0, T0 + 7_100.0, 0);
    assert_eq!(v.readings["five_hour"].resets_at, T0 + 25_000.0, "a new window is a new window");
}

// ---- reading the text `/usage` prints --------------------------------------------------------------------------------------

const USAGE_TEXT: &str = "You are currently using your subscription to power your Claude Code usage\n\nCurrent session: 10% used · resets Oct 10, 7:10am (America/Fortaleza)\nCurrent week (all models): 36% used · resets Oct 16, 5:00am (America/Fortaleza)\nCurrent week (Fable): 0% used · resets Oct 16, 5am (America/Fortaleza)\n\nLast 24h · 3655 requests";

#[test]
fn the_usage_text_gives_the_two_vendor_wide_windows_in_the_pcs_time() {
    let got = claude_usage_windows(USAGE_TEXT, T0, -10_800);
    assert_eq!(got, vec![("five_hour", 0.10, 1_791_627_000.0), ("seven_day", 0.36, 1_792_137_600.0)]);
    // At a different offset the same printed time is a different instant.
    assert_eq!(claude_usage_windows(USAGE_TEXT, T0, 0)[0].2, 1_791_627_000.0 - 10_800.0);
}

#[test]
fn a_reset_without_a_date_is_today_or_tomorrow_and_a_new_year_is_found() {
    let today = claude_usage_windows("Current session: 5% used · resets 7:10am (America/Fortaleza)", T0, -10_800);
    assert_eq!(today[0].2, 1_791_627_000.0);
    let tomorrow = claude_usage_windows("Current session: 5% used · resets 5:00am (America/Fortaleza)", T0, -10_800);
    assert_eq!(tomorrow[0].2, 1_791_705_600.0);
    // 2026-12-30 12:00 UTC; the week resets on 2 January.
    let new_year = claude_usage_windows("Current week (all models): 50% used · resets Jan 2, 3:00am (X/Y)", 1_798_632_000.0, 0);
    assert_eq!(new_year[0].2, 1_798_858_800.0);
}

#[test]
fn text_that_is_not_a_usage_report_gives_nothing() {
    for text in [
        "",
        "Unknown command",
        "Current session: lots used · resets Oct 10, 7:10am (X)",
        "Current session: 10% used",
        "Current session: 10% used · resets Smarch 4, 7:10am (X)",
    ] {
        assert!(claude_usage_windows(text, T0, 0).is_empty(), "{text}");
    }
}

// ---- the engine: readings taken by the watcher, at no model cost -------------------------------------------------------------

const WHEN: u64 = 1_791_620_000;
type Calls = Arc<Mutex<Vec<Vec<String>>>>;

fn usage_json(turns: u32) -> String {
    json!({ "type": "result", "is_error": false, "num_turns": turns, "total_cost_usd": 0, "usage": { "input_tokens": 0, "output_tokens": 0 }, "result": USAGE_TEXT }).to_string()
}

fn printing(out: String) -> Proc {
    let (mut w, r) = tokio::io::duplex(65_536);
    tokio::spawn(async move { drop(w.write_all(out.as_bytes()).await) });
    Proc { stdin: Box::new(tokio::io::sink()), stdout: Box::new(r), stderr: None, child: None, contained: None }
}

/// Answers `/usage` with `text`, `auth status` with a plan, and hands anything else to `other`.
fn spawner(text: String, other: Option<Spawner>) -> (Spawner, Calls) {
    let usage: Calls = Arc::default();
    let seen = usage.clone();
    let spawn: Spawner = Arc::new(move |bin, args, cwd| {
        if args.iter().any(|a| a == "/usage") {
            seen.lock().unwrap().push(args.to_vec());
            Ok(printing(text.clone()))
        } else if args.iter().any(|a| a == "status") {
            Ok(printing(r#"{"loggedIn":true,"subscriptionType":"max"}"#.into()))
        } else if let Some(other) = &other {
            other(bin, args, cwd)
        } else {
            Err(std::io::Error::other("nothing else runs in this test"))
        }
    });
    (spawn, usage)
}

fn start(dir: &Path, spawn: Spawner) -> (Engine, Arc<AtomicU64>) {
    let engine = Engine::new(
        EngineOptions {
            data_dir: dir.into(),
            clis: BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]),
            spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 3,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    );
    let time = Arc::new(AtomicU64::new(WHEN));
    let clock = time.clone();
    engine.set_time_source(Arc::new(move || clock.load(Ordering::SeqCst) as f64));
    (engine, time)
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..150 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("never happened: {what}");
}

#[tokio::test]
async fn the_watcher_reads_claude_through_the_usage_command_and_asks_no_model() {
    let dir = tempfile::tempdir().unwrap();
    let (spawn, calls) = spawner(usage_json(0), None);
    let (engine, _) = start(dir.path(), spawn);
    engine.set_utc_offset(-10_800);
    engine.watch_accounts();
    until("both windows read", || engine.ledger().claude.readings.len() == 2).await;
    let l = engine.ledger().claude;
    assert_eq!((l.readings["five_hour"].used, l.readings["five_hour"].resets_at), (0.10, 1_791_627_000.0));
    assert_eq!((l.readings["seven_day"].used, l.readings["seven_day"].resets_at), (0.36, 1_792_137_600.0));
    let calls = calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    let args = &calls[0];
    // A local command: no prompt but the command itself, no model chosen, no conversation stream, nothing saved.
    assert_eq!(args[..2], ["-p", "/usage"]);
    for flag in ["--model", "--input-format", "--system-prompt", "--effort", "--resume"] {
        assert!(!args.iter().any(|a| a == flag), "{flag} in {args:?}");
    }
    assert!(args.iter().any(|a| a == "--no-session-persistence"));
    support::assert_no_user_customizations(args);
    assert!(l.spans.is_empty() && l.own_spend.is_empty(), "a free reading is never metered as a request");
    engine.shutdown();
}

#[tokio::test]
async fn a_usage_command_that_turns_out_to_ask_a_model_is_never_run_again() {
    let dir = tempfile::tempdir().unwrap();
    let (spawn, calls) = spawner(usage_json(1), None);
    let (engine, time) = start(dir.path(), spawn);
    engine.set_utc_offset(-10_800);
    engine.watch_accounts();
    until("one attempt", || calls.lock().unwrap().len() == 1).await;
    assert!(engine.ledger().claude.readings.is_empty(), "nothing from a command that spent tokens is believed");
    time.store(WHEN + 600, Ordering::SeqCst);
    engine.set_attention(true, true);
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(calls.lock().unwrap().len(), 1);
    engine.shutdown();
}

#[tokio::test]
async fn focus_and_the_end_of_a_turn_read_again_and_a_hidden_window_does_not() {
    let dir = tempfile::tempdir().unwrap();
    let chat = fake(fixture("claude", "chat"));
    let (spawn, calls) = spawner(usage_json(0), Some(chat.spawn.clone()));
    let (engine, time) = start(dir.path(), spawn);
    engine.set_utc_offset(-10_800);
    engine.watch_accounts();
    until("first reading", || calls.lock().unwrap().len() == 1).await;
    until("idle", || !engine.ledger().claude.readings.is_empty()).await;

    time.store(WHEN + 40, Ordering::SeqCst);
    engine.set_attention(false, false);
    engine.set_attention(true, false);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(calls.lock().unwrap().len(), 1, "showing the window is not a reason; focusing it is");
    engine.set_attention(true, true);
    until("a reading on focus", || calls.lock().unwrap().len() == 2).await;
    engine.set_attention(true, true);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(calls.lock().unwrap().len(), 2, "focus twice in a row reads once");

    time.store(WHEN + 400, Ordering::SeqCst);
    let ask = SendRequest {
        conversation_id: None,
        area: Area::Chat,
        project_id: None,
        text: "In one sentence: why is the sky blue?".into(),
        images: vec![],
        files: vec![],
        choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
    };
    let id = engine.send(ask).unwrap();
    until("the turn", || {
        engine.open(&id).is_some_and(|c| c.messages.len() == 2 && c.messages[1].status == Some(faber_engine::model::Status::Done))
    })
    .await;
    until("a reading after the turn", || calls.lock().unwrap().len() == 3).await;
    assert!(engine.ledger().claude.counting(WHEN as f64 + 401.0), "the turn just ended: the counter may not hold it yet");
    engine.shutdown();
}

#[tokio::test]
async fn the_screen_is_told_how_old_the_reading_is_and_that_it_may_still_rise() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::default();
    let now = WHEN as f64;
    ledger.claude = calibrated(now - 300.0);
    ledger.claude.record("five_hour", 0.10, now + 10_000.0, now - 300.0, 0);
    work(&mut ledger.claude, now - 200.0, now - 100.0, "m", Some(0.5));
    Store::new(dir.path()).save_ledger(&ledger);
    let (engine, _) = start(dir.path(), spawner(usage_json(0), None).0);
    let u = engine.state().usage.remove(0);
    assert!(u.counting);
    let five = u.windows.iter().find(|w| w.name == "five_hour").unwrap();
    assert_eq!((five.used, five.read_at), (0.10, now - 300.0), "the reading as it was, and when it was taken");
    assert!((five.uncounted.unwrap() - 0.10).abs() < 1e-9);
    let week = u.windows.iter().find(|w| w.name == "seven_day").unwrap();
    assert_eq!(week.uncounted, None);
    engine.shutdown();
}

#[tokio::test]
async fn codex_is_read_through_its_rate_limit_call_and_no_turn_is_started() {
    let dir = tempfile::tempdir().unwrap();
    let f = fake(vec![
        Entry { input: true, msg: json!({"id":1,"method":"initialize"}) },
        Entry { input: false, msg: json!({"id":1,"result":{}}) },
        Entry { input: true, msg: json!({"method":"initialized"}) },
        Entry { input: true, msg: json!({"id":2,"method":"account/rateLimits/read"}) },
        Entry { input: true, msg: json!({"id":3,"method":"account/read","params":{"refreshToken":false}}) },
        Entry { input: false, msg: json!({"id":3,"result":{"account":{"type":"chatgpt","planType":"plus"}}}) },
        Entry {
            input: false,
            msg: json!({"id":2,"result":{"rateLimits":{"primary":{"usedPercent":44,"windowDurationMins":10080,"resetsAt":WHEN + 400_000}}}}),
        },
    ]);
    let (engine, _) = start(dir.path(), f.spawn.clone());
    engine.replace_clis(BTreeMap::from([(Vendor::Codex, Cli { command: "codex".into(), prefix: vec![] })]));
    engine.set_utc_offset(-10_800);
    engine.watch_accounts();
    until("the weekly reading", || engine.ledger().codex.readings.contains_key("seven_day")).await;
    assert_eq!(engine.ledger().codex.readings["seven_day"].used, 0.44);
    // Everything written to the server: the handshake and the two reads. No thread, no turn, no prompt.
    assert_eq!(f.wire.lines_with("\"method\""), 4);
    for model_call in ["turn/start", "thread/start", "thread/resume", "turn/steer"] {
        assert_eq!(f.wire.lines_with(model_call), 0, "{model_call}");
    }
    assert!(f.mismatches.lock().unwrap().is_empty());
    engine.shutdown();
}

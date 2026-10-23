//! Quota readings that stay fresh at no model cost: when they are taken (fake time), what they say while Faber's own recent
//! work is not in them yet, and that the decisions treat such a reading as a floor.

use crate::support::{self, Entry, fake, fixture};
use faber_engine::budget::{Billing, Purpose, gate};
use faber_engine::cadence::{CLAUDE_GAP, CODEX_GAP, Cadence, EVERY, Reason, TICK};
use faber_engine::cli::Cli;
use faber_engine::events::EngineEvent;
use faber_engine::ledger::{LAG_CLAUDE, Ledger, VendorLedger, Work};
use faber_engine::model::{Area, Vendor};
use faber_engine::proc::{Proc, Spawner};
use faber_engine::router::Choice;
use faber_engine::signals::{Signals, claude_usage_windows};
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
fn the_first_look_reads_everyone_and_then_every_quarter_of_an_hour_visible_or_hidden() {
    // The owner wants a reading at most 15 minutes old: the numbers are written out, not read from the constants.
    for hidden in [false, true] {
        let mut c = Cadence::default();
        assert_eq!(c.tick(T0, &BOTH), vec![(Vendor::Claude, Reason::Start), (Vendor::Codex, Reason::Start)]);
        if hidden {
            assert!(c.attention(T0 + 10.0, &BOTH, false, false).is_empty(), "hiding reads nothing");
        }
        assert!(ticks(&mut c, T0, T0 + 15.0 * 60.0 - 1.0).is_empty(), "a quiet quarter of an hour, hidden: {hidden}");
        let at = ticks(&mut c, T0 + 15.0 * 60.0 - TICK, T0 + 15.0 * 60.0 + TICK);
        assert_eq!(at, vec![(T0 + 900.0, Vendor::Claude, Reason::Timer), (T0 + 900.0, Vendor::Codex, Reason::Timer)]);
    }
    assert_eq!((EVERY, CLAUDE_GAP, CODEX_GAP), (900.0, 300.0, 60.0));
}

#[test]
fn focus_the_end_of_a_request_and_a_wake_share_one_debounce_per_vendor() {
    let mut c = Cadence::default();
    c.tick(T0, &BOTH);
    assert!(c.event(T0 + 10.0, &BOTH).is_empty(), "a reading was just taken");
    // A minute on, the quick vendor is due again and the heavy one is not.
    assert_eq!(c.event(T0 + 60.0, &BOTH), vec![Vendor::Codex]);
    assert!(c.event(T0 + 61.0, &BOTH).is_empty());
    assert_eq!(c.event(T0 + 300.0, &BOTH), vec![Vendor::Claude, Vendor::Codex]);
    // Focus is one of those events; it spends the same debounce a request's end would.
    assert!(c.attention(T0 + 301.0, &BOTH, true, true).is_empty());
    assert!(c.event(T0 + 302.0, &BOTH).is_empty(), "the end of a request right after a focus read reads nothing");
    assert!(c.attention(T0 + 700.0, &BOTH, false, true).is_empty(), "focused but not visible reads nothing");
    assert_eq!(c.attention(T0 + 700.0, &BOTH, true, true), BOTH.to_vec());
    // The timer counts from the last reading, whatever caused it.
    let next = ticks(&mut c, T0 + 700.0, T0 + 700.0 + 900.0 + TICK);
    assert_eq!(next.first().map(|r| r.0), Some(T0 + 700.0 + 900.0));
}

#[test]
fn a_pc_that_woke_goes_through_the_same_debounce() {
    let mut c = Cadence::default();
    c.tick(T0, &BOTH);
    c.tick(T0 + TICK, &BOTH);
    // The loop was not heard from for two minutes: the PC slept. Codex was last read 150 s ago (past its minute), Claude is
    // inside its five.
    let woke = c.tick(T0 + TICK + 120.0, &BOTH);
    assert_eq!(woke, vec![(Vendor::Codex, Reason::Wake)]);
    assert!(c.tick(T0 + 2.0 * TICK + 120.0, &BOTH).is_empty());
    // A reading taken on focus just before the late tick leaves nothing for the wake to do.
    let mut c = Cadence::default();
    c.tick(T0, &BOTH);
    c.tick(T0 + TICK, &BOTH);
    assert_eq!(c.event(T0 + 400.0, &BOTH), BOTH.to_vec());
    assert!(c.tick(T0 + 400.0 + 1.0, &BOTH).is_empty(), "the slept loop wakes just after the focus read: debounced");
    // Once the gap has passed, a wake reads, and a clock set back counts as a wake too.
    let mut c = Cadence::default();
    c.tick(T0, &BOTH);
    c.tick(T0 + TICK, &BOTH);
    assert_eq!(c.tick(T0 + 400.0, &BOTH).len(), 2, "slept past both gaps");
    assert_eq!(c.tick(T0 + 400.0 - 3600.0, &BOTH).len(), 2, "a clock set back reads again");
}

#[test]
fn a_vendor_that_is_not_installed_is_never_read() {
    let mut c = Cadence::default();
    assert_eq!(c.tick(T0, &[Vendor::Codex]), vec![(Vendor::Codex, Reason::Start)]);
    assert!(c.event(T0 + 1000.0, &[]).is_empty());
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
fn time_alone_never_makes_a_reading_final_again() {
    let mut v = calibrated(T0);
    v.record("five_hour", 0.90, T0 + 10_000.0, T0 + 5.0, 0);
    work(&mut v, T0 + 20.0, T0 + 60.0, "m", Some(0.5));
    // Every refresh since has failed: an hour past the lag the counter is still not known to hold the work.
    let late = T0 + 60.0 + LAG_CLAUDE + 3600.0;
    assert!(v.counting(late));
    assert!((used(&v, "five_hour", late) - 1.0).abs() < 1e-9, "the floor stays up: 90% + 10% not counted");
    assert!(gate(&v.snap(late), None, Purpose::UserTurn { by_hand: false }, false, late).is_err(), "and so does the refusal");
}

#[test]
fn a_window_is_settled_only_by_its_own_reading_after_the_work_and_its_lag() {
    let mut v = calibrated(T0);
    work(&mut v, T0 + 20.0, T0 + 60.0, "m", Some(0.5));
    let settled = T0 + 60.0 + LAG_CLAUDE;
    v.record("five_hour", 0.14, T0 + 10_000.0, settled - 1.0, 0);
    v.record("seven_day", 0.31, T0 + 400_000.0, settled - 1.0, 0);
    assert!(v.counting(settled), "readings taken before the counter had time to catch up settle nothing");
    // Only the weekly window is read after the lag: the five-hour window, where the estimate lives, is not settled by it.
    v.record("seven_day", 0.32, T0 + 400_000.0, settled, 0);
    assert!(v.counting(settled + 1.0));
    assert!(v.uncounted("five_hour", settled + 1.0).is_some(), "another window's reading never clears this one's floor");
    assert!(used(&v, "five_hour", settled + 1.0) > 0.14);
    v.record("five_hour", 0.19, T0 + 10_000.0, settled + 5.0, 0);
    assert!(!v.counting(settled + 6.0));
    assert_eq!(used(&v, "five_hour", settled + 6.0), 0.19, "the number as read, no estimate on top");
}

#[test]
fn unsettled_work_is_folded_never_dropped() {
    let mut v = calibrated(T0);
    for i in 0..150 {
        work(&mut v, T0 + 20.0 + i as f64, T0 + 60.0 + i as f64, "m", Some(0.01)); // 0.2 points each, 30 in all
    }
    work(&mut v, T0 + 300.0, T0 + 320.0, "never-measured", Some(0.01));
    assert!(v.work.len() <= 100 && v.folded.is_some(), "too many to keep one by one: the oldest are folded");
    let day_later = T0 + 86_400.0 * 0.1; // every refresh since has failed
    assert!(v.counting(day_later));
    let extra = v.uncounted("five_hour", day_later).unwrap();
    assert!((extra - 0.30).abs() < 1e-9, "what could be converted is all still there: {extra}");
    // The first full reading of every window after the last of them settles all of it, folded or not.
    let after = T0 + 320.0 + LAG_CLAUDE;
    v.record("five_hour", 0.40, T0 + 10_000.0, after, 0);
    v.record("seven_day", 0.50, T0 + 400_000.0, after, 0);
    assert!(!v.counting(after + 1.0));
    assert!(v.folded.is_none() && v.work.is_empty(), "settled work is forgotten");
}

#[test]
fn a_ledger_read_from_disk_folds_too_much_pending_work_instead_of_dropping_it() {
    let mut ledger = Ledger { claude: calibrated(T0), ..Default::default() };
    for i in 0..150 {
        ledger.claude.work.push(Work {
            start: T0 + i as f64,
            finished: T0 + 30.0 + i as f64,
            model: "m".into(),
            cost: Some(0.01),
            incomplete: false,
        });
    }
    ledger.sanitize();
    assert_eq!(ledger.claude.work.len(), 100);
    let folded = ledger.claude.folded.clone().unwrap();
    assert!((folded.points - 50.0 * 0.2).abs() < 1e-9 && !folded.unknown, "{folded:?}");
    assert!(ledger.claude.counting(T0 + 200.0));
}

#[test]
fn unknown_work_never_erases_a_known_floor() {
    let mut v = calibrated(T0);
    v.record("five_hour", 0.90, T0 + 10_000.0, T0 + 5.0, 0);
    work(&mut v, T0 + 20.0, T0 + 60.0, "m", Some(0.5)); // 10 points that are known
    let now = T0 + 120.0;
    assert!(gate(&v.snap(now), None, Purpose::UserTurn { by_hand: false }, false, now).is_err());
    work(&mut v, T0 + 70.0, T0 + 100.0, "never-measured", Some(0.01)); // one that cannot be converted
    v.worked(Work { start: T0 + 80.0, finished: T0 + 100.0, model: "m".into(), cost: None, incomplete: true }, T0 + 100.0);
    assert!((v.uncounted("five_hour", now).unwrap() - 0.10).abs() < 1e-9, "the known part stays");
    assert!(gate(&v.snap(now), None, Purpose::UserTurn { by_hand: false }, false, now).is_err(), "still no admission at the limit");
    assert!(v.counting(now));
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

#[test]
fn hostile_text_never_panics_and_never_makes_a_time_up() {
    let line = |when: &str| format!("Current session: 10% used · resets {when} (X/Y)");
    for when in [
        "Oct 14, 11:29午前", // a multi-byte tail where the old code split the string
        "Oct 14, 11:29 午後",
        "11:29　ａｍ", // full-width "am"
        "Oct 14, 11:29éé",
        "Oct 14, 4:99am", // minutes out of range
        "Oct 14, 4:-5am",
        "Oct 14, 13:00pm", // hours out of range
        "Oct 14, 0:30am",
        "Oct 99, 4:59am", // days out of range
        "Oct 0, 4:59am",
        "Oct -3, 4:59am",
        "Oct 14, 99999999999999999999:00am",
        "Oct 9223372036854775807, 4:59am",
        "Oct 14, 4:9223372036854775807am",
        "Oct 14, éééam",
        "é",
        "am",
        ", am",
    ] {
        assert!(claude_usage_windows(&line(when), T0, 0).is_empty(), "{when}");
    }
    // Absurd clocks and offsets are met with a refusal, not an overflow.
    for offset in [i64::MAX, i64::MIN, i64::MAX / 2] {
        let _ = claude_usage_windows(&line("Oct 14, 4:59am"), T0, offset);
        let _ = claude_usage_windows(&line("4:59am"), f64::MAX, offset);
    }
    for percent in ["NaN", "inf", "-5", "1e999"] {
        let text = format!("Current session: {percent}% used · resets Oct 14, 4:59am (X/Y)");
        assert!(claude_usage_windows(&text, T0, 0).is_empty(), "{percent}");
    }
    // The ordinary spellings still read.
    assert_eq!(claude_usage_windows(&line("Oct 14, 11:29PM"), T0, 0).len(), 1);
    assert_eq!(claude_usage_windows(&line("Oct 14, 12:00am"), T0, 0).len(), 1);
}

// ---- the engine: readings taken by the watcher, at no model cost -------------------------------------------------------------

const WHEN: u64 = 1_791_620_000;
type Calls = Arc<Mutex<Vec<Vec<String>>>>;

fn usage_json(turns: u32) -> String {
    json!({ "type": "result", "is_error": false, "num_turns": turns, "total_cost_usd": 0, "usage": { "input_tokens": 0, "output_tokens": 0, "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0 }, "result": USAGE_TEXT }).to_string()
}

fn printing(out: String) -> Proc {
    let (mut w, r) = tokio::io::duplex(65_536);
    tokio::spawn(async move { drop(w.write_all(out.as_bytes()).await) });
    Proc { stdin: Box::new(tokio::io::sink()), stdout: Box::new(r), stderr: None, child: None, contained: None }
}

/// What the fake CLI saw and what it answers: the arguments of each `/usage` run, whether each was started as a background reader,
/// the text it prints (changeable), and whether a reading is held open until `release`.
#[derive(Clone, Default)]
struct Probe {
    calls: Calls,
    background: Arc<Mutex<Vec<bool>>>,
    status_background: Arc<Mutex<Vec<bool>>>,
    text: Arc<Mutex<String>>,
    hold: Arc<std::sync::atomic::AtomicBool>,
    held: Arc<Mutex<Vec<tokio::io::DuplexStream>>>,
}

impl Probe {
    fn saying(text: String) -> Probe {
        Probe { text: Arc::new(Mutex::new(text)), ..Default::default() }
    }

    /// Lets every held reading finish with the current text.
    async fn release(&self) {
        self.hold.store(false, Ordering::SeqCst);
        let (text, writers) = (self.text.lock().unwrap().clone(), std::mem::take(&mut *self.held.lock().unwrap()));
        for mut w in writers {
            drop(w.write_all(text.as_bytes()).await);
        }
    }

    /// Answers `/usage` as told, `auth status` with a plan, and hands anything else to `other`.
    fn spawner(&self, other: Option<Spawner>) -> Spawner {
        let p = self.clone();
        Arc::new(move |bin, args, cwd| {
            if args.iter().any(|a| a == "/usage") {
                p.calls.lock().unwrap().push(args.to_vec());
                p.background.lock().unwrap().push(faber_engine::proc::starting_in_background());
                if p.hold.load(Ordering::SeqCst) {
                    let (w, r) = tokio::io::duplex(65_536);
                    p.held.lock().unwrap().push(w);
                    return Ok(Proc {
                        stdin: Box::new(tokio::io::sink()),
                        stdout: Box::new(r),
                        stderr: None,
                        child: None,
                        contained: None,
                    });
                }
                Ok(printing(p.text.lock().unwrap().clone()))
            } else if args.iter().any(|a| a == "status") {
                p.status_background.lock().unwrap().push(faber_engine::proc::starting_in_background());
                Ok(printing(r#"{"loggedIn":true,"subscriptionType":"max"}"#.into()))
            } else if let Some(other) = &other {
                other(bin, args, cwd)
            } else {
                Err(std::io::Error::other("nothing else runs in this test"))
            }
        })
    }
}

fn spawner(text: String, other: Option<Spawner>) -> (Spawner, Calls) {
    let probe = Probe::saying(text);
    (probe.spawner(other), probe.calls)
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
    let probe = Probe::saying(usage_json(0));
    let calls = probe.calls.clone();
    let scheduled = probe.spawner(None);
    let status_background = probe.status_background.clone();
    let (engine, _) = start(
        dir.path(),
        Arc::new(move |bin, args, cwd| {
            if args.iter().any(|a| a == "status") {
                status_background.lock().unwrap().push(faber_engine::proc::starting_in_background());
                return Err(std::io::Error::other("the plan check cannot supply this test's no-model evidence"));
            }
            scheduled(bin, args, cwd)
        }),
    );
    engine.set_utc_offset(-10_800);
    engine.note_account_events(
        Vendor::Claude,
        vec![EngineEvent::Quota { window: "five_hour".into(), used: 0.09, resets_at: 1_791_627_030.0 }],
    );
    engine.watch_accounts();
    until("both windows read", || engine.ledger().claude.readings.len() == 2).await;
    let l = engine.ledger().claude;
    assert_eq!((l.readings["five_hour"].used, l.readings["five_hour"].resets_at), (0.10, 1_791_627_030.0));
    assert_eq!((l.readings["seven_day"].used, l.readings["seven_day"].resets_at), (0.36, 1_792_137_600.0));
    for window in ["five_hour", "seven_day"] {
        let evidence = l.zero_turn_readings.get(window).expect("a validated scheduled read records no-model evidence");
        assert_eq!(evidence.used, l.readings[window].used);
        assert_eq!(evidence.resets_at, l.readings[window].resets_at);
        assert_eq!(evidence.read_at, WHEN as f64);
        assert_eq!(Store::new(dir.path()).ledger().claude.zero_turn_readings[window].resets_at, evidence.resets_at);
    }
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
    assert_eq!(*probe.background.lock().unwrap(), vec![true], "the reading is started as a background reader (idle priority)");
    until("the plan check runs", || !probe.status_background.lock().unwrap().is_empty()).await;
    assert!(probe.status_background.lock().unwrap().iter().all(|b| *b), "the periodic account check is a background process too");
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
    assert!(engine.ledger().claude.zero_turn_readings.is_empty(), "model work cannot create no-model evidence");
    engine.shutdown();
}

#[tokio::test]
async fn focus_and_the_end_of_a_turn_read_again_after_the_debounce_and_a_hidden_window_does_not() {
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
    engine.set_attention(true, true);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        calls.lock().unwrap().len(),
        1,
        "showing the window is not a reason, and focusing it within five minutes of a reading is not either"
    );
    time.store(WHEN + 301, Ordering::SeqCst);
    engine.set_attention(true, true);
    until("a reading on focus", || calls.lock().unwrap().len() == 2).await;
    engine.set_attention(true, true);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(calls.lock().unwrap().len(), 2, "focus twice in a row reads once");

    time.store(WHEN + 700, Ordering::SeqCst);
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
    assert!(engine.ledger().claude.counting(WHEN as f64 + 701.0), "the turn just ended: the counter may not hold it yet");
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
    Store::new(dir.path()).save_ledger(&ledger).unwrap();
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
    let (inner, started) = (f.spawn.clone(), Arc::new(Mutex::new(vec![])));
    let seen = started.clone();
    let spawn: Spawner = Arc::new(move |bin, args, cwd| {
        seen.lock().unwrap().push(faber_engine::proc::starting_in_background());
        inner(bin, args, cwd)
    });
    let (engine, _) = start(dir.path(), spawn);
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
    assert_eq!(*started.lock().unwrap(), vec![true], "the one server it started was a background reader (idle priority)");
    engine.shutdown();
}

#[tokio::test]
async fn a_reading_still_under_way_is_not_started_a_second_time() {
    let dir = tempfile::tempdir().unwrap();
    let probe = Probe::saying(usage_json(0));
    probe.hold.store(true, Ordering::SeqCst);
    let (engine, time) = start(dir.path(), probe.spawner(None));
    engine.set_utc_offset(-10_800);
    engine.watch_accounts();
    until("the first reading starts", || probe.calls.lock().unwrap().len() == 1).await;
    // Long past every debounce the cadence would let a second reading through, but the first one has not answered.
    time.store(WHEN + 1200, Ordering::SeqCst);
    engine.set_attention(true, true);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(probe.calls.lock().unwrap().len(), 1, "one reading per vendor at a time");
    probe.release().await;
    until("the answer lands", || engine.ledger().claude.readings.len() == 2).await;
    time.store(WHEN + 2400, Ordering::SeqCst);
    engine.set_attention(true, true);
    until("and the next one runs once the first is over", || probe.calls.lock().unwrap().len() == 2).await;
    engine.shutdown();
}

#[tokio::test]
async fn a_reading_that_fails_does_not_leave_the_vendor_unread() {
    let dir = tempfile::tempdir().unwrap();
    let probe = Probe::saying("this is not json".into());
    let (engine, time) = start(dir.path(), probe.spawner(None));
    engine.set_utc_offset(-10_800);
    engine.watch_accounts();
    until("the failing reading", || probe.calls.lock().unwrap().len() == 1).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(engine.ledger().claude.readings.is_empty());
    assert!(engine.ledger().claude.zero_turn_readings.is_empty(), "a failed reading cannot create no-model evidence");
    *probe.text.lock().unwrap() = usage_json(0);
    time.store(WHEN + 400, Ordering::SeqCst);
    engine.set_attention(true, true);
    until("the next reading works", || engine.ledger().claude.readings.len() == 2).await;
    engine.shutdown();
}

#[tokio::test]
async fn a_running_turn_takes_the_discount_away_for_every_decision() {
    let dir = tempfile::tempdir().unwrap();
    // The CLI that answers the turn starts and then says nothing: the turn stays running.
    let silent: Spawner = Arc::new(|_, _, _| {
        let (w, r) = tokio::io::duplex(1024);
        std::mem::forget(w);
        Ok(Proc { stdin: Box::new(tokio::io::sink()), stdout: Box::new(r), stderr: None, child: None, contained: None })
    });
    let (engine, _) = start(dir.path(), spawner(usage_json(0), Some(silent)).0);
    let quota = |window: &str, used: f64, secs: u64| EngineEvent::Quota { window: window.into(), used, resets_at: (WHEN + secs) as f64 };
    engine.note_account_events(
        Vendor::Claude,
        vec![
            EngineEvent::Signals(Signals {
                plan: Some("max".into()),
                billing: Some(Billing::Subscription),
                paid_fallback: Some(false),
                ..Default::default()
            }),
            quota("five_hour", 0.05, 9000),
            quota("seven_day", 0.2, 3 * 86_400 + 12 * 3600),
        ],
    );
    assert_eq!(engine.state().usage[0].state, "spare");
    let ask = SendRequest {
        conversation_id: None,
        area: Area::Chat,
        project_id: None,
        text: "hello".into(),
        images: vec![],
        files: vec![],
        choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
    };
    engine.send(ask).unwrap();
    until("the turn runs", || engine.state().usage[0].counting).await;
    assert_eq!(engine.state().usage[0].state, "ok", "while a turn runs the reading is a floor: no quota to spare is believed");
    engine.shutdown();
}

#[tokio::test]
async fn no_reading_starts_once_the_engine_is_shutting_down() {
    let dir = tempfile::tempdir().unwrap();
    let silent: Spawner = Arc::new(|_, _, _| {
        let (w, r) = tokio::io::duplex(1024);
        std::mem::forget(w);
        Ok(Proc { stdin: Box::new(tokio::io::sink()), stdout: Box::new(r), stderr: None, child: None, contained: None })
    });
    let probe = Probe::saying(usage_json(0));
    let (engine, time) = start(dir.path(), probe.spawner(Some(silent)));
    engine.set_utc_offset(-10_800);
    engine.watch_accounts();
    until("the first reading", || probe.calls.lock().unwrap().len() == 1).await;
    let ask = SendRequest {
        conversation_id: None,
        area: Area::Chat,
        project_id: None,
        text: "hello".into(),
        images: vec![],
        files: vec![],
        choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
    };
    engine.send(ask).unwrap();
    until("the turn runs", || engine.state().usage[0].counting).await;
    // Far past every debounce: ending the turn would read again, but the app is closing.
    time.store(WHEN + 3600, Ordering::SeqCst);
    engine.shutdown();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(probe.calls.lock().unwrap().len(), 1);
}

use faber_engine::budget::{Billing, Reach};
use faber_engine::learn::Span;
use faber_engine::ledger::{Ledger, PLAN_FRESH, RELEARN, VendorLedger};
use faber_engine::store::{Disk, Fault, RealDisk, Store};
use std::path::Path;
use std::sync::Arc;
use std::{fs, io};

const HOUR: f64 = 3600.0;
const BASE: f64 = 345_600.0 + 2.0 * 86_400.0;
const RESET: f64 = BASE + 30.0 * 86_400.0;

fn near(got: f64, want: f64) {
    assert!((got - want).abs() < 1e-9, "{got} is not {want}");
}

#[test]
fn readings_that_make_no_sense_are_dropped() {
    let mut l = VendorLedger::default();
    for used in [f64::NAN, -0.1, 7.0] {
        l.record("five_hour", used, RESET, BASE, 0);
    }
    l.record("five_hour", 0.2, f64::INFINITY, BASE, 0);
    assert!(l.readings.is_empty() && l.history.is_empty());
    l.record("five_hour", 0.2, RESET, BASE, 0);
    assert_eq!(l.readings["five_hour"].used, 0.2);
}

#[test]
fn the_history_is_bounded() {
    let mut l = VendorLedger::default();
    for i in 0..1000 {
        l.record("five_hour", 0.2, RESET, BASE + i as f64 * 60.0, 0);
    }
    assert_eq!(l.history["five_hour"].len(), 200);
    for i in 0..1000 {
        l.span(Span { incomplete: false, start: i as f64, end: i as f64 + 1.0, model: "a".into(), cost: None });
    }
    assert_eq!(l.spans.len(), 200);
}

#[test]
fn the_shortest_vendor_wide_window_is_the_one_learned_from() {
    let mut l = VendorLedger::default();
    l.record("seven_day", 0.4, RESET, BASE, 0);
    assert_eq!(l.primary(BASE), Some("seven_day"));
    l.record("five_hour", 0.1, RESET, BASE, 0);
    l.record("seven_day_opus", 0.1, RESET, BASE, 0);
    assert_eq!(l.primary(BASE), Some("five_hour"));
    l.record("5_minutes", 0.1, RESET, BASE, 0);
    assert_eq!(l.primary(BASE), Some("5_minutes"));
}

#[test]
fn the_snapshot_carries_windows_with_their_length() {
    let mut l = VendorLedger::default();
    l.record("five_hour", 0.3, RESET, BASE, 0);
    l.record("seven_day_opus", 0.9, RESET, BASE, 0);
    l.billing = Billing::Subscription;
    l.reach = Reach::Out(Some(RESET));
    let snap = l.snap(BASE + 60.0);
    assert_eq!(snap.windows.len(), 2);
    assert_eq!(snap.windows.iter().find(|(n, _)| n == "five_hour").unwrap().1.length, 18_000.0);
    assert_eq!(snap.billing, Billing::Subscription);
    assert_eq!(snap.reach, Reach::Out(Some(RESET)));
}

#[test]
fn a_discount_needs_a_fresh_plan_or_a_typed_one() {
    let mut l = VendorLedger::default();
    assert!(!l.plan_ok(BASE));
    l.plan_detected(Some("max".into()), BASE);
    assert!(l.plan_ok(BASE + PLAN_FRESH - 1.0));
    assert!(!l.plan_ok(BASE + PLAN_FRESH + 1.0));
    assert!(!l.plan_ok(BASE - 10.0), "a clock that went back");
    l.typed_plan = Some("pro".into());
    assert!(l.plan_ok(BASE + 10.0 * HOUR));
}

#[test]
fn a_failed_check_does_not_refresh_and_three_in_a_row_ask_the_user() {
    let mut l = VendorLedger::default();
    l.plan_failed();
    l.plan_failed();
    assert!(!l.ask_for_plan(), "two failures ask nothing");
    assert!(!l.plan_ok(BASE));
    l.plan_failed();
    assert!(l.ask_for_plan());
    l.typed_plan = Some("pro".into());
    assert!(!l.ask_for_plan(), "already answered");
    let mut named_nothing = VendorLedger::default();
    named_nothing.plan_missing();
    assert!(named_nothing.ask_for_plan(), "the command worked but the field is missing");
    named_nothing.plan_detected(Some("max".into()), BASE);
    assert!(!named_nothing.ask_for_plan());
}

#[test]
fn a_different_plan_name_restarts_the_learning_and_the_same_one_does_not() {
    let mut l = VendorLedger::default();
    l.plan_detected(Some("pro".into()), BASE);
    l.weights.capacity = Some(10.0);
    l.plan_detected(Some("pro".into()), BASE + HOUR);
    assert_eq!(l.weights.capacity, Some(10.0));
    assert!(!l.relearning(BASE + HOUR));
    l.plan_detected(Some("max".into()), BASE + 2.0 * HOUR);
    assert_eq!(l.weights.capacity, None);
    assert!(l.relearning(BASE + 3.0 * HOUR));
    assert!(!l.relearning(BASE + 2.0 * HOUR + RELEARN + 1.0), "48 hours is enough");
    assert_eq!(l.since, BASE + 2.0 * HOUR);
}

#[test]
fn a_typed_plan_does_not_survive_a_detection_that_disagrees() {
    let mut l = VendorLedger { typed_plan: Some("pro".into()), ..VendorLedger::default() };
    l.plan_detected(Some("max".into()), BASE);
    assert_eq!(l.typed_plan, None);
}

/// Consecutive 15-second turns: every reading is a completion, starting from the preceding turn's baseline.
fn block(l: &mut VendorLedger, k: usize, points: f64, cost: f64) {
    let t = BASE + 15.0 * k as f64;
    let reset = BASE + 5.0 * HOUR;
    let used = 0.05 + points * k as f64 / 100.0;
    if k == 0 {
        l.record("five_hour", used, reset, t, 0);
    }
    l.span(Span { incomplete: false, start: t, end: t + 15.0, model: "a".into(), cost: Some(cost) });
    l.record("five_hour", used + points / 100.0, reset, t + 15.0, 0);
}

#[test]
fn calibration_fixtures_use_short_turns_and_completion_only_readings() {
    let mut l = VendorLedger::default();
    for k in 0..2 {
        block(&mut l, k, 3.0, 0.5);
    }
    assert!(l.spans.iter().all(|s| s.end - s.start == 15.0), "ordinary turns take 15 seconds in this fixture");
    let completions: Vec<_> = l.spans.iter().map(|s| s.end).collect();
    let readings: Vec<_> = l.history["five_hour"].iter().skip(1).map(|p| p.at).collect();
    assert_eq!(readings, completions, "after the previous completion's baseline, every reading ends a metered turn");
}

#[test]
fn a_capacity_that_jumps_is_a_plan_change_the_name_did_not_announce() {
    let mut l = VendorLedger::default();
    for k in 0..8 {
        block(&mut l, k, 3.0, 0.5);
    }
    let now = BASE + 8.0 * 15.0;
    assert!(!l.learn_weights(now));
    near(l.weights.capacity.unwrap(), 6.0);
    for k in 8..28 {
        block(&mut l, k, 3.0, 2.0);
    }
    let later = BASE + 28.0 * 15.0;
    assert!(l.learn_weights(later), "1.5 points per dollar against 6: an explained plan upgrade");
    assert_eq!(l.weights.capacity, None);
    assert_eq!(l.since, later);
    assert!(l.relearning(later + HOUR));
}

#[test]
fn outside_use_shows_in_the_snapshot_as_a_jump() {
    let mut l = VendorLedger::default();
    let now = BASE + 10.0 * HOUR;
    l.record("five_hour", 0.10, RESET, now - 2.0 * HOUR, 0);
    l.record("five_hour", 0.16, RESET, now, 0);
    assert!(l.snap(now).jumped);
    assert!(l.outside(now).outside > 0.0);
}

#[test]
fn the_week_is_remembered_but_the_moment_is_not() {
    let mut ledger = Ledger::default();
    ledger.claude.record("five_hour", 0.3, RESET, BASE, 0);
    ledger.claude.plan_detected(Some("max".into()), BASE);
    ledger.claude.reach = Reach::Out(None);
    ledger.claude.overage_active = true;
    ledger.claude.billing = Billing::UsageBilled;
    let json = serde_json::to_string(&ledger).unwrap();
    let back: Ledger = serde_json::from_str(&json).unwrap();
    assert_eq!(back.claude.readings["five_hour"].used, 0.3);
    assert_eq!(back.claude.plan.as_deref(), Some("max"));
    assert_eq!(back.claude.reach, Reach::Clear);
    assert!(!back.claude.overage_active);
    assert_eq!(back.claude.billing, Billing::Unknown);
    assert!(back.claude.plan_seen_at.is_none(), "a plan seen last week proves nothing now");
    assert!(json.len() < 20_000, "{}", json.len());
}

#[test]
fn a_file_that_is_not_a_ledger_is_no_ledger() {
    assert!(serde_json::from_str::<Ledger>("not json").is_err());
    // A file from a version that kept fewer fields still opens: every field has a default.
    let older: Ledger = serde_json::from_str("{}").unwrap();
    assert!(older.claude.readings.is_empty());
}

#[test]
fn a_full_ledger_stays_small() {
    let mut ledger = Ledger::default();
    for claude in [true, false] {
        let v = if claude { &mut ledger.claude } else { &mut ledger.codex };
        for window in ["five_hour", "seven_day", "seven_day_opus"] {
            for i in 0..500 {
                v.record(window, (i % 100) as f64 / 100.0, 1_791_627_000.5 + i as f64, 1_791_000_000.123 + i as f64 * 60.0, -10_800);
            }
        }
        for i in 0..500 {
            v.span(Span {
                incomplete: false,
                start: 1_791_000_000.123 + i as f64,
                end: 1_791_000_030.456 + i as f64,
                model: "claude-sonnet-5-5".into(),
                cost: Some(0.123456),
            });
        }
    }
    let size = serde_json::to_string(&ledger).unwrap().len();
    assert!(size < 100_000, "{size} bytes");
}

#[test]
fn a_ledger_read_from_a_damaged_file_cannot_hurt() {
    let json = r#"{"claude": {
        "readings": {"five_hour": {"used": -3.0, "resets_at": 1.0, "read_at": 1.0}, "seven_day": {"used": 0.4, "resets_at": 5.0, "read_at": 1.0}},
        "history": {"five_hour": [{"at": 1.0, "used": 9.0, "resets_at": 5.0}]},
        "spans": [{"start": 9.0, "end": 1.0, "model": "a", "cost": 0.1}, {"start": 1.0, "end": 2.0, "model": "a", "cost": -4.0}],
        "learned": {"mass": [1.0, 2.0], "covered": 5.0},
        "weights": {"w": {"a": 99.0, "b": 0.01}, "capacity": -1.0},
        "stretches": [{"model": "a", "ppd": 0.0}]
    }}"#;
    let mut ledger: Ledger = serde_json::from_str(json).unwrap();
    ledger.sanitize();
    let v = &ledger.claude;
    assert_eq!(v.readings.keys().collect::<Vec<_>>(), ["seven_day"]);
    assert!(v.history.is_empty() && v.spans.is_empty() && v.stretches.is_empty());
    assert_eq!(v.learned.mass.len(), 168, "a shape of the wrong length is reset");
    assert_eq!((v.weights.w["a"], v.weights.w["b"], v.weights.capacity), (2.0, 0.5, None));
    v.shape();
}

#[test]
fn a_damaged_ledger_blocks_projects_and_stays_quarantined_after_chat_rewrites_it() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("budget.json"), "{\"claude\": {\"readings\": ").unwrap();
    let store = Store::new(dir.path());
    assert_eq!(store.ledger_checked().unwrap_err(), Fault::Corrupt);
    // Chat only learns from the ledger, so it starts again from an empty one and may save over the damaged file.
    let chat = store.ledger();
    assert!(chat.claude.readings.is_empty());
    store.save_ledger(&chat).unwrap();
    assert_eq!(store.ledger_checked().unwrap_err(), Fault::Corrupt, "a rewrite does not turn damaged safety data into an empty budget");
    assert!(dir.path().join("budget.json.quarantine").is_file(), "the damaged copy is kept as evidence");
}

#[test]
fn a_missing_ledger_is_an_empty_one_and_a_good_one_reads_back() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    assert!(store.ledger_checked().unwrap().claude.readings.is_empty());
    let mut ledger = Ledger::default();
    ledger.claude.record("five_hour", 0.3, RESET, BASE, 0);
    store.save_ledger(&ledger).unwrap();
    assert_eq!(store.ledger_checked().unwrap().claude.readings["five_hour"].used, 0.3);
}

/// A disk with no room left for the temporary file.
struct FullDisk;
impl Disk for FullDisk {
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        fs::write(path, &bytes[..bytes.len() / 3])?;
        Err(io::Error::from(io::ErrorKind::StorageFull))
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        RealDisk.rename(from, to)
    }
}

#[test]
fn a_ledger_that_cannot_be_saved_says_why_and_keeps_the_old_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::default();
    ledger.codex.record("five_hour", 0.1, RESET, BASE, 0);
    Store::new(dir.path()).save_ledger(&ledger).unwrap();
    let full = Store::with_disk(dir.path(), Arc::new(FullDisk));
    ledger.codex.record("five_hour", 0.9, RESET, BASE + HOUR, 0);
    let err = full.save_ledger(&ledger).unwrap_err();
    assert_eq!((err, err.key()), (Fault::DiskFull, "error.store.full"));
    assert_eq!(full.ledger_checked().unwrap().codex.readings["five_hour"].used, 0.1);
    assert!(!dir.path().join("budget.json.tmp").exists());
}

#[test]
fn fields_a_newer_faber_wrote_survive_in_the_ledger() {
    let json = r#"{"daily": {"day": 3, "spent": 1.5}, "claude": {"since": 1.0, "grants": [{"id": "g1"}]}}"#;
    let ledger: Ledger = serde_json::from_str(json).unwrap();
    let back: serde_json::Value = serde_json::to_value(&ledger).unwrap();
    assert_eq!(back["daily"]["spent"], 1.5, "an older Faber must not drop a newer allocation");
    assert_eq!(back["claude"]["grants"][0]["id"], "g1");
    assert_eq!(back["claude"]["since"], 1.0);
}

#[test]
fn paid_fallback_survives_a_restart_and_missing_evidence_stays_conservative() {
    use faber_engine::budget::{Billing, Purpose, Why, gate};
    for flag in [Some(true), None] {
        let mut ledger = VendorLedger { paid_fallback: flag, ..VendorLedger::default() };
        ledger.record("seven_day", 0.95, RESET, BASE, 0);
        let mut restored: VendorLedger = serde_json::from_str(&serde_json::to_string(&ledger).unwrap()).unwrap();
        restored.billing = Billing::Subscription;
        assert_eq!(restored.paid_fallback, flag);
        assert_eq!(gate(&restored.snap(BASE), None, Purpose::Pulse, false, BASE), Err(Why::Paid));
    }
    let mut legacy: VendorLedger = serde_json::from_str("{}").unwrap();
    legacy.billing = Billing::Subscription;
    legacy.record("seven_day", 0.95, RESET, BASE, 0);
    assert_eq!(gate(&legacy.snap(BASE), None, Purpose::Router, false, BASE), Err(Why::Paid));
}

#[test]
fn expired_readings_leave_the_live_primary_and_are_pruned_on_refresh() {
    let mut l = VendorLedger::default();
    l.record("five_hour", 1.0, BASE + HOUR, BASE, 0);
    l.record("seven_day", 0.05, RESET, BASE, 0);
    assert_eq!(l.primary(BASE + 2.0 * HOUR), Some("seven_day"));
    assert_eq!(l.snap(BASE + 2.0 * HOUR).windows.len(), 1);
    l.record("seven_day", 0.06, RESET, BASE + 2.0 * HOUR, 0);
    assert!(!l.readings.contains_key("five_hour"));
}

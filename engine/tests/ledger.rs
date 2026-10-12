use faber_engine::budget::{Billing, Reach};
use faber_engine::learn::Span;
use faber_engine::ledger::{Ledger, PLAN_FRESH, RELEARN, VendorLedger};

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
        l.span(Span { start: i as f64, end: i as f64 + 1.0, model: "a".into(), cost: None });
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

/// Three hours of readings with one clean stretch in the middle: quiet, quiet, then `points` more with a turn of cost `cost`.
fn block(l: &mut VendorLedger, k: usize, points: f64, cost: f64) {
    let t = BASE + 3.0 * HOUR * k as f64;
    let used = 0.05 + points * k as f64 / 100.0;
    l.record("five_hour", used, RESET, t, 0);
    l.record("five_hour", used, RESET, t + HOUR, 0);
    l.span(Span { start: t + 1.2 * HOUR, end: t + 1.5 * HOUR, model: "a".into(), cost: Some(cost) });
    l.record("five_hour", used + points / 100.0, RESET, t + 2.0 * HOUR, 0);
}

#[test]
fn a_capacity_that_jumps_is_a_plan_change_the_name_did_not_announce() {
    let mut l = VendorLedger::default();
    for k in 0..8 {
        block(&mut l, k, 3.0, 0.5);
    }
    let now = BASE + 30.0 * HOUR;
    assert!(!l.learn_weights(now));
    near(l.weights.capacity.unwrap(), 6.0);
    for k in 8..28 {
        block(&mut l, k, 3.0, 0.125);
    }
    let later = BASE + 100.0 * HOUR;
    assert!(l.learn_weights(later), "24 points per cost unit against 6");
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

//! R132: the one-time waiver of the R131 day-line margin. Runs only where the harness feature is on (the workspace).
#![cfg(feature = "r132-harness")]
use faber_engine::budget::Billing;
use faber_engine::learn::Span;
use faber_engine::ledger::Ledger;
use faber_engine::model::Vendor;
use faber_engine::project::budget::r132::{self, Exception};
use faber_engine::project::budget::{Change, apply};
use serde_json::json;

const DAY: f64 = 86_400.0;
const RESET: f64 = 7.0 * DAY;
const NOW: f64 = 2.0 * DAY + 3600.0;

fn ledger() -> Ledger {
    let mut l = Ledger::default();
    for (vendor, model) in [(Vendor::Codex, "gpt-6.1-sol"), (Vendor::Claude, "claude-sonnet-5-5")] {
        let v = l.of_mut(vendor);
        v.billing = Billing::Subscription;
        v.paid_fallback = Some(false);
        v.paid_fallback_at = Some(NOW);
        for i in 0..=8 {
            let at = 2.0 * DAY - (8 - i) as f64 * 1800.0;
            v.record("seven_day", i as f64 * 0.03, RESET, at, 0);
            if i > 0 {
                v.span(Span { start: at - 1800.0, end: at, model: model.into(), cost: Some(0.25), incomplete: false });
            }
        }
        v.record("seven_day", 0.25, RESET, NOW, 0);
    }
    l
}

/// A vendor with no calibration and a fresh zero-turn weekly reading of `used` (the R131 tests' fixture).
fn uncalibrated(used: f64) -> Ledger {
    let mut l = ledger();
    for vendor in Vendor::ALL {
        let v = l.of_mut(vendor);
        v.history.clear();
        v.spans.clear();
        v.day_baselines.clear();
        v.daily_samples.clear();
        v.record("seven_day", used, RESET, NOW, 0);
    }
    let mut value = serde_json::to_value(&l).unwrap();
    for vendor in Vendor::ALL {
        value[vendor.name()]["zero_turn_readings"] = serde_json::json!({"seven_day":{"used":used,"resets_at":RESET,"read_at":NOW}});
    }
    let mut l: Ledger = serde_json::from_value(value).unwrap();
    for vendor in Vendor::ALL {
        l.of_mut(vendor).billing = Billing::Subscription;
    }
    l
}

fn allocate(l: &mut Ledger) -> Result<(), &'static str> {
    apply(l, Change::Allocate { vendor: Vendor::Codex, cap_micros: 2_000_000 }, NOW, 0).map(|_| ())
}

/// A ledger that remembers a waiver from an earlier process.
fn remembering(record: serde_json::Value) -> Ledger {
    let mut l = uncalibrated(0.45);
    l.extra.insert("r132Waiver".into(), record);
    l
}

#[test]
fn the_waiver_is_explicit_single_use_recorded_bound_to_one_mission_and_only_moves_the_margin() {
    let dir = tempfile::tempdir().unwrap();
    let marker = Exception::R132.marker(dir.path());
    // Over the line: refused without the waiver, as R131 requires.
    assert!(allocate(&mut uncalibrated(0.45)).is_err());
    assert!(!r132::admits("m1"));
    assert!(r132::waive(Exception::R132, dir.path(), "not a valid id!", &json!({})).is_err(), "it names a mission");
    r132::waive(Exception::R132, dir.path(), "m1", &json!({"codexWeeklyUsed":0.45})).unwrap();
    // F3: an allocation is waived only when it is made for the waived mission.
    assert!(allocate(&mut uncalibrated(0.45)).is_err(), "an allocation made for no mission keeps the margin");
    {
        let _other = r132::for_mission("m2");
        assert!(allocate(&mut uncalibrated(0.45)).is_err(), "an allocation made for another mission keeps it too");
    }
    let _m1 = r132::for_mission("m1");
    let mut first = uncalibrated(0.45);
    let admitted = allocate(&mut first);
    assert!(admitted.is_ok(), "the waived margin admits the first mission: {admitted:?}");
    // The ledger remembers the mission whose margin was waived.
    assert_eq!(first.extra.get("r132Waiver"), Some(&json!({"mission": "m1", "spent": false})));
    // It belongs to that mission alone: another mission's changes keep the R131 margin.
    assert!(r132::admits("m1") && !r132::admits("m2"), "another mission is not waived");
    // The mission cap, fresh zero-turn evidence and a valid reading still apply.
    assert!(apply(&mut uncalibrated(0.45), Change::Allocate { vendor: Vendor::Codex, cap_micros: 2_000_001 }, NOW, 0).is_err());
    let mut stale = uncalibrated(0.45);
    stale.of_mut(Vendor::Codex).zero_turn_readings.clear();
    assert!(allocate(&mut stale).is_err());
    assert!(allocate(&mut uncalibrated(1.01)).is_err());
    // F2: a ledger that remembers a waiver for another mission, or a spent one, refuses it: this is what a new process
    // finds after the marker was deleted. The same mission's unspent waiver continues.
    assert!(allocate(&mut remembering(json!({"mission": "m0", "spent": false}))).is_err(), "another mission was waived before");
    assert!(allocate(&mut remembering(json!({"mission": "m1", "spent": true}))).is_err(), "this waiver was spent before");
    assert!(allocate(&mut remembering(json!({"mission": "m1", "spent": false}))).is_ok(), "the same waiver continues");
    // The record names the exception and the mission; the marker file cannot be written twice.
    let text = std::fs::read_to_string(&marker).unwrap();
    assert!(text.contains("R132") && text.contains("codexWeeklyUsed") && text.contains("\"m1\""));
    assert!(r132::waive(Exception::R132, dir.path(), "m1", &json!({})).is_err(), "single use");
    // Deleting the marker, or another data folder or exception, does not reopen it while it is open or after it is spent.
    std::fs::remove_file(&marker).unwrap();
    assert!(r132::waive(Exception::R132, dir.path(), "m1", &json!({})).is_err(), "a deleted marker does not reopen it");
    let other = tempfile::tempdir().unwrap();
    assert!(r132::waive(Exception::R132, other.path(), "m2", &json!({})).is_err(), "another data folder does not either");
    assert!(r132::waive(Exception::R136, dir.path(), "m2", &json!({})).is_err(), "nor the other exception");
    // Another mission finishing its calibration does not spend it; the waived mission's does.
    r132::consume("m2");
    assert!(allocate(&mut uncalibrated(0.45)).is_ok());
    r132::consume("m1");
    assert!(allocate(&mut uncalibrated(0.45)).is_err(), "spent");
    assert!(!r132::admits("m1"));
    assert!(r132::waive(Exception::R132, other.path(), "m1", &json!({})).is_err(), "single use survives being spent");
    r132::close();
}

/// R136: a harness canary's fresh ledger keeps only the waiver records, and an unreadable ledger is not replaced.
#[test]
fn a_fresh_canary_ledger_keeps_the_waiver_records_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("budget.json");
    r132::fresh_ledger(dir.path()).unwrap();
    assert_eq!(serde_json::from_slice::<Ledger>(&std::fs::read(&path).unwrap()).unwrap().extra.len(), 0);
    let mut old = remembering(json!({"mission": "m1", "spent": true}));
    old.extra.insert("r136Waiver".into(), json!({"mission": "c1", "spent": false}));
    std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
    r132::fresh_ledger(dir.path()).unwrap();
    let fresh: Ledger = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(fresh.extra.get("r132Waiver"), Some(&json!({"mission": "m1", "spent": true})));
    assert_eq!(fresh.extra.get("r136Waiver"), Some(&json!({"mission": "c1", "spent": false})));
    assert!(fresh.of(Vendor::Codex).history.is_empty(), "the day sample does not carry over");
    std::fs::write(&path, b"not a ledger").unwrap();
    assert!(r132::fresh_ledger(dir.path()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"not a ledger");
}

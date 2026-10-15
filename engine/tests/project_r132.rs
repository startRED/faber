//! R132: the one-time waiver of the R131 day-line margin. Runs only where the harness feature is on (the workspace).
#![cfg(feature = "r132-harness")]
use faber_engine::budget::Billing;
use faber_engine::learn::Span;
use faber_engine::ledger::Ledger;
use faber_engine::model::Vendor;
use faber_engine::project::budget::{Change, apply, r132};

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

#[test]
fn the_waiver_is_explicit_single_use_recorded_and_only_moves_the_margin() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("r132.json");
    // Over the line: refused without the waiver, as R131 requires.
    assert!(allocate(&mut uncalibrated(0.45)).is_err());
    r132::waive(&marker, &serde_json::json!({"codexWeeklyUsed":0.45})).unwrap();
    let first = allocate(&mut uncalibrated(0.45));
    assert!(first.is_ok(), "the waived margin admits the first mission: {first:?}");
    // The mission cap, fresh zero-turn evidence and a valid reading still apply.
    assert!(apply(&mut uncalibrated(0.45), Change::Allocate { vendor: Vendor::Codex, cap_micros: 2_000_001 }, NOW, 0).is_err());
    let mut stale = uncalibrated(0.45);
    stale.of_mut(Vendor::Codex).zero_turn_readings.clear();
    assert!(allocate(&mut stale).is_err());
    assert!(allocate(&mut uncalibrated(1.01)).is_err());
    // The record names the exception; a second opening is refused even after the first closed.
    let text = std::fs::read_to_string(&marker).unwrap();
    assert!(text.contains("R132") && text.contains("codexWeeklyUsed"));
    assert!(r132::waive(&marker, &serde_json::json!({})).is_err(), "single use");
    r132::close();
    assert!(allocate(&mut uncalibrated(0.45)).is_err(), "closed again");
    assert!(r132::waive(&marker, &serde_json::json!({})).is_err(), "single use survives close");
}

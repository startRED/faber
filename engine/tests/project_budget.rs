//! The ledger's count of requests of unknown cost, and the old mission records that share its place on the ledger.
use faber_engine::ledger::Ledger;
use faber_engine::model::Vendor;
use faber_engine::project::budget::{Budget, finish_unbounded, recover_unbounded, start_unbounded};
use faber_engine::store::Store;
use serde_json::json;

const NOW: f64 = 2.0 * 86_400.0 + 3600.0;

/// What a version with missions wrote under `missionBudget`: grants, receipts and calibrations beside the counts.
fn old_ledger() -> Ledger {
    let mut l = Ledger::default();
    l.extra.insert(
        "missionBudget".into(),
        json!({
            "schema": 1, "allocations": {}, "bases": {}, "disabled": ["m0"], "unbounded": {"codex": 0}, "lastAt": 12.0,
            "grants": {"m1": {"capMicros": 250000, "workerMicros": 250000, "vendors": {"codex": 250000}, "reviewMicros": 1,
                "repairMicros": 1, "canaryMicros": 1, "paused": true}},
            "invocations": {}, "calibrations": {}, "futureField": {"keep": true}
        }),
    );
    l
}

#[test]
fn a_ledger_written_with_missions_still_loads_and_keeps_what_it_held() {
    let mut l = old_ledger();
    let budget = Budget::load(&l).unwrap();
    assert_eq!(budget.last_at, 12.0);
    start_unbounded(&mut l, Vendor::Codex, NOW).unwrap();
    finish_unbounded(&mut l, Vendor::Codex).unwrap();
    let kept = &l.extra["missionBudget"];
    assert_eq!(kept["grants"]["m1"]["capMicros"], 250000);
    assert_eq!(kept["futureField"]["keep"], true);
    assert_eq!(kept["unbounded"]["codex"], 0);
}

#[test]
fn a_request_of_unknown_cost_is_counted_until_it_ends() {
    let mut l = Ledger::default();
    assert_eq!(Budget::load(&l).unwrap().unbounded.len(), 0);
    start_unbounded(&mut l, Vendor::Codex, NOW).unwrap();
    start_unbounded(&mut l, Vendor::Codex, NOW + 1.0).unwrap();
    assert_eq!(Budget::load(&l).unwrap().unbounded[&Vendor::Codex], 2);
    finish_unbounded(&mut l, Vendor::Codex).unwrap();
    assert_eq!(Budget::load(&l).unwrap().unbounded[&Vendor::Codex], 1);
    assert_eq!(start_unbounded(&mut l, Vendor::Codex, f64::NAN), Err("reason.budget.clock"));
}

#[test]
fn a_request_a_dead_engine_left_counted_is_reconciled_once_as_unknown_spend() {
    let mut l = Ledger::default();
    start_unbounded(&mut l, Vendor::Codex, NOW).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    store.save_ledger(&l).unwrap();
    let mut saved = store.ledger_checked().unwrap();
    assert!(recover_unbounded(&mut saved).unwrap());
    assert!(!recover_unbounded(&mut saved).unwrap());
    assert_eq!(Budget::load(&saved).unwrap().unbounded[&Vendor::Codex], 0);
    assert_eq!(saved.codex.unmetered[&2], 1);
}

#[test]
fn a_damaged_count_is_corrupt_and_the_file_is_kept_aside_before_it_is_cleaned() {
    for value in [json!({}), json!({"schema": 999}), json!({"schema": 1, "unbounded": "x", "lastAt": 0.0}), serde_json::Value::Null] {
        let mut l = Ledger::default();
        l.extra.insert("missionBudget".into(), value);
        assert_eq!(Budget::load(&l).err(), Some("error.store.corrupt"));
    }
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let mut l = Ledger::default();
    l.extra.insert("missionBudget".into(), json!({}));
    store.save_ledger(&l).unwrap();
    assert!(store.ledger_checked().is_err());
    assert!(dir.path().join("budget.json.quarantine").is_file());
}

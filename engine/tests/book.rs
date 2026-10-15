//! Scars carry proven regression checks, never prompt rules or failure text.
use crate::support;
use faber_engine::book::{Scar, ScarStore};
use faber_engine::project::wire::{ArtifactRef, CheckKind, CheckOutcome, CheckReceipt, CheckResult};
use support::tempdir;
fn receipt(id: &str, check: &str, tree: &str, result: CheckResult) -> CheckReceipt {
    CheckReceipt {
        schema: 1,
        id: id.into(),
        task: "sentinel".into(),
        candidate: tree.into(),
        checks: vec![CheckOutcome {
            id: check.into(),
            kind: CheckKind::Program,
            result,
            evidence: vec![ArtifactRef { path: "proofs/sentinel.log".into(), sha256: "a".repeat(64) }],
            reason: if result == CheckResult::Pass { None } else { Some("reason.test.red".into()) },
        }],
        artifacts: vec![],
        model: "engine".into(),
        effort: "none".into(),
        cost_micros: Some(0),
        effect_receipts: vec![],
        extra: Default::default(),
    }
}
#[test]
fn only_red_green_exact_check_evidence_enters_the_scar_store() {
    let dir = tempdir();
    let mut store = ScarStore::open(dir.path()).unwrap();
    for id in ["AS-1", "AS-2", "AS-3", "AS-4"] {
        let scar = Scar {
            schema: 1,
            id: id.into(),
            paths: vec!["ui".into()],
            check: format!("sentinel.{id}"),
            red_tree: "1".repeat(40),
            green_tree: "2".repeat(40),
            red_receipt: format!("red-{id}"),
            green_receipt: format!("green-{id}"),
        };
        let red = receipt(&scar.red_receipt, &scar.check, &scar.red_tree, CheckResult::Fail);
        let green = receipt(&scar.green_receipt, &scar.check, &scar.green_tree, CheckResult::Pass);
        assert!(store.record(scar.clone(), &green, &green).is_err());
        assert!(store.record(scar.clone(), &red, &red).is_err());
        store.record(scar, &red, &green).unwrap();
    }
    let restored = ScarStore::open(dir.path()).unwrap();
    assert_eq!(restored.checks_for(&["ui/Projects.tsx".into()]).len(), 4);
    assert!(restored.checks_for(&["engine/src/run.rs".into()]).is_empty());
    let text = std::fs::read_to_string(dir.path().join("scars.json")).unwrap();
    let mut records: serde_json::Value = serde_json::from_str(&text).unwrap();
    records[0]["rule"] = "Never put scar rules in a prompt".into();
    std::fs::write(dir.path().join("scars.json"), serde_json::to_vec(&records).unwrap()).unwrap();
    assert!(ScarStore::open(dir.path()).is_err());
}
#[test]
fn unknown_or_corrupt_scar_records_are_not_an_empty_store() {
    let dir = tempdir();
    std::fs::write(dir.path().join("scars.json"), "{broken").unwrap();
    assert!(ScarStore::open(dir.path()).is_err());
}

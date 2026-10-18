use faber_engine::project::wire::{ArtifactRef, CheckKind, CheckOutcome, CheckResult, KitPin, Labels, WireError, check_id};
use serde_json::json;

#[test]
fn check_ids_use_the_published_dotted_ids() {
    for id in ["proof.cargo-fmt", "proof.osv-scan", "C1", "screen.keyboard"] {
        assert_eq!(check_id(id), Ok(()), "{id}");
    }
    for id in ["", "has space", "a/b", "semi;colon", &"x".repeat(129)] {
        assert_eq!(check_id(id), Err(WireError::Malformed), "{id:?}");
    }
}

#[test]
fn credential_directories_are_forbidden_even_inside_a_worktree() {
    for path in [".codex/auth.json", "nested/.claude/session.json", ".gemini/oauth.json"] {
        let reference = ArtifactRef { path: path.into(), sha256: "0".repeat(64) };
        assert_eq!(reference.validate(), Err(WireError::Path), "{path}");
    }
}

fn outcome(result: CheckResult, evidence: Vec<ArtifactRef>, reason: Option<&str>) -> CheckOutcome {
    CheckOutcome { id: "screen.keyboard".into(), kind: CheckKind::Program, result, evidence, reason: reason.map(String::from) }
}

#[test]
fn check_receipt_claims_cannot_infer_a_pass_without_evidence() {
    let proof = || vec![ArtifactRef { path: "evidence/keyboard.json".into(), sha256: "0".repeat(64) }];
    assert_eq!(outcome(CheckResult::Pass, vec![], None).validate(), Err(WireError::Malformed));
    assert_eq!(outcome(CheckResult::NotRun, vec![], None).validate(), Err(WireError::Malformed));
    assert_eq!(outcome(CheckResult::NotRun, vec![], Some("No hidden app evidence")).validate(), Ok(()));
    assert!(!outcome(CheckResult::NotRun, vec![], Some("No hidden app evidence")).is_pass());
    assert!(outcome(CheckResult::Pass, proof(), None).is_pass());
}

#[test]
fn label_limits_and_nested_unknown_kit_fields_remain_strict() {
    let labels = |needs: serde_json::Value| -> Result<Labels, serde_json::Error> {
        serde_json::from_value(json!({"verb":"research","needs":needs,"risk":"low","fresh":"stable"}))
    };
    assert!(labels(json!(["lang:rust"])).unwrap().validate().is_ok());
    for bad in [json!(["not-a-need"]), json!(["lang:rust!!"]), json!(vec!["lang:rust"; 9])] {
        assert!(labels(bad).unwrap().validate().is_err());
    }
    assert!(serde_json::from_value::<KitPin>(json!({"receipt":"a-1","digest":"0".repeat(64)})).is_ok());
    assert!(serde_json::from_value::<KitPin>(json!({"receipt":"a-1","digest":"0".repeat(64),"install":true})).is_err());
}

//! Exact-tree queue gates; all receipts are fixture observations.
use faber_engine::model::Vendor;
use faber_engine::project::proof::{Definition, Observation, ProofSet};
use faber_engine::project::queue::{Candidate, Queue, Review};
use faber_engine::project::wire::ArtifactRef;
use std::collections::BTreeMap;
fn candidate(id: &str) -> Candidate {
    Candidate::new(
        id,
        "1".repeat(40),
        "2".repeat(40),
        3,
        Vendor::Codex,
        "input".into(),
        vec!["engine/src/a.rs".into()],
        proof().definitions().to_vec(),
    )
    .unwrap()
}
fn proof() -> ProofSet {
    let d = Definition {
        id: "rust".into(),
        version: 1,
        program: "/native/cargo".into(),
        argv: vec!["t".into()],
        checks: vec!["C1".into()],
        toolchain: "rust-1".into(),
        dependencies: "deps-1".into(),
        environment: BTreeMap::new(),
        heavy: true,
    };
    let mut p = ProofSet::new("2".repeat(40), "input".into(), vec![d], vec!["engine/src/a.rs".into()]).unwrap();
    p.record(
        "r1",
        "rust",
        Observation {
            exit: Some(0),
            stdout: b"pass".to_vec(),
            stderr: vec![],
            evidence: ArtifactRef { path: "proofs/rust.log".into(), sha256: faber_engine::project::lease::hash(b"pass") },
        },
    )
    .unwrap();
    p
}
fn review() -> Review {
    Review {
        vendor: Vendor::Claude,
        tree: "2".repeat(40),
        proof_digest: proof().digest(),
        session: "fresh-review-1".into(),
        fresh: true,
        accepted: true,
        findings: vec![],
    }
}
#[test]
fn exact_composed_tree_proofs_precede_fresh_other_vendor_review_and_cas() {
    let mut q = Queue::default();
    q.enqueue(candidate("w1")).unwrap();
    assert!(q.review("w1", review()).is_err());
    q.prove("w1", proof()).unwrap();
    assert!(q.accepted("w1", &"1".repeat(40), 3).is_err());
    q.review("w1", review()).unwrap();
    assert!(q.accepted("w1", &"1".repeat(40), 3).is_ok());
    assert!(q.accepted("w1", &"3".repeat(40), 3).is_err());
    assert!(q.accepted("w1", &"1".repeat(40), 4).is_err());
}
#[test]
fn unavailable_same_vendor_reused_session_wrong_tree_and_stale_proofs_block_landing() {
    for r in [
        Review { vendor: Vendor::Codex, ..review() },
        Review { fresh: false, ..review() },
        Review { tree: "3".repeat(40), ..review() },
        Review { proof_digest: "stale".into(), ..review() },
        Review { accepted: false, findings: vec!["defect".into()], ..review() },
    ] {
        let mut q = Queue::default();
        q.enqueue(candidate("w1")).unwrap();
        q.prove("w1", proof()).unwrap();
        assert!(q.review("w1", r).is_err());
        assert!(q.accepted("w1", &"1".repeat(40), 3).is_err());
    }
    let mut q = Queue::default();
    q.enqueue(candidate("w1")).unwrap();
    q.prove("w1", proof()).unwrap();
    q.review("w1", review()).unwrap();
    q.recompose("w1", "1".repeat(40), "2".repeat(40), "input".into()).unwrap();
    q.prove("w1", proof()).unwrap();
    assert!(q.review("w1", review()).is_err());
}
#[test]
fn repairs_and_head_changes_repeat_both_gates_and_conflicts_keep_artifacts() {
    let mut q = Queue::default();
    q.enqueue(candidate("w1")).unwrap();
    q.prove("w1", proof()).unwrap();
    q.review("w1", review()).unwrap();
    q.recompose("w1", "3".repeat(40), "4".repeat(40), "changed".into()).unwrap();
    assert!(q.accepted("w1", &"3".repeat(40), 3).is_err());
    assert!(q.review("w1", review()).is_err());
    q.conflict("w1", vec!["artifacts/left.patch".into(), "artifacts/right.patch".into()]).unwrap();
    assert_eq!(q.get("w1").unwrap().conflicts.len(), 2);
    let restored: Queue = serde_json::from_str(&serde_json::to_string(&q).unwrap()).unwrap();
    assert_eq!(restored.get("w1").unwrap().conflicts.len(), 2);
}
#[test]
fn queue_is_serial_and_bounded_without_marking_queued_work_done() {
    let mut q = Queue::default();
    q.enqueue(candidate("w1")).unwrap();
    q.enqueue(candidate("w2")).unwrap();
    assert!(q.enqueue(candidate("w3")).is_err());
    assert_eq!(q.front().unwrap().id, "w1");
    assert!(q.accepted("w2", &"1".repeat(40), 3).is_err());
}

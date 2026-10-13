//! Scheduling and proof receipts replay observations; no real process.
use faber_engine::project::proof::{Definition, Observation, ProofSet};
use faber_engine::project::run::{Capacity, schedule};
use faber_engine::project::wire::{ArtifactRef, CheckKind, CheckResult};
use faber_engine::project::{Contract, Decision, DecisionKind, Plan, Task, TaskKind, TaskState};
use std::collections::{BTreeMap, BTreeSet};
fn task(id: &str, kind: TaskKind) -> Task {
    let mut t = Task::new(id, id, kind);
    t.checks = vec!["C1".into()];
    t.paths = vec!["engine/src/a.rs".into()];
    t
}
fn capacity() -> Capacity {
    Capacity { cost: true, daily: true, ram_slots: 8, review_slots: 1, waiting_candidates: 0, writer_active: false }
}
#[test]
fn every_eligible_node_starts_without_waves_and_only_one_source_writer() {
    let p = Plan::new(
        "m1",
        1,
        vec![task("a", TaskKind::Research), task("b", TaskKind::TestProposal), task("w", TaskKind::Writer), task("x", TaskKind::Writer)],
    );
    let ready = schedule(&p, &[], &[], capacity()).unwrap();
    assert_eq!(ready.start, vec!["a", "b", "w"]);
    assert_eq!(ready.wait["x"], "reason.lease.conflict");
}
#[test]
fn resource_brakes_explain_each_wait_without_polling() {
    let p = Plan::new("m1", 1, vec![task("w", TaskKind::Writer)]);
    for (c, key) in [
        (Capacity { cost: false, ..capacity() }, "reason.budget.cap"),
        (Capacity { daily: false, ..capacity() }, "reason.budget.daily"),
        (Capacity { ram_slots: 0, ..capacity() }, "reason.ram"),
        (Capacity { review_slots: 0, ..capacity() }, "reason.review.capacity"),
        (Capacity { waiting_candidates: 2, ..capacity() }, "reason.queue.capacity"),
    ] {
        assert_eq!(schedule(&p, &[], &[], c).unwrap().wait["w"], key);
    }
}
#[test]
fn a_decision_or_stale_contract_parks_only_its_dependents_and_critical_path_goes_first() {
    let mut a = task("a", TaskKind::Research);
    a.decisions = vec!["d".into()];
    let mut b = task("b", TaskKind::Research);
    b.requires = vec!["a".into()];
    let c = task("c", TaskKind::Research);
    let p = Plan::new("m1", 1, vec![a, b, c]);
    let d = Decision::new("d", "a", "Choose", DecisionKind::Money);
    let ready = schedule(&p, &[], &[d], capacity()).unwrap();
    assert_eq!(ready.start, vec!["c"]);
    assert!(ready.blocked.contains_key("b"));
    let mut p = p;
    p.tasks[0].decisions.clear();
    assert_eq!(schedule(&p, &[], &[], capacity()).unwrap().start, vec!["a", "c"]);
    let contract = Contract::new("c1", "lead", &"1".repeat(40)).seal();
    p.tasks[0].contracts = vec![contract.pin()];
    let mut changed = contract;
    changed.version = 2;
    changed = changed.seal();
    assert!(schedule(&p, &[changed], &[], capacity()).unwrap().blocked.contains_key("a"));
}
#[test]
fn unfinished_dependencies_cannot_be_done_and_cyclic_plans_fail() {
    let mut a = task("a", TaskKind::Research);
    a.requires = vec!["b".into()];
    let mut b = task("b", TaskKind::Research);
    b.requires = vec!["a".into()];
    assert!(schedule(&Plan::new("m1", 1, vec![a, b]), &[], &[], capacity()).is_err());
    let mut t = task("w", TaskKind::Writer);
    t.state = TaskState::Running;
    assert!(schedule(&Plan::new("m1", 1, vec![t]), &[], &[], capacity()).unwrap().start.is_empty());
}
fn definition() -> Definition {
    Definition {
        id: "rust".into(),
        version: 1,
        program: "/native/cargo".into(),
        argv: vec!["t".into()],
        checks: vec!["C1".into()],
        toolchain: "rust-1".into(),
        dependencies: "deps-1".into(),
        environment: BTreeMap::new(),
        heavy: true,
    }
}
fn observation() -> Observation {
    Observation {
        exit: Some(0),
        stdout: b"pass".to_vec(),
        stderr: vec![],
        evidence: ArtifactRef { path: "proofs/rust.log".into(), sha256: faber_engine::project::lease::hash(b"pass") },
    }
}
#[test]
fn executable_receipts_bind_the_exact_tree_definition_environment_and_recorded_test_selection() {
    let mut set = ProofSet::new("1".repeat(40), "input".into(), vec![definition()], vec!["engine/src/a.rs".into()]).unwrap();
    assert_eq!(set.definitions()[0].argv, vec!["t", "--all"]); // Clean candidates cannot infer changed tests from status.
    let receipt = set.record("r1", "rust", observation()).unwrap();
    assert_eq!(receipt.checks[0].result, CheckResult::Pass);
    assert_eq!(receipt.checks[0].kind, CheckKind::Program);
    assert!(set.verified());
    assert!(set.reusable(&"1".repeat(40), "input", set.definitions(), &["engine/src/a.rs".into()]));
    assert!(!set.reusable(&"2".repeat(40), "input", set.definitions(), &["engine/src/a.rs".into()]));
    let mut defs = set.definitions().to_vec();
    defs[0].environment.insert("FLAG".into(), "new".into());
    assert!(!set.reusable(&"1".repeat(40), "input", &defs, &["engine/src/a.rs".into()]));
}
#[test]
fn missing_failed_or_forged_program_evidence_never_becomes_verified_done() {
    for exit in [None, Some(1)] {
        let mut set = ProofSet::new("1".repeat(40), "input".into(), vec![definition()], vec![]).unwrap();
        let mut o = observation();
        o.exit = exit;
        set.record("r1", "rust", o).unwrap();
        assert!(!set.verified());
    }
    let mut set = ProofSet::new("1".repeat(40), "input".into(), vec![definition()], vec![]).unwrap();
    let mut o = observation();
    o.evidence.sha256 = "a".repeat(64);
    assert!(set.record("r1", "rust", o).is_err());
    assert!(!set.verified());
    let mut d = definition();
    d.program = "cmd.exe".into();
    assert!(ProofSet::new("1".repeat(40), "input".into(), vec![d], vec![]).is_err());
}
#[test]
fn partial_reruns_invalidate_only_readers_and_descendants() {
    let mut a = task("a", TaskKind::Research);
    a.paths = vec!["one.rs".into()];
    let mut b = task("b", TaskKind::Research);
    b.requires = vec!["a".into()];
    let mut c = task("c", TaskKind::Research);
    c.paths = vec!["two.rs".into()];
    let p = Plan::new("m1", 1, vec![a, b, c]);
    assert_eq!(faber_engine::project::run::invalidated(&p, &["one.rs".into()]), BTreeSet::from(["a".into(), "b".into()]));
}

#[test]
fn only_one_heavy_proof_owns_the_build_lane() {
    let mut other = definition();
    other.id = "other".into();
    let mut set = ProofSet::new("1".repeat(40), "input".into(), vec![definition(), other], vec![]).unwrap();
    set.start("rust").unwrap();
    assert!(set.start("other").is_err());
    assert!(!set.verified());
    set.record("r1", "rust", observation()).unwrap();
    set.start("other").unwrap();
}

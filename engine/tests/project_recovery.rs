//! Safety persistence, retirement and disk guard; no real git or process.
use faber_engine::model::Vendor;
use faber_engine::project::disk::{DiskBudget, ResourceKind, Resources};
use faber_engine::project::lease::Termination;
use faber_engine::project::run::Runner;
use faber_engine::project::store::Journal;
use faber_engine::project::worker::{ModelPin, Worker};
use faber_engine::project::{Brief, ContractPin, Intent, IntentKind, Mission, MissionState, ProcessId, Role};
fn mission() -> Mission {
    Mission::new("m1", "p1", "repo", "Fix", &"1".repeat(40), 3, 1)
}
fn pin() -> ModelPin {
    ModelPin { vendor: Vendor::Codex, model: "gpt-6.1-sol".into(), effort: "medium".into(), cli_version: "0.162.1".into() }
}
#[test]
fn pause_resume_reconciles_pending_effects_without_replay_or_done() {
    let repo = tempdir();
    let data = tempdir();
    let j = Journal::open(repo.path(), data.path(), "m1").unwrap();
    let mut r = Runner::create(&j, mission()).unwrap();
    r.pause(&j).unwrap();
    assert_eq!(r.mission.state, MissionState::Paused);
    j.record_intent(&Intent::new("dispatch", "m1", IntentKind::Dispatch, 3, 1)).unwrap();
    assert!(r.resume(&j, 3).is_err());
    let restored = Runner::load(&j).unwrap();
    assert_eq!(restored.mission.state, MissionState::Paused);
    assert_eq!(j.pending().unwrap().len(), 1);
    assert!(r.resume(&j, 4).is_err());
    assert!(restored.queue.front().is_none());
}
#[test]
fn reflex_budget_and_session_state_survive_crash_and_real_dispatch_stays_under_t2() {
    let repo = tempdir();
    let data = tempdir();
    let j = Journal::open(repo.path(), data.path(), "m1").unwrap();
    let mut r = Runner::create(&j, mission()).unwrap();
    let mut b = Brief::new("w1", 1, Role::Research, "Read", &"1".repeat(40), ContractPin { id: "c1".into(), digest: "a".repeat(64) });
    b.grant_micros = Some(100);
    b.stop = "Done or blocked".into();
    b.scope = vec!["one.rs".into()];
    let mut w = Worker::new(b, pin(), 12).unwrap();
    w.begin_turn(&pin()).unwrap();
    w.failed_proof("rust", "same").unwrap();
    w.failed_proof("rust", "same").unwrap();
    r.workers.insert("w1".into(), w);
    r.save(&j).unwrap();
    let mut recovered = Runner::load(&j).unwrap();
    assert_eq!(recovered.workers["w1"].spent_turns, 1);
    assert_eq!(recovered.workers.get_mut("w1").unwrap().failed_proof("rust", "same"), Err("reason.loop"));
    assert!(recovered.admit(Vendor::Codex).is_err());
    assert!(recovered.admit(Vendor::Claude).is_err());
    assert!(j.pending().unwrap().is_empty());
}
#[test]
fn r106_low_space_unknown_measurement_and_disk_budget_pause_new_builds() {
    let b = DiskBudget { limit_bytes: 1000, reserve_bytes: 100 };
    assert!(b.admit(Some(500), 200, 300).is_ok());
    assert!(b.admit(Some(399), 200, 300).is_err());
    assert!(b.admit(Some(5000), 800, 201).is_err());
    assert!(b.admit(None, 0, 1).is_err());
    assert!(b.admit(Some(u64::MAX), u64::MAX, 1).is_err());
}
#[test]
fn r106_unused_build_temp_and_old_logs_are_deleted_but_live_unknown_or_needed_resources_stay() {
    let repo = tempdir();
    let data = tempdir();
    let j = Journal::open(repo.path(), data.path(), "m1").unwrap();
    let mut resources = Resources::load(&j).unwrap();
    for (n, kind) in [("build", ResourceKind::Build), ("temp", ResourceKind::Temp), ("log", ResourceKind::Log)] {
        let path = resources.register(&j, n, kind, None).unwrap();
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("output"), b"data").unwrap();
        assert!(resources.clean(&j, n, Termination { process: None, tree_dead: true, cost_settled: true }).is_err());
        resources.unneeded(&j, n).unwrap();
        assert!(resources.clean(&j, n, Termination { process: None, tree_dead: false, cost_settled: true }).is_err());
        assert!(path.exists());
        resources.clean(&j, n, Termination { process: None, tree_dead: true, cost_settled: true }).unwrap();
        assert!(!path.exists());
    }
    let process = ProcessId { pid: 10, started_at: 42 };
    let path = resources.register(&j, "live", ResourceKind::Build, Some(process)).unwrap();
    std::fs::create_dir_all(&path).unwrap();
    resources.unneeded(&j, "live").unwrap();
    assert!(
        resources
            .clean(&j, "live", Termination { process: Some(ProcessId { pid: 10, started_at: 43 }), tree_dead: true, cost_settled: true })
            .is_err()
    );
    assert!(resources.clean(&j, "live", Termination { process: Some(process), tree_dead: true, cost_settled: false }).is_err());
    assert!(path.exists());
    assert!(j.private().join("effects.json").exists());
    let restored = Resources::load(&j).unwrap();
    assert_eq!(restored.entries().len(), 1);
}
#[test]
fn r106_cleanup_refuses_path_traversal_links_and_pending_recovery() {
    let repo = tempdir();
    let data = tempdir();
    let j = Journal::open(repo.path(), data.path(), "m1").unwrap();
    let mut resources = Resources::load(&j).unwrap();
    assert!(resources.register(&j, "../outside", ResourceKind::Build, None).is_err());
    let path = resources.register(&j, "b", ResourceKind::Build, None).unwrap();
    std::fs::create_dir_all(&path).unwrap();
    resources.unneeded(&j, "b").unwrap();
    j.record_intent(&Intent::new("unknown", "m1", IntentKind::Commit, 3, 1)).unwrap();
    assert!(resources.clean(&j, "b", Termination { process: None, tree_dead: true, cost_settled: true }).is_err());
    assert!(path.exists());
}

#[test]
fn low_space_guard_pauses_runner_before_creating_build_resources() {
    let repo = tempdir();
    let data = tempdir();
    let j = Journal::open(repo.path(), data.path(), "m1").unwrap();
    let mut r = Runner::create(&j, mission()).unwrap();
    assert!(r.prepare_build_sample(&j, DiskBudget { limit_bytes: 100, reserve_bytes: 10 }, 50, Some(59)).is_err());
    assert_eq!(r.mission.state, MissionState::Paused);
    assert_eq!(r.mission.reason.as_deref(), Some("reason.disk.low"));
    assert!(Resources::load(&j).unwrap().entries().is_empty());
}

#[test]
fn stopped_runner_never_admits_builds_even_with_ample_disk_space() {
    let repo = tempdir();
    let data = tempdir();
    let j = Journal::open(repo.path(), data.path(), "m1").unwrap();
    let mut r = Runner::create(&j, mission()).unwrap();
    r.pause(&j).unwrap();
    assert!(r.prepare_build_sample(&j, DiskBudget { limit_bytes: 1000, reserve_bytes: 1 }, 10, Some(100)).is_err());
}

#[test]
fn durable_write_fault_closes_runner_admission_and_keeps_the_old_checkpoint() {
    use faber_engine::store::{Disk, RealDisk};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    struct Full(AtomicBool);
    impl Disk for Full {
        fn write(&self, p: &std::path::Path, b: &[u8]) -> std::io::Result<()> {
            if self.0.load(Ordering::SeqCst) { Err(std::io::Error::from(std::io::ErrorKind::StorageFull)) } else { RealDisk.write(p, b) }
        }
        fn rename(&self, a: &std::path::Path, b: &std::path::Path) -> std::io::Result<()> {
            RealDisk.rename(a, b)
        }
    }
    let repo = tempdir();
    let data = tempdir();
    let disk = Arc::new(Full(AtomicBool::new(false)));
    let j = Journal::open_with(repo.path(), data.path(), "m1", disk.clone()).unwrap();
    let mut r = Runner::create(&j, mission()).unwrap();
    disk.0.store(true, Ordering::SeqCst);
    assert!(r.pause(&j).is_err());
    assert_eq!(r.admit(Vendor::Codex), Err("error.store.write"));
    assert_eq!(Runner::load(&j).unwrap().mission.state, MissionState::Planning);
}

#[test]
fn completions_require_the_engine_receipt_for_the_exact_candidate_and_input() {
    use faber_engine::project::proof::{Definition, Observation, ProofSet};
    use faber_engine::project::wire::{ArtifactRef, Complete, Verbatim};
    let repo = tempdir();
    let data = tempdir();
    let j = Journal::open(repo.path(), data.path(), "m1").unwrap();
    let mut r = Runner::create(&j, mission()).unwrap();
    let mut b = Brief::new("w1", 1, Role::Research, "Read", &"1".repeat(40), ContractPin { id: "c1".into(), digest: "a".repeat(64) });
    b.grant_micros = Some(100);
    b.stop = "Done or blocked".into();
    b.scope = vec!["one.rs".into()];
    b.checks = vec![Verbatim { id: "C1".into(), text: "Verify".into() }];
    let w = Worker::new(b, pin(), 12).unwrap();
    let digest = w.digest();
    r.workers.insert("w1".into(), w);
    let def = Definition {
        id: "rust".into(),
        version: 1,
        program: std::env::temp_dir().join("cargo"),
        argv: vec!["t".into()],
        checks: vec!["C1".into()],
        toolchain: "rust-1".into(),
        dependencies: "deps-1".into(),
        environment: Default::default(),
        heavy: true,
    };
    let mut proof = ProofSet::new("2".repeat(40), digest, vec![def], vec!["one.rs".into()]).unwrap();
    let receipt = proof
        .record(
            "green",
            "rust",
            Observation {
                exit: Some(0),
                stdout: b"pass".to_vec(),
                stderr: vec![],
                evidence: ArtifactRef { path: "proofs/rust.log".into(), sha256: faber_engine::project::lease::hash(b"pass") },
            },
        )
        .unwrap();
    j.record_check(&receipt).unwrap();
    let complete: Complete = serde_json::from_value(serde_json::json!({"summary":"Done","next":"review","receipt":"green"})).unwrap();
    assert!(r.completion(&j, "w1", &"2".repeat(40), &complete).unwrap());
    assert!(r.completion(&j, "w1", &"3".repeat(40), &complete).is_err());
}

fn tempdir() -> tempfile::TempDir {
    tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
}

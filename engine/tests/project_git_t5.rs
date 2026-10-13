//! T5 adds gates to the broker; fixture executor only, never real git.
use faber_engine::project::git::{Broker, Executor, GitError, Invocation, Output, Repository};
use faber_engine::project::lease::{Termination, hash};
use faber_engine::project::store::Journal;
use faber_engine::project::wire::{FileOp, WriteOp};
use faber_engine::project::{Author, GitMode, Grant, Lease, RepoIdentity};
use std::collections::VecDeque;
const BASE: &str = "1111111111111111111111111111111111111111";
const TREE: &str = "2222222222222222222222222222222222222222";
struct Fake {
    outputs: VecDeque<String>,
    calls: Vec<Invocation>,
}
impl Executor for Fake {
    fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
        self.calls.push(call.clone());
        if call.args.contains(&"remove".into()) {
            let path = std::path::Path::new(call.args.last().unwrap());
            std::fs::remove_dir_all(path).unwrap();
        }
        Ok(Output { success: true, stdout: self.outputs.pop_front().expect("unexpected git").into_bytes() })
    }
}
fn setup(outputs: &[&str]) -> (tempfile::TempDir, tempfile::TempDir, Broker<Fake>, Grant, Lease, std::path::PathBuf) {
    setup_with(outputs, std::sync::Arc::new(faber_engine::store::RealDisk))
}
fn setup_with(
    outputs: &[&str],
    disk: std::sync::Arc<dyn faber_engine::store::Disk>,
) -> (tempfile::TempDir, tempfile::TempDir, Broker<Fake>, Grant, Lease, std::path::PathBuf) {
    setup_for(outputs, disk, "w1")
}
fn setup_for(
    outputs: &[&str],
    disk: std::sync::Arc<dyn faber_engine::store::Disk>,
    task: &str,
) -> (tempfile::TempDir, tempfile::TempDir, Broker<Fake>, Grant, Lease, std::path::PathBuf) {
    let dir = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let data = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    let identity = RepoIdentity::new(
        &dir.path().join(".git").canonicalize().unwrap().to_string_lossy(),
        &dir.path().canonicalize().unwrap().to_string_lossy(),
    );
    let repository = Repository {
        identity: identity.clone(),
        base: BASE.into(),
        author: Some(Author { name: "Test".into(), email: "test@example.invalid".into() }),
    };
    let mut grant = Grant::new(&identity.common_dir, GitMode::Seguro, 3, 1);
    grant.author = repository.author.clone();
    let mut broker = Broker::open(
        repository,
        Journal::open_with(dir.path(), data.path(), "m1", disk).unwrap(),
        "m1",
        3,
        Fake { outputs: std::iter::once(String::new()).chain(outputs.iter().map(|s| s.to_string())).collect(), calls: vec![] },
    )
    .unwrap();
    broker.create_branch(|| Ok(Some(grant.clone())), "branch").unwrap();
    let lease =
        broker.leases().acquire(Lease::new(&identity.common_dir, "m1", task, 1, vec!["src".into()], BASE, &"a".repeat(64), 1)).unwrap();
    let root = broker.journal().private().join(format!("writer-{}", lease.generation));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/a.rs"), "before").unwrap();
    broker.leases().bind(&lease, &root).unwrap();
    broker
        .leases()
        .patch(
            &lease,
            Some(&grant),
            3,
            &FileOp::Write(WriteOp { path: "src/a.rs".into(), expect: Some(hash(b"before")), content: "after".into() }),
        )
        .unwrap();
    (dir, data, broker, grant, lease, root)
}
#[test]
fn a_tree_different_from_the_proof_never_reaches_commit_tree() {
    let (_dir, _data, mut b, g, l, _root) = setup(&[BASE, "", "", "", "src/a.rs\0", TREE]);
    assert!(b.commit_proven(|| Ok(Some(g.clone())), &l, "commit", "Fix", &["src/a.rs".into()], &"3".repeat(40)).is_err());
    assert!(!b.executor().calls.iter().any(|c| c.args.contains(&"commit-tree".into())));
}
#[test]
fn compose_uses_a_private_index_and_the_current_own_head() {
    let (_dir, _data, mut b, g, l, _root) = setup(&[BASE, BASE, "", "", TREE]);
    assert_eq!(b.compose_tree(|| Ok(Some(g.clone())), &l, &["src/a.rs".into()]).unwrap(), TREE);
    let calls = &b.executor().calls;
    let indexed: Vec<_> = calls.iter().filter(|c| c.private_index.is_some()).collect();
    assert_eq!(indexed.len(), 3);
    for c in indexed {
        assert!(c.private_index.as_ref().unwrap().starts_with(b.journal().private()));
    }
}
#[test]
fn r106_worktrees_delete_only_after_termination_settlement_and_lease_release() {
    let (_dir, _data, mut b, g, l, root) = setup(&[""]);
    assert!(b.retire_worktree(|| Ok(Some(g.clone())), &l, "cleanup").is_err());
    assert_eq!(b.executor().calls.len(), 1);
    assert!(root.exists());
    b.leases().release(&l, Termination { process: None, tree_dead: true, cost_settled: true }).unwrap();
    b.retire_worktree(|| Ok(Some(g.clone())), &l, "cleanup").unwrap();
    assert!(!root.exists());
    assert!(b.journal().private().join("effects.json").exists());
    b.retire_worktree(|| Ok(Some(g.clone())), &l, "cleanup").unwrap();
    assert_eq!(b.executor().calls.len(), 2);
}

#[test]
fn r106_disk_budget_counts_owned_worktrees_as_well_as_build_resources() {
    let (_repo, _data, b, _grant, _lease, _root) = setup(&[]);
    let mut resources = faber_engine::project::disk::Resources::load(b.journal()).unwrap();
    let before = resources.used_bytes(b.journal()).unwrap();
    assert_eq!(before, 5);
    let output = resources.register(b.journal(), "build", faber_engine::project::disk::ResourceKind::Build, None).unwrap();
    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(output.join("output"), b"compiled").unwrap();
    assert_eq!(resources.used_bytes(b.journal()).unwrap(), before + 8);
}

#[test]
fn build_admission_checks_a_separate_git_common_volume() {
    let (_repo, _data, b, _grant, l, _root) = setup(&[]);
    let mut r = runner(&b);
    let mut sampled = false;
    assert_eq!(
        r.prepare_build_with(
            b.journal(),
            "w1",
            faber_engine::project::disk::DiskBudget { limit_bytes: 10000, reserve_bytes: 10 },
            20,
            |path| {
                if path == std::path::Path::new(&l.repo) {
                    sampled = true;
                    Some(0)
                } else {
                    Some(10000)
                }
            }
        ),
        Err("reason.disk.low")
    );
    assert!(sampled);
    assert_eq!(r.mission.state, faber_engine::project::MissionState::Paused);
}

fn runner(b: &Broker<Fake>) -> faber_engine::project::run::Runner {
    runner_for(b, "w1")
}
fn runner_for(b: &Broker<Fake>, task: &str) -> faber_engine::project::run::Runner {
    use faber_engine::model::Vendor;
    use faber_engine::project::proof::{Definition, Observation, ProofSet};
    use faber_engine::project::queue::{Candidate, Review};
    use faber_engine::project::wire::ArtifactRef;
    use faber_engine::project::{Mission, MissionState, Plan, Task, TaskKind};
    let j = b.journal();
    let mut r = faber_engine::project::run::Runner::create(j, Mission::new("m1", "p1", "repo", "Fix", BASE, 3, 1)).unwrap();
    r.mission.state = MissionState::Running;
    let def = Definition {
        id: "rust".into(),
        version: 1,
        program: std::env::temp_dir().join("cargo"),
        argv: vec!["t".into()],
        checks: vec!["C1".into()],
        toolchain: "rust".into(),
        dependencies: "deps".into(),
        environment: Default::default(),
        heavy: true,
    };
    let paths = vec!["src/a.rs".into()];
    let mut p = ProofSet::new(TREE.into(), "input".into(), vec![def], paths.clone()).unwrap();
    p.record(
        "green",
        "rust",
        Observation {
            exit: Some(0),
            stdout: b"pass".to_vec(),
            stderr: vec![],
            evidence: ArtifactRef { path: "proofs/rust.log".into(), sha256: hash(b"pass") },
        },
    )
    .unwrap();
    let c = Candidate::new(task, BASE.into(), TREE.into(), 3, Vendor::Codex, "input".into(), paths, p.definitions().to_vec()).unwrap();
    r.queue.enqueue(c).unwrap();
    r.queue.prove(task, p.clone()).unwrap();
    r.queue
        .review(
            task,
            Review {
                vendor: Vendor::Claude,
                tree: TREE.into(),
                proof_digest: p.digest(),
                session: "fresh".into(),
                fresh: true,
                accepted: true,
                findings: vec![],
            },
        )
        .unwrap();
    let plan = Plan::new("m1", 1, vec![Task::new(task, "Fix", TaskKind::Writer)]);
    j.save_plan(&plan).unwrap();
    r.save(j).unwrap();
    r
}

fn landed_receipts(b: &Broker<Fake>, r: &mut faber_engine::project::run::Runner) {
    use faber_engine::project::{Intent, IntentKind, Outcome, Receipt};
    let mut raw = serde_json::to_value(&*r).unwrap();
    raw["queue"]["entries"][0]["commitIntent"] = "commit".into();
    raw["queue"]["entries"][0]["forwardIntent"] = "forward".into();
    *r = serde_json::from_value(raw).unwrap();
    for (id, kind, evidence) in [
        ("commit", IntentKind::Commit, vec![("tree", TREE), ("parent", BASE), ("head", TREE)]),
        ("forward", IntentKind::FastForward, vec![("old", BASE), ("new", TREE)]),
    ] {
        b.journal().record_intent(&Intent::new(id, "m1", kind, 3, 1)).unwrap();
        let mut receipt = Receipt::new(id, Outcome::Done, 2);
        receipt.evidence = evidence.into_iter().map(|(k, v)| (k.into(), v.into())).collect();
        b.journal().record_receipt(&receipt).unwrap();
    }
}

#[test]
fn opus_h3_landing_checkpoint_recovers_without_requeued_retirement() {
    use faber_engine::store::{Disk, RealDisk};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct Crash(AtomicUsize, AtomicUsize);
    impl Disk for Crash {
        fn write(&self, p: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
            if p.file_name().unwrap().to_string_lossy().starts_with("run.json")
                && self.0.load(Ordering::SeqCst) != 0
                && self.1.fetch_add(1, Ordering::SeqCst) + 1 == self.0.load(Ordering::SeqCst)
            {
                return Err(std::io::ErrorKind::StorageFull.into());
            }
            RealDisk.write(p, bytes)
        }
        fn rename(&self, a: &std::path::Path, b: &std::path::Path) -> std::io::Result<()> {
            RealDisk.rename(a, b)
        }
    }
    let disk = Arc::new(Crash(AtomicUsize::new(0), AtomicUsize::new(0)));
    let (repo, data, mut b, g, l, root) = setup_with(&[TREE, ""], disk.clone());
    let mut r = runner(&b);
    landed_receipts(&b, &mut r);
    disk.0.store(3, Ordering::SeqCst);
    assert_eq!(
        r.land(
            &mut b,
            "w1",
            &l,
            || Ok(Some(g.clone())),
            "Fix",
            Termination { process: None, tree_dead: true, cost_settled: true },
            |_| Ok(())
        ),
        Err("error.store.full")
    );
    disk.0.store(0, Ordering::SeqCst);
    let removed = b.executor().calls.iter().filter(|c| c.args.contains(&"remove".into())).count();
    let repository = Repository {
        identity: RepoIdentity::new(
            &repo.path().join(".git").canonicalize().unwrap().to_string_lossy(),
            &repo.path().canonicalize().unwrap().to_string_lossy(),
        ),
        base: BASE.into(),
        author: g.author.clone(),
    };
    drop(b);
    let mut b = Broker::open(
        repository,
        Journal::open(repo.path(), data.path(), "m1").unwrap(),
        "m1",
        3,
        Fake { outputs: VecDeque::from([String::new()]), calls: vec![] },
    )
    .unwrap();
    let mut r = faber_engine::project::run::Runner::load(b.journal()).unwrap();
    r.recover_retirements(&mut b, || Ok(Some(g.clone()))).unwrap();
    assert!(!root.exists());
    assert!(r.queue.get("w1").is_none());
    r.recover_retirements(&mut b, || Ok(Some(g.clone()))).unwrap();
    assert_eq!(removed + b.executor().calls.iter().filter(|c| c.args.contains(&"remove".into())).count(), 1);
}

#[test]
fn recovered_commit_checks_current_grant_before_any_fast_forward() {
    use faber_engine::project::{Intent, IntentKind, Outcome, Receipt};
    let (_repo, _data, mut b, mut g, l, _root) = setup(&[]);
    let mut r = runner(&b);
    let mut raw = serde_json::to_value(&r).unwrap();
    raw["queue"]["entries"][0]["commitIntent"] = "commit".into();
    raw["queue"]["entries"][0]["forwardIntent"] = "forward".into();
    r = serde_json::from_value(raw).unwrap();
    let j = b.journal();
    j.record_intent(&Intent::new("commit", "m1", IntentKind::Commit, 3, 1)).unwrap();
    let mut receipt = Receipt::new("commit", Outcome::Done, 2);
    receipt.evidence = [("tree", TREE), ("parent", BASE), ("head", "3333333333333333333333333333333333333333")]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
    j.record_receipt(&receipt).unwrap();
    g.revision = 4;
    assert_eq!(
        r.land(
            &mut b,
            "w1",
            &l,
            || Ok(Some(g.clone())),
            "Fix",
            Termination { process: None, tree_dead: true, cost_settled: true },
            |_| Ok(())
        ),
        Err("reason.queue.stale")
    );
    assert_eq!(b.executor().calls.len(), 1);
}

#[test]
fn recovered_landing_rejects_a_moved_own_head_before_bar_or_cleanup() {
    let (_repo, _data, mut b, g, l, root) = setup(&[BASE]);
    let mut r = runner(&b);
    landed_receipts(&b, &mut r);
    let mut bar_called = false;
    assert_eq!(
        r.land(
            &mut b,
            "w1",
            &l,
            || Ok(Some(g.clone())),
            "Fix",
            Termination { process: None, tree_dead: true, cost_settled: true },
            |_| {
                bar_called = true;
                Ok(())
            }
        ),
        Err("reason.queue.stale")
    );
    assert!(!bar_called);
    assert!(root.exists());
    assert!(r.queue.front().is_some());
    assert_eq!(b.executor().calls.len(), 2);
}

#[test]
fn forwarded_receipt_without_its_commit_parks_recovery_before_any_new_effect() {
    let (_repo, _data, mut b, g, l, root) = setup(&[BASE]);
    let mut r = runner(&b);
    landed_receipts(&b, &mut r);
    let path = b.journal().private().join("effects.json");
    let mut effects: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    effects["entries"].as_array_mut().unwrap().retain(|e| e["intent"]["id"] != "commit");
    std::fs::write(path, serde_json::to_vec(&effects).unwrap()).unwrap();
    assert_eq!(
        r.land(
            &mut b,
            "w1",
            &l,
            || Ok(Some(g.clone())),
            "Fix",
            Termination { process: None, tree_dead: true, cost_settled: true },
            |_| Err("bar.called")
        ),
        Err("reason.project.reconcile")
    );
    assert_eq!(b.executor().calls.len(), 1);
    assert!(root.exists());
}

#[test]
fn quarantined_retirement_keeps_t4_termination_settlement_and_pending_guards() {
    let (_repo, _data, mut b, g, l, root) = setup(&[]);
    b.leases().quarantine(&l).unwrap();
    assert!(b.retire_worktree(|| Ok(Some(g.clone())), &l, "cleanup").is_err());
    assert!(b.leases().archive(&l, Termination { process: None, tree_dead: true, cost_settled: false }).is_err());
    assert!(
        b.leases()
            .archive(
                &l,
                Termination {
                    process: Some(faber_engine::project::ProcessId { pid: 10, started_at: 42 }),
                    tree_dead: true,
                    cost_settled: true
                }
            )
            .is_err()
    );
    b.leases().archive(&l, Termination { process: None, tree_dead: true, cost_settled: true }).unwrap();
    b.journal()
        .record_intent(&faber_engine::project::Intent::new("unknown", "m1", faber_engine::project::IntentKind::Worktree, 3, 1))
        .unwrap();
    let mut r = runner(&b);
    assert!(r.cleanup_task(b.journal(), "w1", Termination { process: None, tree_dead: true, cost_settled: true }).is_err());
    assert!(b.retire_worktree(|| Ok(Some(g.clone())), &l, "cleanup").is_err());
    assert_eq!(b.executor().calls.len(), 1);
    assert!(root.exists());
    assert_eq!(b.journal().runtime().unwrap().leases[0].state, faber_engine::project::LeaseState::Quarantined);
}

#[test]
fn landing_checkpoint_retains_cleanup_after_the_candidate_is_popped() {
    // Fail lease release after the durable landing checkpoint, the reported crash window.
    use faber_engine::store::{Disk, RealDisk};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    struct Crash(AtomicBool);
    impl Disk for Crash {
        fn write(&self, p: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
            if self.0.load(Ordering::SeqCst) && p.file_name().unwrap().to_string_lossy().starts_with("runtime") {
                return Err(std::io::ErrorKind::StorageFull.into());
            }
            RealDisk.write(p, bytes)
        }
        fn rename(&self, a: &std::path::Path, b: &std::path::Path) -> std::io::Result<()> {
            RealDisk.rename(a, b)
        }
    }
    let fault = Arc::new(Crash(AtomicBool::new(false)));
    let (repo, data, mut b, g, l, root) = setup_with(&[TREE], fault.clone());
    let mut r = runner(&b);
    landed_receipts(&b, &mut r);
    fault.0.store(true, Ordering::SeqCst);
    assert!(
        r.land(
            &mut b,
            "w1",
            &l,
            || Ok(Some(g.clone())),
            "Fix",
            Termination { process: None, tree_dead: true, cost_settled: true },
            |_| Ok(())
        )
        .is_err()
    );
    let checkpoint: serde_json::Value = serde_json::from_slice(&std::fs::read(b.journal().private().join("run.json")).unwrap()).unwrap();
    assert_eq!(checkpoint["retirements"].as_object().map(|m| m.len()), Some(1));
    assert!(root.exists());
    assert!(b.journal().pending().unwrap().is_empty());
    assert!(r.queue.front().is_none());
    fault.0.store(false, Ordering::SeqCst);
    drop(b);
    let identity = RepoIdentity::new(
        &repo.path().join(".git").canonicalize().unwrap().to_string_lossy(),
        &repo.path().canonicalize().unwrap().to_string_lossy(),
    );
    let mut reopened = Broker::open(
        Repository { identity, base: BASE.into(), author: g.author.clone() },
        Journal::open_with(repo.path(), data.path(), "m1", fault).unwrap(),
        "m1",
        3,
        Fake { outputs: VecDeque::from([String::new()]), calls: vec![] },
    )
    .unwrap();
    let mut recovered = faber_engine::project::run::Runner::load(reopened.journal()).unwrap();
    recovered.recover_retirements(&mut reopened, || Ok(Some(g.clone()))).unwrap();
    assert!(!root.exists());
    assert_eq!(reopened.journal().runtime().unwrap().leases[0].state, faber_engine::project::LeaseState::Released);
    assert_eq!(reopened.executor().calls.len(), 1);
    recovered.recover_retirements(&mut reopened, || Ok(Some(g.clone()))).unwrap();
    assert_eq!(reopened.executor().calls.len(), 1);
}

#[test]
fn failed_stalled_abandoned_and_rolled_back_tasks_retire_without_becoming_done() {
    for reason in ["reason.loop", "reason.stall", "reason.abandoned", "reason.rollback"] {
        let (_repo, _data, mut b, g, l, root) = setup(&[""]);
        let mut r = runner(&b);
        let backups: Vec<_> = std::fs::read_dir(b.journal().private())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("patch-"))
            .collect();
        assert!(!backups.is_empty());
        let mut plan = b.journal().plan().unwrap().unwrap();
        plan.tasks[0].state = faber_engine::project::TaskState::Blocked;
        plan.tasks[0].reason = Some(reason.into());
        b.journal().save_plan(&plan).unwrap();
        let mut resources = faber_engine::project::disk::Resources::load(b.journal()).unwrap();
        let output = resources.register_for(b.journal(), "build", "w1", faber_engine::project::disk::ResourceKind::Build, None).unwrap();
        std::fs::create_dir_all(&output).unwrap();
        if reason == "reason.rollback" {
            b.leases().quarantine(&l).unwrap();
        }
        assert!(
            r.retire_task(&mut b, &l, Termination { process: None, tree_dead: false, cost_settled: true }, || Ok(Some(g.clone()))).is_err()
        );
        assert!(root.exists());
        r.retire_task(&mut b, &l, Termination { process: None, tree_dead: true, cost_settled: true }, || Ok(Some(g.clone()))).unwrap();
        assert!(!root.exists());
        assert!(!output.exists());
        assert!(backups.iter().all(|p| !p.exists()), "last-consumer retirement also removes obsolete patch backups");
        assert!(r.queue.get("w1").is_none(), "retired candidates cannot block independent ready work");
        assert!(b.journal().records().join(format!("artifacts/abandoned-{}.json", l.generation)).is_file());
        let artifact: serde_json::Value = serde_json::from_slice(
            &std::fs::read(b.journal().records().join(format!("artifacts/abandoned-{}.json", l.generation))).unwrap(),
        )
        .unwrap();
        assert!(artifact.get("review").is_none(), "review session identities remain private");
        let metadata = b.journal().artifact(&format!("abandoned-{}", l.generation)).unwrap().unwrap();
        assert_eq!(metadata.tree.as_deref(), Some(TREE));
        assert_eq!(metadata.changed, vec!["src/a.rs"]);
        assert_eq!(b.journal().plan().unwrap().unwrap(), plan);
        assert_eq!(r.mission.state, faber_engine::project::MissionState::Running);
    }
}

#[test]
fn longest_valid_task_id_can_retire_and_keep_a_t1_artifact() {
    let task = "w".repeat(64);
    let (_repo, _data, mut b, g, l, root) = setup_for(&[""], std::sync::Arc::new(faber_engine::store::RealDisk), &task);
    let mut r = runner_for(&b, &task);
    r.retire_task(&mut b, &l, Termination { process: None, tree_dead: true, cost_settled: true }, || Ok(Some(g.clone()))).unwrap();
    assert!(!root.exists());
    assert!(r.queue.front().is_none());
}

#[test]
fn resume_distinguishes_archived_dead_processes_from_live_quarantine_without_restoring_source_authority() {
    let (_repo, _data, mut b, _g, l, _root) = setup(&[]);
    let mut r = runner(&b);
    let process = faber_engine::project::ProcessId { pid: 10, started_at: 42 };
    b.leases().prepare_launch(&l, "launch", 3, 1).unwrap();
    b.leases().attach_process(&l, process).unwrap();
    b.leases().quarantine(&l).unwrap();
    r.pause(b.journal()).unwrap();
    assert_eq!(r.reconcile(b.journal(), 3), Err("reason.lease.live"));
    b.leases().archive(&l, Termination { process: Some(process), tree_dead: true, cost_settled: true }).unwrap();
    r.reconcile(b.journal(), 3).unwrap();
    assert_eq!(b.journal().runtime().unwrap().leases[0].state, faber_engine::project::LeaseState::Quarantined);
    assert!(b.leases().check(&l, "src/a.rs").is_err());
    assert_eq!(b.executor().calls.len(), 1);
}

#[test]
fn landing_sets_promotion_pending_when_grant_has_self_update() {
    let (_repo, _data, mut b, mut g, l, _root) = setup(&[TREE, ""]);
    g.self_update = true;
    let mut r = runner(&b);
    landed_receipts(&b, &mut r);
    let head = r
        .land(
            &mut b,
            "w1",
            &l,
            || Ok(Some(g.clone())),
            "Fix",
            Termination { process: None, tree_dead: true, cost_settled: true },
            |_| Ok(()),
        )
        .unwrap();
    assert_eq!(head, TREE);
    assert!(r.promotion_pending, "landing with self_update must set promotion_pending");
}

#[test]
fn landing_writes_mission_report_with_tokens_checks_receipts_and_timings() {
    use faber_engine::project::report::UnknownOr;
    let (_repo, _data, mut b, g, l, _root) = setup(&[TREE, ""]);
    let mut r = runner(&b);
    landed_receipts(&b, &mut r);
    r.record_tokens("writer", Some(500), Some(200), Some(50), Some(100));
    let head = r
        .land(
            &mut b,
            "w1",
            &l,
            || Ok(Some(g.clone())),
            "Fix",
            Termination { process: None, tree_dead: true, cost_settled: true },
            |_| Ok(()),
        )
        .unwrap();
    assert_eq!(head, TREE);

    let report_file = b.journal().records().join("report.json");
    assert!(report_file.exists(), "report.json must be written upon landing");

    let report = b.journal().report().unwrap().expect("report must be loadable");
    assert_eq!(report.mission, "m1");
    assert_eq!(report.goal, "Fix");
    assert_eq!(report.state, "done");

    // Tokens per role
    let writer_tokens = report.tokens_per_role.get("writer").expect("writer tokens must exist");
    assert_eq!(writer_tokens.input, UnknownOr::Known(500));
    assert_eq!(writer_tokens.cache_read, UnknownOr::Known(200));
    assert_eq!(writer_tokens.cache_write, UnknownOr::Known(50));
    assert_eq!(writer_tokens.output, UnknownOr::Known(100));

    // Unknown tokens marked as unknown, never guessed (spec G8)
    let lead_tokens = report.tokens_per_role.get("lead").expect("lead tokens must exist");
    assert_eq!(lead_tokens.input, UnknownOr::Unknown("unknown".into()));
    assert_eq!(lead_tokens.output, UnknownOr::Unknown("unknown".into()));

    // Checks
    assert!(!report.checks.is_empty(), "checks must be reported");
    assert_eq!(report.checks[0].id, "C1");
    assert_eq!(report.checks[0].result, "pass");

    // Commit and tree receipts
    let commit = report.commit_receipt.expect("commit receipt must be present");
    assert_eq!(commit.head, UnknownOr::Known(TREE.into()));
    assert_eq!(commit.tree, UnknownOr::Known(TREE.into()));
    assert_eq!(commit.parent, UnknownOr::Known(BASE.into()));

    let forward = report.tree_receipt.expect("tree receipt must be present");
    assert_eq!(forward.old_head, UnknownOr::Known(BASE.into()));
    assert_eq!(forward.new_head, UnknownOr::Known(TREE.into()));
    assert_eq!(forward.tree, UnknownOr::Known(TREE.into()));

    // Timings
    assert!(matches!(report.timings.landed_at, UnknownOr::Known(_)));
    assert!(matches!(report.timings.duration_seconds, UnknownOr::Known(_)));
}

//! Exclusive leases and file requests on temporary directories; never spawn git or a worker.
use faber_engine::project::lease::{Closure, LeaseError, Leases, Termination, hash, real_path};
use faber_engine::project::store::Journal;
use faber_engine::project::wire::{DeleteOp, FileOp, RenameOp, WriteOp};
use faber_engine::project::{DecidedBy, Decision, DecisionKind, GraphError, Intent, IntentKind, Outcome, Plan, Receipt, Task, TaskKind};
use faber_engine::project::{GitMode, Grant, Lease, ProcessId, RepoIdentity};
use faber_engine::store::{Disk, Fault, RealDisk};
use std::fs;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn tempdir() -> tempfile::TempDir {
    // The private sidecar and enrolled repository both start at canonical roots.
    tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
}

struct Setup {
    owner: tempfile::TempDir,
    data: tempfile::TempDir,
    repo: RepoIdentity,
}
impl Setup {
    fn new() -> Self {
        let owner = tempdir();
        let data = tempdir();
        fs::create_dir(owner.path().join(".git")).unwrap();
        let repo = RepoIdentity::new(
            &owner.path().join(".git").canonicalize().unwrap().to_string_lossy(),
            &owner.path().canonicalize().unwrap().to_string_lossy(),
        );
        Self { owner, data, repo }
    }
    fn open(&self) -> Result<Leases, LeaseError> {
        Leases::open(self.repo.clone(), Journal::open(self.owner.path(), self.data.path(), "m1").unwrap(), "m1")
    }
    fn request(&self, paths: &[&str]) -> Lease {
        Lease::new(&self.repo.common_dir, "m1", "task", 1, paths.iter().map(|p| (*p).into()).collect(), &"1".repeat(40), &"a".repeat(64), 0)
    }
    fn grant(&self) -> Grant {
        Grant::new(&self.repo.common_dir, GitMode::Seguro, 3, 1)
    }
    fn candidate(&self) -> std::path::PathBuf {
        let root = self.data.path().join("missions/m1/candidate");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/a.rs"), "before").unwrap();
        root
    }
}

#[test]
fn a_repository_has_one_coordinator_and_paths_have_one_holder() {
    let s = Setup::new();
    let mut leases = s.open().unwrap();
    assert!(matches!(s.open(), Err(LeaseError::Owned)));
    let first = leases.acquire(s.request(&["src"])).unwrap();
    for paths in [&["SRC/a.rs"][..], &["src"], &["."]] {
        let mut request = s.request(paths);
        request.task = "other".into();
        assert_eq!(leases.acquire(request), Err(LeaseError::Conflict));
    }
    let mut request = s.request(&["ui"]);
    request.task = "other".into();
    let second = leases.acquire(request).unwrap();
    assert!(second.generation > first.generation);
    drop(leases);
    let mut leases = s.open().unwrap();
    assert_eq!(leases.acquire(s.request(&["src"])), Err(LeaseError::Conflict));
}

#[test]
fn release_requires_exact_dead_tree_and_settled_cost_never_a_timeout() {
    let s = Setup::new();
    let mut leases = s.open().unwrap();
    let lease = leases.acquire(s.request(&["src"])).unwrap();
    let process = ProcessId { pid: 42, started_at: 9 };
    leases.prepare_launch(&lease, "launch", 3, 1).unwrap();
    leases.attach_process(&lease, process).unwrap();
    for proof in [
        Termination { process: Some(process), tree_dead: false, cost_settled: true },
        Termination { process: Some(ProcessId { pid: 42, started_at: 10 }), tree_dead: true, cost_settled: true },
        Termination { process: Some(process), tree_dead: true, cost_settled: false },
    ] {
        assert_eq!(leases.release(&lease, proof), Err(LeaseError::Live));
    }
    leases.release(&lease, Termination { process: Some(process), tree_dead: true, cost_settled: true }).unwrap();
    let new = leases.acquire(s.request(&["src"])).unwrap();
    assert!(new.generation > lease.generation);
    assert_eq!(leases.check(&lease, "src/a.rs"), Err(LeaseError::Stale));
}

#[test]
fn patches_check_hash_scope_both_rename_paths_and_revocation() {
    let s = Setup::new();
    let mut leases = s.open().unwrap();
    let lease = leases.acquire(s.request(&["src"])).unwrap();
    let root = s.candidate();
    leases.bind(&lease, &root).unwrap();
    let mut grant = s.grant();
    let write = FileOp::Write(WriteOp { path: "src/a.rs".into(), expect: Some(hash(b"before")), content: "after".into() });
    grant.revoked_at = Some(2);
    assert_eq!(leases.patch(&lease, Some(&grant), 3, &write), Err(LeaseError::Authority("reason.grant.revoked")));
    grant.revoked_at = None;
    let rename = FileOp::Rename(RenameOp { from: "src/a.rs".into(), to: "ui/a.rs".into(), expect: hash(b"before") });
    assert_eq!(leases.patch(&lease, Some(&grant), 3, &rename), Err(LeaseError::Scope));
    leases.patch(&lease, Some(&grant), 3, &write).unwrap();
    assert_eq!(leases.patch(&lease, Some(&grant), 3, &write), Err(LeaseError::Hash));
    let rename = FileOp::Rename(RenameOp { from: "src/a.rs".into(), to: "src/b.rs".into(), expect: hash(b"after") });
    leases.patch(&lease, Some(&grant), 3, &rename).unwrap();
    assert_eq!(leases.audit(&lease).unwrap(), vec!["src/a.rs", "src/b.rs"]);
    leases.patch(&lease, Some(&grant), 3, &FileOp::Delete(DeleteOp { path: "src/b.rs".into(), expect: hash(b"after") })).unwrap();
    assert!(!s.owner.path().join("src").exists(), "source edits stay in the private candidate");
}

#[test]
fn manual_edits_quarantine_and_metadata_aliases_are_denied() {
    let s = Setup::new();
    let mut leases = s.open().unwrap();
    for path in [
        ".git/config",
        ".faber/missions/m1",
        "src/../outside",
        "file:stream",
        "NUL",
        "CONIN$",
        "CONOUT$",
        "COM¹",
        "LPT².txt",
        "src/a. ",
        "src/~alias",
    ] {
        assert_eq!(leases.acquire(s.request(&[path])), Err(LeaseError::Path), "{path}");
    }
    let lease = leases.acquire(s.request(&["src"])).unwrap();
    let root = s.candidate();
    leases.bind(&lease, &root).unwrap();
    fs::write(root.join("src/a.rs"), "manual").unwrap();
    assert_eq!(leases.audit(&lease), Err(LeaseError::Quarantined));
    assert_eq!(leases.check(&lease, "src/a.rs"), Err(LeaseError::Stale));
    assert_eq!(leases.release(&lease, Termination { process: None, tree_dead: true, cost_settled: true }), Err(LeaseError::Stale));
    drop(leases);
    assert!(s.open().unwrap().check(&lease, "src/a.rs").is_err());
}

#[test]
fn losing_a_saved_runtime_never_recreates_empty_authority() {
    let s = Setup::new();
    let mut leases = s.open().unwrap();
    leases.acquire(s.request(&["src"])).unwrap();
    drop(leases);
    fs::remove_file(s.data.path().join("missions/m1/runtime.json")).unwrap();
    assert!(matches!(s.open(), Err(LeaseError::Store(Fault::Corrupt))));
}

struct FailingDisk(AtomicBool);
impl Disk for FailingDisk {
    fn write(&self, path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
        RealDisk.write(path, bytes)
    }
    fn rename(&self, from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
        if self.0.load(Ordering::SeqCst) { Err(std::io::Error::from(std::io::ErrorKind::StorageFull)) } else { RealDisk.rename(from, to) }
    }
}

#[test]
fn a_failed_lease_save_blocks_the_coordinator_and_preserves_the_old_record() {
    let s = Setup::new();
    let disk = Arc::new(FailingDisk(AtomicBool::new(false)));
    let journal = Journal::open_with(s.owner.path(), s.data.path(), "m1", disk.clone()).unwrap();
    let mut leases = Leases::open(s.repo.clone(), journal, "m1").unwrap();
    let first = leases.acquire(s.request(&["src"])).unwrap();
    let before = fs::read(s.data.path().join("missions/m1/runtime.json")).unwrap();
    disk.0.store(true, Ordering::SeqCst);
    let mut second = s.request(&["ui"]);
    second.task = "second".into();
    assert_eq!(leases.acquire(second), Err(LeaseError::Journal(faber_engine::project::store::JournalError::Store(Fault::DiskFull))));
    assert_eq!(leases.check(&first, "src/a.rs"), Err(LeaseError::Quarantined));
    assert_eq!(fs::read(s.data.path().join("missions/m1/runtime.json")).unwrap(), before);
}

#[test]
fn metadata_edits_and_permission_edits_quarantine_the_candidate() {
    let s = Setup::new();
    let mut leases = s.open().unwrap();
    let lease = leases.acquire(s.request(&["src"])).unwrap();
    let root = s.candidate();
    fs::write(root.join(".git"), "gitdir: private").unwrap();
    leases.bind(&lease, &root).unwrap();
    fs::write(root.join(".git"), "gitdir: owner").unwrap();
    assert_eq!(leases.audit(&lease), Err(LeaseError::Quarantined));
    let s = Setup::new();
    let mut leases = s.open().unwrap();
    let lease = leases.acquire(s.request(&["src"])).unwrap();
    let root = s.candidate();
    leases.bind(&lease, &root).unwrap();
    let mut permissions = fs::metadata(root.join("src/a.rs")).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(root.join("src/a.rs"), permissions).unwrap();
    assert_eq!(leases.audit(&lease), Err(LeaseError::Quarantined));
}

#[test]
fn dropping_a_coordinator_cannot_transfer_a_live_mission_to_another_private_folder() {
    let s = Setup::new();
    let leases = s.open().unwrap();
    drop(leases);
    let elsewhere = tempdir();
    let journal = Journal::open(elsewhere.path(), elsewhere.path(), "m1").unwrap();
    assert!(matches!(Leases::open(s.repo.clone(), journal, "m1"), Err(LeaseError::Owned)));
    let journal = Journal::open(s.data.path(), s.data.path(), "m2").unwrap();
    assert!(matches!(Leases::open(s.repo.clone(), journal, "m2"), Err(LeaseError::Owned)));
}

struct EditingDisk {
    armed: AtomicBool,
    path: std::path::PathBuf,
}
impl Disk for EditingDisk {
    fn write(&self, path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
        RealDisk.write(path, bytes)
    }
    fn rename(&self, from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
        RealDisk.rename(from, to)?;
        if self.armed.load(Ordering::SeqCst) {
            fs::write(&self.path, "manual")?;
        }
        Ok(())
    }
}

#[test]
fn a_concurrent_edit_during_a_patch_is_not_absorbed_into_the_broker_manifest() {
    let s = Setup::new();
    let root = s.data.path().join("missions/m1/candidate");
    let disk = Arc::new(EditingDisk { armed: AtomicBool::new(false), path: root.join("src/other.rs") });
    let journal = Journal::open_with(s.owner.path(), s.data.path(), "m1", disk.clone()).unwrap();
    let mut leases = Leases::open(s.repo.clone(), journal, "m1").unwrap();
    let lease = leases.acquire(s.request(&["src"])).unwrap();
    let root = s.candidate();
    leases.bind(&lease, &root).unwrap();
    disk.armed.store(true, Ordering::SeqCst);
    let write = FileOp::Write(WriteOp { path: "src/a.rs".into(), expect: Some(hash(b"before")), content: "after".into() });
    assert_eq!(leases.patch(&lease, Some(&s.grant()), 3, &write), Err(LeaseError::Quarantined));
    assert_eq!(leases.check(&lease, "src/a.rs"), Err(LeaseError::Stale));
}

#[test]
fn review_1_a_crashed_mission_can_be_recovered_without_transferring_live_ownership() {
    let s = Setup::new();
    let mut leases = s.open().unwrap();
    let lease = leases.acquire(s.request(&["src"])).unwrap();
    let process = ProcessId { pid: 42, started_at: 9 };
    leases.prepare_launch(&lease, "launch", 3, 1).unwrap();
    leases.attach_process(&lease, process).unwrap();
    drop(leases);
    let other = || Leases::open(s.repo.clone(), Journal::open(s.data.path(), s.data.path(), "m2").unwrap(), "m2");
    assert!(matches!(other(), Err(LeaseError::Owned)));
    let mut recovered = s.open().unwrap();
    assert_eq!(
        recovered.release(&lease, Termination { process: Some(process), tree_dead: false, cost_settled: true }),
        Err(LeaseError::Live)
    );
    assert_eq!(
        recovered.release(&lease, Termination { process: Some(process), tree_dead: true, cost_settled: false }),
        Err(LeaseError::Live)
    );
    assert!(matches!(other(), Err(LeaseError::Owned)));
    recovered.release(&lease, Termination { process: Some(process), tree_dead: true, cost_settled: true }).unwrap();
    recovered.finish().unwrap();
    other().unwrap().finish().unwrap();
}

#[test]
fn review_3_repository_local_records_are_allowed_and_excluded_without_changing_source() {
    let s = Setup::new();
    fs::create_dir(s.owner.path().join(".git/info")).unwrap();
    fs::write(s.owner.path().join(".git/info/exclude"), b"# owner's patterns\nlocal-cache\n").unwrap();
    fs::write(s.owner.path().join(".gitignore"), "user rules\n").unwrap();
    let journal = Journal::open(s.owner.path(), s.data.path(), "m1").unwrap();
    let leases = Leases::open(s.repo.clone(), journal, "m1").unwrap();
    let exclude = fs::read(s.owner.path().join(".git/info/exclude")).unwrap();
    assert!(exclude.starts_with(b"# owner's patterns\nlocal-cache\n"));
    assert!(exclude.windows(b"/.faber/".len()).any(|line| line == b"/.faber/"));
    assert_eq!(fs::read_to_string(s.owner.path().join(".gitignore")).unwrap(), "user rules\n");
    drop(leases);
    Leases::open(s.repo.clone(), Journal::open(s.owner.path(), s.data.path(), "m1").unwrap(), "m1").unwrap().finish().unwrap();
    assert_eq!(fs::read(s.owner.path().join(".git/info/exclude")).unwrap(), exclude, "exclusion is idempotent");
}

#[test]
fn review_4_a_dead_settled_quarantined_attempt_can_be_archived_without_losing_its_files() {
    let s = Setup::new();
    let mut leases = s.open().unwrap();
    let lease = leases.acquire(s.request(&["src"])).unwrap();
    let root = s.candidate();
    leases.bind(&lease, &root).unwrap();
    let process = ProcessId { pid: 42, started_at: 9 };
    leases.prepare_launch(&lease, "launch", 3, 1).unwrap();
    leases.attach_process(&lease, process).unwrap();
    fs::write(root.join("src/a.rs"), "manual artifact").unwrap();
    assert_eq!(leases.audit(&lease), Err(LeaseError::Quarantined));
    for proof in [
        Termination { process: Some(process), tree_dead: false, cost_settled: true },
        Termination { process: Some(process), tree_dead: true, cost_settled: false },
        Termination { process: Some(ProcessId { pid: 42, started_at: 10 }), tree_dead: true, cost_settled: true },
    ] {
        assert_eq!(leases.archive(&lease, proof), Err(LeaseError::Live));
    }
    leases.archive(&lease, Termination { process: Some(process), tree_dead: true, cost_settled: true }).unwrap();
    assert_eq!(leases.check(&lease, "src/a.rs"), Err(LeaseError::Stale));
    assert_eq!(leases.acquire(s.request(&["src"])), Err(LeaseError::Conflict));
    drop(leases);
    let leases = s.open().unwrap();
    assert_eq!(leases.journal().runtime().unwrap().leases[0].state, faber_engine::project::LeaseState::Quarantined);
    leases.finish().unwrap();
    assert_eq!(fs::read_to_string(root.join("src/a.rs")).unwrap(), "manual artifact");
}

#[test]
fn review_4_unknown_effect_receipts_prevent_releasing_repository_ownership() {
    let s = Setup::new();
    let leases = s.open().unwrap();
    leases.journal().record_intent(&Intent::new("uncertain", "m1", IntentKind::Commit, 3, 1)).unwrap();
    leases.journal().record_receipt(&Receipt::new("uncertain", Outcome::Unknown, 2)).unwrap();
    assert_eq!(leases.finish(), Err(LeaseError::Live));
    let other = Leases::open(s.repo.clone(), Journal::open(s.data.path(), s.data.path(), "m2").unwrap(), "m2");
    assert!(matches!(other, Err(LeaseError::Owned)));
}

#[test]
fn review_8_plan_write_paths_use_the_same_lexical_rules_as_leases() {
    for path in ["CON", ".git", ".faber/missions/m1", "foo~1", "trailing.", "trailing "] {
        let mut task = Task::new("writer", "test", TaskKind::Writer);
        task.writes = vec![path.into()];
        assert_eq!(Plan::new("m1", 1, vec![task]).validate(), Err(GraphError::BadPath { task: "writer".into(), path: path.into() }));
    }
}

#[test]
fn review_4_failed_retirement_persistence_keeps_the_attempt_and_repository_quarantined() {
    let s = Setup::new();
    let disk = Arc::new(FailingDisk(AtomicBool::new(false)));
    let journal = Journal::open_with(s.owner.path(), s.data.path(), "m1", disk.clone()).unwrap();
    let mut leases = Leases::open(s.repo.clone(), journal, "m1").unwrap();
    let lease = leases.acquire(s.request(&["src"])).unwrap();
    let root = s.candidate();
    leases.bind(&lease, &root).unwrap();
    fs::write(root.join("src/a.rs"), "manual artifact").unwrap();
    assert_eq!(leases.audit(&lease), Err(LeaseError::Quarantined));
    let before = fs::read(s.data.path().join("missions/m1/runtime.json")).unwrap();
    disk.0.store(true, Ordering::SeqCst);
    assert!(matches!(
        leases.archive(&lease, Termination { process: None, tree_dead: true, cost_settled: true }),
        Err(LeaseError::Journal(_))
    ));
    assert_eq!(fs::read(s.data.path().join("missions/m1/runtime.json")).unwrap(), before);
    assert_eq!(leases.finish(), Err(LeaseError::Live));
    assert_eq!(s.open().unwrap().finish(), Err(LeaseError::Live));
    assert!(matches!(
        Leases::open(s.repo.clone(), Journal::open(s.owner.path(), s.data.path(), "m2").unwrap(), "m2"),
        Err(LeaseError::Owned)
    ));
    assert_eq!(fs::read_to_string(root.join("src/a.rs")).unwrap(), "manual artifact");
}

#[test]
fn opus_h2_failed_initialization_does_not_publish_an_owner() {
    let s = Setup::new();
    let disk = Arc::new(FailingDisk(AtomicBool::new(false)));
    let journal = Journal::open_with(s.owner.path(), s.data.path(), "m1", disk.clone()).unwrap();
    disk.0.store(true, Ordering::SeqCst);
    assert!(Leases::open(s.repo.clone(), journal, "m1").is_err());
    Leases::open(s.repo.clone(), Journal::open(s.owner.path(), s.data.path(), "m2").unwrap(), "m2").unwrap().finish().unwrap();
}

#[test]
fn opus_m1_missing_or_damaged_private_ledger_is_never_empty_and_public_receipts_are_untrusted() {
    let s = Setup::new();
    let leases = s.open().unwrap();
    let journal = leases.journal();
    journal.record_intent(&Intent::new("pending", "m1", IntentKind::Commit, 3, 1)).unwrap();
    let public = journal.records().join("receipts/pending.json");
    let mut forged: serde_json::Value = serde_json::from_slice(&fs::read(&public).unwrap()).unwrap();
    forged["receipt"] = serde_json::to_value(Receipt::new("pending", Outcome::Done, 2)).unwrap();
    fs::write(&public, forged.to_string()).unwrap();
    assert_eq!(journal.pending().unwrap().len(), 1);
    fs::remove_file(journal.private().join("effects.json")).unwrap();
    assert!(journal.pending().is_err());
    assert!(journal.record_intent(&Intent::new("pending", "m1", IntentKind::Commit, 3, 1)).is_err());
    assert!(leases.finish().is_err());
}

#[test]
fn opus_h3_unknown_receipts_have_an_explicit_resolution() {
    let s = Setup::new();
    let leases = s.open().unwrap();
    let journal = leases.journal();
    journal.record_intent(&Intent::new("uncertain", "m1", IntentKind::Commit, 3, 1)).unwrap();
    journal.record_receipt(&Receipt::new("uncertain", Outcome::Unknown, 2)).unwrap();
    let mut receipt = Receipt::new("uncertain", Outcome::Failed, 3);
    receipt.evidence.insert("inspection".into(), "unapproved private commit retained, never landed".into());
    let mut decision = Decision::new("resolve", "lead", "Close the inspected effect?", DecisionKind::Technical);
    decision.answer(DecidedBy::Ai, "close", 3).unwrap();
    assert!(journal.reconcile(&receipt, &decision).is_err());
    decision.answered_by = Some(DecidedBy::Person);
    journal.reconcile(&receipt, &decision).unwrap();
    assert!(journal.reconcile(&receipt, &decision).is_err(), "a final outcome cannot be changed again");
    let ledger = fs::read_to_string(journal.private().join("effects.json")).unwrap();
    assert!(ledger.contains("unknown") && ledger.contains("reconciliation"));
    assert!(journal.pending().unwrap().is_empty());
    leases.finish().unwrap();
}

#[test]
fn opus_m1_removing_public_records_cannot_erase_or_forge_effects() {
    let s = Setup::new();
    let leases = s.open().unwrap();
    let intent = Intent::new("uncertain", "m1", IntentKind::Commit, 3, 1);
    leases.journal().record_intent(&intent).unwrap();
    fs::remove_dir_all(s.owner.path().join(".faber")).unwrap();
    assert_eq!(leases.journal().pending().unwrap(), vec![intent.clone()]);
    assert!(leases.journal().record_intent(&intent).is_err());
    assert_eq!(leases.finish(), Err(LeaseError::Live));
}

#[test]
fn opus_m2_a_held_repository_lock_cannot_be_deleted() {
    let s = Setup::new();
    let leases = s.open().unwrap();
    #[cfg(windows)]
    assert!(fs::remove_file(s.owner.path().join(".git/faber-project.lock")).is_err());
    assert!(matches!(s.open(), Err(LeaseError::Owned)));
    leases.finish().unwrap();
}

fn closure(dead: bool) -> Closure {
    let mut decision = Decision::new("close", "lead", "Close this abandoned mission and reserve unknown cost?", DecisionKind::Money);
    decision.answer(DecidedBy::Person, "close and reserve", 4).unwrap();
    Closure { decision, processes: vec![], tree_dead: dead, cost_settled: false, reserve_unknown_cost: true }
}

#[test]
fn opus_h2_explicit_closure_preserves_unknown_effects_cost_and_artifacts() {
    let s = Setup::new();
    let mut leases = s.open().unwrap();
    let lease = leases.acquire(s.request(&["src"])).unwrap();
    let root = s.candidate();
    leases.bind(&lease, &root).unwrap();
    leases.journal().record_intent(&Intent::new("pending", "m1", IntentKind::Commit, 3, 1)).unwrap();
    let journal = Journal::open(s.owner.path(), s.data.path(), "m1").unwrap();
    assert!(matches!(Leases::close_abandoned(&s.repo, &journal, "m1", closure(true)), Err(LeaseError::Owned)));
    drop(leases);
    assert_eq!(Leases::close_abandoned(&s.repo, &journal, "m1", closure(false)), Err(LeaseError::Live));
    Leases::close_abandoned(&s.repo, &journal, "m1", closure(true)).unwrap();
    assert!(journal.private().join("closure.json").is_file());
    assert!(fs::read_to_string(journal.private().join("closure.json")).unwrap().contains("\"unknownCostReserved\": true"));
    assert_eq!(journal.pending().unwrap().len(), 1);
    assert!(journal.record_intent(&Intent::new("again", "m1", IntentKind::Commit, 3, 2)).is_err());
    assert_eq!(fs::read(root.join("src/a.rs")).unwrap(), b"before");
    assert!(matches!(s.open(), Err(LeaseError::Stale)));
    Leases::open(s.repo.clone(), Journal::open(s.owner.path(), s.data.path(), "m2").unwrap(), "m2").unwrap().finish().unwrap();
}

#[test]
fn opus_h2_missing_runtime_requires_explicit_closure_never_empty_recovery() {
    let s = Setup::new();
    let mut leases = s.open().unwrap();
    leases.acquire(s.request(&["src"])).unwrap();
    drop(leases);
    fs::remove_file(s.data.path().join("missions/m1/runtime.json")).unwrap();
    assert!(s.open().is_err());
    let journal = Journal::open(s.owner.path(), s.data.path(), "m1").unwrap();
    Leases::close_abandoned(&s.repo, &journal, "m1", closure(true)).unwrap();
    Leases::open(s.repo.clone(), Journal::open(s.owner.path(), s.data.path(), "m2").unwrap(), "m2").unwrap().finish().unwrap();
}

#[test]
fn opus_h2_moved_repository_can_be_closed_without_reusing_old_source_authority() {
    let s = Setup::new();
    let mut leases = s.open().unwrap();
    let lease = leases.acquire(s.request(&["src"])).unwrap();
    leases.prepare_launch(&lease, "launch", 3, 1).unwrap();
    let process = ProcessId { pid: 42, started_at: 9 };
    leases.attach_process(&lease, process).unwrap();
    drop(leases);
    let moved = tempdir();
    let root = moved.path().join("moved");
    fs::rename(s.owner.path(), &root).unwrap();
    let repo =
        RepoIdentity::new(&root.join(".git").canonicalize().unwrap().to_string_lossy(), &root.canonicalize().unwrap().to_string_lossy());
    let journal = Journal::open(&root, s.data.path(), "m1").unwrap();
    assert!(Leases::open(repo.clone(), Journal::open(&root, s.data.path(), "m1").unwrap(), "m1").is_err());
    assert_eq!(Leases::close_abandoned(&repo, &journal, "m1", closure(true)), Err(LeaseError::Live));
    let mut proof = closure(true);
    proof.processes = vec![process];
    Leases::close_abandoned(&repo, &journal, "m1", proof).unwrap();
    Leases::open(repo, Journal::open(&root, s.data.path(), "m2").unwrap(), "m2").unwrap().finish().unwrap();
    assert_eq!(journal.runtime().unwrap().leases[0].repo, s.repo.common_dir, "history keeps the original identity");
}

#[test]
fn opus_h2_failed_closure_persistence_keeps_repository_ownership() {
    let s = Setup::new();
    let leases = s.open().unwrap();
    drop(leases);
    let disk = Arc::new(FailingDisk(AtomicBool::new(true)));
    let journal = Journal::open_with(s.owner.path(), s.data.path(), "m1", disk).unwrap();
    assert!(Leases::close_abandoned(&s.repo, &journal, "m1", closure(true)).is_err());
    assert!(matches!(
        Leases::open(s.repo.clone(), Journal::open(s.owner.path(), s.data.path(), "m2").unwrap(), "m2"),
        Err(LeaseError::Owned)
    ));
}

#[test]
fn opus_h2_interrupted_owner_write_has_an_explicit_close_path_only_without_authority() {
    let s = Setup::new();
    drop(s.open().unwrap());
    fs::write(s.owner.path().join(".git/faber-project.lock"), "m1\n").unwrap();
    let journal = Journal::open(s.owner.path(), s.data.path(), "m1").unwrap();
    assert!(s.open().is_err());
    Leases::close_abandoned(&s.repo, &journal, "m1", closure(true)).unwrap();
    Leases::open(s.repo.clone(), Journal::open(s.owner.path(), s.data.path(), "m2").unwrap(), "m2").unwrap().finish().unwrap();
}

#[cfg(unix)]
#[test]
fn opus_m2_unlink_cannot_admit_another_live_coordinator_or_leave_old_effect_authority() {
    let s = Setup::new();
    let mut leases = s.open().unwrap();
    fs::remove_file(s.owner.path().join(".git/faber-project.lock")).unwrap();
    assert!(matches!(s.open(), Err(LeaseError::Owned)));
    assert_eq!(leases.acquire(s.request(&["src"])), Err(LeaseError::Owned));
}

#[test]
fn opus_l2_process_identity_cannot_be_attached_without_a_durable_launch_intent() {
    let s = Setup::new();
    let mut leases = s.open().unwrap();
    let lease = leases.acquire(s.request(&["src"])).unwrap();
    let process = ProcessId { pid: 42, started_at: 9 };
    assert_eq!(leases.attach_process(&lease, process), Err(LeaseError::Live));
    leases.prepare_launch(&lease, "launch", 3, 1).unwrap();
    drop(leases);
    let mut recovered = s.open().unwrap();
    assert_eq!(recovered.release(&lease, Termination { process: None, tree_dead: true, cost_settled: true }), Err(LeaseError::Live));
    recovered.attach_process(&lease, process).unwrap();
    recovered.release(&lease, Termination { process: Some(process), tree_dead: true, cost_settled: true }).unwrap();
    recovered.finish().unwrap();
}

#[test]
fn opus_l2_an_unresolved_launch_cannot_use_a_no_process_termination() {
    let s = Setup::new();
    let mut leases = s.open().unwrap();
    let lease = leases.acquire(s.request(&["src"])).unwrap();
    let mut launch = Intent::new("launch", "m1", IntentKind::Dispatch, 3, 1);
    launch.task = Some(lease.task.clone());
    launch.expected.insert("generation".into(), lease.generation.to_string());
    leases.journal().record_intent(&launch).unwrap();
    assert_eq!(leases.release(&lease, Termination { process: None, tree_dead: true, cost_settled: true }), Err(LeaseError::Live));
}

#[test]
fn opus_l3_invalid_file_ancestors_are_refused_without_quarantine() {
    let s = Setup::new();
    let mut leases = s.open().unwrap();
    let lease = leases.acquire(s.request(&["src"])).unwrap();
    let root = s.candidate();
    leases.bind(&lease, &root).unwrap();
    let write = FileOp::Write(WriteOp { path: "src/a.rs/b.rs".into(), expect: None, content: "invalid".into() });
    assert_eq!(leases.patch(&lease, Some(&s.grant()), 3, &write), Err(LeaseError::Path));
    assert_eq!(leases.read(&lease, "src/a.rs").unwrap().0, b"before");
}

#[test]
fn path_validation_refuses_existing_file_ancestors_on_every_os() {
    let dir = tempdir();
    let file = dir.path().join("file");
    fs::write(&file, "bytes").unwrap();
    assert_eq!(real_path(&file.join("child")), Err(LeaseError::Path));
    assert_eq!(real_path(&file), Ok(file), "a regular final file is still a valid path");
}

#[test]
fn opus_m3_broker_io_is_refused_while_a_worker_or_unacknowledged_launch_may_be_alive() {
    let s = Setup::new();
    let mut leases = s.open().unwrap();
    let lease = leases.acquire(s.request(&["src"])).unwrap();
    let root = s.candidate();
    leases.bind(&lease, &root).unwrap();
    let op = FileOp::Write(WriteOp { path: "src/a.rs".into(), expect: Some(hash(b"before")), content: "approved".into() });
    leases.prepare_launch(&lease, "launch", 3, 1).unwrap();
    assert_eq!(leases.read(&lease, "src/a.rs"), Err(LeaseError::Live));
    assert_eq!(leases.patch(&lease, Some(&s.grant()), 3, &op), Err(LeaseError::Live));
    let process = ProcessId { pid: 42, started_at: 9 };
    leases.attach_process(&lease, process).unwrap();
    assert_eq!(leases.read(&lease, "src/a.rs"), Err(LeaseError::Live));
    assert_eq!(leases.patch(&lease, Some(&s.grant()), 3, &op), Err(LeaseError::Live));
    assert_eq!(
        leases.settle_process(&lease, Termination { process: Some(process), tree_dead: false, cost_settled: true }),
        Err(LeaseError::Live)
    );
    assert_eq!(
        leases.settle_process(&lease, Termination { process: Some(process), tree_dead: true, cost_settled: false }),
        Err(LeaseError::Live)
    );
    leases.settle_process(&lease, Termination { process: Some(process), tree_dead: true, cost_settled: true }).unwrap();
    assert_eq!(leases.read(&lease, "src/a.rs").unwrap().0, b"before");
    leases.patch(&lease, Some(&s.grant()), 3, &op).unwrap();
    assert_eq!(leases.read(&lease, "src/a.rs").unwrap().0, b"approved");
    assert_eq!(leases.journal().runtime().unwrap().leases[0].process, Some(process));
}

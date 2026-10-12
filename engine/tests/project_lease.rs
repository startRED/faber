//! Exclusive leases and file requests on temporary directories; never spawn git or a worker.
use faber_engine::project::lease::{LeaseError, Leases, Termination, hash};
use faber_engine::project::store::Journal;
use faber_engine::project::wire::{DeleteOp, FileOp, RenameOp, WriteOp};
use faber_engine::project::{GitMode, Grant, Lease, ProcessId, RepoIdentity};
use faber_engine::store::{Disk, Fault, RealDisk};
use std::fs;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

struct Setup {
    owner: tempfile::TempDir,
    data: tempfile::TempDir,
    repo: RepoIdentity,
}
impl Setup {
    fn new() -> Self {
        let owner = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        fs::create_dir(owner.path().join(".git")).unwrap();
        let repo = RepoIdentity::new(
            &owner.path().join(".git").canonicalize().unwrap().to_string_lossy(),
            &owner.path().canonicalize().unwrap().to_string_lossy(),
        );
        Self { owner, data, repo }
    }
    fn open(&self) -> Result<Leases, LeaseError> {
        Leases::open(self.repo.clone(), Journal::open(self.data.path(), self.data.path(), "m1").unwrap(), "m1")
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
    assert_eq!(fs::read_dir(s.owner.path()).unwrap().count(), 1, "the owner is untouched");
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
    let journal = Journal::open_with(s.data.path(), s.data.path(), "m1", disk.clone()).unwrap();
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
    let elsewhere = tempfile::tempdir().unwrap();
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
    let journal = Journal::open_with(s.data.path(), s.data.path(), "m1", disk.clone()).unwrap();
    let mut leases = Leases::open(s.repo.clone(), journal, "m1").unwrap();
    let lease = leases.acquire(s.request(&["src"])).unwrap();
    let root = s.candidate();
    leases.bind(&lease, &root).unwrap();
    disk.armed.store(true, Ordering::SeqCst);
    let write = FileOp::Write(WriteOp { path: "src/a.rs".into(), expect: Some(hash(b"before")), content: "after".into() });
    assert_eq!(leases.patch(&lease, Some(&s.grant()), 3, &write), Err(LeaseError::Quarantined));
    assert_eq!(leases.check(&lease, "src/a.rs"), Err(LeaseError::Stale));
}

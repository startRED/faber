//! Tests for local update slots, exact-tree verification, canary gating,
//! probation deadlines, atomic manifest state, disk ownership (AS-6), and rollback.

use faber_engine::project::update::{
    CanaryVerdict, ExactTree, LastGoodRecord, RollbackAction, RollbackReason, Slot, SlotLayout, UpdateError, UpdateManager, UpdateManifest,
    UpdateState, is_forbidden_root,
};
use faber_engine::project::{GitMode, Grant, RepoIdentity, SCHEMA};
use faber_engine::store::{Disk, Fault};
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn tempdir() -> tempfile::TempDir {
    tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
}

fn valid_tree(hash: &str) -> ExactTree {
    ExactTree::new(&"1".repeat(40), &"2".repeat(40), hash, 1).unwrap()
}

fn sample_grant(repo: &str, self_update: bool) -> Grant {
    let mut grant = Grant::new(repo, GitMode::Seguro, 1, 100);
    grant.self_update = self_update;
    grant
}

fn sample_repo(root: &Path) -> RepoIdentity {
    let common_dir = root.join(".git").to_string_lossy().to_string();
    RepoIdentity::new(&common_dir, &root.to_string_lossy())
}

#[test]
fn slot_layout_rejects_installed_release_and_canonical_aliases() {
    for forbidden in [
        Path::new(r"E:\Apps\Faber"),
        Path::new(r"e:\apps\faber"),
        Path::new("E:/Apps/Faber"),
        Path::new("E:/Apps/Faber/"),
        Path::new(r"E:\Apps\Faber\sub"),
        Path::new(""),
        Path::new("/"),
        Path::new("E:"),
        Path::new("C:"),
    ] {
        assert!(is_forbidden_root(forbidden), "should be forbidden: {:?}", forbidden);
        assert!(
            matches!(SlotLayout::new(forbidden), Err(UpdateError::ForbiddenPath(_))),
            "SlotLayout::new must reject forbidden path: {:?}",
            forbidden
        );
    }

    let dir = tempdir();
    let dev_root = dir.path().join("faber-dev");
    let layout = SlotLayout::new(&dev_root).unwrap();
    assert_eq!(layout.dev_root(), &dev_root);
    assert_eq!(layout.slot_dir(Slot::A), dev_root.join("slots").join("a"));
    assert_eq!(layout.slot_dir(Slot::B), dev_root.join("slots").join("b"));
    assert_eq!(layout.staging_dir(), dev_root.join("staging"));
    assert_eq!(layout.quarantine_dir(), dev_root.join("quarantine"));
    assert_eq!(layout.manifest_path(), dev_root.join("manifest.json"));

    layout.init_dirs().unwrap();
    assert!(layout.slot_dir(Slot::A).is_dir());
    assert!(layout.slot_dir(Slot::B).is_dir());
    assert!(layout.staging_dir().is_dir());
    assert!(layout.quarantine_dir().is_dir());
}

#[test]
fn exact_tree_identity_validates_oids_and_verifies_payload_sha256() {
    let payload = b"hello binary world";
    let hash = ExactTree::compute_build_hash(payload);
    assert_eq!(hash.len(), 64);

    let tree = ExactTree::new(&"a".repeat(40), &"b".repeat(40), &hash, 1).unwrap();
    assert!(tree.matches_payload(payload));
    assert!(!tree.matches_payload(b"tampered content"));

    // Invalid commit OID
    assert!(matches!(ExactTree::new("short", &"b".repeat(40), &hash, 1), Err(UpdateError::InvalidTree(_))));
    // Invalid tree OID
    assert!(matches!(ExactTree::new(&"a".repeat(40), "invalid_oid!", &hash, 1), Err(UpdateError::InvalidTree(_))));
    // Invalid build hash
    assert!(matches!(ExactTree::new(&"a".repeat(40), &"b".repeat(40), "short", 1), Err(UpdateError::InvalidTree(_))));
    // Schema version 0
    assert!(matches!(ExactTree::new(&"a".repeat(40), &"b".repeat(40), &hash, 0), Err(UpdateError::InvalidTree(_))));
}

#[test]
fn crash_safe_atomic_manifest_round_trip_and_newer_schema_refusal() {
    let dir = tempdir();
    let layout = SlotLayout::new(&dir.path().join("dev")).unwrap();
    let mgr = UpdateManager::new(layout.clone()).unwrap();

    assert_eq!(mgr.manifest().unwrap(), None);

    let mut manifest = UpdateManifest::initial(Slot::A, 1000);
    manifest.extra.insert("customField".into(), serde_json::json!("preserved-value"));
    mgr.save_manifest(&manifest).unwrap();

    let read_back = mgr.manifest().unwrap().expect("manifest exists");
    assert_eq!(read_back.active_slot, Slot::A);
    assert_eq!(read_back.state, UpdateState::Idle);
    assert_eq!(read_back.extra.get("customField").unwrap(), "preserved-value");

    // Newer schema refusal
    let newer_manifest_json = serde_json::json!({
        "schema": SCHEMA + 1,
        "channel": "development",
        "activeSlot": "a",
        "state": "idle",
        "rollbackCount": 0,
        "updatedAt": 1000
    });
    fs::write(layout.manifest_path(), newer_manifest_json.to_string()).unwrap();
    assert_eq!(mgr.manifest(), Err(UpdateError::Newer));

    // Corrupt file
    fs::write(layout.manifest_path(), "{ broken json ...").unwrap();
    assert_eq!(mgr.manifest(), Err(UpdateError::Store(Fault::Corrupt)));
}

struct FaultyDisk {
    fail_write: AtomicBool,
    fail_rename: AtomicBool,
}

impl Disk for FaultyDisk {
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        if self.fail_write.load(Ordering::SeqCst) {
            return Err(io::Error::new(io::ErrorKind::StorageFull, "disk full"));
        }
        let mut file = fs::File::create(path)?;
        io::Write::write_all(&mut file, bytes)?;
        file.sync_all()
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        if self.fail_rename.load(Ordering::SeqCst) {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "rename denied"));
        }
        fs::rename(from, to)
    }
}

#[test]
fn disk_full_failure_preserves_previous_manifest() {
    let dir = tempdir();
    let layout = SlotLayout::new(&dir.path().join("dev")).unwrap();
    let disk = Arc::new(FaultyDisk { fail_write: AtomicBool::new(false), fail_rename: AtomicBool::new(false) });
    let mgr = UpdateManager::with_disk(layout.clone(), disk.clone()).unwrap();

    let initial = UpdateManifest::initial(Slot::A, 500);
    mgr.save_manifest(&initial).unwrap();

    // Now make writes fail
    disk.fail_write.store(true, Ordering::SeqCst);
    let mut updated = initial.clone();
    updated.active_slot = Slot::B;
    let err = mgr.save_manifest(&updated).unwrap_err();
    assert_eq!(err, UpdateError::Store(Fault::DiskFull));

    // Previous file remains untouched
    disk.fail_write.store(false, Ordering::SeqCst);
    let current = mgr.manifest().unwrap().unwrap();
    assert_eq!(current.active_slot, Slot::A);
}

#[test]
fn self_update_grant_permission_is_strictly_enforced() {
    let dir = tempdir();
    let layout = SlotLayout::new(&dir.path().join("dev")).unwrap();
    let mgr = UpdateManager::new(layout).unwrap();
    let repo = sample_repo(dir.path());
    let tree = valid_tree(&"a".repeat(64));

    // Grant without self_update
    let grant_no = sample_grant(&repo.common_dir, false);
    let err = mgr.prepare_next(tree.clone(), &grant_no, &repo, 1000).unwrap_err();
    assert_eq!(err, UpdateError::Grant("reason.grant.selfUpdate"));

    // Grant with self_update
    let grant_yes = sample_grant(&repo.common_dir, true);
    let target_slot = mgr.prepare_next(tree, &grant_yes, &repo, 1000).unwrap();
    assert_eq!(target_slot, Slot::B); // Active is A, next is B

    let m = mgr.manifest().unwrap().unwrap();
    assert_eq!(m.state, UpdateState::PreparingNext { slot: Slot::B, tree: valid_tree(&"a".repeat(64)) });
}

#[test]
fn canary_verdict_gates_readiness() {
    let dir = tempdir();
    let layout = SlotLayout::new(&dir.path().join("dev")).unwrap();
    let mgr = UpdateManager::new(layout).unwrap();
    let repo = sample_repo(dir.path());
    let grant = sample_grant(&repo.common_dir, true);
    let tree = valid_tree(&"a".repeat(64));

    mgr.prepare_next(tree.clone(), &grant, &repo, 1000).unwrap();

    // Failing canary marks Quarantined
    mgr.record_canary_verdict(CanaryVerdict::Fail { at: 1010, reason: "normalization test failed".into() }, 1010).unwrap();

    let m = mgr.manifest().unwrap().unwrap();
    assert!(matches!(m.state, UpdateState::Quarantined { slot: Slot::B, .. }));

    // Cannot swap while quarantined
    assert!(matches!(mgr.begin_swap(1020), Err(UpdateError::InvalidState(_))));

    // Reset and pass canary
    mgr.prepare_next(tree.clone(), &grant, &repo, 1030).unwrap();
    mgr.record_canary_verdict(CanaryVerdict::Pass { verified_at: 1040, receipt_id: "canary-receipt-1".into() }, 1040).unwrap();

    let m = mgr.manifest().unwrap().unwrap();
    assert!(matches!(m.state, UpdateState::Ready { slot: Slot::B, .. }));
}

#[test]
fn quiescence_checks_active_window_and_active_operations() {
    let dir = tempdir();
    let layout = SlotLayout::new(&dir.path().join("dev")).unwrap();
    let mgr = UpdateManager::new(layout).unwrap();
    let repo = sample_repo(dir.path());
    let grant = sample_grant(&repo.common_dir, true);
    let tree = valid_tree(&"a".repeat(64));

    mgr.prepare_next(tree, &grant, &repo, 1000).unwrap();
    mgr.record_canary_verdict(CanaryVerdict::Pass { verified_at: 1010, receipt_id: "r1".into() }, 1010).unwrap();

    // Active window blocks
    assert_eq!(mgr.enter_quiescence(true, false), Err(UpdateError::ActiveWindow));

    // Active operations block
    assert_eq!(mgr.enter_quiescence(false, true), Err(UpdateError::ActiveOperations));

    // Quiescence clean
    mgr.enter_quiescence(false, false).unwrap();
    let m = mgr.manifest().unwrap().unwrap();
    assert_eq!(m.state, UpdateState::Quiescent);
}

#[test]
fn probation_requires_health_ack_within_30_seconds() {
    let dir = tempdir();
    let layout = SlotLayout::new(&dir.path().join("dev")).unwrap();
    let mgr = UpdateManager::new(layout).unwrap();
    let repo = sample_repo(dir.path());
    let grant = sample_grant(&repo.common_dir, true);
    let tree = valid_tree(&"a".repeat(64));

    // Set initial last good on Slot A
    let initial_tree = valid_tree(&"0".repeat(64));
    let mut init_manifest = UpdateManifest::initial(Slot::A, 500);
    init_manifest.last_good = Some(LastGoodRecord::new(Slot::A, initial_tree, 400, 500));
    mgr.save_manifest(&init_manifest).unwrap();

    mgr.prepare_next(tree.clone(), &grant, &repo, 1000).unwrap();
    mgr.record_canary_verdict(CanaryVerdict::Pass { verified_at: 1010, receipt_id: "r1".into() }, 1010).unwrap();
    mgr.enter_quiescence(false, false).unwrap();
    mgr.begin_swap(1020).unwrap();

    // Start probation at t=1030 -> deadline is 1060
    mgr.begin_probation(1030).unwrap();
    let m = mgr.manifest().unwrap().unwrap();
    assert_eq!(m.active_slot, Slot::B);
    if let UpdateState::Probation { health_ack_deadline, .. } = m.state {
        assert_eq!(health_ack_deadline, 1060);
    } else {
        panic!("expected probation state");
    }

    // Health acknowledged at t=1050 (within 30s)
    mgr.acknowledge_health(1050).unwrap();

    // Complete probation at t=1330 (after 300s)
    mgr.finish_probation(1330).unwrap();
    let m = mgr.manifest().unwrap().unwrap();
    assert_eq!(m.state, UpdateState::Healthy);
    assert_eq!(m.active_slot, Slot::B);
    assert_eq!(m.last_good.as_ref().unwrap().slot, Slot::A);
}

#[test]
fn rollback_decision_restores_last_good_and_quarantines_failed_build() {
    let dir = tempdir();
    let layout = SlotLayout::new(&dir.path().join("dev")).unwrap();
    let mgr = UpdateManager::new(layout).unwrap();
    let repo = sample_repo(dir.path());
    let grant = sample_grant(&repo.common_dir, true);
    let tree = valid_tree(&"a".repeat(64));

    // Setup initial state with Slot A as last_good
    let mut manifest = UpdateManifest::initial(Slot::A, 500);
    manifest.last_good = Some(LastGoodRecord::new(Slot::A, valid_tree(&"0".repeat(64)), 400, 500));
    mgr.save_manifest(&manifest).unwrap();

    mgr.prepare_next(tree, &grant, &repo, 1000).unwrap();
    mgr.record_canary_verdict(CanaryVerdict::Pass { verified_at: 1010, receipt_id: "r1".into() }, 1010).unwrap();
    mgr.enter_quiescence(false, false).unwrap();
    mgr.begin_swap(1020).unwrap();
    mgr.begin_probation(1030).unwrap();

    // Past 30s without ack (at t=1070 > 1060)
    let action = mgr.trigger_rollback(RollbackReason::HealthTimeout, 1070).unwrap();
    assert_eq!(action, RollbackAction::RollbackToLastGood { target_slot: Slot::A, quarantine_slot: Slot::B });

    let m = mgr.manifest().unwrap().unwrap();
    assert_eq!(m.active_slot, Slot::A);
    assert_eq!(m.rollback_count, 1);
    assert!(matches!(
        m.state,
        UpdateState::RolledBack { restored_slot: Slot::A, quarantined_slot: Slot::B, reason: RollbackReason::HealthTimeout, .. }
    ));
}

#[test]
fn anti_restart_loop_guard_prevents_infinite_rollback() {
    let dir = tempdir();
    let layout = SlotLayout::new(&dir.path().join("dev")).unwrap();
    let mgr = UpdateManager::new(layout).unwrap();

    // Manifest that already performed a rollback (rollback_count = 1)
    let mut manifest = UpdateManifest::initial(Slot::A, 500);
    manifest.last_good = Some(LastGoodRecord::new(Slot::A, valid_tree(&"0".repeat(64)), 400, 500));
    manifest.rollback_count = 1;
    mgr.save_manifest(&manifest).unwrap();

    // Triggering another rollback without an intervening healthy promotion must be rejected
    let action = mgr.trigger_rollback(RollbackReason::ProbationCrash, 600).unwrap();
    assert_eq!(action, RollbackAction::CannotRollbackLoopPrevented);

    let m = mgr.manifest().unwrap().unwrap();
    assert!(matches!(m.state, UpdateState::Quarantined { .. }));
}

#[test]
fn disk_space_ownership_as6_retains_current_next_last_good_and_quarantine() {
    let dir = tempdir();
    let dev_root = dir.path().join("faber-dev");
    let layout = SlotLayout::new(&dev_root).unwrap();
    layout.init_dirs().unwrap();

    // Files in Slot A (current)
    let slot_a_bin = layout.slot_dir(Slot::A).join("faber.exe");
    fs::write(&slot_a_bin, b"slot a binary").unwrap();

    // Files in Slot B (staged next)
    let slot_b_bin = layout.slot_dir(Slot::B).join("faber.exe");
    fs::write(&slot_b_bin, b"slot b binary").unwrap();

    // Files in Quarantine
    let quar_file = layout.quarantine_dir().join("bad_build.dump");
    fs::write(&quar_file, b"evidence dump").unwrap();

    // Files in Staging (obsolete build output)
    let staging_file = layout.staging_dir().join("leftover.tmp");
    fs::write(&staging_file, b"temporary junk").unwrap();

    // Ownership check
    assert!(layout.is_owned_path(&slot_a_bin));
    assert!(layout.is_owned_path(&slot_b_bin));
    assert!(layout.is_owned_path(&quar_file));
    assert!(layout.is_owned_path(&staging_file));
    assert!(layout.is_owned_path(&layout.manifest_path()));
    assert!(!layout.is_owned_path(Path::new(r"E:\Apps\Faber")));
    assert!(!layout.is_owned_path(&dir.path().join("outside.txt")));

    // Clean stale builds: Slot A is current, Slot B is staged, last good is A
    let cleaned = layout.clean_stale_builds(Slot::A, Some(Slot::B), Some(Slot::A)).unwrap();

    // Staging file should be cleaned
    assert!(cleaned.contains(&staging_file));
    assert!(!staging_file.exists());

    // Slot A, Slot B, and Quarantine MUST be preserved
    assert!(slot_a_bin.exists());
    assert!(slot_b_bin.exists());
    assert!(quar_file.exists());
}

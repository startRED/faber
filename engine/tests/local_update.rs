//! Tests for local update slots, exact-tree verification, canary gating,
//! probation deadlines, atomic manifest state, disk ownership (AS-6), and rollback.

use faber_engine::project::update::{
    CanaryVerdict, ExactTree, LastGoodRecord, RollbackAction, RollbackReason, Slot, SlotLayout, SwapCheck, UpdateError, UpdateManager,
    UpdateManifest, UpdateState, is_forbidden_root,
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

// Synthetic states and corrupt records are test fixtures, never a public updater write path.
fn seed(mgr: &UpdateManager, manifest: &UpdateManifest) {
    fs::write(mgr.layout().manifest_path(), serde_json::to_vec(manifest).unwrap()).unwrap();
    assert_eq!(mgr.manifest().unwrap().as_ref(), Some(manifest));
}

fn slot_layout(root: &Path) -> Result<SlotLayout, UpdateError> {
    SlotLayout::new(root, &root.parent().unwrap_or(root).join("installed-data"), &root.parent().unwrap_or(root).join("installed-release"))
}

fn valid_tree(hash: &str) -> ExactTree {
    ExactTree::new(
        &"1".repeat(40),
        &ExactTree::compute_build_hash(hash.as_bytes())[..40],
        &ExactTree::compute_build_hash(hash.as_bytes()),
        1,
    )
    .unwrap()
}

fn payload() -> Vec<u8> {
    "a".repeat(64).into_bytes()
}

fn pass_for(tree: ExactTree, at: u64) -> CanaryVerdict {
    CanaryVerdict::Pass {
        verified_at: at,
        receipt_id: "canary-1".into(),
        tree,
        proof_receipt: "proof-1".into(),
        compatible_schemas: vec![SCHEMA],
    }
}

fn passed(at: u64) -> CanaryVerdict {
    pass_for(valid_tree(&"a".repeat(64)), at)
}

fn empty_pass() -> CanaryVerdict {
    let mut verdict = passed(1100);
    if let CanaryVerdict::Pass { receipt_id, .. } = &mut verdict {
        receipt_id.clear();
    }
    verdict
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

fn swap(mgr: &UpdateManager, now: u64) -> Result<Slot, UpdateError> {
    let repo = sample_repo(mgr.layout().dev_root().parent().unwrap());
    mgr.begin_swap(
        &sample_grant(&repo.common_dir, true),
        &repo,
        1,
        SwapCheck { payload: &payload(), active_window: false, active_operations: false },
        now,
    )
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
            matches!(slot_layout(forbidden), Err(UpdateError::ForbiddenPath(_))),
            "SlotLayout::new must reject forbidden path: {:?}",
            forbidden
        );
    }

    let dir = tempdir();
    let dev_root = dir.path().join("faber-dev");
    let layout = slot_layout(&dev_root).unwrap();
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
    assert_eq!(ExactTree::compute_build_hash(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
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
    assert!(matches!(ExactTree::new(&"a".repeat(40), &"b".repeat(64), &hash, 1), Err(UpdateError::InvalidTree(_))));
    // Invalid build hash
    assert!(matches!(ExactTree::new(&"a".repeat(40), &"b".repeat(40), "short", 1), Err(UpdateError::InvalidTree(_))));
    // Schema version 0
    assert!(matches!(ExactTree::new(&"a".repeat(40), &"b".repeat(40), &hash, 0), Err(UpdateError::InvalidTree(_))));
}

#[test]
fn crash_safe_atomic_manifest_round_trip_and_newer_schema_refusal() {
    let dir = tempdir();
    let layout = slot_layout(&dir.path().join("dev")).unwrap();
    let mgr = UpdateManager::new(layout.clone()).unwrap();

    assert_eq!(mgr.manifest().unwrap(), None);

    let mut manifest = UpdateManifest::initial(Slot::A, 1000);
    manifest.extra.insert("customField".into(), serde_json::json!("preserved-value"));
    seed(&mgr, &manifest);

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
    let layout = slot_layout(&dir.path().join("dev")).unwrap();
    let disk = Arc::new(FaultyDisk { fail_write: AtomicBool::new(false), fail_rename: AtomicBool::new(false) });
    let mgr = UpdateManager::with_disk(layout.clone(), disk.clone()).unwrap();

    let initial = UpdateManifest::initial(Slot::A, 500);
    seed(&mgr, &initial);

    // Now make writes fail
    disk.fail_write.store(true, Ordering::SeqCst);
    let repo = sample_repo(dir.path());
    let err = mgr.prepare_next(valid_tree("next"), &sample_grant(&repo.common_dir, true), &repo, 1, &[], 600).unwrap_err();
    assert_eq!(err, UpdateError::Store(Fault::DiskFull));

    // Previous file remains untouched
    disk.fail_write.store(false, Ordering::SeqCst);
    let current = mgr.manifest().unwrap().unwrap();
    assert_eq!(current.active_slot, Slot::A);
}

#[test]
fn self_update_grant_permission_is_strictly_enforced() {
    let dir = tempdir();
    let layout = slot_layout(&dir.path().join("dev")).unwrap();
    let mgr = UpdateManager::new(layout).unwrap();
    let repo = sample_repo(dir.path());
    let tree = valid_tree(&"a".repeat(64));

    // Grant without self_update
    let grant_no = sample_grant(&repo.common_dir, false);
    let err = mgr.prepare_next(tree.clone(), &grant_no, &repo, 1, &[], 1000).unwrap_err();
    assert_eq!(err, UpdateError::Grant("reason.grant.selfUpdate"));

    // Grant with self_update
    let grant_yes = sample_grant(&repo.common_dir, true);
    let target_slot = mgr.prepare_next(tree, &grant_yes, &repo, 1, &[], 1000).unwrap();
    assert_eq!(target_slot, Slot::B); // Active is A, next is B

    let m = mgr.manifest().unwrap().unwrap();
    assert_eq!(m.state, UpdateState::PreparingNext { slot: Slot::B, tree: valid_tree(&"a".repeat(64)) });
}

#[test]
fn canary_verdict_gates_readiness() {
    let dir = tempdir();
    let layout = slot_layout(&dir.path().join("dev")).unwrap();
    let mgr = UpdateManager::new(layout).unwrap();
    let repo = sample_repo(dir.path());
    let grant = sample_grant(&repo.common_dir, true);
    let tree = valid_tree(&"a".repeat(64));

    mgr.prepare_next(tree.clone(), &grant, &repo, 1, &[], 1000).unwrap();

    // Failing canary marks Quarantined
    mgr.record_canary_verdict(CanaryVerdict::Fail { at: 1010, reason: "normalization test failed".into() }, &payload(), 1010).unwrap();

    let m = mgr.manifest().unwrap().unwrap();
    assert!(matches!(m.state, UpdateState::Quarantined { slot: Slot::B, .. }));

    // Cannot swap while quarantined
    assert!(matches!(swap(&mgr, 1020), Err(UpdateError::InvalidState(_))));

    // Reset and pass canary
    mgr.prepare_next(valid_tree(&"b".repeat(64)), &grant, &repo, 1, &[], 1030).unwrap();
    mgr.record_canary_verdict(pass_for(valid_tree(&"b".repeat(64)), 1040), "b".repeat(64).as_bytes(), 1040).unwrap();

    let m = mgr.manifest().unwrap().unwrap();
    assert!(matches!(m.state, UpdateState::Ready { slot: Slot::B, .. }));
}

#[test]
fn quiescence_checks_active_window_and_active_operations() {
    let dir = tempdir();
    let layout = slot_layout(&dir.path().join("dev")).unwrap();
    let mgr = UpdateManager::new(layout).unwrap();
    let repo = sample_repo(dir.path());
    let grant = sample_grant(&repo.common_dir, true);
    let tree = valid_tree(&"a".repeat(64));

    mgr.prepare_next(tree, &grant, &repo, 1, &[], 1000).unwrap();
    mgr.record_canary_verdict(passed(1010), &payload(), 1010).unwrap();

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
    let layout = slot_layout(&dir.path().join("dev")).unwrap();
    let mgr = UpdateManager::new(layout).unwrap();
    let repo = sample_repo(dir.path());
    let grant = sample_grant(&repo.common_dir, true);
    let tree = valid_tree(&"a".repeat(64));

    // Set initial last good on Slot A
    let initial_tree = valid_tree(&"0".repeat(64));
    let mut init_manifest = UpdateManifest::initial(Slot::A, 500);
    init_manifest.last_good = Some(LastGoodRecord::new(Slot::A, initial_tree, 400, 500));
    seed(&mgr, &init_manifest);

    mgr.prepare_next(tree.clone(), &grant, &repo, 1, &[], 1000).unwrap();
    mgr.record_canary_verdict(passed(1010), &payload(), 1010).unwrap();
    mgr.enter_quiescence(false, false).unwrap();
    swap(&mgr, 1020).unwrap();

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
    mgr.complete_first_operation(1060).unwrap();
    mgr.finish_probation(1330).unwrap();
    let m = mgr.manifest().unwrap().unwrap();
    assert_eq!(m.state, UpdateState::Healthy);
    assert_eq!(m.active_slot, Slot::B);
    assert_eq!(m.last_good.as_ref().unwrap().slot, Slot::A);
}

#[test]
fn rollback_decision_restores_last_good_and_quarantines_failed_build() {
    let dir = tempdir();
    let layout = slot_layout(&dir.path().join("dev")).unwrap();
    let mgr = UpdateManager::new(layout).unwrap();
    let repo = sample_repo(dir.path());
    let grant = sample_grant(&repo.common_dir, true);
    let tree = valid_tree(&"a".repeat(64));

    // Setup initial state with Slot A as last_good
    let mut manifest = UpdateManifest::initial(Slot::A, 500);
    manifest.last_good = Some(LastGoodRecord::new(Slot::A, valid_tree(&"0".repeat(64)), 400, 500));
    seed(&mgr, &manifest);

    mgr.prepare_next(tree, &grant, &repo, 1, &[], 1000).unwrap();
    mgr.record_canary_verdict(passed(1010), &payload(), 1010).unwrap();
    mgr.enter_quiescence(false, false).unwrap();
    swap(&mgr, 1020).unwrap();
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
    let layout = slot_layout(&dir.path().join("dev")).unwrap();
    let mgr = UpdateManager::new(layout).unwrap();

    // Manifest that already performed a rollback (rollback_count = 1)
    let mut manifest = UpdateManifest::initial(Slot::A, 500);
    manifest.last_good = Some(LastGoodRecord::new(Slot::A, valid_tree(&"0".repeat(64)), 400, 500));
    manifest.rollback_count = 1;
    seed(&mgr, &manifest);

    // Triggering another rollback without an intervening healthy promotion must be rejected
    let action = mgr.trigger_rollback(RollbackReason::ProbationCrash, 600).unwrap();
    assert_eq!(action, RollbackAction::CannotRollbackLoopPrevented);

    let m = mgr.manifest().unwrap().unwrap();
    assert_eq!(m.state, UpdateState::Idle);
}

#[test]
fn disk_space_ownership_as6_retains_current_next_last_good_and_quarantine() {
    let dir = tempdir();
    let dev_root = dir.path().join("faber-dev");
    let layout = slot_layout(&dev_root).unwrap();
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
    let cleaned = layout.clean_stale_builds(&[(staging_file.clone(), ExactTree::compute_build_hash(b"temporary junk"))]).unwrap();

    // Staging file should be cleaned
    assert!(cleaned.contains(&staging_file));
    assert!(!staging_file.exists());

    // Slot A, Slot B, and Quarantine MUST be preserved
    assert!(slot_a_bin.exists());
    assert!(slot_b_bin.exists());
    assert!(quar_file.exists());
}

fn probation_manager(root: &Path) -> UpdateManager {
    let mgr = UpdateManager::new(slot_layout(&root.join("dev")).unwrap()).unwrap();
    let repo = sample_repo(root);
    mgr.adopt_baseline(Slot::A, valid_tree(&"0".repeat(64)), "0".repeat(64).as_bytes(), 500).unwrap();
    mgr.prepare_next(valid_tree(&"a".repeat(64)), &sample_grant(&repo.common_dir, true), &repo, 1, &[], 1000).unwrap();
    mgr.record_canary_verdict(passed(1010), &payload(), 1010).unwrap();
    mgr.enter_quiescence(false, false).unwrap();
    swap(&mgr, 1020).unwrap();
    mgr.begin_probation(1030).unwrap();
    mgr
}

#[test]
fn probation_cannot_complete_before_five_minutes() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    mgr.acknowledge_health(1050).unwrap();
    mgr.complete_first_operation(1051).unwrap();
    let before = mgr.manifest().unwrap();
    assert!(matches!(mgr.finish_probation(1051), Err(UpdateError::InvalidState(_))));
    assert_eq!(mgr.manifest().unwrap(), before);
}

#[test]
fn next_update_retains_the_healthy_current_build_before_reusing_previous_slot() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    mgr.acknowledge_health(1050).unwrap();
    mgr.complete_first_operation(1060).unwrap();
    mgr.finish_probation(1330).unwrap();
    let repo = sample_repo(dir.path());
    mgr.prepare_next(valid_tree(&"b".repeat(64)), &sample_grant(&repo.common_dir, true), &repo, 1, &[], 1400).unwrap();
    let m = mgr.manifest().unwrap().unwrap();
    let good = m.last_good.unwrap();
    assert_eq!(good.slot, Slot::B);
    assert_eq!(good.tree, valid_tree(&"a".repeat(64)));
}

#[test]
fn ready_cannot_swap_without_quiescence() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    let mut m = mgr.manifest().unwrap().unwrap();
    m.staged_tree = Some(valid_tree(&"b".repeat(64)));
    m.candidate_evidence = Some(pass_for(valid_tree(&"b".repeat(64)), 1010));
    m.state = UpdateState::Ready { slot: Slot::A, tree: valid_tree(&"b".repeat(64)), canary_receipt: "canary-1".into() };
    seed(&mgr, &m);
    assert!(matches!(swap(&mgr, 1400), Err(UpdateError::InvalidState(_))));
    assert_eq!(mgr.manifest().unwrap(), Some(m));
}

#[test]
fn repeated_rollback_does_not_quarantine_the_restored_build() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    mgr.trigger_rollback(RollbackReason::ProbationCrash, 1040).unwrap();
    let restored = mgr.manifest().unwrap();
    assert_eq!(mgr.trigger_rollback(RollbackReason::ProbationCrash, 1041).unwrap(), RollbackAction::CannotRollbackLoopPrevented);
    assert_eq!(mgr.manifest().unwrap(), restored);
}

#[test]
fn failed_candidate_cannot_be_retried_after_rollback() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    mgr.trigger_rollback(RollbackReason::ProbationCrash, 1040).unwrap();
    let repo = sample_repo(dir.path());
    assert!(matches!(
        mgr.prepare_next(valid_tree(&"a".repeat(64)), &sample_grant(&repo.common_dir, true), &repo, 1, &[], 1100),
        Err(UpdateError::InvalidState(_))
    ));
}

#[test]
fn timeout_returns_rollback_persistence_failure() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    let disk = Arc::new(FaultyDisk { fail_write: AtomicBool::new(true), fail_rename: AtomicBool::new(false) });
    let faulty = UpdateManager::with_disk(mgr.layout().clone(), disk).unwrap();
    assert_eq!(faulty.acknowledge_health(1061), Err(UpdateError::Store(Fault::DiskFull)));
    assert_eq!(faulty.finish_probation(1330), Err(UpdateError::Store(Fault::DiskFull)));
    assert!(matches!(mgr.manifest().unwrap().unwrap().state, UpdateState::Probation { .. }));
}

#[test]
fn ownership_rejects_traversal_prefixes_and_unknown_slot_names() {
    let dir = tempdir();
    let layout = slot_layout(&dir.path().join("dev")).unwrap();
    for path in [
        layout.dev_root().join("staging/../../outside"),
        layout.dev_root().join("staging-other/file"),
        layout.dev_root().join("quarantine-other/file"),
        layout.dev_root().join("slots/c/file"),
        dir.path().join("devstaging/file"),
    ] {
        assert!(!layout.is_owned_path(&path), "{}", path.display());
    }
    assert!(slot_layout(Path::new("relative-development")).is_err());
}

#[test]
fn cleanup_preserves_uncertain_staging_and_inactive_running_slot() {
    let dir = tempdir();
    let layout = slot_layout(&dir.path().join("dev")).unwrap();
    layout.init_dirs().unwrap();
    let running = layout.slot_dir(Slot::B).join("faber");
    let uncertain = layout.staging_dir().join("unfinished-build");
    fs::write(&running, b"running binary").unwrap();
    fs::write(&uncertain, b"not proven obsolete").unwrap();
    assert!(layout.clean_stale_builds(&[]).unwrap().is_empty());
    assert!(running.exists());
    assert!(uncertain.exists());
}

#[test]
fn rename_failure_preserves_entire_transition_and_removes_temp() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    let before = mgr.manifest().unwrap();
    let disk = Arc::new(FaultyDisk { fail_write: AtomicBool::new(false), fail_rename: AtomicBool::new(true) });
    let faulty = UpdateManager::with_disk(mgr.layout().clone(), disk).unwrap();
    assert_eq!(faulty.trigger_rollback(RollbackReason::ProbationCrash, 1040), Err(UpdateError::Store(Fault::Write)));
    assert_eq!(mgr.manifest().unwrap(), before);
    assert!(!mgr.layout().dev_root().join("manifest.json.tmp").exists());
}

#[test]
fn newer_nested_schema_and_invalid_deserialized_tree_fail_closed() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    let mut m = mgr.manifest().unwrap().unwrap();
    m.last_good.as_mut().unwrap().schema = SCHEMA + 1;
    fs::write(mgr.layout().manifest_path(), serde_json::to_vec(&m).unwrap()).unwrap();
    assert_eq!(mgr.manifest(), Err(UpdateError::Newer));
    m.last_good.as_mut().unwrap().schema = SCHEMA;
    m.last_good.as_mut().unwrap().tree.commit_oid = "invalid".into();
    fs::write(mgr.layout().manifest_path(), serde_json::to_vec(&m).unwrap()).unwrap();
    assert!(matches!(mgr.manifest(), Err(UpdateError::InvalidTree(_))));
}

#[test]
fn health_acknowledgment_is_monotonic_and_idempotent() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    assert!(mgr.acknowledge_health(1029).is_err());
    mgr.acknowledge_health(1060).unwrap();
    let acknowledged = mgr.manifest().unwrap();
    mgr.acknowledge_health(1100).unwrap();
    assert_eq!(mgr.manifest().unwrap(), acknowledged);
}

#[test]
fn healthy_promotion_requires_the_first_operation() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    mgr.acknowledge_health(1050).unwrap();
    assert!(mgr.finish_probation(1330).is_err());
}

#[test]
fn empty_canary_receipt_cannot_make_a_candidate_ready() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    let mut m = mgr.manifest().unwrap().unwrap();
    m.active_slot = Slot::A;
    m.state = UpdateState::PreparingNext { slot: Slot::B, tree: valid_tree(&"a".repeat(64)) };
    seed(&mgr, &m);
    assert!(mgr.record_canary_verdict(empty_pass(), &payload(), 1100).is_err());
}

#[test]
fn swap_requires_a_proven_previous_build() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    let mut m = mgr.manifest().unwrap().unwrap();
    m.active_slot = Slot::A;
    m.last_good = None;
    m.state = UpdateState::Quiescent;
    seed(&mgr, &m);
    assert!(swap(&mgr, 1100).is_err());
}

#[test]
fn interrupted_swap_quarantines_next_instead_of_current() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    let mut m = mgr.manifest().unwrap().unwrap();
    m.active_slot = Slot::A;
    m.state = UpdateState::Swapping { from_slot: Slot::A, to_slot: Slot::B, tree: valid_tree(&"a".repeat(64)) };
    seed(&mgr, &m);
    assert_eq!(
        mgr.trigger_rollback(RollbackReason::StartupCorrupt, 1040).unwrap(),
        RollbackAction::RollbackToLastGood { target_slot: Slot::A, quarantine_slot: Slot::B }
    );
}

#[cfg(unix)]
#[test]
fn linked_root_staging_and_manifest_are_refused() {
    use std::os::unix::fs::symlink;
    let dir = tempdir();
    let outside = dir.path().join("outside");
    fs::create_dir(&outside).unwrap();
    let alias = dir.path().join("alias");
    symlink(&outside, &alias).unwrap();
    assert!(slot_layout(&alias).is_err());
    let mgr = UpdateManager::new(slot_layout(&dir.path().join("dev")).unwrap()).unwrap();
    fs::remove_dir(mgr.layout().staging_dir()).unwrap();
    symlink(&outside, mgr.layout().staging_dir()).unwrap();
    let evidence = outside.join("evidence");
    fs::write(&evidence, b"keep").unwrap();
    assert!(!mgr.layout().is_owned_path(&mgr.layout().staging_dir().join("evidence")));
    assert!(mgr.layout().clean_stale_builds(&[]).is_err());
    assert_eq!(fs::read(&evidence).unwrap(), b"keep");
    symlink(&evidence, mgr.layout().dev_root().join("manifest.json.tmp")).unwrap();
    assert!(mgr.adopt_baseline(Slot::A, valid_tree("baseline"), b"baseline", 1).is_err());
    assert_eq!(fs::read(&evidence).unwrap(), b"keep");
}

#[test]
fn canary_requires_exact_payload_green_proof_and_rollback_schema() {
    for mutation in 0..6 {
        let dir = tempdir();
        let mgr = UpdateManager::new(slot_layout(&dir.path().join("dev")).unwrap()).unwrap();
        let repo = sample_repo(dir.path());
        mgr.prepare_next(valid_tree(&"a".repeat(64)), &sample_grant(&repo.common_dir, true), &repo, 1, &[], 1000).unwrap();
        let before = mgr.manifest().unwrap();
        let mut verdict = passed(1010);
        let mut bytes = payload();
        if let CanaryVerdict::Pass { tree, proof_receipt, compatible_schemas, verified_at, receipt_id } = &mut verdict {
            match mutation {
                0 => *tree = valid_tree(&"b".repeat(64)),
                1 => bytes.push(0),
                2 => proof_receipt.clear(),
                3 => compatible_schemas.clear(),
                4 => *verified_at = 999,
                5 => receipt_id.clear(),
                _ => unreachable!(),
            }
        }
        assert!(mgr.record_canary_verdict(verdict, &bytes, 1010).is_err(), "mutation {mutation}");
        assert_eq!(mgr.manifest().unwrap(), before);
    }
}

#[test]
fn cleanup_requires_explicit_unchanged_owned_file_inventory() {
    let dir = tempdir();
    let layout = slot_layout(&dir.path().join("dev")).unwrap();
    layout.init_dirs().unwrap();
    let stale = layout.staging_dir().join("old");
    fs::write(&stale, b"new bytes").unwrap();
    assert!(layout.clean_stale_builds(&[(stale.clone(), ExactTree::compute_build_hash(b"old bytes"))]).is_err());
    assert_eq!(fs::read(&stale).unwrap(), b"new bytes");
    let running = layout.slot_dir(Slot::B).join("running");
    fs::write(&running, b"running").unwrap();
    assert!(layout.clean_stale_builds(&[(running.clone(), ExactTree::compute_build_hash(b"running"))]).is_err());
    assert_eq!(fs::read(&running).unwrap(), b"running");
    let large = layout.staging_dir().join("large-output");
    let bytes = vec![7; 2 * 1024 * 1024];
    fs::write(&large, &bytes).unwrap();
    assert_eq!(layout.clean_stale_builds(&[(large.clone(), ExactTree::compute_build_hash(&bytes))]).unwrap(), vec![large.clone()]);
    assert!(!large.exists());
}

#[test]
fn open_request_cancels_quiescence_and_requires_a_new_gate() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    let mut manifest = mgr.manifest().unwrap().unwrap();
    manifest.active_slot = Slot::A;
    manifest.state = UpdateState::Quiescent;
    seed(&mgr, &manifest);
    mgr.cancel_quiescence().unwrap();
    assert!(swap(&mgr, 1100).is_err());
    assert!(mgr.enter_quiescence(false, false).is_err());
    mgr.record_canary_verdict(passed(1100), &payload(), 1100).unwrap();
    assert_eq!(mgr.enter_quiescence(true, false), Err(UpdateError::ActiveWindow));
}

#[test]
fn recovery_after_pending_swap_and_after_probation_restores_once() {
    for swapping in [true, false] {
        let dir = tempdir();
        let mgr = probation_manager(dir.path());
        if swapping {
            let mut manifest = mgr.manifest().unwrap().unwrap();
            manifest.active_slot = Slot::A;
            manifest.state = UpdateState::Swapping { from_slot: Slot::A, to_slot: Slot::B, tree: valid_tree(&"a".repeat(64)) };
            seed(&mgr, &manifest);
        }
        let restarted = UpdateManager::new(mgr.layout().clone()).unwrap();
        assert_eq!(
            restarted.recover_transition(1100).unwrap(),
            Some(RollbackAction::RollbackToLastGood { target_slot: Slot::A, quarantine_slot: Slot::B })
        );
        let restored = restarted.manifest().unwrap().unwrap();
        assert_eq!(restored.last_good.as_ref().unwrap().tree, valid_tree(&"0".repeat(64)));
        assert_eq!(restarted.recover_transition(1101).unwrap(), None);
        assert_eq!(restarted.manifest().unwrap(), Some(restored));
    }
}

#[test]
fn a_distinct_candidate_after_rollback_has_its_own_single_restore() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    mgr.trigger_rollback(RollbackReason::ProbationCrash, 1040).unwrap();
    let repo = sample_repo(dir.path());
    let candidate = valid_tree(&"b".repeat(64));
    mgr.prepare_next(candidate.clone(), &sample_grant(&repo.common_dir, true), &repo, 1, &[], 1100).unwrap();
    mgr.record_canary_verdict(pass_for(candidate, 1110), "b".repeat(64).as_bytes(), 1110).unwrap();
    mgr.enter_quiescence(false, false).unwrap();
    mgr.begin_swap(
        &sample_grant(&repo.common_dir, true),
        &repo,
        1,
        SwapCheck { payload: "b".repeat(64).as_bytes(), active_window: false, active_operations: false },
        1120,
    )
    .unwrap();
    mgr.begin_probation(1130).unwrap();
    assert_eq!(
        mgr.trigger_rollback(RollbackReason::FirstOperationFailed, 1140).unwrap(),
        RollbackAction::RollbackToLastGood { target_slot: Slot::A, quarantine_slot: Slot::B }
    );
    let restored = mgr.manifest().unwrap().unwrap();
    assert_eq!(restored.rejected_build_hashes.len(), 2);
    assert_eq!(mgr.trigger_rollback(RollbackReason::ProbationCrash, 1141).unwrap(), RollbackAction::CannotRollbackLoopPrevented);
    assert_eq!(mgr.manifest().unwrap(), Some(restored));
}

#[test]
fn probation_deadline_overflow_does_not_change_the_selected_slot() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    let mut manifest = mgr.manifest().unwrap().unwrap();
    manifest.active_slot = Slot::A;
    manifest.state = UpdateState::Swapping { from_slot: Slot::A, to_slot: Slot::B, tree: valid_tree(&"a".repeat(64)) };
    seed(&mgr, &manifest);
    assert!(mgr.begin_probation(u64::MAX - 299).is_err());
    assert_eq!(mgr.manifest().unwrap(), Some(manifest));
}

#[test]
fn all_transition_write_boundaries_preserve_the_previous_manifest() {
    for rename in [false, true] {
        for transition in 0..8 {
            let dir = tempdir();
            let mgr = probation_manager(dir.path());
            let mut manifest = mgr.manifest().unwrap().unwrap();
            match transition {
                0 => {
                    manifest.active_slot = Slot::A;
                    manifest.state = UpdateState::Idle;
                }
                1 => {
                    manifest.active_slot = Slot::A;
                    manifest.state = UpdateState::PreparingNext { slot: Slot::B, tree: valid_tree(&"a".repeat(64)) };
                }
                2 => {
                    manifest.active_slot = Slot::A;
                    manifest.state =
                        UpdateState::Ready { slot: Slot::B, tree: valid_tree(&"a".repeat(64)), canary_receipt: "canary-1".into() };
                }
                3 => {
                    manifest.active_slot = Slot::A;
                    manifest.state = UpdateState::Quiescent;
                }
                4 => {
                    manifest.active_slot = Slot::A;
                    manifest.state = UpdateState::Swapping { from_slot: Slot::A, to_slot: Slot::B, tree: valid_tree(&"a".repeat(64)) };
                }
                6 | 7 => {
                    if let UpdateState::Probation { health_acknowledged, .. } = &mut manifest.state {
                        *health_acknowledged = true;
                    }
                    manifest.first_operation_completed = transition == 7;
                }
                _ => {}
            }
            seed(&mgr, &manifest);
            let before = fs::read(mgr.layout().manifest_path()).unwrap();
            let disk = Arc::new(FaultyDisk { fail_write: AtomicBool::new(!rename), fail_rename: AtomicBool::new(rename) });
            let faulty = UpdateManager::with_disk(mgr.layout().clone(), disk).unwrap();
            let repo = sample_repo(dir.path());
            let grant = sample_grant(&repo.common_dir, true);
            let result = match transition {
                0 => faulty.prepare_next(valid_tree(&"b".repeat(64)), &grant, &repo, 1, &[], 1100).map(|_| ()),
                1 => faulty.record_canary_verdict(passed(1100), &payload(), 1100),
                2 => faulty.enter_quiescence(false, false),
                3 => swap(&faulty, 1100).map(|_| ()),
                4 => faulty.begin_probation(1100),
                5 => faulty.acknowledge_health(1050),
                6 => faulty.complete_first_operation(1100),
                7 => faulty.finish_probation(1330),
                _ => unreachable!(),
            };
            assert_eq!(result, Err(UpdateError::Store(if rename { Fault::Write } else { Fault::DiskFull })), "transition {transition}");
            assert_eq!(fs::read(mgr.layout().manifest_path()).unwrap(), before);
            assert!(!mgr.layout().dev_root().join("manifest.json.tmp").exists());
        }
    }
}

#[test]
fn additive_manifest_records_survive_an_old_reader_and_health_rollback() {
    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct OldManifest {
        schema: u32,
        channel: String,
        active_slot: Slot,
        state: serde_json::Value,
        staged_tree: Option<ExactTree>,
        last_good: Option<LastGoodRecord>,
        rollback_count: u32,
        updated_at: u64,
        #[serde(flatten)]
        extra: faber_engine::project::Extra,
    }
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    let mut manifest = mgr.manifest().unwrap().unwrap();
    manifest.extra.insert("consumedGrant".into(), serde_json::json!({ "revision": 9, "spent": 42 }));
    manifest.last_good.as_mut().unwrap().extra.insert("extension".into(), serde_json::json!("keep"));
    seed(&mgr, &manifest);
    mgr.acknowledge_health(1050).unwrap();
    mgr.complete_first_operation(1060).unwrap();
    let current = mgr.manifest().unwrap().unwrap();
    let old: OldManifest = serde_json::from_slice(&fs::read(mgr.layout().manifest_path()).unwrap()).unwrap();
    fs::write(mgr.layout().manifest_path(), serde_json::to_vec(&old).unwrap()).unwrap();
    assert_eq!(mgr.manifest().unwrap(), Some(current));
    mgr.trigger_rollback(RollbackReason::FirstOperationFailed, 1070).unwrap();
    let restored = mgr.manifest().unwrap().unwrap();
    assert_eq!(restored.extra, manifest.extra);
    assert_eq!(restored.last_good, manifest.last_good);
    assert_eq!(restored.current_good, manifest.last_good);
    assert_eq!(restored.rejected_build_hashes, vec![valid_tree(&"a".repeat(64)).build_hash]);
    let old: OldManifest = serde_json::from_slice(&fs::read(mgr.layout().manifest_path()).unwrap()).unwrap();
    fs::write(mgr.layout().manifest_path(), serde_json::to_vec(&old).unwrap()).unwrap();
    assert_eq!(mgr.manifest().unwrap(), Some(restored));
}

#[test]
fn manifest_larger_than_broker_text_limit_round_trips_through_a_transition() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    let mut manifest = mgr.manifest().unwrap().unwrap();
    manifest.extra.insert("extension".into(), serde_json::json!("x".repeat(16 * 1024)));
    seed(&mgr, &manifest);
    assert_eq!(mgr.manifest().unwrap(), Some(manifest.clone()));
    mgr.acknowledge_health(1050).unwrap();
    assert_eq!(mgr.manifest().unwrap().unwrap().extra, manifest.extra);
}

#[test]
fn manifest_temp_hardlink_never_truncates_an_outside_file() {
    let dir = tempdir();
    let mgr = UpdateManager::new(slot_layout(&dir.path().join("dev")).unwrap()).unwrap();
    let outside = dir.path().join("outside");
    fs::write(&outside, b"keep outside bytes").unwrap();
    fs::hard_link(&outside, mgr.layout().dev_root().join("manifest.json.tmp")).unwrap();
    mgr.adopt_baseline(Slot::A, valid_tree("baseline"), b"baseline", 1).unwrap();
    assert_eq!(fs::read(&outside).unwrap(), b"keep outside bytes");
}

#[test]
fn staging_cannot_overwrite_a_slot_still_used_by_an_older_process() {
    let dir = tempdir();
    let mgr = UpdateManager::new(slot_layout(&dir.path().join("dev")).unwrap()).unwrap();
    let repo = sample_repo(dir.path());
    let grant = sample_grant(&repo.common_dir, true);
    assert_eq!(mgr.prepare_next(valid_tree(&"a".repeat(64)), &grant, &repo, 1, &[Slot::B], 1000), Err(UpdateError::ActiveOperations));
    assert_eq!(mgr.manifest().unwrap(), None);
}

#[test]
fn swap_rechecks_current_grant_trust_revision_revocation_and_repo() {
    for mutation in 0..4 {
        let dir = tempdir();
        let mgr = probation_manager(dir.path());
        let mut manifest = mgr.manifest().unwrap().unwrap();
        manifest.active_slot = Slot::A;
        manifest.state = UpdateState::Quiescent;
        seed(&mgr, &manifest);
        let repo = sample_repo(dir.path());
        let mut grant = sample_grant(&repo.common_dir, true);
        let expected = match mutation {
            0 => {
                grant.self_update = false;
                "reason.grant.selfUpdate"
            }
            1 => {
                grant.revoked_at = Some(1100);
                "reason.grant.revoked"
            }
            2 => {
                grant.revision = 2;
                "reason.grant.stale"
            }
            3 => {
                grant.repo = "another-repository".into();
                "reason.grant.otherRepo"
            }
            _ => unreachable!(),
        };
        assert_eq!(
            mgr.begin_swap(&grant, &repo, 1, SwapCheck { payload: &payload(), active_window: false, active_operations: false }, 1100),
            Err(UpdateError::Grant(expected))
        );
        assert_eq!(mgr.manifest().unwrap(), Some(manifest));
    }
}

#[test]
fn corrupted_candidate_evidence_and_production_channel_are_refused_on_reload() {
    for mutation in 0..4 {
        let dir = tempdir();
        let mgr = probation_manager(dir.path());
        let mut manifest = mgr.manifest().unwrap().unwrap();
        match mutation {
            0 => manifest.channel = "release".into(),
            1 => manifest.candidate_evidence = None,
            2 => {
                if let Some(CanaryVerdict::Pass { proof_receipt, .. }) = &mut manifest.candidate_evidence {
                    proof_receipt.clear();
                }
            }
            3 => {
                if let Some(CanaryVerdict::Pass { compatible_schemas, .. }) = &mut manifest.candidate_evidence {
                    compatible_schemas.clear();
                }
            }
            _ => unreachable!(),
        }
        fs::write(mgr.layout().manifest_path(), serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(mgr.manifest().is_err(), "mutation {mutation}");
    }
}

#[test]
fn development_layout_rejects_configured_installed_data_and_overlapping_roots() {
    let dir = tempdir();
    let installed = dir.path().join("installed-data");
    for development in [installed.clone(), installed.join("development"), dir.path().to_path_buf()] {
        assert!(SlotLayout::new(&development, &installed, &dir.path().join("installed-release")).is_err());
    }
    assert!(SlotLayout::new(&dir.path().join("development"), &installed, &dir.path().join("installed-release")).is_ok());
}

#[cfg(windows)]
#[test]
fn installed_data_overlap_cannot_be_hidden_by_a_verbatim_path_prefix() {
    let dir = tempdir();
    let installed = dir.path().join("installed-data");
    let ordinary = std::path::PathBuf::from(installed.to_string_lossy().trim_start_matches(r"\\?\"));
    assert!(SlotLayout::new(&installed, &ordinary, &dir.path().join("installed-release")).is_err());
    assert!(SlotLayout::new(&ordinary, &installed, &dir.path().join("installed-release")).is_err());
}

#[test]
fn clock_step_back_cannot_block_a_safety_rollback() {
    for recovery in [false, true] {
        let dir = tempdir();
        let mgr = probation_manager(dir.path());
        let action = if recovery {
            mgr.recover_transition(900).unwrap().unwrap()
        } else {
            mgr.trigger_rollback(RollbackReason::ProbationCrash, 900).unwrap()
        };
        assert_eq!(action, RollbackAction::RollbackToLastGood { target_slot: Slot::A, quarantine_slot: Slot::B });
        let restored = mgr.manifest().unwrap().unwrap();
        assert_eq!(restored.active_slot, Slot::A);
        assert_eq!(restored.updated_at, 1030);
    }
}

#[test]
fn clock_step_back_during_probation_restores_previous_instead_of_waiting() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    assert!(mgr.finish_probation(900).is_err());
    assert_eq!(mgr.manifest().unwrap().unwrap().active_slot, Slot::A);
}

#[test]
fn bounded_first_operation_does_not_wait_for_a_user_and_restart_reconciles_probation() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    mgr.acknowledge_health(1050).unwrap();
    mgr.complete_first_operation(1051).unwrap();
    mgr.finish_probation(1330).unwrap();
    assert_eq!(mgr.recover_transition(1400).unwrap(), None);
    assert_eq!(mgr.manifest().unwrap().unwrap().state, UpdateState::Healthy);
    let interrupted = tempdir();
    let mgr = probation_manager(interrupted.path());
    mgr.acknowledge_health(1050).unwrap();
    assert!(mgr.complete_first_operation(1331).is_err());
    assert!(mgr.finish_probation(1331).is_err());
    assert_eq!(mgr.manifest().unwrap().unwrap().active_slot, Slot::A);
}

#[test]
fn rejected_history_stops_admission_before_it_can_exhaust_rollback_storage() {
    let dir = tempdir();
    let mgr = UpdateManager::new(slot_layout(&dir.path().join("dev")).unwrap()).unwrap();
    let repo = sample_repo(dir.path());
    let grant = sample_grant(&repo.common_dir, true);
    let mut stopped = false;
    for i in 0..130 {
        let now = 1000 + i * 2;
        if mgr.prepare_next(valid_tree(&format!("failed-{i}")), &grant, &repo, 1, &[], now).is_err() {
            stopped = true;
            break;
        }
        mgr.record_canary_verdict(CanaryVerdict::Fail { at: now + 1, reason: "canary".into() }, b"", now + 1).unwrap();
    }
    assert!(stopped, "history must be bounded without forgetting rejected builds");
    assert!(fs::metadata(mgr.layout().manifest_path()).unwrap().len() < 128 * 1024);
}

#[test]
fn rejection_identifies_a_build_and_does_not_ban_a_different_rebuild_of_the_same_source() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    mgr.trigger_rollback(RollbackReason::ProbationCrash, 1040).unwrap();
    let repo = sample_repo(dir.path());
    let mut rebuilt = valid_tree("different executable");
    rebuilt.tree_oid = valid_tree(&"a".repeat(64)).tree_oid;
    assert_eq!(mgr.prepare_next(rebuilt, &sample_grant(&repo.common_dir, true), &repo, 1, &[], 1100).unwrap(), Slot::B);
}

#[test]
fn swap_cannot_substitute_another_fully_authorized_repository() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    let mut manifest = mgr.manifest().unwrap().unwrap();
    manifest.active_slot = Slot::A;
    manifest.state = UpdateState::Quiescent;
    seed(&mgr, &manifest);
    let other = sample_repo(&dir.path().join("other"));
    assert!(
        mgr.begin_swap(
            &sample_grant(&other.common_dir, true),
            &other,
            1,
            SwapCheck { payload: &payload(), active_window: false, active_operations: false },
            1100
        )
        .is_err()
    );
    assert_eq!(mgr.manifest().unwrap(), Some(manifest));
}

#[test]
fn atomic_manifest_orphans_are_owned_but_require_explicit_obsolete_inventory() {
    let dir = tempdir();
    let layout = slot_layout(&dir.path().join("dev")).unwrap();
    layout.init_dirs().unwrap();
    let orphan = layout.dev_root().join(format!(".faber-patch-{}.tmp", uuid::Uuid::new_v4()));
    let unrelated = layout.dev_root().join(".faber-patch-owner.tmp");
    fs::write(&orphan, b"partial manifest").unwrap();
    fs::write(&unrelated, b"owner").unwrap();
    assert!(layout.is_owned_path(&orphan));
    assert!(!layout.is_owned_path(&unrelated));
    assert!(layout.clean_stale_builds(&[]).unwrap().is_empty());
    assert!(orphan.exists());
    assert_eq!(
        layout.clean_stale_builds(&[(orphan.clone(), ExactTree::compute_build_hash(b"partial manifest"))]).unwrap(),
        vec![orphan.clone()]
    );
    assert!(!orphan.exists());
    assert_eq!(fs::read(&unrelated).unwrap(), b"owner");
}

#[test]
fn uppercase_struct_literal_cannot_bypass_rejected_build_admission() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    mgr.trigger_rollback(RollbackReason::ProbationCrash, 1040).unwrap();
    let repo = sample_repo(dir.path());
    let before = mgr.manifest().unwrap();
    let mut tree = valid_tree(&"a".repeat(64));
    tree.commit_oid = tree.commit_oid.to_ascii_uppercase();
    tree.tree_oid = tree.tree_oid.to_ascii_uppercase();
    tree.build_hash = tree.build_hash.to_ascii_uppercase();
    assert!(mgr.prepare_next(tree, &sample_grant(&repo.common_dir, true), &repo, 1, &[], 1100).is_err());
    assert_eq!(mgr.manifest().unwrap(), before);
}

#[test]
fn newer_exact_tree_schema_is_refused_at_admission_and_nested_reload() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    let before = fs::read(mgr.layout().manifest_path()).unwrap();
    let repo = sample_repo(dir.path());
    let mut tree = valid_tree("newer");
    tree.schema_version = SCHEMA + 1;
    assert_eq!(mgr.prepare_next(tree.clone(), &sample_grant(&repo.common_dir, true), &repo, 1, &[], 1100), Err(UpdateError::Newer));
    assert_eq!(fs::read(mgr.layout().manifest_path()).unwrap(), before);
    for location in 0..5 {
        let mut manifest: UpdateManifest = serde_json::from_slice(&before).unwrap();
        match location {
            0 => manifest.last_good.as_mut().unwrap().tree.schema_version = SCHEMA + 1,
            1 => manifest.staged_tree.as_mut().unwrap().schema_version = SCHEMA + 1,
            2 => manifest.rejected_trees.push(tree.clone()),
            3 => {
                if let UpdateState::Probation { tree, .. } = &mut manifest.state {
                    tree.schema_version = SCHEMA + 1
                }
            }
            4 => {
                if let Some(CanaryVerdict::Pass { tree, .. }) = &mut manifest.candidate_evidence {
                    tree.schema_version = SCHEMA + 1
                }
            }
            _ => unreachable!(),
        }
        fs::write(mgr.layout().manifest_path(), serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert_eq!(mgr.manifest(), Err(UpdateError::Newer), "nested location {location}");
    }
}

#[test]
fn baseline_adoption_verifies_payload_and_never_overwrites_existing_state() {
    let dir = tempdir();
    let mgr = UpdateManager::new(slot_layout(&dir.path().join("dev")).unwrap()).unwrap();
    let tree = valid_tree("running healthy build");
    assert!(mgr.adopt_baseline(Slot::A, tree.clone(), b"wrong bytes", 500).is_err());
    assert_eq!(mgr.manifest().unwrap(), None);
    mgr.adopt_baseline(Slot::A, tree.clone(), b"running healthy build", 500).unwrap();
    let baseline = mgr.manifest().unwrap().unwrap();
    assert_eq!(baseline.last_good.as_ref().unwrap().tree, tree);
    assert_eq!(baseline.current_good, baseline.last_good);
    assert!(mgr.adopt_baseline(Slot::B, valid_tree("other"), b"other", 501).is_err());
    assert_eq!(mgr.manifest().unwrap(), Some(baseline));
}

#[test]
fn configured_installed_release_is_excluded_even_outside_apps_faber() {
    let dir = tempdir();
    let release = dir.path().join("custom-install");
    let data = dir.path().join("installed-data");
    for development in [release.clone(), release.join("dev"), dir.path().to_path_buf()] {
        assert!(SlotLayout::new(&development, &data, &release).is_err());
    }
    assert!(SlotLayout::new(&dir.path().join("development"), &data, &release).is_ok());
    assert!(SlotLayout::new(&dir.path().join("development"), &data, Path::new(r"E:\Apps\Faber")).is_ok());
}

#[test]
fn swap_requires_fresh_window_process_and_payload_observations() {
    for mutation in 0..3 {
        let dir = tempdir();
        let mgr = probation_manager(dir.path());
        let mut manifest = mgr.manifest().unwrap().unwrap();
        manifest.active_slot = Slot::A;
        manifest.state = UpdateState::Quiescent;
        seed(&mgr, &manifest);
        let repo = sample_repo(dir.path());
        let bytes = if mutation == 2 { b"changed after canary".to_vec() } else { payload() };
        let check = SwapCheck { payload: &bytes, active_window: mutation == 0, active_operations: mutation == 1 };
        assert!(mgr.begin_swap(&sample_grant(&repo.common_dir, true), &repo, 1, check, 1100).is_err(), "mutation {mutation}");
        assert_eq!(mgr.manifest().unwrap(), Some(manifest));
    }
}

#[test]
fn swap_reserves_space_for_rollback_before_selecting_the_candidate() {
    let dir = tempdir();
    let mgr = probation_manager(dir.path());
    let mut manifest = mgr.manifest().unwrap().unwrap();
    manifest.active_slot = Slot::A;
    manifest.state = UpdateState::Quiescent;
    manifest.last_good.as_mut().unwrap().extra.insert("extension".into(), serde_json::json!("x".repeat(64 * 1024)));
    manifest.extra.insert("extension".into(), serde_json::json!("x".repeat(945 * 1024)));
    seed(&mgr, &manifest);
    assert!(fs::metadata(mgr.layout().manifest_path()).unwrap().len() < 1024 * 1024);
    assert!(swap(&mgr, 1100).is_err());
    assert_eq!(mgr.manifest().unwrap(), Some(manifest));
}

#[test]
fn cleanup_error_reports_the_files_already_removed() {
    let dir = tempdir();
    let layout = slot_layout(&dir.path().join("dev")).unwrap();
    layout.init_dirs().unwrap();
    let old = layout.staging_dir().join("old");
    let changed = layout.staging_dir().join("changed");
    fs::write(&old, b"old").unwrap();
    fs::write(&changed, b"new").unwrap();
    let error = layout
        .clean_stale_builds(&[
            (old.clone(), ExactTree::compute_build_hash(b"old")),
            (changed.clone(), ExactTree::compute_build_hash(b"old")),
        ])
        .unwrap_err();
    assert!(matches!(error, UpdateError::Cleanup { cleaned, .. } if cleaned == vec![old.clone()]));
    assert!(!old.exists());
    assert_eq!(fs::read(changed).unwrap(), b"new");
}

#[cfg(target_os = "macos")]
#[test]
fn macos_case_variant_overlap_is_conservatively_refused() {
    let dir = tempdir();
    let installed = dir.path().join("installed-data");
    let release = dir.path().join("release");
    assert!(SlotLayout::new(&dir.path().join("INSTALLED-DATA/dev"), &installed, &release).is_err());
    assert!(SlotLayout::new(&dir.path().join("RELEASE/dev"), &installed, &release).is_err());
}

#[test]
fn corrupted_healthy_rollback_and_ready_receipt_state_is_refused() {
    for mutation in 0..3 {
        let dir = tempdir();
        let mgr = probation_manager(dir.path());
        if mutation == 0 {
            mgr.acknowledge_health(1050).unwrap();
            mgr.complete_first_operation(1060).unwrap();
            mgr.finish_probation(1330).unwrap();
        } else if mutation == 1 {
            mgr.trigger_rollback(RollbackReason::ProbationCrash, 1040).unwrap();
        }
        let mut manifest = mgr.manifest().unwrap().unwrap();
        if mutation < 2 {
            manifest.active_slot = manifest.active_slot.other();
        } else {
            manifest.active_slot = Slot::A;
            manifest.state =
                UpdateState::Ready { slot: Slot::B, tree: valid_tree(&"a".repeat(64)), canary_receipt: "another-receipt".into() };
        }
        fs::write(mgr.layout().manifest_path(), serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(mgr.manifest().is_err(), "mutation {mutation}");
    }
}

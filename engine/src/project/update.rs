//! Local update slots and rollback state machine (R93, R96 Task 11).
//!
//! Controls two development slots (A and B), exact-tree verification,
//! last-good build retention, canary verdict gating, quiescence checks,
//! 30-second health acknowledgment and 5-minute probation, atomic manifest
//! persistence, and automatic rollback with anti-restart loop protection.

use super::git::valid_oid;
use super::{Effect, Extra, Grant, RepoIdentity, SCHEMA};
use crate::store::{Disk, Fault, RealDisk, read_checked, replace};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Whether `path` resolves to a forbidden destination such as installed release `E:\Apps\Faber`
/// or any root/alias of it.
pub fn is_forbidden_root(path: &Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/").to_lowercase();
    let trimmed = s.trim_end_matches('/');
    if trimmed.is_empty() {
        return true;
    }
    if trimmed == "e:/apps/faber" || trimmed.ends_with("/apps/faber") || trimmed.contains("apps/faber") {
        return true;
    }
    if trimmed.len() <= 3 && trimmed.ends_with(':') {
        return true;
    }
    false
}

/// The two local build slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Slot {
    A,
    B,
}

impl Slot {
    pub fn other(self) -> Slot {
        match self {
            Slot::A => Slot::B,
            Slot::B => Slot::A,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Slot::A => "a",
            Slot::B => "b",
        }
    }
}

impl std::fmt::Display for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Reason why an update operation was refused or failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    Store(Fault),
    ForbiddenPath(String),
    InvalidTree(String),
    Grant(&'static str),
    InvalidState(&'static str),
    Newer,
    ActiveWindow,
    ActiveOperations,
    NoLastGood,
    RollbackLoop,
    CanaryFailed(String),
}

impl UpdateError {
    pub fn key(&self) -> &'static str {
        match self {
            Self::Store(fault) => fault.key(),
            Self::ForbiddenPath(_) => "error.update.forbiddenPath",
            Self::InvalidTree(_) => "error.update.invalidTree",
            Self::Grant(k) => k,
            Self::InvalidState(_) => "error.update.invalidState",
            Self::Newer => "error.update.newer",
            Self::ActiveWindow => "error.update.activeWindow",
            Self::ActiveOperations => "error.update.activeOperations",
            Self::NoLastGood => "error.update.noLastGood",
            Self::RollbackLoop => "error.update.rollbackLoop",
            Self::CanaryFailed(_) => "error.update.canaryFailed",
        }
    }
}

impl From<Fault> for UpdateError {
    fn from(fault: Fault) -> Self {
        Self::Store(fault)
    }
}

/// Verifiable identity of an exact tree and build payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExactTree {
    pub commit_oid: String,
    pub tree_oid: String,
    pub build_hash: String,
    pub schema_version: u32,
    #[serde(flatten)]
    pub extra: Extra,
}

impl ExactTree {
    pub fn new(commit_oid: &str, tree_oid: &str, build_hash: &str, schema_version: u32) -> Result<Self, UpdateError> {
        if !valid_oid(commit_oid) {
            return Err(UpdateError::InvalidTree("invalid commit oid".into()));
        }
        if !valid_oid(tree_oid) {
            return Err(UpdateError::InvalidTree("invalid tree oid".into()));
        }
        if build_hash.len() != 64 || !build_hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(UpdateError::InvalidTree("invalid build hash".into()));
        }
        if schema_version == 0 {
            return Err(UpdateError::InvalidTree("schema version must be greater than 0".into()));
        }
        Ok(ExactTree {
            commit_oid: commit_oid.to_ascii_lowercase(),
            tree_oid: tree_oid.to_ascii_lowercase(),
            build_hash: build_hash.to_ascii_lowercase(),
            schema_version,
            extra: Extra::new(),
        })
    }

    pub fn compute_build_hash(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn matches_payload(&self, bytes: &[u8]) -> bool {
        Self::compute_build_hash(bytes) == self.build_hash
    }
}

/// Structured outcome of a miniature canary build run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CanaryVerdict {
    Pass { verified_at: u64, receipt_id: String },
    Fail { at: u64, reason: String },
    NotRun,
}

impl CanaryVerdict {
    pub fn is_pass(&self) -> bool {
        matches!(self, CanaryVerdict::Pass { .. })
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            CanaryVerdict::Fail { reason, .. } => Some(reason.as_str()),
            _ => None,
        }
    }
}

/// The immutable record of the last known good build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastGoodRecord {
    pub schema: u32,
    pub slot: Slot,
    pub tree: ExactTree,
    pub promoted_at: u64,
    pub healthy_at: u64,
    #[serde(flatten)]
    pub extra: Extra,
}

impl LastGoodRecord {
    pub fn new(slot: Slot, tree: ExactTree, promoted_at: u64, healthy_at: u64) -> Self {
        LastGoodRecord { schema: SCHEMA, slot, tree, promoted_at, healthy_at, extra: Extra::new() }
    }
}

/// Why a rollback was triggered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RollbackReason {
    HealthTimeout,
    ProbationCrash,
    IncompatibleSchema,
    FirstOperationFailed,
    StartupCorrupt,
    Manual,
    Custom(String),
}

/// Action determined by the rollback evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RollbackAction {
    RollbackToLastGood { target_slot: Slot, quarantine_slot: Slot },
    CannotRollbackNoLastGood,
    CannotRollbackLoopPrevented,
}

/// Lifecycle states of local update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateState {
    Idle,
    PreparingNext { slot: Slot, tree: ExactTree },
    CanaryPending { slot: Slot, tree: ExactTree },
    Ready { slot: Slot, tree: ExactTree, canary_receipt: String },
    Quiescent,
    Swapping { from_slot: Slot, to_slot: Slot, tree: ExactTree },
    Probation { slot: Slot, tree: ExactTree, started_at: u64, health_ack_deadline: u64, probation_deadline: u64, health_acknowledged: bool },
    Healthy,
    RolledBack { restored_slot: Slot, quarantined_slot: Slot, reason: RollbackReason, at: u64 },
    Quarantined { slot: Slot, reason: String, at: u64 },
}

/// Manifest storing the state of local update slots and last-good record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateManifest {
    pub schema: u32,
    pub channel: String,
    pub active_slot: Slot,
    pub state: UpdateState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub staged_tree: Option<ExactTree>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_good: Option<LastGoodRecord>,
    pub rollback_count: u32,
    pub updated_at: u64,
    #[serde(flatten)]
    pub extra: Extra,
}

impl UpdateManifest {
    pub fn initial(active_slot: Slot, at: u64) -> Self {
        UpdateManifest {
            schema: SCHEMA,
            channel: "development".into(),
            active_slot,
            state: UpdateState::Idle,
            staged_tree: None,
            last_good: None,
            rollback_count: 0,
            updated_at: at,
            extra: Extra::new(),
        }
    }
}

/// Layout of slot directories under the development root.
#[derive(Debug, Clone)]
pub struct SlotLayout {
    dev_root: PathBuf,
}

impl SlotLayout {
    pub fn new(dev_root: &Path) -> Result<Self, UpdateError> {
        if is_forbidden_root(dev_root) {
            return Err(UpdateError::ForbiddenPath(dev_root.to_string_lossy().into()));
        }
        Ok(SlotLayout { dev_root: dev_root.to_path_buf() })
    }

    pub fn dev_root(&self) -> &Path {
        &self.dev_root
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.dev_root.join("manifest.json")
    }

    pub fn slots_dir(&self) -> PathBuf {
        self.dev_root.join("slots")
    }

    pub fn slot_dir(&self, slot: Slot) -> PathBuf {
        self.slots_dir().join(slot.as_str())
    }

    pub fn staging_dir(&self) -> PathBuf {
        self.dev_root.join("staging")
    }

    pub fn quarantine_dir(&self) -> PathBuf {
        self.dev_root.join("quarantine")
    }

    pub fn init_dirs(&self) -> Result<(), UpdateError> {
        for dir in [self.slot_dir(Slot::A), self.slot_dir(Slot::B), self.staging_dir(), self.quarantine_dir()] {
            fs::create_dir_all(&dir).map_err(|e| UpdateError::Store(Fault::of_write(&e)))?;
        }
        Ok(())
    }

    pub fn is_owned_path(&self, path: &Path) -> bool {
        let norm_path = path.to_string_lossy().replace('\\', "/").to_lowercase();
        let norm_root = self.dev_root.to_string_lossy().replace('\\', "/").to_lowercase();
        if !norm_path.starts_with(&norm_root) {
            return false;
        }
        let rel = norm_path.strip_prefix(&norm_root).unwrap_or("").trim_start_matches('/');
        rel.starts_with("slots/")
            || rel.starts_with("staging")
            || rel.starts_with("quarantine")
            || rel == "manifest.json"
            || rel.starts_with("manifest.json.tmp")
    }

    pub fn clean_stale_builds(
        &self,
        current: Slot,
        staged: Option<Slot>,
        last_good_slot: Option<Slot>,
    ) -> Result<Vec<PathBuf>, UpdateError> {
        let mut cleaned = Vec::new();
        let inactive = current.other();
        let is_preserved = staged == Some(inactive) || last_good_slot == Some(inactive);
        if !is_preserved {
            let inactive_dir = self.slot_dir(inactive);
            if let Ok(entries) = fs::read_dir(&inactive_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if fs::remove_file(&path).is_ok() || fs::remove_dir_all(&path).is_ok() {
                        cleaned.push(path);
                    }
                }
            }
        }
        let staging = self.staging_dir();
        if let Ok(entries) = fs::read_dir(&staging) {
            for entry in entries.flatten() {
                let path = entry.path();
                if fs::remove_file(&path).is_ok() || fs::remove_dir_all(&path).is_ok() {
                    cleaned.push(path);
                }
            }
        }
        Ok(cleaned)
    }
}

/// Manages slots, transitions, manifests, and rollback decisions.
pub struct UpdateManager {
    layout: SlotLayout,
    disk: Arc<dyn Disk>,
}

impl UpdateManager {
    pub fn new(layout: SlotLayout) -> Result<Self, UpdateError> {
        Self::with_disk(layout, Arc::new(RealDisk))
    }

    pub fn with_disk(layout: SlotLayout, disk: Arc<dyn Disk>) -> Result<Self, UpdateError> {
        layout.init_dirs()?;
        Ok(UpdateManager { layout, disk })
    }

    pub fn layout(&self) -> &SlotLayout {
        &self.layout
    }

    pub fn manifest(&self) -> Result<Option<UpdateManifest>, UpdateError> {
        match read_checked::<UpdateManifest>(&self.layout.manifest_path())? {
            Some(m) if m.schema > SCHEMA => Err(UpdateError::Newer),
            found => Ok(found),
        }
    }

    pub fn save_manifest(&self, manifest: &UpdateManifest) -> Result<(), UpdateError> {
        let json = serde_json::to_string_pretty(manifest).expect("plain data serializes");
        replace(&*self.disk, &self.layout.manifest_path(), json.as_bytes())?;
        Ok(())
    }

    pub fn prepare_next(&self, tree: ExactTree, grant: &Grant, repo: &RepoIdentity, now: u64) -> Result<Slot, UpdateError> {
        grant.allows(repo, grant.revision, Effect::SelfUpdate).map_err(UpdateError::Grant)?;

        let mut manifest = self.manifest()?.unwrap_or_else(|| UpdateManifest::initial(Slot::A, now));
        let next_slot = manifest.active_slot.other();

        match &manifest.state {
            UpdateState::Idle | UpdateState::RolledBack { .. } | UpdateState::Quarantined { .. } | UpdateState::Healthy => {}
            _ => return Err(UpdateError::InvalidState("must be idle, rolled back or quarantined to prepare next")),
        }

        manifest.staged_tree = Some(tree.clone());
        manifest.state = UpdateState::PreparingNext { slot: next_slot, tree };
        manifest.updated_at = now;
        self.save_manifest(&manifest)?;
        Ok(next_slot)
    }

    pub fn record_canary_verdict(&self, verdict: CanaryVerdict, now: u64) -> Result<(), UpdateError> {
        let mut manifest = self.manifest()?.ok_or(UpdateError::InvalidState("no manifest initialized"))?;
        let (slot, tree) = match &manifest.state {
            UpdateState::PreparingNext { slot, tree } | UpdateState::CanaryPending { slot, tree } => (*slot, tree.clone()),
            _ => return Err(UpdateError::InvalidState("not in canary state")),
        };

        match verdict {
            CanaryVerdict::Pass { receipt_id, .. } => {
                manifest.state = UpdateState::Ready { slot, tree, canary_receipt: receipt_id };
            }
            CanaryVerdict::Fail { reason, at } => {
                manifest.state = UpdateState::Quarantined { slot, reason, at };
            }
            CanaryVerdict::NotRun => {
                return Err(UpdateError::InvalidState("canary not run"));
            }
        }
        manifest.updated_at = now;
        self.save_manifest(&manifest)?;
        Ok(())
    }

    pub fn enter_quiescence(&self, active_window: bool, active_operations: bool) -> Result<(), UpdateError> {
        if active_window {
            return Err(UpdateError::ActiveWindow);
        }
        if active_operations {
            return Err(UpdateError::ActiveOperations);
        }
        let mut manifest = self.manifest()?.ok_or(UpdateError::InvalidState("no manifest initialized"))?;
        match &manifest.state {
            UpdateState::Ready { .. } => {
                manifest.state = UpdateState::Quiescent;
                self.save_manifest(&manifest)?;
                Ok(())
            }
            UpdateState::Quiescent => Ok(()),
            _ => Err(UpdateError::InvalidState("must be ready to enter quiescence")),
        }
    }

    pub fn begin_swap(&self, now: u64) -> Result<Slot, UpdateError> {
        let mut manifest = self.manifest()?.ok_or(UpdateError::InvalidState("no manifest initialized"))?;
        let (target_slot, tree) = match &manifest.state {
            UpdateState::Ready { slot, tree, .. } => (*slot, tree.clone()),
            UpdateState::Quiescent => {
                let slot = manifest.active_slot.other();
                let tree = manifest.staged_tree.clone().ok_or(UpdateError::InvalidState("no staged tree"))?;
                (slot, tree)
            }
            _ => return Err(UpdateError::InvalidState("must be ready or quiescent to swap")),
        };

        let from_slot = manifest.active_slot;
        manifest.state = UpdateState::Swapping { from_slot, to_slot: target_slot, tree };
        manifest.updated_at = now;
        self.save_manifest(&manifest)?;
        Ok(target_slot)
    }

    pub fn begin_probation(&self, now: u64) -> Result<(), UpdateError> {
        let mut manifest = self.manifest()?.ok_or(UpdateError::InvalidState("no manifest initialized"))?;
        let (to_slot, tree) = match &manifest.state {
            UpdateState::Swapping { to_slot, tree, .. } => (*to_slot, tree.clone()),
            _ => return Err(UpdateError::InvalidState("must be in swapping state")),
        };
        manifest.active_slot = to_slot;
        manifest.state = UpdateState::Probation {
            slot: to_slot,
            tree,
            started_at: now,
            health_ack_deadline: now + 30,
            probation_deadline: now + 300,
            health_acknowledged: false,
        };
        manifest.updated_at = now;
        self.save_manifest(&manifest)?;
        Ok(())
    }

    pub fn acknowledge_health(&self, now: u64) -> Result<(), UpdateError> {
        let mut manifest = self.manifest()?.ok_or(UpdateError::InvalidState("no manifest initialized"))?;
        match &mut manifest.state {
            UpdateState::Probation { health_ack_deadline, health_acknowledged, .. } => {
                if now > *health_ack_deadline {
                    drop(self.trigger_rollback(RollbackReason::HealthTimeout, now));
                    return Err(UpdateError::InvalidState("health acknowledgment deadline expired"));
                }
                *health_acknowledged = true;
                manifest.updated_at = now;
                self.save_manifest(&manifest)?;
                Ok(())
            }
            _ => Err(UpdateError::InvalidState("not in probation state")),
        }
    }

    pub fn finish_probation(&self, now: u64) -> Result<(), UpdateError> {
        let mut manifest = self.manifest()?.ok_or(UpdateError::InvalidState("no manifest initialized"))?;
        let (slot, tree, acknowledged, promoted_at) = match &manifest.state {
            UpdateState::Probation { slot, tree, health_acknowledged, started_at, .. } => {
                (*slot, tree.clone(), *health_acknowledged, *started_at)
            }
            _ => return Err(UpdateError::InvalidState("not in probation state")),
        };

        if !acknowledged {
            drop(self.trigger_rollback(RollbackReason::HealthTimeout, now));
            return Err(UpdateError::InvalidState("health not acknowledged"));
        }

        let old_active = slot.other();
        if manifest.last_good.as_ref().is_none_or(|lg| lg.slot != old_active) {
            manifest.last_good = Some(LastGoodRecord::new(old_active, tree, promoted_at, now));
        }
        manifest.rollback_count = 0;
        manifest.state = UpdateState::Healthy;
        manifest.updated_at = now;
        self.save_manifest(&manifest)?;
        Ok(())
    }

    pub fn evaluate_rollback(manifest: &UpdateManifest, _reason: &RollbackReason) -> RollbackAction {
        if manifest.rollback_count > 0 {
            RollbackAction::CannotRollbackLoopPrevented
        } else if let Some(ref lg) = manifest.last_good {
            RollbackAction::RollbackToLastGood { target_slot: lg.slot, quarantine_slot: manifest.active_slot }
        } else {
            RollbackAction::CannotRollbackNoLastGood
        }
    }

    pub fn trigger_rollback(&self, reason: RollbackReason, now: u64) -> Result<RollbackAction, UpdateError> {
        let mut manifest = self.manifest()?.ok_or(UpdateError::InvalidState("no manifest initialized"))?;
        let action = Self::evaluate_rollback(&manifest, &reason);
        match action {
            RollbackAction::RollbackToLastGood { target_slot, quarantine_slot } => {
                manifest.active_slot = target_slot;
                manifest.rollback_count += 1;
                manifest.state = UpdateState::RolledBack { restored_slot: target_slot, quarantined_slot: quarantine_slot, reason, at: now };
                manifest.updated_at = now;
                self.save_manifest(&manifest)?;
            }
            RollbackAction::CannotRollbackLoopPrevented => {
                manifest.state = UpdateState::Quarantined { slot: manifest.active_slot, reason: format!("{reason:?}"), at: now };
                manifest.updated_at = now;
                self.save_manifest(&manifest)?;
            }
            RollbackAction::CannotRollbackNoLastGood => {
                manifest.state =
                    UpdateState::Quarantined { slot: manifest.active_slot, reason: "no last good build available".into(), at: now };
                manifest.updated_at = now;
                self.save_manifest(&manifest)?;
            }
        }
        Ok(action)
    }
}

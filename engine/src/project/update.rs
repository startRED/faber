//! Local update slots and rollback state machine (R93, R96 Task 11).
//!
//! Controls two development slots (A and B), exact-tree verification,
//! last-good build retention, canary verdict gating, quiescence checks,
//! 30-second health acknowledgment and 5-minute probation, atomic manifest
//! persistence, and automatic rollback with anti-restart loop protection.

use super::files::ScopedFile;
use super::git::valid_oid;
use super::lease::{LeaseError, hash, real_path};
use super::{Effect, Extra, Grant, RepoIdentity, SCHEMA};
use crate::store::{Disk, Fault, replace};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAX_MANIFEST: u64 = 1024 * 1024;
const MAX_REJECTIONS: usize = 128;

/// Whether `path` resolves to a forbidden destination such as installed release `E:\Apps\Faber`
/// or any root/alias of it.
pub fn is_forbidden_root(path: &Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/").to_lowercase();
    let trimmed = s.trim_end_matches('/');
    if trimmed.is_empty() {
        return true;
    }
    let parts: Vec<_> = trimmed.split('/').filter(|part| !part.is_empty()).collect();
    if parts.windows(2).any(|pair| pair[0] == "apps" && pair[1].trim_end_matches([' ', '.']) == "faber") {
        return true;
    }
    if parts.last().is_some_and(|part| part.ends_with(':')) {
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
    Cleanup { cleaned: Vec<PathBuf>, error: Box<UpdateError> },
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
            Self::Cleanup { error, .. } => error.key(),
        }
    }
}

impl From<Fault> for UpdateError {
    fn from(fault: Fault) -> Self {
        Self::Store(fault)
    }
}

impl From<LeaseError> for UpdateError {
    fn from(error: LeaseError) -> Self {
        match error {
            LeaseError::Store(fault) => Self::Store(fault),
            LeaseError::Hash => Self::InvalidState("claimed obsolete artifact changed"),
            _ => Self::ForbiddenPath("invalid development path".into()),
        }
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
        if commit_oid.len() != tree_oid.len() {
            return Err(UpdateError::InvalidTree("mixed object hash formats".into()));
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
        hash(bytes)
    }

    pub fn matches_payload(&self, bytes: &[u8]) -> bool {
        Self::compute_build_hash(bytes) == self.build_hash
    }

    fn validate(&self) -> Result<(), UpdateError> {
        let normalized = Self::new(&self.commit_oid, &self.tree_oid, &self.build_hash, self.schema_version)?;
        if self.schema_version > SCHEMA {
            return Err(UpdateError::Newer);
        }
        if self.commit_oid != normalized.commit_oid || self.tree_oid != normalized.tree_oid || self.build_hash != normalized.build_hash {
            return Err(UpdateError::InvalidTree("noncanonical hash spelling".into()));
        }
        Ok(())
    }
}

/// Structured outcome of a miniature canary build run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CanaryVerdict {
    Pass { verified_at: u64, receipt_id: String, tree: ExactTree, proof_receipt: String, compatible_schemas: Vec<u32> },
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_good: Option<LastGoodRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected_trees: Vec<ExactTree>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected_build_hashes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_repo: Option<String>,
    #[serde(default)]
    pub first_operation_completed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_evidence: Option<CanaryVerdict>,
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
            current_good: None,
            rejected_trees: vec![],
            rejected_build_hashes: vec![],
            candidate_repo: None,
            first_operation_completed: false,
            candidate_evidence: None,
            rollback_count: 0,
            updated_at: at,
            extra: Extra::new(),
        }
    }

    fn validate(&self) -> Result<(), UpdateError> {
        if self.schema > SCHEMA || [&self.last_good, &self.current_good].into_iter().flatten().any(|good| good.schema > SCHEMA) {
            return Err(UpdateError::Newer);
        }
        if self.schema == 0 || self.channel != "development" {
            return Err(UpdateError::InvalidState("invalid development manifest"));
        }
        for good in [&self.last_good, &self.current_good].into_iter().flatten() {
            good.tree.validate()?;
            if good.schema == 0 || good.healthy_at < good.promoted_at {
                return Err(UpdateError::InvalidState("invalid healthy record"));
            }
        }
        for tree in self.staged_tree.iter().chain(self.rejected_trees.iter()) {
            tree.validate()?;
        }
        if self
            .rejected_build_hashes
            .iter()
            .any(|hash| hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
        {
            return Err(UpdateError::InvalidTree("invalid rejected build hash".into()));
        }
        let pending = match &self.state {
            UpdateState::PreparingNext { slot, tree }
            | UpdateState::CanaryPending { slot, tree }
            | UpdateState::Ready { slot, tree, .. } => {
                if *slot == self.active_slot {
                    return Err(UpdateError::InvalidState("candidate occupies active slot"));
                }
                Some(tree)
            }
            UpdateState::Swapping { from_slot, to_slot, tree } => {
                if *from_slot != self.active_slot || to_slot == from_slot {
                    return Err(UpdateError::InvalidState("invalid swap slots"));
                }
                Some(tree)
            }
            UpdateState::Probation { slot, tree, started_at, health_ack_deadline, probation_deadline, .. } => {
                if *slot != self.active_slot
                    || started_at.checked_add(30) != Some(*health_ack_deadline)
                    || started_at.checked_add(300) != Some(*probation_deadline)
                {
                    return Err(UpdateError::InvalidState("invalid probation"));
                }
                Some(tree)
            }
            _ => None,
        };
        if let Some(tree) = pending {
            tree.validate()?;
            if self.staged_tree.as_ref() != Some(tree) {
                return Err(UpdateError::InvalidState("staged tree mismatch"));
            }
        }
        if let Some(evidence) = &self.candidate_evidence {
            match evidence {
                CanaryVerdict::Pass { tree, receipt_id, proof_receipt, compatible_schemas, verified_at } => {
                    tree.validate()?;
                    if self.staged_tree.as_ref() != Some(tree)
                        || receipt_id.trim().is_empty()
                        || proof_receipt.trim().is_empty()
                        || *verified_at > self.updated_at
                        || !compatible_schemas.contains(&tree.schema_version)
                        || self.last_good.as_ref().is_some_and(|good| !compatible_schemas.contains(&good.tree.schema_version))
                    {
                        return Err(UpdateError::InvalidState("invalid candidate evidence"));
                    }
                }
                _ => return Err(UpdateError::InvalidState("candidate evidence did not pass")),
            }
        }
        if matches!(
            self.state,
            UpdateState::Ready { .. } | UpdateState::Quiescent | UpdateState::Swapping { .. } | UpdateState::Probation { .. }
        ) && self.candidate_evidence.is_none()
        {
            return Err(UpdateError::InvalidState("missing candidate evidence"));
        }
        match &self.state {
            UpdateState::Healthy
                if self.current_good.as_ref().is_none_or(|good| good.slot != self.active_slot)
                    || self.last_good.as_ref().is_none_or(|good| good.slot == self.active_slot) =>
            {
                return Err(UpdateError::InvalidState("invalid healthy slots"));
            }
            UpdateState::RolledBack { restored_slot, quarantined_slot, .. }
                if *restored_slot != self.active_slot
                    || restored_slot == quarantined_slot
                    || self.rollback_count == 0
                    || self.last_good.as_ref().is_none_or(|good| good.slot != *restored_slot) =>
            {
                return Err(UpdateError::InvalidState("invalid rollback slots"));
            }
            UpdateState::Ready { canary_receipt, .. }
                if !matches!(&self.candidate_evidence,
                Some(CanaryVerdict::Pass { receipt_id, .. }) if receipt_id == canary_receipt) =>
            {
                return Err(UpdateError::InvalidState("canary receipt mismatch"));
            }
            _ => {}
        }
        Ok(())
    }
}

/// Layout of slot directories under the development root.
#[derive(Debug, Clone)]
pub struct SlotLayout {
    dev_root: PathBuf,
}

pub struct Coordinator {
    _file: File,
    #[cfg(unix)]
    _directory: File,
}

impl SlotLayout {
    pub fn new(dev_root: &Path, installed_data: &Path, installed_release: &Path) -> Result<Self, UpdateError> {
        if is_forbidden_root(dev_root) || is_forbidden_root(installed_data) {
            return Err(UpdateError::ForbiddenPath(dev_root.to_string_lossy().into()));
        }
        let dev_root = real_path(dev_root)?;
        let installed_data = real_path(installed_data)?;
        // A literal protected release is an exclusion, never a path to inspect (including E:\Apps\Faber).
        let installed_release =
            if is_forbidden_root(installed_release) { installed_release.to_path_buf() } else { real_path(installed_release)? };
        for path in [&dev_root, &installed_data, &installed_release] {
            if path == &installed_release && is_forbidden_root(path) {
                continue;
            }
            if is_forbidden_root(path) {
                return Err(UpdateError::ForbiddenPath(path.to_string_lossy().into()));
            }
            let mut ancestor = path.as_path();
            while !ancestor.exists() {
                ancestor = ancestor.parent().ok_or(UpdateError::ForbiddenPath("missing parent".into()))?;
            }
            let canonical = ancestor.canonicalize().map_err(|_| Fault::Read)?;
            #[cfg(windows)]
            let alias = canonical.to_string_lossy().trim_start_matches(r"\\?\").to_lowercase()
                != ancestor.to_string_lossy().trim_start_matches(r"\\?\").to_lowercase();
            #[cfg(not(windows))]
            let alias = canonical != ancestor;
            if alias || is_forbidden_root(&canonical) {
                return Err(UpdateError::ForbiddenPath(path.to_string_lossy().into()));
            }
        }
        for installed in [&installed_data, &installed_release] {
            // Conservatively exclude case aliases on macOS even when this particular volume is case-sensitive.
            #[cfg(any(windows, target_os = "macos"))]
            let (development, installed) = (
                PathBuf::from(dev_root.to_string_lossy().trim_start_matches(r"\\?\").to_lowercase()),
                PathBuf::from(installed.to_string_lossy().trim_start_matches(r"\\?\").to_lowercase()),
            );
            #[cfg(not(any(windows, target_os = "macos")))]
            let development = &dev_root;
            if development.starts_with(&installed) || installed.starts_with(&development) {
                return Err(UpdateError::ForbiddenPath(dev_root.to_string_lossy().into()));
            }
        }
        Ok(SlotLayout { dev_root })
    }

    pub fn dev_root(&self) -> &Path {
        &self.dev_root
    }

    pub fn coordinate(&self) -> Result<Coordinator, UpdateError> {
        let path = real_path(&self.dev_root.join("update.lock"))?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(3).custom_flags(0x0020_0000);
        }
        let file = options.open(path).map_err(|_| Fault::Write)?;
        file.try_lock().map_err(|_| UpdateError::ActiveOperations)?;
        #[cfg(unix)]
        let directory = {
            let directory = File::open(self.dev_root()).map_err(|_| Fault::Read)?;
            directory.try_lock().map_err(|_| UpdateError::ActiveOperations)?;
            directory
        };
        Ok(Coordinator {
            _file: file,
            #[cfg(unix)]
            _directory: directory,
        })
    }

    pub fn missions_quiescent(&self) -> Result<(), UpdateError> {
        let data = real_path(&self.dev_root.join("data"))?;
        let ledger = crate::store::Store::new(&data).ledger_checked()?;
        let budget = super::budget::Budget::load(&ledger).map_err(|_| Fault::Corrupt)?;
        if budget.invocations.values().any(|r| !r.terminated || !r.settled) {
            return Err(UpdateError::ActiveOperations);
        }
        let root = real_path(&data.join("missions"))?;
        if !root.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(root).map_err(|_| Fault::Read)? {
            let path = real_path(&entry.map_err(|_| Fault::Read)?.path())?;
            if !path.is_dir() {
                return Err(Fault::Corrupt.into());
            }
            let read = |name: &str| ScopedFile::new(&path.join(name), false)?.read_limited(16 * 1024 * 1024);
            if let Some((bytes, _)) = read("run.json")? {
                let runner: super::run::Runner = serde_json::from_slice(&bytes).map_err(|_| Fault::Corrupt)?;
                if super::Record::schema(&runner) != SCHEMA || path.file_name().and_then(|s| s.to_str()) != Some(&runner.mission.id) {
                    return Err(Fault::Corrupt.into());
                }
                runner.update_quiescent().map_err(|_| UpdateError::ActiveOperations)?;
                if !super::store::Journal::pending_private(&path).map_err(|_| Fault::Corrupt)?.is_empty() {
                    return Err(UpdateError::ActiveOperations);
                }
            }
            if let Some((bytes, _)) = read("runtime.json")? {
                let runtime: super::Runtime = serde_json::from_slice(&bytes).map_err(|_| Fault::Corrupt)?;
                if runtime.schema != SCHEMA || runtime.leases.iter().any(|lease| lease.schema != SCHEMA) {
                    return Err(Fault::Corrupt.into());
                }
                if runtime.leases.iter().any(|lease| !super::lease::retirement_known(lease)) {
                    return Err(UpdateError::ActiveOperations);
                }
            }
        }
        Ok(())
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
            real_path(&dir)?;
            ScopedFile::new(&dir.join(".anchor"), true)?;
        }
        Ok(())
    }

    pub fn is_owned_path(&self, path: &Path) -> bool {
        if real_path(path).is_err() {
            return false;
        }
        [self.slot_dir(Slot::A), self.slot_dir(Slot::B), self.staging_dir(), self.quarantine_dir()].iter().any(|dir| path.starts_with(dir))
            || path == self.manifest_path()
            || path == self.dev_root.join("manifest.json.tmp")
            || self.is_manifest_temp(path)
    }

    fn is_manifest_temp(&self, path: &Path) -> bool {
        path.parent() == Some(self.dev_root())
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_prefix(".faber-patch-"))
                .and_then(|name| name.strip_suffix(".tmp"))
                .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
    }

    pub fn clean_stale_builds(&self, obsolete: &[(PathBuf, String)]) -> Result<Vec<PathBuf>, UpdateError> {
        let mut cleaned = Vec::new();
        let staging = self.staging_dir();
        real_path(&staging)?;
        // Slots and unlisted artifacts may still be in use; only the caller's obsolete inventory authorizes cleanup.
        for (path, expected_hash) in obsolete {
            let result = (|| -> Result<bool, UpdateError> {
                if !self.is_owned_path(path) || (!path.starts_with(&staging) && !self.is_manifest_temp(path)) || path == &staging {
                    return Err(UpdateError::ForbiddenPath(path.to_string_lossy().into()));
                }
                let file = ScopedFile::new(path, false)?;
                if let Some(found_hash) = file.hash()? {
                    if found_hash != *expected_hash {
                        return Err(UpdateError::InvalidState("obsolete artifact changed"));
                    }
                    let claimed = file.claim(expected_hash)?;
                    claimed.delete()?;
                    return Ok(true);
                }
                Ok(false)
            })();
            match result {
                Ok(true) => cleaned.push(path.clone()),
                Ok(false) => {}
                Err(error) if cleaned.is_empty() => return Err(error),
                Err(error) => return Err(UpdateError::Cleanup { cleaned, error: Box::new(error) }),
            }
        }
        Ok(cleaned)
    }
}

/// Fresh observations supplied by the settled-process adapter immediately before the swap.
pub struct SwapCheck<'a> {
    pub payload: &'a [u8],
    pub active_window: bool,
    pub active_operations: bool,
}

/// Manages slots, transitions, manifests, and rollback decisions.
/// Manifest writes cannot bypass the public transition API.
/// ```compile_fail
/// use faber_engine::project::update::{UpdateManager, UpdateManifest};
/// fn bypass(manager: &UpdateManager, fabricated: &UpdateManifest) {
///     manager.save_manifest(fabricated).unwrap();
/// }
/// ```
pub struct UpdateManager {
    layout: SlotLayout,
    disk: Option<Arc<dyn Disk>>,
}

impl UpdateManager {
    pub fn new(layout: SlotLayout) -> Result<Self, UpdateError> {
        layout.init_dirs()?;
        Ok(UpdateManager { layout, disk: None })
    }

    /// Fault injection for tests; production uses directory-anchored atomic IO in `new`.
    pub fn with_disk(layout: SlotLayout, disk: Arc<dyn Disk>) -> Result<Self, UpdateError> {
        layout.init_dirs()?;
        Ok(UpdateManager { layout, disk: Some(disk) })
    }

    pub fn layout(&self) -> &SlotLayout {
        &self.layout
    }

    pub fn manifest(&self) -> Result<Option<UpdateManifest>, UpdateError> {
        let path = real_path(&self.layout.manifest_path())?;
        let Some((bytes, _)) = ScopedFile::new(&path, false)?.read_limited(MAX_MANIFEST)? else { return Ok(None) };
        let manifest: UpdateManifest = serde_json::from_slice(&bytes).map_err(|_| Fault::Corrupt)?;
        manifest.validate()?;
        Ok(Some(manifest))
    }

    fn save_manifest(&self, manifest: &UpdateManifest) -> Result<(), UpdateError> {
        manifest.validate()?;
        real_path(&self.layout.manifest_path())?;
        real_path(&self.layout.dev_root.join("manifest.json.tmp"))?;
        let json = serde_json::to_string_pretty(manifest).expect("plain data serializes");
        if json.len() as u64 > MAX_MANIFEST {
            return Err(UpdateError::InvalidState("manifest too large"));
        }
        match &self.disk {
            Some(disk) => replace(&**disk, &self.layout.manifest_path(), json.as_bytes())?,
            None => ScopedFile::new(&self.layout.manifest_path(), false)?.write(json.as_bytes(), None)?,
        }
        Ok(())
    }

    fn checkpoint(&self, manifest: &UpdateManifest) -> Result<(), UpdateError> {
        manifest.validate()?;
        let bytes = serde_json::to_vec(manifest).expect("plain data serializes");
        if bytes.len() as u64 > MAX_MANIFEST {
            return Err(UpdateError::InvalidState("checkpoint too large"));
        }
        ScopedFile::new(&self.layout.dev_root().join("rollback.json"), false)?.write(&bytes, None)?;
        Ok(())
    }

    /// Register the running, healthy development build once; the adapter supplies its verified payload.
    pub fn adopt_baseline(&self, slot: Slot, tree: ExactTree, payload: &[u8], now: u64) -> Result<(), UpdateError> {
        tree.validate()?;
        if !tree.matches_payload(payload) || self.manifest()?.is_some() {
            return Err(UpdateError::InvalidState("unverified or already registered baseline"));
        }
        let mut manifest = UpdateManifest::initial(slot, now);
        let good = LastGoodRecord::new(slot, tree, now, now);
        manifest.last_good = Some(good.clone());
        manifest.current_good = Some(good);
        self.checkpoint(&manifest)?;
        self.save_manifest(&manifest)
    }

    pub fn prepare_next(
        &self,
        tree: ExactTree,
        grant: &Grant,
        repo: &RepoIdentity,
        revision: u64,
        running_slots: &[Slot],
        now: u64,
    ) -> Result<Slot, UpdateError> {
        grant.allows(repo, revision, Effect::SelfUpdate).map_err(UpdateError::Grant)?;
        tree.validate()?;

        let mut manifest = self.manifest()?.unwrap_or_else(|| UpdateManifest::initial(Slot::A, now));
        let next_slot = manifest.active_slot.other();
        if running_slots.contains(&next_slot) {
            return Err(UpdateError::ActiveOperations);
        }

        match &manifest.state {
            UpdateState::Idle | UpdateState::RolledBack { .. } | UpdateState::Quarantined { .. } | UpdateState::Healthy => {}
            _ => return Err(UpdateError::InvalidState("must be idle, rolled back or quarantined to prepare next")),
        }

        if now < manifest.updated_at
            || manifest.rejected_trees.iter().any(|bad| bad.build_hash == tree.build_hash)
            || manifest.rejected_build_hashes.contains(&tree.build_hash)
        {
            return Err(UpdateError::InvalidState("stale time or quarantined candidate"));
        }
        if manifest.rejected_trees.len() + manifest.rejected_build_hashes.len() >= MAX_REJECTIONS {
            return Err(UpdateError::InvalidState("rejected build history full"));
        }
        if let Some(good) = &manifest.current_good {
            if good.slot != manifest.active_slot {
                return Err(UpdateError::InvalidState("current healthy slot mismatch"));
            }
            manifest.last_good = Some(good.clone());
        }
        if manifest.last_good.as_ref().is_some_and(|good| good.slot != manifest.active_slot) {
            return Err(UpdateError::NoLastGood);
        }

        // Commit the current healthy tuple before its older fallback slot may be reused.
        let mut safe = manifest.clone();
        safe.state = UpdateState::Idle;
        safe.staged_tree = None;
        safe.candidate_repo = None;
        safe.candidate_evidence = None;
        safe.first_operation_completed = false;
        safe.updated_at = now;
        self.checkpoint(&safe)?;

        manifest.staged_tree = Some(tree.clone());
        manifest.candidate_repo = Some(repo.common_dir.clone());
        manifest.candidate_evidence = None;
        manifest.first_operation_completed = false;
        // A distinct accepted candidate starts a new transition; rejected identities remain durable.
        manifest.rollback_count = 0;
        manifest.state = UpdateState::PreparingNext { slot: next_slot, tree };
        manifest.updated_at = now;
        self.save_manifest(&manifest)?;
        Ok(next_slot)
    }

    pub fn record_canary_verdict(&self, verdict: CanaryVerdict, payload: &[u8], now: u64) -> Result<(), UpdateError> {
        let mut manifest = self.manifest()?.ok_or(UpdateError::InvalidState("no manifest initialized"))?;
        let (slot, tree) = match &manifest.state {
            UpdateState::PreparingNext { slot, tree } | UpdateState::CanaryPending { slot, tree } => (*slot, tree.clone()),
            _ => return Err(UpdateError::InvalidState("not in canary state")),
        };

        let evidence = verdict.clone();
        match verdict {
            CanaryVerdict::Pass { verified_at, receipt_id, tree: verified, proof_receipt, compatible_schemas } => {
                let previous_schema = manifest.last_good.as_ref().map_or(SCHEMA, |good| good.tree.schema_version);
                if verified != tree
                    || !tree.matches_payload(payload)
                    || receipt_id.trim().is_empty()
                    || proof_receipt.trim().is_empty()
                    || verified_at < manifest.updated_at
                    || verified_at > now
                    || !compatible_schemas.contains(&previous_schema)
                    || !compatible_schemas.contains(&tree.schema_version)
                {
                    return Err(UpdateError::InvalidState("unverified tree, proof, canary or rollback schema"));
                }
                manifest.state = UpdateState::Ready { slot, tree, canary_receipt: receipt_id };
                manifest.candidate_evidence = Some(evidence);
            }
            CanaryVerdict::Fail { reason, at } => {
                manifest.rejected_build_hashes.push(tree.build_hash);
                manifest.staged_tree = None;
                manifest.candidate_repo = None;
                manifest.candidate_evidence = None;
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

    /// The trusted adapter has accepted a newer tree and holds the inactive slot's process lock.
    /// A live swap/probation is immutable; only an unpromoted candidate can be coalesced.
    pub fn supersede_next(&self, grant: &Grant, repo: &RepoIdentity, revision: u64, now: u64) -> Result<(), UpdateError> {
        grant.allows(repo, revision, Effect::SelfUpdate).map_err(UpdateError::Grant)?;
        let mut manifest = self.manifest()?.ok_or(UpdateError::InvalidState("no manifest initialized"))?;
        if !matches!(manifest.state, UpdateState::PreparingNext { .. } | UpdateState::CanaryPending { .. } | UpdateState::Ready { .. })
            || now < manifest.updated_at
            || manifest.candidate_repo.as_ref() != Some(&repo.common_dir)
        {
            return Err(UpdateError::InvalidState("candidate cannot be superseded"));
        }
        manifest.state = UpdateState::Idle;
        manifest.staged_tree = None;
        manifest.candidate_repo = None;
        manifest.candidate_evidence = None;
        manifest.first_operation_completed = false;
        manifest.updated_at = now;
        self.save_manifest(&manifest)
    }

    pub fn wait_for_canary(&self, now: u64) -> Result<(), UpdateError> {
        let mut manifest = self.manifest()?.ok_or(UpdateError::NoLastGood)?;
        let (slot, tree) = match &manifest.state {
            UpdateState::PreparingNext { slot, tree } | UpdateState::CanaryPending { slot, tree } => (*slot, tree.clone()),
            _ => return Err(UpdateError::InvalidState("not waiting for canary")),
        };
        if now < manifest.updated_at {
            return Err(UpdateError::InvalidState("stale canary time"));
        }
        manifest.state = UpdateState::CanaryPending { slot, tree };
        manifest.updated_at = now;
        self.save_manifest(&manifest)
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

    pub fn begin_swap(
        &self,
        grant: &Grant,
        repo: &RepoIdentity,
        revision: u64,
        check: SwapCheck<'_>,
        now: u64,
    ) -> Result<Slot, UpdateError> {
        grant.allows(repo, revision, Effect::SelfUpdate).map_err(UpdateError::Grant)?;
        if check.active_window {
            return Err(UpdateError::ActiveWindow);
        }
        if check.active_operations {
            return Err(UpdateError::ActiveOperations);
        }
        let mut manifest = self.manifest()?.ok_or(UpdateError::InvalidState("no manifest initialized"))?;
        let (target_slot, tree) = match &manifest.state {
            UpdateState::Quiescent => {
                let slot = manifest.active_slot.other();
                let tree = manifest.staged_tree.clone().ok_or(UpdateError::InvalidState("no staged tree"))?;
                (slot, tree)
            }
            _ => return Err(UpdateError::InvalidState("must be quiescent to swap")),
        };

        if now < manifest.updated_at {
            return Err(UpdateError::InvalidState("stale transition time"));
        }
        if manifest.candidate_repo.as_ref() != Some(&repo.common_dir) {
            return Err(UpdateError::Grant("reason.grant.otherRepo"));
        }
        if !tree.matches_payload(check.payload) {
            return Err(UpdateError::InvalidState("candidate payload changed before swap"));
        }
        if manifest.last_good.as_ref().is_none_or(|good| good.slot != manifest.active_slot) {
            return Err(UpdateError::NoLastGood);
        }
        if !matches!(&manifest.candidate_evidence, Some(CanaryVerdict::Pass { tree: verified, .. }) if *verified == tree) {
            return Err(UpdateError::InvalidState("no verified candidate evidence"));
        }

        let from_slot = manifest.active_slot;
        manifest.state = UpdateState::Swapping { from_slot, to_slot: target_slot, tree };
        manifest.updated_at = now;
        // Reserve space for the worst bounded rollback before a candidate can become active.
        let mut rollback = manifest.clone();
        Self::restore(&mut rollback, RollbackReason::Custom("\0".repeat(1024)), u64::MAX);
        if serde_json::to_vec_pretty(&rollback).expect("plain data serializes").len() as u64 > MAX_MANIFEST {
            return Err(UpdateError::InvalidState("insufficient rollback manifest headroom"));
        }
        // The trusted helper can restore the exact last-good tuple even when next cannot decode the live manifest.
        self.checkpoint(&manifest)?;
        self.save_manifest(&manifest)?;
        Ok(target_slot)
    }

    pub fn begin_probation(&self, now: u64) -> Result<(), UpdateError> {
        let mut manifest = self.manifest()?.ok_or(UpdateError::InvalidState("no manifest initialized"))?;
        let (to_slot, tree) = match &manifest.state {
            UpdateState::Swapping { to_slot, tree, .. } => (*to_slot, tree.clone()),
            _ => return Err(UpdateError::InvalidState("must be in swapping state")),
        };
        if now < manifest.updated_at || now.checked_add(300).is_none() {
            return Err(UpdateError::InvalidState("invalid probation time"));
        }
        manifest.active_slot = to_slot;
        manifest.extra.remove("cleanExit");
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
                if now < manifest.updated_at {
                    return Err(UpdateError::InvalidState("stale health time"));
                }
                if *health_acknowledged {
                    return Ok(());
                }
                if now > *health_ack_deadline {
                    self.trigger_rollback(RollbackReason::HealthTimeout, now)?;
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
        let (slot, tree, acknowledged, promoted_at, deadline, completed) = match &manifest.state {
            UpdateState::Probation { slot, tree, health_acknowledged, started_at, probation_deadline, .. } => {
                (*slot, tree.clone(), *health_acknowledged, *started_at, *probation_deadline, manifest.first_operation_completed)
            }
            _ => return Err(UpdateError::InvalidState("not in probation state")),
        };

        if now < manifest.updated_at {
            self.trigger_rollback(RollbackReason::StartupCorrupt, now)?;
            return Err(UpdateError::InvalidState("probation clock moved backwards"));
        }

        if now < deadline {
            return Err(UpdateError::InvalidState("probation not complete"));
        }

        if !acknowledged {
            self.trigger_rollback(RollbackReason::HealthTimeout, now)?;
            return Err(UpdateError::InvalidState("health not acknowledged"));
        }
        if !completed {
            self.trigger_rollback(RollbackReason::FirstOperationFailed, now)?;
            return Err(UpdateError::InvalidState("first operation not complete"));
        }
        manifest.current_good = Some(LastGoodRecord::new(slot, tree, promoted_at, now));
        if let Some(evidence) = &manifest.candidate_evidence {
            manifest
                .current_good
                .as_mut()
                .unwrap()
                .extra
                .insert("canaryEvidence".into(), serde_json::to_value(evidence).expect("plain data serializes"));
        }
        manifest.staged_tree = None;
        manifest.candidate_repo = None;
        manifest.candidate_evidence = None;
        manifest.rollback_count = 0;
        manifest.state = UpdateState::Healthy;
        manifest.updated_at = now;
        // This is the promotion commit point; a crash before the live write must never replay the older rollback.
        self.checkpoint(&manifest)?;
        self.save_manifest(&manifest)?;
        Ok(())
    }

    pub fn evaluate_rollback(manifest: &UpdateManifest, _reason: &RollbackReason) -> RollbackAction {
        if manifest.rollback_count > 0 {
            RollbackAction::CannotRollbackLoopPrevented
        } else if let Some(ref lg) = manifest.last_good {
            let quarantine_slot = match manifest.state {
                UpdateState::Swapping { to_slot, .. } => to_slot,
                UpdateState::Probation { slot, .. } => slot,
                _ => return RollbackAction::CannotRollbackNoLastGood,
            };
            if lg.slot == quarantine_slot {
                return RollbackAction::CannotRollbackNoLastGood;
            }
            RollbackAction::RollbackToLastGood { target_slot: lg.slot, quarantine_slot }
        } else {
            RollbackAction::CannotRollbackNoLastGood
        }
    }

    /// A trusted orderly exit/session end retries the full probation without credit for elapsed time.
    pub fn pause_probation(&self, now: u64) -> Result<(), UpdateError> {
        let mut manifest = self.manifest()?.ok_or(UpdateError::NoLastGood)?;
        let (slot, tree) = match &manifest.state {
            UpdateState::Probation { slot, tree, .. } => (*slot, tree.clone()),
            UpdateState::Swapping { to_slot, tree, .. } => (*to_slot, tree.clone()),
            _ => return Err(UpdateError::InvalidState("clean exit outside transition")),
        };
        let from_slot = manifest.last_good.as_ref().ok_or(UpdateError::NoLastGood)?.slot;
        manifest.state = UpdateState::Swapping { from_slot, to_slot: slot, tree };
        manifest.active_slot = from_slot;
        manifest.first_operation_completed = false;
        manifest.extra.insert("cleanExit".into(), true.into());
        manifest.updated_at = now.max(manifest.updated_at);
        self.checkpoint(&manifest)?;
        self.save_manifest(&manifest)
    }

    pub fn trigger_rollback(&self, reason: RollbackReason, now: u64) -> Result<RollbackAction, UpdateError> {
        let mut manifest = self.manifest()?.ok_or(UpdateError::InvalidState("no manifest initialized"))?;
        let action = Self::evaluate_rollback(&manifest, &reason);
        if action == RollbackAction::CannotRollbackLoopPrevented {
            return Ok(action);
        }
        if !matches!(manifest.state, UpdateState::Swapping { .. } | UpdateState::Probation { .. }) {
            return Err(UpdateError::InvalidState("no pending transition"));
        }
        Self::restore(&mut manifest, reason, now);
        self.save_manifest(&manifest)?;
        ScopedFile::new(&self.layout.dev_root().join("rollback.json"), false)?
            .write(&serde_json::to_vec(&manifest).expect("plain data serializes"), None)?;
        Ok(action)
    }

    fn restore(manifest: &mut UpdateManifest, mut reason: RollbackReason, now: u64) {
        let now = now.max(manifest.updated_at);
        if let RollbackReason::Custom(text) = &mut reason {
            // Bound diagnostics so an arbitrary message cannot consume the reserved safety write.
            let mut end = text.len().min(1024);
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
        }
        let action = Self::evaluate_rollback(manifest, &reason);
        let candidate = match &manifest.state {
            UpdateState::Swapping { tree, .. } | UpdateState::Probation { tree, .. } => tree.clone(),
            _ => unreachable!(),
        };
        if !manifest.rejected_build_hashes.contains(&candidate.build_hash) {
            manifest.rejected_build_hashes.push(candidate.build_hash);
        }
        manifest.staged_tree = None;
        manifest.candidate_repo = None;
        manifest.candidate_evidence = None;
        manifest.first_operation_completed = false;
        match action {
            RollbackAction::RollbackToLastGood { target_slot, quarantine_slot } => {
                manifest.active_slot = target_slot;
                manifest.current_good = manifest.last_good.clone();
                manifest.rollback_count = 1;
                manifest.state = UpdateState::RolledBack { restored_slot: target_slot, quarantined_slot: quarantine_slot, reason, at: now };
                manifest.updated_at = now;
            }
            RollbackAction::CannotRollbackLoopPrevented => unreachable!(),
            RollbackAction::CannotRollbackNoLastGood => {
                manifest.state =
                    UpdateState::Quarantined { slot: manifest.active_slot, reason: "no last good build available".into(), at: now };
                manifest.updated_at = now;
            }
        }
    }

    pub fn complete_first_operation(&self, now: u64) -> Result<(), UpdateError> {
        let mut manifest = self.manifest()?.ok_or(UpdateError::InvalidState("no manifest initialized"))?;
        match &manifest.state {
            UpdateState::Probation { health_acknowledged: true, probation_deadline, .. }
                if now >= manifest.updated_at && now <= *probation_deadline =>
            {
                manifest.first_operation_completed = true;
                manifest.updated_at = now;
                self.save_manifest(&manifest)
            }
            _ => Err(UpdateError::InvalidState("first operation outside acknowledged probation")),
        }
    }

    pub fn cancel_quiescence(&self) -> Result<(), UpdateError> {
        let mut manifest = self.manifest()?.ok_or(UpdateError::InvalidState("no manifest initialized"))?;
        if manifest.state != UpdateState::Quiescent {
            return Err(UpdateError::InvalidState("not quiescent"));
        }
        // Re-run the canary gate before another attempt to quiesce.
        let tree = manifest.staged_tree.clone().ok_or(UpdateError::InvalidState("no staged tree"))?;
        manifest.state = UpdateState::CanaryPending { slot: manifest.active_slot.other(), tree };
        self.save_manifest(&manifest)
    }

    pub fn recover_transition(&self, now: u64) -> Result<Option<RollbackAction>, UpdateError> {
        let manifest = match self.manifest() {
            Ok(Some(manifest)) => manifest,
            Ok(None) => return Ok(None),
            Err(error @ UpdateError::Store(Fault::Corrupt))
            | Err(error @ UpdateError::Newer)
            | Err(error @ UpdateError::InvalidState(_))
            | Err(error @ UpdateError::InvalidTree(_)) => {
                let Some((bytes, _)) = ScopedFile::new(&self.layout.dev_root().join("rollback.json"), false)?.read_limited(MAX_MANIFEST)?
                else {
                    return Err(error);
                };
                let mut checkpoint: UpdateManifest = serde_json::from_slice(&bytes).map_err(|_| Fault::Corrupt)?;
                checkpoint.validate()?;
                if (matches!(checkpoint.state, UpdateState::Healthy | UpdateState::Idle) && checkpoint.current_good.is_some())
                    || (matches!(checkpoint.state, UpdateState::Swapping { .. })
                        && checkpoint.extra.get("cleanExit") == Some(&serde_json::Value::Bool(true)))
                {
                    self.save_manifest(&checkpoint)?;
                    return Ok(None);
                }
                if !matches!(checkpoint.state, UpdateState::Swapping { .. }) {
                    return Err(UpdateError::InvalidState("invalid rollback checkpoint"));
                }
                let action = Self::evaluate_rollback(&checkpoint, &RollbackReason::StartupCorrupt);
                Self::restore(&mut checkpoint, RollbackReason::StartupCorrupt, now);
                self.save_manifest(&checkpoint)?;
                ScopedFile::new(&self.layout.dev_root().join("rollback.json"), false)?
                    .write(&serde_json::to_vec(&checkpoint).expect("plain data serializes"), None)?;
                return Ok(Some(action));
            }
            Err(error) => return Err(error),
        };
        if let UpdateState::Probation { tree, probation_deadline, .. } = &manifest.state {
            let checkpoint = ScopedFile::new(&self.layout.dev_root().join("rollback.json"), false)?.read_limited(MAX_MANIFEST)?;
            if let Some((bytes, _)) = checkpoint {
                let committed: UpdateManifest = serde_json::from_slice(&bytes).map_err(|_| Fault::Corrupt)?;
                committed.validate()?;
                let healthy = committed.state == UpdateState::Healthy
                    && committed.current_good.as_ref().is_some_and(|good| good.tree == *tree && good.healthy_at >= *probation_deadline);
                let paused = matches!(&committed.state, UpdateState::Swapping { tree: paused_tree, .. } if paused_tree == tree)
                    && committed.extra.get("cleanExit") == Some(&serde_json::Value::Bool(true))
                    && committed.updated_at >= manifest.updated_at;
                if healthy || paused {
                    self.save_manifest(&committed)?;
                    return Ok(None);
                }
            }
        }
        if manifest.extra.get("cleanExit") == Some(&serde_json::Value::Bool(true)) && matches!(manifest.state, UpdateState::Swapping { .. })
        {
            return Ok(None);
        }
        if matches!(manifest.state, UpdateState::Swapping { .. } | UpdateState::Probation { .. }) {
            self.trigger_rollback(RollbackReason::StartupCorrupt, now).map(Some)
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversize_private_write_preserves_previous_manifest() {
        let area = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let layout = SlotLayout::new(&area.path().join("dev"), &area.path().join("installed-data"), &area.path().join("release")).unwrap();
        let manager = UpdateManager::new(layout).unwrap();
        let mut manifest = UpdateManifest::initial(Slot::A, 1);
        manifest.extra.insert("extension".into(), serde_json::json!("x".repeat(16 * 1024)));
        manager.save_manifest(&manifest).unwrap();
        let before = std::fs::read(manager.layout.manifest_path()).unwrap();
        manifest.extra.insert("extension".into(), serde_json::json!("x".repeat(1024 * 1024)));
        assert!(manager.save_manifest(&manifest).is_err());
        assert_eq!(std::fs::read(manager.layout.manifest_path()).unwrap(), before);
    }
}

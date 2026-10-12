//! Durable, exclusive writer leases. Neither elapsed time nor dropping a coordinator releases a writer's authority.
use super::store::{Journal, JournalError};
use super::wire::FileOp;
use super::{Effect, Grant, Lease, LeaseState, ProcessId, RepoIdentity, Runtime, SCHEMA, clean_path, overlaps, valid_id};
use crate::store::{Fault, RealDisk, replace};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseError {
    Owned,
    Conflict,
    Path,
    Scope,
    Stale,
    Live,
    Hash,
    Quarantined,
    Authority(&'static str),
    Journal(JournalError),
    Store(Fault),
}
impl LeaseError {
    pub fn key(self) -> &'static str {
        match self {
            Self::Owned => "reason.lease.owned",
            Self::Conflict => "reason.lease.conflict",
            Self::Path => "error.wire.path",
            Self::Scope => "reason.lease.scope",
            Self::Stale => "error.wire.stale",
            Self::Live => "reason.lease.live",
            Self::Hash => "reason.lease.hash",
            Self::Quarantined => "reason.lease.quarantined",
            Self::Authority(key) => key,
            Self::Journal(e) => e.key(),
            Self::Store(e) => e.key(),
        }
    }
}
impl From<JournalError> for LeaseError {
    fn from(e: JournalError) -> Self {
        Self::Journal(e)
    }
}
impl From<Fault> for LeaseError {
    fn from(e: Fault) -> Self {
        Self::Store(e)
    }
}

/// Supplied by the engine's process containment and spending ledgers, never a worker or heartbeat.
pub struct Termination {
    pub process: Option<ProcessId>,
    pub tree_dead: bool,
    pub cost_settled: bool,
}

pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn file_hash(path: &Path) -> Result<String, LeaseError> {
    let mut file = File::open(path).map_err(|_| Fault::Read)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let read = file.read(&mut buffer).map_err(|_| Fault::Read)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn reparse(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

/// Reject links in every existing ancestor, including Windows junctions. New paths resolve through their real parent.
pub fn real_path(path: &Path) -> Result<PathBuf, LeaseError> {
    if !path.is_absolute() {
        return Err(LeaseError::Path);
    }
    let mut part = PathBuf::new();
    for component in path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(LeaseError::Path);
        }
        part.push(component);
        if matches!(component, std::path::Component::Prefix(_)) {
            continue;
        }
        if !part.has_root() {
            continue;
        }
        match fs::symlink_metadata(&part) {
            Ok(meta) if reparse(&meta) => return Err(LeaseError::Path),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(LeaseError::Store(Fault::Read)),
        }
    }
    Ok(path.to_path_buf())
}

pub(crate) fn normalized(raw: &str) -> Result<String, LeaseError> {
    let clean = clean_path(raw).ok_or(LeaseError::Path)?;
    for part in clean.split('/').filter(|p| !p.is_empty()) {
        let lower = part.to_lowercase();
        let stem = lower.split('.').next().unwrap_or_default();
        if lower == ".git"
            || lower == ".faber"
            || part.ends_with([' ', '.'])
            || part.chars().any(char::is_control)
            || part.contains(['~', '*', '?', '[', ']', '<', '>', '|', '"'])
            || matches!(stem, "con" | "prn" | "aux" | "nul" | "conin$" | "conout$")
            || stem
                .strip_prefix("com")
                .or_else(|| stem.strip_prefix("lpt"))
                .is_some_and(|suffix| matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"))
        {
            return Err(LeaseError::Path);
        }
    }
    Ok(clean)
}

fn scoped_path(root: &Path, raw: &str) -> Result<PathBuf, LeaseError> {
    let clean = normalized(if raw.is_empty() { "." } else { raw })?;
    let joined = if clean.is_empty() { real_path(root)? } else { real_path(&root.join(clean))? };
    let mut ancestor = joined.as_path();
    while !ancestor.exists() {
        ancestor = ancestor.parent().ok_or(LeaseError::Path)?;
    }
    let canonical = ancestor.canonicalize().map_err(|_| Fault::Read)?;
    let canonical_root = root.canonicalize().map_err(|_| Fault::Read)?;
    if !canonical.starts_with(&canonical_root) {
        return Err(LeaseError::Path);
    }
    // Canonical spelling rejects short-name aliases without rejecting ordinary case differences.
    if canonical.to_string_lossy().to_lowercase() != ancestor.to_string_lossy().to_lowercase() {
        return Err(LeaseError::Path);
    }
    let suffix = joined.strip_prefix(ancestor).map_err(|_| LeaseError::Path)?;
    Ok(if suffix.as_os_str().is_empty() { canonical } else { canonical.join(suffix) })
}

type Files = BTreeMap<String, String>;
fn snapshot(root: &Path) -> Result<Files, LeaseError> {
    fn visit(root: &Path, dir: &Path, out: &mut Files, names: &mut BTreeSet<String>) -> Result<(), LeaseError> {
        for entry in fs::read_dir(dir).map_err(|_| Fault::Read)? {
            let path = entry.map_err(|_| Fault::Read)?.path();
            let name = path.strip_prefix(root).map_err(|_| LeaseError::Path)?.to_string_lossy().replace('\\', "/");
            let meta = fs::symlink_metadata(&path).map_err(|_| Fault::Read)?;
            if reparse(&meta) {
                return Err(LeaseError::Path);
            }
            if name != ".git" && !name.starts_with(".faber/") && name != ".faber" {
                normalized(&name)?;
            }
            if name == ".git" && !meta.is_file() {
                return Err(LeaseError::Path);
            }
            if !names.insert(name.to_lowercase()) {
                return Err(LeaseError::Path);
            }
            if meta.is_dir() {
                visit(root, &path, out, names)?;
            } else if meta.is_file() {
                let mut fingerprint = file_hash(&path)?;
                fingerprint.push_str(&format!(":{}", meta.permissions().readonly()));
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fingerprint.push_str(&format!(":{:o}", meta.permissions().mode() & 0o777));
                }
                out.insert(name, fingerprint);
            } else {
                return Err(LeaseError::Path);
            }
        }
        Ok(())
    }
    let mut files = Files::new();
    visit(root, root, &mut files, &mut BTreeSet::new())?;
    Ok(files)
}

pub struct Leases {
    repo: RepoIdentity,
    mission: String,
    journal: Journal,
    runtime: Runtime,
    lock: File,
    blocked: bool,
}

impl Leases {
    pub fn open(repo: RepoIdentity, journal: Journal, mission: &str) -> Result<Self, LeaseError> {
        if !valid_id(mission) || repo.schema != SCHEMA {
            return Err(LeaseError::Stale);
        }
        let common = real_path(Path::new(&repo.common_dir))?.canonicalize().map_err(|_| Fault::Read)?;
        let root = real_path(Path::new(&repo.root))?.canonicalize().map_err(|_| Fault::Read)?;
        if common.to_string_lossy() != repo.common_dir || root.to_string_lossy() != repo.root {
            return Err(LeaseError::Path);
        }
        let private = real_path(journal.private())?.canonicalize().map_err(|_| Fault::Read)?;
        let records = real_path(journal.records())?.canonicalize().map_err(|_| Fault::Read)?;
        if private.starts_with(&root) || (records.starts_with(&root) && records != root.join(".faber/missions").join(mission)) {
            return Err(LeaseError::Path);
        }
        let lock_path = real_path(&common.join("faber-project.lock"))?;
        let mut lock =
            OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&lock_path).map_err(|e| Fault::of_write(&e))?;
        lock.try_lock().map_err(|_| LeaseError::Owned)?;
        let owner = format!("{mission}\n{}", journal.private().canonicalize().map_err(|_| Fault::Read)?.display());
        let mut saved = String::new();
        lock.read_to_string(&mut saved).map_err(|_| Fault::Read)?;
        if !saved.is_empty() && saved != owner {
            return Err(LeaseError::Owned);
        }
        if !saved.is_empty() && !journal.private().join("runtime.json").is_file() {
            return Err(Fault::Corrupt.into());
        }
        if saved.is_empty() {
            lock.write_all(owner.as_bytes()).and_then(|()| lock.sync_all()).map_err(|e| Fault::of_write(&e))?;
        }
        let runtime = journal.runtime()?;
        if runtime.schema != SCHEMA {
            return Err(Fault::Corrupt.into());
        }
        if saved.is_empty() {
            journal.save_runtime(&runtime)?;
        }
        let mut generations = BTreeSet::new();
        for lease in &runtime.leases {
            if lease.schema != SCHEMA
                || lease.repo != repo.common_dir
                || lease.mission != mission
                || lease.generation == 0
                || !valid_id(&lease.task)
                || !generations.insert(lease.generation)
                || lease.paths.is_empty()
            {
                return Err(Fault::Corrupt.into());
            }
            for path in &lease.paths {
                scoped_path(&root, path)?;
            }
        }
        if records.starts_with(&root) {
            let info = real_path(&common.join("info"))?;
            fs::create_dir_all(&info).map_err(|e| Fault::of_write(&e))?;
            let exclude = real_path(&info.join("exclude"))?;
            let contents = match fs::read(&exclude) {
                Ok(bytes) => bytes,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![],
                Err(_) => return Err(Fault::Read.into()),
            };
            if !contents.split(|b| *b == b'\n').any(|line| line.strip_suffix(b"\r").unwrap_or(line) == b"/.faber/") {
                // Append only: concurrent edits to the owner's existing patterns must not be overwritten.
                let addition =
                    if contents.is_empty() || contents.ends_with(b"\n") { b"/.faber/\n".as_slice() } else { b"\n/.faber/\n".as_slice() };
                let mut file = OpenOptions::new().create(true).append(true).open(exclude).map_err(|e| Fault::of_write(&e))?;
                file.write_all(addition).and_then(|()| file.sync_all()).map_err(|e| Fault::of_write(&e))?;
            }
        }
        Ok(Self { repo, mission: mission.into(), journal, runtime, lock, blocked: false })
    }

    pub fn journal(&self) -> &Journal {
        &self.journal
    }
    pub fn repo(&self) -> &RepoIdentity {
        &self.repo
    }
    pub(crate) fn owns_branch(&self, branch: &str) -> bool {
        !self.blocked && self.runtime.extra.get("gitBranch").and_then(|v| v.as_str()) == Some(branch)
    }
    pub(crate) fn record_branch(&mut self, branch: &str) -> Result<(), LeaseError> {
        let mut runtime = self.runtime.clone();
        runtime.extra.insert("gitBranch".into(), serde_json::json!(branch));
        self.persist(runtime)
    }

    fn persist(&mut self, runtime: Runtime) -> Result<(), LeaseError> {
        if self.blocked {
            return Err(LeaseError::Quarantined);
        }
        if let Err(e) = self.journal.save_runtime(&runtime) {
            self.blocked = true;
            return Err(e.into());
        }
        self.runtime = runtime;
        Ok(())
    }
    pub(crate) fn index(&self, lease: &Lease) -> Result<usize, LeaseError> {
        let index = self.matching(lease)?;
        if self.runtime.leases[index].state != LeaseState::Held {
            return Err(LeaseError::Stale);
        }
        Ok(index)
    }
    fn matching(&self, lease: &Lease) -> Result<usize, LeaseError> {
        if self.blocked {
            return Err(LeaseError::Quarantined);
        }
        self.runtime
            .leases
            .iter()
            .position(|l| {
                l.generation == lease.generation
                    && l.repo == lease.repo
                    && l.mission == lease.mission
                    && l.task == lease.task
                    && l.attempt == lease.attempt
                    && l.base == lease.base
                    && l.contract_digest == lease.contract_digest
                    && l.paths == lease.paths
            })
            .ok_or(LeaseError::Stale)
    }
    pub fn acquire(&mut self, mut lease: Lease) -> Result<Lease, LeaseError> {
        if self.blocked {
            return Err(LeaseError::Quarantined);
        }
        if lease.schema != SCHEMA
            || lease.repo != self.repo.common_dir
            || lease.mission != self.mission
            || !valid_id(&lease.task)
            || lease.attempt == 0
            || lease.paths.is_empty()
            || !super::git::valid_oid(&lease.base)
            || lease.contract_digest.len() != 64
            || !lease.contract_digest.bytes().all(|b| b.is_ascii_hexdigit())
            || lease.process.is_some()
            || lease.state != LeaseState::Held
            || !lease.extra.is_empty()
        {
            return Err(LeaseError::Stale);
        }
        let root = Path::new(&self.repo.root);
        for path in &mut lease.paths {
            *path = normalized(path)?;
            scoped_path(root, if path.is_empty() { "." } else { path })?;
        }
        if self.runtime.leases.iter().any(|held| {
            held.state != LeaseState::Released
                && (held.task == lease.task || held.paths.iter().any(|a| lease.paths.iter().any(|b| overlaps(a, b))))
        }) {
            return Err(LeaseError::Conflict);
        }
        lease.generation = self.runtime.leases.iter().map(|l| l.generation).max().unwrap_or(0).checked_add(1).ok_or(LeaseError::Stale)?;
        let mut runtime = self.runtime.clone();
        runtime.leases.push(lease.clone());
        self.persist(runtime)?;
        Ok(lease)
    }
    pub fn check(&self, lease: &Lease, path: &str) -> Result<(), LeaseError> {
        let index = self.index(lease)?;
        let clean = normalized(path)?;
        if clean.is_empty() {
            return Err(LeaseError::Path);
        }
        if !self.runtime.leases[index].paths.iter().any(|area| {
            area.is_empty() || clean.eq_ignore_ascii_case(area) || clean.to_lowercase().starts_with(&format!("{}/", area.to_lowercase()))
        }) {
            return Err(LeaseError::Scope);
        }
        Ok(())
    }
    pub fn attach_process(&mut self, lease: &Lease, process: ProcessId) -> Result<(), LeaseError> {
        let index = self.index(lease)?;
        if process.pid == 0 || process.started_at == 0 || self.runtime.leases[index].process.is_some() {
            return Err(LeaseError::Live);
        }
        let mut runtime = self.runtime.clone();
        runtime.leases[index].process = Some(process);
        self.persist(runtime)
    }
    pub fn release(&mut self, lease: &Lease, proof: Termination) -> Result<(), LeaseError> {
        let index = self.index(lease)?;
        if !proof.tree_dead || !proof.cost_settled || proof.process != self.runtime.leases[index].process {
            return Err(LeaseError::Live);
        }
        if self.runtime.leases[index].extra.contains_key("gitRoot") {
            self.audit(lease)?;
        }
        let mut runtime = self.runtime.clone();
        runtime.leases[index].state = LeaseState::Released;
        self.persist(runtime)
    }
    /// Explicit recovery of a failed attempt. Its files and quarantine remain; no source authority is restored.
    pub fn archive(&mut self, lease: &Lease, proof: Termination) -> Result<(), LeaseError> {
        let index = self.matching(lease)?;
        let held = &self.runtime.leases[index];
        if held.state != LeaseState::Quarantined {
            return Err(LeaseError::Stale);
        }
        if !proof.tree_dead || !proof.cost_settled || proof.process != held.process || !self.journal.pending()?.is_empty() {
            return Err(LeaseError::Live);
        }
        let mut runtime = self.runtime.clone();
        runtime.leases[index].extra.insert(
            "retirement".into(),
            serde_json::json!({
                "generation": lease.generation, "process": proof.process, "treeDead": true, "costSettled": true
            }),
        );
        self.persist(runtime)
    }
    pub fn bind(&mut self, lease: &Lease, worktree: &Path) -> Result<(), LeaseError> {
        let index = self.index(lease)?;
        let root = real_path(worktree)?.canonicalize().map_err(|_| Fault::Read)?;
        let private = real_path(self.journal.private())?.canonicalize().map_err(|_| Fault::Read)?;
        if !root.starts_with(&private)
            || root == private
            || root.starts_with(Path::new(&self.repo.root))
            || self.runtime.leases[index].extra.contains_key("gitRoot")
        {
            return Err(LeaseError::Path);
        }
        let files = snapshot(&root)?;
        let mut runtime = self.runtime.clone();
        let extra = &mut runtime.leases[index].extra;
        extra.insert("gitRoot".into(), serde_json::json!(root));
        extra.insert("initialFiles".into(), serde_json::json!(files));
        extra.insert("files".into(), serde_json::json!(files));
        self.persist(runtime)
    }
    pub fn worktree(&self, lease: &Lease) -> Result<PathBuf, LeaseError> {
        let index = self.index(lease)?;
        let root: PathBuf = serde_json::from_value(self.runtime.leases[index].extra.get("gitRoot").cloned().ok_or(LeaseError::Stale)?)
            .map_err(|_| Fault::Corrupt)?;
        let root = real_path(&root)?.canonicalize().map_err(|_| Fault::Read)?;
        let private = real_path(self.journal.private())?.canonicalize().map_err(|_| Fault::Read)?;
        if !root.starts_with(&private) || root == private || root.starts_with(Path::new(&self.repo.root)) {
            return Err(LeaseError::Path);
        }
        Ok(root)
    }
    pub fn quarantine(&mut self, lease: &Lease) -> Result<(), LeaseError> {
        let index = self.index(lease)?;
        let mut runtime = self.runtime.clone();
        runtime.leases[index].state = LeaseState::Quarantined;
        self.persist(runtime)
    }
    pub fn read(&mut self, lease: &Lease, path: &str) -> Result<(Vec<u8>, String), LeaseError> {
        self.audit(lease)?;
        self.check(lease, path)?;
        let source = scoped_path(&self.worktree(lease)?, path)?;
        if fs::metadata(&source).map_err(|_| Fault::Read)?.len() > super::wire::MAX_TEXT as u64 {
            return Err(LeaseError::Path);
        }
        let bytes = fs::read(&source).map_err(|_| Fault::Read)?;
        let digest = hash(&bytes);
        Ok((bytes, digest))
    }
    pub fn audit(&mut self, lease: &Lease) -> Result<Vec<String>, LeaseError> {
        let index = self.index(lease)?;
        let root = self.worktree(lease)?;
        let stored = |key| -> Result<Files, LeaseError> {
            serde_json::from_value(self.runtime.leases[index].extra.get(key).cloned().ok_or(LeaseError::Stale)?)
                .map_err(|_| Fault::Corrupt.into())
        };
        let initial = stored("initialFiles")?;
        let expected = stored("files")?;
        let actual = snapshot(&root);
        if actual.as_ref().ok() != Some(&expected) {
            self.quarantine(lease)?;
            return Err(LeaseError::Quarantined);
        }
        let changed: BTreeSet<_> = initial.keys().chain(expected.keys()).filter(|p| initial.get(*p) != expected.get(*p)).cloned().collect();
        for path in &changed {
            self.check(lease, path)?;
        }
        Ok(changed.into_iter().collect())
    }
    /// One explicit file operation. A crash or failed persistence leaves a durable quarantine and the old content artifact.
    pub fn patch(&mut self, lease: &Lease, grant: Option<&Grant>, revision: u64, op: &FileOp) -> Result<(), LeaseError> {
        grant
            .ok_or(LeaseError::Authority("reason.grant.none"))?
            .allows(&self.repo, revision, Effect::Worktree)
            .map_err(LeaseError::Authority)?;
        self.audit(lease)?;
        let root = self.worktree(lease)?;
        let (path, expect) = match op {
            FileOp::Write(w) => (w.path.as_str(), w.expect.as_deref()),
            FileOp::Rename(r) => (r.from.as_str(), Some(r.expect.as_str())),
            FileOp::Delete(d) => (d.path.as_str(), Some(d.expect.as_str())),
        };
        self.check(lease, path)?;
        let source = scoped_path(&root, path)?;
        if fs::metadata(&source).is_ok_and(|m| m.len() > super::wire::MAX_TEXT as u64) {
            return Err(LeaseError::Path);
        }
        let old = match fs::read(&source) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err(Fault::Read.into()),
        };
        if old.as_ref().map(|b| hash(b)).as_deref() != expect {
            return Err(LeaseError::Hash);
        }
        let target = if let FileOp::Rename(r) = op {
            self.check(lease, &r.to)?;
            let target = scoped_path(&root, &r.to)?;
            if target.exists() || source.to_string_lossy().to_lowercase() == target.to_string_lossy().to_lowercase() {
                return Err(LeaseError::Hash);
            }
            Some(target)
        } else {
            None
        };
        if let FileOp::Write(w) = op
            && (w.content.len() > super::wire::MAX_TEXT
                || source.with_file_name(format!("{}.tmp", source.file_name().unwrap_or_default().to_string_lossy())).exists())
        {
            return Err(LeaseError::Path);
        }
        let artifact = self.journal.private().join(format!("patch-{}-{}.json", lease.generation, uuid::Uuid::new_v4()));
        let evidence = serde_json::to_vec(&serde_json::json!({"operation": op, "before": old})).map_err(|_| Fault::Write)?;
        replace(&RealDisk, &artifact, &evidence)?;
        let index = self.index(lease)?;
        let mut expected: Files = serde_json::from_value(self.runtime.leases[index].extra.get("files").cloned().ok_or(LeaseError::Stale)?)
            .map_err(|_| Fault::Corrupt)?;
        let source_name = normalized(path)?;
        let source_name = expected.keys().find(|name| name.to_lowercase() == source_name.to_lowercase()).cloned().unwrap_or(source_name);
        let old_fingerprint = expected.remove(&source_name);
        let mut runtime = self.runtime.clone();
        runtime.leases[index].state = LeaseState::Quarantined;
        self.persist(runtime)?;
        let result = match op {
            FileOp::Write(w) => {
                let permissions = fs::metadata(&source).ok().map(|meta| meta.permissions());
                fs::create_dir_all(source.parent().ok_or(LeaseError::Path)?).map_err(|e| Fault::of_write(&e))?;
                replace(&RealDisk, &source, w.content.as_bytes())?;
                if let Some(permissions) = permissions {
                    fs::set_permissions(&source, permissions).map_err(|e| Fault::of_write(&e))?;
                }
                Ok(())
            }
            FileOp::Rename(_) => {
                let target = target.ok_or(LeaseError::Path)?;
                fs::create_dir_all(target.parent().ok_or(LeaseError::Path)?).map_err(|e| Fault::of_write(&e))?;
                fs::rename(&source, target).map_err(|e| Fault::of_write(&e))
            }
            FileOp::Delete(_) => fs::remove_file(&source).map_err(|e| Fault::of_write(&e)),
        };
        result?;
        let files = snapshot(&root)?;
        match op {
            FileOp::Write(w) => {
                let (name, fingerprint) =
                    files.iter().find(|(name, _)| name.to_lowercase() == source_name.to_lowercase()).ok_or(LeaseError::Quarantined)?;
                let digest = hash(w.content.as_bytes());
                let permissions = old_fingerprint.as_ref().and_then(|old| old.split_once(':').map(|(_, permissions)| permissions));
                if !fingerprint.starts_with(&format!("{digest}:"))
                    || permissions.is_some_and(|mode| fingerprint != &format!("{digest}:{mode}"))
                {
                    return Err(LeaseError::Quarantined);
                }
                expected.insert(name.clone(), fingerprint.clone());
            }
            FileOp::Rename(r) => {
                let target = normalized(&r.to)?;
                let name = files.keys().find(|name| name.to_lowercase() == target.to_lowercase()).ok_or(LeaseError::Quarantined)?;
                expected.insert(name.clone(), old_fingerprint.ok_or(LeaseError::Hash)?);
            }
            FileOp::Delete(_) => {}
        }
        if files != expected {
            return Err(LeaseError::Quarantined);
        }
        let mut runtime = self.runtime.clone();
        runtime.leases[index].state = LeaseState::Held;
        runtime.leases[index].extra.insert("files".into(), serde_json::json!(files));
        self.persist(runtime)
    }
    /// Explicit mission completion, after reconciliation. No worktree, artifact, branch or lease record is deleted.
    pub fn finish(self) -> Result<(), LeaseError> {
        if self.blocked
            || self.runtime.leases.iter().any(|l| {
                let retirement = l.extra.get("retirement");
                l.state != LeaseState::Released
                    && !(l.state == LeaseState::Quarantined
                        && retirement.is_some_and(|r| {
                            r["generation"].as_u64() == Some(l.generation)
                                && r["process"] == serde_json::json!(l.process)
                                && r["treeDead"] == true
                                && r["costSettled"] == true
                        }))
            })
            || !self.journal.pending()?.is_empty()
        {
            return Err(LeaseError::Live);
        }
        self.lock.set_len(0).and_then(|()| self.lock.sync_all()).map_err(|e| Fault::of_write(&e))?;
        Ok(())
    }
}

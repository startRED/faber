//! Private, bounded turn history. Git only lists names; project bytes always come from held plain-file handles.
use crate::model::{ContextFill, Conversation, Message, Plan, QueuedTurn, Vendor};
use crate::project::git::{Executor, Invocation};
use crate::router::Choice;
#[cfg(test)]
use faber_winproc::rewind::cache_fingerprint;
use faber_winproc::rewind::{Root, SwapStage, capture_metadata, file_identity, fingerprint, protection, resolved_path, swap_paths};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

mod pack;
mod store;
pub use store::History;

pub const FILE_LIMIT: u64 = 8 * 1024 * 1024;
pub const BASELINE_LIMIT: u64 = 64 * 1024 * 1024;
pub const PATH_LIMIT: usize = 20_000;
pub const TOTAL_LIMIT: u64 = 1024 * 1024 * 1024;
pub const FOLDER_LIMIT: u64 = 256 * 1024 * 1024;
pub const RECOVERY_RESERVE: u64 = 64 * 1024 * 1024;
pub const RETENTION: u64 = 30 * 24 * 60 * 60 * 1000;
const SCAN_TIME: Duration = Duration::from_secs(10);

/// Per-operation diagnostics for the standalone cost proof; never persisted or polled in the background.
#[derive(Debug, Clone, Copy, Default)]
pub struct Work {
    pub files_read: u64,
    pub files_reused: u64,
    pub bytes_read: u64,
    pub objects_written: u64,
    pub object_flushes: u64,
    pub policy_us: u64,
    pub metadata_us: u64,
    pub object_us: u64,
    pub cleanup_us: u64,
    pub manifest_us: u64,
    pub full_cleanup_runs: u64,
    pub owned_files_deleted: u64,
    pub object_reads: u64,
}

pub(super) fn micros(start: Instant) -> u64 {
    start.elapsed().as_micros().min(u64::MAX as u128) as u64
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub saved: usize,
    pub skipped: usize,
    pub unavailable: bool,
    pub incomplete: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cursor {
    pub entry: Option<String>,
    pub turn: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PointInfo {
    pub id: String,
    pub message_id: String,
    pub turn_id: String,
    pub at: u64,
    pub coverage: Coverage,
    pub sealed: bool,
    pub cursor: Cursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    Files,
    Conversation,
    Both,
}
impl Mode {
    pub fn files(self) -> bool {
        self != Self::Conversation
    }
    pub fn messages(self) -> bool {
        self != Self::Files
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreRequest {
    pub conversation_id: String,
    pub message_id: String,
    pub mode: Mode,
    /// Each conflicting path needs its own explicit choice. The default is to keep the current file.
    #[serde(default)]
    pub saved_versions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedoRequest {
    pub conversation_id: String,
    pub id: String,
    #[serde(default)]
    pub saved_versions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedoInfo {
    pub id: String,
    pub message_id: String,
    pub mode: Mode,
    pub expires_at: u64,
    pub recovery: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Running {
    pub conversation_id: String,
    pub writers: Vec<String>,
    pub unknown: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePreview {
    pub path: String,
    pub action: String,
    pub conflict: bool,
    pub unavailable: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub files: Vec<FilePreview>,
    pub replacements: usize,
    pub recreations: usize,
    pub removals: usize,
    pub conflicts: usize,
    pub messages: usize,
    pub coverage: Coverage,
    pub running: Vec<Running>,
    pub recovery: bool,
    pub redo_expires_at: Option<u64>,
    /// The screen must disclose the known limit of writers outside Faber with an already-open handle.
    pub notice: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub id: String,
    pub restored: Vec<String>,
    pub kept: Vec<String>,
    pub failed: Vec<String>,
    pub partial: bool,
    pub recovery: bool,
    pub notice: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Image {
    oid: String,
    bytes: u64,
    protection: Vec<u8>,
    identity: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pack: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    offset: u64,
    /// Only live snapshots can reuse a checked source; restart and recovery always begin cold.
    #[serde(skip)]
    token: Option<Vec<u8>>,
}
fn is_zero(value: &u64) -> bool {
    *value == 0
}
impl Image {
    fn same_bytes(&self, other: &Self) -> bool {
        self.oid == other.oid && self.bytes == other.bytes
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct Snapshot {
    files: BTreeMap<String, Image>,
    absent: BTreeSet<String>,
    skipped: BTreeSet<String>,
    complete: bool,
    #[serde(default)]
    excluded: BTreeSet<String>,
}
impl Snapshot {
    fn coverage(&self) -> Coverage {
        Coverage {
            saved: self.files.len(),
            skipped: self.skipped.len(),
            unavailable: false,
            incomplete: !self.complete || !self.skipped.is_empty(),
        }
    }
    fn known(&self, path: &str) -> Option<Option<&Image>> {
        if self.skipped.contains(path)
            || self.excluded.iter().any(|p| path == p || path.strip_prefix(p).is_some_and(|tail| tail.starts_with('/')))
        {
            None
        } else if let Some(image) = self.files.get(path) {
            Some(Some(image))
        } else if self.complete || self.absent.contains(path) {
            Some(None)
        } else {
            None
        }
    }
}

/// Only conversation state that can go back. Grants, spend and publication are deliberately absent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SavedConversation {
    messages: Vec<Message>,
    sessions: BTreeMap<Vendor, String>,
    tentative: BTreeMap<Vendor, String>,
    choice: Option<Choice>,
    plan: Option<Plan>,
    context: Option<ContextFill>,
    summary: Option<String>,
    queued: Vec<QueuedTurn>,
}
impl SavedConversation {
    fn bindings(c: &Conversation) -> Self {
        Self {
            messages: Vec::new(),
            sessions: c.sessions.clone(),
            tentative: c.tentative.clone(),
            choice: c.choice.clone(),
            plan: c.plan.clone(),
            context: c.context,
            summary: c.rewind_summary.clone(),
            queued: Vec::new(),
        }
    }
    pub(crate) fn take(c: &Conversation) -> Self {
        Self {
            messages: c.messages.clone(),
            sessions: c.sessions.clone(),
            tentative: c.tentative.clone(),
            choice: c.choice.clone(),
            plan: c.plan.clone(),
            context: c.context,
            summary: c.rewind_summary.clone(),
            queued: c.queued.clone(),
        }
    }
    pub(crate) fn apply(&self, c: &mut Conversation) {
        c.messages.clone_from(&self.messages);
        // Native rewind semantics are unproved. Both restore and redo continue from saved messages.
        c.sessions.clear();
        c.tentative.clear();
        c.choice.clone_from(&self.choice);
        c.plan.clone_from(&self.plan);
        c.context = self.context;
        c.reported = (None, None);
        c.keep_going = None;
        c.queued.clone_from(&self.queued);
        c.queued.iter_mut().for_each(|q| q.sending = false);
        c.compacting = false;
        c.weighing = None;
        c.rewind_notice = Some("note.rewind.savedMessages".into());
        c.rewind_summary.clone_from(&self.summary);
    }
}

/// Capturing file history never clones the conversation's messages or attachment bytes.
pub(crate) struct Capture {
    conversation: String,
    saved: SavedConversation,
}
impl Capture {
    pub(crate) fn new(c: &Conversation) -> Self {
        Self { conversation: c.id.clone(), saved: SavedConversation::bindings(c) }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Point {
    info: PointInfo,
    folder: String,
    root: PathBuf,
    root_identity: Vec<u8>,
    conversation: String,
    parent: Option<String>,
    sequence: u64,
    before: Arc<Snapshot>,
    after: Option<Arc<Snapshot>>,
    saved: SavedConversation,
    writers: Vec<String>,
    unknown_author: bool,
    #[serde(skip)]
    active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct FileOperation {
    path: String,
    before: Option<Image>,
    after: Option<Image>,
    applied: bool,
    after_identity: Option<Vec<u8>>,
    temporary: Option<TemporaryFile>,
    #[serde(default)]
    displaced: Option<TemporaryFile>,
    #[serde(default)]
    installing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct TemporaryFile {
    path: String,
    identity: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Journal {
    id: String,
    conversation: String,
    point: Option<String>,
    #[serde(default)]
    message_id: Option<String>,
    #[serde(default)]
    mode: Option<Mode>,
    folder: String,
    root: PathBuf,
    root_identity: Vec<u8>,
    at: u64,
    expires: u64,
    operations: Vec<FileOperation>,
    before: SavedConversation,
    after: SavedConversation,
    files_done: bool,
    conversation_done: bool,
    redo: bool,
    retained: Vec<(String, Image)>,
    partial: bool,
}

pub(super) fn error(_: impl std::fmt::Debug) -> String {
    "error.rewind.storage".into()
}

/// A link or writer introduced after open must not turn a later chunk into an unchecked read.
struct StableRead<'a> {
    file: &'a mut std::fs::File,
    identity: &'a [u8],
}
impl Read for StableRead<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if fingerprint(self.file)? != self.identity {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let n = self.file.read(bytes)?;
        if fingerprint(self.file)? != self.identity {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(n)
    }
}

fn allowed(root: &Path, path: &str) -> bool {
    allowed_with(root, path, &|key| std::env::var(key).ok())
}

fn allowed_with(root: &Path, path: &str, var: &dyn Fn(&str) -> Option<String>) -> bool {
    if !crate::changes::safe(path) || crate::permissions::secret(&root.join(path), true) {
        return false;
    }
    if root.join(path).canonicalize().is_ok_and(|real| {
        crate::permissions::secret(&real, true) || real.components().any(|p| p.as_os_str().to_string_lossy().eq_ignore_ascii_case(".git"))
    }) {
        return false;
    }
    let normal = |p: &str| {
        // Metadata resolution catches relocated directory aliases too, without reading a credential's contents.
        let real = Path::new(p).canonicalize().unwrap_or_else(|_| PathBuf::from(p));
        let p = real.to_string_lossy().replace('\\', "/").to_lowercase();
        let p = if let Some(unc) = p.strip_prefix("//?/unc/") { format!("//{unc}") } else { p.strip_prefix("//?/").unwrap_or(&p).into() };
        let mut parts = Vec::new();
        for part in p.split('/') {
            if part == "." {
                continue;
            }
            if part == ".." {
                if parts.last().is_some_and(|p: &&str| !p.ends_with(':') && !p.is_empty()) {
                    parts.pop();
                }
            } else {
                parts.push(part);
            }
        }
        parts.join("/").trim_end_matches('/').to_string()
    };
    let absolute = normal(&root.join(path).to_string_lossy());
    !crate::permissions::relocated_credentials(var).into_iter().any(|(p, file)| {
        let p = normal(&p);
        absolute == p || (!file && absolute.strip_prefix(&p).is_some_and(|tail| tail.starts_with('/')))
    })
}

fn names(exec: &mut dyn Executor, root: &Path, git_dir: &Path, deadline: Instant, ignored: bool) -> Result<Vec<u8>, String> {
    let args = if ignored {
        vec!["ls-files", "--others", "--ignored", "--exclude-standard", "--directory", "-z", "--"]
    } else {
        vec!["ls-files", "--cached", "--others", "--exclude-standard", "-z", "--"]
    };
    let mut call = Invocation::new(root, &args).before(deadline);
    // Listing reads names and ignore rules, never content, filters, an index write or a person's refs.
    let out = exec.execute_bounded(&call, 4 * 1024 * 1024).map_err(error)?;
    if out.success {
        return Ok(out.stdout);
    }
    let dir = format!("--git-dir={}", crate::project::git::native_path(git_dir));
    let tree = format!("--work-tree={}", crate::project::git::native_path(root));
    let mut private = vec![dir.as_str(), tree.as_str()];
    private.extend(args.into_iter().filter(|a| *a != "--cached"));
    call = Invocation::new(root, &private).before(deadline);
    let out = exec.execute_bounded(&call, 4 * 1024 * 1024).map_err(error)?;
    if !out.success {
        return Err("error.rewind.git".into());
    }
    Ok(out.stdout)
}

fn nested_repository(root: &Path, path: &str) -> bool {
    let mut parent = Path::new(path).parent();
    while let Some(p) = parent.filter(|p| !p.as_os_str().is_empty()) {
        if root.join(p).join(".git").symlink_metadata().is_ok() {
            return true;
        }
        parent = p.parent();
    }
    false
}

fn silently_excluded(root: &Path, path: &str) -> bool {
    nested_repository(root, path) || root.join(path).symlink_metadata().is_ok_and(|m| !m.is_file() || m.file_type().is_symlink())
}

/// The root is the canonical path held by the safe walker. Relative opens cannot follow a different alias.
struct CapturePolicy<'a> {
    root: &'a Path,
}
impl CapturePolicy<'_> {
    fn allows_file(&self, path: &str, file: &std::fs::File) -> io::Result<bool> {
        let resolved = resolved_path(file)?.unwrap_or_else(|| self.root.join(path));
        Ok(self.allows_resolved(&resolved, &|key| std::env::var(key).ok()))
    }

    fn can_reuse_admission(&self, path: &str, var: &dyn Fn(&str) -> Option<String>) -> bool {
        !path.contains('/') && crate::permissions::relocated_credentials(var).is_empty()
    }

    fn names_allow(&self, path: &str) -> bool {
        crate::changes::safe(path)
            && !crate::permissions::secret(&self.root.join(path), true)
            && !crate::verify::own_log(
                &format!("{}/{}", crate::project::git::native_path(self.root).replace('\\', "/"), path).to_lowercase(),
            )
            && !self.root.components().any(|p| p.as_os_str().to_string_lossy().eq_ignore_ascii_case(".git"))
    }

    #[cfg(test)]
    fn allows(&self, path: &str, var: &dyn Fn(&str) -> Option<String>) -> bool {
        if !self.names_allow(path) {
            return false;
        }
        self.allows_resolved(&self.root.join(path), var)
    }

    fn allows_resolved(&self, path: &Path, var: &dyn Fn(&str) -> Option<String>) -> bool {
        if crate::permissions::secret(path, true)
            || crate::verify::own_log(&crate::project::git::native_path(path).replace('\\', "/").to_lowercase())
            || path.components().any(|part| part.as_os_str().to_string_lossy().eq_ignore_ascii_case(".git"))
        {
            return false;
        }
        let normal = |p: &Path| {
            let raw = crate::project::git::native_path(p).replace('\\', "/").to_lowercase();
            let mut parts = Vec::new();
            for part in raw.split('/') {
                if part == "." {
                    continue;
                }
                if part == ".." && parts.last().is_some_and(|p: &&str| !p.is_empty() && !p.ends_with(':')) {
                    parts.pop();
                } else {
                    parts.push(part);
                }
            }
            parts.join("/").trim_end_matches('/').to_string()
        };
        let moved = crate::permissions::relocated_credentials(var);
        if moved.is_empty() {
            return true;
        }
        let absolute = normal(path);
        !moved.into_iter().any(|(path, file)| {
            // Resolve moved credential aliases on every content admission, even when their environment value is unchanged.
            let path = Path::new(&path).canonicalize().unwrap_or_else(|_| PathBuf::from(path));
            let path = normal(&path);
            absolute == path || (!file && absolute.strip_prefix(&path).is_some_and(|tail| tail.starts_with('/')))
        })
    }
}

impl History {
    fn range_coverage(&self, first: &Point) -> Coverage {
        let mut coverage = first.info.coverage.clone();
        for point in
            self.points.values().filter(|p| p.conversation == first.conversation && p.folder == first.folder && p.sequence > first.sequence)
        {
            coverage.skipped += point.info.coverage.skipped;
            coverage.incomplete |= point.info.coverage.incomplete;
            coverage.unavailable |= point.info.coverage.unavailable;
        }
        coverage
    }

    pub fn has_open(&self, root: &Path) -> bool {
        self.points.values().any(|p| p.active && !p.info.sealed && p.root == root)
    }

    pub fn recovery_conversations(&self) -> Vec<String> {
        self.journals
            .values()
            .filter(|j| !j.conversation_done)
            .map(|j| j.conversation.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn pending_acknowledgements(&self, conversation: &str) -> Vec<String> {
        self.journals.values().filter(|j| j.conversation == conversation && !j.conversation_done).map(|j| j.id.clone()).collect()
    }

    pub fn point_root(&self, conversation: &str, message: &str) -> Option<&Path> {
        self.points
            .values()
            .filter(|p| p.conversation == conversation && p.info.message_id == message)
            .max_by_key(|p| p.sequence)
            .map(|p| p.root.as_path())
    }

    pub(crate) fn missing_point(&self, point: &PointInfo) -> bool {
        point.coverage.unavailable || !self.points.contains_key(&point.id)
    }

    pub fn redo_expiry(&self, conversation: &str) -> Option<u64> {
        self.journals.values().filter(|j| j.conversation == conversation && !j.redo).map(|j| j.expires).max()
    }

    pub fn redo_root(&self, id: &str) -> Option<&Path> {
        self.journals.get(id).filter(|j| !j.operations.is_empty()).map(|j| j.root.as_path())
    }

    pub fn redos(&self, conversation: &Conversation, at: u64) -> Vec<RedoInfo> {
        let mut journals: Vec<_> = self
            .journals
            .values()
            .filter(|j| {
                j.conversation == conversation.id
                    && (!j.conversation_done || (!j.redo && j.files_done && j.expires > at && j.after.messages == conversation.messages))
            })
            .collect();
        journals.sort_by_key(|j| (j.conversation_done, std::cmp::Reverse(j.at)));
        journals
            .into_iter()
            .map(|j| RedoInfo {
                id: j.id.clone(),
                message_id: j
                    .message_id
                    .clone()
                    .or_else(|| j.point.as_ref().and_then(|id| self.points.get(id)).map(|p| p.info.message_id.clone()))
                    .or_else(|| j.before.messages.get(j.after.messages.len()).map(|m| m.id.clone()))
                    .unwrap_or_default(),
                mode: j.mode.unwrap_or(if j.before.messages == j.after.messages {
                    Mode::Files
                } else if j.operations.is_empty() {
                    Mode::Conversation
                } else {
                    Mode::Both
                }),
                expires_at: j.expires,
                recovery: !j.conversation_done,
            })
            .collect()
    }

    pub fn redo_preview(&self, request: &RedoRequest, conversation: &Conversation, at: u64) -> Result<Preview, String> {
        if let Some(journal) = self.journals.get(&request.id).filter(|j| j.conversation == request.conversation_id && !j.conversation_done)
        {
            return Ok(Preview {
                recovery: true,
                coverage: Coverage { incomplete: true, ..Coverage::default() },
                redo_expires_at: Some(journal.expires),
                notice: "note.rewind.oldHandle".into(),
                ..Preview::default()
            });
        }
        let journal = self
            .journals
            .get(&request.id)
            .filter(|j| j.conversation == request.conversation_id && !j.redo && j.expires > at && j.files_done)
            .ok_or("error.rewind.unavailable")?;
        if conversation.messages != journal.after.messages {
            return Err("error.rewind.conversationChanged".into());
        }
        let mut preview = Preview {
            messages: journal.before.messages.len().saturating_sub(journal.after.messages.len()),
            coverage: Coverage { incomplete: journal.partial, ..Coverage::default() },
            redo_expires_at: Some(journal.expires),
            recovery: self.recovery_for(&request.conversation_id),
            notice: "note.rewind.oldHandle".into(),
            ..Preview::default()
        };
        if journal.operations.is_empty() {
            return Ok(preview);
        }
        let root = Root::open(&journal.root).map_err(error)?;
        if root.identity().map_err(error)? != journal.root_identity {
            return Err("error.rewind.folderChanged".into());
        }
        for op in journal.operations.iter().filter(|op| op.applied) {
            let now = if allowed(&journal.root, &op.path) { current(&root, &op.path) } else { Err("error.rewind.path".into()) };
            let unavailable = now.is_err() || op.before.as_ref().is_some_and(|image| self.blob(&journal.folder, image).is_err());
            let conflict = unavailable
                || match now.as_ref().ok().and_then(|c| c.as_ref()) {
                    Some((id, bytes, _)) => {
                        op.after_identity.as_ref() != Some(id) || op.after.as_ref().is_none_or(|p| !self.matches(&journal.folder, p, bytes))
                    }
                    None => op.after.is_some(),
                };
            let action = if op.before.is_none() {
                preview.removals += 1;
                "remove"
            } else if now.as_ref().ok().is_some_and(Option::is_none) {
                preview.recreations += 1;
                "recreate"
            } else {
                preview.replacements += 1;
                "replace"
            };
            preview.coverage.incomplete |= unavailable;
            preview.coverage.saved += usize::from(!unavailable);
            preview.coverage.skipped += usize::from(unavailable);
            preview.conflicts += usize::from(conflict);
            preview.files.push(FilePreview { path: op.path.clone(), action: action.into(), conflict, unavailable });
        }
        Ok(preview)
    }

    fn scan_held(&mut self, exec: &mut dyn Executor, root: &Path, folder: &str, reader: &Root) -> Result<Snapshot, String> {
        let policy = CapturePolicy { root: reader.path() };
        let previous = self
            .points
            .values()
            .filter(|p| p.folder == folder)
            .max_by_key(|p| p.sequence)
            .map(|p| p.after.as_ref().unwrap_or(&p.before).clone());
        let deadline = Instant::now() + SCAN_TIME;
        let bytes = names(exec, root, &self.folder_dir(folder), deadline, false)?;
        let mut snapshot = Snapshot { complete: bytes.len() <= 4 * 1024 * 1024, ..Snapshot::default() };
        let ignored = names(exec, root, &self.folder_dir(folder), deadline, true)?;
        snapshot.complete &= ignored.len() <= 4 * 1024 * 1024;
        for path in ignored.split(|b| *b == 0).filter(|p| !p.is_empty()) {
            if let Ok(path) = std::str::from_utf8(path) {
                snapshot.excluded.insert(path.trim_end_matches('/').into());
                if snapshot.excluded.len() > PATH_LIMIT {
                    snapshot.complete = false;
                    break;
                }
            } else {
                snapshot.complete = false;
            }
        }
        let mut size = 0;
        let mut paths = BTreeSet::new();
        for raw in bytes.split(|b| *b == 0).filter(|p| !p.is_empty()) {
            let Ok(path) = std::str::from_utf8(raw) else {
                snapshot.complete = false;
                continue;
            };
            let began = Instant::now();
            // The safe open itself rejects special leaves without blocking; only failed opens need a separate type probe.
            let eligible = policy.names_allow(path) && !nested_repository(reader.path(), path);
            self.record_work(|work| work.policy_us += micros(began));
            if eligible {
                paths.insert(path.to_string());
            } else {
                snapshot.excluded.insert(path.trim_end_matches('/').into());
            }
            if paths.len() > PATH_LIMIT {
                snapshot.complete = false;
                break;
            }
        }
        for path in paths.into_iter().take(PATH_LIMIT) {
            if Instant::now() >= deadline {
                snapshot.complete = false;
                snapshot.skipped.insert(path);
                continue;
            }
            let mut image = None;
            for _ in 0..2 {
                let result: io::Result<Image> = (|| {
                    let began = Instant::now();
                    let mut file = reader.open_file(&path)?;
                    self.record_work(|work| work.metadata_us += micros(began));
                    let began = Instant::now();
                    let (identity, token, len) = capture_metadata(&file)?;
                    let protect = protection(&file)?;
                    if len > FILE_LIMIT || size + len > BASELINE_LIMIT {
                        return Err(io::ErrorKind::FileTooLarge.into());
                    }
                    self.record_work(|work| work.metadata_us += micros(began));
                    if let Some(cached) = previous.as_ref().and_then(|p| p.files.get(&path))
                        && token.is_some()
                        && cached.token == token
                        && cached.identity == identity
                        && cached.protection == protect
                        && cached.bytes == len
                    {
                        // A pinned root plus a generation-stable leaf cannot acquire a different Windows long name.
                        // Nested parents and relocated credential aliases still require fresh policy resolution.
                        if policy.can_reuse_admission(&path, &|key| std::env::var(key).ok()) {
                            self.record_work(|work| work.files_reused += 1);
                            return Ok(cached.clone());
                        }
                    }
                    let began = Instant::now();
                    // Short names can hide secrets; resolve the held file before every new content admission.
                    let admission = policy.allows_file(&path, &file);
                    self.record_work(|work| work.policy_us += micros(began));
                    if !admission? {
                        snapshot.excluded.insert(path.clone());
                        return Err(io::ErrorKind::PermissionDenied.into());
                    }
                    if let Some(cached) = previous.as_ref().and_then(|p| p.files.get(&path))
                        && token.is_some()
                        && cached.token == token
                        && cached.identity == identity
                        && cached.protection == protect
                        && cached.bytes == len
                    {
                        self.record_work(|work| work.files_reused += 1);
                        return Ok(cached.clone());
                    }
                    let saved = self.write_capture_blob(folder, &mut StableRead { file: &mut file, identity: &identity }, len);
                    if fingerprint(&file)? != identity {
                        return Err(io::ErrorKind::WouldBlock.into());
                    }
                    self.record_work(|work| {
                        work.files_read += 1;
                        work.bytes_read += len;
                    });
                    let (oid, pack, offset) = saved.map_err(io::Error::other)?;
                    Ok(Image { oid, pack: Some(pack), offset, bytes: len, protection: protect, identity, token })
                })();
                match result {
                    Ok(saved) => {
                        image = Some(saved);
                        break;
                    }
                    Err(_) if snapshot.excluded.contains(&path) || silently_excluded(reader.path(), &path) => {
                        snapshot.excluded.insert(path.clone());
                        break;
                    }
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {
                        snapshot.absent.insert(path.clone());
                        break;
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(_) => break,
                }
            }
            if let Some(image) = image {
                size += image.bytes;
                snapshot.files.insert(path, image);
            } else if !snapshot.absent.contains(&path) && !snapshot.excluded.contains(&path) {
                snapshot.skipped.insert(path);
            }
        }
        Ok(snapshot)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn capture(
        &mut self,
        exec: &mut dyn Executor,
        root: &Path,
        c: &Conversation,
        message: &str,
        turn: &str,
        at: u64,
        unknown_author: bool,
    ) -> Result<PointInfo, String> {
        if !c.messages.iter().any(|m| m.id == message && m.role == crate::model::Role::User) {
            return Err("error.rewind.message".into());
        }
        self.capture_prepared(exec, root, Capture::new(c), message, turn, at, unknown_author)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn capture_prepared(
        &mut self,
        exec: &mut dyn Executor,
        root: &Path,
        source: Capture,
        message: &str,
        turn: &str,
        at: u64,
        unknown_author: bool,
    ) -> Result<PointInfo, String> {
        let reader = Root::open(root).map_err(error)?;
        let identity = reader.identity().map_err(error)?;
        let folder = self.folder_id(root, &identity);
        // Reserve the maximum payload, allowed manifest, and pack index/rounding before admitting a capture.
        self.admit(&folder, BASELINE_LIMIT + RECOVERY_RESERVE / 2 + 4 * 1024 * 1024)?;
        self.prepare_folder(&folder)?;
        self.begin_batch(&folder)?;
        let mut before = self.scan_held(exec, root, &folder, &reader)?;
        self.finish_batch(&mut before)?;
        let previous = self.points.values().filter(|p| p.folder == folder).max_by_key(|p| p.sequence);
        let sequence = previous.map_or(1, |p| p.sequence.saturating_add(1));
        let parent = previous.map(|p| p.info.id.clone());
        let info = PointInfo {
            id: Uuid::new_v4().to_string(),
            message_id: message.into(),
            turn_id: turn.into(),
            at,
            coverage: before.coverage(),
            sealed: false,
            cursor: Cursor::default(),
        };
        let point = Point {
            info: info.clone(),
            root: root.into(),
            root_identity: identity,
            folder,
            parent,
            sequence,
            conversation: source.conversation,
            before: Arc::new(before),
            after: None,
            saved: source.saved,
            writers: vec![format!("turn:{turn}")],
            unknown_author,
            active: true,
        };
        self.save_point(&point)?;
        self.points.insert(info.id.clone(), point);
        Ok(info)
    }

    pub fn list(&self, conversation: &str) -> Vec<PointInfo> {
        let mut points: Vec<_> = self.points.values().filter(|p| p.conversation == conversation).map(|p| p.info.clone()).collect();
        points.sort_by_key(|p| p.at);
        points
    }

    pub fn cursor(&mut self, conversation: &str, cursor: Cursor) {
        if let Some(p) = self.points.values_mut().filter(|p| p.conversation == conversation && !p.info.sealed).max_by_key(|p| p.sequence) {
            p.info.cursor = cursor;
        }
    }

    pub(crate) fn turn_cursor(&mut self, conversation: &str, turn: &str, cursor: Cursor) {
        for p in self.points.values_mut().filter(|p| p.conversation == conversation && p.info.turn_id == turn) {
            p.info.cursor = cursor.clone();
        }
    }

    pub(crate) fn record_writers(&mut self, conversation: &str, writers: &[String]) {
        for p in self.points.values_mut().filter(|p| p.conversation == conversation && p.active) {
            p.writers.extend(writers.iter().cloned());
            p.writers.sort();
            p.writers.dedup();
        }
    }

    /// A scan alone never proves completion: the engine rechecks its admission revision before publishing it.
    pub fn scan_seal(&mut self, exec: &mut dyn Executor, root: &Path) -> Result<Sealing, String> {
        let reader = Root::open(root).map_err(error)?;
        let identity = reader.identity().map_err(error)?;
        let folder = self.folder_id(root, &identity);
        let ids: Vec<_> = self
            .points
            .values()
            .filter(|p| p.folder == folder && p.active && !p.info.sealed && p.root_identity == identity)
            .map(|p| p.info.id.clone())
            .collect();
        if ids.is_empty() {
            return Err("error.rewind.folderChanged".into());
        }
        self.begin_batch(&folder)?;
        let mut snapshot = self.scan_held(exec, root, &folder, &reader)?;
        self.finish_batch(&mut snapshot)?;
        Ok(Sealing { ids, snapshot })
    }

    pub fn seal(&mut self, seal: Sealing, at: u64) -> Result<(), String> {
        let publication = self.prepare_seal(seal);
        self.publish_seal(&publication);
        self.persist_seal(publication, at)
    }

    pub fn prepare_seal(&self, seal: Sealing) -> Publication {
        let snapshot = Arc::new(seal.snapshot);
        let mut updates = Vec::new();
        for id in seal.ids {
            if let Some(mut point) = self.points.get(&id).cloned() {
                point.after = Some(snapshot.clone());
                point.info.sealed = true;
                point.active = false;
                let after = snapshot.coverage();
                point.info.coverage.incomplete |= after.incomplete;
                point.info.coverage.skipped += after.skipped;
                updates.push(point);
            }
        }
        Publication { updates }
    }

    /// Memory publication shares the engine's idle/admission check. Disk work runs after releasing that lock.
    pub fn publish_seal(&mut self, publication: &Publication) {
        for point in &publication.updates {
            self.points.insert(point.info.id.clone(), point.clone());
        }
    }

    pub fn persist_seal(&mut self, publication: Publication, at: u64) -> Result<(), String> {
        for point in publication.updates {
            self.save_point(&point)?;
        }
        self.cleanup_known(at)
    }
}

pub struct Sealing {
    ids: Vec<String>,
    snapshot: Snapshot,
}
pub struct Publication {
    updates: Vec<Point>,
}

/// File bytes and identity are checked independently: the same bytes in a replaced inode still conflict for redo.
type CurrentFile = (Vec<u8>, Vec<u8>, Vec<u8>);
type FileRange = BTreeMap<String, (Option<Image>, Option<Image>, bool)>;

fn current(root: &Root, path: &str) -> Result<Option<CurrentFile>, String> {
    match root.open_file(path) {
        Ok(mut file) => read_current(root, path, &mut file).map(Some),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(error(e)),
    }
}

fn read_current(root: &Root, path: &str, file: &mut std::fs::File) -> Result<CurrentFile, String> {
    if !(CapturePolicy { root: root.path() }).allows_file(path, file).map_err(error)? {
        return Err("error.rewind.path".into());
    }
    let identity = fingerprint(file).map_err(error)?;
    let protect = protection(file).map_err(error)?;
    if file.metadata().map_err(error)?.len() > FILE_LIMIT {
        return Err("error.rewind.tooLarge".into());
    }
    let mut bytes = Vec::new();
    StableRead { file: &mut *file, identity: &identity }.take(FILE_LIMIT + 1).read_to_end(&mut bytes).map_err(error)?;
    if bytes.len() as u64 > FILE_LIMIT || fingerprint(file).map_err(error)? != identity {
        return Err("error.rewind.changed".into());
    }
    Ok((identity, bytes, protect))
}

fn equal(a: Option<&Image>, b: Option<&Image>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.same_bytes(b),
        _ => false,
    }
}

impl History {
    fn range(&self, conversation: &str, message: &str) -> Result<(&Point, FileRange), String> {
        let first = self
            .points
            .values()
            .filter(|p| p.conversation == conversation && p.info.message_id == message)
            .max_by_key(|p| p.sequence)
            .ok_or("error.rewind.unavailable")?;
        let mut later: Vec<_> = self
            .points
            .values()
            .filter(|p| p.conversation == conversation && p.folder == first.folder && p.sequence >= first.sequence)
            .collect();
        later.sort_by_key(|p| p.sequence);
        let mut paths = BTreeSet::new();
        for point in &later {
            paths.extend(point.before.files.keys().cloned());
            paths.extend(point.before.absent.iter().cloned());
            if let Some(after) = &point.after {
                paths.extend(after.files.keys().cloned());
                paths.extend(after.absent.iter().cloned());
            }
        }
        let mut range = BTreeMap::new();
        for path in paths {
            let target = first.before.known(&path);
            let mut post = None;
            let mut unknown = first.unknown_author;
            for point in &later {
                if let Some(after) = &point.after {
                    post = after.known(&path);
                } else {
                    unknown = true;
                }
                unknown |= point.unknown_author;
            }
            let Some(target) = target else {
                continue;
            };
            if post.is_some_and(|post| equal(target, post)) {
                continue;
            }
            // A file written by another conversation after this point has an unknown author for this range.
            unknown |=
                self.points.values().any(|p| p.folder == first.folder && p.conversation != conversation && p.sequence >= first.sequence);
            range.insert(path, (target.cloned(), post.flatten().cloned(), unknown || post.is_none()));
        }
        Ok((first, range))
    }

    pub fn preview(&self, request: &RestoreRequest) -> Result<Preview, String> {
        let mut preview = Preview { notice: "note.rewind.oldHandle".into(), ..Preview::default() };
        preview.redo_expires_at = self.redo_expiry(&request.conversation_id);
        preview.recovery = self.recovery_for(&request.conversation_id);
        if !request.mode.files() {
            return Ok(preview);
        }
        let (point, range) = match self.range(&request.conversation_id, &request.message_id) {
            Ok(range) => range,
            Err(key) if key == "error.rewind.unavailable" && request.mode.messages() => {
                preview.coverage = Coverage { unavailable: true, incomplete: true, ..Coverage::default() };
                return Ok(preview);
            }
            Err(key) => return Err(key),
        };
        preview.coverage = self.range_coverage(point);
        let root = Root::open(&point.root).map_err(error)?;
        if root.identity().map_err(error)? != point.root_identity {
            return Err("error.rewind.folderChanged".into());
        }
        for (path, (target, post, unknown)) in range {
            let now = if allowed(&point.root, &path) { current(&root, &path) } else { Err("error.rewind.path".into()) };
            let unavailable = now.is_err() || target.as_ref().is_some_and(|image| self.blob(&point.folder, image).is_err());
            preview.coverage.incomplete |= unavailable;
            let conflict = unknown
                || unavailable
                || match now.as_ref().ok().and_then(|c| c.as_ref()) {
                    Some((_, bytes, _)) => post.as_ref().is_none_or(|p| !self.matches(&point.folder, p, bytes)),
                    None => post.is_some(),
                };
            let action = if target.is_none() {
                preview.removals += 1;
                "remove"
            } else if now.as_ref().ok().is_some_and(Option::is_none) {
                preview.recreations += 1;
                "recreate"
            } else {
                preview.replacements += 1;
                "replace"
            };
            preview.conflicts += usize::from(conflict);
            preview.files.push(FilePreview { path, action: action.into(), conflict, unavailable });
        }
        Ok(preview)
    }

    pub fn restore(&mut self, request: &RestoreRequest, conversation: &mut Conversation, at: u64) -> Result<RestoreResult, String> {
        let (point, range) = if request.mode.files() {
            match self.range(&request.conversation_id, &request.message_id) {
                Ok((p, r)) => (Some(p.clone()), r),
                Err(key) if key == "error.rewind.unavailable" && request.mode.messages() => (None, BTreeMap::new()),
                Err(key) => return Err(key),
            }
        } else {
            (
                self.points
                    .values()
                    .find(|p| p.conversation == request.conversation_id && p.info.message_id == request.message_id)
                    .cloned(),
                BTreeMap::new(),
            )
        };
        let before = SavedConversation::take(conversation);
        let mut after = before.clone();
        if request.mode.messages() {
            let cut = after
                .messages
                .iter()
                .position(|m| m.id == request.message_id && m.role == crate::model::Role::User)
                .ok_or("error.rewind.message")?;
            after.messages.truncate(cut);
            after.queued.clear();
            if let Some(point) = &point {
                after.plan = point.saved.plan.clone();
                after.context = point.saved.context;
                after.choice = point.saved.choice.clone();
            } else {
                after.plan = None;
                after.context = None;
            }
        }
        if request.mode == Mode::Files {
            after.summary = Some(format!(
                "Faber restored the files to their saved state before message {}. The conversation remains unchanged. Some files may have been kept because they changed or could not be restored; inspect the current files before continuing. Do not replay earlier commands.",
                request.message_id
            ));
        }
        let root_path = point.as_ref().map(|p| p.root.clone()).unwrap_or_else(|| PathBuf::from("."));
        let root = if request.mode.files() && point.is_some() { Some(Root::open(&root_path).map_err(error)?) } else { None };
        let identity = match &root {
            Some(root) => root.identity().map_err(error)?,
            None => Vec::new(),
        };
        if point.as_ref().is_some_and(|p| request.mode.files() && p.root_identity != identity) {
            return Err("error.rewind.folderChanged".into());
        }
        let folder = point.as_ref().map(|p| p.folder.clone()).unwrap_or_else(|| self.folder_id(&root_path, &identity));
        self.prepare_folder(&folder)?;
        let gap = conversation
            .messages
            .iter()
            .skip_while(|m| m.id != request.message_id)
            .any(|m| m.rewind.as_ref().is_some_and(|p| self.missing_point(p)));
        let partial = request.mode.files()
            && (gap
                || point.as_ref().is_none_or(|p| {
                    let coverage = self.range_coverage(p);
                    coverage.incomplete || coverage.unavailable
                }));
        let mut journal = Journal {
            id: Uuid::new_v4().to_string(),
            conversation: conversation.id.clone(),
            point: point.as_ref().map(|p| p.info.id.clone()),
            message_id: Some(request.message_id.clone()),
            mode: Some(request.mode),
            folder,
            root: root_path,
            root_identity: identity,
            at,
            expires: at.saturating_add(RETENTION),
            operations: Vec::new(),
            before,
            after,
            files_done: false,
            conversation_done: false,
            redo: false,
            retained: Vec::new(),
            partial,
        };
        let mut result =
            RestoreResult { id: journal.id.clone(), notice: Some("note.rewind.savedMessages".into()), ..RestoreResult::default() };
        // The conversation and cursor are journaled even if every file must be kept.
        self.save_journal(&journal)?;
        self.journals.insert(journal.id.clone(), journal.clone());
        for (path, (target, post, unknown)) in range {
            if !allowed(&journal.root, &path) {
                result.failed.push(path);
                continue;
            }
            let root = root.as_ref().expect("file mode has a held root");
            let now = match current(root, &path) {
                Ok(now) => now,
                Err(_) => {
                    result.failed.push(path);
                    continue;
                }
            };
            let conflict = unknown
                || gap
                || match &now {
                    Some((_, bytes, _)) => post.as_ref().is_none_or(|p| !self.matches(&journal.folder, p, bytes)),
                    None => post.is_some(),
                };
            if conflict && !request.saved_versions.contains(&path) {
                result.kept.push(path);
                continue;
            }
            let saved = match &now {
                Some((id, bytes, protection)) => match self.save_bytes(&journal.folder, bytes, protection.clone(), id.clone()) {
                    Ok(saved) => Some(saved),
                    Err(_) => {
                        result.failed.push(path);
                        continue;
                    }
                },
                None => None,
            };
            let index = journal.operations.len();
            let name = format!(".faber-rewind-{}-{index}", journal.id);
            let (temp_path, _) = swap_paths(&path, &name);
            journal.operations.push(FileOperation {
                path: path.clone(),
                before: saved,
                after: target.clone(),
                applied: false,
                after_identity: None,
                temporary: target.as_ref().map(|_| TemporaryFile { path: temp_path, identity: None }),
                displaced: None,
                installing: false,
            });
            if self.save_journal(&journal).is_err() {
                result.failed.push(path);
                journal.operations.pop();
                continue;
            }
            self.journals.insert(journal.id.clone(), journal.clone());
            let desired = match &target {
                Some(image) => match self.blob(&journal.folder, image) {
                    Ok(b) => Some(b),
                    Err(_) => {
                        result.failed.push(path);
                        continue;
                    }
                },
                None => None,
            };
            let expected = now.as_ref().map(|(id, bytes, _)| (id.as_slice(), bytes.as_slice()));
            let desired = desired.as_ref().zip(target.as_ref()).map(|(bytes, image)| (bytes.as_slice(), image.protection.as_slice()));
            let mutation = root.mutate_staged(&path, expected, desired, &name, |stage, file| {
                self.record_swap(&mut journal, index, &name, stage, file).map_err(io::Error::other)
            });
            match mutation {
                Ok(()) => {
                    let op = journal.operations.last_mut().expect("operation is journaled");
                    op.applied = true;
                    op.after_identity = current(root, &path).ok().flatten().map(|(id, _, _)| id);
                    if self.save_journal(&journal).is_err() {
                        result.recovery = true;
                    }
                    self.journals.insert(journal.id.clone(), journal.clone());
                    result.restored.push(path);
                }
                Err(_) => result.failed.push(path),
            }
        }
        journal.partial |= !result.kept.is_empty() || !result.failed.is_empty();
        journal.files_done = self.recover_temporaries(&mut journal).is_ok();
        if !journal.files_done {
            result.recovery = true;
        }
        if self.save_journal(&journal).is_ok() && journal.files_done {
            journal.after.apply(conversation);
        } else {
            journal.files_done = false;
            result.recovery = true;
        }
        result.partial = partial || journal.partial || result.recovery || !result.kept.is_empty() || !result.failed.is_empty();
        self.journals.insert(journal.id.clone(), journal);
        Ok(result)
    }

    /// The caller durably saves the conversation before acknowledging the cursor part of the journal.
    pub fn acknowledge(&mut self, id: &str) -> Result<(), String> {
        let mut journal = self.journals.get(id).cloned().ok_or("error.rewind.unavailable")?;
        if !journal.files_done {
            return Err("error.rewind.recovery".into());
        }
        journal.conversation_done = true;
        self.save_journal(&journal)?;
        self.journals.insert(id.into(), journal);
        Ok(())
    }

    pub fn redo(&mut self, request: &RedoRequest, conversation: &mut Conversation, at: u64) -> Result<RestoreResult, String> {
        let mut journal = self
            .journals
            .get(&request.id)
            .cloned()
            .filter(|j| j.conversation == request.conversation_id && !j.redo && j.expires > at && j.files_done)
            .ok_or("error.rewind.unavailable")?;
        let root = if journal.operations.is_empty() {
            None
        } else {
            let root = Root::open(&journal.root).map_err(error)?;
            if root.identity().map_err(error)? != journal.root_identity {
                return Err("error.rewind.folderChanged".into());
            }
            Some(root)
        };
        let mut result =
            RestoreResult { id: journal.id.clone(), notice: Some("note.rewind.savedMessages".into()), ..RestoreResult::default() };
        // Redo never sends a turn or replays a tool. A changed conversation needs an explicit new restore.
        if conversation.messages != journal.after.messages {
            return Err("error.rewind.conversationChanged".into());
        }
        journal.redo = true;
        journal.files_done = false;
        journal.conversation_done = false;
        self.save_journal(&journal)?;
        self.journals.insert(journal.id.clone(), journal.clone());
        for index in (0..journal.operations.len()).rev() {
            let op = journal.operations[index].clone();
            if !op.applied {
                continue;
            }
            let root = root.as_ref().expect("operations have a held root");
            if !allowed(&journal.root, &op.path) {
                result.failed.push(op.path.clone());
                continue;
            }
            let now = match current(root, &op.path) {
                Ok(now) => now,
                Err(_) => {
                    result.failed.push(op.path.clone());
                    continue;
                }
            };
            let conflict = match &now {
                Some((id, bytes, _)) => {
                    op.after_identity.as_ref() != Some(id) || op.after.as_ref().is_none_or(|p| !self.matches(&journal.folder, p, bytes))
                }
                None => op.after.is_some(),
            };
            if conflict && !request.saved_versions.contains(&op.path) {
                result.kept.push(op.path.clone());
                continue;
            }
            // Save even a conflicting version before replacing or removing it.
            if let Some((id, bytes, protection)) = &now {
                match self.save_bytes(&journal.folder, bytes, protection.clone(), id.clone()) {
                    Ok(image) => journal.retained.push((op.path.clone(), image)),
                    Err(_) => {
                        result.failed.push(op.path.clone());
                        continue;
                    }
                }
            }
            if self.save_journal(&journal).is_err() {
                result.failed.push(op.path.clone());
                continue;
            }
            self.journals.insert(journal.id.clone(), journal.clone());
            let desired = match &op.before {
                Some(image) => match self.blob(&journal.folder, image) {
                    Ok(b) => Some(b),
                    Err(_) => {
                        result.failed.push(op.path.clone());
                        continue;
                    }
                },
                None => None,
            };
            let expected = now.as_ref().map(|(id, bytes, _)| (id.as_slice(), bytes.as_slice()));
            let desired = desired.as_ref().zip(op.before.as_ref()).map(|(bytes, image)| (bytes.as_slice(), image.protection.as_slice()));
            let name = format!(".faber-rewind-{}-redo-{index}", journal.id);
            let (temp_path, _) = swap_paths(&op.path, &name);
            journal.operations[index].temporary = op.before.as_ref().map(|_| TemporaryFile { path: temp_path, identity: None });
            if self.save_journal(&journal).is_err() {
                result.failed.push(op.path.clone());
                continue;
            }
            journal.operations[index].displaced = None;
            journal.operations[index].installing = false;
            let mutation = root.mutate_staged(&op.path, expected, desired, &name, |stage, file| {
                self.record_swap(&mut journal, index, &name, stage, file).map_err(io::Error::other)
            });
            match mutation {
                Ok(()) => {
                    journal.operations[index].applied = false;
                    if self.save_journal(&journal).is_err() {
                        result.recovery = true;
                    }
                    self.journals.insert(journal.id.clone(), journal.clone());
                    result.restored.push(op.path.clone());
                }
                Err(_) => result.failed.push(op.path.clone()),
            }
        }
        journal.partial |= !result.kept.is_empty() || !result.failed.is_empty();
        journal.files_done = self.recover_temporaries(&mut journal).is_ok();
        if !journal.files_done {
            result.recovery = true;
        }
        if self.save_journal(&journal).is_ok() && journal.files_done {
            journal.before.apply(conversation);
        } else {
            journal.files_done = false;
            result.recovery = true;
        }
        result.partial = journal.partial || result.recovery || !result.kept.is_empty() || !result.failed.is_empty();
        self.journals.insert(journal.id.clone(), journal);
        Ok(result)
    }

    pub fn recovery_for(&self, conversation: &str) -> bool {
        self.journals.values().any(|j| j.conversation == conversation && !j.conversation_done)
    }

    pub fn recover_conversation(&mut self, c: &mut Conversation) -> Result<(), String> {
        let mut pending: Vec<_> = self.journals.values().filter(|j| j.conversation == c.id && !j.conversation_done).cloned().collect();
        pending.sort_by_key(|j| j.at);
        for mut journal in pending {
            self.recover_temporaries(&mut journal)?;
            // A crash can be between rename and journal acknowledgement. Preserve every image and stop admission until
            // recovery has classified it; an unexpected version is kept, never overwritten at startup.
            if !journal.operations.is_empty() {
                let root = Root::open(&journal.root).map_err(error)?;
                if root.identity().map_err(error)? != journal.root_identity {
                    return Err("error.rewind.folderChanged".into());
                }
                for op in &mut journal.operations {
                    if !allowed(&journal.root, &op.path) {
                        return Err("error.rewind.path".into());
                    }
                    let now = match current(&root, &op.path) {
                        Ok(now) => now,
                        Err(_) => {
                            journal.partial = true;
                            op.applied = false;
                            op.after_identity = None;
                            continue;
                        }
                    };
                    op.applied = match &now {
                        Some((_, bytes, _)) => op.after.as_ref().is_some_and(|p| self.matches(&journal.folder, p, bytes)),
                        None => op.after.is_none(),
                    };
                    // A recovered identity is unknown: redo asks before touching a file whose rename was not acknowledged.
                    if !journal.files_done {
                        op.after_identity = None;
                    }
                }
            }
            if journal.files_done {
                if journal.redo {
                    journal.before.apply(c);
                } else {
                    journal.after.apply(c);
                }
            } else {
                // Keep the durable cursor on a partial transaction. The saved versions stay available for redo.
                journal.after = SavedConversation::take(c);
            }
            journal.files_done = true;
            self.save_journal(&journal)?;
            self.journals.insert(journal.id.clone(), journal);
        }
        Ok(())
    }

    fn record_swap(
        &mut self,
        journal: &mut Journal,
        index: usize,
        temporary: &str,
        stage: SwapStage,
        file: &std::fs::File,
    ) -> Result<(), String> {
        let identity = file_identity(file).map_err(error)?;
        match stage {
            SwapStage::Prepared => {
                journal.operations[index].temporary.as_mut().expect("replacement has a journaled temporary").identity = Some(identity)
            }
            SwapStage::BeforeDisplace => {
                let (_, path) = swap_paths(&journal.operations[index].path, temporary);
                journal.operations[index].displaced = Some(TemporaryFile { path, identity: Some(identity) });
            }
            SwapStage::Displaced => {
                // Unix can move a newer name than the validated handle. Persist that version before rejecting it.
                if !(CapturePolicy { root: &journal.root }).allows_file(&journal.operations[index].path, file).map_err(error)? {
                    return Err("error.rewind.path".into());
                }
                let fingerprint = fingerprint(file).map_err(error)?;
                let mut held = file.try_clone().map_err(error)?;
                use std::io::Seek;
                held.rewind().map_err(error)?;
                let mut bytes = Vec::new();
                StableRead { file: &mut held, identity: &fingerprint }.take(FILE_LIMIT + 1).read_to_end(&mut bytes).map_err(error)?;
                if bytes.len() as u64 > FILE_LIMIT {
                    return Err("error.rewind.changed".into());
                }
                let image = self.save_bytes(&journal.folder, &bytes, protection(file).map_err(error)?, fingerprint)?;
                let displaced = journal.operations[index].displaced.as_mut().expect("displacement was journaled");
                displaced.identity = Some(identity);
                journal.retained.push((displaced.path.clone(), image));
            }
            SwapStage::BeforeInstall => journal.operations[index].installing = true,
        }
        self.save_journal(journal)?;
        self.journals.insert(journal.id.clone(), journal.clone());
        Ok(())
    }

    pub fn project_cleanup_roots(&self) -> Vec<PathBuf> {
        self.points
            .values()
            .map(|point| point.root.clone())
            .chain(self.journals.values().map(|journal| journal.root.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Called only after the engine has checked this root's admission and writers.
    pub fn cleanup_empty_project_quarantines(&self, path: &Path) -> Result<(), String> {
        let root = match Root::open(path) {
            Ok(root) => root,
            Err(_) => return Ok(()),
        };
        let identity = root.identity().map_err(error)?;
        if !self.points.values().any(|point| point.root == path && point.root_identity == identity)
            && !self.journals.values().any(|journal| journal.root == path && journal.root_identity == identity)
        {
            return Ok(());
        }
        root.recover_cleanup_tree().map_err(error)?;
        Ok(())
    }

    pub(super) fn inspect_project_cleanup(&self) -> Result<(), String> {
        let mut roots: BTreeMap<PathBuf, Vec<Vec<u8>>> = BTreeMap::new();
        for point in self.points.values() {
            roots.entry(point.root.clone()).or_default().push(point.root_identity.clone());
        }
        for journal in self.journals.values() {
            roots.entry(journal.root.clone()).or_default().push(journal.root_identity.clone());
        }
        let mut incomplete = false;
        for (path, identities) in roots {
            let root = match Root::open(&path) {
                Ok(root) => root,
                // A pending journal remains retryable when a removed or unavailable working folder returns.
                Err(_) => continue,
            };
            if !identities.contains(&root.identity().map_err(error)?) {
                continue;
            }
            for (retained, _) in root.inspect_cleanup_tree().map_err(error)? {
                if !self.journals.values().any(|journal| journal.root == path && Self::cleanup_original(journal, &retained).is_some()) {
                    // No journal supplies a safe original policy path: keep the bytes and make history unavailable.
                    incomplete = true;
                }
            }
        }
        if incomplete { Err("error.rewind.recovery".into()) } else { Ok(()) }
    }

    fn cleanup_original(journal: &Journal, retained: &str) -> Option<String> {
        let swap = Path::new(retained).parent().and_then(Path::parent);
        journal.operations.iter().enumerate().find_map(|(index, op)| {
            [format!(".faber-rewind-{}-{index}", journal.id), format!(".faber-rewind-{}-redo-{index}", journal.id)]
                .iter()
                .any(|name| {
                    let (prepared, _) = swap_paths(&op.path, name);
                    Path::new(&prepared).parent() == swap
                })
                .then(|| op.path.clone())
        })
    }

    fn recover_cleanup_file(&mut self, root: &Root, retained: &str, original: &str, journal: &mut Journal) -> Result<(), String> {
        let saved = root
            .open_removable(retained)
            .map_err(error)
            .and_then(|mut file| read_current(root, original, &mut file).map(|image| (file, image)))
            .and_then(|(file, (identity, bytes, security))| {
                self.save_bytes(&journal.folder, &bytes, security, identity).map(|image| (file, image))
            });
        match saved {
            Ok((file, image)) => {
                journal.retained.push((retained.into(), image));
                self.save_journal(journal)?;
                self.journals.insert(journal.id.clone(), journal.clone());
                self.remove_saved(root, retained, &file, journal)?;
            }
            Err(_) => {
                root.rename_new(retained, original).map_err(error)?;
                journal.partial = true;
                self.save_journal(journal)?;
                self.journals.insert(journal.id.clone(), journal.clone());
            }
        }
        Ok(())
    }

    fn recover_temporaries(&mut self, journal: &mut Journal) -> Result<(), String> {
        if journal.operations.is_empty() {
            return Ok(());
        }
        let root = Root::open(&journal.root).map_err(error)?;
        if root.identity().map_err(error)? != journal.root_identity {
            return Err("error.rewind.folderChanged".into());
        }
        // Project mutation stays behind the engine's writer/admission checks, including after a failed startup.
        for (retained, _) in root.recover_cleanup_tree().map_err(error)? {
            if let Some(original) = Self::cleanup_original(journal, &retained) {
                self.recover_cleanup_file(&root, &retained, &original, journal)?;
            } else if !self.journals.values().any(|other| other.root == journal.root && Self::cleanup_original(other, &retained).is_some())
            {
                return Err("error.rewind.recovery".into());
            }
        }
        for index in 0..journal.operations.len() {
            let op = journal.operations[index].clone();
            let base = if journal.redo {
                format!(".faber-rewind-{}-redo-{index}", journal.id)
            } else {
                format!(".faber-rewind-{}-{index}", journal.id)
            };
            let (prepared_path, displaced_path) = swap_paths(&op.path, &base);
            match root.validate_swap(&op.path, &base) {
                Ok(()) => (),
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(reason) => return Err(error(reason)),
            }
            for retained in root.recover_cleanup(&op.path, &base).map_err(error)? {
                self.recover_cleanup_file(&root, &retained, &op.path, journal)?;
            }
            if let Some(displaced) = &op.displaced {
                if !allowed(&journal.root, &displaced.path) || displaced.path != displaced_path {
                    return Err("error.rewind.path".into());
                }
                let opened = root.open_removable(&displaced.path);
                if !opened.as_ref().is_err_and(|e| e.kind() == io::ErrorKind::NotFound) {
                    let saved = match opened {
                        Ok(mut file) => read_current(&root, &displaced.path, &mut file).map(|image| (file, image)),
                        Err(reason) => Err(error(reason)),
                    }
                    .and_then(|(file, (identity, bytes, security))| {
                        // A crash can precede the displaced callback; save every actual version, even a new identity.
                        self.save_bytes(&journal.folder, &bytes, security, identity).map(|image| (file, image))
                    });
                    if let Ok((file, image)) = saved {
                        journal.retained.push((displaced.path.clone(), image));
                        self.save_journal(journal)?;
                        self.journals.insert(journal.id.clone(), journal.clone());
                        let desired = if journal.redo { op.before.as_ref() } else { op.after.as_ref() };
                        let mut returned = false;
                        if root.open_file(&op.path).is_err_and(|e| e.kind() == io::ErrorKind::NotFound)
                            && (desired.is_some() || !op.installing)
                        {
                            // Reverse an interrupted swap. No-replace leaves a concurrent creation intact.
                            returned = root.rename_new(&displaced.path, &op.path).is_ok();
                            if !returned && root.open_file(&op.path).is_err_and(|e| e.kind() == io::ErrorKind::NotFound) {
                                return Err("error.rewind.recovery".into());
                            }
                        }
                        if !returned {
                            self.remove_saved(&root, &displaced.path, &file, journal)?;
                        }
                    } else {
                        // Returning an uncapturable displaced version needs no content read and never replaces a save.
                        root.rename_new(&displaced.path, &op.path).map_err(error)?;
                        journal.partial = true;
                    }
                }
                journal.operations[index].displaced = None;
                self.save_journal(journal)?;
            }
            journal.operations[index].applied = match current(&root, &op.path) {
                Ok(Some((_, bytes, _))) => op.after.as_ref().is_some_and(|image| self.matches(&journal.folder, image, &bytes)),
                Ok(None) => op.after.is_none(),
                Err(_) => {
                    journal.partial = true;
                    false
                }
            };

            let Some(temp) = op.temporary else {
                root.finish_swap(&op.path, &base).map_err(error)?;
                continue;
            };
            if !allowed(&journal.root, &temp.path) || temp.path != prepared_path {
                return Err("error.rewind.path".into());
            }
            let mut file = match root.open_removable(&temp.path) {
                Ok(file) => file,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    journal.operations[index].temporary = None;
                    root.finish_swap(&op.path, &base).map_err(error)?;
                    continue;
                }
                Err(e) => return Err(error(e)),
            };
            let identity = file_identity(&file).map_err(error)?;
            if temp.identity.as_ref().is_some_and(|saved| saved != &identity) {
                continue;
            }
            // Creation can precede its identity receipt; preserve unexpected bytes before deleting the held file.
            let (expected, bytes, security) = read_current(&root, &temp.path, &mut file)?;
            // A partial write of the desired image adds no new version. Unexpected edits are saved before cleanup.
            let desired = if journal.redo { op.before.as_ref() } else { op.after.as_ref() };
            let known = desired.and_then(|image| self.blob(&journal.folder, image).ok()).is_some_and(|b| b.starts_with(&bytes));
            if !known {
                let image = self.save_bytes(&journal.folder, &bytes, security, expected.clone())?;
                journal.retained.push((temp.path.clone(), image));
                self.save_journal(journal)?;
                self.journals.insert(journal.id.clone(), journal.clone());
            }
            self.remove_saved(&root, &temp.path, &file, journal)?;
            drop(file);
            journal.operations[index].temporary = None;
            root.finish_swap(&op.path, &base).map_err(error)?;
        }
        Ok(())
    }

    fn remove_saved(&mut self, root: &Root, path: &str, file: &std::fs::File, journal: &mut Journal) -> Result<(), String> {
        if let Some(kept) = root.remove_preserved(path, file).map_err(error)? {
            journal.partial = true;
            // A replacement is kept on disk even when its new bytes cannot fit in history.
            if let Ok(Some((identity, bytes, security))) = current(root, &kept)
                && let Ok(image) = self.save_bytes(&journal.folder, &bytes, security, identity)
            {
                journal.retained.push((kept, image));
            }
            self.save_journal(journal)?;
            self.journals.insert(journal.id.clone(), journal.clone());
        }
        Ok(())
    }
}

#[cfg(test)]
mod validation_tests {
    use super::*;

    #[test]
    fn rewind_restore_and_redo_refuse_credentials_before_reading_bytes() {
        use std::io::Seek;
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("credentials.json"), b"fake credentials must never enter history").unwrap();
        let root = Root::open(folder.path()).unwrap();
        let mut held = root.open_file("credentials.json").unwrap();
        assert_eq!(read_current(&root, "credentials.json", &mut held).unwrap_err(), "error.rewind.path");
        assert_eq!(held.stream_position().unwrap(), 0, "policy denial precedes the first byte read");
        assert_eq!(current(&root, "credentials.json").unwrap_err(), "error.rewind.path");
    }

    #[cfg(windows)]
    #[test]
    fn rewind_restore_admits_the_opened_long_name_after_the_checked_name_changes() {
        use std::io::Seek;
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("ordinary.txt"), b"allowed contents").unwrap();
        std::fs::write(folder.path().join("credentials.json"), b"fake credentials").unwrap();
        let root = Root::open(folder.path()).unwrap();
        assert!(allowed(root.path(), "ordinary.txt"));
        // A rebound NTFS short name can now open this handle instead of the name admitted above.
        let mut rebound = root.open_file("credentials.json").unwrap();
        assert_eq!(read_current(&root, "ordinary.txt", &mut rebound).unwrap_err(), "error.rewind.path");
        assert_eq!(rebound.stream_position().unwrap(), 0);
        let mut ordinary = root.open_file("ordinary.txt").unwrap();
        assert_eq!(read_current(&root, "ordinary.txt", &mut ordinary).unwrap().1, b"allowed contents");
    }

    #[test]
    fn rewind_unfinished_redo_is_discoverable_even_without_files_done_or_live_expiry() {
        let data = tempfile::tempdir().unwrap();
        let mut history = History::open(data.path(), 1).unwrap();
        let mut conversation: Conversation = serde_json::from_value(serde_json::json!({
            "id": "3611dcb7-6387-47c6-8b25-1b6f20903171", "title": "Rewind", "area": "chat",
            "createdAt": 1, "updatedAt": 1, "messages": []
        }))
        .unwrap();
        conversation.messages.push(Message::new("cut".into(), crate::model::Role::User, "later".into(), 1));
        let request = RestoreRequest {
            conversation_id: conversation.id.clone(),
            message_id: "cut".into(),
            mode: Mode::Conversation,
            saved_versions: vec![],
        };
        let restored = history.restore(&request, &mut conversation, 2).unwrap();
        let journal = history.journals.get_mut(&restored.id).unwrap();
        journal.files_done = false;
        journal.redo = true;
        journal.expires = 0;
        let infos = history.redos(&conversation, 3);
        assert_eq!(infos.len(), 1);
        assert!(infos[0].recovery);
        let request = RedoRequest { conversation_id: conversation.id.clone(), id: restored.id, saved_versions: vec![] };
        assert!(history.redo_preview(&request, &conversation, 3).unwrap().recovery);
    }

    #[test]
    #[ignore = "manual component cost probe; no Git or CLI processes"]
    fn rewind_component_cost() {
        use sha2::{Digest, Sha256};
        use std::io::Write;
        let folder = tempfile::tempdir().unwrap();
        let mut data = vec![0u8; 16 * 1024];
        let mut seed = 123u64;
        for byte in &mut data {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            *byte = seed as u8;
        }
        for n in 0..1000 {
            std::fs::write(folder.path().join(format!("file-{n}")), &data).unwrap();
        }
        let root = Root::open(folder.path()).unwrap();
        let began = Instant::now();
        for n in 0..1000 {
            let path = format!("file-{n}");
            assert!(allowed(folder.path(), &path));
            assert!(!silently_excluded(folder.path(), &path));
            assert!(allowed(folder.path(), &path));
        }
        println!("rewind_components 1000: eligibility_ms={:.3}", began.elapsed().as_secs_f64() * 1000.0);
        let policy = CapturePolicy { root: root.path() };
        let began = Instant::now();
        for n in 0..1000 {
            let path = format!("file-{n}");
            assert!(policy.names_allow(&path));
            assert!(!nested_repository(root.path(), &path));
            assert!(policy.allows(&path, &|key| std::env::var(key).ok()));
        }
        println!("rewind_components 1000: pinned_eligibility_ms={:.3}", began.elapsed().as_secs_f64() * 1000.0);
        let began = Instant::now();
        let mut reusable = 0;
        for n in 0..1000 {
            let file = root.open_file(&format!("file-{n}")).unwrap();
            std::hint::black_box(fingerprint(&file).unwrap());
            std::hint::black_box(protection(&file).unwrap());
            std::hint::black_box(file.metadata().unwrap().len());
            reusable += usize::from(cache_fingerprint(&file).unwrap().is_some());
        }
        println!("rewind_components 1000: reuse_metadata_ms={:.3} reusable={reusable}", began.elapsed().as_secs_f64() * 1000.0);
        let began = Instant::now();
        let mut costs = [0u64; 6];
        for n in 0..1000 {
            let path = format!("file-{n}");
            let stage = Instant::now();
            assert!(policy.names_allow(&path));
            assert!(!nested_repository(root.path(), &path));
            costs[0] += micros(stage);
            let stage = Instant::now();
            let file = root.open_file(&path).unwrap();
            costs[1] += micros(stage);
            let stage = Instant::now();
            let resolved = resolved_path(&file).unwrap().unwrap_or_else(|| root.path().join(&path));
            costs[2] += micros(stage);
            let stage = Instant::now();
            assert!(policy.allows_resolved(&resolved, &|key| std::env::var(key).ok()));
            costs[3] += micros(stage);
            let stage = Instant::now();
            std::hint::black_box(capture_metadata(&file).unwrap());
            costs[4] += micros(stage);
            let stage = Instant::now();
            std::hint::black_box(protection(&file).unwrap());
            costs[5] += micros(stage);
        }
        println!(
            "rewind_components 1000: warm_validation_ms={:.3} stage_us(filter,open,resolved,policy,metadata,protection)={costs:?}",
            began.elapsed().as_secs_f64() * 1000.0
        );
        let began = Instant::now();
        for n in 0..1000 {
            let mut file = root.open_file(&format!("file-{n}")).unwrap();
            let identity = fingerprint(&file).unwrap();
            let _protect = protection(&file).unwrap();
            let mut bytes = Vec::new();
            StableRead { file: &mut file, identity: &identity }.read_to_end(&mut bytes).unwrap();
            assert_eq!(bytes, data);
        }
        println!("rewind_components 1000: validated_reads_ms={:.3}", began.elapsed().as_secs_f64() * 1000.0);
        let began = Instant::now();
        for _ in 0..1000 {
            let mut hash = Sha256::new();
            hash.update(b"blob 16384\0");
            hash.update(&data);
            std::hint::black_box(hash.finalize());
            let mut encoder = flate2::write::ZlibEncoder::new(std::io::sink(), flate2::Compression::fast());
            encoder.write_all(b"blob 16384\0").unwrap();
            encoder.write_all(&data).unwrap();
            encoder.finish().unwrap();
        }
        println!("rewind_components 1000: hash_compress_ms={:.3}", began.elapsed().as_secs_f64() * 1000.0);
        let began = Instant::now();
        for _ in 0..1000 {
            let mut hash = Sha256::new();
            hash.update(b"blob 16384\0");
            hash.update(&data);
            std::hint::black_box(hash.finalize());
            let mut encoder = flate2::write::ZlibEncoder::new(std::io::sink(), flate2::Compression::none());
            encoder.write_all(b"blob 16384\0").unwrap();
            encoder.write_all(&data).unwrap();
            encoder.finish().unwrap();
        }
        println!("rewind_components 1000: hash_store_ms={:.3}", began.elapsed().as_secs_f64() * 1000.0);
        let began = Instant::now();
        for n in 0..1000 {
            let mut file = std::fs::File::create(folder.path().join(format!("object-{n}"))).unwrap();
            file.write_all(&data).unwrap();
            file.sync_all().unwrap();
        }
        println!("rewind_components 1000: create_write_flush_ms={:.3}", began.elapsed().as_secs_f64() * 1000.0);
        for n in 0u64..1000 {
            let mut unique = data.clone();
            unique[..8].copy_from_slice(&n.to_le_bytes());
            std::fs::write(folder.path().join(format!("file-{n}")), unique).unwrap();
        }
        let reading = || {
            let mut system = sysinfo::System::new();
            let pid = sysinfo::Pid::from_u32(std::process::id());
            system.refresh_processes_specifics(
                sysinfo::ProcessesToUpdate::Some(&[pid]),
                true,
                sysinfo::ProcessRefreshKind::nothing().with_memory().with_cpu(),
            );
            let process = system.process(pid).unwrap();
            (process.accumulated_cpu_time(), process.memory())
        };
        let (cpu_before, rss_before) = reading();
        let began = Instant::now();
        let mut batch = pack::Batch::new(root.private_create("payload.pack").unwrap()).unwrap();
        for n in 0..1000 {
            let mut file = root.open_file(&format!("file-{n}")).unwrap();
            let identity = fingerprint(&file).unwrap();
            batch.add(&mut StableRead { file: &mut file, identity: &identity }, data.len() as u64).unwrap();
        }
        assert_eq!(batch.count(), 1000);
        let (payload, _, index) = batch.finish().unwrap();
        payload.sync_all().unwrap();
        let wall = began.elapsed();
        let (cpu_after, rss_after) = reading();
        println!(
            "rewind_components 1000: private_batch_payload_ms={:.3} process_cpu_ms={} resident_before_mib={:.3} resident_after_mib={:.3} payload_bytes={} index_bytes={} payload_flushes=1; excludes point manifest/publication, RSS is not a peak",
            wall.as_secs_f64() * 1000.0,
            cpu_after.saturating_sub(cpu_before),
            rss_before as f64 / (1024.0 * 1024.0),
            rss_after as f64 / (1024.0 * 1024.0),
            payload.metadata().unwrap().len(),
            index.len()
        );
    }

    #[test]
    fn rewind_relocated_credential_aliases_are_refused_before_content_reads() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir(folder.path().join("sub")).unwrap();
        std::fs::write(folder.path().join("ordinary"), b"fake credentials").unwrap();
        let aliased = folder.path().join("sub/../ordinary").to_string_lossy().into_owned();
        assert!(!allowed_with(folder.path(), "ordinary", &|key| (key == "AWS_SHARED_CREDENTIALS_FILE").then(|| aliased.clone())));
        assert!(allowed_with(folder.path(), "source.rs", &|_| None));
        let root = Root::open(folder.path()).unwrap();
        let policy = CapturePolicy { root: root.path() };
        assert!(!policy.allows("ordinary", &|key| (key == "AWS_SHARED_CREDENTIALS_FILE").then(|| aliased.clone())));
        assert!(policy.allows("source.rs", &|_| None));
        for secret in ["../outside", ".git/config", "sub/.env", "sub/auth.json", ".ssh/key", "sub/source.key"] {
            assert!(!policy.allows(secret, &|_| None), "{secret}");
        }
        // Environment changes are checked again even when a source image could otherwise be reused.
        assert!(policy.allows("ordinary", &|_| None));
        assert!(!policy.allows("ordinary", &|key| (key == "AWS_SHARED_CREDENTIALS_FILE").then(|| aliased.clone())));
        assert!(policy.can_reuse_admission("ordinary", &|_| None));
        assert!(!policy.can_reuse_admission("sub/ordinary", &|_| None), "an intermediate directory can change names independently");
        assert!(
            !policy.can_reuse_admission("ordinary", &|key| (key == "AWS_SHARED_CREDENTIALS_FILE").then(|| aliased.clone())),
            "a newly relocated credential must invalidate the earlier admission even if the source generation stayed equal"
        );
    }

    #[test]
    fn rewind_excluded_prefixes_never_become_proven_absence() {
        let mut before = Snapshot { complete: true, ..Snapshot::default() };
        before.excluded.insert("cache".into());
        assert!(before.known("cache/file").is_none());
        assert!(before.known("cache").is_none());
        assert!(matches!(before.known("cache-other"), Some(None)));
        let log_root = Path::new("project/.faber/verify");
        assert!(!CapturePolicy { root: log_root }.names_allow("output.log"));
    }
}

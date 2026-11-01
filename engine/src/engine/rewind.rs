//! Turn admission and private history work. The engine lock never holds a disk scan or a restore.
use super::*;
use crate::rewind::{Coverage, Cursor, History, PointInfo, Preview, RedoInfo, RedoRequest, RestoreRequest, RestoreResult, Running};
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
mod retire_tests;
mod steer;

pub(super) struct State {
    history: Option<Arc<Mutex<History>>>,
    worker: Arc<AtomicBool>,
    attempted: HashSet<String>,
    pending: HashSet<String>,
    preparing_steers: HashMap<String, String>,
    steers: HashMap<String, PointInfo>,
    writers: HashMap<String, (Vec<String>, bool)>,
    session_writers: HashMap<(String, u64), (Vec<String>, bool)>,
    retired_sessions: HashMap<(String, u64), (Vendor, Session)>,
    session_folders: HashMap<(String, u64), PathBuf>,
    starting_sessions: HashSet<(String, u64)>,
    exited_sessions: HashSet<(String, u64)>,
    // Deleting the visible conversation does not terminate its writers; None keeps an unlocated writer unknown.
    deleted_folders: HashMap<String, Option<PathBuf>>,
    cursors: HashMap<(String, String), Cursor>,
    seen_writers: HashMap<String, HashSet<String>>,
    admission: Vec<PathBuf>,
    admission_changed: Arc<std::sync::Condvar>,
    sealing: HashSet<PathBuf>,
    deferred_seals: HashSet<String>,
    revisions: HashMap<String, u64>,
    recovery_failed: bool,
    helper_writers: HashMap<(String, u64), (String, Vec<String>, bool)>,
    helper_folders: HashMap<(String, u64), PathBuf>,
    helper_sessions: HashMap<(String, u64), Session>,
    unconfirmed_checks: HashSet<String>,
}

impl State {
    #[cfg(test)]
    pub(super) fn captured_for_test(&mut self, reply: &str) {
        self.attempted.insert(reply.into());
    }
    fn observe_writers(&mut self, conversation: &str, sid: u64, active: &[String], unknown: bool) {
        let key = (conversation.to_string(), sid);
        if active.is_empty() && !unknown {
            self.session_writers.remove(&key);
        } else {
            self.session_writers.insert(key, (active.to_vec(), unknown));
        }
        self.seen_writers.entry(conversation.into()).or_default().extend(active.iter().cloned());
        self.refresh_writers(conversation);
    }

    fn refresh_writers(&mut self, conversation: &str) {
        let mut combined = Vec::new();
        let mut unclear = false;
        for ((conv, _), (writers, unknown)) in &self.session_writers {
            if conv == conversation {
                combined.extend(writers.iter().cloned());
                unclear |= unknown;
            }
        }
        if self.starting_sessions.iter().any(|(conv, _)| conv == conversation) {
            combined.push("startup".into());
            unclear = true;
        }
        if self.retired_sessions.keys().any(|(conv, _)| conv == conversation) {
            combined.push("session-closing".into());
            unclear = true;
        }
        combined.sort_unstable();
        combined.dedup();
        if self.writers.get(conversation).is_none_or(|(old, unknown)| old != &combined || *unknown != unclear) {
            self.bump(conversation);
            self.writers.insert(conversation.into(), (combined, unclear));
        }
    }

    pub(super) fn bump(&mut self, conversation: &str) {
        let generation = self.revisions.entry(conversation.into()).or_default();
        *generation = generation.wrapping_add(1);
    }
    pub(super) fn new(data: &Path) -> Self {
        let opened = History::open(data, now());
        let recovery_failed = opened.is_err() && pending_recovery(data);
        Self {
            history: opened.ok().map(|h| Arc::new(Mutex::new(h))),
            worker: Arc::new(AtomicBool::new(false)),
            attempted: HashSet::new(),
            pending: HashSet::new(),
            writers: HashMap::new(),
            session_writers: HashMap::new(),
            retired_sessions: HashMap::new(),
            session_folders: HashMap::new(),
            starting_sessions: HashSet::new(),
            exited_sessions: HashSet::new(),
            deleted_folders: HashMap::new(),
            cursors: HashMap::new(),
            preparing_steers: HashMap::new(),
            steers: HashMap::new(),
            admission: Vec::new(),
            admission_changed: Arc::new(std::sync::Condvar::new()),
            sealing: HashSet::new(),
            deferred_seals: HashSet::new(),
            revisions: HashMap::new(),
            recovery_failed,
            helper_writers: HashMap::new(),
            helper_folders: HashMap::new(),
            helper_sessions: HashMap::new(),
            unconfirmed_checks: HashSet::new(),
            seen_writers: HashMap::new(),
        }
    }
}

/// A damaged/unreadable journal must never turn into ordinary capture-unavailable admission.
fn pending_recovery(data: &Path) -> bool {
    let root = data.join("rewind");
    match root.symlink_metadata() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return false,
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
        _ => return true,
    }
    let Ok(folders) = std::fs::read_dir(&root) else {
        return true;
    };
    for entry in folders {
        let Ok(entry) = entry else {
            return true;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.len() != 64 || !name.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        if !entry.file_type().is_ok_and(|m| m.is_dir() && !m.is_symlink()) {
            return true;
        }
        let dir = entry.path().join("journals");
        match dir.symlink_metadata() {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
            _ => return true,
        }
        let Ok(journals) = std::fs::read_dir(dir) else {
            return true;
        };
        for journal in journals {
            let Ok(journal) = journal else {
                return true;
            };
            if journal.file_name().to_str().and_then(|n| n.strip_suffix(".json")).is_some_and(|id| Uuid::parse_str(id).is_ok()) {
                return true;
            }
        }
    }
    false
}

fn overlap(a: &Path, b: &Path) -> bool {
    // These paths have already been resolved by cwd_of; Windows comparisons also ignore case.
    let normal = |p: &Path| {
        let raw = p.to_string_lossy().replace('\\', "/");
        let raw = raw.strip_prefix("//?/").unwrap_or(&raw).trim_end_matches('/').to_string();
        if cfg!(windows) { raw.to_lowercase() } else { raw }
    };
    let (a, b) = (normal(a), normal(b));
    a == b || a.strip_prefix(&b).is_some_and(|p| p.starts_with('/')) || b.strip_prefix(&a).is_some_and(|p| p.starts_with('/'))
}

struct Admission {
    engine: Engine,
    folder: PathBuf,
}
fn same_folder(current: &Path, saved: Option<&Path>) -> Result<(), String> {
    let saved = saved.ok_or("error.rewind.unavailable")?;
    if current.canonicalize().ok().zip(saved.canonicalize().ok()).is_some_and(|(a, b)| a == b) {
        Ok(())
    } else {
        Err("error.rewind.folderChanged".into())
    }
}
impl Drop for Admission {
    fn drop(&mut self) {
        self.engine.with(|c| {
            c.rewind.admission.retain(|p| p != &self.folder);
            c.rewind.admission_changed.notify_all();
        });
    }
}

impl Engine {
    /// A capture may hold history for a disk scan; deletion waits without holding the engine lock.
    pub(super) fn with_rewind_delete<T>(&self, id: &str, mut action: impl FnMut(&mut Core) -> Result<T, String>) -> Result<T, String> {
        let _inside = self.runtime.enter();
        let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            let result = core.rewind_delete_allowed(id).and_then(|()| action(&mut core));
            if !matches!(&result, Err(error) if error == "error.rewind.busy") {
                return result;
            }
            if core.rewind_delete_admitted(id) {
                let changed = core.rewind.admission_changed.clone();
                core = changed.wait(core).unwrap_or_else(|e| e.into_inner());
            } else {
                let history = core.rewind.history.clone().expect("a busy history check has its worker");
                drop(core);
                drop(history.lock().unwrap_or_else(|e| e.into_inner()));
                core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            }
        }
    }

    /// Startup recovery runs before keep-going, queue or PR-watch admission. It never replays a command.
    pub fn recover_rewind(&self) {
        let _ = self.retry_rewind_recovery(None);
    }

    pub fn rewind_recover(&self, id: &str) -> Result<(), String> {
        self.retry_rewind_recovery(Some(id))
    }

    fn retry_rewind_recovery(&self, requested: Option<&str>) -> Result<(), String> {
        self.with(|c| {
            if let Some(id) = requested {
                c.load(id).ok_or("error.conversationGone")?;
            }
            if c.rewind.history.is_none() {
                match History::open(&c.opts.data_dir, now()) {
                    Ok(history) => c.rewind.history = Some(Arc::new(Mutex::new(history))),
                    Err(_) => {
                        c.rewind.recovery_failed |= pending_recovery(&c.opts.data_dir);
                        return Err("error.rewind.recovery".into());
                    }
                }
            }
            let history = c.rewind.history.clone().ok_or("error.rewind.recovery")?;
            let mut history = history.lock().unwrap_or_else(|e| e.into_inner());
            for root in history.project_cleanup_roots() {
                if !c.rewind.admission.iter().any(|folder| overlap(folder, &root)) && c.rewind_running(&root).is_empty() {
                    history.cleanup_empty_project_quarantines(&root)?;
                }
            }
            let ids = history.recovery_conversations();
            if ids.is_empty() && !c.rewind.recovery_failed {
                return Ok(());
            }
            c.rewind.recovery_failed |= !ids.is_empty();
            // Hold engine admission closed while retrying every pending journal, just as at startup.
            for id in &ids {
                let conversation = c.load(id).ok_or("error.rewind.recovery")?.clone();
                let mut folders: Vec<PathBuf> = c.cwd_of(&conversation).ok().into_iter().collect();
                folders.extend(
                    history.pending_acknowledgements(id).iter().filter_map(|journal| history.redo_root(journal).map(Path::to_path_buf)),
                );
                if folders.is_empty() {
                    return Err("error.rewind.recovery".into());
                }
                for folder in folders {
                    if c.rewind.admission.iter().any(|p| overlap(p, &folder)) {
                        return Err("error.rewind.busy".into());
                    }
                    if !c.rewind_running(&folder).is_empty() {
                        return Err("error.rewind.running".into());
                    }
                }
            }
            let mut failed = false;
            for id in ids {
                c.keep_going_off(&id);
                // Recovery rereads the file, so helper activity its record lacks is written first.
                let _ = c.flush_transcript(&id);
                let Some(mut conversation) = c.store.get(&id) else {
                    failed = true;
                    continue;
                };
                if history.recover_conversation(&mut conversation).is_err() || c.store.save_checked(&conversation).is_err() {
                    failed = true;
                    continue;
                }
                for journal in history.pending_acknowledgements(&id) {
                    if history.acknowledge(&journal).is_err() {
                        failed = true;
                    }
                }
                c.compacts.remove(&id);
                c.rewind.bump(&id);
                c.rewind.writers.remove(&id);
                c.pending.remove(&id);
                c.convs.insert(id, conversation);
            }
            c.rewind.recovery_failed = failed;
            c.list_changed();
            if failed { Err("error.rewind.recovery".into()) } else { Ok(()) }
        })
    }

    pub fn rewind_redos(&self, id: &str) -> Result<Vec<RedoInfo>, String> {
        let (conversation, history, recovery) = self.with(|c| {
            c.load(id).ok_or("error.conversationGone")?;
            Ok::<_, String>((c.convs[id].clone(), c.rewind.history.clone(), c.rewind.recovery_failed))
        })?;
        let mut infos = history.map(|h| h.lock().unwrap_or_else(|e| e.into_inner()).redos(&conversation, now())).unwrap_or_default();
        if recovery && !infos.iter().any(|info| info.recovery) {
            infos.insert(
                0,
                RedoInfo { id: String::new(), message_id: String::new(), mode: crate::rewind::Mode::Both, expires_at: 0, recovery: true },
            );
        }
        Ok(infos)
    }

    pub fn rewind_redo_preview(&self, request: RedoRequest) -> Result<Preview, String> {
        let (conversation, history, running, recovery, folder) = self.with(|c| {
            c.load(&request.conversation_id).ok_or("error.conversationGone")?;
            let conversation = c.convs[&request.conversation_id].clone();
            let folder = c.cwd_of(&conversation)?;
            Ok::<_, String>((conversation, c.rewind.history.clone(), c.rewind_running(&folder), c.rewind.recovery_failed, folder))
        })?;
        if request.id.is_empty() && recovery {
            return Ok(Preview {
                recovery: true,
                running,
                coverage: Coverage { incomplete: true, ..Coverage::default() },
                notice: "note.rewind.oldHandle".into(),
                ..Preview::default()
            });
        }
        let history = history.ok_or("error.rewind.unavailable")?;
        let history = history.lock().unwrap_or_else(|e| e.into_inner());
        let mut preview = history.redo_preview(&request, &conversation, now())?;
        if !preview.recovery
            && let Some(root) = history.redo_root(&request.id)
        {
            same_folder(&folder, Some(root))?;
        }
        preview.running = running;
        preview.recovery |= recovery;
        Ok(preview)
    }

    pub fn rewind_points(&self, id: &str) -> Result<Vec<PointInfo>, String> {
        let (mut infos, history) = self.with(|c| {
            c.load(id).ok_or("error.conversationGone")?;
            Ok::<_, String>((c.convs[id].messages.iter().filter_map(|m| m.rewind.clone()).collect::<Vec<_>>(), c.rewind.history.clone()))
        })?;
        if let Some(history) = history {
            let stored = history.lock().unwrap_or_else(|e| e.into_inner()).list(id);
            for info in &mut infos {
                if let Some(current) = stored.iter().find(|p| p.id == info.id) {
                    *info = current.clone();
                } else {
                    info.coverage.unavailable = true;
                    info.coverage.incomplete = true;
                }
            }
        }
        Ok(infos)
    }

    pub fn rewind_preview(&self, request: RestoreRequest) -> Result<Preview, String> {
        let (history, running, messages, recovery, folder, markers) = self.with(|c| {
            c.load(&request.conversation_id).ok_or("error.conversationGone")?;
            let conversation = &c.convs[&request.conversation_id];
            let cut = conversation
                .messages
                .iter()
                .position(|m| m.id == request.message_id && m.role == Role::User)
                .ok_or("error.rewind.message")?;
            let folder = c.cwd_of(conversation)?;
            Ok::<_, String>((
                c.rewind.history.clone(),
                c.rewind_running(&folder),
                if request.mode.messages() { conversation.messages.len() - cut } else { 0 },
                c.rewind.recovery_failed,
                folder,
                conversation.messages[cut..].iter().filter_map(|m| m.rewind.clone()).collect::<Vec<_>>(),
            ))
        })?;
        let mut gap = markers.iter().any(|p| p.coverage.unavailable);
        let mut preview = match history {
            Some(history) => {
                let history = history.lock().unwrap_or_else(|e| e.into_inner());
                gap |= markers.iter().any(|p| history.missing_point(p));
                if request.mode.files() {
                    match history.point_root(&request.conversation_id, &request.message_id) {
                        Some(root) => same_folder(&folder, Some(root))?,
                        None if !request.mode.messages() => return Err("error.rewind.unavailable".into()),
                        None => (),
                    }
                }
                history.preview(&request)
            }
            None if request.mode.messages() => Ok(Preview {
                coverage: Coverage { unavailable: request.mode.files(), incomplete: request.mode.files(), ..Coverage::default() },
                ..Preview::default()
            }),
            None => Err("error.rewind.unavailable".into()),
        };
        if request.mode == crate::rewind::Mode::Conversation && preview.is_err() {
            preview = Ok(Preview::default());
        }
        let mut preview = preview?;
        preview.running = running;
        preview.messages = messages;
        preview.recovery |= recovery;
        if request.mode.files() && gap {
            preview.coverage.incomplete = true;
            preview.coverage.unavailable = true;
            for file in &mut preview.files {
                file.conflict = true;
            }
            preview.conflicts = preview.files.len();
        }
        preview.notice = "note.rewind.oldHandle".into();
        Ok(preview)
    }

    fn rewind_admission(&self, id: &str) -> Result<(Admission, Arc<Mutex<History>>, Conversation), String> {
        self.with(|c| {
            c.load(id).ok_or("error.conversationGone")?;
            let conversation = c.convs[id].clone();
            let folder = c.cwd_of(&conversation)?;
            if c.rewind.recovery_failed {
                return Err("error.rewind.recovery".into());
            }
            if c.rewind.admission.iter().any(|p| overlap(p, &folder)) {
                return Err("error.rewind.busy".into());
            }
            // Closing admission and checking all overlapping writers share one engine lock.
            c.rewind.admission.push(folder.clone());
            if !c.rewind_running(&folder).is_empty() {
                c.rewind.admission.retain(|p| p != &folder);
                return Err("error.rewind.running".into());
            }
            let Some(history) = c.rewind.history.clone() else {
                c.rewind.admission.retain(|p| p != &folder);
                return Err("error.rewind.unavailable".into());
            };
            Ok((Admission { engine: self.clone(), folder }, history, conversation))
        })
    }

    fn finish_rewind(
        &self,
        id: &str,
        conversation: &Conversation,
        result: &mut RestoreResult,
        history: &Arc<Mutex<History>>,
    ) -> Result<(), String> {
        let saved = self.with(|c| {
            c.keep_going_off(id);
            c.compacts.remove(id);
            let actual = c.convs.get_mut(id).ok_or("error.conversationGone")?;
            crate::rewind::SavedConversation::take(conversation).apply(actual);
            actual.updated_at = now();
            c.store.save_checked(actual).map_err(|_| "error.rewind.storage")?;
            c.rewind.bump(id);
            c.rewind.writers.remove(id);
            c.pending.remove(id);
            c.list_changed();
            c.refresh_running();
            Ok::<_, String>(())
        });
        if let Err(error) = saved {
            self.with(|c| c.rewind.recovery_failed = true);
            result.recovery = true;
            return Err(error);
        }
        if history.lock().unwrap_or_else(|e| e.into_inner()).acknowledge(&result.id).is_err() {
            result.recovery = true;
            result.partial = true;
            result.notice = Some("note.rewind.recovery".into());
            self.with(|c| c.rewind.recovery_failed = true);
        }
        Ok(())
    }

    pub fn rewind_restore(&self, request: RestoreRequest) -> Result<RestoreResult, String> {
        let (guard, history, mut conversation) = self.rewind_admission(&request.conversation_id)?;
        let mut result = {
            let mut history = history.lock().unwrap_or_else(|e| e.into_inner());
            if request.mode.files() {
                match history.point_root(&request.conversation_id, &request.message_id) {
                    Some(root) => same_folder(&guard.folder, Some(root))?,
                    None if !request.mode.messages() => return Err("error.rewind.unavailable".into()),
                    None => (),
                }
            }
            history.restore(&request, &mut conversation, now())?
        };
        if self.finish_rewind(&request.conversation_id, &conversation, &mut result, &history).is_err() {
            result.recovery = true;
            result.partial = true;
            result.notice = Some("note.rewind.recovery".into());
        }
        Ok(result)
    }

    pub fn rewind_redo(&self, request: RedoRequest) -> Result<RestoreResult, String> {
        let (guard, history, mut conversation) = self.rewind_admission(&request.conversation_id)?;
        let mut result = {
            let mut history = history.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(root) = history.redo_root(&request.id) {
                same_folder(&guard.folder, Some(root))?;
            }
            history.redo(&request, &mut conversation, now())?
        };
        if self.finish_rewind(&request.conversation_id, &conversation, &mut result, &history).is_err() {
            result.recovery = true;
            result.partial = true;
            result.notice = Some("note.rewind.recovery".into());
        }
        Ok(result)
    }
}

impl Core {
    fn rewind_delete_admitted(&self, id: &str) -> bool {
        if self.rewind.admission.is_empty() {
            return false;
        }
        let conversation = self.convs.get(id).cloned().or_else(|| self.store.get(id));
        if conversation.is_none() && !self.rewind.deleted_folders.contains_key(id) {
            return false;
        }
        let folder =
            conversation.as_ref().and_then(|c| self.cwd_of(c).ok()).or_else(|| self.rewind.deleted_folders.get(id).cloned().flatten());
        self.rewind.admission.iter().any(|root| folder.as_ref().is_none_or(|folder| overlap(root, folder)))
    }

    pub(super) fn rewind_delete_allowed(&self, id: &str) -> Result<(), String> {
        if self.rewind_delete_admitted(id) {
            return Err("error.rewind.busy".into());
        }
        let Some(history) = &self.rewind.history else {
            return if self.rewind.recovery_failed { Err("error.rewind.deleteRecovery".into()) } else { Ok(()) };
        };
        let history = match history.try_lock() {
            Ok(history) => history,
            Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return Err("error.rewind.busy".into()),
        };
        if history.pending_acknowledgements(id).is_empty() { Ok(()) } else { Err("error.rewind.deleteRecovery".into()) }
    }

    pub(super) fn rewind_checks_stopping(&mut self, conv: &str) {
        self.rewind.unconfirmed_checks.insert(conv.into());
        self.rewind.bump(conv);
    }
    pub(super) fn rewind_checks_ended(&mut self, conv: &str) {
        if self.rewind.unconfirmed_checks.remove(conv) {
            self.rewind.bump(conv);
        }
        self.try_seal_rewind(conv);
    }
    fn rewind_revision(&self, root: &Path) -> BTreeMap<String, u64> {
        self.convs
            .iter()
            .filter(|(_, c)| self.cwd_of(c).is_ok_and(|p| overlap(&p, root)))
            .map(|(id, _)| (id.clone(), self.rewind.revisions.get(id).copied().unwrap_or(0)))
            .chain(
                self.rewind
                    .session_folders
                    .iter()
                    .filter(|(_, folder)| overlap(folder, root))
                    .map(|((id, _), _)| (id.clone(), self.rewind.revisions.get(id).copied().unwrap_or(0))),
            )
            .chain(self.rewind.helper_folders.iter().filter(|(_, folder)| overlap(folder, root)).filter_map(|(key, _)| {
                self.rewind.helper_writers.get(key).map(|(id, _, _)| (id.clone(), self.rewind.revisions.get(id).copied().unwrap_or(0)))
            }))
            .chain(
                self.rewind
                    .deleted_folders
                    .iter()
                    .filter(|(_, folder)| folder.as_ref().is_none_or(|p| overlap(p, root)))
                    .map(|(id, _)| (id.clone(), self.rewind.revisions.get(id).copied().unwrap_or(0))),
            )
            .collect()
    }
    pub(super) fn rewind_blocked(&self, id: &str) -> bool {
        if self.rewind.recovery_failed {
            return true;
        }
        self.convs.get(id).and_then(|c| self.cwd_of(c).ok()).is_some_and(|folder| self.rewind.admission.iter().any(|p| overlap(p, &folder)))
    }

    fn rewind_running(&self, folder: &Path) -> Vec<Running> {
        let mut running = Vec::new();
        let owners: HashSet<&String> = self
            .convs
            .keys()
            .chain(self.rewind.deleted_folders.keys())
            .chain(self.rewind.session_folders.keys().map(|(owner, _)| owner))
            .chain(self.rewind.helper_writers.values().map(|(owner, _, _)| owner))
            .collect();
        for id in owners {
            let c = self.convs.get(id);
            let root = c.and_then(|c| self.cwd_of(c).ok()).or_else(|| self.rewind.deleted_folders.get(id).cloned().flatten());
            let owned_overlap = self.rewind.session_folders.iter().any(|((owner, _), root)| owner == id && overlap(root, folder))
                || self.rewind.helper_folders.iter().any(|(key, root)| {
                    overlap(root, folder) && self.rewind.helper_writers.get(key).is_some_and(|(owner, _, _)| owner == id)
                });
            let located = root.is_some()
                || self.rewind.session_folders.keys().any(|(owner, _)| owner == id)
                || self
                    .rewind
                    .helper_folders
                    .keys()
                    .any(|key| self.rewind.helper_writers.get(key).is_some_and(|(owner, _, _)| owner == id));
            if located && !root.as_ref().is_some_and(|root| overlap(root, folder)) && !owned_overlap {
                continue;
            }
            let mut writers = self.rewind.writers.get(id).map(|(w, _)| w.clone()).unwrap_or_default();
            let mut unknown = self.rewind.writers.get(id).is_some_and(|(_, unknown)| *unknown);
            if self.live.get(id).is_some_and(|l| l.turn.is_some()) {
                writers.push("turn".into());
            }
            if self.live.get(id).is_some_and(|l| !l.steering.is_empty()) {
                writers.push("steer".into());
            }
            if self.copying.contains_key(id)
                || self.rewind.pending.contains(id)
                || self.rewind.preparing_steers.values().any(|conv| conv == id)
            {
                writers.push("admission".into());
            }
            if c.is_some_and(|c| c.compacting) {
                writers.push("compaction".into());
            }
            if self.helpers_in(id) {
                writers.push("helpers".into());
            }
            for ((call, sid), (_, active, unclear)) in self.rewind.helper_writers.iter().filter(|(_, (conv, _, _))| conv == id) {
                writers.extend(active.iter().map(|w| format!("helper:{w}")));
                unknown |= *unclear;
                if !self.helpers.get(call).is_some_and(|h| h.hsid == *sid) {
                    writers.push("helper:session-closing".into());
                    unknown = true;
                }
            }
            // A verifier can still be draining a stopped command after the visible checks row disappears.
            if self.verifier.as_ref().is_some_and(|v| v.active_in(folder)) {
                writers.push("checks".into());
                unknown |= self.verifier.as_ref().is_some_and(|v| v.unknown_in(folder));
            }
            if self.rewind.unconfirmed_checks.contains(id) {
                writers.push("checks".into());
                unknown = true;
            }
            if !writers.is_empty() || unknown {
                unknown |= !located;
                running.push(Running { conversation_id: id.clone(), writers, unknown });
            }
        }
        running
    }

    pub(super) fn rewind_forget(&mut self, conv: &str) {
        if let Some(c) = self.convs.get(conv) {
            self.rewind.deleted_folders.insert(conv.into(), self.cwd_of(c).ok());
            self.rewind.bump(conv);
        }
    }

    pub(super) fn rewind_writers_active(&self, conv: &str) -> bool {
        self.rewind.writers.get(conv).is_some_and(|(writers, unknown)| !writers.is_empty() || *unknown)
            || self.helpers_in(conv)
            || self.rewind.helper_writers.values().any(|(owner, _, _)| owner == conv)
            || self.rewind.unconfirmed_checks.contains(conv)
            || self.rewind.pending.contains(conv)
            || self.copying.contains_key(conv)
            || self.rewind.preparing_steers.values().any(|owner| owner == conv)
    }

    fn rewind_deleted_settled(&mut self, conv: &str) -> bool {
        if !self.rewind.deleted_folders.contains_key(conv) {
            return false;
        }
        if !self.rewind_writers_active(conv) {
            let folder = self.rewind.deleted_folders.remove(conv).flatten();
            let survivors: Vec<String> = self
                .convs
                .iter()
                .filter(|(_, c)| folder.as_ref().is_none_or(|folder| self.cwd_of(c).is_ok_and(|root| overlap(folder, &root))))
                .map(|(id, _)| id.clone())
                .collect();
            for id in survivors {
                self.try_seal_rewind(&id);
            }
        }
        true
    }

    pub(super) fn rewind_writing(&self, conv: &str) -> bool {
        // Closing markers block restore until exit is confirmed, but cannot keep their own shared server alive.
        self.live.get(conv).is_some_and(|l| self.rewind.starting_sessions.contains(&(conv.to_string(), l.sid)))
            || self.rewind.session_writers.iter().any(|((owner, _), (writers, unknown))| owner == conv && (!writers.is_empty() || *unknown))
            || self.rewind.helper_writers.values().any(|(owner, writers, unknown)| owner == conv && (!writers.is_empty() || *unknown))
    }

    pub(super) fn rewind_retire(&mut self, conv: &str, live: &Live) {
        let key = (conv.to_string(), live.sid);
        if self.rewind.exited_sessions.remove(&key) {
            return;
        }
        self.rewind.session_folders.entry(key.clone()).or_insert_with(|| live.cwd.clone());
        self.rewind.retired_sessions.insert(key, (live.vendor, live.session.clone()));
        self.rewind.refresh_writers(conv);
    }

    pub(super) fn rewind_session_started(&mut self, conv: &str, sid: u64, folder: PathBuf) {
        let key = (conv.to_string(), sid);
        self.rewind.session_folders.insert(key.clone(), folder);
        self.rewind.starting_sessions.insert(key);
        self.rewind.refresh_writers(conv);
    }

    pub(super) fn rewind_stop_retired(&self, conv: &str) {
        for ((owner, _), (_, session)) in &self.rewind.retired_sessions {
            if owner == conv {
                session.interrupt();
            }
        }
        for ((call, sid), session) in &self.rewind.helper_sessions {
            if !self.helpers.get(call).is_some_and(|helper| helper.hsid == *sid)
                && self.rewind.helper_writers.get(&(call.clone(), *sid)).is_some_and(|(owner, _, _)| owner == conv)
            {
                session.interrupt();
            }
        }
    }

    pub(super) fn rewind_tracks_session(&self, conv: &str, sid: u64) -> bool {
        let key = (conv.to_string(), sid);
        self.rewind.session_folders.contains_key(&key) || self.rewind.session_writers.contains_key(&key)
    }

    pub(super) fn rewind_retired_session(&self, conv: &str, sid: u64) -> Option<(Vendor, Session)> {
        self.rewind.retired_sessions.get(&(conv.to_string(), sid)).cloned()
    }

    pub(super) fn rewind_event(&mut self, conv: &str, sid: u64, event: &EngineEvent) -> bool {
        // Repeated idle snapshots cannot invalidate the final scan they just requested.
        if !matches!(event, EngineEvent::Writers { .. }) {
            self.rewind.bump(conv);
        }
        match event {
            EngineEvent::WriterExit { confirmed } => {
                let key = (conv.to_string(), sid);
                if *confirmed {
                    self.rewind.retired_sessions.remove(&key);
                    self.rewind.session_folders.remove(&key);
                    self.rewind.starting_sessions.remove(&key);
                    if self.live.get(conv).is_some_and(|live| live.sid == sid) {
                        self.rewind.exited_sessions.insert(key);
                    }
                    self.rewind.observe_writers(conv, sid, &[], false);
                } else {
                    self.rewind.observe_writers(conv, sid, &[], true);
                }
                self.try_seal_rewind(conv);
                true
            }
            EngineEvent::Started { .. } => {
                self.rewind.starting_sessions.remove(&(conv.to_string(), sid));
                self.rewind.refresh_writers(conv);
                false
            }
            EngineEvent::Writers { active, unknown } => {
                self.rewind.observe_writers(conv, sid, active, *unknown);
                self.try_seal_rewind(conv);
                true
            }
            EngineEvent::Cursor { entry, turn } => {
                if let Some(reply) = self.live.get(conv).and_then(|l| l.turn.clone()) {
                    self.rewind.cursors.insert((conv.into(), reply), Cursor { entry: entry.clone(), turn: turn.clone() });
                }
                true
            }
            _ => false,
        }
    }

    pub(super) fn rewind_helper_started(&mut self, call: &str, sid: u64, conv: &str, folder: PathBuf) {
        self.rewind.bump(conv);
        self.rewind.helper_folders.insert((call.into(), sid), folder);
        self.rewind.helper_writers.insert((call.into(), sid), (conv.into(), vec!["admission".into()], true));
    }

    pub(super) fn rewind_helper_session(&mut self, call: &str, sid: u64, session: Session) {
        self.rewind.helper_sessions.insert((call.into(), sid), session);
    }

    pub(super) fn rewind_helper_retire(&mut self, call: &str, sid: u64) {
        if let Some((owner, _, _)) = self.rewind.helper_writers.get(&(call.into(), sid)) {
            let owner = owner.clone();
            self.rewind.bump(&owner);
        }
    }

    pub(super) fn rewind_helper_event(&mut self, call: &str, sid: u64, event: &EngineEvent) {
        let key = (call.to_string(), sid);
        let Some((conv, writers, unknown)) = self.rewind.helper_writers.get_mut(&key) else {
            return;
        };
        let conv = conv.clone();
        if let EngineEvent::WriterExit { confirmed: true } = event {
            self.rewind.helper_writers.remove(&key);
            self.rewind.helper_folders.remove(&key);
            self.rewind.helper_sessions.remove(&key);
        } else if let EngineEvent::WriterExit { confirmed: false } = event {
            *unknown = true;
        } else if let EngineEvent::Writers { active, unknown: unclear } = event {
            *writers = active.clone();
            *unknown = *unclear;
        }
        self.rewind.bump(&conv);
        self.try_seal_rewind(&conv);
        self.refresh_running();
    }

    fn mark_rewind(&mut self, id: &str, reply_id: &str, result: Result<PointInfo, String>) {
        let Some(c) = self.convs.get_mut(id) else {
            return;
        };
        let Some(at) = c.messages.iter().position(|m| m.id == reply_id) else {
            return;
        };
        let Some(question) = c.messages[..at].iter_mut().rev().find(|m| m.role == Role::User) else {
            return;
        };
        let info = result.unwrap_or_else(|_| PointInfo {
            id: new_id(),
            message_id: question.id.clone(),
            turn_id: reply_id.into(),
            at: question.at,
            coverage: Coverage { unavailable: true, incomplete: true, ..Coverage::default() },
            sealed: false,
            cursor: Cursor::default(),
        });
        let message_id = question.id.clone();
        question.rewind = Some(info);
        self.save(id);
        self.emit_message(id, &message_id);
    }

    /// Only this turn waits for its own bounded baseline. Other turns, steering and automatic work keep being admitted;
    /// a busy history worker makes coverage unavailable rather than queueing work behind it.
    pub(super) fn capture_rewind(
        &mut self,
        id: &str,
        reply_id: &str,
        pick: &Pick,
        text: String,
        images: Vec<Image>,
        by_hand: bool,
    ) -> bool {
        if self.convs.get(id).is_some_and(|c| c.messages.iter().any(|m| m.rewind.as_ref().is_some_and(|point| point.turn_id == reply_id))) {
            return false;
        }
        if self.rewind.attempted.contains(reply_id) {
            return self.rewind.pending.contains(id);
        }
        self.rewind.attempted.insert(reply_id.into());
        let Some(c) = self.convs.get(id).filter(|c| c.area == Area::Project || c.working_folder.is_some()) else {
            return false;
        };
        let source = crate::rewind::Capture::new(c);
        let message = c.messages.iter().take_while(|m| m.id != reply_id).filter(|m| m.role == Role::User).last().map(|m| m.id.clone());
        let (Some(message), Ok(root), Some(history), Some(git)) = (message, self.cwd_of(c), self.rewind.history.clone(), self.git.clone())
        else {
            self.mark_rewind(id, reply_id, Err("error.rewind.unavailable".into()));
            return false;
        };
        if self.rewind.worker.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
            self.mark_rewind(id, reply_id, Err("error.rewind.busy".into()));
            return false;
        }
        let unknown_author = !self.rewind_running(&root).is_empty();
        self.rewind.pending.insert(id.into());
        self.rewind.bump(id);
        let (me, id, reply_id, pick) = (self.me.clone(), id.to_string(), reply_id.to_string(), pick.clone());
        let runtime = self.opts.runtime.clone();
        let (capture_id, capture_reply) = (id.clone(), reply_id.clone());
        let work = runtime.spawn_blocking(move || {
            let mut exec = git().ok_or("error.rewind.git")?;
            history.lock().unwrap_or_else(|e| e.into_inner()).capture_prepared(
                &mut *exec,
                &root,
                source,
                &message,
                &capture_reply,
                now(),
                unknown_author,
            )
        });
        runtime.spawn(async move {
            let result = work.await.unwrap_or_else(|_| Err("error.rewind.storage".into()));
            let Some(core) = me.upgrade() else {
                return;
            };
            let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
            core.rewind.pending.remove(&capture_id);
            core.mark_rewind(&id, &reply_id, result);
            let stopped = core.convs.get(&id).is_none_or(|c| {
                c.stopped || c.messages.iter().find(|m| m.id == reply_id).is_none_or(|m| m.status != Some(Status::Streaming))
            });
            if stopped {
                core.finish_worker_admission(&Self::turn_admission_key(&id, &reply_id));
                if let Some(reply) = core.convs.get_mut(&id).and_then(|c| c.messages.iter_mut().find(|m| m.id == reply_id)) {
                    reply.status = Some(Status::Stopped);
                }
                core.save(&id);
                core.push(&id, Some(&reply_id), true);
                core.release_rewind_worker();
                return;
            }
            if let Err(error) = core.start(&id, &reply_id, &pick, text, images, by_hand) {
                core.fail_reply(&id, &reply_id, error);
            }
            core.release_rewind_worker();
        });
        true
    }

    pub(super) fn try_seal_rewind(&mut self, conv: &str) {
        if self.rewind_deleted_settled(conv) {
            return;
        }
        let Some(root) =
            self.convs.get(conv).filter(|c| c.area == Area::Project || c.working_folder.is_some()).and_then(|c| self.cwd_of(c).ok())
        else {
            return;
        };
        if self.rewind.admission.iter().any(|p| overlap(p, &root)) {
            return;
        }
        if !self.rewind_running(&root).is_empty() || self.rewind.sealing.contains(&root) {
            return;
        }
        let (Some(history), Some(git)) = (self.rewind.history.clone(), self.git.clone()) else {
            return;
        };
        if self.rewind.worker.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
            self.rewind.deferred_seals.insert(conv.into());
            return;
        }
        self.rewind.sealing.insert(root.clone());
        let (me, revision, runtime, root_copy, conv) =
            (self.me.clone(), self.rewind_revision(&root), self.opts.runtime.clone(), root.clone(), conv.to_string());
        let cursors = self.rewind.cursors.clone();
        let writers = self.rewind.seen_writers.clone();
        let history_copy = history.clone();
        let scan = runtime.spawn_blocking(move || {
            let mut history = history_copy.lock().unwrap_or_else(|e| e.into_inner());
            if !history.has_open(&root_copy) {
                // Even an unavailable capture needs budget/expiry maintenance, after this turn's writers drain.
                history.cleanup_known(now())?;
                return Ok(None);
            }
            let mut exec = git().ok_or("error.rewind.git")?;
            for ((conv, turn), cursor) in cursors {
                history.turn_cursor(&conv, &turn, cursor);
            }
            for (conv, writers) in writers {
                history.record_writers(&conv, &writers.into_iter().collect::<Vec<_>>());
            }
            let seal = history.scan_seal(&mut *exec, &root_copy)?;
            Ok::<_, String>(Some(history.prepare_seal(seal)))
        });
        runtime.spawn(async move {
            let result = scan.await.unwrap_or_else(|_| Err("error.rewind.storage".into()));
            let Some(core) = me.upgrade() else {
                return;
            };
            let publish = if let Ok(Some(publication)) = &result {
                let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
                core.rewind.sealing.remove(&root);
                if core.rewind_revision(&root) != revision
                    || !core.rewind_running(&root).is_empty()
                    || core.rewind.admission.iter().any(|p| overlap(p, &root))
                {
                    core.rewind.deferred_seals.insert(conv.clone());
                    false
                } else if let Ok(mut history) = history.try_lock() {
                    history.publish_seal(publication);
                    true
                } else {
                    core.rewind.deferred_seals.insert(conv.clone());
                    false
                }
            } else {
                core.lock().unwrap_or_else(|e| e.into_inner()).rewind.sealing.remove(&root);
                false
            };
            if !publish {
                core.lock().unwrap_or_else(|e| e.into_inner()).release_rewind_worker();
                return;
            }
            if let Ok(Some(publication)) = result {
                let _ =
                    tokio::task::spawn_blocking(move || history.lock().unwrap_or_else(|e| e.into_inner()).persist_seal(publication, now()))
                        .await;
            }
            core.lock().unwrap_or_else(|e| e.into_inner()).release_rewind_worker();
        });
    }

    pub(super) fn release_rewind_worker(&mut self) {
        self.rewind.worker.store(false, Ordering::Release);
        for conv in self.rewind.deleted_folders.keys().cloned().collect::<Vec<_>>() {
            self.rewind_deleted_settled(&conv);
        }
        for conv in std::mem::take(&mut self.rewind.deferred_seals) {
            self.try_seal_rewind(&conv);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn move_chat_to_project_refuses_unconfirmed_writers_without_blocking_another_chat() {
        let data = crate::workspace::TestRoot::new();
        let folder = tempfile::tempdir().unwrap();
        let engine = Engine::new(
            EngineOptions {
                data_dir: data.path().into(),
                #[cfg(windows)]
                chat_dir: data.work().into(),
                clis: Default::default(),
                spawn: Arc::new(|_, _, _| panic!("moving must not launch a process")),
                windows: cfg!(windows),
                claude_env_billed: false,
                max_warm: 0,
                idle: Duration::from_secs(60),
                runtime: tokio::runtime::Handle::current(),
                ask: None,
            },
            Arc::new(|_| {}),
        )
        .unwrap();
        let project = engine.add_project(&folder.path().to_string_lossy()).unwrap();
        let (id, other) = engine.with(|core| {
            let id = core.blank(Area::Chat, None);
            let other = core.blank(Area::Chat, None);
            for chat in [&id, &other] {
                core.convs.get_mut(chat).unwrap().working_folder = Some(folder.path().to_string_lossy().into_owned());
                core.save(chat);
            }
            core.rewind.session_folders.insert((id.clone(), 1), folder.path().into());
            core.handle(&id, 1, EngineEvent::Writers { active: vec![], unknown: true });
            (id, other)
        });
        let before = serde_json::to_value(engine.open(&id).unwrap()).unwrap();
        assert_eq!(engine.move_to_project(&id, &project.id).unwrap_err(), "error.moveChat.busy");
        engine.with(|core| core.handle(&id, 1, EngineEvent::WriterExit { confirmed: false }));
        assert_eq!(engine.move_to_project(&id, &project.id).unwrap_err(), "error.moveChat.busy");
        assert_eq!(serde_json::to_value(engine.open(&id).unwrap()).unwrap(), before);
        engine.with(|core| assert_eq!(serde_json::to_value(core.store.get(&id).unwrap()).unwrap(), before));
        assert_eq!(engine.move_to_project(&other, &project.id).unwrap().area, Area::Project, "only the owning chat is busy");
        engine.with(|core| core.handle(&id, 1, EngineEvent::WriterExit { confirmed: true }));
        assert_eq!(engine.move_to_project(&id, &project.id).unwrap().area, Area::Project);
        engine.shutdown();
    }

    #[tokio::test]
    async fn move_chat_to_project_waits_for_rewind_admission_to_reopen() {
        let data = crate::workspace::TestRoot::new();
        let folder = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        let engine = Engine::new(
            EngineOptions {
                data_dir: data.path().into(),
                #[cfg(windows)]
                chat_dir: data.work().into(),
                clis: Default::default(),
                spawn: Arc::new(|_, _, _| panic!("moving must not launch a process")),
                windows: cfg!(windows),
                claude_env_billed: false,
                max_warm: 0,
                idle: Duration::from_secs(60),
                runtime: tokio::runtime::Handle::current(),
                ask: None,
            },
            Arc::new(|_| {}),
        )
        .unwrap();
        let project = engine.add_project(&destination.path().to_string_lossy()).unwrap();
        let id = engine.with(|core| {
            let id = core.blank(Area::Chat, None);
            let c = core.convs.get_mut(&id).unwrap();
            c.working_folder = Some(folder.path().to_string_lossy().into_owned());
            c.messages.push(Message::new("earlier".into(), Role::User, "Keep this message".into(), 1));
            core.save(&id);
            id
        });
        let before = serde_json::to_value(engine.open(&id).unwrap()).unwrap();
        let restoring = engine.clone();
        let target = id.clone();
        let (entered, ready) = tokio::sync::oneshot::channel();
        let (release, gate) = std::sync::mpsc::channel();
        let worker = tokio::task::spawn_blocking(move || {
            // Hold the same admission as restore/redo while their disk work runs outside the engine lock.
            let (_admission, _history, _conversation) = restoring.rewind_admission(&target).unwrap();
            entered.send(()).unwrap();
            let _ = gate.recv_timeout(Duration::from_secs(10));
        });
        ready.await.unwrap();
        let result = engine.move_to_project(&id, &project.id);
        let after = serde_json::to_value(engine.open(&id).unwrap()).unwrap();
        release.send(()).unwrap();
        worker.await.unwrap();
        assert_eq!(result.unwrap_err(), "error.rewind.busy");
        assert_eq!(after, before, "moving cannot change the folder or record while a rewind can still publish its snapshot");
        let moved = engine.move_to_project(&id, &project.id).unwrap();
        assert_eq!(moved.area, Area::Project);
        assert_eq!(moved.messages[0].id, "earlier");
        engine.shutdown();
    }

    #[tokio::test]
    async fn rewind_deletion_checks_unreadable_recovery_before_any_side_effect() {
        let data = crate::workspace::TestRoot::new();
        let folder = tempfile::tempdir().unwrap();
        let engine = Engine::new(
            EngineOptions {
                data_dir: data.path().into(),
                #[cfg(windows)]
                chat_dir: data.work().into(),
                clis: Default::default(),
                spawn: Arc::new(|_, _, _| panic!("deletion must not launch a process")),
                windows: cfg!(windows),
                claude_env_billed: false,
                max_warm: 0,
                idle: Duration::from_secs(60),
                runtime: tokio::runtime::Handle::current(),
                ask: None,
            },
            Arc::new(|_| {}),
        )
        .unwrap();
        let project = engine.add_project(&folder.path().to_string_lossy()).unwrap();
        let id = engine.with(|core| {
            let id = core.blank(Area::Project, Some(project.id));
            core.save(&id);
            id
        });
        engine.with(|core| {
            core.rewind.history = None;
            core.rewind.recovery_failed = true;
        });
        for discard in [false, true] {
            assert_eq!(engine.remove_with(&id, discard), Err("error.rewind.deleteRecovery".into()));
        }
        engine.with(|core| {
            assert_eq!(core.remove(&id), Err("error.rewind.deleteRecovery".into()), "discard cannot bypass unreadable recovery");
            assert!(core.convs.contains_key(&id) && core.store.get(&id).is_some());
            core.rewind.recovery_failed = false;
        });
        engine.remove_with(&id, true).unwrap();
        engine.with(|core| assert!(!core.convs.contains_key(&id) && core.store.get(&id).is_none()));
        engine.shutdown();
    }

    #[tokio::test]
    async fn rewind_deletion_waits_for_capture_without_holding_the_engine_or_losing_the_unsaved_check() {
        use crate::project::git::{Executor, GitError, Invocation, Output};
        struct UnavailableGit;
        impl Executor for UnavailableGit {
            fn execute(&mut self, _: &Invocation) -> Result<Output, GitError> {
                Err(GitError::Command)
            }
        }
        for (discard, admitted) in [(false, false), (true, false), (false, true), (true, true)] {
            let data = crate::workspace::TestRoot::new();
            let folder = tempfile::tempdir().unwrap();
            let engine = Engine::new(
                EngineOptions {
                    data_dir: data.path().into(),
                    #[cfg(windows)]
                    chat_dir: data.work().into(),
                    clis: Default::default(),
                    spawn: Arc::new(|_, _, _| panic!("a deleted or stopped capture must not launch a process")),
                    windows: cfg!(windows),
                    claude_env_billed: false,
                    max_warm: 0,
                    idle: Duration::from_secs(60),
                    runtime: tokio::runtime::Handle::current(),
                    ask: None,
                },
                Arc::new(|_| {}),
            )
            .unwrap();
            let project = engine.add_project(&folder.path().to_string_lossy()).unwrap();
            let (capturing, capture_started) = tokio::sync::oneshot::channel();
            let capturing = Mutex::new(Some(capturing));
            engine.set_git(Arc::new(move || {
                if let Some(started) = capturing.lock().unwrap().take() {
                    let _ = started.send(());
                }
                Some(Box::new(UnavailableGit))
            }));
            let history = engine.with(|core| core.rewind.history.clone().unwrap());
            let (held, ready) = tokio::sync::oneshot::channel();
            let (release, gate) = std::sync::mpsc::channel();
            let holder = tokio::task::spawn_blocking(move || {
                let _history = history.lock().unwrap();
                held.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(30)).expect("the test releases the history worker");
            });
            ready.await.unwrap();
            let id = engine.with(|core| {
                let id = core.blank(Area::Project, Some(project.id));
                let c = core.convs.get_mut(&id).unwrap();
                c.copy = Some(crate::project::copy::WorkCopy {
                    path: folder.path().to_string_lossy().into(),
                    root: folder.path().to_string_lossy().into(),
                    branch: "faber/held-capture".into(),
                    sub: String::new(),
                    base: None,
                });
                c.messages.push(Message::new("question".into(), Role::User, "Change things".into(), 1));
                let mut reply = Message::new("reply".into(), Role::Assistant, String::new(), 1);
                reply.status = Some(Status::Streaming);
                c.messages.push(reply);
                core.save(&id);
                let pick = Pick { vendor: Vendor::Claude, model: None, effort: "medium".into(), reason: String::new(), pace: None };
                assert!(core.capture_rewind(&id, "reply", &pick, "Change things".into(), vec![], true));
                id
            });
            tokio::time::timeout(Duration::from_secs(30), capture_started).await.unwrap().unwrap();
            let admission = admitted.then(|| {
                let folder = folder.path().canonicalize().unwrap();
                engine.with(|core| {
                    core.rewind.admission.push(folder.clone());
                    assert_eq!(core.remove(&id), Err("error.rewind.busy".into()), "the destructive boundary stays guarded");
                });
                Admission { engine: engine.clone(), folder }
            });
            let deleting = engine.clone();
            let target = id.clone();
            let (began, starting) = tokio::sync::oneshot::channel();
            let mut removed = tokio::task::spawn_blocking(move || {
                began.send(()).unwrap();
                deleting.remove_with(&target, discard)
            });
            starting.await.unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(50), &mut removed).await.is_err(),
                "deletion waits instead of returning busy"
            );
            engine.with(|core| {
                assert!(core.rewind.pending.contains(&id), "the real capture is still in progress");
                assert!(core.convs.contains_key(&id) && core.store.get(&id).is_some(), "waiting has no deletion side effects");
                // Prevent a later launch when the capture resumes; deletion's wait must leave Core available.
                core.convs.get_mut(&id).unwrap().stopped = true;
            });
            release.send(()).unwrap();
            holder.await.unwrap();
            if admitted {
                assert!(
                    tokio::time::timeout(Duration::from_millis(50), &mut removed).await.is_err(),
                    "deletion waits for restore admission without holding the engine lock"
                );
                engine.with(|core| assert!(core.convs.contains_key(&id)));
            }
            drop(admission);
            let result = tokio::time::timeout(Duration::from_secs(30), removed).await.unwrap().unwrap();
            assert_eq!(result, if discard { Ok(()) } else { Err("error.copy.unsaved".into()) });
            tokio::time::timeout(Duration::from_secs(30), async {
                while engine.with(|core| core.rewind.pending.contains(&id)) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            engine.with(|core| {
                assert_eq!(
                    core.convs.contains_key(&id) && core.store.get(&id).is_some(),
                    !discard,
                    "a late capture cannot resurrect a deleted conversation"
                )
            });
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn rewind_deleted_conversations_block_their_folder_until_all_writers_confirm_exit() {
        let data = crate::workspace::TestRoot::new();
        let shared = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let engine = Engine::new(
            EngineOptions {
                data_dir: data.path().into(),
                #[cfg(windows)]
                chat_dir: data.work().into(),
                clis: Default::default(),
                spawn: Arc::new(|_, _, _| panic!("deleting a conversation must not start a process")),
                windows: cfg!(windows),
                claude_env_billed: false,
                max_warm: 0,
                idle: Duration::from_secs(60),
                runtime: tokio::runtime::Handle::current(),
                ask: None,
            },
            Arc::new(|_| {}),
        )
        .unwrap();
        let project = engine.add_project(&shared.path().to_string_lossy()).unwrap();
        let unrelated = engine.add_project(&other.path().to_string_lossy()).unwrap();
        let (a, b, c) = engine.with(|core| {
            (
                core.blank(Area::Project, Some(project.id.clone())),
                core.blank(Area::Project, Some(project.id)),
                core.blank(Area::Project, Some(unrelated.id)),
            )
        });
        let (sent, mut received) = tokio::sync::mpsc::unbounded_channel();
        let root = shared.path().canonicalize().unwrap();
        engine.with(|core| {
            core.live.insert(
                b.clone(),
                Live {
                    tool_kit: Default::default(),
                    start_card: None,
                    kit_identity: None,
                    kit_resource_digest: None,
                    guide: Default::default(),
                    kit_pending: false,
                    prepared: false,
                    kit_resources: None,
                    group_files: None,
                    group_context: None,
                    vendor: Vendor::Claude,
                    model: None,
                    running: None,
                    capabilities: Default::default(),
                    ended: None,
                    effort: String::new(),
                    sid: 1,
                    session: Session::from_fn(move |cmd| {
                        let _ = sent.send(cmd);
                    }),
                    turn: None,
                    parent_reported: None,
                    break_pending: false,
                    answer_started: false,
                    settled: false,
                    open_tools: Vec::new(),
                    used: 0,
                    cli: Cli { command: "unused".into(), prefix: vec![] },
                    resumed: false,
                    permissions: Permissions::Ask,
                    tuning: Default::default(),
                    research: None,
                    retiring: false,
                    was_idle: false,
                    runs: Permissions::Ask,
                    cwd: root.clone(),
                    visual_root: None,
                    by_hand: true,
                    approved: false,
                    started: 0.0,
                    served: true,
                    steering: vec![],
                    held: None,
                    unsaved_session: None,
                },
            );
            core.rewind_session_started(&b, 1, root.clone());
            core.rewind_helper_started("helper:b", 2, &b, root.clone());
            core.rewind_checks_stopping(&b);
            // Hold publication at its ordinary worker gate, without starting a Git scan.
            core.git = Some(Arc::new(|| panic!("the held worker must not scan")));
            core.rewind.worker.store(true, Ordering::Release);
        });
        engine.archive(&b, true).unwrap();
        assert!(
            matches!(engine.rewind_admission(&a), Err(error) if error == "error.rewind.running"),
            "archiving before the first Writers event keeps admission"
        );
        engine.remove(&b);
        assert!(matches!(received.try_recv(), Ok(crate::events::SessionCommand::Close)));
        engine.interrupt(&b);
        assert!(matches!(received.try_recv(), Ok(crate::events::SessionCommand::Interrupt)), "Stop reaches the deleted session's owner");
        engine.with(|core| {
            assert!(!core.convs.contains_key(&b));
            assert!(core.store.get(&b).is_none());
            assert!(core.rewind.retired_sessions.contains_key(&(b.clone(), 1)), "Stop retains the deleted session's owner");
            assert!(
                core.rewind_running(&root).iter().any(|run| run.conversation_id == b && run.writers.contains(&"session-closing".into()))
            );
            assert!(!core.rewind_running(&root.join("child")).is_empty(), "nested folders overlap too");
            assert!(core.rewind_running(&other.path().canonicalize().unwrap()).is_empty());
            assert!(core.rewind_revision(&root).contains_key(&b), "a deleted writer invalidates an in-flight seal");
            core.try_seal_rewind(&a);
            assert!(!core.rewind.deferred_seals.contains(&a), "sealing must stop before the worker gate");
        });
        assert!(matches!(engine.rewind_admission(&a), Err(error) if error == "error.rewind.running"));
        let (guard, _, _) = engine.rewind_admission(&c).expect("unrelated folder stays available");
        drop(guard);
        engine.with(|core| {
            core.handle(&b, 1, EngineEvent::Writers { active: vec![], unknown: false });
            assert!(core.rewind.retired_sessions.contains_key(&(b.clone(), 1)), "a queued initial idle snapshot is not exit proof");
            core.handle(&b, 1, EngineEvent::WriterExit { confirmed: false });
            assert!(core.rewind.retired_sessions.contains_key(&(b.clone(), 1)), "an unconfirmed exit remains a writer");
            core.handle(&b, 1, EngineEvent::WriterExit { confirmed: true });
            assert!(!core.rewind_running(&root).is_empty(), "helper and checks still drain after the main session exits");
            core.rewind_helper_event("helper:b", 2, &EngineEvent::Writers { active: vec![], unknown: false });
            assert!(core.rewind.helper_writers.contains_key(&("helper:b".into(), 2)), "a queued helper idle snapshot is not exit proof");
            core.rewind_helper_event("helper:b", 2, &EngineEvent::WriterExit { confirmed: true });
            assert!(!core.rewind_running(&root).is_empty(), "checks still drain after the helper exits");
            core.rewind_checks_ended(&b);
            assert!(core.rewind_running(&root).is_empty());
            assert!(!core.rewind.deleted_folders.contains_key(&b), "confirmed shutdown releases retained folder memory");
            assert!(!core.rewind_revision(&root).contains_key(&b));
            assert!(core.rewind.deferred_seals.contains(&a), "the confirmation retries the surviving conversation's seal");
            assert!(!core.convs.contains_key(&b), "late events never resurrect a deleted conversation");
        });
        let (guard, _, _) = engine.rewind_admission(&a).expect("restore and Redo resume after confirmed exit");
        drop(guard);
        engine.shutdown();
    }

    #[tokio::test]
    async fn rewind_removing_an_idle_loaded_project_does_not_invent_an_unknown_writer() {
        let data = crate::workspace::TestRoot::new();
        let removed_folder = tempfile::tempdir().unwrap();
        let other_folder = tempfile::tempdir().unwrap();
        let engine = Engine::new(
            EngineOptions {
                data_dir: data.path().into(),
                #[cfg(windows)]
                chat_dir: data.work().into(),
                clis: Default::default(),
                spawn: Arc::new(|_, _, _| panic!("project removal must not start a process")),
                windows: cfg!(windows),
                claude_env_billed: false,
                max_warm: 0,
                idle: Duration::from_secs(60),
                runtime: tokio::runtime::Handle::current(),
                ask: None,
            },
            Arc::new(|_| {}),
        )
        .unwrap();
        let removed = engine.add_project(&removed_folder.path().to_string_lossy()).unwrap();
        let other = engine.add_project(&other_folder.path().to_string_lossy()).unwrap();
        let (old, current) =
            engine.with(|core| (core.blank(Area::Project, Some(removed.id.clone())), core.blank(Area::Project, Some(other.id))));
        engine.open(&old).unwrap();
        engine.remove_project(&removed.id).unwrap();
        let folder = other_folder.path().canonicalize().unwrap();
        engine.with(|core| {
            assert_eq!(core.cwd_of(&core.convs[&old]).unwrap_err(), "error.projectGone");
            assert!(core.rewind_running(&folder).is_empty(), "a missing idle project must not block unrelated history");
        });
        let (guard, _, _) = engine.rewind_admission(&current).expect("restore and redo share this admission check");
        drop(guard);
        engine.with(|core| {
            for (active, unknown) in [(vec!["command:old".into()], false), (vec![], true)] {
                core.rewind.observe_writers(&old, 1, &active, unknown);
                let running = core.rewind_running(&folder);
                assert!(
                    running.iter().any(|run| run.conversation_id == old && run.unknown),
                    "actual unlocated work still blocks restore and sealing"
                );
            }
            core.rewind.observe_writers(&old, 1, &[], false);
            core.rewind.pending.insert(old.clone());
            assert!(!core.rewind_running(&folder).is_empty(), "unlocated admission is still work");
            core.rewind.pending.remove(&old);
            assert!(core.rewind_running(&folder).is_empty(), "confirmed idle clears the block without restarting");
        });
        engine.shutdown();
    }

    #[test]
    fn rewind_repeated_idle_writers_do_not_invalidate_a_seal_but_new_writers_do() {
        let data = tempfile::tempdir().unwrap();
        let mut state = State::new(data.path());
        state.observe_writers("conversation", 1, &[], false);
        let idle = state.revisions["conversation"];
        state.observe_writers("conversation", 1, &[], false);
        assert_eq!(state.revisions["conversation"], idle);
        state.observe_writers("conversation", 1, &["late-helper".into()], false);
        assert!(state.revisions["conversation"] > idle);
        let active = state.revisions["conversation"];
        state.observe_writers("conversation", 1, &[], false);
        assert!(state.revisions["conversation"] > active);
        let drained = state.revisions["conversation"];
        state.observe_writers("conversation", 1, &[], false);
        assert_eq!(state.revisions["conversation"], drained);
        state.observe_writers("conversation", 1, &[], true);
        assert!(state.revisions["conversation"] > drained, "unknown status still invalidates publication");
    }

    #[test]
    fn rewind_replaced_session_cannot_clear_its_predecessors_writers() {
        let data = tempfile::tempdir().unwrap();
        let mut state = State::new(data.path());
        state.observe_writers("conversation", 1, &["native-command:old".into()], true);
        state.observe_writers("conversation", 2, &["turn:new".into()], false);
        state.observe_writers("conversation", 2, &[], false);
        assert_eq!(state.writers["conversation"], (vec!["native-command:old".into()], true));
        state.observe_writers("conversation", 2, &["native-command:new".into()], true);
        state.observe_writers("conversation", 1, &[], false);
        assert_eq!(state.writers["conversation"], (vec!["native-command:new".into()], true));
        state.observe_writers("conversation", 2, &[], false);
        assert_eq!(state.writers["conversation"], (vec![], false), "confirmed exit releases the conversation");
        assert!(state.session_writers.is_empty(), "settled sessions do not accumulate");
    }

    #[test]
    fn rewind_damaged_recovery_is_distinct_from_missing_history() {
        let data = tempfile::tempdir().unwrap();
        assert!(!pending_recovery(data.path()));
        let journals = data.path().join("rewind").join("a".repeat(64)).join("journals");
        std::fs::create_dir_all(&journals).unwrap();
        std::fs::write(journals.join(format!("{}.json", Uuid::new_v4())), b"{truncated").unwrap();
        assert!(pending_recovery(data.path()));
        let state = State::new(data.path());
        assert!(state.history.is_none());
        assert!(state.recovery_failed, "unreadable recovery must not admit another writer");
    }
    #[test]
    fn rewind_overlap_includes_children_but_not_sibling_names() {
        assert!(overlap(Path::new("/work/a"), Path::new("/work/a/nested")));
        assert!(!overlap(Path::new("/work/a"), Path::new("/work/another")));
    }
}

//! Plain JSON files in the app's data folder: readable by people and by any future tool.
//! Writes go to a temp file first, so a crash never leaves a half-written conversation.
//!
//! Every write that can carry authority or spending (projects with their grants, the quota ledger, mission records)
//! returns a [`Fault`] the screen can translate. Their checked readers tell a missing file from a damaged one: only the
//! former is empty, and a damaged safety file is never read as no data.

use crate::ledger::Ledger;
use crate::model::{BranchOf, Conversation, Project, Settings, Summary};
use crate::project::memory::Approval;
use crate::recents::Recents;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Keep typed deserialization, including duplicate-field rejection, while reading the removed setting.
#[derive(Default, Deserialize)]
struct SettingsFile {
    #[serde(flatten)]
    settings: Settings,
    verbosity: Option<LegacyVerbosity>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum LegacyVerbosity {
    Default,
    Low,
    Medium,
    High,
}

/// Why a file could not be saved or read, as a key the screen translates. A failed save leaves the old file as it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// The disk or the user's quota is full.
    DiskFull,
    /// Any other failure to write or replace the file.
    Write,
    /// The file exists but could not be read.
    Read,
    /// The file was read but is not what it should be.
    Corrupt,
}

impl Fault {
    pub fn key(self) -> &'static str {
        match self {
            Fault::DiskFull => "error.store.full",
            Fault::Write => "error.store.write",
            Fault::Read => "error.store.read",
            Fault::Corrupt => "error.store.corrupt",
        }
    }

    /// The fault of a failed write or folder creation.
    pub fn of_write(e: &io::Error) -> Fault {
        match e.kind() {
            io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => Fault::DiskFull,
            _ => Fault::Write,
        }
    }
}

/// The two file operations an atomic replacement needs. Tests swap in one that fails like a full disk.
pub trait Disk: Send + Sync {
    /// Creates or truncates `path`, writes all of `bytes` and flushes them to the disk.
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;
    /// Moves `from` over `to`, replacing it.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
}

/// The computer's own disk.
pub struct RealDisk;

impl Disk for RealDisk {
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut file = fs::File::create(path)?;
        file.write_all(bytes)?;
        file.sync_all()
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }
}

/// Writes `bytes` beside `path` and renames the result over it. On failure the temp file is removed and the old file
/// stays as it was.
pub fn replace(disk: &dyn Disk, path: &Path, bytes: &[u8]) -> Result<(), Fault> {
    replace_with(disk, path, bytes, true)
}

/// [`replace`], where `patient` false does not wait for a busy rename: the caller already knows this file keeps failing.
pub fn replace_with(disk: &dyn Disk, path: &Path, bytes: &[u8], patient: bool) -> Result<(), Fault> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    let tmp = path.with_file_name(name);
    // Windows lets an antivirus or indexer hold a file it has just seen for a few ms: a busy rename tries again before it
    // counts as a failed save.
    let rename = || if patient { crate::tools::retry_busy(|| disk.rename(&tmp, path)) } else { disk.rename(&tmp, path) };
    disk.write(&tmp, bytes).and_then(|()| rename()).map_err(|e| {
        drop(fs::remove_file(&tmp));
        Fault::of_write(&e)
    })
}

/// Reads a JSON file: `None` when it does not exist, a [`Fault`] when it cannot be read or does not parse.
pub fn read_checked<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, Fault> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) if e.kind() == io::ErrorKind::InvalidData => return Err(Fault::Corrupt),
        Err(_) => return Err(Fault::Read),
    };
    serde_json::from_str(&text).map(Some).map_err(|_| Fault::Corrupt)
}

fn read<T: DeserializeOwned>(path: &Path) -> Option<T> {
    read_checked(path).ok().flatten()
}

pub struct Store {
    conversations: PathBuf,
    /// The side chat of each conversation (R250), a file of its own beside it, so the conversation and its search never read it.
    side_chats: PathBuf,
    search: crate::search::Search,
    projects: PathBuf,
    chat_groups: PathBuf,
    settings: PathBuf,
    recents: PathBuf,
    decisions_kept: PathBuf,
    budget: PathBuf,
    auto_learned: PathBuf,
    cache_health: PathBuf,
    disk: Arc<dyn Disk>,
    budget_owner: Option<fs::File>,
    owner_required: bool,
    /// The grants epoch started when the settings could not be read. It lives here, in memory, and is the epoch in force
    /// until a save writes it: a read that works again later never brings the epoch on disk back.
    recovery_epoch: std::sync::Mutex<Option<String>>,
    approvals_lock: Arc<std::sync::Mutex<()>>,
}

/// The conversation replacement commits the move; until then group membership must roll back after a crash.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupMove {
    conversation_id: String,
    groups: Vec<crate::engine::ChatGroup>,
}

impl Drop for Store {
    fn drop(&mut self) {
        self.search.stop();
    }
}

impl Store {
    pub fn new(data_dir: &Path) -> Self {
        Store::with_disk(data_dir, Arc::new(RealDisk))
    }

    /// A store that writes through `disk`, so tests can make writes fail.
    pub fn with_disk(data_dir: &Path, disk: Arc<dyn Disk>) -> Self {
        let conversations = data_dir.join("conversations");
        let _ = fs::create_dir_all(&conversations);
        Store {
            search: crate::search::Search::new(conversations.clone()),
            conversations,
            side_chats: data_dir.join("side-chats"),
            projects: data_dir.join("projects.json"),
            chat_groups: data_dir.join("chat-groups.json"),
            settings: data_dir.join("settings.json"),
            recents: data_dir.join("recents.json"),
            decisions_kept: data_dir.join("decisions-kept.json"),
            budget: data_dir.join("budget.json"),
            auto_learned: data_dir.join("auto-learned.json"),
            disk,
            budget_owner: None,
            owner_required: false,
            recovery_epoch: std::sync::Mutex::new(None),
            approvals_lock: Arc::new(std::sync::Mutex::new(())),
            cache_health: data_dir.join("cache-health.jsonl"),
        }
    }

    /// Own the spending ledger for this engine's lifetime, including builds with a custom data folder.
    pub(crate) fn claim_budget(&mut self) -> Result<(), Fault> {
        self.owner_required = true;
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.budget.with_extension("lock"))
            .map_err(|e| Fault::of_write(&e))?;
        file.try_lock().map_err(|_| Fault::Write)?;
        self.budget_owner = Some(file);
        Ok(())
    }

    pub(crate) fn release_budget(&mut self) {
        self.budget_owner = None;
    }

    fn write<T: Serialize>(&self, path: &Path, value: &T) -> Result<(), Fault> {
        self.write_with(path, value, true)
    }

    fn write_with<T: Serialize>(&self, path: &Path, value: &T, patient: bool) -> Result<(), Fault> {
        let bytes = serde_json::to_string_pretty(value).expect("plain data always serializes");
        replace_with(&*self.disk, path, bytes.as_bytes(), patient)
    }

    fn file(&self, id: &str) -> PathBuf {
        self.conversations.join(format!("{id}.json"))
    }

    pub fn list(&self) -> Vec<Summary> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        let mut list: Vec<Summary> = fs::read_dir(&self.conversations)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| read::<Conversation>(&e.path()))
            .map(|c| Summary {
                keep_going: c.keep_going.is_some(),
                archived: c.archived_at.is_some(),
                pinned_at: c.pinned_at,
                started_by: c.started_by,
                ended: c.ended.filter(|e| e.current(now)),
                plan: c.plan.as_ref().map(crate::model::Plan::count),
                id: c.id,
                title: c.title,
                area: c.area,
                project_id: c.project_id,
                updated_at: c.updated_at,
            })
            .collect();
        list.sort_by_key(|c| std::cmp::Reverse(c.updated_at));
        list
    }

    /// Which conversation branched from which (child, parent). Only the files that mention a branch are parsed, and only for
    /// that.
    pub fn branch_links(&self) -> Vec<(String, String)> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Link {
            id: String,
            branch_of: Option<BranchOf>,
        }
        fs::read_dir(&self.conversations)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| fs::read_to_string(e.path()).ok().filter(|text| text.contains("\"branchOf\"")))
            .filter_map(|text| serde_json::from_str::<Link>(&text).ok())
            .filter_map(|l| Some((l.id, l.branch_of?.conversation_id)))
            .collect()
    }

    /// The ids of conversations saved with "keep going" on. A file that does not mention it is not parsed, so looking at every
    /// conversation once at start stays a read of the files and nothing more.
    pub fn keep_going_ids(&self) -> Vec<String> {
        fs::read_dir(&self.conversations)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter(|e| fs::read_to_string(e.path()).is_ok_and(|text| text.contains("\"keepGoing\"")))
            .filter_map(|e| e.path().file_stem().map(|id| id.to_string_lossy().into_owned()))
            .collect()
    }

    /// The ids of conversations whose file mentions `text`, read without parsing.
    pub fn mentioning(&self, text: &str) -> Vec<String> {
        fs::read_dir(&self.conversations)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter(|e| fs::read_to_string(e.path()).is_ok_and(|t| t.contains(text)))
            .filter_map(|e| e.path().file_stem().map(|id| id.to_string_lossy().into_owned()))
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<Conversation> {
        // Ids are UUIDs; anything else could point outside the folder.
        if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return None;
        }
        read(&self.file(id))
    }

    pub fn save(&self, conversation: &Conversation) {
        let _ = self.save_checked(conversation);
    }

    /// Replaces the record durably before publishing it to search or acknowledging a rewind cursor.
    pub fn save_checked(&self, conversation: &Conversation) -> Result<(), Fault> {
        self.save_checked_with(conversation, true)
    }

    /// [`Store::save_checked`], where `patient` false does not wait for a busy rename (see [`replace_with`]).
    pub fn save_checked_with(&self, conversation: &Conversation, patient: bool) -> Result<(), Fault> {
        if let Some((pending, area)) = self.read_group_move()?
            && pending.conversation_id == conversation.id
            && area != conversation.area
        {
            self.recover_group_move()?;
        }
        self.write_with(&self.file(&conversation.id), conversation, patient)?;
        self.search.saved(conversation);
        Ok(())
    }

    pub fn remove(&self, id: &str) {
        let _ = self.remove_checked(id);
    }

    /// Deletes the record, or says why it stayed. Only a record that is not there is already removed: one that cannot be
    /// read (locked, damaged) is still a file that stays until it is deleted.
    pub fn remove_checked(&self, id: &str) -> Result<(), Fault> {
        // Ids are UUIDs; anything else could point outside the folder.
        if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Ok(());
        }
        if self.read_group_move()?.is_some_and(|(pending, _)| pending.conversation_id == id) {
            self.recover_group_move()?;
        }
        match crate::tools::retry_busy(|| fs::remove_file(self.file(id))) {
            Ok(()) => self.search.removed(id),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(Fault::of_write(&e)),
        }
        // Its side chat goes with it; best effort, the conversation is gone already.
        let _ = self.save_side_chat::<()>(id, &[]);
        Ok(())
    }

    fn side_file(&self, id: &str) -> Option<PathBuf> {
        // Ids are UUIDs; anything else could point outside the folder.
        id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-').then(|| self.side_chats.join(format!("{id}.json")))
    }

    /// The side chat kept with conversation `id`: its newest turns, or none when it has no file or a damaged one.
    pub fn side_chat<T: DeserializeOwned>(&self, id: &str) -> Vec<T> {
        self.side_file(id).and_then(|path| read(&path)).unwrap_or_default()
    }

    /// Keeps the side chat of conversation `id`; an empty one removes its file.
    pub fn save_side_chat<T: Serialize>(&self, id: &str, turns: &[T]) -> Result<(), Fault> {
        let Some(path) = self.side_file(id) else { return Ok(()) };
        if turns.is_empty() {
            return match crate::tools::retry_busy(|| fs::remove_file(&path)) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(Fault::of_write(&e)),
                _ => Ok(()),
            };
        }
        fs::create_dir_all(&self.side_chats).map_err(|e| Fault::of_write(&e))?;
        self.write(&path, &turns)
    }

    pub fn search(&self) -> crate::search::Search {
        self.search.clone()
    }

    /// The project list for the sidebar: a damaged file shows no projects.
    pub fn projects(&self) -> Vec<Project> {
        read(&self.projects).unwrap_or_default()
    }

    /// The project list with the grants it carries. A missing file is no project; a damaged one is an error, so a lost
    /// grant is never mistaken for a list that never had one.
    pub fn projects_checked(&self) -> Result<Vec<Project>, Fault> {
        read_checked(&self.projects).map(Option::unwrap_or_default)
    }

    pub fn save_projects(&self, projects: &[Project]) -> Result<(), Fault> {
        self.write(&self.projects, &projects)
    }

    pub fn chat_groups(&self) -> Result<Vec<crate::engine::ChatGroup>, Fault> {
        let pending = self.read_group_move()?;
        let groups: Vec<crate::engine::ChatGroup> = match &pending {
            Some((pending, crate::model::Area::Chat)) => pending.groups.clone(),
            _ => read_checked(&self.chat_groups)?.unwrap_or_default(),
        };
        if pending.is_some() {
            // A blocked rollback still reads the old membership; mutations must finish recovery first.
            let _ = self.recover_group_move();
        }
        if groups.iter().any(|g| {
            uuid::Uuid::parse_str(&g.id).is_err()
                || g.files.len() > 10
                || g.instructions.chars().count() > 8000
                || g.files.iter().try_fold(0usize, |size, f| size.checked_add(f.size)).is_none_or(|size| size > 5_000_000)
                || g.files.iter().any(|f| uuid::Uuid::parse_str(&f.id).is_err())
        }) {
            return Err(Fault::Corrupt);
        }
        // A crash between staging a deletion and saving metadata keeps the old record: restore its bytes before use.
        for group in &groups {
            let dir = self.chat_groups_dir().join(&group.id);
            restore_removal(&dir)?;
            for file in &group.files {
                restore_removal(&dir.join(&file.id))?;
            }
        }
        Ok(groups)
    }

    pub fn chat_groups_dir(&self) -> PathBuf {
        self.chat_groups.with_extension("")
    }

    pub fn save_chat_groups(&self, groups: &[crate::engine::ChatGroup]) -> Result<(), Fault> {
        self.recover_group_move()?;
        self.write(&self.chat_groups, &groups)
    }

    /// Remove membership and replace the area under one commit point, without touching the group's knowledge files.
    pub(crate) fn move_chat_to_project(&self, conversation: &Conversation) -> Result<(), Fault> {
        self.recover_group_move()?;
        let groups = self.chat_groups()?;
        if !groups.iter().any(|g| g.conversation_ids.contains(&conversation.id)) {
            return self.save_checked(conversation);
        }
        let journal = self.chat_groups.with_file_name("chat-group-move.json");
        self.write(&journal, &GroupMove { conversation_id: conversation.id.clone(), groups: groups.clone() })?;
        let mut after = groups;
        for group in &mut after {
            group.conversation_ids.retain(|id| id != &conversation.id);
        }
        if let Err(error) = self.write(&self.chat_groups, &after) {
            // Atomic replacement failed: both original files are untouched.
            let _ = fs::remove_file(journal);
            return Err(error);
        }
        if let Err(error) = self.write(&self.file(&conversation.id), conversation) {
            // Keep the journal if rollback also fails, so the next reader retries before using membership.
            let _ = self.recover_group_move();
            return Err(error);
        }
        // Area is now committed; recovery can only finish cleanup, never put this conversation back in its group.
        let _ = fs::remove_file(journal);
        self.search.saved(conversation);
        Ok(())
    }

    fn read_group_move(&self) -> Result<Option<(GroupMove, crate::model::Area)>, Fault> {
        let journal = self.chat_groups.with_file_name("chat-group-move.json");
        let Some(pending) = read_checked::<GroupMove>(&journal)? else {
            return Ok(None);
        };
        if uuid::Uuid::parse_str(&pending.conversation_id).is_err() {
            return Err(Fault::Corrupt);
        }
        let conversation = read_checked::<Conversation>(&self.file(&pending.conversation_id))?.ok_or(Fault::Corrupt)?;
        Ok(Some((pending, conversation.area)))
    }

    pub(crate) fn recover_group_move(&self) -> Result<(), Fault> {
        let Some((pending, area)) = self.read_group_move()? else {
            return Ok(());
        };
        if area == crate::model::Area::Chat {
            self.write(&self.chat_groups, &pending.groups)?;
        }
        fs::remove_file(self.chat_groups.with_file_name("chat-group-move.json")).map_err(|e| Fault::of_write(&e))
    }

    /// A missing file is the defaults (full access). A file that cannot be read or understood (a hand edit, or values a newer
    /// build wrote) is never read as that: it asks before every change.
    pub fn settings(&self) -> Settings {
        let mut recovery = self.recovery_epoch.lock().unwrap_or_else(|e| e.into_inner());
        let read = read_checked::<SettingsFile>(&self.settings).map(|found| {
            let SettingsFile { mut settings, verbosity } = found.unwrap_or_default();
            let migrated = verbosity.is_some();
            if let Some(old) = verbosity {
                use crate::model::ReplyStyle;
                settings.reply_style.get_or_insert(match old {
                    LegacyVerbosity::Low => ReplyStyle::Short,
                    LegacyVerbosity::High => ReplyStyle::Explained,
                    LegacyVerbosity::Default | LegacyVerbosity::Medium => ReplyStyle::Default,
                });
            }
            settings.reply_style_text = settings.reply_style_text.as_deref().map(crate::model::clean_reply_style_text);
            if let Some(epoch) = recovery.clone() {
                settings.grants_epoch = Some(epoch.clone());
                settings.permissions_revision = Some(epoch);
            }
            if migrated {
                // A read-only disk may defer the rewrite until the person's next successful save.
                let _ = self.write(&self.settings, &settings);
            }
            settings
        });
        match read {
            Ok(settings) => settings,
            // No earlier grant counts either: a new epoch, kept in memory and written by the next save.
            Err(_) => Settings {
                permissions: Some(crate::model::Permissions::Ask),
                grants_epoch: Some(recovery.get_or_insert_with(crate::model::new_epoch).clone()),
                permissions_revision: recovery.clone(),
                ..Settings::default()
            },
        }
    }

    /// Forgets the "Always allow" of every conversation on disk except the `loaded` ones, which the engine clears itself.
    pub fn clear_grants(&self, loaded: &dyn Fn(&str) -> bool) {
        let files = fs::read_dir(&self.conversations).into_iter().flatten().flatten();
        for entry in files.filter(|e| e.path().extension().is_some_and(|x| x == "json")) {
            if let Some(mut c) = read::<Conversation>(&entry.path())
                && c.allow_always.is_some()
                && !loaded(&c.id)
            {
                c.allow_always = None;
                c.allow_always_epochs.clear();
                // Silent on purpose: the epoch replaced in `save_settings` already voids these grants, so a file that stays
                // stale costs nothing and the person's mode change is not undone by it.
                self.save(&c);
            }
        }
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<(), Fault> {
        let mut recovery = self.recovery_epoch.lock().unwrap_or_else(|e| e.into_inner());
        self.write(&self.settings, settings)?;
        // The epoch in force is on disk now.
        *recovery = None;
        Ok(())
    }

    fn quarantine(&self) -> PathBuf {
        self.budget.with_file_name("budget.json.quarantine")
    }

    /// Reads `budget.json`; a damaged file is copied aside once, as evidence, before the error is returned.
    fn read_ledger(&self) -> Result<Ledger, Fault> {
        let checked = read_checked::<Ledger>(&self.budget).and_then(|found| {
            let ledger = found.unwrap_or_default();
            if crate::project::budget::Budget::load(&ledger).is_err()
                || crate::model::Vendor::ALL.iter().any(|v| ledger.of(*v).own_spend.values().any(|cost| !cost.is_finite() || *cost < 0.0))
            {
                return Err(Fault::Corrupt);
            }
            Ok(ledger)
        });
        match checked {
            Ok(mut ledger) => {
                ledger.sanitize();
                Ok(ledger)
            }
            Err(Fault::Corrupt) => {
                if !self.quarantine().exists() {
                    drop(fs::copy(&self.budget, self.quarantine()));
                }
                Err(Fault::Corrupt)
            }
            Err(e) => Err(e),
        }
    }

    pub fn recents(&self) -> Recents {
        read::<Recents>(&self.recents).unwrap_or_default().bounded()
    }

    pub fn save_recents(&self, recents: &Recents) -> Result<(), Fault> {
        self.write(&self.recents, recents)
    }

    /// The AI decisions the person kept in a project's decision list (R16), as hashes of their words. They live here and
    /// not in the project, so no agent can write a review. A missing or damaged file keeps nothing: decisions read "not
    /// reviewed" again.
    pub fn kept_decisions(&self, project: &str, list: &crate::project::memory::MemoryList) -> Vec<Approval> {
        self.kept_decisions_reader(project)(list)
    }

    /// `kept_decisions` for a worker: the file is read when the closure is called, away from the engine lock.
    pub fn kept_decisions_reader(
        &self,
        project: &str,
    ) -> impl FnOnce(&crate::project::memory::MemoryList) -> Vec<Approval> + Send + 'static {
        let (path, project, disk, lock) =
            (self.decisions_kept.clone(), project.to_string(), self.disk.clone(), self.approvals_lock.clone());
        move |list| {
            let Ok(_guard) = lock.lock() else { return Vec::new() };
            let Some(mut all) = read::<BTreeMap<String, Vec<Approval>>>(&path) else { return Vec::new() };
            let previous = all.get(&project).cloned().unwrap_or_default();
            let wanted = previous
                .iter()
                .filter_map(|record| match record {
                    Approval::Legacy(hash) => Some(hash.as_str()),
                    _ => None,
                })
                .collect::<std::collections::BTreeSet<_>>();
            let mut changed = false;
            if !wanted.is_empty() {
                let mut verified = BTreeMap::new();
                for body in list.files.iter().flat_map(|file| &file.entries).filter_map(|entry| entry.body.as_deref()) {
                    let current = Approval::of(body);
                    if let Approval::Item { content_hash, .. } = &current
                        && wanted.contains(content_hash.as_str())
                    {
                        verified.insert(content_hash.clone(), current);
                    }
                }
                for record in all.entry(project.clone()).or_default() {
                    if let Approval::Legacy(hash) = record
                        && let Some(current) = verified.get(hash)
                    {
                        *record = current.clone();
                        changed = true;
                    }
                }
            }
            if changed {
                let bytes = serde_json::to_vec_pretty(&all).expect("plain data always serializes");
                if replace(&*disk, &path, &bytes).is_err() {
                    return previous.into_iter().filter(|record| matches!(record, Approval::Item { .. })).collect();
                }
            }
            all.remove(&project).unwrap_or_default().into_iter().filter(|record| matches!(record, Approval::Item { .. })).collect()
        }
    }

    /// Adds one kept decision. The newest 500 per project stay, and projects that are gone (not in `live`) are dropped.
    pub fn keep_decision(&self, project: &str, hash: Approval, live: &[String]) -> Result<(), Fault> {
        let _guard = self.approvals_lock.lock().map_err(|_| Fault::Write)?;
        let mut all = read::<BTreeMap<String, Vec<Approval>>>(&self.decisions_kept).unwrap_or_default();
        all.retain(|id, _| live.contains(id));
        let kept = all.entry(project.into()).or_default();
        kept.retain(|h| !h.matches(&hash));
        kept.push(hash);
        kept.drain(..kept.len().saturating_sub(500));
        self.write(&self.decisions_kept, &all)
    }

    /// Revokes a last copy before deleting it, without holding the engine lock during the file replacement.
    pub fn decision_revoker(&self, project: &str) -> impl FnOnce(&Approval) -> Result<(), Fault> + Send + 'static + use<> {
        let (path, project, disk, lock) =
            (self.decisions_kept.clone(), project.to_string(), self.disk.clone(), self.approvals_lock.clone());
        move |hash| {
            let _guard = lock.lock().map_err(|_| Fault::Write)?;
            let mut all = read_checked::<BTreeMap<String, Vec<Approval>>>(&path)?.unwrap_or_default();
            let Some(kept) = all.get_mut(&project) else { return Ok(()) };
            let previous = kept.len();
            kept.retain(|record| !record.matches(hash));
            if kept.len() == previous {
                return Ok(());
            }
            let bytes = serde_json::to_vec_pretty(&all).expect("plain data always serializes");
            replace(&*disk, &path, &bytes)
        }
    }

    /// What Faber remembers about each vendor's quota, for Chat and Auto. A missing, damaged or unreadable file is an empty
    /// ledger: it only holds what was learned, and learning starts again.
    pub fn ledger(&self) -> Ledger {
        self.read_ledger().unwrap_or_default()
    }

    /// The same ledger for decisions that admit spending. A damaged file is an error, and stays one while its quarantined
    /// copy exists, even after Chat has saved a fresh ledger over it: damaged safety data never becomes an empty budget.
    pub fn ledger_checked(&self) -> Result<Ledger, Fault> {
        let ledger = self.read_ledger()?;
        if self.quarantine().exists() {
            return Err(Fault::Corrupt);
        }
        Ok(ledger)
    }

    /// What Auto learned from the person's redone and kept answers. A missing, damaged or unreadable file is nothing learned: it
    /// only holds a nudge, and learning starts again.
    pub fn auto_learned(&self) -> crate::auto_learned::Learned {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        read::<crate::auto_learned::Learned>(&self.auto_learned).unwrap_or_default().sanitized(now)
    }

    pub fn save_auto_learned(&self, learned: &crate::auto_learned::Learned) -> Result<(), Fault> {
        self.write(&self.auto_learned, learned)
    }

    /// Compact on purpose: it is rewritten often and a person never has to read it.
    pub fn save_ledger(&self, ledger: &Ledger) -> Result<(), Fault> {
        if self.owner_required && self.budget_owner.is_none() {
            return Err(Fault::Write);
        }
        replace(&*self.disk, &self.budget, serde_json::to_string(ledger).expect("plain data always serializes").as_bytes())
    }
}

fn restore_removal(path: &Path) -> Result<(), Fault> {
    let staged = path.with_extension("removing");
    if !path.exists() && staged.exists() {
        fs::rename(staged, path).map_err(|e| Fault::of_write(&e))?;
    }
    Ok(())
}

// ponytail: size cap, rotate if it ever matters.
const CACHE_HEALTH_MAX: u64 = 1_000_000;

/// Adds one line to a local log that nothing reads back for sending: it stops growing at `max` bytes and a failed write is
/// ignored, so it can never break a turn.
pub(crate) fn append_bounded(path: &Path, max: u64, line: &serde_json::Value) {
    use std::io::Write;
    if fs::metadata(path).is_ok_and(|m| m.len() >= max) {
        return;
    }
    let _ = fs::OpenOptions::new().create(true).append(true).open(path).and_then(|mut f| f.write_all(format!("{line}\n").as_bytes()));
}

impl Store {
    /// Adds one line to the local cache-health log. Nothing reads it back and nothing sends it anywhere.
    pub fn log_cache_health(&self, line: &serde_json::Value) {
        append_bounded(&self.cache_health, CACHE_HEALTH_MAX, line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_migration_needs_a_successful_protected_write_and_preserves_unseen_records() {
        struct FullDisk;
        impl Disk for FullDisk {
            fn write(&self, _: &Path, _: &[u8]) -> io::Result<()> {
                Err(io::ErrorKind::StorageFull.into())
            }
            fn rename(&self, _: &Path, _: &Path) -> io::Result<()> {
                panic!("a failed write must never rename")
            }
        }
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        fs::create_dir(project.path().join(".faber")).unwrap();
        fs::write(project.path().join(".faber/decisions.md"), "- Use SQLite. [by AI]\n").unwrap();
        let list = crate::project::memory::list(project.path()).unwrap();
        let current = Approval::of("- Use SQLite.");
        let old = Approval::of("- Previously confirmed.");
        let Approval::Item { content_hash, .. } = &current else { unreachable!() };
        let ledger = serde_json::json!({"project": [content_hash, old, "unseen-legacy"], "other": ["other-legacy"]});
        let path = data.path().join("decisions-kept.json");
        let bytes = serde_json::to_vec(&ledger).unwrap();
        fs::write(&path, &bytes).unwrap();
        let failing = Store::with_disk(data.path(), Arc::new(FullDisk));
        assert_eq!(failing.kept_decisions("project", &list), [old.clone()], "failed migration cannot authorize the legacy item");
        assert_eq!(fs::read(&path).unwrap(), bytes, "the previous protected record survives failure");
        let store = Store::new(data.path());
        assert_eq!(store.kept_decisions("project", &list), [current.clone(), old]);
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["project"][0], serde_json::to_value(current).unwrap());
        assert_eq!(saved["project"][2], "unseen-legacy");
        assert_eq!(saved["other"], ledger["other"], "a bounded read never drops another project's records");
        let once = fs::read(&path).unwrap();
        store.kept_decisions("project", &list);
        assert_eq!(fs::read(&path).unwrap(), once, "an unchanged read does not rewrite the ledger");
    }

    #[test]
    fn a_failed_rename_returns_the_error_and_removes_the_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("settings.json");
        fs::create_dir_all(target.join("inside")).unwrap();
        assert_eq!(replace(&RealDisk, &target, b"{}"), Err(Fault::Write));
        assert!(!dir.path().join("settings.json.tmp").exists());
        assert!(target.join("inside").is_dir(), "the destination was left alone");
    }
}

//! Plain JSON files in the app's data folder: readable by people and by any future tool.
//! Writes go to a temp file first, so a crash never leaves a half-written conversation.
//!
//! Every write that can carry authority or spending (projects with their grants, the quota ledger, mission records)
//! returns a [`Fault`] the screen can translate. Their checked readers tell a missing file from a damaged one: only the
//! former is empty, and a damaged safety file is never read as no data.

use crate::ledger::Ledger;
use crate::model::{BranchOf, Conversation, Project, Settings, Summary};
use crate::project::memory::{AdoptedKind, KeptDecision};
use crate::recents::Recents;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

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
    pub(crate) transcripts: crate::transcript::TranscriptStore,
    conversations: PathBuf,
    /// The side chat of each conversation (R250), a file of its own beside it, so the conversation and its search never read it.
    side_chats: PathBuf,
    creative_reviews: PathBuf,
    search: crate::search::Search,
    projects: PathBuf,
    chat_groups: PathBuf,
    settings: PathBuf,
    recents: PathBuf,
    decisions_kept: PathBuf,
    rules_sources: PathBuf,
    adoptions: PathBuf,
    budget: PathBuf,
    auto_learned: PathBuf,
    cache_health: PathBuf,
    disk: Arc<dyn Disk>,
    budget_owner: Option<fs::File>,
    owner_required: bool,
    /// The grants epoch started when the settings could not be read. It lives here, in memory, and is the epoch in force
    /// until a save writes it: a read that works again later never brings the epoch on disk back.
    recovery_epoch: std::sync::Mutex<Option<String>>,
    /// The sidebar row of each conversation file, kept by the file's length and time, so a list reads only the files that
    /// changed. Reading one decodes the whole conversation to take a few fields: 202 MB and 420 ms for 32 real ones
    /// (2026-10-27), twice a turn. Every save writes a new file and renames it into place, so a changed file has a new
    /// time; only a program that rewrites one in place with its old length and time would go unseen until the next save.
    summaries: Mutex<HashMap<PathBuf, (Stamp, Summary)>>,
    /// How many times a conversation record was written, so a test can tell one write from a hundred.
    writes: std::sync::atomic::AtomicU64,
}

/// A file's length and last change: the same pair means the file was not written since.
type Stamp = (u64, Option<SystemTime>);

fn stamp(path: &Path) -> Option<Stamp> {
    fs::metadata(path).ok().map(|m| (m.len(), m.modified().ok()))
}

/// The sidebar row of a conversation. Its done card is kept whole; whether it still shows is decided when listed.
fn summary(c: &Conversation) -> Summary {
    Summary {
        keep_going: c.keep_going.is_some(),
        archived: c.archived_at.is_some(),
        pinned_at: c.pinned_at,
        started_by: c.started_by.clone(),
        ended: c.ended.clone(),
        plan: c.plan.as_ref().map(crate::model::Plan::count),
        results: Some(crate::results::tally(c)).filter(|r| r.cost.turns > 0),
        stuck: c.stuck.is_some(),
        id: c.id.clone(),
        title: c.title.clone(),
        area: c.area,
        project_id: c.project_id.clone(),
        updated_at: c.updated_at,
    }
}

/// The conversation replacement commits the move; until then group membership must roll back after a crash.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupMove {
    conversation_id: String,
    groups: Vec<crate::engine::ChatGroup>,
}

/// Approving a location never approves its words or permits an adapter to write them.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdoptedFile {
    version: u8,
    root: PathBuf,
    target: String,
}

type Adoptions = BTreeMap<String, BTreeMap<AdoptedKind, AdoptedFile>>;

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
            transcripts: crate::transcript::TranscriptStore::with_disk(data_dir.join("child-transcripts"), disk.clone()),
            search: crate::search::Search::new(conversations.clone()),
            conversations,
            side_chats: data_dir.join("side-chats"),
            creative_reviews: data_dir.join("creative-reviews"),
            projects: data_dir.join("projects.json"),
            chat_groups: data_dir.join("chat-groups.json"),
            settings: data_dir.join("settings.json"),
            recents: data_dir.join("recents.json"),
            decisions_kept: data_dir.join("decisions-kept.json"),
            rules_sources: data_dir.join("rules-sources.json"),
            adoptions: data_dir.join("faber-trust/adoptions.json"),
            budget: data_dir.join("budget.json"),
            auto_learned: data_dir.join("auto-learned.json"),
            disk,
            budget_owner: None,
            owner_required: false,
            recovery_epoch: std::sync::Mutex::new(None),
            cache_health: data_dir.join("cache-health.jsonl"),
            summaries: Mutex::default(),
            writes: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// How many conversation records this store wrote.
    pub fn conversation_writes(&self) -> u64 {
        self.writes.load(std::sync::atomic::Ordering::Relaxed)
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

    /// The id is generated here; caller text can never select a filesystem path.
    pub(crate) fn save_creative_review(&self, value: &serde_json::Value) -> Result<String, Fault> {
        let id = uuid::Uuid::new_v4().to_string();
        fs::create_dir_all(&self.creative_reviews).map_err(|e| Fault::of_write(&e))?;
        self.write(&self.creative_reviews.join(format!("{id}.json")), value)?;
        Ok(id)
    }

    /// Whether conversation `id`'s file is still the one Faber last listed or wrote: nothing else changed or removed it since.
    pub(crate) fn unchanged(&self, id: &str) -> bool {
        let path = self.file(id);
        let known = self.summaries.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(&path).map(|(kept, _)| *kept);
        known.is_some() && stamp(&path) == known
    }

    pub fn list(&self) -> Vec<Summary> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        let paths: Vec<PathBuf> = fs::read_dir(&self.conversations)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        let mut cache = self.summaries.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        cache.retain(|path, _| paths.contains(path));
        let mut list: Vec<Summary> = paths
            .into_iter()
            .filter_map(|path| {
                let now_stamp = stamp(&path)?;
                match cache.get(&path) {
                    Some((kept, row)) if *kept == now_stamp => Some(row.clone()),
                    // A file that cannot be read now (locked by a scanner, half copied) is tried again at the next list.
                    _ => {
                        let row = read::<Conversation>(&path).as_ref().map(summary);
                        if let Some(row) = &row {
                            cache.insert(path, (now_stamp, row.clone()));
                        }
                        row
                    }
                }
            })
            .map(|mut row| {
                row.ended = row.ended.filter(|e| e.current(now));
                row
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
        let path = self.file(id);
        let before = stamp(&path);
        let mut c: Conversation = read(&path)?;
        // Pages and helper steps reach disk before the record counts them: see `TranscriptStore::recover`.
        self.transcripts.recover(id, &mut c.messages);
        // Known as read, unless it changed while it was being read: `unchanged` then says so.
        if let Some(read_stamp) = before.filter(|b| stamp(&path).as_ref() == Some(b)) {
            self.summaries.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(path, (read_stamp, summary(&c)));
        }
        Some(c)
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
        let path = self.file(&conversation.id);
        self.write_with(&path, conversation, patient)?;
        self.writes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.written(path, conversation);
        self.search.saved(conversation);
        Ok(())
    }

    /// The file just written is the conversation in hand, so the next list need not read it back, and `unchanged` knows it.
    fn written(&self, path: PathBuf, conversation: &Conversation) {
        if let Some(now_stamp) = stamp(&path) {
            self.summaries.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(path, (now_stamp, summary(conversation)));
        }
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
        self.write(&self.projects, &projects)?;
        // A project that left the list takes its local method count with it (`method_log`).
        if let Some(dir) = self.projects.parent() {
            crate::method_log::forget(dir, &projects.iter().map(|p| p.id.as_str()).collect::<Vec<_>>());
        }
        Ok(())
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
        self.written(self.file(&conversation.id), conversation);
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

    /// Called under the engine lock. Damaged derived records rebuild without touching approval records.
    pub(crate) fn refresh_rules(&self, project: &str, folder: &Path) -> Result<(), &'static str> {
        let mut all: BTreeMap<String, crate::project::rules::Record> = read(&self.rules_sources).unwrap_or_default();
        let record = crate::project::rules::refresh_cached(folder, all.get(project))?;
        if all.get(project) != Some(&record) {
            all.insert(project.into(), record);
            self.write(&self.rules_sources, &all).map_err(|e| e.key())?;
        }
        Ok(())
    }

    fn adoptions_checked(&self) -> Result<Adoptions, Fault> {
        let all: Adoptions = read_checked(&self.adoptions)?.unwrap_or_default();
        if all.iter().any(|(id, kinds)| {
            !crate::project::valid_id(id) || kinds.values().any(|record| record.version != 1 || !record.root.is_absolute())
        }) {
            return Err(Fault::Corrupt);
        }
        Ok(all)
    }

    /// Person-only approval, called under the engine lock; never exposed as an agent tool. No project file is written.
    pub fn approve_adopted_file(&self, project: &str, root: &Path, kind: AdoptedKind, target: &str) -> Result<(), &'static str> {
        if kind == AdoptedKind::Tasks || !crate::project::valid_id(project) || !root.is_absolute() {
            return Err("error.memory.refused");
        }
        crate::project::memory::read_adopted_source(root, target)?;
        self.save_adoption(project, root, kind, target)
    }

    /// Person-only location approval; repository content grants no authority over the folder's words.
    pub fn approve_adopted_tasks(&self, project: &str, root: &Path, target: &str) -> Result<(), &'static str> {
        if !crate::project::valid_id(project) || !root.is_absolute() {
            return Err("error.memory.refused");
        }
        crate::project::memory::read_adopted_tasks(root, target)?;
        self.save_adoption(project, root, AdoptedKind::Tasks, target)
    }

    fn save_adoption(&self, project: &str, root: &Path, kind: AdoptedKind, target: &str) -> Result<(), &'static str> {
        let mut all = self.adoptions_checked().map_err(Fault::key)?;
        all.entry(project.into()).or_default().insert(kind, AdoptedFile { version: 1, root: root.into(), target: target.into() });
        fs::create_dir_all(self.adoptions.parent().expect("fixed record parent")).map_err(|e| Fault::of_write(&e).key())?;
        self.write(&self.adoptions, &all).map_err(Fault::key)
    }

    /// Reopen the approved location on every read; neither repository text nor another project can redirect it.
    pub fn read_adopted_file(&self, project: &str, root: &Path, kind: AdoptedKind) -> Result<Option<String>, &'static str> {
        if kind == AdoptedKind::Tasks {
            return Err("error.memory.refused");
        }
        let all = self.adoptions_checked().map_err(Fault::key)?;
        let Some(record) = all.get(project).and_then(|kinds| kinds.get(&kind)) else { return Ok(None) };
        if record.root != root {
            return Err("error.memory.refused");
        }
        crate::project::memory::read_adopted_source(root, &record.target).map(Some)
    }

    /// Revalidate the approved task folder and every opened file on each read.
    pub fn read_adopted_tasks(&self, project: &str, root: &Path) -> Result<Option<crate::project::memory::AdoptedTasks>, &'static str> {
        let all = self.adoptions_checked().map_err(Fault::key)?;
        let Some(record) = all.get(project).and_then(|kinds| kinds.get(&AdoptedKind::Tasks)) else { return Ok(None) };
        if record.root != root {
            return Err("error.memory.refused");
        }
        crate::project::memory::read_adopted_tasks(root, &record.target).map(Some)
    }

    /// The decisions the person kept (R16), as item identities and content hashes, or legacy body-only hashes. They live here and
    /// not in the project, so no agent can write a review. A missing or damaged file keeps nothing: decisions read "not
    /// reviewed" again.
    pub fn kept_decisions(&self, project: &str) -> Vec<KeptDecision> {
        self.kept_decisions_reader(project)()
    }

    /// `kept_decisions` for a worker: the file is read when the closure is called, away from the engine lock.
    pub fn kept_decisions_reader(&self, project: &str) -> impl FnOnce() -> Vec<KeptDecision> + Send + 'static {
        let (path, project) = (self.decisions_kept.clone(), project.to_string());
        move || read::<BTreeMap<String, Vec<KeptDecision>>>(&path).unwrap_or_default().remove(&project).unwrap_or_default()
    }

    /// Called under the engine lock: a stale listing must never overwrite a newer Keep.
    pub(crate) fn migrate_kept_decisions(
        &self,
        project: &str,
        expected: &[KeptDecision],
        upgraded: &[KeptDecision],
    ) -> Result<bool, Fault> {
        let mut all: BTreeMap<String, Vec<KeptDecision>> = read_checked(&self.decisions_kept)?.unwrap_or_default();
        let Some(current) = all.get_mut(project).filter(|current| current.as_slice() == expected) else { return Ok(false) };
        *current = upgraded.to_vec();
        self.write(&self.decisions_kept, &all)?;
        Ok(true)
    }

    /// Adds one kept decision without discarding older approvals. Projects that are gone (not in `live`) are dropped.
    pub fn keep_decision(&self, project: &str, record: KeptDecision, live: &[String]) -> Result<(), Fault> {
        let mut all = read_checked::<BTreeMap<String, Vec<KeptDecision>>>(&self.decisions_kept)?.unwrap_or_default();
        all.retain(|id, _| live.contains(id));
        let kept = all.entry(project.into()).or_default();
        kept.retain(|older| !older.matches(&record));
        kept.push(record);
        self.write(&self.decisions_kept, &all)
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
    fn rules_registry_preserves_other_projects_approvals_and_refuses_unsafe_output() {
        let data = tempfile::tempdir().unwrap();
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let store = Store::new(data.path());
        fs::write(&store.decisions_kept, b"approval bytes must stay untouched").unwrap();
        store.refresh_rules("first", first.path()).unwrap();
        store.refresh_rules("second", second.path()).unwrap();
        let before: serde_json::Value = read(&store.rules_sources).unwrap();
        fs::remove_file(first.path().join(".faber/rules.md")).unwrap();
        store.refresh_rules("first", first.path()).unwrap();
        assert_eq!(read::<serde_json::Value>(&store.rules_sources).unwrap(), before);
        fs::write(first.path().join("package.json"), r#"{"scripts":{"test":"vitest"}}"#).unwrap();
        store.refresh_rules("first", first.path()).unwrap();
        let current: serde_json::Value = read(&store.rules_sources).unwrap();
        assert_eq!(current["second"], before["second"]);
        let registered = fs::read(&store.rules_sources).unwrap();
        fs::write(first.path().join(".faber/rules.md"), vec![b'x'; 256 * 1024 + 1]).unwrap();
        assert!(store.refresh_rules("first", first.path()).is_err());
        assert_eq!(fs::read(&store.rules_sources).unwrap(), registered);
        fs::remove_file(first.path().join(".faber/rules.md")).unwrap();
        store.refresh_rules("first", first.path()).unwrap();
        let generated = fs::read(first.path().join(".faber/rules.md")).unwrap();
        fs::write(other.path().join("rules.md"), &generated).unwrap();
        fs::remove_file(first.path().join(".faber/rules.md")).unwrap();
        fs::hard_link(other.path().join("rules.md"), first.path().join(".faber/rules.md")).unwrap();
        assert!(store.refresh_rules("first", first.path()).is_err());
        assert_eq!(fs::read(&store.rules_sources).unwrap(), registered);
        assert_eq!(fs::read(other.path().join("rules.md")).unwrap(), generated);
        fs::remove_file(first.path().join(".faber/rules.md")).unwrap();
        store.refresh_rules("first", first.path()).unwrap();
        fs::rename(first.path().join(".faber"), first.path().join("original")).unwrap();
        #[cfg(windows)]
        junction::create(other.path(), first.path().join(".faber")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(other.path(), first.path().join(".faber")).unwrap();
        assert!(store.refresh_rules("first", first.path()).is_err());
        assert_eq!(fs::read(&store.rules_sources).unwrap(), registered);
        assert_eq!(fs::read(other.path().join("rules.md")).unwrap(), generated);
        assert_eq!(fs::read(&store.decisions_kept).unwrap(), b"approval bytes must stay untouched");
    }
    #[test]
    fn rules_registry_failed_save_keeps_previous_record_and_approvals() {
        struct NoReplace;
        impl Disk for NoReplace {
            fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
                RealDisk.write(path, bytes)
            }
            fn rename(&self, _: &Path, _: &Path) -> io::Result<()> {
                Err(io::ErrorKind::StorageFull.into())
            }
        }
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let store = Store::new(data.path());
        store.refresh_rules("project", project.path()).unwrap();
        let before = fs::read(&store.rules_sources).unwrap();
        fs::write(&store.decisions_kept, b"existing decisions").unwrap();
        fs::write(project.path().join("package.json"), r#"{"scripts":{"test":"vitest"}}"#).unwrap();
        let failing = Store::with_disk(data.path(), Arc::new(NoReplace));
        assert_eq!(failing.refresh_rules("project", project.path()), Err("error.store.full"));
        assert_eq!(fs::read(&store.rules_sources).unwrap(), before);
        assert_eq!(fs::read(&store.decisions_kept).unwrap(), b"existing decisions");
        store.refresh_rules("project", project.path()).unwrap();
        assert_ne!(fs::read(&store.rules_sources).unwrap(), before);
    }

    #[test]
    fn rules_registry_tracks_sources_repairs_planted_output_and_recovers_derived_records() {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let store = Store::new(data.path());
        let manifest = project.path().join("package.json");
        let rules = project.path().join(".faber/rules.md");
        fs::write(&manifest, r#"{"scripts":{"test":"vitest"}}"#).unwrap();
        store.refresh_rules("project", project.path()).unwrap();
        let generated = fs::read(&rules).unwrap();
        let registered = fs::read(&store.rules_sources).unwrap();
        let rules_time = fs::metadata(&rules).unwrap().modified().unwrap();
        let record_time = fs::metadata(&store.rules_sources).unwrap().modified().unwrap();
        store.refresh_rules("project", project.path()).unwrap();
        assert_eq!(fs::metadata(&rules).unwrap().modified().unwrap(), rules_time);
        assert_eq!(fs::metadata(&store.rules_sources).unwrap().modified().unwrap(), record_time);

        // Unchanged sources do not make a planted output trustworthy.
        fs::write(&rules, "PLANTED_SYSTEM_AUTHORITY").unwrap();
        store.refresh_rules("project", project.path()).unwrap();
        assert_eq!(fs::read(&rules).unwrap(), generated);
        assert_eq!(fs::read(&store.rules_sources).unwrap(), registered);

        fs::write(&manifest, r#"{"scripts":{"lint":"eslint"}}"#).unwrap();
        store.refresh_rules("project", project.path()).unwrap();
        assert!(fs::read_to_string(&rules).unwrap().contains("npm run lint"));
        assert_ne!(fs::read(&store.rules_sources).unwrap(), registered);
        fs::write(project.path().join("tsconfig.json"), "{}").unwrap();
        store.refresh_rules("project", project.path()).unwrap();
        assert!(fs::read_to_string(&rules).unwrap().contains("TypeScript"));
        fs::remove_file(&manifest).unwrap();
        store.refresh_rules("project", project.path()).unwrap();
        let current: serde_json::Value = read(&store.rules_sources).unwrap();
        assert!(current["project"]["sources"].get("package.json").is_none());
        assert!(!fs::read_to_string(&rules).unwrap().contains("npm run lint"));

        let mut obsolete = current.clone();
        obsolete["project"]["version"] = 0.into();
        fs::write(&store.rules_sources, serde_json::to_vec(&obsolete).unwrap()).unwrap();
        store.refresh_rules("project", project.path()).unwrap();
        assert_eq!(read::<serde_json::Value>(&store.rules_sources).unwrap(), current);

        for damaged in ["{", "{}", r#"{"project":{"version":0,"sources":{},"hash":"old"}}"#] {
            fs::write(&store.rules_sources, damaged).unwrap();
            fs::write(&rules, "PLANTED_AGAIN").unwrap();
            store.refresh_rules("project", project.path()).unwrap();
            assert_eq!(read::<serde_json::Value>(&store.rules_sources).unwrap(), current);
            assert!(!fs::read_to_string(&rules).unwrap().contains("PLANTED"));
        }
        fs::remove_file(&store.rules_sources).unwrap();
        store.refresh_rules("project", project.path()).unwrap();
        assert_eq!(read::<serde_json::Value>(&store.rules_sources).unwrap(), current);
    }

    #[test]
    fn adopted_task_approvals_survive_reopen_without_trusting_repository_records() {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let store = Store::new(data.path());
        fs::create_dir(project.path().join("tasks")).unwrap();
        fs::write(project.path().join("tasks/b.md"), "second").unwrap();
        fs::write(project.path().join("tasks/a.md"), "first [by person]").unwrap();
        fs::write(project.path().join("decisions.md"), "existing decisions").unwrap();
        fs::write(project.path().join("taste.md"), "existing taste").unwrap();
        fs::write(project.path().join("adoptions.json"), r#"{"project":{"tasks":{"target":"tasks"}}}"#).unwrap();
        assert!(store.read_adopted_tasks("project", project.path()).unwrap().is_none());
        store.approve_adopted_file("project", project.path(), AdoptedKind::Decisions, "decisions.md").unwrap();
        store.approve_adopted_file("other", project.path(), AdoptedKind::Taste, "taste.md").unwrap();
        store.approve_adopted_tasks("project", project.path(), "tasks").unwrap();
        let reopened = Store::new(data.path());
        let tasks = reopened.read_adopted_tasks("project", project.path()).unwrap().unwrap();
        assert_eq!(tasks.files, [("a.md".into(), "first [by person]".into()), ("b.md".into(), "second".into())]);
        assert!(!tasks.incomplete);
        assert!(reopened.read_adopted_tasks("other", other.path()).unwrap().is_none());
        assert!(reopened.read_adopted_tasks("project", other.path()).is_err());
        assert_eq!(
            reopened.read_adopted_file("project", project.path(), AdoptedKind::Decisions).unwrap().as_deref(),
            Some("existing decisions")
        );
        assert_eq!(reopened.read_adopted_file("other", project.path(), AdoptedKind::Taste).unwrap().as_deref(), Some("existing taste"));
        assert_eq!(fs::read_to_string(project.path().join("tasks/a.md")).unwrap(), "first [by person]");
        assert!(!project.path().join(".faber").exists());
        assert!(!data.path().join("decisions-kept.json").exists());
    }

    #[test]
    fn adopted_task_approvals_refuse_file_kind_confusion_and_preserve_damaged_records() {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let store = Store::new(data.path());
        fs::create_dir(project.path().join("tasks")).unwrap();
        fs::write(project.path().join("tasks/a.md"), "original").unwrap();
        fs::write(project.path().join("ordinary.md"), "not a folder").unwrap();
        store.approve_adopted_tasks("project", project.path(), "tasks").unwrap();
        let before = fs::read(&store.adoptions).unwrap();
        assert!(store.approve_adopted_file("project", project.path(), AdoptedKind::Tasks, "ordinary.md").is_err());
        assert!(store.read_adopted_file("project", project.path(), AdoptedKind::Tasks).is_err());
        assert!(store.approve_adopted_tasks("project", project.path(), "ordinary.md").is_err());
        assert!(store.approve_adopted_tasks("project", project.path(), "../tasks").is_err());
        assert_eq!(fs::read(&store.adoptions).unwrap(), before);
        for damaged in [
            b"{broken".as_slice(),
            br#"{"project":{"unknown":{}}}"#,
            br#"{"project":{"tasks":{"version":99,"root":"/","target":"tasks"}}}"#,
        ] {
            fs::write(&store.adoptions, damaged).unwrap();
            assert!(store.read_adopted_tasks("project", project.path()).is_err());
            assert!(store.approve_adopted_tasks("project", project.path(), "tasks").is_err());
            assert_eq!(fs::read(&store.adoptions).unwrap(), damaged);
        }
    }

    #[test]
    fn adopted_task_approvals_recheck_replaced_folders_without_overwriting_the_record() {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let store = Store::new(data.path());
        fs::create_dir(project.path().join("tasks")).unwrap();
        fs::write(project.path().join("tasks/a.md"), "inside").unwrap();
        fs::write(outside.path().join("a.md"), "outside").unwrap();
        store.approve_adopted_tasks("project", project.path(), "tasks").unwrap();
        let before = fs::read(&store.adoptions).unwrap();
        fs::rename(project.path().join("tasks"), project.path().join("original-tasks")).unwrap();
        #[cfg(windows)]
        junction::create(outside.path(), project.path().join("tasks")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), project.path().join("tasks")).unwrap();
        assert!(store.read_adopted_tasks("project", project.path()).is_err());
        assert!(store.approve_adopted_tasks("project", project.path(), "tasks").is_err());
        assert_eq!(fs::read(&store.adoptions).unwrap(), before);
        assert_eq!(fs::read_to_string(outside.path().join("a.md")).unwrap(), "outside");
        #[cfg(windows)]
        junction::delete(project.path().join("tasks")).unwrap();
        #[cfg(unix)]
        fs::remove_file(project.path().join("tasks")).unwrap();
        assert_eq!(fs::read_to_string(project.path().join("original-tasks/a.md")).unwrap(), "inside");
    }

    #[test]
    fn adopted_task_approvals_failed_save_preserves_the_previous_folder() {
        struct NoReplace;
        impl Disk for NoReplace {
            fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
                RealDisk.write(path, bytes)
            }
            fn rename(&self, _: &Path, _: &Path) -> io::Result<()> {
                Err(io::ErrorKind::StorageFull.into())
            }
        }
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        for folder in ["tasks", "replacement"] {
            fs::create_dir(project.path().join(folder)).unwrap();
            fs::write(project.path().join(folder).join("a.md"), folder).unwrap();
        }
        let store = Store::new(data.path());
        store.approve_adopted_tasks("project", project.path(), "tasks").unwrap();
        let before = fs::read(&store.adoptions).unwrap();
        let failing = Store::with_disk(data.path(), Arc::new(NoReplace));
        assert_eq!(failing.approve_adopted_tasks("project", project.path(), "replacement"), Err("error.store.full"));
        assert_eq!(fs::read(&store.adoptions).unwrap(), before);
        assert_eq!(store.read_adopted_tasks("project", project.path()).unwrap().unwrap().files, [("a.md".into(), "tasks".into())]);
        assert!(!store.adoptions.with_file_name("adoptions.json.tmp").exists());
    }

    #[test]
    fn adopted_private_files_cannot_replace_an_approved_source() {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let store = Store::new(data.path());
        fs::write(project.path().join("decisions.md"), "approved decisions").unwrap();
        store.approve_adopted_file("project", project.path(), AdoptedKind::Decisions, "decisions.md").unwrap();
        let saved = fs::read(&store.adoptions).unwrap();
        for name in [".npmrc", "auth.json", "credentials.json", ".env", "private.pem", "private.key", "private.p12", "private.pfx"] {
            fs::write(project.path().join(name), "synthetic private record, never a real credential").unwrap();
            assert!(store.approve_adopted_file("project", project.path(), AdoptedKind::Decisions, name).is_err(), "{name}");
            assert_eq!(fs::read(&store.adoptions).unwrap(), saved);
        }
    }

    #[test]
    fn adopted_files_need_external_approval_and_recheck_the_selected_project() {
        use crate::project::memory::AdoptedKind;
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let store = Store::new(data.path());
        fs::write(project.path().join("decisions.md"), "original decisions").unwrap();
        fs::write(other.path().join("decisions.md"), "other project").unwrap();
        fs::write(project.path().join("adoptions.json"), r#"{"project":{"decisions":{"target":"decisions.md"}}}"#).unwrap();
        assert_eq!(store.read_adopted_file("project", project.path(), AdoptedKind::Decisions).unwrap(), None);
        store.approve_adopted_file("project", project.path(), AdoptedKind::Decisions, "decisions.md").unwrap();
        let reopened = Store::new(data.path());
        assert_eq!(
            reopened.read_adopted_file("project", project.path(), AdoptedKind::Decisions).unwrap().as_deref(),
            Some("original decisions")
        );
        assert!(reopened.read_adopted_file("project", other.path(), AdoptedKind::Decisions).is_err());
        assert_eq!(reopened.read_adopted_file("other", other.path(), AdoptedKind::Decisions).unwrap(), None);
        let saved = fs::read(data.path().join("faber-trust/adoptions.json")).unwrap();
        fs::remove_file(project.path().join("decisions.md")).unwrap();
        assert!(reopened.read_adopted_file("project", project.path(), AdoptedKind::Decisions).is_err());
        assert!(reopened.approve_adopted_file("project", project.path(), AdoptedKind::Taste, "../outside.md").is_err());
        assert_eq!(fs::read(data.path().join("faber-trust/adoptions.json")).unwrap(), saved);
        assert_eq!(fs::read_to_string(other.path().join("decisions.md")).unwrap(), "other project");
    }

    #[test]
    fn adopted_files_preserve_other_approvals_and_refuse_damaged_records() {
        use crate::project::memory::AdoptedKind;
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let store = Store::new(data.path());
        fs::write(project.path().join("decisions.md"), "decisions").unwrap();
        fs::write(project.path().join("taste.md"), "taste").unwrap();
        store.approve_adopted_file("project", project.path(), AdoptedKind::Decisions, "decisions.md").unwrap();
        store.approve_adopted_file("other", project.path(), AdoptedKind::Taste, "taste.md").unwrap();
        assert_eq!(store.read_adopted_file("project", project.path(), AdoptedKind::Decisions).unwrap().as_deref(), Some("decisions"));
        assert_eq!(store.read_adopted_file("other", project.path(), AdoptedKind::Taste).unwrap().as_deref(), Some("taste"));
        let path = data.path().join("faber-trust/adoptions.json");
        for damaged in [b"{broken".as_slice(), br#"{"project":{"decisions":{"version":99,"root":"/","target":"decisions.md"}}}"#] {
            fs::write(&path, damaged).unwrap();
            assert!(store.read_adopted_file("project", project.path(), AdoptedKind::Decisions).is_err());
            assert!(store.approve_adopted_file("project", project.path(), AdoptedKind::Decisions, "decisions.md").is_err());
            assert_eq!(fs::read(&path).unwrap(), damaged);
        }
    }

    #[test]
    fn adopted_files_recheck_approved_targets_after_a_directory_is_replaced() {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let store = Store::new(data.path());
        fs::create_dir(project.path().join("docs")).unwrap();
        fs::write(project.path().join("docs/decisions.md"), "inside").unwrap();
        fs::write(outside.path().join("decisions.md"), "outside").unwrap();
        store.approve_adopted_file("project", project.path(), AdoptedKind::Decisions, "docs/decisions.md").unwrap();
        let approval = fs::read(&store.adoptions).unwrap();
        fs::rename(project.path().join("docs"), project.path().join("original-docs")).unwrap();
        #[cfg(windows)]
        junction::create(outside.path(), project.path().join("docs")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), project.path().join("docs")).unwrap();
        assert!(store.read_adopted_file("project", project.path(), AdoptedKind::Decisions).is_err());
        assert!(store.approve_adopted_file("project", project.path(), AdoptedKind::Taste, "docs/decisions.md").is_err());
        assert_eq!(fs::read(&store.adoptions).unwrap(), approval);
        assert_eq!(fs::read_to_string(outside.path().join("decisions.md")).unwrap(), "outside");
        #[cfg(windows)]
        junction::delete(project.path().join("docs")).unwrap();
        #[cfg(unix)]
        fs::remove_file(project.path().join("docs")).unwrap();
    }

    #[test]
    fn adopted_files_failed_replacement_keeps_the_previous_approval() {
        struct NoReplace;
        impl Disk for NoReplace {
            fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
                RealDisk.write(path, bytes)
            }
            fn rename(&self, _: &Path, _: &Path) -> io::Result<()> {
                Err(io::ErrorKind::StorageFull.into())
            }
        }
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        fs::write(project.path().join("original.md"), "original").unwrap();
        fs::write(project.path().join("replacement.md"), "replacement").unwrap();
        let store = Store::new(data.path());
        store.approve_adopted_file("project", project.path(), AdoptedKind::Decisions, "original.md").unwrap();
        let before = fs::read(&store.adoptions).unwrap();
        let failing = Store::with_disk(data.path(), Arc::new(NoReplace));
        assert_eq!(
            failing.approve_adopted_file("project", project.path(), AdoptedKind::Decisions, "replacement.md"),
            Err("error.store.full")
        );
        assert_eq!(fs::read(&store.adoptions).unwrap(), before);
        assert_eq!(store.read_adopted_file("project", project.path(), AdoptedKind::Decisions).unwrap().as_deref(), Some("original"));
        assert!(!store.adoptions.with_file_name("adoptions.json.tmp").exists());
    }

    #[test]
    fn adopted_files_support_the_reference_decision_log_without_removing_the_size_bound() {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let store = Store::new(data.path());
        // The approved benchmark uses Faber's decision log, now larger than Memory's 256 KiB file cap.
        let reference = "x".repeat(270_492);
        fs::write(project.path().join("decisions.md"), &reference).unwrap();
        store.approve_adopted_file("project", project.path(), AdoptedKind::Decisions, "decisions.md").unwrap();
        assert_eq!(
            store.read_adopted_file("project", project.path(), AdoptedKind::Decisions).unwrap().as_deref(),
            Some(reference.as_str())
        );
        let saved = fs::read(&store.adoptions).unwrap();
        fs::write(project.path().join("oversize.md"), vec![b'x'; 512 * 1024 + 1]).unwrap();
        assert!(store.approve_adopted_file("project", project.path(), AdoptedKind::Taste, "oversize.md").is_err());
        assert_eq!(fs::read(&store.adoptions).unwrap(), saved);
    }

    #[test]
    fn creative_receipts_use_generated_paths_and_report_failed_writes() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let value = serde_json::json!({"first":"../outside", "reason":"Full reason"});
        let one = store.save_creative_review(&value).unwrap();
        let two = store.save_creative_review(&value).unwrap();
        assert_ne!(one, two);
        assert!(uuid::Uuid::parse_str(&one).is_ok());
        assert_eq!(read::<serde_json::Value>(&dir.path().join("creative-reviews").join(format!("{one}.json"))), Some(value));
        let blocked = tempfile::tempdir().unwrap();
        fs::write(blocked.path().join("creative-reviews"), "a file, not a folder").unwrap();
        assert_eq!(Store::new(blocked.path()).save_creative_review(&serde_json::json!({})), Err(Fault::Write));
    }

    #[test]
    fn kept_item_records_upgrade_without_losing_concurrent_or_other_project_approvals() {
        use crate::project::memory::{self, KeptDecision};
        let dir = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        fs::create_dir(project.path().join(".faber")).unwrap();
        fs::write(project.path().join(".faber/decisions.md"), "- Keep exports. [by AI]\n").unwrap();
        let store = Store::new(dir.path());
        let item = KeptDecision::of("- Keep exports.");
        let encoded = serde_json::to_value(&item).unwrap();
        let old = encoded["hash"].as_str().unwrap()[..32].to_string();
        let initial = serde_json::json!({"project": [old, "unmatched legacy"], "other": ["other approval"]});
        store.write(&store.decisions_kept, &initial).unwrap();
        let expected = store.kept_decisions("project");
        let list = memory::list(project.path()).unwrap();
        let upgraded = memory::upgrade_kept(&list, &expected);
        assert_eq!(upgraded, vec![item.clone(), KeptDecision::Legacy("unmatched legacy".into())]);
        assert!(store.migrate_kept_decisions("project", &expected, &upgraded).unwrap());
        assert_eq!(Store::new(dir.path()).kept_decisions("project"), upgraded);
        assert_eq!(store.kept_decisions("other"), vec![KeptDecision::Legacy("other approval".into())]);
        let before = fs::read(&store.decisions_kept).unwrap();
        assert!(!store.migrate_kept_decisions("project", &expected, &upgraded).unwrap());
        assert_eq!(fs::read(&store.decisions_kept).unwrap(), before);
        let second = KeptDecision::of("- Keep exports.\n  Also keep older formats.");
        store.keep_decision("project", second.clone(), &["project".into(), "other".into()]).unwrap();
        assert!(!store.migrate_kept_decisions("project", &upgraded, &expected).unwrap());
        let after = store.kept_decisions("project");
        assert!(after.contains(&item) && after.contains(&second), "same heading cannot delete a distinct approval");
        assert_eq!(store.kept_decisions("other"), vec![KeptDecision::Legacy("other approval".into())]);
    }

    #[test]
    fn kept_file_reference_records_preserve_mixed_approvals_and_metadata_cas() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let old = KeptDecision::of("- Keep exports.");
        let mut value = serde_json::to_value(&old).unwrap();
        value["files"] = serde_json::json!(["src/lib.rs"]);
        let enriched: KeptDecision = serde_json::from_value(value.clone()).unwrap();
        let live = vec!["project".into(), "other".into()];
        store.keep_decision("other", KeptDecision::Legacy("other approval".into()), &live).unwrap();
        store.keep_decision("project", KeptDecision::Legacy("unmatched legacy".into()), &live).unwrap();
        store.keep_decision("project", old, &live).unwrap();
        let expected = store.kept_decisions("project");
        store.keep_decision("project", enriched.clone(), &live).unwrap();
        let before = fs::read(&store.decisions_kept).unwrap();
        assert!(!store.migrate_kept_decisions("project", &expected, &expected).unwrap());
        assert_eq!(fs::read(&store.decisions_kept).unwrap(), before);
        assert_eq!(Store::new(dir.path()).kept_decisions("project"), [KeptDecision::Legacy("unmatched legacy".into()), enriched]);
        assert_eq!(store.kept_decisions("other"), [KeptDecision::Legacy("other approval".into())]);
        value["files"] = serde_json::json!("not an array");
        let corrupt = serde_json::to_vec(&serde_json::json!({"project": [value]})).unwrap();
        fs::write(&store.decisions_kept, &corrupt).unwrap();
        assert!(store.keep_decision("project", KeptDecision::of("- Another."), &live).is_err());
        assert_eq!(fs::read(&store.decisions_kept).unwrap(), corrupt);
    }

    #[test]
    fn kept_item_records_refuse_unknown_records_and_preserve_failed_migrations() {
        use crate::project::memory::KeptDecision;
        struct NoReplace;
        impl Disk for NoReplace {
            fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
                RealDisk.write(path, bytes)
            }
            fn rename(&self, _: &Path, _: &Path) -> io::Result<()> {
                Err(io::ErrorKind::StorageFull.into())
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let item = KeptDecision::of("- Keep exports.");
        for corrupt in [r#"{"project":[{"id":"id","hash":"hash","authority":true}]}"#, r#"{"project":[null]}"#] {
            fs::write(&store.decisions_kept, corrupt).unwrap();
            assert!(store.kept_decisions("project").is_empty());
            assert_eq!(store.keep_decision("project", item.clone(), &["project".into()]), Err(Fault::Corrupt));
            assert_eq!(store.migrate_kept_decisions("project", &[], &[]), Err(Fault::Corrupt));
            assert_eq!(fs::read_to_string(&store.decisions_kept).unwrap(), corrupt);
        }
        let old = KeptDecision::Legacy("old".into());
        store.write(&store.decisions_kept, &BTreeMap::from([("project", vec![old.clone()])])).unwrap();
        let before = fs::read(&store.decisions_kept).unwrap();
        let failed = Store::with_disk(dir.path(), Arc::new(NoReplace));
        assert!(failed.migrate_kept_decisions("project", &[old], &[item]).is_err());
        assert_eq!(fs::read(&store.decisions_kept).unwrap(), before);
    }

    #[test]
    fn keeping_a_decision_refuses_corrupt_approvals_without_overwriting_them() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let corrupt = b"{\"project\":[\"existing approval\"],";
        fs::write(&store.decisions_kept, corrupt).unwrap();
        assert_eq!(store.keep_decision("project", KeptDecision::Legacy("new approval".into()), &["project".into()]), Err(Fault::Corrupt));
        assert_eq!(fs::read(&store.decisions_kept).unwrap(), corrupt);
    }

    #[test]
    fn keeping_a_decision_preserves_more_than_500_existing_approvals() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let records: Vec<_> = (0..501).map(|i| format!("{i:032x}")).collect();
        store.write(&store.decisions_kept, &BTreeMap::from([("project", &records)])).unwrap();
        store.keep_decision("project", KeptDecision::Legacy("new approval".into()), &["project".into()]).unwrap();
        let kept = Store::new(dir.path()).kept_decisions("project");
        assert_eq!(kept.len(), 502, "a new approval must not discard older approvals");
        assert_eq!(&kept[..501], records.into_iter().map(KeptDecision::Legacy).collect::<Vec<_>>());
        assert_eq!(kept.last(), Some(&KeptDecision::Legacy("new approval".into())));
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

//! Plain JSON files in the app's data folder: readable by people and by any future tool.
//! Writes go to a temp file first, so a crash never leaves a half-written conversation.
//!
//! Every write that can carry authority or spending (projects with their grants, the quota ledger, mission records)
//! returns a [`Fault`] the screen can translate. Their checked readers tell a missing file from a damaged one: only the
//! former is empty, and a damaged safety file is never read as no data.

use crate::ledger::Ledger;
use crate::model::{Conversation, Project, Settings, Summary};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    let tmp = path.with_file_name(name);
    disk.write(&tmp, bytes).and_then(|()| disk.rename(&tmp, path)).map_err(|e| {
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
    projects: PathBuf,
    settings: PathBuf,
    budget: PathBuf,
    disk: Arc<dyn Disk>,
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
            conversations,
            projects: data_dir.join("projects.json"),
            settings: data_dir.join("settings.json"),
            budget: data_dir.join("budget.json"),
            disk,
        }
    }

    fn write<T: Serialize>(&self, path: &Path, value: &T) -> Result<(), Fault> {
        replace(&*self.disk, path, serde_json::to_string_pretty(value).expect("plain data always serializes").as_bytes())
    }

    fn file(&self, id: &str) -> PathBuf {
        self.conversations.join(format!("{id}.json"))
    }

    pub fn list(&self) -> Vec<Summary> {
        let mut list: Vec<Summary> = fs::read_dir(&self.conversations)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| read::<Conversation>(&e.path()))
            .map(|c| Summary { id: c.id, title: c.title, area: c.area, project_id: c.project_id, updated_at: c.updated_at })
            .collect();
        list.sort_by_key(|c| std::cmp::Reverse(c.updated_at));
        list
    }

    pub fn get(&self, id: &str) -> Option<Conversation> {
        // Ids are UUIDs; anything else could point outside the folder.
        if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return None;
        }
        read(&self.file(id))
    }

    pub fn save(&self, conversation: &Conversation) {
        let _ = self.write(&self.file(&conversation.id), conversation);
    }

    pub fn remove(&self, id: &str) {
        if self.get(id).is_some() {
            let _ = fs::remove_file(self.file(id));
        }
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

    pub fn settings(&self) -> Settings {
        read(&self.settings).unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<(), Fault> {
        self.write(&self.settings, settings)
    }

    fn quarantine(&self) -> PathBuf {
        self.budget.with_file_name("budget.json.quarantine")
    }

    /// Reads `budget.json`; a damaged file is copied aside once, as evidence, before the error is returned.
    fn read_ledger(&self) -> Result<Ledger, Fault> {
        match read_checked::<Ledger>(&self.budget) {
            Ok(found) => {
                let mut ledger = found.unwrap_or_default();
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

    /// Compact on purpose: it is rewritten often and a person never has to read it.
    pub fn save_ledger(&self, ledger: &Ledger) -> Result<(), Fault> {
        replace(&*self.disk, &self.budget, serde_json::to_string(ledger).expect("plain data always serializes").as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

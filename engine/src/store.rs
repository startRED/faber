//! Plain JSON files in the app's data folder: readable by people and by any future tool.
//! Writes go to a temp file first, so a crash never leaves a half-written conversation.

use crate::ledger::Ledger;
use crate::model::{Conversation, Project, Settings, Summary};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub struct Store {
    conversations: PathBuf,
    projects: PathBuf,
    settings: PathBuf,
    budget: PathBuf,
    cache_health: PathBuf,
}

/// Writes through a temp file. On failure the temp file is removed and the error returned; the old file stays as it was.
fn write<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    write_text(path, &serde_json::to_string_pretty(value).expect("plain data always serializes"))
}

fn write_text(path: &Path, json: &str) -> io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).and_then(|()| fs::rename(&tmp, path)).inspect_err(|_| drop(fs::remove_file(&tmp)))
}

fn read<T: DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

impl Store {
    pub fn new(data_dir: &Path) -> Self {
        let conversations = data_dir.join("conversations");
        let _ = fs::create_dir_all(&conversations);
        Store {
            conversations,
            projects: data_dir.join("projects.json"),
            settings: data_dir.join("settings.json"),
            budget: data_dir.join("budget.json"),
            cache_health: data_dir.join("cache-health.jsonl"),
        }
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
        let _ = write(&self.file(&conversation.id), conversation);
    }

    pub fn remove(&self, id: &str) {
        if self.get(id).is_some() {
            let _ = fs::remove_file(self.file(id));
        }
    }

    pub fn projects(&self) -> Vec<Project> {
        read(&self.projects).unwrap_or_default()
    }

    pub fn save_projects(&self, projects: &[Project]) {
        let _ = write(&self.projects, &projects);
    }

    pub fn settings(&self) -> Settings {
        read(&self.settings).unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &Settings) -> io::Result<()> {
        write(&self.settings, settings)
    }

    /// What Faber remembers about each vendor's quota. A missing, damaged or unreadable file is an empty ledger: it only
    /// holds what was learned, and learning starts again.
    pub fn ledger(&self) -> Ledger {
        let mut ledger: Ledger = read(&self.budget).unwrap_or_default();
        ledger.sanitize();
        ledger
    }

    /// Compact on purpose: it is rewritten often and a person never has to read it.
    pub fn save_ledger(&self, ledger: &Ledger) {
        let _ = write_text(&self.budget, &serde_json::to_string(ledger).expect("plain data always serializes"));
    }
}

// ponytail: size cap, rotate if it ever matters.
const CACHE_HEALTH_MAX: u64 = 1_000_000;

impl Store {
    /// Adds one line to the local cache-health log. Nothing reads it back and nothing sends it anywhere; it stops growing
    /// at 1 MB and a failed write is ignored, so it can never break a turn.
    pub fn log_cache_health(&self, line: &serde_json::Value) {
        use std::io::Write;
        if fs::metadata(&self.cache_health).is_ok_and(|m| m.len() >= CACHE_HEALTH_MAX) {
            return;
        }
        let _ = fs::OpenOptions::new().create(true).append(true).open(&self.cache_health).and_then(|mut f| {
            f.write_all(
                format!(
                    "{line}
"
                )
                .as_bytes(),
            )
        });
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
        assert!(write(&target, &Settings::default()).is_err());
        assert!(!dir.path().join("settings.json.tmp").exists());
        assert!(target.join("inside").is_dir(), "the destination was left alone");
    }
}

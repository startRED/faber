//! Plain JSON files in the app's data folder: readable by people and by any future tool.
//! Writes go to a temp file first, so a crash never leaves a half-written conversation.

use crate::model::{Conversation, Project, Settings, Summary};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Store {
    conversations: PathBuf,
    projects: PathBuf,
    settings: PathBuf,
}

fn write<T: Serialize>(path: &Path, value: &T) {
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(value).expect("plain data always serializes");
    if fs::write(&tmp, json).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}

fn read<T: DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

impl Store {
    pub fn new(data_dir: &Path) -> Self {
        let conversations = data_dir.join("conversations");
        let _ = fs::create_dir_all(&conversations);
        Store { conversations, projects: data_dir.join("projects.json"), settings: data_dir.join("settings.json") }
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
        write(&self.file(&conversation.id), conversation);
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
        write(&self.projects, &projects);
    }

    pub fn settings(&self) -> Settings {
        read(&self.settings).unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &Settings) {
        write(&self.settings, settings);
    }
}

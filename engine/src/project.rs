//! What a project conversation stands on: its separate copy (`copy`), the checks it runs (`checks`, `proof`), the git it
//! reads (`git`), the pull requests it follows (`remote`) and the budget of its unbounded turns (`budget`). The contracts
//! are in `docs/design/projects.md`.
//!
//! The records keep the fields a newer Faber wrote in `extra`, so an older build can read and rewrite them without dropping
//! anything.

pub mod budget;
pub mod checks;
pub mod copy;
pub mod git;
pub mod goals;
pub mod lease;
pub mod memory;
pub mod proof;
pub mod remote;
pub mod rules;

use crate::model::Project;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

/// Fields a newer Faber wrote that this build does not know, kept as they were.
pub type Extra = BTreeMap<String, Value>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FolderState {
    #[default]
    Present,
    Missing,
    Disconnected,
}

impl FolderState {
    pub fn is_present(self) -> bool {
        self == Self::Present
    }
}

impl Project {
    pub fn state(&self) -> FolderState {
        self.extra.get("state").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or(FolderState::Present)
    }

    pub fn set_state(&mut self, state: FolderState) {
        if let Ok(val) = serde_json::to_value(state) {
            self.extra.insert("state".into(), val);
        }
    }
}

/// Checks a project folder with one metadata call, plus one for the volume root only when missing on a mounted-volume path.
/// Reads no directory contents or files.
pub fn folder_state(path: &Path) -> FolderState {
    folder_state_with(path, &mut |p| std::fs::metadata(p).map(|m| m.is_dir()))
}

fn folder_state_with(path: &Path, metadata: &mut impl FnMut(&Path) -> std::io::Result<bool>) -> FolderState {
    match metadata(path) {
        Ok(true) => FolderState::Present,
        Ok(false) => FolderState::Missing,
        Err(err) => classify_missing(path, &err, metadata),
    }
}

/// Fills each project's live `state` from its folder and, when needed, its volume root.
pub fn with_folder_states(projects: Vec<Project>) -> Vec<Project> {
    with_folder_states_using(projects, &mut |p| std::fs::metadata(p).map(|m| m.is_dir()))
}

fn with_folder_states_using(mut projects: Vec<Project>, metadata: &mut impl FnMut(&Path) -> std::io::Result<bool>) -> Vec<Project> {
    for project in &mut projects {
        let state = folder_state_with(Path::new(&project.path), metadata);
        project.set_state(state);
    }
    projects
}

fn classify_missing(path: &Path, err: &std::io::Error, metadata: &mut impl FnMut(&Path) -> std::io::Result<bool>) -> FolderState {
    if is_disconnected_os_error(err.raw_os_error()) {
        return FolderState::Disconnected;
    }
    if let Some(root) = volume_root(path) {
        if cfg!(windows) && err.raw_os_error() == Some(2) && path != root {
            return FolderState::Missing;
        }

        if !metadata(&root).unwrap_or(false) {
            return FolderState::Disconnected;
        }
    }
    FolderState::Missing
}

fn is_disconnected_os_error(code: Option<i32>) -> bool {
    let Some(code) = code else { return false };
    if cfg!(windows) {
        // ERROR_INVALID_DRIVE (15), ERROR_NOT_READY (21), ERROR_BAD_NETPATH (53), ERROR_DEV_NOT_EXIST (55),
        // ERROR_NETNAME_DELETED (64), ERROR_BAD_NET_NAME (67), ERROR_DEVICE_NOT_CONNECTED (1167),
        // ERROR_NO_NETWORK (1222), ERROR_NETWORK_UNREACHABLE (1231), ERROR_HOST_UNREACHABLE (1232).
        matches!(code, 15 | 21 | 53 | 55 | 64 | 67 | 1167 | 1222 | 1231 | 1232)
    } else {
        // EIO (5), ENXIO (6), ENODEV (19), ENOTCONN (107), EHOSTDOWN (112), EHOSTUNREACH (113).
        matches!(code, 5 | 6 | 19 | 107 | 112 | 113)
    }
}

fn volume_root(path: &Path) -> Option<PathBuf> {
    if let Some(Component::Prefix(prefix)) = path.components().next() {
        let mut root = PathBuf::from(prefix.as_os_str());
        root.push(std::path::MAIN_SEPARATOR_STR);
        return Some(root);
    }
    let raw = path.to_string_lossy();
    let bytes = raw.as_bytes();
    if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'/' | b'\\') {
        return Some(PathBuf::from(&raw[..3]));
    }
    let mut parts = path.components();
    if matches!(parts.next(), Some(Component::RootDir))
        && let Some(Component::Normal(mount)) = parts.next()
        && matches!(mount.to_str(), Some("Volumes" | "mnt" | "media"))
        && let Some(Component::Normal(volume)) = parts.next()
        && parts.next().is_some()
    {
        return Some(Path::new("/").join(mount).join(volume));
    }
    None
}

/// Whether `id` can name a record file: 1 to 64 ASCII letters, digits, `-` or `_`.
pub fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// A process by its id and the moment it started: the start time tells a reused id from the original.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessId {
    pub pid: u32,
    pub started_at: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_320_twenty_project_refresh_uses_twenty_metadata_calls_and_zero_content_reads() {
        let dir = tempfile::tempdir().unwrap();
        let mut projects = Vec::new();
        for i in 0..20 {
            let folder = dir.path().join(format!("p-{i}"));
            if i % 2 == 0 {
                std::fs::create_dir(&folder).unwrap();
            }
            projects.push(Project::new(format!("id-{i}"), format!("p-{i}"), folder.to_string_lossy().into_owned()));
        }
        let mut calls = 0;
        let checked = with_folder_states_using(projects, &mut |p| {
            calls += 1;
            std::fs::metadata(p).map(|m| m.is_dir())
        });

        assert_eq!(calls, 20, "one metadata check per project and zero content reads");
        for (i, p) in checked.iter().enumerate() {
            assert_eq!(p.state(), if i % 2 == 0 { FolderState::Present } else { FolderState::Missing });
        }
        if cfg!(windows) && !Path::new("Q:/").exists() {
            assert_eq!(folder_state(Path::new("Q:/missing-drive/project")), FolderState::Disconnected);
        }
        assert_eq!(
            classify_missing(Path::new("/Volumes/Unmounted/project"), &std::io::Error::from_raw_os_error(6), &mut |_| Ok(false)),
            FolderState::Disconnected
        );
    }
}

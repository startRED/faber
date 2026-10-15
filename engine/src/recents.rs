//! The last folders the person worked in and the environments they saved (named sets of folders), kept in `recents.json`
//! next to the settings. A folder is stored as the real path `permissions::checked_folder` returns, so two spellings of one
//! folder are one entry, and a folder Faber refuses (R124) is never stored. Whether a stored folder is still usable is
//! judged again each time the list is shown.

use crate::permissions::checked_folder;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// How many recent folders are kept.
pub const MAX_RECENT: usize = 5;
const MAX_ENVIRONMENTS: usize = 12;
const MAX_FOLDERS: usize = 8;
const MAX_NAME: usize = 60;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Recents {
    /// Newest first.
    folders: Vec<String>,
    environments: Vec<Environment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Environment {
    pub id: String,
    pub name: String,
    pub folders: Vec<String>,
}

/// A stored folder as the screen shows it. `reason` is a key (`reason.recents.*`) when the folder cannot be picked.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FolderView {
    pub path: String,
    pub name: String,
    pub parent: String,
    pub reason: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EnvironmentView {
    pub id: String,
    pub name: String,
    pub folders: Vec<FolderView>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct RecentsView {
    pub folders: Vec<FolderView>,
    pub environments: Vec<EnvironmentView>,
}

/// Paths are equal when they name the same place; on Windows letter case does not count.
pub fn same(a: &str, b: &str) -> bool {
    if cfg!(windows) { a.eq_ignore_ascii_case(b) } else { a == b }
}

/// Whether two spellings name the same folder: both are resolved first, so a stored entry that has since become a link to the
/// other (or was written another way) still counts.
pub fn same_place(a: &str, b: &str) -> bool {
    let real = |p: &str| resolved(p).unwrap_or_else(|| p.to_string());
    same(&real(a), &real(b))
}

/// The path to store for `path`: the checked real path, or `None` when the folder is refused.
pub fn resolved(path: &str) -> Option<String> {
    checked_folder(Path::new(path)).map(|p| p.to_string_lossy().into_owned())
}

fn view(path: &str) -> FolderView {
    let p = Path::new(path);
    let reason = if checked_folder(p).is_none() {
        Some("reason.recents.refused")
    } else if !p.is_dir() {
        Some("reason.recents.gone")
    } else {
        None
    };
    let name = p.file_name().map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned());
    let parent = p.parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
    FolderView { path: path.to_string(), name, parent, reason }
}

impl Recents {
    /// Puts the folder first, once. Returns the stored path, or `None` when the folder is refused and nothing changes.
    pub fn used(&mut self, path: &str) -> Option<String> {
        let path = resolved(path)?;
        self.folders.retain(|f| !same_place(f, &path));
        self.folders.insert(0, path.clone());
        self.folders.truncate(MAX_RECENT);
        Some(path)
    }

    pub fn forget(&mut self, path: &str) {
        self.folders.retain(|f| !same_place(f, path));
    }

    /// Saves (or replaces, when the name is already used) an environment. Errors are translation keys.
    pub fn save_environment(&mut self, id: String, name: &str, folders: &[String]) -> Result<(), &'static str> {
        let name: String = name.trim().chars().take(MAX_NAME).collect();
        if name.is_empty() {
            return Err("error.environment.name");
        }
        let mut stored: Vec<String> = Vec::new();
        for folder in folders {
            let folder = resolved(folder).ok_or("error.workingFolder")?;
            if !stored.iter().any(|f| same_place(f, &folder)) {
                stored.push(folder);
            }
        }
        if stored.is_empty() || stored.len() > MAX_FOLDERS {
            return Err("error.environment.folders");
        }
        match self.environments.iter().position(|e| e.name.to_lowercase() == name.to_lowercase()) {
            Some(at) => (self.environments[at].name, self.environments[at].folders) = (name, stored),
            None if self.environments.len() >= MAX_ENVIRONMENTS => return Err("error.environment.full"),
            None => self.environments.push(Environment { id, name, folders: stored }),
        }
        Ok(())
    }

    /// The data as read from disk, cut to the same limits `used` and `save_environment` keep, since the file can be edited by hand.
    pub fn bounded(mut self) -> Self {
        self.folders.retain(|f| !f.is_empty());
        self.folders.truncate(MAX_RECENT);
        self.environments.retain_mut(|e| {
            e.name = e.name.trim().chars().take(MAX_NAME).collect();
            e.folders.retain(|f| !f.is_empty());
            e.folders.truncate(MAX_FOLDERS);
            !e.name.is_empty() && !e.folders.is_empty()
        });
        self.environments.truncate(MAX_ENVIRONMENTS);
        self
    }

    pub fn forget_environment(&mut self, id: &str) {
        self.environments.retain(|e| e.id != id);
    }

    pub fn environment(&self, id: &str) -> Option<&Environment> {
        self.environments.iter().find(|e| e.id == id)
    }

    pub fn view(&self) -> RecentsView {
        RecentsView {
            folders: self.folders.iter().map(|f| view(f)).collect(),
            environments: self
                .environments
                .iter()
                .map(|e| EnvironmentView { id: e.id.clone(), name: e.name.clone(), folders: e.folders.iter().map(|f| view(f)).collect() })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(n: usize) -> (tempfile::TempDir, Vec<String>) {
        // System temp can sit in a folder Faber refuses, so the folders live in the test workspace.
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/test-workspaces");
        std::fs::create_dir_all(&base).unwrap();
        let root = tempfile::tempdir_in(base).unwrap();
        let paths = (0..n)
            .map(|i| {
                let d = root.path().join(format!("p{i}"));
                std::fs::create_dir(&d).unwrap();
                d.to_string_lossy().into_owned()
            })
            .collect();
        (root, paths)
    }

    #[test]
    fn keeps_five_newest_first() {
        let (_root, p) = dirs(7);
        let mut r = Recents::default();
        for f in &p {
            r.used(f).unwrap();
        }
        let names: Vec<_> = r.view().folders.into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["p6", "p5", "p4", "p3", "p2"]);
    }

    #[test]
    fn using_a_folder_again_moves_it_first_and_spellings_are_one_entry() {
        let (_root, p) = dirs(3);
        let mut r = Recents::default();
        for f in &p {
            r.used(f).unwrap();
        }
        r.used(&format!("{}{}.", p[0], std::path::MAIN_SEPARATOR)).unwrap();
        let v = r.view().folders;
        assert_eq!(v.len(), 3);
        assert_eq!(v[0].name, "p0");
    }

    #[test]
    fn a_missing_folder_is_kept_but_marked_gone_and_a_refused_one_is_never_stored() {
        let (root, p) = dirs(1);
        let mut r = Recents::default();
        r.used(&p[0]).unwrap();
        std::fs::remove_dir(&p[0]).unwrap();
        assert_eq!(r.view().folders[0].reason, Some("reason.recents.gone"));
        let ssh = root.path().join(".ssh");
        std::fs::create_dir(&ssh).unwrap();
        assert_eq!(r.used(&ssh.to_string_lossy()), None);
        assert_eq!(r.view().folders.len(), 1);
        // A folder stored before it became refused shows as refused.
        r.folders.insert(0, ssh.to_string_lossy().into_owned());
        assert_eq!(r.view().folders[0].reason, Some("reason.recents.refused"));
    }

    #[test]
    fn a_stored_spelling_of_the_same_folder_is_replaced_not_kept_beside_it() {
        let (_root, p) = dirs(1);
        let sep = std::path::MAIN_SEPARATOR;
        let mut r = Recents { folders: vec![format!("{}{sep}..{sep}p0", p[0])], ..Recents::default() };
        r.used(&p[0]).unwrap();
        assert_eq!(r.view().folders.len(), 1);
    }

    #[test]
    fn loaded_data_is_cut_to_the_limits() {
        let (_root, p) = dirs(2);
        let many = |n: usize| p.iter().cycle().take(n).cloned().collect::<Vec<_>>();
        let env = |i: usize, name: &str, n: usize| Environment { id: format!("e{i}"), name: name.into(), folders: many(n) };
        let mut environments: Vec<_> = (0..15).map(|i| env(i, &format!("n{i}"), 12)).collect();
        environments.push(env(98, "  ", 2));
        environments.push(env(99, "none", 0));
        let r = Recents { folders: many(9), environments }.bounded();
        assert_eq!(r.folders.len(), MAX_RECENT);
        assert_eq!(r.environments.len(), MAX_ENVIRONMENTS);
        assert!(r.environments.iter().all(|e| e.folders.len() == MAX_FOLDERS));
    }

    #[test]
    fn a_folder_can_be_forgotten() {
        let (_root, p) = dirs(2);
        let mut r = Recents::default();
        r.used(&p[0]).unwrap();
        r.used(&p[1]).unwrap();
        r.forget(&p[1]);
        assert_eq!(r.view().folders.len(), 1);
    }

    #[test]
    fn environments_check_name_and_folders_and_replace_by_name() {
        let (root, p) = dirs(2);
        let mut r = Recents::default();
        assert_eq!(r.save_environment("a".into(), "  ", &p), Err("error.environment.name"));
        assert_eq!(r.save_environment("a".into(), "Work", &[]), Err("error.environment.folders"));
        let ssh = root.path().join(".ssh");
        std::fs::create_dir(&ssh).unwrap();
        assert_eq!(r.save_environment("a".into(), "Work", &[ssh.to_string_lossy().into_owned()]), Err("error.workingFolder"));
        let sep = std::path::MAIN_SEPARATOR;
        r.save_environment("a".into(), "Work", &[p[0].clone(), format!("{}{sep}.", p[0]), p[1].clone()]).unwrap();
        assert_eq!(r.view().environments[0].folders.len(), 2, "two spellings of one folder are one");
        r.save_environment("b".into(), "work", &p[..1]).unwrap();
        let v = r.view().environments;
        assert_eq!((v.len(), v[0].id.as_str(), v[0].name.as_str(), v[0].folders.len()), (1, "a", "work", 1));
        r.forget_environment("a");
        assert!(r.view().environments.is_empty());
    }
}

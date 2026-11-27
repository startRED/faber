use super::{Item, Store, small_file, valid_id};
use crate::project::lease::real_path;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const FOLDER: &str = "locations";

fn record(store: &Path, id: &str) -> PathBuf {
    store.join(FOLDER).join(format!("{id}.json"))
}

pub(super) fn external(store: &Path, root: &Path) -> bool {
    store.parent().zip(root.parent()).is_some_and(|(data, parent)| {
        root.file_name().is_some_and(|name| name == "tools")
            && crate::storage::external_root_name(parent)
            && real_path(root).is_ok()
            && real_path(&parent.join(".faber-owner")).is_ok()
            && crate::storage::owns_external_root(data, parent)
    })
}

pub(super) fn source_root(store: &Path, id: &str) -> PathBuf {
    let unavailable = || store.join("installed.json").join(".unavailable");
    if !valid_id(id) {
        return unavailable();
    }
    let path = record(store, id);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => store.to_path_buf(),
        _ => real_path(&path)
            .ok()
            .and_then(|path| small_file(&path))
            .and_then(|bytes| serde_json::from_slice::<PathBuf>(&bytes).ok())
            .filter(|root| external(store, root))
            // A known location stays reserved while the drive is unavailable.
            .unwrap_or_else(unavailable),
    }
}

impl Store {
    pub(super) fn recorded_root(&self, id: &str) -> io::Result<Option<PathBuf>> {
        if !valid_id(id) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let file = record(&self.root, id);
        real_path(&file).map_err(|_| io::ErrorKind::InvalidInput)?;
        match fs::symlink_metadata(&file) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Ok(_) => {
                let bytes = small_file(&file).ok_or(io::ErrorKind::InvalidData)?;
                serde_json::from_slice(&bytes).map(Some).map_err(io::Error::other)
            }
            Err(error) => Err(error),
        }
    }

    pub fn with_new_storage(mut self, root: Option<&Path>) -> Self {
        self.new_root = root.map(|root| root.join("tools"));
        self
    }

    pub(super) fn source_root(&self, id: &str) -> PathBuf {
        source_root(&self.root, id)
    }

    /// The item was stored on a chosen drive that is away now.
    pub fn location_unavailable(&self, id: &str) -> bool {
        let root = self.source_root(id);
        valid_id(id) && root != self.root && !external(&self.root, &root)
    }

    pub(super) fn download_root(&self, item: &Item) -> Result<PathBuf, String> {
        let existing = self.source_root(&item.id);
        if existing != self.root && !external(&self.root, &existing) {
            // Its recorded drive is away: say so, and never install a second copy somewhere else.
            return Err(if valid_id(&item.id) { "error.storage.unreachable" } else { "error.tools.install" }.into());
        }
        if self.initial_ledger()?.contains_key(&item.id) || self.occupied_install(item)? || self.has_old_move(&item.id) {
            return Ok(existing);
        }
        let root = self.new_root.as_ref().unwrap_or(&self.root);
        if self.new_root.is_some() {
            self.check_file_root(root).map_err(|_| "error.storage.unreachable")?;
        }
        Ok(root.clone())
    }

    pub(super) fn record_location(&self, id: &str, root: &Path) -> io::Result<()> {
        let file = record(&self.root, id);
        if !valid_id(id) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if root == self.root {
            if matches!(fs::symlink_metadata(&file), Err(error) if error.kind() == io::ErrorKind::NotFound) {
                return Ok(());
            }
            real_path(&file).map_err(|_| io::ErrorKind::InvalidInput)?;
            return match fs::remove_file(file) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                result => result,
            };
        }
        if real_path(&file).is_err() || !external(&self.root, root) {
            return Err(io::ErrorKind::NotFound.into());
        }
        let parent = self.root.join(FOLDER);
        fs::create_dir_all(&parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(&serde_json::to_vec(root).map_err(io::Error::other)?)?;
        temp.as_file().sync_all()?;
        faber_winproc::publish_storage_record(temp.path(), &file)?;
        // Once published, the location is authoritative even if a later metadata flush fails.
        let _ = faber_winproc::sync_storage_publication(&self.root, &[Path::new(FOLDER).join(format!("{id}.json"))]);
        Ok(())
    }

    pub(super) fn file_roots(&self) -> std::collections::BTreeSet<PathBuf> {
        std::iter::once(self.root.clone())
            .chain(
                self.items
                    .iter()
                    .map(|item| self.source_root(&item.id))
                    .chain(self.new_root.iter().cloned())
                    .filter(|root| root != &self.root && self.check_file_root(root).is_ok()),
            )
            .collect()
    }

    pub(super) fn check_file_root(&self, root: &Path) -> Result<(), String> {
        if root != self.root && !external(&self.root, root) {
            return Err("error.storage.unreachable".into());
        }
        for path in std::iter::once(root.to_path_buf()).chain(
            ["plugin", "plugin/skills", "servers", "plugins", "runtimes", "disabled", "removed", "replacements", "runtime-pins"]
                .map(|sub| root.join(sub)),
        ) {
            match fs::symlink_metadata(&path) {
                Ok(_) => super::replacement::plain(&path, true)?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
            if root != self.root {
                real_path(&path).map_err(|_| "The tool store contains a linked or invalid directory")?;
            }
        }
        Ok(())
    }
}

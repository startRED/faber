//! Recover a private chat root using authority kept outside its writable contents.
use faber_winproc::MemoryDirectory;
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

const RECORD: &str = "chat-workspace.json";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    data_parent: String,
    workspace_parent: String,
    workspace_leaf: String,
    root: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    BeforeRecord,
    Recorded,
    Moved,
}

fn refused() -> io::Error {
    io::ErrorKind::PermissionDenied.into()
}

fn identity(parent: &MemoryDirectory, leaf: &str) -> io::Result<Option<String>> {
    match parent.under(Path::new(leaf)) {
        Ok(child) => child.identity().map(Some),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// The caller supplies existing parents, with Faber data protected from AI writes. This does not change engine paths yet.
/// File flushing and an atomic rename cover process interruption, not a guarantee against power loss.
pub fn relocate(data: &Path, workspace_parent: &Path, leaf: &str) -> io::Result<PathBuf> {
    relocate_staged(data, workspace_parent, leaf, |_| Ok(()))
}

fn relocate_staged(
    data: &Path,
    workspace_parent: &Path,
    leaf: &str,
    mut stage: impl FnMut(Stage) -> io::Result<()>,
) -> io::Result<PathBuf> {
    let mut parts = Path::new(leaf).components();
    if !matches!(parts.next(), Some(Component::Normal(_))) || parts.next().is_some() || leaf.contains(['/', '\\', ':']) {
        return Err(refused());
    }
    let data = MemoryDirectory::open(data, Path::new(""))?;
    let parent = MemoryDirectory::open(workspace_parent, Path::new(""))?;
    let protected = data.resolved_path()?;
    let destination = parent.resolved_path()?.join(leaf);
    if destination.starts_with(&protected) || protected.starts_with(&destination) {
        return Err(refused());
    }
    let record = match data.file(RECORD, false) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(4097).read_to_end(&mut bytes)?;
            if bytes.len() > 4096 {
                return Err(refused());
            }
            serde_json::from_slice::<Record>(&bytes).map_err(|_| refused())?
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if identity(&parent, leaf)?.is_some() {
                return Err(refused());
            }
            let root = match identity(&data, "chat")? {
                Some(root) => root,
                None => data.create_under("chat")?.identity()?,
            };
            let record = Record {
                version: 1,
                data_parent: data.identity()?,
                workspace_parent: parent.identity()?,
                workspace_leaf: leaf.to_string(),
                root,
            };
            stage(Stage::BeforeRecord)?;
            let temporary = format!("chat-workspace-{}.tmp", uuid::Uuid::new_v4());
            let mut file = data.file(&temporary, true)?;
            let saved = (|| {
                serde_json::to_writer(&mut file, &record)?;
                file.flush()?;
                file.sync_all()?;
                data.publish(&file, RECORD)
            })();
            if saved.is_err() {
                let _ = data.discard(&file);
            }
            saved?;
            drop(file);
            record
        }
        Err(error) => return Err(error),
    };
    if record.version != 1
        || record.data_parent != data.identity()?
        || record.workspace_parent != parent.identity()?
        || record.workspace_leaf != leaf
    {
        return Err(refused());
    }
    stage(Stage::Recorded)?;
    match (identity(&data, "chat")?, identity(&parent, leaf)?) {
        (Some(source), None) if source == record.root => {
            data.relocate_directory("chat", &parent, leaf, &record.root)?;
            stage(Stage::Moved)?;
        }
        (None, Some(target)) if target == record.root => {}
        _ => return Err(refused()),
    }
    if identity(&data, "chat")?.is_some() || identity(&parent, leaf)?.as_deref() != Some(record.root.as_str()) {
        return Err(refused());
    }
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let owned = tempfile::tempdir().unwrap();
        let root = owned.path().canonicalize().unwrap();
        let data = root.join("data");
        let work = root.join("work");
        std::fs::create_dir(&data).unwrap();
        std::fs::create_dir(&work).unwrap();
        std::fs::create_dir(data.join("chat")).unwrap();
        std::fs::write(data.join("chat/answer.txt"), b"saved work").unwrap();
        (owned, data, work)
    }

    #[test]
    fn chat_relocation_recovers_each_interruption_without_copying_or_replacing_work() {
        for interrupted in [Stage::BeforeRecord, Stage::Recorded, Stage::Moved] {
            let (_owned, data, work) = fixture();
            let error = relocate_staged(&data, &work, "private", |current| {
                if current == interrupted { Err(io::ErrorKind::Interrupted.into()) } else { Ok(()) }
            })
            .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::Interrupted);
            let destination = relocate(&data, &work, "private").unwrap();
            assert_eq!(std::fs::read(destination.join("answer.txt")).unwrap(), b"saved work");
            assert!(!data.join("chat").exists());
            let record = std::fs::read(data.join(RECORD)).unwrap();
            assert_eq!(relocate(&data, &work, "private").unwrap(), destination);
            assert_eq!(std::fs::read(data.join(RECORD)).unwrap(), record);
        }
    }

    #[test]
    fn chat_relocation_refuses_unrecorded_destinations_and_corrupt_records() {
        let (_owned, data, work) = fixture();
        std::fs::create_dir(work.join("private")).unwrap();
        std::fs::write(work.join("private/answer.txt"), b"other work").unwrap();
        assert!(relocate(&data, &work, "private").is_err());
        assert!(!data.join(RECORD).exists());
        assert_eq!(std::fs::read(work.join("private/answer.txt")).unwrap(), b"other work");
        std::fs::write(data.join(RECORD), b"broken authority").unwrap();
        assert!(relocate(&data, &work, "elsewhere").is_err());
        assert_eq!(std::fs::read(data.join(RECORD)).unwrap(), b"broken authority");
        assert_eq!(std::fs::read(data.join("chat/answer.txt")).unwrap(), b"saved work");
    }

    #[test]
    fn chat_relocation_refuses_changed_configuration_and_substituted_roots() {
        let (_owned, data, work) = fixture();
        assert!(relocate_staged(&data, &work, "private", |_| Err(io::ErrorKind::Interrupted.into())).is_err());
        // Stop after the record exists but before the move.
        assert!(
            relocate_staged(&data, &work, "private", |s| {
                if s == Stage::Recorded { Err(io::ErrorKind::Interrupted.into()) } else { Ok(()) }
            })
            .is_err()
        );
        assert!(relocate(&data, &work, "changed").is_err());
        std::fs::rename(data.join("chat"), data.join("original")).unwrap();
        std::fs::create_dir(data.join("chat")).unwrap();
        assert!(relocate(&data, &work, "private").is_err());
        assert_eq!(std::fs::read(data.join("original/answer.txt")).unwrap(), b"saved work");
        assert!(!work.join("private").exists());
    }

    #[test]
    fn chat_relocation_initializes_without_authority_but_never_recreates_a_recorded_missing_root() {
        let owned = tempfile::tempdir().unwrap();
        let root = owned.path().canonicalize().unwrap();
        let data = root.join("data");
        let work = root.join("work");
        std::fs::create_dir(&data).unwrap();
        std::fs::create_dir(&work).unwrap();
        // A first run may stop after creating the fixed legacy source, before its record exists.
        assert!(
            relocate_staged(&data, &work, "private", |stage| {
                if stage == Stage::BeforeRecord { Err(io::ErrorKind::Interrupted.into()) } else { Ok(()) }
            })
            .is_err()
        );
        assert!(data.join("chat").is_dir());
        assert!(!data.join(RECORD).exists());
        let destination = relocate(&data, &work, "private").unwrap();
        std::fs::write(destination.join("work.txt"), b"work must survive").unwrap();
        let record = std::fs::read(data.join(RECORD)).unwrap();
        // Once recorded, a missing root is never silently replaced with a fresh, empty one.
        std::fs::rename(&destination, work.join("preserved")).unwrap();
        assert!(relocate(&data, &work, "private").is_err());
        assert!(!destination.exists());
        assert!(!data.join("chat").exists());
        assert_eq!(std::fs::read(data.join(RECORD)).unwrap(), record);
        assert_eq!(std::fs::read(work.join("preserved/work.txt")).unwrap(), b"work must survive");
    }

    #[test]
    fn chat_relocation_preserves_both_locations_on_a_collision_after_recording() {
        let (_owned, data, work) = fixture();
        assert!(
            relocate_staged(&data, &work, "private", |stage| {
                if stage == Stage::Recorded {
                    std::fs::create_dir(work.join("private"))?;
                    std::fs::write(work.join("private/other.txt"), b"other work")?;
                }
                Ok(())
            })
            .is_err()
        );
        assert_eq!(std::fs::read(data.join("chat/answer.txt")).unwrap(), b"saved work");
        assert_eq!(std::fs::read(work.join("private/other.txt")).unwrap(), b"other work");
        let record = std::fs::read(data.join(RECORD)).unwrap();
        assert!(relocate(&data, &work, "private").is_err());
        assert_eq!(std::fs::read(data.join(RECORD)).unwrap(), record);
    }

    #[test]
    fn chat_relocation_refuses_nested_workspaces_invalid_leaves_and_linked_authority() {
        let (owned, data, work) = fixture();
        for leaf in ["", ".", "..", "../elsewhere", "nested/leaf", "nested\\leaf", "C:leaf"] {
            assert!(relocate(&data, &work, leaf).is_err());
        }
        assert!(relocate(&data, &data, "nested").is_err());
        assert!(relocate(&data, data.parent().unwrap(), "data").is_err());
        let linked = owned.path().join("linked");
        std::os::windows::fs::symlink_dir(&work, &linked).unwrap();
        assert!(relocate(&data, &linked, "private").is_err());
        let outside = owned.path().join("outside.json");
        std::fs::write(&outside, b"outside record").unwrap();
        std::os::windows::fs::symlink_file(&outside, data.join(RECORD)).unwrap();
        assert!(relocate(&data, &work, "private").is_err());
        assert!(data.join(RECORD).symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read(outside).unwrap(), b"outside record");
        assert_eq!(std::fs::read(data.join("chat/answer.txt")).unwrap(), b"saved work");
        assert!(!work.join("private").exists());
    }
}

//! Move private Chat work outside the protected Store, retaining its identity across interrupted starts.
use rustix::fs::{AtFlags, Mode, OFlags, RenameFlags, mkdirat, openat, renameat_with, statat, unlinkat};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{self, Read};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

const RECORD: &str = "chat-workspace-unix.json";
const FLAGS: OFlags = OFlags::RDONLY.union(OFlags::NOFOLLOW).union(OFlags::NONBLOCK).union(OFlags::CLOEXEC);

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    root: (u64, u64),
}

fn refused() -> io::Error {
    io::ErrorKind::PermissionDenied.into()
}

fn identity(file: &File) -> io::Result<(u64, u64)> {
    let meta = file.metadata()?;
    Ok((meta.dev(), meta.ino()))
}

fn child(parent: &File, name: &std::ffi::OsStr) -> io::Result<Option<(u64, u64)>> {
    match statat(parent, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(_) => {
            let dir: File = openat(parent, name, FLAGS | OFlags::DIRECTORY, Mode::empty())?.into();
            identity(&dir).map(Some)
        }
        Err(rustix::io::Errno::NOENT) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Recover the recorded private-work root, refusing links, collisions and replaced or missing work.
pub fn open(data: &Path) -> io::Result<PathBuf> {
    // Resolve /var and /private/var identically before deriving the sibling or comparing identities.
    let data = data.canonicalize()?;
    let parent_path = data.parent().ok_or_else(refused)?;
    let mut leaf = data.file_name().ok_or_else(refused)?.to_os_string();
    leaf.push("-chat-work");
    let parent = File::open(parent_path)?;
    let dir: File = openat(&parent, data.file_name().ok_or_else(refused)?, FLAGS | OFlags::DIRECTORY, Mode::empty())?.into();
    let source = std::ffi::OsStr::new("chat");
    let record: Record = match openat(&dir, RECORD, FLAGS, Mode::empty()) {
        Ok(fd) => {
            let file = File::from(fd);
            let meta = file.metadata()?;
            if !meta.is_file() || meta.nlink() != 1 || meta.len() > 4096 {
                return Err(refused());
            }
            let mut bytes = Vec::new();
            file.take(4097).read_to_end(&mut bytes)?;
            serde_json::from_slice(&bytes).map_err(|_| refused())?
        }
        Err(rustix::io::Errno::NOENT) => {
            if child(&parent, &leaf)?.is_some() {
                return Err(refused());
            }
            if child(&dir, source)?.is_none() {
                mkdirat(&dir, source, Mode::RUSR | Mode::WUSR | Mode::XUSR)?;
            }
            let record = Record { root: child(&dir, source)?.ok_or_else(refused)? };
            let temporary = format!("chat-workspace-{}.tmp", uuid::Uuid::new_v4());
            let mut file = File::from(openat(
                &dir,
                temporary.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            )?);
            let saved = (|| -> io::Result<()> {
                serde_json::to_writer(&mut file, &record)?;
                file.sync_all()?;
                renameat_with(&dir, temporary.as_str(), &dir, RECORD, RenameFlags::NOREPLACE)?;
                dir.sync_all()
            })();
            if saved.is_err() {
                let _ = unlinkat(&dir, temporary.as_str(), AtFlags::empty());
            }
            saved?;
            record
        }
        Err(error) => return Err(error.into()),
    };
    match (child(&dir, source)?, child(&parent, &leaf)?) {
        (Some(root), None) if root == record.root => {
            renameat_with(&dir, source, &parent, &leaf, RenameFlags::NOREPLACE)?;
            dir.sync_all()?;
            parent.sync_all()?;
        }
        (None, Some(root)) if root == record.root => {}
        _ => return Err(refused()),
    }
    if child(&dir, source)?.is_some() || child(&parent, &leaf)? != Some(record.root) {
        return Err(refused());
    }
    Ok(parent_path.join(leaf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_chat_relocation_recovers_before_and_after_the_move_without_losing_work() {
        let owned = tempfile::tempdir().unwrap();
        let data = owned.path().join("data");
        std::fs::create_dir_all(data.join("chat")).unwrap();
        std::fs::write(data.join("chat/answer.txt"), "keep").unwrap();
        let work = open(&data).unwrap();
        assert!(!work.starts_with(data.canonicalize().unwrap()));
        assert_eq!(std::fs::read_to_string(work.join("answer.txt")).unwrap(), "keep");
        assert_eq!(open(&data).unwrap(), work);
        // A recorded source is the recoverable state after a crash immediately before the move.
        std::fs::rename(&work, data.join("chat")).unwrap();
        assert_eq!(open(&data).unwrap(), work);
        let alias = owned.path().join("alias");
        std::os::unix::fs::symlink(owned.path().canonicalize().unwrap(), &alias).unwrap();
        assert_eq!(open(&alias.join("data")).unwrap(), work);
        assert_eq!(std::fs::read_to_string(work.join("answer.txt")).unwrap(), "keep");
        // Restoring the data directory must still reopen the original, unchanged work root.
        let record = std::fs::read(data.join(RECORD)).unwrap();
        std::fs::rename(&data, owned.path().join("old-data")).unwrap();
        std::fs::create_dir(&data).unwrap();
        std::fs::write(data.join(RECORD), record).unwrap();
        assert_eq!(open(&data).unwrap(), work);
        assert_eq!(std::fs::read_to_string(work.join("answer.txt")).unwrap(), "keep");
    }

    #[test]
    fn unix_chat_relocation_refuses_collisions_links_corrupt_records_and_replaced_roots() {
        for case in ["collision", "link", "record", "missing", "replaced", "both"] {
            let owned = tempfile::tempdir().unwrap();
            let data = owned.path().join("data");
            let work = owned.path().join("data-chat-work");
            std::fs::create_dir_all(data.join("chat")).unwrap();
            std::fs::write(data.join("chat/answer.txt"), "keep").unwrap();
            match case {
                "collision" => std::fs::create_dir(&work).unwrap(),
                "link" => std::os::unix::fs::symlink(data.join("chat"), &work).unwrap(),
                _ => {
                    open(&data).unwrap();
                    match case {
                        "record" => std::fs::write(data.join(RECORD), "broken").unwrap(),
                        "missing" | "replaced" => {
                            std::fs::rename(&work, owned.path().join("preserved")).unwrap();
                            if case == "replaced" {
                                std::fs::create_dir(&work).unwrap();
                            }
                        }
                        "both" => std::fs::create_dir(data.join("chat")).unwrap(),
                        _ => unreachable!(),
                    }
                }
            }
            assert!(open(&data).is_err(), "{case}");
            assert!(
                [data.join("chat"), work, owned.path().join("preserved")]
                    .iter()
                    .any(|root| std::fs::read_to_string(root.join("answer.txt")).is_ok_and(|text| text == "keep")),
                "{case} lost work"
            );
        }
    }
}

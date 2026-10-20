use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const BACKUP_SEMANTICS: u32 = 0x0200_0000;
const REPARSE_POINT: u32 = 0x400;
const SHARE_READ: u32 = 1;

/// Opens a plain file without letting another process replace any parent during traversal.
/// The caller keeps the returned handle open while reading; no content is read through a path.
pub fn open_plain_file(root: &Path, relative: &Path) -> io::Result<File> {
    open_with(root, relative, || {})
}

fn denied() -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, "the path is not a plain file inside the folder")
}

fn open_with(root: &Path, relative: &Path, before_file: impl FnOnce()) -> io::Result<File> {
    if !root.is_absolute() || relative.as_os_str().is_empty() || relative.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(denied());
    }
    // Canonicalization resolves junctions in the selected folder itself, before pinning its physical ancestors.
    let root = root.canonicalize()?;
    let target = root.join(relative);
    let mut full = PathBuf::new();
    let mut parents = Vec::new();
    let mut components = target.components().peekable();
    while let Some(component) = components.next() {
        full.push(component);
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        let last = components.peek().is_none();
        if last {
            before_file();
            let file = open(&full)?;
            let meta = file.metadata()?;
            return if meta.is_file() && meta.file_attributes() & REPARSE_POINT == 0 { Ok(file) } else { Err(denied()) };
        }
        let parent = open(&full)?;
        let meta = parent.metadata()?;
        if !meta.is_dir() || meta.file_attributes() & REPARSE_POINT != 0 {
            return Err(denied());
        }
        parents.push(parent);
    }
    Err(denied())
}

fn open(path: &Path) -> io::Result<File> {
    // Denying write and delete sharing pins both the name and its non-reparse state until the child handle is open.
    OpenOptions::new().read(true).share_mode(SHARE_READ).custom_flags(OPEN_REPARSE_POINT | BACKUP_SEMANTICS).open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Folder(PathBuf);

    impl Folder {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let folder = Self(std::env::temp_dir().join(format!(
                "faber-plain-file-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )));
            std::fs::create_dir(&folder.0).unwrap();
            folder
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn changes_parent_cannot_be_replaced_between_validation_and_file_open() {
        let folder = Folder::new();
        let parent = folder.0.join("notes");
        std::fs::create_dir(&parent).unwrap();
        std::fs::write(parent.join("report.txt"), "inside").unwrap();
        let mut file = open_with(&folder.0, Path::new("notes/report.txt"), || {
            let rename = std::fs::rename(&parent, folder.0.join("moved"));
            assert!(rename.is_err(), "a validated parent was replaceable before opening its child");
            assert_eq!(rename.unwrap_err().raw_os_error(), Some(32), "the parent is pinned by its open handle");
        })
        .unwrap();
        let mut text = String::new();
        file.read_to_string(&mut text).unwrap();
        assert_eq!(text, "inside");
    }

    #[test]
    fn changes_open_rejects_parent_traversal_and_directories() {
        let folder = Folder::new();
        std::fs::create_dir(folder.0.join("notes")).unwrap();
        assert!(open_plain_file(&folder.0, Path::new("../outside.txt")).is_err());
        assert!(open_plain_file(&folder.0, Path::new("notes")).is_err());
        assert!(open_plain_file(&folder.0, Path::new("")).is_err());
    }
}

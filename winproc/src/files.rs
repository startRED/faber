//! A single validated handle traversal shared by changes and rewind.
pub use crate::rewind::platform::{open_catalog_file, open_plain_file};

use std::io;
use std::path::Path;

/// Moves `from` to `to` only while nothing is at `to`: an existing file or folder there, even an empty one, is never
/// replaced (`AlreadyExists`). Where the system cannot promise that, it moves nothing (`Unsupported`).
pub fn rename_new(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide = |p: &Path| p.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<u16>>();
        let (from, to) = (wide(from), wide(to));
        // No MOVEFILE_REPLACE_EXISTING: the move fails when the destination exists.
        // SAFETY: both strings are NUL-terminated and outlive the call.
        let moved = unsafe { windows_sys::Win32::Storage::FileSystem::MoveFileExW(from.as_ptr(), to.as_ptr(), 0) };
        if moved == 0 {
            let error = io::Error::last_os_error();
            // ERROR_ALREADY_EXISTS and ERROR_FILE_EXISTS.
            return Err(if matches!(error.raw_os_error(), Some(183 | 80)) { io::ErrorKind::AlreadyExists.into() } else { error });
        }
        Ok(())
    }
    #[cfg(any(target_vendor = "apple", target_os = "linux", target_os = "android"))]
    {
        rustix::fs::renameat_with(rustix::fs::CWD, from, rustix::fs::CWD, to, rustix::fs::RenameFlags::NOREPLACE)?;
        Ok(())
    }
    #[cfg(not(any(windows, target_vendor = "apple", target_os = "linux", target_os = "android")))]
    {
        let _ = (from, to);
        Err(io::ErrorKind::Unsupported.into())
    }
}

/// Opens a regular file to read, never following a link and never waiting on a pipe or a device that took its place.
pub fn open_regular(path: &Path) -> io::Result<std::fs::File> {
    #[cfg(unix)]
    let file = {
        use rustix::fs::{Mode, OFlags};
        let flags = OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        std::fs::File::from(rustix::fs::open(path, flags, Mode::empty())?)
    };
    #[cfg(not(unix))]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_FLAG_OPEN_REPARSE_POINT: a link is opened as itself, and then refused below.
        std::fs::OpenOptions::new().read(true).custom_flags(0x0020_0000).open(path)?
    };
    let meta = file.metadata()?;
    #[cfg(windows)]
    let reparse = std::os::windows::fs::MetadataExt::file_attributes(&meta) & 0x400 != 0;
    #[cfg(not(windows))]
    let reparse = false;
    if !meta.is_file() || reparse {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn a_new_name_is_never_given_over_something_already_there() {
        let folder = tempfile::tempdir().unwrap();
        let (from, to) = (folder.path().join("staged"), folder.path().join("target"));
        std::fs::create_dir(&from).unwrap();
        std::fs::write(from.join("kept"), b"staged").unwrap();
        std::fs::create_dir(&to).unwrap();
        assert_eq!(rename_new(&from, &to).unwrap_err().kind(), std::io::ErrorKind::AlreadyExists, "not even an empty folder");
        assert!(from.join("kept").is_file());
        std::fs::remove_dir(&to).unwrap();
        rename_new(&from, &to).unwrap();
        assert!(to.join("kept").is_file() && !from.exists());
    }

    #[test]
    fn only_a_regular_file_opens() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("file"), b"bytes").unwrap();
        assert!(open_regular(&folder.path().join("file")).is_ok());
        assert!(open_regular(folder.path()).is_err(), "a folder");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(folder.path().join("file"), folder.path().join("link")).unwrap();
            assert!(open_regular(&folder.path().join("link")).is_err(), "a link");
        }
        #[cfg(target_os = "linux")]
        {
            rustix::fs::mknodat(
                rustix::fs::CWD,
                folder.path().join("pipe"),
                rustix::fs::FileType::Fifo,
                rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
                0,
            )
            .unwrap();
            assert!(open_regular(&folder.path().join("pipe")).is_err(), "a pipe, without waiting for a writer");
        }
    }

    #[test]
    fn catalog_read_accepts_snapshot_hardlinks_without_weakening_strict_reads() {
        use std::io::Read;
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("source"), b"catalog bytes").unwrap();
        std::fs::hard_link(folder.path().join("source"), folder.path().join("snapshot")).unwrap();
        assert!(open_plain_file(folder.path(), Path::new("source")).is_err());
        let mut read = crate::open_catalog_file(folder.path(), Path::new("snapshot")).unwrap();
        let mut text = String::new();
        read.read_to_string(&mut text).unwrap();
        assert_eq!(text, "catalog bytes");
        assert!(crate::open_catalog_file(folder.path(), Path::new("../source")).is_err());
        assert!(crate::open_catalog_file(folder.path(), Path::new("")).is_err());
        std::fs::create_dir(folder.path().join("directory")).unwrap();
        assert!(crate::open_catalog_file(folder.path(), Path::new("directory")).is_err());
    }

    #[test]
    fn changes_open_rejects_parent_traversal_and_directories() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir(folder.path().join("notes")).unwrap();
        assert!(open_plain_file(folder.path(), Path::new("../outside.txt")).is_err());
        assert!(open_plain_file(folder.path(), Path::new("notes")).is_err());
        assert!(open_plain_file(folder.path(), Path::new("")).is_err());
    }
}

//! Handle-relative project memory I/O: a changed ancestor can never redirect a read or a replacement.
use std::fs::File;
use std::io;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::{Component, Path, PathBuf};
use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem::*;
use windows_sys::Win32::Foundation::{RtlNtStatusToDosError, UNICODE_STRING};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_ID_BOTH_DIR_INFO, FILE_RENAME_INFO, FileIdBothDirectoryInfo, FileIdBothDirectoryRestartInfo,
    FileRenameInfoEx, GetFileInformationByHandle, GetFileInformationByHandleEx, GetFinalPathNameByHandleW, SetFileInformationByHandle,
    VOLUME_NAME_GUID,
};
use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

fn denied() -> io::Error {
    io::ErrorKind::PermissionDenied.into()
}

fn status(result: i32) -> io::Result<()> {
    if result < 0 {
        // SAFETY: this only converts an error code.
        Err(io::Error::from_raw_os_error(unsafe { RtlNtStatusToDosError(result) } as i32))
    } else {
        Ok(())
    }
}

fn name(name: &str) -> io::Result<Vec<u16>> {
    if name.is_empty() || name.contains(['/', '\\', ':', '\0']) || matches!(name, "." | "..") {
        return Err(denied());
    }
    let wide: Vec<_> = name.encode_utf16().collect();
    if wide.len() > 255 {
        return Err(denied());
    }
    Ok(wide)
}

fn child(parent: &File, name_text: &str, directory: bool, create: bool, replacing: bool) -> io::Result<File> {
    child_access(parent, name_text, directory, create, replacing, 0)
}

fn child_access(parent: &File, name_text: &str, directory: bool, create: bool, replacing: bool, access: u32) -> io::Result<File> {
    let mut wide = name(name_text)?;
    let mut unicode = UNICODE_STRING { Length: (wide.len() * 2) as u16, MaximumLength: (wide.len() * 2) as u16, Buffer: wide.as_mut_ptr() };
    let attributes = OBJECT_ATTRIBUTES {
        Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: parent.as_raw_handle(),
        ObjectName: &mut unicode,
        Attributes: 0x40,
        ..Default::default()
    };
    let mut handle = std::ptr::null_mut();
    let mut ios = IO_STATUS_BLOCK::default();
    let access = access | 0x8000_0000 | if create { 0x4001_0000 } else { 0 };
    let options =
        FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT | if directory { FILE_DIRECTORY_FILE } else { FILE_NON_DIRECTORY_FILE };
    // SAFETY: the counted UTF-16 name and attributes live through this synchronous call. RootDirectory is an owned
    // open directory; the single child is opened without following reparse points, with no path re-resolution.
    status(unsafe {
        NtCreateFile(
            &mut handle,
            access | 0x0010_0000,
            &attributes,
            &mut ios,
            std::ptr::null(),
            0,
            if directory {
                3
            } else if replacing {
                5
            } else {
                1
            },
            if create { FILE_CREATE } else { FILE_OPEN },
            options,
            std::ptr::null(),
            0,
        )
    })?;
    // SAFETY: a successful NtCreateFile returns a new owned handle; File closes it on every return path.
    let file = unsafe { File::from_raw_handle(handle) };
    let meta = file.metadata()?;
    if !directory {
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: file owns a live handle and info is writable for the complete structure.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if info.nNumberOfLinks != 1 {
            return Err(denied());
        }
    }
    if meta.file_attributes() & 0x400 != 0 || (directory && !meta.is_dir()) || (!directory && !meta.is_file()) {
        return Err(denied());
    }
    Ok(file)
}

/// Owns the physical directory chain. Directory names cannot be replaced; all child operations use its last handle.
pub struct MemoryDirectory {
    parents: Vec<File>,
}

impl MemoryDirectory {
    pub fn open(root: &Path, relative: &Path) -> io::Result<Self> {
        Self::walk(root, relative, false)
    }

    /// Create missing components exclusively through held parents; a concurrent creator is never adopted.
    pub fn open_or_create(root: &Path) -> io::Result<Self> {
        Self::walk(root, Path::new(""), true)
    }

    fn walk(root: &Path, relative: &Path, create: bool) -> io::Result<Self> {
        if !root.is_absolute() || relative.components().any(|c| !matches!(c, Component::Normal(_))) {
            return Err(denied());
        }
        let target = root.join(relative);
        if create && target.components().any(|part| matches!(part, Component::ParentDir)) {
            return Err(denied());
        }
        let mut drive = PathBuf::new();
        let mut parents = Vec::new();
        for part in target.components() {
            match part {
                Component::Prefix(prefix) if matches!(prefix.kind(), std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)) => {
                    drive.push(part)
                }
                Component::RootDir => {
                    drive.push(part);
                    parents
                        .push(std::fs::OpenOptions::new().read(true).share_mode(3).custom_flags(0x0020_0000 | 0x0200_0000).open(&drive)?);
                }
                Component::Normal(value) => {
                    let parent = parents.last().ok_or_else(denied)?;
                    let name = value.to_str().ok_or_else(denied)?;
                    let opened = match child(parent, name, true, false, false) {
                        Err(error) if create && error.kind() == io::ErrorKind::NotFound => child(parent, name, true, true, false),
                        result => result,
                    }?;
                    parents.push(opened);
                }
                _ => return Err(denied()),
            }
        }
        if parents.is_empty() {
            return Err(denied());
        }
        Ok(Self { parents })
    }

    fn directory(&self) -> &File {
        self.parents.last().expect("open retained a directory")
    }

    /// The held directory's long path, without resolving or reopening its original spelling.
    pub fn resolved_path(&self) -> io::Result<PathBuf> {
        crate::rewind::resolved_path(self.directory())?.ok_or_else(denied)
    }

    pub fn file(&self, name: &str, create: bool) -> io::Result<File> {
        child(self.directory(), name, false, create, false)
    }

    /// Keep in-place writers out through replacement, while allowing the prepared file to atomically replace this name.
    pub fn replacement_file(&self, name: &str) -> io::Result<File> {
        child(self.directory(), name, false, false, true)
    }

    pub fn under(&self, relative: &Path) -> io::Result<Self> {
        let mut parents = self.parents.iter().map(File::try_clone).collect::<io::Result<Vec<_>>>()?;
        for part in relative.components() {
            let Component::Normal(value) = part else {
                return Err(denied());
            };
            parents.push(child(parents.last().ok_or_else(denied)?, value.to_str().ok_or_else(denied)?, true, false, false)?);
        }
        Ok(Self { parents })
    }

    /// Creates a child directory through this handle, never following a link in its place, and holds it.
    pub fn create_under(&self, name: &str) -> io::Result<Self> {
        let mut parents = self.parents.iter().map(File::try_clone).collect::<io::Result<Vec<_>>>()?;
        parents.push(child(self.directory(), name, true, true, false)?);
        Ok(Self { parents })
    }

    pub fn identity(&self) -> io::Result<String> {
        Self::file_identity(self.directory())
    }

    /// Move exactly the recorded directory between pinned parents, without replacing a destination.
    pub fn relocate_directory(&self, source: &str, destination: &Self, target: &str, expected_identity: &str) -> io::Result<()> {
        let wide = name(target)?;
        // DELETE is limited to this source handle; ordinary memory readers retain their existing sharing rules.
        let source = child_access(self.directory(), source, true, false, false, 0x0001_0000)?;
        if Self::file_identity(&source)? != expected_identity {
            return Err(denied());
        }
        let offset = std::mem::offset_of!(FILE_RENAME_INFORMATION, FileName);
        let bytes = offset + (wide.len() + 1) * 2;
        let mut buffer = vec![0usize; bytes.div_ceil(std::mem::size_of::<usize>())];
        let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
        let mut ios = IO_STATUS_BLOCK::default();
        // SAFETY: the aligned buffer holds the complete counted name. The exact source and both no-follow parent
        // chains remain owned. A single relative leaf and ReplaceIfExists=false cannot redirect or replace the target.
        status(unsafe {
            (*info).Anonymous.ReplaceIfExists = false;
            (*info).RootDirectory = destination.directory().as_raw_handle();
            (*info).FileNameLength = (wide.len() * 2) as u32;
            std::ptr::copy_nonoverlapping(wide.as_ptr(), (*info).FileName.as_mut_ptr(), wide.len());
            NtSetInformationFile(source.as_raw_handle(), &mut ios, info.cast(), bytes as u32, FileRenameInformation)
        })
    }

    pub fn file_identity(file: &File) -> io::Result<String> {
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: the owned handle remains open and info holds the complete output structure.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(format!("{:x}-{:x}-{:x}", info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow))
    }

    pub fn names(&self, limit: usize) -> io::Result<Vec<String>> {
        self.list_names(limit, false).map(|(names, _)| names)
    }

    pub fn names_bounded(&self, limit: usize) -> io::Result<(Vec<String>, bool)> {
        self.list_names(limit, true)
    }

    fn list_names(&self, limit: usize, bounded: bool) -> io::Result<(Vec<String>, bool)> {
        let mut buffer = vec![0usize; 64 * 1024 / std::mem::size_of::<usize>()];
        let mut names = Vec::new();
        let mut first = true;
        let mut examined = 0;
        let mut incomplete = false;
        if bounded && limit == 0 {
            return Ok((names, true));
        }
        loop {
            // SAFETY: the pointer-aligned allocation holds 64 KiB and the directory is owned through this call.
            let success = unsafe {
                GetFileInformationByHandleEx(
                    self.directory().as_raw_handle(),
                    if first { FileIdBothDirectoryRestartInfo } else { FileIdBothDirectoryInfo },
                    buffer.as_mut_ptr().cast(),
                    64 * 1024,
                )
            };
            first = false;
            if success == 0 {
                let error = io::Error::last_os_error();
                return if error.raw_os_error() == Some(18) { Ok((names, incomplete)) } else { Err(error) };
            }
            let mut offset = 0;
            loop {
                let header = std::mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileName);
                if offset + std::mem::size_of::<FILE_ID_BOTH_DIR_INFO>() > 64 * 1024 {
                    return Err(denied());
                }
                // SAFETY: header bounds were checked; Windows returned linked records in this allocation.
                let entry = unsafe { &*buffer.as_ptr().cast::<u8>().add(offset).cast::<FILE_ID_BOTH_DIR_INFO>() };
                let length = entry.FileNameLength as usize;
                if !length.is_multiple_of(2) || offset + header + length > 64 * 1024 {
                    return Err(denied());
                }
                // SAFETY: the UTF-16 tail is in the checked allocation, aligned to at least two bytes.
                let wide =
                    unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>().add(offset + header).cast::<u16>(), length / 2) };
                match String::from_utf16(wide) {
                    Ok(name) if !matches!(name.as_str(), "." | "..") => {
                        examined += 1;
                        names.push(name);
                    }
                    Err(_) => {
                        examined += 1;
                        incomplete = true;
                    }
                    _ => {}
                }
                if bounded && examined >= limit {
                    return Ok((names, true));
                }
                if !bounded && names.len() >= limit {
                    return Ok((names, incomplete));
                }
                if entry.NextEntryOffset == 0 {
                    break;
                }
                let next = entry.NextEntryOffset as usize;
                if next < header || !next.is_multiple_of(std::mem::align_of::<FILE_ID_BOTH_DIR_INFO>()) {
                    return Err(denied());
                }
                offset += next;
            }
        }
    }

    /// Atomically replace the destination with this exact prepared file, even while the final reader holds it open.
    pub fn replace(&self, file: &File, target: &str) -> io::Result<()> {
        self.rename_file(file, target, FILE_RENAME_REPLACE_IF_EXISTS | FILE_RENAME_POSIX_SEMANTICS)
    }

    /// Publish a prepared record only if its name is absent; existing authority must never be replaced.
    pub fn publish(&self, file: &File, target: &str) -> io::Result<()> {
        self.rename_file(file, target, 0)
    }

    fn rename_file(&self, file: &File, target: &str, flags: u32) -> io::Result<()> {
        let child = name(target)?;
        // Win32 rejects a non-null RootDirectory here. Query the verified directory handle for its volume-GUID
        // name; held ancestors pin its components, and a remapped drive letter cannot redirect the destination.
        // SAFETY: the directory is owned throughout; a zero-sized query writes no memory.
        let needed = unsafe { GetFinalPathNameByHandleW(self.directory().as_raw_handle(), std::ptr::null_mut(), 0, VOLUME_NAME_GUID) };
        if needed == 0 {
            return Err(io::Error::last_os_error());
        }
        if needed > 32_767 {
            return Err(denied());
        }
        let mut wide = vec![0u16; needed as usize + 1];
        // SAFETY: the allocation covers its reported capacity; the owned no-follow directory cannot be renamed.
        let written =
            unsafe { GetFinalPathNameByHandleW(self.directory().as_raw_handle(), wide.as_mut_ptr(), wide.len() as u32, VOLUME_NAME_GUID) };
        if written == 0 {
            return Err(io::Error::last_os_error());
        }
        if written as usize >= wide.len() {
            return Err(denied());
        }
        wide.truncate(written as usize);
        if wide.last() != Some(&(b'\\' as u16)) {
            wide.push(b'\\' as u16);
        }
        wide.extend(child);
        let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
        let bytes = offset + (wide.len() + 1) * 2;
        let mut buffer = vec![0usize; bytes.div_ceil(std::mem::size_of::<usize>())];
        let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        // SAFETY: the aligned allocation holds the header and counted UTF-16 tail; both owned handles stay open.
        let success = unsafe {
            (*info).Anonymous.Flags = flags;
            (*info).RootDirectory = std::ptr::null_mut();
            (*info).FileNameLength = (wide.len() * 2) as u32;
            std::ptr::copy_nonoverlapping(wide.as_ptr(), (*info).FileName.as_mut_ptr(), wide.len());
            SetFileInformationByHandle(file.as_raw_handle(), FileRenameInfoEx, info.cast(), bytes as u32)
        };
        if success == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
    }

    pub fn discard(&self, file: &File) -> io::Result<()> {
        let mut ios = IO_STATUS_BLOCK::default();
        let delete = FILE_DISPOSITION_INFORMATION { DeleteFile: true };
        // SAFETY: file is an owned temporary with DELETE access and delete lives through this synchronous call.
        status(unsafe {
            NtSetInformationFile(
                file.as_raw_handle(),
                &mut ios,
                (&delete as *const FILE_DISPOSITION_INFORMATION).cast(),
                std::mem::size_of_val(&delete) as u32,
                FileDispositionInformation,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn adopted_task_folder_native_listing_reports_overflow_without_changing_legacy_listing() {
        let owned = tempfile::tempdir().unwrap();
        let root = owned.path().canonicalize().unwrap();
        for index in 0..12 {
            std::fs::write(root.join(format!("{index:02}.md")), "synthetic task").unwrap();
        }
        let directory = MemoryDirectory::open(&root, Path::new("")).unwrap();
        let (names, incomplete) = directory.names_bounded(3).unwrap();
        assert!(incomplete);
        assert!(names.len() <= 3);
        assert_eq!(directory.names(20).unwrap().len(), 12);
    }

    #[test]
    fn adopted_task_folder_native_listing_reports_undecodable_entries() {
        use std::os::windows::ffi::OsStringExt;
        let owned = tempfile::tempdir().unwrap();
        let root = owned.path().canonicalize().unwrap();
        let invalid = std::ffi::OsString::from_wide(&[0xd800, b'.' as u16, b'm' as u16, b'd' as u16]);
        std::fs::write(root.join(invalid), "synthetic invalid-name task").unwrap();
        let directory = MemoryDirectory::open(&root, Path::new("")).unwrap();
        let (names, incomplete) = directory.names_bounded(10).unwrap();
        assert!(incomplete);
        assert!(names.is_empty());
    }

    use super::*;
    use std::io::Write;
    #[test]
    fn publishing_a_migration_record_never_replaces_existing_authority() {
        let owned = tempfile::tempdir().unwrap();
        let root = owned.path().canonicalize().unwrap();
        let directory = MemoryDirectory::open(&root, Path::new("")).unwrap();
        let mut first = directory.file("first.tmp", true).unwrap();
        first.write_all(b"original authority").unwrap();
        first.sync_all().unwrap();
        directory.publish(&first, "record.json").unwrap();
        drop(first);
        let mut next = directory.file("next.tmp", true).unwrap();
        next.write_all(b"competing authority").unwrap();
        next.sync_all().unwrap();
        assert!(directory.publish(&next, "record.json").is_err());
        assert_eq!(std::fs::read(root.join("record.json")).unwrap(), b"original authority");
        assert_eq!(std::fs::read(root.join("next.tmp")).unwrap(), b"competing authority");
    }

    #[test]
    fn chat_directory_move_refuses_linked_sources_destinations_and_parents() {
        let owned = tempfile::tempdir().unwrap();
        let root = owned.path().canonicalize().unwrap();
        for directory in ["before", "after", "outside", "before/chat"] {
            std::fs::create_dir(root.join(directory)).unwrap();
        }
        std::fs::write(root.join("before/chat/work.txt"), b"private work").unwrap();
        std::fs::write(root.join("outside/untouched.txt"), b"outside work").unwrap();
        std::os::windows::fs::symlink_dir(root.join("outside"), root.join("before/linked")).unwrap();
        std::os::windows::fs::symlink_dir(root.join("outside"), root.join("after/linked")).unwrap();
        std::os::windows::fs::symlink_dir(root.join("before"), root.join("linked-parent")).unwrap();
        let before = MemoryDirectory::open(&root, Path::new("before")).unwrap();
        let after = MemoryDirectory::open(&root, Path::new("after")).unwrap();
        let outside = MemoryDirectory::open(&root, Path::new("outside")).unwrap().identity().unwrap();
        let identity = before.under(Path::new("chat")).unwrap().identity().unwrap();
        assert!(before.relocate_directory("linked", &after, "escaped", &outside).is_err());
        assert!(before.relocate_directory("chat", &after, "linked", &identity).is_err());
        assert!(MemoryDirectory::open(&root, Path::new("linked-parent")).is_err());
        assert!(MemoryDirectory::open(&root.join("linked-parent"), Path::new("chat")).is_err());
        assert_eq!(std::fs::read(root.join("before/chat/work.txt")).unwrap(), b"private work");
        assert_eq!(std::fs::read(root.join("outside/untouched.txt")).unwrap(), b"outside work");
        assert!(!root.join("after/escaped").exists());
        assert!(root.join("before/linked").symlink_metadata().unwrap().file_type().is_symlink());
        assert!(root.join("after/linked").symlink_metadata().unwrap().file_type().is_symlink());
    }

    #[test]
    fn chat_directory_move_preserves_identity_and_refuses_replacement() {
        let owned = tempfile::tempdir().unwrap();
        let root = owned.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("before")).unwrap();
        std::fs::create_dir(root.join("after")).unwrap();
        std::fs::create_dir(root.join("before/chat")).unwrap();
        std::fs::write(root.join("before/chat/work.txt"), b"private work").unwrap();
        let before = MemoryDirectory::open(&root, Path::new("before")).unwrap();
        let after = MemoryDirectory::open(&root, Path::new("after")).unwrap();
        let held = before.under(Path::new("chat")).unwrap();
        let identity = held.identity().unwrap();
        assert!(before.relocate_directory("chat", &after, "chat", &identity).is_err(), "existing readers still pin their directory");
        drop(held);
        assert!(before.relocate_directory("chat", &after, "chat", "another identity").is_err());
        assert!(root.join("before/chat/work.txt").exists());
        std::fs::create_dir(root.join("after/chat")).unwrap();
        std::fs::write(root.join("after/chat/other.txt"), b"other work").unwrap();
        assert!(before.relocate_directory("chat", &after, "chat", &identity).is_err());
        assert_eq!(std::fs::read(root.join("after/chat/other.txt")).unwrap(), b"other work");
        before.relocate_directory("chat", &after, "moved", &identity).unwrap();
        assert!(!root.join("before/chat").exists());
        assert_eq!(after.under(Path::new("moved")).unwrap().identity().unwrap(), identity);
        assert_eq!(std::fs::read(root.join("after/moved/work.txt")).unwrap(), b"private work");
        assert!(before.relocate_directory("../after/moved", &after, "escape", &identity).is_err());
        assert!(after.relocate_directory("moved", &before, "../escape", &identity).is_err());
        assert!(std::fs::rename(root.join("after"), root.join("replaced")).is_err());
        std::fs::write(root.join("before/plain-file"), b"not a directory").unwrap();
        let file = before.file("plain-file", false).unwrap();
        let file_identity = MemoryDirectory::file_identity(&file).unwrap();
        drop(file);
        assert!(before.relocate_directory("plain-file", &after, "plain-file", &file_identity).is_err());
        assert_eq!(std::fs::read(root.join("before/plain-file")).unwrap(), b"not a directory");
    }

    #[test]
    fn project_memory_directory_replaces_a_child_without_allowing_parent_replacement() {
        let path = std::env::temp_dir().join(format!("faber-memory-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("AGENTS.md"), "before").unwrap();
        {
            let dir = MemoryDirectory::open(&path.canonicalize().unwrap(), Path::new("")).unwrap();
            assert!(std::fs::rename(&path, path.with_extension("moved")).is_err());
            assert!(dir.names(100).unwrap().contains(&"AGENTS.md".to_string()));
            let mut temp = dir.file("new.tmp", true).unwrap();
            temp.write_all(b"after").unwrap();
            temp.sync_all().unwrap();
            let guard = dir.replacement_file("AGENTS.md").unwrap();
            assert!(std::fs::write(path.join("AGENTS.md"), "another writer").is_err());
            dir.replace(&temp, "AGENTS.md").unwrap();
            assert_eq!(guard.metadata().unwrap().len(), 6);
            assert_eq!(std::fs::read(path.join("AGENTS.md")).unwrap(), b"after");
            assert_eq!(dir.names(100).unwrap(), ["AGENTS.md"]);
        }
        assert_eq!(std::fs::read_to_string(path.join("AGENTS.md")).unwrap(), "after");
        std::fs::remove_dir_all(path).unwrap();
    }
}

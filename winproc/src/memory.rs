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
    let access = 0x8000_0000 | if create { 0x4001_0000 } else { 0 };
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
        if !root.is_absolute() || relative.components().any(|c| !matches!(c, Component::Normal(_))) {
            return Err(denied());
        }
        let target = root.join(relative);
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
                    parents.push(child(parents.last().ok_or_else(denied)?, value.to_str().ok_or_else(denied)?, true, false, false)?)
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

    pub fn identity(&self) -> io::Result<String> {
        Self::file_identity(self.directory())
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
        let mut buffer = vec![0usize; 64 * 1024 / std::mem::size_of::<usize>()];
        let mut names = Vec::new();
        let mut first = true;
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
                return if error.raw_os_error() == Some(18) { Ok(names) } else { Err(error) };
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
                if let Ok(name) = String::from_utf16(wide)
                    && !matches!(name.as_str(), "." | "..")
                {
                    names.push(name);
                }
                if names.len() >= limit {
                    return Ok(names);
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
            (*info).Anonymous.Flags = FILE_RENAME_REPLACE_IF_EXISTS | FILE_RENAME_POSIX_SEMANTICS;
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
    use super::*;
    use std::io::Write;
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

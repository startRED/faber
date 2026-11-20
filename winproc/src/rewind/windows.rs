use super::{SwapStage, changed, compare, compare_displaced, relative, temporary_name};
use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem::{
    FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_IF, FILE_OPEN_REPARSE_POINT,
    FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree, UNICODE_STRING};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, GetSecurityDescriptorControl, GetSecurityDescriptorLength, GetTokenInformation,
    IsValidSecurityDescriptor, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, SE_DACL_PROTECTED, SetKernelObjectSecurity,
    TOKEN_QUERY, TOKEN_USER, TokenUser, UNPROTECTED_DACL_SECURITY_INFORMATION,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_BASIC_INFO, FILE_DISPOSITION_FLAG_DELETE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
    FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX, FILE_RENAME_INFO, FILE_STANDARD_INFO, FileBasicInfo, FileDispositionInfo,
    FileDispositionInfoEx, FileRenameInfo, FileStandardInfo, GetDiskFreeSpaceExW, GetFileInformationByHandle, GetFileInformationByHandleEx,
    GetFinalPathNameByHandleW, SetFileInformationByHandle,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken};

const REPARSE: u32 = 0x400;
const READ_CONTROL: u32 = 0x20000;
const WRITE_DAC: u32 = 0x40000;
const WRITE_OWNER: u32 = 0x80000;
const DELETE: u32 = 0x10000;
const SYNCHRONIZE: u32 = 0x100000;
const SECURITY: u32 = OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}
fn handle(file: &File) -> HANDLE {
    file.as_raw_handle()
}
fn win(ok: i32) -> io::Result<()> {
    if ok == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
}

fn info(file: &File) -> io::Result<BY_HANDLE_FILE_INFORMATION> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: the held handle and writable output are valid for the call.
    win(unsafe { GetFileInformationByHandle(handle(file), &mut info) })?;
    Ok(info)
}

pub fn single_link(file: &File) -> io::Result<bool> {
    Ok(info(file)?.nNumberOfLinks == 1)
}

pub fn file_identity(file: &File) -> io::Result<Vec<u8>> {
    let i = info(file)?;
    Ok([i.dwVolumeSerialNumber.to_le_bytes(), i.nFileIndexHigh.to_le_bytes(), i.nFileIndexLow.to_le_bytes()].concat())
}

/// Resolve the normalized long DOS name through the held handle without opening or reading another name.
pub fn resolved_path(file: &File) -> io::Result<Option<PathBuf>> {
    let mut buffer = vec![0u16; 512];
    loop {
        // SAFETY: the held handle is valid and the output buffer has its stated length; flags request the normalized DOS path.
        let length = unsafe { GetFinalPathNameByHandleW(handle(file), buffer.as_mut_ptr(), buffer.len() as u32, 0) } as usize;
        if length == 0 {
            return Err(io::Error::last_os_error());
        }
        if length < buffer.len() {
            return Ok(Some(PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length]))));
        }
        if length >= 32768 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        buffer.resize(length + 1, 0);
    }
}

fn checked(file: File, directory: bool) -> io::Result<File> {
    checked_links(file, directory, false)
}

fn checked_links(file: File, directory: bool, allow_hardlinks: bool) -> io::Result<File> {
    let meta = file.metadata()?;
    if meta.file_attributes() & REPARSE != 0
        || meta.is_dir() != directory
        || (!directory && (!meta.is_file() || (!allow_hardlinks && !single_link(&file)?)))
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(file)
}

struct Security(*mut std::ffi::c_void);
impl Drop for Security {
    fn drop(&mut self) {
        // SAFETY: every Security owns a LocalAlloc allocation returned by the security API.
        unsafe {
            LocalFree(self.0);
        }
    }
}

fn private_security() -> io::Result<Security> {
    // SAFETY: all buffers are sized by Windows and every returned allocation/handle is released here.
    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut token) == 0 {
            if io::Error::last_os_error().raw_os_error() != Some(1008) {
                return Err(io::Error::last_os_error());
            }
            win(OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token))?;
        }
        let result = (|| {
            let mut size = 0;
            GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut size);
            let mut buffer = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
            win(GetTokenInformation(token, TokenUser, buffer.as_mut_ptr().cast(), size, &mut size))?;
            let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
            let mut sid = std::ptr::null_mut();
            win(ConvertSidToStringSidW(user.User.Sid, &mut sid))?;
            let allocation = Security(sid.cast());
            let mut len = 0;
            while *sid.add(len) != 0 {
                len += 1;
            }
            let sid = String::from_utf16_lossy(std::slice::from_raw_parts(sid, len));
            let sddl = wide(OsStr::new(&format!("O:{sid}D:P(A;;FA;;;{sid})")));
            let mut descriptor = std::ptr::null_mut();
            win(ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), 1, &mut descriptor, std::ptr::null_mut()))?;
            drop(allocation);
            Ok(Security(descriptor))
        })();
        CloseHandle(token);
        result
    }
}

fn open_at(
    parent: &File,
    name: &OsStr,
    directory: bool,
    disposition: u32,
    write: bool,
    deleting: bool,
    security: Option<&Security>,
) -> io::Result<File> {
    open_at_access(parent, name, directory, disposition, write, deleting, security, false, false)
}

#[allow(clippy::too_many_arguments)]
fn open_at_access(
    parent: &File,
    name: &OsStr,
    directory: bool,
    disposition: u32,
    write: bool,
    deleting: bool,
    security: Option<&Security>,
    metadata_only: bool,
    allow_hardlinks: bool,
) -> io::Result<File> {
    let name: Vec<u16> = name.encode_wide().collect();
    let bytes = u16::try_from(name.len().checked_mul(2).ok_or(io::ErrorKind::InvalidInput)?).map_err(|_| io::ErrorKind::InvalidInput)?;
    let unicode = UNICODE_STRING { Length: bytes, MaximumLength: bytes, Buffer: name.as_ptr().cast_mut() };
    let attributes = OBJECT_ATTRIBUTES {
        Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: handle(parent),
        ObjectName: &unicode,
        Attributes: 0x40,
        SecurityDescriptor: security.map_or(std::ptr::null(), |s| s.0.cast()),
        SecurityQualityOfService: std::ptr::null(),
    };
    let mut file = std::ptr::null_mut();
    let mut status = IO_STATUS_BLOCK::default();
    let access = SYNCHRONIZE
        | READ_CONTROL
        | 0x80
        | if write {
            0xC0000000 | WRITE_DAC | WRITE_OWNER | DELETE
        } else {
            (if metadata_only { 0 } else { 0x80000000 }) | if deleting { DELETE } else { 0 }
        };
    // SAFETY: name, attributes and output buffers live across the call; parent is held open. No reparse point is followed.
    let result = unsafe {
        NtCreateFile(
            &mut file,
            access,
            &attributes,
            &mut status,
            std::ptr::null(),
            0x80,
            1 | if directory { 2 } else { 0 } | if deleting { 4 } else { 0 },
            disposition,
            FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT | if directory { FILE_DIRECTORY_FILE } else { FILE_NON_DIRECTORY_FILE },
            std::ptr::null(),
            0,
        )
    };
    if result < 0 {
        // SAFETY: RtlNtStatusToDosError is a pure conversion of the returned status.
        unsafe extern "system" {
            fn RtlNtStatusToDosError(status: i32) -> u32;
        }
        return Err(io::Error::from_raw_os_error(unsafe { RtlNtStatusToDosError(result) } as i32));
    }
    // SAFETY: NtCreateFile transferred ownership of one valid file handle.
    checked_links(unsafe { File::from_raw_handle(file) }, directory, allow_hardlinks)
}

pub struct Root {
    dir: File,
    path: PathBuf,
    id: Vec<u8>,
    _ancestors: Vec<File>,
}
impl Root {
    pub fn open(path: &Path) -> io::Result<Self> {
        if !path.is_absolute() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let path = path.canonicalize()?;
        let mut full = PathBuf::new();
        let mut ancestors = vec![];
        for part in path.components() {
            full.push(part);
            if matches!(part, Component::Prefix(_)) {
                continue;
            }
            let dir = if let Some(parent) = ancestors.last() {
                open_at(parent, part.as_os_str(), true, FILE_OPEN, false, false, None)?
            } else {
                checked(OpenOptions::new().read(true).share_mode(3).custom_flags(0x00200000 | 0x02000000).open(&full)?, true)?
            };
            ancestors.push(dir);
        }
        let dir = ancestors.pop().ok_or(io::ErrorKind::InvalidInput)?;
        let id = file_identity(&dir)?;
        Ok(Self { dir, path, id, _ancestors: ancestors })
    }
    pub fn identity(&self) -> io::Result<Vec<u8>> {
        Ok(self.id.clone())
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn available_space(&self) -> io::Result<u64> {
        let mut available = 0;
        // SAFETY: the canonical root and its ancestors are pinned; the output pointer is valid.
        win(unsafe {
            GetDiskFreeSpaceExW(wide(self.path.as_os_str()).as_ptr(), &mut available, std::ptr::null_mut(), std::ptr::null_mut())
        })?;
        Ok(available)
    }
    fn parent(&self, path: &Path, create: bool) -> io::Result<(File, PathBuf, Vec<File>)> {
        relative(path)?;
        let mut dir = self.dir.try_clone()?;
        let mut full = self.path.clone();
        let mut parents = vec![];
        let security = if create { Some(private_security()?) } else { None };
        for part in path.parent().into_iter().flat_map(Path::components) {
            let child =
                open_at(&dir, part.as_os_str(), true, if create { FILE_OPEN_IF } else { FILE_OPEN }, false, false, security.as_ref())?;
            parents.push(dir);
            dir = child;
            full.push(part);
        }
        Ok((dir, full, parents))
    }
    pub fn open_file(&self, path: &str) -> io::Result<File> {
        self.open_with(path, || {})
    }
    /// Holds the very object that may be removed after the caller has preserved its bytes.
    pub fn open_removable(&self, path: &str) -> io::Result<File> {
        let path = Path::new(path);
        let (dir, _, _parents) = self.parent(path, false)?;
        open_at(&dir, path.file_name().unwrap(), false, FILE_OPEN, false, true, None)
    }
    /// Returns a surviving replacement name instead of deleting an object the caller never preserved.
    pub fn remove_preserved(&self, path: &str, file: &File) -> io::Result<Option<String>> {
        self.remove_preserved_with(path, file, || Ok(()))
    }
    fn remove_preserved_with(&self, path: &str, file: &File, before_delete: impl FnOnce() -> io::Result<()>) -> io::Result<Option<String>> {
        let path_ref = Path::new(path);
        let (_dir, parent, _parents) = self.parent(path_ref, false)?;
        checked(file.try_clone()?, false)?;
        if resolved_path(file)?.as_deref() != Some(parent.join(path_ref.file_name().unwrap()).as_path()) {
            return Ok(Some(path.to_owned()));
        }
        before_delete()?;
        let disposition = FILE_DISPOSITION_INFO_EX { Flags: FILE_DISPOSITION_FLAG_DELETE | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS };
        // SAFETY: the caller kept this DELETE-capable handle from preservation; no pathname is reopened for deletion.
        win(unsafe {
            SetFileInformationByHandle(
                handle(file),
                FileDispositionInfoEx,
                (&disposition as *const FILE_DISPOSITION_INFO_EX).cast(),
                std::mem::size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
            )
        })?;
        Ok(None)
    }
    /// Deletes only an immutable private-history file whose ownership name the caller already validated.
    /// No project restore may use this: its current bytes must first be saved for redo.
    pub fn remove_owned(&self, path: &str) -> io::Result<()> {
        self.remove_owned_with(path, || Ok(()))
    }
    fn remove_owned_with(&self, path: &str, before_delete: impl FnOnce() -> io::Result<()>) -> io::Result<()> {
        let file = self.open_removable(path)?;
        before_delete()?;
        if self.remove_preserved(path, &file)?.is_some() {
            return Err(changed());
        }
        Ok(())
    }
    fn open_with(&self, path: &str, before_leaf: impl FnOnce()) -> io::Result<File> {
        let path = Path::new(path);
        relative(path)?;
        if path.parent().is_some_and(|parent| parent.as_os_str().is_empty()) {
            before_leaf();
            return open_at(&self.dir, path.file_name().unwrap(), false, FILE_OPEN, false, false, None);
        }
        let (dir, _, _parents) = self.parent(path, false)?;
        before_leaf();
        open_at(&dir, path.file_name().unwrap(), false, FILE_OPEN, false, false, None)
    }
    pub fn private_write(&self, path: &str, bytes: &[u8]) -> io::Result<()> {
        let mut file = self.private_create(path)?;
        file.write_all(bytes)?;
        file.sync_all()
    }
    pub fn private_create(&self, path: &str) -> io::Result<File> {
        let path = Path::new(path);
        let (dir, _, _parents) = self.parent(path, false)?;
        let security = private_security()?;
        open_at(&dir, path.file_name().unwrap(), false, FILE_CREATE, true, false, Some(&security))
    }
    pub fn rename_new(&self, from: &str, to: &str) -> io::Result<()> {
        let from = Path::new(from);
        let to = Path::new(to);
        let (source, _, _source_parents) = self.parent(from, false)?;
        let (_target, target_path, _target_parents) = self.parent(to, false)?;
        let file = open_at_access(&source, from.file_name().unwrap(), false, FILE_OPEN, false, true, None, true, false)?;
        rename_handle(&file, &target_path.join(to.file_name().unwrap()), false)
    }
    /// Atomic replacement is restricted to metadata in the caller's owner-only history namespace.
    pub fn replace_owned(&self, path: &str, expected: (&[u8], &[u8]), desired: (&[u8], &[u8])) -> io::Result<()> {
        self.replace_owned_with(path, expected, desired, || Ok(()))
    }

    pub(crate) fn replace_owned_with(
        &self,
        path: &str,
        expected: (&[u8], &[u8]),
        desired: (&[u8], &[u8]),
        before: impl FnOnce() -> io::Result<()>,
    ) -> io::Result<()> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = Path::new(path);
        let (dir, parent, _parents) = self.parent(path, false)?;
        let current = open_at(&dir, path.file_name().unwrap(), false, FILE_OPEN, false, true, None)?;
        compare(&current, expected)?;
        let name = format!(".faber-rewind-metadata-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        let security = private_security()?;
        let mut file = open_at(&dir, OsStr::new(&name), false, FILE_CREATE, true, true, Some(&security))?;
        let result = (|| {
            file.write_all(desired.0)?;
            apply(&file, desired.1)?;
            file.sync_all()?;
            compare(&current, expected)?;
            before()?;
            // Windows refuses replacing a destination while our validation handle is still open.
            drop(current);
            rename_handle(&file, &parent.join(path.file_name().unwrap()), true)
        })();
        if result.is_err() {
            let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
            // SAFETY: only the newly created private metadata handle is discarded on a pre-commit failure.
            win(unsafe {
                SetFileInformationByHandle(
                    handle(&file),
                    FileDispositionInfo,
                    (&disposition as *const FILE_DISPOSITION_INFO).cast(),
                    std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
                )
            })?;
        }
        result
    }

    pub fn mutate(&self, path: &str, expected: Option<(&[u8], &[u8])>, desired: Option<(&[u8], &[u8])>) -> io::Result<()> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temp = format!(".faber-rewind-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        self.mutate_journaled(path, expected, desired, &temp, |_| Ok(()))
    }

    pub fn mutate_journaled(
        &self,
        path: &str,
        expected: Option<(&[u8], &[u8])>,
        desired: Option<(&[u8], &[u8])>,
        temporary: &str,
        created: impl FnOnce(&File) -> io::Result<()>,
    ) -> io::Result<()> {
        let mut created = Some(created);
        let result = self.mutate_staged(path, expected, desired, temporary, |stage, file| {
            if stage == SwapStage::Prepared {
                created.take().unwrap()(file)?;
            }
            Ok(())
        });
        let aside = Path::new(path).with_file_name(format!("{temporary}-aside")).to_string_lossy().into_owned();
        if result.is_ok() {
            if expected.is_some() {
                self.remove_owned(&aside)?;
            }
        } else if self.open_file(path).is_err_and(|e| e.kind() == io::ErrorKind::NotFound) {
            let _ = self.rename_new(&aside, path);
        }
        result
    }

    pub fn finish_swap(&self, _path: &str, _temporary: &str) -> io::Result<()> {
        Ok(())
    }
    pub fn validate_swap(&self, _path: &str, _temporary: &str) -> io::Result<()> {
        Ok(())
    }

    pub fn recover_cleanup(&self, _path: &str, _temporary: &str) -> io::Result<Vec<String>> {
        // Windows deletes through the preserved handle and never creates cleanup quarantines.
        Ok(vec![])
    }

    pub fn recover_cleanup_directory(&self, _directory: &str) -> io::Result<Vec<String>> {
        Ok(vec![])
    }

    pub fn recover_cleanup_tree(&self) -> io::Result<Vec<(String, Option<Vec<u8>>)>> {
        Ok(vec![])
    }

    pub fn inspect_cleanup_tree(&self) -> io::Result<Vec<(String, Option<Vec<u8>>)>> {
        Ok(vec![])
    }

    pub fn mutate_staged(
        &self,
        path: &str,
        expected: Option<(&[u8], &[u8])>,
        desired: Option<(&[u8], &[u8])>,
        temporary: &str,
        mut journal: impl FnMut(SwapStage, &File) -> io::Result<()>,
    ) -> io::Result<()> {
        temporary_name(temporary)?;
        let path = Path::new(path);
        let (dir, parent, _parents) = self.parent(path, expected.is_none() && desired.is_some())?;
        let name = path.file_name().unwrap();
        let current = match open_at(&dir, name, false, FILE_OPEN, false, true, None) {
            Ok(file) => Some(file),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        match (&current, expected) {
            (Some(file), Some(wanted)) => compare(file, wanted)?,
            (None, None) => (),
            _ => return Err(changed()),
        }
        let aside = format!("{temporary}-aside");
        let mut prepared: Option<File> = None;
        let mut displacement_started = false;
        let result = (|| {
            if let Some((bytes, saved)) = desired {
                let security = private_security()?;
                let file = open_at(&dir, OsStr::new(temporary), false, FILE_CREATE, true, true, Some(&security))?;
                prepared = Some(file);
                let file = prepared.as_mut().unwrap();
                journal(SwapStage::Prepared, file)?;
                file.write_all(bytes)?;
                let security = if let Some(old) = current.as_ref() { protection(old)? } else { saved.to_vec() };
                apply(file, &security)?;
                file.sync_all()?;
                compare(file, (&fingerprint(file)?, bytes))?;
                if resolved_path(file)?.as_deref() != Some(parent.join(temporary).as_path()) {
                    return Err(changed());
                }
            }
            if let (Some(original), Some(wanted)) = (&current, expected) {
                compare(original, wanted)?;
                journal(SwapStage::BeforeDisplace, original)?;
                displacement_started = true;
                rename_handle(original, &parent.join(&aside), false)?;
                journal(SwapStage::Displaced, original)?;
                compare_displaced(original, original, wanted.1)?;
                if open_at(&dir, name, false, FILE_OPEN, false, true, None).is_ok() {
                    return Err(changed());
                }
                if desired.is_none() {
                    journal(SwapStage::BeforeInstall, original)?;
                }
            }
            if let Some(file) = prepared.as_ref() {
                journal(SwapStage::BeforeInstall, file)?;
                rename_handle(file, &parent.join(name), false)?;
            }
            Ok(())
        })();
        // A failed native rename never destroys a surviving replacement or displaced version.
        if !displacement_started
            && result.is_err()
            && let Some(file) = prepared.as_ref()
            && resolved_path(file)?.as_deref() == Some(parent.join(temporary).as_path())
        {
            let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
            // SAFETY: cleanup targets the created handle only while it still has its owned temporary name.
            win(unsafe {
                SetFileInformationByHandle(
                    handle(file),
                    FileDispositionInfo,
                    (&disposition as *const FILE_DISPOSITION_INFO).cast(),
                    std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
                )
            })?;
        }
        drop(prepared);
        result
    }
}

fn rename_handle(file: &File, destination: &Path, replace: bool) -> io::Result<()> {
    let name: Vec<u16> = destination.as_os_str().encode_wide().collect();
    let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
    let size = offset + (name.len() + 1) * 2;
    let mut buffer = vec![0usize; size.div_ceil(std::mem::size_of::<usize>())];
    // SAFETY: the aligned buffer contains FILE_RENAME_INFO and its full trailing UTF-16 name.
    // Project swaps refuse replacement; only immutable metadata in the private history may replace atomically.
    unsafe {
        let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        (*info).Anonymous.ReplaceIfExists = replace;
        (*info).RootDirectory = std::ptr::null_mut();
        (*info).FileNameLength = (name.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(name.as_ptr(), (*info).FileName.as_mut_ptr(), name.len());
        win(SetFileInformationByHandle(handle(file), FileRenameInfo, info.cast(), size as u32))
    }
}

pub fn open_plain_file(root: &Path, path: &Path) -> io::Result<File> {
    Root::open(root)?.open_file(path.to_str().ok_or(io::ErrorKind::InvalidInput)?)
}

/// Catalog snapshots legitimately share plain files by hardlink; traversal still pins every ancestor and refuses reparse points.
pub fn open_catalog_file(root: &Path, path: &Path) -> io::Result<File> {
    let root = Root::open(root)?;
    let (dir, _, _parents) = root.parent(path, false)?;
    open_at_access(&dir, path.file_name().ok_or(io::ErrorKind::InvalidInput)?, false, FILE_OPEN, false, false, None, false, true)
}

pub fn protection(file: &File) -> io::Result<Vec<u8>> {
    let mut descriptor = std::ptr::null_mut();
    // SAFETY: reads security through the held file handle; the returned allocation is released below.
    let code = unsafe {
        GetSecurityInfo(
            handle(file),
            SE_FILE_OBJECT,
            SECURITY,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if code != 0 {
        return Err(io::Error::from_raw_os_error(code as i32));
    }
    let owned = Security(descriptor);
    // SAFETY: GetSecurityInfo returns a valid self-relative descriptor with the reported length.
    let bytes = unsafe { std::slice::from_raw_parts(owned.0.cast::<u8>(), GetSecurityDescriptorLength(owned.0) as usize) }.to_vec();
    Ok(bytes)
}

fn apply(file: &File, saved: &[u8]) -> io::Result<()> {
    if !valid_protection(saved) {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut aligned = vec![0usize; saved.len().div_ceil(std::mem::size_of::<usize>())];
    // SAFETY: aligned owns at least saved.len bytes; descriptor APIs validate this captured security descriptor.
    unsafe {
        std::ptr::copy_nonoverlapping(saved.as_ptr(), aligned.as_mut_ptr().cast::<u8>(), saved.len());
    }
    let descriptor = aligned.as_mut_ptr().cast();
    // SAFETY: every relative SID and ACL region was bounded before passing it to the native validator.
    if unsafe { IsValidSecurityDescriptor(descriptor) } == 0 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut control = 0;
    let mut revision = 0;
    // SAFETY: aligned holds the full self-relative descriptor and output pointers are valid.
    win(unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) })?;
    let flags = if control & SE_DACL_PROTECTED != 0 { PROTECTED_DACL_SECURITY_INFORMATION } else { UNPROTECTED_DACL_SECURITY_INFORMATION };
    // SAFETY: applies captured owner/group/DACL to the owner-only temporary, never to an unchecked target.
    win(unsafe { SetKernelObjectSecurity(handle(file), SECURITY | flags, descriptor) })
}

fn valid_protection(saved: &[u8]) -> bool {
    if saved.len() < 20 || saved[0] != 1 || u16::from_le_bytes([saved[2], saved[3]]) & 0x8000 == 0 {
        return false;
    }
    for at in [4, 8, 12, 16] {
        let offset = u32::from_le_bytes(saved[at..at + 4].try_into().unwrap()) as usize;
        if offset == 0 {
            continue;
        }
        if offset < 20 || offset > saved.len().saturating_sub(8) {
            return false;
        }
        let size = if at <= 8 {
            if saved[offset] != 1 || saved[offset + 1] > 15 {
                return false;
            }
            8 + usize::from(saved[offset + 1]) * 4
        } else {
            let size = usize::from(u16::from_le_bytes([saved[offset + 2], saved[offset + 3]]));
            if size < 8 {
                return false;
            }
            size
        };
        if size > saved.len() - offset {
            return false;
        }
    }
    true
}

pub fn fingerprint(file: &File) -> io::Result<Vec<u8>> {
    Ok(file_metadata(file)?.0)
}

fn file_metadata(file: &File) -> io::Result<(Vec<u8>, u64)> {
    let i = info(file)?;
    if i.nNumberOfLinks != 1 {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let mut basic = FILE_BASIC_INFO::default();
    // SAFETY: the output has the native size and the validated file handle remains held.
    win(unsafe {
        GetFileInformationByHandleEx(
            handle(file),
            FileBasicInfo,
            (&mut basic as *mut FILE_BASIC_INFO).cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
    })?;
    let mut fingerprint: Vec<_> = [
        i.dwVolumeSerialNumber,
        i.nFileIndexHigh,
        i.nFileIndexLow,
        i.nFileSizeHigh,
        i.nFileSizeLow,
        i.ftLastWriteTime.dwHighDateTime,
        i.ftLastWriteTime.dwLowDateTime,
    ]
    .iter()
    .flat_map(|v| v.to_le_bytes())
    .collect();
    // Include change time in comparisons; cache reuse additionally requires the USN generation below.
    fingerprint.extend(basic.ChangeTime.to_le_bytes());
    Ok((fingerprint, (u64::from(i.nFileSizeHigh) << 32) | u64::from(i.nFileSizeLow)))
}

/// A filesystem generation is required for cache reuse: timestamps alone can be restored or share one clock tick.
pub fn cache_fingerprint(file: &File) -> io::Result<Option<Vec<u8>>> {
    Ok(capture_metadata(file)?.1)
}

pub fn capture_metadata(file: &File) -> io::Result<super::CaptureMetadata> {
    let (fingerprint, len) = file_metadata(file)?;
    let versions = [2u16, 2u16];
    let mut record = [0u8; 1024];
    let mut returned = 0;
    // SAFETY: the synchronous handle remains held, and both buffers have their reported native sizes.
    let success = unsafe {
        DeviceIoControl(
            handle(file),
            0x000900eb,
            versions.as_ptr().cast(),
            std::mem::size_of_val(&versions) as u32,
            record.as_mut_ptr().cast(),
            record.len() as u32,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if success == 0 {
        return Ok((fingerprint, None, len));
    }
    let record_length = u32::from_le_bytes(record[..4].try_into().unwrap());
    if returned < 60 || record_length < 60 || record_length > returned || u16::from_le_bytes(record[4..6].try_into().unwrap()) != 2 {
        return Ok((fingerprint, None, len));
    }
    if i64::from_le_bytes(record[24..32].try_into().unwrap()) <= 0 {
        return Ok((fingerprint, None, len));
    }
    if self::fingerprint(file)? != fingerprint {
        return Err(changed());
    }
    let mut cache = fingerprint.clone();
    cache.extend_from_slice(&record[24..32]);
    Ok((fingerprint, Some(cache), len))
}

pub fn allocated(file: &File) -> io::Result<u64> {
    let mut data = FILE_STANDARD_INFO::default();
    // SAFETY: the output is a correctly sized native struct and the file handle is held open.
    win(unsafe {
        GetFileInformationByHandleEx(
            handle(file),
            FileStandardInfo,
            (&mut data as *mut FILE_STANDARD_INFO).cast(),
            std::mem::size_of::<FILE_STANDARD_INFO>() as u32,
        )
    })?;
    Ok(data.AllocationSize.max(0) as u64)
}

pub fn private_dir(path: &Path) -> io::Result<()> {
    let root = Root::open(path.parent().ok_or(io::ErrorKind::InvalidInput)?)?;
    let security = private_security()?;
    let name = path.file_name().ok_or(io::ErrorKind::InvalidInput)?;
    // SAFETY: Security owns the valid self-relative descriptor returned by Windows.
    let bytes = unsafe { std::slice::from_raw_parts(security.0.cast::<u8>(), GetSecurityDescriptorLength(security.0) as usize) };
    let existing = match open_at(&root.dir, name, true, FILE_OPEN, false, false, None) {
        Ok(dir) => Some(dir),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    if let Some(dir) = existing {
        let saved = protection(&dir)?;
        if descriptor_piece(&saved, 4) != descriptor_piece(bytes, 4) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        if descriptor_piece(&saved, 16) == descriptor_piece(bytes, 16) && u16::from_le_bytes([saved[2], saved[3]]) & SE_DACL_PROTECTED != 0
        {
            return Ok(());
        }
        drop(dir);
    }
    let dir = open_at(&root.dir, name, true, FILE_OPEN_IF, true, false, Some(&security))?;
    // SAFETY: the descriptor was created here; tightening the DACL never changes an existing owner or group.
    win(unsafe {
        SetKernelObjectSecurity(
            handle(&dir),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            bytes.as_ptr().cast_mut().cast(),
        )
    })
}

fn descriptor_piece(bytes: &[u8], at: usize) -> Option<&[u8]> {
    if !valid_protection(bytes) {
        return None;
    }
    let offset = u32::from_le_bytes(bytes[at..at + 4].try_into().ok()?) as usize;
    if offset == 0 {
        return None;
    }
    let size = if at <= 8 {
        8 + usize::from(bytes[offset + 1]) * 4
    } else {
        usize::from(u16::from_le_bytes([bytes[offset + 2], bytes[offset + 3]]))
    };
    bytes.get(offset..offset + size)
}

#[cfg(test)]
pub(crate) fn same_protection(actual: &[u8], saved: &[u8]) -> bool {
    valid_protection(actual) && valid_protection(saved)
        && [4, 8, 16].into_iter().all(|at| descriptor_piece(actual, at) == descriptor_piece(saved, at))
        // Applying an explicit DACL can clear the bookkeeping flag AUTO_INHERITED without changing any ACE.
        && (u16::from_le_bytes([actual[2], actual[3]]) & SE_DACL_PROTECTED)
            == (u16::from_le_bytes([saved[2], saved[3]]) & SE_DACL_PROTECTED)
}

pub fn private_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    Root::open(path.parent().ok_or(io::ErrorKind::InvalidInput)?)?
        .private_write(path.file_name().and_then(|s| s.to_str()).ok_or(io::ErrorKind::InvalidInput)?, bytes)
}

pub fn private_create(path: &Path) -> io::Result<File> {
    Root::open(path.parent().ok_or(io::ErrorKind::InvalidInput)?)?
        .private_create(path.file_name().and_then(|s| s.to_str()).ok_or(io::ErrorKind::InvalidInput)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewind_atomic_metadata_replacement_preserves_protection_and_releases_its_destination() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("journal.json");
        std::fs::write(&path, b"old journal").unwrap();
        let root = Root::open(folder.path()).unwrap();
        let file = root.open_file("journal.json").unwrap();
        let identity = fingerprint(&file).unwrap();
        let security = protection(&file).unwrap();
        drop(file);

        let failed = root.replace_owned_with("journal.json", (&identity, b"old journal"), (b"new journal", &security), || {
            assert_eq!(std::fs::read(&path)?, b"old journal");
            Err(io::ErrorKind::Interrupted.into())
        });
        assert_eq!(failed.unwrap_err().kind(), io::ErrorKind::Interrupted);
        assert_eq!(std::fs::read(&path).unwrap(), b"old journal");
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);

        root.replace_owned("journal.json", (&identity, b"old journal"), (b"new journal", &security)).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new journal");
        let file = root.open_file("journal.json").unwrap();
        assert!(same_protection(&protection(&file).unwrap(), &security));
        let identity = fingerprint(&file).unwrap();
        assert!(root.replace_owned("journal.json", (&identity, b"new journal"), (b"third journal", &security)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"new journal", "another reader still prevents replacement");
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
        drop(file);
        root.replace_owned("journal.json", (&identity, b"new journal"), (b"third journal", &security)).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"third journal");
    }
    use std::io::Read;

    #[test]
    fn rewind_owned_removal_refuses_a_name_replaced_before_deletion() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("orphan");
        std::fs::write(&path, b"owned original").unwrap();
        let root = Root::open(folder.path()).unwrap();
        let result = root.remove_owned_with("orphan", || {
            std::fs::rename(&path, folder.path().join("moved"))?;
            std::fs::write(&path, b"racing replacement")
        });
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::WouldBlock);
        assert_eq!(std::fs::read(&path).unwrap(), b"racing replacement");
        assert_eq!(std::fs::read(folder.path().join("moved")).unwrap(), b"owned original");
    }

    #[test]
    fn rewind_created_temporary_cleanup_keeps_a_substitution_before_cleanup() {
        use std::io::{Seek, Write};
        let folder = tempfile::tempdir().unwrap();
        let root = Root::open(folder.path()).unwrap();
        let mut created = root.private_create("temporary").unwrap();
        created.write_all(b"created bytes").unwrap();
        rename_handle(&created, folder.path().canonicalize().unwrap().join("moved").as_path(), false).unwrap();
        std::fs::write(folder.path().join("temporary"), b"never preserved").unwrap();
        assert_eq!(root.remove_preserved("temporary", &created).unwrap(), Some("temporary".to_owned()));
        assert_eq!(std::fs::read(folder.path().join("temporary")).unwrap(), b"never preserved");
        let mut bytes = vec![];
        created.rewind().unwrap();
        created.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"created bytes");
    }

    #[test]
    fn rewind_final_cleanup_deletes_the_preserved_handle_even_when_its_name_is_replaced() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("displaced");
        std::fs::write(&path, b"preserved").unwrap();
        let root = Root::open(folder.path()).unwrap();
        let preserved = root.open_removable("displaced").unwrap();
        assert_eq!(
            root.remove_preserved_with("displaced", &preserved, || {
                std::fs::rename(&path, folder.path().join("moved"))?;
                std::fs::write(&path, b"final-window replacement")
            })
            .unwrap(),
            None
        );
        drop(preserved);
        assert_eq!(std::fs::read(&path).unwrap(), b"final-window replacement");
        assert!(!folder.path().join("moved").exists());
    }

    #[test]
    fn rewind_returning_a_displaced_file_uses_a_handle_without_read_data_access() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("displaced"), b"cannot read through the returning handle").unwrap();
        let root = Root::open(folder.path()).unwrap();
        let mut metadata = open_at_access(&root.dir, OsStr::new("displaced"), false, FILE_OPEN, false, true, None, true, false).unwrap();
        assert_eq!(metadata.read(&mut [0u8; 1]).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        drop(metadata);
        root.rename_new("displaced", "original").unwrap();
        assert!(!folder.path().join("displaced").exists());
        assert_eq!(std::fs::read(folder.path().join("original")).unwrap(), b"cannot read through the returning handle");
    }

    #[test]
    fn rewind_resolved_path_uses_the_held_files_long_name() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("credentials.json");
        std::fs::write(&path, b"fake credentials").unwrap();
        let root = Root::open(folder.path()).unwrap();
        let file = root.open_file("credentials.json").unwrap();
        assert_eq!(resolved_path(&file).unwrap().unwrap(), path.canonicalize().unwrap());
        let mut short = vec![0u16; 32768];
        // SAFETY: the path is null-terminated and the writable output buffer has the reported length.
        let length = unsafe {
            windows_sys::Win32::Storage::FileSystem::GetShortPathNameW(
                wide(path.as_os_str()).as_ptr(),
                short.as_mut_ptr(),
                short.len() as u32,
            )
        } as usize;
        assert!(length > 0 && length < short.len());
        let short = PathBuf::from(std::ffi::OsString::from_wide(&short[..length]));
        let leaf = short.file_name().unwrap().to_str().unwrap();
        // Volumes can disable short-name creation; when an alias exists it must resolve to the same denied long name.
        if !leaf.eq_ignore_ascii_case("credentials.json") {
            let alias = root.open_file(leaf).unwrap();
            assert_eq!(resolved_path(&alias).unwrap().unwrap(), path.canonicalize().unwrap());
        }
    }

    #[test]
    fn changes_parent_cannot_be_replaced_between_validation_and_file_open() {
        let folder = tempfile::tempdir().unwrap();
        let parent = folder.path().join("notes");
        std::fs::create_dir(&parent).unwrap();
        std::fs::write(parent.join("report.txt"), b"inside").unwrap();
        let root = Root::open(folder.path()).unwrap();
        let mut file = root
            .open_with("notes/report.txt", || {
                assert_eq!(std::fs::rename(&parent, folder.path().join("moved")).unwrap_err().raw_os_error(), Some(32));
            })
            .unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"inside");
    }

    #[test]
    fn rewind_leaf_swap_to_a_hardlink_is_refused_before_reading() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("leaf"), b"inside").unwrap();
        std::fs::write(folder.path().join("secret"), b"outside").unwrap();
        let root = Root::open(folder.path()).unwrap();
        assert!(
            root.open_with("leaf", || {
                std::fs::remove_file(folder.path().join("leaf")).unwrap();
                std::fs::hard_link(folder.path().join("secret"), folder.path().join("leaf")).unwrap();
            })
            .is_err()
        );
    }

    #[test]
    fn rewind_fingerprint_detects_same_size_writes_with_restored_modified_time() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("leaf");
        std::fs::write(&path, b"before").unwrap();
        let root = Root::open(folder.path()).unwrap();
        let file = root.open_file("leaf").unwrap();
        let modified = file.metadata().unwrap().modified().unwrap();
        let before = cache_fingerprint(&file).unwrap();
        let before_stamp = fingerprint(&file).unwrap();
        drop(file);
        let mut writer = OpenOptions::new().write(true).open(&path).unwrap();
        writer.write_all(b"after!").unwrap();
        writer.set_modified(modified).unwrap();
        writer.sync_all().unwrap();
        drop(writer);
        let file = root.open_file("leaf").unwrap();
        assert_eq!(file.metadata().unwrap().modified().unwrap(), modified);
        assert_eq!(file.metadata().unwrap().len(), 6);
        let after = cache_fingerprint(&file).unwrap();
        let after_stamp = fingerprint(&file).unwrap();
        eprintln!(
            "rewind cache generation: before={:?} after={:?}; metadata_changed={}",
            before.as_ref().map(|token| i64::from_le_bytes(token[token.len() - 8..].try_into().unwrap())),
            after.as_ref().map(|token| i64::from_le_bytes(token[token.len() - 8..].try_into().unwrap())),
            before_stamp != after_stamp
        );
        // Unsupported filesystems must disable reuse instead of trusting an unchanged timestamp.
        assert!(after.is_none() || after != before);
    }

    #[test]
    fn rewind_private_creation_has_one_owner_ace_before_writing() {
        let folder = tempfile::tempdir().unwrap();
        let mut file = private_create(&folder.path().join("private")).unwrap();
        let saved = protection(&file).unwrap();
        let owner = descriptor_piece(&saved, 4).unwrap();
        let acl = descriptor_piece(&saved, 16).unwrap();
        assert_eq!(u16::from_le_bytes([acl[4], acl[5]]), 1);
        assert_eq!(acl[8], 0);
        assert_eq!(&acl[16..], owner);
        assert!(u16::from_le_bytes([saved[2], saved[3]]) & SE_DACL_PROTECTED != 0);
        file.write_all(b"only after the protection check").unwrap();
    }
}

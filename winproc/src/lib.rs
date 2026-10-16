//! A process created with `CREATE_SUSPENDED` runs nothing until its main thread is resumed. Faber creates every CLI that way,
//! puts it in its job object, and only then resumes it, so nothing the CLI starts can ever be outside the job. Neither std nor
//! tokio gives the thread handle, so the thread is found through a toolhelp snapshot. Asking whether a process has ended is the other call. This is the only unsafe code in Faber.
//!
//! While a process is still suspended, two more calls act on it: lowering its token to Low integrity (a proof of code a
//! role wrote may then write only into folders labelled Low, `label_low`) and reading which file its image came from.

#[cfg(windows)]
pub use imp::{image_path, label_desktop_low, label_low, lower_to_low_integrity, process_ended, resume_process};

#[cfg(windows)]
mod label {
    use std::ffi::OsString;
    use std::io;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ACCESS_DENIED, LocalFree};
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, EXPLICIT_ACCESS_W, GRANT_ACCESS, GetNamedSecurityInfoW, NO_MULTIPLE_TRUSTEE,
        SDDL_REVISION_1, SE_FILE_OBJECT, SE_WINDOW_OBJECT, SetEntriesInAclW, SetNamedSecurityInfoW, SetSecurityInfo, TRUSTEE_IS_SID,
        TRUSTEE_IS_WELL_KNOWN_GROUP, TRUSTEE_W,
    };
    use windows_sys::Win32::Security::{
        CreateWellKnownSid, DACL_SECURITY_INFORMATION, GetLengthSid, GetSecurityDescriptorSacl, GetSidSubAuthority,
        GetSidSubAuthorityCount, GetTokenInformation, LABEL_SECURITY_INFORMATION, SECURITY_MAX_SID_SIZE, SID_AND_ATTRIBUTES,
        SUB_CONTAINERS_AND_OBJECTS_INHERIT, SetTokenInformation, TOKEN_ADJUST_DEFAULT, TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
        TokenIntegrityLevel, WinCreatorOwnerRightsSid, WinLowLabelSid,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };

    const SE_GROUP_INTEGRITY: u32 = 0x20;
    const SECURITY_MANDATORY_LOW_RID: u32 = 0x1000;

    /// Runs `f` on a query handle of process `pid`, closed afterwards. The callers pass a process they created and still
    /// hold, so the pid cannot have been reused.
    fn with_process<T>(pid: u32, f: impl FnOnce(HANDLE) -> io::Result<T>) -> io::Result<T> {
        // SAFETY: the handle is opened and closed here and only lent to `f` meanwhile.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return Err(io::Error::last_os_error());
        }
        let result = f(process);
        // SAFETY: closes the handle opened above.
        unsafe { CloseHandle(process) };
        result
    }

    /// Sets the Low integrity level on the primary token of process `pid`, a process this caller created suspended and that
    /// has run nothing yet, and reads it back. Everything it starts inherits the Low token. Lowering a level needs no
    /// privilege; raising one would.
    pub fn lower_to_low_integrity(pid: u32) -> io::Result<()> {
        with_process(pid, lower)
    }

    fn lower(process: HANDLE) -> io::Result<()> {
        // SAFETY: the token handle is opened and closed here; the SID lives in a local buffer of the documented maximum size,
        // and the label read back lands in an 8-byte-aligned buffer large enough for a label and its SID.
        unsafe {
            let mut token = std::ptr::null_mut();
            if OpenProcessToken(process, TOKEN_ADJUST_DEFAULT | TOKEN_QUERY, &mut token) == 0 {
                return Err(io::Error::last_os_error());
            }
            let result = (|| {
                let mut sid = [0u8; SECURITY_MAX_SID_SIZE as usize];
                let mut size = SECURITY_MAX_SID_SIZE;
                if CreateWellKnownSid(WinLowLabelSid, std::ptr::null_mut(), sid.as_mut_ptr().cast(), &mut size) == 0 {
                    return Err(io::Error::last_os_error());
                }
                let label =
                    TOKEN_MANDATORY_LABEL { Label: SID_AND_ATTRIBUTES { Sid: sid.as_mut_ptr().cast(), Attributes: SE_GROUP_INTEGRITY } };
                let length = std::mem::size_of::<TOKEN_MANDATORY_LABEL>() as u32 + GetLengthSid(sid.as_mut_ptr().cast());
                if SetTokenInformation(token, TokenIntegrityLevel, (&label as *const TOKEN_MANDATORY_LABEL).cast(), length) == 0 {
                    return Err(io::Error::last_os_error());
                }
                let mut read = [0u64; 16];
                let mut returned = 0;
                if GetTokenInformation(
                    token,
                    TokenIntegrityLevel,
                    read.as_mut_ptr().cast(),
                    std::mem::size_of_val(&read) as u32,
                    &mut returned,
                ) == 0
                {
                    return Err(io::Error::last_os_error());
                }
                let now = &*(read.as_ptr() as *const TOKEN_MANDATORY_LABEL);
                let count = u32::from(*GetSidSubAuthorityCount(now.Label.Sid));
                if count == 0 || *GetSidSubAuthority(now.Label.Sid, count - 1) != SECURITY_MANDATORY_LOW_RID {
                    return Err(io::Error::other("the process token is not at Low integrity"));
                }
                Ok(())
            })();
            CloseHandle(token);
            result
        }
    }

    /// Labels a folder Low with inheritance, for every file and folder below it now and later: a Low-integrity process may
    /// write there. Nothing else changes (owner, access lists). Windows applies the inheritable label to what is already inside.
    pub fn label_low(path: &Path) -> io::Result<()> {
        let raw = path.as_os_str().to_string_lossy();
        // A verbatim drive path (`\\?\C:\...`) is given in its ordinary form; other paths are passed as they are.
        let plain = match raw.strip_prefix(r"\\?\") {
            Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
            _ => path.to_path_buf(),
        };
        let wide: Vec<u16> = plain.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let sddl: Vec<u16> = "S:(ML;OICI;NW;;;LW)".encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: both strings are NUL-terminated and outlive the calls; the descriptor is freed with LocalFree as documented,
        // after the last use of the SACL that points into it.
        unsafe {
            let mut descriptor = std::ptr::null_mut();
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut descriptor, std::ptr::null_mut())
                == 0
            {
                return Err(io::Error::last_os_error());
            }
            let (mut present, mut defaulted, mut sacl) = (0, 0, std::ptr::null_mut());
            let code = if GetSecurityDescriptorSacl(descriptor, &mut present, &mut sacl, &mut defaulted) == 0 || present == 0 {
                Err(io::Error::other("the Low label could not be built"))
            } else {
                let set = || {
                    SetNamedSecurityInfoW(
                        wide.as_ptr(),
                        SE_FILE_OBJECT,
                        LABEL_SECURITY_INFORMATION,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null(),
                        sacl,
                    )
                };
                // Setting a label needs WRITE_OWNER. A folder that grants its user only Modify (common on a second drive)
                // lacks it, so the owner (this user, who made the folder) first grants itself that right.
                let mut code = set();
                if code == ERROR_ACCESS_DENIED {
                    code = grant_owner_rights(&wide);
                    if code == 0 {
                        code = set();
                    }
                }
                match code {
                    0 => Ok(()),
                    code => Err(io::Error::from_raw_os_error(code as i32)),
                }
            };
            LocalFree(descriptor);
            code
        }
    }

    /// Adds one inheritable entry to a folder's access list: its owner may read and change its permissions and owner. Only
    /// the owner can do this (owners may always change the access list). Returns the Win32 error code, 0 on success.
    ///
    /// SAFETY: `wide` is a NUL-terminated path.
    unsafe fn grant_owner_rights(wide: &[u16]) -> u32 {
        const READ_CONTROL: u32 = 0x0002_0000;
        const WRITE_DAC: u32 = 0x0004_0000;
        const WRITE_OWNER: u32 = 0x0008_0000;
        // SAFETY: the SID buffer has the documented maximum size; the old access list points into `descriptor` and the new one
        // is allocated by SetEntriesInAclW; both are freed with LocalFree after their last use.
        unsafe {
            let mut sid = [0u8; SECURITY_MAX_SID_SIZE as usize];
            let mut size = SECURITY_MAX_SID_SIZE;
            if CreateWellKnownSid(WinCreatorOwnerRightsSid, std::ptr::null_mut(), sid.as_mut_ptr().cast(), &mut size) == 0 {
                return ERROR_ACCESS_DENIED;
            }
            let (mut old, mut descriptor) = (std::ptr::null_mut(), std::ptr::null_mut());
            let code = GetNamedSecurityInfoW(
                wide.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut old,
                std::ptr::null_mut(),
                &mut descriptor,
            );
            if code != 0 {
                return code;
            }
            let entry = EXPLICIT_ACCESS_W {
                grfAccessPermissions: READ_CONTROL | WRITE_DAC | WRITE_OWNER,
                grfAccessMode: GRANT_ACCESS,
                grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT,
                Trustee: TRUSTEE_W {
                    pMultipleTrustee: std::ptr::null_mut(),
                    MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                    TrusteeForm: TRUSTEE_IS_SID,
                    TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
                    ptstrName: sid.as_mut_ptr().cast(),
                },
            };
            let mut new = std::ptr::null_mut();
            let mut code = SetEntriesInAclW(1, &entry, old, &mut new);
            if code == 0 {
                code = SetNamedSecurityInfoW(
                    wide.as_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    new,
                    std::ptr::null(),
                );
                LocalFree(new.cast());
            }
            LocalFree(descriptor);
            code
        }
    }

    /// Labels a desktop Low, so a Low-integrity process started on it can attach to it (user32 cannot start otherwise).
    /// For a desktop of its own that a test runner made; the person's desktop already allows it. `desktop` is the handle's
    /// value; the system checks it.
    pub fn label_desktop_low(desktop: isize) -> io::Result<()> {
        let sddl: Vec<u16> = "S:(ML;;NW;;;LW)".encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: the string is NUL-terminated; the descriptor is freed with LocalFree after the last use of its SACL.
        unsafe {
            let mut descriptor = std::ptr::null_mut();
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut descriptor, std::ptr::null_mut())
                == 0
            {
                return Err(io::Error::last_os_error());
            }
            let (mut present, mut defaulted, mut sacl) = (0, 0, std::ptr::null_mut());
            let code = if GetSecurityDescriptorSacl(descriptor, &mut present, &mut sacl, &mut defaulted) == 0 || present == 0 {
                u32::MAX
            } else {
                SetSecurityInfo(
                    desktop as HANDLE,
                    SE_WINDOW_OBJECT,
                    LABEL_SECURITY_INFORMATION,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    sacl,
                )
            };
            LocalFree(descriptor);
            match code {
                0 => Ok(()),
                u32::MAX => Err(io::Error::other("the Low label could not be built")),
                code => Err(io::Error::from_raw_os_error(code as i32)),
            }
        }
    }

    /// The file the image of process `pid` was loaded from.
    pub fn image_path(pid: u32) -> io::Result<PathBuf> {
        with_process(pid, image)
    }

    fn image(process: HANDLE) -> io::Result<PathBuf> {
        let mut buffer = vec![0u16; 32 * 1024];
        let mut size = buffer.len() as u32;
        // SAFETY: the buffer and its length are passed together; the call writes at most `size` characters.
        if unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut size) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(PathBuf::from(OsString::from_wide(&buffer[..size as usize])))
    }
}

#[cfg(windows)]
mod imp {
    pub use super::label::{image_path, label_desktop_low, label_low, lower_to_low_integrity};
    use std::io;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, OpenThread, PROCESS_QUERY_LIMITED_INFORMATION, ResumeThread, THREAD_SUSPEND_RESUME,
    };

    const STILL_ACTIVE: u32 = 259;

    /// Whether no process with this pid is running any more. A pid that was reused answers "running", so a caller waiting for
    /// the end only ever waits too long, never too short.
    pub fn process_ended(pid: u32) -> bool {
        // SAFETY: a plain query on a handle opened and closed here; `code` is a plain integer the call fills in.
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return true;
            }
            let mut code = 0;
            let known = GetExitCodeProcess(process, &mut code) != 0;
            CloseHandle(process);
            known && code != STILL_ACTIVE
        }
    }

    /// Lets the (only) thread of a process that was created suspended run. Fails when no thread of the process was resumed.
    pub fn resume_process(pid: u32) -> io::Result<()> {
        // SAFETY: plain Win32 calls on handles opened and closed in this function; `entry` is a zeroed plain-data struct whose
        // size field is set as the API requires.
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            let mut entry: THREADENTRY32 = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
            let mut resumed = 0;
            let mut more = Thread32First(snapshot, &mut entry);
            while more != 0 {
                if entry.th32OwnerProcessID == pid {
                    let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                    if !thread.is_null() {
                        if ResumeThread(thread) != u32::MAX {
                            resumed += 1;
                        }
                        CloseHandle(thread);
                    }
                }
                more = Thread32Next(snapshot, &mut entry);
            }
            CloseHandle(snapshot);
            if resumed == 0 { Err(io::Error::other("no thread of the process could be resumed")) } else { Ok(()) }
        }
    }
}

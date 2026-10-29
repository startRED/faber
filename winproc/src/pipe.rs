//! The named pipe of Faber's command line (`channel/`). Windows gives a new pipe a list that lets every account read it, so
//! the pipe is made here with a list that names only the signed-in user, and each end checks the account of the process on
//! the other side: a pipe another account made first under the same name is refused, and so is a client of another account.
//! No integrity label is set, so a Low integrity process (a mission proof) cannot write to it. A pipe such a process made
//! first, while Faber was closed, is refused too: each end also wants the other at its own integrity or above, and in no
//! sandbox (an app container or a restricted token).

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    EqualSid, GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, IsTokenRestricted, SECURITY_ATTRIBUTES,
    TOKEN_INFORMATION_CLASS, TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TOKEN_USER, TokenIntegrityLevel, TokenIsAppContainer, TokenUser,
};
use windows_sys::Win32::System::Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION};

fn win(ok: i32) -> io::Result<()> {
    if ok == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
}

/// One class of information about a token, in a buffer sized by Windows.
///
/// # Safety
/// `token` must be an open token handle with `TOKEN_QUERY`.
unsafe fn token_info(token: HANDLE, class: TOKEN_INFORMATION_CLASS) -> io::Result<Vec<usize>> {
    // SAFETY: the caller's token is open; the buffer is sized by the first call and aligned for any of the structures.
    unsafe {
        let mut size = 0;
        GetTokenInformation(token, class, std::ptr::null_mut(), 0, &mut size);
        let mut buffer = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>()).max(1)];
        win(GetTokenInformation(token, class, buffer.as_mut_ptr().cast(), size, &mut size))?;
        Ok(buffer)
    }
}

/// What the pipe's ends compare: the account of a process's token, its integrity and whether it runs in a sandbox.
struct User {
    /// A `TOKEN_USER`.
    user: Vec<usize>,
    /// The integrity level's relative id (Low 0x1000, Medium 0x2000, High 0x3000).
    integrity: u32,
    sandboxed: bool,
}

impl User {
    fn of(process: HANDLE) -> io::Result<User> {
        // SAFETY: the token handle is closed before returning; each buffer holds the structure its class names.
        unsafe {
            let mut token = std::ptr::null_mut();
            win(OpenProcessToken(process, TOKEN_QUERY, &mut token))?;
            let result = (|| {
                let user = token_info(token, TokenUser)?;
                let label = token_info(token, TokenIntegrityLevel)?;
                let sid = (*label.as_ptr().cast::<TOKEN_MANDATORY_LABEL>()).Label.Sid;
                let count = *GetSidSubAuthorityCount(sid);
                let integrity = if count == 0 { 0 } else { *GetSidSubAuthority(sid, u32::from(count) - 1) };
                let container = token_info(token, TokenIsAppContainer)?;
                let sandboxed = *container.as_ptr().cast::<u32>() != 0 || IsTokenRestricted(token) != 0;
                Ok(User { user, integrity, sandboxed })
            })();
            CloseHandle(token);
            result
        }
    }

    fn current() -> io::Result<User> {
        // SAFETY: the pseudo handle of the current process needs no closing.
        User::of(unsafe { GetCurrentProcess() })
    }

    fn of_pid(pid: u32) -> io::Result<User> {
        // SAFETY: the process handle is closed after its token is read.
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return Err(io::Error::last_os_error());
            }
            let user = User::of(process);
            CloseHandle(process);
            user
        }
    }

    fn sid(&self) -> *mut core::ffi::c_void {
        // SAFETY: the buffer holds a TOKEN_USER written by GetTokenInformation, and lives as long as self.
        unsafe { (*self.user.as_ptr().cast::<TOKEN_USER>()).User.Sid }
    }

    /// The same account, at this process's integrity or above, and in no sandbox.
    fn trusted_by(&self, us: &User) -> bool {
        // SAFETY: both SIDs point into live TOKEN_USER buffers.
        let same = unsafe { EqualSid(self.sid(), us.sid()) != 0 };
        same && self.integrity >= us.integrity && !self.sandboxed
    }

    fn sid_string(&self) -> io::Result<String> {
        // SAFETY: the string is a LocalAlloc allocation, read up to its terminating zero and then freed.
        unsafe {
            let mut text = std::ptr::null_mut();
            win(ConvertSidToStringSidW(self.sid(), &mut text))?;
            let mut len = 0;
            while *text.add(len) != 0 {
                len += 1;
            }
            let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
            LocalFree(text.cast());
            Ok(sid)
        }
    }
}

/// Makes one instance of the pipe `name` that only the signed-in user can open, from this machine only. The first instance
/// fails when the name is already taken, by another Faber or by anyone else.
pub fn user_pipe(name: &str, first: bool) -> io::Result<NamedPipeServer> {
    let sid = User::current()?.sid_string()?;
    // Protected list (`P`): full access for this user, nothing for anyone else, nothing inherited.
    let sddl: Vec<u16> = OsStr::new(&format!("D:P(A;;GA;;;{sid})")).encode_wide().chain(Some(0)).collect();
    // SAFETY: the descriptor is a LocalAlloc allocation that outlives the call that uses it and is freed after it.
    unsafe {
        let mut descriptor = std::ptr::null_mut();
        win(ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut descriptor, std::ptr::null_mut()))?;
        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let pipe = ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(name, (&mut attributes as *mut SECURITY_ATTRIBUTES).cast());
        LocalFree(descriptor);
        pipe
    }
}

/// Whether the process at the other end of a connected pipe runs as the signed-in user, at this process's integrity or
/// above and in no sandbox. `client` says which end that is: the server asks about its client, the client about the server.
pub fn pipe_peer_is_user(pipe: &impl AsRawHandle, client: bool) -> io::Result<bool> {
    let mut pid = 0;
    let handle = pipe.as_raw_handle() as HANDLE;
    // SAFETY: the handle is a connected pipe borrowed for the call.
    win(unsafe { if client { GetNamedPipeClientProcessId(handle, &mut pid) } else { GetNamedPipeServerProcessId(handle, &mut pid) } })?;
    Ok(User::of_pid(pid)?.trusted_by(&User::current()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::windows::named_pipe::ClientOptions;

    #[tokio::test]
    async fn the_pipe_is_one_instance_first_and_both_ends_see_the_same_user() {
        let name = format!(r"\\.\pipe\faber-winproc-test-{}", std::process::id());
        let server = user_pipe(&name, true).unwrap();
        assert!(user_pipe(&name, true).is_err(), "a second first instance is refused");
        let client = ClientOptions::new().open(&name).unwrap();
        server.connect().await.unwrap();
        assert!(pipe_peer_is_user(&server, true).unwrap());
        assert!(pipe_peer_is_user(&client, false).unwrap());
    }

    #[test]
    fn the_same_account_below_our_integrity_or_in_a_sandbox_is_not_trusted() {
        let us = User::current().unwrap();
        assert!(us.integrity >= 0x2000, "tests run at Medium integrity or above");
        assert!(User::current().unwrap().trusted_by(&us));
        let low = User { integrity: 0x1000, ..User::current().unwrap() };
        assert!(!low.trusted_by(&us), "a Low integrity process of the same account");
        let boxed = User { sandboxed: true, ..User::current().unwrap() };
        assert!(!boxed.trusted_by(&us), "an app container or a restricted token");
    }
}

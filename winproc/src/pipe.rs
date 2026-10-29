//! The named pipe of Faber's command line (`channel/`). Windows gives a new pipe a list that lets every account read it, so
//! the pipe is made here with a list that names only the signed-in user, and each end checks the account of the process on
//! the other side: a pipe another account made first under the same name is refused, and so is a client of another account.
//! No integrity label is set, so a Low integrity process (a mission proof) cannot write to it.

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{EqualSid, GetTokenInformation, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser};
use windows_sys::Win32::System::Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION};

fn win(ok: i32) -> io::Result<()> {
    if ok == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
}

/// The `TOKEN_USER` of a process's token, in a buffer sized by Windows.
struct User(Vec<usize>);

impl User {
    fn of(process: HANDLE) -> io::Result<User> {
        // SAFETY: the token handle is closed before returning, and the buffer is sized by the first call.
        unsafe {
            let mut token = std::ptr::null_mut();
            win(OpenProcessToken(process, TOKEN_QUERY, &mut token))?;
            let result = (|| {
                let mut size = 0;
                GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut size);
                let mut buffer = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
                win(GetTokenInformation(token, TokenUser, buffer.as_mut_ptr().cast(), size, &mut size))?;
                Ok(User(buffer))
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
        unsafe { (*self.0.as_ptr().cast::<TOKEN_USER>()).User.Sid }
    }

    fn same(&self, other: &User) -> bool {
        // SAFETY: both SIDs point into live TOKEN_USER buffers.
        unsafe { EqualSid(self.sid(), other.sid()) != 0 }
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

/// Whether the process at the other end of a connected pipe runs as the signed-in user. `client` says which end that is:
/// the server asks about its client, the client about the server.
pub fn pipe_peer_is_user(pipe: &impl AsRawHandle, client: bool) -> io::Result<bool> {
    let mut pid = 0;
    let handle = pipe.as_raw_handle() as HANDLE;
    // SAFETY: the handle is a connected pipe borrowed for the call.
    win(unsafe { if client { GetNamedPipeClientProcessId(handle, &mut pid) } else { GetNamedPipeServerProcessId(handle, &mut pid) } })?;
    Ok(User::of_pid(pid)?.same(&User::current()?))
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
}

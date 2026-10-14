//! A process created with `CREATE_SUSPENDED` runs nothing until its main thread is resumed. Faber creates every CLI that way,
//! puts it in its job object, and only then resumes it, so nothing the CLI starts can ever be outside the job. Neither std nor
//! tokio gives the thread handle, so the thread is found through a toolhelp snapshot. This is the only unsafe code in Faber.

#[cfg(windows)]
pub use imp::resume_process;

#[cfg(windows)]
mod imp {
    use std::io;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

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

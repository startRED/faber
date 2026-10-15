//! A process created with `CREATE_SUSPENDED` runs nothing until its main thread is resumed. Faber creates every CLI that way,
//! puts it in its job object, and only then resumes it, so nothing the CLI starts can ever be outside the job. Neither std nor
//! tokio gives the thread handle, so the thread is found through a toolhelp snapshot. This and the one-line priority call below
//! are the only unsafe code in Faber.

#[cfg(windows)]
pub use imp::{lower_priority, resume_process};

#[cfg(windows)]
mod imp {
    use std::io;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::Threading::{
        IDLE_PRIORITY_CLASS, OpenProcess, OpenThread, PROCESS_SET_INFORMATION, ResumeThread, SetPriorityClass, THREAD_SUSPEND_RESUME,
    };

    /// Puts a process (and what it starts afterwards) at idle priority, so a background reading never competes with a game or
    /// the person's own work. Failing to do so changes nothing else.
    pub fn lower_priority(pid: u32) -> io::Result<()> {
        // SAFETY: a plain Win32 call on a handle opened and closed in this function.
        unsafe {
            let process = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
            if process.is_null() {
                return Err(io::Error::last_os_error());
            }
            let done = SetPriorityClass(process, IDLE_PRIORITY_CLASS);
            let error = io::Error::last_os_error();
            CloseHandle(process);
            if done == 0 { Err(error) } else { Ok(()) }
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

#[cfg(all(test, windows))]
mod tests {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{GetPriorityClass, IDLE_PRIORITY_CLASS, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    #[test]
    fn a_background_reading_runs_at_idle_priority() {
        let mut child =
            std::process::Command::new("ping").args(["-n", "4", "127.0.0.1"]).stdout(std::process::Stdio::null()).spawn().unwrap();
        crate::lower_priority(child.id()).unwrap();
        // SAFETY: a handle opened and closed here, on a process this test started.
        let class = unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, child.id());
            let class = GetPriorityClass(handle);
            CloseHandle(handle);
            class
        };
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(class, IDLE_PRIORITY_CLASS);
    }
}

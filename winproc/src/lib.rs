//! A process created with `CREATE_SUSPENDED` runs nothing until its main thread is resumed. Faber creates every CLI that way,
//! puts it in its job object, and only then resumes it, so nothing the CLI starts can ever be outside the job. Neither std nor
//! tokio gives the thread handle, so the thread is found through a toolhelp snapshot. Asking whether a process has ended is the
//! other call. With the Windows API files of the shell (awake, autostart, the chime, pane), this is the only unsafe code in Faber.
//!
//! The test runner reads the system's process list here too (`processes`): one call, where WMI took 100 ms or more.
//! The index worker also uses the safe background-priority wrapper, on Windows and macOS.
//! Project memory uses handle-relative, no-follow file access through `MemoryDirectory` on Windows.
//! The command line's named pipe is made here too, open only to the signed-in user (`pipe.rs`), with the user's language for its words (`locale.rs`).

/// Puts this dedicated worker in macOS's background quality-of-service class until it exits.
#[cfg(target_os = "macos")]
pub fn background_thread() -> std::io::Result<()> {
    unsafe extern "C" {
        fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
    }
    // SAFETY: this call changes only the calling thread's scheduling, using QOS_CLASS_BACKGROUND and no relative offset.
    let error = unsafe { pthread_set_qos_class_self_np(0x09, 0) };
    if error == 0 { Ok(()) } else { Err(std::io::Error::from_raw_os_error(error)) }
}

#[cfg(windows)]
pub use imp::{
    background_thread, full_screen_in_front, job_accounting, job_active_processes, job_cpu_cap, job_memory_cap, job_peak_memory,
    job_terminate, process_ended, resume_process,
};

/// What a job object's processes used together since it was made (`job_accounting`).
#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JobAccounting {
    /// Every process ever started in the job, ended ones included.
    pub processes: u32,
    /// Their processor time, user and kernel, in seconds.
    pub cpu_seconds: f64,
    /// The bytes they wrote, to files and devices.
    pub written_bytes: u64,
}

mod files;
#[cfg(windows)]
mod locale;
#[cfg(windows)]
pub use locale::user_language;
#[cfg(windows)]
mod pipe;
pub use files::{open_catalog_file, open_plain_file};
#[cfg(windows)]
pub use pipe::{pipe_peer_is_user, user_pipe};
#[cfg(windows)]
mod memory;
#[cfg(windows)]
pub use memory::MemoryDirectory;
pub mod front;
pub mod process_tree;
pub mod sound;

#[cfg(unix)]
fn owned_group(pgid: u32) -> std::io::Result<rustix::process::Pid> {
    i32::try_from(pgid)
        .ok()
        .filter(|pid| *pid > 1)
        .and_then(rustix::process::Pid::from_raw)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid owned process group"))
}

/// Signal only a dedicated group created by Faber, never the application's current group.
#[cfg(unix)]
pub fn process_group_terminate(pgid: u32) -> std::io::Result<()> {
    match rustix::process::kill_process_group(owned_group(pgid)?, rustix::process::Signal::KILL) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Only ESRCH proves no member remains. Permission failures and other query failures are not proof.
#[cfg(unix)]
pub fn process_group_empty(pgid: u32) -> std::io::Result<bool> {
    group_empty_result(rustix::process::test_kill_process_group(owned_group(pgid)?))
}

#[cfg(unix)]
fn group_empty_result(result: rustix::io::Result<()>) -> std::io::Result<bool> {
    match result {
        Err(rustix::io::Errno::SRCH) => Ok(true),
        Ok(()) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[cfg(all(test, unix))]
mod group_tests {
    use super::*;

    #[test]
    fn only_no_such_process_confirms_a_group_empty() {
        assert!(group_empty_result(Err(rustix::io::Errno::SRCH)).unwrap());
        assert!(!group_empty_result(Ok(())).unwrap());
        assert!(group_empty_result(Err(rustix::io::Errno::PERM)).is_err());
        for id in [0, 1, u32::MAX] {
            assert!(owned_group(id).is_err());
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::io;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, OpenThread, PROCESS_QUERY_LIMITED_INFORMATION, ResumeThread, THREAD_SUSPEND_RESUME,
    };

    const STILL_ACTIVE: u32 = 259;

    /// Puts the current dedicated worker at idle CPU and background I/O priority, for the rest of its lifetime.
    pub fn background_thread() -> io::Result<()> {
        use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN};
        // SAFETY: the pseudo handle names this thread, and the mode affects only this thread until it exits.
        if unsafe { SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    /// How many processes the job object `job` (a handle) still has running.
    pub fn job_active_processes(job: isize) -> io::Result<u32> {
        use windows_sys::Win32::System::JobObjects::{
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobObjectBasicAccountingInformation, QueryInformationJobObject,
        };
        // SAFETY: `info` is a plain-data struct the call fills in, and the size passed is its size; the handle is only read.
        unsafe {
            let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = std::mem::zeroed();
            let ok = QueryInformationJobObject(
                job as _,
                JobObjectBasicAccountingInformation,
                (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                std::ptr::null_mut(),
            );
            if ok == 0 { Err(io::Error::last_os_error()) } else { Ok(info.ActiveProcesses) }
        }
    }

    /// What the job object `job` (a handle) has used so far: every process ever started in it, their processor time (user
    /// and kernel, in seconds) and the bytes they wrote.
    pub fn job_accounting(job: isize) -> io::Result<super::JobAccounting> {
        use windows_sys::Win32::System::JobObjects::{
            JOBOBJECT_BASIC_AND_IO_ACCOUNTING_INFORMATION, JobObjectBasicAndIoAccountingInformation, QueryInformationJobObject,
        };
        // SAFETY: `info` is a plain-data struct the call fills in, and the size passed is its size; the handle is only read.
        unsafe {
            let mut info: JOBOBJECT_BASIC_AND_IO_ACCOUNTING_INFORMATION = std::mem::zeroed();
            let ok = QueryInformationJobObject(
                job as _,
                JobObjectBasicAndIoAccountingInformation,
                (&mut info as *mut JOBOBJECT_BASIC_AND_IO_ACCOUNTING_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_BASIC_AND_IO_ACCOUNTING_INFORMATION>() as u32,
                std::ptr::null_mut(),
            );
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            // The times are in 100-nanosecond units.
            let ticks = info.BasicInfo.TotalUserTime.saturating_add(info.BasicInfo.TotalKernelTime).max(0) as u64;
            Ok(super::JobAccounting {
                processes: info.BasicInfo.TotalProcesses,
                cpu_seconds: ticks as f64 / 1e7,
                written_bytes: info.IoInfo.WriteTransferCount,
            })
        }
    }

    /// Whether Windows names a full-screen, Direct3D, presentation or store app in front (the states the tray reads, R191);
    /// false when Windows does not say. A borderless game is not seen here.
    pub fn full_screen_in_front() -> bool {
        use windows_sys::Win32::UI::Shell::{
            QUNS_APP, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState,
        };
        let mut state = 0;
        // SAFETY: the call only writes the state into the integer it is given, which lives across it.
        let ok = unsafe { SHQueryUserNotificationState(&mut state) } >= 0;
        ok && [QUNS_BUSY, QUNS_RUNNING_D3D_FULL_SCREEN, QUNS_PRESENTATION_MODE, QUNS_APP].contains(&state)
    }

    /// Caps the processor the job object `job` (a handle) may use at `percent` of the whole machine, as a hard cap, or lifts
    /// the cap with `None`.
    pub fn job_cpu_cap(job: isize, percent: Option<u32>) -> io::Result<()> {
        use windows_sys::Win32::System::JobObjects::{
            JOB_OBJECT_CPU_RATE_CONTROL_ENABLE, JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP, JOBOBJECT_CPU_RATE_CONTROL_INFORMATION,
            JobObjectCpuRateControlInformation, SetInformationJobObject,
        };
        let mut info = JOBOBJECT_CPU_RATE_CONTROL_INFORMATION::default();
        if let Some(percent) = percent {
            info.ControlFlags = JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP;
            // The rate is in hundredths of a percent, from 1 to 10 000.
            info.Anonymous.CpuRate = (percent.clamp(1, 100)) * 100;
        }
        // SAFETY: `info` is a plain-data struct that lives across the call, and the size passed is its size; the handle is
        // one the caller holds open.
        let ok = unsafe {
            SetInformationJobObject(
                job as _,
                JobObjectCpuRateControlInformation,
                (&info as *const JOBOBJECT_CPU_RATE_CONTROL_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_CPU_RATE_CONTROL_INFORMATION>() as u32,
            )
        };
        if ok == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
    }

    /// The most memory the processes of the job object `job` (a handle) held together at once, in bytes.
    pub fn job_peak_memory(job: isize) -> io::Result<u64> {
        use windows_sys::Win32::System::JobObjects::{
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, QueryInformationJobObject,
        };
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        // SAFETY: `info` is a plain-data struct that lives across the call, and the size passed is its size; the handle is
        // one the caller holds open; no returned length is asked for.
        let ok = unsafe {
            QueryInformationJobObject(
                job as _,
                JobObjectExtendedLimitInformation,
                (&mut info as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 { Err(io::Error::last_os_error()) } else { Ok(info.PeakJobMemoryUsed as u64) }
    }

    /// Caps the memory the processes of the job object `job` (a handle) may commit together at `bytes`, keeping the job's
    /// other limits: past it Windows refuses their allocations, so a runaway build fails instead of paging the PC.
    pub fn job_memory_cap(job: isize, bytes: u64) -> io::Result<()> {
        use windows_sys::Win32::System::JobObjects::{
            JOB_OBJECT_LIMIT_JOB_MEMORY, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            QueryInformationJobObject, SetInformationJobObject,
        };
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        let size = std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;
        // SAFETY: `info` is a plain-data struct that lives across both calls, and the size passed is its size; the handle
        // is one the caller holds open; no returned length is asked for.
        let read = unsafe {
            QueryInformationJobObject(
                job as _,
                JobObjectExtendedLimitInformation,
                (&mut info as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size,
                std::ptr::null_mut(),
            )
        };
        if read == 0 {
            return Err(io::Error::last_os_error());
        }
        info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_JOB_MEMORY;
        info.JobMemoryLimit = usize::try_from(bytes).unwrap_or(usize::MAX);
        // SAFETY: as above.
        let ok = unsafe {
            SetInformationJobObject(
                job as _,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size,
            )
        };
        if ok == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
    }

    /// Ends every process in the job object `job` (a handle) at once.
    pub fn job_terminate(job: isize) -> io::Result<()> {
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;
        // SAFETY: a plain call on a job handle the caller holds open.
        if unsafe { TerminateJobObject(job as _, 1) } == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
    }

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

/// Every process on the computer in one system call, with what the test runner's audit and measurements read: its id, its
/// parent's id, its name, when it was created, the processor time it used and its private working set. The test runner
/// used to ask WMI for the same columns, which opened a new COM connection and took 100 ms or more per question.
#[cfg(windows)]
mod processes {
    use std::io;
    use windows_sys::Wdk::System::SystemInformation::{NtQuerySystemInformation, SystemProcessInformation};
    use windows_sys::Win32::Foundation::{HANDLE, UNICODE_STRING};

    const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004_u32 as i32;

    /// One process as the system lists it. Times are in 100 ns ticks; `created` counts from 1601, as `FILETIME` does.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ProcessInfo {
        pub pid: u32,
        pub parent: u32,
        pub name: String,
        pub created: u64,
        pub cpu: u64,
        pub private_bytes: u64,
    }

    /// The start of `SYSTEM_PROCESS_INFORMATION`, with the fields windows-sys leaves named `Reserved` (the layout of the
    /// Windows Driver Kit's `ntexapi.h`). Only this prefix is read; every entry the system writes is longer.
    #[repr(C)]
    struct Entry {
        next_entry_offset: u32,
        number_of_threads: u32,
        working_set_private_size: i64,
        hard_fault_count: u32,
        number_of_threads_high_watermark: u32,
        cycle_time: u64,
        create_time: i64,
        user_time: i64,
        kernel_time: i64,
        image_name: UNICODE_STRING,
        base_priority: i32,
        unique_process_id: HANDLE,
        inherited_from_unique_process_id: HANDLE,
    }

    pub fn processes() -> io::Result<Vec<ProcessInfo>> {
        // 8-byte words, so every entry the system writes is aligned for `Entry`.
        let mut buffer: Vec<u64> = vec![0; 128 * 1024];
        loop {
            let bytes = u32::try_from(buffer.len() * 8).map_err(|_| io::Error::other("the process list is too large"))?;
            let mut needed = 0u32;
            // SAFETY: the buffer is writable for `bytes` bytes and outlives the call; `needed` is a plain integer.
            let status = unsafe { NtQuerySystemInformation(SystemProcessInformation, buffer.as_mut_ptr().cast(), bytes, &mut needed) };
            if status == STATUS_INFO_LENGTH_MISMATCH {
                // Processes may start between two calls; leave room for them.
                buffer.resize(needed as usize / 8 + 8 * 1024, 0);
                continue;
            }
            if status < 0 {
                return Err(io::Error::other(format!("NtQuerySystemInformation failed with status {status:#x}")));
            }
            return Ok(read(&buffer, (needed as usize).min(buffer.len() * 8)));
        }
    }

    /// Walks the entries the call wrote into the first `filled` bytes of `buffer`.
    fn read(buffer: &[u64], filled: usize) -> Vec<ProcessInfo> {
        let base = buffer.as_ptr().cast::<u8>();
        let (mut offset, mut list) = (0usize, Vec::new());
        while offset % 8 == 0 && offset + std::mem::size_of::<Entry>() <= filled {
            // SAFETY: `offset` is a sum of the system's own entry offsets, checked to stay 8-byte aligned, and the check above
            // keeps the whole prefix inside the filled part of the buffer.
            let entry = unsafe { &*base.add(offset).cast::<Entry>() };
            let name = if entry.image_name.Buffer.is_null() || entry.image_name.Length == 0 {
                // The idle process has no name; WMI called it this.
                if entry.unique_process_id.is_null() { "System Idle Process".to_string() } else { String::new() }
            } else {
                // SAFETY: the system points the name into the same buffer, `Length` bytes long.
                let units = unsafe { std::slice::from_raw_parts(entry.image_name.Buffer, usize::from(entry.image_name.Length) / 2) };
                String::from_utf16_lossy(units)
            };
            list.push(ProcessInfo {
                pid: entry.unique_process_id as usize as u32,
                parent: entry.inherited_from_unique_process_id as usize as u32,
                name,
                created: entry.create_time.max(0) as u64,
                cpu: (entry.user_time.max(0) + entry.kernel_time.max(0)) as u64,
                private_bytes: entry.working_set_private_size.max(0) as u64,
            });
            if entry.next_entry_offset == 0 {
                break;
            }
            offset += entry.next_entry_offset as usize;
        }
        list
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_list_holds_this_process_with_its_parent_name_creation_time_and_memory() {
            let list = processes().unwrap();
            assert!(list.len() > 20, "{} processes", list.len());
            let me = list.iter().find(|p| p.pid == std::process::id()).expect("this process is in the list");
            assert!(me.parent != 0 && me.name.to_lowercase().ends_with(".exe") && me.created > 0, "{me:?}");
            assert!(me.private_bytes > 64 * 1024, "{me:?}");
            assert!(list.iter().any(|p| p.pid == 4 && p.name == "System"), "the System process");
        }

        #[test]
        fn processor_time_grows_while_this_process_works() {
            let cpu = || processes().unwrap().into_iter().find(|p| p.pid == std::process::id()).unwrap().cpu;
            let before = cpu();
            let started = std::time::Instant::now();
            let mut x = 0u64;
            while started.elapsed() < std::time::Duration::from_millis(200) {
                x = std::hint::black_box(x.wrapping_mul(31).wrapping_add(7));
            }
            assert!(cpu() > before, "{x}");
        }
    }
}

#[cfg(windows)]
pub use processes::{ProcessInfo, processes};

pub mod rewind;

//! A process created with `CREATE_SUSPENDED` runs nothing until its main thread is resumed. Faber creates every CLI that way,
//! creates it inside its job object, and only then resumes it, so nothing the CLI starts can ever be outside the job. Neither std nor
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
#[cfg(windows)]
mod user_file;
pub use files::{open_catalog_file, open_plain_file};
#[cfg(windows)]
pub use pipe::{PipeClient, pipe_peer_is_user, user_pipe};
#[cfg(windows)]
pub use user_file::write_user_only;
#[cfg(windows)]
mod memory;
#[cfg(windows)]
pub use memory::MemoryDirectory;
pub mod front;
pub mod process_tree;
pub mod sound;

#[cfg(windows)]
pub use launch::{Child, Output, spawn_in_job};

#[cfg(windows)]
mod launch {
    use std::ffi::{OsStr, OsString};
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::os::windows::process::ExitStatusExt;
    use std::sync::Mutex;
    use std::task::{Poll, Waker};
    use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Threading::*;

    struct Exit {
        process: OwnedHandle,
        wait: isize,
        wake: Box<Mutex<[Option<Waker>; 2]>>,
    }

    impl Exit {
        fn new(process: OwnedHandle) -> io::Result<std::sync::Arc<Self>> {
            let mut exit = Self { process, wait: 0, wake: Box::new(Mutex::new([None, None])) };
            let mut wait = std::ptr::null_mut();
            // SAFETY: the boxed context and process outlive the registration; Drop joins the callback.
            if unsafe {
                RegisterWaitForSingleObject(
                    &mut wait,
                    exit.process.as_raw_handle(),
                    Some(ended),
                    (&*exit.wake as *const Mutex<[Option<Waker>; 2]>).cast_mut().cast(),
                    INFINITE,
                    WT_EXECUTEONLYONCE,
                )
            } == 0
            {
                let error = io::Error::last_os_error();
                // SAFETY: startup failed before resume; do not leave a suspended child in the caller's still-open job.
                unsafe {
                    TerminateProcess(exit.process.as_raw_handle(), 1);
                }
                return Err(error);
            }
            exit.wait = wait as isize;
            Ok(std::sync::Arc::new(exit))
        }

        fn poll(&self, cx: &std::task::Context<'_>, slot: usize) -> Poll<io::Result<()>> {
            self.wake.lock().unwrap_or_else(|e| e.into_inner())[slot] = Some(cx.waker().clone());
            // SAFETY: a zero-time wait queries the owned process without blocking the executor.
            match unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } {
                0 => {
                    self.wake.lock().unwrap_or_else(|e| e.into_inner())[slot] = None;
                    Poll::Ready(Ok(()))
                }
                258 => Poll::Pending,
                _ => Poll::Ready(Err(io::Error::last_os_error())),
            }
        }
    }

    /// A suspended CLI born in its kill-on-close job, with a cancellation-safe wait and kill on drop.
    pub struct Child {
        exit: std::sync::Arc<Exit>,
        thread: OwnedHandle,
        pid: u32,
        status: Option<std::process::ExitStatus>,
    }

    impl Child {
        pub fn id(&self) -> Option<u32> {
            Some(self.pid)
        }
        pub fn resume(&self) -> io::Result<()> {
            // SAFETY: this is the owned primary thread, still suspended until this call.
            if unsafe { ResumeThread(self.thread.as_raw_handle()) } == u32::MAX { Err(io::Error::last_os_error()) } else { Ok(()) }
        }
        pub fn start_kill(&mut self) -> io::Result<()> {
            // SAFETY: the owned process handle remains open throughout the call.
            if unsafe { TerminateProcess(self.exit.process.as_raw_handle(), 1) } == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
        }
        pub async fn wait(&mut self) -> io::Result<std::process::ExitStatus> {
            if let Some(status) = self.status {
                return Ok(status);
            }
            std::future::poll_fn(|cx| self.exit.poll(cx, 1)).await?;
            let mut code = 0;
            // SAFETY: the process has signaled; code is writable and the handle is owned.
            if unsafe { GetExitCodeProcess(self.exit.process.as_raw_handle(), &mut code) } == 0 {
                return Err(io::Error::last_os_error());
            }
            let status = std::process::ExitStatus::from_raw(code);
            self.status = Some(status);
            Ok(status)
        }
    }

    unsafe extern "system" fn ended(context: *mut std::ffi::c_void, _: bool) {
        // SAFETY: Exit owns this box and joins the callback before releasing it.
        let wake = unsafe { &*context.cast::<Mutex<[Option<Waker>; 2]>>() };
        let wakers = std::mem::take(&mut *wake.lock().unwrap_or_else(|e| e.into_inner()));
        for waker in wakers.into_iter().flatten() {
            waker.wake();
        }
    }

    impl Drop for Child {
        fn drop(&mut self) {
            if self.status.is_none() {
                let _ = self.start_kill();
            }
        }
    }

    impl Drop for Exit {
        fn drop(&mut self) {
            if self.wait != 0 {
                // SAFETY: this wait registration is ours; the sentinel joins callbacks before the context is dropped.
                if unsafe { UnregisterWaitEx(self.wait as HANDLE, INVALID_HANDLE_VALUE) } == 0 {
                    // Preserve callback memory if Windows cannot confirm that it stopped using it.
                    std::mem::forget(std::mem::take(&mut self.wake));
                }
            }
        }
    }

    /// Root exit ends inherited writers, then drains real EOF, including reads whose completion is still queued.
    pub struct Output {
        pipe: tokio::net::windows::named_pipe::NamedPipeServer,
        exit: std::sync::Arc<Exit>,
        job: OwnedHandle,
        stopped: bool,
    }

    impl tokio::io::AsyncRead for Output {
        fn poll_read(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            if !self.stopped
                && let Poll::Ready(result) = self.exit.poll(cx, 0)
            {
                result?;
                super::job_terminate(self.job.as_raw_handle() as isize)?;
                self.stopped = true;
            }
            std::pin::Pin::new(&mut self.pipe).poll_read(cx, buf)
        }
    }

    fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
        let mut value: Vec<_> = value.encode_wide().collect();
        if value.contains(&0) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "embedded NUL in process input"));
        }
        value.push(0);
        Ok(value)
    }

    fn quote(arg: &OsStr, line: &mut Vec<u16>) -> io::Result<()> {
        line.push(b'"' as u16);
        let mut slashes = 0;
        for c in wide(arg)?.into_iter().take_while(|c| *c != 0) {
            if c == b'\\' as u16 {
                slashes += 1;
                continue;
            }
            line.extend(std::iter::repeat_n(b'\\' as u16, slashes * if c == b'"' as u16 { 2 } else { 1 }));
            if c == b'"' as u16 {
                line.push(b'\\' as u16);
            }
            line.push(c);
            slashes = 0;
        }
        line.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
        line.push(b'"' as u16);
        Ok(())
    }

    struct Attributes(Vec<usize>);
    impl Attributes {
        fn ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
            self.0.as_mut_ptr().cast()
        }
    }
    impl Drop for Attributes {
        fn drop(&mut self) {
            // SAFETY: only a successfully initialized list is wrapped in Attributes.
            unsafe {
                DeleteProcThreadAttributeList(self.ptr());
            }
        }
    }

    /// Creates a CLI with the command's plain arguments, environment overrides and directory, using connected async pipes.
    /// The engine's commands inherit the environment; raw shell arguments and env_clear are not part of this interface.
    pub fn spawn_in_job(
        command: &std::process::Command,
        job: isize,
        flags: u32,
    ) -> io::Result<(Child, tokio::net::windows::named_pipe::NamedPipeServer, Output, tokio::net::windows::named_pipe::NamedPipeServer)>
    {
        use std::fs::OpenOptions;
        use std::os::windows::io::BorrowedHandle;
        use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};
        if !std::path::Path::new(command.get_program()).is_absolute() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "the CLI executable path must be resolved before launch"));
        }
        let application = wide(command.get_program())?;
        // SAFETY: the caller keeps this job open through creation; the duplicate belongs only to this process.
        let job_handle = unsafe { BorrowedHandle::borrow_raw(job as HANDLE) }.try_clone_to_owned()?;
        // A private, first-instance pipe keeps another local process from replacing the CLI's standard streams.
        let pipe = |input: bool| -> io::Result<_> {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let name = format!(r"\\.\pipe\faber-cli-{}-{n}", std::process::id());
            let server = super::pipe::with_user_security(|attributes| {
                // SAFETY: the user-only descriptor stays valid throughout pipe creation.
                unsafe {
                    tokio::net::windows::named_pipe::ServerOptions::new()
                        .first_pipe_instance(true)
                        .reject_remote_clients(true)
                        .access_inbound(!input)
                        .access_outbound(input)
                        .in_buffer_size(4096)
                        .out_buffer_size(4096)
                        .create_with_security_attributes_raw(
                            &name,
                            (attributes as *mut windows_sys::Win32::Security::SECURITY_ATTRIBUTES).cast(),
                        )
                }
            })?;
            let client = OpenOptions::new().read(input).write(!input).open(name)?;
            {
                use std::future::Future;
                let mut connect = std::pin::pin!(server.connect());
                match connect.as_mut().poll(&mut std::task::Context::from_waker(Waker::noop())) {
                    Poll::Ready(result) => result?,
                    Poll::Pending => return Err(io::Error::other("the child pipe did not connect")),
                }
            }
            // SAFETY: only the three child ends are inheritable, and HANDLE_LIST restricts inheritance to them.
            if unsafe { SetHandleInformation(client.as_raw_handle(), HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok((server, client))
        };
        let ((stdin, input), (stdout, output), (stderr, error)) = (pipe(true)?, pipe(false)?, pipe(false)?);
        let handles = [input.as_raw_handle(), output.as_raw_handle(), error.as_raw_handle()];
        let jobs = [job as HANDLE];
        let mut bytes = 0;
        // SAFETY: the first call obtains the required size; the aligned allocation lives through process creation.
        unsafe {
            InitializeProcThreadAttributeList(std::ptr::null_mut(), 2, 0, &mut bytes);
        }
        let mut storage = vec![0usize; bytes.div_ceil(std::mem::size_of::<usize>())];
        // SAFETY: storage is aligned and writable for the requested byte size.
        if unsafe { InitializeProcThreadAttributeList(storage.as_mut_ptr().cast(), 2, 0, &mut bytes) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut attributes = Attributes(storage);
        for (key, values) in
            [(PROC_THREAD_ATTRIBUTE_JOB_LIST as usize, jobs.as_slice()), (PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize, handles.as_slice())]
        {
            // SAFETY: the borrowed handle arrays remain alive until CreateProcessW returns; no handle ownership is transferred.
            if unsafe {
                UpdateProcThreadAttribute(
                    attributes.ptr(),
                    0,
                    key,
                    values.as_ptr().cast(),
                    std::mem::size_of_val(values),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        let mut line = Vec::new();
        quote(command.get_program(), &mut line)?;
        for arg in command.get_args() {
            line.push(b' ' as u16);
            quote(arg, &mut line)?;
        }
        line.push(0);
        let mut env: Vec<(OsString, OsString)> = std::env::vars_os().collect();
        for (key, value) in command.get_envs() {
            env.retain(|(existing, _)| !existing.as_encoded_bytes().eq_ignore_ascii_case(key.as_encoded_bytes()));
            if let Some(value) = value {
                env.push((key.into(), value.into()));
            }
        }
        env.sort_by_key(|(key, _)| key.to_ascii_uppercase());
        let mut environment = Vec::new();
        for (key, value) in env {
            let mut entry = key;
            entry.push("=");
            entry.push(value);
            environment.extend(wide(&entry)?);
        }
        environment.push(0);
        if environment.len() == 1 {
            environment.push(0);
        }
        let cwd = command.get_current_dir().map(|p| wide(p.as_os_str())).transpose()?;
        // SAFETY: all input buffers, handles and the attribute list outlive the call; successful output handles are owned below.
        unsafe {
            let mut startup: STARTUPINFOEXW = std::mem::zeroed();
            startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
            startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            startup.StartupInfo.hStdInput = handles[0];
            startup.StartupInfo.hStdOutput = handles[1];
            startup.StartupInfo.hStdError = handles[2];
            startup.lpAttributeList = attributes.ptr();
            let mut info: PROCESS_INFORMATION = std::mem::zeroed();
            if CreateProcessW(
                application.as_ptr(),
                line.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                flags | EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT | CREATE_SUSPENDED | CREATE_NO_WINDOW,
                environment.as_ptr().cast(),
                cwd.as_ref().map_or(std::ptr::null(), |p| p.as_ptr()),
                &startup.StartupInfo,
                &mut info,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let process = OwnedHandle::from_raw_handle(info.hProcess);
            let thread = OwnedHandle::from_raw_handle(info.hThread);
            let exit = Exit::new(process)?;
            Ok((
                Child { exit: exit.clone(), thread, pid: info.dwProcessId, status: None },
                stdin,
                Output { pipe: stdout, exit, job: job_handle, stopped: false },
                stderr,
            ))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn cli_lifetime_arguments_quote_spaces_quotes_and_trailing_slashes() {
            for (arg, expected) in [("", "\"\""), ("a b", "\"a b\""), ("a\"b", "\"a\\\"b\""), ("a\\", "\"a\\\\\"")] {
                let mut line = Vec::new();
                quote(OsStr::new(arg), &mut line).unwrap();
                assert_eq!(String::from_utf16(&line).unwrap(), expected);
            }
            assert!(quote(OsStr::new("a\0b"), &mut Vec::new()).is_err());
        }
    }
}

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

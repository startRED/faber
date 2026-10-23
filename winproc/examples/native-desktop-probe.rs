//! Fixed synthetic UIA fixture/probe. Never switch desktops or use the person's input desktop.
#[cfg(not(all(windows, feature = "test-hooks")))]
fn main() {
    eprintln!("failed native_provider_experiment_unavailable");
    std::process::exit(1);
}

#[cfg(all(windows, feature = "test-hooks"))]
mod probe {
    use std::{
        io::{self, BufRead, BufReader, Write},
        os::windows::{io::AsRawHandle, process::CommandExt},
        process::{Child, Command, Stdio},
        sync::mpsc,
        time::Duration,
    };
    use windows::{
        Win32::{
            Foundation::{FILETIME, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
            System::{JobObjects::*, LibraryLoader::GetModuleHandleW, StationsAndDesktops::*, Threading::*},
            UI::WindowsAndMessaging::*,
        },
        core::{PCWSTR, w},
    };
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Kind {
        Foreground,
        ProviderOnly,
    }
    const TITLE: &str = "Faber synthetic native provider";
    const PUBLIC: &str = "PUBLIC_VISIBLE_SENTINEL";
    const PASSWORD: &str = "PASSWORD_SYNTHETIC_SENTINEL";
    const HIDDEN: &str = "HIDDEN_SYNTHETIC_SENTINEL";
    const OFFSCREEN: &str = "OFFSCREEN_SYNTHETIC_SENTINEL";
    type Result<T> = std::result::Result<T, &'static str>;

    struct InputDesktop(HDESK);
    impl Drop for InputDesktop {
        fn drop(&mut self) {
            let _ = unsafe { CloseDesktop(self.0) };
        }
    }
    fn desktop_name(desktop: HDESK) -> Result<String> {
        let mut name = [0u16; 256];
        unsafe {
            GetUserObjectInformationW(
                HANDLE(desktop.0),
                UOI_NAME,
                Some(name.as_mut_ptr().cast()),
                std::mem::size_of_val(&name) as u32,
                None,
            )
        }
        .map_err(|_| "desktop_name_unavailable")?;
        let end = name.iter().position(|v| *v == 0).ok_or("desktop_name_too_long")?;
        String::from_utf16(&name[..end]).map_err(|_| "invalid_desktop_name")
    }
    fn isolated() -> Result<()> {
        let expected = std::env::var("FABER_NATIVE_PROBE_DESKTOP").map_err(|_| "missing_private_desktop")?;
        if !expected.starts_with("faber-test-") || expected.len() > 128 || !expected.bytes().all(|v| v.is_ascii_alphanumeric() || v == b'-')
        {
            return Err("invalid_private_desktop");
        }
        let own = desktop_name(unsafe { GetThreadDesktop(GetCurrentThreadId()) }.map_err(|_| "thread_desktop_unavailable")?)?;
        let input = InputDesktop(
            unsafe { OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS) }
                .map_err(|_| "input_desktop_identity_unavailable")?,
        );
        if own != expected || own.eq_ignore_ascii_case("default") || own == desktop_name(input.0)? {
            return Err("desktop_is_not_isolated");
        }
        let mut in_job = windows::core::BOOL(0);
        unsafe { IsProcessInJob(GetCurrentProcess(), None, &mut in_job) }.map_err(|_| "owned_job_unavailable")?;
        if !in_job.as_bool() {
            return Err("owned_job_required");
        }
        Ok(())
    }
    unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if message == WM_DESTROY {
            unsafe { PostQuitMessage(0) };
            return LRESULT(0);
        }
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }
    fn child(parent: HWND, class: PCWSTR, text: &str, style: WINDOW_STYLE, rect: (i32, i32, i32, i32)) -> Result<()> {
        let (x, y, width, height) = rect;
        let text: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                PCWSTR(text.as_ptr()),
                WS_CHILD | style,
                x,
                y,
                width,
                height,
                Some(parent),
                None,
                None,
                None,
            )
        }
        .map_err(|_| "synthetic_control_creation_failed")?;
        Ok(())
    }
    fn fixture(kind: Kind) -> Result<()> {
        isolated()?;
        let module = unsafe { GetModuleHandleW(None) }.map_err(|_| "module_unavailable")?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: HINSTANCE(module.0),
            lpszClassName: w!("FaberSyntheticNativeProvider"),
            ..Default::default()
        };
        if unsafe { RegisterClassW(&class) } == 0 {
            return Err("synthetic_class_registration_failed");
        }
        // All controls are fixed synthetic data, and desktop identity was checked before any window was created.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("FaberSyntheticNativeProvider"),
                w!("Faber synthetic native provider"),
                WS_OVERLAPPEDWINDOW,
                40,
                40,
                600,
                460,
                None,
                None,
                Some(HINSTANCE(module.0)),
                None,
            )
        }
        .map_err(|_| "synthetic_window_creation_failed")?;
        child(hwnd, w!("STATIC"), PUBLIC, WS_VISIBLE, (12, 12, 500, 28))?;
        child(hwnd, w!("EDIT"), "PUBLIC_EDIT_SENTINEL", WS_VISIBLE, (12, 52, 500, 28))?;
        child(hwnd, w!("EDIT"), PASSWORD, WS_VISIBLE | WINDOW_STYLE(ES_PASSWORD as u32), (12, 92, 500, 28))?;
        child(hwnd, w!("STATIC"), HIDDEN, WINDOW_STYLE(0), (12, 132, 500, 28))?;
        child(hwnd, w!("STATIC"), OFFSCREEN, WS_VISIBLE, (4000, 4000, 100, 28))?;
        child(hwnd, w!("STATIC"), &format!("{PUBLIC} ").repeat(1000), WS_VISIBLE, (12, 172, 500, 100))?;
        isolated()?;
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            if kind == Kind::Foreground {
                let _ = SetForegroundWindow(hwnd);
            }
        }
        println!("ready {}", hwnd.0 as usize);
        io::stdout().flush().map_err(|_| "readiness_pipe_closed")?;
        let mut message = MSG::default();
        loop {
            let state = unsafe { GetMessageW(&mut message, None, 0, 0) }.0;
            if state == -1 {
                return Err("synthetic_message_loop_failed");
            }
            if state == 0 {
                break;
            }
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        Ok(())
    }
    struct HeldFixture(Child);
    impl HeldFixture {
        fn reap(&mut self) -> Result<()> {
            if self.0.try_wait().map_err(|_| "fixture_status_failed")?.is_none() {
                self.0.kill().map_err(|_| "fixture_kill_failed")?;
            }
            self.0.wait().map_err(|_| "fixture_reap_failed")?;
            Ok(())
        }
    }
    impl Drop for HeldFixture {
        fn drop(&mut self) {
            let _ = self.reap();
        }
    }
    fn creation(child: &Child) -> Result<u64> {
        let mut created = FILETIME::default();
        let mut ended = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        unsafe { GetProcessTimes(HANDLE(child.as_raw_handle()), &mut created, &mut ended, &mut kernel, &mut user) }
            .map_err(|_| "fixture_identity_unavailable")?;
        let ticks = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
        Ok(ticks.saturating_sub(116_444_736_000_000_000) / 10)
    }
    fn metrics() -> Result<(u64, usize)> {
        let mut cpu = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        let mut memory = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        // NULL queries this process's immediate owned job, not any unrelated job or process.
        // https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-queryinformationjobobject
        unsafe {
            QueryInformationJobObject(
                None,
                JobObjectBasicAccountingInformation,
                (&mut cpu as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                std::mem::size_of_val(&cpu) as u32,
                None,
            )
            .map_err(|_| "job_cpu_measurement_failed")?;
            QueryInformationJobObject(
                None,
                JobObjectExtendedLimitInformation,
                (&mut memory as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&memory) as u32,
                None,
            )
            .map_err(|_| "job_memory_measurement_failed")?;
        }
        Ok((((cpu.TotalKernelTime.max(0) as u64) + (cpu.TotalUserTime.max(0) as u64)) / 10, memory.PeakJobMemoryUsed))
    }
    fn read(kind: Kind) -> Result<()> {
        isolated()?;
        let binary = std::env::current_exe().map_err(|_| "probe_path_unavailable")?;
        let mut fixture = HeldFixture(
            Command::new(binary)
                .arg(match kind {
                    Kind::Foreground => "--fixture",
                    Kind::ProviderOnly => "--provider-fixture",
                })
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .creation_flags(0x0800_0000)
                .spawn()
                .map_err(|_| "fixture_start_failed")?,
        );
        let pid = fixture.0.id();
        let born = creation(&fixture.0)?;
        let pipe = fixture.0.stdout.take().ok_or("fixture_pipe_unavailable")?;
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let mut ready = String::new();
            let window = if BufReader::new(pipe).read_line(&mut ready).is_ok() {
                ready.strip_prefix("ready ").and_then(|value| value.trim().parse::<usize>().ok()).filter(|value| *value != 0)
            } else {
                None
            };
            let _ = send.send(window);
        });
        let window =
            receive.recv_timeout(Duration::from_secs(3)).map_err(|_| "fixture_readiness_timeout")?.ok_or("fixture_readiness_failed")?;
        eprintln!("fixture_ready {pid} {born}");
        isolated()?;
        if kind == Kind::Foreground {
            // Metadata only: the original mode still refuses any foreground other than the retained fixture.
            let foreground = unsafe { GetForegroundWindow() };
            let mut actual = 0;
            unsafe {
                GetWindowThreadProcessId(foreground, Some(&mut actual));
            }
            if actual != pid {
                return Err("foreground_unavailable_on_private_desktop");
            }
        }
        // The outer Owned job harness supplies the wall deadline. Do not kill the child while a UIA worker can still run.
        let observation = std::thread::spawn(move || {
            isolated()?;
            match kind {
                Kind::Foreground => faber_winproc::desktop::read_foreground_for_test(pid),
                Kind::ProviderOnly => {
                    if faber_winproc::desktop::read_synthetic_window_for_test(window, 0).is_ok() {
                        return Err("mismatched_fixture_pid_was_accepted");
                    }
                    faber_winproc::desktop::read_synthetic_window_for_test(window, pid)
                }
            }
            .map_err(|_| "guarded_observation_failed")
        })
        .join()
        .map_err(|_| "observation_thread_failed")??;
        if observation.title != TITLE
            || !observation.text.contains(PUBLIC)
            || observation.text.contains(PASSWORD)
            || observation.text.contains(HIDDEN)
            || observation.text.contains(OFFSCREEN)
            || observation.title.len() > 256
            || observation.text.len() > 16_000
            || !observation.truncated
        {
            return Err("synthetic_observation_assertion_failed");
        }
        let (cpu, memory) = metrics()?;
        eprintln!("metrics {cpu} {memory}");
        fixture.reap()?;
        Ok(())
    }
    pub fn run() -> Result<()> {
        match std::env::args().skip(1).collect::<Vec<_>>().as_slice() {
            [] => {
                read(Kind::Foreground)?;
                eprintln!("passed synthetic_foreground_text_and_cleanup");
                Ok(())
            }
            [mode] if mode == "--provider-only" => {
                read(Kind::ProviderOnly)?;
                eprintln!("passed synthetic_provider_text_and_cleanup");
                Ok(())
            }
            [mode] if mode == "--fixture" => fixture(Kind::Foreground),
            [mode] if mode == "--provider-fixture" => fixture(Kind::ProviderOnly),
            _ => Err("invalid_fixed_probe_mode"),
        }
    }
}
#[cfg(all(windows, feature = "test-hooks"))]
fn main() {
    if let Err(reason) = probe::run() {
        eprintln!("failed {reason}");
        std::process::exit(1);
    }
}

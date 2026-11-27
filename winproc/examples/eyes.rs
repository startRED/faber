#[cfg(not(windows))]
fn main() {
    eprintln!("eyes requires Windows");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    match eyes::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(windows)]
mod eyes {
    use std::convert::identity;
    use std::ffi::{OsStr, OsString};
    use std::fs::{self, File};
    use std::io::Write;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use windows::Wdk::System::SystemServices::RtlGetVersion;
    use windows::Win32::Foundation::*;
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::System::Com::*;
    use windows::Win32::System::JobObjects::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::StationsAndDesktops::*;
    use windows::Win32::System::SystemInformation::{GetSystemDirectoryW, OSVERSIONINFOW};
    use windows::Win32::System::Threading::*;
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};
    use windows::Win32::UI::{Accessibility::*, WindowsAndMessaging::*};
    use windows::core::{BOOL, BSTR, HRESULT, Interface, PCWSTR, PWSTR, w};

    type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
    const NODE_LIMIT: usize = 400;
    const DEFAULT_DEPTH: usize = 8;
    const POLL: Duration = Duration::from_millis(50);
    const DESKTOP_PREFIX: &str = "FaberEyes-";
    const WEB_TITLE: &str = "Faber Eyes Fixture";
    const HTML: &str = "<title>Faber Eyes Fixture</title><h1>Faber Eyes Fixture</h1><label>Eyes input <input></label><button onclick='this.textContent=\"Eyes clicked\"'>Eyes button</button>";

    fn web_url() -> OsString {
        use std::fmt::Write as _;
        let mut url = String::from("data:text/html,");
        for byte in HTML.bytes() {
            write!(url, "%{byte:02X}").expect("writing to a String cannot fail");
        }
        url.into()
    }

    fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
        text.as_ref().encode_wide().chain([0]).collect()
    }

    struct Runtime;
    impl Runtime {
        fn start() -> Result<Self> {
            unsafe { RoInitialize(RO_INIT_MULTITHREADED)? };
            Ok(Self)
        }
    }
    impl Drop for Runtime {
        fn drop(&mut self) {
            unsafe { RoUninitialize() };
        }
    }

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    struct Snapshot(usize, Result<POINT>);

    fn snapshot() -> Snapshot {
        let mut point = POINT::default();
        unsafe { Snapshot(GetForegroundWindow().0 as usize, GetCursorPos(&mut point).map(|()| point).map_err(Into::into)) }
    }

    fn input_check(Snapshot(foreground, mouse): Snapshot, Snapshot(after_foreground, after_mouse): Snapshot) -> (bool, String) {
        let focus = if foreground == after_foreground { "focus kept" } else { "FOCUS MOVED" };
        let mouse = match (mouse, after_mouse) {
            (Ok(before), Ok(after)) if before == after => "mouse kept".to_string(),
            (Ok(_), Ok(_)) => "MOUSE MOVED".to_string(),
            (Err(error), _) | (_, Err(error)) => format!("mouse check failed: {error}"),
        };
        (focus == "focus kept" && mouse == "mouse kept", format!("{focus}, {mouse}"))
    }

    fn timed<T>(observe: &dyn Fn() -> Snapshot, step: impl FnOnce() -> Result<T>, describe: impl FnOnce(T) -> String) -> (bool, String) {
        let before = observe();
        let start = Instant::now();
        let result = step();
        let elapsed = start.elapsed().as_millis();
        let (input_ok, input) = input_check(before, observe());
        let ok = result.is_ok();
        let detail = result.map_or_else(|error| format!("failed: {error}"), describe);
        let line = format!("{detail}, {elapsed} ms, {input}");
        println!("{line}");
        (ok && input_ok, line)
    }

    unsafe extern "system" fn collect_window(hwnd: HWND, context: LPARAM) -> BOOL {
        let found = unsafe { &mut *(context.0 as *mut Vec<HWND>) };
        found.push(hwnd);
        true.into()
    }

    fn windows() -> Result<Vec<HWND>> {
        let mut found = Vec::new();
        unsafe {
            let desktop = GetThreadDesktop(GetCurrentThreadId())?;
            SetLastError(WIN32_ERROR(0));
            if let Err(error) = EnumDesktopWindows(Some(desktop), Some(collect_window), LPARAM(std::ptr::from_mut(&mut found) as isize))
                && error.code() != HRESULT(0)
            {
                return Err(error.into());
            }
        }
        Ok(found)
    }

    fn title(hwnd: HWND) -> String {
        let mut buffer = [0; 1024];
        let len = unsafe { GetWindowTextW(hwnd, &mut buffer) };
        String::from_utf16_lossy(&buffer[..len as usize])
    }

    fn list() -> Result<()> {
        for w in faber_winproc::eyes::windows()? {
            let minimized = if w.minimized { " minimized" } else { "" };
            println!("{:#x} pid={} class={:?} title={:?} {}x{}{minimized}", w.handle, w.pid, w.class, w.title, w.width, w.height);
        }
        Ok(())
    }

    fn automation() -> Result<IUIAutomation> {
        unsafe {
            let uia: IUIAutomation = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?;
            let settings: IUIAutomation2 = uia.cast()?;
            settings.SetConnectionTimeout(1500)?;
            settings.SetTransactionTimeout(1500)?;
            Ok(uia)
        }
    }

    // UIA returns a null COM interface when a search or walk reaches the end.
    fn optional<T>(result: windows::core::Result<T>) -> Result<Option<T>> {
        match result {
            Ok(value) => Ok(Some(value)),
            // A walker or search with nothing to return fails with E_POINTER, or with S_OK and a null pointer.
            Err(error) if error.code() == E_POINTER || error.code().is_ok() => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    // The reading moved into `faber_winproc::eyes`; the walk keeps the proof's caps (the engine's are `faber_engine_core::eyes::walk`).
    fn tree(hwnd: HWND, depth: usize) -> Result<usize> {
        let handle = hwnd.0 as usize as u64;
        let deadline = Instant::now() + Duration::from_secs(8);
        faber_winproc::eyes::request_accessibility(handle, deadline);
        let uia = faber_winproc::eyes::Automation::open(handle)?;
        let root = uia.root();
        let mut stack = vec![(root, 0)];
        let mut count = 0;
        while let Some((element, level)) = stack.pop() {
            let d = uia.describe(&element, deadline);
            let value = d.value.map(|v| format!(" value={v:?}")).unwrap_or_default();
            println!(
                "{:indent$}{} name={:?} id={:?} enabled={}{value} @{:?}",
                "",
                d.kind,
                d.name,
                d.id,
                d.enabled,
                d.rect,
                indent = level * 2
            );
            count += 1;
            if count == NODE_LIMIT {
                break;
            }
            if level != 0
                && let Some(next) = uia.next_sibling(&element)?
            {
                stack.push((next, level));
            }
            if level < depth
                && let Some(child) = uia.first_child(&element)?
            {
                stack.push((child, level + 1));
            }
        }
        Ok(count)
    }

    enum Action<'a> {
        Invoke(&'a str),
        Set(&'a str, &'a str),
        SetEdit,
    }

    fn act(hwnd: HWND, action: Action<'_>) -> Result<String> {
        let uia = automation()?;
        unsafe {
            let root = uia.ElementFromHandle(hwnd)?;
            let condition = match action {
                Action::Invoke(key) | Action::Set(key, _) => {
                    let key = BSTR::from(key).into();
                    uia.CreateOrCondition(
                        &uia.CreatePropertyCondition(UIA_NamePropertyId, &key)?,
                        &uia.CreatePropertyCondition(UIA_AutomationIdPropertyId, &key)?,
                    )?
                }
                Action::SetEdit => {
                    let kind = uia.CreateOrCondition(
                        &uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &UIA_EditControlTypeId.0.into())?,
                        &uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &UIA_DocumentControlTypeId.0.into())?,
                    )?;
                    let writable = uia.CreatePropertyCondition(UIA_ValueIsReadOnlyPropertyId, &false.into())?;
                    let supports_value = uia.CreatePropertyCondition(UIA_IsValuePatternAvailablePropertyId, &true.into())?;
                    uia.CreateAndCondition(&kind, &uia.CreateAndCondition(&writable, &supports_value)?)?
                }
            };
            let element = optional(root.FindFirst(TreeScope_Subtree, &condition))?.ok_or("no matching UI Automation element")?;
            if !element.CurrentIsEnabled()?.as_bool() {
                return Err("matching element is disabled".into());
            }
            let name = element.CurrentName()?;
            let id = element.CurrentAutomationId()?;
            let how = match action {
                Action::Invoke(_) => {
                    element.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)?.Invoke()?;
                    "InvokePattern"
                }
                Action::Set(_, text) => set_value(&element, text)?,
                Action::SetEdit => set_value(&element, "Faber Eyes proof")?,
            };
            Ok(format!("ok by {how}: name={name:?} id={id:?}"))
        }
    }

    fn set_value(element: &IUIAutomationElement, text: &str) -> Result<&'static str> {
        unsafe {
            let pattern = element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)?;
            let text = BSTR::from(text);
            let set = pattern.SetValue(&text);
            for _ in 0..5 {
                if set.is_ok() && pattern.CurrentValue()? == text {
                    return Ok("ValuePattern");
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            // A classic edit control takes its text by window message even when UI Automation's write does not land.
            let native = element.CurrentNativeWindowHandle()?;
            if native.is_invalid() {
                return Err(format!("ValuePattern did not take the text ({set:?}) and the element has no window of its own").into());
            }
            let wide: Vec<u16> = text.to_string().encode_utf16().chain(Some(0)).collect();
            SendMessageW(native, WM_SETTEXT, None, Some(LPARAM(wide.as_ptr() as isize)));
            let mut back = [0u16; 256];
            let len = SendMessageW(native, WM_GETTEXT, Some(WPARAM(back.len())), Some(LPARAM(back.as_mut_ptr() as isize))).0 as usize;
            if String::from_utf16_lossy(&back[..len.min(back.len())]) == text {
                return Ok("WM_SETTEXT (ValuePattern did not take it)");
            }
            Err("neither ValuePattern nor WM_SETTEXT changed the text".into())
        }
    }

    struct Desktop(HDESK);
    impl Drop for Desktop {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseDesktop(self.0);
            }
        }
    }

    // Quote for the Windows C runtime, including paths with trailing backslashes.
    fn quote(word: &OsStr) -> Vec<u16> {
        let mut out = vec![b'"' as u16];
        let mut slashes = 0;
        for ch in word.encode_wide() {
            match ch {
                92 => slashes += 1,
                34 => {
                    out.extend(std::iter::repeat_n(92, slashes * 2 + 1));
                    out.push(ch);
                    slashes = 0;
                }
                _ => {
                    out.extend(std::iter::repeat_n(92, slashes));
                    out.push(ch);
                    slashes = 0;
                }
            }
        }
        out.extend(std::iter::repeat_n(92, slashes * 2));
        out.push(b'"' as u16);
        out
    }

    fn launch(desktop: &[u16], exe: &Path, args: &[OsString]) -> Result<Handle> {
        let exe_wide = wide(exe);
        let mut command = quote(exe.as_os_str());
        for arg in args {
            command.push(b' ' as u16);
            command.extend(quote(arg));
        }
        command.push(0);
        unsafe {
            let job = Handle(CreateJobObjectW(None, PCWSTR::null())?);
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&limits).cast(),
                size_of_val(&limits) as u32,
            )?;
            let mut desktop = desktop.to_vec();
            let startup =
                STARTUPINFOW { cb: size_of::<STARTUPINFOW>() as u32, lpDesktop: PWSTR(desktop.as_mut_ptr()), ..Default::default() };
            let mut info = PROCESS_INFORMATION::default();
            CreateProcessW(
                PCWSTR(exe_wide.as_ptr()),
                Some(PWSTR(command.as_mut_ptr())),
                None,
                None,
                false,
                CREATE_SUSPENDED | CREATE_NO_WINDOW,
                None,
                PCWSTR::null(),
                &startup,
                &mut info,
            )?;
            let process = Handle(info.hProcess);
            let thread = Handle(info.hThread);
            if let Err(error) = AssignProcessToJobObject(job.0, process.0) {
                let _ = TerminateProcess(process.0, 1);
                return Err(error.into());
            }
            if ResumeThread(thread.0) == u32::MAX {
                return Err(windows::core::Error::from_thread().into());
            }
            Ok(job)
        }
    }

    fn in_job(hwnd: HWND, job: &Handle) -> bool {
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                return false;
            };
            let process = Handle(process);
            let mut member = BOOL::default();
            IsProcessInJob(process.0, Some(job.0), &mut member).is_ok() && member.as_bool()
        }
    }

    fn wait_window(job: &Handle, required_title: &str) -> Result<HWND> {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            for hwnd in windows()? {
                if unsafe { IsWindowVisible(hwnd).as_bool() }
                    && in_job(hwnd, job)
                    && !title(hwnd).is_empty()
                    && title(hwnd).contains(required_title)
                {
                    return Ok(hwnd);
                }
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("no matching window owned by the job on the hidden desktop within 15 s".into());
            }
            thread::sleep(POLL.min(remaining));
        }
    }

    fn system_dir() -> Result<PathBuf> {
        use std::os::windows::ffi::OsStringExt;
        let mut path = [0; 260];
        let len = unsafe { GetSystemDirectoryW(Some(&mut path)) } as usize;
        if len == 0 || len >= path.len() {
            return Err("GetSystemDirectoryW failed".into());
        }
        Ok(OsString::from_wide(&path[..len]).into())
    }

    fn classic_exe() -> &'static str {
        let mut version = OSVERSIONINFOW { dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32, ..Default::default() };
        // Windows 11's Notepad launcher can activate a packaged app outside this job.
        if unsafe { RtlGetVersion(&mut version).is_ok() } && version.dwBuildNumber < 22000 { "notepad.exe" } else { "charmap.exe" }
    }

    fn edge_exe() -> Result<PathBuf> {
        ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"]
            .into_iter()
            .filter_map(std::env::var_os)
            .flat_map(|base| {
                [r"Microsoft\Edge\Application\msedge.exe", r"Google\Chrome\Application\chrome.exe"]
                    .map(|exe| PathBuf::from(&base).join(exe))
            })
            .find(|path| path.is_file())
            .ok_or_else(|| "neither msedge.exe nor chrome.exe was found in the standard locations".into())
    }

    fn draw_fixture() -> Result<()> {
        unsafe {
            let desktop = GetThreadDesktop(GetCurrentThreadId())?;
            let mut name = [0u16; 256];
            GetUserObjectInformationW(HANDLE(desktop.0), UOI_NAME, Some(name.as_mut_ptr().cast()), size_of_val(&name) as u32, None)?;
            if !String::from_utf16_lossy(&name).starts_with(DESKTOP_PREFIX) {
                return Err("the custom-drawn fixture requires the suite desktop".into());
            }

            let module = HINSTANCE(GetModuleHandleW(None)?.0);
            let class = WNDCLASSW {
                lpfnWndProc: Some(fixture_proc),
                hInstance: module,
                lpszClassName: w!("FaberEyesFixture"),
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 {
                return Err(windows::core::Error::from_thread().into());
            }
            CreateWindowExW(
                WS_EX_NOACTIVATE,
                class.lpszClassName,
                w!("Faber Eyes GDI"),
                WS_POPUP | WS_VISIBLE,
                0,
                0,
                420,
                220,
                None,
                None,
                Some(module),
                None,
            )?;
            let mut message = MSG::default();
            loop {
                match GetMessageW(&mut message, None, 0, 0).0 {
                    -1 => return Err(windows::core::Error::from_thread().into()),
                    0 => return Ok(()),
                    _ => {
                        DispatchMessageW(&message);
                    }
                }
            }
        }
    }

    unsafe fn paint(hwnd: HWND, dc: HDC) {
        let mut rect = RECT::default();
        unsafe {
            let _ = GetClientRect(hwnd, &mut rect);
            FillRect(dc, &rect, HBRUSH(GetStockObject(WHITE_BRUSH).0));
            let _ = Rectangle(dc, 16, 16, rect.right - 16, rect.bottom - 16);
            let label: Vec<_> = "GDI fixture: no UI Automation controls".encode_utf16().collect();
            let _ = TextOutW(dc, 28, 32, &label);
        }
    }

    unsafe extern "system" fn fixture_proc(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        unsafe {
            match message {
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    let dc = BeginPaint(hwnd, &mut ps);
                    paint(hwnd, dc);
                    let _ = EndPaint(hwnd, &ps);
                }
                WM_PRINT | WM_PRINTCLIENT => paint(hwnd, HDC(wp.0 as *mut _)),
                WM_DESTROY => PostQuitMessage(0),
                _ => return DefWindowProcW(hwnd, message, wp, lp),
            }
        }
        LRESULT(0)
    }

    struct Case<'a> {
        name: &'a str,
        file: &'a str,
        program: Result<PathBuf>,
        args: Vec<OsString>,
        title: &'a str,
        action: Action<'a>,
    }

    fn record(report: &mut File, fields: &[&str]) -> Result<()> {
        for field in fields {
            write!(report, "| {} ", field.replace('|', r"\|").replace(['\r', '\n'], " "))?;
        }
        writeln!(report, "|")?;
        Ok(())
    }

    fn measure(case: Case<'_>, desktop: &[u16], report: &mut File, observe: &dyn Fn() -> Snapshot) -> Result<()> {
        println!("\n{}", case.name);
        let before = observe();
        let started = Instant::now();
        let ready = (|| {
            let job = launch(desktop, &case.program?, &case.args)?;
            let hwnd = wait_window(&job, case.title)?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>((job, hwnd))
        })();
        let elapsed = started.elapsed().as_millis();
        let (_, input) = input_check(before, observe());
        let (job, hwnd) = match ready {
            Ok(ready) => ready,
            Err(error) => {
                let note = if case.file == "calculator" { " (packaged activation may refuse or leave the job)" } else { "" };
                let failure = format!("failed: {error}{note}, {elapsed} ms, {input}");
                println!("{failure}");
                return record(report, &[case.name, &failure, "skipped", "skipped"]);
            }
        };
        let target =
            || -> Result<HWND> { if in_job(hwnd, &job) { Ok(hwnd) } else { Err("fixture window is no longer owned by the job".into()) } };
        let window = format!("{:#x} {:?}, {elapsed} ms, {input}", hwnd.0 as usize, title(hwnd));
        println!("{window}");
        let (_, tree) = timed(observe, || tree(target()?, DEFAULT_DEPTH), tree_summary);
        let (_, action) = timed(observe, || act(target()?, case.action), identity);
        record(report, &[case.name, &window, &tree, &action])
    }

    fn measure_suite(out: &Path, electron: PathBuf, desktop: &[u16], observe: &dyn Fn() -> Snapshot) -> Result<()> {
        let _runtime = Runtime::start()?;
        let temp = tempfile::tempdir()?;
        let app = temp.path().join("electron-app");
        fs::create_dir_all(app.join("profile"))?;
        fs::write(app.join("index.html"), HTML)?;
        fs::write(app.join("package.json"), r#"{"name":"faber-eyes-fixture","version":"1.0.0","main":"main.js"}"#)?;
        fs::write(
            app.join("main.js"),
            r#"
const { app, BrowserWindow } = require('electron');
const path = require('node:path');
app.setPath('userData', path.join(__dirname, 'profile'));
app.commandLine.appendSwitch('force-renderer-accessibility');
app.whenReady().then(() => {
    const window = new BrowserWindow({ width: 640, height: 420 });
    window.loadFile(path.join(__dirname, 'index.html'));
});
app.on('window-all-closed', () => app.quit());
"#,
        )?;
        let mut profile = OsString::from("--user-data-dir=");
        profile.push(temp.path().join("edge-profile"));
        let classic = classic_exe();
        let classic_name = format!("Classic Win32 ({classic})");
        let system = system_dir()?;
        let cases = [
            Case {
                name: &classic_name,
                file: "win32",
                program: Ok(system.join(classic)),
                args: vec![],
                title: "",
                action: Action::SetEdit,
            },
            Case {
                name: "Modern Windows (calc.exe)",
                file: "calculator",
                // A packaged app starts through the activation broker on the person's own desktop, whatever
                // `lpDesktop` says, so it would show on their screen: it runs only when asked for.
                program: if std::env::var_os("FABER_EYES_PACKAGED").is_some() {
                    Ok(system.join("calc.exe"))
                } else {
                    Err("skipped: a packaged app opens on the person's desktop, not the hidden one (FABER_EYES_PACKAGED=1 runs it)".into())
                },
                args: vec![],
                title: "",
                action: Action::Invoke("num1Button"),
            },
            Case {
                name: "Electron",
                file: "electron",
                program: electron.canonicalize().map_err(Into::into),
                args: vec![app.into_os_string()],
                title: WEB_TITLE,
                action: Action::Invoke("Eyes button"),
            },
            Case {
                name: "Browser (Edge, or Chrome without it)",
                file: "browser",
                program: edge_exe(),
                args: vec![
                    profile,
                    "--no-first-run".into(),
                    "--no-default-browser-check".into(),
                    "--disable-background-networking".into(),
                    "--force-renderer-accessibility".into(),
                    web_url(),
                ],
                title: WEB_TITLE,
                action: Action::Set("Eyes input", "Faber Eyes proof"),
            },
            Case {
                name: "Custom-drawn (GDI)",
                file: "custom",
                program: std::env::current_exe().map_err(Into::into),
                args: vec!["__draw".into()],
                title: "Faber Eyes GDI",
                action: Action::SetEdit,
            },
        ];
        let mut report = File::create(out.join("results.md"))?;
        writeln!(
            report,
            "# Eyes proof of concept\n\n| Program | Window and wait | UI Automation tree | Background action |\n| --- | --- | --- | --- |"
        )?;
        for case in cases {
            measure(case, desktop, &mut report, observe)?;
        }
        Ok(())
    }

    fn suite(out: PathBuf, electron: PathBuf) -> Result<()> {
        fs::create_dir_all(&out)?;
        let time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let name = wide(format!("{DESKTOP_PREFIX}{}-{time}", std::process::id()));
        let original = unsafe { GetThreadDesktop(GetCurrentThreadId())? };
        let desktop =
            Desktop(unsafe { CreateDesktopW(PCWSTR(name.as_ptr()), PCWSTR::null(), None, DESKTOP_CONTROL_FLAGS(0), GENERIC_ALL.0, None)? });
        unsafe { SetThreadDesktop(original)? };
        let desktop_raw = desktop.0.0 as usize;
        thread::scope(|scope| {
            // Sample the original desktop from this thread even when GetCursorPos is denied on the worker's desktop.
            let (request, requests) = mpsc::sync_channel(0);
            let (reply, replies) = mpsc::sync_channel(0);
            let worker = scope.spawn(move || {
                unsafe { SetThreadDesktop(HDESK(desktop_raw as *mut _))? };
                measure_suite(&out, electron, &name, &|| {
                    request.send(()).expect("desktop observer is running");
                    replies.recv().expect("desktop observer replied")
                })
            });
            while requests.recv().is_ok() {
                let _ = reply.send(snapshot());
            }
            worker.join().map_err(|_| "hidden desktop worker panicked")?
        })
    }

    fn hwnd(value: &OsStr) -> Result<HWND> {
        let value = value.to_str().ok_or("window handle must be text")?;
        let raw = if let Some(hex) = value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")) {
            usize::from_str_radix(hex, 16)?
        } else {
            value.parse::<usize>()?
        };
        let hwnd = HWND(raw as *mut _);
        if !unsafe { IsWindow(Some(hwnd)).as_bool() } {
            return Err(format!("invalid window handle {value}").into());
        }
        Ok(hwnd)
    }

    fn tree_summary(count: usize) -> String {
        let limit = if count == NODE_LIMIT { " (node cap)" } else { "" };
        format!("{count} nodes{limit}")
    }

    fn text(value: &OsStr) -> Result<&str> {
        value.to_str().ok_or_else(|| "name, id, and text arguments must be Unicode".into())
    }

    pub fn run() -> Result<()> {
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        let args: Vec<_> = args.iter().map(OsString::as_os_str).collect();
        let (command, rest) = args.split_first().ok_or(USAGE)?;
        match (command.to_str(), rest) {
            (Some("suite"), [out, flag, electron]) if *flag == "--electron" => return suite(out.into(), electron.into()),
            (Some("__draw"), []) => return draw_fixture(),
            _ => {}
        }
        let _runtime = Runtime::start()?;
        let (ok, _) = match (command.to_str(), rest) {
            (Some("list"), []) => timed(&snapshot, list, |()| "ok".into()),
            (Some("tree"), [target]) => timed(&snapshot, || tree(hwnd(target)?, DEFAULT_DEPTH), tree_summary),
            (Some("tree"), [target, flag, depth]) if *flag == "--depth" => {
                timed(&snapshot, || tree(hwnd(target)?, text(depth)?.parse()?), tree_summary)
            }
            (Some("invoke"), [target, key]) => timed(&snapshot, || act(hwnd(target)?, Action::Invoke(text(key)?)), identity),
            (Some("set"), [target, key, value]) => timed(&snapshot, || act(hwnd(target)?, Action::Set(text(key)?, text(value)?)), identity),
            _ => return Err(USAGE.into()),
        };
        if !ok {
            return Err("eyes command failed; see the result above".into());
        }
        Ok(())
    }

    const USAGE: &str = "Usage: eyes list | tree <hwnd> [--depth N] | invoke <hwnd> <name-or-id> | set <hwnd> <name-or-id> <text> | suite <out-dir> --electron <exe>";
}

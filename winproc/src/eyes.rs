//! Eyes (task 304): read the person's other windows as text without touching them. Lists top-level windows, names the
//! process that draws one (a UWP frame's hosted app), and walks a window's UI Automation tree one element at a time (the
//! caps live in `faber_engine_core::eyes::walk`, which tests them without Windows). Nothing here activates, restores,
//! shows or moves a window, takes the focus or moves the pointer (C006); there is no input call and no picture. Measured
//! in `winproc/examples/eyes.rs` (`docs/dev/eyes-proof.md`, `docs/design/eyes.md`).

use std::io;
use std::marker::PhantomData;
use std::time::Instant;

use windows::Win32::Foundation::{CloseHandle, E_POINTER, FILETIME, HANDLE, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, IUnknown, Interface, PWSTR};

/// UI Automation's own limits for one call into another program.
const UIA_TIMEOUT_MS: u32 = 1000;

fn os(error: windows::core::Error) -> io::Error {
    io::Error::other(error)
}

fn hwnd(handle: u64) -> HWND {
    HWND(handle as usize as *mut _)
}

/// One top-level window as the list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub handle: u64,
    /// The process that made the window (for a UWP app, ApplicationFrameHost; see [`drawn_by`]).
    pub pid: u32,
    /// The thread that made it: a later window under a reused handle, even of the same process, can tell by it.
    pub thread: u32,
    pub title: String,
    pub class: String,
    pub width: i32,
    pub height: i32,
    pub minimized: bool,
    /// Hidden by the system's compositor: on another virtual desktop, or a suspended app.
    pub cloaked: bool,
}

unsafe extern "system" fn collect(window: HWND, context: LPARAM) -> BOOL {
    // SAFETY: `context` is the address of the Vec the caller passes and keeps alive for the whole enumeration.
    let found = unsafe { &mut *(context.0 as *mut Vec<HWND>) };
    found.push(window);
    true.into()
}

fn children(top: HWND) -> Vec<HWND> {
    let mut found: Vec<HWND> = Vec::new();
    // SAFETY: the callback only pushes into `found`, which outlives the call. An error means no children.
    let _ = unsafe { EnumChildWindows(Some(top), Some(collect), LPARAM(std::ptr::from_mut(&mut found) as isize)) };
    found
}

fn text(window: HWND) -> String {
    let mut buffer = [0u16; 512];
    // SAFETY: the buffer is ours and its length is passed; an invalid handle returns 0.
    let len = unsafe { GetWindowTextW(window, &mut buffer) }.max(0) as usize;
    String::from_utf16_lossy(&buffer[..len.min(buffer.len())])
}

fn class(window: HWND) -> String {
    let mut buffer = [0u16; 256];
    // SAFETY: as in `text`.
    let len = unsafe { GetClassNameW(window, &mut buffer) }.max(0) as usize;
    String::from_utf16_lossy(&buffer[..len.min(buffer.len())])
}

/// The process and the thread that made a window, 0 and 0 for a handle that is gone.
fn made_by(window: HWND) -> (u32, u32) {
    let mut pid = 0;
    // SAFETY: `pid` is ours; an invalid handle leaves it 0 and returns 0.
    let thread = unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
    (pid, thread)
}

fn pid_of(window: HWND) -> u32 {
    made_by(window).0
}

fn cloaked(window: HWND) -> bool {
    let mut value = 0u32;
    // SAFETY: the attribute is a DWORD written into `value`, whose size is passed.
    let read = unsafe { DwmGetWindowAttribute(window, DWMWA_CLOAKED, std::ptr::from_mut(&mut value).cast(), size_of::<u32>() as u32) };
    read.is_ok() && value != 0
}

/// The visible, titled, unowned top-level windows of this thread's desktop, front to back. Reading a title sends no
/// message to another program (`GetWindowTextW` reads the cached caption of other processes' windows).
pub fn windows() -> io::Result<Vec<Window>> {
    let mut found: Vec<HWND> = Vec::new();
    // SAFETY: the callback only pushes into `found`, which outlives the call.
    unsafe { EnumWindows(Some(collect), LPARAM(std::ptr::from_mut(&mut found) as isize)) }.map_err(os)?;
    let mut list = Vec::new();
    for window in found {
        // SAFETY: plain queries on a handle; a window that closed meanwhile answers false or zero.
        let (visible, owner, minimized) =
            unsafe { (IsWindowVisible(window).as_bool(), GetWindow(window, GW_OWNER).unwrap_or_default(), IsIconic(window).as_bool()) };
        let title = text(window);
        if !visible || !owner.is_invalid() || title.trim().is_empty() {
            continue;
        }
        let mut rect = RECT::default();
        // SAFETY: `rect` is ours.
        let _ = unsafe { GetWindowRect(window, &mut rect) };
        let (pid, thread) = made_by(window);
        list.push(Window {
            handle: window.0 as usize as u64,
            pid,
            thread,
            title,
            class: class(window),
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
            minimized,
            cloaked: cloaked(window),
        });
    }
    Ok(list)
}

/// The window with this handle, when it still exists and is one of [`windows`]; `None` otherwise.
pub fn window(handle: u64) -> Option<Window> {
    // SAFETY: `IsWindow` accepts any value.
    if !unsafe { IsWindow(Some(hwnd(handle))).as_bool() } {
        return None;
    }
    windows().ok()?.into_iter().find(|w| w.handle == handle)
}

/// The process that draws a window. A UWP app's top-level window belongs to ApplicationFrameHost, which hosts the app's
/// own CoreWindow as a child of another process: that process is the one named ([`hosted`]). `None` when a frame's app
/// cannot be told for sure, which the caller treats as out of reach.
pub fn drawn_by(handle: u64) -> Option<u32> {
    let top = hwnd(handle);
    let pid = pid_of(top);
    if pid == 0 {
        return None;
    }
    let frame = path(pid).is_some_and(|p| p.rsplit(['\\', '/']).next().is_some_and(|n| n.eq_ignore_ascii_case("ApplicationFrameHost.exe")));
    if !frame {
        return Some(pid);
    }
    hosted(pid, &children(top).into_iter().map(|c| (pid_of(c), class(c))).collect::<Vec<_>>())
}

/// The app a UWP frame of process `host` shows, from its child windows (process, class): the one other process that
/// draws a `Windows.UI.Core.CoreWindow` in it, and no other foreign child beside it. Any doubt (no such child, two
/// apps, or a window of yet another process inside the frame) is `None`, so a foreign child is never taken for the app
/// and a blocked app never hides behind one.
pub fn hosted(host: u32, children: &[(u32, String)]) -> Option<u32> {
    let mut foreign = children.iter().filter(|(pid, _)| *pid != 0 && *pid != host);
    let app = foreign.clone().find(|(_, class)| class == "Windows.UI.Core.CoreWindow")?.0;
    foreign.all(|(pid, _)| *pid == app).then_some(app)
}

fn open(pid: u32) -> Option<Process> {
    // SAFETY: the handle is closed by `Process`'s drop.
    unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok().map(Process)
}

struct Process(HANDLE);
impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: the handle came from `OpenProcess` and is closed once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// The full path of a process's executable, or `None` when it cannot be read (protected, or gone).
pub fn path(pid: u32) -> Option<String> {
    let process = open(pid)?;
    let mut buffer = [0u16; 1024];
    let mut len = buffer.len() as u32;
    // SAFETY: the buffer is ours and its size is passed in and out.
    unsafe { QueryFullProcessImageNameW(process.0, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut len) }.ok()?;
    Some(String::from_utf16_lossy(&buffer[..(len as usize).min(buffer.len())])).filter(|p| !p.is_empty())
}

/// When a process started (100 ns units since 1601), which tells a process from a later one that reuses its id.
pub fn started(pid: u32) -> Option<u64> {
    let process = open(pid)?;
    let (mut created, mut exited, mut kernel, mut user) =
        (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    // SAFETY: four FILETIMEs of ours are written.
    unsafe { GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user) }.ok()?;
    Some((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

/// Whether a process runs elevated, or `None` when its token cannot be read (often an elevated process seen from a plain one).
pub fn elevated(pid: u32) -> Option<bool> {
    let process = open(pid)?;
    let mut token = HANDLE::default();
    // SAFETY: the token handle is closed by `Process`'s drop below.
    unsafe { OpenProcessToken(process.0, TOKEN_QUERY, &mut token) }.ok()?;
    let token = Process(token);
    let mut elevation = TOKEN_ELEVATION::default();
    let mut len = 0u32;
    // SAFETY: a TOKEN_ELEVATION of ours is written, its size passed.
    unsafe {
        GetTokenInformation(
            token.0,
            TokenElevation,
            Some(std::ptr::from_mut(&mut elevation).cast()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
    }
    .ok()?;
    Some(elevation.TokenIsElevated != 0)
}

/// Whether a window's program has stopped answering its messages; such a window is not read (a read would wait on it).
pub fn hung(handle: u64) -> bool {
    // SAFETY: a plain query on a handle.
    unsafe { IsHungAppWindow(hwnd(handle)).as_bool() }
}

/// COM for this thread, in the multithreaded apartment, for as long as it lives. Not Send: it belongs to its thread.
struct Apartment(bool, PhantomData<*const ()>);
impl Apartment {
    fn enter() -> Self {
        // SAFETY: balanced by `CoUninitialize` in drop only when this call succeeded (S_OK or S_FALSE).
        Apartment(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok(), PhantomData)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: matches the successful `CoInitializeEx` of `enter`, on the same thread (the type is not Send).
            unsafe { CoUninitialize() };
        }
    }
}

/// Chromium (browsers and Electron apps) builds its page's accessibility tree only once a client asks for it. Asking is
/// what a screen reader does: it gets each page view's accessible object, here released at once. A hung view is skipped
/// and nothing is asked after `deadline`; nothing is shown or focused. Returns whether the window was one to ask. A view
/// that stops answering after the hung check holds this call (the request has no timeout of its own), so a caller that
/// must not wait runs it on a thread it can leave (`faber_engine_core::eyes::within`).
pub fn request_accessibility(handle: u64, deadline: Instant) -> bool {
    let top = hwnd(handle);
    if !class(top).starts_with("Chrome_WidgetWin") {
        return false;
    }
    let _apartment = Apartment::enter();
    for view in children(top).into_iter().filter(|v| class(*v) == "Chrome_RenderWidgetHostHWND").chain([top]) {
        // SAFETY: a plain query on a handle.
        if Instant::now() >= deadline || unsafe { IsHungAppWindow(view).as_bool() } {
            continue;
        }
        let mut object: *mut std::ffi::c_void = std::ptr::null_mut();
        // SAFETY: on success `object` holds one reference to an IUnknown, taken over by `from_raw` and released when it drops.
        unsafe {
            if AccessibleObjectFromWindow(view, OBJID_CLIENT.0 as u32, &IUnknown::IID, &mut object).is_ok() && !object.is_null() {
                drop(IUnknown::from_raw(object));
            }
        }
    }
    true
}

/// What one element of the tree says about itself.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Described {
    /// The control type in the system's language ("button", "botão").
    pub kind: String,
    pub name: String,
    pub id: String,
    pub enabled: bool,
    /// The text of an edit, document or combo box (its value), when it has one.
    pub value: Option<String>,
    /// Left, top, width and height, relative to the window's top-left corner.
    pub rect: (i32, i32, i32, i32),
}

/// One element of a window's UI Automation tree. It cannot outlive the client that made it, so COM stays initialized for
/// as long as any element is held.
pub struct Element<'a>(IUIAutomationElement, PhantomData<&'a Automation>);

/// A UI Automation client for one walk, with short timeouts so a hung program cannot hold the reader long.
pub struct Automation {
    // Kept so the client lives as long as the walker it made.
    _uia: IUIAutomation,
    walker: IUIAutomationTreeWalker,
    root: IUIAutomationElement,
    origin: (i32, i32),
    // Last: COM is uninitialized only after the interfaces above are released.
    _apartment: Apartment,
}

// UI Automation answers a null element at the end of a walk: as E_POINTER, or as success with a null pointer.
fn optional<T>(result: windows::core::Result<T>) -> io::Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.code() == E_POINTER || error.code().is_ok() => Ok(None),
        Err(error) => Err(os(error)),
    }
}

impl Automation {
    /// A client for `handle`'s tree. Reads only; it registers no event handler.
    pub fn open(handle: u64) -> io::Result<Self> {
        let apartment = Apartment::enter();
        let target = hwnd(handle);
        let mut rect = RECT::default();
        // SAFETY: COM is initialized on this thread by `apartment`; every call below is a read through owned interfaces.
        unsafe {
            let _ = GetWindowRect(target, &mut rect);
            let uia: IUIAutomation = CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).map_err(os)?;
            if let Ok(settings) = uia.cast::<IUIAutomation2>() {
                let _ = settings.SetConnectionTimeout(UIA_TIMEOUT_MS);
                let _ = settings.SetTransactionTimeout(UIA_TIMEOUT_MS);
            }
            let walker = uia.RawViewWalker().map_err(os)?;
            let root = uia.ElementFromHandle(target).map_err(os)?;
            Ok(Automation { _uia: uia, walker, root, origin: (rect.left, rect.top), _apartment: apartment })
        }
    }

    /// The window's own element, where a walk starts.
    pub fn root(&self) -> Element<'_> {
        Element(self.root.clone(), PhantomData)
    }

    pub fn first_child<'a>(&'a self, element: &Element<'a>) -> io::Result<Option<Element<'a>>> {
        // SAFETY: a read through owned interfaces on the thread that made them.
        optional(unsafe { self.walker.GetFirstChildElement(&element.0) }).map(|e| e.map(|e| Element(e, PhantomData)))
    }

    pub fn next_sibling<'a>(&'a self, element: &Element<'a>) -> io::Result<Option<Element<'a>>> {
        // SAFETY: as in `first_child`.
        optional(unsafe { self.walker.GetNextSiblingElement(&element.0) }).map(|e| e.map(|e| Element(e, PhantomData)))
    }

    /// The process that provides an element (0 when it cannot be read).
    pub fn process(&self, element: &Element<'_>) -> u32 {
        // SAFETY: as in `first_child`.
        unsafe { element.0.CurrentProcessId() }.map(|p| p as u32).unwrap_or(0)
    }

    /// The element's type, name, id, state, value and place. A property that fails reads as empty, and none is read once
    /// `deadline` has passed.
    pub fn describe(&self, element: &Element<'_>, deadline: Instant) -> Described {
        let e = &element.0;
        let open = || Instant::now() < deadline;
        let mut d = Described::default();
        // SAFETY: reads through owned interfaces on the thread that made them; failures become defaults.
        unsafe {
            if open() {
                d.kind = e.CurrentLocalizedControlType().map(|s| s.to_string()).unwrap_or_default();
            }
            if open() {
                d.name = e.CurrentName().map(|s| s.to_string()).unwrap_or_default();
            }
            if open() {
                d.id = e.CurrentAutomationId().map(|s| s.to_string()).unwrap_or_default();
            }
            if open() {
                d.enabled = e.CurrentIsEnabled().map(|b| b.as_bool()).unwrap_or(false);
            }
            if open()
                && [UIA_EditControlTypeId, UIA_DocumentControlTypeId, UIA_ComboBoxControlTypeId]
                    .contains(&e.CurrentControlType().unwrap_or_default())
                && open()
            {
                d.value = e
                    .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                    .and_then(|p| p.CurrentValue())
                    .ok()
                    .map(|s| s.to_string());
            }
            if open() {
                let r = e.CurrentBoundingRectangle().unwrap_or_default();
                d.rect = (r.left - self.origin.0, r.top - self.origin.1, r.right - r.left, r.bottom - r.top);
            }
        }
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_uwp_frame_names_its_one_hosted_app_or_nobody() {
        let child = |pid: u32, class: &str| (pid, class.to_string());
        let core = "Windows.UI.Core.CoreWindow";
        assert_eq!(hosted(5, &[child(5, "ApplicationFrameInputSinkWindow"), child(9, core)]), Some(9));
        assert_eq!(hosted(5, &[child(9, core), child(9, "Windows.UI.Input.InputSite.WindowClass")]), Some(9), "the app's own windows");
        assert_eq!(hosted(5, &[child(5, "ApplicationFrameTitleBarWindow")]), None, "a suspended app: no app to name");
        assert_eq!(hosted(5, &[child(7, "IME"), child(9, core)]), None, "a foreign child beside the app");
        assert_eq!(hosted(5, &[child(9, core), child(7, "IME")]), None, "whatever the order");
        assert_eq!(hosted(5, &[child(7, "Other")]), None, "a foreign child that is no app is never taken for one");
        assert_eq!(hosted(5, &[child(9, core), child(8, core)]), None, "two apps");
        assert_eq!(hosted(5, &[child(0, core), child(9, core)]), Some(9), "a child already gone is skipped");
    }
}

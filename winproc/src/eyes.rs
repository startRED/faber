//! Eyes (task 304): read the person's other windows without touching them. Lists top-level windows, names the process that
//! draws one (a UWP frame's hosted app), walks a window's UI Automation tree one element at a time (the caps live in
//! `faber_engine_core::eyes::walk`, which tests them without Windows), and takes a picture: Windows.Graphics.Capture first,
//! PrintWindow with full content as the fallback. Nothing here activates, restores, shows or moves a window, takes the
//! focus or moves the pointer (C006); there is no input call. Measured in `winproc/examples/eyes.rs`
//! (`docs/dev/eyes-proof.md`, `docs/design/eyes.md`).

use std::io;
use std::marker::PhantomData;
use std::time::{Duration, Instant};

use windows::Graphics::Capture::{Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession};
use windows::Graphics::DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat};
use windows::Win32::Foundation::{CloseHandle, E_POINTER, FILETIME, HANDLE, HMODULE, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::System::WinRT::Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, IUnknown, Interface, PWSTR};

/// Pictures larger than this on either side are refused before any buffer is made (4096x4096 RGB is 48 MB).
pub const MAX_SIDE: i32 = 4096;
/// The longest a picture waits for Windows.Graphics.Capture's first frame before PrintWindow is tried.
const FRAME_WAIT: Duration = Duration::from_millis(800);
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

fn pid_of(window: HWND) -> u32 {
    let mut pid = 0;
    // SAFETY: `pid` is ours; an invalid handle leaves it 0.
    unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
    pid
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
        list.push(Window {
            handle: window.0 as usize as u64,
            pid: pid_of(window),
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
/// own CoreWindow as a child of another process: that process is the one named. `None` when a frame's app cannot be
/// found (a suspended app), which the caller treats as out of reach.
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
    children(top).into_iter().map(pid_of).find(|child| *child != 0 && *child != pid)
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
/// and nothing is asked after `deadline`; nothing is shown or focused. Returns whether the window was one to ask.
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

/// A picture of a window: RGB, three bytes a pixel, rows top to bottom.
#[derive(Debug)]
pub struct Pixels {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
    /// "Windows.Graphics.Capture" or "PrintWindow".
    pub method: &'static str,
}

/// Why a picture was not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoPicture {
    /// Minimized or gone: showing it would change the person's screen.
    Minimized,
    /// Its program is not answering.
    Hung,
    /// Larger than [`MAX_SIDE`] on a side.
    TooLarge,
    /// The caller's check failed between the two methods (the window changed hands, the key went off).
    Stopped,
    /// Both methods failed.
    Failed,
}

/// Checks that `pitch` bytes a row hold `width` BGRA pixels and that `len` bytes hold `height` such rows.
pub fn rows_fit(width: usize, height: usize, pitch: usize, len: usize) -> bool {
    let row = width.checked_mul(4);
    row.is_some_and(|row| {
        pitch >= row && height > 0 && pitch.checked_mul(height - 1).and_then(|b| b.checked_add(row)).is_some_and(|need| need <= len)
    })
}

fn rgb_row(row: &[u8], out: &mut Vec<u8>) {
    for pixel in row.as_chunks::<4>().0 {
        out.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
    }
}

/// RGB pixels from tightly or loosely packed BGRA rows, or an error when the rows do not fit the buffer.
pub fn from_bgra(width: usize, height: usize, pitch: usize, bgra: &[u8], method: &'static str) -> io::Result<Pixels> {
    if !rows_fit(width, height, pitch, bgra.len()) {
        return Err(io::Error::other("picture rows do not fit their buffer"));
    }
    let mut rgb = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        rgb_row(&bgra[y * pitch..y * pitch + width * 4], &mut rgb);
    }
    Ok(Pixels { width, height, rgb, method })
}

fn blank(pixels: &Pixels) -> bool {
    let (all, _) = pixels.rgb.as_chunks::<3>();
    all.iter().all(|p| Some(p) == all.first())
}

fn dimensions(width: i32, height: i32) -> Result<(usize, usize), NoPicture> {
    if width < 1 || height < 1 {
        return Err(NoPicture::Failed);
    }
    if width > MAX_SIDE || height > MAX_SIDE {
        return Err(NoPicture::TooLarge);
    }
    Ok((width as usize, height as usize))
}

/// Closes a capture frame on every path.
struct FrameGuard(Direct3D11CaptureFrame);
impl Drop for FrameGuard {
    fn drop(&mut self) {
        let _ = self.0.Close();
    }
}

/// Unmaps a mapped staging texture on every path.
struct Mapped<'a> {
    context: &'a ID3D11DeviceContext,
    texture: &'a ID3D11Texture2D,
    data: D3D11_MAPPED_SUBRESOURCE,
}
impl Drop for Mapped<'_> {
    fn drop(&mut self) {
        // SAFETY: the texture was mapped by `Map` on this context, and is unmapped once.
        unsafe { self.context.Unmap(self.texture, 0) };
    }
}

fn graphics_capture(target: HWND) -> Result<Pixels, NoPicture> {
    let failed = |_| NoPicture::Failed;
    if !GraphicsCaptureSession::IsSupported().map_err(failed)? {
        return Err(NoPicture::Failed);
    }
    let factory = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>().map_err(failed)?;
    // SAFETY: the interop creates a capture item for a window handle; it neither shows nor activates the window.
    let item: GraphicsCaptureItem = unsafe { factory.CreateForWindow(target) }.map_err(failed)?;
    let size = item.Size().map_err(failed)?;
    dimensions(size.Width, size.Height)?;
    // SAFETY: Direct3D objects made, used and released here; the mapped memory is read row by row, inside its pitch,
    // while `Mapped` keeps it mapped.
    unsafe {
        let (mut native, mut context) = (None, None);
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut native),
            None,
            Some(&mut context),
        )
        .map_err(failed)?;
        let native = native.ok_or(NoPicture::Failed)?;
        let context = context.ok_or(NoPicture::Failed)?;
        let dxgi: IDXGIDevice = native.cast().map_err(failed)?;
        let device: IDirect3DDevice = CreateDirect3D11DeviceFromDXGIDevice(&dxgi).map_err(failed)?.cast().map_err(failed)?;
        let pool =
            Direct3D11CaptureFramePool::CreateFreeThreaded(&device, DirectXPixelFormat::B8G8R8A8UIntNormalized, 1, size).map_err(failed)?;
        let session = pool.CreateCaptureSession(&item).map_err(failed)?;
        let _ = session.SetIsCursorCaptureEnabled(false);
        let _ = session.SetIsBorderRequired(false);
        let result = (|| {
            session.StartCapture().map_err(failed)?;
            let deadline = Instant::now() + FRAME_WAIT;
            let frame = loop {
                if let Some(frame) = optional(pool.TryGetNextFrame()).map_err(|_| NoPicture::Failed)? {
                    break FrameGuard(frame);
                }
                if Instant::now() >= deadline {
                    return Err(NoPicture::Failed);
                }
                std::thread::sleep(Duration::from_millis(20));
            };
            let content = frame.0.ContentSize().map_err(failed)?;
            let (width, height) = dimensions(content.Width, content.Height)?;
            let surface: IDirect3DDxgiInterfaceAccess = frame.0.Surface().map_err(failed)?.cast().map_err(failed)?;
            let texture: ID3D11Texture2D = surface.GetInterface().map_err(failed)?;
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            texture.GetDesc(&mut desc);
            if width > desc.Width as usize || height > desc.Height as usize {
                return Err(NoPicture::Failed);
            }
            desc.Usage = D3D11_USAGE_STAGING;
            desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            desc.BindFlags = 0;
            desc.MiscFlags = 0;
            let mut staging = None;
            native.CreateTexture2D(&desc, None, Some(&mut staging)).map_err(failed)?;
            let staging = staging.ok_or(NoPicture::Failed)?;
            context.CopyResource(&staging, &texture);
            drop(frame);
            let mut data = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut data)).map_err(failed)?;
            let mapped = Mapped { context: &context, texture: &staging, data };
            let pitch = mapped.data.RowPitch as usize;
            if mapped.data.pData.is_null() || pitch < width * 4 {
                return Err(NoPicture::Failed);
            }
            // Each row is copied, its pixels only, never the padding after them, which Direct3D leaves undefined.
            let (mut row, mut rgb) = (vec![0u8; width * 4], Vec::with_capacity(width * height * 3));
            for y in 0..height {
                std::ptr::copy_nonoverlapping(mapped.data.pData.cast::<u8>().add(y * pitch), row.as_mut_ptr(), width * 4);
                rgb_row(&row, &mut rgb);
            }
            drop(mapped);
            Ok(Pixels { width, height, rgb, method: "Windows.Graphics.Capture" })
        })();
        let _ = session.Close();
        let _ = pool.Close();
        result
    }
}

fn print_window(target: HWND) -> Result<Pixels, NoPicture> {
    let mut rect = RECT::default();
    // SAFETY: `rect` is ours.
    unsafe { GetWindowRect(target, &mut rect) }.map_err(|_| NoPicture::Failed)?;
    let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
    let (width, height) = dimensions(w, h)?;
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: the memory DC and DIB section are made, selected, read and released here; `bits` is checked non-null and
    // spans width*height*4 bytes, zeroed before PrintWindow writes into it.
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return Err(NoPicture::Failed);
        }
        let result = (|| {
            let mut bits = std::ptr::null_mut();
            let bitmap = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).map_err(|_| NoPicture::Failed)?;
            let old = SelectObject(dc, bitmap.into());
            let result = if old.is_invalid() || bits.is_null() {
                Err(NoPicture::Failed)
            } else {
                let len = width * height * 4;
                std::ptr::write_bytes(bits.cast::<u8>(), 0, len);
                let printed = PrintWindow(target, dc, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT));
                let _ = GdiFlush();
                let pixels = if printed.as_bool() {
                    from_bgra(width, height, width * 4, std::slice::from_raw_parts(bits.cast(), len), "PrintWindow")
                        .map_err(|_| NoPicture::Failed)
                } else {
                    Err(NoPicture::Failed)
                };
                SelectObject(dc, old);
                pixels
            };
            let _ = DeleteObject(bitmap.into());
            result
        })();
        let _ = DeleteDC(dc);
        result
    }
}

/// A picture of the whole window, as it is now. A minimized window is refused (showing it would change the person's
/// screen), and so is a hung one. `still` is asked before the fallback, once the first picture is freed: a false answer
/// stops there.
pub fn picture(handle: u64, still: &dyn Fn() -> bool) -> Result<Pixels, NoPicture> {
    let target = hwnd(handle);
    // SAFETY: plain queries on a handle.
    if unsafe { !IsWindow(Some(target)).as_bool() || IsIconic(target).as_bool() } {
        return Err(NoPicture::Minimized);
    }
    if hung(handle) {
        return Err(NoPicture::Hung);
    }
    let mut rect = RECT::default();
    // SAFETY: `rect` is ours.
    unsafe { GetWindowRect(target, &mut rect) }.map_err(|_| NoPicture::Failed)?;
    dimensions(rect.right - rect.left, rect.bottom - rect.top)?;
    let _apartment = Apartment::enter();
    let first = match graphics_capture(target) {
        Ok(pixels) if !blank(&pixels) => return Ok(pixels),
        // A blank first picture is freed here, before the second is made.
        Ok(_) => NoPicture::Failed,
        Err(why) => why,
    };
    if first == NoPicture::TooLarge {
        return Err(first);
    }
    if !still() {
        return Err(NoPicture::Stopped);
    }
    if hung(handle) {
        return Err(NoPicture::Hung);
    }
    print_window(target)
}

/// Whether a picture has a single color everywhere (a Chromium page off screen, a window not yet painted).
pub fn is_blank(pixels: &Pixels) -> bool {
    blank(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_that_do_not_fit_their_buffer_are_refused() {
        assert!(rows_fit(2, 2, 8, 16));
        assert!(rows_fit(2, 2, 12, 20), "the last row needs no padding after it");
        assert!(!rows_fit(2, 2, 7, 16), "a pitch shorter than a row");
        assert!(!rows_fit(2, 2, 0, 16));
        assert!(!rows_fit(2, 2, 12, 19));
        assert!(!rows_fit(2, 0, 8, 16));
        assert!(!rows_fit(usize::MAX, 2, 8, 16), "no overflow");
        // Two pixels a row, a pitch of 12 bytes: the 4 padding bytes are never read.
        let bgra = [1, 2, 3, 0, 4, 5, 6, 0, 9, 9, 9, 9, 7, 8, 9, 0, 10, 11, 12, 0];
        let pixels = from_bgra(2, 2, 12, &bgra, "test").unwrap();
        assert_eq!(pixels.rgb, [3, 2, 1, 6, 5, 4, 9, 8, 7, 12, 11, 10]);
        assert!(from_bgra(2, 2, 7, &bgra, "test").is_err());
        assert!(from_bgra(3, 2, 8, &bgra, "test").is_err());
        assert_eq!(dimensions(MAX_SIDE + 1, 10).unwrap_err(), NoPicture::TooLarge);
        assert_eq!(dimensions(0, 10).unwrap_err(), NoPicture::Failed);
    }
}

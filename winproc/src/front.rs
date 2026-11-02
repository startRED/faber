//! What Windows says about the window in front, for the tray's rules (R191): whether it may show a notification, and
//! whether another program's window covers its whole monitor (a borderless game). Both are reads that cost nothing and
//! open nothing; the rules that decide with them are Faber's own and live in `faber-host`, so the shell that draws the
//! tray and the sidecar that decides for it ask the same questions.

/// A window or monitor rectangle in physical pixels, as Windows gives it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// The window in front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Front {
    /// One of the asking process's own windows.
    Own,
    /// Another program's window, and the monitor it is mostly on.
    Other { window: Rect, monitor: Rect },
}

/// `SHQueryUserNotificationState`: the `QUERY_USER_NOTIFICATION_STATE` number, or the failed HRESULT.
#[cfg(windows)]
pub fn notification_state() -> Result<i32, i32> {
    use windows_sys::Win32::UI::Shell::SHQueryUserNotificationState;
    let mut state = 0;
    // SAFETY: Windows writes one notification-state enum to this live, aligned i32.
    let result = unsafe { SHQueryUserNotificationState(&mut state) };
    if result < 0 { Err(result) } else { Ok(state) }
}

/// The window in front, or None when Windows does not give its bounds (no window, no monitor, a call that failed).
/// `own_process` is the id of the process whose windows count as [`Front::Own`].
#[cfg(windows)]
pub fn foreground(own_process: u32) -> Option<Front> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONULL, MONITORINFO, MonitorFromWindow};
    use windows_sys::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId};

    // SAFETY: Query-only APIs; the process id is written to a live, aligned u32.
    let window = unsafe { GetForegroundWindow() };
    if window.is_null() {
        return None;
    }
    let mut process = 0;
    if unsafe { GetWindowThreadProcessId(window, &mut process) } == 0 {
        return None;
    }
    if process == own_process {
        return Some(Front::Own);
    }
    // GetWindowRect is DPI-virtualized; use physical coordinates for both rectangles on this worker, then restore it.
    // SAFETY: This is the documented pseudo-handle, not dereferenced by Rust.
    let previous = unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if previous.is_null() {
        return None;
    }
    let mut bounds = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    // SAFETY: Handles come from Windows; the output structs have the documented C layouts and correct cbSize.
    let read = unsafe {
        let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONULL); // Do not guess another monitor.
        !monitor.is_null() && GetWindowRect(window, &mut bounds) != 0 && GetMonitorInfoW(monitor, &mut info) != 0
    };
    // SAFETY: Restore the valid context returned for this same thread, including after a failed bounds query.
    let restored = unsafe { SetThreadDpiAwarenessContext(previous) };
    let plain = |r: RECT| Rect { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
    (read && !restored.is_null()).then(|| Front::Other { window: plain(bounds), monitor: plain(info.rcMonitor) })
}

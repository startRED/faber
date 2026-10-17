//! Windowless Windows state queries and chime playback.
#![allow(unsafe_code)]

use super::decide::{Attention, Foreground, Rect, covers_monitor, game_in_front, windows_attention};
use std::ffi::c_void;

#[repr(C)]
struct MonitorInfo {
    size: u32,
    monitor: Rect,
    work: Rect,
    flags: u32,
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn SHQueryUserNotificationState(state: *mut i32) -> i32;
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetForegroundWindow() -> *mut c_void;
    fn GetWindowThreadProcessId(window: *mut c_void, process: *mut u32) -> u32;
    fn GetWindowRect(window: *mut c_void, rect: *mut Rect) -> i32;
    fn MonitorFromWindow(window: *mut c_void, flags: u32) -> *mut c_void;
    fn GetMonitorInfoW(monitor: *mut c_void, info: *mut MonitorInfo) -> i32;
    fn SetThreadDpiAwarenessContext(context: *mut c_void) -> *mut c_void;
}

#[link(name = "winmm")]
unsafe extern "system" {
    fn PlaySoundA(sound: *const u8, module: *mut std::ffi::c_void, flags: u32) -> i32;
}

pub(super) fn attention() -> Attention {
    let state = notification_state();
    if let Err(result) = state {
        eprintln!("faber: the notification state could not be read (HRESULT {result:#x}); suppressing the toast");
    }
    let attention = windows_attention(state.ok(), false);
    if attention != Attention::Normal {
        return attention;
    }
    match foreground() {
        Some(Foreground { own: true, .. }) => Attention::Silent,
        Some(front) => windows_attention(Some(5), front.covers),
        None => {
            eprintln!("faber: foreground window bounds could not be read; suppressing the toast");
            Attention::SoundOnly
        }
    }
}

/// Whether a game or another full-screen app is in front (`decide::game_in_front`). The screen asks every few seconds
/// while its window is shown without the focus, so a failed query says nothing in the log and only means "no".
pub(super) fn game() -> bool {
    let state = notification_state().ok();
    // A locked screen, quiet time or an unknown state is never a game, so the window in front is read only for the others.
    let front = if matches!(state, Some(2..=5 | 7)) { foreground() } else { None };
    game_in_front(state, front)
}

fn notification_state() -> Result<i32, i32> {
    let mut state = 0;
    // SAFETY: Windows writes one notification-state enum to this live, aligned i32.
    let result = unsafe { SHQueryUserNotificationState(&mut state) };
    if result < 0 { Err(result) } else { Ok(state) }
}

fn foreground() -> Option<Foreground> {
    // SAFETY: Query-only APIs; the process id is written to a live, aligned u32.
    let window = unsafe { GetForegroundWindow() };
    if window.is_null() {
        return None;
    }
    let mut process = 0;
    if unsafe { GetWindowThreadProcessId(window, &mut process) } == 0 {
        return None;
    }
    if process == std::process::id() {
        return Some(Foreground { own: true, covers: false });
    }
    // GetWindowRect is DPI-virtualized; use physical coordinates for both rectangles on this worker, then restore it.
    const PER_MONITOR_AWARE_V2: isize = -4;
    // SAFETY: This is the documented DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 pseudo-handle, not dereferenced by Rust.
    let previous = unsafe { SetThreadDpiAwarenessContext(PER_MONITOR_AWARE_V2 as *mut c_void) };
    if previous.is_null() {
        return None;
    }
    let mut rect = Rect::default();
    let mut info =
        MonitorInfo { size: std::mem::size_of::<MonitorInfo>() as u32, monitor: Rect::default(), work: Rect::default(), flags: 0 };
    // SAFETY: Handles come from Windows; the output structs have the documented C layouts and correct cbSize.
    let read = unsafe {
        let monitor = MonitorFromWindow(window, 0); // MONITOR_DEFAULTTONULL: do not guess another monitor.
        !monitor.is_null() && GetWindowRect(window, &mut rect) != 0 && GetMonitorInfoW(monitor, &mut info) != 0
    };
    // SAFETY: Restore the valid context returned for this same thread, including after a failed bounds query.
    let restored = unsafe { SetThreadDpiAwarenessContext(previous) };
    (read && !restored.is_null()).then(|| Foreground { own: false, covers: covers_monitor(rect, info.monitor) })
}

pub(super) fn play() -> Result<(), String> {
    static WAV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/attention.wav"));
    const SND_ASYNC: u32 = 0x0001;
    const SND_NODEFAULT: u32 = 0x0002;
    const SND_MEMORY: u32 = 0x0004;
    // SAFETY: WAV is a complete PCM WAVE and remains alive for the entire asynchronous playback; no module is used.
    let played = unsafe { PlaySoundA(WAV.as_ptr(), std::ptr::null_mut(), SND_ASYNC | SND_NODEFAULT | SND_MEMORY) };
    if played == 0 { Err("PlaySound failed".into()) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_query_structs_match_the_win32_abi() {
        assert_eq!(std::mem::size_of::<Rect>(), 16);
        assert_eq!(std::mem::size_of::<MonitorInfo>(), 40);
        assert_eq!(std::mem::offset_of!(MonitorInfo, monitor), 4);
        assert_eq!(std::mem::offset_of!(MonitorInfo, work), 20);
        assert_eq!(std::mem::offset_of!(MonitorInfo, flags), 36);
    }
}

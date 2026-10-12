//! The two windowless Windows APIs needed for attention notifications.
#![allow(unsafe_code)]

#[link(name = "shell32")]
unsafe extern "system" {
    fn SHQueryUserNotificationState(state: *mut i32) -> i32;
}

#[link(name = "winmm")]
unsafe extern "system" {
    fn PlaySoundA(sound: *const u8, module: *mut std::ffi::c_void, flags: u32) -> i32;
}

pub(super) fn can_toast() -> bool {
    let mut state = 0;
    // SAFETY: Windows writes one notification-state enum to this live, aligned i32.
    let result = unsafe { SHQueryUserNotificationState(&mut state) };
    if result < 0 {
        eprintln!("faber: the notification state could not be read (HRESULT {result:#x}); suppressing the toast");
    }
    super::decide::toast_allowed((result >= 0).then_some(state))
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

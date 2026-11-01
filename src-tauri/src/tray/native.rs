//! The attention chime on Windows. What Windows says about the window in front (may a notification show, is a game in
//! front) is read by `faber_host::tray::front`, through `faber-winproc`.
#![allow(unsafe_code)]

#[link(name = "winmm")]
unsafe extern "system" {
    fn PlaySoundA(sound: *const u8, module: *mut std::ffi::c_void, flags: u32) -> i32;
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

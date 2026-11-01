//! A short sound from memory, for the attention chime of a process with no window (the sidecar). Windows only: other
//! systems play the same file with their own player.

/// Whether `wav` is a whole RIFF WAVE whose every chunk ends inside it, so the system, which reads it by pointer and the
/// sizes it holds, never reads past the slice.
pub fn whole_wave(wav: &[u8]) -> bool {
    let size = |at: usize| wav.get(at..at + 4).map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize);
    if wav.get(0..4) != Some(b"RIFF") || wav.get(8..12) != Some(b"WAVE") {
        return false;
    }
    let Some(end) = size(4).and_then(|riff| riff.checked_add(8)) else { return false };
    if end > wav.len() {
        return false;
    }
    let mut at = 12;
    while at < end {
        let Some(chunk) = size(at + 4) else { return false };
        // A chunk of odd size is followed by one byte of padding.
        match chunk.checked_add(chunk & 1).and_then(|body| (at + 8).checked_add(body)) {
            Some(next) if next <= end => at = next,
            _ => return false,
        }
    }
    true
}

/// Starts playing `wav` (a whole PCM WAVE) and returns at once; the sound keeps playing on its own. A buffer that is not a
/// whole WAVE is refused before the system sees it.
#[cfg(windows)]
pub fn play_wave(wav: &'static [u8]) -> std::io::Result<()> {
    use windows_sys::Win32::Media::Audio::{PlaySoundA, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    if !whole_wave(wav) {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "not a whole WAVE"));
    }
    // SAFETY: `wav` is a whole RIFF WAVE (checked above: every chunk ends inside the slice) that lives for the whole program,
    // so the asynchronous playback never reads past it or reads freed memory; no module is passed.
    let played = unsafe { PlaySoundA(wav.as_ptr(), std::ptr::null_mut(), SND_ASYNC | SND_NODEFAULT | SND_MEMORY) };
    if played == 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::whole_wave;

    fn wave(chunks: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        let mut body = b"WAVE".to_vec();
        for (id, data) in chunks {
            body.extend_from_slice(*id);
            body.extend_from_slice(&(data.len() as u32).to_le_bytes());
            body.extend_from_slice(data);
            if data.len() % 2 == 1 {
                body.push(0);
            }
        }
        let mut file = b"RIFF".to_vec();
        file.extend_from_slice(&(body.len() as u32).to_le_bytes());
        file.extend_from_slice(&body);
        file
    }

    #[test]
    fn only_a_wave_whose_chunks_all_end_inside_it_is_played() {
        let good = wave(&[(b"fmt ", &[0; 16]), (b"data", &[1, 2, 3])]);
        assert!(whole_wave(&good));
        assert!(!whole_wave(&good[..good.len() - 1]), "cut short");
        let mut lying = good.clone();
        let data = lying.len() - 4 - 4; // the data chunk's size field (3 bytes and a pad byte follow it)
        lying[data..data + 4].copy_from_slice(&1000u32.to_le_bytes());
        assert!(!whole_wave(&lying), "a chunk that says it is longer than the file");
        let mut riff = good.clone();
        riff[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(!whole_wave(&riff), "a RIFF size past the end");
        assert!(!whole_wave(b"RIFF\0\0\0\0WAVX"));
        assert!(!whole_wave(b""));
    }
}

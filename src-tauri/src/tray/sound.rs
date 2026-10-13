#[cfg(not(windows))]
pub(super) fn prepare(dir: &std::path::Path) -> Result<(), String> {
    std::fs::write(dir.join("attention.wav"), include_bytes!(concat!(env!("OUT_DIR"), "/attention.wav"))).map_err(|e| e.to_string())
}

#[cfg(not(windows))]
pub(super) fn play(dir: &std::path::Path) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let path = dir.join("attention.wav");
    #[cfg(target_os = "macos")]
    let players = ["/usr/bin/afplay"];
    #[cfg(not(target_os = "macos"))]
    let players = ["paplay", "aplay"];
    let mut errors = Vec::new();
    for player in players {
        match Command::new(player).arg(&path).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status() {
            Ok(status) if status.success() => return Ok(()),
            Ok(status) => errors.push(format!("{player}: {status}")),
            Err(e) => errors.push(format!("{player}: {e}")),
        }
    }
    Err(errors.join("; "))
}

#[cfg(test)]
#[path = "../../chime.rs"]
mod chime;

#[cfg(test)]
mod tests {
    #[test]
    fn the_original_chime_is_a_short_valid_pcm_wave_without_clipping() {
        let wav = super::chime::wav();
        assert_eq!(wav.as_slice(), include_bytes!(concat!(env!("OUT_DIR"), "/attention.wav")), "the build embeds the original chime");
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(&wav[36..40], b"data");
        let u32_at = |offset| u32::from_le_bytes(wav[offset..offset + 4].try_into().unwrap());
        assert_eq!(u32_at(4) as usize + 8, wav.len());
        assert_eq!(u32_at(40) as usize + 44, wav.len());
        assert_eq!(u32_at(24), 24_000);
        assert_eq!(&wav[20..24], &[1, 0, 1, 0], "PCM mono");
        assert_eq!(&wav[32..36], &[2, 0, 16, 0], "16-bit samples");
        assert_eq!(wav.len(), 44 + 24_000 * 48 / 100 * 2);
        let samples: Vec<_> = wav[44..].as_chunks::<2>().0.iter().map(|s| i16::from_le_bytes(*s)).collect();
        assert_eq!(samples[0], 0);
        assert!(samples.last().unwrap().abs() < 20, "soft ending");
        assert!(samples.iter().all(|s| s.abs() < 14_000));
        assert!(samples.iter().any(|s| s.abs() > 5000), "audible signal");
    }
}

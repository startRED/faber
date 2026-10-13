/// Faber's original two-note chime: 480 ms of mono 16-bit PCM, with soft edges and a decaying fifth.
pub fn wav() -> Vec<u8> {
    const RATE: u32 = 24_000;
    const SAMPLES: u32 = RATE * 48 / 100;
    let mut wav = Vec::with_capacity(44 + SAMPLES as usize * 2);
    wav.extend(b"RIFF");
    wav.extend((36 + SAMPLES * 2).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16_u32.to_le_bytes());
    wav.extend(1_u16.to_le_bytes());
    wav.extend(1_u16.to_le_bytes());
    wav.extend(RATE.to_le_bytes());
    wav.extend((RATE * 2).to_le_bytes());
    wav.extend(2_u16.to_le_bytes());
    wav.extend(16_u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend((SAMPLES * 2).to_le_bytes());
    for i in 0..SAMPLES {
        let t = i as f64 / RATE as f64;
        let note = |start: f64, hz: f64| {
            let age = t - start;
            if age < 0.0 {
                return 0.0;
            }
            let envelope = (age / 0.008).min(1.0) * (-10.0 * age).exp() * ((0.48 - t) / 0.025).min(1.0);
            (std::f64::consts::TAU * hz * age).sin() * envelope
        };
        let sample = ((note(0.0, 660.0) + note(0.14, 990.0)) * 7000.0) as i16;
        wav.extend(sample.to_le_bytes());
    }
    wav
}

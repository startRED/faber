//! The app icon: the Windows icon holds every size the shell asks for, and the two small ones are the hand-tuned drawings.

use std::fs;
use std::path::PathBuf;

fn icon(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("icons").join(name);
    fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn le(bytes: &[u8], at: usize, len: usize) -> usize {
    bytes[at..at + len].iter().rev().fold(0, |n, b| (n << 8) | *b as usize)
}

/// The frames of an ICO file: (side in pixels, the bytes of its picture), from the directory at the top of the file.
fn frames(ico: &[u8]) -> Vec<(usize, Vec<u8>)> {
    assert_eq!((le(ico, 0, 2), le(ico, 2, 2)), (0, 1), "not an icon file");
    (0..le(ico, 4, 2))
        .map(|i| {
            let entry = 6 + 16 * i;
            let side = ico[entry] as usize;
            let (size, offset) = (le(ico, entry + 8, 4), le(ico, entry + 12, 4));
            (if side == 0 { 256 } else { side }, ico[offset..offset + size].to_vec())
        })
        .collect()
}

#[test]
fn icon_ico_has_the_six_sizes_and_the_tuned_small_ones() {
    let frames = frames(&icon("icon.ico"));
    let sizes: Vec<usize> = frames.iter().map(|(side, _)| *side).collect();
    assert_eq!(sizes, [16, 24, 32, 48, 64, 256]);
    for (side, bytes) in &frames {
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "the {side} px frame is not a PNG");
        if matches!(side, 16 | 24) {
            assert!(*bytes == icon(&format!("{side}x{side}.png")), "the {side} px frame is not byte for byte {side}x{side}.png");
        }
    }
}

#[test]
fn the_tray_has_one_picture_per_scale_and_each_is_the_size_it_says() {
    for side in [16usize, 24, 32] {
        let png = icon(&format!("tray-{side}.png"));
        // A PNG's header holds the width and the height as big-endian numbers right after "IHDR".
        let be = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
        assert_eq!((be(16), be(20)), (side, side), "tray-{side}.png");
    }
    assert!(icon("tray-16.png") == icon("16x16.png") && icon("tray-24.png") == icon("24x24.png"), "the tray uses the tuned drawings");
}

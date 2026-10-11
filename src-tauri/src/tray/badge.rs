//! The badge: an amber circle with the number of questions that wait for the user, drawn at run time on the tray
//! picture that fits the display. Plain functions of pixels, tested without a tray.

/// The amber that means "needs you", and the dark ink printed on it [tokens].
const AMBER: [u8; 3] = [0xf2, 0xb1, 0x3c];
const AMBER_INK: [u8; 3] = [0x1d, 0x14, 0x04];

/// The character printed on the badge: 1 to 9, "+" for ten and more, nothing for none.
pub(super) fn glyph(needs_you: usize) -> Option<char> {
    match needs_you {
        0 => None,
        n @ 1..=9 => char::from_digit(n as u32, 10),
        _ => Some('+'),
    }
}

/// The 3 by 5 pixel font of the badge: one row per number, the left pixel in bit 2.
fn glyph_rows(c: char) -> Option<[u8; 5]> {
    Some(match c {
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b010, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        '+' => [0b000, 0b010, 0b111, 0b010, 0b000],
        _ => return None,
    })
}

/// The tray picture that fits a display scale: the smallest of the three that is not smaller than 16 px times the
/// scale, so a picture is scaled down by the system and never up.
pub(super) fn tray_side(scale: f64) -> u32 {
    if scale.is_nan() || scale <= 1.0 {
        16
    } else if scale <= 1.5 {
        24
    } else {
        32
    }
}

pub(super) fn tray_png(side: u32) -> &'static [u8] {
    match side {
        16 => include_bytes!("../../icons/tray-16.png"),
        24 => include_bytes!("../../icons/tray-24.png"),
        _ => include_bytes!("../../icons/tray-32.png"),
    }
}

/// Source-over of one colour on one RGBA pixel, with `cover` sixteenths of the pixel covered.
fn blend(px: &mut [u8], color: [u8; 3], cover: u32) {
    let (dst_a, back) = (u32::from(px[3]), 16 - cover);
    let total = cover * 255 + dst_a * back;
    if total == 0 {
        return;
    }
    for i in 0..3 {
        px[i] = ((u32::from(color[i]) * cover * 255 + u32::from(px[i]) * dst_a * back + total / 2) / total) as u8;
    }
    px[3] = ((total + 8) / 16) as u8;
}

/// The badge on a square RGBA picture: an amber circle at the bottom right with the character in dark ink. None for
/// a picture it does not know how to draw on (not square, not a tray size, wrong buffer, a character without a glyph),
/// so the caller keeps the plain icon.
pub(super) fn with_badge(rgba: &[u8], width: u32, height: u32, c: char) -> Option<Vec<u8>> {
    // Circle diameter and pixel size of the character. The diameter and the character have the same parity, so the
    // character sits in the middle.
    let (diameter, scale) = match width {
        16 => (9, 1),
        24 => (14, 2),
        32 => (18, 2),
        _ => return None,
    };
    let rows = glyph_rows(c)?;
    if width != height || rgba.len() != (width * height * 4) as usize {
        return None;
    }
    let mut out = rgba.to_vec();
    let side = width;
    // Work in eighths of a pixel: 4 by 4 samples per pixel give the circle a soft edge.
    let (centre, radius) = (i64::from(side * 8 - diameter * 4), i64::from(diameter * 4));
    for y in side - diameter..side {
        for x in side - diameter..side {
            let mut cover = 0;
            for sy in 0..4 {
                for sx in 0..4 {
                    let (dx, dy) = (i64::from(x * 8 + sx * 2 + 1) - centre, i64::from(y * 8 + sy * 2 + 1) - centre);
                    cover += u32::from(dx * dx + dy * dy <= radius * radius);
                }
            }
            if cover > 0 {
                let at = ((y * side + x) * 4) as usize;
                blend(&mut out[at..at + 4], AMBER, cover);
            }
        }
    }
    let (left, top) = (side - diameter + (diameter - 3 * scale) / 2, side - diameter + (diameter - 5 * scale) / 2);
    for (row, bits) in rows.iter().enumerate() {
        for col in 0..3u32 {
            if bits >> (2 - col) & 1 == 1 {
                for dy in 0..scale {
                    for dx in 0..scale {
                        let (x, y) = (left + col * scale + dx, top + row as u32 * scale + dy);
                        let at = ((y * side + x) * 4) as usize;
                        out[at..at + 3].copy_from_slice(&AMBER_INK);
                        out[at + 3] = 255;
                    }
                }
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::image::Image;

    /// The badge on a transparent icon, as text: `#` is the character, `o` full amber, `:` the soft edge of the circle and
    /// `.` a pixel the badge left alone.
    fn art(side: u32, c: char) -> Vec<String> {
        let plain = vec![0u8; (side * side * 4) as usize];
        let badge = with_badge(&plain, side, side, c).expect("a tray size");
        (0..side)
            .map(|y| {
                (0..side)
                    .map(|x| {
                        let at = ((y * side + x) * 4) as usize;
                        match (badge[at + 3], badge[at..at + 3] == AMBER_INK) {
                            (0, _) => '.',
                            (255, true) => '#',
                            (255, false) => 'o',
                            _ => ':',
                        }
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn the_badge_shows_1_to_9_then_a_plus_and_nothing_for_none() {
        assert_eq!(glyph(0), None);
        assert_eq!(glyph(1), Some('1'));
        assert_eq!(glyph(9), Some('9'));
        assert_eq!(glyph(10), Some('+'));
        assert_eq!(glyph(12), Some('+'));
        assert!(('1'..='9').chain(['+']).all(|c| glyph_rows(c).is_some()));
    }

    /// The nine rows of the circle (the top of the icon, above them, stays empty).
    fn corner(side: u32, c: char) -> Vec<String> {
        let rows = art(side, c);
        assert!(rows[..7].iter().all(|r| r.chars().all(|p| p == '.')), "the top of the icon is untouched");
        rows[7..].to_vec()
    }

    #[test]
    fn the_badge_of_1_at_16_px_is_drawn_pixel_by_pixel() {
        assert_eq!(
            corner(16, '1'),
            [
                "........:::o:::.",
                ".......::ooooo::",
                ".......:ooo#ooo:",
                ".......:oo##ooo:",
                ".......oooo#oooo",
                ".......:ooo#ooo:",
                ".......:oo###oo:",
                ".......::ooooo::",
                "........:::o:::.",
            ]
        );
    }

    #[test]
    fn the_badge_of_9_at_16_px_is_drawn_pixel_by_pixel() {
        assert_eq!(
            corner(16, '9'),
            [
                "........:::o:::.",
                ".......::ooooo::",
                ".......:oo###oo:",
                ".......:oo#o#oo:",
                ".......ooo###ooo",
                ".......:oooo#oo:",
                ".......:oo###oo:",
                ".......::ooooo::",
                "........:::o:::.",
            ]
        );
    }

    #[test]
    fn the_badge_of_12_is_a_plus_at_16_px() {
        let c = glyph(12).expect("12 has a character");
        assert_eq!(
            corner(16, c),
            [
                "........:::o:::.",
                ".......::ooooo::",
                ".......:ooooooo:",
                ".......:ooo#ooo:",
                ".......ooo###ooo",
                ".......:ooo#ooo:",
                ".......:ooooooo:",
                ".......::ooooo::",
                "........:::o:::.",
            ]
        );
    }

    #[test]
    fn a_badge_is_drawn_at_every_tray_size_and_stays_inside_its_corner() {
        for (side, diameter) in [(16u32, 9u32), (24, 14), (32, 18)] {
            let rows = art(side, '5');
            for (y, row) in rows.iter().enumerate() {
                for (x, px) in row.chars().enumerate() {
                    let inside = x as u32 >= side - diameter && y as u32 >= side - diameter;
                    assert!(inside || px == '.', "{side} px: ({x}, {y}) is outside the corner");
                }
            }
            let ink = rows.iter().flat_map(|r| r.chars()).filter(|c| *c == '#').count();
            assert!(ink > 0, "{side} px has a character");
            // The middle of the circle is amber, or ink where the character is.
            let mid = (side - diameter / 2) as usize;
            assert_ne!(rows[mid].chars().nth(mid), Some('.'));
        }
    }

    #[test]
    fn the_badge_blends_over_the_icon_and_keeps_what_is_under_the_character() {
        let side = 16u32;
        let lime = [0xd6u8, 0xdc, 0x48, 255];
        let plain: Vec<u8> = (0..side * side).flat_map(|_| lime).collect();
        let badge = with_badge(&plain, side, side, '1').unwrap();
        let px = |x: u32, y: u32| &badge[((y * side + x) * 4) as usize..][..4];
        assert_eq!(px(2, 2), lime, "far from the corner nothing changes");
        assert_eq!(px(8, 12), [AMBER[0], AMBER[1], AMBER[2], 255], "inside the circle it is amber");
        assert_eq!(px(11, 11), [AMBER_INK[0], AMBER_INK[1], AMBER_INK[2], 255], "the character is dark ink");
        assert_eq!(px(8, 7)[3], 255, "the soft edge of the circle keeps an opaque icon opaque");
        assert_ne!(px(8, 7), lime, "but it does take some amber");
    }

    #[test]
    fn a_picture_the_badge_cannot_be_drawn_on_is_refused() {
        assert_eq!(with_badge(&[0; 20 * 20 * 4], 20, 20, '1'), None, "not a tray size");
        assert_eq!(with_badge(&[0; 16 * 16 * 4], 16, 8, '1'), None, "not square");
        assert_eq!(with_badge(&[0; 10], 16, 16, '1'), None, "wrong buffer");
        assert_eq!(with_badge(&[0; 16 * 16 * 4], 16, 16, 'x'), None, "no glyph");
    }

    #[test]
    fn the_shipped_tray_pictures_decode_and_take_a_badge() {
        for side in [16, 24, 32] {
            let image = Image::from_bytes(tray_png(side)).expect("the picture decodes");
            assert_eq!((image.width(), image.height()), (side, side));
            assert!(with_badge(image.rgba(), side, side, '+').is_some());
        }
    }

    #[test]
    fn the_tray_picture_follows_the_display_scale() {
        let sides: Vec<u32> = [0.0, 1.0, 1.25, 1.5, 1.75, 2.0, 3.0, f64::NAN].iter().map(|s| tray_side(*s)).collect();
        assert_eq!(sides, [16, 16, 24, 24, 32, 32, 32, 16]);
    }
}

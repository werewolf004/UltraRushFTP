//! Application icon, drawn in code: a blue rounded square holding a dark folder with an
//! upload and a download arrow. The window uses it directly; the executables, the
//! installer and the notification area use `assets/UltraRushFTP.ico`, written from the
//! same drawing by `cargo run -p urftp-gui --example write_icon` (a test checks that the
//! file matches).

// The `.ico` writer is used by the tests and the `write_icon` example only.
#[cfg_attr(not(test), allow(dead_code))]
mod art;

use art::rgba;

/// Icon of the window title bar and taskbar.
pub fn window_icon() -> eframe::egui::IconData {
    eframe::egui::IconData { rgba: rgba(128), width: 128, height: 128 }
}

#[cfg(test)]
mod tests {
    use super::art::{ARROW, BLUE, FOLDER, ICO_SIZES, TAB, ico};
    use super::*;

    #[test]
    fn icon_has_the_right_size_and_transparent_corners() {
        for size in ICO_SIZES {
            let px = rgba(size);
            assert_eq!(px.len(), (size * size * 4) as usize);
            assert_eq!(px[3], 0, "top-left corner is transparent");
            let centre = ((size / 2 * size + size / 2) * 4) as usize;
            assert_eq!(px[centre + 3], 255, "centre is opaque");
        }
    }

    #[test]
    fn icon_shows_its_four_colours() {
        let px = rgba(64);
        let opaque: Vec<&[u8]> = px.chunks(4).filter(|p| p[3] == 255).collect();
        for colour in [BLUE, FOLDER, TAB, ARROW] {
            assert!(opaque.iter().any(|p| p[..3] == colour), "{colour:?} missing");
        }
    }

    #[test]
    fn small_sizes_keep_the_arrows_visible() {
        // At 16 px both arrows must still stand out from the folder around them.
        let px = rgba(16);
        let light = |half: std::ops::Range<u32>| {
            (0..16u32)
                .flat_map(|y| half.clone().map(move |x| ((y * 16 + x) * 4) as usize))
                .filter(|&i| px[i + 3] == 255 && px[i] > 0x90 && px[i + 2] > 0xC0)
                .count()
        };
        assert!(light(0..8) >= 4, "upload arrow: {} light pixels", light(0..8));
        assert!(light(8..16) >= 4, "download arrow: {} light pixels", light(8..16));
    }

    #[test]
    fn ico_file_holds_every_size() {
        let data = ico(&ICO_SIZES);
        let u16_at = |i: usize| u16::from_le_bytes([data[i], data[i + 1]]);
        let u32_at = |i: usize| u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        assert_eq!((u16_at(0), u16_at(2)), (0, 1), "icon header");
        assert_eq!(usize::from(u16_at(4)), ICO_SIZES.len());
        let mut end = 6 + 16 * ICO_SIZES.len();
        for (i, size) in ICO_SIZES.into_iter().enumerate() {
            let entry = 6 + 16 * i;
            let side = if size == 256 { 0 } else { size };
            assert_eq!((u32::from(data[entry]), u32::from(data[entry + 1])), (side, side));
            assert_eq!(u16_at(entry + 6), 32, "32 bits per pixel");
            let (len, offset) = (u32_at(entry + 8) as usize, u32_at(entry + 12) as usize);
            assert_eq!(offset, end, "images follow each other");
            assert_eq!(u32_at(offset), 40, "bitmap header");
            assert_eq!(u32_at(offset + 4), size);
            assert_eq!(u32_at(offset + 8), size * 2, "colour image plus mask");
            // Pixels are stored bottom-up in BGRA: compare with the RGBA image.
            let px = rgba(size);
            let first = offset + 40;
            let row_bytes = size as usize * 4;
            for (stored_row, row) in px.chunks(row_bytes).rev().enumerate() {
                for (x, p) in row.chunks(4).enumerate() {
                    let at = first + stored_row * row_bytes + x * 4;
                    assert_eq!(&data[at..at + 4], &[p[2], p[1], p[0], p[3]]);
                }
            }
            end = offset + len;
        }
        assert_eq!(data.len(), end, "nothing after the last image");
    }

    #[test]
    fn the_ico_file_of_the_repository_matches_the_drawing() {
        let committed = include_bytes!("../assets/UltraRushFTP.ico");
        assert!(
            committed.as_slice() == ico(&ICO_SIZES).as_slice(),
            "assets/UltraRushFTP.ico is out of date: run `cargo run -p urftp-gui --example write_icon`"
        );
    }
}

//! The application icon, drawn in code: a blue rounded square holding a dark folder
//! with an upload and a download arrow (file transfers).
//!
//! This file depends on nothing but `std`, so that the build script and the
//! `write_icon` example can include it too (`#[path]`).

/// Sizes stored in the `.ico` file: the ones Windows picks for the title bar,
/// taskbar, Explorer views and Start menu, at 100 % to 200 % display scaling.
pub const ICO_SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];

/// Background (blue).
pub const BLUE: [u8; 3] = [0x37, 0x8A, 0xDD];
/// Folder body (navy).
pub const FOLDER: [u8; 3] = [0x04, 0x2C, 0x53];
/// Folder tab and back (dark blue).
pub const TAB: [u8; 3] = [0x0C, 0x44, 0x7C];
/// Arrows (light blue).
pub const ARROW: [u8; 3] = [0xB5, 0xD4, 0xF4];

/// Samples per pixel side: 4 × 4 samples give smooth edges at every size.
const SAMPLES: u32 = 4;

/// An arrow, in a 64 × 64 grid: its shaft, the two strokes of its head, half its width.
struct Arrow {
    shaft: ((f64, f64), (f64, f64)),
    head: [((f64, f64), (f64, f64)); 2],
    half_width: f64,
}

/// Upload (left, pointing up) and download (right, pointing down) arrows.
const ARROWS: [Arrow; 2] = [
    Arrow { shaft: ((21.0, 47.0), (21.0, 29.0)), head: [((15.0, 35.0), (21.0, 28.5)), ((27.0, 35.0), (21.0, 28.5))], half_width: 2.25 },
    Arrow { shaft: ((43.0, 27.0), (43.0, 45.0)), head: [((37.0, 39.0), (43.0, 45.5)), ((49.0, 39.0), (43.0, 45.5))], half_width: 2.25 },
];

/// Arrows drawn at 24 px and below: larger and thicker, so that they stay readable.
const SMALL_ARROWS: [Arrow; 2] = [
    Arrow { shaft: ((20.0, 49.0), (20.0, 28.0)), head: [((14.0, 35.0), (20.0, 27.0)), ((26.0, 35.0), (20.0, 27.0))], half_width: 3.0 },
    Arrow { shaft: ((44.0, 25.0), (44.0, 46.0)), head: [((38.0, 39.0), (44.0, 47.0)), ((50.0, 39.0), (44.0, 47.0))], half_width: 3.0 },
];

/// Colour of the topmost shape at `(x, y)` (64 × 64 grid), or `None` outside
/// the icon. `small`: the simpler drawing used at 24 px and below.
fn paint(x: f64, y: f64, small: bool) -> Option<[u8; 3]> {
    if !in_rounded_rect(x, y, (2.0, 2.0, 62.0, 62.0), 13.0) {
        return None;
    }
    let arrows = if small { &SMALL_ARROWS } else { &ARROWS };
    for a in arrows {
        let w = a.half_width;
        if in_stroke(x, y, a.shaft, w) || a.head.iter().any(|&s| in_stroke(x, y, s, w)) {
            return Some(ARROW);
        }
    }
    if in_rounded_rect(x, y, (10.0, 20.0, 54.0, 53.0), 3.5) {
        return Some(FOLDER);
    }
    // Tab on the left, then the back of the folder showing above the body.
    if in_rounded_rect(x, y, (10.0, 12.0, 26.0, 22.0), 3.0) || in_rounded_rect(x, y, (10.0, 16.0, 54.0, 24.0), 3.0) {
        return Some(TAB);
    }
    Some(BLUE)
}

/// RGBA pixels (unpremultiplied) of a `size` × `size` icon.
pub fn rgba(size: u32) -> Vec<u8> {
    let size = size.max(1);
    let step = 64.0 / f64::from(size * SAMPLES);
    let small = size <= 24;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            // Colour sums and number of samples inside the icon.
            let (mut rgb, mut covered) = ([0.0_f64; 3], 0.0_f64);
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = (f64::from(px * SAMPLES + sx) + 0.5) * step;
                    let y = (f64::from(py * SAMPLES + sy) + 0.5) * step;
                    if let Some(c) = paint(x, y, small) {
                        for (s, v) in rgb.iter_mut().zip(c) {
                            *s += f64::from(v);
                        }
                        covered += 1.0;
                    }
                }
            }
            for s in rgb {
                out.push(if covered > 0.0 { to_byte(s / covered / 255.0) } else { 0 });
            }
            out.push(to_byte(covered / f64::from(SAMPLES * SAMPLES)));
        }
    }
    out
}

/// The icon as a Windows `.ico` file holding one 32-bit image per size (each at most
/// 256): the executables, the installer and the notification area use it.
pub fn ico(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<(u32, Vec<u8>)> = sizes.iter().map(|&s| s.clamp(1, 256)).map(|s| (s, bmp(s))).collect();
    let count = u16::try_from(images.len()).unwrap_or(u16::MAX);
    let mut out = Vec::new();
    // ICONDIR: reserved, type 1 (icon), image count.
    for v in [0u16, 1, count] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    // One 16-byte ICONDIRENTRY per image, then the images.
    let mut offset = 6 + 16 * u32::from(count);
    for (size, data) in &images {
        // 256 is written as 0.
        let side = u8::try_from(*size).unwrap_or(0);
        out.extend_from_slice(&[side, side, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        let len = u32::try_from(data.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset = offset.saturating_add(len);
    }
    for (_, data) in images {
        out.extend_from_slice(&data);
    }
    out
}

/// One icon image: BITMAPINFOHEADER, BGRA pixels bottom-up, empty AND mask.
fn bmp(size: u32) -> Vec<u8> {
    let px = rgba(size);
    // Rows of the AND mask are padded to 4 bytes; it stays empty (alpha is used).
    let mask_row = size.div_ceil(32) * 4;
    let mut out = Vec::with_capacity((40 + size * size * 4 + mask_row * size) as usize);
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    // The height counts the colour image and the mask.
    out.extend_from_slice(&(size * 2).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]); // no compression; sizes and palette unused
    for row in px.chunks_exact(size as usize * 4).rev() {
        for &[r, g, b, a] in row.as_chunks::<4>().0 {
            out.extend_from_slice(&[b, g, r, a]);
        }
    }
    out.resize(out.len() + (mask_row * size) as usize, 0);
    out
}

/// `true` inside the rectangle `(x0, y0, x1, y1)` with corners rounded by `r`.
fn in_rounded_rect(x: f64, y: f64, (x0, y0, x1, y1): (f64, f64, f64, f64), r: f64) -> bool {
    if x < x0 || x > x1 || y < y0 || y > y1 {
        return false;
    }
    let cx = x.clamp(x0 + r, x1 - r);
    let cy = y.clamp(y0 + r, y1 - r);
    (x - cx).hypot(y - cy) <= r
}

/// `true` within `half_width` of the segment `a`–`b` (a stroke with round ends).
fn in_stroke(x: f64, y: f64, ((ax, ay), (bx, by)): ((f64, f64), (f64, f64)), half_width: f64) -> bool {
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((x - ax) * dx + (y - ay) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    (x - (ax + t * dx)).hypot(y - (ay + t * dy)) <= half_width
}

/// Converts 0–1 to 0–255, saturating.
fn to_byte(x: f64) -> u8 {
    // The value is clamped to 0..=255 first, so the conversion cannot truncate.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let byte = (x * 255.0).round().clamp(0.0, 255.0) as u8;
    byte
}

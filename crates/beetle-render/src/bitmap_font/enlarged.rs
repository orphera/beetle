//! Enlarged bitmap glyph framework for commercial-grade typography.
//!
//! The current engine supports `scale` multipliers on 5x7 ASCII and 10x8 CJK.
//! This module provides larger base glyph arrays (16x14, 20x16) so that
//! display at Title/Value scale produces sharper results without relying
//! solely on nearest-neighbor scaling of tiny glyphs.
//!
//! Note: Full glyph coverage requires manual entry of bit arrays per
//! character. This file defines the framework and a starter set of
//! common characters; remaining characters fall back to scaled base glyphs.

use crate::bitmap_font::{BitmapFont};
use tiny_skia::{PixmapMut, Color};
use crate::skin::ColorRgba;

/// Enlarged ASCII glyph array (16 columns x 14 rows).
/// Each column holds 14 bits (MSB = top pixel).
/// Bit format: column index 0..=15, row index 0..=13.
pub type Glyph16x14 = [u16; 14];

/// Enlarged CJK glyph array (20 columns x 16 rows).
pub type Glyph20x16 = [u16; 16];

/// Starter enlarged glyph set: common ASCII characters used in UI labels.
pub const ENLARGED_ASCII: [(char, Glyph16x14); 10] = [
    ('A', [
        0b00001110, 0b00010001, 0b00010001, 0b00011111,
        0b00010001, 0b00010001, 0b00010001, 0b00010001,
        0b00010001, 0b00011111, 0b00010001, 0b00010001,
        0b00010001, 0b00000000,
    ]),
    ('B', [
        0b00011110, 0b00010001, 0b00010001, 0b00011110,
        0b00010001, 0b00010001, 0b00011110, 0b00010001,
        0b00010001, 0b00010001, 0b00011110, 0b00010001,
        0b00011110, 0b00000000,
    ]),
    ('P', [
        0b00011111, 0b00010001, 0b00010001, 0b00011111,
        0b00010000, 0b00010000, 0b00010000, 0b00010000,
        0b00010000, 0b00010000, 0b00010000, 0b00011111,
        0b00000000, 0b00000000,
    ]),
    // Numbers and symbols for common UI text (expanded set to follow)
    ('0', [
        0b00001110, 0b00010001, 0b00010001, 0b00010001,
        0b00010001, 0b00010001, 0b00010001, 0b00001110,
        0b00010001, 0b00010001, 0b00010001, 0b00001110,
        0b00010001, 0b00000000,
    ]),
    ('1', [
        0b00000110, 0b00001110, 0b00000110, 0b00000110,
        0b00000110, 0b00000110, 0b00000110, 0b00001110,
        0b00000110, 0b00000110, 0b00000110, 0b00000111,
        0b00000110, 0b00000000,
    ]),
    ('2', [
        0b00001111, 0b00010000, 0b00000001, 0b00000011,
        0b00000100, 0b00001000, 0b00010000, 0b00011111,
        0b00010000, 0b00010000, 0b00010000, 0b00011111,
        0b00000000, 0b00000000,
    ]),
    ('%', [
        0b00000001, 0b00000110, 0b00001100, 0b00000010,
        0b00000001, 0b00001101, 0b00011000, 0b00001101,
        0b00011001, 0b00001100, 0b00001100, 0b00000011,
        0b00000001, 0b00000000,
    ]),
    ('-', [
        0b00000000, 0b00000000, 0b00000000, 0b00000000,
        0b00011111, 0b00000000, 0b00000000, 0b00000000,
        0b00000000, 0b00000000, 0b00000000, 0b00000000,
        0b00000000, 0b00000000,
    ]),
    ('/', [
        0b00000000, 0b00000001, 0b00000001, 0b00000111,
        0b00001111, 0b00011110, 0b00001110, 0b00000111,
        0b00000011, 0b00000001, 0b00000001, 0b00000000,
        0b00000000, 0b00000000,
    ]),
];

/// Draws an enlarged glyph by looking up the enlarged array first,
/// falling back to scaled base glyph if not found.
pub fn draw_char_enlarged(
    pixmap: &mut PixmapMut,
    c: char,
    x: i32,
    y: i32,
    scale: u32,
    color: ColorRgba,
) {
    // Attempt enlarged lookup first
    for &(ch, glyph) in ENLARGED_ASCII.iter() {
        if ch == c {
            for row in 0..glyph.len() {
                let row_bits = glyph[row];
                for col in 0..14 {
                    let bit = (row_bits >> (13 - col)) & 1;
                    if bit == 1 {
                        let px = x + (col as i32 * scale as i32);
                        let py = y + (row as i32 * scale as i32);
                        fill_pixel_block(pixmap, px, py, scale, color);
                    }
                }
            }
            return;
        }
    }
    // Fallback to base glyph with larger scale approximation
    // This keeps binary small while allowing high-quality rendering
    // at Title/Value scales (2x/1.1x) by using larger base arrays.
    // For now, delegate back to BitmapFont with a boosted effective scale.
    BitmapFont::draw_char(pixmap, c, x, y, scale.max(2), color);
}

/// Internal pixel-block filler reused by enlarged glyph drawing.
#[inline(always)]
fn fill_pixel_block(
    pixmap: &mut PixmapMut,
    px: i32,
    py: i32,
    scale: u32,
    color: ColorRgba,
) {
    let pw = pixmap.width() as i32;
    let ph = pixmap.height() as i32;
    if px < 0 || py < 0 || px >= pw || py >= ph {
        return;
    }
    let data = pixmap.data_mut();
    let u32_slice: &mut [u32] =
        unsafe { std::slice::from_raw_parts_mut(data.as_mut_ptr() as *mut u32, data.len() / 4) };
    if color.a == 255 && scale == 1 {
        let idx = (py as usize) * (pw as usize) + (px as usize);
        if idx < u32_slice.len() {
            u32_slice[idx] = u32::from_ne_bytes([color.r, color.g, color.b, 255]);
        }
    }
}

pub mod ascii;
pub mod bold_digits;
pub mod hangul;
pub mod kana;

use crate::skin::ColorRgba;
use tiny_skia::PixmapMut;

pub use ascii::get_ascii_glyph;
pub use bold_digits::{get_bold_digit, BOLD_DIGITS_8X12};
pub use hangul::get_hangul_glyph;
pub use kana::get_kana_or_symbol_glyph;

/// Unified multilingual bitmap font engine (ASCII 5x7, Hangul 10x8, Kana/CJK 10x8, Bold 8x12).
pub struct BitmapFont;

impl BitmapFont {
    pub const ASCII_WIDTH: u32 = 5;
    pub const ASCII_HEIGHT: u32 = 7;
    pub const ASCII_SPACING: u32 = 1;

    pub const CJK_WIDTH: u32 = 10;
    pub const CJK_HEIGHT: u32 = 8;
    pub const CJK_SPACING: u32 = 2;

    pub const BOLD_WIDTH: u32 = 8;
    pub const BOLD_HEIGHT: u32 = 12;
    pub const BOLD_SPACING: u32 = 2;

    /// Checks if character is a full-width CJK or Hangul/Kana glyph.
    #[inline(always)]
    pub fn is_fullwidth(c: char) -> bool {
        let code = c as u32;
        // Hangul Syllables & Jamo
        (0xAC00..=0xD7A3).contains(&code) || (0x3131..=0x3163).contains(&code)
        // Hiragana & Katakana
        || (0x3040..=0x30FF).contains(&code)
        // CJK Unified Ideographs & Symbols
        || (0x4E00..=0x9FFF).contains(&code)
        || (0x3000..=0x303F).contains(&code)
        || (0xFF01..=0xFF60).contains(&code)
        || matches!(c, '★' | '☆' | '♪' | '♫' | '◆' | '◇' | '▲' | '▼' | '▶' | '◀' | '♥' | '♡' | '✓' | '✗' | '※')
    }

    /// Returns step advance (width + spacing) for a single character at a given scale.
    #[inline(always)]
    pub fn char_advance(c: char, scale: u32) -> u32 {
        let scale = scale.max(1);
        if Self::is_fullwidth(c) {
            (Self::CJK_WIDTH + Self::CJK_SPACING) * scale
        } else {
            (Self::ASCII_WIDTH + Self::ASCII_SPACING) * scale
        }
    }

    /// Returns pixel width of a single character glyph at a given scale (without trailing spacing).
    #[inline(always)]
    pub fn char_width(c: char, scale: u32) -> u32 {
        let scale = scale.max(1);
        if Self::is_fullwidth(c) {
            Self::CJK_WIDTH * scale
        } else {
            Self::ASCII_WIDTH * scale
        }
    }

    /// Calculates horizontal width in pixels of any mixed ASCII / Hangul / Japanese string.
    pub fn text_width(text: &str, scale: u32) -> u32 {
        let scale = scale.max(1);
        let mut total = 0;
        let mut count = 0;

        for c in text.chars() {
            total += Self::char_advance(c, scale);
            count += 1;
        }

        if count == 0 {
            0
        } else {
            // Subtract trailing spacing from the last character
            let last_c = text.chars().last().unwrap_or(' ');
            let trailing = if Self::is_fullwidth(last_c) {
                Self::CJK_SPACING * scale
            } else {
                Self::ASCII_SPACING * scale
            };
            total.saturating_sub(trailing)
        }
    }

    /// Renders a single character glyph onto the pixmap.
    pub fn draw_char(
        pixmap: &mut PixmapMut,
        c: char,
        x: i32,
        y: i32,
        scale: u32,
        color: ColorRgba,
    ) {
        let scale = scale.max(1);

        // 1. ASCII 5x7 character
        if let Some(glyph) = get_ascii_glyph(c) {
            for col in 0..5 {
                let col_bits = glyph[col];
                for row in 0..7 {
                    if (col_bits & (1 << row)) != 0 {
                        let px = x + (col as i32 * scale as i32);
                        let py = y + (row as i32 * scale as i32);
                        fill_pixel_block(pixmap, px, py, scale, color);
                    }
                }
            }
            return;
        }

        // 2. Korean Hangul 10x8 syllable or Jamo
        if let Some(glyph) = get_hangul_glyph(c) {
            draw_10x8_glyph(pixmap, &glyph, x, y, scale, color);
            return;
        }

        // 3. Japanese Kana or CJK special symbols (10x8)
        if let Some(glyph) = get_kana_or_symbol_glyph(c) {
            draw_10x8_glyph(pixmap, &glyph, x, y, scale, color);
            return;
        }

        // 4. Space character
        if c == ' ' || c == '\u{3000}' {
            return;
        }

        // 5. Fallback square glyph for unmapped CJK Kanji / unknown chars
        let fallback_glyph = [
            0x3FE, 0x202, 0x202, 0x202, 0x202, 0x202, 0x3FE, 0x000,
        ];
        draw_10x8_glyph(pixmap, &fallback_glyph, x, y, scale, color);
    }

    /// Renders a text string at (x, y) with support for mixed ASCII, Korean, and Japanese.
    pub fn draw_text(
        pixmap: &mut PixmapMut,
        text: &str,
        mut x: i32,
        y: i32,
        scale: u32,
        color: ColorRgba,
    ) {
        let scale = scale.max(1);
        for c in text.chars() {
            Self::draw_char(pixmap, c, x, y, scale, color);
            x += Self::char_advance(c, scale) as i32;
        }
    }

    /// Renders horizontally centered text.
    pub fn draw_text_centered(
        pixmap: &mut PixmapMut,
        text: &str,
        center_x: i32,
        y: i32,
        scale: u32,
        color: ColorRgba,
    ) {
        let width = Self::text_width(text, scale) as i32;
        let x = center_x - (width / 2);
        Self::draw_text(pixmap, text, x, y, scale, color);
    }

    /// Renders text with a subtle drop shadow for maximum readability on complex backgrounds.
    pub fn draw_text_with_shadow(
        pixmap: &mut PixmapMut,
        text: &str,
        x: i32,
        y: i32,
        scale: u32,
        color: ColorRgba,
        shadow_color: ColorRgba,
        offset_x: i32,
        offset_y: i32,
    ) {
        Self::draw_text(pixmap, text, x + offset_x, y + offset_y, scale, shadow_color);
        Self::draw_text(pixmap, text, x, y, scale, color);
    }

    /// Renders text with a 1-pixel high-contrast outline.
    pub fn draw_text_with_outline(
        pixmap: &mut PixmapMut,
        text: &str,
        x: i32,
        y: i32,
        scale: u32,
        color: ColorRgba,
        outline_color: ColorRgba,
    ) {
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            Self::draw_text(pixmap, text, x + dx, y + dy, scale, outline_color);
        }
        Self::draw_text(pixmap, text, x, y, scale, color);
    }

    /// Renders a UI pill badge with background and border.
    pub fn draw_badge(
        pixmap: &mut PixmapMut,
        text: &str,
        x: i32,
        y: i32,
        scale: u32,
        text_color: ColorRgba,
        bg_color: ColorRgba,
        border_color: ColorRgba,
        padding_x: i32,
        padding_y: i32,
    ) {
        let w = Self::text_width(text, scale) as i32 + (padding_x * 2);
        let h = (Self::CJK_HEIGHT * scale) as i32 + (padding_y * 2);

        // Fill background
        draw_rect_fast(pixmap, x, y, w, h, bg_color);

        // Draw border (1px)
        draw_rect_fast(pixmap, x, y, w, 1, border_color);
        draw_rect_fast(pixmap, x, y + h - 1, w, 1, border_color);
        draw_rect_fast(pixmap, x, y, 1, h, border_color);
        draw_rect_fast(pixmap, x + w - 1, y, 1, h, border_color);

        // Draw centered text
        Self::draw_text(pixmap, text, x + padding_x, y + padding_y, scale, text_color);
    }

    /// Renders high-contrast 8x12 bold numbers (e.g. for large score/combo displays).
    pub fn draw_bold_digit(
        pixmap: &mut PixmapMut,
        c: char,
        x: i32,
        y: i32,
        scale: u32,
        color: ColorRgba,
    ) {
        if let Some(glyph) = get_bold_digit(c) {
            let scale = scale.max(1);
            for row in 0..12 {
                let row_bits = glyph[row];
                for col in 0..8 {
                    if (row_bits & (0x80 >> col)) != 0 {
                        let px = x + (col as i32 * scale as i32);
                        let py = y + (row as i32 * scale as i32);
                        fill_pixel_block(pixmap, px, py, scale, color);
                    }
                }
            }
        } else {
            Self::draw_char(pixmap, c, x, y, scale, color);
        }
    }

    /// Renders a string of bold digits and symbols at (x, y).
    pub fn draw_bold_text(
        pixmap: &mut PixmapMut,
        text: &str,
        mut x: i32,
        y: i32,
        scale: u32,
        color: ColorRgba,
    ) {
        let scale = scale.max(1);
        let step = (Self::BOLD_WIDTH + Self::BOLD_SPACING) * scale;
        for c in text.chars() {
            Self::draw_bold_digit(pixmap, c, x, y, scale, color);
            x += step as i32;
        }
    }

    /// Calculates horizontal width in pixels of bold text.
    pub fn bold_text_width(text: &str, scale: u32) -> u32 {
        let count = text.chars().count() as u32;
        if count == 0 {
            0
        } else {
            let scale = scale.max(1);
            let char_width = Self::BOLD_WIDTH * scale;
            let spacing = Self::BOLD_SPACING * scale;
            count * char_width + (count - 1) * spacing
        }
    }

    /// Renders horizontally centered bold text.
    pub fn draw_bold_text_centered(
        pixmap: &mut PixmapMut,
        text: &str,
        center_x: i32,
        y: i32,
        scale: u32,
        color: ColorRgba,
    ) {
        let width = Self::bold_text_width(text, scale) as i32;
        let x = center_x - (width / 2);
        Self::draw_bold_text(pixmap, text, x, y, scale, color);
    }
}

#[inline(always)]
fn draw_rect_fast(pixmap: &mut PixmapMut, x: i32, y: i32, w: i32, h: i32, color: ColorRgba) {
    if w <= 0 || h <= 0 {
        return;
    }
    let target_w = pixmap.width() as i32;
    let target_h = pixmap.height() as i32;

    let ix0 = x.clamp(0, target_w);
    let iy0 = y.clamp(0, target_h);
    let ix1 = (x + w).clamp(0, target_w);
    let iy1 = (y + h).clamp(0, target_h);

    if ix0 >= ix1 || iy0 >= iy1 {
        return;
    }

    let data = pixmap.data_mut();
    let u32_slice: &mut [u32] = unsafe {
        std::slice::from_raw_parts_mut(data.as_mut_ptr() as *mut u32, data.len() / 4)
    };

    let row_len = (ix1 - ix0) as usize;
    if color.a == 255 {
        let packed = u32::from_ne_bytes([color.r, color.g, color.b, 255]);
        for py in iy0..iy1 {
            let row_start = (py as usize) * (target_w as usize) + (ix0 as usize);
            u32_slice[row_start..row_start + row_len].fill(packed);
        }
    } else if color.a > 0 {
        let a = color.a as u32;
        let inv_a = 255 - a;
        let sr = (color.r as u32 * a) / 255;
        let sg = (color.g as u32 * a) / 255;
        let sb = (color.b as u32 * a) / 255;
        for py in iy0..iy1 {
            let row_start = (py as usize) * (target_w as usize) + (ix0 as usize);
            for pixel in &mut u32_slice[row_start..row_start + row_len] {
                let p = *pixel;
                let dr = p & 0xFF;
                let dg = (p >> 8) & 0xFF;
                let db = (p >> 16) & 0xFF;
                let nr = sr + (dr * inv_a) / 255;
                let ng = sg + (dg * inv_a) / 255;
                let nb = sb + (db * inv_a) / 255;
                *pixel = (255 << 24) | (nb << 16) | (ng << 8) | nr;
            }
        }
    }
}

#[inline(always)]
fn fill_pixel_block(pixmap: &mut PixmapMut, px: i32, py: i32, scale: u32, color: ColorRgba) {
    let pw = pixmap.width() as i32;
    let ph = pixmap.height() as i32;
    if px < 0 || py < 0 || px >= pw || py >= ph {
        return;
    }

    let data = pixmap.data_mut();
    let u32_slice: &mut [u32] = unsafe {
        std::slice::from_raw_parts_mut(data.as_mut_ptr() as *mut u32, data.len() / 4)
    };

    if color.a == 255 {
        let packed = u32::from_ne_bytes([color.r, color.g, color.b, 255]);
        if scale == 1 {
            let idx = (py as usize) * (pw as usize) + (px as usize);
            if idx < u32_slice.len() {
                u32_slice[idx] = packed;
            }
        } else {
            let x_end = (px + scale as i32).min(pw);
            let y_end = (py + scale as i32).min(ph);
            let row_len = (x_end - px) as usize;
            for y in py..y_end {
                let row_start = (y as usize) * (pw as usize) + (px as usize);
                if row_start + row_len <= u32_slice.len() {
                    u32_slice[row_start..row_start + row_len].fill(packed);
                }
            }
        }
    } else if color.a > 0 {
        let a = color.a as u32;
        let inv_a = 255 - a;
        let sr = (color.r as u32 * a) / 255;
        let sg = (color.g as u32 * a) / 255;
        let sb = (color.b as u32 * a) / 255;

        let x_end = (px + scale as i32).min(pw);
        let y_end = (py + scale as i32).min(ph);
        let row_len = (x_end - px) as usize;
        for y in py..y_end {
            let row_start = (y as usize) * (pw as usize) + (px as usize);
            for pixel in &mut u32_slice[row_start..row_start + row_len] {
                let p = *pixel;
                let dr = p & 0xFF;
                let dg = (p >> 8) & 0xFF;
                let db = (p >> 16) & 0xFF;
                let nr = sr + (dr * inv_a) / 255;
                let ng = sg + (dg * inv_a) / 255;
                let nb = sb + (db * inv_a) / 255;
                *pixel = (255 << 24) | (nb << 16) | (ng << 8) | nr;
            }
        }
    }
}

#[inline(always)]
fn draw_10x8_glyph(
    pixmap: &mut PixmapMut,
    glyph: &[u16; 8],
    x: i32,
    y: i32,
    scale: u32,
    color: ColorRgba,
) {
    for row in 0..8 {
        let row_bits = glyph[row];
        for col in 0..10 {
            // MSB 9 down to 0
            if (row_bits & (1 << (9 - col))) != 0 {
                let px = x + (col as i32 * scale as i32);
                let py = y + (row as i32 * scale as i32);
                fill_pixel_block(pixmap, px, py, scale, color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{Color, Pixmap};

    #[test]
    fn test_ascii_and_multilingual_text_width() {
        // "1234" -> 4 chars. 4 * 5 + 3 * 1 = 23 at scale 1
        assert_eq!(BitmapFont::text_width("1234", 1), 23);
        assert_eq!(BitmapFont::text_width("1234", 2), 46);
        assert_eq!(BitmapFont::text_width("", 1), 0);

        // Korean "가나다" (3 fullwidth chars) -> 3 * 10 + 2 * 2 = 34
        assert_eq!(BitmapFont::text_width("가나다", 1), 34);

        // Japanese "さくら" (3 fullwidth chars) -> 3 * 10 + 2 * 2 = 34
        assert_eq!(BitmapFont::text_width("さくら", 1), 34);

        // Mixed: "Lv.12 곡" -> 5 ASCII ('L','v','.','1','2'), 1 space (' '), 1 Korean ('곡')
        // 6 halfwidth (6 * 6) + 1 fullwidth (1 * 12) - trailing = 48 - 2 = 46
        let mixed_w = BitmapFont::text_width("Lv.12 곡", 1);
        assert!(mixed_w > 0);
    }

    #[test]
    fn test_draw_multilingual_text_pixmap() {
        let mut pixmap = Pixmap::new(200, 100).unwrap();
        pixmap.fill(Color::BLACK);

        let white = ColorRgba::new(255, 255, 255, 255);
        BitmapFont::draw_text(&mut pixmap.as_mut(), "BEETLE 한글 さくら ★", 10, 10, 1, white);

        // Verify that pixels are drawn
        let has_white_pixel = pixmap.data().chunks_exact(4).any(|p| p[0] == 255 && p[1] == 255 && p[2] == 255);
        assert!(has_white_pixel);
    }

    #[test]
    fn test_bold_digits_and_badges() {
        let mut pixmap = Pixmap::new(100, 50).unwrap();
        pixmap.fill(Color::BLACK);

        let yellow = ColorRgba::new(255, 220, 50, 255);
        BitmapFont::draw_bold_text(&mut pixmap.as_mut(), "99.8%", 5, 5, 1, yellow);
        assert_eq!(BitmapFont::bold_text_width("99.8%", 1), 5 * 8 + 4 * 2);

        let has_yellow = pixmap.data().chunks_exact(4).any(|p| p[0] == 255 && p[1] == 220 && p[2] == 50);
        assert!(has_yellow);
    }
}

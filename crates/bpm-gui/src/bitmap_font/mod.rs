pub mod ascii;
#[cfg(target_os = "windows")]
pub mod gdi_fallback;
pub mod hangul;
pub mod kana;
pub mod truetype_font;

use beetle_render::ColorRgba;
use tiny_skia::PixmapMut;

pub use ascii::get_ascii_glyph;
pub use hangul::get_hangul_glyph;
pub use kana::get_kana_or_symbol_glyph;

/// U+00B7, the separator the window puts between facts.
const MIDDLE_DOT: char = '\u{00B7}';

/// Text engine for bpm-gui: proportional, antialiased TrueType text at a
/// real pixel size, with the hand-drawn bitmap tables as a last resort.
pub struct BitmapFont;

impl BitmapFont {
    /// Em size in px for one unit of `scale`. Scale 1 is 13px (cap height
    /// ~9px), the smallest size where 1px Latin strokes and Hangul stay
    /// legible; scale 2 is the header title size.
    pub const PX_PER_SCALE: u32 = 13;

    /// Cell sizes used only by the hand-drawn bitmap fallback.
    pub const ASCII_WIDTH: u32 = 5;
    pub const ASCII_SPACING: u32 = 1;

    pub const CJK_WIDTH: u32 = 10;
    pub const CJK_HEIGHT: u32 = 8;
    pub const CJK_SPACING: u32 = 2;

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

    /// Em size in px for a scale factor.
    #[inline(always)]
    fn em_px(scale: u32) -> u16 {
        (Self::PX_PER_SCALE * scale.max(1)).min(u16::MAX as u32) as u16
    }

    /// Returns the horizontal advance in px for a single character at a given scale.
    pub fn char_advance(c: char, scale: u32) -> f32 {
        let scale = scale.max(1);
        if let Some(advance) = truetype_font::advance(c, Self::em_px(scale), false) {
            return advance;
        }
        // Characters outside the embedded fonts keep the old cell width.
        if Self::is_fullwidth(c) {
            ((Self::CJK_WIDTH + Self::CJK_SPACING) * scale) as f32
        } else {
            ((Self::ASCII_WIDTH + Self::ASCII_SPACING) * scale) as f32
        }
    }

    /// Calculates horizontal width in pixels of any mixed ASCII / Hangul / Japanese string.
    #[allow(dead_code)] // whole-scale API: the window draws with `draw_text_px`
    pub fn text_width(text: &str, scale: u32) -> f32 {
        text.chars().map(|c| Self::char_advance(c, scale)).sum()
    }

    /// Renders a single character with its top (cap-height line) at `y`.
    ///
    /// Tries the embedded TrueType fonts first (real antialiased glyphs for
    /// Latin, common-use Hangul, and joyo-kanji+kana Japanese). Falls
    /// through to the hand bitmap tables only for characters outside the
    /// embedded fonts' subsetted coverage (rare Hanja, obscure Hangul,
    /// symbols) — those stay hand-drawn or go to the Windows GDI fallback.
    #[allow(dead_code)] // whole-scale API: the window draws with `draw_text_px`
    pub fn draw_char(
        pixmap: &mut PixmapMut,
        c: char,
        x: i32,
        y: i32,
        scale: u32,
        color: ColorRgba,
    ) {
        Self::draw_char_weighted(pixmap, c, x, y, scale, color, false);
    }

    /// Same as `draw_char`, but prefers the embedded TrueType **bold**
    /// Latin face when the character is ASCII. Fullwidth script (Hangul/
    /// Kana/CJK) is unaffected since those embedded fonts only ship one
    /// weight.
    pub fn draw_char_weighted(
        pixmap: &mut PixmapMut,
        c: char,
        x: i32,
        y: i32,
        scale: u32,
        color: ColorRgba,
        bold: bool,
    ) {
        let scale = scale.max(1);

        if c == ' ' || c == '\u{3000}' {
            return;
        }

        let px = Self::em_px(scale);
        let baseline = y + truetype_font::cap_height(px);
        if truetype_font::draw_char(pixmap, c, x, baseline, px, bold, color) {
            return;
        }

        // --- Fallback: hand bitmap tables for characters outside the
        // embedded fonts' subsetted coverage ---

        // 1. ASCII 5x7 character (synthetic-bold dilated)
        if let Some(glyph) = get_ascii_glyph(c) {
            let mut prev_bits = 0u8;
            for col in 0..5 {
                let col_bits = glyph[col] | prev_bits;
                prev_bits = glyph[col];
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

        // 4. Runtime GDI glyph cache fallback for rare CJK Kanji (Windows only)
        #[cfg(target_os = "windows")]
        if gdi_fallback::draw_char_fallback(pixmap, c, x, y, scale, color) {
            return;
        }

        // 5. Fallback square glyph for unmapped characters
        let fallback_glyph = [0x3FE, 0x202, 0x202, 0x202, 0x202, 0x202, 0x3FE, 0x000];
        draw_10x8_glyph(pixmap, &fallback_glyph, x, y, scale, color);
    }

    /// Renders a text string with its cap-height line at `y`, mixing ASCII,
    /// Korean and Japanese with each glyph's own advance.
    #[allow(dead_code)] // whole-scale API: the window draws with `draw_text_px`
    pub fn draw_text(
        pixmap: &mut PixmapMut,
        text: &str,
        x: i32,
        y: i32,
        scale: u32,
        color: ColorRgba,
    ) {
        let scale = scale.max(1);
        let mut pen = x as f32;
        for c in text.chars() {
            Self::draw_char(pixmap, c, pen.round() as i32, y, scale, color);
            pen += Self::char_advance(c, scale);
        }
    }

    /// Width in px of `text` drawn at an em size of `px` (see `draw_text_px`).
    pub fn text_width_px(text: &str, px: u16, bold: bool) -> f32 {
        let scale = (px as u32 / Self::PX_PER_SCALE).max(1);
        text.chars()
            .map(|c| {
                let c = if c == MIDDLE_DOT { ' ' } else { c };
                truetype_font::advance(c, px, bold).unwrap_or_else(|| Self::char_advance(c, scale))
            })
            .sum()
    }

    /// Renders `text` at any em size `px` with its cap-height line at `y`.
    /// Characters outside the embedded fonts fall back to the nearest whole
    /// bitmap scale.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_px(
        pixmap: &mut PixmapMut,
        text: &str,
        x: f32,
        y: f32,
        px: u16,
        bold: bool,
        color: ColorRgba,
    ) {
        let scale = (px as u32 / Self::PX_PER_SCALE).max(1);
        let top = y.round() as i32;
        let baseline = top + truetype_font::cap_height(px);
        let mut pen = x;
        for c in text.chars() {
            if c == MIDDLE_DOT {
                // The Latin subset has no U+00B7: a dot centered in a space.
                let advance = truetype_font::advance(' ', px, bold).unwrap_or(px as f32 * 0.6);
                let size = if px >= 15 { 3 } else { 2 };
                let dot_x = (pen + advance / 2.0).round() as i32 - size as i32 / 2;
                let dot_y = top + truetype_font::cap_height(px) / 2 - size as i32 / 2;
                fill_pixel_block(pixmap, dot_x, dot_y, size, color);
                pen += advance;
                continue;
            }
            let advance = truetype_font::advance(c, px, bold);
            if c != ' '
                && c != '\u{3000}'
                && !truetype_font::draw_char(
                    pixmap,
                    c,
                    pen.round() as i32,
                    baseline,
                    px,
                    bold,
                    color,
                )
            {
                Self::draw_char_weighted(pixmap, c, pen.round() as i32, top, scale, color, bold);
            }
            pen += advance.unwrap_or_else(|| Self::char_advance(c, scale));
        }
    }

    /// Renders horizontally centered text.
    #[allow(dead_code)] // whole-scale API: the window draws with `draw_text_px`
    pub fn draw_text_centered(
        pixmap: &mut PixmapMut,
        text: &str,
        center_x: i32,
        y: i32,
        scale: u32,
        color: ColorRgba,
    ) {
        let width = Self::text_width(text, scale);
        let x = center_x - (width / 2.0).round() as i32;
        Self::draw_text(pixmap, text, x, y, scale, color);
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
    let u32_slice: &mut [u32] =
        unsafe { std::slice::from_raw_parts_mut(data.as_mut_ptr() as *mut u32, data.len() / 4) };

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
        assert_eq!(BitmapFont::text_width("", 1), 0.0);

        // Latin and Hangul share one em grid, so Hangul is roughly one em
        // per glyph while Latin is narrower.
        let one = BitmapFont::text_width("1", 1);
        let four = BitmapFont::text_width("1234", 1);
        assert!(four > one * 3.0 && four < one * 5.0, "{one} {four}");
        assert!(BitmapFont::text_width("1234", 2) > BitmapFont::text_width("1234", 1) * 1.8);

        let hangul = BitmapFont::text_width("가나다", 1);
        assert!(hangul > 3.0 * 11.0 && hangul < 3.0 * 14.0, "{hangul}");
    }

    #[test]
    fn test_draw_multilingual_text_pixmap() {
        let mut pixmap = Pixmap::new(240, 100).unwrap();
        pixmap.fill(Color::BLACK);

        let white = ColorRgba::new(255, 255, 255, 255);
        BitmapFont::draw_text(
            &mut pixmap.as_mut(),
            "BEETLE 한글 さくら 桜 龍 ★",
            10,
            10,
            1,
            white,
        );

        // Verify that pixels are drawn
        let has_white_pixel = pixmap
            .data()
            .chunks_exact(4)
            .any(|p| p[0] == 255 && p[1] == 255 && p[2] == 255);
        assert!(has_white_pixel);

        // Most of this string (Latin, common Hangul/Kana, joyo kanji) is
        // rendered by the embedded TrueType fonts (truetype_font.rs), not
        // the GDI system-font fallback. GDI is only still reached for
        // characters outside those subsets — here, '龍' is not in the
        // joyo-kanji list (its joyo equivalent is the simplified '竜'), so
        // exactly one GDI cache entry is expected.
        #[cfg(target_os = "windows")]
        {
            assert!(gdi_fallback::cache_len() >= 1);
        }
    }

    #[test]
    fn test_draw_fallback_square_for_unmapped_character() {
        let mut pixmap = Pixmap::new(50, 50).unwrap();
        pixmap.fill(Color::BLACK);

        let white = ColorRgba::new(255, 255, 255, 255);
        // Null character has no printable ASCII, Hangul, or Kana glyph,
        // and GDI fallback returns None for it. It falls through to step 5 (square glyph).
        BitmapFont::draw_char(&mut pixmap.as_mut(), '\0', 10, 10, 1, white);

        let white_pixels = pixmap
            .data()
            .chunks_exact(4)
            .filter(|p| p[0] == 255 && p[1] == 255 && p[2] == 255)
            .count();
        // The fallback square glyph has exactly 28 pixels (9 + 2*5 + 9)
        assert_eq!(white_pixels, 28);
    }
}

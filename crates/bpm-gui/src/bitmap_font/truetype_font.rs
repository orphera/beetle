//! Cross-platform TrueType glyph rendering via `fontdue` (pure Rust, no
//! GPU/shader dependency — see AGENTS.md §3 for the scoped dependency
//! exception and docs/plans/2026-10-03-pulse-redesign.md for why this
//! replaced the two-tier hand-bitmap system).
//!
//! Three embedded, pre-subsetted font files cover the UI's actual script
//! range (not full Unicode — see the asset README for the subsetting
//! recipe that keeps these small):
//!   - JetBrains Mono (Latin, OFL-1.1): Basic Latin only, Regular + Bold.
//!   - Noto Sans JP (joyo-kanji + kana, OFL-1.1): the ~2137 Japanese
//!     "common use" kanji plus hiragana/katakana/CJK punctuation.
//!   - Noto Sans KR (OFL-1.1): the 2,350 KS X 1001 "common use" Hangul
//!     syllables plus compatibility jamo.
//!
//! Every glyph is rasterized then resampled to fit *within* the existing
//! fixed-width bitmap-font cell grid (`BitmapFont::ASCII_WIDTH/HEIGHT` for
//! Latin, `CJK_WIDTH/HEIGHT` for fullwidth script) instead of using the
//! font's own proportional advance metrics. This is deliberate: an earlier
//! attempt that let a real font's natural glyph width flow into the
//! existing fixed-advance layout (GDI's MS Gothic covering ASCII too)
//! caused glyphs to overflow their cell and overlap neighboring
//! characters, because "half-width" Latin in a CJK font is proportioned
//! for a CJK monospace grid, not our narrow ASCII grid. Clamping to
//! whichever of width/height is the limiting dimension and centering the
//! other avoids that class of bug entirely, for any embedded font.

use beetle_render::ColorRgba;
use fontdue::{Font, FontSettings};
use std::cell::RefCell;
use std::collections::HashMap;
use tiny_skia::PixmapMut;

const LATIN_REGULAR_BYTES: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");
const LATIN_BOLD_BYTES: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Bold.ttf");
// Shared with the new text engine so the font data is embedded only once.
const JAPANESE_BYTES: &[u8] = beetle_render::text::JP_BYTES;
const KOREAN_BYTES: &[u8] = beetle_render::text::KR_BYTES;

/// Fixed rasterization size in px. Quality only (final size is resampled to
/// the target cell), so this just needs to be large enough to avoid
/// hinting/rounding artifacts at the smallest practical downsample.
const RASTER_PX: f32 = 48.0;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum FontTag {
    LatinRegular,
    LatinBold,
    Japanese,
    Korean,
}

/// Picks which embedded font should render a given character.
fn font_for_char(c: char, bold: bool) -> FontTag {
    let code = c as u32;
    if (0xAC00..=0xD7A3).contains(&code) || (0x3131..=0x318F).contains(&code) {
        FontTag::Korean
    } else if (0x3000..=0x30FF).contains(&code)
        || (0x4E00..=0x9FFF).contains(&code)
        || (0xFF00..=0xFFEF).contains(&code)
    {
        FontTag::Japanese
    } else if bold {
        FontTag::LatinBold
    } else {
        FontTag::LatinRegular
    }
}

struct CachedGlyph {
    coverage: Vec<u8>,
    w: i32,
    h: i32,
    /// Top-left of the resampled glyph bitmap, relative to the cell origin.
    x_off: i32,
    y_off: i32,
}

struct GlyphCache {
    latin_regular: Font,
    latin_bold: Font,
    japanese: Font,
    korean: Font,
    // Keyed by (font, char, cell_w, cell_h) since the resample target
    // depends on both the caller's scale and whether this is an ASCII or
    // fullwidth cell.
    cache: HashMap<(FontTag, char, i32, i32), Option<CachedGlyph>>,
}

impl GlyphCache {
    fn new() -> Option<Self> {
        let settings = FontSettings::default();
        Some(Self {
            latin_regular: Font::from_bytes(LATIN_REGULAR_BYTES, settings).ok()?,
            latin_bold: Font::from_bytes(LATIN_BOLD_BYTES, settings).ok()?,
            japanese: Font::from_bytes(JAPANESE_BYTES, settings).ok()?,
            korean: Font::from_bytes(KOREAN_BYTES, settings).ok()?,
            cache: HashMap::new(),
        })
    }

    fn font(&self, tag: FontTag) -> &Font {
        match tag {
            FontTag::LatinRegular => &self.latin_regular,
            FontTag::LatinBold => &self.latin_bold,
            FontTag::Japanese => &self.japanese,
            FontTag::Korean => &self.korean,
        }
    }

    fn get_or_rasterize(
        &mut self,
        tag: FontTag,
        c: char,
        cell_w: i32,
        cell_h: i32,
    ) -> Option<&CachedGlyph> {
        let key = (tag, c, cell_w, cell_h);
        if self.cache.contains_key(&key) {
            return self.cache.get(&key).and_then(|g| g.as_ref());
        }

        let font = self.font(tag);
        if !font.has_glyph(c) {
            self.cache.insert(key, None);
            return None;
        }

        let (metrics, bitmap) = font.rasterize(c, RASTER_PX);
        if metrics.width == 0 || metrics.height == 0 {
            self.cache.insert(key, None);
            return None;
        }
        let line_metrics =
            font.horizontal_line_metrics(RASTER_PX)
                .unwrap_or(fontdue::LineMetrics {
                    ascent: RASTER_PX * 0.8,
                    descent: -RASTER_PX * 0.2,
                    line_gap: 0.0,
                    new_line_size: RASTER_PX,
                });
        let line_span = (line_metrics.ascent - line_metrics.descent).max(1.0);

        // Fit by line height first (keeps every glyph in a font visually
        // consistent in size), then clamp by width if this particular
        // glyph would overflow the cell (wide glyphs, e.g. 'M' or 'W').
        let mut resize = cell_h as f32 / line_span;
        let target_w_check = metrics.width as f32 * resize;
        if target_w_check > cell_w as f32 {
            resize = cell_w as f32 / metrics.width as f32;
        }

        // Baseline sits `ascent * resize` down from the cell top; the glyph
        // bitmap's top edge sits `(ymin + height)` above the baseline in
        // raster-px space.
        let baseline_y = line_metrics.ascent * resize;
        let glyph_top_above_baseline = (metrics.ymin as f32 + metrics.height as f32) * resize;
        let y_off = (baseline_y - glyph_top_above_baseline).round() as i32;
        let x_off = (metrics.xmin as f32 * resize).round() as i32;

        let gw = metrics.width;
        let gh = metrics.height;
        let glyph_w_px = (gw as f32 * resize).round().max(1.0) as i32;
        let glyph_h_px = (gh as f32 * resize).round().max(1.0) as i32;

        // Box-filter (area-average) downsample instead of nearest-neighbor
        // point sampling. At the extreme reduction ratios here (a ~48px
        // raster collapsed into a 5-10px cell, 5-8x downscale) point
        // sampling skips most of the glyph's actual "ink" pixels, landing
        // on faint antialiased edges more often than solid strokes — the
        // whole string reads as a washed-out ghost instead of legible
        // text. Averaging every source pixel that maps into each
        // destination pixel preserves the glyph's true coverage/weight at
        // any downscale ratio.
        let mut coverage = vec![0u8; (glyph_w_px * glyph_h_px) as usize];
        for ty in 0..glyph_h_px {
            let sy0 = ((ty as f32) / resize).floor().max(0.0) as usize;
            let sy1 = (((ty + 1) as f32) / resize).ceil().max(1.0) as usize;
            let sy1 = sy1.min(gh).max(sy0 + 1);
            for tx in 0..glyph_w_px {
                let sx0 = ((tx as f32) / resize).floor().max(0.0) as usize;
                let sx1 = (((tx + 1) as f32) / resize).ceil().max(1.0) as usize;
                let sx1 = sx1.min(gw).max(sx0 + 1);

                let mut sum: u32 = 0;
                let mut count: u32 = 0;
                for sy in sy0..sy1.min(gh) {
                    for sx in sx0..sx1.min(gw) {
                        sum += bitmap[sy * gw + sx] as u32;
                        count += 1;
                    }
                }
                if count > 0 {
                    coverage[(ty * glyph_w_px + tx) as usize] = (sum / count) as u8;
                }
            }
        }

        let glyph = CachedGlyph {
            coverage,
            w: glyph_w_px,
            h: glyph_h_px,
            x_off,
            y_off,
        };

        self.cache.insert(key, Some(glyph));
        self.cache.get(&key).and_then(|g| g.as_ref())
    }
}

thread_local! {
    static LOCAL_CACHE: RefCell<Option<GlyphCache>> = RefCell::new(None);
}

/// Draws a single character using the embedded TrueType fonts, fit within a
/// `cell_w` x `cell_h` box anchored at `(x, y)`. Returns `false` if no
/// embedded font has this glyph (caller should fall through to the hand
/// bitmap tables).
#[allow(clippy::too_many_arguments)]
pub fn draw_char_ttf(
    pixmap: &mut PixmapMut,
    c: char,
    x: i32,
    y: i32,
    cell_w: i32,
    cell_h: i32,
    bold: bool,
    color: ColorRgba,
) -> bool {
    if color.a == 0 || cell_w <= 0 || cell_h <= 0 {
        return true;
    }
    let tag = font_for_char(c, bold);

    LOCAL_CACHE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = GlyphCache::new();
        }
        let cache = match slot.as_mut() {
            Some(c) => c,
            None => return false,
        };

        let glyph = match cache.get_or_rasterize(tag, c, cell_w, cell_h) {
            Some(g) => g,
            None => return false,
        };

        let pw = pixmap.width() as i32;
        let ph = pixmap.height() as i32;
        let data = pixmap.data_mut();
        let u32_slice: &mut [u32] = unsafe {
            std::slice::from_raw_parts_mut(data.as_mut_ptr() as *mut u32, data.len() / 4)
        };

        let base_x = x + glyph.x_off;
        let base_y = y + glyph.y_off;

        for ty in 0..glyph.h {
            let py = base_y + ty;
            if py < 0 || py >= ph {
                continue;
            }
            for tx in 0..glyph.w {
                let px = base_x + tx;
                if px < 0 || px >= pw {
                    continue;
                }
                let cov = glyph.coverage[(ty * glyph.w + tx) as usize];
                if cov == 0 {
                    continue;
                }
                let effective_a = (color.a as u32 * cov as u32) / 255;
                if effective_a == 0 {
                    continue;
                }
                let idx = (py as usize) * (pw as usize) + (px as usize);
                if idx >= u32_slice.len() {
                    continue;
                }
                if effective_a == 255 {
                    u32_slice[idx] = u32::from_ne_bytes([color.r, color.g, color.b, 255]);
                    continue;
                }
                let p = u32_slice[idx];
                let dr = p & 0xFF;
                let dg = (p >> 8) & 0xFF;
                let db = (p >> 16) & 0xFF;
                let inv_a = 255 - effective_a;
                let nr = (color.r as u32 * effective_a) / 255 + (dr * inv_a) / 255;
                let ng = (color.g as u32 * effective_a) / 255 + (dg * inv_a) / 255;
                let nb = (color.b as u32 * effective_a) / 255 + (db * inv_a) / 255;
                u32_slice[idx] = (255 << 24) | (nb << 16) | (ng << 8) | nr;
            }
        }

        true
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{Color, Pixmap};

    #[test]
    fn test_embedded_fonts_load() {
        let cache = GlyphCache::new();
        assert!(cache.is_some());
    }

    #[test]
    fn test_draw_ascii_char_fits_cell() {
        let mut pixmap = Pixmap::new(40, 40).unwrap();
        pixmap.fill(Color::BLACK);
        let white = ColorRgba::new(255, 255, 255, 255);

        // Narrow ASCII cell (matches BitmapFont::ASCII_WIDTH/HEIGHT at scale 1)
        let drawn = draw_char_ttf(&mut pixmap.as_mut(), 'W', 10, 10, 5, 7, false, white);
        assert!(drawn);

        // No lit pixel should fall outside the 5x7 cell box.
        let mut out_of_bounds = 0;
        for py in 0..40usize {
            for px in 0..40usize {
                let idx = (py * 40 + px) * 4;
                let pixel = pixmap.data();
                if pixel[idx] > 10 {
                    if !(10..15).contains(&px) || !(10..17).contains(&py) {
                        out_of_bounds += 1;
                    }
                }
            }
        }
        assert_eq!(out_of_bounds, 0, "glyph overflowed its cell");
    }

    #[test]
    fn test_draw_hangul_char() {
        let mut pixmap = Pixmap::new(40, 40).unwrap();
        pixmap.fill(Color::BLACK);
        let white = ColorRgba::new(255, 255, 255, 255);
        let drawn = draw_char_ttf(&mut pixmap.as_mut(), '가', 5, 5, 10, 8, false, white);
        assert!(drawn);
        let has_pixel = pixmap.data().chunks_exact(4).any(|p| p[0] > 10);
        assert!(has_pixel);
    }

    #[test]
    fn test_draw_kanji_char() {
        let mut pixmap = Pixmap::new(40, 40).unwrap();
        pixmap.fill(Color::BLACK);
        let white = ColorRgba::new(255, 255, 255, 255);
        let drawn = draw_char_ttf(&mut pixmap.as_mut(), '桜', 5, 5, 10, 8, false, white);
        assert!(drawn);
        let has_pixel = pixmap.data().chunks_exact(4).any(|p| p[0] > 10);
        assert!(has_pixel);
    }

    #[test]
    fn test_unmapped_glyph_returns_false() {
        let mut pixmap = Pixmap::new(20, 20).unwrap();
        pixmap.fill(Color::BLACK);
        let white = ColorRgba::new(255, 255, 255, 255);
        // A rare Hanja outside the common-use Korean subset and outside the
        // joyo kanji list should not be in any embedded font.
        let drawn = draw_char_ttf(&mut pixmap.as_mut(), '\u{3400}', 5, 5, 10, 8, false, white);
        assert!(!drawn);
    }
}

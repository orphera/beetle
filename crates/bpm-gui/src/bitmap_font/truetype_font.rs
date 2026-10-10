//! Cross-platform TrueType glyph rendering via `fontdue` (pure Rust, no
//! GPU/shader dependency — see AGENTS.md §3 for the scoped dependency
//! exception and docs/plans/2026-10-03-pulse-redesign.md for the history).
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
//! Glyphs are rasterized at their real pixel size (the em size the caller
//! asks for) and advance by their true width, like the beetle-app text
//! engine. The earlier approach squeezed every glyph into a 5x7 / 10x8
//! grid cell, which left 1px strokes half-covered and unreadable.

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
    /// Offset of the bitmap's top-left from the pen position on the baseline.
    x_off: i32,
    y_off: i32,
    advance: f32,
}

struct GlyphCache {
    latin_regular: Font,
    latin_bold: Font,
    japanese: Font,
    korean: Font,
    // Keyed by (font, char, em px) — one rasterization per glyph per size.
    cache: HashMap<(FontTag, char, u16), Option<CachedGlyph>>,
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

    fn get_or_rasterize(&mut self, tag: FontTag, c: char, px: u16) -> Option<&CachedGlyph> {
        let key = (tag, c, px);
        if !self.cache.contains_key(&key) {
            let font = self.font(tag);
            let glyph = if font.has_glyph(c) {
                let (m, bitmap) = font.rasterize(c, px as f32);
                Some(CachedGlyph {
                    coverage: bitmap,
                    w: m.width as i32,
                    h: m.height as i32,
                    x_off: m.xmin,
                    // Bitmap bottom sits `ymin` above the baseline (negative
                    // for descenders), so its top is `ymin + height` above.
                    y_off: -(m.ymin + m.height as i32),
                    advance: m.advance_width,
                })
            } else {
                None
            };
            self.cache.insert(key, glyph);
        }
        self.cache.get(&key).and_then(|g| g.as_ref())
    }
}

thread_local! {
    static LOCAL_CACHE: RefCell<Option<GlyphCache>> = const { RefCell::new(None) };
}

/// Runs `f` with this thread's glyph cache, creating it on first use.
fn with_cache<R>(f: impl FnOnce(&mut GlyphCache) -> Option<R>) -> Option<R> {
    LOCAL_CACHE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = GlyphCache::new();
        }
        f(slot.as_mut()?)
    })
}

/// Horizontal advance in px for `c` at em size `px`, or `None` if no
/// embedded font covers it (the caller then uses the hand bitmap fallback).
pub fn advance(c: char, px: u16, bold: bool) -> Option<f32> {
    with_cache(|cache| {
        cache
            .get_or_rasterize(font_for_char(c, bold), c, px)
            .map(|g| g.advance)
    })
}

/// Height in px from the baseline up to the top of a capital letter at em
/// size `px`. Callers place the baseline at `top + cap_height` so lines of
/// text share a capital-height top edge.
pub fn cap_height(px: u16) -> i32 {
    with_cache(|cache| {
        let m = cache.latin_regular.metrics('H', px as f32);
        Some(m.ymin + m.height as i32)
    })
    .unwrap_or(px as i32 * 3 / 4)
}

/// Draws a single character with its pen at `x` and baseline at `baseline`,
/// rasterized at em size `px`. Returns `false` if no embedded font has this
/// glyph (caller should fall through to the hand bitmap tables).
#[allow(clippy::too_many_arguments)]
pub fn draw_char(
    pixmap: &mut PixmapMut,
    c: char,
    x: i32,
    baseline: i32,
    px: u16,
    bold: bool,
    color: ColorRgba,
) -> bool {
    if color.a == 0 || px == 0 {
        return true;
    }
    with_cache(|cache| {
        let glyph = cache.get_or_rasterize(font_for_char(c, bold), c, px)?;
        blit_coverage(pixmap, glyph, x, baseline, color);
        Some(())
    })
    .is_some()
}

/// Alpha-blends a glyph's coverage mask onto the pixmap at its pen position.
fn blit_coverage(
    pixmap: &mut PixmapMut,
    glyph: &CachedGlyph,
    x: i32,
    baseline: i32,
    color: ColorRgba,
) {
    let pw = pixmap.width() as i32;
    let ph = pixmap.height() as i32;
    let data = pixmap.data_mut();
    let u32_slice: &mut [u32] =
        unsafe { std::slice::from_raw_parts_mut(data.as_mut_ptr() as *mut u32, data.len() / 4) };

    let base_x = x + glyph.x_off;
    let base_y = baseline + glyph.y_off;

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{Color, Pixmap};

    fn lit_pixels(pixmap: &Pixmap) -> usize {
        pixmap.data().chunks_exact(4).filter(|p| p[0] > 10).count()
    }

    #[test]
    fn test_embedded_fonts_load() {
        let cache = GlyphCache::new();
        assert!(cache.is_some());
    }

    #[test]
    fn test_advance_is_proportional_to_em() {
        // Hangul is roughly a full em wide; Latin is narrower than that.
        let hangul = advance('가', 13, false).unwrap();
        let latin = advance('W', 13, false).unwrap();
        assert!(hangul > 11.0 && hangul < 14.0, "{hangul}");
        assert!(latin > 6.0 && latin < 9.0, "{latin}");
    }

    #[test]
    fn test_draw_ascii_char_lights_pixels() {
        let mut pixmap = Pixmap::new(40, 40).unwrap();
        pixmap.fill(Color::BLACK);
        let white = ColorRgba::new(255, 255, 255, 255);
        let drawn = draw_char(&mut pixmap.as_mut(), 'S', 10, 30, 13, false, white);
        assert!(drawn);
        assert!(lit_pixels(&pixmap) > 20);
    }

    #[test]
    fn test_draw_hangul_char() {
        let mut pixmap = Pixmap::new(40, 40).unwrap();
        pixmap.fill(Color::BLACK);
        let white = ColorRgba::new(255, 255, 255, 255);
        let drawn = draw_char(&mut pixmap.as_mut(), '가', 5, 25, 13, false, white);
        assert!(drawn);
        assert!(lit_pixels(&pixmap) > 0);
    }

    #[test]
    fn test_draw_kanji_char() {
        let mut pixmap = Pixmap::new(40, 40).unwrap();
        pixmap.fill(Color::BLACK);
        let white = ColorRgba::new(255, 255, 255, 255);
        let drawn = draw_char(&mut pixmap.as_mut(), '桜', 5, 25, 13, false, white);
        assert!(drawn);
        assert!(lit_pixels(&pixmap) > 0);
    }

    #[test]
    fn test_unmapped_glyph_returns_false() {
        let mut pixmap = Pixmap::new(20, 20).unwrap();
        pixmap.fill(Color::BLACK);
        let white = ColorRgba::new(255, 255, 255, 255);
        // A rare Hanja outside the common-use Korean subset and outside the
        // joyo kanji list should not be in any embedded font.
        let drawn = draw_char(&mut pixmap.as_mut(), '\u{3400}', 5, 15, 13, false, white);
        assert!(!drawn);
    }
}

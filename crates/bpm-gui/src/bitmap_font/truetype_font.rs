//! Cross-platform TrueType glyph rendering via `fontdue` (pure Rust, no
//! GPU/shader dependency — see AGENTS.md §3 for the scoped dependency
//! exception and docs/plans/2026-10-03-pulse-redesign.md for the history).
//!
//! Two embedded, pre-subsetted font files cover the UI's actual script
//! range (not full Unicode — see the asset README for the subsetting
//! recipe that keeps these small):
//!   - Noto Sans JP (joyo-kanji + kana, OFL-1.1): the ~2137 Japanese
//!     "common use" kanji plus hiragana/katakana/CJK punctuation.
//!   - Noto Sans KR (OFL-1.1): the 2,350 KS X 1001 "common use" Hangul
//!     syllables plus compatibility jamo, and Basic Latin. One family draws
//!     Latin and Hangul alike, so a line never mixes two typefaces.
//!
//! Bold is synthetic (the same outlines, dilated), like the beetle-app text
//! engine, so every script gets the same weight.
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

/// Synthetic bold stroke growth, as a fraction of the em size (as in beetle-render).
const BOLD_STRENGTH: f32 = 0.04;
// Shared with the new text engine so the font data is embedded only once.
const JAPANESE_BYTES: &[u8] = beetle_render::text::JP_BYTES;
const KOREAN_BYTES: &[u8] = beetle_render::text::KR_BYTES;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum FontTag {
    Japanese,
    /// Hangul and Basic Latin.
    Korean,
}

/// Picks which embedded font should render a given character.
fn font_for_char(c: char) -> FontTag {
    let code = c as u32;
    if (0x3000..=0x30FF).contains(&code)
        || (0x4E00..=0x9FFF).contains(&code)
        || (0xFF00..=0xFFEF).contains(&code)
    {
        FontTag::Japanese
    } else {
        FontTag::Korean
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
    japanese: Font,
    korean: Font,
    // Keyed by (font, char, em px) — one rasterization per glyph per size.
    cache: HashMap<(FontTag, char, u16, bool), Option<CachedGlyph>>,
}

impl GlyphCache {
    fn new() -> Option<Self> {
        let settings = FontSettings::default();
        Some(Self {
            japanese: Font::from_bytes(JAPANESE_BYTES, settings).ok()?,
            korean: Font::from_bytes(KOREAN_BYTES, settings).ok()?,
            cache: HashMap::new(),
        })
    }

    fn font(&self, tag: FontTag) -> &Font {
        match tag {
            FontTag::Japanese => &self.japanese,
            FontTag::Korean => &self.korean,
        }
    }

    fn get_or_rasterize(
        &mut self,
        tag: FontTag,
        c: char,
        px: u16,
        bold: bool,
    ) -> Option<&CachedGlyph> {
        let key = (tag, c, px, bold);
        if !self.cache.contains_key(&key) {
            let font = self.font(tag);
            let glyph = if font.has_glyph(c) {
                let (m, bitmap) = font.rasterize(c, px as f32);
                let mut g = CachedGlyph {
                    coverage: bitmap,
                    w: m.width as i32,
                    h: m.height as i32,
                    x_off: m.xmin,
                    // Bitmap bottom sits `ymin` above the baseline (negative
                    // for descenders), so its top is `ymin + height` above.
                    y_off: -(m.ymin + m.height as i32),
                    advance: m.advance_width,
                };
                if bold {
                    embolden(&mut g, px as f32 * BOLD_STRENGTH);
                }
                Some(g)
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
            .get_or_rasterize(font_for_char(c), c, px, bold)
            .map(|g| g.advance)
    })
}

/// Height in px from the baseline up to the top of a capital letter at em
/// size `px`. Callers place the baseline at `top + cap_height` so lines of
/// text share a capital-height top edge.
pub fn cap_height(px: u16) -> i32 {
    with_cache(|cache| {
        let m = cache.korean.metrics('H', px as f32);
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
        let glyph = cache.get_or_rasterize(font_for_char(c), c, px, bold)?;
        blit_coverage(pixmap, glyph, x, baseline, color);
        Some(())
    })
    .is_some()
}

/// Synthetic bold: smears the coverage right by `r` px (the fractional part
/// blended) and widens the advance to match, keeping the left bearing and baseline.
fn embolden(g: &mut CachedGlyph, r: f32) {
    g.advance += r;
    if g.w == 0 || g.h == 0 || r <= 0.0 {
        return;
    }
    let full = r.floor() as i32;
    let frac = r - full as f32;
    let ext = r.ceil() as i32;
    let (w, nw) = (g.w, g.w + ext);
    let mut out = vec![0u8; (nw * g.h) as usize];
    for y in 0..g.h {
        let row = &g.coverage[(y * w) as usize..((y + 1) * w) as usize];
        for x in 0..nw {
            let at = |dx: i32| -> f32 {
                let sx = x - dx;
                if (0..w).contains(&sx) {
                    row[sx as usize] as f32
                } else {
                    0.0
                }
            };
            let mut v = (0..=full).map(at).fold(0.0, f32::max);
            v = v.max(at(full + 1) * frac);
            out[(y * nw + x) as usize] = v as u8;
        }
    }
    g.coverage = out;
    g.w = nw;
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
        let latin = advance('i', 13, false).unwrap();
        assert!(hangul > 11.0 && hangul < 14.0, "{hangul}");
        assert!(latin > 2.0 && latin < 6.0, "{latin}");
    }

    #[test]
    fn bold_is_wider_with_more_ink() {
        let plain = advance('가', 18, false).unwrap();
        let bold = advance('가', 18, true).unwrap();
        assert!(bold > plain);
        let ink = |bold| {
            let mut pixmap = Pixmap::new(40, 40).unwrap();
            pixmap.fill(Color::BLACK);
            let white = ColorRgba::new(255, 255, 255, 255);
            draw_char(&mut pixmap.as_mut(), 'E', 5, 30, 18, bold, white);
            pixmap
                .data()
                .chunks_exact(4)
                .map(|p| p[0] as u32)
                .sum::<u32>()
        };
        assert!(ink(true) > ink(false));
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

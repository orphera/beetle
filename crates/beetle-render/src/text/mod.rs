//! Text engine: proportional, kerned text rasterized at the exact pixel size
//! requested and cached as glyphs in the Canvas' shared atlas
//! (docs/plans/2026-10-04-d3d11-ui-rebuild.md P2).
//!
//! Font fallback per character:
//!   1. Embedded Noto Sans KR subset — Latin (with baked kerning, see
//!      scripts/add-latin-kern.py) + 2,350 common Hangul.
//!   2. Embedded Noto Sans JP subset — kana, joyo kanji, CJK punctuation.
//!      (CJK ranges try JP first so kana/punctuation use Japanese forms.)
//!   3. Windows GDI on demand (`gdi.rs`) — rare kanji, hanzi, symbols.
//!   4. Hollow box ("tofu").
//!
//! Bold comes from synthetic emboldening of the same outlines for the
//! embedded fonts (no bold files are embedded) and FW_BOLD for GDI.

#[cfg(target_os = "windows")]
mod gdi;

use crate::canvas::{AtlasRegion, Canvas, Rect};
use crate::skin::ColorRgba;
use fontdue::{Font, FontSettings};
use std::collections::HashMap;

/// Embedded Noto Sans KR subset (Latin + Hangul); shared with `bpm-gui`.
pub const KR_BYTES: &[u8] = include_bytes!("../../assets/fonts/NotoSansKR-Common-Subset.ttf");
/// Embedded Noto Sans JP subset (kana + joyo kanji); shared with `bpm-gui`.
pub const JP_BYTES: &[u8] = include_bytes!("../../assets/fonts/NotoSansJP-Subset.ttf");

/// Sizes are rasterized at whole pixels: crisp stems, bounded cache.
const MIN_PX: f32 = 6.0;
const MAX_PX: f32 = 256.0;

/// Synthetic bold stroke growth, as a fraction of the em size.
const BOLD_STRENGTH: f32 = 0.04;

/// Coverage gamma. The UI target is not sRGB and blends coverage linearly in
/// gamma space, so partially covered edge pixels come out too dark on the
/// dark theme and light strokes look thin; lifting them restores the weight.
const COVERAGE_GAMMA: f32 = 1.4;

fn apply_coverage_gamma(coverage: &mut [u8]) {
    let exp = 1.0 / COVERAGE_GAMMA;
    for a in coverage.iter_mut() {
        *a = (255.0 * (*a as f32 / 255.0).powf(exp) + 0.5) as u8;
    }
}

const ELLIPSIS: &str = "...";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Weight {
    #[default]
    Regular,
    Bold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// How a run of text looks. Build with `TextStyle::new(px)` and chain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    /// Em size in pixels.
    pub size: f32,
    pub weight: Weight,
    pub color: ColorRgba,
    /// Extra space added after every glyph, in pixels (letter-spacing).
    pub tracking: f32,
}

impl TextStyle {
    pub const fn new(size: f32) -> Self {
        Self {
            size,
            weight: Weight::Regular,
            color: ColorRgba::new(255, 255, 255, 255),
            tracking: 0.0,
        }
    }

    pub const fn bold(mut self) -> Self {
        self.weight = Weight::Bold;
        self
    }

    pub const fn color(mut self, color: ColorRgba) -> Self {
        self.color = color;
        self
    }

    pub const fn tracking(mut self, px: f32) -> Self {
        self.tracking = px;
        self
    }
}

/// Vertical metrics for a size, in pixels (y down; ascent > 0, descent > 0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_height: f32,
    /// Height of capital letters above the baseline (for optical centering).
    pub cap_height: f32,
}

/// A rasterized glyph before atlas placement. `left`/`top` offset the
/// bitmap's top-left from the pen position on the baseline (y down).
pub(crate) struct RasterGlyph {
    pub width: u32,
    pub height: u32,
    pub left: i32,
    pub top: i32,
    pub advance: f32,
    pub coverage: Vec<u8>,
}

impl RasterGlyph {
    pub(crate) fn empty(advance: f32) -> Self {
        Self {
            width: 0,
            height: 0,
            left: 0,
            top: 0,
            advance,
            coverage: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Source {
    Kr,
    Jp,
    Gdi,
    Missing,
}

#[derive(Debug, Clone, Copy)]
struct Glyph {
    source: Source,
    region: Option<AtlasRegion>,
    left: i32,
    top: i32,
    advance: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    c: char,
    px: u16,
    weight: Weight,
}

pub struct TextEngine {
    kr: Font,
    jp: Font,
    #[cfg(target_os = "windows")]
    gdi: Option<gdi::GdiRasterizer>,
    glyphs: HashMap<Key, Glyph>,
    metrics: HashMap<u16, FontMetrics>,
    atlas_full_warned: bool,
}

impl TextEngine {
    /// Parses the embedded fonts (~65 ms total in release builds).
    pub fn new() -> Self {
        let settings = FontSettings::default();
        Self {
            kr: Font::from_bytes(KR_BYTES, settings).expect("embedded KR font"),
            jp: Font::from_bytes(JP_BYTES, settings).expect("embedded JP font"),
            #[cfg(target_os = "windows")]
            gdi: gdi::GdiRasterizer::new(),
            glyphs: HashMap::with_capacity(512),
            metrics: HashMap::new(),
            atlas_full_warned: false,
        }
    }

    /// Drops cached glyph placements (after `Canvas::reset_atlas`).
    pub fn clear_cache(&mut self) {
        self.glyphs.clear();
        self.atlas_full_warned = false;
    }

    /// Number of distinct (char, size, weight) glyphs cached so far.
    pub fn cached_glyphs(&self) -> usize {
        self.glyphs.len()
    }

    fn px(size: f32) -> u16 {
        size.clamp(MIN_PX, MAX_PX).round() as u16
    }

    pub fn metrics(&mut self, size: f32) -> FontMetrics {
        let px = Self::px(size);
        if let Some(m) = self.metrics.get(&px) {
            return *m;
        }
        let pxf = px as f32;
        let lm = self.kr.horizontal_line_metrics(pxf);
        let (ascent, descent, gap) =
            lm.map(|l| (l.ascent, -l.descent, l.line_gap))
                .unwrap_or((pxf * 0.88, pxf * 0.24, 0.0));
        let cap = self.kr.metrics('H', pxf);
        let m = FontMetrics {
            ascent,
            descent,
            line_height: ascent + descent + gap,
            cap_height: (cap.height as f32 + cap.ymin as f32).max(pxf * 0.6),
        };
        self.metrics.insert(px, m);
        m
    }

    fn source_order(c: char) -> [Source; 2] {
        let cp = c as u32;
        let cjk = (0x3000..=0x30FF).contains(&cp)
            || (0x3400..=0x4DBF).contains(&cp)
            || (0x4E00..=0x9FFF).contains(&cp)
            || (0xFF00..=0xFFEF).contains(&cp);
        if cjk {
            [Source::Jp, Source::Kr]
        } else {
            [Source::Kr, Source::Jp]
        }
    }

    fn font(&self, s: Source) -> Option<&Font> {
        match s {
            Source::Kr => Some(&self.kr),
            Source::Jp => Some(&self.jp),
            _ => None,
        }
    }

    /// Looks up (or rasterizes and uploads) a glyph.
    fn glyph(&mut self, canvas: &mut Canvas, c: char, px: u16, weight: Weight) -> Glyph {
        let key = Key { c, px, weight };
        if let Some(g) = self.glyphs.get(&key) {
            return *g;
        }
        let (source, raster) = self.rasterize(c, px, weight);
        let region = if raster.width > 0 && raster.height > 0 {
            let r = canvas.atlas_mut().alloc(raster.width, raster.height);
            match r {
                Some(r) => canvas.atlas_mut().write_alpha(r, &raster.coverage),
                None if !self.atlas_full_warned => {
                    self.atlas_full_warned = true;
                    eprintln!("[text] UI atlas full; further new glyphs are not drawn");
                }
                None => {}
            }
            r
        } else {
            None
        };
        let g = Glyph {
            source,
            region,
            left: raster.left,
            top: raster.top,
            advance: raster.advance,
        };
        self.glyphs.insert(key, g);
        g
    }

    fn rasterize(&mut self, c: char, px: u16, weight: Weight) -> (Source, RasterGlyph) {
        let (source, mut g) = self.rasterize_coverage(c, px, weight);
        apply_coverage_gamma(&mut g.coverage);
        (source, g)
    }

    fn rasterize_coverage(&mut self, c: char, px: u16, weight: Weight) -> (Source, RasterGlyph) {
        let pxf = px as f32;
        for source in Self::source_order(c) {
            let font = self.font(source).unwrap();
            if font.lookup_glyph_index(c) == 0 {
                continue;
            }
            let (m, bitmap) = font.rasterize(c, pxf);
            let mut g = RasterGlyph {
                width: m.width as u32,
                height: m.height as u32,
                left: m.xmin,
                top: -(m.ymin + m.height as i32),
                advance: m.advance_width,
                coverage: bitmap,
            };
            if weight == Weight::Bold {
                embolden(&mut g, pxf * BOLD_STRENGTH);
            }
            return (source, g);
        }
        #[cfg(target_os = "windows")]
        if let Some(gdi) = self.gdi.as_mut() {
            if let Some(g) = gdi.rasterize(c, px, weight == Weight::Bold) {
                return (Source::Gdi, g);
            }
        }
        (Source::Missing, RasterGlyph::empty(pxf * 0.6))
    }

    fn kern(&self, prev: Option<(char, Source)>, c: char, source: Source, px: f32) -> f32 {
        match (prev, self.font(source)) {
            (Some((p, ps)), Some(font)) if ps == source => {
                font.horizontal_kern(p, c, px).unwrap_or(0.0)
            }
            _ => 0.0,
        }
    }

    /// Width of `text` in pixels (including tracking between glyphs).
    pub fn measure(&mut self, canvas: &mut Canvas, text: &str, style: &TextStyle) -> f32 {
        self.run(canvas, text, 0.0, 0.0, style, 1.0, false)
    }

    /// Draws `text` with its baseline at `y`, starting at `x`. Returns the advance.
    pub fn draw(
        &mut self,
        canvas: &mut Canvas,
        text: &str,
        x: f32,
        y: f32,
        style: &TextStyle,
    ) -> f32 {
        self.run(canvas, text, x, y, style, 1.0, true)
    }

    /// Draws text rasterized at `style.size` but scaled by `scale` around
    /// its pen origin — for pop/bounce animations without caching every
    /// intermediate size.
    pub fn draw_scaled(
        &mut self,
        canvas: &mut Canvas,
        text: &str,
        x: f32,
        y: f32,
        style: &TextStyle,
        scale: f32,
    ) -> f32 {
        self.run(canvas, text, x, y, style, scale, true)
    }

    #[allow(clippy::too_many_arguments)]
    fn run(
        &mut self,
        canvas: &mut Canvas,
        text: &str,
        x: f32,
        y: f32,
        style: &TextStyle,
        scale: f32,
        draw: bool,
    ) -> f32 {
        let px = Self::px(style.size);
        let pxf = px as f32;
        let mut pen = 0.0f32;
        let mut prev: Option<(char, Source)> = None;
        let mut first = true;
        for c in text.chars() {
            if c == '\n' || c == '\r' {
                continue;
            }
            let g = self.glyph(canvas, c, px, style.weight);
            pen += self.kern(prev, c, g.source, pxf);
            if !first {
                pen += style.tracking;
            }
            first = false;
            if draw {
                // Unscaled glyphs snap to whole pixels: bitmap texels map 1:1
                // onto screen pixels, so text stays sharp.
                let (gx, gy) = if scale == 1.0 {
                    ((x + pen).round() + g.left as f32, y.round() + g.top as f32)
                } else {
                    (x + (pen + g.left as f32) * scale, y + g.top as f32 * scale)
                };
                match g.region {
                    Some(r) => canvas.sprite(
                        r,
                        Rect::new(gx, gy, r.w as f32 * scale, r.h as f32 * scale),
                        style.color,
                    ),
                    None if g.source == Source::Missing => {
                        let m = self.metrics(pxf);
                        let w = g.advance * 0.8 * scale;
                        let h = m.cap_height * scale;
                        let bx = x + (pen + g.advance * 0.1) * scale;
                        canvas.stroke_rect(Rect::new(bx, y - h, w, h), 1.0, style.color);
                    }
                    None => {}
                }
            }
            pen += g.advance;
            prev = Some((c, g.source));
        }
        pen * scale
    }

    /// Draws `text` aligned horizontally inside `rect` and optically
    /// centered vertically (capital-letter height centered on the rect).
    pub fn draw_in(
        &mut self,
        canvas: &mut Canvas,
        text: &str,
        rect: Rect,
        align: Align,
        style: &TextStyle,
    ) -> f32 {
        let fitted = self.fit(canvas, text, rect.w, style);
        let w = self.measure(canvas, &fitted, style);
        let x = match align {
            Align::Left => rect.x,
            Align::Center => rect.x + (rect.w - w) / 2.0,
            Align::Right => rect.right() - w,
        };
        let cap = self.metrics(style.size).cap_height;
        let baseline = rect.y + (rect.h + cap) / 2.0;
        self.draw(canvas, &fitted, x, baseline, style)
    }

    /// Returns `text` shortened with a trailing ellipsis so it fits `max_w`.
    pub fn fit<'a>(
        &mut self,
        canvas: &mut Canvas,
        text: &'a str,
        max_w: f32,
        style: &TextStyle,
    ) -> std::borrow::Cow<'a, str> {
        if self.measure(canvas, text, style) <= max_w {
            return std::borrow::Cow::Borrowed(text);
        }
        let ell = self.measure(canvas, ELLIPSIS, style) + style.tracking;
        let budget = max_w - ell;
        let px = Self::px(style.size);
        let mut pen = 0.0;
        let mut end = 0;
        let mut prev: Option<(char, Source)> = None;
        for (i, c) in text.char_indices() {
            let g = self.glyph(canvas, c, px, style.weight);
            let next = pen + self.kern(prev, c, g.source, px as f32) + g.advance;
            if next > budget {
                break;
            }
            pen = next + style.tracking;
            end = i + c.len_utf8();
            prev = Some((c, g.source));
        }
        std::borrow::Cow::Owned(format!("{}{ELLIPSIS}", text[..end].trim_end()))
    }
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Synthetic bold: dilates coverage right by `strength` px and up by half
/// of it (fractional parts blended), widening the advance to match. Growing
/// right/up keeps the left bearing and baseline where they were.
fn embolden(g: &mut RasterGlyph, strength: f32) {
    if g.width == 0 || g.height == 0 || strength <= 0.0 {
        g.advance += strength;
        return;
    }
    let dilate =
        |src: &[u8], w: usize, h: usize, r: f32, horizontal: bool| -> (Vec<u8>, usize, usize) {
            let ext = r.ceil() as usize;
            let (nw, nh) = if horizontal {
                (w + ext, h)
            } else {
                (w, h + ext)
            };
            let full = r.floor() as usize;
            let frac = r - full as f32;
            let mut out = vec![0u8; nw * nh];
            let get = |x: isize, y: isize| -> f32 {
                if x < 0 || y < 0 || x >= w as isize || y >= h as isize {
                    0.0
                } else {
                    src[y as usize * w + x as usize] as f32
                }
            };
            for y in 0..nh {
                for x in 0..nw {
                    // horizontal: the source sits at the left (x offset 0) and
                    // smears right; vertical: source sits at the bottom (y offset
                    // ext) and smears up.
                    let (sx, sy) = if horizontal {
                        (x as isize, y as isize)
                    } else {
                        (x as isize, y as isize - ext as isize)
                    };
                    let mut v = 0.0f32;
                    for d in 0..=full {
                        let s = if horizontal {
                            get(sx - d as isize, sy)
                        } else {
                            get(sx, sy + d as isize)
                        };
                        v = v.max(s);
                    }
                    if frac > 0.0 {
                        let d = full as isize + 1;
                        let s = if horizontal {
                            get(sx - d, sy)
                        } else {
                            get(sx, sy + d)
                        };
                        v = v.max(s * frac);
                    }
                    out[y * nw + x] = v as u8;
                }
            }
            (out, nw, nh)
        };
    let (w, h) = (g.width as usize, g.height as usize);
    let (hx, w2, h2) = dilate(&g.coverage, w, h, strength, true);
    let ry = strength * 0.5;
    let (out, w3, h3) = dilate(&hx, w2, h2, ry, false);
    g.top -= ry.ceil() as i32;
    g.coverage = out;
    g.width = w3 as u32;
    g.height = h3 as u32;
    g.advance += strength;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (TextEngine, Canvas) {
        (TextEngine::new(), Canvas::new(1024))
    }

    #[test]
    fn latin_is_proportional_and_kerned() {
        let (mut t, mut c) = setup();
        let s = TextStyle::new(20.0);
        let i = t.measure(&mut c, "iiii", &s);
        let m = t.measure(&mut c, "MMMM", &s);
        assert!(m > i * 2.0, "proportional: MMMM {m} vs iiii {i}");
        let av = t.measure(&mut c, "AV", &s);
        let a_v = t.measure(&mut c, "A", &s) + t.measure(&mut c, "V", &s);
        assert!(av < a_v - 0.2, "kerned AV {av} < A+V {a_v}");
    }

    #[test]
    fn digits_are_tabular() {
        let (mut t, mut c) = setup();
        let s = TextStyle::new(32.0).bold();
        let one = t.measure(&mut c, "1111", &s);
        let eight = t.measure(&mut c, "8888", &s);
        assert!((one - eight).abs() < 0.01, "score digits must not jitter");
    }

    #[test]
    fn sizes_scale_continuously_not_in_7px_steps() {
        let (mut t, mut c) = setup();
        let widths: Vec<f32> = [12.0, 13.0, 14.0, 15.0, 16.0]
            .iter()
            .map(|&px| t.measure(&mut c, "Beetle", &TextStyle::new(px)))
            .collect();
        assert!(widths.windows(2).all(|w| w[1] > w[0]), "{widths:?}");
    }

    #[test]
    fn mixed_scripts_resolve_to_real_glyphs() {
        let (mut t, mut c) = setup();
        let s = TextStyle::new(18.0);
        for ch in ['A', '한', 'あ', 'カ', '漢', '「'] {
            let g = t.glyph(&mut c, ch, 18, Weight::Regular);
            assert!(g.region.is_some(), "{ch} has a bitmap");
            assert_ne!(g.source, Source::Missing, "{ch}");
        }
        assert_eq!(
            t.glyph(&mut c, '한', 18, Weight::Regular).source,
            Source::Kr
        );
        assert_eq!(
            t.glyph(&mut c, 'あ', 18, Weight::Regular).source,
            Source::Jp
        );
        let w = t.measure(&mut c, "한글 テスト", &s);
        assert!(w > 18.0 * 5.0);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn rare_kanji_and_symbols_fall_back_to_gdi() {
        let (mut t, mut c) = setup();
        // 鬱 (non-joyo until 2010 lists vary) / 龍 / ★ are outside the subsets
        for ch in ['龍', '★', '…'] {
            let g = t.glyph(&mut c, ch, 18, Weight::Regular);
            assert_eq!(g.source, Source::Gdi, "{ch}");
            assert!(g.region.is_some() && g.advance > 0.0, "{ch}");
        }
    }

    #[test]
    fn bold_is_wider_with_more_ink() {
        let (mut t, mut c) = setup();
        let r = t.glyph(&mut c, 'H', 24, Weight::Regular);
        let b = t.glyph(&mut c, 'H', 24, Weight::Bold);
        assert!(b.advance > r.advance);
        assert!(b.region.unwrap().w > r.region.unwrap().w);
        assert_eq!(b.top, r.top - 1, "grows upward, baseline fixed");
    }

    #[test]
    fn glyphs_are_cached_once_per_size_and_weight() {
        let (mut t, mut c) = setup();
        let s = TextStyle::new(16.0);
        t.measure(&mut c, "aaaa", &s);
        t.measure(&mut c, "aaaa", &s);
        assert_eq!(t.cached_glyphs(), 1);
        t.measure(&mut c, "a", &s.bold());
        t.measure(&mut c, "a", &TextStyle::new(16.4)); // rounds to 16
        assert_eq!(t.cached_glyphs(), 2);
    }

    #[test]
    fn fit_truncates_with_ellipsis_within_budget() {
        let (mut t, mut c) = setup();
        let s = TextStyle::new(16.0);
        let long = "Extremely Long Song Title That Will Not Fit";
        let fitted = t.fit(&mut c, long, 120.0, &s);
        assert!(fitted.ends_with("..."), "{fitted}");
        assert!(t.measure(&mut c, &fitted, &s) <= 120.0);
        assert_eq!(t.fit(&mut c, "Short", 120.0, &s), "Short");
    }

    #[test]
    fn draw_snaps_glyphs_to_pixels_and_uses_one_batch() {
        let (mut t, mut c) = setup();
        c.begin(640, 360);
        c.fill_rect(
            Rect::new(0.0, 0.0, 10.0, 10.0),
            ColorRgba::new(0, 0, 0, 255),
        );
        let adv = t.draw(&mut c, "Hi 한글", 10.3, 40.6, &TextStyle::new(18.0));
        assert!(adv > 0.0);
        c.fill_rect(
            Rect::new(0.0, 0.0, 10.0, 10.0),
            ColorRgba::new(0, 0, 0, 255),
        );
        let stats = c.debug_batches();
        assert_eq!(stats.len(), 1, "text interleaved with shapes = 1 batch");
        assert!(c
            .debug_vertices()
            .iter()
            .all(|v| v.position[0].fract() == 0.0 && v.position[1].fract() == 0.0));
    }
}

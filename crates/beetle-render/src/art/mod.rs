//! Generated default skin (docs/plans/2026-10-04-d3d11-ui-rebuild.md P3).
//!
//! No image files: every sprite is computed from signed-distance functions
//! at startup, at the current UI scale (so edges stay 1-px anti-aliased at
//! any resolution), and packed into the Canvas' shared atlas.
//!
//! Most sprites are white masks tinted at draw time, so one sprite serves
//! every color. Shaded sprites (notes) store luminance, so the tint becomes
//! the base color and the shading darkens it.
//!
//! Diagonal or curved edges must come from these sprites, not from raw
//! `Canvas::fill_quad` polygons: the swapchain has no MSAA, so polygon
//! edges alias while SDF sprite edges do not.

pub mod sdf;

use crate::canvas::{AtlasRegion, Canvas, Insets, Rect, UiAtlas};
use crate::skin::ColorRgba;
use sdf::{coverage, cut_box, lerp, round_box, segment, smoothstep, Bitmap};

/// A 9-slice sprite: `border` (texels) keeps its size when stretched.
#[derive(Debug, Clone, Copy)]
pub struct NineSlice {
    pub region: AtlasRegion,
    pub border: Insets,
    /// The center cell is never drawn (transparent outline, or a shadow
    /// whose center is covered by its panel) — saves fill rate.
    pub hollow: bool,
}

/// A sprite whose visible shape is inset from the destination rect, e.g.
/// a drop shadow drawn `margin` px outside the panel it belongs to.
#[derive(Debug, Clone, Copy)]
pub struct Halo {
    pub slice: NineSlice,
    pub margin: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Icons {
    pub play: AtlasRegion,
    pub chevron_right: AtlasRegion,
    pub chevron_left: AtlasRegion,
    pub star: AtlasRegion,
    pub check: AtlasRegion,
    pub dot: AtlasRegion,
}

#[derive(Debug, Clone, Copy)]
pub struct Skin {
    /// UI scale the sprites were generated at (viewport height / 720).
    pub scale: f32,
    /// Rounded panels, radius 4 / 8 / 16 px (at 720p).
    pub panel_sm: NineSlice,
    pub panel: NineSlice,
    pub panel_lg: NineSlice,
    /// 1-px outline of `panel` (focus / selection rings).
    pub panel_outline: NineSlice,
    /// Soft drop shadow for `panel`-radius boxes.
    pub shadow: Halo,
    /// Cut-corner (top-left + bottom-right chamfer) fill and outline.
    pub cut_panel: NineSlice,
    pub cut_outline: NineSlice,
    /// Gaussian blob for additive glows and particles.
    pub glow: AtlasRegion,
    /// Key beam: bright at the bottom, fading up, soft sides.
    pub beam: AtlasRegion,
    /// Note head (shaded) + additive gloss overlay. White and blue keys
    /// share the body shape; the tint is the only difference.
    pub note: NineSlice,
    pub note_gloss: NineSlice,
    /// Scratch head: the body shape with a dark groove across its middle.
    /// Drawn with `scratch_mark` on top.
    pub note_scratch: NineSlice,
    /// Long-note tail cap: the body shape lit from below, so head and tail
    /// read differently at a glance.
    pub note_tail: NineSlice,
    /// Long-note body with crisp edges (rails under `ln_flow`).
    pub ln_body: NineSlice,
    /// Vertically periodic stripe texture for long-note bodies. One period
    /// is the region's height, so tiles abut and a phase offset makes the
    /// stripes flow.
    pub ln_flow: AtlasRegion,
    /// Double chevron drawn over a scratch head (white, tinted at draw time).
    pub scratch_mark: AtlasRegion,
    /// Hit-burst ring, streak spark, cross flare.
    pub ring: AtlasRegion,
    pub spark: AtlasRegion,
    pub flare: AtlasRegion,
    /// 256×256 grain tile and full-screen vignette. The grain is per-pixel
    /// noise, so a larger tile only lengthens the repeat; it needs a quarter of
    /// the quads a 128 px tile did, which keeps menu screens within one batch.
    pub noise: AtlasRegion,
    pub vignette: AtlasRegion,
    pub icons: Icons,
}

fn put(atlas: &mut UiAtlas, bmp: &Bitmap) -> AtlasRegion {
    let r = atlas
        .alloc(bmp.w, bmp.h)
        .expect("skin fits in a fresh atlas");
    atlas.write_rgba(r, &bmp.to_premultiplied_rgba());
    r
}

fn put_black(atlas: &mut UiAtlas, bmp: &Bitmap) -> AtlasRegion {
    let r = atlas
        .alloc(bmp.w, bmp.h)
        .expect("skin fits in a fresh atlas");
    atlas.write_rgba(r, &bmp.to_black_rgba());
    r
}

fn px(v: f32) -> u32 {
    v.round().max(1.0) as u32
}

impl Skin {
    /// Generates every sprite at `scale` into `atlas`. ~10–30 ms.
    pub fn generate(atlas: &mut UiAtlas, scale: f32) -> Skin {
        let s = scale.max(0.25);
        Skin {
            scale,
            panel_sm: rounded(atlas, 4.0 * s, None),
            panel: rounded(atlas, 8.0 * s, None),
            panel_lg: rounded(atlas, 16.0 * s, None),
            panel_outline: rounded(atlas, 8.0 * s, Some(s.max(1.0))),
            shadow: shadow(atlas, 8.0 * s, 10.0 * s),
            cut_panel: cut(atlas, 12.0 * s, None),
            cut_outline: cut(atlas, 12.0 * s, Some(s.max(1.0))),
            glow: glow(atlas, px(64.0 * s)),
            beam: beam(atlas, px(32.0 * s), px(160.0 * s)),
            note: head(atlas, 12.0 * s, 3.0 * s, HeadShape::Body, false),
            note_gloss: head(atlas, 12.0 * s, 3.0 * s, HeadShape::Body, true),
            note_scratch: head(atlas, 12.0 * s, 3.0 * s, HeadShape::Scratch, false),
            note_tail: head(atlas, 12.0 * s, 3.0 * s, HeadShape::Tail, false),
            ln_body: ln_body(atlas, s),
            ln_flow: put(atlas, &ln_flow_bmp(px(24.0 * s))),
            scratch_mark: put(atlas, &scratch_mark_bmp(px(12.0 * s), px(10.0 * s))),
            ring: ring(atlas, px(96.0 * s), 3.0 * s),
            spark: spark(atlas, px(64.0 * s), px(12.0 * s)),
            flare: flare(atlas, px(96.0 * s)),
            noise: noise(atlas, 256),
            vignette: vignette(atlas, 256),
            icons: Icons {
                play: icon_play(atlas, px(32.0 * s)),
                chevron_right: icon_chevron(atlas, px(32.0 * s), false),
                chevron_left: icon_chevron(atlas, px(32.0 * s), true),
                star: icon_star(atlas, px(32.0 * s)),
                check: icon_check(atlas, px(32.0 * s)),
                dot: icon_dot(atlas, px(32.0 * s)),
            },
        }
    }
}

// ---------------------------------------------------------------------
// Generators
// ---------------------------------------------------------------------

/// Rounded-rect 9-slice; `stroke` makes it an inner outline of that width.
fn rounded(atlas: &mut UiAtlas, r: f32, stroke: Option<f32>) -> NineSlice {
    let b = r.ceil() as u32 + 2;
    let size = 2 * b + 2;
    let half = size as f32 / 2.0;
    let bmp = Bitmap::mask(size, size, |x, y| {
        let d = round_box(x, y, half, half, half, half, r);
        match stroke {
            Some(w) => coverage((d + w / 2.0).abs() - w / 2.0),
            None => coverage(d),
        }
    });
    NineSlice {
        region: put(atlas, &bmp),
        border: Insets::uniform(b as f32),
        hollow: stroke.is_some(),
    }
}

/// Gaussian drop shadow around a rounded box of radius `r`.
fn shadow(atlas: &mut UiAtlas, r: f32, sigma: f32) -> Halo {
    let m = (sigma * 2.5).ceil();
    let b = (m + r).ceil() as u32 + 1;
    let size = 2 * b + 2;
    let half = size as f32 / 2.0;
    let bmp = Bitmap::mask(size, size, |x, y| {
        let d = round_box(x, y, half, half, half - m, half - m, r);
        if d <= 0.0 {
            1.0
        } else {
            (-(d * d) / (2.0 * sigma * sigma)).exp()
        }
    });
    Halo {
        slice: NineSlice {
            region: put_black(atlas, &bmp),
            border: Insets::uniform(b as f32),
            hollow: true,
        },
        margin: m,
    }
}

fn cut(atlas: &mut UiAtlas, c: f32, stroke: Option<f32>) -> NineSlice {
    let b = c.ceil() as u32 + 2;
    let size = 2 * b + 2;
    let sz = size as f32;
    let bmp = Bitmap::mask(size, size, |x, y| {
        let d = cut_box(x, y, 0.0, 0.0, sz, sz, c);
        match stroke {
            Some(w) => coverage((d + w / 2.0).abs() - w / 2.0),
            None => coverage(d),
        }
    });
    NineSlice {
        region: put(atlas, &bmp),
        border: Insets::uniform(b as f32),
        hollow: stroke.is_some(),
    }
}

fn glow(atlas: &mut UiAtlas, size: u32) -> AtlasRegion {
    let half = size as f32 / 2.0;
    let sigma = half / 2.6;
    let floor = (-(half * half) / (2.0 * sigma * sigma)).exp();
    let bmp = Bitmap::mask(size, size, |x, y| {
        let d2 = (x - half).powi(2) + (y - half).powi(2);
        (((-d2 / (2.0 * sigma * sigma)).exp() - floor) / (1.0 - floor)).max(0.0)
    });
    put(atlas, &bmp)
}

fn beam(atlas: &mut UiAtlas, w: u32, h: u32) -> AtlasRegion {
    let (hw, hf) = (w as f32 / 2.0, h as f32);
    let bmp = Bitmap::mask(w, h, |x, y| {
        // Soft wide body plus a narrow hot core, both fading toward the top.
        let dx = (x - hw) / hw;
        let side = (1.0 - dx * dx).max(0.0).powf(1.5);
        let core = (1.0 - dx.abs() / 0.3).max(0.0).powi(2);
        let up = (y / hf).powf(1.6);
        (0.5 * side + 0.9 * core) * up
    });
    put(atlas, &bmp)
}

/// Which note shape a head sprite is drawn as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadShape {
    /// White and blue keys: bright top rim, darker lower edge.
    Body,
    /// Scratch: the body with a dark groove across its middle.
    Scratch,
    /// Long-note tail: lit from below, dark top edge.
    Tail,
}

/// Head geometry shared by the body and gloss sprites so they 9-slice
/// identically: the side border in texels and the bitmap width.
fn head_geometry(r: f32) -> (u32, u32) {
    let b = r.ceil() as u32 + 2;
    (b, 2 * b + 2)
}

/// Note head bitmap (luminance shades the tint): a rounded bar of height `h`
/// with a bevel. A 1-px rim light on the lit edge and a darker 1-px lip on
/// the other, a gradient across the face, and small dark notches on both
/// sides at mid height. The notches sit in the fixed-width side border, so
/// they keep their size when the head is stretched to the lane width.
pub fn head_bmp(h: f32, r: f32, shape: HeadShape) -> Bitmap {
    let hp = px(h);
    let (_, w) = head_geometry(r);
    let (half_w, half_h) = (w as f32 / 2.0, hp as f32 / 2.0);
    let hf = hp as f32;
    Bitmap::from_fn(w, hp, |x, y| {
        let a = coverage(round_box(x, y, half_w, half_h, half_w, half_h, r));
        let t = y / hf;
        let mut luma = match shape {
            HeadShape::Tail => {
                if t < 0.5 {
                    lerp(0.42, 0.68, t / 0.5)
                } else {
                    lerp(0.68, 0.92, (t - 0.5) / 0.5)
                }
            }
            HeadShape::Body | HeadShape::Scratch => {
                if t < 0.5 {
                    lerp(0.96, 0.80, t / 0.5)
                } else {
                    lerp(0.80, 0.50, (t - 0.5) / 0.5)
                }
            }
        };
        match shape {
            HeadShape::Tail => {
                if y < 1.2 {
                    luma = 0.30;
                } else if y > hf - 1.2 {
                    luma = 1.0;
                }
            }
            HeadShape::Body | HeadShape::Scratch => {
                // Lit top: a white rim with a softer second row; a dark lip below.
                if y < 1.2 {
                    luma = 1.0;
                } else if y < 2.4 {
                    luma = luma.max(0.9);
                } else if y > hf - 1.2 {
                    luma = 0.25;
                } else if y > hf - 2.4 {
                    luma = luma.min(0.5);
                }
            }
        }
        if shape == HeadShape::Scratch && (y - half_h).abs() < 0.8 {
            luma *= 0.6;
        }
        let notch = (y - half_h).abs() < hf * 0.2 && (x < 1.5 || x > w as f32 - 1.5);
        if notch {
            luma *= 0.55;
        }
        (luma, a)
    })
}

/// Additive highlight for a head: a bright cap on the upper third that
/// fades out, with a sharper rim line at the very top.
pub fn head_gloss_bmp(h: f32, r: f32) -> Bitmap {
    let hp = px(h);
    let (_, w) = head_geometry(r);
    let (half_w, half_h) = (w as f32 / 2.0, hp as f32 / 2.0);
    let hf = hp as f32;
    Bitmap::from_fn(w, hp, |x, y| {
        let a = coverage(round_box(x, y, half_w, half_h, half_w, half_h, r));
        let g = (1.0 - (y / hf) / 0.45).max(0.0);
        let rim = if y < 1.2 { 0.8 } else { 0.0 };
        (1.0, a * (g * g * 0.55 + rim))
    })
}

fn head(atlas: &mut UiAtlas, h: f32, r: f32, shape: HeadShape, gloss: bool) -> NineSlice {
    let (b, _) = head_geometry(r);
    let bmp = if gloss {
        head_gloss_bmp(h, r)
    } else {
        head_bmp(h, r, shape)
    };
    NineSlice {
        region: put(atlas, &bmp),
        border: Insets::new(b as f32, 0.0, b as f32, 0.0),
        hollow: false,
    }
}

/// Long-note body texture: `h` texels is one period of diagonal-free
/// stripes that are bright in the middle of each period and dark between,
/// with lit rails on both edges. Periodic in y, so tiles abut seamlessly.
pub fn ln_flow_bmp(h: u32) -> Bitmap {
    let w = 8u32;
    let hf = h as f32;
    let wf = w as f32;
    Bitmap::from_fn(w, h, |x, y| {
        // Diagonal stripes: the phase also slides one period across the width,
        // so the stripes lean. Still periodic in y, so tiles join seamlessly.
        let phase = std::f32::consts::TAU * (y / hf + x / wf);
        let v = 0.5 + 0.5 * phase.cos();
        let rail = x < 1.5 || x > wf - 1.5;
        let luma = if rail { 1.0 } else { 0.55 + 0.45 * v * v };
        (luma, 1.0)
    })
}

/// Chevron pair pointing up, white, for a scratch head. Symmetric about the
/// vertical axis.
pub fn scratch_mark_bmp(w: u32, h: u32) -> Bitmap {
    let (fw, fh) = (w as f32, h as f32);
    let th = 0.16 * fh;
    Bitmap::mask(w, h, |x, y| {
        let chevron = |top: f32| {
            segment(x, y, 0.18 * fw, top + 0.36 * fh, 0.5 * fw, top + 0.06 * fh).min(segment(
                x,
                y,
                0.5 * fw,
                top + 0.06 * fh,
                0.82 * fw,
                top + 0.36 * fh,
            ))
        };
        let d = chevron(0.0).min(chevron(0.36 * fh));
        coverage(d - th / 2.0)
    })
}

fn ln_body(atlas: &mut UiAtlas, s: f32) -> NineSlice {
    let edge = s.max(1.0).round();
    let b = edge as u32 + 1;
    let (w, h) = (2 * b + 2, 4);
    let bmp = Bitmap::mask(w, h, |x, _| {
        let from_edge = x.min(w as f32 - x);
        if from_edge < edge {
            0.95
        } else {
            0.42
        }
    });
    NineSlice {
        region: put(atlas, &bmp),
        border: Insets::new(b as f32, 0.0, b as f32, 0.0),
        hollow: false,
    }
}

fn ring(atlas: &mut UiAtlas, size: u32, thickness: f32) -> AtlasRegion {
    put(atlas, &ring_bmp(size, thickness))
}

/// Ring with a soft halo, centred in a `size` square.
pub fn ring_bmp(size: u32, thickness: f32) -> Bitmap {
    let half = size as f32 / 2.0;
    let radius = half - thickness * 2.0;
    Bitmap::mask(size, size, |x, y| {
        let d = sdf::circle(x, y, half, half, radius).abs();
        let core = coverage(d - thickness / 2.0);
        let halo = (-(d * d) / (2.0 * (thickness * 1.5).powi(2))).exp() * 0.45;
        // The halo must reach zero before the square's edge, or it draws a box.
        let from_centre = ((x - half).powi(2) + (y - half).powi(2)).sqrt();
        let fade = 1.0 - smoothstep(half * 0.8, half, from_centre);
        core.max(halo * fade)
    })
}

fn spark(atlas: &mut UiAtlas, w: u32, h: u32) -> AtlasRegion {
    let (hw, hh) = (w as f32 / 2.0, h as f32 / 2.0);
    let bmp = Bitmap::mask(w, h, |x, y| {
        let k = 1.0 - ((x - hw).abs() / hw + (y - hh).abs() / hh);
        k.max(0.0).powf(1.3)
    });
    put(atlas, &bmp)
}

fn flare(atlas: &mut UiAtlas, size: u32) -> AtlasRegion {
    let half = size as f32 / 2.0;
    let thin = half / 14.0;
    let bmp = Bitmap::mask(size, size, |x, y| {
        let (dx, dy) = ((x - half) / half, (y - half) / half);
        let fade = |t: f32| (1.0 - t.abs()).max(0.0).powi(2);
        let h = (-((y - half).powi(2)) / (2.0 * thin * thin)).exp() * fade(dx);
        let v = (-((x - half).powi(2)) / (2.0 * thin * thin)).exp() * fade(dy);
        let core = (-(dx * dx + dy * dy) / 0.02).exp();
        h.max(v).max(core)
    });
    put(atlas, &bmp)
}

fn noise(atlas: &mut UiAtlas, size: u32) -> AtlasRegion {
    let bmp = Bitmap::mask(size, size, |x, y| {
        sdf::hash_noise(x as u32, y as u32, 0xBEE7)
    });
    put(atlas, &bmp)
}

fn vignette(atlas: &mut UiAtlas, size: u32) -> AtlasRegion {
    let half = size as f32 / 2.0;
    let bmp = Bitmap::mask(size, size, |x, y| {
        let r = (((x - half) / half).powi(2) + ((y - half) / half).powi(2)).sqrt();
        smoothstep(0.45, 1.25, r).powf(1.3)
    });
    put_black(atlas, &bmp)
}

fn icon_play(atlas: &mut UiAtlas, size: u32) -> AtlasRegion {
    let f = size as f32;
    let pts = [
        (0.30 * f, 0.20 * f),
        (0.82 * f, 0.50 * f),
        (0.30 * f, 0.80 * f),
    ];
    put(
        atlas,
        &Bitmap::supersampled(size, size, 4, |x, y| sdf::in_polygon(x, y, &pts)),
    )
}

fn icon_chevron(atlas: &mut UiAtlas, size: u32, left: bool) -> AtlasRegion {
    let f = size as f32;
    let th = 0.10 * f;
    let (x0, x1) = if left { (0.62, 0.38) } else { (0.38, 0.62) };
    let bmp = Bitmap::mask(size, size, |x, y| {
        let d = segment(x, y, x0 * f, 0.22 * f, x1 * f, 0.5 * f).min(segment(
            x,
            y,
            x1 * f,
            0.5 * f,
            x0 * f,
            0.78 * f,
        ));
        coverage(d - th / 2.0)
    });
    put(atlas, &bmp)
}

fn icon_star(atlas: &mut UiAtlas, size: u32) -> AtlasRegion {
    let f = size as f32;
    let c = f / 2.0;
    let pts: Vec<(f32, f32)> = (0..10)
        .map(|i| {
            let r = if i % 2 == 0 { 0.47 * f } else { 0.20 * f };
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
            (c + r * a.cos(), c + 0.04 * f + r * a.sin())
        })
        .collect();
    put(
        atlas,
        &Bitmap::supersampled(size, size, 4, |x, y| sdf::in_polygon(x, y, &pts)),
    )
}

fn icon_check(atlas: &mut UiAtlas, size: u32) -> AtlasRegion {
    let f = size as f32;
    let th = 0.11 * f;
    let bmp = Bitmap::mask(size, size, |x, y| {
        let d = segment(x, y, 0.22 * f, 0.52 * f, 0.42 * f, 0.72 * f).min(segment(
            x,
            y,
            0.42 * f,
            0.72 * f,
            0.80 * f,
            0.30 * f,
        ));
        coverage(d - th / 2.0)
    });
    put(atlas, &bmp)
}

fn icon_dot(atlas: &mut UiAtlas, size: u32) -> AtlasRegion {
    let c = size as f32 / 2.0;
    put(
        atlas,
        &Bitmap::mask(size, size, |x, y| {
            coverage(sdf::circle(x, y, c, c, size as f32 * 0.3))
        }),
    )
}

// ---------------------------------------------------------------------
// Drawing helpers
// ---------------------------------------------------------------------

impl Canvas {
    /// Draws a 9-slice skin sprite at its native (generated) scale.
    pub fn nine(&mut self, n: &NineSlice, dst: Rect, tint: ColorRgba) {
        self.nine_slice_ex(n.region, n.border, 1.0, dst, tint, tint, n.hollow);
    }

    /// 9-slice skin sprite with a left → right gradient tint.
    pub fn nine_hgradient(&mut self, n: &NineSlice, dst: Rect, left: ColorRgba, right: ColorRgba) {
        self.nine_slice_ex(n.region, n.border, 1.0, dst, left, right, n.hollow);
    }

    /// Draws a halo (e.g. drop shadow) around `target`.
    pub fn halo(&mut self, h: &Halo, target: Rect, tint: ColorRgba) {
        self.nine(&h.slice, target.inset(-h.margin), tint);
    }

    /// Repeats `region` across `dst` at 1:1 texel size (clipped to `dst`).
    pub fn tile(&mut self, region: AtlasRegion, dst: Rect, tint: ColorRgba) {
        let (tw, th) = (region.w as f32, region.h as f32);
        self.push_clip(dst);
        let mut y = dst.y;
        while y < dst.bottom() {
            let mut x = dst.x;
            while x < dst.right() {
                self.sprite(region, Rect::new(x, y, tw, th), tint);
                x += tw;
            }
            y += th;
        }
        self.pop_clip();
    }

    /// Draws a sprite centered on (cx, cy) at `size`.
    pub fn sprite_centered(
        &mut self,
        region: AtlasRegion,
        cx: f32,
        cy: f32,
        w: f32,
        h: f32,
        tint: ColorRgba,
    ) {
        self.sprite(region, Rect::new(cx - w / 2.0, cy - h / 2.0, w, h), tint);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_at_multiple_scales_within_budget() {
        for scale in [0.75, 1.0, 1.5, 2.0, 3.0] {
            let mut atlas = UiAtlas::new(crate::canvas::DEFAULT_ATLAS_PAGE_SIZE);
            let t = std::time::Instant::now();
            let skin = Skin::generate(&mut atlas, scale);
            let ms = t.elapsed().as_secs_f32() * 1000.0;
            assert_eq!(atlas.page_count(), 1, "skin fits one page at {scale}x");
            assert_eq!(skin.note.region.h as u32, px(12.0 * scale));
            // Debug builds are ~10x slower; keep a generous ceiling here and
            // measure release timing in the GPU showcase test.
            assert!(ms < 2000.0, "{scale}x took {ms} ms");
        }
    }

    fn mirror_equal(v: &[f32], w: u32, h: u32) -> bool {
        (0..h).all(|y| {
            (0..w).all(|x| {
                let a = v[(y * w + x) as usize];
                let b = v[(y * w + (w - 1 - x)) as usize];
                (a - b).abs() < 1e-4
            })
        })
    }

    #[test]
    fn note_heads_are_bevelled_and_left_right_symmetric() {
        for shape in [HeadShape::Body, HeadShape::Scratch, HeadShape::Tail] {
            let b = head_bmp(12.0, 3.0, shape);
            assert_eq!(b.h, 12, "{shape:?} keeps the note height");
            assert!(
                b.alpha.iter().all(|a| (0.0..=1.0).contains(a)),
                "{shape:?} alpha bounded"
            );
            assert!(
                b.luma.iter().all(|l| (0.0..=1.0).contains(l)),
                "{shape:?} luma bounded"
            );
            assert!(
                b.alpha.iter().any(|&a| a > 0.99),
                "{shape:?} has a solid face"
            );
            assert!(mirror_equal(&b.alpha, b.w, b.h), "{shape:?} alpha mirrors");
            assert!(mirror_equal(&b.luma, b.w, b.h), "{shape:?} luma mirrors");
        }
        let (w, h) = {
            let b = head_bmp(12.0, 3.0, HeadShape::Body);
            (b.w, b.h)
        };
        let body = head_bmp(12.0, 3.0, HeadShape::Body);
        let mid = (w / 2) as usize;
        let top_rim = body.luma[mid];
        let below_rim = body.luma[w as usize + mid];
        let bottom_lip = body.luma[((h - 1) * w) as usize + mid];
        assert_eq!(top_rim, 1.0, "rim light on the top row");
        assert!(bottom_lip < below_rim, "lower edge darker than the face");
        // Side notch: the left edge is darker at mid height than a few rows above.
        let notch_row = (h / 2) as usize * w as usize;
        let plain_row = 3 * w as usize;
        assert!(body.luma[notch_row] < body.luma[plain_row], "side notch");
    }

    #[test]
    fn scratch_head_has_a_groove_and_tail_is_lit_from_below() {
        let s = head_bmp(12.0, 3.0, HeadShape::Scratch);
        let body = head_bmp(12.0, 3.0, HeadShape::Body);
        let (w, mid) = (s.w as usize, (s.h / 2) as usize);
        let c = w / 2;
        assert!(
            s.luma[mid * w + c] < body.luma[mid * w + c] * 0.8,
            "scratch groove is darker than the plain body"
        );
        let tail = head_bmp(12.0, 3.0, HeadShape::Tail);
        let last = (tail.h - 1) as usize * w + c;
        assert!(tail.luma[last] > tail.luma[c], "tail rim is on the bottom");
    }

    #[test]
    fn ring_halo_is_gone_at_the_square_edges() {
        let r = ring_bmp(96, 3.0);
        let w = r.w as usize;
        let edge = [(0, 48), (95, 48), (48, 0), (48, 95), (0, 0), (95, 95)];
        for (x, y) in edge {
            assert!(
                r.alpha[y * w + x] < 0.01,
                "edge pixel ({x},{y}) is {}",
                r.alpha[y * w + x]
            );
        }
        // The ring itself is still there, halfway out.
        assert!(r.alpha.iter().any(|&a| a > 0.9));
    }

    #[test]
    fn head_gloss_is_brightest_at_the_top_and_fades() {
        let g = head_gloss_bmp(12.0, 3.0);
        let w = g.w as usize;
        let c = w / 2;
        assert!(g.alpha[c] > g.alpha[(g.h as usize / 2) * w + c]);
        assert_eq!(g.alpha[(g.h as usize - 1) * w + c], 0.0);
    }

    #[test]
    fn ln_flow_tiles_seamlessly_and_has_lit_rails() {
        let f = ln_flow_bmp(24);
        let w = f.w as usize;
        // One period: across the seam (row 23 -> row 0) the step is no larger
        // than any step inside the tile, so the tiles join without a visible line.
        let step = |a: usize, b: usize| {
            (0..w)
                .map(|x| (f.luma[a * w + x] - f.luma[b * w + x]).abs())
                .fold(0.0f32, f32::max)
        };
        let seam = step(23, 0);
        let inner = (0..23).map(|y| step(y, y + 1)).fold(0.0f32, f32::max);
        assert!(seam <= inner + 1e-4, "seam step {seam} vs inner {inner}");
        assert!(f.luma[0] == 1.0 && f.luma[w - 1] == 1.0, "rails are lit");
        let l = &f.luma;
        let interior: Vec<f32> = (0..24)
            .flat_map(|y| (2..6).map(move |x| l[y * w + x]))
            .collect();
        assert!(
            interior.iter().any(|&l| l > 0.95),
            "stripe crests are bright"
        );
        assert!(interior.iter().any(|&l| l < 0.7), "troughs are darker");
        // Diagonal: a column near the left differs from one near the right.
        assert!(
            (0..24).any(|y| (f.luma[y * w] - f.luma[y * w + 4]).abs() > 0.2),
            "stripes lean"
        );
    }

    #[test]
    fn scratch_mark_is_symmetric_and_non_empty() {
        let m = scratch_mark_bmp(12, 10);
        assert!(m.alpha.iter().any(|&a| a > 0.9), "chevron has solid pixels");
        assert!(mirror_equal(&m.alpha, m.w, m.h), "chevron mirrors");
    }

    #[test]
    fn panel_corners_are_antialiased_and_center_is_solid() {
        let mut atlas = UiAtlas::new(256);
        let p = rounded(&mut atlas, 8.0, None);
        let size = p.region.w as u32;
        let bmp = Bitmap::mask(size, size, |x, y| {
            let h = size as f32 / 2.0;
            coverage(round_box(x, y, h, h, h, h, 8.0))
        });
        assert_eq!(bmp.alpha[0], 0.0, "corner pixel outside the arc");
        assert_eq!(bmp.alpha[(size / 2 * size + size / 2) as usize], 1.0);
        assert!(
            bmp.alpha.iter().any(|&a| a > 0.1 && a < 0.9),
            "has AA pixels"
        );
    }
}

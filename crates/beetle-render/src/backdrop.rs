//! Blurred jacket backdrop for the menu screens (P2-3).
//!
//! The stage image of the selected song (or, without one, the procedural
//! placeholder of its jacket) is reduced to a tiny 64×36 picture: a centred
//! 16:9 crop, a box downscale, a few box-blur passes and a slight saturation
//! boost. The picture lives in the shared UI atlas, in one of two slots. When
//! the song changes, the new picture takes the free slot and the two
//! crossfade over `FADE`. The screen draws it stretched over the viewport,
//! the sampler is bilinear, so the blockiness of 64 texels is not visible.
//!
//! Everything here is pure pixel math and slot bookkeeping. Decoding the
//! stage image happens off the render thread (`beetle-app/src/loader.rs`).

use std::time::{Duration, Instant};

use crate::canvas::{AtlasRegion, UiAtlas};
use crate::image::ImageBuffer;
use crate::jacket::{placeholder_pixels, JacketKey, THUMB_PX};

/// Width of the backdrop picture, in texels.
pub const BACKDROP_W: u32 = 64;
/// Height of the backdrop picture, in texels (16:9 with `BACKDROP_W`).
pub const BACKDROP_H: u32 = 36;
/// Box-blur passes. Three of a 3-tap box approximate a Gaussian.
const PASSES: usize = 3;
/// Saturation multiplier around the luma (1.0 leaves the colours alone).
const SATURATION: f32 = 1.35;
/// Mean luma every picture is scaled to (0..=255). The picture keeps its hue
/// and its contrast, and a bright jacket and a dark one cover the menu with
/// the same weight. Set high enough that the picture reads on the screen.
pub const BACKDROP_LUMA: f32 = 56.0;
/// Luma no single texel may exceed after scaling (0..=255). Keeps a bright
/// highlight from blowing out to white, and keeps its hue.
pub const BACKDROP_PEAK_LUMA: f32 = 140.0;
/// Largest brightening applied to a very dark picture.
const MAX_GAIN: f32 = 4.0;
/// Length of one crossfade between two backdrops.
pub const FADE: Duration = Duration::from_millis(250);

type Rgb = [f32; 3];

/// The centred 16:9 crop of a `w`×`h` image: `(x, y, width, height)`.
pub fn cover_16_9(w: u32, h: u32) -> (u32, u32, u32, u32) {
    let (cw, ch) = if w * 9 >= h * 16 {
        (h * 16 / 9, h)
    } else {
        (w, w * 9 / 16)
    };
    ((w - cw) / 2, (h - ch) / 2, cw, ch)
}

/// Box-averages the `(x, y, w, h)` part of a `sample`d image down to
/// `dw`×`dh`. `sample(x, y)` gives the colour of one source texel.
fn box_downscale(
    (x0, y0, w, h): (u32, u32, u32, u32),
    dw: u32,
    dh: u32,
    sample: impl Fn(u32, u32) -> Rgb,
) -> Vec<Rgb> {
    let edge = |start: u32, t: u32, span: u32, n: u32| start + (t * span / n).min(span.max(1) - 1);
    let mut out = Vec::with_capacity((dw * dh) as usize);
    for ty in 0..dh {
        let sy0 = edge(y0, ty, h, dh);
        let sy1 = edge(y0, ty + 1, h, dh).max(sy0 + 1);
        for tx in 0..dw {
            let sx0 = edge(x0, tx, w, dw);
            let sx1 = edge(x0, tx + 1, w, dw).max(sx0 + 1);
            let mut acc = [0.0f32; 3];
            let mut n = 0.0f32;
            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let c = sample(sx, sy);
                    acc[0] += c[0];
                    acc[1] += c[1];
                    acc[2] += c[2];
                    n += 1.0;
                }
            }
            out.push([acc[0] / n, acc[1] / n, acc[2] / n]);
        }
    }
    out
}

/// One pass of a 3-tap box blur along rows and then columns, with clamped
/// edges. Clamped edges keep the sum of every channel exactly.
fn blur_once(buf: &mut [Rgb], w: usize, h: usize) {
    let mut tmp = vec![[0.0f32; 3]; buf.len()];
    for y in 0..h {
        for x in 0..w {
            let l = buf[y * w + x.saturating_sub(1)];
            let c = buf[y * w + x];
            let r = buf[y * w + (x + 1).min(w - 1)];
            tmp[y * w + x] = [0, 1, 2].map(|k| (l[k] + c[k] + r[k]) / 3.0);
        }
    }
    for y in 0..h {
        for x in 0..w {
            let u = tmp[y.saturating_sub(1) * w + x];
            let c = tmp[y * w + x];
            let d = tmp[(y + 1).min(h - 1) * w + x];
            buf[y * w + x] = [0, 1, 2].map(|k| (u[k] + c[k] + d[k]) / 3.0);
        }
    }
}

/// Pushes every colour away from its luma by `SATURATION`, keeping the luma.
fn saturate(buf: &mut [Rgb]) {
    for c in buf.iter_mut() {
        let luma = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        for v in c.iter_mut() {
            *v = (luma + (*v - luma) * SATURATION).clamp(0.0, 255.0);
        }
    }
}

/// Premultiplied, opaque RGBA8 of a finished backdrop buffer.
fn to_rgba(buf: &[Rgb]) -> Vec<u8> {
    let mut out = Vec::with_capacity(buf.len() * 4);
    for c in buf {
        out.extend_from_slice(&[
            c[0].round().clamp(0.0, 255.0) as u8,
            c[1].round().clamp(0.0, 255.0) as u8,
            c[2].round().clamp(0.0, 255.0) as u8,
            255,
        ]);
    }
    out
}

/// Blur and saturation of a downscaled picture, then RGBA8.
fn finish(mut small: Vec<Rgb>) -> Vec<u8> {
    let (w, h) = (BACKDROP_W as usize, BACKDROP_H as usize);
    for _ in 0..PASSES {
        blur_once(&mut small, w, h);
    }
    saturate(&mut small);
    normalise_luma(&mut small, BACKDROP_LUMA);
    clamp_peak_luma(&mut small, BACKDROP_PEAK_LUMA);
    to_rgba(&small)
}

/// Scales each texel whose luma is above `peak` down to `peak`, keeping its
/// hue. Texels at or below `peak` are left alone.
fn clamp_peak_luma(buf: &mut [Rgb], peak: f32) {
    for c in buf.iter_mut() {
        let luma = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        if luma > peak {
            let k = peak / luma;
            for v in c.iter_mut() {
                *v *= k;
            }
        }
    }
}

/// Scales the picture so its mean luma is `target`. A near-black picture is
/// left nearly black: its gain is capped at `MAX_GAIN`.
fn normalise_luma(buf: &mut [Rgb], target: f32) {
    if buf.is_empty() {
        return;
    }
    let luma = |c: &Rgb| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    let mean = buf.iter().map(luma).sum::<f32>() / buf.len() as f32;
    if mean < 1.0 {
        return;
    }
    let gain = (target / mean).min(MAX_GAIN);
    for c in buf.iter_mut() {
        for v in c.iter_mut() {
            *v = (*v * gain).min(255.0);
        }
    }
}

/// The backdrop picture of a stage image: `BACKDROP_W`×`BACKDROP_H`
/// premultiplied RGBA8 (opaque). Transparent texels are composited over
/// black. `None` for an empty image.
pub fn blur_backdrop(img: &ImageBuffer) -> Option<Vec<u8>> {
    let (w, h) = (img.width, img.height);
    if w == 0 || h == 0 || img.pixels.len() < (w as usize) * (h as usize) {
        return None;
    }
    let crop = cover_16_9(w, h);
    let sample = |x: u32, y: u32| {
        let p = img.pixels[(y * w + x) as usize];
        let a = p.a as f32 / 255.0;
        [p.r as f32 * a, p.g as f32 * a, p.b as f32 * a]
    };
    Some(finish(box_downscale(crop, BACKDROP_W, BACKDROP_H, sample)))
}

/// The backdrop of a song without artwork: its placeholder jacket, stretched
/// to the backdrop shape and blurred like a photo would be.
pub fn placeholder_backdrop(seed: u64) -> Vec<u8> {
    let px = placeholder_pixels(seed);
    let n = THUMB_PX;
    let sample = |x: u32, y: u32| {
        let o = ((y * n + x) * 4) as usize;
        [px[o] as f32, px[o + 1] as f32, px[o + 2] as f32]
    };
    finish(box_downscale((0, 0, n, n), BACKDROP_W, BACKDROP_H, sample))
}

/// Eased progress of a crossfade `elapsed` after it started: 0 at the start,
/// 1 from `FADE` on, smoothstep in between.
pub fn fade_progress(elapsed: Duration) -> f32 {
    let t = (elapsed.as_secs_f32() / FADE.as_secs_f32()).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One backdrop picture to draw, with its share of the frame's alpha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layer {
    pub region: AtlasRegion,
    /// 0..=1; the two layers of a crossfade sum to 1.
    pub alpha: f32,
}

/// What to draw this frame: the picture fading out and the one fading in.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Mix {
    pub outgoing: Option<Layer>,
    pub incoming: Option<Layer>,
}

/// The two backdrop slots and the crossfade between them.
#[derive(Debug, Default)]
pub struct BlurBackdrop {
    regions: [Option<AtlasRegion>; 2],
    /// What each slot's pixels are, `None` when empty.
    keys: [Option<JacketKey>; 2],
    /// The slot being shown.
    front: usize,
    /// When `front` last changed.
    changed: Option<Instant>,
}

impl BlurBackdrop {
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes `key` the backdrop from `now`. `pixels` is only called when the
    /// picture is not already in the free slot. Returns `false` when the
    /// atlas has no room or the pixels have the wrong size.
    pub fn show(
        &mut self,
        atlas: &mut UiAtlas,
        key: JacketKey,
        now: Instant,
        pixels: impl FnOnce() -> Vec<u8>,
    ) -> bool {
        if self.keys[self.front] == Some(key) {
            return true;
        }
        let back = 1 - self.front;
        if self.keys[back] != Some(key) {
            let region = match self.regions[back] {
                Some(r) => r,
                None => {
                    let Some(r) = atlas.alloc(BACKDROP_W, BACKDROP_H) else {
                        return false;
                    };
                    self.regions[back] = Some(r);
                    r
                }
            };
            let px = pixels();
            if px.len() != (BACKDROP_W * BACKDROP_H * 4) as usize {
                return false;
            }
            atlas.write_rgba(region, &px);
            self.keys[back] = Some(key);
        }
        self.front = back;
        self.changed = Some(now);
        true
    }

    /// Forgets the slots (their pixels are gone after an atlas reset). The
    /// next `show` allocates them again.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The layers to draw at `now`.
    pub fn mix(&self, now: Instant) -> Mix {
        let Some(incoming) = self.regions[self.front].filter(|_| self.keys[self.front].is_some())
        else {
            return Mix::default();
        };
        let elapsed = self.elapsed(now);
        if elapsed >= FADE {
            return Mix {
                outgoing: None,
                incoming: Some(Layer {
                    region: incoming,
                    alpha: 1.0,
                }),
            };
        }
        let t = fade_progress(elapsed);
        let back = 1 - self.front;
        let outgoing = match (self.regions[back], self.keys[back]) {
            (Some(region), Some(_)) => Some(Layer {
                region,
                alpha: 1.0 - t,
            }),
            _ => None,
        };
        Mix {
            outgoing,
            incoming: Some(Layer {
                region: incoming,
                alpha: t,
            }),
        }
    }

    /// Whether a crossfade is still running at `now` (the screen must keep
    /// drawing until it ends).
    pub fn is_fading(&self, now: Instant) -> bool {
        self.changed.is_some() && self.elapsed(now) < FADE
    }

    fn elapsed(&self, now: Instant) -> Duration {
        self.changed
            .map_or(Duration::MAX, |c| now.saturating_duration_since(c))
    }

    /// The region of the picture in the front slot (test and debug aid).
    pub fn front_region(&self) -> Option<AtlasRegion> {
        self.regions[self.front].filter(|_| self.keys[self.front].is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mean_luma(px: &[u8]) -> f32 {
        let l = |p: &[u8]| 0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32;
        px.chunks_exact(4).map(l).sum::<f32>() / (px.len() / 4) as f32
    }

    fn image(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> ImageBuffer {
        let mut img = ImageBuffer::new(w, h, crate::skin::ColorRgba::new(0, 0, 0, 255));
        for y in 0..h {
            for x in 0..w {
                let [r, g, b, a] = f(x, y);
                img.pixels[(y * w + x) as usize] = crate::skin::ColorRgba::new(r, g, b, a);
            }
        }
        img
    }

    #[test]
    fn cover_crop_is_centred_and_sixteen_by_nine() {
        assert_eq!(cover_16_9(1920, 1080), (0, 0, 1920, 1080));
        assert_eq!(cover_16_9(800, 600), (0, 75, 800, 450));
        assert_eq!(cover_16_9(1000, 1000), (0, 219, 1000, 562));
        assert_eq!(cover_16_9(1600, 900), (0, 0, 1600, 900));
    }

    #[test]
    fn constant_image_stays_constant() {
        let img = image(37, 23, |_, _| [90, 140, 200, 255]);
        let px = blur_backdrop(&img).expect("pixels");
        assert_eq!(px.len(), (BACKDROP_W * BACKDROP_H * 4) as usize);
        // Every texel is the same colour (the blur changes nothing), opaque,
        // and sits at the luma target (the picture is scaled to it).
        let first = &px[0..4];
        assert!(px.chunks_exact(4).all(|p| p == first && p[3] == 255));
        assert!((mean_luma(first) - BACKDROP_LUMA).abs() <= 1.5, "{first:?}");
    }

    #[test]
    fn blur_keeps_the_mean_energy() {
        // A hard edge plus a bright speck: blurring moves the energy around
        // but must not create or lose any of it.
        let n = (BACKDROP_W * BACKDROP_H) as usize;
        let mut buf: Vec<Rgb> = (0..n)
            .map(|i| {
                let v = if i % 7 == 0 { 255.0 } else { (i % 50) as f32 };
                [v, v * 0.5, 10.0]
            })
            .collect();
        let before: f32 = buf.iter().map(|c| c[0] + c[1] + c[2]).sum::<f32>() / n as f32;
        for _ in 0..PASSES {
            blur_once(&mut buf, BACKDROP_W as usize, BACKDROP_H as usize);
        }
        let after: f32 = buf.iter().map(|c| c[0] + c[1] + c[2]).sum::<f32>() / n as f32;
        assert!(
            (after - before).abs() < 0.01 * before,
            "{before} vs {after}"
        );
    }

    #[test]
    fn saturation_keeps_luma_and_pushes_colour() {
        let mut buf = vec![[200.0, 100.0, 100.0]];
        let luma = |c: Rgb| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        let before = luma(buf[0]);
        saturate(&mut buf);
        assert!((luma(buf[0]) - before).abs() < 0.01);
        assert!(buf[0][0] - buf[0][1] > 100.0, "{:?}", buf[0]);
        // Grey stays grey.
        let mut grey = vec![[120.0, 120.0, 120.0]];
        saturate(&mut grey);
        assert_eq!(grey[0], [120.0, 120.0, 120.0]);
    }

    #[test]
    fn left_and_right_halves_keep_their_colours() {
        let img = image(64, 36, |x, _| {
            if x < 32 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            }
        });
        let px = blur_backdrop(&img).expect("pixels");
        let at = |x: usize, y: usize| &px[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        assert!(at(2, 18)[0] > at(2, 18)[2], "left is red");
        assert!(at(61, 18)[2] > at(61, 18)[0], "right is blue");
    }

    #[test]
    fn pictures_are_scaled_to_the_target_luma() {
        // Bright and dark pictures both land on the target; black stays black.
        let bright = image(64, 36, |_, _| [250, 250, 250, 255]);
        let m = mean_luma(&blur_backdrop(&bright).expect("pixels"));
        assert!((m - BACKDROP_LUMA).abs() < 1.0, "{m}");
        let colourful = image(64, 36, |x, y| [(x * 4) as u8, (y * 7) as u8, 180, 255]);
        let m = mean_luma(&blur_backdrop(&colourful).expect("pixels"));
        assert!((m - BACKDROP_LUMA).abs() < 1.0, "{m}");
        let black = image(8, 8, |_, _| [0, 0, 0, 255]);
        assert!(mean_luma(&blur_backdrop(&black).expect("pixels")) < 0.5);
    }

    #[test]
    fn transparent_pixels_composite_over_black() {
        let img = image(4, 4, |_, _| [255, 255, 255, 0]);
        let px = blur_backdrop(&img).expect("pixels");
        assert!(px.chunks_exact(4).all(|p| p[0] == 0 && p[3] == 255));
        let empty = ImageBuffer {
            width: 0,
            height: 0,
            pixels: Vec::new(),
        };
        assert!(blur_backdrop(&empty).is_none());
    }

    #[test]
    fn placeholder_backdrop_is_deterministic_sized_and_varies_by_song() {
        let a = placeholder_backdrop(12345);
        assert_eq!(a, placeholder_backdrop(12345));
        assert_eq!(a.len(), (BACKDROP_W * BACKDROP_H * 4) as usize);
        assert_ne!(a, placeholder_backdrop(999_999_999));
        assert!(mean_luma(&a) > 0.0);
    }

    #[test]
    fn peak_clamp_keeps_hue_and_caps_only_the_bright_texels() {
        let mut buf = vec![[255.0, 200.0, 100.0], [10.0, 10.0, 10.0]];
        clamp_peak_luma(&mut buf, BACKDROP_PEAK_LUMA);
        let luma = |c: Rgb| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        assert!(
            (luma(buf[0]) - BACKDROP_PEAK_LUMA).abs() < 0.01,
            "{:?}",
            buf[0]
        );
        assert!(
            (buf[0][0] / buf[0][1] - 255.0 / 200.0).abs() < 1e-4,
            "hue kept"
        );
        assert_eq!(buf[1], [10.0, 10.0, 10.0]);
    }

    #[test]
    fn crossfade_eases_from_zero_to_one_over_fade() {
        assert_eq!(fade_progress(Duration::ZERO), 0.0);
        assert_eq!(fade_progress(FADE), 1.0);
        assert_eq!(fade_progress(FADE * 3), 1.0);
        let half = fade_progress(FADE / 2);
        assert!((half - 0.5).abs() < 1e-6);
        assert!(fade_progress(FADE / 4) < 0.25, "eased, not linear");
    }

    fn atlas_with_backdrop() -> (UiAtlas, BlurBackdrop) {
        (UiAtlas::new(256), BlurBackdrop::new())
    }

    #[test]
    fn slots_alternate_and_the_free_one_is_reused() {
        let (mut atlas, mut bd) = atlas_with_backdrop();
        let t0 = Instant::now();
        let px = |v: u8| vec![v; (BACKDROP_W * BACKDROP_H * 4) as usize];
        let a = JacketKey::Placeholder(1);
        let b = JacketKey::Placeholder(2);
        let c = JacketKey::Placeholder(3);

        assert!(bd.show(&mut atlas, a, t0, || px(10)));
        let slot_a = bd.front_region().unwrap();
        assert!(bd.show(&mut atlas, b, t0, || px(20)));
        let slot_b = bd.front_region().unwrap();
        assert_ne!(slot_a, slot_b);
        // Third song: the slot of the first one (no longer shown) is reused.
        assert!(bd.show(&mut atlas, c, t0, || px(30)));
        assert_eq!(bd.front_region(), Some(slot_a));
        assert_eq!(atlas.page_count(), 1);
    }

    #[test]
    fn showing_the_same_song_writes_nothing() {
        let (mut atlas, mut bd) = atlas_with_backdrop();
        let t0 = Instant::now();
        let key = JacketKey::Placeholder(7);
        bd.show(&mut atlas, key, t0, || {
            vec![1; (BACKDROP_W * BACKDROP_H * 4) as usize]
        });
        let mut called = false;
        bd.show(&mut atlas, key, t0, || {
            called = true;
            Vec::new()
        });
        assert!(!called);
    }

    #[test]
    fn switching_back_to_the_previous_song_reuses_its_slot_without_a_write() {
        let (mut atlas, mut bd) = atlas_with_backdrop();
        let t0 = Instant::now();
        let px = || vec![5u8; (BACKDROP_W * BACKDROP_H * 4) as usize];
        let (a, b) = (JacketKey::Placeholder(1), JacketKey::Placeholder(2));
        bd.show(&mut atlas, a, t0, px);
        bd.show(&mut atlas, b, t0, px);
        let mut called = false;
        bd.show(&mut atlas, a, t0, || {
            called = true;
            px()
        });
        assert!(!called, "a still-resident picture is not uploaded again");
    }

    #[test]
    fn mix_crossfades_and_settles_on_the_incoming_layer() {
        let (mut atlas, mut bd) = atlas_with_backdrop();
        let t0 = Instant::now();
        let px = || vec![5u8; (BACKDROP_W * BACKDROP_H * 4) as usize];
        bd.show(&mut atlas, JacketKey::Placeholder(1), t0, px);
        bd.show(&mut atlas, JacketKey::Placeholder(2), t0, px);

        let mid = bd.mix(t0 + FADE / 2);
        let (out, inc) = (mid.outgoing.unwrap(), mid.incoming.unwrap());
        assert!((out.alpha + inc.alpha - 1.0).abs() < 1e-6);
        assert!((inc.alpha - 0.5).abs() < 1e-6);
        assert_ne!(out.region, inc.region);
        assert!(bd.is_fading(t0 + FADE / 2));

        let done = bd.mix(t0 + FADE);
        assert!(done.outgoing.is_none());
        assert_eq!(done.incoming.map(|l| l.alpha), Some(1.0));
        assert!(!bd.is_fading(t0 + FADE));
    }

    #[test]
    fn reset_forgets_slots_and_nothing_is_drawn() {
        let (mut atlas, mut bd) = atlas_with_backdrop();
        let t0 = Instant::now();
        bd.show(&mut atlas, JacketKey::Placeholder(1), t0, || {
            vec![5u8; (BACKDROP_W * BACKDROP_H * 4) as usize]
        });
        bd.reset();
        assert_eq!(bd.mix(t0), Mix::default());
        assert!(!bd.is_fading(t0));
    }
}

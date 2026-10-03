//! Tiny signed-distance / coverage toolkit for generating skin sprites.
//! Distances are in pixels, negative inside. Coverage of an SDF edge is
//! `clamp(0.5 - d, 0, 1)`: an exact 1-pixel anti-aliased edge at any size.

/// Grayscale-with-alpha bitmap; `luma` and `alpha` are 0..=1 per pixel.
pub struct Bitmap {
    pub w: u32,
    pub h: u32,
    pub luma: Vec<f32>,
    pub alpha: Vec<f32>,
}

impl Bitmap {
    /// Samples `f(x, y) -> (luma, alpha)` at every pixel center.
    pub fn from_fn(w: u32, h: u32, f: impl Fn(f32, f32) -> (f32, f32)) -> Self {
        let n = (w * h) as usize;
        let (mut luma, mut alpha) = (Vec::with_capacity(n), Vec::with_capacity(n));
        for y in 0..h {
            for x in 0..w {
                let (l, a) = f(x as f32 + 0.5, y as f32 + 0.5);
                luma.push(l.clamp(0.0, 1.0));
                alpha.push(a.clamp(0.0, 1.0));
            }
        }
        Self { w, h, luma, alpha }
    }

    /// Alpha-only mask (luma 1) from a coverage function.
    pub fn mask(w: u32, h: u32, f: impl Fn(f32, f32) -> f32) -> Self {
        Self::from_fn(w, h, |x, y| (1.0, f(x, y)))
    }

    /// Mask from an inside test, supersampled `n`×`n` per pixel (for shapes
    /// without a convenient distance function, e.g. stars).
    pub fn supersampled(w: u32, h: u32, n: u32, inside: impl Fn(f32, f32) -> bool) -> Self {
        let step = 1.0 / n as f32;
        Self::mask(w, h, |cx, cy| {
            let (x0, y0) = (cx - 0.5 + step / 2.0, cy - 0.5 + step / 2.0);
            let mut hits = 0;
            for j in 0..n {
                for i in 0..n {
                    if inside(x0 + i as f32 * step, y0 + j as f32 * step) {
                        hits += 1;
                    }
                }
            }
            hits as f32 / (n * n) as f32
        })
    }

    /// Premultiplied RGBA8 (gray = luma × alpha) for `UiAtlas::write_rgba`.
    pub fn to_premultiplied_rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.luma.len() * 4);
        for (&l, &a) in self.luma.iter().zip(&self.alpha) {
            let g = (l * a * 255.0 + 0.5) as u8;
            out.extend_from_slice(&[g, g, g, (a * 255.0 + 0.5) as u8]);
        }
        out
    }

    /// Premultiplied black with alpha (shadows, vignette).
    pub fn to_black_rgba(&self) -> Vec<u8> {
        self.alpha
            .iter()
            .flat_map(|&a| [0, 0, 0, (a * 255.0 + 0.5) as u8])
            .collect()
    }
}

pub fn coverage(d: f32) -> f32 {
    (0.5 - d).clamp(0.0, 1.0)
}

pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Rounded box centered at (cx, cy) with half-size (hw, hh) and radius r.
pub fn round_box(x: f32, y: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = (x - cx).abs() - hw + r;
    let qy = (y - cy).abs() - hh + r;
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - r
}

/// Box from (l, t) to (r, b) with the top-left and bottom-right corners
/// chamfered by `cut` pixels (the "arcade cabinet" cut-corner shape).
pub fn cut_box(x: f32, y: f32, l: f32, t: f32, r: f32, b: f32, cut: f32) -> f32 {
    let bx = round_box(x, y, (l + r) / 2.0, (t + b) / 2.0, (r - l) / 2.0, (b - t) / 2.0, 0.0);
    let s = std::f32::consts::FRAC_1_SQRT_2;
    let tl = (cut - ((x - l) + (y - t))) * s;
    let br = (cut - ((r - x) + (b - y))) * s;
    bx.max(tl).max(br)
}

pub fn circle(x: f32, y: f32, cx: f32, cy: f32, radius: f32) -> f32 {
    ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() - radius
}

/// Distance to the segment (ax, ay)–(bx, by).
pub fn segment(x: f32, y: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (px, py, dx, dy) = (x - ax, y - ay, bx - ax, by - ay);
    let t = ((px * dx + py * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    ((px - dx * t).powi(2) + (py - dy * t).powi(2)).sqrt()
}

/// Even-odd point-in-polygon.
pub fn in_polygon(x: f32, y: f32, pts: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let mut j = pts.len() - 1;
    for i in 0..pts.len() {
        let (xi, yi) = pts[i];
        let (xj, yj) = pts[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Deterministic per-pixel hash noise in 0..1.
pub fn hash_noise(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x.wrapping_mul(0x8da6_b343) ^ y.wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65535.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_box_distances() {
        assert!((round_box(0.0, 0.0, 0.0, 0.0, 10.0, 5.0, 0.0) + 5.0).abs() < 1e-5);
        assert!((round_box(12.0, 0.0, 0.0, 0.0, 10.0, 5.0, 2.0) - 2.0).abs() < 1e-5);
        // corner with radius 2: point on the diagonal 2px out from the arc
        let d = round_box(10.0, 5.0, 0.0, 0.0, 10.0, 5.0, 2.0);
        assert!((d - (2.0f32.sqrt() * 2.0 - 2.0)).abs() < 1e-4);
    }

    #[test]
    fn cut_box_removes_corners() {
        assert!(cut_box(0.5, 0.5, 0.0, 0.0, 20.0, 20.0, 6.0) > 0.0, "TL corner cut");
        assert!(cut_box(19.5, 19.5, 0.0, 0.0, 20.0, 20.0, 6.0) > 0.0, "BR corner cut");
        assert!(cut_box(19.5, 0.5, 0.0, 0.0, 20.0, 20.0, 6.0) < 0.0, "TR corner kept");
    }

    #[test]
    fn edges_are_antialiased() {
        // Edge at x = 4.25: pixel 4 (center 4.5) is a quarter covered.
        let b = Bitmap::mask(8, 1, |x, _| coverage(x - 4.25));
        assert_eq!(b.alpha[3], 1.0);
        assert!((b.alpha[4] - 0.25).abs() < 1e-5);
        assert_eq!(b.alpha[5], 0.0);
    }
}

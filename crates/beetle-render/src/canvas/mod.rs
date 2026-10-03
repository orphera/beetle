//! Canvas: the single immediate-mode drawing API every screen renders through
//! (ADR-026, docs/plans/2026-10-04-d3d11-ui-rebuild.md P1).
//!
//! Draw calls are *recorded* (no backend access while drawing) and submitted
//! in `end`. Everything is premultiplied alpha, and solid fills, generated
//! skin sprites and text glyphs all sample the same atlas pages, so a batch
//! only breaks when the texture actually changes (atlas page ↔ external
//! jacket/BGA texture) — never because of draw order or additive glow.
//!
//! Additive light is a draw state (`set_additive`), not a blend-mode switch:
//! a premultiplied color with alpha 0 is added to the destination by the
//! same `src + dst * (1 - a)` blend equation.

pub mod atlas;
pub mod geom;

pub use atlas::{AtlasRegion, UiAtlas};
pub use geom::{Insets, Rect};

use crate::backend::{BlendMode, GpuBackend, TextureId, Vertex2D};
use crate::skin::ColorRgba;

/// Per-draw-call limits of the D3D11 backend's dynamic buffers.
const MAX_BATCH_VERTICES: usize = 8192;
const MAX_BATCH_INDICES: usize = 12288;

/// Default atlas page edge (texels). 2048 is within feature level 10_0's
/// 8192 limit and leaves room for skin sprites plus several font sizes.
pub const DEFAULT_ATLAS_PAGE_SIZE: u32 = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TexSlot {
    Atlas(u16),
    External(TextureId),
}

#[derive(Debug, Clone, Copy)]
struct Batch {
    tex: TexSlot,
    vtx_start: usize,
    idx_start: usize,
}

/// Vertex used during clipping (position, uv, premultiplied color).
#[derive(Debug, Clone, Copy)]
struct V {
    x: f32,
    y: f32,
    u: f32,
    v: f32,
    c: [f32; 4],
}

impl V {
    fn lerp(&self, o: &V, t: f32) -> V {
        let l = |a: f32, b: f32| a + (b - a) * t;
        V {
            x: l(self.x, o.x),
            y: l(self.y, o.y),
            u: l(self.u, o.u),
            v: l(self.v, o.v),
            c: [
                l(self.c[0], o.c[0]),
                l(self.c[1], o.c[1]),
                l(self.c[2], o.c[2]),
                l(self.c[3], o.c[3]),
            ],
        }
    }
}

pub struct Canvas {
    atlas: UiAtlas,
    white_uv: [f32; 2],
    vertices: Vec<Vertex2D>,
    indices: Vec<u16>,
    batches: Vec<Batch>,
    clip_stack: Vec<Rect>,
    additive: bool,
    width: u32,
    height: u32,
    last_draw_calls: usize,
    scratch_a: Vec<V>,
    scratch_b: Vec<V>,
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new(DEFAULT_ATLAS_PAGE_SIZE)
    }
}

impl Canvas {
    pub fn new(atlas_page_size: u32) -> Self {
        let mut atlas = UiAtlas::new(atlas_page_size);
        let white_uv = Self::alloc_white(&mut atlas);
        Self {
            atlas,
            white_uv,
            vertices: Vec::with_capacity(MAX_BATCH_VERTICES),
            indices: Vec::with_capacity(MAX_BATCH_INDICES),
            batches: Vec::with_capacity(8),
            clip_stack: Vec::with_capacity(8),
            additive: false,
            width: 1,
            height: 1,
            last_draw_calls: 0,
            scratch_a: Vec::with_capacity(16),
            scratch_b: Vec::with_capacity(16),
        }
    }

    /// 3×3 opaque white block; solid fills sample its center texel so they
    /// share the atlas texture (and batch) with sprites and glyphs.
    fn alloc_white(atlas: &mut UiAtlas) -> [f32; 2] {
        let white = atlas.alloc(3, 3).expect("atlas page too small");
        atlas.write_alpha(white, &[255; 9]);
        let uv = atlas.uv(white, 1.5, 1.5, 0.0, 0.0);
        [uv[0], uv[1]]
    }

    /// Empties the atlas (all previously returned regions become invalid).
    /// Callers must regenerate sprites and drop glyph caches.
    pub fn reset_atlas(&mut self) {
        self.atlas.clear();
        self.white_uv = Self::alloc_white(&mut self.atlas);
    }

    pub fn atlas(&self) -> &UiAtlas {
        &self.atlas
    }

    /// For generators (skin sprites, glyph cache) to allocate and write texels.
    pub fn atlas_mut(&mut self) -> &mut UiAtlas {
        &mut self.atlas
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Draw calls submitted by the most recent `end`.
    pub fn last_draw_calls(&self) -> usize {
        self.last_draw_calls
    }

    /// Starts recording a frame of `width`×`height` target pixels.
    pub fn begin(&mut self, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
        self.vertices.clear();
        self.indices.clear();
        self.batches.clear();
        self.clip_stack.clear();
        self.additive = false;
    }

    /// Uploads dirty atlas pages, then submits every recorded batch.
    /// Returns the number of draw calls issued.
    pub fn end(&mut self, backend: &mut dyn GpuBackend) -> usize {
        self.atlas.sync(backend);
        let mut calls = 0;
        for (i, b) in self.batches.iter().enumerate() {
            let (vtx_end, idx_end) = match self.batches.get(i + 1) {
                Some(next) => (next.vtx_start, next.idx_start),
                None => (self.vertices.len(), self.indices.len()),
            };
            if idx_end == b.idx_start {
                continue;
            }
            let tex = match b.tex {
                TexSlot::Atlas(page) => self.atlas.page_texture(page),
                TexSlot::External(id) => Some(id),
            };
            let Some(tex) = tex else { continue };
            backend.draw_batch(
                &self.vertices[b.vtx_start..vtx_end],
                &self.indices[b.idx_start..idx_end],
                Some(tex),
                BlendMode::Premultiplied,
            );
            calls += 1;
        }
        self.last_draw_calls = calls;
        calls
    }

    /// Recorded batch count of the frame in progress (tests only).
    #[cfg(test)]
    pub(crate) fn debug_batches(&self) -> Vec<usize> {
        self.batches.iter().map(|b| b.vtx_start).collect()
    }

    #[cfg(test)]
    pub(crate) fn debug_vertices(&self) -> &[Vertex2D] {
        &self.vertices
    }

    // ------------------------------------------------------------------
    // State
    // ------------------------------------------------------------------

    /// While on, every draw adds light instead of covering (glows, beams).
    pub fn set_additive(&mut self, on: bool) {
        self.additive = on;
    }

    /// Restricts subsequent draws to `rect` ∩ current clip.
    pub fn push_clip(&mut self, rect: Rect) {
        let clipped = match self.clip_stack.last() {
            Some(cur) => cur.intersect(&rect).unwrap_or_default(),
            None => rect,
        };
        self.clip_stack.push(clipped);
    }

    pub fn pop_clip(&mut self) {
        self.clip_stack.pop();
    }

    // ------------------------------------------------------------------
    // Shapes
    // ------------------------------------------------------------------

    pub fn fill_rect(&mut self, r: Rect, color: ColorRgba) {
        self.fill_rect_corners(r, [color; 4]);
    }

    /// Left → right gradient.
    pub fn fill_rect_hgradient(&mut self, r: Rect, left: ColorRgba, right: ColorRgba) {
        self.fill_rect_corners(r, [left, right, right, left]);
    }

    /// Top → bottom gradient.
    pub fn fill_rect_vgradient(&mut self, r: Rect, top: ColorRgba, bottom: ColorRgba) {
        self.fill_rect_corners(r, [top, top, bottom, bottom]);
    }

    /// Per-corner colors in order top-left, top-right, bottom-right, bottom-left.
    pub fn fill_rect_corners(&mut self, r: Rect, colors: [ColorRgba; 4]) {
        let pts = [
            (r.x, r.y),
            (r.right(), r.y),
            (r.right(), r.bottom()),
            (r.x, r.bottom()),
        ];
        self.fill_quad(pts, colors);
    }

    /// Hairline / border: four edge rects of `thickness` inside `r`.
    pub fn stroke_rect(&mut self, r: Rect, thickness: f32, color: ColorRgba) {
        let t = thickness.min(r.w / 2.0).min(r.h / 2.0);
        self.fill_rect(Rect::new(r.x, r.y, r.w, t), color);
        self.fill_rect(Rect::new(r.x, r.bottom() - t, r.w, t), color);
        self.fill_rect(Rect::new(r.x, r.y + t, t, r.h - 2.0 * t), color);
        self.fill_rect(Rect::new(r.right() - t, r.y + t, t, r.h - 2.0 * t), color);
    }

    /// Straight line of `thickness` centered on the segment.
    pub fn line(&mut self, p0: (f32, f32), p1: (f32, f32), thickness: f32, color: ColorRgba) {
        let (dx, dy) = (p1.0 - p0.0, p1.1 - p0.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len <= f32::EPSILON {
            return;
        }
        let (nx, ny) = (-dy / len * thickness / 2.0, dx / len * thickness / 2.0);
        self.fill_quad(
            [
                (p0.0 + nx, p0.1 + ny),
                (p1.0 + nx, p1.1 + ny),
                (p1.0 - nx, p1.1 - ny),
                (p0.0 - nx, p0.1 - ny),
            ],
            [color; 4],
        );
    }

    /// Convex quad, points in winding order, one color per point.
    pub fn fill_quad(&mut self, pts: [(f32, f32); 4], colors: [ColorRgba; 4]) {
        let [u, v] = self.white_uv;
        let verts = [0, 1, 2, 3].map(|i| V {
            x: pts[i].0,
            y: pts[i].1,
            u,
            v,
            c: self.pm(colors[i]),
        });
        self.push_poly(TexSlot::Atlas(0), &verts);
    }

    pub fn fill_triangle(&mut self, pts: [(f32, f32); 3], colors: [ColorRgba; 3]) {
        let [u, v] = self.white_uv;
        let verts = [0, 1, 2].map(|i| V {
            x: pts[i].0,
            y: pts[i].1,
            u,
            v,
            c: self.pm(colors[i]),
        });
        self.push_poly(TexSlot::Atlas(0), &verts);
    }

    // ------------------------------------------------------------------
    // Sprites
    // ------------------------------------------------------------------

    /// Draws a whole atlas region stretched into `dst`.
    pub fn sprite(&mut self, region: AtlasRegion, dst: Rect, tint: ColorRgba) {
        self.sprite_sub(
            region,
            Rect::new(0.0, 0.0, region.w as f32, region.h as f32),
            dst,
            tint,
        );
    }

    /// Draws `src` (texels, relative to the region) of an atlas region into `dst`.
    pub fn sprite_sub(&mut self, region: AtlasRegion, src: Rect, dst: Rect, tint: ColorRgba) {
        let uv = self.atlas.uv(region, src.x, src.y, src.w, src.h);
        self.textured_rect(TexSlot::Atlas(region.page), dst, uv, tint);
    }

    /// 9-slice: corners keep their size (`src_border` texels × `scale`), edges
    /// stretch along one axis, the center stretches both ways. Borders shrink
    /// proportionally if `dst` is smaller than their sum.
    pub fn nine_slice(
        &mut self,
        region: AtlasRegion,
        src_border: Insets,
        scale: f32,
        dst: Rect,
        tint: ColorRgba,
    ) {
        self.nine_slice_hgradient(region, src_border, scale, dst, tint, tint);
    }

    /// `nine_slice` tinted with a left → right gradient across `dst`
    /// (e.g. a gradient fill that follows a cut-corner shape exactly).
    pub fn nine_slice_hgradient(
        &mut self,
        region: AtlasRegion,
        src_border: Insets,
        scale: f32,
        dst: Rect,
        left: ColorRgba,
        right: ColorRgba,
    ) {
        let mut b = src_border.scaled(scale);
        let fx = (dst.w / (b.left + b.right)).min(1.0);
        let fy = (dst.h / (b.top + b.bottom)).min(1.0);
        b = Insets::new(b.left * fx, b.top * fy, b.right * fx, b.bottom * fy);

        let (rw, rh) = (region.w as f32, region.h as f32);
        let sx = [0.0, src_border.left, rw - src_border.right, rw];
        let sy = [0.0, src_border.top, rh - src_border.bottom, rh];
        let dx = [dst.x, dst.x + b.left, dst.right() - b.right, dst.right()];
        let dy = [dst.y, dst.y + b.top, dst.bottom() - b.bottom, dst.bottom()];
        for j in 0..3 {
            for i in 0..3 {
                let d = Rect::from_ltrb(dx[i], dy[j], dx[i + 1], dy[j + 1]);
                if d.is_empty() {
                    continue;
                }
                let s = Rect::from_ltrb(sx[i], sy[j], sx[i + 1], sy[j + 1]);
                let uv = self.atlas.uv(region, s.x, s.y, s.w, s.h);
                let at = |x: f32| lerp_color(left, right, (x - dst.x) / dst.w.max(f32::EPSILON));
                let (cl, cr) = (at(d.x), at(d.right()));
                self.textured_rect_colors(TexSlot::Atlas(region.page), d, uv, [cl, cr, cr, cl]);
            }
        }
    }

    /// Draws an external texture (jacket art, BGA frame). Its pixels must be
    /// opaque or premultiplied. `uv` is `[u0, v0, u1, v1]`.
    pub fn image(&mut self, tex: TextureId, dst: Rect, uv: [f32; 4], tint: ColorRgba) {
        self.textured_rect(TexSlot::External(tex), dst, uv, tint);
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    /// Straight-alpha 8-bit color → premultiplied float, alpha zeroed when
    /// additive.
    fn pm(&self, c: ColorRgba) -> [f32; 4] {
        let a = c.a as f32 / 255.0;
        let k = a / 255.0;
        [
            c.r as f32 * k,
            c.g as f32 * k,
            c.b as f32 * k,
            if self.additive { 0.0 } else { a },
        ]
    }

    fn textured_rect(&mut self, tex: TexSlot, r: Rect, uv: [f32; 4], tint: ColorRgba) {
        self.textured_rect_colors(tex, r, uv, [tint; 4]);
    }

    /// Colors in order top-left, top-right, bottom-right, bottom-left.
    fn textured_rect_colors(&mut self, tex: TexSlot, r: Rect, uv: [f32; 4], c: [ColorRgba; 4]) {
        let verts = [
            V { x: r.x, y: r.y, u: uv[0], v: uv[1], c: self.pm(c[0]) },
            V { x: r.right(), y: r.y, u: uv[2], v: uv[1], c: self.pm(c[1]) },
            V { x: r.right(), y: r.bottom(), u: uv[2], v: uv[3], c: self.pm(c[2]) },
            V { x: r.x, y: r.bottom(), u: uv[0], v: uv[3], c: self.pm(c[3]) },
        ];
        self.push_poly(tex, &verts);
    }

    /// Clips a convex polygon against the current clip rect
    /// (Sutherland–Hodgman), then emits it as a triangle fan.
    fn push_poly(&mut self, tex: TexSlot, verts: &[V]) {
        let Some(clip) = self.clip_stack.last().copied() else {
            self.emit_fan(tex, verts);
            return;
        };
        if clip.is_empty() {
            return;
        }
        let (min_x, max_x, min_y, max_y) = verts.iter().fold(
            (f32::MAX, f32::MIN, f32::MAX, f32::MIN),
            |(a, b, c, d), p| (a.min(p.x), b.max(p.x), c.min(p.y), d.max(p.y)),
        );
        let bounds = Rect::from_ltrb(min_x, min_y, max_x, max_y);
        if clip.contains_rect(&bounds) {
            self.emit_fan(tex, verts);
            return;
        }
        if bounds.intersect(&clip).is_none() {
            return;
        }

        let mut a = std::mem::take(&mut self.scratch_a);
        let mut b = std::mem::take(&mut self.scratch_b);
        a.clear();
        a.extend_from_slice(verts);
        // (axis value of a vertex, boundary, keep when value >= boundary?)
        let edges: [(fn(&V) -> f32, f32, bool); 4] = [
            (|p| p.x, clip.x, true),
            (|p| p.x, clip.right(), false),
            (|p| p.y, clip.y, true),
            (|p| p.y, clip.bottom(), false),
        ];
        for (axis, bound, keep_greater) in edges {
            b.clear();
            let inside = |p: &V| {
                if keep_greater {
                    axis(p) >= bound
                } else {
                    axis(p) <= bound
                }
            };
            for i in 0..a.len() {
                let cur = a[i];
                let prev = a[(i + a.len() - 1) % a.len()];
                let (ci, pi) = (inside(&cur), inside(&prev));
                if ci != pi {
                    let t = (bound - axis(&prev)) / (axis(&cur) - axis(&prev));
                    b.push(prev.lerp(&cur, t));
                }
                if ci {
                    b.push(cur);
                }
            }
            std::mem::swap(&mut a, &mut b);
            if a.len() < 3 {
                break;
            }
        }
        if a.len() >= 3 {
            self.emit_fan(tex, &a);
        }
        self.scratch_a = a;
        self.scratch_b = b;
    }

    fn emit_fan(&mut self, tex: TexSlot, verts: &[V]) {
        let n = verts.len();
        if n < 3 {
            return;
        }
        let ni = (n - 2) * 3;
        let need_new = match self.batches.last() {
            Some(b) => {
                b.tex != tex
                    || self.vertices.len() - b.vtx_start + n > MAX_BATCH_VERTICES
                    || self.indices.len() - b.idx_start + ni > MAX_BATCH_INDICES
            }
            None => true,
        };
        if need_new {
            self.batches.push(Batch {
                tex,
                vtx_start: self.vertices.len(),
                idx_start: self.indices.len(),
            });
        }
        let base = (self.vertices.len() - self.batches.last().unwrap().vtx_start) as u16;
        for p in verts {
            self.vertices.push(Vertex2D::new(p.x, p.y, p.u, p.v, p.c));
        }
        for i in 1..(n as u16 - 1) {
            self.indices.extend_from_slice(&[base, base + i, base + i + 1]);
        }
    }
}

fn lerp_color(a: ColorRgba, b: ColorRgba, t: f32) -> ColorRgba {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    ColorRgba::new(l(a.r, b.r), l(a.g, b.g), l(a.b, b.b), l(a.a, b.a))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    enum Call {
        Create,
        Update,
        Draw {
            verts: Vec<Vertex2D>,
            indices: Vec<u16>,
            tex: Option<TextureId>,
            blend: BlendMode,
        },
    }

    #[derive(Default)]
    struct Mock {
        calls: Vec<Call>,
        next: u32,
    }

    impl GpuBackend for Mock {
        fn begin_frame(&mut self, _: u32, _: u32, _: [f32; 4]) {}
        fn create_texture(&mut self, _: u32, _: u32, _: &[u8]) -> Option<TextureId> {
            self.next += 1;
            let id = TextureId(self.next);
            self.calls.push(Call::Create);
            Some(id)
        }
        fn update_texture(&mut self, _: TextureId, _: u32, _: u32, _: &[u8]) {
            self.calls.push(Call::Update);
        }
        fn destroy_texture(&mut self, _: TextureId) {}
        fn draw_batch(&mut self, v: &[Vertex2D], i: &[u16], t: Option<TextureId>, b: BlendMode) {
            self.calls.push(Call::Draw {
                verts: v.to_vec(),
                indices: i.to_vec(),
                tex: t,
                blend: b,
            });
        }
        fn end_frame(&mut self) {}
        fn resize(&mut self, _: u32, _: u32) {}
        fn backend_name(&self) -> &'static str {
            "mock"
        }
    }

    impl Mock {
        fn draws(&self) -> Vec<(&Vec<Vertex2D>, &Vec<u16>, Option<TextureId>, BlendMode)> {
            self.calls
                .iter()
                .filter_map(|c| match c {
                    Call::Draw {
                        verts,
                        indices,
                        tex,
                        blend,
                    } => Some((verts, indices, *tex, *blend)),
                    _ => None,
                })
                .collect()
        }
    }

    const RED: ColorRgba = ColorRgba::new(255, 0, 0, 255);
    const HALF_WHITE: ColorRgba = ColorRgba::new(255, 255, 255, 128);

    #[test]
    fn shapes_sprites_and_glow_share_one_draw_call() {
        let mut canvas = Canvas::new(256);
        let glyph = canvas.atlas_mut().alloc(8, 8).unwrap();
        canvas.atlas_mut().write_alpha(glyph, &[200; 64]);

        let mut gpu = Mock::default();
        canvas.begin(1280, 720);
        canvas.fill_rect(Rect::new(0.0, 0.0, 100.0, 100.0), RED);
        canvas.sprite(glyph, Rect::new(10.0, 10.0, 8.0, 8.0), HALF_WHITE);
        canvas.set_additive(true);
        canvas.fill_rect_hgradient(Rect::new(0.0, 0.0, 50.0, 4.0), RED, HALF_WHITE);
        canvas.set_additive(false);
        canvas.fill_triangle([(0.0, 0.0), (5.0, 0.0), (0.0, 5.0)], [RED; 3]);
        assert_eq!(canvas.end(&mut gpu), 1);

        let draws = gpu.draws();
        assert_eq!(draws[0].3, BlendMode::Premultiplied);
        assert!(draws[0].2.is_some());
        // atlas upload must precede the first draw
        assert!(matches!(gpu.calls[0], Call::Create));
        // additive vertices carry alpha 0, normal ones keep it
        assert_eq!(draws[0].0[8].color[3], 0.0);
        assert_eq!(draws[0].0[0].color, [1.0, 0.0, 0.0, 1.0]);
        // premultiplied: 50% white → rgb ≈ 0.5
        let c = draws[0].0[4].color;
        assert!((c[0] - 0.5).abs() < 0.01 && (c[3] - 0.5).abs() < 0.01);
    }

    #[test]
    fn external_texture_breaks_batch_only_where_needed() {
        let mut canvas = Canvas::new(256);
        let mut gpu = Mock::default();
        canvas.begin(1280, 720);
        canvas.fill_rect(Rect::new(0.0, 0.0, 10.0, 10.0), RED);
        canvas.image(TextureId(99), Rect::new(0.0, 0.0, 10.0, 10.0), [0.0, 0.0, 1.0, 1.0], RED);
        canvas.image(TextureId(99), Rect::new(20.0, 0.0, 10.0, 10.0), [0.0, 0.0, 1.0, 1.0], RED);
        canvas.fill_rect(Rect::new(0.0, 0.0, 10.0, 10.0), RED);
        assert_eq!(canvas.end(&mut gpu), 3);
        let draws = gpu.draws();
        assert_eq!(draws[1].2, Some(TextureId(99)));
        assert_eq!(draws[1].0.len(), 8, "both images merged");
    }

    #[test]
    fn splits_batches_at_backend_buffer_limits() {
        let mut canvas = Canvas::new(256);
        let mut gpu = Mock::default();
        canvas.begin(1280, 720);
        for i in 0..3000 {
            canvas.fill_rect(Rect::new(i as f32, 0.0, 1.0, 1.0), RED);
        }
        assert_eq!(canvas.end(&mut gpu), 2);
        for (v, idx, _, _) in gpu.draws() {
            assert!(v.len() <= MAX_BATCH_VERTICES && idx.len() <= MAX_BATCH_INDICES);
            assert!(idx.iter().all(|&i| (i as usize) < v.len()), "indices are batch-local");
        }
    }

    #[test]
    fn clipping_trims_geometry_and_interpolates_uv_and_color() {
        let mut canvas = Canvas::new(256);
        let mut gpu = Mock::default();
        canvas.begin(1280, 720);
        canvas.push_clip(Rect::new(0.0, 0.0, 50.0, 100.0));
        canvas.image(
            TextureId(7),
            Rect::new(0.0, 0.0, 100.0, 10.0),
            [0.0, 0.0, 1.0, 1.0],
            RED,
        );
        canvas.fill_rect_hgradient(
            Rect::new(0.0, 20.0, 100.0, 10.0),
            ColorRgba::new(0, 0, 0, 255),
            ColorRgba::new(255, 255, 255, 255),
        );
        canvas.fill_rect(Rect::new(60.0, 0.0, 10.0, 10.0), RED); // fully outside
        canvas.pop_clip();
        canvas.end(&mut gpu);

        let draws = gpu.draws();
        let img = draws[0].0;
        assert!(img.iter().all(|v| v.position[0] <= 50.0));
        let max_u = img.iter().map(|v| v.uv[0]).fold(0.0, f32::max);
        assert!((max_u - 0.5).abs() < 1e-4, "uv clipped to half: {max_u}");

        let grad = draws[1].0;
        assert_eq!(grad.len(), 4, "fully-outside rect emitted nothing");
        let right = grad.iter().find(|v| v.position[0] == 50.0).unwrap();
        assert!((right.color[0] - 0.5).abs() < 0.01, "color interpolated at clip edge");
    }

    #[test]
    fn nine_slice_keeps_corners_and_shrinks_when_too_small() {
        let mut canvas = Canvas::new(256);
        let panel = canvas.atlas_mut().alloc(16, 16).unwrap();
        let mut gpu = Mock::default();
        canvas.begin(1280, 720);
        canvas.nine_slice(
            panel,
            Insets::uniform(4.0),
            2.0,
            Rect::new(0.0, 0.0, 200.0, 100.0),
            RED,
        );
        canvas.end(&mut gpu);
        let v = gpu.draws()[0].0.clone();
        assert_eq!(v.len(), 9 * 4);
        // top-left corner quad spans 4 texels × scale 2 = 8 px
        assert_eq!(v[2].position, [8.0, 8.0]);

        canvas.begin(1280, 720);
        canvas.nine_slice(panel, Insets::uniform(4.0), 2.0, Rect::new(0.0, 0.0, 8.0, 8.0), RED);
        gpu.calls.clear();
        canvas.end(&mut gpu);
        let v = gpu.draws()[0].0.clone();
        assert!(v.iter().all(|p| p.position[0] <= 8.0 && p.position[1] <= 8.0));
    }

    #[test]
    fn atlas_reuploads_only_when_dirty() {
        let mut canvas = Canvas::new(256);
        let mut gpu = Mock::default();
        for _ in 0..2 {
            canvas.begin(1280, 720);
            canvas.fill_rect(Rect::new(0.0, 0.0, 1.0, 1.0), RED);
            canvas.end(&mut gpu);
        }
        let uploads = |g: &Mock| {
            g.calls
                .iter()
                .filter(|c| matches!(c, Call::Create | Call::Update))
                .count()
        };
        assert_eq!(uploads(&gpu), 1);
        let r = canvas.atlas_mut().alloc(2, 2).unwrap();
        canvas.atlas_mut().write_alpha(r, &[1; 4]);
        canvas.begin(1280, 720);
        canvas.end(&mut gpu);
        assert_eq!(uploads(&gpu), 2);
    }
}

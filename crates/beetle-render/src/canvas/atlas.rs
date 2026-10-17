//! Shared UI texture atlas: generated skin sprites and text glyphs live in
//! the same GPU texture pages so the Canvas can draw them in any order
//! without breaking the batch (ADR-026).
//!
//! Pixels are stored **premultiplied** RGBA8. Untouched texels are
//! transparent black (0,0,0,0), which is the correct premultiplied
//! "nothing", so bilinear sampling never bleeds a dark fringe.

use crate::backend::{GpuBackend, TextureId};

/// Transparent border kept around every allocation so bilinear filtering
/// never samples a neighbouring sprite.
pub const GUTTER: u32 = 1;

/// Upper bound on pages; allocation fails beyond this instead of growing
/// GPU memory without limit.
const MAX_PAGES: usize = 8;

/// A packed rectangle inside one atlas page, in texels (gutter excluded).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtlasRegion {
    pub page: u16,
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

struct Shelf {
    y: u32,
    h: u32,
    cursor_x: u32,
}

/// A half-open texel rectangle `[x0, x1) × [y0, y1)` inside one atlas page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRect {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

impl DirtyRect {
    /// The texels of `r` (gutter excluded).
    pub fn of(r: AtlasRegion) -> Self {
        let (x0, y0) = (r.x as u32, r.y as u32);
        Self {
            x0,
            y0,
            x1: x0 + r.w as u32,
            y1: y0 + r.h as u32,
        }
    }

    /// The smallest rectangle covering both.
    pub fn union(self, other: Self) -> Self {
        Self {
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }

    /// The part inside a `size`×`size` page, `None` when nothing is left.
    pub fn clamp_to(self, size: u32) -> Option<Self> {
        let clamped = Self {
            x0: self.x0.min(size),
            y0: self.y0.min(size),
            x1: self.x1.min(size),
            y1: self.y1.min(size),
        };
        (clamped.x0 < clamped.x1 && clamped.y0 < clamped.y1).then_some(clamped)
    }

    /// `(x, y, w, h)`, the form the backend takes.
    pub fn xywh(self) -> [u32; 4] {
        [self.x0, self.y0, self.x1 - self.x0, self.y1 - self.y0]
    }

    /// Bytes of RGBA8 texels covered.
    pub fn bytes(self) -> usize {
        ((self.x1 - self.x0) * (self.y1 - self.y0) * 4) as usize
    }
}

struct Page {
    pixels: Vec<u8>,
    shelves: Vec<Shelf>,
    next_shelf_y: u32,
    texture: Option<TextureId>,
    /// The whole page must be (re)uploaded: new page, cleared, or released.
    full: bool,
    /// Union of the texels written since the last sync (`None` when clean).
    dirty: Option<DirtyRect>,
}

impl Page {
    fn new(size: u32) -> Self {
        Self {
            pixels: vec![0; (size * size * 4) as usize],
            shelves: Vec::new(),
            next_shelf_y: 0,
            texture: None,
            full: true,
            dirty: None,
        }
    }

    fn mark_dirty(&mut self, r: AtlasRegion) {
        let rect = DirtyRect::of(r);
        self.dirty = Some(self.dirty.map_or(rect, |d| d.union(rect)));
    }

    /// Shelf packing: reuse a shelf whose height fits without wasting more
    /// than ~25%, otherwise open a new shelf below the last one.
    fn alloc(&mut self, size: u32, pw: u32, ph: u32) -> Option<(u32, u32)> {
        for shelf in &mut self.shelves {
            if ph <= shelf.h && shelf.h <= ph + ph / 4 + 2 && shelf.cursor_x + pw <= size {
                let x = shelf.cursor_x;
                shelf.cursor_x += pw;
                return Some((x, shelf.y));
            }
        }
        if self.next_shelf_y + ph > size || pw > size {
            return None;
        }
        let y = self.next_shelf_y;
        self.next_shelf_y += ph;
        self.shelves.push(Shelf {
            y,
            h: ph,
            cursor_x: pw,
        });
        Some((0, y))
    }
}

pub struct UiAtlas {
    size: u32,
    pages: Vec<Page>,
}

impl UiAtlas {
    pub fn new(page_size: u32) -> Self {
        Self {
            size: page_size,
            pages: Vec::new(),
        }
    }

    pub fn page_size(&self) -> u32 {
        self.size
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Reserves a `w`×`h` texel region. Returns `None` if it can never fit
    /// or all pages are full.
    pub fn alloc(&mut self, w: u32, h: u32) -> Option<AtlasRegion> {
        let (pw, ph) = (w + 2 * GUTTER, h + 2 * GUTTER);
        if w == 0 || h == 0 || pw > self.size || ph > self.size {
            return None;
        }
        let size = self.size;
        let found = self
            .pages
            .iter_mut()
            .enumerate()
            .find_map(|(i, p)| p.alloc(size, pw, ph).map(|pos| (i, pos)));
        let (page, (x, y)) = match found {
            Some(hit) => hit,
            None if self.pages.len() < MAX_PAGES => {
                let mut p = Page::new(size);
                let pos = p.alloc(size, pw, ph)?;
                self.pages.push(p);
                (self.pages.len() - 1, pos)
            }
            None => return None,
        };
        Some(AtlasRegion {
            page: page as u16,
            x: (x + GUTTER) as u16,
            y: (y + GUTTER) as u16,
            w: w as u16,
            h: h as u16,
        })
    }

    /// Writes premultiplied RGBA8 pixels (`w*h*4` bytes, tightly packed).
    pub fn write_rgba(&mut self, r: AtlasRegion, premultiplied_rgba: &[u8]) {
        let row = r.w as usize * 4;
        assert_eq!(premultiplied_rgba.len(), row * r.h as usize);
        let size = self.size as usize;
        let page = &mut self.pages[r.page as usize];
        for (j, src) in premultiplied_rgba.chunks_exact(row).enumerate() {
            let dst = ((r.y as usize + j) * size + r.x as usize) * 4;
            page.pixels[dst..dst + row].copy_from_slice(src);
        }
        page.mark_dirty(r);
    }

    /// Writes a coverage mask as premultiplied white (`a,a,a,a`) — glyphs and
    /// single-color shapes, tinted by the vertex color when drawn.
    pub fn write_alpha(&mut self, r: AtlasRegion, coverage: &[u8]) {
        assert_eq!(coverage.len(), r.w as usize * r.h as usize);
        let size = self.size as usize;
        let page = &mut self.pages[r.page as usize];
        for (j, src) in coverage.chunks_exact(r.w as usize).enumerate() {
            let dst = ((r.y as usize + j) * size + r.x as usize) * 4;
            for (i, &a) in src.iter().enumerate() {
                page.pixels[dst + i * 4..dst + i * 4 + 4].copy_from_slice(&[a, a, a, a]);
            }
        }
        page.mark_dirty(r);
    }

    /// Normalized `[u0, v0, u1, v1]` of a sub-rectangle given in region texels.
    pub fn uv(&self, r: AtlasRegion, sx: f32, sy: f32, sw: f32, sh: f32) -> [f32; 4] {
        let inv = 1.0 / self.size as f32;
        let (x, y) = (r.x as f32 + sx, r.y as f32 + sy);
        [x * inv, y * inv, (x + sw) * inv, (y + sh) * inv]
    }

    pub fn page_texture(&self, page: u16) -> Option<TextureId> {
        self.pages.get(page as usize).and_then(|p| p.texture)
    }

    /// Creates textures for new pages and uploads what changed since the last
    /// call: only the dirty rectangle of a page that has a texture, the whole
    /// page when it is new or was cleared. Must run before any batch
    /// referencing the atlas is submitted. Returns the bytes handed to the
    /// backend.
    pub fn sync(&mut self, backend: &mut dyn GpuBackend) -> usize {
        let size = self.size;
        let whole = (size * size * 4) as usize;
        let mut bytes = 0;
        for page in &mut self.pages {
            if !page.full && page.dirty.is_none() {
                continue;
            }
            match page.texture {
                Some(id) if !page.full => {
                    if let Some(rect) = page.dirty.and_then(|d| d.clamp_to(size)) {
                        backend.update_texture_rect(id, size, size, rect.xywh(), &page.pixels);
                        bytes += rect.bytes();
                    }
                }
                Some(id) => {
                    backend.update_texture(id, size, size, &page.pixels);
                    bytes += whole;
                }
                None => {
                    page.texture = backend.create_texture(size, size, &page.pixels);
                    bytes += whole;
                }
            }
            page.full = false;
            page.dirty = None;
        }
        bytes
    }

    /// Forgets every allocation and zeroes all pages (GPU textures are kept
    /// and re-uploaded on the next `sync`). Used when the UI scale changes
    /// and all sprites/glyphs are regenerated.
    pub fn clear(&mut self) {
        for page in &mut self.pages {
            page.pixels.fill(0);
            page.shelves.clear();
            page.next_shelf_y = 0;
            page.full = true;
            page.dirty = None;
        }
    }

    /// Frees GPU textures (e.g. before the backend is dropped or recreated).
    /// Pages are marked for a full upload so the next `sync` re-creates them.
    pub fn release(&mut self, backend: &mut dyn GpuBackend) {
        for page in &mut self.pages {
            if let Some(id) = page.texture.take() {
                backend.destroy_texture(id);
            }
            page.full = true;
            page.dirty = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlaps(a: &AtlasRegion, b: &AtlasRegion) -> bool {
        a.page == b.page && a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    #[test]
    fn allocations_never_overlap_and_keep_gutter() {
        let mut atlas = UiAtlas::new(128);
        let mut regions = Vec::new();
        for i in 0..60 {
            let (w, h) = (5 + (i * 7) % 23, 6 + (i * 5) % 17);
            regions.push(atlas.alloc(w, h).expect("fits"));
        }
        for (i, a) in regions.iter().enumerate() {
            assert!(a.x >= 1 && a.y >= 1);
            assert!(a.x as u32 + a.w as u32 + GUTTER <= 128);
            for b in &regions[i + 1..] {
                let grown = AtlasRegion {
                    x: a.x - 1,
                    y: a.y - 1,
                    w: a.w + 2,
                    h: a.h + 2,
                    ..*a
                };
                assert!(!overlaps(&grown, b), "{a:?} touches {b:?}");
            }
        }
        assert!(atlas.page_count() >= 2, "should have spilled to a 2nd page");
    }

    #[test]
    fn rejects_oversized_and_caps_pages() {
        let mut atlas = UiAtlas::new(64);
        assert!(atlas.alloc(63, 1).is_none());
        for _ in 0..MAX_PAGES {
            assert!(atlas.alloc(62, 62).is_some());
        }
        assert!(atlas.alloc(62, 62).is_none());
    }

    /// Records what the atlas asks of the backend: `(kind, rect)` per call.
    #[derive(Default)]
    struct Recorder {
        calls: Vec<(&'static str, Option<[u32; 4]>)>,
        next: u32,
    }

    impl GpuBackend for Recorder {
        fn begin_frame(&mut self, _: u32, _: u32, _: [f32; 4]) {}
        fn create_texture(&mut self, _: u32, _: u32, _: &[u8]) -> Option<TextureId> {
            self.calls.push(("create", None));
            self.next += 1;
            Some(TextureId(self.next))
        }
        fn update_texture(&mut self, _: TextureId, _: u32, _: u32, _: &[u8]) {
            self.calls.push(("update", None));
        }
        fn update_texture_rect(&mut self, _: TextureId, _: u32, _: u32, r: [u32; 4], _: &[u8]) {
            self.calls.push(("rect", Some(r)));
        }
        fn destroy_texture(&mut self, _: TextureId) {}
        fn draw_batch(
            &mut self,
            _: &[crate::backend::Vertex2D],
            _: &[u16],
            _: Option<TextureId>,
            _: crate::backend::BlendMode,
        ) {
        }
        fn end_frame(&mut self) {}
        fn resize(&mut self, _: u32, _: u32) {}
        fn backend_name(&self) -> &'static str {
            "recorder"
        }
    }

    #[test]
    fn dirty_rects_union_and_clamp() {
        let r = |x: u16, y: u16, w: u16, h: u16| AtlasRegion {
            page: 0,
            x,
            y,
            w,
            h,
        };
        let a = DirtyRect::of(r(10, 20, 4, 6));
        assert_eq!((a.x0, a.y0, a.x1, a.y1), (10, 20, 14, 26));
        let b = DirtyRect::of(r(30, 5, 2, 2));
        let u = a.union(b);
        assert_eq!((u.x0, u.y0, u.x1, u.y1), (10, 5, 32, 26));
        assert_eq!(u.xywh(), [10, 5, 22, 21]);
        assert_eq!(a.bytes(), 4 * 6 * 4);
        // Clamped to the page; a rect fully outside is dropped.
        let far = DirtyRect {
            x0: 120,
            y0: 120,
            x1: 200,
            y1: 200,
        };
        assert_eq!(
            far.clamp_to(128),
            Some(DirtyRect {
                x0: 120,
                y0: 120,
                x1: 128,
                y1: 128
            })
        );
        assert_eq!(
            DirtyRect {
                x0: 128,
                y0: 0,
                x1: 140,
                y1: 4
            }
            .clamp_to(128),
            None
        );
    }

    #[test]
    fn sync_uploads_the_whole_page_once_then_only_dirty_rects() {
        let mut atlas = UiAtlas::new(128);
        let mut gpu = Recorder::default();
        let a = atlas.alloc(16, 16).unwrap();
        let b = atlas.alloc(16, 16).unwrap();
        atlas.write_rgba(a, &[7; 16 * 16 * 4]);
        // First sync: the page is new, so it is created whole.
        assert_eq!(atlas.sync(&mut gpu), 128 * 128 * 4);
        assert_eq!(gpu.calls, vec![("create", None)]);
        // Nothing changed: nothing uploaded.
        assert_eq!(atlas.sync(&mut gpu), 0);

        // One thumbnail-sized write: only its 16×16 texels go up.
        gpu.calls.clear();
        atlas.write_rgba(b, &[9; 16 * 16 * 4]);
        assert_eq!(atlas.sync(&mut gpu), 16 * 16 * 4);
        assert_eq!(
            gpu.calls,
            vec![("rect", Some([b.x as u32, b.y as u32, 16, 16]))]
        );

        // Two writes in one frame upload their union: `a` and `b` sit side by
        // side on one shelf (x 1..17 and 19..35, y 1..17), so the union is 34×16.
        gpu.calls.clear();
        atlas.write_rgba(a, &[1; 16 * 16 * 4]);
        atlas.write_alpha(b, &[200; 16 * 16]);
        assert_eq!(atlas.sync(&mut gpu), 34 * 16 * 4);
        assert_eq!(gpu.calls, vec![("rect", Some([1, 1, 34, 16]))]);

        // Clearing forces a whole-page upload again.
        gpu.calls.clear();
        atlas.clear();
        assert_eq!(atlas.sync(&mut gpu), 128 * 128 * 4);
        assert_eq!(gpu.calls, vec![("update", None)]);
    }

    #[test]
    fn write_alpha_is_premultiplied_white() {
        let mut atlas = UiAtlas::new(16);
        let r = atlas.alloc(2, 1).unwrap();
        atlas.write_alpha(r, &[255, 64]);
        let px = &atlas.pages[0].pixels;
        let at = |x: usize, y: usize| &px[(y * 16 + x) * 4..(y * 16 + x) * 4 + 4];
        assert_eq!(at(r.x as usize, r.y as usize), &[255, 255, 255, 255]);
        assert_eq!(at(r.x as usize + 1, r.y as usize), &[64, 64, 64, 64]);
        assert_eq!(at(0, 0), &[0, 0, 0, 0], "gutter stays transparent");
    }
}

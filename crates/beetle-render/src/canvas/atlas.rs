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

struct Page {
    pixels: Vec<u8>,
    shelves: Vec<Shelf>,
    next_shelf_y: u32,
    texture: Option<TextureId>,
    dirty: bool,
}

impl Page {
    fn new(size: u32) -> Self {
        Self {
            pixels: vec![0; (size * size * 4) as usize],
            shelves: Vec::new(),
            next_shelf_y: 0,
            texture: None,
            dirty: true,
        }
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
        page.dirty = true;
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
        page.dirty = true;
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

    /// Creates textures for new pages and re-uploads dirty ones. Must run
    /// before any batch referencing the atlas is submitted.
    pub fn sync(&mut self, backend: &mut dyn GpuBackend) {
        let size = self.size;
        for page in &mut self.pages {
            if !page.dirty {
                continue;
            }
            match page.texture {
                Some(id) => backend.update_texture(id, size, size, &page.pixels),
                None => page.texture = backend.create_texture(size, size, &page.pixels),
            }
            page.dirty = false;
        }
    }

    /// Frees GPU textures (e.g. before the backend is dropped or recreated).
    /// Pages are marked dirty so the next `sync` re-creates them.
    pub fn release(&mut self, backend: &mut dyn GpuBackend) {
        for page in &mut self.pages {
            if let Some(id) = page.texture.take() {
                backend.destroy_texture(id);
            }
            page.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlaps(a: &AtlasRegion, b: &AtlasRegion) -> bool {
        a.page == b.page
            && a.x < b.x + b.w
            && b.x < a.x + a.w
            && a.y < b.y + b.h
            && b.y < a.y + a.h
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

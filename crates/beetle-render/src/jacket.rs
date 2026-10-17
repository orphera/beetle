//! Song jacket thumbnails for the select list (P2-2).
//!
//! A fixed grid of 80×80 slots lives in the shared UI atlas, next to the skin
//! sprites and glyphs. Because it is the same atlas page, a row jacket is one
//! more textured quad in the screen's main batch: thumbnails add no batches.
//! Slots are handed out by a least-recently-used map (`SlotLru`), so the
//! number of resident thumbnails is bounded at `SLOTS` whatever the library
//! size.
//!
//! Pixels are decoded and downscaled off the render thread by the app
//! (`beetle-app/src/thumbs.rs`); this module only does the pure pixel math
//! (centre-crop and box downscale, procedural placeholder) and slot bookkeeping.

use crate::canvas::{AtlasRegion, UiAtlas};
use crate::image::ImageBuffer;
use crate::skin::ColorRgba;
use crate::theme;
use beetle_core::ChartId;
use std::collections::HashMap;
use std::hash::Hash;

/// Edge of one thumbnail, in texels.
pub const THUMB_PX: u32 = 80;
/// Texels between neighbouring slots, so bilinear sampling never reads the
/// next thumbnail.
const GUTTER: u32 = 1;
const PITCH: u32 = THUMB_PX + 2 * GUTTER;
/// Slot grid: 10 × 6 = 60 resident thumbnails. The grid is about 980×588
/// texels, so it stays on the first atlas page at every UI scale the app
/// supports (0.75 to 3.0), and the list's prefetch range fits in it.
pub const COLS: u32 = 10;
pub const ROWS: u32 = 6;
pub const SLOTS: usize = (COLS * ROWS) as usize;

/// What a slot holds: the artwork of one chart, or a procedural placeholder
/// keyed by the hash of its title and artist (see [`jacket_seed`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JacketKey {
    Chart(ChartId),
    Placeholder(u64),
}

/// Least-recently-used map from keys to slot indices. Pure bookkeeping, no
/// atlas involved.
pub struct SlotLru<K: Copy + Eq + Hash> {
    keys: Vec<Option<K>>,
    stamps: Vec<u64>,
    index: HashMap<K, usize>,
    tick: u64,
}

impl<K: Copy + Eq + Hash> SlotLru<K> {
    pub fn new(slots: usize) -> Self {
        Self {
            keys: vec![None; slots],
            stamps: vec![0; slots],
            index: HashMap::new(),
            tick: 0,
        }
    }

    /// The slot holding `key`, marking it most recently used.
    pub fn get(&mut self, key: &K) -> Option<usize> {
        let slot = *self.index.get(key)?;
        self.tick += 1;
        self.stamps[slot] = self.tick;
        Some(slot)
    }

    pub fn contains(&self, key: &K) -> bool {
        self.index.contains_key(key)
    }

    /// Gives `key` (not present yet) a slot: a free one, else the least
    /// recently used slot, whose previous key is returned as evicted.
    pub fn insert(&mut self, key: K) -> (usize, Option<K>) {
        self.tick += 1;
        let (slot, evicted) = match self.keys.iter().position(Option::is_none) {
            Some(free) => (free, None),
            None => {
                let lru = (0..self.keys.len())
                    .min_by_key(|&i| self.stamps[i])
                    .unwrap_or(0);
                let old = self.keys[lru].take();
                if let Some(old) = old {
                    self.index.remove(&old);
                }
                (lru, old)
            }
        };
        self.keys[slot] = Some(key);
        self.stamps[slot] = self.tick;
        self.index.insert(key, slot);
        (slot, evicted)
    }

    pub fn clear(&mut self) {
        self.keys.iter_mut().for_each(|k| *k = None);
        self.index.clear();
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }
}

/// The thumbnail slots, in one region of the shared atlas.
pub struct JacketAtlas {
    base: Option<AtlasRegion>,
    lru: SlotLru<JacketKey>,
}

impl JacketAtlas {
    /// Reserves the slot grid in `atlas`. Call right after the skin is
    /// generated so the grid lands on the same page as the sprites.
    pub fn new(atlas: &mut UiAtlas) -> Self {
        Self {
            base: atlas.alloc(COLS * PITCH, ROWS * PITCH),
            lru: SlotLru::new(SLOTS),
        }
    }

    fn slot_region(base: AtlasRegion, slot: usize) -> AtlasRegion {
        let (col, row) = (slot as u32 % COLS, slot as u32 / COLS);
        AtlasRegion {
            page: base.page,
            x: base.x + (col * PITCH + GUTTER) as u16,
            y: base.y + (row * PITCH + GUTTER) as u16,
            w: THUMB_PX as u16,
            h: THUMB_PX as u16,
        }
    }

    /// The region of `key`'s thumbnail, marking it recently used.
    pub fn get(&mut self, key: JacketKey) -> Option<AtlasRegion> {
        let base = self.base?;
        let slot = self.lru.get(&key)?;
        Some(Self::slot_region(base, slot))
    }

    pub fn contains(&self, key: JacketKey) -> bool {
        self.lru.contains(&key)
    }

    /// The atlas page the grid sits on (`None` when it could not be reserved).
    pub fn page(&self) -> Option<u16> {
        self.base.map(|b| b.page)
    }

    /// Stores premultiplied RGBA8 pixels (`THUMB_PX`² × 4 bytes) for `key`,
    /// evicting the least recently used thumbnail when the grid is full.
    /// Returns `false` when there is no grid or the pixels have the wrong size.
    pub fn insert(&mut self, atlas: &mut UiAtlas, key: JacketKey, pixels: &[u8]) -> bool {
        let Some(base) = self.base else {
            return false;
        };
        if pixels.len() != (THUMB_PX * THUMB_PX * 4) as usize {
            return false;
        }
        let slot = match self.lru.get(&key) {
            Some(slot) => slot,
            None => self.lru.insert(key).0,
        };
        atlas.write_rgba(Self::slot_region(base, slot), pixels);
        true
    }

    /// Makes sure the placeholder for `seed` is resident, generating it on a miss.
    pub fn ensure_placeholder(&mut self, atlas: &mut UiAtlas, seed: u64) -> bool {
        let key = JacketKey::Placeholder(seed);
        if self.contains(key) {
            return true;
        }
        self.insert(atlas, key, &placeholder_pixels(seed))
    }

    /// The jacket to draw for a song: its artwork if resident, else the
    /// placeholder for its seed. The flag is `true` for a placeholder.
    pub fn lookup(&mut self, id: ChartId, seed: u64) -> Option<(AtlasRegion, bool)> {
        if let Some(r) = self.get(JacketKey::Chart(id)) {
            return Some((r, false));
        }
        self.get(JacketKey::Placeholder(seed)).map(|r| (r, true))
    }

    /// Forgets every resident thumbnail (the pixels stay in the atlas until
    /// their slots are reused).
    pub fn clear(&mut self) {
        self.lru.clear();
    }
}

/// Square crop of the largest centred square: `(x, y, side)`.
pub fn centre_square(w: u32, h: u32) -> (u32, u32, u32) {
    let side = w.min(h);
    ((w - side) / 2, (h - side) / 2, side)
}

/// Centre-crops `img` to a square and box-filters it down to
/// `THUMB_PX`² premultiplied RGBA8. `None` for an empty image.
pub fn thumb_pixels(img: &ImageBuffer) -> Option<Vec<u8>> {
    let (w, h) = (img.width, img.height);
    if w == 0 || h == 0 || img.pixels.len() < (w as usize) * (h as usize) {
        return None;
    }
    let (x0, y0, side) = centre_square(w, h);
    let n = THUMB_PX as usize;
    let mut out = vec![0u8; n * n * 4];
    let edge = |start: u32, t: u32| start + t * side / THUMB_PX;
    for ty in 0..THUMB_PX {
        let sy0 = edge(y0, ty);
        let sy1 = edge(y0, ty + 1).max(sy0 + 1);
        for tx in 0..THUMB_PX {
            let sx0 = edge(x0, tx);
            let sx1 = edge(x0, tx + 1).max(sx0 + 1);
            let (mut r, mut g, mut b, mut a, mut count) = (0u32, 0u32, 0u32, 0u32, 0u32);
            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let p = img.pixels[(sy * w + sx) as usize];
                    let pa = p.a as u32;
                    r += p.r as u32 * pa / 255;
                    g += p.g as u32 * pa / 255;
                    b += p.b as u32 * pa / 255;
                    a += pa;
                    count += 1;
                }
            }
            let o = (ty as usize * n + tx as usize) * 4;
            let avg = |v: u32| ((v + count / 2) / count) as u8;
            out[o..o + 4].copy_from_slice(&[avg(r), avg(g), avg(b), avg(a)]);
        }
    }
    Some(out)
}

/// Seed of a song's placeholder: FNV-1a over title, separator, artist. Stable
/// across runs and builds, unlike `DefaultHasher`.
pub fn jacket_seed(title: &str, artist: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |bytes: &[u8]| {
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    mix(title.as_bytes());
    mix(&[0xff]);
    mix(artist.as_bytes());
    h
}

/// The line pattern of a placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pattern {
    DiagonalStripes,
    Rings,
    DotGrid,
    SplitDiagonal,
    Chevrons,
}

/// Pattern and the two hues of a placeholder, all derived from its seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaceholderLook {
    pub pattern: Pattern,
    pub hue_a: usize,
    pub hue_b: usize,
}

/// The hues placeholders pick from: the theme's accent colours.
pub const PLACEHOLDER_HUES: [ColorRgba; 10] = [
    theme::CYAN,
    theme::MAGENTA,
    theme::PURPLE,
    theme::GOLD,
    theme::ORANGE,
    theme::GREEN,
    theme::BLUE,
    theme::RED,
    theme::FAST,
    theme::SLOW,
];

pub fn placeholder_look(seed: u64) -> PlaceholderLook {
    let n = PLACEHOLDER_HUES.len() as u64;
    let hue_a = (seed % n) as usize;
    // A different hue: an offset of 1..n from the first.
    let hue_b = (hue_a + 1 + ((seed >> 8) % (n - 1)) as usize) % PLACEHOLDER_HUES.len();
    let pattern = match (seed >> 20) % 5 {
        0 => Pattern::DiagonalStripes,
        1 => Pattern::Rings,
        2 => Pattern::DotGrid,
        3 => Pattern::SplitDiagonal,
        _ => Pattern::Chevrons,
    };
    PlaceholderLook {
        pattern,
        hue_a,
        hue_b,
    }
}

const INK: ColorRgba = ColorRgba::new(0x0b, 0x0e, 0x16, 255);

fn lerp(a: ColorRgba, b: ColorRgba, t: f32) -> ColorRgba {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    ColorRgba::new(l(a.r, b.r), l(a.g, b.g), l(a.b, b.b), 255)
}

/// Colour of one texel of `pattern`. `x`/`y` are texel positions.
fn pattern_colour(pattern: Pattern, x: f32, y: f32, a: ColorRgba, b: ColorRgba) -> ColorRgba {
    let n = THUMB_PX as f32;
    let field_a = lerp(INK, a, 0.82);
    let field_b = lerp(INK, b, 0.82);
    match pattern {
        Pattern::DiagonalStripes => {
            if ((x + y) / 14.0).floor() as i32 % 2 == 0 {
                field_a
            } else {
                field_b
            }
        }
        Pattern::Rings => {
            let r = ((x - n / 2.0).powi(2) + (y - n / 2.0).powi(2)).sqrt();
            if (r / 9.0).floor() as i32 % 2 == 0 {
                field_a
            } else {
                field_b
            }
        }
        Pattern::DotGrid => {
            let (cx, cy) = (x % 16.0 - 8.0, y % 16.0 - 8.0);
            if (cx * cx + cy * cy).sqrt() < 4.5 {
                lerp(field_b, b, 0.6)
            } else {
                lerp(INK, a, 0.3)
            }
        }
        Pattern::SplitDiagonal => {
            if x + y < n {
                field_a
            } else {
                field_b
            }
        }
        Pattern::Chevrons => {
            let zig = ((x % 24.0) - 12.0).abs();
            if ((y + zig) / 12.0).floor() as i32 % 2 == 0 {
                field_a
            } else {
                field_b
            }
        }
    }
}

/// The placeholder for `seed` as premultiplied (opaque) RGBA8, `THUMB_PX`² texels.
/// A slight darkening towards the bottom-right gives it depth.
pub fn placeholder_pixels(seed: u64) -> Vec<u8> {
    let look = placeholder_look(seed);
    let (a, b) = (PLACEHOLDER_HUES[look.hue_a], PLACEHOLDER_HUES[look.hue_b]);
    let n = THUMB_PX as usize;
    let mut out = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        for x in 0..n {
            let c = pattern_colour(look.pattern, x as f32 + 0.5, y as f32 + 0.5, a, b);
            let k = 1.0 - 0.22 * (x + y) as f32 / (2 * n) as f32;
            let s = |v: u8| (v as f32 * k).round() as u8;
            out.extend_from_slice(&[s(c.r), s(c.g), s(c.b), 255]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn lru_evicts_least_recently_used_and_keeps_touched_keys() {
        let mut lru: SlotLru<u32> = SlotLru::new(3);
        assert_eq!(lru.insert(1), (0, None));
        assert_eq!(lru.insert(2), (1, None));
        assert_eq!(lru.insert(3), (2, None));
        // Touch 1 so 2 becomes the oldest.
        assert_eq!(lru.get(&1), Some(0));
        let (slot, evicted) = lru.insert(4);
        assert_eq!((slot, evicted), (1, Some(2)));
        assert!(!lru.contains(&2));
        assert!(lru.contains(&1) && lru.contains(&3) && lru.contains(&4));
        assert_eq!(lru.len(), 3);
        lru.clear();
        assert!(lru.is_empty());
    }

    #[test]
    fn lru_reuses_slots_in_order_as_keys_rotate() {
        let mut lru: SlotLru<u32> = SlotLru::new(2);
        lru.insert(10);
        lru.insert(11);
        let evicted: Vec<u32> = (12..16).filter_map(|k| lru.insert(k).1).collect();
        assert_eq!(evicted, vec![10, 11, 12, 13]);
        assert!(lru.contains(&14) && lru.contains(&15));
    }

    #[test]
    fn centre_square_crops_the_long_side() {
        assert_eq!(centre_square(200, 100), (50, 0, 100));
        assert_eq!(centre_square(100, 300), (0, 100, 100));
        assert_eq!(centre_square(64, 64), (0, 0, 64));
    }

    #[test]
    fn thumbnail_is_centre_cropped_and_box_filtered() {
        // 4 wide, 2 high: the crop is the middle 2×2 (columns 1..3).
        // Columns 1 and 2 are white and red; the box filter averages them.
        let white = ColorRgba::new(255, 255, 255, 255);
        let red = ColorRgba::new(255, 0, 0, 255);
        let black = ColorRgba::new(0, 0, 0, 255);
        let mut img = ImageBuffer::new(4, 2, black);
        for y in 0..2 {
            img.pixels[y * 4] = black;
            img.pixels[y * 4 + 1] = white;
            img.pixels[y * 4 + 2] = red;
            img.pixels[y * 4 + 3] = black;
        }
        let px = thumb_pixels(&img).expect("pixels");
        let n = THUMB_PX as usize;
        assert_eq!(px.len(), n * n * 4);
        // Left half of the thumbnail samples white, right half red.
        assert_eq!(&px[0..4], &[255, 255, 255, 255]);
        let right = (n - 1) * 4;
        assert_eq!(&px[right..right + 4], &[255, 0, 0, 255]);
        // Every texel is opaque.
        assert!(px.chunks_exact(4).all(|p| p[3] == 255));
    }

    #[test]
    fn thumbnail_premultiplies_partial_alpha() {
        let mut img = ImageBuffer::new(1, 1, ColorRgba::new(0, 0, 0, 0));
        img.pixels[0] = ColorRgba::new(200, 100, 0, 128);
        let px = thumb_pixels(&img).expect("pixels");
        assert_eq!(&px[0..4], &[100, 50, 0, 128]);
    }

    #[test]
    fn empty_image_has_no_thumbnail() {
        let img = ImageBuffer {
            width: 0,
            height: 0,
            pixels: Vec::new(),
        };
        assert!(thumb_pixels(&img).is_none());
    }

    #[test]
    fn seeds_are_stable_and_separate_title_from_artist() {
        assert_eq!(jacket_seed("Song", "Artist"), jacket_seed("Song", "Artist"));
        assert_ne!(jacket_seed("ab", "c"), jacket_seed("a", "bc"));
    }

    fn sample_titles() -> Vec<(String, String)> {
        (0..200)
            .map(|i| {
                (
                    format!("Title {i:03} {}", i * 7919 % 97),
                    format!("Artist {}", i % 13),
                )
            })
            .collect()
    }

    #[test]
    fn placeholders_vary_across_songs_and_are_deterministic() {
        let looks: Vec<PlaceholderLook> = sample_titles()
            .iter()
            .map(|(t, a)| placeholder_look(jacket_seed(t, a)))
            .collect();
        let patterns: HashSet<Pattern> = looks.iter().map(|l| l.pattern).collect();
        let pairs: HashSet<(usize, usize)> = looks.iter().map(|l| (l.hue_a, l.hue_b)).collect();
        assert!(patterns.len() >= 5, "patterns: {}", patterns.len());
        assert!(pairs.len() >= 12, "hue pairs: {}", pairs.len());
        assert!(looks.iter().all(|l| l.hue_a != l.hue_b));
        let (t, a) = &sample_titles()[7];
        let seed = jacket_seed(t, a);
        assert_eq!(placeholder_pixels(seed), placeholder_pixels(seed));
        assert_eq!(
            placeholder_pixels(seed).len(),
            (THUMB_PX * THUMB_PX * 4) as usize
        );
    }

    #[test]
    fn slot_regions_tile_the_grid_without_overlap() {
        let base = AtlasRegion {
            page: 0,
            x: 10,
            y: 20,
            w: (COLS * PITCH) as u16,
            h: (ROWS * PITCH) as u16,
        };
        let mut seen = HashSet::new();
        for slot in 0..SLOTS {
            let r = JacketAtlas::slot_region(base, slot);
            assert!(r.x as u32 + r.w as u32 <= base.x as u32 + base.w as u32);
            assert!(r.y as u32 + r.h as u32 <= base.y as u32 + base.h as u32);
            assert!(seen.insert((r.x, r.y)));
        }
        assert_eq!(seen.len(), SLOTS);
    }
}

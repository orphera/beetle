//! Song title and artist word pictures of game discs (pop'n), drawn in the
//! select list in place of the text title.
//!
//! A fixed grid of slots lives in the shared UI atlas, like the jackets. Each
//! slot holds one song chart's title (180×18) and artist (200×30) as alpha
//! coverage, so a word is one more tinted quad in the screen's main batch,
//! the same as a glyph. Slots are handed out least-recently-used
//! (`SlotLru`); the app fills them (`beetle-app/src/words.rs`) and this module
//! only does the slot bookkeeping.

use std::collections::HashMap;

use crate::canvas::{AtlasRegion, UiAtlas};
use crate::jacket::SlotLru;
use beetle_core::ChartId;

/// The size of a title word picture, in texels.
pub const TITLE_SIZE: (u32, u32) = (180, 18);
/// The size of an artist word picture, in texels.
pub const ARTIST_SIZE: (u32, u32) = (200, 30);

/// Texels between the title and the artist within a slot.
const GAP: u32 = 2;
const SLOT_W: u32 = ARTIST_SIZE.0;
const SLOT_H: u32 = TITLE_SIZE.1 + GAP + ARTIST_SIZE.1;
/// Gutter between slots, so bilinear sampling never reads the next word.
const PITCH_X: u32 = SLOT_W + 2;
const PITCH_Y: u32 = SLOT_H + 2;
const COLS: u32 = 8;
const ROWS: u32 = 12;
const SLOTS: usize = (COLS * ROWS) as usize;

/// Where one chart's words are in the atlas. `None` for a word the disc does
/// not have (or a size that does not fit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordRegions {
    pub title: Option<AtlasRegion>,
    pub artist: Option<AtlasRegion>,
}

/// Slot bookkeeping for the word pictures.
pub struct WordAtlas {
    base: Option<AtlasRegion>,
    lru: SlotLru<ChartId>,
    /// Each resident chart's slot and what it holds.
    slots: HashMap<ChartId, (usize, WordRegions)>,
}

impl WordAtlas {
    /// Reserves the slot grid in `atlas`. Call once the skin is generated, like
    /// `JacketAtlas::new`; after an atlas rebuild, make a new one.
    pub fn new(atlas: &mut UiAtlas) -> Self {
        Self {
            base: atlas.alloc_dedicated(COLS * PITCH_X, ROWS * PITCH_Y),
            lru: SlotLru::new(SLOTS),
            slots: HashMap::new(),
        }
    }

    /// Whether `id`'s words are resident.
    pub fn contains(&self, id: ChartId) -> bool {
        self.slots.contains_key(&id)
    }

    /// The resident words of `id`.
    pub fn get(&self, id: ChartId) -> Option<WordRegions> {
        self.slots.get(&id).map(|&(_, regions)| regions)
    }

    /// Writes `id`'s words into its slot (taking the least recently used slot
    /// when it has none). `title` and `artist` are coverage bytes, one a
    /// texel, of `TITLE_SIZE` and `ARTIST_SIZE`; a word of another size is
    /// left out. Returns `false` when the grid is not reserved.
    pub fn insert(
        &mut self,
        atlas: &mut UiAtlas,
        id: ChartId,
        title: Option<&[u8]>,
        artist: Option<&[u8]>,
    ) -> bool {
        let Some(base) = self.base else {
            return false;
        };
        let slot = match self.lru.get(&id) {
            Some(slot) => slot,
            None => {
                let (slot, evicted) = self.lru.insert(id);
                if let Some(old) = evicted {
                    self.slots.remove(&old);
                }
                slot
            }
        };
        let (x, y) = (
            base.x as u32 + (slot as u32 % COLS) * PITCH_X,
            base.y as u32 + (slot as u32 / COLS) * PITCH_Y,
        );
        let title_region = title
            .filter(|c| fits(c, TITLE_SIZE))
            .map(|c| region(atlas, base, x, y, TITLE_SIZE, c));
        let artist_region = artist
            .filter(|c| fits(c, ARTIST_SIZE))
            .map(|c| region(atlas, base, x, y + TITLE_SIZE.1 + GAP, ARTIST_SIZE, c));
        self.slots.insert(
            id,
            (
                slot,
                WordRegions {
                    title: title_region,
                    artist: artist_region,
                },
            ),
        );
        true
    }

    /// Forgets every resident chart (the pixels stay in the atlas until their
    /// slots are reused).
    pub fn clear(&mut self) {
        self.lru.clear();
        self.slots.clear();
    }
}

fn fits(coverage: &[u8], (w, h): (u32, u32)) -> bool {
    coverage.len() == (w * h) as usize
}

/// Writes `coverage` (of `size`) at `(x, y)` of the page `base` lies on.
fn region(
    atlas: &mut UiAtlas,
    base: AtlasRegion,
    x: u32,
    y: u32,
    (w, h): (u32, u32),
    coverage: &[u8],
) -> AtlasRegion {
    let r = AtlasRegion {
        page: base.page,
        x: x as u16,
        y: y as u16,
        w: w as u16,
        h: h as u16,
    };
    atlas.write_alpha(r, coverage);
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_fit_their_grid_and_do_not_overlap() {
        assert!(SLOT_W < PITCH_X && SLOT_H < PITCH_Y);
        assert_eq!(SLOTS, 96);
    }

    #[test]
    fn a_word_of_the_wrong_size_is_left_out() {
        assert!(fits(&[0; 180 * 18], TITLE_SIZE));
        assert!(!fits(&[0; 180 * 17], TITLE_SIZE));
        assert!(!fits(&[0; 200 * 18], ARTIST_SIZE));
    }
}

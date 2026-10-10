//! Click regions. Screens record them while they draw (the layout math
//! lives in the drawing code only); the app hit-tests the list of the last
//! drawn frame, so a click always lands on what the player saw.

use crate::canvas::Rect;

/// What a click on a recorded region means. Each screen records only the
/// ids it can act on; the app maps them to the same actions as the keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitId {
    /// A song row; the index is into the visible (filtered, sorted) list.
    SongRow(usize),
    FolderPrev,
    FolderNext,
    /// The sort selector (cycles the sort mode).
    Sort,
    /// The search box (starts a search).
    Search,
    Play,
    Replay,
    /// Opens the play options modal.
    Settings,
    /// A play options row (select it).
    OptionRow(usize),
    /// The `<` / `>` value arrows of a play options row.
    OptionPrev(usize),
    OptionNext(usize),
    ExitQuit,
    ExitCancel,
    /// Full-screen region a modal records before its own parts: a click that
    /// reaches it closes the modal (cancel for the exit prompt).
    Blocker,
    /// A modal's panel. Swallows clicks on its empty space so they do not
    /// reach the `Blocker`.
    ModalPanel,
}

/// One recorded region, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub rect: Rect,
    pub id: HitId,
}

/// The id of the topmost region under `(x, y)`: regions recorded later draw
/// over earlier ones, so the last match wins.
pub fn hit_at(hits: &[Hit], x: f32, y: f32) -> Option<HitId> {
    hits.iter()
        .rev()
        .find(|h| contains(h.rect, x, y))
        .map(|h| h.id)
}

/// Whether `pointer` is over `rect` (false without a pointer).
pub fn hovered(pointer: Option<(f32, f32)>, rect: Rect) -> bool {
    pointer.is_some_and(|(x, y)| contains(rect, x, y))
}

fn contains(r: Rect, x: f32, y: f32) -> bool {
    x >= r.x && x < r.right() && y >= r.y && y < r.bottom()
}

/// Records hits and answers hover queries for one drawing pass. Borrowing
/// `Ui::hits` and `Ui::pointer` on their own lets a screen keep its canvas
/// and text borrows while it records.
pub struct HitSink<'a> {
    hits: &'a mut Vec<Hit>,
    pointer: Option<(f32, f32)>,
}

impl<'a> HitSink<'a> {
    pub fn new(hits: &'a mut Vec<Hit>, pointer: Option<(f32, f32)>) -> Self {
        Self { hits, pointer }
    }

    pub fn add(&mut self, rect: Rect, id: HitId) {
        self.hits.push(Hit { rect, id });
    }

    pub fn hovered(&self, rect: Rect) -> bool {
        hovered(self.pointer, rect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(x, y, w, h)
    }

    #[test]
    fn topmost_region_wins() {
        let hits = [
            Hit {
                rect: rect(0.0, 0.0, 100.0, 100.0),
                id: HitId::Blocker,
            },
            Hit {
                rect: rect(10.0, 10.0, 50.0, 50.0),
                id: HitId::ModalPanel,
            },
            Hit {
                rect: rect(20.0, 20.0, 10.0, 10.0),
                id: HitId::Play,
            },
        ];
        assert_eq!(hit_at(&hits, 25.0, 25.0), Some(HitId::Play));
        assert_eq!(hit_at(&hits, 15.0, 15.0), Some(HitId::ModalPanel));
        assert_eq!(hit_at(&hits, 90.0, 90.0), Some(HitId::Blocker));
        assert_eq!(hit_at(&hits, 150.0, 5.0), None);
    }

    #[test]
    fn edges_are_half_open() {
        let hits = [Hit {
            rect: rect(10.0, 10.0, 10.0, 10.0),
            id: HitId::Search,
        }];
        assert_eq!(hit_at(&hits, 10.0, 10.0), Some(HitId::Search));
        assert_eq!(hit_at(&hits, 20.0, 10.0), None);
        assert_eq!(hit_at(&hits, 10.0, 20.0), None);
    }

    #[test]
    fn hover_needs_a_pointer_inside() {
        let r = rect(0.0, 0.0, 10.0, 10.0);
        assert!(!hovered(None, r));
        assert!(hovered(Some((5.0, 5.0)), r));
        assert!(!hovered(Some((10.0, 5.0)), r));
    }
}

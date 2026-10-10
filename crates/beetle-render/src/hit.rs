//! Click regions. Screens record them while they draw (the layout math
//! lives in the drawing code only); the app hit-tests the list of the last
//! drawn frame, so a click always lands on what the player saw.

use crate::canvas::Rect;

/// What a click on a recorded region means. Each screen records only the
/// ids it can act on; the app maps them to the same actions as the keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitId {
    /// A row of the song list (a song or a folder); the index is into the
    /// visible rows of the current folder.
    ListRow(usize),
    /// The `<` `>` arrows beside the folder breadcrumb: the previous / next folder at this depth.
    FolderPrev,
    FolderNext,
    /// Goes up one folder (BKSP in the footer).
    FolderUp,
    /// A breadcrumb segment: go to the folder at this depth (0 = the root).
    Crumb(usize),
    /// A chart of a group: a chip on a group row (`row` is the row's index in
    /// the visible rows) or a difficulty tab of the detail panel. `pos` is the
    /// chart's position in the group.
    ChartTab {
        row: usize,
        pos: usize,
    },
    /// The sort selector: opens or closes the sort menu.
    Sort,
    /// A row of the open sort menu (its index in `SortMode::ALL`).
    SortOption(usize),
    /// A filter row item (its index in `filter_items`): a mode chip toggles,
    /// a level bound steps up, a toggle flips, 초기화 clears.
    FilterItem(usize),
    /// A level bound's arrow: step it down (`up` false) or up.
    FilterStep {
        item: usize,
        up: bool,
    },
    /// The empty list's 필터 초기화 button.
    FilterReset,
    /// The search box (starts a search).
    Search,
    Play,
    Replay,
    /// Opens the play options panel (the top bar OPTIONS button, TAB).
    PlayOptions,
    /// Opens the Settings screen (the top bar SETTINGS button, F4).
    OpenSettings,
    /// Toggles auto play.
    Auto,
    /// Opens the key configuration screen.
    KeyConfig,
    /// Opens (or the overlay's own close) the help overlay (the footer `?`).
    Help,
    /// The first-run guide: open the song manager, open the songs folder, rescan.
    OpenManager,
    OpenSongsFolder,
    Rescan,
    /// Opens the quit prompt.
    Quit,
    /// Result screen footer: back to song select, play again, play again with
    /// the play options changed (opens the options panel), screenshot.
    ResultSongSelect,
    ResultRetry,
    ResultRetryOptions,
    ResultScreenshot,
    /// The play options panel opened from the result: play again with the
    /// options as changed.
    OptionStart,
    /// Key configuration: a mode tab (index into `KEY_MODES`).
    KeyModeTab(usize),
    /// Key configuration: a lane button (index into the screen lane order).
    KeyLane(usize),
    /// Key configuration footer actions.
    KeySet,
    KeyAdd,
    KeyClear,
    KeyPreset,
    KeyScratch,
    KeyForm,
    KeyReset,
    KeyBack,
    /// An option row of the open panel or the Settings screen (its index in
    /// that screen's table): select it.
    OptionRow(usize),
    /// The `<` / `>` value arrows of an option row.
    OptionPrev(usize),
    OptionNext(usize),
    /// Settings footer: back to song select (saves the settings).
    SettingsBack,
    /// Judge offset calibration: apply the suggestion, measure again, or
    /// leave without changing the offset.
    CalibrateApply,
    CalibrateRetry,
    CalibrateCancel,
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

//! Song jacket thumbnails for the song select list (P2-2).
//!
//! The render thread asks for the thumbnails of the rows around the list
//! window. Requests go to a small pool of worker threads, which decode the
//! stage image (`load_stage_image`, the same source as the detail panel) and
//! centre-crop it to a 96×96 thumbnail. Finished thumbnails are copied into
//! the jacket slot grid by the render thread, at most `UPLOADS_PER_FRAME` a
//! frame. Songs with no artwork are remembered in `missing` and get the
//! procedural placeholder instead; they are never decoded again.
//!
//! The queue is replaced, not appended to: every frame the wanted set is
//! rebuilt in priority order and truncated to `QUEUE_CAP`, so work for rows
//! the list has scrolled away from is dropped. While the list is still moving
//! (no cursor input for `SETTLE`), nothing is requested at all.

use std::collections::{HashSet, VecDeque};
use std::ops::Range;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use beetle_core::{ChartId, SongMetadata};
use beetle_render::jacket::{jacket_seed, thumb_pixels, JacketKey};

use crate::loader::load_stage_image;
use crate::state::AppState;

/// Rows requested on each side of the visible window.
pub const PREFETCH_ROWS: usize = 6;
/// Most thumbnails waiting to be decoded.
pub const QUEUE_CAP: usize = 8;
/// How long the cursor must rest before the list asks for thumbnails.
pub const SETTLE: Duration = Duration::from_millis(120);
/// Thumbnails copied into the atlas per frame.
pub const UPLOADS_PER_FRAME: usize = 2;
const WORKERS: usize = 2;

/// Replaces `queue` with `wanted` (highest priority first), keeping at most `cap`.
pub fn replace_wanted<T>(queue: &mut VecDeque<T>, wanted: impl IntoIterator<Item = T>, cap: usize) {
    queue.clear();
    queue.extend(wanted.into_iter().take(cap));
}

/// The rows to request: the visible window plus `PREFETCH_ROWS` each side,
/// clamped to the list.
pub fn prefetch_rows(first_visible: usize, visible: usize, total: usize) -> Range<usize> {
    let end = (first_visible + visible + PREFETCH_ROWS).min(total);
    let start = first_visible.saturating_sub(PREFETCH_ROWS).min(end);
    start..end
}

/// Row indices ordered by distance from the selected row (nearer first, the
/// upper row first on a tie).
pub fn by_distance(rows: Range<usize>, selected: usize) -> Vec<usize> {
    let mut order: Vec<usize> = rows.collect();
    order.sort_by_key(|&r| (r.abs_diff(selected), r));
    order
}

type SharedPair = (Mutex<Shared>, Condvar);
type Done = (ChartId, Option<Vec<u8>>);

struct Request {
    id: ChartId,
    song: SongMetadata,
}

#[derive(Default)]
struct Shared {
    queue: VecDeque<Request>,
    in_flight: HashSet<ChartId>,
    closing: bool,
}

fn lock(m: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Background decoder for jacket thumbnails, owned by the app state.
pub struct ThumbLoader {
    shared: Arc<SharedPair>,
    done: Receiver<Done>,
    /// Songs with no artwork on disk or in a package (negative cache).
    pub missing: HashSet<ChartId>,
}

impl ThumbLoader {
    pub fn new() -> Self {
        let shared: Arc<SharedPair> = Arc::new((Mutex::new(Shared::default()), Condvar::new()));
        let (tx, rx) = channel();
        for _ in 0..WORKERS {
            let shared = Arc::clone(&shared);
            let tx = tx.clone();
            thread::spawn(move || worker(shared, tx));
        }
        Self {
            shared,
            done: rx,
            missing: HashSet::new(),
        }
    }

    /// Makes the queue hold exactly `wanted` (priority order), minus work
    /// already in flight. An unchanged queue is left alone.
    pub fn set_wanted(&self, wanted: &[(ChartId, &SongMetadata)]) {
        let (m, cv) = &*self.shared;
        let mut st = lock(m);
        let Shared {
            queue, in_flight, ..
        } = &mut *st;
        let fresh: Vec<(ChartId, &SongMetadata)> = wanted
            .iter()
            .copied()
            .filter(|(id, _)| !in_flight.contains(id))
            .take(QUEUE_CAP)
            .collect();
        if queue.len() == fresh.len() && queue.iter().zip(&fresh).all(|(r, (id, _))| r.id == *id) {
            return;
        }
        replace_wanted(
            queue,
            fresh.into_iter().map(|(id, song)| Request {
                id,
                song: song.clone(),
            }),
            QUEUE_CAP,
        );
        cv.notify_all();
    }

    /// Up to `max` finished decodes, oldest first. `None` pixels mean the
    /// song has no artwork.
    pub fn drain(&self, max: usize) -> Vec<Done> {
        let mut out = Vec::new();
        while out.len() < max {
            match self.done.try_recv() {
                Ok(done) => out.push(done),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        out
    }

    /// Drops queued work and the negative cache (after a library rescan).
    pub fn reset(&mut self) {
        self.missing.clear();
        self.set_wanted(&[]);
    }
}

impl Drop for ThumbLoader {
    fn drop(&mut self) {
        let (m, cv) = &*self.shared;
        let mut st = lock(m);
        st.closing = true;
        st.queue.clear();
        cv.notify_all();
    }
}

/// Next request for a worker, waiting while there is none. `None` once the
/// loader is dropped.
fn next_request(shared: &SharedPair) -> Option<Request> {
    let (m, cv) = shared;
    let mut st = lock(m);
    loop {
        if st.closing {
            return None;
        }
        if let Some(req) = st.queue.pop_front() {
            st.in_flight.insert(req.id);
            return Some(req);
        }
        st = cv.wait(st).unwrap_or_else(|e| e.into_inner());
    }
}

fn worker(shared: Arc<SharedPair>, done: Sender<Done>) {
    while let Some(req) = next_request(&shared) {
        let pixels = load_stage_image(&req.song).and_then(|img| thumb_pixels(&img));
        let _ = done.send((req.id, pixels));
        lock(&shared.0).in_flight.remove(&req.id);
    }
}

/// The song a list row shows (a group shows its chosen chart).
fn row_song(entry: &crate::folders::ListEntry) -> Option<usize> {
    use crate::folders::ListEntry;
    match entry {
        ListEntry::Song(i) => Some(*i),
        ListEntry::Group {
            charts, selected, ..
        } => charts.get(*selected).copied(),
        ListEntry::Folder { .. } => None,
    }
}

/// Per-frame step, run while the song select list is drawn: moves finished
/// thumbnails into the atlas, makes placeholders for songs without artwork, and
/// requests the thumbnails still wanted. Returns `true` while some row in the
/// prefetch range is still waiting, so the caller keeps redrawing.
pub fn pump(state: &mut AppState, scroll: f32, visible: usize, settled: bool) -> bool {
    let AppState {
        songs,
        entries,
        selected_entry,
        thumbs,
        gpu_ui,
        ..
    } = state;
    let ui = &mut gpu_ui.ui;

    for (id, pixels) in thumbs.drain(UPLOADS_PER_FRAME) {
        match pixels {
            Some(px) => {
                ui.jackets
                    .insert(ui.canvas.atlas_mut(), JacketKey::Chart(id), &px);
            }
            None => {
                thumbs.missing.insert(id);
            }
        }
    }

    let range = prefetch_rows(scroll.max(0.0) as usize, visible, entries.len());
    let mut busy = false;
    let mut wanted: Vec<(ChartId, &SongMetadata)> = Vec::new();
    for row in by_distance(range, *selected_entry) {
        let Some(song) = entries
            .get(row)
            .and_then(row_song)
            .and_then(|i| songs.get(i))
        else {
            continue;
        };
        if ui.jackets.contains(JacketKey::Chart(song.id)) {
            continue;
        }
        if thumbs.missing.contains(&song.id) {
            let seed = jacket_seed(&song.title, &song.artist);
            ui.jackets.ensure_placeholder(ui.canvas.atlas_mut(), seed);
            continue;
        }
        busy = true;
        wanted.push((song.id, song));
    }
    if settled {
        thumbs.set_wanted(&wanted);
    } else {
        // The list is still moving: cancel what was queued for the old rows.
        thumbs.set_wanted(&[]);
    }
    busy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefetch_covers_the_window_plus_six_rows_each_side() {
        assert_eq!(prefetch_rows(10, 8, 100), 4..24);
        // Clamped at the top and the bottom of the list.
        assert_eq!(prefetch_rows(0, 8, 100), 0..14);
        assert_eq!(prefetch_rows(95, 8, 100), 89..100);
        // A list shorter than the window.
        assert_eq!(prefetch_rows(0, 8, 3), 0..3);
        assert_eq!(prefetch_rows(5, 8, 0), 0..0);
    }

    #[test]
    fn rows_are_requested_nearest_the_selection_first() {
        assert_eq!(by_distance(2..7, 4), vec![4, 3, 5, 2, 6]);
        assert_eq!(by_distance(0..3, 9), vec![2, 1, 0]);
        assert!(by_distance(3..3, 0).is_empty());
    }

    #[test]
    fn replacing_the_queue_drops_stale_work_and_respects_the_cap() {
        let mut queue: VecDeque<u32> = VecDeque::from(vec![1, 2, 3]);
        replace_wanted(&mut queue, [9, 8, 7], QUEUE_CAP);
        assert_eq!(queue.iter().copied().collect::<Vec<_>>(), vec![9, 8, 7]);
        // Overflow keeps the highest priority items at the front.
        replace_wanted(&mut queue, 0..20, QUEUE_CAP);
        assert_eq!(queue.len(), QUEUE_CAP);
        assert_eq!(queue.front(), Some(&0));
        assert_eq!(queue.back(), Some(&7));
        replace_wanted(&mut queue, std::iter::empty(), QUEUE_CAP);
        assert!(queue.is_empty());
    }

    #[test]
    fn a_scrolled_past_row_is_not_requested_again_once_it_is_gone() {
        // The first frame wants rows 0..8; after scrolling the wanted set is
        // rows 20..28, so no request for the old rows remains queued.
        let mut queue: VecDeque<u32> = VecDeque::new();
        replace_wanted(
            &mut queue,
            by_distance(0..8, 0).into_iter().map(|r| r as u32),
            QUEUE_CAP,
        );
        replace_wanted(
            &mut queue,
            by_distance(20..28, 20).into_iter().map(|r| r as u32),
            QUEUE_CAP,
        );
        assert!(queue.iter().all(|&r| r >= 20));
    }

    #[test]
    fn negative_cache_is_a_plain_set_of_ids() {
        let mut missing: HashSet<ChartId> = HashSet::new();
        let id = ChartId::synthetic(7);
        missing.insert(id);
        assert!(missing.contains(&id));
        missing.clear();
        assert!(!missing.contains(&id));
    }
}

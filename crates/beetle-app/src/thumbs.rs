//! Song jacket thumbnails for the song select list (P2-2).
//!
//! The render thread asks for the thumbnails of the rows around the list
//! window. Requests go to a small pool of worker threads, which decode the
//! stage image (`load_stage_image`, the same source as the detail panel) and
//! centre-crop it to an 80×80 thumbnail. Finished thumbnails are copied into
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
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use beetle_core::{ChartId, SongMetadata};
use beetle_render::jacket::{jacket_seed, thumb_pixels, JacketKey};

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
    /// Finished decodes received from the workers and not yet uploaded. A
    /// song in here is neither requested again nor decoded twice.
    ready: VecDeque<Done>,
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
        Self::with_channel(shared, rx)
    }

    fn with_channel(shared: Arc<SharedPair>, done: Receiver<Done>) -> Self {
        Self {
            shared,
            done,
            ready: VecDeque::new(),
            missing: HashSet::new(),
        }
    }

    /// Moves every finished decode the channel holds into `ready`.
    fn fill_ready(&mut self) {
        while let Ok(done) = self.done.try_recv() {
            self.ready.push_back(done);
        }
    }

    /// Makes the queue hold exactly `wanted` (priority order), minus work
    /// already in flight or finished (in `ready`). An unchanged queue is left
    /// alone.
    pub fn set_wanted(&mut self, wanted: &[(ChartId, &SongMetadata)]) {
        let shared = Arc::clone(&self.shared);
        let (m, cv) = &*shared;
        let mut st = lock(m);
        // Taken under the lock: a worker sends its result before it removes
        // the id from `in_flight`, so an id in neither set has its result in
        // the channel, which this drain moves into `ready`.
        self.fill_ready();
        let Shared {
            queue, in_flight, ..
        } = &mut *st;
        let ready = &self.ready;
        let fresh: Vec<(ChartId, &SongMetadata)> = wanted
            .iter()
            .copied()
            .filter(|(id, _)| !in_flight.contains(id) && !ready.iter().any(|(r, _)| r == id))
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

    /// Up to `max` finished decodes, oldest first, after taking in everything
    /// the workers have sent. `None` pixels mean the song has no artwork. The
    /// rest stay ready for the next frame.
    pub fn take_ready(&mut self, max: usize) -> Vec<Done> {
        self.fill_ready();
        let n = max.min(self.ready.len());
        self.ready.drain(..n).collect()
    }

    /// Drops queued work, finished decodes and the negative cache (after a
    /// library rescan).
    pub fn reset(&mut self) {
        self.missing.clear();
        self.ready.clear();
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
        finish_job(&shared, &done, req.id, || {
            crate::loader::select_stage_image(&req.song).and_then(|img| thumb_pixels(&img))
        });
    }
}

/// Runs one decode and reports it. A panic counts as "no artwork" (the song is
/// then remembered as missing), and the id always leaves `in_flight`, so a bad
/// file cannot leave its row stuck or take the worker down.
fn finish_job(
    shared: &SharedPair,
    done: &Sender<Done>,
    id: ChartId,
    job: impl FnOnce() -> Option<Vec<u8>>,
) {
    let pixels = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job))
        .ok()
        .flatten();
    let _ = done.send((id, pixels));
    lock(&shared.0).in_flight.remove(&id);
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
    let ui = gpu_ui.ui.menu();

    for (id, pixels) in thumbs.take_ready(UPLOADS_PER_FRAME) {
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
    fn a_panicking_decode_reports_missing_and_leaves_nothing_in_flight() {
        let shared: Arc<SharedPair> = Arc::new((Mutex::new(Shared::default()), Condvar::new()));
        let (tx, rx) = channel();
        let id = ChartId::default();
        lock(&shared.0).in_flight.insert(id);
        finish_job(&shared, &tx, id, || panic!("broken artwork"));
        assert!(lock(&shared.0).in_flight.is_empty());
        assert!(matches!(rx.try_recv(), Ok((got, None)) if got == id));
    }

    #[test]
    fn a_finished_decode_reports_its_pixels_and_clears_in_flight() {
        let shared: Arc<SharedPair> = Arc::new((Mutex::new(Shared::default()), Condvar::new()));
        let (tx, rx) = channel();
        let id = ChartId::default();
        lock(&shared.0).in_flight.insert(id);
        finish_job(&shared, &tx, id, || Some(vec![1, 2, 3, 4]));
        assert!(lock(&shared.0).in_flight.is_empty());
        assert!(matches!(rx.try_recv(), Ok((got, Some(px))) if got == id && px == [1, 2, 3, 4]));
    }

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

    fn song_of(n: u64) -> SongMetadata {
        SongMetadata {
            id: ChartId::synthetic(n),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: n,
            file_path: String::new(),
            title: format!("Song {n}"),
            subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            bpm: 120.0,
            bpm_min: 120.0,
            bpm_max: 120.0,
            play_level: 1,
            notes_count: 0,
            play_mode: beetle_core::PlayMode::Keys7,
            difficulty: None,
            also_on: Vec::new(),
        }
    }

    #[test]
    fn a_finished_decode_is_not_requested_again_while_it_waits_to_upload() {
        let shared: Arc<SharedPair> = Arc::new((Mutex::new(Shared::default()), Condvar::new()));
        let (tx, rx) = channel();
        let mut loader = ThumbLoader::with_channel(Arc::clone(&shared), rx);
        // Five decodes finish between two frames, each sent before it leaves
        // `in_flight`, as the workers do.
        let ids: Vec<ChartId> = (1..=5).map(ChartId::synthetic).collect();
        for &id in &ids {
            lock(&shared.0).in_flight.insert(id);
            finish_job(&shared, &tx, id, || Some(vec![1, 2, 3, 4]));
        }
        // A frame uploads at most two; the other three wait in `ready`.
        let first = loader.take_ready(2);
        assert_eq!(first.iter().map(|d| d.0).collect::<Vec<_>>(), ids[..2]);
        assert_eq!(loader.ready.len(), 3);
        // The next frame still wants rows 3..=6 (rows 1 and 2 are uploaded
        // and resident, so the caller skips them). Rows 3..=5 are waiting in
        // `ready`: only the new row 6 is queued, nothing is decoded twice.
        let songs: Vec<SongMetadata> = (3..=6).map(song_of).collect();
        let wanted: Vec<(ChartId, &SongMetadata)> = songs.iter().map(|s| (s.id, s)).collect();
        loader.set_wanted(&wanted);
        let queued: Vec<ChartId> = lock(&shared.0).queue.iter().map(|r| r.id).collect();
        assert_eq!(queued, vec![ChartId::synthetic(6)]);
        // Once uploaded they are gone for good, and the rest drains next frame.
        assert_eq!(loader.take_ready(2).len(), 2);
        assert_eq!(loader.take_ready(2).len(), 1);
        assert!(loader.take_ready(2).is_empty());
    }

    #[test]
    fn a_result_still_in_the_channel_counts_as_finished_when_wanted_is_set() {
        let shared: Arc<SharedPair> = Arc::new((Mutex::new(Shared::default()), Condvar::new()));
        let (tx, rx) = channel();
        let mut loader = ThumbLoader::with_channel(Arc::clone(&shared), rx);
        let id = ChartId::synthetic(9);
        lock(&shared.0).in_flight.insert(id);
        finish_job(&shared, &tx, id, || None);
        // Nothing was drained yet, but the worker is done with the id.
        let song = song_of(9);
        loader.set_wanted(&[(id, &song)]);
        assert!(lock(&shared.0).queue.is_empty());
        assert_eq!(loader.take_ready(2).len(), 1);
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

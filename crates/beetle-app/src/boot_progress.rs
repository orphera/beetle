//! What the song library read is doing (startup and rescan), for the Boot
//! screen: the stage, and how far it is when it counts items. One read runs
//! at a time, so the worker writes these and the render thread reads them.
//! Each stage's time goes to stdout when the next one starts, so a slow
//! startup can be traced to its stage.

use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Stage {
    /// Reading `scores.dat`.
    Scores,
    /// Reading `songs.cache`.
    Cache,
    /// Walking the song folders for chart files (counts files found, no total).
    Listing,
    /// Reading and parsing chart files.
    Charts,
    /// Reading the charts of the installed packages.
    Packages,
    /// Choosing which copy of a chart found twice to load.
    Copies,
    /// Matching the songs to the difficulty tables.
    Tables,
    /// Looking for `.bmsp` files in the song folders that are not installed.
    Uninstalled,
    /// The read is over.
    Done,
}

const STAGES: [Stage; 9] = [
    Stage::Scores,
    Stage::Cache,
    Stage::Listing,
    Stage::Charts,
    Stage::Packages,
    Stage::Copies,
    Stage::Tables,
    Stage::Uninstalled,
    Stage::Done,
];

static STAGE: AtomicU8 = AtomicU8::new(Stage::Done as u8);
static DONE: AtomicUsize = AtomicUsize::new(0);
static TOTAL: AtomicUsize = AtomicUsize::new(0);
static STARTED: Mutex<Option<(Stage, Instant)>> = Mutex::new(None);

/// Starts `stage` with `total` items (0 when it has no total), and logs how
/// long the stage before it took.
pub fn begin(stage: Stage, total: usize) {
    if let Ok(mut started) = STARTED.lock() {
        if let Some((prev, at)) = started.take() {
            println!("[library] {prev:?}: {} ms", at.elapsed().as_millis());
        }
        if stage != Stage::Done {
            *started = Some((stage, Instant::now()));
        }
    }
    DONE.store(0, Ordering::Relaxed);
    TOTAL.store(total, Ordering::Relaxed);
    STAGE.store(stage as u8, Ordering::Relaxed);
}

/// One more item of the current stage is done.
pub fn step() {
    DONE.fetch_add(1, Ordering::Relaxed);
}

/// The stage, items done and items in it (0 when it has no total).
pub fn read() -> (Stage, usize, usize) {
    let stage = STAGES
        .get(STAGE.load(Ordering::Relaxed) as usize)
        .copied()
        .unwrap_or(Stage::Done);
    (
        stage,
        DONE.load(Ordering::Relaxed),
        TOTAL.load(Ordering::Relaxed),
    )
}

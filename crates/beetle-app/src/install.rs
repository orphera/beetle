//! Songs dropped on the window (plan S2, `docs/plans/2026-10-21-song-install-in-app.md`).
//! Archives and `.bmsp` / `.bmdp` files are installed into `packages/`
//! (archives packed with the Turbo profile), folders are linked in
//! `library.dat` where they are. Each dropped item is added on a worker
//! thread (INV-5), one at a time; a held toast shows the progress and cancels
//! on a click. When the queue is empty the library is read again, the cursor
//! goes to the first new song and one summary toast replaces the rescan's.
//! Nothing starts or finishes during a play: items dropped then wait for the
//! next menu.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;

use beetle_render::{strings, ToastKind};
use bms_package_manager::{
    add_path, AddError, AddOptions, AddProgress, AddReport, AddSource, Added, FolderMode,
    PackProfile, PackageManager, SkipReason,
};

use crate::state::{AppScreen, AppState};
use crate::transition::{self, ToastLife};

/// Whether dropping `path` adds it (rather than opening a chart).
pub fn is_installable(path: &Path) -> bool {
    AddSource::of(path).is_some()
}

/// A worker's word to the loop.
enum Msg {
    /// The current item's share of songs done, 0 to 100 (folders and archives of several).
    Percent(u8),
    /// The item is done. With it, where the songs it added load from: the
    /// installed packages' `.bmsp` paths and the linked folders.
    Done(Result<AddReport, AddError>, Vec<String>),
}

struct Running {
    rx: Receiver<Msg>,
    cancel: Arc<AtomicBool>,
    name: String,
    percent: Option<u8>,
}

/// One line of the list the summary toast opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkipRow {
    pub name: String,
    pub reason: String,
}

/// The summary of a finished batch, shown once the rescan it started is read.
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub text: String,
    pub kind: ToastKind,
    /// The new songs' load paths start with one of these.
    pub focus: Vec<String>,
}

/// Running counts for the songs added in one batch of drops.
#[derive(Default)]
struct Batch {
    /// Items dropped, done or not.
    items: usize,
    done: usize,
    /// Names of installed songs and linked folders, in order.
    added: Vec<String>,
    linked: usize,
    skipped: Vec<SkipRow>,
    cancelled: bool,
    focus: Vec<String>,
}

#[derive(Default)]
pub struct Installs {
    queue: VecDeque<PathBuf>,
    running: Option<Running>,
    batch: Batch,
    /// Set when the batch ends, taken by `poll_library` after the rescan.
    pub summary: Option<Summary>,
    /// The last batch's skipped items, for the list the toast opens.
    pub skipped: Vec<SkipRow>,
    /// The list of skipped items is open (song select).
    pub show_skipped: bool,
}

impl Installs {
    /// Queues a dropped file or folder.
    pub fn push(&mut self, path: PathBuf) {
        self.batch.items += 1;
        self.queue.push_back(path);
    }

    /// Something is queued or running.
    pub fn busy(&self) -> bool {
        self.running.is_some() || !self.queue.is_empty()
    }
}

/// Screens a worker may start or a batch may finish on: the menus. A play, its
/// loading and a library read wait.
fn may_run(state: &AppState) -> bool {
    matches!(
        state.screen,
        AppScreen::Title
            | AppScreen::SongSelect
            | AppScreen::Settings
            | AppScreen::KeyConfig
            | AppScreen::Result
    ) && state.library_receiver.is_none()
}

/// Queues `path` and starts on it if nothing else is running.
pub fn queue(state: &mut AppState, path: PathBuf) {
    state.installs.push(path);
    poll(state);
}

/// Moves the queue along. Call every loop iteration.
pub fn poll(state: &mut AppState) {
    let mut changed = false;
    if let Some(run) = &mut state.installs.running {
        loop {
            match run.rx.try_recv() {
                Ok(Msg::Percent(p)) => {
                    changed |= run.percent != Some(p);
                    run.percent = Some(p);
                }
                Ok(Msg::Done(result, focus)) => {
                    let name = run.name.clone();
                    state.installs.running = None;
                    record(&mut state.installs.batch, &name, result, focus);
                    changed = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    let name = run.name.clone();
                    state.installs.running = None;
                    let batch = &mut state.installs.batch;
                    batch.done += 1;
                    batch.skipped.push(SkipRow {
                        name,
                        reason: strings::SKIP_FAILED().to_string(),
                    });
                    changed = true;
                    break;
                }
            }
        }
    }
    if state.installs.running.is_none() && may_run(state) {
        if state.installs.batch.cancelled {
            state.installs.queue.clear();
        }
        if let Some(path) = state.installs.queue.pop_front() {
            state.installs.running = Some(spawn(path));
            changed = true;
        } else if state.installs.batch.items > 0 {
            finish(state);
            return;
        }
    }
    if changed {
        show_progress(state);
    }
}

/// Adds the outcome of one item to the batch.
fn record(batch: &mut Batch, name: &str, result: Result<AddReport, AddError>, focus: Vec<String>) {
    batch.done += 1;
    batch.focus.extend(focus);
    let report = match result {
        Ok(report) => report,
        Err(AddError::Cancelled(report)) => {
            batch.cancelled = true;
            report
        }
        Err(AddError::Unsupported) => {
            batch.skipped.push(SkipRow {
                name: name.to_string(),
                reason: strings::SKIP_UNSUPPORTED().to_string(),
            });
            return;
        }
        Err(AddError::Library(_)) => {
            batch.skipped.push(SkipRow {
                name: name.to_string(),
                reason: strings::SKIP_LIBRARY().to_string(),
            });
            return;
        }
    };
    batch.added.extend(
        report
            .installed
            .iter()
            .filter(|s| s.how != Added::Bga)
            .map(|s| s.name.clone()),
    );
    batch.added.extend(report.linked.iter().cloned());
    batch.linked += report.linked.len();
    batch.skipped.extend(report.skipped.iter().map(|s| SkipRow {
        name: s.name.clone(),
        reason: skip_reason(&s.reason).to_string(),
    }));
}

fn skip_reason(reason: &SkipReason) -> &'static str {
    match reason {
        SkipReason::NoCharts => strings::SKIP_NO_CHARTS(),
        SkipReason::Unpack(_) => strings::SKIP_UNPACK(),
        SkipReason::AlreadyInstalled => strings::SKIP_ALREADY_INSTALLED(),
        SkipReason::AlreadyLinked => strings::SKIP_ALREADY_LINKED(),
        SkipReason::Failed(_) => strings::SKIP_FAILED(),
    }
}

fn spawn(path: PathBuf) -> Running {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (tx, rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    std::thread::spawn(move || {
        let root = bms_package::installed::packages_root();
        let mut mgr = match PackageManager::new(&root) {
            Ok(mgr) => mgr,
            Err(e) => {
                let report = AddReport {
                    skipped: vec![bms_package_manager::Skipped {
                        name: path.to_string_lossy().into_owned(),
                        reason: SkipReason::Failed(e),
                    }],
                    ..Default::default()
                };
                let _ = tx.send(Msg::Done(Ok(report), Vec::new()));
                return;
            }
        };
        let options = AddOptions {
            folders: FolderMode::Link(crate::scanner::library_file()),
            profile: PackProfile::Turbo,
        };
        let mut last = None;
        let result = add_path(&mut mgr, &path, &options, Some(&flag), |p| {
            // A single song's steps each count from 0 to 100 on their own,
            // so only the share of songs in a folder or archive is shown.
            let percent = match p {
                AddProgress::Song { index, total, .. } => Some(index * 100 / total.max(1)),
                _ => None,
            };
            if let Some(p) = percent.map(|p| p as u8).filter(|p| last != Some(*p)) {
                last = Some(p);
                let _ = tx.send(Msg::Percent(p));
            }
        });
        let focus = match &result {
            Ok(report) | Err(AddError::Cancelled(report)) => focus_paths(&root, report),
            Err(_) => Vec::new(),
        };
        let _ = tx.send(Msg::Done(result, focus));
    });
    Running {
        rx,
        cancel,
        name,
        percent: None,
    }
}

/// Where the songs in `report` load from, in the form the scanner writes
/// `file_path`: `<package.bmsp>::<entry>` and `<library folder>/...`.
fn focus_paths(root: &Path, report: &AddReport) -> Vec<String> {
    let ids: Vec<&str> = report.installed.iter().map(|s| s.id.as_str()).collect();
    let mut paths: Vec<String> = bms_package::installed::read_active_states(root)
        .unwrap_or_default()
        .into_iter()
        .filter(|s| ids.contains(&s.id.as_str()))
        .map(|s| format!("{}::", s.bmsp_path.to_string_lossy()))
        .collect();
    paths.extend(report.linked.iter().cloned());
    paths
}

/// The held toast for the item being added.
fn show_progress(state: &mut AppState) {
    let installs = &state.installs;
    let Some(run) = &installs.running else {
        return;
    };
    let text = if run.cancel.load(Ordering::Relaxed) {
        strings::INSTALL_CANCELLING().to_string()
    } else {
        let label = match run.percent {
            Some(p) => format!("{} {p}%", run.name),
            None => run.name.clone(),
        };
        let index = (installs.batch.done + 1).to_string();
        let items = installs.batch.items.to_string();
        strings::fill(strings::INSTALL_PROGRESS(), &[&index, &items, &label])
    };
    transition::show_lasting_toast(state, ToastKind::Info, text, ToastLife::Held);
}

/// Ends the batch: reads the library again when anything came in, and leaves
/// the summary for `poll_library`; otherwise shows it now.
fn finish(state: &mut AppState) {
    let batch = std::mem::take(&mut state.installs.batch);
    let summary = summarize(&batch);
    state.installs.skipped = batch.skipped;
    if batch.added.is_empty() {
        show_summary(state, summary);
    } else {
        state.installs.summary = Some(summary);
        state.start_rescan();
    }
}

fn summarize(batch: &Batch) -> Summary {
    let added = batch.added.len();
    let skipped = batch.skipped.len();
    let (text, kind) = if batch.cancelled {
        let mut text = strings::INSTALL_CANCELLED().to_string();
        if added > 0 {
            text = format!(
                "{text}, {}",
                strings::fill(strings::INSTALL_ADDED_MANY(), &[&added.to_string()])
            );
        }
        (text, ToastKind::Info)
    } else if added == 1 && skipped == 0 {
        let template = if batch.linked == 1 {
            strings::INSTALL_LINKED_ONE()
        } else {
            strings::INSTALL_ADDED_ONE()
        };
        (
            strings::fill(template, &[&batch.added[0]]),
            ToastKind::Success,
        )
    } else if added == 0 && skipped == 1 {
        let row = &batch.skipped[0];
        (
            strings::fill(strings::INSTALL_FAILED_ONE(), &[&row.name, &row.reason]),
            ToastKind::Error,
        )
    } else if added == 0 && skipped == 0 {
        (strings::INSTALL_NOTHING().to_string(), ToastKind::Info)
    } else {
        let mut parts = Vec::new();
        let songs = added - batch.linked;
        if songs > 0 {
            parts.push(strings::fill(
                strings::INSTALL_ADDED_MANY(),
                &[&songs.to_string()],
            ));
        }
        if batch.linked > 0 {
            parts.push(strings::fill(
                strings::INSTALL_LINKED_MANY(),
                &[&batch.linked.to_string()],
            ));
        }
        if skipped > 0 {
            parts.push(strings::fill(
                strings::INSTALL_SKIPPED_MANY(),
                &[&skipped.to_string()],
            ));
        }
        let mut text = parts.join(", ");
        if skipped > 0 {
            text.push_str(strings::INSTALL_SEE_WHY());
        }
        let kind = if added == 0 {
            ToastKind::Error
        } else if skipped > 0 {
            ToastKind::Info
        } else {
            ToastKind::Success
        };
        (text, kind)
    };
    Summary {
        text,
        kind,
        focus: batch.focus.clone(),
    }
}

/// Shows the batch summary. It stays longer and opens the list when items
/// were left out of a batch of several.
pub fn show_summary(state: &mut AppState, summary: Summary) {
    if summary.text.ends_with(strings::INSTALL_SEE_WHY()) {
        transition::show_lasting_toast(state, summary.kind, summary.text, ToastLife::Long);
    } else {
        transition::show_toast(state, summary.kind, summary.text);
    }
}

/// The first song in the list whose load path starts with one of `focus`.
pub fn focus_new_song(state: &mut AppState, focus: &[String]) {
    let matches = |path: &str| focus.iter().any(|f| path.starts_with(f.as_str()));
    if focus.is_empty() {
        return;
    }
    let found = |state: &AppState| {
        state.entries.iter().position(|e| {
            e.song()
                .and_then(|i| state.songs.get(i))
                .is_some_and(|s| matches(&s.file_path))
        })
    };
    if found(state).is_none() && state.songs.iter().any(|s| matches(&s.file_path)) {
        // Not in the open folder or behind the search: show every song.
        state.search_query.clear();
        state.folder_path = crate::folders::FolderPath::top("all");
        state.recompute_entries();
    }
    if let Some(row) = found(state) {
        state.selected_entry = row;
    }
}

/// A click on the toast: cancels the item being added, or opens the list of
/// items the last batch left out.
pub fn toast_clicked(state: &mut AppState) {
    if let Some(run) = &state.installs.running {
        run.cancel.store(true, Ordering::SeqCst);
        state.installs.batch.cancelled = true;
        show_progress(state);
    } else if !state.installs.skipped.is_empty() && state.screen == AppScreen::SongSelect {
        state.installs.show_skipped = true;
        state.toast = None;
        state.window.request_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bms_package_manager::{InstalledSong, Skipped};

    fn report(installed: &[&str], linked: &[&str], skipped: &[&str]) -> AddReport {
        AddReport {
            installed: installed
                .iter()
                .map(|n| InstalledSong {
                    id: n.to_string(),
                    name: n.to_string(),
                    how: Added::Imported,
                })
                .collect(),
            linked: linked.iter().map(|s| s.to_string()).collect(),
            skipped: skipped
                .iter()
                .map(|n| Skipped {
                    name: n.to_string(),
                    reason: SkipReason::NoCharts,
                })
                .collect(),
        }
    }

    fn batch_of(results: Vec<Result<AddReport, AddError>>) -> Batch {
        let mut batch = Batch {
            items: results.len(),
            ..Default::default()
        };
        for r in results {
            record(&mut batch, "item", r, Vec::new());
        }
        batch
    }

    #[test]
    fn one_song_names_it() {
        strings::set_lang(strings::Lang::Ko);
        let s = summarize(&batch_of(vec![Ok(report(&["AIR"], &[], &[]))]));
        assert_eq!(s.text, "곡을 넣었어요: AIR");
        assert_eq!(s.kind, ToastKind::Success);
        let s = summarize(&batch_of(vec![Ok(report(&[], &["D:\\BMS"], &[]))]));
        assert_eq!(s.text, "폴더를 연결했어요: D:\\BMS");
    }

    #[test]
    fn one_failure_says_why() {
        strings::set_lang(strings::Lang::Ko);
        let s = summarize(&batch_of(vec![Ok(report(&[], &[], &["pics"]))]));
        assert_eq!(s.text, "넣지 못했어요: pics (BMS 채보 파일이 없어요)");
        assert_eq!(s.kind, ToastKind::Error);
    }

    #[test]
    fn several_are_counted_and_skips_point_to_the_list() {
        strings::set_lang(strings::Lang::Ko);
        let s = summarize(&batch_of(vec![
            Ok(report(&["a", "b"], &[], &["c"])),
            Ok(report(&["d"], &["D:\\BMS"], &[])),
            Err(AddError::Unsupported),
        ]));
        assert_eq!(
            s.text,
            "3곡 추가, 폴더 1개 연결, 2개 건너뜀 - 누르면 이유 보기"
        );
        assert_eq!(s.kind, ToastKind::Info);
        let s = summarize(&batch_of(vec![
            Ok(report(&["a"], &[], &[])),
            Ok(report(&["b"], &[], &[])),
        ]));
        assert_eq!(s.text, "2곡 추가");
        assert_eq!(s.kind, ToastKind::Success);
    }

    #[test]
    fn cancelling_keeps_what_came_in() {
        strings::set_lang(strings::Lang::Ko);
        let batch = batch_of(vec![
            Ok(report(&["a"], &[], &[])),
            Err(AddError::Cancelled(report(&["b"], &[], &[]))),
        ]);
        assert!(batch.cancelled);
        assert_eq!(summarize(&batch).text, "곡 넣기를 멈췄어요, 2곡 추가");
    }
}

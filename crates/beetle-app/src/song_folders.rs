//! Linking existing BMS folders (plan S3, `docs/plans/2026-10-21-song-install-in-app.md`).
//! The first-run guide's button and the Song Folders list (Settings) open the
//! folder picker; a picked folder goes through the same queue as a dropped one
//! (`install.rs`), so it is linked in `library.dat`, never copied. The list
//! shows the linked folders and unlinks them, which deletes no files.

use std::path::PathBuf;
use std::sync::mpsc::TryRecvError;

use beetle_audio::UiSound;
use beetle_render::{strings, HitId, ToastKind};
use bms_package_manager::file_dialog::{self, PickKind};
use bms_package_manager::library::{load_library, save_library};
use winit::keyboard::KeyCode;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::menu_audio::play_ui;
use crate::state::AppState;
use crate::transition::show_toast;

/// The Song Folders list while it is open.
pub struct FolderList {
    /// The folders in `library.dat`, in its order.
    pub paths: Vec<String>,
    /// A folder's index, or `paths.len()` for the add button.
    pub row: usize,
}

impl FolderList {
    fn read() -> Self {
        Self {
            paths: load_library().paths().to_vec(),
            row: 0,
        }
    }

    /// Reads `library.dat` again (after a link or a rescan), keeping the row in range.
    pub fn reload(&mut self) {
        self.paths = load_library().paths().to_vec();
        self.row = self.row.min(self.paths.len());
    }
}

/// Opens the Song Folders list (ENTER on its Settings row).
pub fn open(state: &mut AppState) {
    play_ui(state, UiSound::Open);
    state.song_folders = Some(FolderList::read());
}

/// Opens the folder picker over the window, unless it is already open. The
/// dialog runs on its own thread; `poll` takes its answer.
pub fn pick(state: &mut AppState) {
    if state.folder_pick.is_some() {
        return;
    }
    play_ui(state, UiSound::Open);
    let owner = match state.window.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(win32)) => win32.hwnd.get(),
        _ => 0,
    };
    state.folder_pick = Some(file_dialog::pick(
        PickKind::Folder,
        strings::FOLDER_PICK_TITLE(),
        owner,
    ));
}

/// Takes the picked folder, if any, and queues it to be linked. Call every
/// loop iteration.
pub fn poll(state: &mut AppState) {
    let Some(rx) = &state.folder_pick else {
        return;
    };
    let picked: Option<PathBuf> = match rx.try_recv() {
        Ok(picked) => picked,
        Err(TryRecvError::Empty) => return,
        Err(TryRecvError::Disconnected) => None,
    };
    state.folder_pick = None;
    if let Some(path) = picked {
        crate::install::queue(state, path);
    }
}

/// Removes folder `i` from `library.dat` and reads the library again.
fn unlink(state: &mut AppState, i: usize) {
    let Some(path) = state
        .song_folders
        .as_ref()
        .and_then(|l| l.paths.get(i).cloned())
    else {
        return;
    };
    let mut library = load_library();
    library.remove(&path);
    play_ui(state, UiSound::Toast);
    if save_library(&library).is_err() {
        show_toast(
            state,
            ToastKind::Error,
            strings::TOAST_LIBRARY_SAVE_FAILED(),
        );
        return;
    }
    if let Some(list) = &mut state.song_folders {
        list.reload();
    }
    // Shown in place of the rescan's toast once the read is done.
    state.installs.summary = Some(crate::install::Summary {
        text: strings::fill(strings::TOAST_UNLINKED(), &[&path]),
        kind: ToastKind::Success,
        focus: Vec::new(),
    });
    state.start_rescan();
}

fn close(state: &mut AppState) {
    play_ui(state, UiSound::Close);
    state.song_folders = None;
}

/// Keys while the list is open: arrows move, DEL unlinks, ENTER on the add
/// button picks a folder, ESC closes.
pub fn key(state: &mut AppState, code: KeyCode) {
    let Some(list) = &mut state.song_folders else {
        return;
    };
    let last = list.paths.len();
    let row = list.row;
    match code {
        KeyCode::Escape => close(state),
        KeyCode::ArrowUp | KeyCode::KeyK if row > 0 => {
            list.row -= 1;
            play_ui(state, UiSound::Move);
        }
        KeyCode::ArrowDown | KeyCode::KeyJ if row < last => {
            list.row += 1;
            play_ui(state, UiSound::Move);
        }
        KeyCode::Delete | KeyCode::Backspace if row < last => unlink(state, row),
        KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space if row == last => pick(state),
        _ => (),
    }
}

/// A click while the list is open. A click outside the panel closes it.
pub fn click(state: &mut AppState, id: HitId) {
    match id {
        HitId::Blocker => close(state),
        HitId::FolderRow(i) => {
            if let Some(list) = &mut state.song_folders {
                list.row = i;
            }
        }
        HitId::UnlinkFolder(i) => unlink(state, i),
        HitId::LinkFolder => pick(state),
        _ => (),
    }
}

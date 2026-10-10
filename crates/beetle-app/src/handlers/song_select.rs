use std::fs;

use beetle_core::{sort_songs, ReplayData};
use winit::event::ElementState;
use winit::keyboard::KeyCode;

use crate::folders::{self, FolderPath, ListEntry};
use crate::gameplay::queue_start_gameplay;
use crate::handlers::key_config::open_key_config_from;
use crate::handlers::options::{handle_option_modal_input, open_options};
use crate::handlers::settings::open_settings;
use crate::ime::{append_text, backspace, key_text_to_append, set_search_active};
use crate::state::{replay_path, AppScreen, AppState};

/// Handles keyboard input for the Song Select screen.
pub fn handle_song_select_input(
    state: &mut AppState,
    key_state: ElementState,
    code: KeyCode,
    text: Option<&str>,
) {
    if key_state != ElementState::Pressed {
        return;
    }

    // If live search is active, capture search text input. While an IME
    // composition is open (`search_preedit`), Enter, Escape and Backspace
    // belong to the IME, so they do nothing here.
    if state.is_search_active {
        let composing = !state.search_preedit.is_empty();
        match code {
            KeyCode::Escape => {
                if composing {
                    return;
                }
                if !state.search_query.is_empty() {
                    state.search_query.clear();
                    state.recompute_entries();
                    state.cursor_settle_time = std::time::Instant::now();
                } else {
                    set_search_active(state, false);
                }
            }
            KeyCode::Enter => {
                if !composing {
                    set_search_active(state, false);
                }
            }
            KeyCode::Backspace => {
                if backspace(&mut state.search_query, &state.search_preedit) {
                    state.recompute_entries();
                    state.cursor_settle_time = std::time::Instant::now();
                }
            }
            _ => {
                if let Some(t) = text.and_then(|t| key_text_to_append(t, !composing)) {
                    if append_text(&mut state.search_query, t) {
                        state.recompute_entries();
                        state.cursor_settle_time = std::time::Instant::now();
                    }
                }
            }
        }
        return;
    }

    // If exit confirmation modal is open, handle confirm/cancel
    if state.show_exit_modal {
        match code {
            KeyCode::Enter | KeyCode::KeyY => {
                state.should_exit_app = true;
            }
            KeyCode::Escape | KeyCode::KeyN => {
                state.show_exit_modal = false;
            }
            _ => (),
        }
        return;
    }

    // If option modal is open, delegate to options handler
    if state.show_option_modal {
        handle_option_modal_input(state, code);
        return;
    }

    // Normal SongSelect navigation & hotkeys
    match code {
        // ESC goes up a folder; at the root it asks to quit.
        KeyCode::Escape => {
            if state.folder_path.is_root() {
                open_exit_prompt(state)
            } else {
                go_up(state)
            }
        }
        KeyCode::Slash => set_search_active(state, true),
        KeyCode::F1 => cycle_folder(state, false),
        KeyCode::F3 => cycle_folder(state, true),
        KeyCode::Tab | KeyCode::KeyO => open_options(state),
        KeyCode::KeyA => toggle_auto(state),
        KeyCode::KeyR => start_replay(state),
        KeyCode::F12 | KeyCode::KeyC => open_key_config(state),
        KeyCode::F4 => open_settings(state),
        KeyCode::F2 => cycle_sort(state),
        KeyCode::ArrowUp | KeyCode::KeyK => move_selection(state, false),
        KeyCode::ArrowDown | KeyCode::KeyJ => move_selection(state, true),
        KeyCode::ArrowRight => enter_folder(state),
        KeyCode::ArrowLeft | KeyCode::Backspace => go_up(state),
        KeyCode::PageUp => {
            if !state.entries.is_empty() {
                state.selected_entry = state.selected_entry.saturating_sub(10);
            }
            state.cursor_settle_time = std::time::Instant::now();
        }
        KeyCode::PageDown => {
            if !state.entries.is_empty() {
                state.selected_entry = (state.selected_entry + 10).min(state.entries.len() - 1);
            }
            state.cursor_settle_time = std::time::Instant::now();
        }
        KeyCode::Home => {
            state.selected_entry = 0;
            state.cursor_settle_time = std::time::Instant::now();
        }
        KeyCode::End => {
            if !state.entries.is_empty() {
                state.selected_entry = state.entries.len() - 1;
            }
            state.cursor_settle_time = std::time::Instant::now();
        }
        KeyCode::Enter | KeyCode::Space => activate_selected(state),
        KeyCode::F5 => state.start_rescan(),
        _ => {
            if let Some(t) = text {
                if t == "/" {
                    set_search_active(state, true);
                }
            }
        }
    }
}

/// Shows the quit prompt (ESC, or the footer button).
pub fn open_exit_prompt(state: &mut AppState) {
    state.show_exit_modal = true;
}

/// Turns auto play on or off (A, or the footer button).
pub fn toggle_auto(state: &mut AppState) {
    state.is_auto_play = !state.is_auto_play;
}

/// Opens the key configuration for the selected song's mode (F12 / C, or the footer button).
pub fn open_key_config(state: &mut AppState) {
    open_key_config_from(state, AppScreen::SongSelect);
}

/// Moves the highlight one row down or up, wrapping at the ends of the list.
/// Keeps the cursor still for the preview / jacket loader while it moves.
pub fn move_selection(state: &mut AppState, down: bool) {
    let len = state.entries.len();
    if len > 0 {
        state.selected_entry = if down {
            (state.selected_entry + 1) % len
        } else if state.selected_entry > 0 {
            state.selected_entry - 1
        } else {
            len - 1
        };
    }
    state.cursor_settle_time = std::time::Instant::now();
}

/// Shows `path` and saves it as the folder to open in next time. The cursor
/// goes to `focus` (a child folder id) when it is listed, else to the top.
pub fn set_folder(state: &mut AppState, path: FolderPath, focus: Option<&str>) {
    state.folder_path = path;
    state.recompute_entries();
    state.selected_entry = folders::focus_index(&state.entries, focus);
    state.cursor_settle_time = std::time::Instant::now();
    state.save_config();
}

/// Enters the highlighted folder row (ENTER on it, RIGHT, or a click on the selected row).
pub fn enter_folder(state: &mut AppState) {
    let Some(ListEntry::Folder { id, .. }) = state.entries.get(state.selected_entry) else {
        return;
    };
    let path = state.folder_path.child(id);
    set_folder(state, path, None);
}

/// Goes up one folder (BACKSPACE, LEFT, ESC). The cursor lands on the folder
/// the player came from. At the root this does nothing.
pub fn go_up(state: &mut AppState) {
    if state.folder_path.is_root() {
        return;
    }
    let came_from = state.folder_path.last().map(str::to_string);
    let parent = state.folder_path.parent();
    set_folder(state, parent, came_from.as_deref());
}

/// Goes to the breadcrumb at `depth` (0 = the root). A click on the current one does nothing.
pub fn go_to_crumb(state: &mut AppState, depth: usize) {
    if depth >= state.folder_path.depth() {
        return;
    }
    let came_from = state.folder_path.segments().get(depth).cloned();
    let target = state.folder_path.truncated(depth);
    set_folder(state, target, came_from.as_deref());
}

/// Moves to the next (or previous) folder at the same depth, wrapping.
pub fn cycle_folder(state: &mut AppState, forward: bool) {
    if let Some(next) = folders::sibling(&state.folder_tree, &state.folder_path, forward) {
        set_folder(state, next, None);
    }
}

/// ENTER or a click on the selected row: a folder opens, a song plays.
pub fn activate_selected(state: &mut AppState) {
    if state.current_selected_song().is_some() {
        start_selected(state);
    } else {
        enter_folder(state);
    }
}

/// Cycles the sort mode, re-sorts the library and saves the choice.
pub fn cycle_sort(state: &mut AppState) {
    state.sort_mode = state.sort_mode.next();
    let ln_option = state.ln_option();
    sort_songs(
        &mut state.songs,
        state.sort_mode,
        &state.score_store,
        ln_option,
    );
    state.recompute_entries();
    state.cursor_settle_time = std::time::Instant::now();
    state.save_config();
}

/// Plays the highlighted song (ENTER, or a click on the selected row).
pub fn start_selected(state: &mut AppState) {
    if let Some(song) = state.current_selected_song().cloned() {
        state.is_replay_playback = false;
        queue_start_gameplay(state, &song);
    }
}

/// Plays the highlighted song's saved replay (R), if it has one.
pub fn start_replay(state: &mut AppState) {
    if let Some(song) = state.current_selected_song().cloned() {
        let path_str = replay_path(song.id, song.score_rule(state.ln_option()));
        if let Ok(rep_str) = fs::read_to_string(&path_str) {
            if let Some(replay) = ReplayData::parse_from_str(&rep_str) {
                state.is_replay_playback = true;
                state.playback_replay = Some(replay);
                state.playback_cursor = 0;
                queue_start_gameplay(state, &song);
            }
        }
    }
}

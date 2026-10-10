use std::fs;

use beetle_core::{sort_songs, ReplayData};
use winit::event::ElementState;
use winit::keyboard::KeyCode;

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
                    state.recompute_filtered_songs();
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
                    state.recompute_filtered_songs();
                    state.cursor_settle_time = std::time::Instant::now();
                }
            }
            _ => {
                if let Some(t) = text.and_then(|t| key_text_to_append(t, !composing)) {
                    if append_text(&mut state.search_query, t) {
                        state.recompute_filtered_songs();
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
        KeyCode::Escape => open_exit_prompt(state),
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
        KeyCode::PageUp => {
            if !state.filtered_indices.is_empty() {
                state.selected_song_idx = state.selected_song_idx.saturating_sub(10);
            }
            state.cursor_settle_time = std::time::Instant::now();
        }
        KeyCode::PageDown => {
            if !state.filtered_indices.is_empty() {
                state.selected_song_idx =
                    (state.selected_song_idx + 10).min(state.filtered_indices.len() - 1);
            }
            state.cursor_settle_time = std::time::Instant::now();
        }
        KeyCode::Home => {
            state.selected_song_idx = 0;
            state.cursor_settle_time = std::time::Instant::now();
        }
        KeyCode::End => {
            if !state.filtered_indices.is_empty() {
                state.selected_song_idx = state.filtered_indices.len() - 1;
            }
            state.cursor_settle_time = std::time::Instant::now();
        }
        KeyCode::Enter | KeyCode::Space => start_selected(state),
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
    let len = state.filtered_indices.len();
    if len > 0 {
        state.selected_song_idx = if down {
            (state.selected_song_idx + 1) % len
        } else if state.selected_song_idx > 0 {
            state.selected_song_idx - 1
        } else {
            len - 1
        };
    }
    state.cursor_settle_time = std::time::Instant::now();
}

/// Moves the folder selector to the next (or previous) folder.
pub fn cycle_folder(state: &mut AppState, forward: bool) {
    let tables = state.tables.tables().len();
    state.category_mode = if forward {
        state.category_mode.next(tables)
    } else {
        state.category_mode.prev(tables)
    };
    state.recompute_filtered_songs();
    state.cursor_settle_time = std::time::Instant::now();
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
    state.recompute_filtered_songs();
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

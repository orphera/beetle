use std::fs;

use beetle_core::{sort_songs, ReplayData};
use winit::event::ElementState;
use winit::keyboard::KeyCode;

use crate::gameplay::queue_start_gameplay;
use crate::handlers::options::handle_option_modal_input;
use crate::state::{init_songs_and_scores, AppScreen, AppState, REPLAYS_DIR};

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

    // If live search is active, capture search text input
    if state.is_search_active {
        match code {
            KeyCode::Escape => {
                if !state.search_query.is_empty() {
                    state.search_query.clear();
                    state.recompute_filtered_songs();
                } else {
                    state.is_search_active = false;
                }
            }
            KeyCode::Enter => {
                state.is_search_active = false;
            }
            KeyCode::Backspace => {
                state.search_query.pop();
                state.recompute_filtered_songs();
            }
            _ => {
                if let Some(t) = text {
                    for c in t.chars() {
                        if !c.is_control() {
                            state.search_query.push(c);
                        }
                    }
                    state.recompute_filtered_songs();
                }
            }
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
        KeyCode::Slash => {
            state.is_search_active = true;
        }
        KeyCode::F1 => {
            state.category_mode = state.category_mode.prev();
            state.recompute_filtered_songs();
        }
        KeyCode::F3 => {
            state.category_mode = state.category_mode.next();
            state.recompute_filtered_songs();
        }
        KeyCode::Tab | KeyCode::KeyO => {
            state.show_option_modal = true;
            state.modal_row = 0;
        }
        KeyCode::KeyA => {
            state.is_auto_play = !state.is_auto_play;
        }
        KeyCode::KeyR => {
            // Launch replay playback if replay file exists
            if let Some(song) = state.current_selected_song().cloned() {
                let path_str = format!("{}/{:016x}.rep", REPLAYS_DIR, song.hash);
                if let Ok(rep_str) = fs::read_to_string(&path_str) {
                    if let Some(replay) = ReplayData::parse_from_str(&rep_str) {
                        state.is_replay_playback = true;
                        state.playback_replay = Some(replay);
                        state.playback_cursor = 0;
                        queue_start_gameplay(state, &song);
                        return;
                    }
                }
            }
        }
        KeyCode::F12 | KeyCode::KeyC => {
            state.screen = AppScreen::KeyConfig;
            state.selected_key_idx = 0;
        }
        KeyCode::F2 => {
            // Cycle Sort Mode
            state.sort_mode = state.sort_mode.next();
            sort_songs(&mut state.songs, state.sort_mode, &state.score_store);
            state.recompute_filtered_songs();
            state.save_config();
        }
        KeyCode::ArrowUp | KeyCode::KeyK => {
            if state.selected_song_idx > 0 {
                state.selected_song_idx -= 1;
            } else if !state.filtered_indices.is_empty() {
                state.selected_song_idx = state.filtered_indices.len() - 1;
            }
        }
        KeyCode::ArrowDown | KeyCode::KeyJ => {
            if !state.filtered_indices.is_empty() {
                state.selected_song_idx = (state.selected_song_idx + 1) % state.filtered_indices.len();
            }
        }
        KeyCode::Enter | KeyCode::Space => {
            if let Some(song) = state.current_selected_song().cloned() {
                state.is_replay_playback = false;
                queue_start_gameplay(state, &song);
            }
        }
        KeyCode::F5 => {
            let (mut songs, _) = init_songs_and_scores(state.sort_mode);
            sort_songs(&mut songs, state.sort_mode, &state.score_store);
            state.songs = songs;
            state.recompute_filtered_songs();
        }
        _ => {
            if let Some(t) = text {
                if t == "/" {
                    state.is_search_active = true;
                }
            }
        }
    }
}

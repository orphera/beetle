use std::fs;

use std::time::Instant;

use beetle_core::{sort_songs, ReplayData, SortMode};
use beetle_render::{filter_items, FilterItem};
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

    // The sort menu and the filter row take their keys first. Any other key
    // closes the menu or leaves the row, and then acts as usual.
    if sort_menu_key(state, code) || filter_key(state, code) {
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
        KeyCode::F10 => focus_filter(state),
        KeyCode::ArrowUp | KeyCode::KeyK => move_selection(state, false),
        KeyCode::ArrowDown | KeyCode::KeyJ => move_selection(state, true),
        // On a group row the arrows switch its chart; elsewhere they open or leave a folder.
        KeyCode::ArrowRight => {
            if !cycle_chart(state, true) {
                enter_folder(state);
            }
        }
        KeyCode::ArrowLeft => {
            if !cycle_chart(state, false) {
                go_up(state);
            }
        }
        KeyCode::Backspace => go_up(state),
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

/// Moves a group row to its next (or previous) chart, wrapping. Returns false
/// when the highlighted row is not a group (the arrows then do their folder job).
pub fn cycle_chart(state: &mut AppState, forward: bool) -> bool {
    let Some(ListEntry::Group {
        charts, selected, ..
    }) = state.entries.get(state.selected_entry)
    else {
        return false;
    };
    let n = charts.len();
    let next = if forward {
        (selected + 1) % n
    } else {
        (selected + n - 1) % n
    };
    let row = state.selected_entry;
    choose_chart(state, row, next);
    true
}

/// Shows chart `pos` of the group at `row` and remembers it (a chip or tab
/// click, or the arrows). The first chart is the default, so it is not stored.
pub fn choose_chart(state: &mut AppState, row: usize, pos: usize) {
    let Some(ListEntry::Group { key, charts, .. }) = state.entries.get(row) else {
        return;
    };
    let (key, Some(&chart)) = (*key, charts.get(pos)) else {
        return;
    };
    if pos == 0 {
        state.chart_choice.remove(&key);
    } else {
        state.chart_choice.insert(key, state.songs[chart].id);
    }
    state.selected_entry = row;
    state.recompute_entries();
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

/// Cycles the sort mode (F2), re-sorts the library and saves the choice.
pub fn cycle_sort(state: &mut AppState) {
    let next = state.sort_mode.next();
    set_sort(state, next);
}

/// Sorts the library by `mode`, re-sorts the rows and saves the choice.
pub fn set_sort(state: &mut AppState, mode: SortMode) {
    state.sort_mode = mode;
    let ln_option = state.ln_option();
    sort_songs(
        &mut state.songs,
        state.sort_mode,
        &state.score_store,
        ln_option,
    );
    state.recompute_entries();
    state.cursor_settle_time = Instant::now();
    state.save_config();
}

/// The position of the sort mode in the sort menu.
fn sort_index(mode: SortMode) -> usize {
    SortMode::ALL.iter().position(|&m| m == mode).unwrap_or(0)
}

/// Opens the sort menu with the mode in use highlighted.
pub fn open_sort_menu(state: &mut AppState) {
    state.sort_menu = Some(sort_index(state.sort_mode));
}

/// Opens the sort menu, or closes it when it is open (a click on the sort selector).
pub fn toggle_sort_menu(state: &mut AppState) {
    if state.sort_menu.is_some() {
        close_sort_menu(state);
    } else {
        open_sort_menu(state);
    }
}

/// Closes the sort menu without changing the sort (ESC, or a click outside).
pub fn close_sort_menu(state: &mut AppState) {
    state.sort_menu = None;
}

/// Moves the menu's highlight one option down or up, wrapping.
pub fn move_sort_highlight(state: &mut AppState, down: bool) {
    let n = SortMode::ALL.len();
    if let Some(highlight) = state.sort_menu.as_mut() {
        *highlight = if down {
            (*highlight + 1) % n
        } else {
            (*highlight + n - 1) % n
        };
    }
}

/// Picks the sort option at `index` (a click, or ENTER on the highlight) and closes the menu.
pub fn pick_sort(state: &mut AppState, index: usize) {
    state.sort_menu = None;
    if let Some(&mode) = SortMode::ALL.get(index) {
        set_sort(state, mode);
    }
}

/// The sort menu's keys while it is open. Returns false (after closing it)
/// for a key the menu does not use, so that key acts as usual.
fn sort_menu_key(state: &mut AppState, code: KeyCode) -> bool {
    let Some(highlight) = state.sort_menu else {
        return false;
    };
    match code {
        KeyCode::Escape => close_sort_menu(state),
        KeyCode::ArrowUp | KeyCode::KeyK => move_sort_highlight(state, false),
        KeyCode::ArrowDown | KeyCode::KeyJ => move_sort_highlight(state, true),
        KeyCode::Enter | KeyCode::Space => pick_sort(state, highlight),
        _ => {
            close_sort_menu(state);
            return false;
        }
    }
    true
}

/// The filter row item at `index`, as the row lists them now.
fn filter_item_at(state: &AppState, index: usize) -> Option<FilterItem> {
    filter_items(state.present_modes.len(), state.filter.is_active())
        .get(index)
        .copied()
}

/// Gives the filter row keyboard focus (F10), or takes it back.
pub fn focus_filter(state: &mut AppState) {
    state.filter_focus = match state.filter_focus {
        Some(_) => None,
        None => Some(0),
    };
}

/// Runs filter item `index` as ENTER or a click does: a mode chip toggles, a
/// level bound steps up, a toggle flips, 초기화 clears the filter.
pub fn activate_filter(state: &mut AppState, index: usize) {
    let Some(item) = filter_item_at(state, index) else {
        return;
    };
    match item {
        FilterItem::Mode(i) => match state.present_modes.get(i) {
            Some(&mode) => state.filter.toggle_mode(mode),
            None => return,
        },
        FilterItem::LevelMin => state.filter.step_level(&state.level_steps, false, true),
        FilterItem::LevelMax => state.filter.step_level(&state.level_steps, true, true),
        FilterItem::Unplayed => state.filter.only_unplayed = !state.filter.only_unplayed,
        FilterItem::Uncleared => state.filter.only_uncleared = !state.filter.only_uncleared,
        FilterItem::Reset => state.filter.clear(),
    }
    apply_filter(state);
}

/// Steps a level bound of filter item `index` down or up (its arrows, or UP / DOWN on it).
pub fn step_filter_level(state: &mut AppState, index: usize, up: bool) {
    match filter_item_at(state, index) {
        Some(FilterItem::LevelMin) => state.filter.step_level(&state.level_steps, false, up),
        Some(FilterItem::LevelMax) => state.filter.step_level(&state.level_steps, true, up),
        _ => return,
    }
    apply_filter(state);
}

/// Shows every song again (the 초기화 item, or the empty list's button).
pub fn clear_filter(state: &mut AppState) {
    state.filter.clear();
    apply_filter(state);
}

/// Rebuilds the list after a filter change and saves the filter.
fn apply_filter(state: &mut AppState) {
    state.recompute_entries();
    state.cursor_settle_time = Instant::now();
    state.save_config();
}

/// The filter row's keys while it has focus: LEFT / RIGHT move the focus,
/// ENTER activates the item, UP / DOWN step a level bound, ESC / F10 leave the
/// row. Returns false (after leaving the row) for any other key.
fn filter_key(state: &mut AppState, code: KeyCode) -> bool {
    let Some(focus) = state.filter_focus else {
        return false;
    };
    let last = filter_items(state.present_modes.len(), state.filter.is_active()).len() - 1;
    match code {
        KeyCode::Escape | KeyCode::F10 => state.filter_focus = None,
        KeyCode::ArrowLeft => state.filter_focus = Some(focus.saturating_sub(1)),
        KeyCode::ArrowRight => state.filter_focus = Some((focus + 1).min(last)),
        KeyCode::Enter | KeyCode::Space => {
            activate_filter(state, focus);
            // Reset may have left the row: keep the focus on an item.
            let last = filter_items(state.present_modes.len(), state.filter.is_active()).len() - 1;
            state.filter_focus = Some(focus.min(last));
        }
        KeyCode::ArrowUp | KeyCode::ArrowDown
            if matches!(
                filter_item_at(state, focus),
                Some(FilterItem::LevelMin | FilterItem::LevelMax)
            ) =>
        {
            step_filter_level(state, focus, code == KeyCode::ArrowUp);
        }
        _ => {
            state.filter_focus = None;
            return false;
        }
    }
    true
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

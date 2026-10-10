//! Mouse input. Clicks and the wheel run the same actions as the keys. The
//! hit regions come from the last presented frame (`Ui::hits`), so a click
//! lands on what the player saw. Only the Song Select screen and its two
//! modals take the mouse for now; other screens and gameplay ignore it.

use std::time::Instant;

use beetle_render::{hit_at, HitId};
use winit::event::MouseScrollDelta;

use crate::handlers::options::{change_option, close_options, move_option_row, open_options};
use crate::handlers::song_select::{
    cycle_folder, cycle_sort, move_selection, start_replay, start_selected,
};
use crate::state::{AppScreen, AppState};

/// Precise (touchpad) scrolling reports pixels; this many make one notch.
const PIXELS_PER_NOTCH: f32 = 40.0;
/// Most notches one event may move the list, so a fast flick stays usable.
const MAX_NOTCHES: i32 = 8;

/// A left button press at the cursor position.
pub fn handle_press(state: &mut AppState) {
    if state.screen != AppScreen::SongSelect {
        return;
    }
    let Some((x, y)) = state.cursor else {
        return;
    };
    let id = hit_at(&state.gpu_ui.ui.hits, x, y);
    // Clicking anywhere but the search box ends the search.
    if state.is_search_active && id != Some(HitId::Search) {
        state.is_search_active = false;
    }
    if let Some(id) = id {
        handle_click(state, id);
    }
}

/// Runs the action of a clicked region.
pub fn handle_click(state: &mut AppState, id: HitId) {
    if state.screen != AppScreen::SongSelect {
        return;
    }
    if state.show_exit_modal {
        match id {
            HitId::ExitQuit => state.should_exit_app = true,
            HitId::ExitCancel | HitId::Blocker => state.show_exit_modal = false,
            _ => (),
        }
        return;
    }
    if state.show_option_modal {
        match id {
            HitId::OptionRow(row) => state.modal_row = row,
            HitId::OptionPrev(row) => {
                state.modal_row = row;
                change_option(state, row, false);
            }
            HitId::OptionNext(row) => {
                state.modal_row = row;
                change_option(state, row, true);
            }
            HitId::Blocker => close_options(state),
            _ => (),
        }
        return;
    }
    match id {
        // The first click selects a row; a click on the selected row plays it.
        HitId::SongRow(i) if i == state.selected_song_idx => start_selected(state),
        HitId::SongRow(i) => {
            state.selected_song_idx = i;
            state.cursor_settle_time = Instant::now();
        }
        HitId::FolderPrev => cycle_folder(state, false),
        HitId::FolderNext => cycle_folder(state, true),
        HitId::Sort => cycle_sort(state),
        HitId::Search => state.is_search_active = true,
        HitId::Play => start_selected(state),
        HitId::Replay => start_replay(state),
        HitId::Settings => open_options(state),
        _ => (),
    }
}

/// A wheel or touchpad scroll. Up moves the highlight up; in the options
/// modal it moves the highlighted row instead.
pub fn handle_wheel(state: &mut AppState, delta: MouseScrollDelta) {
    if state.screen != AppScreen::SongSelect || state.show_exit_modal {
        return;
    }
    let lines = match delta {
        MouseScrollDelta::LineDelta(_, y) => y,
        MouseScrollDelta::PixelDelta(p) => p.y as f32 / PIXELS_PER_NOTCH,
    };
    state.wheel_carry += lines;
    let whole = state.wheel_carry.trunc() as i32;
    state.wheel_carry -= whole as f32;
    let notches = whole.clamp(-MAX_NOTCHES, MAX_NOTCHES);
    let down = notches < 0;
    for _ in 0..notches.abs() {
        if state.show_option_modal {
            move_option_row(state, down);
        } else {
            move_selection(state, down);
        }
    }
}

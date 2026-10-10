//! Mouse input. Clicks and the wheel run the same actions as the keys. The
//! hit regions come from the last presented frame (`Ui::hits`), so a click
//! lands on what the player saw. Song select (with its modals), the result
//! screen, key configuration and settings take the mouse; gameplay, loading
//! and boot ignore it.

use std::time::Instant;

use beetle_render::{hit_at, HitId, Rebind, KEY_MODES};
use winit::event::MouseScrollDelta;

use crate::handlers::key_config::{
    clear_lane_keys, cycle_key_preset, leave_key_config, reset_key_layout, set_key_mode,
    step_key_mode, toggle_eight_k_form, toggle_scratch_side,
};
use crate::handlers::options::{
    change_option, close_options, move_option_row, move_row, open_options,
};
use crate::handlers::result::{retry_song, take_screenshot, to_song_select};
use crate::handlers::settings::{open_settings, settings_click};
use crate::handlers::song_select::{
    activate_selected, cycle_folder, cycle_sort, go_to_crumb, go_up, move_selection,
    open_exit_prompt, open_key_config, start_replay, toggle_auto,
};
use crate::ime::set_search_active;
use crate::options_table::SETTINGS;
use crate::state::{AppScreen, AppState};

/// Precise (touchpad) scrolling reports pixels; this many make one notch.
const PIXELS_PER_NOTCH: f32 = 40.0;
/// Most notches one event may move the list, so a fast flick stays usable.
const MAX_NOTCHES: i32 = 8;

/// A left button press at the cursor position.
pub fn handle_press(state: &mut AppState) {
    match state.screen {
        // A click anywhere cancels a pending key bind, as ESC does.
        AppScreen::KeyConfig if state.rebinding.is_some() => state.rebinding = None,
        AppScreen::SongSelect | AppScreen::KeyConfig | AppScreen::Result | AppScreen::Settings => {
            let Some((x, y)) = state.cursor else {
                return;
            };
            let id = hit_at(&state.gpu_ui.ui.hits, x, y);
            // Clicking anywhere but the search box ends the search.
            if state.screen == AppScreen::SongSelect
                && state.is_search_active
                && id != Some(HitId::Search)
            {
                set_search_active(state, false);
            }
            if let Some(id) = id {
                handle_click(state, id);
            }
        }
        _ => (),
    }
}

/// Runs the action of a clicked region on the current screen.
pub fn handle_click(state: &mut AppState, id: HitId) {
    match state.screen {
        AppScreen::SongSelect => song_select_click(state, id),
        AppScreen::Result => result_click(state, id),
        AppScreen::KeyConfig => key_config_click(state, id),
        AppScreen::Settings => settings_click(state, id),
        _ => (),
    }
}

fn song_select_click(state: &mut AppState, id: HitId) {
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
        HitId::ListRow(i) if i == state.selected_entry => activate_selected(state),
        HitId::ListRow(i) => {
            state.selected_entry = i;
            state.cursor_settle_time = Instant::now();
        }
        HitId::FolderPrev => cycle_folder(state, false),
        HitId::FolderNext => cycle_folder(state, true),
        HitId::FolderUp => go_up(state),
        HitId::Crumb(depth) => go_to_crumb(state, depth),
        HitId::Sort => cycle_sort(state),
        HitId::Search => set_search_active(state, true),
        HitId::Play => activate_selected(state),
        HitId::Replay => start_replay(state),
        HitId::PlayOptions => open_options(state),
        HitId::OpenSettings => open_settings(state),
        HitId::Auto => toggle_auto(state),
        HitId::KeyConfig => open_key_config(state),
        HitId::Quit => open_exit_prompt(state),
        _ => (),
    }
}

fn result_click(state: &mut AppState, id: HitId) {
    match id {
        HitId::ResultSongSelect => to_song_select(state),
        HitId::ResultRetry => retry_song(state),
        HitId::ResultScreenshot => take_screenshot(state),
        _ => (),
    }
}

fn key_config_click(state: &mut AppState, id: HitId) {
    match id {
        HitId::KeyModeTab(i) => {
            if let Some(&mode) = KEY_MODES.get(i) {
                set_key_mode(state, mode);
            }
        }
        // The first click selects a lane; a click on the selected lane binds
        // a key, as ENTER does.
        HitId::KeyLane(i) if i == state.selected_key_idx => state.rebinding = Some(Rebind::Replace),
        HitId::KeyLane(i) => state.selected_key_idx = i,
        HitId::KeySet => state.rebinding = Some(Rebind::Replace),
        HitId::KeyAdd => state.rebinding = Some(Rebind::Add),
        HitId::KeyClear => clear_lane_keys(state),
        HitId::KeyPreset => cycle_key_preset(state),
        HitId::KeyScratch => toggle_scratch_side(state),
        HitId::KeyForm => toggle_eight_k_form(state),
        HitId::KeyReset => reset_key_layout(state),
        HitId::KeyBack => leave_key_config(state),
        _ => (),
    }
}

/// A wheel or touchpad scroll. Up moves the highlight up; in the options
/// panel and on the Settings screen it moves the highlighted row; on key
/// configuration it switches the key mode (up = previous).
pub fn handle_wheel(state: &mut AppState, delta: MouseScrollDelta) {
    let lines = match delta {
        MouseScrollDelta::LineDelta(_, y) => y,
        MouseScrollDelta::PixelDelta(p) => p.y as f32 / PIXELS_PER_NOTCH,
    };
    state.wheel_carry += lines;
    let whole = state.wheel_carry.trunc() as i32;
    state.wheel_carry -= whole as f32;
    let notches = whole.clamp(-MAX_NOTCHES, MAX_NOTCHES);
    if notches == 0 {
        return;
    }
    let up = notches > 0;
    for _ in 0..notches.abs() {
        match state.screen {
            AppScreen::SongSelect if !state.show_exit_modal => {
                if state.show_option_modal {
                    move_option_row(state, !up);
                } else {
                    move_selection(state, !up);
                }
            }
            AppScreen::KeyConfig if state.rebinding.is_none() => step_key_mode(state, up),
            AppScreen::Settings if state.calibration.is_none() => {
                move_row(&mut state.settings_row, SETTINGS.len(), !up)
            }
            _ => (),
        }
    }
}

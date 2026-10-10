//! The Settings screen (F4 on song select, or the SETTINGS button): the
//! values that are set once. Keys and clicks run the same actions.

use beetle_render::{strings, HitId, ToastKind};
use winit::event::ElementState;
use winit::keyboard::KeyCode;

use crate::handlers::options::{enter_row, move_row};
use crate::options_table::{self, activation, SETTINGS};
use crate::state::{AppScreen, AppState};
use crate::transition::show_toast;

/// Opens the Settings screen (F4, or the SETTINGS button).
pub fn open_settings(state: &mut AppState) {
    state.screen = AppScreen::Settings;
    state.settings_row = 0;
    state.show_option_modal = false;
}

/// Back to song select (ESC, or the footer button). Saves the settings, and
/// says so when a value only applies after a restart.
pub fn leave_settings(state: &mut AppState) {
    state.screen = AppScreen::SongSelect;
    state.save_config();
    if state.gpu_backend != state.gpu_backend_at_start {
        show_toast(state, ToastKind::Info, strings::TOAST_RESTART_NEEDED);
    }
}

/// Changes the value of Settings row `row` one step and saves.
fn change_setting(state: &mut AppState, row: usize, forward: bool) {
    if let Some(desc) = SETTINGS.get(row) {
        options_table::step(state, desc.id, forward);
        state.save_config();
    }
}

/// Handles keyboard input on the Settings screen.
pub fn handle_settings_input(state: &mut AppState, key_state: ElementState, code: KeyCode) {
    if key_state != ElementState::Pressed {
        return;
    }
    let row = state.settings_row;
    match code {
        KeyCode::Escape => leave_settings(state),
        KeyCode::ArrowUp | KeyCode::KeyK => {
            move_row(&mut state.settings_row, SETTINGS.len(), false)
        }
        KeyCode::ArrowDown | KeyCode::KeyJ => {
            move_row(&mut state.settings_row, SETTINGS.len(), true)
        }
        KeyCode::ArrowLeft | KeyCode::KeyH => change_setting(state, row, false),
        KeyCode::ArrowRight | KeyCode::KeyL => change_setting(state, row, true),
        KeyCode::Enter | KeyCode::Space => {
            enter_row(state, SETTINGS, row, AppScreen::Settings);
        }
        _ => (),
    }
}

/// A click on the Settings screen. The first click on a row selects it; a
/// click on the selected row runs its activation (the key layout opens Key
/// Config). `<` and `>` change the value.
pub fn settings_click(state: &mut AppState, id: HitId) {
    match id {
        HitId::OptionRow(i) if i == state.settings_row => {
            let opens = SETTINGS.get(i).is_some_and(|d| activation(d.id).is_some());
            if opens {
                enter_row(state, SETTINGS, i, AppScreen::Settings);
            }
        }
        HitId::OptionRow(i) => state.settings_row = i,
        HitId::OptionPrev(i) => {
            state.settings_row = i;
            change_setting(state, i, false);
        }
        HitId::OptionNext(i) => {
            state.settings_row = i;
            change_setting(state, i, true);
        }
        HitId::SettingsBack => leave_settings(state),
        _ => (),
    }
}

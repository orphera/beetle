//! The Settings screen (F4 on song select, or the SETTINGS button): the
//! values that are set once. Keys and clicks run the same actions. The judge
//! offset calibration is a sub-state of this screen: while it is open, keys
//! and clicks go to it instead of the rows.

use beetle_render::{strings, HitId, ToastKind};
use winit::event::ElementState;
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::calibration::Session;
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
    state.calibration = None;
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

/// Opens the judge offset calibration with a fresh test (ENTER on the
/// judge offset row). Does nothing when it is already open.
pub fn open_calibration(state: &mut AppState) {
    if state.calibration.is_none() {
        state.calibration = Some(Session::open(state.master_volume));
    }
}

/// Changes the calibration test when a click or key asks for it: applies the
/// suggestion (only once it is done), measures again, or cancels. Cancel
/// drops the session, which releases its audio engine.
fn calibration_action(state: &mut AppState, id: HitId) {
    match id {
        HitId::CalibrateApply => apply_calibration(state),
        HitId::CalibrateRetry => {
            if let Some(cal) = &mut state.calibration {
                cal.restart();
            }
        }
        HitId::CalibrateCancel => state.calibration = None,
        _ => (),
    }
}

/// Sets the judge offset to the suggestion, saves, and returns to Settings.
/// Does nothing before the test is done.
fn apply_calibration(state: &mut AppState) {
    let suggestion = state
        .calibration
        .as_ref()
        .filter(|c| c.is_done())
        .and_then(|c| c.test().suggestion());
    let Some(offset) = suggestion else {
        return;
    };
    state.calibration = None;
    state.play_options.judge_offset_ms = offset;
    state.save_config();
    show_toast(
        state,
        ToastKind::Success,
        strings::fill(
            strings::TOAST_CALIBRATE_APPLIED,
            &[&format!("{offset:+.0}")],
        ),
    );
}

/// Keys while the calibration is open. ESC cancels, R measures again. ENTER
/// applies once the test is done. While it runs, Space, ENTER and any lane key
/// of the selected song's mode are presses (repeats do not count).
pub fn handle_calibration_input(
    state: &mut AppState,
    key_state: ElementState,
    code: KeyCode,
    physical_key: PhysicalKey,
    repeat: bool,
) {
    if key_state != ElementState::Pressed {
        return;
    }
    let done = state.calibration.as_ref().is_some_and(|c| c.is_done());
    match code {
        KeyCode::Escape => calibration_action(state, HitId::CalibrateCancel),
        KeyCode::KeyR => calibration_action(state, HitId::CalibrateRetry),
        KeyCode::Enter | KeyCode::NumpadEnter if done => {
            calibration_action(state, HitId::CalibrateApply)
        }
        // Taps come from the raw input thread when it runs (`calibration_tap`).
        _ if repeat || state.raw_keys.is_some() => (),
        _ if is_calibration_tap(state, physical_key) => {
            if let Some(cal) = state.calibration.as_mut() {
                cal.press();
            }
        }
        _ => (),
    }
}

/// A lane key of the layout being calibrated, Space or Enter.
fn is_calibration_tap(state: &AppState, physical_key: PhysicalKey) -> bool {
    let mode = state.key_config_mode();
    state.key_bindings.get(mode).map_key(physical_key).is_some()
        || matches!(
            physical_key,
            PhysicalKey::Code(KeyCode::Space | KeyCode::Enter | KeyCode::NumpadEnter)
        )
}

/// A key from the raw input thread during the calibration, pressed at `at`.
/// ESC, R and Enter on a finished test stay with winit's key events.
pub fn calibration_tap(state: &mut AppState, code: KeyCode, at: std::time::Instant) {
    let done = state.calibration.as_ref().is_some_and(|c| c.is_done());
    if done || matches!(code, KeyCode::Escape | KeyCode::KeyR) {
        return;
    }
    if is_calibration_tap(state, PhysicalKey::Code(code)) {
        if let Some(cal) = state.calibration.as_mut() {
            cal.press_at(at);
        }
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
/// Config, the judge offset opens the calibration). `<` and `>` change the
/// value. While the calibration is open only its own buttons respond.
pub fn settings_click(state: &mut AppState, id: HitId) {
    if state.calibration.is_some() {
        calibration_action(state, id);
        return;
    }
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

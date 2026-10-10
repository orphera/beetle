use beetle_core::{Lane, PlayMode};
use beetle_render::{scratch_side_applies, Rebind, KEY_MODES};
use winit::event::ElementState;
use winit::keyboard::KeyCode;

use crate::input::{lanes_for, screen_lanes_for, KeyPreset};
use crate::state::{AppScreen, AppState};
use crate::transition::show_toast;
use beetle_render::{strings, theme, ToastKind};

/// Handles keyboard input on the Key Configuration screen.
///
/// Left/Right pick a lane (in screen order), Up/Down switch the key mode
/// being edited (each mode has its own layout). Enter waits for a key that
/// replaces the lane's keys, A waits for one more key, Backspace clears the
/// lane. F1 cycles the key preset, F2 puts the scratch on the other edge
/// (5K / 7K / 8K), F3 switches 8K between a straight row and 6 keys + L/R
/// triggers.
pub fn handle_key_config_input(state: &mut AppState, key_state: ElementState, code: KeyCode) {
    if key_state != ElementState::Pressed {
        return;
    }

    let mode = state.key_config_edit_mode;

    if let Some(rebind) = state.rebinding {
        state.rebinding = None;
        if code == KeyCode::Escape {
            return;
        }
        if let Some(lane) = selected_lane(state) {
            let layout = state.key_bindings.get_mut(mode);
            match rebind {
                Rebind::Replace => layout.bind_key(code, lane),
                Rebind::Add => layout.add_key(code, lane),
            }
            state.save_config();
        }
        return;
    }

    match code {
        KeyCode::Escape => leave_key_config(state),
        KeyCode::Enter | KeyCode::Space => state.rebinding = Some(Rebind::Replace),
        KeyCode::KeyA => state.rebinding = Some(Rebind::Add),
        KeyCode::Backspace => clear_lane_keys(state),
        KeyCode::ArrowLeft | KeyCode::KeyH => {
            state.selected_key_idx = state.selected_key_idx.saturating_sub(1);
        }
        KeyCode::ArrowRight | KeyCode::KeyL => {
            let lanes = screen_lanes_for(&state.view.skin, mode);
            state.selected_key_idx =
                (state.selected_key_idx + 1).min(lanes.len().saturating_sub(1));
        }
        KeyCode::ArrowUp | KeyCode::KeyK => step_key_mode(state, true),
        KeyCode::ArrowDown | KeyCode::KeyJ => step_key_mode(state, false),
        KeyCode::F1 => cycle_key_preset(state),
        KeyCode::F2 if scratch_side_applies(mode) => toggle_scratch_side(state),
        KeyCode::F3 if mode == PlayMode::Keys8 => toggle_eight_k_form(state),
        KeyCode::Delete => reset_key_layout(state),
        _ => (),
    }
}

/// The lane under the selection in the edited mode.
fn selected_lane(state: &AppState) -> Option<Lane> {
    screen_lanes_for(&state.view.skin, state.key_config_edit_mode)
        .get(state.selected_key_idx)
        .copied()
}

/// Back to the song list (ESC, or the footer button).
pub fn leave_key_config(state: &mut AppState) {
    state.screen = AppScreen::SongSelect;
    state.save_config();
}

/// Removes every key of the selected lane (BKSP, or the footer button).
pub fn clear_lane_keys(state: &mut AppState) {
    if let Some(lane) = selected_lane(state) {
        let mode = state.key_config_edit_mode;
        state.key_bindings.get_mut(mode).clear_lane(lane);
        state.save_config();
    }
}

/// Edits the layout of `mode` (a tab click, or Up/Down); the selection stays
/// on the same index, clamped to the new mode's lane count.
pub fn set_key_mode(state: &mut AppState, mode: PlayMode) {
    state.key_config_edit_mode = mode;
    let count = lanes_for(mode).len();
    state.selected_key_idx = state.selected_key_idx.min(count.saturating_sub(1));
}

/// The previous (`up`) or next key mode in tab order, wrapping around.
pub fn step_key_mode(state: &mut AppState, up: bool) {
    let mode = state.key_config_edit_mode;
    let i = KEY_MODES.iter().position(|&m| m == mode).unwrap_or(0);
    let n = KEY_MODES.len();
    let next = if up { (i + n - 1) % n } else { (i + 1) % n };
    set_key_mode(state, KEY_MODES[next]);
}

/// Cycles the key preset of the edited mode (F1, or the footer button).
pub fn cycle_key_preset(state: &mut AppState) {
    let mode = state.key_config_edit_mode;
    let lane = selected_lane(state);
    state.key_bindings.get_mut(mode).cycle_preset(mode);
    if mode == PlayMode::Keys8 {
        state.sync_eight_k_form();
        follow_lane(state, mode, lane);
    }
    state.save_config();
    let preset = state.key_bindings.get(mode).preset.as_str();
    show_toast(
        state,
        ToastKind::Info,
        strings::fill(strings::TOAST_PRESET, &[theme::mode_label(mode), preset]),
    );
}

/// Puts the scratch lane on the other edge (F2, modes with a scratch only).
pub fn toggle_scratch_side(state: &mut AppState) {
    let mode = state.key_config_edit_mode;
    if !scratch_side_applies(mode) {
        return;
    }
    let lane = selected_lane(state);
    let side = state.view.skin.scratch_side_of(mode).toggle();
    state.view.skin.set_scratch_side(mode, side);
    follow_lane(state, mode, lane);
    state.save_config();
}

/// Switches 8K between the straight row and the trigger form (F3).
pub fn toggle_eight_k_form(state: &mut AppState) {
    let mode = state.key_config_edit_mode;
    if mode != PlayMode::Keys8 {
        return;
    }
    let lane = selected_lane(state);
    let form = state.view.skin.eight_k_form.toggle();
    state.view.skin.set_eight_k_form(form);
    // A built-in preset goes with its form; custom keys stay as they are.
    let layout = state.key_bindings.get_mut(mode);
    if layout.preset != KeyPreset::Custom {
        layout.reset_to_preset(match form {
            beetle_render::EightKForm::Inline => KeyPreset::Ue8K,
            beetle_render::EightKForm::Triggers => KeyPreset::Ue8KTriggers,
        });
    }
    follow_lane(state, mode, lane);
    state.save_config();
}

/// Resets the edited mode to its default layout (DEL, or the footer button).
pub fn reset_key_layout(state: &mut AppState) {
    let mode = state.key_config_edit_mode;
    let preset = KeyPreset::default_for(mode);
    state.key_bindings.get_mut(mode).reset_to_preset(preset);
    state.save_config();
    show_toast(
        state,
        ToastKind::Info,
        strings::fill(
            strings::TOAST_LAYOUT_RESET,
            &[theme::mode_label(mode), preset.as_str()],
        ),
    );
}

/// After the lanes were rearranged, keeps the selection on the same lane.
fn follow_lane(state: &mut AppState, mode: PlayMode, lane: Option<Lane>) {
    let Some(lane) = lane else { return };
    if let Some(i) = screen_lanes_for(&state.view.skin, mode)
        .iter()
        .position(|&l| l == lane)
    {
        state.selected_key_idx = i;
    }
}

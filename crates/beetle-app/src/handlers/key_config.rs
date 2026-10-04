use beetle_render::{Rebind, KEY_MODES};
use winit::event::ElementState;
use winit::keyboard::KeyCode;

use crate::input::{lanes_for, KeyPreset};
use crate::state::{AppScreen, AppState};

/// Handles keyboard input on the Key Configuration screen.
///
/// Left/Right pick a lane, Up/Down switch the key mode being edited (each
/// mode has its own layout). Enter waits for a key that replaces the lane's
/// keys, A waits for one more key, Backspace clears the lane.
pub fn handle_key_config_input(state: &mut AppState, key_state: ElementState, code: KeyCode) {
    if key_state != ElementState::Pressed {
        return;
    }

    let mode = state.key_config_edit_mode;
    let lanes = lanes_for(mode);
    let lane = lanes.get(state.selected_key_idx).copied();

    if let Some(rebind) = state.rebinding {
        state.rebinding = None;
        if code == KeyCode::Escape {
            return;
        }
        if let Some(lane) = lane {
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
        KeyCode::Escape => {
            state.screen = AppScreen::SongSelect;
            state.save_config();
        }
        KeyCode::Enter | KeyCode::Space => state.rebinding = Some(Rebind::Replace),
        KeyCode::KeyA => state.rebinding = Some(Rebind::Add),
        KeyCode::Backspace => {
            if let Some(lane) = lane {
                state.key_bindings.get_mut(mode).clear_lane(lane);
                state.save_config();
            }
        }
        KeyCode::ArrowLeft | KeyCode::KeyH => {
            state.selected_key_idx = state.selected_key_idx.saturating_sub(1);
        }
        KeyCode::ArrowRight | KeyCode::KeyL => {
            state.selected_key_idx = (state.selected_key_idx + 1).min(lanes.len().saturating_sub(1));
        }
        KeyCode::ArrowUp | KeyCode::ArrowDown | KeyCode::KeyK | KeyCode::KeyJ => {
            let i = KEY_MODES.iter().position(|&m| m == mode).unwrap_or(0);
            let n = KEY_MODES.len();
            let next = if matches!(code, KeyCode::ArrowUp | KeyCode::KeyK) { (i + n - 1) % n } else { (i + 1) % n };
            state.key_config_edit_mode = KEY_MODES[next];
            let count = lanes_for(KEY_MODES[next]).len();
            state.selected_key_idx = state.selected_key_idx.min(count.saturating_sub(1));
        }
        KeyCode::F1 => {
            state.key_bindings.get_mut(mode).cycle_preset(mode);
            state.save_config();
        }
        KeyCode::Delete => {
            state.key_bindings.get_mut(mode).reset_to_preset(KeyPreset::default_for(mode));
            state.save_config();
        }
        _ => (),
    }
}

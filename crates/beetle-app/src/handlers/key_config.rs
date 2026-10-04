use beetle_core::Lane;
use winit::event::ElementState;
use winit::keyboard::KeyCode;

use crate::input::KeyPreset;
use crate::state::{AppScreen, AppState};

/// Handles keyboard input on the Key Configuration screen.
pub fn handle_key_config_input(state: &mut AppState, key_state: ElementState, code: KeyCode) {
    if key_state != ElementState::Pressed {
        return;
    }

    let lanes = state.key_config_lanes();
    let max_idx = lanes.len().saturating_sub(1);

    if state.is_rebinding_key {
        if code == KeyCode::Escape {
            state.is_rebinding_key = false;
        } else {
            let target_lane = lanes
                .get(state.selected_key_idx)
                .copied()
                .unwrap_or(Lane::Key1);
            let mode = state.key_config_mode();
            state.key_bindings.get_mut(mode).bind_key(code, target_lane);
            state.is_rebinding_key = false;
            state.save_config();
        }
    } else {
        match code {
            KeyCode::Escape => {
                state.screen = AppScreen::SongSelect;
                state.save_config();
            }
            KeyCode::Enter | KeyCode::Space => {
                state.is_rebinding_key = true;
            }
            KeyCode::ArrowUp | KeyCode::ArrowLeft | KeyCode::KeyK => {
                state.selected_key_idx = state.selected_key_idx.saturating_sub(1);
            }
            KeyCode::ArrowDown | KeyCode::ArrowRight | KeyCode::KeyJ => {
                state.selected_key_idx = (state.selected_key_idx + 1).min(max_idx);
            }
            KeyCode::F1 => {
                let mode = state.key_config_mode();
                state.key_bindings.get_mut(mode).cycle_preset(mode);
                state.save_config();
            }
            KeyCode::Delete | KeyCode::Backspace => {
                let mode = state.key_config_mode();
                state.key_bindings.get_mut(mode).reset_to_preset(KeyPreset::default_for(mode));
                state.save_config();
            }
            _ => (),
        }
    }
}

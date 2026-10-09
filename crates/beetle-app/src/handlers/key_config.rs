use beetle_core::{Lane, PlayMode};
use beetle_render::{scratch_side_applies, Rebind, KEY_MODES};
use winit::event::ElementState;
use winit::keyboard::KeyCode;

use crate::input::{lanes_for, screen_lanes_for, KeyPreset};
use crate::state::{AppScreen, AppState};

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
    let lanes = screen_lanes_for(&state.view.skin, mode);
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
            state.selected_key_idx =
                (state.selected_key_idx + 1).min(lanes.len().saturating_sub(1));
        }
        KeyCode::ArrowUp | KeyCode::ArrowDown | KeyCode::KeyK | KeyCode::KeyJ => {
            let i = KEY_MODES.iter().position(|&m| m == mode).unwrap_or(0);
            let n = KEY_MODES.len();
            let next = if matches!(code, KeyCode::ArrowUp | KeyCode::KeyK) {
                (i + n - 1) % n
            } else {
                (i + 1) % n
            };
            state.key_config_edit_mode = KEY_MODES[next];
            let count = lanes_for(KEY_MODES[next]).len();
            state.selected_key_idx = state.selected_key_idx.min(count.saturating_sub(1));
        }
        KeyCode::F1 => {
            state.key_bindings.get_mut(mode).cycle_preset(mode);
            if mode == PlayMode::Keys8 {
                state.sync_eight_k_form();
                follow_lane(state, mode, lane);
            }
            state.save_config();
        }
        KeyCode::F2 if scratch_side_applies(mode) => {
            let side = state.view.skin.scratch_side_of(mode).toggle();
            state.view.skin.set_scratch_side(mode, side);
            follow_lane(state, mode, lane);
            state.save_config();
        }
        KeyCode::F3 if mode == PlayMode::Keys8 => {
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
        KeyCode::Delete => {
            state
                .key_bindings
                .get_mut(mode)
                .reset_to_preset(KeyPreset::default_for(mode));
            state.save_config();
        }
        _ => (),
    }
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

use beetle_core::{GaugeType, LaneModifier, LnOption};
use winit::keyboard::KeyCode;

use crate::state::{AppScreen, AppState};

const FPS_PRESETS: [u32; 6] = [60, 120, 144, 240, 360, 0];

/// Rows of the play options modal, in the order `present::option_modal_rows`
/// lists them (`state.modal_row` indexes this).
pub const OPTION_ROWS: usize = 16;

/// Handles keyboard input when the play options modal is open.
pub fn handle_option_modal_input(state: &mut AppState, code: KeyCode) {
    let forward = match code {
        KeyCode::Tab | KeyCode::Escape => {
            state.show_option_modal = false;
            state.save_config();
            return;
        }
        KeyCode::ArrowUp | KeyCode::KeyK => {
            state.modal_row = state.modal_row.saturating_sub(1);
            return;
        }
        KeyCode::ArrowDown | KeyCode::KeyJ => {
            state.modal_row = (state.modal_row + 1).min(OPTION_ROWS - 1);
            return;
        }
        KeyCode::ArrowLeft => false,
        KeyCode::ArrowRight | KeyCode::Enter | KeyCode::Space => true,
        _ => return,
    };
    let step = |back: f32, fwd: f32| if forward { fwd } else { back };
    match state.modal_row {
        0 => {
            // Hi-Speed
            let o = &mut state.play_options;
            o.hi_speed = (o.hi_speed + step(-25.0, 25.0)).clamp(100.0, 1200.0);
            state.view.skin.hi_speed = o.hi_speed;
        }
        1 => {
            // Lane Modifier
            const ORDER: [LaneModifier; 5] =
                [LaneModifier::Regular, LaneModifier::Mirror, LaneModifier::Random, LaneModifier::RRandom, LaneModifier::SRandom];
            state.play_options.lane_modifier = cycle(&ORDER, state.play_options.lane_modifier, forward);
        }
        2 => {
            // Gauge
            const ORDER: [GaugeType; 4] = [GaugeType::Easy, GaugeType::Groove, GaugeType::Hard, GaugeType::Hazard];
            state.play_options.gauge_type = cycle(&ORDER, state.play_options.gauge_type, forward);
        }
        3 => {
            // LN Mode
            const ORDER: [LnOption; 3] = [LnOption::Auto, LnOption::Ln, LnOption::Cn];
            state.play_options.ln = cycle(&ORDER, state.play_options.ln, forward);
            state.resort_songs();
        }
        4 => {
            // Judge Offset
            let o = &mut state.play_options;
            o.judge_offset_ms = (o.judge_offset_ms + step(-1.0, 1.0) as f64).clamp(-100.0, 100.0);
        }
        5 => {
            // Master Volume
            state.master_volume = (state.master_volume + step(-0.05, 0.05)).clamp(0.0, 2.0);
            if let Some(audio) = &mut state.audio_engine {
                let _ = audio.set_master_volume(state.master_volume);
            }
        }
        6 => {
            // Playfield position
            let skin = &mut state.view.skin;
            let position = if forward { skin.field_position.next() } else { skin.field_position.prev() };
            skin.set_field_layout(position, skin.scratch_side_of(skin.play_mode));
        }
        7 => {
            // BGA on/off
            state.bga_enabled = !state.bga_enabled;
        }
        8 => {
            // Track BGA
            state.track_bga = if forward { state.track_bga.next() } else { state.track_bga.prev() };
        }
        9 => {
            // Display Mode
            state.display_mode = if forward { state.display_mode.next() } else { state.display_mode.prev() };
            state.apply_display_mode();
        }
        10 => {
            // Resolution
            state.cycle_resolution(forward);
        }
        11 => {
            // Graphics GPU
            // Takes effect on the next start (see AppState::d3d11).
            state.gpu_backend = if forward { state.gpu_backend.next() } else { state.gpu_backend.prev() };
        }
        12 => {
            // Target FPS
            state.target_fps = cycle(&FPS_PRESETS, state.target_fps, forward);
        }
        13 => {
            // Key Layout (of the selected song's key mode); Enter edits it.
            if code == KeyCode::Enter || code == KeyCode::Space {
                state.screen = AppScreen::KeyConfig;
                state.key_config_edit_mode = state.key_config_mode();
                state.selected_key_idx = 0;
                state.show_option_modal = false;
            } else {
                let mode = state.key_config_mode();
                state.key_bindings.get_mut(mode).cycle_preset(mode);
                state.sync_eight_k_form();
            }
        }
        14 => {
            // Auto Play
            state.is_auto_play = !state.is_auto_play;
        }
        15 => {
            // Start Measure
            state.start_measure = if forward { (state.start_measure + 1).min(200) } else { state.start_measure.saturating_sub(1) };
        }
        _ => (),
    }
    state.save_config();
}

/// The value after (or before) `cur` in `order`, wrapping around. A value
/// not in the list (a hand-edited FPS) counts as the 4th entry (240 FPS).
fn cycle<T: Copy + PartialEq>(order: &[T], cur: T, forward: bool) -> T {
    let n = order.len();
    let i = order.iter().position(|&v| v == cur).unwrap_or(3.min(n - 1));
    order[if forward { (i + 1) % n } else { (i + n - 1) % n }]
}

#[cfg(test)]
mod tests {
    use super::cycle;

    #[test]
    fn cycle_wraps_both_ways() {
        assert_eq!(cycle(&[1, 2, 3], 3, true), 1);
        assert_eq!(cycle(&[1, 2, 3], 1, false), 3);
        assert_eq!(cycle(&[60, 120, 144, 240, 360, 0], 75, true), 360);
    }
}

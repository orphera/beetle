use beetle_core::{GaugeType, LaneModifier};
use winit::keyboard::KeyCode;

use crate::state::{AppScreen, AppState};

const FPS_PRESETS: [u32; 6] = [60, 120, 144, 240, 360, 0];

/// Handles keyboard input when the play options modal is open.
pub fn handle_option_modal_input(state: &mut AppState, code: KeyCode) {
    match code {
        KeyCode::Tab | KeyCode::Escape => {
            state.show_option_modal = false;
            state.save_config();
        }
        KeyCode::ArrowUp | KeyCode::KeyK => {
            state.modal_row = state.modal_row.saturating_sub(1);
        }
        KeyCode::ArrowDown | KeyCode::KeyJ => {
            state.modal_row = (state.modal_row + 1).min(11);
        }
        KeyCode::ArrowLeft => {
            match state.modal_row {
                0 => { // Hi-Speed
                    state.play_options.hi_speed = (state.play_options.hi_speed - 25.0).max(100.0);
                    state.renderer.skin.hi_speed = state.play_options.hi_speed;
                }
                1 => { // Lane Modifier
                    state.play_options.lane_modifier = match state.play_options.lane_modifier {
                        LaneModifier::Regular => LaneModifier::SRandom,
                        LaneModifier::Mirror => LaneModifier::Regular,
                        LaneModifier::Random => LaneModifier::Mirror,
                        LaneModifier::RRandom => LaneModifier::Random,
                        LaneModifier::SRandom => LaneModifier::RRandom,
                    };
                }
                2 => { // Gauge
                    state.play_options.gauge_type = match state.play_options.gauge_type {
                        GaugeType::Easy => GaugeType::Hazard,
                        GaugeType::Groove => GaugeType::Easy,
                        GaugeType::Hard => GaugeType::Groove,
                        GaugeType::Hazard => GaugeType::Hard,
                    };
                }
                3 => { // Judge Offset
                    state.play_options.judge_offset_ms = (state.play_options.judge_offset_ms - 1.0).max(-100.0);
                }
                4 => { // Master Volume
                    state.master_volume = (state.master_volume - 0.05).max(0.0);
                    if let Some(audio) = &mut state.audio_engine {
                        let _ = audio.set_master_volume(state.master_volume);
                    }
                }
                5 => { // Display Mode
                    state.display_mode = state.display_mode.prev();
                    state.apply_display_mode();
                }
                6 => { // Resolution
                    state.cycle_resolution(false);
                }
                7 => { // Graphics GPU
                    state.gpu_backend = state.gpu_backend.prev();
                }
                8 => { // Target FPS
                    let cur_idx = FPS_PRESETS.iter().position(|&f| f == state.target_fps).unwrap_or(3);
                    let prev_idx = if cur_idx == 0 { FPS_PRESETS.len() - 1 } else { cur_idx - 1 };
                    state.target_fps = FPS_PRESETS[prev_idx];
                }
                9 => { // Key Layout
                    state.input_config.toggle_preset();
                }
                10 => { // Auto Play
                    state.is_auto_play = !state.is_auto_play;
                }
                11 => { // Start Measure
                    state.start_measure = state.start_measure.saturating_sub(1);
                }
                _ => (),
            }
            state.save_config();
        }
        KeyCode::ArrowRight | KeyCode::Enter | KeyCode::Space => {
            match state.modal_row {
                0 => { // Hi-Speed
                    state.play_options.hi_speed = (state.play_options.hi_speed + 25.0).min(1200.0);
                    state.renderer.skin.hi_speed = state.play_options.hi_speed;
                }
                1 => { // Lane Modifier
                    state.play_options.lane_modifier = match state.play_options.lane_modifier {
                        LaneModifier::Regular => LaneModifier::Mirror,
                        LaneModifier::Mirror => LaneModifier::Random,
                        LaneModifier::Random => LaneModifier::RRandom,
                        LaneModifier::RRandom => LaneModifier::SRandom,
                        LaneModifier::SRandom => LaneModifier::Regular,
                    };
                }
                2 => { // Gauge
                    state.play_options.gauge_type = match state.play_options.gauge_type {
                        GaugeType::Easy => GaugeType::Groove,
                        GaugeType::Groove => GaugeType::Hard,
                        GaugeType::Hard => GaugeType::Hazard,
                        GaugeType::Hazard => GaugeType::Easy,
                    };
                }
                3 => { // Judge Offset
                    state.play_options.judge_offset_ms = (state.play_options.judge_offset_ms + 1.0).min(100.0);
                }
                4 => { // Master Volume
                    state.master_volume = (state.master_volume + 0.05).min(2.0);
                    if let Some(audio) = &mut state.audio_engine {
                        let _ = audio.set_master_volume(state.master_volume);
                    }
                }
                5 => { // Display Mode
                    state.display_mode = state.display_mode.next();
                    state.apply_display_mode();
                }
                6 => { // Resolution
                    state.cycle_resolution(true);
                }
                7 => { // Graphics GPU
                    state.gpu_backend = state.gpu_backend.next();
                }
                8 => { // Target FPS
                    let cur_idx = FPS_PRESETS.iter().position(|&f| f == state.target_fps).unwrap_or(3);
                    let next_idx = (cur_idx + 1) % FPS_PRESETS.len();
                    state.target_fps = FPS_PRESETS[next_idx];
                }
                9 => { // Key Layout
                    if code == KeyCode::Enter || code == KeyCode::Space {
                        state.screen = AppScreen::KeyConfig;
                        state.show_option_modal = false;
                    } else {
                        state.input_config.toggle_preset();
                    }
                }
                10 => { // Auto Play
                    state.is_auto_play = !state.is_auto_play;
                }
                11 => { // Start Measure
                    state.start_measure = (state.start_measure + 1).min(200);
                }
                _ => (),
            }
            state.save_config();
        }
        _ => (),
    }
}

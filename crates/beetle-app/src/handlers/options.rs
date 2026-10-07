use beetle_core::{GaugeType, LaneModifier, LnOption};
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
            state.modal_row = (state.modal_row + 1).min(13);
        }
        KeyCode::ArrowLeft => {
            match state.modal_row {
                0 => {
                    // Hi-Speed
                    state.play_options.hi_speed = (state.play_options.hi_speed - 25.0).max(100.0);
                    state.view.skin.hi_speed = state.play_options.hi_speed;
                }
                1 => {
                    // Lane Modifier
                    state.play_options.lane_modifier = match state.play_options.lane_modifier {
                        LaneModifier::Regular => LaneModifier::SRandom,
                        LaneModifier::Mirror => LaneModifier::Regular,
                        LaneModifier::Random => LaneModifier::Mirror,
                        LaneModifier::RRandom => LaneModifier::Random,
                        LaneModifier::SRandom => LaneModifier::RRandom,
                    };
                }
                2 => {
                    // Gauge
                    state.play_options.gauge_type = match state.play_options.gauge_type {
                        GaugeType::Easy => GaugeType::Hazard,
                        GaugeType::Groove => GaugeType::Easy,
                        GaugeType::Hard => GaugeType::Groove,
                        GaugeType::Hazard => GaugeType::Hard,
                    };
                }
                3 => {
                    // LN Mode
                    state.play_options.ln = match state.play_options.ln {
                        LnOption::Auto => LnOption::Cn,
                        LnOption::Ln => LnOption::Auto,
                        LnOption::Cn => LnOption::Ln,
                    };
                    state.resort_songs();
                }
                4 => {
                    // Judge Offset
                    state.play_options.judge_offset_ms =
                        (state.play_options.judge_offset_ms - 1.0).max(-100.0);
                }
                5 => {
                    // Master Volume
                    state.master_volume = (state.master_volume - 0.05).max(0.0);
                    if let Some(audio) = &mut state.audio_engine {
                        let _ = audio.set_master_volume(state.master_volume);
                    }
                }
                6 => {
                    // Display Mode
                    state.display_mode = state.display_mode.prev();
                    state.apply_display_mode();
                }
                7 => {
                    // Resolution
                    state.cycle_resolution(false);
                }
                8 => {
                    // Graphics GPU
                    // Takes effect on the next start (see AppState::d3d11).
                    state.gpu_backend = state.gpu_backend.prev();
                }
                9 => {
                    // Target FPS
                    let cur_idx = FPS_PRESETS
                        .iter()
                        .position(|&f| f == state.target_fps)
                        .unwrap_or(3);
                    let prev_idx = if cur_idx == 0 {
                        FPS_PRESETS.len() - 1
                    } else {
                        cur_idx - 1
                    };
                    state.target_fps = FPS_PRESETS[prev_idx];
                }
                10 => {
                    // Key Layout (of the selected song's key mode)
                    let mode = state.key_config_mode();
                    state.key_bindings.get_mut(mode).cycle_preset(mode);
                }
                11 => {
                    // Auto Play
                    state.is_auto_play = !state.is_auto_play;
                }
                12 => {
                    // Start Measure
                    state.start_measure = state.start_measure.saturating_sub(1);
                }
                13 => {
                    // Track BGA
                    state.track_bga = state.track_bga.prev();
                }
                _ => (),
            }
            state.save_config();
        }
        KeyCode::ArrowRight | KeyCode::Enter | KeyCode::Space => {
            match state.modal_row {
                0 => {
                    // Hi-Speed
                    state.play_options.hi_speed = (state.play_options.hi_speed + 25.0).min(1200.0);
                    state.view.skin.hi_speed = state.play_options.hi_speed;
                }
                1 => {
                    // Lane Modifier
                    state.play_options.lane_modifier = match state.play_options.lane_modifier {
                        LaneModifier::Regular => LaneModifier::Mirror,
                        LaneModifier::Mirror => LaneModifier::Random,
                        LaneModifier::Random => LaneModifier::RRandom,
                        LaneModifier::RRandom => LaneModifier::SRandom,
                        LaneModifier::SRandom => LaneModifier::Regular,
                    };
                }
                2 => {
                    // Gauge
                    state.play_options.gauge_type = match state.play_options.gauge_type {
                        GaugeType::Easy => GaugeType::Groove,
                        GaugeType::Groove => GaugeType::Hard,
                        GaugeType::Hard => GaugeType::Hazard,
                        GaugeType::Hazard => GaugeType::Easy,
                    };
                }
                3 => {
                    // LN Mode
                    state.play_options.ln = match state.play_options.ln {
                        LnOption::Auto => LnOption::Ln,
                        LnOption::Ln => LnOption::Cn,
                        LnOption::Cn => LnOption::Auto,
                    };
                    state.resort_songs();
                }
                4 => {
                    // Judge Offset
                    state.play_options.judge_offset_ms =
                        (state.play_options.judge_offset_ms + 1.0).min(100.0);
                }
                5 => {
                    // Master Volume
                    state.master_volume = (state.master_volume + 0.05).min(2.0);
                    if let Some(audio) = &mut state.audio_engine {
                        let _ = audio.set_master_volume(state.master_volume);
                    }
                }
                6 => {
                    // Display Mode
                    state.display_mode = state.display_mode.next();
                    state.apply_display_mode();
                }
                7 => {
                    // Resolution
                    state.cycle_resolution(true);
                }
                8 => {
                    // Graphics GPU
                    // Takes effect on the next start (see AppState::d3d11).
                    state.gpu_backend = state.gpu_backend.next();
                }
                9 => {
                    // Target FPS
                    let cur_idx = FPS_PRESETS
                        .iter()
                        .position(|&f| f == state.target_fps)
                        .unwrap_or(3);
                    let next_idx = (cur_idx + 1) % FPS_PRESETS.len();
                    state.target_fps = FPS_PRESETS[next_idx];
                }
                10 => {
                    // Key Layout
                    if code == KeyCode::Enter || code == KeyCode::Space {
                        state.screen = AppScreen::KeyConfig;
                        state.key_config_edit_mode = state.key_config_mode();
                        state.selected_key_idx = 0;
                        state.show_option_modal = false;
                    } else {
                        let mode = state.key_config_mode();
                        state.key_bindings.get_mut(mode).cycle_preset(mode);
                    }
                }
                11 => {
                    // Auto Play
                    state.is_auto_play = !state.is_auto_play;
                }
                12 => {
                    // Start Measure
                    state.start_measure = (state.start_measure + 1).min(200);
                }
                13 => {
                    // Track BGA
                    state.track_bga = state.track_bga.next();
                }
                _ => (),
            }
            state.save_config();
        }
        _ => (),
    }
}

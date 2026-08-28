use beetle_core::{GaugeType, LaneModifier};
use winit::keyboard::KeyCode;

use crate::state::{AppScreen, AppState};

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
            state.modal_row = (state.modal_row + 1).min(7);
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
                    if let Some(preview) = &mut state.preview_audio {
                        let _ = preview.set_master_volume(state.master_volume);
                    }
                }
                5 => { // Key Layout
                    state.input_config.toggle_preset();
                }
                6 => { // Auto Play
                    state.is_auto_play = !state.is_auto_play;
                }
                7 => { // Start Measure
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
                    if let Some(preview) = &mut state.preview_audio {
                        let _ = preview.set_master_volume(state.master_volume);
                    }
                }
                5 => { // Key Layout
                    if code == KeyCode::Enter || code == KeyCode::Space {
                        state.screen = AppScreen::KeyConfig;
                        state.show_option_modal = false;
                    } else {
                        state.input_config.toggle_preset();
                    }
                }
                6 => { // Auto Play
                    state.is_auto_play = !state.is_auto_play;
                }
                7 => { // Start Measure
                    state.start_measure = (state.start_measure + 1).min(200);
                }
                _ => (),
            }
            state.save_config();
        }
        _ => (),
    }
}

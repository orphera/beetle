use beetle_audio::AudioCommand;
use winit::event::ElementState;
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::gameplay::queue_start_gameplay;
use crate::state::{AppScreen, AppState};

/// Handles keyboard input during gameplay, pause modal, and live hotkeys.
pub fn handle_gameplay_input(
    state: &mut AppState,
    key_state: ElementState,
    code: KeyCode,
    physical_key: PhysicalKey,
) {
    if key_state == ElementState::Pressed {
        // If paused, handle pause modal interactions
        if state.is_gameplay_paused {
            handle_pause_modal_input(state, code);
            return;
        }

        // Normal gameplay hotkeys (when unpaused)
        match code {
            KeyCode::Escape => {
                if state.is_auto_play || state.is_replay_playback {
                    state.screen = AppScreen::SongSelect;
                    state.audio_engine = None;
                    state.video_players.clear();
                    state.video_start_times.clear();
                } else {
                    state.is_gameplay_paused = true;
                    state.pause_selected_option = 0;
                    if let Some(audio) = &mut state.audio_engine {
                        let _ = audio.pause();
                    }
                }
                return;
            }
            KeyCode::F3 | KeyCode::PageUp | KeyCode::Digit1 => {
                state.play_options.hi_speed = (state.play_options.hi_speed + 25.0).min(1200.0);
                state.renderer.skin.hi_speed = state.play_options.hi_speed;
                state.save_config();
                return;
            }
            KeyCode::F4 | KeyCode::PageDown | KeyCode::Digit2 => {
                state.play_options.hi_speed = (state.play_options.hi_speed - 25.0).max(100.0);
                state.renderer.skin.hi_speed = state.play_options.hi_speed;
                state.save_config();
                return;
            }
            KeyCode::F10 => {
                state.renderer.skin.lane_cover_ratio =
                    (state.renderer.skin.lane_cover_ratio + 0.05).min(0.80);
                state.save_config();
                return;
            }
            KeyCode::F11 => {
                state.renderer.skin.lane_cover_ratio =
                    (state.renderer.skin.lane_cover_ratio - 0.05).max(0.0);
                state.save_config();
                return;
            }
            _ => (),
        }
    }

    // Block lane keys if paused or during replay/auto-play
    if state.is_gameplay_paused || state.is_auto_play || state.is_replay_playback {
        return;
    }

    // Handle lane key presses and releases
    if let Some(lane) = state.input_config.map_key(physical_key) {
        let audio_time = state
            .audio_engine
            .as_ref()
            .map(|a| a.clock().current_time_seconds())
            .unwrap_or(0.0);

        let effective_judge_time = audio_time + (state.play_options.judge_offset_ms / 1000.0);

        match key_state {
            ElementState::Pressed => {
                if let Some(rep) = &mut state.current_replay {
                    rep.record(audio_time, lane, true);
                }

                state.renderer.set_key_state(lane, true);
                if let Some(judge) = &mut state.active_judge {
                    if let Some((judge_result, wav_id)) =
                            judge.handle_key_down(lane, effective_judge_time)
                        {
                        if judge_result.grade == beetle_core::JudgeGrade::Miss
                            || judge_result.grade == beetle_core::JudgeGrade::Poor
                        {
                            state.poor_until_time = audio_time + 0.4;
                        }
                        state.renderer.trigger_judge_with_lane(
                            lane,
                            judge_result.grade,
                            audio_time,
                            judge_result.delta_ms,
                        );

                        if let (Some(id), Some(audio)) = (wav_id, &mut state.audio_engine) {
                            let _ = audio.send_command(AudioCommand::PlaySample {
                                sample_id: id,
                                volume: 1.0,
                                pan: 0.0,
                            });
                        }
                    } else {
                        // No visible note was judged on this lane. Implement transparent-note
                        // fallback: if there are no visible unjudged notes near the judgment
                        // line, play the nearest BGM/freezone sample (3x/4x) if available.
                        let mut visible_near = false;
                        if let (Some(chart), Some(timing)) = (&state.active_chart, &state.active_timing) {
                            // Use judge window poor threshold based on chart rank
                            let window = beetle_core::JudgeWindow::from_rank(chart.header.rank);
                            let poor_ms = window.poor_ms;

                            if let Some(j) = &state.active_judge {
                                for pn in j.notes() {
                                    if pn.is_judged {
                                        continue;
                                    }
                                    let delta_ms = (effective_judge_time - pn.target_time_seconds).abs() * 1000.0;
                                    if delta_ms <= poor_ms {
                                        visible_near = true;
                                        break;
                                    }
                                }
                            }

                            if !visible_near {
                                // Find nearest bgm/freezone sample by absolute time distance
                                let mut best: Option<(f64, beetle_core::WavId)> = None;
                                for (m, f, wav_id) in &chart.bgm_notes {
                                    let t = timing.beat_to_time_seconds(*m, *f);
                                    let dms = (effective_judge_time - t).abs() * 1000.0;
                                    if best.is_none() || dms < best.unwrap().0 {
                                        best = Some((dms, *wav_id));
                                    }
                                }

                                if let (Some((_, wav_id)), Some(audio)) = (best.map(|b| b.1), &mut state.audio_engine) {
                                    let _ = audio.send_command(AudioCommand::PlaySample {
                                        sample_id: wav_id,
                                        volume: 1.0,
                                        pan: 0.0,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            ElementState::Released => {
                if let Some(rep) = &mut state.current_replay {
                    rep.record(audio_time, lane, false);
                }

                state.renderer.set_key_state(lane, false);
                if let Some(judge) = &mut state.active_judge {
                    if let Some(judge_result) = judge.handle_key_up(lane, effective_judge_time) {
                        state.renderer.trigger_judge_with_lane(
                            lane,
                            judge_result.grade,
                            audio_time,
                            judge_result.delta_ms,
                        );
                    }
                }
            }
        }
    }
}

/// Handles keyboard input inside the pause modal overlay.
pub fn handle_pause_modal_input(state: &mut AppState, code: KeyCode) {
    match code {
        KeyCode::Escape => {
            // Resume playback
            state.is_gameplay_paused = false;
            if let Some(audio) = &mut state.audio_engine {
                let _ = audio.resume();
            }
        }
        KeyCode::KeyR => {
            // Instant Restart
            state.is_gameplay_paused = false;
            if let Some(song) = state.current_selected_song().cloned() {
                queue_start_gameplay(state, &song);
            }
        }
        KeyCode::ArrowUp | KeyCode::KeyK => {
            state.pause_selected_option = state.pause_selected_option.saturating_sub(1);
        }
        KeyCode::ArrowDown | KeyCode::KeyJ => {
            state.pause_selected_option = (state.pause_selected_option + 1).min(2);
        }
        KeyCode::Enter | KeyCode::Space => {
            match state.pause_selected_option {
                0 => {
                    // Resume
                    state.is_gameplay_paused = false;
                    if let Some(audio) = &mut state.audio_engine {
                        let _ = audio.resume();
                    }
                }
                1 => {
                    // Restart
                    state.is_gameplay_paused = false;
                    if let Some(song) = state.current_selected_song().cloned() {
                        queue_start_gameplay(state, &song);
                    }
                }
                2 => {
                    // Quit to Song Select
                    state.is_gameplay_paused = false;
                    state.audio_engine = None;
                    state.video_players.clear();
                    state.video_start_times.clear();
                    state.screen = AppScreen::SongSelect;
                }
                _ => (),
            }
        }
        _ => (),
    }
}

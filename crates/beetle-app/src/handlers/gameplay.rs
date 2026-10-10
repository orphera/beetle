use beetle_audio::AudioCommand;
use beetle_core::{BmsChart, JudgeGrade, JudgeWindow, Lane, TimingModel, WavId};
use beetle_render::strings;
use winit::event::ElementState;
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::calibration::judged_time;
use crate::gameplay::{audio_time_now, leave_gameplay, queue_start_gameplay};
use crate::lane_logic::transparent_sound;
use crate::options_table::green_ms_next;
use crate::state::{AppScreen, AppState};

/// Shows the green number just changed, over the lane for a second.
fn show_green_readout(state: &mut AppState) {
    let at = audio_time_now(state);
    let ms = state.play_options.green_ms.to_string();
    state.gameplay_readout = Some((strings::fill(strings::READOUT_GREEN, &[&ms]), at));
}

/// Shows the lane cover just changed, over the lane for a second.
fn show_cover_readout(state: &mut AppState) {
    let at = audio_time_now(state);
    let percent = (state.view.skin.lane_cover_ratio * 100.0).round() as u32;
    let text = strings::fill(strings::READOUT_COVER, &[&percent.to_string()]);
    state.gameplay_readout = Some((text, at));
}

/// Handles keyboard input during gameplay, pause modal, and live hotkeys.
pub fn handle_gameplay_input(
    state: &mut AppState,
    key_state: ElementState,
    code: KeyCode,
    physical_key: PhysicalKey,
) {
    // The end banner: ENTER or ESC goes straight to the result screen, and
    // nothing else reaches the play.
    if state.gameplay_end.is_some() {
        if key_state == ElementState::Pressed && matches!(code, KeyCode::Enter | KeyCode::Escape) {
            leave_gameplay(state);
        }
        return;
    }
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
            // F3 / 1: faster (a smaller green number), F4 / 2: slower.
            KeyCode::F3 | KeyCode::PageUp | KeyCode::Digit1 => {
                state.play_options.green_ms = green_ms_next(state.play_options.green_ms, false);
                state.sync_hi_speed();
                state.save_config();
                show_green_readout(state);
                return;
            }
            KeyCode::F4 | KeyCode::PageDown | KeyCode::Digit2 => {
                state.play_options.green_ms = green_ms_next(state.play_options.green_ms, true);
                state.sync_hi_speed();
                state.save_config();
                show_green_readout(state);
                return;
            }
            KeyCode::F10 => {
                state.view.skin.lane_cover_ratio =
                    (state.view.skin.lane_cover_ratio + 0.05).min(0.80);
                state.sync_hi_speed();
                state.save_config();
                show_cover_readout(state);
                return;
            }
            KeyCode::F11 => {
                state.view.skin.lane_cover_ratio =
                    (state.view.skin.lane_cover_ratio - 0.05).max(0.0);
                state.sync_hi_speed();
                state.save_config();
                show_cover_readout(state);
                return;
            }
            _ => (),
        }
    }

    // Lane keys come from the raw input thread when it runs, stamped when
    // they arrived (`main::drain_raw_keys`); winit's are judged only without it.
    if state.raw_keys.is_none() {
        if let PhysicalKey::Code(code) = physical_key {
            let audio_time = audio_time_now(state);
            handle_lane_key(state, code, key_state == ElementState::Pressed, audio_time);
        }
    }
}

/// Keys that act on the play (pause, green number, lane cover, the global
/// F6–F9 options) rather than hit a lane, even when bound to one.
pub fn is_gameplay_hotkey(code: KeyCode) -> bool {
    matches!(
        code,
        KeyCode::Escape
            | KeyCode::F3
            | KeyCode::F4
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Digit1
            | KeyCode::Digit2
            | KeyCode::F6
            | KeyCode::F7
            | KeyCode::F8
            | KeyCode::F9
            | KeyCode::F10
            | KeyCode::F11
    )
}

/// A lane key going down or up at `audio_time` on the audio clock (the
/// moment it arrived, not when it is handled).
pub fn handle_lane_key(state: &mut AppState, code: KeyCode, pressed: bool, audio_time: f64) {
    if state.gameplay_end.is_some() {
        return;
    }
    // Block lane keys if paused or during replay/auto-play (but still notice
    // releases, so a key let go while paused is not considered held later).
    if state.is_gameplay_paused || state.is_auto_play || state.is_replay_playback {
        if !pressed {
            state.held_keys.retain(|&(k, _)| k != code);
        }
        return;
    }

    // The layout of the chart's key mode (set when the song starts).
    let mode = state.view.skin.play_mode;
    let Some(lane) = state
        .key_bindings
        .get(mode)
        .map_key(PhysicalKey::Code(code))
    else {
        return;
    };
    if crate::input::lane_transition(&mut state.held_keys, code, lane, pressed).is_none() {
        return;
    }
    let judge_time = judged_time(audio_time, state.play_options.judge_offset_ms);
    let sound = judge_lane(state, lane, pressed, audio_time, judge_time);
    // A press that judged no note may still sound a hidden sample.
    let sound = match (pressed, sound) {
        (true, None) => match (
            &state.active_judge,
            &state.active_chart,
            &state.active_timing,
        ) {
            (Some(judge), Some(chart), Some(timing)) => transparent_sound(
                judge,
                &freezone_sounds(chart, timing),
                JudgeWindow::from_rank(chart.header.rank).poor_ms,
                judge_time,
            ),
            _ => None,
        },
        (_, sound) => sound,
    };
    if let (Some(id), Some(audio)) = (sound, &mut state.audio_engine) {
        let _ = audio.send_command(AudioCommand::PlaySample {
            sample_id: id,
            volume: 1.0,
            pan: 0.0,
        });
    }
}

/// Judges a lane going down or up at `judge_time` (the key arrived at
/// `audio_time`) and shows it: the replay, the key beam, the judgment. The
/// note's sound when a press hit one, for the caller to start. The input
/// thread's calls come here too (`main::drain_lane_events`), already sounded.
pub fn judge_lane(
    state: &mut AppState,
    lane: Lane,
    pressed: bool,
    audio_time: f64,
    judge_time: f64,
) -> Option<WavId> {
    if let Some(rep) = &mut state.current_replay {
        rep.record(audio_time, lane, pressed);
    }
    state.view.set_key_state(lane, pressed);
    let judge = state.active_judge.as_mut()?;
    let (result, sound) = if pressed {
        let (result, sound) = judge.handle_key_down(lane, judge_time)?;
        if matches!(result.grade, JudgeGrade::Miss | JudgeGrade::Poor) {
            state.poor_until_time = audio_time + 0.4;
        }
        (result, sound)
    } else {
        (judge.handle_key_up(lane, judge_time)?, None)
    };
    state
        .view
        .trigger_judge_with_lane(lane, result.grade, audio_time, result.delta_ms);
    sound
}

/// The chart's hidden (freezone) samples by time.
pub fn freezone_sounds(chart: &BmsChart, timing: &TimingModel) -> Vec<(f64, WavId)> {
    chart
        .freezone_notes
        .iter()
        .map(|&(m, f, id)| (timing.beat_to_time_seconds(m, f), id))
        .collect()
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

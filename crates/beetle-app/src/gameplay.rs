use std::fs;
use std::path::Path;
use std::time::Instant;

use beetle_audio::{AudioCommand, AudioEngine, SampleBank};
use beetle_core::{
    apply_lane_modifier, BmsChart, ClearType, GaugeTrend, JudgeEngine, JudgeGrade, PlayResult,
    ReplayData, ScoreUpdate, SongMetadata, TimingModel,
};

use crate::calibration::judged_time;
use crate::loader::{load_stage_image, spawn_background_song_loader};
use crate::state::{replay_path, save_scores, AppScreen, AppState, REPLAYS_DIR};

fn fresh_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(42)
}

pub fn queue_start_gameplay(state: &mut AppState, song: &SongMetadata) {
    state.screen = AppScreen::Loading;
    state.loading_song = Some(song.clone());
    state.loading_spinner_frame = 0;
    state.loading_anim_time = Instant::now();
    state.loading_started_at = Instant::now();

    // Cache stage image for loading screen
    state
        .stage_image_cache
        .entry(song.id)
        .or_insert_with(|| load_stage_image(song));

    // A replay re-rolls `#RANDOM` with the seed it was recorded with; a fresh
    // play rolls anew.
    let seed = state
        .playback_replay
        .as_ref()
        .filter(|_| state.is_replay_playback)
        .and_then(|r| r.random_seed)
        .unwrap_or_else(fresh_seed);
    state.loading_receiver = Some(spawn_background_song_loader(song, seed, state.bga_enabled));
    state.window.request_redraw();
}

pub fn finalize_start_gameplay(
    state: &mut AppState,
    song: &SongMetadata,
    chart: BmsChart,
    timing: TimingModel,
    soundbank: SampleBank,
    bga_bank: std::collections::HashMap<beetle_core::BmpId, beetle_render::ImageBuffer>,
    mut video_sources: std::collections::HashMap<beetle_core::BmpId, crate::loader::VideoSource>,
) {
    // Apply Lane Modifier (Mirror, Random, R-Random, S-Random)
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(42);

    let mut play_chart = chart.clone();
    if !state.is_replay_playback {
        play_chart.notes =
            apply_lane_modifier(&chart.notes, state.play_options.lane_modifier, seed);
    }

    let play_chart_has_long_notes = play_chart
        .notes
        .iter()
        .any(|n| n.note_type == beetle_core::NoteType::LongNoteStart);
    let ruleset = state.play_ruleset(song);
    state.active_hcn = play_chart_has_long_notes && ruleset.hcn_requested;
    let mut judge_engine =
        JudgeEngine::new(&play_chart, &timing, state.play_options.gauge_type, ruleset);
    let total_duration = timing.total_duration_seconds(&play_chart);

    let mut video_players = std::collections::HashMap::new();

    // Filter to only video IDs actually referenced by the chart's BGA timeline events
    let referenced_video_ids: Vec<beetle_core::BmpId> = {
        let mut ids = Vec::new();
        for ev in &play_chart.bga_events {
            if video_sources.contains_key(&ev.bmp_id) && !ids.contains(&ev.bmp_id) {
                ids.push(ev.bmp_id);
            }
        }
        // If no BGA events explicitly referenced a video, pick the first video as background fallback
        if ids.is_empty() {
            if let Some(&first_id) = video_sources.keys().next() {
                ids.push(first_id);
            }
        }
        ids
    };

    let mut consecutive_failures = 0;
    const MAX_ACTIVE_VIDEO_PLAYERS: usize = 2;

    for bmp_id in referenced_video_ids {
        if video_players.len() >= MAX_ACTIVE_VIDEO_PLAYERS {
            break;
        }
        if consecutive_failures >= 2 {
            // Fail-fast on consecutive unsupported video formats to prevent UI freeze
            break;
        }

        if let Some(source) = video_sources.remove(&bmp_id) {
            let player = match source {
                crate::loader::VideoSource::File(p) => beetle_render::BgaVideoPlayer::open(&p),
                crate::loader::VideoSource::Memory {
                    bytes,
                    filename_hint,
                } => beetle_render::BgaVideoPlayer::open_from_memory(
                    &bytes,
                    filename_hint.as_deref(),
                ),
            };
            if let Some(player) = player {
                video_players.insert(bmp_id, player);
                consecutive_failures = 0;
            } else {
                consecutive_failures += 1;
            }
        }
    }
    let mut video_start_times = std::collections::HashMap::new();

    let mut bgm_cursor = 0;
    let mut bga_cursor = 0;
    let mut initial_base_bmp = None;
    let mut initial_layer_bmp = None;
    let mut initial_poor_bmp = None;

    // Practice mode fast forward
    if state.start_measure > 0 && !state.is_replay_playback {
        let start_time = timing.beat_to_time_seconds(state.start_measure, 0.0);
        judge_engine.advance_to_time(start_time);

        while bgm_cursor < play_chart.bgm_notes.len() {
            let (m, f, _) = play_chart.bgm_notes[bgm_cursor];
            if timing.beat_to_time_seconds(m, f) < start_time {
                bgm_cursor += 1;
            } else {
                break;
            }
        }

        while bga_cursor < play_chart.bga_events.len() {
            let ev = &play_chart.bga_events[bga_cursor];
            let ev_t = timing.beat_to_time_seconds(ev.measure, ev.fraction);
            if ev_t < start_time {
                match ev.channel {
                    beetle_core::BgaChannel::Base => {
                        initial_base_bmp = Some(ev.bmp_id);
                        if video_players.contains_key(&ev.bmp_id) {
                            video_start_times.insert(ev.bmp_id, ev_t);
                        }
                    }
                    beetle_core::BgaChannel::Poor => {
                        initial_poor_bmp = Some(ev.bmp_id);
                    }
                    beetle_core::BgaChannel::Layer => {
                        initial_layer_bmp = Some(ev.bmp_id);
                        if video_players.contains_key(&ev.bmp_id) {
                            video_start_times.insert(ev.bmp_id, ev_t);
                        }
                    }
                }
                bga_cursor += 1;
            } else {
                break;
            }
        }
    }

    // If the chart does not define any Base BGA events but a background video exists,
    // start playing the video from the beginning (0.0s).
    if initial_base_bmp.is_none()
        && !play_chart
            .bga_events
            .iter()
            .any(|ev| ev.channel == beetle_core::BgaChannel::Base)
    {
        if let Some((&first_id, _)) = video_players.iter().next() {
            initial_base_bmp = Some(first_id);
            video_start_times.entry(first_id).or_insert(0.0);
        }
    }

    let is_pms = song.file_path.to_lowercase().ends_with(".pms");
    let play_mode = play_chart.detect_play_mode_with_hint(is_pms);
    state.view.skin.set_play_mode(play_mode);
    state.sync_hi_speed();

    let mut audio_engine = AudioEngine::new(soundbank).ok();
    if let Some(audio) = &mut audio_engine {
        let _ = audio.set_master_volume(state.master_volume);
    }

    state.active_chart = Some(play_chart);
    state.active_timing = Some(timing);
    state.active_chart_id = song.id;
    state.active_ln = play_chart_has_long_notes.then_some(ruleset.ln);
    state.active_judge = Some(judge_engine);
    state.bga_bank = bga_bank;
    state.bga_cursor = bga_cursor;
    state.current_bga_bmp = initial_base_bmp;
    state.current_layer_bmp = initial_layer_bmp;
    state.poor_bga_bmp = initial_poor_bmp;
    state.poor_until_time = 0.0;
    state.video_players = video_players;
    state.video_start_times = video_start_times;
    state.active_bga_image = load_stage_image(song)
        .filter(|_| state.bga_enabled)
        .map(|img| img.create_scaled(320, 180));
    state.song_end_time = total_duration;
    state.bgm_cursor = bgm_cursor;
    state.score_update = ScoreUpdate::default();
    state.current_replay = if !state.is_replay_playback && !state.is_auto_play {
        let mut replay = ReplayData::new(song.id);
        replay.random_seed = chart.random_seed;
        replay.modifier = Some(state.play_options.lane_modifier);
        replay.gauge = Some(state.play_options.gauge_type);
        Some(replay)
    } else {
        None
    };
    state.playback_cursor = 0;
    state.is_gameplay_paused = false;
    state.pause_selected_option = 0;
    state.gameplay_end = None;
    state.play_end.reset();
    state.gauge_trend = GaugeTrend::new(total_duration);
    state.gameplay_readout = None;
    state.audio_engine = audio_engine;
    state.screen = AppScreen::Gameplay;
    state.view.reset_feedback();
    state.held_keys.clear();
    state.gpu_ui.release_song_textures(&mut state.d3d11);
    state.window.request_redraw();
}

/// How long the end banner stays before the result screen (wall clock, see
/// `GameplayEnd`).
pub const END_BANNER_SECONDS: f64 = 1.5;

/// The end banner after a song: the clear lamp of the play, and when the
/// banner started. The timer is `Instant`, not the audio clock, on purpose:
/// nothing is judged any more and the audio may already be stopped (a failed
/// stage stops it), so INV-1 has nothing to protect here.
#[derive(Debug, Clone, Copy)]
pub struct GameplayEnd {
    pub lamp: ClearType,
    pub started: Instant,
}

/// The audio clock's time in the current song (0 when there is no audio).
pub fn audio_time_now(state: &AppState) -> f64 {
    state
        .audio_engine
        .as_ref()
        .map(|a| a.clock().current_time_seconds())
        .unwrap_or(0.0)
}

/// Whether a play's end has been taken yet. A play ends once: the end
/// saves the play, and every later end event of the same play (a second
/// tick during the banner, or one after ENTER skipped the banner) is ignored.
/// Reset when a new play starts; leaving the banner does not reset it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PlayEndGuard {
    ended: bool,
}

impl PlayEndGuard {
    /// `true` for the first end of the play, `false` for every later call.
    pub fn take(&mut self) -> bool {
        !std::mem::replace(&mut self.ended, true)
    }

    /// A new play starts.
    pub fn reset(&mut self) {
        self.ended = false;
    }
}

/// The song is over (played out, or the stage failed). Saves the play once
/// and starts the end banner; the result screen follows in `leave_gameplay`.
/// Calling it again during the banner, or after the banner was skipped, does
/// nothing (see `PlayEndGuard`).
pub fn finish_gameplay(state: &mut AppState) {
    if !state.play_end.take() {
        return;
    }
    let lamp = match &state.active_judge {
        Some(judge) => judge.score().clear_type(),
        None => ClearType::Failed,
    };
    record_play(state);
    state.gameplay_end = Some(GameplayEnd {
        lamp,
        started: Instant::now(),
    });
    state.window.request_redraw();
}

/// Leaves the end banner for the result screen (its time is up, or ENTER / ESC).
pub fn leave_gameplay(state: &mut AppState) {
    state.gameplay_end = None;
    state.video_players.clear();
    state.video_start_times.clear();
    state.screen = AppScreen::Result;
    state.result_entered_at = Instant::now();
    state.window.request_redraw();
}

/// Writes the play's score (and replay) for a manual play from the start.
fn record_play(state: &mut AppState) {
    let Some(judge) = &state.active_judge else {
        return;
    };
    let score = judge.score();
    let (ex_score, max_combo) = (score.ex_score, score.max_combo);

    let play = PlayResult::from_tracker(
        state.active_chart_id,
        score,
        state.play_options.lane_modifier,
        state.active_chart.as_ref().and_then(|c| c.random_seed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        state.active_ln,
    );

    // Only save score records and replays for actual manual playthroughs from start
    state.previous_best = state
        .score_store
        .get_for(state.active_chart_id, state.active_ln)
        .cloned();
    if !state.is_auto_play && !state.is_replay_playback && state.start_measure == 0 {
        let update = state.score_store.update(play);
        state.score_update = update;
        save_scores(&state.score_store);

        // The replay on disk is the one that set the best EX score; a play
        // that only raised the lamp or combo must not replace it.
        let rep_path = replay_path(state.active_chart_id, state.active_ln);
        if update.ex || !Path::new(&rep_path).exists() {
            if let Some(mut rep) = state.current_replay.take() {
                rep.set_score(ex_score, max_combo);
                rep.ln = state.active_ln;
                let _ = fs::create_dir_all(REPLAYS_DIR);
                let _ = fs::write(&rep_path, rep.serialize_to_string());
            }
        }
    } else {
        state.score_update = ScoreUpdate::default();
    }
}

/// Result of an in-game simulation tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameplayTickResult {
    /// Gameplay continues normally.
    Continue,
    /// Player failed the stage (e.g. gauge reached 0 on Hard / Hazard).
    StageFailed,
    /// Song has finished playing (audio passed end time + padding).
    SongFinished,
}

/// Advances gameplay timelines, processes replay/autoplay drivers, updates judge misses,
/// and checks for stage failure / completion.
pub fn tick_gameplay(state: &mut AppState, audio_time: f64) -> GameplayTickResult {
    let effective_judge_time = judged_time(audio_time, state.play_options.judge_offset_ms);

    if !state.is_gameplay_paused {
        // 1. Advance BGM notes and BGA timeline events
        state.advance_gameplay_timelines(audio_time);

        // 2. Replay Playback driver, Auto-play driver, or Manual update misses
        if state.is_replay_playback {
            if let Some(replay) = &state.playback_replay {
                while state.playback_cursor < replay.events.len() {
                    let ev = &replay.events[state.playback_cursor];
                    if audio_time >= ev.time_seconds {
                        if ev.is_down {
                            state.view.set_key_state(ev.lane, true);
                            if let Some(judge) = &mut state.active_judge {
                                if let Some((res, wav_id)) =
                                    judge.handle_key_down(ev.lane, ev.time_seconds)
                                {
                                    if res.grade == JudgeGrade::Miss
                                        || res.grade == JudgeGrade::Poor
                                    {
                                        state.poor_until_time = audio_time + 0.4;
                                    }
                                    state.view.trigger_judge_with_lane(
                                        ev.lane,
                                        res.grade,
                                        audio_time,
                                        res.delta_ms,
                                    );
                                    if let (Some(id), Some(audio)) =
                                        (wav_id, &mut state.audio_engine)
                                    {
                                        let _ = audio.send_command(AudioCommand::PlaySample {
                                            sample_id: id,
                                            volume: 1.0,
                                            pan: 0.0,
                                        });
                                    }
                                }
                            }
                        } else {
                            state.view.set_key_state(ev.lane, false);
                            if let Some(judge) = &mut state.active_judge {
                                if let Some(res) = judge.handle_key_up(ev.lane, ev.time_seconds) {
                                    state.view.trigger_judge_with_lane(
                                        ev.lane,
                                        res.grade,
                                        audio_time,
                                        res.delta_ms,
                                    );
                                }
                            }
                        }
                        state.playback_cursor += 1;
                    } else {
                        break;
                    }
                }
            }
            if let Some(judge) = &mut state.active_judge {
                let misses = judge.update_misses(effective_judge_time);
                for (_lane, miss_res) in misses {
                    state.poor_until_time = audio_time + 0.4;
                    state.view.trigger_judge(miss_res.grade, audio_time, 0.0);
                }
            }
        } else if state.is_auto_play {
            if let Some(judge) = &mut state.active_judge {
                let hits = judge.auto_play_update(audio_time);
                for (lane, hit_res, wav_id) in hits {
                    state.view.trigger_judge_with_lane(
                        lane,
                        hit_res.grade,
                        audio_time,
                        hit_res.delta_ms,
                    );
                    if let (Some(id), Some(audio)) = (wav_id, &mut state.audio_engine) {
                        let _ = audio.send_command(AudioCommand::PlaySample {
                            sample_id: id,
                            volume: 1.0,
                            pan: 0.0,
                        });
                    }
                }
            }
        } else if let Some(judge) = &mut state.active_judge {
            let misses = judge.update_misses(effective_judge_time);
            for (lane, miss_res) in misses {
                state.poor_until_time = audio_time + 0.4;
                state
                    .view
                    .trigger_judge_with_lane(lane, miss_res.grade, audio_time, 0.0);
            }
        }
    }

    // 3. Advance video frame for BGA
    state.update_video_bga(audio_time);

    // 4. Check Stage Failure (Hard / Hazard gauge depleted to 0). The gauge
    // is sampled here too, so the trend graph ends at the failure point.
    let is_stage_failed = match state.active_judge.as_ref().map(|j| j.score()) {
        Some(score) => {
            state
                .gauge_trend
                .sample(audio_time, score.gauge, score.is_failed);
            score.is_failed
        }
        None => false,
    };

    if is_stage_failed && !state.is_auto_play && !state.is_replay_playback {
        return GameplayTickResult::StageFailed;
    }

    // 5. Check Song Completion
    if audio_time >= state.song_end_time + 1.5 {
        return GameplayTickResult::SongFinished;
    }

    GameplayTickResult::Continue
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_play_ends_once_even_when_the_banner_is_skipped() {
        let mut guard = PlayEndGuard::default();
        // The song ends: the first end saves and starts the banner.
        assert!(guard.take());
        // A second end event during the banner does nothing.
        assert!(!guard.take());
        // ENTER skips the banner (`leave_gameplay` clears it), then a
        // stray end event arrives: still no second save.
        assert!(!guard.take());
        // A new play starts with a fresh guard.
        guard.reset();
        assert!(guard.take());
        assert!(!guard.take());
    }

    #[test]
    fn saves_counted_through_the_end_phase_stay_at_one() {
        let mut guard = PlayEndGuard::default();
        let mut banner: Option<GameplayEnd> = None;
        let mut saves = 0;
        // Ends fired from the frame loop and from a fail event, then the
        // banner is skipped with ENTER and the frame loop keeps calling.
        for step in 0..6 {
            if step == 3 {
                banner = None; // leave_gameplay
            }
            if guard.take() {
                saves += 1;
                banner = Some(GameplayEnd {
                    lamp: ClearType::Failed,
                    started: Instant::now(),
                });
            }
        }
        assert_eq!(saves, 1);
        // The banner stays skipped: no end after the skip brings it back.
        assert!(banner.is_none());
    }
}

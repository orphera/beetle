use std::fs;
use std::path::Path;
use std::time::Instant;

use beetle_audio::{AudioCommand, AudioEngine, SampleBank};
use beetle_core::{
    apply_lane_modifier, BmsChart, JudgeEngine, JudgeGrade, PlayResult, ReplayData, ScoreStore, ScoreUpdate,
    SongMetadata, TimingModel,
};

use crate::loader::{load_stage_image, spawn_background_song_loader};
use crate::state::{AppScreen, AppState, REPLAYS_DIR, SCORES_FILE};

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
    let selected_hash = song.hash;
    state
        .stage_image_cache
        .entry(selected_hash)
        .or_insert_with(|| load_stage_image(song));

    // A replay re-rolls `#RANDOM` with the seed it was recorded with; a fresh
    // play rolls anew.
    let seed = state
        .playback_replay
        .as_ref()
        .filter(|_| state.is_replay_playback)
        .and_then(|r| r.random_seed)
        .unwrap_or_else(fresh_seed);
    state.loading_receiver = Some(spawn_background_song_loader(song, seed));
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

    let mut judge_engine = JudgeEngine::new(&play_chart, &timing, state.play_options.gauge_type);
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
    state.view.skin.hi_speed = state.play_options.hi_speed;

    let mut audio_engine = AudioEngine::new(soundbank).ok();
    if let Some(audio) = &mut audio_engine {
        let _ = audio.set_master_volume(state.master_volume);
    }

    state.active_chart = Some(play_chart);
    state.active_timing = Some(timing);
    state.active_chart_hash = song.hash;
    state.active_judge = Some(judge_engine);
    state.bga_bank = bga_bank;
    state.bga_cursor = bga_cursor;
    state.current_bga_bmp = initial_base_bmp;
    state.current_layer_bmp = initial_layer_bmp;
    state.poor_bga_bmp = initial_poor_bmp;
    state.poor_until_time = 0.0;
    state.video_players = video_players;
    state.video_start_times = video_start_times;
    state.active_bga_image = load_stage_image(song).map(|img| img.create_scaled(320, 180));
    state.song_end_time = total_duration;
    state.bgm_cursor = bgm_cursor;
    state.score_update = ScoreUpdate::default();
    state.current_replay = if !state.is_replay_playback && !state.is_auto_play {
        let mut replay = ReplayData::new(song.hash);
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
    state.audio_engine = audio_engine;
    state.screen = AppScreen::Gameplay;
    state.view.reset_feedback();
    state.held_keys.clear();
    state.gpu_ui.release_song_textures(&mut state.d3d11);
    state.window.request_redraw();
}

/// Writes the score file. The first save over a file in the original format
/// keeps a copy of it, and the write goes through a temporary file so an
/// interrupted save cannot leave a half-written `scores.dat`.
fn save_scores(store: &ScoreStore) {
    if let Ok(old) = fs::read_to_string(SCORES_FILE) {
        let backup = format!("{SCORES_FILE}.v1.bak");
        if ScoreStore::is_legacy_format(&old) && !Path::new(&backup).exists() {
            let _ = fs::write(backup, old);
        }
    }
    let temp = format!("{SCORES_FILE}.tmp");
    if fs::write(&temp, store.save_to_string()).is_ok() {
        let _ = fs::rename(&temp, SCORES_FILE);
    }
}

pub fn finish_gameplay(state: &mut AppState) {
    if let Some(judge) = &state.active_judge {
        let score = judge.score();
        let (ex_score, max_combo) = (score.ex_score, score.max_combo);

        let play = PlayResult::from_tracker(
            state.active_chart_hash,
            score,
            state.play_options.lane_modifier,
            state.active_chart.as_ref().and_then(|c| c.random_seed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        );

        // Only save score records and replays for actual manual playthroughs from start
        state.previous_best = state.score_store.get(state.active_chart_hash).cloned();
        if !state.is_auto_play && !state.is_replay_playback && state.start_measure == 0 {
            let update = state.score_store.update(play);
            state.score_update = update;
            save_scores(&state.score_store);

            // The replay on disk is the one that set the best EX score; a play
            // that only raised the lamp or combo must not replace it.
            let rep_path = format!("{}/{:016x}.rep", REPLAYS_DIR, state.active_chart_hash);
            if update.ex || !Path::new(&rep_path).exists() {
                if let Some(mut rep) = state.current_replay.take() {
                    rep.set_score(ex_score, max_combo);
                    let _ = fs::create_dir_all(REPLAYS_DIR);
                    let _ = fs::write(&rep_path, rep.serialize_to_string());
                }
            }
        } else {
            state.score_update = ScoreUpdate::default();
        }
    }

    state.video_players.clear();
    state.video_start_times.clear();
    state.screen = AppScreen::Result;
    state.result_entered_at = std::time::Instant::now();

    state.window.request_redraw();
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
    let effective_judge_time = audio_time + (state.play_options.judge_offset_ms / 1000.0);

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
                    state
                        .view
                        .trigger_judge(miss_res.grade, audio_time, 0.0);
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

    // 4. Check Stage Failure (Hard / Hazard gauge depleted to 0)
    let is_stage_failed = state
        .active_judge
        .as_ref()
        .map(|j| j.score().is_failed)
        .unwrap_or(false);

    if is_stage_failed && !state.is_auto_play && !state.is_replay_playback {
        return GameplayTickResult::StageFailed;
    }

    // 5. Check Song Completion
    if audio_time >= state.song_end_time + 1.5 {
        return GameplayTickResult::SongFinished;
    }

    GameplayTickResult::Continue
}

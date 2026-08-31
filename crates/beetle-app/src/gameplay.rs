use std::fs;
use std::path::Path;
use std::time::Instant;

use beetle_audio::{AudioEngine, SampleBank};
use beetle_core::{
    apply_lane_modifier, BmsChart, ClearType, JudgeEngine, ReplayData, ScoreRecord, SongMetadata,
    TimingModel,
};

use crate::loader::{load_stage_image, spawn_background_song_loader};
use crate::state::{AppScreen, AppState, REPLAYS_DIR, SCORES_FILE};

pub fn queue_start_gameplay(state: &mut AppState, song: &SongMetadata) {
    state.screen = AppScreen::Loading;
    state.loading_song = Some(song.clone());
    state.loading_spinner_frame = 0;
    state.loading_anim_time = Instant::now();

    // Cache stage image for loading screen
    let selected_hash = song.hash;
    if !state.stage_image_cache.contains_key(&selected_hash) {
        let img = load_stage_image(song);
        state.stage_image_cache.insert(selected_hash, img);
    }

    state.loading_receiver = Some(spawn_background_song_loader(song));
    state.window.request_redraw();
}

pub fn finalize_start_gameplay(
    state: &mut AppState,
    song: &SongMetadata,
    chart: BmsChart,
    timing: TimingModel,
    soundbank: SampleBank,
    bga_bank: std::collections::HashMap<beetle_core::BmpId, beetle_render::ImageBuffer>,
    video_path: Option<std::path::PathBuf>,
) {
    // Apply Lane Modifier (Mirror, Random, R-Random, S-Random)
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(42);

    let mut play_chart = chart.clone();
    if !state.is_replay_playback {
        play_chart.notes = apply_lane_modifier(&chart.notes, state.play_options.lane_modifier, seed);
    }

    let mut judge_engine = JudgeEngine::new(&play_chart, &timing, state.play_options.gauge_type);
    let total_duration = timing.total_duration_seconds(&play_chart);

    let mut bgm_cursor = 0;
    let mut bga_cursor = 0;
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
            if timing.beat_to_time_seconds(ev.measure, ev.fraction) < start_time {
                bga_cursor += 1;
            } else {
                break;
            }
        }
    }

    let play_mode = play_chart.detect_play_mode();
    state.renderer.skin.set_play_mode(play_mode);
    state.renderer.skin.hi_speed = state.play_options.hi_speed;

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
    state.current_bga_bmp = None;
    state.current_layer_bmp = None;
    state.poor_bga_bmp = None;
    state.poor_until_time = 0.0;
    state.active_video_player = video_path.and_then(beetle_render::BgaVideoPlayer::open);
    state.active_bga_image = load_stage_image(song).map(|img| img.create_scaled(320, 180));
    state.song_end_time = total_duration;
    state.bgm_cursor = bgm_cursor;
    state.is_new_record = false;
    state.current_replay = if !state.is_replay_playback && !state.is_auto_play {
        Some(ReplayData::new(song.hash))
    } else {
        None
    };
    state.playback_cursor = 0;
    state.is_gameplay_paused = false;
    state.pause_selected_option = 0;
    state.audio_engine = audio_engine;
    state.screen = AppScreen::Gameplay;
    state.window.request_redraw();
}

pub fn finish_gameplay(state: &mut AppState) {
    if let Some(judge) = &state.active_judge {
        let score = judge.score();
        let clear_type = if score.is_cleared() {
            if score.miss_count == 0 && score.poor_count == 0 && score.bad_count == 0 {
                if score.great_count == 0 && score.good_count == 0 {
                    ClearType::Perfect
                } else {
                    ClearType::FullCombo
                }
            } else {
                ClearType::Clear
            }
        } else {
            ClearType::Failed
        };

        let record = ScoreRecord {
            chart_hash: state.active_chart_hash,
            ex_score: score.ex_score,
            max_combo: score.max_combo,
            accuracy_rate: score.accuracy_rate(),
            clear_type,
            pgreat_count: score.pgreat_count,
            great_count: score.great_count,
            good_count: score.good_count,
            bad_count: score.bad_count,
            poor_count: score.poor_count,
            miss_count: score.miss_count,
        };

        // Only save score records and replays for actual manual playthroughs from start
        state.previous_best = state.score_store.get(state.active_chart_hash).cloned();
        if !state.is_auto_play && !state.is_replay_playback && state.start_measure == 0 {
            state.is_new_record = state.score_store.update(record.clone());
            let score_data = state.score_store.save_to_string();
            let _ = fs::write(SCORES_FILE, score_data);

            // Save replay file
            let rep_path = format!("{}/{:016x}.rep", REPLAYS_DIR, state.active_chart_hash);
            if state.is_new_record || !Path::new(&rep_path).exists() {
                if let Some(mut rep) = state.current_replay.take() {
                    rep.set_score(&record);
                    let _ = fs::create_dir_all(REPLAYS_DIR);
                    let _ = fs::write(&rep_path, rep.serialize_to_string());
                }
            }
        } else {
            state.is_new_record = false;
        }
    }

    state.screen = AppScreen::Result;
    state.window.request_redraw();
}

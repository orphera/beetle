#![windows_subsystem = "windows"]

mod config;
mod demo;
mod gameplay;
mod handlers;
mod input;
mod loader;
mod scanner;
mod state;

use std::env;
use std::fs;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use beetle_audio::AudioCommand;
use beetle_core::{GaugeType, Lane, LaneModifier, SongMetadata};
use beetle_render::{SkinConfig, SoftwareRenderer};
use config::AppConfig;
use gameplay::{finalize_start_gameplay, finish_gameplay, queue_start_gameplay};
use handlers::{
    handle_gameplay_input, handle_key_config_input, handle_result_input, handle_song_select_input,
};
use input::{InputConfig, KeyPreset};
use loader::load_stage_image;
use softbuffer::{Context, Surface};
use state::{
    init_songs_and_scores, AppScreen, AppState, SongCategory, REPLAYS_DIR,
};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

struct BeetleApp {
    state: Option<AppState>,
    cli_bms_path: Option<String>,
}

impl BeetleApp {
    pub fn new(cli_bms_path: Option<String>) -> Self {
        Self {
            state: None,
            cli_bms_path,
        }
    }
}

impl ApplicationHandler for BeetleApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let window_attributes = Window::default_attributes()
            .with_title("Beetle — BMS Rhythm Engine")
            .with_inner_size(LogicalSize::new(1024, 768))
            .with_min_inner_size(LogicalSize::new(800, 600));

        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
        let context = Context::new(window.clone()).unwrap();
        let mut surface = Surface::new(&context, window.clone()).unwrap();

        let size = window.inner_size();
        let _ = surface.resize(
            NonZeroU32::new(size.width.max(1)).unwrap(),
            NonZeroU32::new(size.height.max(1)).unwrap(),
        );

        let saved_config = AppConfig::load();
        let mut skin = SkinConfig::default();
        skin.hi_speed = saved_config.play_options.hi_speed;
        skin.lane_cover_ratio = saved_config.lane_cover_ratio;

        let renderer = SoftwareRenderer::new(size.width, size.height, skin)
            .expect("Failed to initialize software renderer");

        let (songs, score_store) = init_songs_and_scores(saved_config.sort_mode);

        let mut app_state = AppState {
            window,
            _context: context,
            surface,
            renderer,
            audio_engine: None,
            screen: AppScreen::SongSelect,
            songs,
            filtered_indices: Vec::new(),
            selected_song_idx: 0,
            search_query: String::new(),
            is_search_active: false,
            category_mode: SongCategory::All,
            sort_mode: saved_config.sort_mode,
            show_option_modal: false,
            modal_row: 0,
            selected_key_idx: 0,
            score_store,
            play_options: saved_config.play_options,
            is_auto_play: false,
            is_replay_playback: false,
            is_gameplay_paused: false,
            pause_selected_option: 0,
            current_replay: None,
            playback_replay: None,
            playback_cursor: 0,
            start_measure: 0,
            stage_image_cache: std::collections::HashMap::new(),
            active_bga_image: None,
            active_chart: None,
            active_timing: None,
            active_chart_hash: 0,
            active_judge: None,
            song_end_time: 0.0,
            is_new_record: false,
            previous_best: None,
            input_config: {
                let mut cfg = InputConfig::new(saved_config.key_preset);
                if !saved_config.custom_key_bindings.is_empty() {
                    cfg.deserialize_bindings(&saved_config.custom_key_bindings);
                }
                cfg
            },
            is_rebinding_key: false,
            master_volume: saved_config.master_volume,
            bgm_cursor: 0,
            loading_song: None,
            loading_receiver: None,
            loading_spinner_frame: 0,
            loading_anim_time: Instant::now(),
            last_render_time: Instant::now(),
        };

        app_state.recompute_filtered_songs();

        // If a specific file path was provided via CLI, launch directly into gameplay
        if let Some(cli_path) = &self.cli_bms_path {
            let p = Path::new(cli_path);
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext.eq_ignore_ascii_case("bmsp") {
                if let Ok(mut pkg) = bms_package::PackageReader::open_file(p) {
                    let path_str = p.to_string_lossy();
                    let chart_entries: Vec<String> = pkg
                        .entries()
                        .iter()
                        .filter_map(|e| {
                            let e_ext = e.path.rsplit('.').next().unwrap_or("");
                            if e_ext.eq_ignore_ascii_case("bms")
                                || e_ext.eq_ignore_ascii_case("bme")
                                || e_ext.eq_ignore_ascii_case("bml")
                            {
                                Some(e.path.clone())
                            } else {
                                None
                            }
                        })
                        .collect();

                    for entry_path in chart_entries {
                        if let Ok(bytes) = pkg.read_entry(&entry_path) {
                            let content = String::from_utf8_lossy(&bytes);
                            let virtual_path = format!("{}::{}", path_str, entry_path);
                            if let Some(meta) = SongMetadata::from_content(&virtual_path, &content) {
                                queue_start_gameplay(&mut app_state, &meta);
                                break;
                            }
                        }
                    }
                }
            } else if let Ok(bytes) = fs::read(cli_path) {
                let content = String::from_utf8_lossy(&bytes);
                if let Some(meta) = SongMetadata::from_content(cli_path, &content) {
                    queue_start_gameplay(&mut app_state, &meta);
                }
            }
        }

        self.state = Some(app_state);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(state) = &mut self.state else {
            return;
        };

        match state.screen {
            AppScreen::Loading => {
                if let Some(rx) = &state.loading_receiver {
                    if let Ok(res) = rx.try_recv() {
                        state.loading_receiver = None;
                        match res {
                            Ok((chart, timing, soundbank)) => {
                                if let Some(song) = state.loading_song.take() {
                                    finalize_start_gameplay(state, &song, chart, timing, soundbank);
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to load song: {e}");
                                state.screen = AppScreen::SongSelect;
                                state.window.request_redraw();
                            }
                        }
                    }
                }

                let now = Instant::now();
                if now.duration_since(state.loading_anim_time) >= Duration::from_millis(30) {
                    state.loading_spinner_frame = state.loading_spinner_frame.wrapping_add(1);
                    state.loading_anim_time = now;
                    state.window.request_redraw();
                }
                event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(16)));
            }
            AppScreen::Gameplay => {
                let now = Instant::now();
                let elapsed = now.duration_since(state.last_render_time);
                let target = Duration::from_millis(4);
                if elapsed < target {
                    std::thread::sleep(target - elapsed);
                }
                state.last_render_time = Instant::now();
                state.window.request_redraw();
                event_loop.set_control_flow(ControlFlow::Poll);
            }
            _ => {
                // Static screens (SongSelect, Result, KeyConfig) only update on events (keys, resizing)
                event_loop.set_control_flow(ControlFlow::Wait);
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = &mut self.state else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                state.save_config();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
                    let _ = state.surface.resize(w, h);
                    state.renderer.resize(size.width, size.height);
                    state.window.request_redraw();
                }
            }
            WindowEvent::DroppedFile(path) => {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext.eq_ignore_ascii_case("bmsp") {
                    if let Ok(mut pkg) = bms_package::PackageReader::open_file(&path) {
                        let path_str = path.to_string_lossy();
                        let chart_entries: Vec<String> = pkg
                            .entries()
                            .iter()
                            .filter_map(|e| {
                                let e_ext = e.path.rsplit('.').next().unwrap_or("");
                                if e_ext.eq_ignore_ascii_case("bms")
                                    || e_ext.eq_ignore_ascii_case("bme")
                                    || e_ext.eq_ignore_ascii_case("bml")
                                {
                                    Some(e.path.clone())
                                } else {
                                    None
                                }
                            })
                            .collect();

                        for entry_path in chart_entries {
                            if let Ok(bytes) = pkg.read_entry(&entry_path) {
                                let content = String::from_utf8_lossy(&bytes);
                                let virtual_path = format!("{}::{}", path_str, entry_path);
                                if let Some(meta) = SongMetadata::from_content(&virtual_path, &content) {
                                    queue_start_gameplay(state, &meta);
                                    break;
                                }
                            }
                        }
                    }
                } else if ext.eq_ignore_ascii_case("bms")
                    || ext.eq_ignore_ascii_case("bme")
                    || ext.eq_ignore_ascii_case("bml")
                {
                    if let Ok(bytes) = fs::read(&path) {
                        let content = String::from_utf8_lossy(&bytes);
                        if let Some(meta) = SongMetadata::from_content(&path.to_string_lossy(), &content) {
                            queue_start_gameplay(state, &meta);
                        }
                    }
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    ref key_event @ KeyEvent {
                        physical_key,
                        state: key_state,
                        repeat,
                        ..
                    },
                ..
            } => {
                handle_keyboard_input(state, physical_key, key_state, repeat, key_event.text.as_deref());
                state.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                match state.screen {
                    AppScreen::SongSelect => {
                        let selected_hash = state.current_selected_song().map(|s| s.hash).unwrap_or(0);
                        if selected_hash != 0 && !state.stage_image_cache.contains_key(&selected_hash) {
                            let img = state.current_selected_song().and_then(load_stage_image);
                            state.stage_image_cache.insert(selected_hash, img);
                        }

                        let visible_songs = state.current_visible_songs();
                        let stage_img = state.stage_image_cache.get(&selected_hash).and_then(|opt| opt.as_ref());
                        state.renderer.render_song_select(
                            &visible_songs,
                            state.selected_song_idx,
                            &state.score_store,
                            state.sort_mode.as_str(),
                            state.category_mode.as_str(),
                            &state.search_query,
                            state.is_search_active,
                            stage_img,
                            state.songs.len(),
                        );

                        // Check replay existence for selected song
                        let has_replay = state
                            .current_selected_song()
                            .map(|s| Path::new(&format!("{}/{:016x}.rep", REPLAYS_DIR, s.hash)).exists())
                            .unwrap_or(false);

                        // Song select options bar
                        let rep_str = if has_replay { "  [R]: Replay" } else { "" };
                        let auto_str = if state.is_auto_play { "[AUTO: ON]" } else { "[AUTO: OFF]" };
                        let opt_bar = format!(
                            "SPD: {:.0} (F3/F4)  MOD: {} (F7)  GAUGE: {} (F6)  {}{}  [Tab]: Options  [A]: AutoPlay",
                            state.play_options.hi_speed,
                            state.play_options.lane_modifier.as_str(),
                            state.play_options.gauge_type.as_str(),
                            auto_str,
                            rep_str,
                        );
                        state.renderer.draw_footer_text(&opt_bar);

                        // If option modal is open, overlay modal on top
                        if state.show_option_modal {
                            state.renderer.render_option_modal(
                                &state.play_options,
                                state.input_config.preset.as_str(),
                                state.is_auto_play,
                                state.start_measure,
                                state.master_volume,
                                state.modal_row,
                            );
                        }
                    }
                    AppScreen::Loading => {
                        let selected_hash = state.loading_song.as_ref().map(|s| s.hash).unwrap_or(0);
                        let stage_img = state.stage_image_cache.get(&selected_hash).and_then(|opt| opt.as_ref());

                        let title = state.loading_song.as_ref().map(|s| s.title.as_str()).unwrap_or("Unknown");
                        let artist = state.loading_song.as_ref().map(|s| s.artist.as_str()).unwrap_or("Unknown");
                        let genre = state.loading_song.as_ref().map(|s| s.genre.as_str()).unwrap_or("");

                        state.renderer.render_loading_screen(
                            title,
                            artist,
                            genre,
                            stage_img,
                            state.loading_spinner_frame,
                            "Decoding soundbank & preparing audio engine...",
                        );
                    }
                    AppScreen::Gameplay => {
                        let audio_time = state
                            .audio_engine
                            .as_ref()
                            .map(|a| a.clock().current_time_seconds())
                            .unwrap_or(0.0);

                        let effective_judge_time = audio_time + (state.play_options.judge_offset_ms / 1000.0);

                        if !state.is_gameplay_paused {
                            // 1. BGM schedule
                            if let (Some(audio), Some(chart), Some(timing)) =
                                (&mut state.audio_engine, &state.active_chart, &state.active_timing)
                            {
                                while state.bgm_cursor < chart.bgm_notes.len() {
                                    let (m, f, wav_id) = chart.bgm_notes[state.bgm_cursor];
                                    let target_t = timing.beat_to_time_seconds(m, f);

                                    if audio_time >= target_t {
                                        let _ = audio.send_command(AudioCommand::PlaySample {
                                            sample_id: wav_id,
                                            volume: 1.0,
                                            pan: 0.0,
                                        });
                                        state.bgm_cursor += 1;
                                    } else {
                                        break;
                                    }
                                }
                            }

                            // 2. Replay Playback driver or Auto-play driver or Manual update misses
                            if state.is_replay_playback {
                                if let Some(replay) = &state.playback_replay {
                                    while state.playback_cursor < replay.events.len() {
                                        let ev = &replay.events[state.playback_cursor];
                                        if audio_time >= ev.time_seconds {
                                            if ev.is_down {
                                                state.renderer.set_key_state(ev.lane, true);
                                                if let Some(judge) = &mut state.active_judge {
                                                    if let Some((res, wav_id)) = judge.handle_key_down(ev.lane, ev.time_seconds) {
                                                        state.renderer.trigger_judge_with_lane(ev.lane, res.grade, audio_time, res.delta_ms);
                                                        if let (Some(id), Some(audio)) = (wav_id, &mut state.audio_engine) {
                                                            let _ = audio.send_command(AudioCommand::PlaySample {
                                                                sample_id: id,
                                                                volume: 1.0,
                                                                pan: 0.0,
                                                            });
                                                        }
                                                    }
                                                }
                                            } else {
                                                state.renderer.set_key_state(ev.lane, false);
                                                if let Some(judge) = &mut state.active_judge {
                                                    if let Some(res) = judge.handle_key_up(ev.lane, ev.time_seconds) {
                                                        state.renderer.trigger_judge_with_lane(ev.lane, res.grade, audio_time, res.delta_ms);
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
                                        state.renderer.trigger_judge(miss_res.grade, audio_time, 0.0);
                                    }
                                }
                            } else if state.is_auto_play {
                                if let Some(judge) = &mut state.active_judge {
                                    let hits = judge.auto_play_update(audio_time);
                                    for (lane, hit_res, wav_id) in hits {
                                        state.renderer.trigger_judge_with_lane(lane, hit_res.grade, audio_time, hit_res.delta_ms);
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
                                    state.renderer.trigger_judge_with_lane(lane, miss_res.grade, audio_time, 0.0);
                                }
                            }
                        }

                        let mut visual_levels = [0.0; 16];
                        if let Some(audio) = &state.audio_engine {
                            audio.get_visual_levels(&mut visual_levels);
                        }

                        // Render gameplay frame
                        if let (Some(chart), Some(timing), Some(judge)) =
                            (&state.active_chart, &state.active_timing, &state.active_judge)
                        {
                            state.renderer.render_gameplay(
                                chart,
                                timing,
                                audio_time,
                                judge.score(),
                                &visual_levels,
                                state.active_bga_image.as_ref(),
                            );
                        }

                        // Overlay Pause Modal if active
                        if state.is_gameplay_paused {
                            let title = state.active_chart.as_ref().map(|c| c.header.title.as_str()).unwrap_or("Unknown");
                            let artist = state.active_chart.as_ref().map(|c| c.header.artist.as_str()).unwrap_or("Unknown");
                            state.renderer.render_pause_modal(
                                title,
                                artist,
                                audio_time,
                                state.song_end_time,
                                state.pause_selected_option,
                            );
                        } else {
                            let footer_text = if state.is_replay_playback {
                                "[ REPLAY PLAYBACK MODE - Press ESC to Return ]"
                            } else if state.is_auto_play {
                                "[ AUTO PLAY ACTIVE - Press ESC to Return ]"
                            } else {
                                match state.input_config.preset {
                                    KeyPreset::HomeRow => "KEYS: [Shift]+S D F Space J K L  (F1: Layout | 1/2: Speed | F10/F11: Cover | Esc: Pause)",
                                    KeyPreset::ArcadeZx => "KEYS: [Shift]+Z S X D C F V      (F1: Layout | 1/2: Speed | F10/F11: Cover | Esc: Pause)",
                                    KeyPreset::Custom => "KEYS: Custom Key Layout Active    (F1: Layout | 1/2: Speed | F10/F11: Cover | Esc: Pause)",
                                }
                            };
                            state.renderer.draw_footer_text(footer_text);
                        }

                        // Check Song End
                        if audio_time >= state.song_end_time + 1.5 {
                            finish_gameplay(state);
                        }
                    }
                    AppScreen::Result => {
                        if let (Some(chart), Some(judge)) = (&state.active_chart, &state.active_judge) {
                            state.renderer.render_result(chart, judge.score(), state.is_new_record, state.previous_best.as_ref());
                        }
                    }
                    AppScreen::KeyConfig => {
                        let key_names = [
                            ("SCRATCH (1S)", state.input_config.get_key_name_for_lane(Lane::Scratch)),
                            ("KEY 1 (1P)", state.input_config.get_key_name_for_lane(Lane::Key1)),
                            ("KEY 2 (1P)", state.input_config.get_key_name_for_lane(Lane::Key2)),
                            ("KEY 3 (1P)", state.input_config.get_key_name_for_lane(Lane::Key3)),
                            ("KEY 4 (1P)", state.input_config.get_key_name_for_lane(Lane::Key4)),
                            ("KEY 5 (1P)", state.input_config.get_key_name_for_lane(Lane::Key5)),
                            ("KEY 6 (1P)", state.input_config.get_key_name_for_lane(Lane::Key6)),
                            ("KEY 7 (1P)", state.input_config.get_key_name_for_lane(Lane::Key7)),
                        ];
                        state.renderer.render_key_config(
                            &key_names,
                            state.selected_key_idx,
                            state.input_config.preset.as_str(),
                            state.is_rebinding_key,
                        );
                    }
                }

                // Blit to softbuffer
                let width = state.renderer.width();
                let height = state.renderer.height();
                if width > 0 && height > 0 {
                    if let Ok(mut buffer) = state.surface.buffer_mut() {
                        let data = state.renderer.data();
                        let buffer_slice = buffer.as_mut();
                        for (dest, src) in buffer_slice.iter_mut().zip(data.chunks_exact(4)) {
                            *dest = ((src[0] as u32) << 16) | ((src[1] as u32) << 8) | (src[2] as u32);
                        }
                        let _ = buffer.present();
                    }
                }
            }
            _ => (),
        }
    }
}

fn handle_keyboard_input(
    state: &mut AppState,
    physical_key: PhysicalKey,
    key_state: ElementState,
    _repeat: bool,
    text: Option<&str>,
) {
    let PhysicalKey::Code(code) = physical_key else {
        return;
    };

    // Global Hotkeys (when key is pressed)
    if key_state == ElementState::Pressed && !state.is_search_active && !state.is_rebinding_key {
        if code == KeyCode::F6 {
            state.play_options.gauge_type = match state.play_options.gauge_type {
                GaugeType::Easy => GaugeType::Groove,
                GaugeType::Groove => GaugeType::Hard,
                GaugeType::Hard => GaugeType::Hazard,
                GaugeType::Hazard => GaugeType::Easy,
            };
            state.save_config();
            return;
        } else if code == KeyCode::F7 {
            state.play_options.lane_modifier = match state.play_options.lane_modifier {
                LaneModifier::Regular => LaneModifier::Mirror,
                LaneModifier::Mirror => LaneModifier::Random,
                LaneModifier::Random => LaneModifier::RRandom,
                LaneModifier::RRandom => LaneModifier::SRandom,
                LaneModifier::SRandom => LaneModifier::Regular,
            };
            state.save_config();
            return;
        } else if code == KeyCode::F8 {
            state.play_options.judge_offset_ms = (state.play_options.judge_offset_ms - 2.0).max(-100.0);
            state.save_config();
            return;
        } else if code == KeyCode::F9 {
            state.play_options.judge_offset_ms = (state.play_options.judge_offset_ms + 2.0).min(100.0);
            state.save_config();
            return;
        }
    }

    match state.screen {
        AppScreen::SongSelect => handle_song_select_input(state, key_state, code, text),
        AppScreen::Loading => {
            if key_state == ElementState::Pressed && code == KeyCode::Escape {
                state.loading_receiver = None;
                state.loading_song = None;
                state.screen = AppScreen::SongSelect;
            }
        }
        AppScreen::Gameplay => handle_gameplay_input(state, key_state, code, physical_key),
        AppScreen::Result => handle_result_input(state, key_state, code),
        AppScreen::KeyConfig => handle_key_config_input(state, key_state, code),
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let bms_path = args.get(1).cloned();

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = BeetleApp::new(bms_path);
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("Application error: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beetle_core::SortMode;
    use state::filter_song_indices;

    #[test]
    fn test_song_category_transitions() {
        assert_eq!(SongCategory::All.next(), SongCategory::Keys5);
        assert_eq!(SongCategory::Keys5.next(), SongCategory::Keys7);
        assert_eq!(SongCategory::Keys7.next(), SongCategory::Keys9);
        assert_eq!(SongCategory::Keys9.next(), SongCategory::Keys10);
        assert_eq!(SongCategory::Keys10.next(), SongCategory::Keys14);
        assert_eq!(SongCategory::Keys14.next(), SongCategory::Level);
        assert_eq!(SongCategory::Level.next(), SongCategory::ClearStatus);
        assert_eq!(SongCategory::ClearStatus.next(), SongCategory::All);
    }

    #[test]
    fn test_search_and_category_filtering() {
        let (mut songs, score_store) = init_songs_and_scores(SortMode::Title);
        songs.push(SongMetadata {
            hash: 101,
            file_path: "test1.bms".to_string(),
            title: "First Anthem".to_string(),
            subtitle: "".to_string(),
            artist: "Sound Artist".to_string(),
            genre: "Trance".to_string(),
            bpm: 140.0,
            play_level: 5,
            notes_count: 500,
            play_mode: beetle_core::PlayMode::Keys7,
        });
        songs.push(SongMetadata {
            hash: 102,
            file_path: "test2.bms".to_string(),
            title: "Second Beat".to_string(),
            subtitle: "".to_string(),
            artist: "DJ Beat".to_string(),
            genre: "Hardcore".to_string(),
            bpm: 180.0,
            play_level: 10,
            notes_count: 1200,
            play_mode: beetle_core::PlayMode::Keys5,
        });

        // 1. Initial unfiltered indices
        let all_indices = filter_song_indices(&songs, "", SongCategory::All, &score_store);
        assert!(all_indices.len() >= 2);

        // 2. Filter by title "anthem"
        let anthem_indices = filter_song_indices(&songs, "anthem", SongCategory::All, &score_store);
        assert_eq!(anthem_indices.len(), 1);
        let match_song = &songs[anthem_indices[0]];
        assert_eq!(match_song.title, "First Anthem");

        // 3. Filter by artist "dj beat"
        let artist_indices = filter_song_indices(&songs, "dj beat", SongCategory::All, &score_store);
        assert_eq!(artist_indices.len(), 1);
        assert_eq!(songs[artist_indices[0]].title, "Second Beat");

        // 4. Filter by genre "hardcore"
        let genre_indices = filter_song_indices(&songs, "hardcore", SongCategory::All, &score_store);
        assert_eq!(genre_indices.len(), 1);
        assert_eq!(songs[genre_indices[0]].title, "Second Beat");

        // 5. Non-matching search query
        let empty_indices = filter_song_indices(&songs, "nonexistentxyz", SongCategory::All, &score_store);
        assert_eq!(empty_indices.len(), 0);
    }
}

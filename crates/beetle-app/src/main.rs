mod demo;
mod input;
mod scanner;

use std::env;
use std::fs;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::Arc;

use beetle_audio::{AudioCommand, AudioEngine, SampleBank};
use beetle_core::{
    apply_lane_modifier, compute_chart_hash, parse_bms, sort_songs, BmsChart, ClearType,
    GaugeType, JudgeEngine, LaneModifier, PlayOptions, ScoreRecord, ScoreStore, SongMetadata,
    SortMode, TimingModel,
};
use beetle_render::{SkinConfig, SoftwareRenderer};
use input::{InputConfig, KeyPreset};
use scanner::{load_or_scan_songs, DEFAULT_SONGS_DIR};
use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

const SCORES_FILE: &str = "scores.dat";

/// Application screens for song select, gameplay, and results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppScreen {
    SongSelect,
    Gameplay,
    Result,
}

struct AppState {
    window: Arc<Window>,
    _context: Context<Arc<Window>>,
    surface: Surface<Arc<Window>, Arc<Window>>,
    renderer: SoftwareRenderer,
    audio_engine: Option<AudioEngine>,
    screen: AppScreen,
    songs: Vec<SongMetadata>,
    selected_song_idx: usize,
    sort_mode: SortMode,
    show_option_modal: bool,
    modal_row: usize,
    score_store: ScoreStore,
    play_options: PlayOptions,
    active_chart: Option<BmsChart>,
    active_timing: Option<TimingModel>,
    active_chart_hash: u64,
    active_judge: Option<JudgeEngine>,
    song_end_time: f64,
    is_new_record: bool,
    input_config: InputConfig,
    bgm_cursor: usize,
}

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

fn init_songs_and_scores() -> (Vec<SongMetadata>, ScoreStore) {
    let mut score_store = ScoreStore::new();
    if Path::new(SCORES_FILE).exists() {
        if let Ok(score_data) = fs::read_to_string(SCORES_FILE) {
            score_store.load_from_str(&score_data);
        }
    }

    let mut songs = load_or_scan_songs(DEFAULT_SONGS_DIR);

    // Always ensure demo track is available in library
    let demo_chart = demo::create_demo_chart();
    let demo_meta = SongMetadata {
        hash: compute_chart_hash(b"BEETLE_INTERNAL_DEMO_CHART_V1"),
        file_path: ":demo:".to_string(),
        title: demo_chart.header.title,
        subtitle: demo_chart.header.subtitle,
        artist: demo_chart.header.artist,
        genre: demo_chart.header.genre,
        bpm: demo_chart.header.bpm,
        play_level: demo_chart.header.play_level,
        notes_count: demo_chart.notes.len(),
    };

    if !songs.iter().any(|s| s.file_path == ":demo:") {
        songs.insert(0, demo_meta);
    }

    sort_songs(&mut songs, SortMode::Title, &score_store);

    (songs, score_store)
}

fn load_chart_and_audio(song: &SongMetadata) -> (BmsChart, TimingModel, SampleBank) {
    if song.file_path == ":demo:" {
        let chart = demo::create_demo_chart();
        let timing = TimingModel::from_chart(&chart);
        let soundbank = demo::create_demo_sample_bank();
        return (chart, timing, soundbank);
    }

    let path = Path::new(&song.file_path);
    if let Ok(content) = fs::read_to_string(path) {
        if let Ok(chart) = parse_bms(&content) {
            let timing = TimingModel::from_chart(&chart);
            let parent_dir = path.parent().unwrap_or_else(|| Path::new("."));
            let (soundbank, loaded) = SampleBank::load_chart_soundbank(&chart, parent_dir);
            println!(
                "Loaded BMS: '{}' ({} keysounds loaded)",
                chart.header.title, loaded
            );
            return (chart, timing, soundbank);
        }
    }

    // Fallback demo
    let chart = demo::create_demo_chart();
    let timing = TimingModel::from_chart(&chart);
    let soundbank = demo::create_demo_sample_bank();
    (chart, timing, soundbank)
}

impl ApplicationHandler for BeetleApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let window_attributes = Window::default_attributes()
            .with_title("Beetle BMS Player")
            .with_inner_size(LogicalSize::new(800.0, 720.0));

        let window = match event_loop.create_window(window_attributes) {
            Ok(w) => Arc::new(w),
            Err(err) => {
                eprintln!("Failed to create window: {err}");
                return;
            }
        };

        let context = match Context::new(window.clone()) {
            Ok(c) => c,
            Err(err) => {
                eprintln!("Failed to create softbuffer context: {err}");
                return;
            }
        };

        let surface = match Surface::new(&context, window.clone()) {
            Ok(s) => s,
            Err(err) => {
                eprintln!("Failed to create softbuffer surface: {err}");
                return;
            }
        };

        let (songs, score_store) = init_songs_and_scores();
        let size = window.inner_size();
        let renderer = SoftwareRenderer::new(size.width, size.height, SkinConfig::default())
            .expect("Failed to initialize software renderer");

        let mut app_state = AppState {
            window,
            _context: context,
            surface,
            renderer,
            audio_engine: None,
            screen: AppScreen::SongSelect,
            songs,
            selected_song_idx: 0,
            sort_mode: SortMode::Title,
            show_option_modal: false,
            modal_row: 0,
            score_store,
            play_options: PlayOptions::default(),
            active_chart: None,
            active_timing: None,
            active_chart_hash: 0,
            active_judge: None,
            song_end_time: 0.0,
            is_new_record: false,
            input_config: InputConfig::default(),
            bgm_cursor: 0,
        };

        // If a specific file path was provided via CLI, launch directly into gameplay
        if let Some(cli_path) = &self.cli_bms_path {
            if let Ok(content) = fs::read_to_string(cli_path) {
                if let Some(meta) = SongMetadata::from_content(cli_path, &content) {
                    start_gameplay(&mut app_state, &meta);
                }
            }
        }

        self.state = Some(app_state);
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
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
                    let _ = state.surface.resize(w, h);
                    state.renderer.resize(size.width, size.height);
                    state.window.request_redraw();
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key,
                        state: key_state,
                        repeat: false,
                        ..
                    },
                ..
            } => {
                handle_keyboard_input(state, physical_key, key_state);
            }
            WindowEvent::RedrawRequested => {
                match state.screen {
                    AppScreen::SongSelect => {
                        state.renderer.render_song_select(
                            &state.songs,
                            state.selected_song_idx,
                            &state.score_store,
                            state.sort_mode.as_str(),
                        );

                        // Song select options bar
                        let opt_bar = format!(
                            "SPD: {:.0} (F3/F4)  MOD: {} (F7)  GAUGE: {} (F6)  OFFS: {:+.0}ms (F8/F9)  [Tab]: Options",
                            state.play_options.hi_speed,
                            state.play_options.lane_modifier.as_str(),
                            state.play_options.gauge_type.as_str(),
                            state.play_options.judge_offset_ms,
                        );
                        state.renderer.draw_footer_text(&opt_bar);

                        // If option modal is open, overlay modal on top
                        if state.show_option_modal {
                            state.renderer.render_option_modal(
                                &state.play_options,
                                state.input_config.preset.as_str(),
                                state.modal_row,
                            );
                        }
                    }
                    AppScreen::Gameplay => {
                        let audio_time = state
                            .audio_engine
                            .as_ref()
                            .map(|a| a.clock().current_time_seconds())
                            .unwrap_or(0.0);

                        let effective_judge_time = audio_time + (state.play_options.judge_offset_ms / 1000.0);

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

                        // 2. Miss updates
                        if let Some(judge) = &mut state.active_judge {
                            let misses = judge.update_misses(effective_judge_time);
                            for (_lane, miss_res) in misses {
                                state.renderer.trigger_judge(miss_res.grade, audio_time);
                            }
                        }

                        // 3. Render gameplay frame
                        if let (Some(chart), Some(timing), Some(judge)) =
                            (&state.active_chart, &state.active_timing, &state.active_judge)
                        {
                            state.renderer.render_gameplay(chart, timing, audio_time, judge.score());

                            // Check song finish
                            if audio_time > state.song_end_time + 1.5 {
                                finish_gameplay(state);
                            }
                        }

                        // 4. Footer info
                        let preset_info = match state.input_config.preset {
                            KeyPreset::HomeRow => "KEYS: [Shift]+S D F Space J K L  (F1: Switch layout | F3/F4: Speed)",
                            KeyPreset::ArcadeZx => "KEYS: [Shift]+Z S X D C F V      (F1: Switch layout | F3/F4: Speed)",
                        };
                        state.renderer.draw_footer_text(preset_info);
                    }
                    AppScreen::Result => {
                        if let (Some(chart), Some(judge)) = (&state.active_chart, &state.active_judge) {
                            state.renderer.render_result(chart, judge.score(), state.is_new_record);
                        }
                    }
                }

                // Blit to softbuffer
                let width = state.renderer.width();
                let height = state.renderer.height();
                if let (Some(w), Some(h)) = (NonZeroU32::new(width), NonZeroU32::new(height)) {
                    let _ = state.surface.resize(w, h);
                    if let Ok(mut buffer) = state.surface.buffer_mut() {
                        let src = state.renderer.data();
                        for (dst, chunk) in buffer.iter_mut().zip(src.chunks_exact(4)) {
                            let r = chunk[0] as u32;
                            let g = chunk[1] as u32;
                            let b = chunk[2] as u32;
                            *dst = (r << 16) | (g << 8) | b;
                        }
                        let _ = buffer.present();
                    }
                }

                state.window.request_redraw();
            }
            _ => (),
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }
}

fn handle_keyboard_input(
    state: &mut AppState,
    physical_key: PhysicalKey,
    key_state: ElementState,
) {
    let PhysicalKey::Code(code) = physical_key else {
        return;
    };

    // Global layout preset toggle (F1)
    if key_state == ElementState::Pressed && code == KeyCode::F1 {
        state.input_config.toggle_preset();
        return;
    }

    // Hi-Speed adjustments (F3: Speed+, F4: Speed-)
    if key_state == ElementState::Pressed {
        if code == KeyCode::F3 || code == KeyCode::PageUp {
            state.play_options.hi_speed = (state.play_options.hi_speed + 25.0).min(1200.0);
            state.renderer.skin.hi_speed = state.play_options.hi_speed;
            return;
        } else if code == KeyCode::F4 || code == KeyCode::PageDown {
            state.play_options.hi_speed = (state.play_options.hi_speed - 25.0).max(100.0);
            state.renderer.skin.hi_speed = state.play_options.hi_speed;
            return;
        } else if code == KeyCode::F6 {
            // Cycle Gauge Type
            state.play_options.gauge_type = match state.play_options.gauge_type {
                GaugeType::Easy => GaugeType::Groove,
                GaugeType::Groove => GaugeType::Hard,
                GaugeType::Hard => GaugeType::Hazard,
                GaugeType::Hazard => GaugeType::Easy,
            };
            return;
        } else if code == KeyCode::F7 {
            // Cycle Lane Modifier
            state.play_options.lane_modifier = match state.play_options.lane_modifier {
                LaneModifier::Regular => LaneModifier::Mirror,
                LaneModifier::Mirror => LaneModifier::Random,
                LaneModifier::Random => LaneModifier::RRandom,
                LaneModifier::RRandom => LaneModifier::SRandom,
                LaneModifier::SRandom => LaneModifier::Regular,
            };
            return;
        } else if code == KeyCode::F8 {
            state.play_options.judge_offset_ms = (state.play_options.judge_offset_ms - 2.0).max(-100.0);
            return;
        } else if code == KeyCode::F9 {
            state.play_options.judge_offset_ms = (state.play_options.judge_offset_ms + 2.0).min(100.0);
            return;
        }
    }

    match state.screen {
        AppScreen::SongSelect => {
            if key_state == ElementState::Pressed {
                // If option modal is open, handle modal interactions
                if state.show_option_modal {
                    match code {
                        KeyCode::Tab | KeyCode::Escape => {
                            state.show_option_modal = false;
                        }
                        KeyCode::ArrowUp | KeyCode::KeyK => {
                            state.modal_row = state.modal_row.saturating_sub(1);
                        }
                        KeyCode::ArrowDown | KeyCode::KeyJ => {
                            state.modal_row = (state.modal_row + 1).min(4);
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
                                4 => { // Key Layout
                                    state.input_config.toggle_preset();
                                }
                                _ => (),
                            }
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
                                4 => { // Key Layout
                                    state.input_config.toggle_preset();
                                }
                                _ => (),
                            }
                        }
                        _ => (),
                    }
                    return;
                }

                // Normal SongSelect interactions
                match code {
                    KeyCode::Tab | KeyCode::KeyO => {
                        state.show_option_modal = true;
                        state.modal_row = 0;
                    }
                    KeyCode::F2 => {
                        // Cycle Sort Mode
                        state.sort_mode = state.sort_mode.next();
                        sort_songs(&mut state.songs, state.sort_mode, &state.score_store);
                        state.selected_song_idx = 0;
                    }
                    KeyCode::ArrowUp | KeyCode::KeyK => {
                        if state.selected_song_idx > 0 {
                            state.selected_song_idx -= 1;
                        } else if !state.songs.is_empty() {
                            state.selected_song_idx = state.songs.len() - 1;
                        }
                    }
                    KeyCode::ArrowDown | KeyCode::KeyJ => {
                        if !state.songs.is_empty() {
                            state.selected_song_idx = (state.selected_song_idx + 1) % state.songs.len();
                        }
                    }
                    KeyCode::Enter | KeyCode::Space => {
                        if let Some(song) = state.songs.get(state.selected_song_idx).cloned() {
                            start_gameplay(state, &song);
                        }
                    }
                    KeyCode::F5 => {
                        let (mut songs, _) = init_songs_and_scores();
                        sort_songs(&mut songs, state.sort_mode, &state.score_store);
                        state.songs = songs;
                        state.selected_song_idx = 0;
                    }
                    _ => (),
                }
            }
        }
        AppScreen::Gameplay => {
            if key_state == ElementState::Pressed && code == KeyCode::Escape {
                finish_gameplay(state);
                return;
            }

            if let Some(lane) = state.input_config.map_key(physical_key) {
                let audio_time = state
                    .audio_engine
                    .as_ref()
                    .map(|a| a.clock().current_time_seconds())
                    .unwrap_or(0.0);

                let effective_judge_time = audio_time + (state.play_options.judge_offset_ms / 1000.0);

                match key_state {
                    ElementState::Pressed => {
                        state.renderer.set_key_state(lane, true);
                        if let Some(judge) = &mut state.active_judge {
                            if let Some((judge_result, wav_id)) = judge.handle_key_down(lane, effective_judge_time) {
                                state.renderer.trigger_judge(judge_result.grade, audio_time);

                                if let (Some(id), Some(audio)) = (wav_id, &mut state.audio_engine) {
                                    let _ = audio.send_command(AudioCommand::PlaySample {
                                        sample_id: id,
                                        volume: 1.0,
                                        pan: 0.0,
                                    });
                                }
                            }
                        }
                    }
                    ElementState::Released => {
                        state.renderer.set_key_state(lane, false);
                        if let Some(judge) = &mut state.active_judge {
                            if let Some(judge_result) = judge.handle_key_up(lane, effective_judge_time) {
                                state.renderer.trigger_judge(judge_result.grade, audio_time);
                            }
                        }
                    }
                }
            }
        }
        AppScreen::Result => {
            if key_state == ElementState::Pressed && (code == KeyCode::Enter || code == KeyCode::Space || code == KeyCode::Escape) {
                state.screen = AppScreen::SongSelect;
            }
        }
    }
}

fn start_gameplay(state: &mut AppState, song: &SongMetadata) {
    let (chart, timing, soundbank) = load_chart_and_audio(song);

    // Apply Lane Modifier (Mirror, Random, R-Random, S-Random)
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(42);

    let mut play_chart = chart.clone();
    play_chart.notes = apply_lane_modifier(&chart.notes, state.play_options.lane_modifier, seed);

    let judge_engine = JudgeEngine::new(&play_chart, &timing, state.play_options.gauge_type);
    let total_duration = timing.total_duration_seconds(&play_chart);

    state.renderer.skin.hi_speed = state.play_options.hi_speed;

    let audio_engine = AudioEngine::new(soundbank).ok();

    state.active_chart = Some(play_chart);
    state.active_timing = Some(timing);
    state.active_chart_hash = song.hash;
    state.active_judge = Some(judge_engine);
    state.song_end_time = total_duration;
    state.bgm_cursor = 0;
    state.is_new_record = false;
    state.audio_engine = audio_engine;
    state.screen = AppScreen::Gameplay;
}

fn finish_gameplay(state: &mut AppState) {
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

        state.is_new_record = state.score_store.update(record);
        let score_data = state.score_store.save_to_string();
        let _ = fs::write(SCORES_FILE, score_data);
    }

    state.screen = AppScreen::Result;
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

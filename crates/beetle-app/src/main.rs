mod demo;
mod input;

use std::env;
use std::fs;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::Arc;

use beetle_audio::{AudioCommand, AudioEngine, SampleBank};
use beetle_core::{parse_bms, BmsChart, GaugeType, JudgeEngine, TimingModel};
use beetle_render::{SkinConfig, SoftwareRenderer};
use input::{InputConfig, KeyPreset};
use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

struct AppState {
    window: Arc<Window>,
    _context: Context<Arc<Window>>,
    surface: Surface<Arc<Window>, Arc<Window>>,
    renderer: SoftwareRenderer,
    audio_engine: Option<AudioEngine>,
    judge_engine: JudgeEngine,
    chart: BmsChart,
    timing: TimingModel,
    input_config: InputConfig,
    bgm_cursor: usize,
}

#[derive(Default)]
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

    fn load_game_data(&self) -> (BmsChart, TimingModel, SampleBank) {
        if let Some(path_str) = &self.cli_bms_path {
            let path = Path::new(path_str);
            if path.exists() {
                if let Ok(content) = fs::read_to_string(path) {
                    if let Ok(chart) = parse_bms(&content) {
                        let timing = TimingModel::from_chart(&chart);
                        let parent_dir = path.parent().unwrap_or_else(|| Path::new("."));
                        let (soundbank, loaded) = SampleBank::load_chart_soundbank(&chart, parent_dir);
                        println!(
                            "Loaded BMS: '{}' by '{}' ({} keysounds loaded)",
                            chart.header.title, chart.header.artist, loaded
                        );
                        return (chart, timing, soundbank);
                    }
                }
            }
        }

        // Fallback to built-in demonstration chart and synthesized soundbank
        println!("Using built-in Beetle demonstration chart and synthetic keysound bank.");
        let chart = demo::create_demo_chart();
        let timing = TimingModel::from_chart(&chart);
        let soundbank = demo::create_demo_sample_bank();
        (chart, timing, soundbank)
    }
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

        let (chart, timing, soundbank) = self.load_game_data();
        let judge_engine = JudgeEngine::new(&chart, &timing, GaugeType::Groove);

        let audio_engine = match AudioEngine::new(soundbank) {
            Ok(engine) => Some(engine),
            Err(err) => {
                eprintln!("AudioEngine init warning (running in silent mode): {err}");
                None
            }
        };

        let size = window.inner_size();
        let renderer = SoftwareRenderer::new(size.width, size.height, SkinConfig::default())
            .expect("Failed to initialize software renderer");

        self.state = Some(AppState {
            window,
            _context: context,
            surface,
            renderer,
            audio_engine,
            judge_engine,
            chart,
            timing,
            input_config: InputConfig::default(),
            bgm_cursor: 0,
        });
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
                // Preset toggle (F1 or Tab)
                if key_state == ElementState::Pressed {
                    if let PhysicalKey::Code(code) = physical_key {
                        if code == KeyCode::F1 || code == KeyCode::Tab {
                            state.input_config.toggle_preset();
                            return;
                        }
                    }
                }

                // Game lane input mapping
                if let Some(lane) = state.input_config.map_key(physical_key) {
                    let audio_time = state
                        .audio_engine
                        .as_ref()
                        .map(|a| a.clock().current_time_seconds())
                        .unwrap_or(0.0);

                    match key_state {
                        ElementState::Pressed => {
                            state.renderer.set_key_state(lane, true);
                            if let Some((judge_result, wav_id)) =
                                state.judge_engine.handle_key_down(lane, audio_time)
                            {
                                state.renderer.trigger_judge(judge_result.grade, audio_time);

                                // Trigger keysound via lock-free audio queue
                                if let (Some(id), Some(audio)) = (wav_id, &mut state.audio_engine) {
                                    let _ = audio.send_command(AudioCommand::PlaySample {
                                        sample_id: id,
                                        volume: 1.0,
                                        pan: 0.0,
                                    });
                                }
                            }
                        }
                        ElementState::Released => {
                            state.renderer.set_key_state(lane, false);
                            if let Some(judge_result) =
                                state.judge_engine.handle_key_up(lane, audio_time)
                            {
                                state.renderer.trigger_judge(judge_result.grade, audio_time);
                            }
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let audio_time = state
                    .audio_engine
                    .as_ref()
                    .map(|a| a.clock().current_time_seconds())
                    .unwrap_or(0.0);

                // 1. Process BGM scheduler
                if let Some(audio) = &mut state.audio_engine {
                    while state.bgm_cursor < state.chart.bgm_notes.len() {
                        let (m, f, wav_id) = state.chart.bgm_notes[state.bgm_cursor];
                        let target_time = state.timing.beat_to_time_seconds(m, f);

                        if audio_time >= target_time {
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

                // 2. Update missed notes
                let misses = state.judge_engine.update_misses(audio_time);
                for (_lane, miss_res) in misses {
                    state.renderer.trigger_judge(miss_res.grade, audio_time);
                }

                // 3. Render gameplay frame
                let width = state.renderer.width();
                let height = state.renderer.height();

                state.renderer.render_gameplay(
                    &state.chart,
                    &state.timing,
                    audio_time,
                    state.judge_engine.score(),
                );

                // 4. Draw Input Preset Footer Info
                let preset_info = match state.input_config.preset {
                    KeyPreset::HomeRow => "KEYS: [Shift] + S D F Space J K L   (F1/Tab: Switch layout)",
                    KeyPreset::ArcadeZx => "KEYS: [Shift] + Z S X D C F V       (F1/Tab: Switch layout)",
                };
                state.renderer.draw_footer_text(preset_info);

                // 5. Present to softbuffer
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

                // Continue 60fps loop
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

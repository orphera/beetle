#![windows_subsystem = "windows"]

mod config;
mod demo;
mod devtools;
mod gameplay;
mod gpu_ui;
mod handlers;
mod input;
mod loader;
mod preview;
mod present;
mod scanner;
mod state;
mod tables;

#[cfg(not(target_os = "windows"))]
compile_error!("beetle-app is Windows-only: it renders with Direct3D 11 (ADR-026).");

use std::env;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use beetle_core::{GaugeType, LaneModifier, ScoreUpdate, SongMetadata};
use beetle_render::{SkinConfig, ViewState};
use config::{AppConfig, DisplayMode};
use gameplay::{
    finalize_start_gameplay, finish_gameplay, queue_start_gameplay, tick_gameplay,
    GameplayTickResult,
};

use handlers::{
    handle_gameplay_input, handle_key_config_input, handle_result_input, handle_song_select_input,
};
use input::KeyBindings;
use loader::spawn_background_stage_image_loader;
use beetle_render::GpuBackend;
use state::{spawn_library_load, AppScreen, AppState, LibraryJob, SongCategory};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

#[cfg(windows)]
#[link(name = "winmm")]
extern "system" {
    fn timeBeginPeriod(uPeriod: u32) -> u32;
    fn timeEndPeriod(uPeriod: u32) -> u32;
}

#[cfg(windows)]
struct MultimediaTimerGuard;

#[cfg(windows)]
impl MultimediaTimerGuard {
    pub fn new() -> Self {
        unsafe {
            timeBeginPeriod(1);
        }
        Self
    }
}

#[cfg(windows)]
impl Drop for MultimediaTimerGuard {
    fn drop(&mut self) {
        unsafe {
            timeEndPeriod(1);
        }
    }
}

#[link(name = "user32")]
extern "system" {
    fn MessageBoxW(hwnd: *mut std::ffi::c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
}

/// Shows an error dialog (the app has no console: `windows_subsystem`).
fn fatal_error(msg: &str) {
    const MB_ICONERROR: u32 = 0x10;
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (text, caption) = (wide(msg), wide("Beetle"));
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), MB_ICONERROR);
    }
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

impl ApplicationHandler for BeetleApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let saved_config = AppConfig::load();
        // The song library is read on a worker while the window and Direct3D
        // come up; the Boot screen covers the wait (INV-5).
        let library_receiver = spawn_library_load(saved_config.sort_mode);

        let window_attributes = Window::default_attributes()
            .with_title("Beetle — BMS Rhythm Engine")
            .with_inner_size(LogicalSize::new(
                saved_config.window_width,
                saved_config.window_height,
            ))
            .with_min_inner_size(LogicalSize::new(800, 600))
            .with_resizable(false);

        let window = match event_loop.create_window(window_attributes) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("Failed to create window: {e}");
                return;
            }
        };
        let size = window.inner_size();
        let skin = SkinConfig {
            hi_speed: saved_config.play_options.hi_speed,
            lane_cover_ratio: saved_config.lane_cover_ratio,
            field_position: saved_config.field_position,
            scratch_side: saved_config.scratch_side,
            ..Default::default()
        };
        let view = ViewState::new(size.width, size.height, skin);

        // Direct3D 11 is the only renderer (ADR-026); without it there is
        // nothing to draw with.
        let d3d11 = match gpu_ui::create_backend(&window, saved_config.gpu_backend) {
            Ok(d3d) => d3d,
            Err(e) => {
                fatal_error(&format!(
                    "Beetle could not start Direct3D 11 (hardware or WARP).

{e}"
                ));
                event_loop.exit();
                return;
            }
        };
        let gpu_ui = gpu_ui::GpuUi::new(view.viewport.scale);

        let mut app_state = AppState {
            window,
            view,
            audio_engine: None,
            screen: AppScreen::Boot,
            songs: Vec::new(),
            filtered_indices: Vec::new(),
            selected_song_idx: 0,
            search_query: String::new(),
            is_search_active: false,
            category_mode: SongCategory::All,
            tables: beetle_core::TableIndex::default(),
            sort_mode: saved_config.sort_mode,
            show_option_modal: false,
            show_exit_modal: false,
            should_exit_app: false,
            modal_row: 0,
            selected_key_idx: 0,
            score_store: beetle_core::ScoreStore::new(),
            play_options: saved_config.play_options,
            is_auto_play: devtools::autoplay_requested(),
            is_replay_playback: false,
            is_gameplay_paused: false,
            pause_selected_option: 0,
            current_replay: None,
            playback_replay: None,
            playback_cursor: 0,
            start_measure: 0,
            stage_image_cache: std::collections::HashMap::new(),
            bga_bank: std::collections::HashMap::new(),
            bga_cursor: 0,
            current_bga_bmp: None,
            current_layer_bmp: None,
            poor_bga_bmp: None,
            poor_until_time: 0.0,
            active_bga_image: None,
            video_players: std::collections::HashMap::new(),
            video_start_times: std::collections::HashMap::new(),
            active_chart: None,
            active_timing: None,
            active_chart_id: beetle_core::ChartId::default(),
            active_ln: None,
            active_hcn: false,
            active_judge: None,
            song_end_time: 0.0,
            score_update: ScoreUpdate::default(),
            previous_best: None,
            key_bindings: KeyBindings::load(
                &saved_config.key_layouts,
                saved_config.legacy_key_layout.as_ref(),
            ),
            rebinding: None,
            key_config_edit_mode: beetle_core::PlayMode::Keys7,
            held_keys: Vec::new(),
            master_volume: saved_config.master_volume,
            display_mode: saved_config.display_mode,
            gpu_backend: saved_config.gpu_backend,
            target_fps: saved_config.target_fps,
            track_bga: saved_config.track_bga,
            is_alt_pressed: false,
            bgm_cursor: 0,
            library_receiver: Some(library_receiver),
            library_job: LibraryJob::Startup,
            library_started_at: Instant::now(),
            loading_song: None,
            loading_receiver: None,
            loading_spinner_frame: 0,
            loading_anim_time: Instant::now(),
            loading_started_at: Instant::now(),
            result_entered_at: Instant::now(),
            last_render_time: Instant::now(),
            cursor_settle_time: Instant::now(),
            stage_image_receiver: None,
            stage_image_loading_id: None,
            preview: preview::Preview::default(),
            gpu_ui,
            capture: devtools::Capture::from_env(),
            pending_screenshot: None,
            d3d11,
            gpu_backend_at_start: saved_config.gpu_backend,
        };

        app_state.apply_display_mode();
        app_state.recompute_filtered_songs();
        (app_state.show_option_modal, app_state.show_exit_modal) = devtools::modal_requested();

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
                                || e_ext.eq_ignore_ascii_case("pms")
                            {
                                Some(e.path.clone())
                            } else {
                                None
                            }
                        })
                        .collect();

                    for entry_path in chart_entries {
                        if let Ok(bytes) = pkg.read_entry(&entry_path) {
                            let virtual_path = format!("{}::{}", path_str, entry_path);
                            if let Some(meta) = SongMetadata::from_bytes(&virtual_path, &bytes) {
                                queue_start_gameplay(&mut app_state, &meta);
                                break;
                            }
                        }
                    }
                }
            } else if let Ok(bytes) = fs::read(cli_path) {
                if let Some(meta) = SongMetadata::from_bytes(cli_path, &bytes) {
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

        if state.should_exit_app {
            state.save_config();
            event_loop.exit();
            return;
        }

        // Menus present on vblank; gameplay follows the target FPS setting
        // (60 = vsync, otherwise paced by the event loop below).
        let vsync = state.screen != AppScreen::Gameplay || state.target_fps == 60;
        state.d3d11.set_vsync(vsync);

        if state.screen != AppScreen::SongSelect {
            state.preview.stop();
        }

        state.poll_library();

        match state.screen {
            AppScreen::Boot => {
                state.window.request_redraw();
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(16),
                ));
            }
            AppScreen::Loading => {
                if let Some(rx) = &state.loading_receiver {
                    match rx.try_recv() {
                        Ok(res) => {
                            state.loading_receiver = None;
                            match res {
                                Ok((chart, timing, soundbank, bga_bank, video_sources)) => {
                                    if let Some(song) = state.loading_song.take() {
                                        finalize_start_gameplay(
                                            state,
                                            &song,
                                            chart,
                                            timing,
                                            soundbank,
                                            bga_bank,
                                            video_sources,
                                        );
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Failed to load song: {e}");
                                    state.screen = AppScreen::SongSelect;
                                    state.window.request_redraw();
                                }
                            }
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {}
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            state.loading_receiver = None;
                            state.screen = AppScreen::SongSelect;
                            state.window.request_redraw();
                        }
                    }
                }

                let now = Instant::now();
                // ~60 fps while loading: the Canvas loading screen animates
                // continuously (INV-5).
                if now.duration_since(state.loading_anim_time) >= Duration::from_millis(15) {
                    state.loading_spinner_frame = state.loading_spinner_frame.wrapping_add(1);
                    state.loading_anim_time = now;
                    state.window.request_redraw();
                }
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(16),
                ));
            }
            AppScreen::Gameplay => {
                let is_d3d_vsync = state.target_fps == 60;
                if is_d3d_vsync {
                    // D3D11 Present(1, 0) locks to hardware monitor refresh at 0% CPU jitter-free
                    state.last_render_time = Instant::now();
                    state.window.request_redraw();
                    event_loop.set_control_flow(ControlFlow::Poll);
                } else if state.target_fps == 0 {
                    state.last_render_time = Instant::now();
                    state.window.request_redraw();
                    event_loop.set_control_flow(ControlFlow::Poll);
                } else {
                    let now = Instant::now();
                    let elapsed = now.duration_since(state.last_render_time);
                    let target = Duration::from_secs_f64(1.0 / state.target_fps as f64);
                    if elapsed >= target {
                        state.last_render_time = now;
                        state.window.request_redraw();
                        let next = now + target;
                        event_loop.set_control_flow(ControlFlow::WaitUntil(next));
                    } else {
                        let next = state.last_render_time + target;
                        event_loop.set_control_flow(ControlFlow::WaitUntil(next));
                    }
                }
            }
            AppScreen::SongSelect => {
                // 1. Receive background artwork loader results without blocking UI
                if let Some(rx) = &state.stage_image_receiver {
                    if let Ok((id, img)) = rx.try_recv() {
                        state.stage_image_cache.insert(id, img);
                        state.stage_image_receiver = None;
                        state.stage_image_loading_id = None;
                        state.window.request_redraw();
                    }
                }

                // 2. Dispatch background loading ONLY if cursor has settled for at least 150ms
                let is_settled = state.cursor_settle_time.elapsed() >= Duration::from_millis(150);
                let selected_song = state.current_selected_song().cloned();
                let selected_id = selected_song.as_ref().map(|s| s.id);
                let preview_wait =
                    state.preview.update(selected_song.as_ref(), is_settled, state.master_volume);

                if !is_settled {
                    // While holding arrow key or scrolling, don't spawn background threads
                    let rem = Duration::from_millis(150)
                        .saturating_sub(state.cursor_settle_time.elapsed());
                    event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + rem));
                } else if selected_id.is_some_and(|id| !state.stage_image_cache.contains_key(&id)) {
                    if state.stage_image_loading_id != selected_id {
                        if let Some(song) = selected_song {
                            state.stage_image_loading_id = selected_id;
                            state.stage_image_receiver =
                                Some(spawn_background_stage_image_loader(&song));
                        }
                    }
                    event_loop.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                } else if state.stage_image_receiver.is_some() {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                } else {
                    if state.preview.playing_for().is_some() {
                        state.window.request_redraw();
                    }
                    event_loop.set_control_flow(match preview_wait {
                        Some(wait) => ControlFlow::WaitUntil(Instant::now() + wait),
                        None => ControlFlow::Wait,
                    });
                }
            }
            _ => {
                // Static screens (KeyConfig, and Result once its reveal
                // animation settles) only update on events (keys, resizing).
                // Result needs a brief exception: for the first
                // RESULT_REVEAL_DURATION_SECONDS after entry it's playing
                // the EX-score count-up / rank pop-in (see
                // screens/result.rs), so it needs periodic wake-ups the same
                // way SongSelect's background-loader polling above does —
                // relying on `request_redraw()` alone to escape
                // ControlFlow::Wait proved unreliable here.
                let result_animating = state.screen == AppScreen::Result
                    && state.result_entered_at.elapsed().as_secs_f64()
                        < beetle_render::RESULT_REVEAL_DURATION_SECONDS;
                if result_animating {
                    state.window.request_redraw();
                    event_loop.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                } else {
                    event_loop.set_control_flow(ControlFlow::Wait);
                }
            }
        }

        // devtools: keep frames coming until a pending capture is taken,
        // even on screens that otherwise sleep until the next input event.
        if state.capture.as_ref().is_some_and(|c| c.pending()) {
            state.window.request_redraw();
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(16),
            ));
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

        if state.should_exit_app {
            state.save_config();
            event_loop.exit();
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                state.save_config();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    state.view.resize(size.width, size.height);
                    state.d3d11.resize(size.width, size.height);
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
                                    || e_ext.eq_ignore_ascii_case("pms")
                                {
                                    Some(e.path.clone())
                                } else {
                                    None
                                }
                            })
                            .collect();

                        for entry_path in chart_entries {
                            if let Ok(bytes) = pkg.read_entry(&entry_path) {
                                let virtual_path = format!("{}::{}", path_str, entry_path);
                                if let Some(meta) = SongMetadata::from_bytes(&virtual_path, &bytes) {
                                    queue_start_gameplay(state, &meta);
                                    break;
                                }
                            }
                        }
                    }
                } else if ext.eq_ignore_ascii_case("bms")
                    || ext.eq_ignore_ascii_case("bme")
                    || ext.eq_ignore_ascii_case("bml")
                    || ext.eq_ignore_ascii_case("pms")
                {
                    if let Ok(bytes) = fs::read(&path) {
                        if let Some(meta) = SongMetadata::from_bytes(&path.to_string_lossy(), &bytes) {
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
                handle_keyboard_input(
                    state,
                    physical_key,
                    key_state,
                    repeat,
                    key_event.text.as_deref(),
                );
                state.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let size = state.window.inner_size();
                if size.width == 0 || size.height == 0 {
                    return;
                }
                match state.screen {
                    AppScreen::Gameplay => {
                        let audio_time = state
                            .audio_engine
                            .as_ref()
                            .map(|a| a.clock().current_time_seconds())
                            .unwrap_or(0.0);
                        match tick_gameplay(state, audio_time) {
                            GameplayTickResult::StageFailed => {
                                if let Some(audio) = &mut state.audio_engine {
                                    let _ = audio.stop_all();
                                }
                                finish_gameplay(state);
                                return;
                            }
                            GameplayTickResult::SongFinished => {
                                finish_gameplay(state);
                                return;
                            }
                            GameplayTickResult::Continue => {}
                        }
                        let mut visual_levels = [0.0; 16];
                        if let Some(audio) = &state.audio_engine {
                            audio.get_visual_levels(&mut visual_levels);
                        }
                        present::gameplay(state, size, audio_time, &visual_levels);
                    }
                    AppScreen::Boot => present::boot(state, size),
                    AppScreen::SongSelect => present::song_select(state, size),
                    AppScreen::Loading => present::loading(state, size),
                    AppScreen::Result => {
                        present::result(state, size);
                        // Keep frames coming while the reveal animation runs.
                        if state.result_entered_at.elapsed().as_secs_f64()
                            < beetle_render::RESULT_REVEAL_DURATION_SECONDS
                        {
                            state.window.request_redraw();
                        }
                    }
                    AppScreen::KeyConfig => present::key_config(state, size),
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

    // Track Alt key state
    if code == KeyCode::AltLeft || code == KeyCode::AltRight {
        state.is_alt_pressed = key_state == ElementState::Pressed;
    }

    // Alt + Enter to toggle Fullscreen / Windowed
    if key_state == ElementState::Pressed
        && (code == KeyCode::Enter || code == KeyCode::NumpadEnter)
        && state.is_alt_pressed
    {
        state.display_mode = match state.display_mode {
            DisplayMode::Windowed => DisplayMode::Borderless,
            DisplayMode::Borderless | DisplayMode::ExclusiveFullscreen => DisplayMode::Windowed,
        };
        state.apply_display_mode();
        state.save_config();
        return;
    }

    // Global Hotkeys (when key is pressed)
    if key_state == ElementState::Pressed && !state.is_search_active && state.rebinding.is_none() {
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
            state.play_options.judge_offset_ms =
                (state.play_options.judge_offset_ms - 2.0).max(-100.0);
            state.save_config();
            return;
        } else if code == KeyCode::F9 {
            state.play_options.judge_offset_ms =
                (state.play_options.judge_offset_ms + 2.0).min(100.0);
            state.save_config();
            return;
        }
    }

    match state.screen {
        // Nothing to do until the library is read; the window can still be closed.
        AppScreen::Boot => {}
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
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("PANIC: {info}\n");
        let _ = std::fs::write("panic.log", &msg);
        eprintln!("{msg}");
    }));

    #[cfg(windows)]
    let _timer_guard = MultimediaTimerGuard::new();

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
    use state::{filter_song_indices, init_songs_and_scores};

    #[test]
    fn test_song_category_transitions() {
        assert_eq!(SongCategory::All.next(0), SongCategory::Keys5);
        assert_eq!(SongCategory::Keys5.next(0), SongCategory::Keys7);
        assert_eq!(SongCategory::Keys7.next(0), SongCategory::Keys9);
        assert_eq!(SongCategory::Keys9.next(0), SongCategory::Keys10);
        assert_eq!(SongCategory::Keys10.next(0), SongCategory::Keys14);
        assert_eq!(SongCategory::Keys14.next(0), SongCategory::Level);
        assert_eq!(SongCategory::Level.next(0), SongCategory::ClearStatus);
        assert_eq!(SongCategory::ClearStatus.next(0), SongCategory::All);
    }

    #[test]
    fn replays_are_named_by_chart_and_long_note_rule() {
        let id = beetle_core::ChartId::of_bytes(b"x");
        let short = id.short();
        assert_eq!(state::replay_path(id, None), format!("replays/{short}.rep"));
        assert_eq!(state::replay_path(id, Some(beetle_core::LnRule::Cn)), format!("replays/{short}-cn.rep"));
        assert_eq!(state::replay_path(id, Some(beetle_core::LnRule::Ln)), format!("replays/{short}-ln.rep"));
    }

    #[test]
    fn table_folders_follow_the_built_in_ones_and_wrap_both_ways() {
        let forward: Vec<SongCategory> =
            std::iter::successors(Some(SongCategory::ClearStatus), |c| Some(c.next(2))).skip(1).take(4).collect();
        assert_eq!(
            forward,
            [SongCategory::Table(0), SongCategory::Table(1), SongCategory::All, SongCategory::Keys5]
        );
        let backward: Vec<SongCategory> =
            std::iter::successors(Some(SongCategory::Keys5), |c| Some(c.prev(2))).skip(1).take(4).collect();
        assert_eq!(
            backward,
            [SongCategory::All, SongCategory::Table(1), SongCategory::Table(0), SongCategory::ClearStatus]
        );
        // With no tables the cycle is the old one.
        assert_eq!(SongCategory::All.prev(0), SongCategory::ClearStatus);
        assert_eq!(SongCategory::ClearStatus.next(0), SongCategory::All);
    }

    fn table_with(name: &str, symbol: &str, levels_by_song: &[(u64, &str)], total: usize) -> beetle_core::DifficultyTable {
        let mut entries: Vec<beetle_core::TableEntry> = levels_by_song
            .iter()
            .map(|(n, level)| beetle_core::TableEntry {
                level: (*level).into(),
                sha256: Some(beetle_core::ChartId::synthetic(*n)),
                ..Default::default()
            })
            .collect();
        // Charts the player does not have.
        while entries.len() < total {
            entries.push(beetle_core::TableEntry {
                level: "1".into(),
                sha256: Some(beetle_core::ChartId::synthetic(1_000_000 + entries.len() as u64)),
                ..Default::default()
            });
        }
        beetle_core::DifficultyTable { name: name.into(), symbol: symbol.into(), entries, ..Default::default() }
    }

    fn song(n: u64, title: &str) -> SongMetadata {
        SongMetadata {
            id: beetle_core::ChartId::synthetic(n),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: n,
            file_path: format!("{title}.bms"),
            title: title.into(),
            subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            bpm: 120.0,
            bpm_min: 120.0,
            bpm_max: 120.0,
            play_level: 5,
            notes_count: 100,
            play_mode: beetle_core::PlayMode::Keys7,
        }
    }

    #[test]
    fn a_table_folder_lists_its_charts_by_level_then_by_the_current_sort() {
        // Songs arrive in title order; the table puts level 2 before 10 before "?".
        let songs = vec![song(1, "Apple"), song(2, "Banana"), song(3, "Cherry"), song(4, "Date"), song(5, "Elder")];
        let mut tables = beetle_core::TableIndex::new(vec![table_with(
            "Sat", "sl", &[(1, "10"), (2, "2"), (3, "?"), (4, "2")], 4,
        )]);
        tables.match_songs(songs.iter().map(|s| (s.id, s.md5)));

        let store = beetle_core::ScoreStore::new();
        let folder = filter_song_indices(&songs, "", SongCategory::Table(0), &store, &tables, beetle_core::LnOption::Cn);
        // Banana and Date are both level 2 and keep title order; Elder is not in the table.
        assert_eq!(folder, [1, 3, 0, 2]);

        // Search works inside the folder.
        let searched = filter_song_indices(&songs, "date", SongCategory::Table(0), &store, &tables, beetle_core::LnOption::Cn);
        assert_eq!(searched, [3]);
        // A table that is not installed matches nothing.
        assert!(filter_song_indices(&songs, "", SongCategory::Table(7), &store, &tables, beetle_core::LnOption::Cn).is_empty());
        // The other folders ignore tables.
        assert_eq!(filter_song_indices(&songs, "", SongCategory::All, &store, &tables, beetle_core::LnOption::Cn).len(), 5);
    }

    #[test]
    fn folder_titles_show_the_owned_count() {
        let songs: Vec<SongMetadata> = (1..=15).map(|n| song(n, &format!("S{n}"))).collect();
        let levels: Vec<(u64, &str)> = (1..=15).map(|n| (n, "1")).collect();
        let mut tables = beetle_core::TableIndex::new(vec![
            table_with("Satellite", "sl", &levels, 2467),
            table_with("A Table With A Very Long Name Indeed", "x", &levels[..1], 1),
        ]);
        tables.match_songs(songs.iter().map(|s| (s.id, s.md5)));

        assert_eq!(SongCategory::Table(0).title(&tables), "SATELLITE  15 / 2,467");
        assert_eq!(SongCategory::Table(1).title(&tables), "A TABLE WITH A VERY…  1 / 1");
        assert_eq!(SongCategory::Keys7.title(&tables), "7 KEYS");
        assert_eq!(SongCategory::Table(9).title(&tables), "ALL SONGS", "a table that is gone");
    }

    #[test]
    fn test_search_and_category_filtering() {
        let (mut songs, score_store) = init_songs_and_scores(SortMode::Title);
        songs.push(SongMetadata {
            id: beetle_core::ChartId::synthetic(101),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: 101,
            file_path: "test1.bms".to_string(),
            title: "First Anthem".to_string(),
            subtitle: "".to_string(),
            artist: "Sound Artist".to_string(),
            genre: "Trance".to_string(),
            bpm: 140.0,
            bpm_min: 140.0,
            bpm_max: 140.0,
            play_level: 5,
            notes_count: 500,
            play_mode: beetle_core::PlayMode::Keys7,
        });
        songs.push(SongMetadata {
            id: beetle_core::ChartId::synthetic(102),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: 102,
            file_path: "test2.bms".to_string(),
            title: "Second Beat".to_string(),
            subtitle: "".to_string(),
            artist: "DJ Beat".to_string(),
            genre: "Hardcore".to_string(),
            bpm: 180.0,
            bpm_min: 180.0,
            bpm_max: 180.0,
            play_level: 10,
            notes_count: 1200,
            play_mode: beetle_core::PlayMode::Keys5,
        });

        // 1. Initial unfiltered indices
        let all_indices = filter_song_indices(&songs, "", SongCategory::All, &score_store, &beetle_core::TableIndex::default(), beetle_core::LnOption::Cn);
        assert!(all_indices.len() >= 2);

        // 2. Filter by title "anthem"
        let anthem_indices = filter_song_indices(&songs, "anthem", SongCategory::All, &score_store, &beetle_core::TableIndex::default(), beetle_core::LnOption::Cn);
        assert_eq!(anthem_indices.len(), 1);
        let match_song = &songs[anthem_indices[0]];
        assert_eq!(match_song.title, "First Anthem");

        // 3. Filter by artist "dj beat"
        let artist_indices =
            filter_song_indices(&songs, "dj beat", SongCategory::All, &score_store, &beetle_core::TableIndex::default(), beetle_core::LnOption::Cn);
        assert_eq!(artist_indices.len(), 1);
        assert_eq!(songs[artist_indices[0]].title, "Second Beat");

        // 4. Filter by genre "hardcore"
        let genre_indices =
            filter_song_indices(&songs, "hardcore", SongCategory::All, &score_store, &beetle_core::TableIndex::default(), beetle_core::LnOption::Cn);
        assert_eq!(genre_indices.len(), 1);
        assert_eq!(songs[genre_indices[0]].title, "Second Beat");

        // 5. Non-matching search query
        let empty_indices =
            filter_song_indices(&songs, "nonexistentxyz", SongCategory::All, &score_store, &beetle_core::TableIndex::default(), beetle_core::LnOption::Cn);
        assert_eq!(empty_indices.len(), 0);

        // 6. Strict play mode category filter tests
        let test_songs = vec![
            SongMetadata {
                id: beetle_core::ChartId::synthetic(1),
                md5: [0; 16],
                ln_count: 0,
                ln_mode: None,
                legacy_hash: 1,
                file_path: "pms_song.pms".to_string(),
                title: "Popn Track".to_string(),
                subtitle: "".to_string(),
                artist: "".to_string(),
                genre: "".to_string(),
                bpm: 150.0,
                bpm_min: 150.0,
                bpm_max: 150.0,
                play_level: 5,
                notes_count: 500,
                play_mode: beetle_core::PlayMode::Keys9,
            },
            SongMetadata {
                id: beetle_core::ChartId::synthetic(2),
                md5: [0; 16],
                ln_count: 0,
                ln_mode: None,
                legacy_hash: 2,
                file_path: "dp_10k.bms".to_string(),
                title: "10K DP Track".to_string(),
                subtitle: "".to_string(),
                artist: "".to_string(),
                genre: "".to_string(),
                bpm: 160.0,
                bpm_min: 160.0,
                bpm_max: 160.0,
                play_level: 8,
                notes_count: 800,
                play_mode: beetle_core::PlayMode::Keys10,
            },
            SongMetadata {
                id: beetle_core::ChartId::synthetic(3),
                md5: [0; 16],
                ln_count: 0,
                ln_mode: None,
                legacy_hash: 3,
                file_path: "dp_14k.bme".to_string(),
                title: "14K DP Track".to_string(),
                subtitle: "".to_string(),
                artist: "".to_string(),
                genre: "".to_string(),
                bpm: 180.0,
                bpm_min: 180.0,
                bpm_max: 180.0,
                play_level: 12,
                notes_count: 1400,
                play_mode: beetle_core::PlayMode::Keys14,
            },
        ];

        let keys9_indices = filter_song_indices(&test_songs, "", SongCategory::Keys9, &score_store, &beetle_core::TableIndex::default(), beetle_core::LnOption::Cn);
        assert_eq!(keys9_indices.len(), 1);
        assert_eq!(
            test_songs[keys9_indices[0]].play_mode,
            beetle_core::PlayMode::Keys9
        );

        let keys10_indices =
            filter_song_indices(&test_songs, "", SongCategory::Keys10, &score_store, &beetle_core::TableIndex::default(), beetle_core::LnOption::Cn);
        assert_eq!(keys10_indices.len(), 1);
        assert_eq!(
            test_songs[keys10_indices[0]].play_mode,
            beetle_core::PlayMode::Keys10
        );

        let keys14_indices =
            filter_song_indices(&test_songs, "", SongCategory::Keys14, &score_store, &beetle_core::TableIndex::default(), beetle_core::LnOption::Cn);
        assert_eq!(keys14_indices.len(), 1);
        assert_eq!(
            test_songs[keys14_indices[0]].play_mode,
            beetle_core::PlayMode::Keys14
        );
    }

    #[test]
    fn test_resolve_bga_priority() {
        use beetle_core::BmpId;
        use state::resolve_bga_id;
        let (base, poor) = (Some(BmpId(1)), Some(BmpId(2)));
        let all = |_| true;

        // Nothing scheduled: stage image fallback.
        assert_eq!(resolve_bga_id(0.0, None, None, all, 1.0), None);
        // Base BGA channel active.
        assert_eq!(resolve_bga_id(0.0, None, base, all, 2.0), Some(BmpId(1)));
        // POOR image overrides it during the miss penalty window...
        assert_eq!(resolve_bga_id(3.0, poor, base, all, 2.5), Some(BmpId(2)));
        // ...and the base BGA returns once the window expires.
        assert_eq!(resolve_bga_id(3.0, poor, base, all, 3.5), Some(BmpId(1)));
        // A POOR image that failed to load does not blank the BGA.
        assert_eq!(resolve_bga_id(3.0, poor, base, |id| id == BmpId(1), 2.5), Some(BmpId(1)));
    }
}

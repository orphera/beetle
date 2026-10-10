#![windows_subsystem = "windows"]

mod calibration;
mod config;
mod devtools;
mod filters;
mod folders;
mod gameplay;
mod gpu_ui;
mod handlers;
mod ime;
mod input;
mod loader;
mod options_table;
mod present;
mod preview;
mod raw_input;
mod scanner;
mod state;
mod tables;
mod transition;

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
    finalize_start_gameplay, finish_gameplay, leave_gameplay, queue_start_gameplay, tick_gameplay,
    GameplayTickResult, END_BANNER_SECONDS,
};

use beetle_render::{strings, GpuBackend, ToastKind};
use handlers::{
    handle_calibration_input, handle_gameplay_input, handle_key_config_input, handle_result_input,
    handle_settings_input, handle_song_select_input,
};
use input::KeyBindings;
use loader::spawn_background_stage_image_loader;
use state::{spawn_library_load, AppScreen, AppState, LibraryJob};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, DeviceEvents, EventLoop};
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
    fn MessageBoxW(
        hwnd: *mut std::ffi::c_void,
        text: *const u16,
        caption: *const u16,
        kind: u32,
    ) -> i32;
}

/// Shows an error dialog (the app has no console: `windows_subsystem`).
fn fatal_error(msg: &str) {
    const MB_ICONERROR: u32 = 0x10;
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (text, caption) = (wide(msg), wide(strings::APP_NAME));
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_ICONERROR,
        );
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
            .with_title(strings::WINDOW_TITLE)
            .with_inner_size(LogicalSize::new(
                saved_config.window_width,
                saved_config.window_height,
            ))
            .with_min_inner_size(LogicalSize::new(800, 600))
            // Shown once the first frame is drawn, so no unpainted window is seen.
            .with_visible(false)
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
            hi_speed: beetle_render::green_ms_to_px_per_sec(
                saved_config.play_options.green_ms as f32,
                saved_config.lane_cover_ratio,
            ),
            lane_cover_ratio: saved_config.lane_cover_ratio,
            field_position: saved_config.field_position,
            scratch_sides: saved_config.scratch_sides,
            eight_k_form: saved_config.eight_k_form,
            ..Default::default()
        };
        let view = ViewState::new(size.width, size.height, skin);

        // Direct3D 11 is the only renderer (ADR-026); without it there is
        // nothing to draw with.
        let d3d11 = match gpu_ui::create_backend(&window, saved_config.gpu_backend) {
            Ok(d3d) => d3d,
            Err(e) => {
                fatal_error(&strings::fill(strings::FATAL_D3D11, &[&e.to_string()]));
                event_loop.exit();
                return;
            }
        };
        let gpu_ui = gpu_ui::GpuUi::new(view.viewport.scale);

        // Lane keys are read on their own thread (see `raw_input`). Raw
        // input has one target per process, so winit's is switched off first.
        event_loop.listen_device_events(DeviceEvents::Never);
        let raw_keys = raw_input::RawKeyboard::spawn_for(&window);

        let mut app_state = AppState {
            window,
            view,
            raw_keys,
            audio_engine: None,
            screen: AppScreen::Boot,
            songs: Vec::new(),
            folder_path: saved_config.folder_path.clone(),
            folder_tree: Vec::new(),
            entries: Vec::new(),
            selected_entry: 0,
            list_scroll: 0,
            chart_choice: saved_config.chart_choices.clone(),
            filter: saved_config.filter.clone(),
            filter_focus: None,
            sort_menu: None,
            present_modes: Vec::new(),
            level_steps: Vec::new(),
            result_count: None,
            search_query: String::new(),
            search_preedit: String::new(),
            is_search_active: false,
            ime_caret_sent: None,
            tables: beetle_core::TableIndex::default(),
            sort_mode: saved_config.sort_mode,
            show_option_modal: false,
            show_exit_modal: false,
            show_help: false,
            drop_hover: false,
            should_exit_app: false,
            modal_row: 0,
            settings_row: 0,
            calibration: None,
            key_config_return: AppScreen::SongSelect,
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
            bga_enabled: saved_config.bga_enabled,
            key_hint: saved_config.key_hint,
            gameplay_readout: None,
            gameplay_end: None,
            play_end: crate::gameplay::PlayEndGuard::default(),
            gauge_trend: beetle_core::GaugeTrend::default(),
            is_alt_pressed: false,
            bgm_cursor: 0,
            autoplay_sound_until: f64::NEG_INFINITY,
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
            screen_entry: transition::ScreenEntry::new(AppScreen::Boot, Instant::now()),
            toast: None,
            anim_tail: false,
            d3d11,
            gpu_backend_at_start: saved_config.gpu_backend,
            cursor: None,
            wheel_carry: 0.0,
        };

        app_state.apply_display_mode();
        app_state.recompute_entries();
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

        // A hidden window gets no redraw events, so draw the first frame here;
        // presenting it shows the window.
        if app_state.screen == AppScreen::Boot {
            let size = app_state.window.inner_size();
            present::boot(&mut app_state, size);
        }
        app_state.window.set_visible(true);

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

        drain_raw_keys(state);
        ime::close_search_if_unavailable(state);
        state.sync_screen_entry();

        // Menus present on vblank; gameplay follows the target FPS setting
        // (60 = vsync, otherwise paced by the event loop below).
        let vsync = state.screen != AppScreen::Gameplay || state.target_fps == 60;
        state.d3d11.set_vsync(vsync);

        if state.screen != AppScreen::SongSelect {
            state.preview.stop();
        }
        // The calibration lives on the Settings screen only.
        if state.screen != AppScreen::Settings {
            state.calibration = None;
        }
        if let Some(cal) = &mut state.calibration {
            cal.tick();
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
                                    eprintln!("Failed to load chart: {e:?}");
                                    fail_song_load(state);
                                }
                            }
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {}
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            state.loading_receiver = None;
                            fail_song_load(state);
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
                    state
                        .preview
                        .update(selected_song.as_ref(), is_settled, state.master_volume);

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
                let calibrating = state.calibration.as_ref().is_some_and(|c| c.is_running());
                if calibrating || result_animating {
                    state.window.request_redraw();
                    event_loop.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                } else {
                    event_loop.set_control_flow(ControlFlow::Wait);
                }
            }
        }

        // Fade-ins and toasts animate on the menus; once they end, one more
        // frame draws the cleared state before the loop sleeps again.
        let now = Instant::now();
        // The screen may have changed in the match above (library poll).
        state.sync_screen_entry();
        if state.presentation_animating(now) {
            state.anim_tail = true;
            state.window.request_redraw();
            event_loop.set_control_flow(ControlFlow::WaitUntil(now + transition::FRAME));
        } else if state.anim_tail {
            state.anim_tail = false;
            if state.screen != AppScreen::Gameplay {
                state.window.request_redraw();
                event_loop.set_control_flow(ControlFlow::WaitUntil(now + transition::FRAME));
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
            // The drag overlay shows on song select only (see `present::song_select`).
            WindowEvent::HoveredFile(_) => {
                state.drop_hover = true;
                state.window.request_redraw();
            }
            WindowEvent::HoveredFileCancelled => {
                state.drop_hover = false;
                state.window.request_redraw();
            }
            WindowEvent::DroppedFile(path) => {
                state.drop_hover = false;
                open_dropped_file(state, &path);
            }
            WindowEvent::CursorMoved { position, .. } => {
                state.cursor = Some((position.x as f32, position.y as f32));
                if is_menu_screen(state.screen) {
                    state.window.request_redraw();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                state.cursor = None;
                if is_menu_screen(state.screen) {
                    state.window.request_redraw();
                }
            }
            WindowEvent::MouseInput {
                state: button_state,
                button: MouseButton::Left,
                ..
            } => {
                if button_state == ElementState::Pressed {
                    handlers::mouse::handle_press(state);
                }
                if is_menu_screen(state.screen) {
                    state.window.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                handlers::mouse::handle_wheel(state, delta);
                if is_menu_screen(state.screen) {
                    state.window.request_redraw();
                }
            }
            WindowEvent::Ime(ime_event) => {
                ime::handle_ime(state, ime_event);
                state.window.request_redraw();
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
                // Lane keys that came in before this one go first.
                drain_raw_keys(state);
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
                        // Every key that arrived so far is judged before the
                        // tick below can count its note as missed.
                        drain_raw_keys(state);
                        let audio_time = state
                            .audio_engine
                            .as_ref()
                            .map(|a| a.clock().current_time_seconds())
                            .unwrap_or(0.0);
                        // The end banner: the play is saved and judged; wait
                        // for its time (or ENTER / ESC) and then show the result.
                        if let Some(end) = state.gameplay_end {
                            if end.started.elapsed().as_secs_f64() >= END_BANNER_SECONDS {
                                leave_gameplay(state);
                                return;
                            }
                        } else {
                            match tick_gameplay(state, audio_time) {
                                GameplayTickResult::StageFailed => {
                                    if let Some(audio) = &mut state.audio_engine {
                                        let _ = audio.stop_all();
                                    }
                                    finish_gameplay(state);
                                }
                                GameplayTickResult::SongFinished => finish_gameplay(state),
                                GameplayTickResult::Continue => {}
                            }
                        }
                        let mut visual_levels = [0.0; 16];
                        if let Some(audio) = &state.audio_engine {
                            audio.get_visual_levels(&mut visual_levels);
                        }
                        present::gameplay(state, size, audio_time, &visual_levels);
                        if state.gameplay_end.is_some() {
                            // The banner animates and times out on its own.
                            state.window.request_redraw();
                        }
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
                    AppScreen::Settings => present::settings(state, size),
                }
            }
            _ => (),
        }
    }
}

/// Starts a play from a dropped `.bmsp` package or chart file. Anything else,
/// or a file that does not open or has no playable chart, gets an error toast.
/// Back to Song Select after the chart of the song being loaded failed to load.
/// Nothing was played or scored; the Error toast names the file.
fn fail_song_load(state: &mut AppState) {
    if let Some(song) = state.loading_song.take() {
        transition::show_toast(
            state,
            ToastKind::Error,
            loader::chart_load_failure_message(&song),
        );
    }
    state.screen = AppScreen::SongSelect;
    state.window.request_redraw();
}

fn open_dropped_file(state: &mut AppState, path: &Path) {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let is_chart = ["bms", "bme", "bml", "pms"]
        .iter()
        .any(|x| ext.eq_ignore_ascii_case(x));
    let started = if ext.eq_ignore_ascii_case("bmsp") {
        start_dropped_package(state, path)
    } else if is_chart {
        fs::read(path)
            .ok()
            .and_then(|bytes| SongMetadata::from_bytes(&path.to_string_lossy(), &bytes))
            .map(|meta| queue_start_gameplay(state, &meta))
            .is_some()
    } else {
        transition::show_toast(state, ToastKind::Error, strings::TOAST_UNSUPPORTED_FILE);
        return;
    };
    if !started {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        transition::show_toast(
            state,
            ToastKind::Error,
            strings::fill(strings::TOAST_OPEN_FAILED, &[&name]),
        );
    }
}

/// Queues the first chart of a dropped package. `false` when it does not open
/// or holds no chart that parses.
fn start_dropped_package(state: &mut AppState, path: &Path) -> bool {
    let Ok(mut pkg) = bms_package::PackageReader::open_file(path) else {
        return false;
    };
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
                return true;
            }
        }
    }
    false
}

/// Hands the keys queued by the raw input thread to the play (or the judge
/// offset calibration), each timed on the audio clock at the moment it
/// arrived, however late this runs. Elsewhere they are dropped: the menus
/// read winit's key events.
fn drain_raw_keys(state: &mut AppState) {
    while let Some(key) = state.raw_keys.as_mut().and_then(|r| r.pop()) {
        match state.screen {
            AppScreen::Gameplay if !handlers::gameplay::is_gameplay_hotkey(key.code) => {
                let audio_time = gameplay::audio_time_at(state, key.at);
                handlers::gameplay::handle_lane_key(state, key.code, key.down, audio_time);
            }
            AppScreen::Settings if key.down && state.calibration.is_some() => {
                handlers::settings::calibration_tap(state, key.code, key.at);
            }
            _ => {}
        }
    }
}

/// Screens that draw a menu and take the mouse (gameplay and loading do not).
fn is_menu_screen(screen: AppScreen) -> bool {
    matches!(
        screen,
        AppScreen::SongSelect | AppScreen::Result | AppScreen::KeyConfig | AppScreen::Settings
    )
}

fn handle_keyboard_input(
    state: &mut AppState,
    physical_key: PhysicalKey,
    key_state: ElementState,
    repeat: bool,
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

    ime::close_search_if_unavailable(state);

    // Global Hotkeys (when key is pressed)
    // Not during the judge offset calibration: its cancel must leave the
    // play options as they were.
    if key_state == ElementState::Pressed
        && !state.is_search_active
        && state.rebinding.is_none()
        && state.calibration.is_none()
    {
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
        AppScreen::Settings if state.calibration.is_some() => {
            handle_calibration_input(state, key_state, code, physical_key, repeat)
        }
        AppScreen::Settings => handle_settings_input(state, key_state, code),
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

    #[test]
    fn replays_are_named_by_chart_and_long_note_rule() {
        let id = beetle_core::ChartId::of_bytes(b"x");
        let short = id.short();
        assert_eq!(state::replay_path(id, None), format!("replays/{short}.rep"));
        assert_eq!(
            state::replay_path(id, Some(beetle_core::LnRule::Cn)),
            format!("replays/{short}-cn.rep")
        );
        assert_eq!(
            state::replay_path(id, Some(beetle_core::LnRule::Ln)),
            format!("replays/{short}-ln.rep")
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
        assert_eq!(
            resolve_bga_id(3.0, poor, base, |id| id == BmpId(1), 2.5),
            Some(BmpId(1))
        );
    }
}

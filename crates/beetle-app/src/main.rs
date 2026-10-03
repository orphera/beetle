#![windows_subsystem = "windows"]

mod config;
mod demo;
mod devtools;
mod gameplay;
#[cfg(target_os = "windows")]
mod gpu_ui;
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

use beetle_core::{GaugeType, LaneModifier, SongMetadata};
use beetle_render::{SkinConfig, SoftwareRenderer};
use config::{AppConfig, DisplayMode};
use gameplay::{
    finalize_start_gameplay, finish_gameplay, queue_start_gameplay, tick_gameplay,
    GameplayTickResult,
};

use handlers::{
    handle_gameplay_input, handle_key_config_input, handle_result_input, handle_song_select_input,
};
use input::{InputConfig, KeyPreset};
use loader::spawn_background_stage_image_loader;
use softbuffer::{Context, Surface};
#[cfg(target_os = "windows")]
use gpu_ui::{bga_texture, gameplay_bga_texture};
use state::{init_songs_and_scores, AppScreen, AppState, SongCategory, REPLAYS_DIR};
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
        let context = match Context::new(window.clone()) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to create softbuffer context: {e}");
                return;
            }
        };
        let mut surface = match Surface::new(&context, window.clone()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to create softbuffer surface: {e}");
                return;
            }
        };

        let size = window.inner_size();
        if let (Some(w), Some(h)) = (
            NonZeroU32::new(size.width.max(1)),
            NonZeroU32::new(size.height.max(1)),
        ) {
            let _ = surface.resize(w, h);
        }

        let skin = SkinConfig {
            hi_speed: saved_config.play_options.hi_speed,
            lane_cover_ratio: saved_config.lane_cover_ratio,
            ..Default::default()
        };

        let renderer = match SoftwareRenderer::new(size.width, size.height, skin) {
            Some(r) => r,
            None => {
                eprintln!("Failed to initialize software renderer");
                return;
            }
        };

        let (songs, score_store) = init_songs_and_scores(saved_config.sort_mode);

        #[cfg(target_os = "windows")]
        let (d3d11_backend, d3d11_frame_texture, gpu_ui) = {
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            let mut backend = None;
            let mut texture = None;
            let mut ui = None;
            if saved_config.gpu_backend != config::GpuBackendSetting::Software {
                if let Ok(handle) = window.window_handle() {
                    if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
                        let hwnd = win32_handle.hwnd.get() as *mut std::ffi::c_void;
                        if let Ok(mut d3d) =
                            beetle_render::D3d11Backend::new(hwnd, size.width, size.height)
                        {
                            use beetle_render::GpuBackend;
                            texture = d3d.create_texture(size.width, size.height, renderer.data());
                            ui = Some(gpu_ui::GpuUi::new(renderer.viewport.scale));
                            backend = Some(d3d);
                        }
                    }
                }
            }
            (backend, texture, ui)
        };

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
            show_exit_modal: false,
            should_exit_app: false,
            modal_row: 0,
            selected_key_idx: 0,
            score_store,
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
            display_mode: saved_config.display_mode,
            gpu_backend: saved_config.gpu_backend,
            target_fps: saved_config.target_fps,
            track_bga: saved_config.track_bga,
            is_alt_pressed: false,
            bgm_cursor: 0,
            loading_song: None,
            loading_receiver: None,
            loading_spinner_frame: 0,
            loading_anim_time: Instant::now(),
            result_entered_at: Instant::now(),
            last_render_time: Instant::now(),
            cursor_settle_time: Instant::now(),
            stage_image_receiver: None,
            stage_image_loading_hash: None,
            is_dirty: true,
            #[cfg(target_os = "windows")]
            gpu_ui,
            capture: devtools::Capture::from_env(),
            #[cfg(target_os = "windows")]
            d3d11_backend,
            #[cfg(target_os = "windows")]
            d3d11_frame_texture,
        };

        app_state.apply_display_mode();
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
                            let content = beetle_core::decode_bms_text(&bytes);
                            let virtual_path = format!("{}::{}", path_str, entry_path);
                            if let Some(meta) = SongMetadata::from_content(&virtual_path, &content)
                            {
                                queue_start_gameplay(&mut app_state, &meta);
                                break;
                            }
                        }
                    }
                }
            } else if let Ok(bytes) = fs::read(cli_path) {
                let content = beetle_core::decode_bms_text(&bytes);
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

        if state.should_exit_app {
            state.save_config();
            event_loop.exit();
            return;
        }

        match state.screen {
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
                if now.duration_since(state.loading_anim_time) >= Duration::from_millis(30) {
                    state.loading_spinner_frame = state.loading_spinner_frame.wrapping_add(1);
                    state.loading_anim_time = now;
                    state.window.request_redraw();
                }
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(16),
                ));
            }
            AppScreen::Gameplay => {
                let is_d3d_vsync = {
                    #[cfg(target_os = "windows")]
                    {
                        state.is_d3d11_active() && state.target_fps == 60
                    }
                    #[cfg(not(target_os = "windows"))]
                    {
                        false
                    }
                };

                #[cfg(target_os = "windows")]
                if let Some(d3d11) = &mut state.d3d11_backend {
                    d3d11.set_vsync(is_d3d_vsync);
                }

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
                #[cfg(target_os = "windows")]
                if let Some(d3d11) = &mut state.d3d11_backend {
                    d3d11.set_vsync(true);
                }

                // 1. Receive background artwork loader results without blocking UI
                if let Some(rx) = &state.stage_image_receiver {
                    if let Ok((hash, img)) = rx.try_recv() {
                        state.stage_image_cache.insert(hash, img);
                        state.stage_image_receiver = None;
                        state.stage_image_loading_hash = None;
                        state.mark_dirty();
                        state.window.request_redraw();
                    }
                }

                // 2. Dispatch background loading ONLY if cursor has settled for at least 150ms
                let is_settled = state.cursor_settle_time.elapsed() >= Duration::from_millis(150);
                let selected_song = state.current_selected_song().cloned();
                let selected_hash = selected_song.as_ref().map(|s| s.hash).unwrap_or(0);

                if !is_settled {
                    // While holding arrow key or scrolling, don't spawn background threads
                    let rem = Duration::from_millis(150)
                        .saturating_sub(state.cursor_settle_time.elapsed());
                    event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + rem));
                } else if selected_hash != 0
                    && !state.stage_image_cache.contains_key(&selected_hash)
                {
                    if state.stage_image_loading_hash != Some(selected_hash) {
                        if let Some(song) = selected_song {
                            state.stage_image_loading_hash = Some(selected_hash);
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
                    event_loop.set_control_flow(ControlFlow::Wait);
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
                if let (Some(w), Some(h)) =
                    (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                {
                    let _ = state.surface.resize(w, h);
                    state.renderer.resize(size.width, size.height);
                    #[cfg(target_os = "windows")]
                    if let Some(d3d11) = &mut state.d3d11_backend {
                        use beetle_render::GpuBackend;
                        d3d11.resize(size.width, size.height);
                        if let Some(old_tex) = state.d3d11_frame_texture.take() {
                            d3d11.destroy_texture(old_tex);
                        }
                        state.d3d11_frame_texture =
                            d3d11.create_texture(size.width, size.height, state.renderer.data());
                    }
                    state.mark_dirty();
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
                                let content = beetle_core::decode_bms_text(&bytes);
                                let virtual_path = format!("{}::{}", path_str, entry_path);
                                if let Some(meta) =
                                    SongMetadata::from_content(&virtual_path, &content)
                                {
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
                        let content = beetle_core::decode_bms_text(&bytes);
                        if let Some(meta) =
                            SongMetadata::from_content(&path.to_string_lossy(), &content)
                        {
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
                state.mark_dirty();
                state.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let audio_time = state
                    .audio_engine
                    .as_ref()
                    .map(|a| a.clock().current_time_seconds())
                    .unwrap_or(0.0);

                let mut visual_levels = [0.0; 16];
                if let Some(audio) = &state.audio_engine {
                    audio.get_visual_levels(&mut visual_levels);
                }

                match state.screen {
                    AppScreen::SongSelect => {
                        if state.is_dirty {
                            let selected_hash =
                                state.current_selected_song().map(|s| s.hash).unwrap_or(0);
                            let visible_songs = state.current_visible_songs();
                            let stage_img = state
                                .stage_image_cache
                                .get(&selected_hash)
                                .and_then(|opt| opt.as_ref());
                            // Check replay existence for selected song
                            let has_replay = state
                                .current_selected_song()
                                .map(|s| {
                                    Path::new(&format!("{}/{:016x}.rep", REPLAYS_DIR, s.hash))
                                        .exists()
                                })
                                .unwrap_or(false);

                            // Song select options bar (rendered as the footer's second line)
                            let rep_str = if has_replay { "  [R]: Replay" } else { "" };
                            let auto_str = if state.is_auto_play {
                                "[AUTO: ON]"
                            } else {
                                "[AUTO: OFF]"
                            };
                            let opt_bar = format!(
                                "SPD: {:.0} (F3/F4)  MOD: {} (F7)  GAUGE: {} (F6)  {}{}  [Tab]: Options  [A]: AutoPlay",
                                state.play_options.hi_speed,
                                state.play_options.lane_modifier.as_str(),
                                state.play_options.gauge_type.as_str(),
                                auto_str,
                                rep_str,
                            );

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
                                &opt_bar,
                            );

                            // If option modal is open, overlay modal on top
                            if state.show_option_modal {
                                state.renderer.render_option_modal(
                                    &state.play_options,
                                    state.input_config.preset.as_str(),
                                    state.is_auto_play,
                                    state.start_measure,
                                    state.master_volume,
                                    state.display_mode.as_str(),
                                    state.current_resolution_label(),
                                    state.gpu_backend.as_str(),
                                    state.target_fps,
                                    state.track_bga.as_str(),
                                    state.modal_row,
                                );
                            }

                            // If exit confirmation modal is open, overlay modal on top
                            if state.show_exit_modal {
                                state.renderer.render_exit_confirm_modal();
                            }
                        }
                    }
                    AppScreen::Loading => {
                        let selected_hash =
                            state.loading_song.as_ref().map(|s| s.hash).unwrap_or(0);
                        let stage_img = state
                            .stage_image_cache
                            .get(&selected_hash)
                            .and_then(|opt| opt.as_ref());

                        let title = state
                            .loading_song
                            .as_ref()
                            .map(|s| s.title.as_str())
                            .unwrap_or("Unknown");
                        let artist = state
                            .loading_song
                            .as_ref()
                            .map(|s| s.artist.as_str())
                            .unwrap_or("Unknown");
                        let genre = state
                            .loading_song
                            .as_ref()
                            .map(|s| s.genre.as_str())
                            .unwrap_or("");

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
                        let active_bga = state::resolve_bga_hierarchy(
                            state.poor_until_time,
                            state.poor_bga_bmp,
                            state.current_bga_bmp,
                            &state.bga_bank,
                            &state.video_players,
                            state.active_bga_image.as_ref(),
                            audio_time,
                        );

                        let active_layer = state.current_layer_bmp.and_then(|id| {
                            if let Some(vp) = state.video_players.get(&id) {
                                vp.current_frame()
                            } else {
                                state.bga_bank.get(&id)
                            }
                        });

                        // Software rendering path: only needed when GPU pipeline is inactive or when gameplay is paused
                        let is_gpu_gameplay = {
                            #[cfg(target_os = "windows")]
                            {
                                state.is_d3d11_active() && state.gpu_ui.is_some()
                            }
                            #[cfg(not(target_os = "windows"))]
                            {
                                false
                            }
                        };
                        let should_render_software = !is_gpu_gameplay;
                        if should_render_software {
                            if let (Some(chart), Some(judge), Some(timing)) = (
                                &state.active_chart,
                                &state.active_judge,
                                &state.active_timing,
                            ) {
                                state.renderer.render_gameplay(
                                    chart,
                                    judge.notes(),
                                    audio_time,
                                    judge.score(),
                                    &visual_levels,
                                    active_bga,
                                    active_layer,
                                    state.track_bga.opacity(),
                                    timing,
                                );
                            }

                            // Overlay Pause Modal if active
                            if state.is_gameplay_paused {
                                let title = state
                                    .active_chart
                                    .as_ref()
                                    .map(|c| c.header.title.as_str())
                                    .unwrap_or("Unknown");
                                let artist = state
                                    .active_chart
                                    .as_ref()
                                    .map(|c| c.header.artist.as_str())
                                    .unwrap_or("Unknown");
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
                                        KeyPreset::Pms9K => "KEYS: S D F Space J K L ; '      (F1: Layout | 1/2: Speed | F10/F11: Cover | Esc: Pause)",
                                        KeyPreset::DoublePlay => "KEYS: [Shift]+ZSXDCFV / [RShift]+UIOP[]\\  (F1: Layout | 1/2: Speed | F10/F11: Cover | Esc: Pause)",
                                        KeyPreset::Custom => "KEYS: Custom Key Layout Active    (F1: Layout | 1/2: Speed | F10/F11: Cover | Esc: Pause)",
                                    }
                                };
                                state.renderer.draw_footer_text(footer_text);
                            }
                        }
                    }
                    AppScreen::Result => {
                        let elapsed = state.result_entered_at.elapsed().as_secs_f64();
                        let animating = elapsed < beetle_render::RESULT_REVEAL_DURATION_SECONDS;
                        // Result is otherwise a static dirty-flag screen (see
                        // AppState::mark_dirty), unlike Gameplay's per-frame
                        // driver, so keep rendering past the dirty-flag reset
                        // below while the score count-up / rank reveal is
                        // still running.
                        if state.is_dirty || animating {
                            if let (Some(chart), Some(judge)) =
                                (&state.active_chart, &state.active_judge)
                            {
                                state.renderer.render_result(
                                    chart,
                                    judge.score(),
                                    state.is_new_record,
                                    state.previous_best.as_ref(),
                                    elapsed,
                                );
                            }
                            if animating {
                                state.window.request_redraw();
                            }
                        }
                    }
                    AppScreen::KeyConfig => {
                        if state.is_dirty {
                            let lanes = state.key_config_lanes();
                            let key_names: Vec<(&'static str, String)> = lanes
                                .iter()
                                .map(|&lane| {
                                    (
                                        crate::input::lane_label(lane),
                                        state.input_config.get_key_name_for_lane(lane),
                                    )
                                })
                                .collect();
                            state.renderer.render_key_config(
                                &key_names,
                                state.selected_key_idx,
                                state.input_config.preset.as_str(),
                                state.is_rebinding_key,
                            );
                        }
                    }
                }

                // Frame presentation: Hardware Direct3D 11 FLIP swapchain or CPU softbuffer
                let width = state.renderer.width();
                let height = state.renderer.height();
                #[cfg(target_os = "windows")]
                let mut presented_d3d11 = false;
                #[cfg(target_os = "windows")]
                if state.is_d3d11_active() && width > 0 && height > 0 {
                    use beetle_render::GpuBackend;
                    if let Some(d3d11) = &mut state.d3d11_backend {
                        // 1. Canvas UI path (ADR-026): screens already ported to `Ui`.
                        if state.screen == AppScreen::Gameplay {
                            if let (Some(gpu), Some(chart), Some(judge), Some(timing)) = (
                                &mut state.gpu_ui,
                                &state.active_chart,
                                &state.active_judge,
                                &state.active_timing,
                            ) {
                                d3d11.begin_frame(width, height, [0.0, 0.0, 0.0, 1.0]);
                                let bga = gameplay_bga_texture(
                                    gpu,
                                    d3d11,
                                    &state.bga_bank,
                                    &state.video_players,
                                    state.poor_until_time,
                                    state.poor_bga_bmp,
                                    state.current_bga_bmp,
                                    state.active_bga_image.as_ref(),
                                    state.active_chart_hash,
                                    audio_time,
                                );
                                let layer = state.current_layer_bmp.and_then(|id| {
                                    bga_texture(gpu, d3d11, &state.bga_bank, &state.video_players, id, true)
                                });

                                state.renderer.clean_expired_hit_bursts(audio_time);
                                let key_pressed = *state.renderer.key_pressed();
                                let (badge, hint) = gameplay_badge_and_hint(
                                    state.is_replay_playback,
                                    state.is_auto_play,
                                    state.input_config.preset,
                                );
                                let vp = state.renderer.viewport;
                                gpu.ui.lite = d3d11.is_warp();
                                gpu.ui.begin(width, height, vp.scale);
                                beetle_render::draw_gameplay(
                                    &mut gpu.ui,
                                    &beetle_render::PlayFrame {
                                        viewport: &vp,
                                        layout: &state.renderer.skin,
                                        chart,
                                        notes: judge.notes(),
                                        timing,
                                        score: judge.score(),
                                        audio_time,
                                        song_length: state.song_end_time,
                                        visual_levels: &visual_levels,
                                        bga,
                                        layer,
                                        track_bga_opacity: state.track_bga.opacity(),
                                        key_pressed: &key_pressed,
                                        hit_bursts: state.renderer.hit_bursts(),
                                        last_judge: state.renderer.last_judge(),
                                        hint,
                                        badge,
                                        pause: state
                                            .is_gameplay_paused
                                            .then_some(state.pause_selected_option),
                                    },
                                );
                                gpu.ui.end(d3d11);
                                if let Some(cap) = &mut state.capture {
                                    if cap.on_frame(state.screen, d3d11) {
                                        state.should_exit_app = true;
                                    }
                                }
                                d3d11.end_frame();
                                presented_d3d11 = true;
                            }
                        }

                        // 2. UI, Modal, and Fallback screens via SoftwareRenderer texture
                        if !presented_d3d11 {
                            d3d11.begin_frame(width, height, [0.0, 0.0, 0.0, 1.0]);
                            if let Some(tex_id) = state.d3d11_frame_texture {
                                if state.is_dirty
                                    || state.screen == AppScreen::Loading
                                    || state.is_gameplay_paused
                                {
                                    d3d11.update_texture(
                                        tex_id,
                                        width,
                                        height,
                                        state.renderer.data(),
                                    );
                                }
                                let w = width as f32;
                                let h = height as f32;
                                let quad_vertices = [
                                    beetle_render::Vertex2D::new(
                                        0.0,
                                        0.0,
                                        0.0,
                                        0.0,
                                        [1.0, 1.0, 1.0, 1.0],
                                    ),
                                    beetle_render::Vertex2D::new(
                                        w,
                                        0.0,
                                        1.0,
                                        0.0,
                                        [1.0, 1.0, 1.0, 1.0],
                                    ),
                                    beetle_render::Vertex2D::new(
                                        w,
                                        h,
                                        1.0,
                                        1.0,
                                        [1.0, 1.0, 1.0, 1.0],
                                    ),
                                    beetle_render::Vertex2D::new(
                                        0.0,
                                        h,
                                        0.0,
                                        1.0,
                                        [1.0, 1.0, 1.0, 1.0],
                                    ),
                                ];
                                let quad_indices = [0, 1, 2, 0, 2, 3];
                                d3d11.draw_batch(
                                    &quad_vertices,
                                    &quad_indices,
                                    Some(tex_id),
                                    beetle_render::BlendMode::Alpha,
                                );
                            }
                            if let Some(cap) = &mut state.capture {
                                if cap.on_frame(state.screen, d3d11) {
                                    state.should_exit_app = true;
                                }
                            }
                            d3d11.end_frame();
                            presented_d3d11 = true;
                        }
                    }
                }

                #[cfg(not(target_os = "windows"))]
                let presented_d3d11 = false;

                if !presented_d3d11
                    && width > 0
                    && height > 0
                    && (state.is_dirty
                        || state.screen == AppScreen::Gameplay
                        || state.screen == AppScreen::Loading)
                {
                    if let Ok(mut buffer) = state.surface.buffer_mut() {
                        let data = state.renderer.data();
                        let buffer_slice = buffer.as_mut();
                        for (dest, src) in buffer_slice.iter_mut().zip(data.chunks_exact(4)) {
                            *dest =
                                ((src[0] as u32) << 16) | ((src[1] as u32) << 8) | (src[2] as u32);
                        }
                        let _ = buffer.present();
                    }
                }

                state.is_dirty = false;
            }
            _ => (),
        }
    }
}

/// Mode badge and key-hint line for the gameplay HUD.
fn gameplay_badge_and_hint(
    is_replay: bool,
    is_auto: bool,
    preset: KeyPreset,
) -> (Option<&'static str>, &'static str) {
    if is_replay {
        return (Some("REPLAY"), "ESC  Return to song select");
    }
    if is_auto {
        return (Some("AUTO PLAY"), "ESC  Return to song select");
    }
    let hint = match preset {
        KeyPreset::HomeRow => "KEYS  Shift+S D F Space J K L    1/2 SPEED    F10/F11 COVER    ESC PAUSE",
        KeyPreset::ArcadeZx => "KEYS  Shift+Z S X D C F V    1/2 SPEED    F10/F11 COVER    ESC PAUSE",
        KeyPreset::Pms9K => "KEYS  S D F Space J K L ; '    1/2 SPEED    F10/F11 COVER    ESC PAUSE",
        KeyPreset::DoublePlay => "KEYS  Shift+ZSXDCFV / RShift+UIOP[]\\    1/2 SPEED    ESC PAUSE",
        KeyPreset::Custom => "KEYS  Custom layout    1/2 SPEED    F10/F11 COVER    ESC PAUSE",
    };
    (None, hint)
}

fn handle_keyboard_input(
    state: &mut AppState,
    physical_key: PhysicalKey,
    key_state: ElementState,
    _repeat: bool,
    text: Option<&str>,
) {
    state.mark_dirty();

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
        let artist_indices =
            filter_song_indices(&songs, "dj beat", SongCategory::All, &score_store);
        assert_eq!(artist_indices.len(), 1);
        assert_eq!(songs[artist_indices[0]].title, "Second Beat");

        // 4. Filter by genre "hardcore"
        let genre_indices =
            filter_song_indices(&songs, "hardcore", SongCategory::All, &score_store);
        assert_eq!(genre_indices.len(), 1);
        assert_eq!(songs[genre_indices[0]].title, "Second Beat");

        // 5. Non-matching search query
        let empty_indices =
            filter_song_indices(&songs, "nonexistentxyz", SongCategory::All, &score_store);
        assert_eq!(empty_indices.len(), 0);

        // 6. Strict play mode category filter tests
        let test_songs = vec![
            SongMetadata {
                hash: 1,
                file_path: "pms_song.pms".to_string(),
                title: "Popn Track".to_string(),
                subtitle: "".to_string(),
                artist: "".to_string(),
                genre: "".to_string(),
                bpm: 150.0,
                play_level: 5,
                notes_count: 500,
                play_mode: beetle_core::PlayMode::Keys9,
            },
            SongMetadata {
                hash: 2,
                file_path: "dp_10k.bms".to_string(),
                title: "10K DP Track".to_string(),
                subtitle: "".to_string(),
                artist: "".to_string(),
                genre: "".to_string(),
                bpm: 160.0,
                play_level: 8,
                notes_count: 800,
                play_mode: beetle_core::PlayMode::Keys10,
            },
            SongMetadata {
                hash: 3,
                file_path: "dp_14k.bme".to_string(),
                title: "14K DP Track".to_string(),
                subtitle: "".to_string(),
                artist: "".to_string(),
                genre: "".to_string(),
                bpm: 180.0,
                play_level: 12,
                notes_count: 1400,
                play_mode: beetle_core::PlayMode::Keys14,
            },
        ];

        let keys9_indices = filter_song_indices(&test_songs, "", SongCategory::Keys9, &score_store);
        assert_eq!(keys9_indices.len(), 1);
        assert_eq!(
            test_songs[keys9_indices[0]].play_mode,
            beetle_core::PlayMode::Keys9
        );

        let keys10_indices =
            filter_song_indices(&test_songs, "", SongCategory::Keys10, &score_store);
        assert_eq!(keys10_indices.len(), 1);
        assert_eq!(
            test_songs[keys10_indices[0]].play_mode,
            beetle_core::PlayMode::Keys10
        );

        let keys14_indices =
            filter_song_indices(&test_songs, "", SongCategory::Keys14, &score_store);
        assert_eq!(keys14_indices.len(), 1);
        assert_eq!(
            test_songs[keys14_indices[0]].play_mode,
            beetle_core::PlayMode::Keys14
        );
    }

    #[test]
    fn test_resolve_active_bga_hierarchy() {
        use beetle_render::{ColorRgba, ImageBuffer};
        use state::resolve_bga_hierarchy;
        use std::collections::HashMap;

        let static_stage = ImageBuffer::new(320, 180, ColorRgba::new(10, 10, 10, 255));
        let bmp_base = ImageBuffer::new(320, 180, ColorRgba::new(50, 50, 50, 255));
        let bmp_poor = ImageBuffer::new(320, 180, ColorRgba::new(255, 0, 0, 255));

        let mut bga_bank = HashMap::new();
        bga_bank.insert(beetle_core::BmpId(1), bmp_base);
        bga_bank.insert(beetle_core::BmpId(2), bmp_poor);
        let video_players = HashMap::new();

        // 1. Initial state: Static stage artwork fallback
        let bga = resolve_bga_hierarchy(
            0.0,
            None,
            None,
            &bga_bank,
            &video_players,
            Some(&static_stage),
            1.0,
        )
        .unwrap();
        assert_eq!(bga.pixels[0], ColorRgba::new(10, 10, 10, 255));

        // 2. Base BGA channel active
        let bga = resolve_bga_hierarchy(
            0.0,
            None,
            Some(beetle_core::BmpId(1)),
            &bga_bank,
            &video_players,
            Some(&static_stage),
            2.0,
        )
        .unwrap();
        assert_eq!(bga.pixels[0], ColorRgba::new(50, 50, 50, 255));

        // 3. POOR BGA override active during miss penalty window
        let bga = resolve_bga_hierarchy(
            3.0,
            Some(beetle_core::BmpId(2)),
            Some(beetle_core::BmpId(1)),
            &bga_bank,
            &video_players,
            Some(&static_stage),
            2.5,
        )
        .unwrap();
        assert_eq!(bga.pixels[0], ColorRgba::new(255, 0, 0, 255));

        // 4. After POOR window expires (t = 3.5), reverts back to Base BGA
        let bga = resolve_bga_hierarchy(
            3.0,
            Some(beetle_core::BmpId(2)),
            Some(beetle_core::BmpId(1)),
            &bga_bank,
            &video_players,
            Some(&static_stage),
            3.5,
        )
        .unwrap();
        assert_eq!(bga.pixels[0], ColorRgba::new(50, 50, 50, 255));
    }
}

#![windows_subsystem = "windows"]

mod calibration;
mod config;
mod devtools;
mod filters;
mod folders;
mod gameplay;
mod gpu_ui;
mod handlers;
mod hid;
mod ime;
mod input;
mod install;
mod lane_logic;
mod loader;
mod menu_audio;
mod options_table;
mod present;
mod preview;
mod raw_input;
mod result_reveal;
mod scanner;
mod song_folders;
mod state;
mod tables;
mod thumbs;
mod transition;
mod turntable;
mod xinput;

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
use input::{KeyBindings, Source};
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
    let (text, caption) = (wide(msg), wide(strings::APP_NAME()));
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

        // The Windows UI language is used until the config is read (and by the
        // error dialog below if the config never loads).
        strings::set_lang(crate::config::os_language());
        let saved_config = AppConfig::load();
        // `BEETLE_LANG` (devtools) overrides the saved choice for this run only.
        strings::set_lang(
            crate::devtools::lang_override()
                .or(saved_config.language)
                .unwrap_or_else(crate::config::os_language),
        );
        // The song library is read on a worker while the window and Direct3D
        // come up; the Boot screen covers the wait (INV-5).
        let library_receiver = spawn_library_load(saved_config.sort_mode);

        let window_attributes = Window::default_attributes()
            .with_title(strings::WINDOW_TITLE())
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
            note_size: saved_config.note_size,
            judge_position: saved_config.judge_position,
            ..Default::default()
        };
        let mut view = ViewState::new(size.width, size.height, skin);
        view.set_ui_scale(saved_config.ui_scale);

        // Direct3D 11 is the only renderer (ADR-026); without it there is
        // nothing to draw with.
        let d3d11 = match gpu_ui::create_backend(&window, saved_config.gpu_backend) {
            Ok(d3d) => d3d,
            Err(e) => {
                fatal_error(&strings::fill(strings::FATAL_D3D11(), &[&e.to_string()]));
                event_loop.exit();
                return;
            }
        };
        let gpu_ui = gpu_ui::GpuUi::new(view.ui.scale);

        // Lane keys are read on their own thread (see `raw_input`). Raw
        // input has one target per process, so winit's is switched off first.
        event_loop.listen_device_events(DeviceEvents::Never);
        let raw_keys = raw_input::RawKeyboard::spawn_for(&window, saved_config.turntable_axis);

        let mut app_state = AppState {
            window,
            view,
            raw_keys,
            pad: xinput::XInputPad::load(),
            turntable_axis: saved_config.turntable_axis,
            lane_session: None,
            lane_session_seq: 0,
            audio_engine: None,
            screen: AppScreen::Boot,
            songs: Vec::new(),
            folder_path: saved_config.folder_path.clone(),
            folder_tree: Vec::new(),
            entries: Vec::new(),
            selected_entry: 0,
            list_scroll: 0,
            list_scroll_visual: 0.0,
            motion_time: Instant::now(),
            motion_selection: None,
            selected_since: None,
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
            settings_scroll: [0.0; 2],
            calibration: None,
            key_config_return: AppScreen::SongSelect,
            settings_return: AppScreen::SongSelect,
            title_row: 0,
            title_on_boot: devtools::title_after_boot(self.cli_bms_path.is_some()),
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
            stage_backdrop_cache: std::collections::HashMap::new(),
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
            first_note_time: None,
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
            bgm_volume: saved_config.bgm_volume,
            key_volume: saved_config.key_volume,
            sfx_volume: saved_config.sfx_volume,
            visual_offset_ms: saved_config.visual_offset_ms,
            reduce_flashes: saved_config.reduce_flashes,
            reduce_motion: saved_config.reduce_motion,
            high_contrast: saved_config.high_contrast_notes,
            language: saved_config.language,
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
            rescan_return: AppScreen::SongSelect,
            pending_uninstalled_toast: None,
            library_started_at: Instant::now(),
            loading_song: None,
            loading_receiver: None,
            loading_spinner_frame: 0,
            loading_anim_time: Instant::now(),
            loading_started_at: Instant::now(),
            result_entered_at: Instant::now(),
            result_reveal_skipped: false,
            result_sounds: result_reveal::RevealSounds::default(),
            last_render_time: Instant::now(),
            cursor_settle_time: Instant::now(),
            stage_image_receiver: None,
            stage_image_loading_id: None,
            thumbs: thumbs::ThumbLoader::new(),
            jacket_busy: false,
            preview: preview::Preview::default(),
            menu_audio: menu_audio::MenuAudio::default(),
            gpu_ui,
            capture: devtools::Capture::from_env(),
            pending_screenshot: None,
            screen_entry: transition::ScreenEntry::new(AppScreen::Boot, Instant::now()),
            toast: None,
            anim_tail: false,
            ambient_frame_at: Instant::now(),
            d3d11,
            gpu_backend_at_start: saved_config.gpu_backend,
            cursor: None,
            wheel_carry: 0.0,
            last_library_mtime: None,
            installs: Default::default(),
            folder_pick: None,
            song_folders: None,
            linked_folders: 0,
        };

        app_state.apply_display_mode();
        if app_state.capture.is_some() {
            devtools::log(&format!("present: {}", app_state.d3d11.present_mode()));
        }
        app_state.recompute_entries();
        (app_state.show_option_modal, app_state.show_exit_modal) = devtools::modal_requested();
        menu_audio::sync_with_screen(&mut app_state);
        for path in devtools::dropped_paths() {
            install::queue(&mut app_state, path);
        }

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

        drain_lane_events(state);
        sync_lane_session(state);
        drain_raw_keys(state);
        // The pad is polled here, not on redraw: a menu that sleeps until
        // input (or a timer) still hears the pad (see the wake-ups below).
        poll_pad(state);
        ime::close_search_if_unavailable(state);
        state.sync_screen_entry();

        // Menus present on vblank; gameplay follows the target FPS setting
        // (60 = vsync, otherwise paced by the event loop below).
        let vsync = state.screen != AppScreen::Gameplay || state.target_fps == 60;
        state.d3d11.set_vsync(vsync);

        if state.screen != AppScreen::SongSelect {
            state.preview.release(Instant::now());
        }
        // The calibration lives on the Settings screen only.
        if state.screen != AppScreen::Settings {
            state.calibration = None;
        }
        if let Some(cal) = &mut state.calibration {
            cal.tick();
        }

        state.poll_library();
        song_folders::poll(state);
        install::poll(state);

        // Set by each branch that already wakes on its own frame-rate timer,
        // so the menu backdrop drift below does not change that cadence.
        let mut timed = false;
        // Set by a branch that sleeps until input (no timer of its own).
        let mut idle = false;
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
                    if let Ok((id, img, backdrop)) = rx.try_recv() {
                        if let Some(px) = backdrop {
                            state.stage_backdrop_cache.insert(id, px);
                        }
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
                        .update(selected_song.as_ref(), Instant::now(), state.bgm_level());

                if !is_settled {
                    timed = true;
                    // While holding arrow key or scrolling, don't spawn background threads
                    let rem = Duration::from_millis(150)
                        .saturating_sub(state.cursor_settle_time.elapsed());
                    event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + rem));
                } else if selected_id.is_some_and(|id| !state.stage_image_cache.contains_key(&id)) {
                    timed = true;
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
                    timed = true;
                    event_loop.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                } else {
                    if state.preview.playing_for().is_some() {
                        state.window.request_redraw();
                    }
                    // A short preview wait (fade step, loop restart) must not be
                    // stretched by the ambient frame below.
                    timed = holds_frame_timer(preview_wait);
                    idle = preview_wait.is_none();
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
                let result_animating =
                    state.screen == AppScreen::Result && result_reveal::running(state);
                let calibrating = state.calibration.as_ref().is_some_and(|c| c.is_running());
                // A preview fading out after leaving song select needs ticks
                // until its stream is closed.
                if calibrating || result_animating || state.preview.is_fading() {
                    timed = true;
                    state.window.request_redraw();
                    event_loop.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                } else {
                    idle = true;
                    event_loop.set_control_flow(ControlFlow::Wait);
                }
            }
        }

        // Fade-ins, toasts and the song list's easing animate on the menus;
        // once they end, one more frame draws the settled state before the
        // loop sleeps again.
        let now = Instant::now();
        // The screen may have changed in the match above (library poll).
        state.sync_screen_entry();
        // Gameplay keeps its own pacing (it redraws every frame), so the
        // Gameplay reveal does not override it with a 16 ms timer.
        let presentation_timed =
            state.screen != AppScreen::Gameplay && state.presentation_animating(now);
        if presentation_timed || state.song_select_motion_active(now) {
            state.anim_tail = true;
            timed = true;
            state.window.request_redraw();
            event_loop.set_control_flow(ControlFlow::WaitUntil(now + transition::FRAME));
        } else if state.anim_tail {
            state.anim_tail = false;
            if state.screen != AppScreen::Gameplay {
                timed = true;
                state.window.request_redraw();
                event_loop.set_control_flow(ControlFlow::WaitUntil(now + transition::FRAME));
            }
        }
        // Menu backdrops drift on a capped timer while focused. Existing
        // animations keep their own cadence, so the drift only fills in where
        // nothing else is timed.
        // Song jackets still decoding for the list: keep frames coming so
        // finished thumbnails appear without waiting for input.
        if state.gpu_ui.ui.menu().backdrop.is_fading(now)
            || (state.screen == AppScreen::SongSelect && state.jacket_busy)
        {
            timed = true;
            state.window.request_redraw();
            event_loop.set_control_flow(ControlFlow::WaitUntil(now + transition::FRAME));
        }
        // Songs being added: the worker's progress and its end need a look
        // even on a screen that sleeps until input (the title).
        if state.installs.busy() && !timed {
            timed = true;
            event_loop.set_control_flow(ControlFlow::WaitUntil(now + Duration::from_millis(100)));
        }
        // The menu's own wake-ups, each only ever shortening the one set above
        // except the slot scan, which is set only on an idle screen.
        let focused = state.window.has_focus();
        let minimized = state.window.is_minimized().unwrap_or(false);
        let drift =
            match ambient_frame_step(state.screen, focused, minimized, state.d3d11.is_warp()) {
                Some(step) if !timed => {
                    if now >= state.ambient_frame_at {
                        state.ambient_frame_at = now + step;
                        state.window.request_redraw();
                    }
                    Some(state.ambient_frame_at)
                }
                _ => None,
            };
        let pad = state.pad.as_ref();
        let connected = pad.is_some_and(|p| p.is_connected());
        let scan = match pad {
            Some(p) if !connected && idle && state.screen != AppScreen::Gameplay => {
                Some(p.next_scan(now))
            }
            _ => None,
        };
        let menu_focused = is_menu_screen(state.screen) && focused && !minimized;
        let pad_step = pad_wake(now, connected, menu_focused, scan);
        if let Some(wake) = soonest(drift, pad_step) {
            event_loop.set_control_flow(ControlFlow::WaitUntil(wake));
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
            WindowEvent::Focused(true) => {
                if let Some(pad) = state.pad.as_mut() {
                    pad.resync();
                }
                if is_menu_screen(state.screen) {
                    crate::handlers::song_select::check_autofocus_rescan(state);
                }
            }
            WindowEvent::Focused(false) => {}
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
                        drain_lane_events(state);
                        drain_raw_keys(state);
                        // The banner freezes the frame on the song's last audio
                        // time (the clock keeps running while the tails play).
                        let audio_time = match state.gameplay_end {
                            Some(end) => end.audio_time,
                            None => state
                                .audio_engine
                                .as_ref()
                                .map(|a| a.clock().current_time_seconds())
                                .unwrap_or(0.0),
                        };
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
                    AppScreen::Title => present::title(state, size),
                    AppScreen::SongSelect => present::song_select(state, size),
                    AppScreen::Loading => present::loading(state, size),
                    AppScreen::Result => {
                        present::result(state, size);
                        // Keep frames coming while the reveal runs (60 fps), then idle.
                        if result_reveal::running(state) {
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

/// A dropped chart opens at once; folders, archives and packages are added
/// (see `install.rs`).
fn open_dropped_file(state: &mut AppState, path: &Path) {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let is_chart = ["bms", "bme", "bml", "pms"]
        .iter()
        .any(|x| ext.eq_ignore_ascii_case(x));
    if install::is_installable(path) {
        install::queue(state, path.to_path_buf());
        return;
    }
    let started = if is_chart {
        fs::read(path)
            .ok()
            .and_then(|bytes| SongMetadata::from_bytes(&path.to_string_lossy(), &bytes))
            .map(|meta| queue_start_gameplay(state, &meta))
            .is_some()
    } else {
        transition::show_toast(state, ToastKind::Error, strings::TOAST_UNSUPPORTED_FILE());
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
            strings::fill(strings::TOAST_OPEN_FAILED(), &[&name]),
        );
    }
}

/// Hands the keys queued by the raw input thread to the play (or the judge
/// offset calibration), each timed on the audio clock at the moment it
/// arrived, however late this runs. Elsewhere they are dropped: the menus
/// read winit's key events.
fn drain_raw_keys(state: &mut AppState) {
    while let Some(key) = state.raw_keys.as_mut().and_then(|r| r.pop()) {
        match state.screen {
            AppScreen::Gameplay => {
                let hotkey = matches!(key.source, Source::Key(code)
                    if handlers::gameplay::is_gameplay_hotkey(code));
                if !hotkey {
                    let audio_time = gameplay::audio_time_at(state, key.at);
                    handlers::gameplay::handle_lane_key(state, key.source, key.down, audio_time);
                }
            }
            // A pressed controller button answers the "press a key" prompt.
            AppScreen::KeyConfig if state.rebinding.is_some() => {
                if let (Source::Pad { button, .. }, true) = (key.source, key.down) {
                    handlers::key_config::set_pad(state, button, key.at);
                }
            }
            AppScreen::Settings if key.down && state.calibration.is_some() => {
                if let Source::Key(code) = key.source {
                    handlers::settings::calibration_tap(state, code, key.at);
                }
            }
            _ => {}
        }
    }
}

/// Runs the menu actions of the pad (`xinput`) read this frame, each as the
/// keyboard action it stands for: a press and its release. Not while the
/// window is in the background, nor while a key is being rebound (a pad
/// button must not bind Enter or Escape by accident).
fn poll_pad(state: &mut AppState) {
    // The pad is polled every time, so its state stays current; only the
    // events are dropped where a pad may not act.
    let accept =
        state.screen != AppScreen::Boot && state.rebinding.is_none() && state.window.has_focus();
    // A running play takes no menu input, so it does not scan for a pad.
    let scan = !(state.screen == AppScreen::Gameplay && !state.is_gameplay_paused);
    let context = match state.screen {
        AppScreen::Gameplay if state.is_gameplay_paused => xinput::PadContext::PausedPlay,
        AppScreen::Gameplay => xinput::PadContext::Play,
        _ => xinput::PadContext::Menu,
    };
    let Some(pad) = state.pad.as_mut() else {
        return;
    };
    let inputs = pad.poll(std::time::Instant::now(), scan, accept);
    if inputs.is_empty() {
        return;
    }
    for input in inputs {
        if let Some(code) = xinput::key_for(input, context) {
            let key = PhysicalKey::Code(code);
            handle_keyboard_input(state, key, ElementState::Pressed, false, None);
            handle_keyboard_input(state, key, ElementState::Released, false, None);
        }
    }
    state.window.request_redraw();
}

/// The pad's wake-up for a frame: 16 ms steps while a connected pad is on a
/// focused menu; the next slot scan while none is connected (`next_scan`,
/// given only on an idle screen). `None` when the pad needs no wake.
fn pad_wake(
    now: Instant,
    connected: bool,
    menu_focused: bool,
    next_scan: Option<Instant>,
) -> Option<Instant> {
    if connected {
        menu_focused.then(|| now + PAD_FRAME)
    } else {
        next_scan
    }
}

/// The earlier of two wake-ups, where either may be absent.
fn soonest(a: Option<Instant>, b: Option<Instant>) -> Option<Instant> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// The pad's polling step on a focused menu: the menus' input latency.
const PAD_FRAME: Duration = Duration::from_millis(16);

/// Makes the judge calls of the input thread's play on the game's engine,
/// which is what is drawn and scored (`lane_logic`). Their sounds have
/// already started. Calls of an earlier play, or after this one ended, are
/// dropped.
fn drain_lane_events(state: &mut AppState) {
    while let Some(event) = state.raw_keys.as_mut().and_then(|r| r.pop_event()) {
        let current = state.lane_session.is_some_and(|s| s.id == event.session);
        if !current || state.gameplay_end.is_some() {
            continue;
        }
        match event.call {
            lane_logic::JudgeCall::Key {
                lane,
                down,
                audio_time,
                judge_time,
            } => {
                handlers::gameplay::judge_lane(state, lane, down, audio_time, judge_time);
            }
            lane_logic::JudgeCall::Misses {
                audio_time,
                judge_time,
            } => gameplay::apply_misses(state, audio_time, judge_time),
        }
    }
}

/// Tells the input thread when its play pauses, resumes, changes its judge
/// offset or ends (leaving gameplay, or the end banner).
fn sync_lane_session(state: &mut AppState) {
    let Some(sync) = state.lane_session else {
        return;
    };
    let Some(keys) = &mut state.raw_keys else {
        return;
    };
    if state.screen != AppScreen::Gameplay || state.gameplay_end.is_some() {
        keys.send(lane_logic::LogicCommand::End);
        state.lane_session = None;
        return;
    }
    let mut next = sync;
    if sync.paused != state.is_gameplay_paused
        && keys.send(lane_logic::LogicCommand::Pause(state.is_gameplay_paused))
    {
        next.paused = state.is_gameplay_paused;
    }
    let offset = state.play_options.judge_offset_ms;
    if sync.offset_ms != offset && keys.send(lane_logic::LogicCommand::Offset(offset)) {
        next.offset_ms = offset;
    }
    state.lane_session = Some(next);
}

/// Screens that draw a menu and take the mouse (gameplay and loading do not).
fn is_menu_screen(screen: AppScreen) -> bool {
    matches!(
        screen,
        AppScreen::Title
            | AppScreen::SongSelect
            | AppScreen::Result
            | AppScreen::KeyConfig
            | AppScreen::Settings
    )
}

/// Redraw interval for the drifting menu backdrop: about 30 fps.
const AMBIENT_FRAME: Duration = Duration::from_millis(33);

/// The redraw interval the drifting backdrop needs right now, or `None` when
/// nothing should redraw on a timer. Only focused, visible, full-quality menus
/// drift. Lite (WARP) is static by design. Boot and Loading already redraw
/// every 16 ms for their own entrance and spinner, so they are left alone.
fn ambient_frame_step(
    screen: AppScreen,
    focused: bool,
    minimized: bool,
    lite: bool,
) -> Option<Duration> {
    (is_menu_screen(screen) && focused && !minimized && !lite).then_some(AMBIENT_FRAME)
}

/// Whether a requested wake-up is short enough that the ambient frame must
/// leave the timer alone: anything at or under `AMBIENT_FRAME` already keeps the
/// backdrop drifting, and a longer one is not lengthened by this rule.
fn holds_frame_timer(wait: Option<Duration>) -> bool {
    wait.is_some_and(|w| w <= AMBIENT_FRAME)
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
        AppScreen::Title => handlers::handle_title_input(state, key_state, code),
        AppScreen::SongSelect => handle_song_select_input(state, key_state, code, text),
        AppScreen::Loading => {
            if key_state == ElementState::Pressed && code == KeyCode::Escape {
                state.loading_receiver = None;
                state.loading_song = None;
                state.screen = AppScreen::SongSelect;
            }
        }
        AppScreen::Gameplay => handle_gameplay_input(state, key_state, code, physical_key),
        AppScreen::Result => handle_result_input(state, key_state, code, repeat),
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
    fn a_connected_pad_steps_every_16_ms_on_a_focused_menu_only() {
        let now = Instant::now();
        assert_eq!(pad_wake(now, true, true, None), Some(now + PAD_FRAME));
        // Not focused, or not a menu: the pad does not need its own wake.
        assert_eq!(pad_wake(now, true, false, None), None);
        // A connected pad never scans.
        assert_eq!(
            pad_wake(now, true, true, Some(now + Duration::from_secs(1))),
            Some(now + PAD_FRAME)
        );
    }

    #[test]
    fn with_no_pad_the_only_wake_is_the_slot_scan() {
        let now = Instant::now();
        let scan = now + Duration::from_secs(1);
        assert_eq!(pad_wake(now, false, true, Some(scan)), Some(scan));
        assert_eq!(pad_wake(now, false, true, None), None);
    }

    #[test]
    fn combined_wakes_never_lengthen_an_earlier_one() {
        let now = Instant::now();
        let fast = Some(now + PAD_FRAME);
        let slow = Some(now + Duration::from_millis(500));
        assert_eq!(soonest(fast, slow), fast);
        assert_eq!(soonest(slow, fast), fast);
        assert_eq!(soonest(None, slow), slow);
        assert_eq!(soonest(None, None), None);
        // The 16 ms step shortens a long wait (a preview's), and drift at 33 ms
        // does not lengthen the pad step.
        let drift = Some(now + AMBIENT_FRAME);
        assert_eq!(soonest(drift, fast), fast);
    }

    #[test]
    fn ambient_drift_runs_only_on_focused_full_quality_menus() {
        let step = Some(AMBIENT_FRAME);
        for screen in [
            AppScreen::Title,
            AppScreen::SongSelect,
            AppScreen::Result,
            AppScreen::KeyConfig,
            AppScreen::Settings,
        ] {
            assert_eq!(ambient_frame_step(screen, true, false, false), step);
            assert_eq!(ambient_frame_step(screen, false, false, false), None);
            assert_eq!(ambient_frame_step(screen, true, true, false), None);
            assert_eq!(ambient_frame_step(screen, true, false, true), None);
        }
        // Boot and Loading keep their own 16 ms cadence; gameplay is paced separately.
        for screen in [AppScreen::Boot, AppScreen::Loading, AppScreen::Gameplay] {
            assert_eq!(ambient_frame_step(screen, true, false, false), None);
        }
        assert!(AMBIENT_FRAME <= Duration::from_millis(34));
    }

    #[test]
    fn a_short_preview_wait_is_not_lengthened_by_the_ambient_frame() {
        assert!(holds_frame_timer(Some(Duration::from_millis(16))));
        assert!(holds_frame_timer(Some(AMBIENT_FRAME)));
        assert!(!holds_frame_timer(Some(Duration::from_millis(500))));
        assert!(!holds_frame_timer(None));
    }

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

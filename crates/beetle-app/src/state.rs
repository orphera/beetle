use std::fs;
use std::path::Path;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Instant;

use beetle_audio::AudioEngine;
use beetle_core::{
    sort_songs, BmsChart, ChartId, JudgeEngine, Lane, LnOption, LnRule, PlayMode, PlayOptions,
    ReplayData, ScoreRecord, ScoreStore, ScoreUpdate, SongMetadata, SortMode, TableIndex,
    TimingModel,
};
use beetle_render::{strings, EightKForm, ImageBuffer, Rect, ViewState};
use winit::window::Window;

use crate::config::{AppConfig, DisplayMode, GpuBackendSetting};
use crate::folders::{self, Folder, FolderPath, ListEntry};
use crate::input::KeyPreset;
use crate::scanner::{load_or_scan_songs, DEFAULT_SONGS_DIR};

pub const SCORES_FILE: &str = "scores.dat";
pub const REPLAYS_DIR: &str = "replays";
/// How far ahead of the audio clock BGM and auto-play key sounds are handed
/// to the mixer, which starts each on its exact frame. Longer than a frame at
/// the lowest frame rate, so the tick never has to catch up.
pub const SCHEDULE_AHEAD_SECONDS: f64 = 0.1;

/// Application screens for boot, song select, loading, gameplay, results, key configuration, and settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppScreen {
    /// Reading the song library in the background (startup and F5 rescan).
    Boot,
    SongSelect,
    Loading,
    Gameplay,
    Result,
    KeyConfig,
    /// The settings screen (F4): values that are set once.
    Settings,
}

pub struct AppState {
    pub window: Arc<Window>,
    /// Viewport, gameplay lane layout and judgement feedback.
    pub view: ViewState,
    pub audio_engine: Option<AudioEngine>,
    pub screen: AppScreen,
    pub songs: Vec<SongMetadata>,
    /// The folder the list shows (see `folders.rs`).
    pub folder_path: FolderPath,
    /// The whole folder tree of the library, rebuilt with the list.
    pub folder_tree: Vec<Folder>,
    /// The rows of the current folder (folders or songs), rebuilt with the list.
    pub entries: Vec<ListEntry>,
    /// Highlighted row of `entries`.
    pub selected_entry: usize,
    /// First visible row of the song list (see `beetle_render::window_start`). Keys re-centre it; clicks and the wheel do not.
    pub list_scroll: usize,
    /// The chart each group shows when it is not the first one (saved in `config.dat`).
    pub chart_choice: crate::folders::ChartChoices,
    /// The song list filter (see `filters.rs`), saved in `config.dat`.
    pub filter: crate::filters::Filter,
    /// The filter row's keyboard focus: an index into the row's items (`None` = not focused).
    pub filter_focus: Option<usize>,
    /// The sort menu, while open: its highlighted option.
    pub sort_menu: Option<usize>,
    /// The key modes the library has songs in (the mode chips), rebuilt with the list.
    pub present_modes: Vec<beetle_core::PlayMode>,
    /// The levels the library has, ascending (the level steppers step through them).
    pub level_steps: Vec<u32>,
    /// The "N곡 찾음" count while a search or a filter is on; `None` otherwise.
    pub result_count: Option<usize>,
    pub search_query: String,
    /// IME composition text shown after the query; empty when none. Set only
    /// while the search box is open (see `ime.rs`).
    pub search_preedit: String,
    /// Whether the search box is open. Change it only through `ime::set_search_active`.
    pub is_search_active: bool,
    /// The caret rect last given to the OS as the IME candidate position.
    pub ime_caret_sent: Option<Rect>,
    /// Installed difficulty tables, matched to `songs`.
    pub tables: TableIndex,
    pub sort_mode: SortMode,
    /// The play options panel (per play) is open over song select.
    pub show_option_modal: bool,
    pub show_exit_modal: bool,
    /// The help overlay (?) over song select.
    pub show_help: bool,
    /// A file is dragged over the window (winit HoveredFile until it is dropped or cancelled).
    pub drop_hover: bool,
    pub should_exit_app: bool,
    /// Highlighted row of the play options panel.
    pub modal_row: usize,
    /// Highlighted row of the Settings screen.
    pub settings_row: usize,
    /// The judge offset calibration (a sub-screen of Settings), while open.
    pub calibration: Option<crate::calibration::Session>,
    /// The screen Key Configuration returns to (song select or Settings).
    pub key_config_return: AppScreen,
    pub selected_key_idx: usize,
    pub score_store: ScoreStore,
    pub play_options: PlayOptions,
    pub is_auto_play: bool,
    pub is_replay_playback: bool,
    pub is_gameplay_paused: bool,
    pub pause_selected_option: usize,
    pub current_replay: Option<ReplayData>,
    pub playback_replay: Option<ReplayData>,
    pub playback_cursor: usize,
    pub start_measure: u32,
    pub stage_image_cache: std::collections::HashMap<ChartId, Option<ImageBuffer>>,
    pub bga_bank: std::collections::HashMap<beetle_core::BmpId, ImageBuffer>,
    pub bga_cursor: usize,
    pub current_bga_bmp: Option<beetle_core::BmpId>,
    pub current_layer_bmp: Option<beetle_core::BmpId>,
    pub poor_bga_bmp: Option<beetle_core::BmpId>,
    pub poor_until_time: f64,
    pub active_bga_image: Option<ImageBuffer>,
    pub video_players: std::collections::HashMap<beetle_core::BmpId, beetle_render::BgaVideoPlayer>,
    pub video_start_times: std::collections::HashMap<beetle_core::BmpId, f64>,
    pub active_chart: Option<BmsChart>,
    pub active_timing: Option<TimingModel>,
    pub active_chart_id: ChartId,
    /// The long note rule the loaded chart is played under; `None` when it has no long notes.
    pub active_ln: Option<LnRule>,
    /// The loaded chart asked for HCN, which is played as CN.
    pub active_hcn: bool,
    pub active_judge: Option<JudgeEngine>,
    pub song_end_time: f64,
    /// Which of the chart's bests the last play beat.
    pub score_update: ScoreUpdate,
    pub previous_best: Option<ScoreRecord>,
    /// Key layout per key mode (5K / 7K / 9K / 10K / 14K).
    pub key_bindings: crate::input::KeyBindings,
    /// Key Config is waiting for a key press (to set or add).
    pub rebinding: Option<beetle_render::Rebind>,
    /// Key mode whose layout Key Config is editing.
    pub key_config_edit_mode: PlayMode,
    /// Lane keys held during gameplay. A lane can have several keys; it is
    /// released only when the last of them is.
    pub held_keys: Vec<(winit::keyboard::KeyCode, Lane)>,
    pub master_volume: f32,
    pub display_mode: DisplayMode,
    pub gpu_backend: GpuBackendSetting,
    pub target_fps: u32,
    pub track_bga: crate::config::TrackBgaSetting,
    pub bga_enabled: bool,
    pub key_hint: crate::config::KeyHintSetting,
    /// A value changed during play ("500 ms"), with the audio time it
    /// appeared at. Drawn over the lane for a second.
    pub gameplay_readout: Option<(String, f64)>,
    /// Set once the song is over: the end banner is showing and the play has
    /// been saved (see `gameplay::finish_gameplay`).
    pub gameplay_end: Option<crate::gameplay::GameplayEnd>,
    /// Whether this play has ended yet: it is saved once (see `PlayEndGuard`).
    pub play_end: crate::gameplay::PlayEndGuard,
    /// The gauge over the current play, for the result screen's trend graph.
    /// Reserved at song start and sampled in `tick_gameplay`.
    pub gauge_trend: beetle_core::GaugeTrend,
    pub is_alt_pressed: bool,
    pub bgm_cursor: usize,
    /// Auto-play key sounds due up to this time are already scheduled.
    pub autoplay_sound_until: f64,
    /// The song library being read on a worker thread (`AppScreen::Boot`).
    pub library_receiver: Option<Receiver<LibraryLoad>>,
    pub library_job: LibraryJob,
    pub library_started_at: Instant,
    pub loading_song: Option<SongMetadata>,
    pub loading_receiver: Option<crate::loader::SongLoadReceiver>,
    pub loading_spinner_frame: usize,
    /// When the current song load started (drives the loading animation).
    pub loading_started_at: Instant,
    pub loading_anim_time: Instant,
    pub result_entered_at: Instant,
    pub last_render_time: Instant,
    pub cursor_settle_time: Instant,
    pub stage_image_receiver: Option<Receiver<(ChartId, Option<ImageBuffer>)>>,
    pub stage_image_loading_id: Option<ChartId>,
    /// Song-select audio preview (`#PREVIEW`).
    pub preview: crate::preview::Preview,
    /// Canvas UI + song textures.
    pub gpu_ui: crate::gpu_ui::GpuUi,
    /// Env-driven screenshot hook (see devtools.rs); `None` normally.
    pub capture: Option<crate::devtools::Capture>,
    /// Screenshot path to write from the next presented Canvas-UI frame.
    pub pending_screenshot: Option<String>,
    /// The current screen and when it was entered (fade-in, see `transition.rs`).
    pub screen_entry: crate::transition::ScreenEntry,
    /// The toast showing over the menus, if any.
    pub toast: Option<crate::transition::Toast>,
    /// An animation ended since the last frame: draw one final frame so the
    /// overlays clear, then go back to sleeping.
    pub anim_tail: bool,
    /// The renderer (ADR-026). `gpu_backend` changes apply on restart.
    pub d3d11: beetle_render::D3d11Backend,
    /// `gpu_backend` as it was when `d3d11` was created.
    pub gpu_backend_at_start: GpuBackendSetting,
    /// Cursor position in physical pixels; `None` outside the window.
    pub cursor: Option<(f32, f32)>,
    /// Wheel notches not yet applied (precise wheels report fractions).
    pub wheel_carry: f32,
}

impl AppState {
    pub fn apply_display_mode(&mut self) {
        match self.display_mode {
            DisplayMode::Windowed => {
                self.window.set_fullscreen(None);
                let avail = self.available_resolutions();
                let size = self.window.inner_size();
                if !avail
                    .iter()
                    .any(|&(w, h, _)| w == size.width && h == size.height)
                {
                    if let Some(&(w, h, _)) = avail.first() {
                        let _ = self
                            .window
                            .request_inner_size(winit::dpi::PhysicalSize::new(w, h));
                        self.view.resize(w, h);
                    }
                }
            }
            DisplayMode::Borderless => {
                self.window
                    .set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
            }
            DisplayMode::ExclusiveFullscreen => {
                let fullscreen = if let Some(monitor) = self.window.current_monitor() {
                    if let Some(video_mode) = monitor
                        .video_modes()
                        .max_by_key(|m| m.refresh_rate_millihertz())
                    {
                        Some(winit::window::Fullscreen::Exclusive(video_mode))
                    } else {
                        Some(winit::window::Fullscreen::Borderless(None))
                    }
                } else {
                    Some(winit::window::Fullscreen::Borderless(None))
                };
                self.window.set_fullscreen(fullscreen);
            }
        }
    }

    pub fn available_resolutions(&self) -> Vec<(u32, u32, &'static str)> {
        let (mon_w, mon_h) = self
            .window
            .current_monitor()
            .or_else(|| self.window.primary_monitor())
            .map(|m| (m.size().width, m.size().height))
            .unwrap_or((1920, 1080));

        let is_fullscreen_or_borderless = self.display_mode != DisplayMode::Windowed;

        let list: Vec<_> = crate::config::RESOLUTION_PRESETS
            .iter()
            .copied()
            .filter(|&(w, h, _)| {
                // Must not exceed current monitor dimensions
                if w > mon_w || h > mon_h {
                    return false;
                }
                // If not fullscreen/borderless (i.e. Windowed mode), only allow 16:9
                if !is_fullscreen_or_borderless && (w * 9 != h * 16) {
                    return false;
                }
                true
            })
            .collect();

        if list.is_empty() {
            vec![(1280, 720, "1280x720 (16:9 HD)")]
        } else {
            list
        }
    }

    pub fn current_resolution_label(&self) -> &'static str {
        let size = self.window.inner_size();
        let avail = self.available_resolutions();
        for &(w, h, label) in &avail {
            if size.width == w && size.height == h {
                return label;
            }
        }
        strings::RESOLUTION_CUSTOM
    }

    pub fn cycle_resolution(&mut self, forward: bool) {
        let size = self.window.inner_size();
        let avail = self.available_resolutions();
        if avail.is_empty() {
            return;
        }

        let cur_idx = avail
            .iter()
            .position(|&(w, h, _)| w == size.width && h == size.height);

        let next_idx = match cur_idx {
            Some(idx) => {
                if forward {
                    (idx + 1) % avail.len()
                } else if idx == 0 {
                    avail.len() - 1
                } else {
                    idx - 1
                }
            }
            None => 0,
        };

        let (target_w, target_h, _) = avail[next_idx];
        let _ = self
            .window
            .request_inner_size(winit::dpi::PhysicalSize::new(target_w, target_h));
        self.view.resize(target_w, target_h);
        beetle_render::GpuBackend::resize(&mut self.d3d11, target_w, target_h);
    }

    /// Sets the renderer's scroll speed from the green number and the lane
    /// cover. Call after either changes; the green number itself is kept, so
    /// a new cover keeps the time a note is visible.
    pub fn sync_hi_speed(&mut self) {
        self.view.skin.hi_speed = beetle_render::green_ms_to_px_per_sec(
            self.play_options.green_ms as f32,
            self.view.skin.lane_cover_ratio,
        );
    }

    pub fn save_config(&self) {
        let size = self.window.inner_size();
        let app_config = AppConfig {
            play_options: self.play_options.clone(),
            lane_cover_ratio: self.view.skin.lane_cover_ratio,
            field_position: self.view.skin.field_position,
            scratch_sides: self.view.skin.scratch_sides,
            eight_k_form: self.view.skin.eight_k_form,
            sort_mode: self.sort_mode,
            folder_path: self.folder_path.clone(),
            chart_choices: self.chart_choice.clone(),
            filter: self.filter.clone(),
            key_layouts: self.key_bindings.to_saved().map(Some),
            legacy_key_layout: None,
            master_volume: self.master_volume,
            display_mode: self.display_mode,
            gpu_backend: self.gpu_backend,
            window_width: size.width.max(640),
            window_height: size.height.max(480),
            target_fps: self.target_fps,
            track_bga: self.track_bga,
            bga_enabled: self.bga_enabled,
            key_hint: self.key_hint,
        };
        app_config.save();
    }

    /// Takes the song library from the worker once it is done. The screen
    /// leaves `Boot` for song select; any other screen (a song launched from the
    /// command line meanwhile) stays as it is. Call every loop iteration.
    pub fn poll_library(&mut self) {
        let Some(rx) = &self.library_receiver else {
            return;
        };
        let load = match rx.try_recv() {
            Ok(load) => load,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            // The worker died; carry on with an empty library.
            Err(std::sync::mpsc::TryRecvError::Disconnected) => LibraryLoad::default(),
        };
        self.library_receiver = None;
        self.songs = load.songs;
        self.tables = load.tables;
        match (self.library_job, load.score_store) {
            (LibraryJob::Startup, Some(store)) => self.score_store = store,
            (LibraryJob::Startup, None) => {}
            (LibraryJob::Rescan, _) => {
                migrate_chart_keys(&self.songs, &mut self.score_store);
                sort_songs(
                    &mut self.songs,
                    self.sort_mode,
                    &self.score_store,
                    LnOption::Cn,
                );
            }
        }
        self.recompute_entries();
        if self.library_job == LibraryJob::Rescan {
            let count = self.songs.len().to_string();
            crate::transition::show_toast(
                self,
                beetle_render::ToastKind::Success,
                strings::fill(strings::TOAST_RESCAN_DONE, &[&count]),
            );
        }
        self.cursor_settle_time = Instant::now();
        if self.screen == AppScreen::Boot {
            self.screen = crate::devtools::start_screen().unwrap_or(AppScreen::SongSelect);
            self.key_config_edit_mode = self.key_config_mode();
        }
        // Once per load, and only when song select is the screen that shows.
        if self.screen == AppScreen::SongSelect {
            if let Some(text) = uninstalled_toast_text(&load.uninstalled) {
                crate::transition::show_toast(self, beetle_render::ToastKind::Info, text);
            }
        }
        self.window.request_redraw();
    }

    /// Re-reads the song folders and tables on a worker (F5). Shows the Boot screen meanwhile.
    pub fn start_rescan(&mut self) {
        self.stage_image_cache.clear();
        self.stage_image_receiver = None;
        self.stage_image_loading_id = None;
        self.library_job = LibraryJob::Rescan;
        self.library_started_at = Instant::now();
        self.library_receiver = Some(spawn_library_rescan());
        self.screen = AppScreen::Boot;
        self.window.request_redraw();
    }

    /// The player's long note setting: it decides which rule's records the song
    /// list shows and which rule a play is judged under.
    pub fn ln_option(&self) -> LnOption {
        self.play_options.ln
    }

    /// The rules a play of `song` is judged under, and so the note count and
    /// the max EX every screen shows for it: a replay keeps the rule it was
    /// played with, anything else gets the chart's `#LNMODE` and the setting.
    pub fn play_ruleset(&self, song: &SongMetadata) -> beetle_core::Ruleset {
        use beetle_core::{LnRule, Ruleset};
        match self
            .playback_replay
            .as_ref()
            .filter(|_| self.is_replay_playback)
        {
            Some(replay) => match replay.ln {
                Some(LnRule::Ln) => Ruleset::LN,
                _ => Ruleset::CN,
            },
            None => Ruleset::resolve(song.ln_mode, self.ln_option()),
        }
    }

    /// Sorts and filters the song list again (the records it orders by depend on
    /// the long note setting), keeping the highlighted song highlighted.
    pub fn resort_songs(&mut self) {
        let keep = self.current_selected_song().map(|s| s.id);
        let ln_option = self.ln_option();
        sort_songs(
            &mut self.songs,
            self.sort_mode,
            &self.score_store,
            ln_option,
        );
        self.recompute_entries();
        if let Some(id) = keep {
            if let Some(pos) = folders::row_showing(&self.entries, &self.songs, id) {
                self.selected_entry = pos;
            }
        }
    }

    /// Rebuilds the folder tree and the rows of the current folder. A saved
    /// folder that no longer exists falls back to its nearest existing parent.
    pub fn recompute_entries(&mut self) {
        let ln_option = self.ln_option();
        self.present_modes = folders::present_modes(&self.songs);
        self.level_steps = crate::filters::level_steps(&self.songs);
        let pass =
            crate::filters::pass_mask(&self.songs, &self.score_store, ln_option, &self.filter);
        self.folder_tree = folders::build_tree(
            &self.songs,
            &self.score_store,
            &self.tables,
            ln_option,
            &pass,
        );
        self.folder_path = folders::normalize(&self.folder_tree, &self.folder_path);
        self.entries = folders::entries_for(
            &self.folder_tree,
            &self.folder_path,
            &self.songs,
            &self.search_query,
            &self.chart_choice,
            &pass,
        );
        self.result_count =
            (!self.search_query.trim().is_empty() || self.filter.is_active()).then(|| {
                folders::result_count(
                    &self.folder_tree,
                    &self.folder_path,
                    &self.songs,
                    &self.search_query,
                    &pass,
                )
            });

        if self.entries.is_empty() {
            self.selected_entry = 0;
        } else if self.selected_entry >= self.entries.len() {
            self.selected_entry = self.entries.len() - 1;
        }
    }

    /// The song under the highlight: a group's selected chart. `None` when a folder row is highlighted.
    pub fn current_selected_song(&self) -> Option<&SongMetadata> {
        let i = self.entries.get(self.selected_entry)?.song()?;
        self.songs.get(i)
    }

    /// Keeps the 8K arrangement in step with the 8K key preset: the trigger
    /// preset means the trigger form, the straight-row preset the straight
    /// row; custom bindings keep whatever form is set.
    pub fn sync_eight_k_form(&mut self) {
        let form = match self.key_bindings.get(PlayMode::Keys8).preset {
            KeyPreset::Ue8K => EightKForm::Inline,
            KeyPreset::Ue8KTriggers => EightKForm::Triggers,
            _ => return,
        };
        self.view.skin.set_eight_k_form(form);
    }

    /// Key mode to configure by default: the loaded chart's if there is one,
    /// otherwise the highlighted song's. Key Config opens on this mode.
    pub fn key_config_mode(&self) -> PlayMode {
        if let Some(chart) = &self.active_chart {
            chart.detect_play_mode()
        } else {
            self.current_selected_song()
                .map(|s| s.play_mode)
                .unwrap_or_default()
        }
    }

    /// Schedules BGM notes up to `SCHEDULE_AHEAD_SECONDS` past `audio_time`
    /// and advances BGA timeline events up to `audio_time`.
    pub fn advance_gameplay_timelines(&mut self, audio_time: f64) {
        let (Some(chart), Some(timing)) = (&self.active_chart, &self.active_timing) else {
            return;
        };

        // 1. Schedule BGM notes on their frames
        while self.bgm_cursor < chart.bgm_notes.len() {
            let (m, f, wav_id) = chart.bgm_notes[self.bgm_cursor];
            let target_t = timing.beat_to_time_seconds(m, f);
            if audio_time + SCHEDULE_AHEAD_SECONDS >= target_t {
                if let Some(audio) = &mut self.audio_engine {
                    let _ = audio.play_at(wav_id, target_t);
                }
                self.bgm_cursor += 1;
            } else {
                break;
            }
        }

        // 2. Advance BGA timeline events
        while self.bga_cursor < chart.bga_events.len() {
            let ev = &chart.bga_events[self.bga_cursor];
            let target_t = timing.beat_to_time_seconds(ev.measure, ev.fraction);
            if audio_time >= target_t {
                match ev.channel {
                    beetle_core::BgaChannel::Base => {
                        self.current_bga_bmp = Some(ev.bmp_id);
                        if self.video_players.contains_key(&ev.bmp_id) {
                            self.video_start_times.entry(ev.bmp_id).or_insert(target_t);
                        }
                    }
                    beetle_core::BgaChannel::Poor => {
                        self.poor_bga_bmp = Some(ev.bmp_id);
                    }
                    beetle_core::BgaChannel::Layer => {
                        self.current_layer_bmp = Some(ev.bmp_id);
                        if self.video_players.contains_key(&ev.bmp_id) {
                            self.video_start_times.entry(ev.bmp_id).or_insert(target_t);
                        }
                    }
                }
                self.bga_cursor += 1;
            } else {
                break;
            }
        }
    }

    /// Advances video playback if a video BGA is active.
    pub fn update_video_bga(&mut self, audio_time: f64) {
        if let Some(base_id) = self.current_bga_bmp {
            if let Some(player) = self.video_players.get_mut(&base_id) {
                let start_t = self.video_start_times.get(&base_id).copied().unwrap_or(0.0);
                let video_time = (audio_time - start_t).max(0.0);
                let _ = player.update(video_time);
            }
        }
        if let Some(layer_id) = self.current_layer_bmp {
            if let Some(player) = self.video_players.get_mut(&layer_id) {
                let start_t = self
                    .video_start_times
                    .get(&layer_id)
                    .copied()
                    .unwrap_or(0.0);
                let video_time = (audio_time - start_t).max(0.0);
                let _ = player.update(video_time);
            }
        }
    }
}

/// Which BGA bitmap to show: the POOR image while the miss penalty lasts,
/// else the current BGA; `None` means fall back to the stage image.
/// `available(id)` says whether that BMP has something to draw (a decoded
/// bitmap or a video frame).
pub fn resolve_bga_id(
    poor_until_time: f64,
    poor_bga_bmp: Option<beetle_core::BmpId>,
    current_bga_bmp: Option<beetle_core::BmpId>,
    available: impl Fn(beetle_core::BmpId) -> bool,
    audio_time: f64,
) -> Option<beetle_core::BmpId> {
    let poor = poor_bga_bmp.filter(|_| audio_time < poor_until_time);
    poor.into_iter()
        .chain(current_bga_bmp)
        .find(|&id| available(id))
}

/// Where a chart's replay is kept.
/// A chart with long notes has a replay for each long note rule.
pub fn replay_path(id: ChartId, ln: Option<LnRule>) -> String {
    match ln {
        None => format!("{}/{}.rep", REPLAYS_DIR, id.short()),
        Some(rule) => format!(
            "{}/{}-{}.rep",
            REPLAYS_DIR,
            id.short(),
            rule.as_str().to_lowercase()
        ),
    }
}

/// Writes the score file. The first save over a file in the original format
/// keeps a copy of it, and the write goes through a temporary file so an
/// interrupted save cannot leave a half-written `scores.dat`.
pub fn save_scores(store: &ScoreStore) {
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

/// Moves records and replays made under the old chart keys over to chart ids,
/// for every chart in the song list. Records and replays of charts that are
/// not in the list are left alone until the chart turns up.
pub fn migrate_chart_keys(songs: &[SongMetadata], store: &mut ScoreStore) {
    let pairs: Vec<(u64, ChartId)> = songs.iter().map(|s| (s.legacy_hash, s.id)).collect();

    let long_note_charts: Vec<ChartId> = songs
        .iter()
        .filter(|s| s.ln_count > 0)
        .map(|s| s.id)
        .collect();
    // Records from before long note rules belong to CN: they move under it.
    let ln_pending = long_note_charts.iter().any(|&id| store.get(id).is_some());
    if ln_pending {
        let backup = format!("{SCORES_FILE}.pre-ln.bak");
        if !Path::new(&backup).exists() {
            if let Ok(old) = fs::read_to_string(SCORES_FILE) {
                let _ = fs::write(backup, old);
            }
        }
    }

    if store.legacy_count() > 0 {
        // Keep the file as it was before the keys change, once.
        let backup = format!("{SCORES_FILE}.pre-id.bak");
        if !Path::new(&backup).exists() {
            if let Ok(old) = fs::read_to_string(SCORES_FILE) {
                let _ = fs::write(backup, old);
            }
        }
        if store.migrate(&pairs) > 0 {
            save_scores(store);
        }
    }
    if store.migrate_ln_rules(&long_note_charts) > 0 {
        save_scores(store);
    }

    migrate_replays(&pairs);
    migrate_ln_replays(&long_note_charts);
}

/// A replay of a chart with long notes from before the rules existed was
/// played under what is now CN: copy it to the CN name.
fn migrate_ln_replays(charts: &[ChartId]) {
    for &id in charts {
        let (old, new) = (replay_path(id, None), replay_path(id, Some(LnRule::Cn)));
        if Path::new(&old).exists() && !Path::new(&new).exists() {
            let _ = fs::copy(old, new);
        }
    }
}

/// Copies replays named by the old key to the chart-id names. The old files
/// stay where they are; a new name that already exists is never overwritten.
fn migrate_replays(pairs: &[(u64, ChartId)]) {
    let Ok(entries) = fs::read_dir(REPLAYS_DIR) else {
        return;
    };
    let old_keys: std::collections::HashSet<u64> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let stem = name.strip_suffix(".rep")?;
            (stem.len() == 16)
                .then(|| u64::from_str_radix(stem, 16).ok())
                .flatten()
        })
        .collect();
    for &(legacy, id) in pairs {
        if old_keys.contains(&legacy) {
            let new = replay_path(id, None);
            if !Path::new(&new).exists() {
                let _ = fs::copy(format!("{REPLAYS_DIR}/{legacy:016x}.rep"), new);
            }
        }
    }
}

/// What the library worker hands back.
#[derive(Default)]
pub struct LibraryLoad {
    pub songs: Vec<SongMetadata>,
    pub tables: TableIndex,
    /// Present at startup (the scores are read with the library); a rescan keeps the current ones.
    pub score_store: Option<ScoreStore>,
    /// `.bmsp` files in the library folders that are not installed, sorted.
    pub uninstalled: Vec<String>,
}

/// The notice for uninstalled packages, or `None` when there are none.
pub fn uninstalled_toast_text(names: &[String]) -> Option<String> {
    let first = names.first()?;
    Some(match names.len() {
        1 => strings::fill(strings::TOAST_UNINSTALLED_ONE, &[first.as_str()]),
        n => {
            let count = n.to_string();
            strings::fill(
                strings::TOAST_UNINSTALLED_MANY,
                &[count.as_str(), first.as_str()],
            )
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryJob {
    Startup,
    Rescan,
}

/// Reads scores, songs and tables on a worker thread so the window keeps painting (INV-5).
pub fn spawn_library_load(sort_mode: SortMode) -> Receiver<LibraryLoad> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (songs, score_store) = init_songs_and_scores(sort_mode);
        let tables = crate::tables::build_index(&songs);
        let uninstalled = crate::scanner::uninstalled_in_library(DEFAULT_SONGS_DIR);
        let _ = tx.send(LibraryLoad {
            songs,
            tables,
            score_store: Some(score_store),
            uninstalled,
        });
    });
    rx
}

/// Forces a rescan of the song folders on a worker thread. Sorting and score
/// key migration are left to `poll_library`, which owns the scores.
pub fn spawn_library_rescan() -> Receiver<LibraryLoad> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let songs = rescan_songs();
        let tables = crate::tables::build_index(&songs);
        let uninstalled = crate::scanner::uninstalled_in_library(DEFAULT_SONGS_DIR);
        let _ = tx.send(LibraryLoad {
            songs,
            tables,
            score_store: None,
            uninstalled,
        });
    });
    rx
}

pub fn init_songs_and_scores(sort_mode: SortMode) -> (Vec<SongMetadata>, ScoreStore) {
    let mut score_store = ScoreStore::new();
    if Path::new(SCORES_FILE).exists() {
        if let Ok(score_data) = fs::read_to_string(SCORES_FILE) {
            score_store.load_from_str(&score_data);
        }
    }

    let mut songs = load_or_scan_songs(DEFAULT_SONGS_DIR);

    migrate_chart_keys(&songs, &mut score_store);
    sort_songs(&mut songs, sort_mode, &score_store, LnOption::Cn);

    (songs, score_store)
}

/// Scans the song folders again, ignoring `songs.cache`; unsorted.
fn rescan_songs() -> Vec<SongMetadata> {
    crate::scanner::force_rescan_songs(DEFAULT_SONGS_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uninstalled_toast_names_one_file_or_counts_several() {
        assert_eq!(uninstalled_toast_text(&[]), None);
        assert_eq!(
            uninstalled_toast_text(&["AIRSHAVER.bmsp".to_string()]).as_deref(),
            Some("설치되지 않은 패키지: AIRSHAVER.bmsp / bpm install로 설치하세요")
        );
        let many = ["AIRSHAVER.bmsp", "Other.bmsp", "Third.bmsp"].map(String::from);
        assert_eq!(
            uninstalled_toast_text(&many).as_deref(),
            Some("설치되지 않은 패키지 3개 (AIRSHAVER.bmsp 외) / bpm install로 설치하세요")
        );
    }
}

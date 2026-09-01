use std::fs;
use std::path::Path;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Instant;

use beetle_audio::{AudioEngine, SampleBank};
use beetle_core::{
    compute_chart_hash, sort_songs, BmsChart, JudgeEngine, PlayOptions, ReplayData, ScoreRecord,
    ScoreStore, SongMetadata, SortMode, TimingModel,
};
use beetle_render::{ImageBuffer, SoftwareRenderer};
use softbuffer::{Context, Surface};
use winit::window::Window;

use crate::config::{AppConfig, DisplayMode, GpuBackendSetting};
use crate::demo;
use crate::input::InputConfig;
use crate::scanner::{load_or_scan_songs, DEFAULT_SONGS_DIR};

pub const SCORES_FILE: &str = "scores.dat";
pub const REPLAYS_DIR: &str = "replays";

/// Application screens for song select, loading, gameplay, results, and key configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppScreen {
    SongSelect,
    Loading,
    Gameplay,
    Result,
    KeyConfig,
}

/// Category grouping mode for songs library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SongCategory {
    #[default]
    All,
    Keys5,
    Keys7,
    Keys9,
    Keys10,
    Keys14,
    Level,
    ClearStatus,
}

impl SongCategory {
    pub fn next(self) -> Self {
        match self {
            SongCategory::All => SongCategory::Keys5,
            SongCategory::Keys5 => SongCategory::Keys7,
            SongCategory::Keys7 => SongCategory::Keys9,
            SongCategory::Keys9 => SongCategory::Keys10,
            SongCategory::Keys10 => SongCategory::Keys14,
            SongCategory::Keys14 => SongCategory::Level,
            SongCategory::Level => SongCategory::ClearStatus,
            SongCategory::ClearStatus => SongCategory::All,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            SongCategory::All => SongCategory::ClearStatus,
            SongCategory::Keys5 => SongCategory::All,
            SongCategory::Keys7 => SongCategory::Keys5,
            SongCategory::Keys9 => SongCategory::Keys7,
            SongCategory::Keys10 => SongCategory::Keys9,
            SongCategory::Keys14 => SongCategory::Keys10,
            SongCategory::Level => SongCategory::Keys14,
            SongCategory::ClearStatus => SongCategory::Level,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SongCategory::All => "ALL SONGS",
            SongCategory::Keys5 => "5 KEYS",
            SongCategory::Keys7 => "7 KEYS",
            SongCategory::Keys9 => "9 KEYS",
            SongCategory::Keys10 => "10 KEYS",
            SongCategory::Keys14 => "14 KEYS",
            SongCategory::Level => "BY LEVEL",
            SongCategory::ClearStatus => "BY CLEAR STATUS",
        }
    }
}

pub struct AppState {
    pub window: Arc<Window>,
    pub _context: Context<Arc<Window>>,
    pub surface: Surface<Arc<Window>, Arc<Window>>,
    pub renderer: SoftwareRenderer,
    pub audio_engine: Option<AudioEngine>,
    pub screen: AppScreen,
    pub songs: Vec<SongMetadata>,
    pub filtered_indices: Vec<usize>,
    pub selected_song_idx: usize,
    pub search_query: String,
    pub is_search_active: bool,
    pub category_mode: SongCategory,
    pub sort_mode: SortMode,
    pub show_option_modal: bool,
    pub show_exit_modal: bool,
    pub should_exit_app: bool,
    pub modal_row: usize,
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
    pub stage_image_cache: std::collections::HashMap<u64, Option<ImageBuffer>>,
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
    pub active_chart_hash: u64,
    pub active_judge: Option<JudgeEngine>,
    pub song_end_time: f64,
    pub is_new_record: bool,
    pub previous_best: Option<ScoreRecord>,
    pub input_config: InputConfig,
    pub is_rebinding_key: bool,
    pub master_volume: f32,
    pub display_mode: DisplayMode,
    pub gpu_backend: GpuBackendSetting,
    pub target_fps: u32,
    pub track_bga: crate::config::TrackBgaSetting,
    pub is_alt_pressed: bool,
    pub bgm_cursor: usize,
    pub loading_song: Option<SongMetadata>,
    pub loading_receiver: Option<Receiver<Result<(BmsChart, TimingModel, SampleBank, std::collections::HashMap<beetle_core::BmpId, ImageBuffer>, std::collections::HashMap<beetle_core::BmpId, crate::loader::VideoSource>), String>>>,
    pub loading_spinner_frame: usize,
    pub loading_anim_time: Instant,
    pub last_render_time: Instant,
    pub cursor_settle_time: Instant,
    pub stage_image_receiver: Option<Receiver<(u64, Option<ImageBuffer>)>>,
    pub stage_image_loading_hash: Option<u64>,
    #[cfg(target_os = "windows")]
    pub d3d11_backend: Option<beetle_render::D3d11Backend>,
    #[cfg(target_os = "windows")]
    pub d3d11_frame_texture: Option<beetle_render::TextureId>,
}

impl AppState {
    pub fn apply_display_mode(&mut self) {
        match self.display_mode {
            DisplayMode::Windowed => {
                self.window.set_fullscreen(None);
                let avail = self.available_resolutions();
                let size = self.window.inner_size();
                if !avail.iter().any(|&(w, h, _)| w == size.width && h == size.height) {
                    if let Some(&(w, h, _)) = avail.first() {
                        let _ = self.window.request_inner_size(winit::dpi::PhysicalSize::new(w, h));
                        self.renderer.resize(w, h);
                    }
                }
            }
            DisplayMode::Borderless => {
                self.window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
            }
            DisplayMode::ExclusiveFullscreen => {
                let fullscreen = if let Some(monitor) = self.window.current_monitor() {
                    if let Some(video_mode) = monitor.video_modes().max_by_key(|m| m.refresh_rate_millihertz()) {
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

    pub fn is_d3d11_active(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            (self.gpu_backend == GpuBackendSetting::Auto || self.gpu_backend == GpuBackendSetting::Direct3D11)
                && self.d3d11_backend.is_some()
        }
        #[cfg(not(target_os = "windows"))]
        {
            false
        }
    }

    #[cfg(target_os = "windows")]
    pub fn ensure_d3d11_backend(&mut self) {
        if self.d3d11_backend.is_none() {
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = self.window.window_handle() {
                if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
                    let hwnd = win32_handle.hwnd.get() as *mut std::ffi::c_void;
                    let size = self.window.inner_size();
                    if let Ok(mut d3d) = beetle_render::D3d11Backend::new(hwnd, size.width, size.height) {
                        use beetle_render::GpuBackend;
                        let tex = d3d.create_texture(size.width, size.height, self.renderer.data());
                        self.d3d11_frame_texture = tex;
                        self.d3d11_backend = Some(d3d);
                    }
                }
            }
        }
    }

    pub fn available_resolutions(&self) -> Vec<(u32, u32, &'static str)> {
        let (mon_w, mon_h) = self.window.current_monitor()
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
        "CUSTOM"
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
        let _ = self.window.request_inner_size(winit::dpi::PhysicalSize::new(target_w, target_h));
        self.renderer.resize(target_w, target_h);
        #[cfg(target_os = "windows")]
        if let Some(d3d11) = &mut self.d3d11_backend {
            use beetle_render::GpuBackend;
            d3d11.resize(target_w, target_h);
            if let Some(old_tex) = self.d3d11_frame_texture.take() {
                d3d11.destroy_texture(old_tex);
            }
            self.d3d11_frame_texture = d3d11.create_texture(target_w, target_h, self.renderer.data());
        }
    }

    pub fn save_config(&self) {
        let size = self.window.inner_size();
        let app_config = AppConfig {
            play_options: self.play_options.clone(),
            lane_cover_ratio: self.renderer.skin.lane_cover_ratio,
            sort_mode: self.sort_mode,
            key_preset: self.input_config.preset,
            custom_key_bindings: self.input_config.serialize_bindings(),
            master_volume: self.master_volume,
            display_mode: self.display_mode,
            gpu_backend: self.gpu_backend,
            window_width: size.width.max(640),
            window_height: size.height.max(480),
            target_fps: self.target_fps,
            track_bga: self.track_bga,
        };
        app_config.save();
    }

    pub fn recompute_filtered_songs(&mut self) {
        self.filtered_indices = filter_song_indices(
            &self.songs,
            &self.search_query,
            self.category_mode,
            &self.score_store,
        );

        if self.filtered_indices.is_empty() {
            self.selected_song_idx = 0;
        } else if self.selected_song_idx >= self.filtered_indices.len() {
            self.selected_song_idx = self.filtered_indices.len() - 1;
        }
    }

    pub fn current_selected_song(&self) -> Option<&SongMetadata> {
        let real_idx = *self.filtered_indices.get(self.selected_song_idx)?;
        self.songs.get(real_idx)
    }

    pub fn current_visible_songs(&self) -> Vec<SongMetadata> {
        self.filtered_indices
            .iter()
            .filter_map(|&idx| self.songs.get(idx).cloned())
            .collect()
    }

    /// Advances BGM notes and BGA timeline events up to `audio_time`.
    pub fn advance_gameplay_timelines(&mut self, audio_time: f64) {
        let (Some(chart), Some(timing)) = (&self.active_chart, &self.active_timing) else {
            return;
        };

        // 1. Advance BGM notes
        while self.bgm_cursor < chart.bgm_notes.len() {
            let (m, f, wav_id) = chart.bgm_notes[self.bgm_cursor];
            let target_t = timing.beat_to_time_seconds(m, f);
            if audio_time >= target_t {
                if let Some(audio) = &mut self.audio_engine {
                    let _ = audio.send_command(beetle_audio::AudioCommand::PlaySample {
                        sample_id: wav_id,
                        volume: 1.0,
                        pan: 0.0,
                    });
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
                let start_t = self.video_start_times.get(&layer_id).copied().unwrap_or(0.0);
                let video_time = (audio_time - start_t).max(0.0);
                let _ = player.update(video_time);
            }
        }
    }
}

/// Pure helper to resolve active BGA frame hierarchy without window or surface dependencies.
pub fn resolve_bga_hierarchy<'a>(
    poor_until_time: f64,
    poor_bga_bmp: Option<beetle_core::BmpId>,
    current_bga_bmp: Option<beetle_core::BmpId>,
    bga_bank: &'a std::collections::HashMap<beetle_core::BmpId, ImageBuffer>,
    video_players: &'a std::collections::HashMap<beetle_core::BmpId, beetle_render::BgaVideoPlayer>,
    active_bga_image: Option<&'a ImageBuffer>,
    audio_time: f64,
) -> Option<&'a ImageBuffer> {
    if audio_time < poor_until_time {
        if let Some(id) = poor_bga_bmp {
            if let Some(img) = bga_bank.get(&id) {
                return Some(img);
            }
            if let Some(vp) = video_players.get(&id) {
                if let Some(frame) = vp.current_frame() {
                    return Some(frame);
                }
            }
        }
    }

    if let Some(id) = current_bga_bmp {
        if let Some(vp) = video_players.get(&id) {
            if let Some(frame) = vp.current_frame() {
                return Some(frame);
            }
        }
        if let Some(img) = bga_bank.get(&id) {
            return Some(img);
        }
    }

    active_bga_image
}

pub fn filter_song_indices(
    songs: &[SongMetadata],
    search_query: &str,
    category: SongCategory,
    score_store: &ScoreStore,
) -> Vec<usize> {
    let q = search_query.to_lowercase().trim().to_string();
    songs
        .iter()
        .enumerate()
        .filter_map(|(idx, s)| {
            // 1. Search filter
            if !q.is_empty() {
                let matches_title = s.title.to_lowercase().contains(&q);
                let matches_artist = s.artist.to_lowercase().contains(&q);
                let matches_genre = s.genre.to_lowercase().contains(&q);
                if !matches_title && !matches_artist && !matches_genre {
                    return None;
                }
            }

            // 2. Category filter
            match category {
                SongCategory::All => Some(idx),
                SongCategory::Keys5 => {
                    if s.play_mode == beetle_core::PlayMode::Keys5 || s.file_path == ":demo:" {
                        Some(idx)
                    } else {
                        None
                    }
                }
                SongCategory::Keys7 => {
                    if s.play_mode == beetle_core::PlayMode::Keys7 || s.file_path == ":demo:" {
                        Some(idx)
                    } else {
                        None
                    }
                }
                SongCategory::Keys9 => {
                    if s.play_mode == beetle_core::PlayMode::Keys9 {
                        Some(idx)
                    } else {
                        None
                    }
                }
                SongCategory::Keys10 => {
                    if s.play_mode == beetle_core::PlayMode::Keys10 {
                        Some(idx)
                    } else {
                        None
                    }
                }
                SongCategory::Keys14 => {
                    if s.play_mode == beetle_core::PlayMode::Keys14 {
                        Some(idx)
                    } else {
                        None
                    }
                }
                SongCategory::Level => Some(idx),
                SongCategory::ClearStatus => {
                    let best = score_store.get(s.hash);
                    if best.is_some() || s.file_path == ":demo:" {
                        Some(idx)
                    } else {
                        None
                    }
                }
            }
        })
        .collect()
}

pub fn init_songs_and_scores(sort_mode: SortMode) -> (Vec<SongMetadata>, ScoreStore) {
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
        play_mode: beetle_core::PlayMode::Keys7,
    };

    if !songs.iter().any(|s| s.file_path == ":demo:") {
        songs.insert(0, demo_meta);
    }

    sort_songs(&mut songs, sort_mode, &score_store);

    (songs, score_store)
}

pub fn rescan_songs_and_scores(sort_mode: SortMode, score_store: &ScoreStore) -> Vec<SongMetadata> {
    let mut songs = crate::scanner::force_rescan_songs(DEFAULT_SONGS_DIR);

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
        play_mode: beetle_core::PlayMode::Keys7,
    };

    if !songs.iter().any(|s| s.file_path == ":demo:") {
        songs.insert(0, demo_meta);
    }

    sort_songs(&mut songs, sort_mode, score_store);

    songs
}

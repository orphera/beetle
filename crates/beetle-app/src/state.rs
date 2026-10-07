use std::fs;
use std::path::Path;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Instant;

use beetle_audio::AudioEngine;
use beetle_core::{
    compute_chart_hash, sort_songs, BmsChart, ChartId, JudgeEngine, Lane, LnOption, LnRule, PlayMode,
    PlayOptions,
    ReplayData, ScoreRecord, ScoreStore, ScoreUpdate, SongMetadata, SortMode, TableIndex, TimingModel,
};
use beetle_render::{ImageBuffer, ViewState};
use winit::window::Window;

use crate::config::{AppConfig, DisplayMode, GpuBackendSetting};
use crate::demo;
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
    /// A difficulty table folder: the index into the installed tables.
    Table(usize),
}

impl SongCategory {
    /// The next folder; the installed difficulty tables come after the built-in ones.
    pub fn next(self, tables: usize) -> Self {
        match self {
            SongCategory::ClearStatus if tables > 0 => SongCategory::Table(0),
            SongCategory::Table(i) if i + 1 < tables => SongCategory::Table(i + 1),
            SongCategory::Table(_) => SongCategory::All,
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

    pub fn prev(self, tables: usize) -> Self {
        match self {
            SongCategory::All if tables > 0 => SongCategory::Table(tables - 1),
            SongCategory::Table(0) => SongCategory::ClearStatus,
            SongCategory::Table(i) => SongCategory::Table(i - 1),
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
            SongCategory::Table(_) => "TABLE",
        }
    }

    /// What the folder selector shows: the name, and for a table also how many
    /// of its charts are in the song list (`SATELLITE  15 / 2,467`).
    pub fn title(self, tables: &TableIndex) -> String {
        let SongCategory::Table(i) = self else {
            return self.as_str().to_string();
        };
        let Some(table) = tables.tables().get(i) else {
            return SongCategory::All.as_str().to_string();
        };
        const LONGEST_NAME: usize = 20;
        let mut name: String = table.name.to_uppercase().chars().take(LONGEST_NAME).collect();
        if table.name.chars().count() > LONGEST_NAME {
            name.truncate(name.trim_end().len());
            name.push('…');
        }
        format!("{name}  {} / {}", thousands(tables.owned_count(i)), thousands(table.entries.len()))
    }
}

fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub struct AppState {
    pub window: Arc<Window>,
    /// Viewport, gameplay lane layout and judgement feedback.
    pub view: ViewState,
    pub audio_engine: Option<AudioEngine>,
    pub screen: AppScreen,
    pub songs: Vec<SongMetadata>,
    pub filtered_indices: Vec<usize>,
    pub selected_song_idx: usize,
    pub search_query: String,
    pub is_search_active: bool,
    pub category_mode: SongCategory,
    /// Installed difficulty tables, matched to `songs`.
    pub tables: TableIndex,
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
    pub is_alt_pressed: bool,
    pub bgm_cursor: usize,
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
    /// The renderer (ADR-026). `gpu_backend` changes apply on restart.
    pub d3d11: beetle_render::D3d11Backend,
    /// `gpu_backend` as it was when `d3d11` was created.
    pub gpu_backend_at_start: GpuBackendSetting,
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
        let _ = self
            .window
            .request_inner_size(winit::dpi::PhysicalSize::new(target_w, target_h));
        self.view.resize(target_w, target_h);
        beetle_render::GpuBackend::resize(&mut self.d3d11, target_w, target_h);
    }

    pub fn save_config(&self) {
        let size = self.window.inner_size();
        let app_config = AppConfig {
            play_options: self.play_options.clone(),
            lane_cover_ratio: self.view.skin.lane_cover_ratio,
            sort_mode: self.sort_mode,
            key_layouts: self.key_bindings.to_saved().map(Some),
            legacy_key_layout: None,
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

    /// Reads the tables folder again and matches it to the song list.
    pub fn reload_tables(&mut self) {
        self.tables = crate::tables::build_index(&self.songs);
    }

    /// The long note setting the song list shows records for. Gameplay is still
    /// always CN, so this is too until the LN MODE option exists.
    pub fn ln_option(&self) -> LnOption {
        LnOption::Cn
    }

    pub fn recompute_filtered_songs(&mut self) {
        // A table folder whose table is gone (removed, or the list changed) falls back to all songs.
        if matches!(self.category_mode, SongCategory::Table(i) if i >= self.tables.tables().len()) {
            self.category_mode = SongCategory::All;
        }
        self.filtered_indices = filter_song_indices(
            &self.songs,
            &self.search_query,
            self.category_mode,
            &self.score_store,
            &self.tables,
            self.ln_option(),
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

pub fn filter_song_indices(
    songs: &[SongMetadata],
    search_query: &str,
    category: SongCategory,
    score_store: &ScoreStore,
    tables: &TableIndex,
    ln_option: LnOption,
) -> Vec<usize> {
    let q = search_query.to_lowercase().trim().to_string();
    let mut indices: Vec<usize> = songs
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
                    let best = score_store.best(s, ln_option);
                    if best.is_some() || s.file_path == ":demo:" {
                        Some(idx)
                    } else {
                        None
                    }
                }
                SongCategory::Table(i) => tables.entry_for(i, s.id).map(|_| idx),
            }
        })
        .collect();

    // A table folder lists its charts in the table's level order; within a
    // level the songs stay in the current sort order (the sort is stable).
    if let SongCategory::Table(i) = category {
        if let Some(table) = tables.tables().get(i) {
            let level_of = |idx: usize| tables.entry_for(i, songs[idx].id).map_or("", |e| e.level.as_str());
            indices.sort_by(|&a, &b| table.compare_levels(level_of(a), level_of(b)));
        }
    }
    indices
}

/// Where a chart's replay is kept.
/// A chart with long notes has a replay for each long note rule.
pub fn replay_path(id: ChartId, ln: Option<LnRule>) -> String {
    match ln {
        None => format!("{}/{}.rep", REPLAYS_DIR, id.short()),
        Some(rule) => format!("{}/{}-{}.rep", REPLAYS_DIR, id.short(), rule.as_str().to_lowercase()),
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
    let pairs: Vec<(u64, ChartId)> = songs
        .iter()
        .map(|s| (s.legacy_hash, s.id))
        .collect();

    let long_note_charts: Vec<ChartId> = songs.iter().filter(|s| s.ln_count > 0).map(|s| s.id).collect();
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
            (stem.len() == 16).then(|| u64::from_str_radix(stem, 16).ok()).flatten()
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
    let (bpm_min, bpm_max) = demo_chart.bpm_range();
    let (id, md5) = beetle_core::hash_chart_bytes(b"BEETLE_INTERNAL_DEMO_CHART_V1");
    let demo_meta = SongMetadata {
        id,
        md5,
        ln_count: 0,
        ln_mode: None,
        legacy_hash: compute_chart_hash(b"BEETLE_INTERNAL_DEMO_CHART_V1"),
        file_path: ":demo:".to_string(),
        title: demo_chart.header.title,
        subtitle: demo_chart.header.subtitle,
        artist: demo_chart.header.artist,
        genre: demo_chart.header.genre,
        bpm: demo_chart.header.bpm,
        bpm_min,
        bpm_max,
        play_level: demo_chart.header.play_level,
        notes_count: demo_chart.notes.len(),
        play_mode: beetle_core::PlayMode::Keys7,
    };

    if !songs.iter().any(|s| s.file_path == ":demo:") {
        songs.insert(0, demo_meta);
    }

    migrate_chart_keys(&songs, &mut score_store);
    sort_songs(&mut songs, sort_mode, &score_store, LnOption::Cn);

    (songs, score_store)
}

pub fn rescan_songs_and_scores(sort_mode: SortMode, score_store: &ScoreStore) -> Vec<SongMetadata> {
    let mut songs = crate::scanner::force_rescan_songs(DEFAULT_SONGS_DIR);

    let demo_chart = demo::create_demo_chart();
    let (bpm_min, bpm_max) = demo_chart.bpm_range();
    let (id, md5) = beetle_core::hash_chart_bytes(b"BEETLE_INTERNAL_DEMO_CHART_V1");
    let demo_meta = SongMetadata {
        id,
        md5,
        ln_count: 0,
        ln_mode: None,
        legacy_hash: compute_chart_hash(b"BEETLE_INTERNAL_DEMO_CHART_V1"),
        file_path: ":demo:".to_string(),
        title: demo_chart.header.title,
        subtitle: demo_chart.header.subtitle,
        artist: demo_chart.header.artist,
        genre: demo_chart.header.genre,
        bpm: demo_chart.header.bpm,
        bpm_min,
        bpm_max,
        play_level: demo_chart.header.play_level,
        notes_count: demo_chart.notes.len(),
        play_mode: beetle_core::PlayMode::Keys7,
    };

    if !songs.iter().any(|s| s.file_path == ":demo:") {
        songs.insert(0, demo_meta);
    }

    sort_songs(&mut songs, sort_mode, score_store, LnOption::Cn);

    songs
}

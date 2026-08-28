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

use crate::config::AppConfig;
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
    pub active_bga_image: Option<ImageBuffer>,
    pub preview_audio: Option<AudioEngine>,
    pub preview_song_hash: Option<u64>,
    pub preview_timer: Instant,
    pub preview_duration: f64,
    pub preview_attempted: bool,
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
    pub bgm_cursor: usize,
    pub loading_song: Option<SongMetadata>,
    pub loading_receiver: Option<Receiver<Result<(BmsChart, TimingModel, SampleBank), String>>>,
    pub loading_spinner_frame: usize,
    pub loading_anim_time: Instant,
    pub last_render_time: Instant,
}

impl AppState {
    pub fn save_config(&self) {
        let app_config = AppConfig {
            play_options: self.play_options.clone(),
            lane_cover_ratio: self.renderer.skin.lane_cover_ratio,
            sort_mode: self.sort_mode,
            key_preset: self.input_config.preset,
            custom_key_bindings: self.input_config.serialize_bindings(),
            master_volume: self.master_volume,
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

use crate::filters::Filter;
use crate::folders::{mode_from_id, mode_id, ChartChoices, FolderPath};
use crate::input::{mode_slot_name, KeyPreset, SavedLayout, MODE_SLOTS};
use beetle_core::{ChartId, GaugeType, LaneModifier, LnOption, PlayOptions, SortMode};
use beetle_render::{px_per_sec_to_green_ms, EightKForm, FieldPosition, ScratchSide};
use std::fs;
use std::path::Path;

pub const CONFIG_FILE: &str = "config.dat";

/// Standard resolution presets (16:9 standard and non-16:9 letterbox/pillarbox test modes).
pub const RESOLUTION_PRESETS: &[(u32, u32, &str)] = &[
    (960, 540, "960x540 (16:9 qHD)"),
    (1280, 720, "1280x720 (16:9 HD)"),
    (1600, 900, "1600x900 (16:9 HD+)"),
    (1920, 1080, "1920x1080 (16:9 FHD)"),
    (2560, 1440, "2560x1440 (16:9 QHD)"),
    (3840, 2160, "3840x2160 (16:9 4K)"),
    (1024, 768, "1024x768 (4:3 Pillarbox)"),
    (1280, 800, "1280x800 (16:10 Letterbox)"),
];

/// Display and windowing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplayMode {
    #[default]
    Windowed,
    Borderless,
    ExclusiveFullscreen,
}

impl DisplayMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Windowed => "WINDOWED",
            Self::Borderless => "BORDERLESS",
            Self::ExclusiveFullscreen => "FULLSCREEN",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Windowed => Self::Borderless,
            Self::Borderless => Self::ExclusiveFullscreen,
            Self::ExclusiveFullscreen => Self::Windowed,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Windowed => Self::ExclusiveFullscreen,
            Self::Borderless => Self::Windowed,
            Self::ExclusiveFullscreen => Self::Borderless,
        }
    }
}

/// GPU Graphics Backend selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GpuBackendSetting {
    /// Hardware adapter, falling back to WARP when there is none.
    #[default]
    Auto,
    /// Always WARP (Direct3D 11 on the CPU): for broken GPU drivers.
    Warp,
}

impl GpuBackendSetting {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "AUTO",
            Self::Warp => "WARP (CPU)",
        }
    }

    /// Reads a saved value; settings from before ADR-026 map onto the
    /// Direct3D 11 options ("SOFTWARE (CPU)" was the removed CPU renderer).
    pub fn parse(s: &str) -> Self {
        match s {
            "WARP (CPU)" | "SOFTWARE (CPU)" => Self::Warp,
            _ => Self::Auto,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Auto => Self::Warp,
            Self::Warp => Self::Auto,
        }
    }

    pub fn prev(self) -> Self {
        self.next()
    }
}

/// Playfield Track BGA underlay opacity setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrackBgaSetting {
    #[default]
    Off,
    Low,
    Medium,
    High,
}

impl TrackBgaSetting {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "OFF (0%)",
            Self::Low => "LOW (25%)",
            Self::Medium => "MEDIUM (50%)",
            Self::High => "HIGH (75%)",
        }
    }

    pub fn opacity(&self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Low => 0.25,
            Self::Medium => 0.50,
            Self::High => 0.75,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Low,
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High => Self::Off,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Off => Self::High,
            Self::Low => Self::Off,
            Self::Medium => Self::Low,
            Self::High => Self::Medium,
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "LOW" | "25" | "25%" | "LOW (25%)" => Self::Low,
            "MEDIUM" | "MED" | "50" | "50%" | "MEDIUM (50%)" => Self::Medium,
            "HIGH" | "75" | "75%" | "HIGH (75%)" => Self::High,
            _ => Self::Off,
        }
    }
}

/// Green number range in ms (the Settings and play panel steps stay inside it).
pub const GREEN_MS_MIN: u32 = 100;
pub const GREEN_MS_MAX: u32 = 2000;

/// The green number a file written before U2b meant: its px/s hi-speed at the
/// saved lane cover, rounded to the 10 ms step.
pub fn green_ms_from_hi_speed(px_per_sec: f32, lane_cover: f32) -> u32 {
    let ms = px_per_sec_to_green_ms(px_per_sec, lane_cover);
    (((ms / 10.0).round() as u32) * 10).clamp(GREEN_MS_MIN, GREEN_MS_MAX)
}

/// Persistent application configuration.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub play_options: PlayOptions,
    pub lane_cover_ratio: f32,
    /// Where the single play playfield sits.
    pub field_position: FieldPosition,
    /// Scratch side of 5K, 7K and 8K.
    pub scratch_sides: [ScratchSide; 3],
    pub eight_k_form: EightKForm,
    pub sort_mode: SortMode,
    /// The folder song select opens in (ids from `folders.rs`, `folder_path=mode/7k`).
    pub folder_path: FolderPath,
    /// The chart each group shows, away from its default (`chart_choice=<group key hex>:<chart id>`).
    pub chart_choices: ChartChoices,
    /// The song list filter (`filter_modes`, `filter_level_min` / `_max` (0 = no bound),
    /// `filter_unplayed`, `filter_uncleared`). Defaults to no filter.
    pub filter: Filter,
    /// Key layout per key mode, in `input::MODE_SLOTS` order (`None` = not
    /// in the file yet).
    pub key_layouts: [Option<SavedLayout>; 8],
    /// The single layout older versions shared across all modes
    /// (`key_preset` / `custom_key_bindings`); read only for migration.
    pub legacy_key_layout: Option<SavedLayout>,
    pub master_volume: f32,
    pub display_mode: DisplayMode,
    pub gpu_backend: GpuBackendSetting,
    pub window_width: u32,
    pub window_height: u32,
    pub target_fps: u32,
    pub track_bga: TrackBgaSetting,
    /// BGA images and videos: when off they are neither decoded nor drawn.
    pub bga_enabled: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            play_options: PlayOptions::default(),
            lane_cover_ratio: 0.0,
            field_position: FieldPosition::Left,
            scratch_sides: [ScratchSide::Left; 3],
            eight_k_form: EightKForm::Inline,
            sort_mode: SortMode::Title,
            folder_path: FolderPath::top("all"),
            chart_choices: ChartChoices::new(),
            filter: Filter::default(),
            key_layouts: Default::default(),
            legacy_key_layout: None,
            master_volume: 1.0,
            display_mode: DisplayMode::Windowed,
            gpu_backend: GpuBackendSetting::Auto,
            window_width: 1280,
            window_height: 720,
            target_fps: 240,
            track_bga: TrackBgaSetting::Off,
            bga_enabled: true,
        }
    }
}

impl AppConfig {
    /// Loads configuration from `config.dat` or returns default.
    pub fn load() -> Self {
        let path = Path::new(CONFIG_FILE);
        if !path.exists() {
            return Self::default();
        }

        let Ok(data) = fs::read_to_string(path) else {
            return Self::default();
        };

        Self::parse_str(&data)
    }

    /// Saves configuration to `config.dat`.
    pub fn save(&self) {
        let content = self.serialize_str();
        let _ = fs::write(CONFIG_FILE, content);
    }

    fn parse_str(data: &str) -> Self {
        let mut config = Self::default();
        let mut presets: [Option<KeyPreset>; 8] = [None; 8];
        let mut bindings: [String; 8] = Default::default();
        let (mut legacy_preset, mut legacy_bindings) = (None, String::new());
        // `scratch_side` was one setting for 5K and 7K before each mode got its own.
        let mut legacy_side = None;
        let mut side_set = [false; 3];
        // Files before U2b stored px/s (`hi_speed`); `green_ms` wins when both exist.
        let (mut old_hi_speed, mut green_seen) = (None, false);

        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.splitn(2, '=').collect();
            if parts.len() != 2 {
                continue;
            }

            let key = parts[0].trim();
            let val = parts[1].trim();

            match key {
                "green_ms" => {
                    if let Ok(v) = val.parse::<u32>() {
                        config.play_options.green_ms = v.clamp(GREEN_MS_MIN, GREEN_MS_MAX);
                        green_seen = true;
                    }
                }
                "hi_speed" => {
                    if let Ok(v) = val.parse::<f32>() {
                        old_hi_speed = Some(v.clamp(100.0, 1200.0));
                    }
                }
                "lane_cover_ratio" => {
                    if let Ok(v) = val.parse::<f32>() {
                        config.lane_cover_ratio = v.clamp(0.0, 0.85);
                    }
                }
                "lane_modifier" => {
                    config.play_options.lane_modifier = match val {
                        "MIRROR" => LaneModifier::Mirror,
                        "RANDOM" => LaneModifier::Random,
                        "R-RANDOM" => LaneModifier::RRandom,
                        "S-RANDOM" => LaneModifier::SRandom,
                        _ => LaneModifier::Regular,
                    };
                }
                "gauge_type" => {
                    config.play_options.gauge_type = match val {
                        "EASY" => GaugeType::Easy,
                        "HARD" => GaugeType::Hard,
                        "HAZARD" => GaugeType::Hazard,
                        _ => GaugeType::Groove,
                    };
                }
                "ln_mode" => {
                    config.play_options.ln = LnOption::from_name(val).unwrap_or_default();
                }
                "judge_offset_ms" => {
                    if let Ok(v) = val.parse::<f64>() {
                        config.play_options.judge_offset_ms = v.clamp(-100.0, 100.0);
                    }
                }
                "folder_path" => config.folder_path = FolderPath::parse(val),
                "filter_modes" => {
                    for id in val.split(',') {
                        if let Some(mode) = mode_from_id(id.trim()) {
                            if !config.filter.modes.contains(&mode) {
                                config.filter.modes.push(mode);
                            }
                        }
                    }
                }
                "filter_level_min" => {
                    config.filter.level_min = val.parse::<u32>().ok().filter(|&v| v > 0);
                }
                "filter_level_max" => {
                    config.filter.level_max = val.parse::<u32>().ok().filter(|&v| v > 0);
                }
                "filter_unplayed" => config.filter.only_unplayed = val == "1",
                "filter_uncleared" => config.filter.only_uncleared = val == "1",
                "chart_choice" => {
                    if let Some((key, id)) = val.split_once(':') {
                        if let (Ok(key), Some(id)) =
                            (u64::from_str_radix(key, 16), ChartId::from_hex(id))
                        {
                            config.chart_choices.insert(key, id);
                        }
                    }
                }
                "sort_mode" => {
                    config.sort_mode = match val {
                        "LEVEL" => SortMode::Level,
                        "CLEAR LAMP" => SortMode::ClearLamp,
                        "SCORE RATE" => SortMode::ScoreRate,
                        "BPM" => SortMode::Bpm,
                        _ => SortMode::Title,
                    };
                }
                "key_preset" => legacy_preset = KeyPreset::from_id(val),
                "custom_key_bindings" => legacy_bindings = val.to_string(),
                "master_volume" => {
                    if let Ok(v) = val.parse::<f32>() {
                        config.master_volume = v.clamp(0.0, 2.0);
                    }
                }
                "display_mode" => {
                    config.display_mode = match val {
                        "BORDERLESS" => DisplayMode::Borderless,
                        "FULLSCREEN" => DisplayMode::ExclusiveFullscreen,
                        _ => DisplayMode::Windowed,
                    };
                }
                "gpu_backend" => {
                    config.gpu_backend = GpuBackendSetting::parse(val);
                }
                "window_width" => {
                    if let Ok(w) = val.parse::<u32>() {
                        config.window_width = w.clamp(640, 7680);
                    }
                }
                "window_height" => {
                    if let Ok(h) = val.parse::<u32>() {
                        config.window_height = h.clamp(480, 4320);
                    }
                }
                "target_fps" => {
                    if let Ok(fps) = val.parse::<u32>() {
                        config.target_fps = fps;
                    }
                }
                "field_position" => config.field_position = FieldPosition::from_name(val),
                "scratch_side" => legacy_side = Some(ScratchSide::from_name(val)),
                "scratch_side_5k" | "scratch_side_7k" | "scratch_side_8k" => {
                    let i = match key {
                        "scratch_side_5k" => 0,
                        "scratch_side_7k" => 1,
                        _ => 2,
                    };
                    config.scratch_sides[i] = ScratchSide::from_name(val);
                    side_set[i] = true;
                }
                "eight_k_form" => config.eight_k_form = EightKForm::from_name(val),
                "bga" => config.bga_enabled = val != "OFF",
                "track_bga" => {
                    config.track_bga = TrackBgaSetting::from_str(val);
                }
                _ => {
                    for (i, &mode) in MODE_SLOTS.iter().enumerate() {
                        let slot = mode_slot_name(mode);
                        if key.strip_prefix("key_preset_") == Some(slot) {
                            presets[i] = KeyPreset::from_id(val);
                        } else if key.strip_prefix("key_bindings_") == Some(slot) {
                            bindings[i] = val.to_string();
                        }
                    }
                }
            }
        }

        if let (false, Some(px)) = (green_seen, old_hi_speed) {
            config.play_options.green_ms = green_ms_from_hi_speed(px, config.lane_cover_ratio);
        }
        for i in 0..MODE_SLOTS.len() {
            config.key_layouts[i] = presets[i].map(|p| (p, std::mem::take(&mut bindings[i])));
        }
        config.legacy_key_layout = legacy_preset.map(|p| (p, legacy_bindings));
        if let Some(side) = legacy_side {
            for i in 0..2 {
                if !side_set[i] {
                    config.scratch_sides[i] = side;
                }
            }
        }
        config
    }

    fn serialize_str(&self) -> String {
        let mut out = format!(
            "green_ms={}\nlane_cover_ratio={:.2}\nlane_modifier={}\ngauge_type={}\nln_mode={}\njudge_offset_ms={:.1}\nsort_mode={}\nfolder_path={}\nmaster_volume={:.2}\ndisplay_mode={}\ngpu_backend={}\nwindow_width={}\nwindow_height={}\ntarget_fps={}\nbga={}\ntrack_bga={}\nfield_position={}\nscratch_side_5k={}\nscratch_side_7k={}\nscratch_side_8k={}\neight_k_form={}\n",
            self.play_options.green_ms,
            self.lane_cover_ratio,
            self.play_options.lane_modifier.as_str(),
            self.play_options.gauge_type.as_str(),
            self.play_options.ln.as_str(),
            self.play_options.judge_offset_ms,
            self.sort_mode.as_str(),
            self.folder_path.to_config_string(),
            self.master_volume,
            self.display_mode.as_str(),
            self.gpu_backend.as_str(),
            self.window_width,
            self.window_height,
            self.target_fps,
            if self.bga_enabled { "ON" } else { "OFF" },
            self.track_bga.as_str(),
            self.field_position.as_str(),
            self.scratch_sides[0].as_str(),
            self.scratch_sides[1].as_str(),
            self.scratch_sides[2].as_str(),
            self.eight_k_form.id(),
        );
        for (i, &mode) in MODE_SLOTS.iter().enumerate() {
            if let Some((preset, bindings)) = &self.key_layouts[i] {
                let slot = mode_slot_name(mode);
                out.push_str(&format!(
                    "key_preset_{slot}={}\nkey_bindings_{slot}={bindings}\n",
                    preset.id()
                ));
            }
        }
        // Sorted, so the same choices always write the same file.
        let mut choices: Vec<_> = self.chart_choices.iter().collect();
        choices.sort_by_key(|(key, _)| **key);
        for (key, id) in choices {
            out.push_str(&format!(
                "chart_choice={key:016x}:{}
",
                id.to_hex()
            ));
        }
        let mut modes: Vec<&str> = self.filter.modes.iter().map(|&m| mode_id(m)).collect();
        modes.sort_unstable();
        out.push_str(&format!(
            "filter_modes={}
filter_level_min={}
filter_level_max={}
filter_unplayed={}
filter_uncleared={}
",
            modes.join(","),
            self.filter.level_min.unwrap_or(0),
            self.filter.level_max.unwrap_or(0),
            u8::from(self.filter.only_unplayed),
            u8::from(self.filter.only_uncleared),
        ));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beetle_core::PlayMode;

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = AppConfig {
            play_options: PlayOptions {
                green_ms: 1080,
                lane_modifier: LaneModifier::Random,
                gauge_type: GaugeType::Hard,
                ln: LnOption::Cn,
                judge_offset_ms: -4.0,
            },
            lane_cover_ratio: 0.25,
            field_position: FieldPosition::Center,
            scratch_sides: [ScratchSide::Right, ScratchSide::Left, ScratchSide::Right],
            eight_k_form: EightKForm::Triggers,
            sort_mode: SortMode::Level,
            folder_path: FolderPath::parse("mode/7k"),
            chart_choices: ChartChoices::new(),
            filter: Filter {
                modes: vec![PlayMode::Keys14, PlayMode::Keys7],
                level_min: Some(10),
                level_max: None,
                only_unplayed: true,
                only_uncleared: false,
            },
            key_layouts: [
                Some((KeyPreset::Custom, "Scratch:KeyA,Key1:KeyZ".to_string())),
                Some((KeyPreset::ArcadeZx, String::new())),
                Some((KeyPreset::Pms9K, String::new())),
                Some((KeyPreset::DoublePlay, String::new())),
                Some((KeyPreset::DoublePlay, "P2Scratch:KeyQ".to_string())),
                Some((KeyPreset::Ue4K, String::new())),
                None,
                Some((KeyPreset::Ue8K, String::new())),
            ],
            legacy_key_layout: None,
            master_volume: 0.85,
            display_mode: DisplayMode::Borderless,
            gpu_backend: GpuBackendSetting::Warp,
            window_width: 1920,
            window_height: 1080,
            target_fps: 360,
            track_bga: TrackBgaSetting::Medium,
            bga_enabled: false,
        };

        let serialized = config.serialize_str();
        let parsed = AppConfig::parse_str(&serialized);

        assert_eq!(config.play_options.green_ms, parsed.play_options.green_ms);
        assert_eq!(
            config.play_options.lane_modifier,
            parsed.play_options.lane_modifier
        );
        assert_eq!(
            config.play_options.gauge_type,
            parsed.play_options.gauge_type
        );
        assert_eq!(config.play_options.ln, parsed.play_options.ln);
        assert_eq!(
            config.play_options.judge_offset_ms,
            parsed.play_options.judge_offset_ms
        );
        assert_eq!(config.lane_cover_ratio, parsed.lane_cover_ratio);
        assert_eq!(config.sort_mode, parsed.sort_mode);
        assert_eq!(config.folder_path, parsed.folder_path);
        assert_eq!(config.key_layouts, parsed.key_layouts);
        assert_eq!(parsed.legacy_key_layout, None);
        assert_eq!(config.master_volume, parsed.master_volume);
        assert_eq!(config.display_mode, parsed.display_mode);
        assert_eq!(config.gpu_backend, parsed.gpu_backend);
        assert_eq!(config.window_width, parsed.window_width);
        assert_eq!(config.window_height, parsed.window_height);
        assert_eq!(config.target_fps, parsed.target_fps);
        assert_eq!(config.track_bga, parsed.track_bga);
        assert_eq!(config.bga_enabled, parsed.bga_enabled);
        assert_eq!(config.field_position, parsed.field_position);
        assert_eq!(config.scratch_sides, parsed.scratch_sides);
        assert_eq!(config.eight_k_form, parsed.eight_k_form);
    }

    #[test]
    fn chart_choices_round_trip_and_bad_lines_are_skipped() {
        let mut config = AppConfig::default();
        config.chart_choices.insert(0xabc, ChartId::synthetic(7));
        config.chart_choices.insert(0x12, ChartId::synthetic(8));
        let text = config.serialize_str();
        // Sorted by key, one line each.
        let lines: Vec<&str> = text
            .lines()
            .filter(|l| l.starts_with("chart_choice="))
            .collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("chart_choice=0000000000000012:"));
        let parsed = AppConfig::parse_str(&text);
        assert_eq!(parsed.chart_choices, config.chart_choices);
        // A line with a bad key or id is ignored.
        let bad = AppConfig::parse_str(
            "chart_choice=zz:sha256:00
chart_choice=12
",
        );
        assert!(bad.chart_choices.is_empty());
    }

    #[test]
    fn the_filter_round_trips_and_defaults_to_no_filter() {
        let text = AppConfig::default().serialize_str();
        assert!(text.contains(
            "filter_modes=
filter_level_min=0
filter_level_max=0
"
        ));
        assert_eq!(AppConfig::parse_str(&text).filter, Filter::default());

        let config = AppConfig::parse_str(
            "filter_modes=14k,7k,bogus,7k
filter_level_min=3
filter_level_max=0
filter_unplayed=1
filter_uncleared=0
",
        );
        assert_eq!(config.filter.modes, [PlayMode::Keys14, PlayMode::Keys7]);
        assert_eq!(config.filter.level_min, Some(3));
        assert_eq!(config.filter.level_max, None);
        assert!(config.filter.only_unplayed);
        assert!(!config.filter.only_uncleared);

        let mut saved = AppConfig::default();
        saved.filter.modes = vec![PlayMode::Keys5];
        saved.filter.level_max = Some(9);
        saved.filter.only_uncleared = true;
        assert_eq!(
            AppConfig::parse_str(&saved.serialize_str()).filter,
            saved.filter
        );
    }

    #[test]
    fn the_folder_defaults_to_all_songs_and_is_read_back() {
        // A file from before the folder tree has no `folder_path`: it opens in 전체 곡.
        assert_eq!(
            AppConfig::parse_str("sort_mode=TITLE\n").folder_path,
            FolderPath::top("all")
        );
        assert_eq!(
            AppConfig::parse_str("folder_path=level/12\n").folder_path,
            FolderPath::parse("level/12")
        );
    }

    #[test]
    fn test_old_scratch_side_applies_to_5k_and_7k() {
        let parsed = AppConfig::parse_str("scratch_side=RIGHT\n");
        assert_eq!(
            parsed.scratch_sides,
            [ScratchSide::Right, ScratchSide::Right, ScratchSide::Left]
        );
        // A per-mode value wins, whichever line comes first.
        let parsed = AppConfig::parse_str("scratch_side_7k=LEFT\nscratch_side=RIGHT\n");
        assert_eq!(
            parsed.scratch_sides,
            [ScratchSide::Right, ScratchSide::Left, ScratchSide::Left]
        );
    }

    #[test]
    fn old_hi_speed_file_converts_to_green_with_its_lane_cover() {
        // 1150 px/s at a 5 % cover: 558.4 units visible -> 485.6 ms -> 490 ms.
        let parsed = AppConfig::parse_str(
            "hi_speed=1150.0
lane_cover_ratio=0.05
",
        );
        assert_eq!(parsed.play_options.green_ms, 490);
        // The cover key may come after the old speed.
        let parsed = AppConfig::parse_str(
            "hi_speed=400.0
",
        );
        assert_eq!(parsed.play_options.green_ms, 1480);
        assert_eq!(green_ms_from_hi_speed(1125.0, 0.0), 530);
    }

    #[test]
    fn green_ms_wins_over_an_old_hi_speed_line() {
        let parsed = AppConfig::parse_str(
            "hi_speed=1150.0
green_ms=700
",
        );
        assert_eq!(parsed.play_options.green_ms, 700);
    }

    #[test]
    fn green_ms_is_clamped_and_saved_without_hi_speed() {
        assert_eq!(
            AppConfig::parse_str(
                "green_ms=5
"
            )
            .play_options
            .green_ms,
            GREEN_MS_MIN
        );
        assert_eq!(
            AppConfig::parse_str(
                "green_ms=99999
"
            )
            .play_options
            .green_ms,
            GREEN_MS_MAX
        );
        let config = AppConfig {
            play_options: PlayOptions {
                green_ms: 530,
                ..PlayOptions::default()
            },
            ..AppConfig::default()
        };
        let text = config.serialize_str();
        assert!(
            text.contains(
                "
green_ms=530
"
            ) || text.starts_with(
                "green_ms=530
"
            )
        );
        assert!(!text.contains("hi_speed"));
        assert_eq!(AppConfig::parse_str(&text).play_options.green_ms, 530);
    }
}

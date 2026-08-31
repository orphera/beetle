use beetle_core::{GaugeType, LaneModifier, PlayOptions, SortMode};
use crate::input::KeyPreset;
use std::fs;
use std::path::Path;

pub const CONFIG_FILE: &str = "config.dat";

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

/// Persistent application configuration.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub play_options: PlayOptions,
    pub lane_cover_ratio: f32,
    pub sort_mode: SortMode,
    pub key_preset: KeyPreset,
    pub custom_key_bindings: String,
    pub master_volume: f32,
    pub display_mode: DisplayMode,
    pub window_width: u32,
    pub window_height: u32,
    pub target_fps: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            play_options: PlayOptions::default(),
            lane_cover_ratio: 0.0,
            sort_mode: SortMode::Title,
            key_preset: KeyPreset::HomeRow,
            custom_key_bindings: String::new(),
            master_volume: 1.0,
            display_mode: DisplayMode::Windowed,
            window_width: 1024,
            window_height: 768,
            target_fps: 240,
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
                "hi_speed" => {
                    if let Ok(v) = val.parse::<f32>() {
                        config.play_options.hi_speed = v.clamp(100.0, 1200.0);
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
                "judge_offset_ms" => {
                    if let Ok(v) = val.parse::<f64>() {
                        config.play_options.judge_offset_ms = v.clamp(-100.0, 100.0);
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
                "key_preset" => {
                    config.key_preset = match val {
                        "ArcadeZx" => KeyPreset::ArcadeZx,
                        "Custom" => KeyPreset::Custom,
                        _ => KeyPreset::HomeRow,
                    };
                }
                "custom_key_bindings" => {
                    config.custom_key_bindings = val.to_string();
                }
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
                _ => (),
            }
        }

        config
    }

    fn serialize_str(&self) -> String {
        let preset_str = match self.key_preset {
            KeyPreset::HomeRow => "HomeRow",
            KeyPreset::ArcadeZx => "ArcadeZx",
            KeyPreset::Custom => "Custom",
        };

        format!(
            "hi_speed={:.1}\nlane_cover_ratio={:.2}\nlane_modifier={}\ngauge_type={}\njudge_offset_ms={:.1}\nsort_mode={}\nkey_preset={}\ncustom_key_bindings={}\nmaster_volume={:.2}\ndisplay_mode={}\nwindow_width={}\nwindow_height={}\ntarget_fps={}\n",
            self.play_options.hi_speed,
            self.lane_cover_ratio,
            self.play_options.lane_modifier.as_str(),
            self.play_options.gauge_type.as_str(),
            self.play_options.judge_offset_ms,
            self.sort_mode.as_str(),
            preset_str,
            self.custom_key_bindings,
            self.master_volume,
            self.display_mode.as_str(),
            self.window_width,
            self.window_height,
            self.target_fps,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = AppConfig {
            play_options: PlayOptions {
                hi_speed: 550.0,
                lane_modifier: LaneModifier::Random,
                gauge_type: GaugeType::Hard,
                judge_offset_ms: -4.0,
            },
            lane_cover_ratio: 0.25,
            sort_mode: SortMode::Level,
            key_preset: KeyPreset::Custom,
            custom_key_bindings: "Scratch:KeyA,Key1:KeyZ".to_string(),
            master_volume: 0.85,
            display_mode: DisplayMode::Borderless,
            window_width: 1920,
            window_height: 1080,
            target_fps: 360,
        };

        let serialized = config.serialize_str();
        let parsed = AppConfig::parse_str(&serialized);

        assert_eq!(config.play_options.hi_speed, parsed.play_options.hi_speed);
        assert_eq!(config.play_options.lane_modifier, parsed.play_options.lane_modifier);
        assert_eq!(config.play_options.gauge_type, parsed.play_options.gauge_type);
        assert_eq!(config.play_options.judge_offset_ms, parsed.play_options.judge_offset_ms);
        assert_eq!(config.lane_cover_ratio, parsed.lane_cover_ratio);
        assert_eq!(config.sort_mode, parsed.sort_mode);
        assert_eq!(config.key_preset, parsed.key_preset);
        assert_eq!(config.custom_key_bindings, parsed.custom_key_bindings);
        assert_eq!(config.master_volume, parsed.master_volume);
        assert_eq!(config.display_mode, parsed.display_mode);
        assert_eq!(config.window_width, parsed.window_width);
        assert_eq!(config.window_height, parsed.window_height);
        assert_eq!(config.target_fps, parsed.target_fps);
    }
}

use beetle_core::Lane;
use std::collections::HashMap;
use winit::keyboard::{KeyCode, PhysicalKey};

/// Key mapping presets for 7K + 1S play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyPreset {
    /// Ergonomic Home Row layout: Left Shift (Scratch) + S D F Space J K L (Keys 1..7)
    HomeRow,
    /// Traditional Arcade / LR2 layout: Left Shift (Scratch) + Z S X D C F V (Keys 1..7)
    ArcadeZx,
}

impl KeyPreset {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HomeRow => "HomeRow (S D F Space J K L)",
            Self::ArcadeZx => "ArcadeZx (Z S X D C F V)",
        }
    }
}

/// Input configuration handling key mapping and preset switching.
#[derive(Debug, Clone)]
pub struct InputConfig {
    pub preset: KeyPreset,
    custom_bindings: HashMap<KeyCode, Lane>,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self::new(KeyPreset::HomeRow)
    }
}

impl InputConfig {
    pub fn new(preset: KeyPreset) -> Self {
        Self {
            preset,
            custom_bindings: HashMap::new(),
        }
    }

    /// Toggles between HomeRow and ArcadeZx presets.
    pub fn toggle_preset(&mut self) {
        self.preset = match self.preset {
            KeyPreset::HomeRow => KeyPreset::ArcadeZx,
            KeyPreset::ArcadeZx => KeyPreset::HomeRow,
        };
    }

    /// Adds or overrides a custom key binding.
    #[allow(dead_code)]
    pub fn bind_key(&mut self, key: KeyCode, lane: Lane) {
        self.custom_bindings.insert(key, lane);
    }

    /// Maps a winit PhysicalKey to a rhythm game Lane.
    pub fn map_key(&self, key: PhysicalKey) -> Option<Lane> {
        let PhysicalKey::Code(code) = key else {
            return None;
        };

        // Custom bindings take precedence
        if let Some(&lane) = self.custom_bindings.get(&code) {
            return Some(lane);
        }

        match self.preset {
            KeyPreset::HomeRow => match code {
                KeyCode::ShiftLeft | KeyCode::ControlLeft => Some(Lane::Scratch),
                KeyCode::KeyS => Some(Lane::Key1),
                KeyCode::KeyD => Some(Lane::Key2),
                KeyCode::KeyF => Some(Lane::Key3),
                KeyCode::Space => Some(Lane::Key4),
                KeyCode::KeyJ => Some(Lane::Key5),
                KeyCode::KeyK => Some(Lane::Key6),
                KeyCode::KeyL => Some(Lane::Key7),
                _ => None,
            },
            KeyPreset::ArcadeZx => match code {
                KeyCode::ShiftLeft | KeyCode::ControlLeft => Some(Lane::Scratch),
                KeyCode::KeyZ => Some(Lane::Key1),
                KeyCode::KeyS => Some(Lane::Key2),
                KeyCode::KeyX => Some(Lane::Key3),
                KeyCode::KeyD => Some(Lane::Key4),
                KeyCode::KeyC => Some(Lane::Key5),
                KeyCode::KeyF => Some(Lane::Key6),
                KeyCode::KeyV => Some(Lane::Key7),
                _ => None,
            },
        }
    }

    /// Returns the descriptive key name for a given lane.
    pub fn get_key_name_for_lane(&self, lane: Lane) -> &'static str {
        match self.preset {
            KeyPreset::HomeRow => match lane {
                Lane::Scratch => "Left Shift",
                Lane::Key1 => "S",
                Lane::Key2 => "D",
                Lane::Key3 => "F",
                Lane::Key4 => "Space",
                Lane::Key5 => "J",
                Lane::Key6 => "K",
                Lane::Key7 => "L",
            },
            KeyPreset::ArcadeZx => match lane {
                Lane::Scratch => "Left Shift",
                Lane::Key1 => "Z",
                Lane::Key2 => "S",
                Lane::Key3 => "X",
                Lane::Key4 => "D",
                Lane::Key5 => "C",
                Lane::Key6 => "F",
                Lane::Key7 => "V",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_presets() {
        let mut config = InputConfig::new(KeyPreset::HomeRow);
        assert_eq!(config.map_key(PhysicalKey::Code(KeyCode::KeyS)), Some(Lane::Key1));
        assert_eq!(config.map_key(PhysicalKey::Code(KeyCode::Space)), Some(Lane::Key4));
        assert_eq!(config.map_key(PhysicalKey::Code(KeyCode::KeyL)), Some(Lane::Key7));

        config.toggle_preset();
        assert_eq!(config.preset, KeyPreset::ArcadeZx);
        assert_eq!(config.map_key(PhysicalKey::Code(KeyCode::KeyZ)), Some(Lane::Key1));
        assert_eq!(config.map_key(PhysicalKey::Code(KeyCode::KeyS)), Some(Lane::Key2));
        assert_eq!(config.map_key(PhysicalKey::Code(KeyCode::KeyV)), Some(Lane::Key7));
    }

    #[test]
    fn test_custom_key_binding() {
        let mut config = InputConfig::new(KeyPreset::HomeRow);
        config.bind_key(KeyCode::KeyA, Lane::Scratch);
        assert_eq!(config.map_key(PhysicalKey::Code(KeyCode::KeyA)), Some(Lane::Scratch));
    }
}

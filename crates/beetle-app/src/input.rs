use beetle_core::{Lane, PlayMode};
use beetle_render::SkinConfig;
use std::collections::HashMap;
use winit::keyboard::{KeyCode, PhysicalKey};

/// Key mapping presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyPreset {
    /// Ergonomic Home Row layout: Left Shift (Scratch) + S D F Space J K L (Keys 1..7)
    HomeRow,
    /// Traditional Arcade / LR2 layout: Left Shift (Scratch) + Z S X D C F V (Keys 1..7)
    ArcadeZx,
    /// PMS (9-Key, no scratch): S D F Space J K L ; ' (Keys 1..9)
    Pms9K,
    /// Double Play (10K/14K): 1P side mirrors ArcadeZx, 2P side mirrors it
    /// on the right hand (RShift + U I O P [ ] \)
    DoublePlay,
    /// Custom user-defined key bindings
    Custom,
}

impl KeyPreset {
    /// Stable name stored in `config.dat`.
    pub fn id(self) -> &'static str {
        match self {
            Self::HomeRow => "HomeRow",
            Self::ArcadeZx => "ArcadeZx",
            Self::Pms9K => "Pms9K",
            Self::DoublePlay => "DoublePlay",
            Self::Custom => "Custom",
        }
    }

    pub fn from_id(s: &str) -> Option<Self> {
        [Self::HomeRow, Self::ArcadeZx, Self::Pms9K, Self::DoublePlay, Self::Custom]
            .into_iter()
            .find(|p| p.id() == s)
    }

    /// Built-in presets that bind every lane of `mode`.
    pub fn presets_for(mode: PlayMode) -> &'static [KeyPreset] {
        match mode {
            PlayMode::Keys5 | PlayMode::Keys7 => &[Self::HomeRow, Self::ArcadeZx],
            PlayMode::Keys9 => &[Self::Pms9K],
            PlayMode::Keys10 | PlayMode::Keys14 => &[Self::DoublePlay],
        }
    }

    pub fn default_for(mode: PlayMode) -> KeyPreset {
        Self::presets_for(mode)[0]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::HomeRow => "HomeRow (S D F Space J K L)",
            Self::ArcadeZx => "ArcadeZx (Z S X D C F V)",
            Self::Pms9K => "PMS 9K (S D F Space J K L ; ')",
            Self::DoublePlay => "Double Play (LShift+ZSXDCFV / RShift+UIOP[]\\)",
            Self::Custom => "Custom Layout",
        }
    }
}

/// Input configuration handling key mapping, custom 1:1 rebinding, and preset switching.
#[derive(Debug, Clone)]
pub struct InputConfig {
    pub preset: KeyPreset,
    pub custom_bindings: HashMap<KeyCode, Lane>,
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

    /// Next preset that fits `mode` (its built-ins, then Custom when custom
    /// bindings exist), wrapping around.
    pub fn cycle_preset(&mut self, mode: PlayMode) {
        let mut order: Vec<KeyPreset> = KeyPreset::presets_for(mode).to_vec();
        if !self.custom_bindings.is_empty() {
            order.push(KeyPreset::Custom);
        }
        let next = order
            .iter()
            .position(|&p| p == self.preset)
            .map_or(0, |i| (i + 1) % order.len());
        self.preset = order[next];
    }

    /// Whether every lane of `mode` has a key.
    pub fn covers(&self, mode: PlayMode) -> bool {
        lanes_for(mode)
            .iter()
            .all(|&lane| self.get_key_name_for_lane(lane) != "None")
    }

    /// Resets all bindings to a specific default preset.
    pub fn reset_to_preset(&mut self, preset: KeyPreset) {
        self.preset = preset;
        self.custom_bindings.clear();
    }

    /// Binds a physical key to a lane, resolving any duplicate conflicts automatically.
    pub fn bind_key(&mut self, key: KeyCode, lane: Lane) {
        // If switching from preset to custom, initialize custom map from current preset
        if self.preset != KeyPreset::Custom && self.custom_bindings.is_empty() {
            self.init_custom_from_preset(self.preset);
        }

        // 1. Remove any other key already mapped to this lane
        self.custom_bindings
            .retain(|_, &mut mapped_lane| mapped_lane != lane);

        // 2. Remove this key if it was mapped to another lane
        self.custom_bindings.remove(&key);

        // 3. Set new binding
        self.custom_bindings.insert(key, lane);
        self.preset = KeyPreset::Custom;
    }

    fn init_custom_from_preset(&mut self, preset: KeyPreset) {
        let pairs: Vec<(KeyCode, Lane)> = match preset {
            KeyPreset::HomeRow => vec![
                (KeyCode::ShiftLeft, Lane::Scratch),
                (KeyCode::KeyS, Lane::Key1),
                (KeyCode::KeyD, Lane::Key2),
                (KeyCode::KeyF, Lane::Key3),
                (KeyCode::Space, Lane::Key4),
                (KeyCode::KeyJ, Lane::Key5),
                (KeyCode::KeyK, Lane::Key6),
                (KeyCode::KeyL, Lane::Key7),
            ],
            KeyPreset::ArcadeZx | KeyPreset::Custom => vec![
                (KeyCode::ShiftLeft, Lane::Scratch),
                (KeyCode::KeyZ, Lane::Key1),
                (KeyCode::KeyS, Lane::Key2),
                (KeyCode::KeyX, Lane::Key3),
                (KeyCode::KeyD, Lane::Key4),
                (KeyCode::KeyC, Lane::Key5),
                (KeyCode::KeyF, Lane::Key6),
                (KeyCode::KeyV, Lane::Key7),
            ],
            // PMS has no scratch lane - 9 key buttons only.
            KeyPreset::Pms9K => vec![
                (KeyCode::KeyS, Lane::Key1),
                (KeyCode::KeyD, Lane::Key2),
                (KeyCode::KeyF, Lane::Key3),
                (KeyCode::Space, Lane::Key4),
                (KeyCode::KeyJ, Lane::Key5),
                (KeyCode::KeyK, Lane::Key6),
                (KeyCode::KeyL, Lane::Key7),
                (KeyCode::Semicolon, Lane::Key8),
                (KeyCode::Quote, Lane::Key9),
            ],
            // 1P side mirrors ArcadeZx; 2P side mirrors it on the right hand.
            KeyPreset::DoublePlay => vec![
                (KeyCode::ShiftLeft, Lane::Scratch),
                (KeyCode::KeyZ, Lane::Key1),
                (KeyCode::KeyS, Lane::Key2),
                (KeyCode::KeyX, Lane::Key3),
                (KeyCode::KeyD, Lane::Key4),
                (KeyCode::KeyC, Lane::Key5),
                (KeyCode::KeyF, Lane::Key6),
                (KeyCode::KeyV, Lane::Key7),
                (KeyCode::ShiftRight, Lane::P2Scratch),
                (KeyCode::KeyU, Lane::P2Key1),
                (KeyCode::KeyI, Lane::P2Key2),
                (KeyCode::KeyO, Lane::P2Key3),
                (KeyCode::KeyP, Lane::P2Key4),
                (KeyCode::BracketLeft, Lane::P2Key5),
                (KeyCode::BracketRight, Lane::P2Key6),
                (KeyCode::Backslash, Lane::P2Key7),
            ],
        };

        self.custom_bindings.clear();
        for (k, l) in pairs {
            self.custom_bindings.insert(k, l);
        }
    }

    /// Maps a winit PhysicalKey to a rhythm game Lane.
    pub fn map_key(&self, key: PhysicalKey) -> Option<Lane> {
        let PhysicalKey::Code(code) = key else {
            return None;
        };

        if self.preset == KeyPreset::Custom && !self.custom_bindings.is_empty() {
            return self.custom_bindings.get(&code).copied();
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
            KeyPreset::Pms9K => match code {
                KeyCode::KeyS => Some(Lane::Key1),
                KeyCode::KeyD => Some(Lane::Key2),
                KeyCode::KeyF => Some(Lane::Key3),
                KeyCode::Space => Some(Lane::Key4),
                KeyCode::KeyJ => Some(Lane::Key5),
                KeyCode::KeyK => Some(Lane::Key6),
                KeyCode::KeyL => Some(Lane::Key7),
                KeyCode::Semicolon => Some(Lane::Key8),
                KeyCode::Quote => Some(Lane::Key9),
                _ => None,
            },
            KeyPreset::DoublePlay => match code {
                KeyCode::ShiftLeft | KeyCode::ControlLeft => Some(Lane::Scratch),
                KeyCode::KeyZ => Some(Lane::Key1),
                KeyCode::KeyS => Some(Lane::Key2),
                KeyCode::KeyX => Some(Lane::Key3),
                KeyCode::KeyD => Some(Lane::Key4),
                KeyCode::KeyC => Some(Lane::Key5),
                KeyCode::KeyF => Some(Lane::Key6),
                KeyCode::KeyV => Some(Lane::Key7),
                KeyCode::ShiftRight | KeyCode::ControlRight => Some(Lane::P2Scratch),
                KeyCode::KeyU => Some(Lane::P2Key1),
                KeyCode::KeyI => Some(Lane::P2Key2),
                KeyCode::KeyO => Some(Lane::P2Key3),
                KeyCode::KeyP => Some(Lane::P2Key4),
                KeyCode::BracketLeft => Some(Lane::P2Key5),
                KeyCode::BracketRight => Some(Lane::P2Key6),
                KeyCode::Backslash => Some(Lane::P2Key7),
                _ => None,
            },
            KeyPreset::Custom => self.custom_bindings.get(&code).copied(),
        }
    }

    /// Returns the descriptive key name for a given lane.
    pub fn get_key_name_for_lane(&self, lane: Lane) -> String {
        if self.preset == KeyPreset::Custom && !self.custom_bindings.is_empty() {
            for (&code, &mapped_lane) in &self.custom_bindings {
                if mapped_lane == lane {
                    return key_code_to_str(code).to_string();
                }
            }
        }

        match self.preset {
            KeyPreset::HomeRow => match lane {
                Lane::Scratch => "LShift",
                Lane::Key1 => "S",
                Lane::Key2 => "D",
                Lane::Key3 => "F",
                Lane::Key4 => "Space",
                Lane::Key5 => "J",
                Lane::Key6 => "K",
                Lane::Key7 => "L",
                // HomeRow/ArcadeZx are 7K+1S-only presets (see KeyPreset docs);
                // PMS/DP lanes have no default binding under them yet.
                _ => "None",
            }
            .to_string(),
            KeyPreset::ArcadeZx => match lane {
                Lane::Scratch => "LShift",
                Lane::Key1 => "Z",
                Lane::Key2 => "S",
                Lane::Key3 => "X",
                Lane::Key4 => "D",
                Lane::Key5 => "C",
                Lane::Key6 => "F",
                Lane::Key7 => "V",
                _ => "None",
            }
            .to_string(),
            KeyPreset::Pms9K => match lane {
                Lane::Key1 => "S",
                Lane::Key2 => "D",
                Lane::Key3 => "F",
                Lane::Key4 => "Space",
                Lane::Key5 => "J",
                Lane::Key6 => "K",
                Lane::Key7 => "L",
                Lane::Key8 => ";",
                Lane::Key9 => "'",
                _ => "None",
            }
            .to_string(),
            KeyPreset::DoublePlay => match lane {
                Lane::Scratch => "LShift",
                Lane::Key1 => "Z",
                Lane::Key2 => "S",
                Lane::Key3 => "X",
                Lane::Key4 => "D",
                Lane::Key5 => "C",
                Lane::Key6 => "F",
                Lane::Key7 => "V",
                Lane::P2Scratch => "RShift",
                Lane::P2Key1 => "U",
                Lane::P2Key2 => "I",
                Lane::P2Key3 => "O",
                Lane::P2Key4 => "P",
                Lane::P2Key5 => "[",
                Lane::P2Key6 => "]",
                Lane::P2Key7 => "\\",
                _ => "None",
            }
            .to_string(),
            KeyPreset::Custom => "None".to_string(),
        }
    }

    /// Serializes custom bindings to a compact string format: "Scratch:ShiftLeft,Key1:KeyS,..."
    pub fn serialize_bindings(&self) -> String {
        let mut parts = Vec::new();
        for &lane in &Lane::ALL {
            if let Some((&code, _)) = self.custom_bindings.iter().find(|(_, &l)| l == lane) {
                parts.push(format!(
                    "{}:{}",
                    lane_to_name(lane),
                    key_code_to_identifier(code)
                ));
            }
        }
        parts.join(",")
    }

    /// Restores custom bindings from a compact serialized string.
    pub fn deserialize_bindings(&mut self, s: &str) {
        if s.trim().is_empty() {
            return;
        }

        self.custom_bindings.clear();
        for item in s.split(',') {
            let parts: Vec<&str> = item.splitn(2, ':').collect();
            if parts.len() == 2 {
                if let (Some(lane), Some(code)) =
                    (name_to_lane(parts[0]), identifier_to_key_code(parts[1]))
                {
                    self.custom_bindings.insert(code, lane);
                }
            }
        }
        if !self.custom_bindings.is_empty() {
            self.preset = KeyPreset::Custom;
        }
    }
}

/// Lanes of a key mode, left to right (same order as Key Config).
pub fn lanes_for(mode: PlayMode) -> &'static [Lane] {
    let mut skin = SkinConfig::default();
    skin.set_play_mode(mode);
    skin.active_lanes()
}

/// Key modes that each keep their own layout.
pub const MODE_SLOTS: [PlayMode; 5] = [
    PlayMode::Keys5,
    PlayMode::Keys7,
    PlayMode::Keys9,
    PlayMode::Keys10,
    PlayMode::Keys14,
];

/// `config.dat` suffix for a mode slot ("5k", "7k", ...).
pub fn mode_slot_name(mode: PlayMode) -> &'static str {
    match mode {
        PlayMode::Keys5 => "5k",
        PlayMode::Keys7 => "7k",
        PlayMode::Keys9 => "9k",
        PlayMode::Keys10 => "10k",
        PlayMode::Keys14 => "14k",
    }
}

fn slot(mode: PlayMode) -> usize {
    MODE_SLOTS.iter().position(|&m| m == mode).unwrap_or(1)
}

/// A saved layout: preset + serialized custom bindings.
pub type SavedLayout = (KeyPreset, String);

/// One key layout per key mode, so 5K, 7K, 9K and double play can each be
/// set up without disturbing the others.
#[derive(Debug, Clone)]
pub struct KeyBindings {
    layouts: [InputConfig; 5],
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            layouts: MODE_SLOTS.map(|m| InputConfig::new(KeyPreset::default_for(m))),
        }
    }
}

impl KeyBindings {
    /// Builds from `config.dat`: `saved[i]` is the layout stored for
    /// `MODE_SLOTS[i]`. Slots without one take the pre-per-mode `legacy`
    /// layout (one layout shared by all modes) when it fits that mode, else
    /// the mode's default preset.
    pub fn load(saved: &[Option<SavedLayout>; 5], legacy: Option<&SavedLayout>) -> Self {
        let restore = |(preset, bindings): &SavedLayout| {
            let mut cfg = InputConfig::new(*preset);
            cfg.deserialize_bindings(bindings);
            cfg.preset = *preset;
            cfg
        };
        let mut out = Self::default();
        for (i, &mode) in MODE_SLOTS.iter().enumerate() {
            if let Some(layout) = &saved[i] {
                out.layouts[i] = restore(layout);
            } else if let Some(layout) = legacy {
                let cfg = restore(layout);
                let fits = KeyPreset::presets_for(mode).contains(&cfg.preset)
                    || (cfg.preset == KeyPreset::Custom && cfg.covers(mode));
                if fits {
                    out.layouts[i] = cfg;
                }
            }
        }
        out
    }

    /// Layouts to store, in `MODE_SLOTS` order.
    pub fn to_saved(&self) -> [SavedLayout; 5] {
        std::array::from_fn(|i| (self.layouts[i].preset, self.layouts[i].serialize_bindings()))
    }

    pub fn get(&self, mode: PlayMode) -> &InputConfig {
        &self.layouts[slot(mode)]
    }

    pub fn get_mut(&mut self, mode: PlayMode) -> &mut InputConfig {
        &mut self.layouts[slot(mode)]
    }
}

/// Human-readable Key Config screen label for a lane (e.g. "KEY 1 (1P)").
/// Mode-independent: Key Config builds its row list from
/// `SkinConfig::active_lanes()` for the chart's `PlayMode`, so only lanes
/// that actually apply to the current mode ever get labeled here.
pub fn lane_label(lane: Lane) -> &'static str {
    match lane {
        Lane::Scratch => "SCRATCH (1S)",
        Lane::Key1 => "KEY 1 (1P)",
        Lane::Key2 => "KEY 2 (1P)",
        Lane::Key3 => "KEY 3 (1P)",
        Lane::Key4 => "KEY 4 (1P)",
        Lane::Key5 => "KEY 5 (1P)",
        Lane::Key6 => "KEY 6 (1P)",
        Lane::Key7 => "KEY 7 (1P)",
        Lane::Key8 => "KEY 8 (1P)",
        Lane::Key9 => "KEY 9 (1P)",
        Lane::P2Scratch => "SCRATCH (2S)",
        Lane::P2Key1 => "KEY 1 (2P)",
        Lane::P2Key2 => "KEY 2 (2P)",
        Lane::P2Key3 => "KEY 3 (2P)",
        Lane::P2Key4 => "KEY 4 (2P)",
        Lane::P2Key5 => "KEY 5 (2P)",
        Lane::P2Key6 => "KEY 6 (2P)",
        Lane::P2Key7 => "KEY 7 (2P)",
    }
}

pub fn lane_to_name(lane: Lane) -> &'static str {
    match lane {
        Lane::Scratch => "Scratch",
        Lane::Key1 => "Key1",
        Lane::Key2 => "Key2",
        Lane::Key3 => "Key3",
        Lane::Key4 => "Key4",
        Lane::Key5 => "Key5",
        Lane::Key6 => "Key6",
        Lane::Key7 => "Key7",
        Lane::Key8 => "Key8",
        Lane::Key9 => "Key9",
        Lane::P2Scratch => "P2Scratch",
        Lane::P2Key1 => "P2Key1",
        Lane::P2Key2 => "P2Key2",
        Lane::P2Key3 => "P2Key3",
        Lane::P2Key4 => "P2Key4",
        Lane::P2Key5 => "P2Key5",
        Lane::P2Key6 => "P2Key6",
        Lane::P2Key7 => "P2Key7",
    }
}

pub fn name_to_lane(s: &str) -> Option<Lane> {
    match s {
        "Scratch" => Some(Lane::Scratch),
        "Key1" => Some(Lane::Key1),
        "Key2" => Some(Lane::Key2),
        "Key3" => Some(Lane::Key3),
        "Key4" => Some(Lane::Key4),
        "Key5" => Some(Lane::Key5),
        "Key6" => Some(Lane::Key6),
        "Key7" => Some(Lane::Key7),
        "Key8" => Some(Lane::Key8),
        "Key9" => Some(Lane::Key9),
        "P2Scratch" => Some(Lane::P2Scratch),
        "P2Key1" => Some(Lane::P2Key1),
        "P2Key2" => Some(Lane::P2Key2),
        "P2Key3" => Some(Lane::P2Key3),
        "P2Key4" => Some(Lane::P2Key4),
        "P2Key5" => Some(Lane::P2Key5),
        "P2Key6" => Some(Lane::P2Key6),
        "P2Key7" => Some(Lane::P2Key7),
        _ => None,
    }
}

pub fn key_code_to_str(code: KeyCode) -> &'static str {
    match code {
        KeyCode::KeyA => "A",
        KeyCode::KeyB => "B",
        KeyCode::KeyC => "C",
        KeyCode::KeyD => "D",
        KeyCode::KeyE => "E",
        KeyCode::KeyF => "F",
        KeyCode::KeyG => "G",
        KeyCode::KeyH => "H",
        KeyCode::KeyI => "I",
        KeyCode::KeyJ => "J",
        KeyCode::KeyK => "K",
        KeyCode::KeyL => "L",
        KeyCode::KeyM => "M",
        KeyCode::KeyN => "N",
        KeyCode::KeyO => "O",
        KeyCode::KeyP => "P",
        KeyCode::KeyQ => "Q",
        KeyCode::KeyR => "R",
        KeyCode::KeyS => "S",
        KeyCode::KeyT => "T",
        KeyCode::KeyU => "U",
        KeyCode::KeyV => "V",
        KeyCode::KeyW => "W",
        KeyCode::KeyX => "X",
        KeyCode::KeyY => "Y",
        KeyCode::KeyZ => "Z",

        KeyCode::Digit0 => "0",
        KeyCode::Digit1 => "1",
        KeyCode::Digit2 => "2",
        KeyCode::Digit3 => "3",
        KeyCode::Digit4 => "4",
        KeyCode::Digit5 => "5",
        KeyCode::Digit6 => "6",
        KeyCode::Digit7 => "7",
        KeyCode::Digit8 => "8",
        KeyCode::Digit9 => "9",

        KeyCode::Space => "Space",
        KeyCode::ShiftLeft => "LShift",
        KeyCode::ShiftRight => "RShift",
        KeyCode::ControlLeft => "LCtrl",
        KeyCode::ControlRight => "RCtrl",
        KeyCode::AltLeft => "LAlt",
        KeyCode::AltRight => "RAlt",
        KeyCode::Tab => "Tab",
        KeyCode::Enter => "Enter",
        KeyCode::Escape => "Esc",
        KeyCode::Backspace => "Backspace",
        KeyCode::CapsLock => "Caps",

        KeyCode::Semicolon => ";",
        KeyCode::Quote => "'",
        KeyCode::Comma => ",",
        KeyCode::Period => ".",
        KeyCode::Slash => "/",
        KeyCode::Backslash => "\\",
        KeyCode::BracketLeft => "[",
        KeyCode::BracketRight => "]",
        KeyCode::Minus => "-",
        KeyCode::Equal => "=",
        KeyCode::Backquote => "`",

        KeyCode::ArrowLeft => "Left",
        KeyCode::ArrowRight => "Right",
        KeyCode::ArrowUp => "Up",
        KeyCode::ArrowDown => "Down",

        KeyCode::Numpad0 => "Num0",
        KeyCode::Numpad1 => "Num1",
        KeyCode::Numpad2 => "Num2",
        KeyCode::Numpad3 => "Num3",
        KeyCode::Numpad4 => "Num4",
        KeyCode::Numpad5 => "Num5",
        KeyCode::Numpad6 => "Num6",
        KeyCode::Numpad7 => "Num7",
        KeyCode::Numpad8 => "Num8",
        KeyCode::Numpad9 => "Num9",
        KeyCode::NumpadEnter => "NumEnter",
        KeyCode::NumpadAdd => "Num+",
        KeyCode::NumpadSubtract => "Num-",
        KeyCode::NumpadMultiply => "Num*",
        KeyCode::NumpadDivide => "Num/",
        KeyCode::NumpadDecimal => "Num.",

        _ => "Key",
    }
}

pub fn key_code_to_identifier(code: KeyCode) -> &'static str {
    match code {
        KeyCode::KeyA => "KeyA",
        KeyCode::KeyB => "KeyB",
        KeyCode::KeyC => "KeyC",
        KeyCode::KeyD => "KeyD",
        KeyCode::KeyE => "KeyE",
        KeyCode::KeyF => "KeyF",
        KeyCode::KeyG => "KeyG",
        KeyCode::KeyH => "KeyH",
        KeyCode::KeyI => "KeyI",
        KeyCode::KeyJ => "KeyJ",
        KeyCode::KeyK => "KeyK",
        KeyCode::KeyL => "KeyL",
        KeyCode::KeyM => "KeyM",
        KeyCode::KeyN => "KeyN",
        KeyCode::KeyO => "KeyO",
        KeyCode::KeyP => "KeyP",
        KeyCode::KeyQ => "KeyQ",
        KeyCode::KeyR => "KeyR",
        KeyCode::KeyS => "KeyS",
        KeyCode::KeyT => "KeyT",
        KeyCode::KeyU => "KeyU",
        KeyCode::KeyV => "KeyV",
        KeyCode::KeyW => "KeyW",
        KeyCode::KeyX => "KeyX",
        KeyCode::KeyY => "KeyY",
        KeyCode::KeyZ => "KeyZ",

        KeyCode::Digit0 => "Digit0",
        KeyCode::Digit1 => "Digit1",
        KeyCode::Digit2 => "Digit2",
        KeyCode::Digit3 => "Digit3",
        KeyCode::Digit4 => "Digit4",
        KeyCode::Digit5 => "Digit5",
        KeyCode::Digit6 => "Digit6",
        KeyCode::Digit7 => "Digit7",
        KeyCode::Digit8 => "Digit8",
        KeyCode::Digit9 => "Digit9",

        KeyCode::Space => "Space",
        KeyCode::ShiftLeft => "ShiftLeft",
        KeyCode::ShiftRight => "ShiftRight",
        KeyCode::ControlLeft => "ControlLeft",
        KeyCode::ControlRight => "ControlRight",
        KeyCode::AltLeft => "AltLeft",
        KeyCode::AltRight => "AltRight",
        KeyCode::Tab => "Tab",
        KeyCode::Enter => "Enter",
        KeyCode::Escape => "Escape",
        KeyCode::Backspace => "Backspace",
        KeyCode::CapsLock => "CapsLock",

        KeyCode::Semicolon => "Semicolon",
        KeyCode::Quote => "Quote",
        KeyCode::Comma => "Comma",
        KeyCode::Period => "Period",
        KeyCode::Slash => "Slash",
        KeyCode::Backslash => "Backslash",
        KeyCode::BracketLeft => "BracketLeft",
        KeyCode::BracketRight => "BracketRight",
        KeyCode::Minus => "Minus",
        KeyCode::Equal => "Equal",
        KeyCode::Backquote => "Backquote",

        KeyCode::ArrowLeft => "ArrowLeft",
        KeyCode::ArrowRight => "ArrowRight",
        KeyCode::ArrowUp => "ArrowUp",
        KeyCode::ArrowDown => "ArrowDown",

        KeyCode::Numpad0 => "Numpad0",
        KeyCode::Numpad1 => "Numpad1",
        KeyCode::Numpad2 => "Numpad2",
        KeyCode::Numpad3 => "Numpad3",
        KeyCode::Numpad4 => "Numpad4",
        KeyCode::Numpad5 => "Numpad5",
        KeyCode::Numpad6 => "Numpad6",
        KeyCode::Numpad7 => "Numpad7",
        KeyCode::Numpad8 => "Numpad8",
        KeyCode::Numpad9 => "Numpad9",
        KeyCode::NumpadEnter => "NumpadEnter",
        KeyCode::NumpadAdd => "NumpadAdd",
        KeyCode::NumpadSubtract => "NumpadSubtract",
        KeyCode::NumpadMultiply => "NumpadMultiply",
        KeyCode::NumpadDivide => "NumpadDivide",
        KeyCode::NumpadDecimal => "NumpadDecimal",

        _ => "Unknown",
    }
}

pub fn identifier_to_key_code(s: &str) -> Option<KeyCode> {
    match s {
        "KeyA" => Some(KeyCode::KeyA),
        "KeyB" => Some(KeyCode::KeyB),
        "KeyC" => Some(KeyCode::KeyC),
        "KeyD" => Some(KeyCode::KeyD),
        "KeyE" => Some(KeyCode::KeyE),
        "KeyF" => Some(KeyCode::KeyF),
        "KeyG" => Some(KeyCode::KeyG),
        "KeyH" => Some(KeyCode::KeyH),
        "KeyI" => Some(KeyCode::KeyI),
        "KeyJ" => Some(KeyCode::KeyJ),
        "KeyK" => Some(KeyCode::KeyK),
        "KeyL" => Some(KeyCode::KeyL),
        "KeyM" => Some(KeyCode::KeyM),
        "KeyN" => Some(KeyCode::KeyN),
        "KeyO" => Some(KeyCode::KeyO),
        "KeyP" => Some(KeyCode::KeyP),
        "KeyQ" => Some(KeyCode::KeyQ),
        "KeyR" => Some(KeyCode::KeyR),
        "KeyS" => Some(KeyCode::KeyS),
        "KeyT" => Some(KeyCode::KeyT),
        "KeyU" => Some(KeyCode::KeyU),
        "KeyV" => Some(KeyCode::KeyV),
        "KeyW" => Some(KeyCode::KeyW),
        "KeyX" => Some(KeyCode::KeyX),
        "KeyY" => Some(KeyCode::KeyY),
        "KeyZ" => Some(KeyCode::KeyZ),

        "Digit0" => Some(KeyCode::Digit0),
        "Digit1" => Some(KeyCode::Digit1),
        "Digit2" => Some(KeyCode::Digit2),
        "Digit3" => Some(KeyCode::Digit3),
        "Digit4" => Some(KeyCode::Digit4),
        "Digit5" => Some(KeyCode::Digit5),
        "Digit6" => Some(KeyCode::Digit6),
        "Digit7" => Some(KeyCode::Digit7),
        "Digit8" => Some(KeyCode::Digit8),
        "Digit9" => Some(KeyCode::Digit9),

        "Space" => Some(KeyCode::Space),
        "ShiftLeft" => Some(KeyCode::ShiftLeft),
        "ShiftRight" => Some(KeyCode::ShiftRight),
        "ControlLeft" => Some(KeyCode::ControlLeft),
        "ControlRight" => Some(KeyCode::ControlRight),
        "AltLeft" => Some(KeyCode::AltLeft),
        "AltRight" => Some(KeyCode::AltRight),
        "Tab" => Some(KeyCode::Tab),
        "Enter" => Some(KeyCode::Enter),
        "Escape" => Some(KeyCode::Escape),
        "Backspace" => Some(KeyCode::Backspace),
        "CapsLock" => Some(KeyCode::CapsLock),

        "Semicolon" => Some(KeyCode::Semicolon),
        "Quote" => Some(KeyCode::Quote),
        "Comma" => Some(KeyCode::Comma),
        "Period" => Some(KeyCode::Period),
        "Slash" => Some(KeyCode::Slash),
        "Backslash" => Some(KeyCode::Backslash),
        "BracketLeft" => Some(KeyCode::BracketLeft),
        "BracketRight" => Some(KeyCode::BracketRight),
        "Minus" => Some(KeyCode::Minus),
        "Equal" => Some(KeyCode::Equal),
        "Backquote" => Some(KeyCode::Backquote),

        "ArrowLeft" => Some(KeyCode::ArrowLeft),
        "ArrowRight" => Some(KeyCode::ArrowRight),
        "ArrowUp" => Some(KeyCode::ArrowUp),
        "ArrowDown" => Some(KeyCode::ArrowDown),

        "Numpad0" => Some(KeyCode::Numpad0),
        "Numpad1" => Some(KeyCode::Numpad1),
        "Numpad2" => Some(KeyCode::Numpad2),
        "Numpad3" => Some(KeyCode::Numpad3),
        "Numpad4" => Some(KeyCode::Numpad4),
        "Numpad5" => Some(KeyCode::Numpad5),
        "Numpad6" => Some(KeyCode::Numpad6),
        "Numpad7" => Some(KeyCode::Numpad7),
        "Numpad8" => Some(KeyCode::Numpad8),
        "Numpad9" => Some(KeyCode::Numpad9),
        "NumpadEnter" => Some(KeyCode::NumpadEnter),
        "NumpadAdd" => Some(KeyCode::NumpadAdd),
        "NumpadSubtract" => Some(KeyCode::NumpadSubtract),
        "NumpadMultiply" => Some(KeyCode::NumpadMultiply),
        "NumpadDivide" => Some(KeyCode::NumpadDivide),
        "NumpadDecimal" => Some(KeyCode::NumpadDecimal),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_presets() {
        let mut config = InputConfig::new(KeyPreset::HomeRow);
        assert_eq!(
            config.map_key(PhysicalKey::Code(KeyCode::KeyS)),
            Some(Lane::Key1)
        );
        assert_eq!(
            config.map_key(PhysicalKey::Code(KeyCode::Space)),
            Some(Lane::Key4)
        );
        assert_eq!(
            config.map_key(PhysicalKey::Code(KeyCode::KeyL)),
            Some(Lane::Key7)
        );

        config.cycle_preset(PlayMode::Keys7);
        assert_eq!(config.preset, KeyPreset::ArcadeZx);
        assert_eq!(
            config.map_key(PhysicalKey::Code(KeyCode::KeyZ)),
            Some(Lane::Key1)
        );
        assert_eq!(
            config.map_key(PhysicalKey::Code(KeyCode::KeyS)),
            Some(Lane::Key2)
        );
        assert_eq!(
            config.map_key(PhysicalKey::Code(KeyCode::KeyV)),
            Some(Lane::Key7)
        );
    }

    #[test]
    fn test_custom_key_binding() {
        let mut config = InputConfig::new(KeyPreset::HomeRow);
        config.bind_key(KeyCode::KeyA, Lane::Scratch);
        assert_eq!(config.preset, KeyPreset::Custom);
        assert_eq!(
            config.map_key(PhysicalKey::Code(KeyCode::KeyA)),
            Some(Lane::Scratch)
        );
        assert_eq!(config.get_key_name_for_lane(Lane::Scratch), "A");
    }

    #[test]
    fn test_binding_serialization_roundtrip() {
        let mut config = InputConfig::new(KeyPreset::HomeRow);
        config.bind_key(KeyCode::KeyA, Lane::Scratch);
        config.bind_key(KeyCode::KeyZ, Lane::Key1);
        config.bind_key(KeyCode::KeyX, Lane::Key2);
        config.bind_key(KeyCode::KeyC, Lane::Key3);
        config.bind_key(KeyCode::Space, Lane::Key4);
        config.bind_key(KeyCode::KeyM, Lane::Key5);
        config.bind_key(KeyCode::Comma, Lane::Key6);
        config.bind_key(KeyCode::Period, Lane::Key7);

        let s = config.serialize_bindings();
        let mut restored = InputConfig::new(KeyPreset::HomeRow);
        restored.deserialize_bindings(&s);

        assert_eq!(restored.preset, KeyPreset::Custom);
        assert_eq!(
            restored.map_key(PhysicalKey::Code(KeyCode::KeyA)),
            Some(Lane::Scratch)
        );
        assert_eq!(
            restored.map_key(PhysicalKey::Code(KeyCode::Comma)),
            Some(Lane::Key6)
        );
    }

    #[test]
    fn test_pms_9k_preset_has_no_scratch_and_nine_keys() {
        let config = InputConfig::new(KeyPreset::Pms9K);
        assert_eq!(
            config.map_key(PhysicalKey::Code(KeyCode::ShiftLeft)),
            None,
            "PMS has no scratch lane"
        );
        assert_eq!(
            config.map_key(PhysicalKey::Code(KeyCode::KeyS)),
            Some(Lane::Key1)
        );
        assert_eq!(
            config.map_key(PhysicalKey::Code(KeyCode::Semicolon)),
            Some(Lane::Key8)
        );
        assert_eq!(
            config.map_key(PhysicalKey::Code(KeyCode::Quote)),
            Some(Lane::Key9)
        );
    }

    #[test]
    fn test_double_play_preset_covers_both_1p_and_2p_sides() {
        let config = InputConfig::new(KeyPreset::DoublePlay);
        assert_eq!(
            config.map_key(PhysicalKey::Code(KeyCode::ShiftLeft)),
            Some(Lane::Scratch)
        );
        assert_eq!(
            config.map_key(PhysicalKey::Code(KeyCode::KeyZ)),
            Some(Lane::Key1)
        );
        assert_eq!(
            config.map_key(PhysicalKey::Code(KeyCode::ShiftRight)),
            Some(Lane::P2Scratch)
        );
        assert_eq!(
            config.map_key(PhysicalKey::Code(KeyCode::KeyU)),
            Some(Lane::P2Key1)
        );
        assert_eq!(
            config.map_key(PhysicalKey::Code(KeyCode::Backslash)),
            Some(Lane::P2Key7)
        );
    }

    #[test]
    fn test_cycle_preset_stays_within_the_mode() {
        let mut config = InputConfig::new(KeyPreset::HomeRow);
        config.cycle_preset(PlayMode::Keys7);
        assert_eq!(config.preset, KeyPreset::ArcadeZx);
        config.cycle_preset(PlayMode::Keys7);
        // No custom bindings yet, so it wraps back to HomeRow.
        assert_eq!(config.preset, KeyPreset::HomeRow);

        config.bind_key(KeyCode::KeyA, Lane::Scratch);
        config.cycle_preset(PlayMode::Keys7);
        assert_eq!(config.preset, KeyPreset::HomeRow);
        config.cycle_preset(PlayMode::Keys7);
        config.cycle_preset(PlayMode::Keys7);
        assert_eq!(config.preset, KeyPreset::Custom);

        let mut dp = InputConfig::new(KeyPreset::DoublePlay);
        dp.cycle_preset(PlayMode::Keys14);
        assert_eq!(dp.preset, KeyPreset::DoublePlay, "only one built-in fits DP");
    }

    #[test]
    fn test_modes_keep_separate_layouts() {
        let mut kb = KeyBindings::default();
        assert_eq!(kb.get(PlayMode::Keys9).preset, KeyPreset::Pms9K);
        assert_eq!(kb.get(PlayMode::Keys14).preset, KeyPreset::DoublePlay);

        // Rebinding 5K leaves 7K alone.
        kb.get_mut(PlayMode::Keys5).bind_key(KeyCode::KeyQ, Lane::Key1);
        let q = PhysicalKey::Code(KeyCode::KeyQ);
        assert_eq!(kb.get(PlayMode::Keys5).map_key(q), Some(Lane::Key1));
        assert_eq!(kb.get(PlayMode::Keys7).map_key(q), None);
        assert_eq!(kb.get(PlayMode::Keys7).map_key(PhysicalKey::Code(KeyCode::KeyS)), Some(Lane::Key1));

        // Save / load round trip.
        let saved = kb.to_saved().map(Some);
        let restored = KeyBindings::load(&saved, None);
        assert_eq!(restored.get(PlayMode::Keys5).preset, KeyPreset::Custom);
        assert_eq!(restored.get(PlayMode::Keys7).preset, KeyPreset::HomeRow);
        assert_eq!(restored.get(PlayMode::Keys5).map_key(q), Some(Lane::Key1));
    }

    #[test]
    fn test_legacy_layout_migrates_to_the_modes_it_fits() {
        let none: [Option<SavedLayout>; 5] = Default::default();
        // An old ArcadeZx setting applies to 5K and 7K only.
        let kb = KeyBindings::load(&none, Some(&(KeyPreset::ArcadeZx, String::new())));
        assert_eq!(kb.get(PlayMode::Keys5).preset, KeyPreset::ArcadeZx);
        assert_eq!(kb.get(PlayMode::Keys7).preset, KeyPreset::ArcadeZx);
        assert_eq!(kb.get(PlayMode::Keys9).preset, KeyPreset::Pms9K);
        assert_eq!(kb.get(PlayMode::Keys14).preset, KeyPreset::DoublePlay);

        // Old custom bindings for 8 lanes cover 5K and 7K but not DP.
        let custom = "Scratch:KeyA,Key1:KeyZ,Key2:KeyS,Key3:KeyX,Key4:KeyD,Key5:KeyC,Key6:KeyF,Key7:KeyV";
        let kb = KeyBindings::load(&none, Some(&(KeyPreset::Custom, custom.to_string())));
        assert_eq!(kb.get(PlayMode::Keys7).preset, KeyPreset::Custom);
        assert_eq!(kb.get(PlayMode::Keys5).preset, KeyPreset::Custom);
        assert_eq!(kb.get(PlayMode::Keys10).preset, KeyPreset::DoublePlay);

        // A stored per-mode layout wins over the legacy one.
        let mut saved = none.clone();
        saved[1] = Some((KeyPreset::HomeRow, String::new()));
        let kb = KeyBindings::load(&saved, Some(&(KeyPreset::ArcadeZx, String::new())));
        assert_eq!(kb.get(PlayMode::Keys7).preset, KeyPreset::HomeRow);
        assert_eq!(kb.get(PlayMode::Keys5).preset, KeyPreset::ArcadeZx);
    }

    #[test]
    fn test_serialize_bindings_roundtrips_double_play_lanes() {
        // Regression test: serialize_bindings() used to only enumerate the
        // original 8 lanes, so custom bindings on PMS/DP lanes silently
        // vanished on save/reload.
        let mut config = InputConfig::new(KeyPreset::DoublePlay);
        config.bind_key(KeyCode::KeyU, Lane::P2Key1);
        config.bind_key(KeyCode::Semicolon, Lane::Key8);

        let s = config.serialize_bindings();
        let mut restored = InputConfig::new(KeyPreset::HomeRow);
        restored.deserialize_bindings(&s);

        assert_eq!(
            restored.map_key(PhysicalKey::Code(KeyCode::KeyU)),
            Some(Lane::P2Key1)
        );
        assert_eq!(
            restored.map_key(PhysicalKey::Code(KeyCode::Semicolon)),
            Some(Lane::Key8)
        );
    }
}

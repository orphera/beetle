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
    /// UE 4K: S D L ; (Key1, Key2, Key4, Key5)
    Ue4K,
    /// UE 6K: A S D L ; ' (Key1..Key3, Key5..Key7)
    Ue6K,
    /// UE 8K: A S D F K L ; ' (Scratch, Key1..Key7)
    Ue8K,
    /// UE 8K as six keys with a trigger each side (DJMAX style):
    /// LShift + S D F J K L + RShift (Scratch = left, Key7 = right)
    Ue8KTriggers,
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
            Self::Ue4K => "Ue4K",
            Self::Ue6K => "Ue6K",
            Self::Ue8K => "Ue8K",
            Self::Ue8KTriggers => "Ue8KTriggers",
            Self::DoublePlay => "DoublePlay",
            Self::Custom => "Custom",
        }
    }

    pub fn from_id(s: &str) -> Option<Self> {
        [
            Self::HomeRow,
            Self::ArcadeZx,
            Self::Pms9K,
            Self::Ue4K,
            Self::Ue6K,
            Self::Ue8K,
            Self::Ue8KTriggers,
            Self::DoublePlay,
            Self::Custom,
        ]
            .into_iter()
            .find(|p| p.id() == s)
    }

    /// Built-in presets that bind every lane of `mode`.
    pub fn presets_for(mode: PlayMode) -> &'static [KeyPreset] {
        match mode {
            PlayMode::Keys5 | PlayMode::Keys7 => &[Self::HomeRow, Self::ArcadeZx],
            PlayMode::Keys9 => &[Self::Pms9K],
            PlayMode::Keys4 => &[Self::Ue4K],
            PlayMode::Keys6 => &[Self::Ue6K],
            PlayMode::Keys8 => &[Self::Ue8K, Self::Ue8KTriggers],
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
            Self::Ue4K => "4K (S D L ;)",
            Self::Ue6K => "6K (A S D L ; ')",
            Self::Ue8K => "8K (A S D F K L ; ')",
            Self::Ue8KTriggers => "8K 6K + L/R (LShift S D F J K L RShift)",
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

/// Built-in layouts as (key, lane) pairs. A lane may have several keys
/// (e.g. both Shift and Ctrl on the turntable).
fn preset_pairs(preset: KeyPreset) -> &'static [(KeyCode, Lane)] {
    use KeyCode as K;
    use Lane as L;
    match preset {
        KeyPreset::HomeRow => &[
            (K::ShiftLeft, L::Scratch),
            (K::ControlLeft, L::Scratch),
            (K::KeyS, L::Key1),
            (K::KeyD, L::Key2),
            (K::KeyF, L::Key3),
            (K::Space, L::Key4),
            (K::KeyJ, L::Key5),
            (K::KeyK, L::Key6),
            (K::KeyL, L::Key7),
        ],
        KeyPreset::ArcadeZx => &[
            (K::ShiftLeft, L::Scratch),
            (K::ControlLeft, L::Scratch),
            (K::KeyZ, L::Key1),
            (K::KeyS, L::Key2),
            (K::KeyX, L::Key3),
            (K::KeyD, L::Key4),
            (K::KeyC, L::Key5),
            (K::KeyF, L::Key6),
            (K::KeyV, L::Key7),
        ],
        // PMS has no scratch lane - 9 key buttons only.
        KeyPreset::Pms9K => &[
            (K::KeyS, L::Key1),
            (K::KeyD, L::Key2),
            (K::KeyF, L::Key3),
            (K::Space, L::Key4),
            (K::KeyJ, L::Key5),
            (K::KeyK, L::Key6),
            (K::KeyL, L::Key7),
            (K::Semicolon, L::Key8),
            (K::Quote, L::Key9),
        ],
        KeyPreset::Ue4K => &[
            (K::KeyS, L::Key1),
            (K::KeyD, L::Key2),
            (K::KeyL, L::Key4),
            (K::Semicolon, L::Key5),
        ],
        KeyPreset::Ue6K => &[
            (K::KeyA, L::Key1),
            (K::KeyS, L::Key2),
            (K::KeyD, L::Key3),
            (K::KeyL, L::Key5),
            (K::Semicolon, L::Key6),
            (K::Quote, L::Key7),
        ],
        // The 8K scratch is an ordinary key here (leftmost).
        KeyPreset::Ue8K => &[
            (K::KeyA, L::Scratch),
            (K::KeyS, L::Key1),
            (K::KeyD, L::Key2),
            (K::KeyF, L::Key3),
            (K::KeyK, L::Key4),
            (K::KeyL, L::Key5),
            (K::Semicolon, L::Key6),
            (K::Quote, L::Key7),
        ],
        // DJMAX style: the scratch lane is the left trigger, Key7 the right one.
        KeyPreset::Ue8KTriggers => &[
            (K::ShiftLeft, L::Scratch),
            (K::KeyS, L::Key1),
            (K::KeyD, L::Key2),
            (K::KeyF, L::Key3),
            (K::KeyJ, L::Key4),
            (K::KeyK, L::Key5),
            (K::KeyL, L::Key6),
            (K::ShiftRight, L::Key7),
        ],
        // 1P side mirrors ArcadeZx; 2P side mirrors it on the right hand.
        KeyPreset::DoublePlay => &[
            (K::ShiftLeft, L::Scratch),
            (K::ControlLeft, L::Scratch),
            (K::KeyZ, L::Key1),
            (K::KeyS, L::Key2),
            (K::KeyX, L::Key3),
            (K::KeyD, L::Key4),
            (K::KeyC, L::Key5),
            (K::KeyF, L::Key6),
            (K::KeyV, L::Key7),
            (K::ShiftRight, L::P2Scratch),
            (K::ControlRight, L::P2Scratch),
            (K::KeyU, L::P2Key1),
            (K::KeyI, L::P2Key2),
            (K::KeyO, L::P2Key3),
            (K::KeyP, L::P2Key4),
            (K::BracketLeft, L::P2Key5),
            (K::BracketRight, L::P2Key6),
            (K::Backslash, L::P2Key7),
        ],
        KeyPreset::Custom => &[],
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

    /// Whether every lane of `mode` has at least one key.
    pub fn covers(&self, mode: PlayMode) -> bool {
        lanes_for(mode).iter().all(|&lane| !self.keys_for_lane(lane).is_empty())
    }

    /// Resets all bindings to a specific default preset.
    pub fn reset_to_preset(&mut self, preset: KeyPreset) {
        self.preset = preset;
        self.custom_bindings.clear();
    }

    /// Editing starts from what is active: switch to Custom, copying the
    /// current preset's keys if there are no custom bindings yet.
    fn begin_edit(&mut self) {
        if self.preset != KeyPreset::Custom {
            self.custom_bindings = preset_pairs(self.preset).iter().copied().collect();
            self.preset = KeyPreset::Custom;
        }
    }

    /// Makes `key` the only key of `lane` (taking it away from any other lane).
    pub fn bind_key(&mut self, key: KeyCode, lane: Lane) {
        self.begin_edit();
        self.custom_bindings.retain(|_, &mut l| l != lane);
        self.custom_bindings.insert(key, lane);
    }

    /// Adds `key` to `lane`, keeping its other keys (a key belongs to one lane).
    pub fn add_key(&mut self, key: KeyCode, lane: Lane) {
        self.begin_edit();
        self.custom_bindings.insert(key, lane);
    }

    /// Removes every key of `lane`.
    pub fn clear_lane(&mut self, lane: Lane) {
        self.begin_edit();
        self.custom_bindings.retain(|_, &mut l| l != lane);
    }

    /// Maps a winit PhysicalKey to a rhythm game Lane.
    pub fn map_key(&self, key: PhysicalKey) -> Option<Lane> {
        let PhysicalKey::Code(code) = key else {
            return None;
        };
        match self.preset {
            KeyPreset::Custom => self.custom_bindings.get(&code).copied(),
            preset => preset_pairs(preset)
                .iter()
                .find(|(k, _)| *k == code)
                .map(|&(_, l)| l),
        }
    }

    /// Keys bound to `lane`, in a stable order (preset order, or by name for
    /// custom bindings).
    pub fn keys_for_lane(&self, lane: Lane) -> Vec<KeyCode> {
        match self.preset {
            KeyPreset::Custom => {
                let mut keys: Vec<KeyCode> = self
                    .custom_bindings
                    .iter()
                    .filter(|(_, &l)| l == lane)
                    .map(|(&k, _)| k)
                    .collect();
                keys.sort_by_key(|&k| key_code_to_identifier(k));
                keys
            }
            preset => preset_pairs(preset)
                .iter()
                .filter(|(_, l)| *l == lane)
                .map(|&(k, _)| k)
                .collect(),
        }
    }

    /// Display names of the keys bound to `lane` (empty when unbound).
    pub fn key_names_for_lane(&self, lane: Lane) -> Vec<&'static str> {
        self.keys_for_lane(lane).into_iter().map(key_code_to_str).collect()
    }

    /// Serializes custom bindings: "Scratch:ShiftLeft,Scratch:ControlLeft,Key1:KeyS,..."
    /// (a lane appears once per key; deterministic order).
    pub fn serialize_bindings(&self) -> String {
        let mut parts = Vec::new();
        for &lane in &Lane::ALL {
            let mut keys: Vec<&str> = self
                .custom_bindings
                .iter()
                .filter(|(_, &l)| l == lane)
                .map(|(&k, _)| key_code_to_identifier(k))
                .collect();
            keys.sort_unstable();
            for key in keys {
                parts.push(format!("{}:{}", lane_to_name(lane), key));
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

/// Turns a lane key event into a lane press / release, given the keys held
/// so far (updated here). A lane can have several keys: every new key press
/// presses the lane (two keys can alternate on the turntable), OS key repeat
/// is ignored, and the lane is released only with its last held key.
/// Returns `Some(true)` = press, `Some(false)` = release, `None` = nothing.
pub fn lane_transition(held: &mut Vec<(KeyCode, Lane)>, key: KeyCode, lane: Lane, pressed: bool) -> Option<bool> {
    let was_held = held.iter().any(|&(k, _)| k == key);
    if pressed {
        if was_held {
            return None;
        }
        held.push((key, lane));
        return Some(true);
    }
    held.retain(|&(k, _)| k != key);
    let still_held = held.iter().any(|&(_, l)| l == lane);
    (was_held && !still_held).then_some(false)
}

/// Lanes of a key mode, left to right (same order as Key Config).
pub fn lanes_for(mode: PlayMode) -> &'static [Lane] {
    let mut skin = SkinConfig::default();
    skin.set_play_mode(mode);
    skin.active_lanes()
}

/// Lanes of a key mode left to right as drawn, given the skin's scratch side
/// and 8K form (Key Config selects in this order).
pub fn screen_lanes_for(skin: &SkinConfig, mode: PlayMode) -> Vec<Lane> {
    let mut skin = skin.clone();
    skin.set_play_mode(mode);
    skin.screen_lanes()
}

/// Key modes that each keep their own layout.
pub const MODE_SLOTS: [PlayMode; 8] = [
    PlayMode::Keys5,
    PlayMode::Keys7,
    PlayMode::Keys9,
    PlayMode::Keys10,
    PlayMode::Keys14,
    PlayMode::Keys4,
    PlayMode::Keys6,
    PlayMode::Keys8,
];

/// `config.dat` suffix for a mode slot ("5k", "7k", ...).
pub fn mode_slot_name(mode: PlayMode) -> &'static str {
    match mode {
        PlayMode::Keys5 => "5k",
        PlayMode::Keys7 => "7k",
        PlayMode::Keys9 => "9k",
        PlayMode::Keys10 => "10k",
        PlayMode::Keys14 => "14k",
        PlayMode::Keys4 => "4k",
        PlayMode::Keys6 => "6k",
        PlayMode::Keys8 => "8k",
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
    layouts: [InputConfig; 8],
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
    pub fn load(saved: &[Option<SavedLayout>; 8], legacy: Option<&SavedLayout>) -> Self {
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
    pub fn to_saved(&self) -> [SavedLayout; 8] {
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
        assert_eq!(config.key_names_for_lane(Lane::Scratch), ["A"]);
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
        let none: [Option<SavedLayout>; 8] = Default::default();
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
    fn test_eight_k_trigger_preset_binds_every_lane() {
        let mut config = InputConfig::new(KeyPreset::Ue8K);
        assert!(config.covers(PlayMode::Keys8));
        config.cycle_preset(PlayMode::Keys8);
        assert_eq!(config.preset, KeyPreset::Ue8KTriggers);
        assert!(config.covers(PlayMode::Keys8));
        let k = |c| config.map_key(PhysicalKey::Code(c));
        assert_eq!(k(KeyCode::ShiftLeft), Some(Lane::Scratch));
        assert_eq!(k(KeyCode::ShiftRight), Some(Lane::Key7));
        assert_eq!(k(KeyCode::KeyL), Some(Lane::Key6));
        assert_eq!(KeyPreset::from_id("Ue8KTriggers"), Some(KeyPreset::Ue8KTriggers));
        config.cycle_preset(PlayMode::Keys8);
        assert_eq!(config.preset, KeyPreset::Ue8K);
    }

    #[test]
    fn test_screen_lanes_follow_the_scratch_side() {
        let mut skin = SkinConfig::default();
        assert_eq!(screen_lanes_for(&skin, PlayMode::Keys7)[0], Lane::Scratch);
        skin.set_scratch_side(PlayMode::Keys7, beetle_render::ScratchSide::Right);
        let lanes = screen_lanes_for(&skin, PlayMode::Keys7);
        assert_eq!(lanes.last(), Some(&Lane::Scratch));
        assert_eq!(lanes.len(), 8);
        // Other modes keep theirs.
        assert_eq!(screen_lanes_for(&skin, PlayMode::Keys5)[0], Lane::Scratch);
    }

    #[test]
    fn test_multiple_keys_per_lane() {
        let mut config = InputConfig::new(KeyPreset::HomeRow);
        // Presets may already put two keys on a lane.
        assert_eq!(config.key_names_for_lane(Lane::Scratch).len(), 2);

        config.add_key(KeyCode::KeyQ, Lane::Key1);
        let k = |c| config.map_key(PhysicalKey::Code(c));
        assert_eq!(k(KeyCode::KeyQ), Some(Lane::Key1));
        assert_eq!(k(KeyCode::KeyS), Some(Lane::Key1), "the preset key stays");

        // A key moves: adding S to Key2 takes it off Key1.
        config.add_key(KeyCode::KeyS, Lane::Key2);
        assert_eq!(config.keys_for_lane(Lane::Key1), [KeyCode::KeyQ]);
        assert_eq!(config.keys_for_lane(Lane::Key2), [KeyCode::KeyD, KeyCode::KeyS]);

        // bind_key replaces, clear_lane empties.
        config.bind_key(KeyCode::KeyW, Lane::Key2);
        assert_eq!(config.keys_for_lane(Lane::Key2), [KeyCode::KeyW]);
        config.clear_lane(Lane::Key2);
        assert!(config.keys_for_lane(Lane::Key2).is_empty());
        assert!(!config.covers(PlayMode::Keys7));

        // All keys survive a save / load.
        config.add_key(KeyCode::KeyE, Lane::Key1);
        let s = config.serialize_bindings();
        let mut restored = InputConfig::new(KeyPreset::HomeRow);
        restored.deserialize_bindings(&s);
        assert_eq!(restored.keys_for_lane(Lane::Key1), config.keys_for_lane(Lane::Key1));
        assert_eq!(restored.keys_for_lane(Lane::Key1).len(), 2);
        assert_eq!(s, restored.serialize_bindings(), "serialization is deterministic");
    }

    #[test]
    fn test_lane_held_until_last_key_released() {
        let mut held = Vec::new();
        let (shift, ctrl) = (KeyCode::ShiftLeft, KeyCode::ControlLeft);
        assert_eq!(lane_transition(&mut held, shift, Lane::Scratch, true), Some(true));
        assert_eq!(lane_transition(&mut held, shift, Lane::Scratch, true), None, "key repeat");
        // Second key on the same lane is a new press...
        assert_eq!(lane_transition(&mut held, ctrl, Lane::Scratch, true), Some(true));
        // ...and letting go of one key keeps the lane held (long notes survive).
        assert_eq!(lane_transition(&mut held, shift, Lane::Scratch, false), None);
        assert_eq!(lane_transition(&mut held, ctrl, Lane::Scratch, false), Some(false));
        // A release without a press (e.g. pressed before the song) does nothing.
        assert_eq!(lane_transition(&mut held, KeyCode::KeyS, Lane::Key1, false), None);
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

use beetle_core::{Lane, PlayMode};

/// Straight-alpha RGBA8 color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorRgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl ColorRgba {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn to_u32(self) -> u32 {
        ((self.a as u32) << 24) | ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }

    pub const fn transparent() -> Self {
        Self {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        }
    }

    pub const fn with_alpha(self, a: u8) -> Self {
        Self {
            r: self.r,
            g: self.g,
            b: self.b,
            a,
        }
    }
}

/// Where a single play playfield sits on screen. Double Play fills the
/// screen from the left whatever this says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FieldPosition {
    #[default]
    Left,
    Center,
    Right,
}

impl FieldPosition {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Left => "LEFT",
            Self::Center => "CENTER",
            Self::Right => "RIGHT",
        }
    }

    pub fn from_name(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "CENTER" => Self::Center,
            "RIGHT" => Self::Right,
            _ => Self::Left,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Left => Self::Center,
            Self::Center => Self::Right,
            Self::Right => Self::Left,
        }
    }

    pub fn prev(self) -> Self {
        self.next().next()
    }
}

/// Which edge of a single play playfield (5K / 7K) the scratch lane is on.
/// Double Play keeps the 1P scratch left and the 2P scratch right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScratchSide {
    #[default]
    Left,
    Right,
}

impl ScratchSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Left => "LEFT",
            Self::Right => "RIGHT",
        }
    }

    pub fn from_name(s: &str) -> Self {
        if s.eq_ignore_ascii_case("RIGHT") {
            Self::Right
        } else {
            Self::Left
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

/// How the 8K lanes are arranged. Both keep the chart's lane order
/// (Scratch, Key1..Key7, left to right).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EightKForm {
    /// Eight keys in a row; the scratch lane goes on either edge.
    #[default]
    Inline,
    /// DJMAX 8B style: a six-key field; the side tracks are wide notes over
    /// its left and right halves (Scratch = left side track, Key7 = right).
    Triggers,
}

impl EightKForm {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inline => "INLINE",
            Self::Triggers => "6K + L/R",
        }
    }

    pub fn id(&self) -> &'static str {
        match self {
            Self::Inline => "INLINE",
            Self::Triggers => "TRIGGERS",
        }
    }

    pub fn from_name(s: &str) -> Self {
        if s.eq_ignore_ascii_case("TRIGGERS") {
            Self::Triggers
        } else {
            Self::Inline
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::Inline => Self::Triggers,
            Self::Triggers => Self::Inline,
        }
    }
}

/// The wide side-track lanes of the 8K trigger form.
pub fn is_side_track(lane: Lane) -> bool {
    matches!(lane, Lane::Scratch | Lane::Key7)
}

/// Modes whose scratch lane can sit on either edge (5K / 7K / 8K).
/// Double Play keeps its cabinet arrangement.
pub fn scratch_side_applies(mode: PlayMode) -> bool {
    matches!(mode, PlayMode::Keys5 | PlayMode::Keys7 | PlayMode::Keys8)
}

fn side_slot(mode: PlayMode) -> usize {
    match mode {
        PlayMode::Keys5 => 0,
        PlayMode::Keys7 => 1,
        _ => 2,
    }
}

/// Gameplay lane layout (playfield geometry, lane widths and note colors).
#[derive(Debug, Clone)]
pub struct SkinConfig {
    pub play_mode: PlayMode,
    pub field_position: FieldPosition,
    /// Scratch side of 5K, 7K and 8K (each mode keeps its own).
    pub scratch_sides: [ScratchSide; 3],
    pub eight_k_form: EightKForm,
    /// Horizontal extent and scale of the 16:9 viewport, kept by
    /// `update_layout` so a play mode change can re-place the playfield.
    pub area_x: f32,
    pub area_width: f32,
    pub area_scale: f32,
    pub playfield_x: f32,
    pub playfield_y: f32,
    pub playfield_width: f32,
    pub playfield_height: f32,
    pub judge_line_y: f32,
    pub lane_width: f32,
    pub scratch_lane_width: f32,
    pub hi_speed: f32,
    pub lane_cover_ratio: f32,
    pub white_key_color: ColorRgba,
    pub blue_key_color: ColorRgba,
    pub scratch_key_color: ColorRgba,
}

impl Default for SkinConfig {
    fn default() -> Self {
        let scratch_w = 72.0;
        let key_w = 50.0;
        let total_w = scratch_w + (7.0 * key_w);

        Self {
            play_mode: PlayMode::Keys7,
            field_position: FieldPosition::Left,
            scratch_sides: [ScratchSide::Left; 3],
            eight_k_form: EightKForm::Inline,
            area_x: 0.0,
            area_width: 1280.0,
            area_scale: 1.0,
            playfield_x: 50.0,
            playfield_y: 24.0,
            playfield_width: total_w,
            playfield_height: 672.0,
            judge_line_y: 616.0,
            lane_width: key_w,
            scratch_lane_width: scratch_w,
            hi_speed: 400.0, // Pixels per second
            lane_cover_ratio: 0.0,
            white_key_color: crate::theme::NOTE_WHITE,
            blue_key_color: crate::theme::NOTE_BLUE,
            scratch_key_color: crate::theme::NOTE_SCRATCH,
        }
    }
}

/// Width of one Double Play side's key block, "Scratch + N keys", before the
/// inter-playfield gap. Shared by `playfield_width_for` and `lane_x` so the
/// two stay in lockstep.
fn dp_side_width(mode: PlayMode, lane_width: f32, scratch_lane_width: f32) -> f32 {
    let key_count = if mode == PlayMode::Keys14 { 7.0 } else { 5.0 };
    scratch_lane_width + key_count * lane_width
}

/// Total playfield width for a given PlayMode. Keys9 (PMS) has no scratch
/// lane; Keys10/Keys14 (Double Play) render two side-by-side playfields
/// separated by a small gap, so `playfield_width` spans both - every other
/// screen element (BGA, HUD, gauge) that anchors off
/// `playfield_x + playfield_width` then automatically clears the 2P side.
fn playfield_width_for(mode: PlayMode, lane_width: f32, scratch_lane_width: f32) -> f32 {
    if let Some(scale) = even_lane_scale(mode) {
        return scale * lane_width * lanes_of(mode).len() as f32;
    }
    match mode {
        PlayMode::Keys5 => scratch_lane_width + 5.0 * lane_width,
        PlayMode::Keys7 => scratch_lane_width + 7.0 * lane_width,
        PlayMode::Keys9 => 9.0 * lane_width,
        PlayMode::Keys10 | PlayMode::Keys14 => {
            let side = dp_side_width(mode, lane_width, scratch_lane_width);
            side * 2.0 + lane_width * 0.6
        }
        PlayMode::Keys4 | PlayMode::Keys6 | PlayMode::Keys8 => unreachable!("even-lane modes return above"),
    }
}

/// The trigger form of 8K: six key lanes (a little wider than 7K's, like 6K)
/// with the two side tracks spanning three of them each.
const TRIGGER_KEYS: usize = 6;
const TRIGGER_KEY_SCALE: f32 = 1.2;

/// 4K / 6K / 8K draw every lane (the 8K scratch too) at one width: this
/// many times a key lane, so a narrow field still reads well.
fn even_lane_scale(mode: PlayMode) -> Option<f32> {
    match mode {
        PlayMode::Keys4 => Some(1.5),
        PlayMode::Keys6 => Some(1.2),
        PlayMode::Keys8 => Some(1.0),
        _ => None,
    }
}

/// Lanes of a mode, left to right, for the modes that restrict their lanes.
fn lanes_of(mode: PlayMode) -> &'static [Lane] {
    mode.restricted_lanes().unwrap_or(&[])
}

impl SkinConfig {
    /// Updates playfield geometry and lane dimensions based on the active 16:9 viewport.
    pub fn update_layout(&mut self, vp: &crate::view::Viewport) {
        let s = vp.scale;
        (self.area_x, self.area_width, self.area_scale) = (vp.x, vp.width, s);
        self.playfield_y = vp.y + 24.0 * s;
        self.playfield_height = 672.0 * s;
        self.judge_line_y = vp.y + 616.0 * s;

        self.scratch_lane_width = 72.0 * s;
        self.lane_width = 50.0 * s;
        self.place_field();
    }

    /// Sets `playfield_width` for the play mode and `playfield_x` for the
    /// field position (single play only; Double Play stays on the left).
    fn place_field(&mut self) {
        self.playfield_width = if self.eight_k_triggers() {
            TRIGGER_KEYS as f32 * TRIGGER_KEY_SCALE * self.lane_width
        } else {
            playfield_width_for(self.play_mode, self.lane_width, self.scratch_lane_width)
        };
        let margin = 50.0 * self.area_scale;
        self.playfield_x = match self.effective_position() {
            FieldPosition::Left => self.area_x + margin,
            FieldPosition::Center => self.area_x + (self.area_width - self.playfield_width) / 2.0,
            FieldPosition::Right => self.area_x + self.area_width - margin - self.playfield_width,
        };
    }

    /// Sets where the playfield sits and which side the current mode's
    /// scratch is on.
    pub fn set_field_layout(&mut self, position: FieldPosition, scratch: ScratchSide) {
        self.field_position = position;
        self.set_scratch_side(self.play_mode, scratch);
    }

    /// The scratch side chosen for `mode` (`Left` for modes without a choice).
    pub fn scratch_side_of(&self, mode: PlayMode) -> ScratchSide {
        if scratch_side_applies(mode) {
            self.scratch_sides[side_slot(mode)]
        } else {
            ScratchSide::Left
        }
    }

    pub fn set_scratch_side(&mut self, mode: PlayMode, side: ScratchSide) {
        if scratch_side_applies(mode) {
            self.scratch_sides[side_slot(mode)] = side;
        }
        self.place_field();
    }

    /// Sets how 8K is arranged.
    pub fn set_eight_k_form(&mut self, form: EightKForm) {
        self.eight_k_form = form;
        self.place_field();
    }

    /// 8K in its trigger form: Key1..Key6 are the lanes, the side tracks
    /// (Scratch, Key7) are wide notes over the left / right half.
    pub fn eight_k_triggers(&self) -> bool {
        self.play_mode == PlayMode::Keys8 && self.eight_k_form == EightKForm::Triggers
    }

    /// Where the playfield actually sits: Double Play is too wide to move,
    /// so it is always `Left`.
    pub fn effective_position(&self) -> FieldPosition {
        match self.play_mode {
            PlayMode::Keys10 | PlayMode::Keys14 => FieldPosition::Left,
            _ => self.field_position,
        }
    }

    /// The scratch lane is drawn on the right edge.
    fn scratch_on_right(&self) -> bool {
        self.scratch_side_of(self.play_mode) == ScratchSide::Right && !self.eight_k_triggers()
    }

    /// Lanes left to right as they appear on screen (the scratch moved to
    /// the right edge when it is set that way). Key Config selects in this order.
    pub fn screen_lanes(&self) -> Vec<Lane> {
        let mut lanes = self.active_lanes().to_vec();
        if self.scratch_on_right() && lanes.first() == Some(&Lane::Scratch) {
            lanes.rotate_left(1);
        }
        lanes
    }

    /// Active lane list based on current PlayMode.
    pub fn active_lanes(&self) -> &'static [Lane] {
        match self.play_mode {
            PlayMode::Keys4 | PlayMode::Keys6 | PlayMode::Keys8 => lanes_of(self.play_mode),
            PlayMode::Keys5 => &[
                Lane::Scratch,
                Lane::Key1,
                Lane::Key2,
                Lane::Key3,
                Lane::Key4,
                Lane::Key5,
            ],
            PlayMode::Keys7 => &[
                Lane::Scratch,
                Lane::Key1,
                Lane::Key2,
                Lane::Key3,
                Lane::Key4,
                Lane::Key5,
                Lane::Key6,
                Lane::Key7,
            ],
            // PMS (9K): no scratch, 9 key buttons.
            PlayMode::Keys9 => &[
                Lane::Key1,
                Lane::Key2,
                Lane::Key3,
                Lane::Key4,
                Lane::Key5,
                Lane::Key6,
                Lane::Key7,
                Lane::Key8,
                Lane::Key9,
            ],
            // Double Play (5+5), left to right: 1P scratch, 1P keys, 2P keys,
            // 2P scratch (the 2P turntable sits on the outer/right edge).
            PlayMode::Keys10 => &[
                Lane::Scratch,
                Lane::Key1,
                Lane::Key2,
                Lane::Key3,
                Lane::Key4,
                Lane::Key5,
                Lane::P2Key1,
                Lane::P2Key2,
                Lane::P2Key3,
                Lane::P2Key4,
                Lane::P2Key5,
                Lane::P2Scratch,
            ],
            // Double Play (7+7), same mirrored arrangement as Keys10.
            PlayMode::Keys14 => &[
                Lane::Scratch,
                Lane::Key1,
                Lane::Key2,
                Lane::Key3,
                Lane::Key4,
                Lane::Key5,
                Lane::Key6,
                Lane::Key7,
                Lane::P2Key1,
                Lane::P2Key2,
                Lane::P2Key3,
                Lane::P2Key4,
                Lane::P2Key5,
                Lane::P2Key6,
                Lane::P2Key7,
                Lane::P2Scratch,
            ],
        }
    }

    /// Sets play mode and updates playfield geometry accordingly.
    pub fn set_play_mode(&mut self, mode: PlayMode) {
        self.play_mode = mode;
        self.place_field();
    }

    /// Keys per Double Play side (5 for Keys10, 7 for Keys14).
    fn dp_key_count(&self) -> f32 {
        if self.play_mode == PlayMode::Keys10 {
            5.0
        } else {
            7.0
        }
    }

    /// X offset of the 2P (Double Play) side's playfield start, relative to
    /// `playfield_x`. Only meaningful for `Keys10`/`Keys14`.
    fn p2_side_x(&self) -> f32 {
        let side = dp_side_width(self.play_mode, self.lane_width, self.scratch_lane_width);
        self.playfield_x + side + self.lane_width * 0.6
    }

    /// Returns the X-coordinate for a specific lane.
    pub fn lane_x(&self, lane: Lane) -> f32 {
        if self.eight_k_triggers() {
            let w = self.lane_width * TRIGGER_KEY_SCALE;
            return self.playfield_x + w * match lane {
                Lane::Key1 => 0.0,
                Lane::Key2 => 1.0,
                Lane::Key3 => 2.0,
                Lane::Key4 | Lane::Key7 => 3.0,
                Lane::Key5 => 4.0,
                Lane::Key6 => 5.0,
                _ => 0.0,
            };
        }
        if even_lane_scale(self.play_mode).is_some() {
            let index = self.screen_lanes().iter().position(|&l| l == lane).unwrap_or(0);
            return self.playfield_x + index as f32 * self.lane_width(lane);
        }
        // PMS (9K) has no scratch lane and a right-side scratch comes after
        // the keys, so in both the key block starts at playfield_x.
        let key_area_x = if self.play_mode == PlayMode::Keys9 || self.scratch_on_right() {
            self.playfield_x
        } else {
            self.playfield_x + self.scratch_lane_width
        };
        match lane {
            Lane::Scratch if self.scratch_on_right() => self.playfield_x + self.playfield_width - self.scratch_lane_width,
            Lane::Scratch => self.playfield_x,
            Lane::Key1 => key_area_x,
            Lane::Key2 => key_area_x + self.lane_width,
            Lane::Key3 => key_area_x + self.lane_width * 2.0,
            Lane::Key4 => key_area_x + self.lane_width * 3.0,
            Lane::Key5 => key_area_x + self.lane_width * 4.0,
            Lane::Key6 => key_area_x + self.lane_width * 5.0,
            Lane::Key7 => key_area_x + self.lane_width * 6.0,
            // PMS (9K) extra buttons continue the same strip.
            Lane::Key8 => key_area_x + self.lane_width * 7.0,
            Lane::Key9 => key_area_x + self.lane_width * 8.0,
            // Double Play (10K/14K) 2P side: a genuine second playfield,
            // positioned right after the 1P side plus a small visual gap.
            // Mirrored like an IIDX DP cabinet: 2P keys first, the 2P
            // scratch on the outer (right) edge.
            Lane::P2Key1 => self.p2_side_x(),
            Lane::P2Key2 => self.p2_side_x() + self.lane_width,
            Lane::P2Key3 => self.p2_side_x() + self.lane_width * 2.0,
            Lane::P2Key4 => self.p2_side_x() + self.lane_width * 3.0,
            Lane::P2Key5 => self.p2_side_x() + self.lane_width * 4.0,
            Lane::P2Key6 => self.p2_side_x() + self.lane_width * 5.0,
            Lane::P2Key7 => self.p2_side_x() + self.lane_width * 6.0,
            Lane::P2Scratch => self.p2_side_x() + self.lane_width * self.dp_key_count(),
        }
    }

    /// Returns the width in pixels for a specific lane.
    pub fn lane_width(&self, lane: Lane) -> f32 {
        if self.eight_k_triggers() {
            let w = self.lane_width * TRIGGER_KEY_SCALE;
            return if is_side_track(lane) { w * (TRIGGER_KEYS / 2) as f32 } else { w };
        }
        if let Some(scale) = even_lane_scale(self.play_mode) {
            return self.lane_width * scale;
        }
        match lane {
            Lane::Scratch | Lane::P2Scratch => self.scratch_lane_width,
            _ => self.lane_width,
        }
    }

    /// Get color assigned to note on a lane.
    pub fn lane_color(&self, lane: Lane) -> ColorRgba {
        // 4K / 6K: mirrored colors by position (the empty middle lane means
        // Key parity would give uneven pairs).
        let by_position: Option<&[bool]> = match self.play_mode {
            PlayMode::Keys4 => Some(&[false, true, true, false]),
            PlayMode::Keys6 => Some(&[false, true, false, false, true, false]),
            _ => None,
        };
        if let Some(blue) = by_position {
            let index = lanes_of(self.play_mode).iter().position(|&l| l == lane).unwrap_or(0);
            return if blue[index] { self.blue_key_color } else { self.white_key_color };
        }
        // Straight 8K: red - white - blue - white white - blue - white - red,
        // by position (the scratch may be at either end).
        if self.play_mode == PlayMode::Keys8 && !self.eight_k_triggers() {
            let index = self.screen_lanes().iter().position(|&l| l == lane).unwrap_or(0);
            return match index {
                0 | 7 => self.scratch_key_color,
                2 | 5 => self.blue_key_color,
                _ => self.white_key_color,
            };
        }
        if self.eight_k_triggers() {
            return match lane {
                Lane::Scratch | Lane::Key7 => self.scratch_key_color,
                Lane::Key2 | Lane::Key5 => self.blue_key_color,
                _ => self.white_key_color,
            };
        }
        match lane {
            Lane::Scratch | Lane::P2Scratch => self.scratch_key_color,
            Lane::Key1 | Lane::Key3 | Lane::Key5 | Lane::Key7 | Lane::Key9 => self.white_key_color,
            Lane::Key2 | Lane::Key4 | Lane::Key6 | Lane::Key8 => self.blue_key_color,
            Lane::P2Key1 | Lane::P2Key3 | Lane::P2Key5 | Lane::P2Key7 => self.white_key_color,
            Lane::P2Key2 | Lane::P2Key4 | Lane::P2Key6 => self.blue_key_color,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::Viewport;

    /// Skin with fixed, un-scaled lane dimensions (no Viewport needed) so
    /// expected pixel values are simple arithmetic.
    fn test_skin() -> SkinConfig {
        let mut skin = SkinConfig::default();
        skin.lane_width = 50.0;
        skin.scratch_lane_width = 72.0;
        skin
    }

    #[test]
    fn test_keys7_layout_unchanged_baseline() {
        // Milestone 10 Phase 3 must not regress the existing 5K/7K layout.
        let mut skin = test_skin();
        skin.set_play_mode(PlayMode::Keys7);
        assert_eq!(skin.playfield_width, 72.0 + 7.0 * 50.0);
        assert_eq!(skin.lane_x(Lane::Scratch), skin.playfield_x);
        assert_eq!(skin.lane_x(Lane::Key1), skin.playfield_x + 72.0);
        assert_eq!(
            skin.lane_x(Lane::Key7),
            skin.playfield_x + 72.0 + 6.0 * 50.0
        );
        assert_eq!(skin.active_lanes().len(), 8);
    }

    #[test]
    fn scratch_on_the_right_follows_the_keys() {
        let mut skin = test_skin();
        skin.set_field_layout(FieldPosition::Left, ScratchSide::Right);
        assert_eq!(skin.lane_x(Lane::Key1), skin.playfield_x);
        assert_eq!(skin.lane_x(Lane::Scratch), skin.lane_x(Lane::Key7) + 50.0);
        assert_eq!(skin.lane_x(Lane::Scratch) + 72.0, skin.playfield_x + skin.playfield_width);
        // 5K too; PMS has no scratch and DP keeps the cabinet arrangement.
        skin.set_play_mode(PlayMode::Keys5);
        skin.set_field_layout(FieldPosition::Left, ScratchSide::Right);
        assert_eq!(skin.lane_x(Lane::Scratch), skin.lane_x(Lane::Key5) + 50.0);
        skin.set_play_mode(PlayMode::Keys14);
        assert_eq!(skin.lane_x(Lane::Scratch), skin.playfield_x);
    }

    #[test]
    fn field_position_places_single_play_only() {
        let mut skin = SkinConfig::default();
        skin.update_layout(&Viewport::new(1280, 720));
        let w = skin.playfield_width;
        skin.set_field_layout(FieldPosition::Center, ScratchSide::Left);
        assert!((skin.playfield_x - (1280.0 - w) / 2.0).abs() < 1e-3);
        skin.set_field_layout(FieldPosition::Right, ScratchSide::Left);
        assert!((skin.playfield_x + w - (1280.0 - 50.0)).abs() < 1e-3);
        // Survives a resize and a mode change; DP stays on the left.
        skin.update_layout(&Viewport::new(1920, 1080));
        assert!((skin.playfield_x + skin.playfield_width - (1920.0 - 75.0)).abs() < 1e-3);
        skin.set_play_mode(PlayMode::Keys14);
        assert!((skin.playfield_x - 75.0).abs() < 1e-3);
    }

    #[test]
    fn ue_modes_draw_only_their_lanes_side_by_side() {
        let mut skin = test_skin();
        skin.set_play_mode(PlayMode::Keys4);
        let w = 50.0 * 1.5;
        assert_eq!(skin.playfield_width, 4.0 * w);
        // Key3 is skipped: Key4 sits right after Key2.
        assert_eq!(skin.lane_x(Lane::Key4), skin.playfield_x + 2.0 * w);
        assert_eq!(skin.lane_x(Lane::Key5), skin.playfield_x + 3.0 * w);
        assert_eq!(skin.lane_color(Lane::Key1), skin.lane_color(Lane::Key5));
        assert_eq!(skin.lane_color(Lane::Key2), skin.lane_color(Lane::Key4));

        skin.set_play_mode(PlayMode::Keys6);
        let w = 50.0 * 1.2;
        assert_eq!(skin.active_lanes().len(), 6);
        assert_eq!(skin.lane_x(Lane::Key5), skin.playfield_x + 3.0 * w);
        assert_eq!(skin.playfield_width, 6.0 * w);

        // 8K: the scratch is one more even lane, on the left.
        skin.set_play_mode(PlayMode::Keys8);
        assert_eq!(skin.active_lanes().len(), 8);
        assert_eq!(skin.lane_x(Lane::Scratch), skin.playfield_x);
        assert_eq!(skin.lane_x(Lane::Key1), skin.playfield_x + 50.0);
        assert_eq!(skin.lane_width(Lane::Scratch), 50.0);
    }

    #[test]
    fn each_mode_keeps_its_own_scratch_side() {
        let mut skin = test_skin();
        skin.set_scratch_side(PlayMode::Keys7, ScratchSide::Right);
        assert_eq!(skin.scratch_side_of(PlayMode::Keys7), ScratchSide::Right);
        assert_eq!(skin.scratch_side_of(PlayMode::Keys5), ScratchSide::Left);
        assert_eq!(skin.scratch_side_of(PlayMode::Keys8), ScratchSide::Left);
        // Modes without a choice ignore it.
        skin.set_scratch_side(PlayMode::Keys14, ScratchSide::Right);
        assert_eq!(skin.scratch_side_of(PlayMode::Keys14), ScratchSide::Left);

        skin.set_play_mode(PlayMode::Keys5);
        assert_eq!(skin.lane_x(Lane::Scratch), skin.playfield_x, "5K stays left");
        skin.set_play_mode(PlayMode::Keys7);
        assert_eq!(skin.lane_x(Lane::Scratch), skin.lane_x(Lane::Key7) + 50.0);
        assert_eq!(skin.screen_lanes().last(), Some(&Lane::Scratch));
    }

    #[test]
    fn straight_eight_k_is_red_white_blue_white_white_blue_white_red() {
        let mut skin = test_skin();
        skin.set_play_mode(PlayMode::Keys8);
        let colors = |skin: &SkinConfig| -> Vec<char> {
            skin.screen_lanes()
                .iter()
                .map(|&l| match skin.lane_color(l) {
                    c if c == skin.scratch_key_color => 'R',
                    c if c == skin.blue_key_color => 'B',
                    _ => 'W',
                })
                .collect()
        };
        assert_eq!(colors(&skin).iter().collect::<String>(), "RWBWWBWR");
        // Positional: the same with the scratch on the right.
        skin.set_scratch_side(PlayMode::Keys8, ScratchSide::Right);
        assert_eq!(colors(&skin).iter().collect::<String>(), "RWBWWBWR");
    }

    #[test]
    fn eight_k_scratch_on_the_right_moves_to_the_end() {
        let mut skin = test_skin();
        skin.set_play_mode(PlayMode::Keys8);
        skin.set_scratch_side(PlayMode::Keys8, ScratchSide::Right);
        assert_eq!(skin.lane_x(Lane::Key1), skin.playfield_x);
        assert_eq!(skin.lane_x(Lane::Key7), skin.playfield_x + 6.0 * 50.0);
        assert_eq!(skin.lane_x(Lane::Scratch), skin.playfield_x + 7.0 * 50.0);
        assert_eq!(skin.playfield_width, 8.0 * 50.0);
    }

    #[test]
    fn eight_k_triggers_keep_the_lane_order() {
        let mut skin = test_skin();
        skin.set_play_mode(PlayMode::Keys8);
        skin.set_scratch_side(PlayMode::Keys8, ScratchSide::Right);
        skin.set_eight_k_form(EightKForm::Triggers);
        let near = |a: f32, b: f32| assert!((a - b).abs() < 1e-3, "{a} vs {b}");
        // The scratch side is moot: the lane order stays, the field is six keys.
        assert_eq!(skin.screen_lanes(), skin.active_lanes());
        let w = 50.0 * TRIGGER_KEY_SCALE;
        near(skin.playfield_width, 6.0 * w);
        near(skin.lane_x(Lane::Key6), skin.playfield_x + 5.0 * w);
        // Side tracks cover the left / right three lanes.
        near(skin.lane_x(Lane::Scratch), skin.playfield_x);
        near(skin.lane_width(Lane::Scratch), 3.0 * w);
        near(skin.lane_x(Lane::Key7), skin.playfield_x + 3.0 * w);
        near(skin.lane_x(Lane::Key7) + skin.lane_width(Lane::Key7), skin.playfield_x + skin.playfield_width);
        assert_eq!(skin.lane_color(Lane::Scratch), skin.lane_color(Lane::Key7));
        assert_ne!(skin.lane_color(Lane::Key1), skin.lane_color(Lane::Key7));
        // The form only touches 8K.
        skin.set_play_mode(PlayMode::Keys7);
        assert_eq!(skin.lane_color(Lane::Key7), skin.white_key_color);
    }

    #[test]
    fn test_keys9_pms_has_no_scratch_and_nine_lanes() {
        let mut skin = test_skin();
        skin.set_play_mode(PlayMode::Keys9);
        assert_eq!(skin.playfield_width, 9.0 * 50.0);
        assert_eq!(skin.lane_x(Lane::Key1), skin.playfield_x); // no scratch offset
        assert_eq!(skin.lane_x(Lane::Key9), skin.playfield_x + 8.0 * 50.0);
        assert_eq!(skin.active_lanes().len(), 9);
        assert!(!skin.active_lanes().contains(&Lane::Scratch));
    }

    #[test]
    fn test_keys10_renders_two_five_key_playfields_side_by_side() {
        let mut skin = test_skin();
        skin.set_play_mode(PlayMode::Keys10);
        assert_eq!(skin.active_lanes().len(), 12); // 2x (Scratch + Key1..5)

        let side = 72.0 + 5.0 * 50.0;
        let gap = 50.0 * 0.6;
        assert_eq!(skin.playfield_width, side * 2.0 + gap);
        // 2P side is mirrored: keys first, scratch on the outer right edge.
        assert_eq!(skin.lane_x(Lane::P2Key1), skin.playfield_x + side + gap);
        assert_eq!(
            skin.lane_x(Lane::P2Key5),
            skin.playfield_x + side + gap + 4.0 * 50.0
        );
        assert_eq!(
            skin.lane_x(Lane::P2Scratch),
            skin.lane_x(Lane::P2Key5) + skin.lane_width(Lane::P2Key5)
        );
        assert_eq!(
            skin.lane_x(Lane::P2Scratch) + skin.lane_width(Lane::P2Scratch),
            skin.playfield_x + skin.playfield_width
        );
        // The 2P side must start strictly after the 1P side ends (no overlap).
        assert!(skin.lane_x(Lane::P2Key1) >= skin.lane_x(Lane::Key5) + skin.lane_width(Lane::Key5));
    }

    #[test]
    fn test_keys14_renders_two_seven_key_playfields_side_by_side() {
        let mut skin = test_skin();
        skin.set_play_mode(PlayMode::Keys14);
        assert_eq!(skin.active_lanes().len(), 16); // 2x (Scratch + Key1..7)

        let side = 72.0 + 7.0 * 50.0;
        let gap = 50.0 * 0.6;
        assert_eq!(skin.playfield_width, side * 2.0 + gap);
        assert_eq!(
            skin.lane_x(Lane::P2Key7),
            skin.playfield_x + side + gap + 6.0 * 50.0
        );
        assert_eq!(
            skin.lane_x(Lane::P2Scratch),
            skin.lane_x(Lane::P2Key7) + skin.lane_width(Lane::P2Key7)
        );
        // The 2P side must start strictly after the 1P side ends (no overlap).
        assert!(skin.lane_x(Lane::P2Key1) >= skin.lane_x(Lane::Key7) + skin.lane_width(Lane::Key7));
        // playfield_x + playfield_width (used by BGA/HUD/gauge elsewhere)
        // must end exactly at the 2P scratch's outer edge.
        assert_eq!(
            skin.playfield_x + skin.playfield_width,
            skin.lane_x(Lane::P2Scratch) + skin.lane_width(Lane::P2Scratch)
        );
        // Lanes are listed left to right (key config shows them in this order).
        let xs: Vec<f32> = skin.active_lanes().iter().map(|&l| skin.lane_x(l)).collect();
        assert!(xs.windows(2).all(|w| w[0] < w[1]));
    }
}

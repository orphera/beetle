use beetle_core::{Lane, PlayMode};

/// RGBA color representation for software rendering.
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

    /// Blends toward white by `amount` (0.0 = unchanged, 1.0 = pure white).
    /// Used for the cheap "glossy" note/LN highlight treatment — a flat
    /// stacked-rect bevel instead of a true gradient, so it works
    /// identically on the GPU path's rect-only SpriteBatcher. See
    /// docs/plans/2026-10-03-pulse-redesign.md.
    pub fn lighten(self, amount: f32) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        Self {
            r: (self.r as f32 + (255.0 - self.r as f32) * amount) as u8,
            g: (self.g as f32 + (255.0 - self.g as f32) * amount) as u8,
            b: (self.b as f32 + (255.0 - self.b as f32) * amount) as u8,
            a: self.a,
        }
    }

    /// Blends toward black by `amount` (0.0 = unchanged, 1.0 = pure black).
    /// Pairs with `lighten` for the glossy note bevel treatment.
    pub fn darken(self, amount: f32) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        Self {
            r: (self.r as f32 * (1.0 - amount)) as u8,
            g: (self.g as f32 * (1.0 - amount)) as u8,
            b: (self.b as f32 * (1.0 - amount)) as u8,
            a: self.a,
        }
    }

    pub fn to_f32_array(self) -> [f32; 4] {
        [
            (self.r as f32) / 255.0,
            (self.g as f32) / 255.0,
            (self.b as f32) / 255.0,
            (self.a as f32) / 255.0,
        ]
    }
}

/// Minimal skin configuration (positions, dimensions, colors).
#[derive(Debug, Clone)]
pub struct SkinConfig {
    pub play_mode: PlayMode,
    pub playfield_x: f32,
    pub playfield_y: f32,
    pub playfield_width: f32,
    pub playfield_height: f32,
    pub judge_line_y: f32,
    pub lane_width: f32,
    pub scratch_lane_width: f32,
    pub note_height: f32,
    pub hi_speed: f32,
    pub lane_cover_ratio: f32,
    pub bg_color: ColorRgba,
    pub playfield_bg_color: ColorRgba,
    pub lane_line_color: ColorRgba,
    pub judge_line_color: ColorRgba,
    pub white_key_color: ColorRgba,
    pub blue_key_color: ColorRgba,
    pub scratch_key_color: ColorRgba,
    pub key_beam_white: ColorRgba,
    pub key_beam_blue: ColorRgba,
    pub key_beam_scratch: ColorRgba,
}

impl Default for SkinConfig {
    fn default() -> Self {
        let scratch_w = 72.0;
        let key_w = 50.0;
        let total_w = scratch_w + (7.0 * key_w);

        Self {
            play_mode: PlayMode::Keys7,
            playfield_x: 50.0,
            playfield_y: 24.0,
            playfield_width: total_w,
            playfield_height: 672.0,
            judge_line_y: 616.0,
            lane_width: key_w,
            scratch_lane_width: scratch_w,
            note_height: 12.0,
            hi_speed: 400.0, // Pixels per second
            lane_cover_ratio: 0.0,
            bg_color: ColorRgba::new(8, 8, 12, 255),
            playfield_bg_color: ColorRgba::new(16, 16, 22, 255),
            lane_line_color: ColorRgba::new(45, 45, 55, 255),
            judge_line_color: ColorRgba::new(255, 50, 50, 255),
            white_key_color: crate::theme::NOTE_WHITE,
            blue_key_color: crate::theme::NOTE_BLUE,
            scratch_key_color: crate::theme::NOTE_SCRATCH,
            key_beam_white: ColorRgba::new(200, 200, 255, 60),
            key_beam_blue: ColorRgba::new(60, 140, 255, 80),
            key_beam_scratch: ColorRgba::new(255, 70, 70, 80),
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
    match mode {
        PlayMode::Keys5 => scratch_lane_width + 5.0 * lane_width,
        PlayMode::Keys7 => scratch_lane_width + 7.0 * lane_width,
        PlayMode::Keys9 => 9.0 * lane_width,
        PlayMode::Keys10 | PlayMode::Keys14 => {
            let side = dp_side_width(mode, lane_width, scratch_lane_width);
            side * 2.0 + lane_width * 0.6
        }
    }
}

impl SkinConfig {
    /// Updates playfield geometry and lane dimensions based on the active 16:9 viewport.
    pub fn update_layout(&mut self, vp: &crate::renderer::Viewport) {
        let s = vp.scale;
        self.playfield_x = vp.x + 50.0 * s;
        self.playfield_y = vp.y + 24.0 * s;
        self.playfield_height = 672.0 * s;
        self.judge_line_y = vp.y + 616.0 * s;

        self.scratch_lane_width = 72.0 * s;
        self.lane_width = 50.0 * s;
        self.note_height = (12.0 * s).max(4.0);

        self.playfield_width =
            playfield_width_for(self.play_mode, self.lane_width, self.scratch_lane_width);
    }

    /// Active lane list based on current PlayMode.
    pub fn active_lanes(&self) -> &'static [Lane] {
        match self.play_mode {
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
            // Double Play (5+5): both sides' scratch + Key1..5.
            PlayMode::Keys10 => &[
                Lane::Scratch,
                Lane::Key1,
                Lane::Key2,
                Lane::Key3,
                Lane::Key4,
                Lane::Key5,
                Lane::P2Scratch,
                Lane::P2Key1,
                Lane::P2Key2,
                Lane::P2Key3,
                Lane::P2Key4,
                Lane::P2Key5,
            ],
            // Double Play (7+7): both sides' scratch + Key1..7.
            PlayMode::Keys14 => &[
                Lane::Scratch,
                Lane::Key1,
                Lane::Key2,
                Lane::Key3,
                Lane::Key4,
                Lane::Key5,
                Lane::Key6,
                Lane::Key7,
                Lane::P2Scratch,
                Lane::P2Key1,
                Lane::P2Key2,
                Lane::P2Key3,
                Lane::P2Key4,
                Lane::P2Key5,
                Lane::P2Key6,
                Lane::P2Key7,
            ],
        }
    }

    /// Sets play mode and updates playfield geometry accordingly.
    pub fn set_play_mode(&mut self, mode: PlayMode) {
        self.play_mode = mode;
        self.playfield_width = playfield_width_for(mode, self.lane_width, self.scratch_lane_width);
    }

    /// X offset of the 2P (Double Play) side's playfield start, relative to
    /// `playfield_x`. Only meaningful for `Keys10`/`Keys14`.
    fn p2_side_x(&self) -> f32 {
        let side = dp_side_width(self.play_mode, self.lane_width, self.scratch_lane_width);
        self.playfield_x + side + self.lane_width * 0.6
    }

    /// Returns the X-coordinate for a specific lane.
    pub fn lane_x(&self, lane: Lane) -> f32 {
        // PMS (9K) has no scratch lane, so the key block starts at playfield_x.
        let key_area_x = if self.play_mode == PlayMode::Keys9 {
            self.playfield_x
        } else {
            self.playfield_x + self.scratch_lane_width
        };
        match lane {
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
            Lane::P2Scratch => self.p2_side_x(),
            Lane::P2Key1 => self.p2_side_x() + self.scratch_lane_width,
            Lane::P2Key2 => self.p2_side_x() + self.scratch_lane_width + self.lane_width,
            Lane::P2Key3 => self.p2_side_x() + self.scratch_lane_width + self.lane_width * 2.0,
            Lane::P2Key4 => self.p2_side_x() + self.scratch_lane_width + self.lane_width * 3.0,
            Lane::P2Key5 => self.p2_side_x() + self.scratch_lane_width + self.lane_width * 4.0,
            Lane::P2Key6 => self.p2_side_x() + self.scratch_lane_width + self.lane_width * 5.0,
            Lane::P2Key7 => self.p2_side_x() + self.scratch_lane_width + self.lane_width * 6.0,
        }
    }

    /// Returns the width in pixels for a specific lane.
    pub fn lane_width(&self, lane: Lane) -> f32 {
        match lane {
            Lane::Scratch | Lane::P2Scratch => self.scratch_lane_width,
            _ => self.lane_width,
        }
    }

    /// Get color assigned to note on a lane.
    pub fn lane_color(&self, lane: Lane) -> ColorRgba {
        match lane {
            Lane::Scratch | Lane::P2Scratch => self.scratch_key_color,
            Lane::Key1 | Lane::Key3 | Lane::Key5 | Lane::Key7 | Lane::Key9 => self.white_key_color,
            Lane::Key2 | Lane::Key4 | Lane::Key6 | Lane::Key8 => self.blue_key_color,
            Lane::P2Key1 | Lane::P2Key3 | Lane::P2Key5 | Lane::P2Key7 => self.white_key_color,
            Lane::P2Key2 | Lane::P2Key4 | Lane::P2Key6 => self.blue_key_color,
        }
    }

    /// Get key beam color when a lane is pressed.
    pub fn key_beam_color(&self, lane: Lane) -> ColorRgba {
        match lane {
            Lane::Scratch | Lane::P2Scratch => self.key_beam_scratch,
            Lane::Key1 | Lane::Key3 | Lane::Key5 | Lane::Key7 | Lane::Key9 => self.key_beam_white,
            Lane::Key2 | Lane::Key4 | Lane::Key6 | Lane::Key8 => self.key_beam_blue,
            Lane::P2Key1 | Lane::P2Key3 | Lane::P2Key5 | Lane::P2Key7 => self.key_beam_white,
            Lane::P2Key2 | Lane::P2Key4 | Lane::P2Key6 => self.key_beam_blue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(skin.lane_x(Lane::P2Scratch), skin.playfield_x + side + gap);
        assert_eq!(
            skin.lane_x(Lane::P2Key5),
            skin.playfield_x + side + gap + 72.0 + 4.0 * 50.0
        );
        // The 2P side must start strictly after the 1P side ends (no overlap).
        assert!(
            skin.lane_x(Lane::P2Scratch) >= skin.lane_x(Lane::Key5) + skin.lane_width(Lane::Key5)
        );
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
            skin.playfield_x + side + gap + 72.0 + 6.0 * 50.0
        );
        // The 2P side must start strictly after the 1P side ends (no overlap).
        assert!(
            skin.lane_x(Lane::P2Scratch) >= skin.lane_x(Lane::Key7) + skin.lane_width(Lane::Key7)
        );
        // playfield_x + playfield_width (used by BGA/HUD/gauge elsewhere)
        // must clear the full 2P side, not just the 1P side.
        assert!(
            skin.playfield_x + skin.playfield_width
                >= skin.lane_x(Lane::P2Key7) + skin.lane_width(Lane::P2Key7)
        );
    }
}

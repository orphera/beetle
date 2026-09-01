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
        Self { r: 0, g: 0, b: 0, a: 0 }
    }

    pub const fn with_alpha(self, a: u8) -> Self {
        Self { r: self.r, g: self.g, b: self.b, a }
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
            white_key_color: ColorRgba::new(245, 245, 250, 255),
            blue_key_color: ColorRgba::new(60, 140, 255, 255),
            scratch_key_color: ColorRgba::new(255, 70, 70, 255),
            key_beam_white: ColorRgba::new(200, 200, 255, 60),
            key_beam_blue: ColorRgba::new(60, 140, 255, 80),
            key_beam_scratch: ColorRgba::new(255, 70, 70, 80),
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

        match self.play_mode {
            PlayMode::Keys5 => {
                self.playfield_width = self.scratch_lane_width + (5.0 * self.lane_width);
            }
            PlayMode::Keys7 | PlayMode::Keys9 | PlayMode::Keys10 | PlayMode::Keys14 => {
                self.playfield_width = self.scratch_lane_width + (7.0 * self.lane_width);
            }
        }
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
            PlayMode::Keys7 | PlayMode::Keys9 | PlayMode::Keys10 | PlayMode::Keys14 => &[
                Lane::Scratch,
                Lane::Key1,
                Lane::Key2,
                Lane::Key3,
                Lane::Key4,
                Lane::Key5,
                Lane::Key6,
                Lane::Key7,
            ],
        }
    }

    /// Sets play mode and updates playfield geometry accordingly.
    pub fn set_play_mode(&mut self, mode: PlayMode) {
        self.play_mode = mode;
        match mode {
            PlayMode::Keys5 => {
                self.playfield_width = self.scratch_lane_width + (5.0 * self.lane_width);
            }
            PlayMode::Keys7 | PlayMode::Keys9 | PlayMode::Keys10 | PlayMode::Keys14 => {
                self.playfield_width = self.scratch_lane_width + (7.0 * self.lane_width);
            }
        }
    }
    /// Returns the X-coordinate for a specific lane.
    pub fn lane_x(&self, lane: Lane) -> f32 {
        match lane {
            Lane::Scratch => self.playfield_x,
            Lane::Key1 => self.playfield_x + self.scratch_lane_width,
            Lane::Key2 => self.playfield_x + self.scratch_lane_width + self.lane_width,
            Lane::Key3 => self.playfield_x + self.scratch_lane_width + self.lane_width * 2.0,
            Lane::Key4 => self.playfield_x + self.scratch_lane_width + self.lane_width * 3.0,
            Lane::Key5 => self.playfield_x + self.scratch_lane_width + self.lane_width * 4.0,
            Lane::Key6 => self.playfield_x + self.scratch_lane_width + self.lane_width * 5.0,
            Lane::Key7 => self.playfield_x + self.scratch_lane_width + self.lane_width * 6.0,
        }
    }

    /// Returns the width in pixels for a specific lane.
    pub fn lane_width(&self, lane: Lane) -> f32 {
        match lane {
            Lane::Scratch => self.scratch_lane_width,
            _ => self.lane_width,
        }
    }

    /// Get color assigned to note on a lane.
    pub fn lane_color(&self, lane: Lane) -> ColorRgba {
        match lane {
            Lane::Scratch => self.scratch_key_color,
            Lane::Key1 | Lane::Key3 | Lane::Key5 | Lane::Key7 => self.white_key_color,
            Lane::Key2 | Lane::Key4 | Lane::Key6 => self.blue_key_color,
        }
    }

    /// Get key beam color when a lane is pressed.
    pub fn key_beam_color(&self, lane: Lane) -> ColorRgba {
        match lane {
            Lane::Scratch => self.key_beam_scratch,
            Lane::Key1 | Lane::Key3 | Lane::Key5 | Lane::Key7 => self.key_beam_white,
            Lane::Key2 | Lane::Key4 | Lane::Key6 => self.key_beam_blue,
        }
    }
}

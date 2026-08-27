use beetle_core::Lane;

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
    pub playfield_x: f32,
    pub playfield_y: f32,
    pub playfield_width: f32,
    pub playfield_height: f32,
    pub judge_line_y: f32,
    pub lane_width: f32,
    pub scratch_lane_width: f32,
    pub note_height: f32,
    pub hi_speed: f32,
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
        let scratch_w = 54.0;
        let key_w = 42.0;
        let total_w = scratch_w + (7.0 * key_w);

        Self {
            playfield_x: 60.0,
            playfield_y: 30.0,
            playfield_width: total_w,
            playfield_height: 640.0,
            judge_line_y: 600.0,
            lane_width: key_w,
            scratch_lane_width: scratch_w,
            note_height: 10.0,
            hi_speed: 400.0, // Pixels per second
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

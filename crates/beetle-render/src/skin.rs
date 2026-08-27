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
    pub judge_line_color: ColorRgba,
    pub white_key_color: ColorRgba,
    pub blue_key_color: ColorRgba,
    pub scratch_key_color: ColorRgba,
}

impl Default for SkinConfig {
    fn default() -> Self {
        Self {
            playfield_x: 60.0,
            playfield_y: 20.0,
            playfield_width: 360.0,
            playfield_height: 680.0,
            judge_line_y: 620.0,
            lane_width: 40.0,
            scratch_lane_width: 60.0,
            note_height: 8.0,
            hi_speed: 300.0,
            bg_color: ColorRgba::new(10, 10, 15, 255),
            judge_line_color: ColorRgba::new(255, 60, 60, 255),
            white_key_color: ColorRgba::new(240, 240, 240, 255),
            blue_key_color: ColorRgba::new(60, 140, 255, 255),
            scratch_key_color: ColorRgba::new(255, 60, 60, 255),
        }
    }
}

impl SkinConfig {
    /// Get color assigned to a lane.
    pub fn lane_color(&self, lane: Lane) -> ColorRgba {
        match lane {
            Lane::Scratch => self.scratch_key_color,
            Lane::Key1 | Lane::Key3 | Lane::Key5 | Lane::Key7 => self.white_key_color,
            Lane::Key2 | Lane::Key4 | Lane::Key6 => self.blue_key_color,
        }
    }
}

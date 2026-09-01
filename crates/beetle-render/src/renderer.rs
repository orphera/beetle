use crate::bitmap_font::BitmapFont;
use crate::skin::{ColorRgba, SkinConfig};
use beetle_core::{JudgeGrade, Lane};
use tiny_skia::{Color, Pixmap};

/// A visual particle burst spawned when hitting a note on a lane.
#[derive(Debug, Clone, Copy)]
pub struct HitBurst {
    pub lane: Lane,
    pub spawn_time: f64,
    pub grade: JudgeGrade,
}

/// 16:9 Viewport mapping within the physical window surface.
///
/// Automatically computes pillarbox / letterbox bounds and reference scale (relative to 1280x720).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// X offset in window surface pixels (pillarboxing)
    pub x: f32,
    /// Y offset in window surface pixels (letterboxing)
    pub y: f32,
    /// Width of active 16:9 rendering viewport
    pub width: f32,
    /// Height of active 16:9 rendering viewport
    pub height: f32,
    /// Proportional scale factor relative to standard 720p (1280x720) reference height (height / 720.0)
    pub scale: f32,
}

impl Viewport {
    pub const BASE_WIDTH: f32 = 1280.0;
    pub const BASE_HEIGHT: f32 = 720.0;
    pub const TARGET_ASPECT: f32 = 16.0 / 9.0;

    pub fn new(window_width: u32, window_height: u32) -> Self {
        let w = window_width.max(1) as f32;
        let h = window_height.max(1) as f32;
        let aspect = w / h;

        let (vp_w, vp_h, vp_x, vp_y) = if (aspect - Self::TARGET_ASPECT).abs() < 0.005 {
            (w, h, 0.0, 0.0)
        } else if aspect > Self::TARGET_ASPECT {
            // Window is wider than 16:9 -> Pillarbox (bars on left and right)
            let vp_h = h;
            let vp_w = (h * Self::TARGET_ASPECT).round();
            let vp_x = ((w - vp_w) / 2.0).round();
            let vp_y = 0.0;
            (vp_w, vp_h, vp_x, vp_y)
        } else {
            // Window is taller than 16:9 -> Letterbox (bars on top and bottom)
            let vp_w = w;
            let vp_h = (w / Self::TARGET_ASPECT).round();
            let vp_x = 0.0;
            let vp_y = ((h - vp_h) / 2.0).round();
            (vp_w, vp_h, vp_x, vp_y)
        };

        let scale = vp_h / Self::BASE_HEIGHT;

        Self {
            x: vp_x,
            y: vp_y,
            width: vp_w,
            height: vp_h,
            scale,
        }
    }

    /// Checks if this viewport has active letterbox (top/bottom bars)
    pub fn is_letterboxed(&self) -> bool {
        self.y > 0.5
    }

    /// Checks if this viewport has active pillarbox (left/right bars)
    pub fn is_pillarboxed(&self) -> bool {
        self.x > 0.5
    }
}

/// Software 2D renderer powered by tiny-skia.
pub struct SoftwareRenderer {
    pub(crate) pixmap: Pixmap,
    pub viewport: Viewport,
    pub skin: SkinConfig,
    pub(crate) key_pressed: [bool; 8],
    pub(crate) last_judge: Option<(JudgeGrade, f64, f64)>, // (Grade, time_seconds, delta_ms)
    pub(crate) hit_bursts: Vec<HitBurst>,
}

impl SoftwareRenderer {
    pub fn new(width: u32, height: u32, mut skin: SkinConfig) -> Option<Self> {
        let pixmap = Pixmap::new(width.max(1), height.max(1))?;
        let viewport = Viewport::new(width, height);
        skin.update_layout(&viewport);
        Some(Self {
            pixmap,
            viewport,
            skin,
            key_pressed: [false; 8],
            last_judge: None,
            hit_bursts: Vec::with_capacity(32),
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            if self.pixmap.width() != width || self.pixmap.height() != height {
                if let Some(new_pixmap) = Pixmap::new(width, height) {
                    self.pixmap = new_pixmap;
                }
            }
            self.viewport = Viewport::new(width, height);
            self.skin.update_layout(&self.viewport);
        }
    }

    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    pub fn data(&self) -> &[u8] {
        self.pixmap.data()
    }

    pub fn set_key_state(&mut self, lane: Lane, pressed: bool) {
        let idx = lane_index(lane);
        self.key_pressed[idx] = pressed;
    }

    pub fn trigger_judge(&mut self, grade: JudgeGrade, time_seconds: f64, delta_ms: f64) {
        self.last_judge = Some((grade, time_seconds, delta_ms));
    }

    pub fn trigger_judge_with_lane(&mut self, lane: Lane, grade: JudgeGrade, time_seconds: f64, delta_ms: f64) {
        self.last_judge = Some((grade, time_seconds, delta_ms));
        if grade != JudgeGrade::Miss && grade != JudgeGrade::Poor {
            self.hit_bursts.push(HitBurst {
                lane,
                spawn_time: time_seconds,
                grade,
            });
        }
    }

    /// Clear frame with background color inside active 16:9 viewport and pure black on letterbox/pillarbox margins.
    pub fn clear(&mut self) {
        let bg = self.skin.bg_color;
        if !self.viewport.is_letterboxed() && !self.viewport.is_pillarboxed() {
            self.pixmap.fill(Color::from_rgba8(bg.r, bg.g, bg.b, bg.a));
        } else {
            self.pixmap.fill(Color::BLACK);
            self.draw_rect(
                self.viewport.x,
                self.viewport.y,
                self.viewport.width,
                self.viewport.height,
                bg,
            );
        }
    }

    /// Saves the current rendered framebuffer directly to a 24-bit BMP screenshot file on disk.
    pub fn save_screenshot<P: AsRef<std::path::Path>>(&self, path: P) -> std::io::Result<()> {
        let w = self.width();
        let h = self.height();
        let raw = self.data();
        let mut pixels = Vec::with_capacity((w * h) as usize);

        for chunk in raw.chunks_exact(4) {
            pixels.push(ColorRgba::new(chunk[0], chunk[1], chunk[2], chunk[3]));
        }

        let img = crate::image::ImageBuffer {
            width: w,
            height: h,
            pixels,
        };

        let bmp_data = img.encode_bmp_bytes();
        if let Some(parent) = path.as_ref().parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(path, bmp_data)
    }

    /// Draws a solid color rectangle onto the internal framebuffer with fast 32-bit row fill.
    pub fn draw_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: ColorRgba) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }

        let target_w = self.width() as i32;
        let target_h = self.height() as i32;

        let ix0 = (x.round() as i32).clamp(0, target_w);
        let iy0 = (y.round() as i32).clamp(0, target_h);
        let ix1 = ((x + w).round() as i32).clamp(0, target_w);
        let iy1 = ((y + h).round() as i32).clamp(0, target_h);

        if ix0 >= ix1 || iy0 >= iy1 {
            return;
        }

        if color.a == 255 {
            let packed = u32::from_ne_bytes([color.r, color.g, color.b, 255]);
            let data = self.pixmap.data_mut();
            let u32_slice: &mut [u32] = unsafe {
                std::slice::from_raw_parts_mut(data.as_mut_ptr() as *mut u32, data.len() / 4)
            };

            let row_len = (ix1 - ix0) as usize;
            for py in iy0..iy1 {
                let row_start = (py as usize) * (target_w as usize) + (ix0 as usize);
                u32_slice[row_start..row_start + row_len].fill(packed);
            }
        } else if color.a > 0 {
            // High-performance integer row alpha blending (20x faster than software vector path)
            let a = color.a as u32;
            let inv_a = 255 - a;
            let sr = (color.r as u32 * a) / 255;
            let sg = (color.g as u32 * a) / 255;
            let sb = (color.b as u32 * a) / 255;

            let data = self.pixmap.data_mut();
            let u32_slice: &mut [u32] = unsafe {
                std::slice::from_raw_parts_mut(data.as_mut_ptr() as *mut u32, data.len() / 4)
            };

            let row_len = (ix1 - ix0) as usize;
            for py in iy0..iy1 {
                let row_start = (py as usize) * (target_w as usize) + (ix0 as usize);
                for pixel in &mut u32_slice[row_start..row_start + row_len] {
                    let p = *pixel;
                    let dr = p & 0xFF;
                    let dg = (p >> 8) & 0xFF;
                    let db = (p >> 16) & 0xFF;
                    let nr = sr + (dr * inv_a) / 255;
                    let ng = sg + (dg * inv_a) / 255;
                    let nb = sb + (db * inv_a) / 255;
                    *pixel = (255 << 24) | (nb << 16) | (ng << 8) | nr;
                }
            }
        }
    }

    /// Draws footer text (e.g. keybindings or layout indicators).
    pub fn draw_footer_text(&mut self, text: &str) {
        let s = self.viewport.scale;
        let hud_x = (self.skin.playfield_x + self.skin.playfield_width + 48.0 * s) as i32;
        let y = (self.viewport.y + self.viewport.height - 30.0 * s) as i32;
        let font_scale = (s * 0.9).round().max(1.0) as u32;
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            text,
            hud_x,
            y,
            font_scale,
            ColorRgba::new(140, 140, 160, 255),
        );
    }
}

pub(crate) fn level_color(level: u32) -> ColorRgba {
    match level {
        1..=4 => ColorRgba::new(80, 220, 130, 255),  // Mint Green (Normal)
        5..=8 => ColorRgba::new(60, 180, 255, 255),  // Cyan (Hyper)
        9..=10 => ColorRgba::new(255, 200, 50, 255), // Amber/Yellow (Another)
        11..=12 => ColorRgba::new(255, 70, 70, 255), // Crimson Red (Insane)
        _ => ColorRgba::new(210, 90, 255, 255),      // Purple / Overjoy
    }
}

pub(crate) fn clear_lamp_color(clear_type: Option<beetle_core::ClearType>) -> (&'static str, ColorRgba) {
    match clear_type {
        Some(beetle_core::ClearType::Perfect) => ("PERFECT", ColorRgba::new(255, 230, 80, 255)),
        Some(beetle_core::ClearType::FullCombo) => ("FULL COMBO", ColorRgba::new(80, 255, 140, 255)),
        Some(beetle_core::ClearType::Clear) => ("CLEARED", ColorRgba::new(60, 190, 255, 255)),
        Some(beetle_core::ClearType::Failed) => ("FAILED", ColorRgba::new(240, 60, 60, 255)),
        None => ("NO PLAY", ColorRgba::new(70, 75, 95, 255)),
    }
}

pub(crate) fn accuracy_to_rank(acc: f64) -> (&'static str, ColorRgba) {
    if acc >= (8.0 / 9.0) * 100.0 {
        ("AAA", ColorRgba::new(255, 230, 80, 255))
    } else if acc >= (7.0 / 9.0) * 100.0 {
        ("AA", ColorRgba::new(220, 220, 240, 255))
    } else if acc >= (6.0 / 9.0) * 100.0 {
        ("A", ColorRgba::new(100, 220, 120, 255))
    } else if acc >= (5.0 / 9.0) * 100.0 {
        ("B", ColorRgba::new(80, 180, 255, 255))
    } else if acc >= (4.0 / 9.0) * 100.0 {
        ("C", ColorRgba::new(180, 120, 240, 255))
    } else if acc >= (3.0 / 9.0) * 100.0 {
        ("D", ColorRgba::new(240, 140, 60, 255))
    } else {
        ("F", ColorRgba::new(220, 60, 60, 255))
    }
}

pub(crate) fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        truncated.push_str("...");
        truncated
    }
}

pub(crate) fn lane_index(lane: Lane) -> usize {
    match lane {
        Lane::Scratch => 0,
        Lane::Key1 => 1,
        Lane::Key2 => 2,
        Lane::Key3 => 3,
        Lane::Key4 => 4,
        Lane::Key5 => 5,
        Lane::Key6 => 6,
        Lane::Key7 => 7,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beetle_core::{BmsChart, BmsHeader, GaugeType, NoteEvent, NoteType, ScoreTracker, TimingModel};

    #[test]
    fn test_renderer_initialization_and_resize() {
        let mut renderer = SoftwareRenderer::new(800, 600, SkinConfig::default()).unwrap();
        assert_eq!(renderer.width(), 800);
        assert_eq!(renderer.height(), 600);
        assert_eq!(renderer.data().len(), 800 * 600 * 4);

        renderer.resize(1024, 768);
        assert_eq!(renderer.width(), 1024);
        assert_eq!(renderer.height(), 768);
    }

    #[test]
    fn test_render_gameplay_frame() {
        let mut renderer = SoftwareRenderer::new(800, 720, SkinConfig::default()).unwrap();
        let chart = BmsChart {
            header: BmsHeader {
                title: "Test Track".to_string(),
                artist: "Test Artist".to_string(),
                bpm: 150.0,
                ..Default::default()
            },
            notes: vec![NoteEvent {
                measure: 1,
                fraction: 0.0,
                lane: Lane::Key1,
                wav_id: None,
                note_type: NoteType::Tap,
            }],
            ..Default::default()
        };
        let timing = TimingModel::from_chart(&chart);
        let mut score = ScoreTracker::new(1, 200.0, GaugeType::Groove);
        score.record_hit(JudgeGrade::PerfectGreat);

        renderer.set_key_state(Lane::Key1, true);
        renderer.trigger_judge(JudgeGrade::PerfectGreat, 1.0, 0.0);
        let levels = [0.5f32; 16];
        let judge = beetle_core::JudgeEngine::new(&chart, &timing, GaugeType::Groove);
        renderer.render_gameplay(&chart, judge.notes(), 1.0, &score, &levels, None, None, 0.0, &timing);

        // Validate buffer is not all blank
        let has_content = renderer.data().chunks_exact(4).any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0);
        assert!(has_content);
    }

    #[test]
    fn test_render_gameplay_with_track_bga() {
        use crate::image::ImageBuffer;
        let mut renderer = SoftwareRenderer::new(800, 600, SkinConfig::default()).unwrap();
        let chart = BmsChart::default();
        let timing = TimingModel::from_chart(&chart);
        let score = ScoreTracker::new(1, 200.0, GaugeType::Groove);
        let levels = [0.5f32; 16];
        let judge = beetle_core::JudgeEngine::new(&chart, &timing, GaugeType::Groove);
        let dummy_bga = ImageBuffer::new(320, 180, ColorRgba::new(200, 100, 50, 255));

        renderer.render_gameplay(&chart, judge.notes(), 1.0, &score, &levels, Some(&dummy_bga), None, 0.5, &timing);

        let has_content = renderer.data().chunks_exact(4).any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0);
        assert!(has_content);
    }

    #[test]
    fn test_render_pause_modal() {
        let mut renderer = SoftwareRenderer::new(800, 600, SkinConfig::default()).unwrap();
        renderer.render_pause_modal("Sample Song", "Artist Name", 45.0, 120.0, 0);

        let has_content = renderer.data().chunks_exact(4).any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0);
        assert!(has_content);
    }

    #[test]
    fn test_render_exit_confirm_modal() {
        let mut renderer = SoftwareRenderer::new(800, 600, SkinConfig::default()).unwrap();
        renderer.render_exit_confirm_modal();

        let has_content = renderer.data().chunks_exact(4).any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0);
        assert!(has_content);
    }

    #[test]
    fn test_render_result_screen() {
        let mut renderer = SoftwareRenderer::new(800, 600, SkinConfig::default()).unwrap();
        let chart = BmsChart {
            header: BmsHeader {
                title: "Result Track".to_string(),
                artist: "Artist".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut score = ScoreTracker::new(100, 200.0, GaugeType::Groove);
        score.record_hit_with_delta(JudgeGrade::PerfectGreat, 0.0);
        score.record_hit_with_delta(JudgeGrade::Great, -12.0);
        score.record_hit_with_delta(JudgeGrade::Great, 15.0);

        renderer.render_result(&chart, &score, true, None);

        let has_content = renderer.data().chunks_exact(4).any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0);
        assert!(has_content);
    }

    #[test]
    fn test_render_key_config_and_option_modal() {
        let mut renderer = SoftwareRenderer::new(800, 600, SkinConfig::default()).unwrap();
        let key_names = [
            ("SCRATCH", "LShift".to_string()),
            ("KEY 1", "S".to_string()),
            ("KEY 2", "D".to_string()),
            ("KEY 3", "F".to_string()),
            ("KEY 4", "Space".to_string()),
            ("KEY 5", "J".to_string()),
            ("KEY 6", "K".to_string()),
            ("KEY 7", "L".to_string()),
        ];
        renderer.render_key_config(&key_names, 0, "HomeRow", false);
        let has_content1 = renderer.data().chunks_exact(4).any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0);
        assert!(has_content1);

        let options = beetle_core::PlayOptions::default();
        renderer.render_option_modal(&options, "HomeRow", false, 0, 1.0, "WINDOWED", "1280x720 (16:9)", "AUTO (D3D11/SOFT)", 240, "OFF (0%)", 0);
        let has_content2 = renderer.data().chunks_exact(4).any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0);
        assert!(has_content2);
    }
}

use crate::bitmap_font::BitmapFont;
use crate::skin::{ColorRgba, SkinConfig};
use beetle_core::{JudgeGrade, Lane};
use tiny_skia::{Color, Paint, Pixmap, Rect, Shader, Transform};

/// A visual particle burst spawned when hitting a note on a lane.
#[derive(Debug, Clone, Copy)]
pub struct HitBurst {
    pub lane: Lane,
    pub spawn_time: f64,
    pub grade: JudgeGrade,
}

/// Software 2D renderer powered by tiny-skia.
pub struct SoftwareRenderer {
    pub(crate) pixmap: Pixmap,
    pub skin: SkinConfig,
    pub(crate) key_pressed: [bool; 8],
    pub(crate) last_judge: Option<(JudgeGrade, f64, f64)>, // (Grade, time_seconds, delta_ms)
    pub(crate) hit_bursts: Vec<HitBurst>,
}

impl SoftwareRenderer {
    pub fn new(width: u32, height: u32, skin: SkinConfig) -> Option<Self> {
        let pixmap = Pixmap::new(width.max(1), height.max(1))?;
        Some(Self {
            pixmap,
            skin,
            key_pressed: [false; 8],
            last_judge: None,
            hit_bursts: Vec::with_capacity(32),
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 && (self.pixmap.width() != width || self.pixmap.height() != height) {
            if let Some(new_pixmap) = Pixmap::new(width, height) {
                self.pixmap = new_pixmap;
            }
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

    /// Clear frame with background color.
    pub fn clear(&mut self) {
        let bg = self.skin.bg_color;
        self.pixmap.fill(Color::from_rgba8(bg.r, bg.g, bg.b, bg.a));
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

    /// Draws a solid color rectangle onto the internal framebuffer.
    pub fn draw_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: ColorRgba) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        if let Some(rect) = Rect::from_xywh(x, y, w, h) {
            let skia_color = Color::from_rgba8(color.r, color.g, color.b, color.a);
            self.pixmap.fill_rect(
                rect,
                &Paint {
                    shader: Shader::SolidColor(skia_color),
                    ..Default::default()
                },
                Transform::identity(),
                None,
            );
        }
    }

    /// Draws footer text (e.g. keybindings or layout indicators).
    pub fn draw_footer_text(&mut self, text: &str) {
        let hud_x = (self.skin.playfield_x + self.skin.playfield_width + 60.0) as i32;
        let y = (self.height() - 40) as i32;
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            text,
            hud_x,
            y,
            1,
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
        renderer.render_gameplay(&chart, &timing, 1.0, &score, &levels, None);

        // Validate buffer is not all blank
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
        renderer.render_option_modal(&options, "HomeRow", false, 0, 1.0, 0);
        let has_content2 = renderer.data().chunks_exact(4).any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0);
        assert!(has_content2);
    }
}

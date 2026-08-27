use crate::bitmap_font::BitmapFont;
use crate::skin::{ColorRgba, SkinConfig};
use beetle_core::{BmsChart, GaugeType, JudgeGrade, Lane, NoteType, ScoreTracker, TimingModel};
use tiny_skia::{Color, Paint, Pixmap, Rect, Shader, Transform};

const ALL_LANES: [Lane; 8] = [
    Lane::Scratch,
    Lane::Key1,
    Lane::Key2,
    Lane::Key3,
    Lane::Key4,
    Lane::Key5,
    Lane::Key6,
    Lane::Key7,
];

/// Software 2D renderer powered by tiny-skia.
pub struct SoftwareRenderer {
    pixmap: Pixmap,
    pub skin: SkinConfig,
    key_pressed: [bool; 8],
    last_judge: Option<(JudgeGrade, f64)>, // (Grade, time_seconds)
}

impl SoftwareRenderer {
    pub fn new(width: u32, height: u32, skin: SkinConfig) -> Option<Self> {
        let pixmap = Pixmap::new(width.max(1), height.max(1))?;
        Some(Self {
            pixmap,
            skin,
            key_pressed: [false; 8],
            last_judge: None,
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

    pub fn trigger_judge(&mut self, grade: JudgeGrade, time_seconds: f64) {
        self.last_judge = Some((grade, time_seconds));
    }

    /// Clear frame with background color.
    pub fn clear(&mut self) {
        let bg = self.skin.bg_color;
        self.pixmap.fill(Color::from_rgba8(bg.r, bg.g, bg.b, bg.a));
    }

    /// Renders a single gameplay frame based on current audio time.
    pub fn render_gameplay(
        &mut self,
        chart: &BmsChart,
        timing: &TimingModel,
        audio_time_seconds: f64,
        score: &ScoreTracker,
    ) {
        self.clear();

        self.draw_playfield_bg();
        self.draw_key_beams();
        self.draw_notes(chart, timing, audio_time_seconds);
        self.draw_judge_line();
        self.draw_gauge_bar(score);
        self.draw_combo_and_judge(score, audio_time_seconds);
        self.draw_hud_info(chart, score);
    }

    fn draw_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: ColorRgba) {
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

    fn draw_playfield_bg(&mut self) {
        // Draw playfield main background box
        self.draw_rect(
            self.skin.playfield_x,
            self.skin.playfield_y,
            self.skin.playfield_width,
            self.skin.playfield_height,
            self.skin.playfield_bg_color,
        );

        // Draw lane vertical separator lines
        for &lane in &ALL_LANES {
            let x = self.skin.lane_x(lane);
            self.draw_rect(
                x,
                self.skin.playfield_y,
                1.0,
                self.skin.playfield_height,
                self.skin.lane_line_color,
            );
        }

        // Right boundary line
        let right_x = self.skin.playfield_x + self.skin.playfield_width;
        self.draw_rect(
            right_x,
            self.skin.playfield_y,
            1.0,
            self.skin.playfield_height,
            self.skin.lane_line_color,
        );
    }

    fn draw_key_beams(&mut self) {
        for &lane in &ALL_LANES {
            let idx = lane_index(lane);
            if self.key_pressed[idx] {
                let x = self.skin.lane_x(lane) + 1.0;
                let w = self.skin.lane_width(lane) - 1.0;
                let beam_color = self.skin.key_beam_color(lane);
                let beam_height = self.skin.judge_line_y - self.skin.playfield_y;

                self.draw_rect(x, self.skin.playfield_y, w, beam_height, beam_color);
            }
        }
    }

    fn draw_judge_line(&mut self) {
        self.draw_rect(
            self.skin.playfield_x,
            self.skin.judge_line_y,
            self.skin.playfield_width,
            2.0,
            self.skin.judge_line_color,
        );
    }

    fn draw_notes(&mut self, chart: &BmsChart, timing: &TimingModel, audio_time_seconds: f64) {
        let hi_speed = self.skin.hi_speed;
        let judge_y = self.skin.judge_line_y;
        let top_y = self.skin.playfield_y;
        let note_h = self.skin.note_height;

        let mut i = 0;
        while i < chart.notes.len() {
            let note = &chart.notes[i];
            let note_time = timing.beat_to_time_seconds(note.measure, note.fraction);
            let delta_t = note_time - audio_time_seconds;
            let note_y = judge_y - (delta_t as f32 * hi_speed);

            let lane = note.lane;
            let lane_x = self.skin.lane_x(lane) + 1.0;
            let lane_w = self.skin.lane_width(lane) - 2.0;
            let note_color = self.skin.lane_color(lane);

            match note.note_type {
                NoteType::Tap => {
                    // Only draw if within visible playfield vertical range
                    if note_y + note_h >= top_y && note_y - note_h <= judge_y + 40.0 {
                        self.draw_rect(lane_x, note_y - note_h, lane_w, note_h, note_color);
                    }
                }
                NoteType::LongNoteStart => {
                    // Find next matching LongNoteEnd on the same lane
                    let mut end_time = note_time;
                    for end_note in &chart.notes[i + 1..] {
                        if end_note.lane == lane && end_note.note_type == NoteType::LongNoteEnd {
                            end_time = timing.beat_to_time_seconds(end_note.measure, end_note.fraction);
                            break;
                        }
                    }

                    let end_delta = end_time - audio_time_seconds;
                    let end_y = judge_y - (end_delta as f32 * hi_speed);

                    let body_top = end_y.max(top_y);
                    let body_bottom = note_y.min(judge_y);

                    // Draw LN body
                    if body_bottom > body_top {
                        let body_color = note_color.with_alpha(140);
                        self.draw_rect(
                            lane_x + 3.0,
                            body_top,
                            lane_w - 6.0,
                            body_bottom - body_top,
                            body_color,
                        );
                    }

                    // Draw start head
                    if note_y + note_h >= top_y && note_y <= judge_y + 40.0 {
                        self.draw_rect(lane_x, note_y - note_h, lane_w, note_h, note_color);
                    }

                    // Draw end tail
                    if end_y + note_h >= top_y && end_y <= judge_y + 40.0 {
                        self.draw_rect(lane_x, end_y - note_h, lane_w, note_h, note_color);
                    }
                }
                _ => (),
            }

            i += 1;
        }
    }

    fn draw_gauge_bar(&mut self, score: &ScoreTracker) {
        let gauge_x = self.skin.playfield_x + self.skin.playfield_width + 20.0;
        let gauge_y = self.skin.playfield_y;
        let gauge_w = 20.0;
        let gauge_h = self.skin.playfield_height;

        // Gauge background
        self.draw_rect(
            gauge_x,
            gauge_y,
            gauge_w,
            gauge_h,
            ColorRgba::new(20, 20, 28, 255),
        );

        // Fill height
        let fill_ratio = (score.gauge / 100.0).clamp(0.0, 1.0) as f32;
        let fill_h = gauge_h * fill_ratio;
        let fill_y = gauge_y + gauge_h - fill_h;

        let fill_color = match score.gauge_type {
            GaugeType::Groove => {
                if score.gauge >= 80.0 {
                    ColorRgba::new(60, 240, 100, 255) // Green (Cleared)
                } else {
                    ColorRgba::new(60, 140, 255, 255) // Blue
                }
            }
            GaugeType::Hard => {
                if score.gauge < 30.0 {
                    ColorRgba::new(255, 50, 50, 255) // Red (Danger)
                } else {
                    ColorRgba::new(255, 180, 40, 255) // Orange
                }
            }
        };

        self.draw_rect(gauge_x, fill_y, gauge_w, fill_h, fill_color);

        // Border
        self.draw_rect(gauge_x, gauge_y, gauge_w, 1.0, ColorRgba::new(80, 80, 100, 255));
        self.draw_rect(gauge_x, gauge_y + gauge_h, gauge_w, 1.0, ColorRgba::new(80, 80, 100, 255));
        self.draw_rect(gauge_x, gauge_y, 1.0, gauge_h, ColorRgba::new(80, 80, 100, 255));
        self.draw_rect(gauge_x + gauge_w, gauge_y, 1.0, gauge_h, ColorRgba::new(80, 80, 100, 255));

        // Gauge 80% threshold line for Groove gauge
        if score.gauge_type == GaugeType::Groove {
            let line_y = gauge_y + gauge_h * 0.2;
            self.draw_rect(gauge_x - 3.0, line_y, gauge_w + 6.0, 2.0, ColorRgba::new(255, 220, 50, 255));
        }

        // Percentage text below gauge
        let gauge_str = format!("{:.1}%", score.gauge);
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &gauge_str,
            gauge_x as i32 - 10,
            (gauge_y + gauge_h + 8.0) as i32,
            1,
            ColorRgba::new(220, 220, 240, 255),
        );
    }

    fn draw_combo_and_judge(&mut self, score: &ScoreTracker, audio_time_seconds: f64) {
        let center_x = (self.skin.playfield_x + (self.skin.playfield_width / 2.0)) as i32;
        let judge_center_y = (self.skin.judge_line_y - 100.0) as i32;

        // 1. Draw Combo
        if score.current_combo > 0 {
            let combo_num = format!("{}", score.current_combo);
            BitmapFont::draw_text_centered(
                &mut self.pixmap.as_mut(),
                &combo_num,
                center_x,
                judge_center_y - 30,
                3, // Big font
                ColorRgba::new(255, 255, 255, 255),
            );

            BitmapFont::draw_text_centered(
                &mut self.pixmap.as_mut(),
                "COMBO",
                center_x,
                judge_center_y - 8,
                1,
                ColorRgba::new(180, 180, 200, 255),
            );
        }

        // 2. Draw Judge Popup
        if let Some((grade, judge_time)) = self.last_judge {
            let elapsed = audio_time_seconds - judge_time;
            if elapsed >= 0.0 && elapsed < 0.5 {
                let (text, color) = match grade {
                    JudgeGrade::PerfectGreat => ("PGREAT", ColorRgba::new(255, 230, 80, 255)),
                    JudgeGrade::Great => ("GREAT", ColorRgba::new(255, 170, 50, 255)),
                    JudgeGrade::Good => ("GOOD", ColorRgba::new(60, 220, 120, 255)),
                    JudgeGrade::Bad => ("BAD", ColorRgba::new(180, 70, 240, 255)),
                    JudgeGrade::Poor => ("POOR", ColorRgba::new(240, 50, 50, 255)),
                    JudgeGrade::Miss => ("MISS", ColorRgba::new(140, 140, 140, 255)),
                };

                BitmapFont::draw_text_centered(
                    &mut self.pixmap.as_mut(),
                    text,
                    center_x,
                    judge_center_y + 8,
                    2,
                    color,
                );
            }
        }
    }

    fn draw_hud_info(&mut self, chart: &BmsChart, score: &ScoreTracker) {
        let hud_x = (self.skin.playfield_x + self.skin.playfield_width + 60.0) as i32;
        let mut hud_y = self.skin.playfield_y as i32;

        // Title & Artist
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &chart.header.title,
            hud_x,
            hud_y,
            2,
            ColorRgba::new(255, 255, 255, 255),
        );
        hud_y += 20;

        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &chart.header.artist,
            hud_x,
            hud_y,
            1,
            ColorRgba::new(160, 160, 180, 255),
        );
        hud_y += 30;

        // BPM & Play Level
        let bpm_str = format!("BPM: {:.1}", chart.header.bpm);
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &bpm_str,
            hud_x,
            hud_y,
            1,
            ColorRgba::new(200, 200, 220, 255),
        );
        hud_y += 16;

        let lvl_str = format!("LEVEL: {}", chart.header.play_level);
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &lvl_str,
            hud_x,
            hud_y,
            1,
            ColorRgba::new(200, 200, 220, 255),
        );
        hud_y += 30;

        // EX-Score and Accuracy Rate
        let ex_str = format!("EX SCORE: {} / {}", score.ex_score, score.max_ex_score());
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &ex_str,
            hud_x,
            hud_y,
            1,
            ColorRgba::new(255, 230, 100, 255),
        );
        hud_y += 16;

        let acc_str = format!("ACCURACY: {:.2}%", score.accuracy_rate());
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &acc_str,
            hud_x,
            hud_y,
            1,
            ColorRgba::new(100, 220, 255, 255),
        );
        hud_y += 30;

        // Judge breakdown table
        let counts = [
            ("PGREAT", score.pgreat_count, ColorRgba::new(255, 230, 80, 255)),
            ("GREAT ", score.great_count, ColorRgba::new(255, 170, 50, 255)),
            ("GOOD  ", score.good_count, ColorRgba::new(60, 220, 120, 255)),
            ("BAD   ", score.bad_count, ColorRgba::new(180, 70, 240, 255)),
            ("POOR  ", score.poor_count, ColorRgba::new(240, 50, 50, 255)),
            ("MISS  ", score.miss_count, ColorRgba::new(140, 140, 140, 255)),
        ];

        for (label, count, color) in counts {
            let row = format!("{}: {:>4}", label, count);
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &row, hud_x, hud_y, 1, color);
            hud_y += 14;
        }
    }
}

fn lane_index(lane: Lane) -> usize {
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
    use beetle_core::{BmsHeader, NoteEvent};

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
        renderer.trigger_judge(JudgeGrade::PerfectGreat, 1.0);
        renderer.render_gameplay(&chart, &timing, 1.0, &score);

        // Validate buffer is not all blank
        let has_content = renderer.data().chunks_exact(4).any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0);
        assert!(has_content);
    }
}

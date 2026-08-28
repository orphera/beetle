use crate::bitmap_font::BitmapFont;
use crate::image::ImageBuffer;
use crate::skin::{ColorRgba, SkinConfig};
use beetle_core::{BmsChart, GaugeType, JudgeGrade, Lane, NoteType, ScoreTracker, TimingModel};
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
    pixmap: Pixmap,
    pub skin: SkinConfig,
    key_pressed: [bool; 8],
    last_judge: Option<(JudgeGrade, f64, f64)>, // (Grade, time_seconds, delta_ms)
    hit_bursts: Vec<HitBurst>,
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

    /// Renders a single gameplay frame based on current audio time.
    pub fn render_gameplay(
        &mut self,
        chart: &BmsChart,
        timing: &TimingModel,
        audio_time_seconds: f64,
        score: &ScoreTracker,
        visual_levels: &[f32; 16],
        bga_image: Option<&crate::image::ImageBuffer>,
    ) {
        let is_danger = (score.gauge < 30.0 && matches!(score.gauge_type, GaugeType::Hard | GaugeType::Groove))
            || (score.gauge_type == GaugeType::Hazard && score.gauge < 100.0);
        let danger_blink = is_danger && ((audio_time_seconds * 6.0).sin() > 0.0);

        self.draw_playfield_bg(score.current_combo, danger_blink);
        self.draw_key_beams();
        self.draw_notes(chart, timing, audio_time_seconds);
        self.draw_lane_cover();
        self.draw_judge_line(score.current_combo);
        self.draw_hit_bursts(audio_time_seconds);
        self.draw_gauge_bar(score, danger_blink);
        self.draw_combo_and_judge(score, audio_time_seconds);
        self.draw_hud_info(chart, score);
        self.draw_bga_and_visualizer(visual_levels, bga_image);
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

    fn draw_playfield_bg(&mut self, combo: u32, danger_blink: bool) {
        // Draw playfield main background box
        self.draw_rect(
            self.skin.playfield_x,
            self.skin.playfield_y,
            self.skin.playfield_width,
            self.skin.playfield_height,
            self.skin.playfield_bg_color,
        );

        let line_color = if combo >= 100 {
            ColorRgba::new(50, 100, 180, 220) // Subtle ambient blue for active combo
        } else {
            self.skin.lane_line_color
        };

        // Draw lane vertical separator lines
        for &lane in self.skin.active_lanes() {
            let x = self.skin.lane_x(lane);
            self.draw_rect(
                x,
                self.skin.playfield_y,
                1.0,
                self.skin.playfield_height,
                line_color,
            );
        }

        // Right boundary line
        let right_x = self.skin.playfield_x + self.skin.playfield_width;
        self.draw_rect(
            right_x,
            self.skin.playfield_y,
            1.0,
            self.skin.playfield_height,
            line_color,
        );

        // Danger pulsing border around entire playfield
        if danger_blink {
            let px = self.skin.playfield_x;
            let py = self.skin.playfield_y;
            let pw = self.skin.playfield_width;
            let ph = self.skin.playfield_height;
            let danger_col = ColorRgba::new(255, 40, 40, 220);
            self.draw_rect(px, py, pw, 2.0, danger_col);
            self.draw_rect(px, py + ph - 2.0, pw, 2.0, danger_col);
            self.draw_rect(px, py, 2.0, ph, danger_col);
            self.draw_rect(px + pw - 2.0, py, 2.0, ph, danger_col);
        }
    }

    fn draw_key_beams(&mut self) {
        let active = self.skin.active_lanes().to_vec();
        for lane in active {
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

    fn draw_judge_line(&mut self, combo: u32) {
        let jy = self.skin.judge_line_y;
        let px = self.skin.playfield_x;
        let pw = self.skin.playfield_width;

        // Subtle neon ambient glow above and below judge line
        let glow_color = if combo >= 200 {
            ColorRgba::new(255, 215, 60, 90) // Gold
        } else if combo >= 50 {
            ColorRgba::new(60, 200, 255, 80) // Cyan
        } else {
            ColorRgba::new(255, 70, 70, 70) // Reddish
        };
        self.draw_rect(px, jy - 2.0, pw, 6.0, glow_color);

        // Core bright judge line
        self.draw_rect(
            px,
            jy,
            pw,
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

    fn draw_gauge_bar(&mut self, score: &ScoreTracker, danger_blink: bool) {
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
            GaugeType::Easy => {
                if score.gauge >= 80.0 {
                    ColorRgba::new(80, 255, 160, 255) // Bright Mint Green
                } else {
                    ColorRgba::new(60, 200, 240, 255) // Cyan
                }
            }
            GaugeType::Groove => {
                if score.gauge >= 80.0 {
                    ColorRgba::new(60, 240, 100, 255) // Green (Cleared)
                } else if danger_blink {
                    ColorRgba::new(255, 70, 70, 255) // Pulsing danger red
                } else {
                    ColorRgba::new(60, 140, 255, 255) // Blue
                }
            }
            GaugeType::Hard => {
                if danger_blink {
                    ColorRgba::new(255, 40, 40, 255) // Pulsing danger red
                } else if score.gauge < 30.0 {
                    ColorRgba::new(255, 70, 70, 255) // Red (Danger)
                } else {
                    ColorRgba::new(255, 180, 40, 255) // Orange
                }
            }
            GaugeType::Hazard => {
                if danger_blink {
                    ColorRgba::new(255, 50, 50, 255)
                } else {
                    ColorRgba::new(240, 40, 80, 255) // Crimson Hazard
                }
            }
        };

        self.draw_rect(gauge_x, fill_y, gauge_w, fill_h, fill_color);

        // Border
        let border_color = if danger_blink {
            ColorRgba::new(255, 60, 60, 255)
        } else {
            ColorRgba::new(80, 80, 100, 255)
        };
        self.draw_rect(gauge_x, gauge_y, gauge_w, 1.0, border_color);
        self.draw_rect(gauge_x, gauge_y + gauge_h, gauge_w, 1.0, border_color);
        self.draw_rect(gauge_x, gauge_y, 1.0, gauge_h, border_color);
        self.draw_rect(gauge_x + gauge_w, gauge_y, 1.0, gauge_h, border_color);

        // Gauge 80% threshold line for Easy / Groove gauge
        if matches!(score.gauge_type, GaugeType::Easy | GaugeType::Groove) {
            let line_y = gauge_y + gauge_h * 0.2;
            self.draw_rect(gauge_x - 3.0, line_y, gauge_w + 6.0, 2.0, ColorRgba::new(255, 220, 50, 255));
        }

        // Percentage text below gauge
        let gauge_str = format!("{:.1}%", score.gauge);
        let text_color = if danger_blink {
            ColorRgba::new(255, 80, 80, 255)
        } else {
            ColorRgba::new(220, 220, 240, 255)
        };
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &gauge_str,
            gauge_x as i32 - 10,
            (gauge_y + gauge_h + 8.0) as i32,
            1,
            text_color,
        );
    }

    fn draw_hit_bursts(&mut self, audio_time_seconds: f64) {
        let burst_duration = 0.22;
        self.hit_bursts.retain(|b| {
            let elapsed = audio_time_seconds - b.spawn_time;
            elapsed >= 0.0 && elapsed < burst_duration
        });

        let judge_y = self.skin.judge_line_y;
        let bursts = self.hit_bursts.clone();

        for burst in bursts {
            let elapsed = (audio_time_seconds - burst.spawn_time).max(0.0);
            let progress = (elapsed / burst_duration) as f32;
            let alpha = ((1.0 - progress) * 255.0) as u8;
            if alpha == 0 {
                continue;
            }

            let lx = self.skin.lane_x(burst.lane) + self.skin.lane_width(burst.lane) / 2.0;
            let (r, g, b) = match burst.grade {
                JudgeGrade::PerfectGreat => (255, 230, 80),
                JudgeGrade::Great => (255, 170, 50),
                JudgeGrade::Good => (60, 220, 120),
                _ => (160, 160, 180),
            };

            // 1. Lane neon beam flash for PGREAT
            if burst.grade == JudgeGrade::PerfectGreat {
                let flash_alpha = ((1.0 - progress) * 50.0) as u8;
                let lane_x = self.skin.lane_x(burst.lane);
                let lane_w = self.skin.lane_width(burst.lane);
                self.draw_rect(
                    lane_x,
                    self.skin.playfield_y,
                    lane_w,
                    self.skin.playfield_height,
                    ColorRgba::new(255, 240, 140, flash_alpha),
                );
            }

            // 2. Central expanding burst flare
            let flare_size = (1.0 - progress) * 26.0 + 4.0;
            self.draw_rect(
                lx - flare_size / 2.0,
                judge_y - flare_size / 2.0,
                flare_size,
                flare_size,
                ColorRgba::new(r, g, b, alpha),
            );

            // 3. Radiating particle sparks
            let dist = progress * 32.0;
            let spark_size = (1.0 - progress) * 4.0 + 1.0;
            let spark_alpha = (alpha / 2).max(1);
            let spark_col = ColorRgba::new(r, g, b, spark_alpha);

            let offsets = [
                (0.0, -dist),
                (0.0, dist),
                (-dist, 0.0),
                (dist, 0.0),
                (-dist * 0.7, -dist * 0.7),
                (dist * 0.7, -dist * 0.7),
                (-dist * 0.7, dist * 0.7),
                (dist * 0.7, dist * 0.7),
            ];

            for (dx, dy) in offsets {
                self.draw_rect(
                    lx + dx - spark_size / 2.0,
                    judge_y + dy - spark_size / 2.0,
                    spark_size,
                    spark_size,
                    spark_col,
                );
            }
        }
    }

    fn draw_combo_and_judge(&mut self, score: &ScoreTracker, audio_time_seconds: f64) {
        let center_x = (self.skin.playfield_x + (self.skin.playfield_width / 2.0)) as i32;
        let judge_center_y = (self.skin.judge_line_y - 100.0) as i32;

        // 1. Draw Combo with bounce pulse
        if score.current_combo > 0 {
            let combo_num = format!("{}", score.current_combo);
            let pulse_offset = if let Some((_, judge_time, _)) = self.last_judge {
                let elapsed = audio_time_seconds - judge_time;
                if elapsed >= 0.0 && elapsed < 0.12 {
                    ((1.0 - (elapsed / 0.12)) * 6.0) as i32
                } else {
                    0
                }
            } else {
                0
            };

            let combo_y = judge_center_y - 30 - pulse_offset;

            BitmapFont::draw_text_centered(
                &mut self.pixmap.as_mut(),
                &combo_num,
                center_x,
                combo_y,
                3, // Big font
                ColorRgba::new(255, 255, 255, 255),
            );

            BitmapFont::draw_text_centered(
                &mut self.pixmap.as_mut(),
                "COMBO",
                center_x,
                combo_y + 22,
                1,
                ColorRgba::new(180, 180, 200, 255),
            );
        }

        // 2. Draw Judge Popup & FAST/SLOW
        if let Some((grade, judge_time, delta_ms)) = self.last_judge {
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

                // FAST / SLOW indicator
                if grade != JudgeGrade::Miss && delta_ms.abs() >= 4.0 {
                    let (fast_slow_str, fs_color) = if delta_ms < 0.0 {
                        (format!("FAST {:.0}ms", delta_ms), ColorRgba::new(80, 210, 255, 255))
                    } else {
                        (format!("SLOW +{:.0}ms", delta_ms), ColorRgba::new(255, 140, 60, 255))
                    };

                    BitmapFont::draw_text_centered(
                        &mut self.pixmap.as_mut(),
                        &fast_slow_str,
                        center_x,
                        judge_center_y + 26,
                        1,
                        fs_color,
                    );
                }
            }
        }
    }

    fn draw_lane_cover(&mut self) {
        if self.skin.lane_cover_ratio > 0.0 {
            let ratio = self.skin.lane_cover_ratio.clamp(0.0, 0.85);
            let cover_h = self.skin.playfield_height * ratio;
            self.draw_rect(
                self.skin.playfield_x,
                self.skin.playfield_y,
                self.skin.playfield_width,
                cover_h,
                ColorRgba::new(12, 12, 18, 255),
            );
            self.draw_rect(
                self.skin.playfield_x,
                self.skin.playfield_y + cover_h - 2.0,
                self.skin.playfield_width,
                2.0,
                ColorRgba::new(80, 140, 255, 255),
            );
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
        hud_y += 20;

        // Pacemaker (AAA target = 8/9 of max possible EX score so far)
        let played_notes = score.pgreat_count + score.great_count + score.good_count + score.bad_count + score.poor_count + score.miss_count;
        let max_so_far = played_notes * 2;
        let aaa_target = ((max_so_far as f64) * 8.0 / 9.0).round() as i32;
        let pace_diff = score.ex_score as i32 - aaa_target;
        let (pace_str, pace_color) = if pace_diff >= 0 {
            (format!("PACEMAKER (AAA): +{}", pace_diff), ColorRgba::new(100, 255, 120, 255))
        } else {
            (format!("PACEMAKER (AAA): {}", pace_diff), ColorRgba::new(255, 90, 90, 255))
        };
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &pace_str,
            hud_x,
            hud_y,
            1,
            pace_color,
        );
        hud_y += 24;

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

    fn draw_bga_and_visualizer(
        &mut self,
        levels: &[f32; 16],
        bga_image: Option<&crate::image::ImageBuffer>,
    ) {
        let side_x = self.skin.playfield_x + self.skin.playfield_width + 60.0;
        let bga_y = self.skin.playfield_y + 240.0;
        let bga_w = 320.0;
        let bga_h = 180.0;

        // BGA frame
        if let Some(img) = bga_image {
            img.draw_scaled(&mut self.pixmap, side_x as i32, bga_y as i32, bga_w as u32, bga_h as u32);
            self.draw_rect(side_x, bga_y, bga_w, 1.0, ColorRgba::new(80, 140, 255, 255));
            self.draw_rect(side_x, bga_y + bga_h, bga_w, 1.0, ColorRgba::new(80, 140, 255, 255));
            self.draw_rect(side_x, bga_y, 1.0, bga_h, ColorRgba::new(80, 140, 255, 255));
            self.draw_rect(side_x + bga_w, bga_y, 1.0, bga_h, ColorRgba::new(80, 140, 255, 255));
        } else {
            self.draw_rect(side_x, bga_y, bga_w, bga_h, ColorRgba::new(14, 16, 22, 255));
            self.draw_rect(side_x, bga_y, bga_w, 1.0, ColorRgba::new(50, 60, 80, 255));
            self.draw_rect(side_x, bga_y + bga_h, bga_w, 1.0, ColorRgba::new(50, 60, 80, 255));
            self.draw_rect(side_x, bga_y, 1.0, bga_h, ColorRgba::new(50, 60, 80, 255));
            self.draw_rect(side_x + bga_w, bga_y, 1.0, bga_h, ColorRgba::new(50, 60, 80, 255));
            BitmapFont::draw_text_centered(
                &mut self.pixmap.as_mut(),
                "[ BGA FRAME ]",
                (side_x + bga_w / 2.0) as i32,
                (bga_y + bga_h / 2.0 - 4.0) as i32,
                1,
                ColorRgba::new(80, 90, 110, 255),
            );
        }

        let vis_y = bga_y + bga_h + 16.0;
        let vis_h = 100.0;

        // Visualizer background
        self.draw_rect(side_x, vis_y, bga_w, vis_h, ColorRgba::new(12, 14, 20, 255));
        self.draw_rect(side_x, vis_y, bga_w, 1.0, ColorRgba::new(60, 70, 90, 255));
        self.draw_rect(side_x, vis_y + vis_h, bga_w, 1.0, ColorRgba::new(60, 70, 90, 255));
        self.draw_rect(side_x, vis_y, 1.0, vis_h, ColorRgba::new(60, 70, 90, 255));
        self.draw_rect(side_x + bga_w, vis_y, 1.0, vis_h, ColorRgba::new(60, 70, 90, 255));

        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "SPECTRUM VISUALIZER",
            (side_x + 12.0) as i32,
            (vis_y + 8.0) as i32,
            1,
            ColorRgba::new(140, 150, 180, 255),
        );

        let bar_count = 16;
        let padding = 12.0;
        let usable_w = bga_w - (padding * 2.0);
        let bar_w = (usable_w / bar_count as f32) - 4.0;
        let max_h = 60.0;
        let base_y = vis_y + vis_h - 10.0;

        for (i, &lvl) in levels.iter().enumerate() {
            let clamped_lvl = lvl.clamp(0.0, 1.0);
            let h = (clamped_lvl * max_h).max(2.0);
            let x = side_x + padding + (i as f32 * (bar_w + 4.0));
            let y = base_y - h;

            // Dynamic frequency-based color gradient
            let r = ((60.0 + i as f32 * 10.0 + clamped_lvl * 50.0).min(255.0)) as u8;
            let g = ((160.0 - i as f32 * 4.0 + clamped_lvl * 40.0).clamp(40.0, 255.0)) as u8;
            let b = (255.0 - clamped_lvl * 40.0) as u8;

            self.draw_rect(x, y, bar_w, h, ColorRgba::new(r, g, b, 255));
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

    /// Renders the song select screen with list and metadata panel.
    pub fn render_song_select(
        &mut self,
        songs: &[beetle_core::SongMetadata],
        selected_idx: usize,
        score_store: &beetle_core::ScoreStore,
        sort_mode_str: &str,
        category_str: &str,
        search_query: &str,
        is_search_active: bool,
        stage_image: Option<&crate::image::ImageBuffer>,
        total_library_count: usize,
    ) {
        self.clear();

        let w = self.width() as f32;
        let h = self.height() as f32;

        // 1. Top Header Bar
        self.draw_rect(0.0, 0.0, w, 54.0, ColorRgba::new(14, 16, 26, 255));
        self.draw_rect(0.0, 53.0, w, 1.0, ColorRgba::new(45, 60, 95, 255));

        // Header Title
        BitmapFont::draw_text_with_shadow(
            &mut self.pixmap.as_mut(),
            "BEETLE BMS PLAYER",
            24,
            12,
            2,
            ColorRgba::new(255, 255, 255, 255),
            ColorRgba::new(10, 10, 20, 255),
            1,
            1,
        );

        // Category & Sort Badges
        let folder_badge_text = format!("FOLDER: {}", category_str);
        BitmapFont::draw_badge(
            &mut self.pixmap.as_mut(),
            &folder_badge_text,
            230,
            14,
            1,
            ColorRgba::new(80, 220, 255, 255),
            ColorRgba::new(20, 35, 60, 255),
            ColorRgba::new(60, 140, 240, 255),
            8,
            4,
        );

        let sort_badge_text = format!("SORT: {}", sort_mode_str);
        let sort_x = 230 + BitmapFont::text_width(&folder_badge_text, 1) as i32 + 24;
        BitmapFont::draw_badge(
            &mut self.pixmap.as_mut(),
            &sort_badge_text,
            sort_x,
            14,
            1,
            ColorRgba::new(255, 220, 90, 255),
            ColorRgba::new(45, 40, 20, 255),
            ColorRgba::new(180, 150, 40, 255),
            8,
            4,
        );

        // Search Input Box (Top Right)
        let search_box_w = 260.0;
        let search_box_x = (w - search_box_w - 24.0).max(sort_x as f32 + 150.0);
        let search_border_col = if is_search_active {
            ColorRgba::new(80, 210, 255, 255)
        } else {
            ColorRgba::new(50, 55, 75, 255)
        };

        self.draw_rect(search_box_x, 11.0, search_box_w, 32.0, ColorRgba::new(22, 24, 36, 255));
        self.draw_rect(search_box_x, 11.0, search_box_w, 1.0, search_border_col);
        self.draw_rect(search_box_x, 42.0, search_box_w, 1.0, search_border_col);
        self.draw_rect(search_box_x, 11.0, 1.0, 32.0, search_border_col);
        self.draw_rect(search_box_x + search_box_w - 1.0, 11.0, 1.0, 32.0, search_border_col);

        let search_disp = if search_query.is_empty() && !is_search_active {
            "[/] Search title/artist...".to_string()
        } else {
            format!("Search: {}{}", search_query, if is_search_active { "_" } else { "" })
        };
        let search_text_col = if is_search_active {
            ColorRgba::new(255, 255, 255, 255)
        } else {
            ColorRgba::new(140, 145, 170, 255)
        };
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &search_disp,
            search_box_x as i32 + 10,
            20,
            1,
            search_text_col,
        );

        let total_songs = songs.len();
        if total_songs == 0 {
            let msg = if !search_query.is_empty() {
                format!("No songs match search query: \"{}\"", search_query)
            } else {
                "No BMS songs found in songs/ directory.".to_string()
            };
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &msg, 40, 100, 1, ColorRgba::new(220, 200, 200, 255));
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                "Press [Esc] or [/] to reset search, or place .bms files into songs/ directory.",
                40,
                130,
                1,
                ColorRgba::new(140, 145, 170, 255),
            );
            return;
        }

        // 2. Left Song List Panel (Carousel view)
        let list_x = 24;
        let mut list_y = 70;
        let list_w = ((w * 0.52).min(460.0)).max(340.0);
        let max_visible = ((h - 130.0) / 32.0).max(6.0) as usize;

        let start_idx = if selected_idx >= max_visible / 2 {
            (selected_idx + 1).saturating_sub(max_visible / 2).min(total_songs.saturating_sub(max_visible))
        } else {
            0
        };
        let end_idx = (start_idx + max_visible).min(total_songs);

        for i in start_idx..end_idx {
            let song = &songs[i];
            let is_selected = i == selected_idx;

            let best_record = score_store.get(song.hash);
            let (lamp_str, lamp_color) = clear_lamp_color(best_record.map(|b| b.clear_type));
            let lvl_color = level_color(song.play_level);

            // Card Dimensions
            let card_y = list_y as f32;
            let card_h = 30.0;
            let card_w = if is_selected { list_w + 12.0 } else { list_w };

            // Card Background & Glowing Border
            if is_selected {
                self.draw_rect(list_x as f32, card_y, card_w, card_h, ColorRgba::new(32, 54, 110, 255));
                self.draw_rect(list_x as f32, card_y, card_w, 1.0, ColorRgba::new(90, 190, 255, 255));
                self.draw_rect(list_x as f32, card_y + card_h - 1.0, card_w, 1.0, ColorRgba::new(90, 190, 255, 255));
                self.draw_rect(list_x as f32 + card_w - 1.0, card_y, 1.0, card_h, ColorRgba::new(90, 190, 255, 255));
            } else {
                let bg_col = if i % 2 == 0 { ColorRgba::new(16, 18, 26, 220) } else { ColorRgba::new(20, 22, 32, 220) };
                self.draw_rect(list_x as f32, card_y, card_w, card_h, bg_col);
                self.draw_rect(list_x as f32, card_y, card_w, 1.0, ColorRgba::new(35, 40, 55, 255));
                self.draw_rect(list_x as f32, card_y + card_h - 1.0, card_w, 1.0, ColorRgba::new(35, 40, 55, 255));
            }

            // Left Clear Lamp Bar
            self.draw_rect(list_x as f32, card_y, 5.0, card_h, lamp_color);

            // Level Pill Badge
            let lvl_text = format!("LV.{:>2}", song.play_level);
            BitmapFont::draw_badge(
                &mut self.pixmap.as_mut(),
                &lvl_text,
                list_x + 12,
                list_y + 4,
                1,
                ColorRgba::new(255, 255, 255, 255),
                ColorRgba::new(20, 22, 30, 255),
                lvl_color,
                4,
                2,
            );

            // Song Title (Multilingual BitmapFont!)
            let title_x = list_x + 72;
            let title_color = if is_selected {
                ColorRgba::new(255, 255, 255, 255)
            } else {
                ColorRgba::new(190, 195, 215, 255)
            };

            let truncated_title = truncate_str(&song.title, 24);
            if is_selected {
                BitmapFont::draw_text_with_shadow(
                    &mut self.pixmap.as_mut(),
                    &truncated_title,
                    title_x,
                    list_y + 7,
                    1,
                    title_color,
                    ColorRgba::new(10, 15, 30, 255),
                    1,
                    1,
                );
            } else {
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    &truncated_title,
                    title_x,
                    list_y + 7,
                    1,
                    title_color,
                );
            }

            // Right Mini Lamp Status Tag
            let status_x = (list_x as f32 + card_w - 75.0) as i32;
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                lamp_str,
                status_x,
                list_y + 8,
                1,
                lamp_color,
            );

            list_y += 32;
        }

        // List Scrollbar
        let scroll_track_x = list_x as f32 + list_w + 18.0;
        let scroll_track_y = 70.0;
        let scroll_track_h = (max_visible * 32) as f32;
        self.draw_rect(scroll_track_x, scroll_track_y, 4.0, scroll_track_h, ColorRgba::new(25, 30, 45, 255));

        if total_songs > 0 {
            let thumb_h = ((max_visible as f32 / total_songs as f32) * scroll_track_h).clamp(16.0, scroll_track_h);
            let thumb_y = scroll_track_y + (selected_idx as f32 / total_songs as f32) * (scroll_track_h - thumb_h);
            self.draw_rect(scroll_track_x, thumb_y, 4.0, thumb_h, ColorRgba::new(80, 180, 255, 255));
        }

        // 3. Right Song Detail Card
        let detail_x = scroll_track_x + 20.0;
        let detail_y = 70.0;
        let detail_w = (w - detail_x - 24.0).max(280.0);
        let detail_h = (h - 130.0).max(460.0);

        // Detail Glass Panel
        self.draw_rect(detail_x, detail_y, detail_w, detail_h, ColorRgba::new(14, 16, 26, 255));
        self.draw_rect(detail_x, detail_y, detail_w, 1.0, ColorRgba::new(45, 55, 80, 255));
        self.draw_rect(detail_x, detail_y + detail_h - 1.0, detail_w, 1.0, ColorRgba::new(45, 55, 80, 255));
        self.draw_rect(detail_x, detail_y, 1.0, detail_h, ColorRgba::new(45, 55, 80, 255));
        self.draw_rect(detail_x + detail_w - 1.0, detail_y, 1.0, detail_h, ColorRgba::new(45, 55, 80, 255));

        if let Some(selected_song) = songs.get(selected_idx) {
            let mut cur_y = detail_y + 16.0;

            // Artwork Frame
            let art_w = detail_w - 32.0;
            let art_h = (art_w * 9.0 / 16.0).clamp(100.0, 150.0);
            let art_x = detail_x + 16.0;

            self.draw_rect(art_x - 1.0, cur_y - 1.0, art_w + 2.0, art_h + 2.0, ColorRgba::new(60, 80, 120, 255));
            self.draw_rect(art_x, cur_y, art_w, art_h, ColorRgba::new(10, 12, 18, 255));

            if let Some(img) = stage_image {
                img.draw_scaled(&mut self.pixmap, art_x as i32, cur_y as i32, art_w as u32, art_h as u32);
            } else {
                BitmapFont::draw_text_centered(
                    &mut self.pixmap.as_mut(),
                    "[ STAGE IMAGE ]",
                    (art_x + art_w / 2.0) as i32,
                    (cur_y + art_h / 2.0 - 4.0) as i32,
                    1,
                    ColorRgba::new(75, 85, 110, 255),
                );
            }
            cur_y += art_h + 16.0;

            // Song Title with Drop Shadow
            BitmapFont::draw_text_with_shadow(
                &mut self.pixmap.as_mut(),
                &truncate_str(&selected_song.title, 24),
                art_x as i32,
                cur_y as i32,
                2,
                ColorRgba::new(255, 255, 255, 255),
                ColorRgba::new(10, 10, 20, 255),
                1,
                1,
            );
            cur_y += 24.0;

            // Artist & Genre
            let artist_genre = if !selected_song.genre.is_empty() {
                format!("{} / {}", truncate_str(&selected_song.artist, 18), truncate_str(&selected_song.genre, 14))
            } else {
                truncate_str(&selected_song.artist, 26)
            };
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &artist_genre,
                art_x as i32,
                cur_y as i32,
                1,
                ColorRgba::new(150, 160, 190, 255),
            );
            cur_y += 24.0;

            // Attribute 2x2 Grid
            let grid_box_w = (art_w - 12.0) / 2.0;
            let grid_box_h = 44.0;

            // Box 1: BPM
            self.draw_rect(art_x, cur_y, grid_box_w, grid_box_h, ColorRgba::new(20, 24, 36, 255));
            self.draw_rect(art_x, cur_y, grid_box_w, 1.0, ColorRgba::new(40, 50, 75, 255));
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), "BPM", (art_x + 8.0) as i32, (cur_y + 6.0) as i32, 1, ColorRgba::new(120, 130, 160, 255));
            let bpm_val = format!("{:.1}", selected_song.bpm);
            BitmapFont::draw_bold_text(&mut self.pixmap.as_mut(), &bpm_val, (art_x + 8.0) as i32, (cur_y + 20.0) as i32, 1, ColorRgba::new(255, 220, 90, 255));

            // Box 2: Total Notes
            let box2_x = art_x + grid_box_w + 12.0;
            self.draw_rect(box2_x, cur_y, grid_box_w, grid_box_h, ColorRgba::new(20, 24, 36, 255));
            self.draw_rect(box2_x, cur_y, grid_box_w, 1.0, ColorRgba::new(40, 50, 75, 255));
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), "NOTES", (box2_x + 8.0) as i32, (cur_y + 6.0) as i32, 1, ColorRgba::new(120, 130, 160, 255));
            let notes_val = format!("{}", selected_song.notes_count);
            BitmapFont::draw_bold_text(&mut self.pixmap.as_mut(), &notes_val, (box2_x + 8.0) as i32, (cur_y + 20.0) as i32, 1, ColorRgba::new(100, 220, 255, 255));
            cur_y += grid_box_h + 16.0;

            // Personal Best Card
            self.draw_rect(art_x, cur_y, art_w, 135.0, ColorRgba::new(18, 22, 34, 255));
            self.draw_rect(art_x, cur_y, art_w, 1.0, ColorRgba::new(55, 65, 95, 255));
            self.draw_rect(art_x, cur_y + 134.0, art_w, 1.0, ColorRgba::new(55, 65, 95, 255));
            self.draw_rect(art_x, cur_y, 1.0, 135.0, ColorRgba::new(55, 65, 95, 255));
            self.draw_rect(art_x + art_w - 1.0, cur_y, 1.0, 135.0, ColorRgba::new(55, 65, 95, 255));

            let pb_header_y = cur_y + 8.0;
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), "PERSONAL BEST", (art_x + 10.0) as i32, pb_header_y as i32, 1, ColorRgba::new(255, 210, 80, 255));

            if let Some(best) = score_store.get(selected_song.hash) {
                let (lamp_title, lamp_color) = clear_lamp_color(Some(best.clear_type));
                let (rank_str, rank_color) = accuracy_to_rank(best.accuracy_rate);

                // Lamp badge on PB card
                BitmapFont::draw_badge(
                    &mut self.pixmap.as_mut(),
                    lamp_title,
                    (art_x + art_w - 110.0) as i32,
                    pb_header_y as i32 - 2,
                    1,
                    ColorRgba::new(255, 255, 255, 255),
                    ColorRgba::new(15, 18, 25, 255),
                    lamp_color,
                    6,
                    2,
                );

                let mut row_y = cur_y + 36.0;
                let ex_disp = format!("EX SCORE:   {:>4} pts", best.ex_score);
                BitmapFont::draw_text(&mut self.pixmap.as_mut(), &ex_disp, (art_x + 10.0) as i32, row_y as i32, 1, ColorRgba::new(255, 255, 255, 255));
                row_y += 20.0;

                let acc_disp = format!("ACCURACY:   {:.2}%  [{}]", best.accuracy_rate, rank_str);
                BitmapFont::draw_text(&mut self.pixmap.as_mut(), &acc_disp, (art_x + 10.0) as i32, row_y as i32, 1, rank_color);
                row_y += 20.0;

                let combo_disp = format!("MAX COMBO:  {:>4} / {}", best.max_combo, selected_song.notes_count);
                BitmapFont::draw_text(&mut self.pixmap.as_mut(), &combo_disp, (art_x + 10.0) as i32, row_y as i32, 1, ColorRgba::new(180, 210, 255, 255));
            } else {
                BitmapFont::draw_text_centered(
                    &mut self.pixmap.as_mut(),
                    "NO RECORD REGISTERED",
                    (art_x + art_w / 2.0) as i32,
                    (cur_y + 65.0) as i32,
                    1,
                    ColorRgba::new(100, 110, 135, 255),
                );
            }
        }

        // 4. Bottom Footer Keybindings Guide
        let footer_y = (h - 36.0) as i32;
        self.draw_rect(0.0, footer_y as f32 - 4.0, w, 40.0, ColorRgba::new(12, 14, 22, 255));
        self.draw_rect(0.0, footer_y as f32 - 4.0, w, 1.0, ColorRgba::new(35, 40, 60, 255));

        let match_info = format!("[TOTAL: {}/{}]", total_songs, total_library_count);
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &match_info,
            24,
            footer_y + 4,
            1,
            ColorRgba::new(80, 200, 255, 255),
        );

        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "[Up/Down]: Move  [Enter]: Play  [/]: Search  [F2]: Sort  [F3]: Folder  [Tab]: Options  [F12]: KeyConfig",
            160,
            footer_y + 4,
            1,
            ColorRgba::new(150, 155, 175, 255),
        );
    }


    /// Renders the result summary screen.
    pub fn render_result(
        &mut self,
        chart: &BmsChart,
        score: &ScoreTracker,
        is_new_record: bool,
    ) {
        self.clear();

        let center_x = (self.width() / 2) as i32;
        let mut y = 40;

        // Title
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            "STAGE RESULT",
            center_x,
            y,
            2,
            ColorRgba::new(255, 255, 255, 255),
        );
        y += 40;

        // Cleared / Failed Status
        let (status_text, status_color) = if score.is_cleared() {
            if score.miss_count == 0 && score.poor_count == 0 && score.bad_count == 0 {
                ("FULL COMBO CLEAR!", ColorRgba::new(100, 255, 120, 255))
            } else {
                ("STAGE CLEARED!", ColorRgba::new(60, 220, 255, 255))
            }
        } else {
            ("STAGE FAILED", ColorRgba::new(255, 60, 60, 255))
        };

        BitmapFont::draw_text_centered(&mut self.pixmap.as_mut(), status_text, center_x, y, 3, status_color);
        y += 45;

        // New Record Banner
        if is_new_record {
            BitmapFont::draw_text_centered(
                &mut self.pixmap.as_mut(),
                "*** NEW PERSONAL BEST! ***",
                center_x,
                y,
                1,
                ColorRgba::new(255, 220, 50, 255),
            );
            y += 24;
        }

        // Song Title & Artist
        BitmapFont::draw_text_centered(&mut self.pixmap.as_mut(), &chart.header.title, center_x, y, 2, ColorRgba::new(255, 255, 255, 255));
        y += 24;
        BitmapFont::draw_text_centered(&mut self.pixmap.as_mut(), &chart.header.artist, center_x, y, 1, ColorRgba::new(160, 160, 180, 255));
        y += 35;

        // Statistics Box
        let box_x = center_x - 180;
        self.draw_rect(box_x as f32, y as f32, 360.0, 220.0, ColorRgba::new(16, 16, 24, 255));
        self.draw_rect(box_x as f32, y as f32, 360.0, 1.0, ColorRgba::new(50, 50, 70, 255));
        self.draw_rect(box_x as f32, (y + 220) as f32, 360.0, 1.0, ColorRgba::new(50, 50, 70, 255));
        self.draw_rect(box_x as f32, y as f32, 1.0, 220.0, ColorRgba::new(50, 50, 70, 255));
        self.draw_rect((box_x + 360) as f32, y as f32, 1.0, 220.0, ColorRgba::new(50, 50, 70, 255));

        let stat_x = box_x + 20;
        let mut stat_y = y + 15;

        let ex_str = format!("EX SCORE:   {} / {}", score.ex_score, score.max_ex_score());
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), &ex_str, stat_x, stat_y, 1, ColorRgba::new(255, 230, 80, 255));
        stat_y += 18;

        let acc_str = format!("ACCURACY:   {:.2}%", score.accuracy_rate());
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), &acc_str, stat_x, stat_y, 1, ColorRgba::new(100, 220, 255, 255));
        stat_y += 18;

        let max_c_str = format!("MAX COMBO:  {}", score.max_combo);
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), &max_c_str, stat_x, stat_y, 1, ColorRgba::new(240, 240, 255, 255));
        stat_y += 24;

        // Judge counts
        let counts = [
            ("PGREAT", score.pgreat_count, ColorRgba::new(255, 230, 80, 255)),
            ("GREAT ", score.great_count, ColorRgba::new(255, 170, 50, 255)),
            ("GOOD  ", score.good_count, ColorRgba::new(60, 220, 120, 255)),
            ("BAD   ", score.bad_count, ColorRgba::new(180, 70, 240, 255)),
            ("POOR  ", score.poor_count, ColorRgba::new(240, 50, 50, 255)),
            ("MISS  ", score.miss_count, ColorRgba::new(140, 140, 140, 255)),
        ];

        for (lbl, cnt, clr) in counts {
            let row = format!("{}: {:>4}", lbl, cnt);
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &row, stat_x, stat_y, 1, clr);
            stat_y += 16;
        }

        // Bottom navigation prompt
        let footer_y = (self.height() - 40) as i32;
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            "Press [Enter] or [Escape] to return to Song Select",
            center_x,
            footer_y,
            1,
            ColorRgba::new(160, 160, 180, 255),
        );
    }

    /// Renders the option modal overlay centered on the screen.
    pub fn render_option_modal(
        &mut self,
        options: &beetle_core::PlayOptions,
        key_preset_str: &str,
        is_auto_play: bool,
        start_measure: u32,
        selected_row: usize,
    ) {
        let modal_w = 440.0;
        let modal_h = 330.0;
        let modal_x = (self.width() as f32 - modal_w) / 2.0;
        let modal_y = (self.height() as f32 - modal_h) / 2.0;

        // Background shadow / dim overlay (draw dark background)
        self.draw_rect(modal_x, modal_y, modal_w, modal_h, ColorRgba::new(12, 14, 20, 255));

        // Glowing border
        self.draw_rect(modal_x, modal_y, modal_w, 2.0, ColorRgba::new(80, 140, 255, 255));
        self.draw_rect(modal_x, modal_y + modal_h, modal_w, 2.0, ColorRgba::new(80, 140, 255, 255));
        self.draw_rect(modal_x, modal_y, 2.0, modal_h, ColorRgba::new(80, 140, 255, 255));
        self.draw_rect(modal_x + modal_w, modal_y, 2.0, modal_h, ColorRgba::new(80, 140, 255, 255));

        // Header Title
        let center_x = (self.width() / 2) as i32;
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            "PLAY OPTIONS",
            center_x,
            (modal_y + 16.0) as i32,
            2,
            ColorRgba::new(255, 255, 255, 255),
        );

        let rows = [
            ("HI-SPEED", format!("<  {:.0} px/s  >", options.hi_speed)),
            ("MODIFIER", format!("<  {}  >", options.lane_modifier.as_str())),
            ("GAUGE", format!("<  {}  >", options.gauge_type.as_str())),
            ("JUDGE OFFSET", format!("<  {:+.0} ms  >", options.judge_offset_ms)),
            ("KEY LAYOUT", format!("<  {}  >", key_preset_str)),
            ("AUTO PLAY", if is_auto_play { "<  ON  >".to_string() } else { "<  OFF  >".to_string() }),
            ("START MEASURE", format!("<  M.{}  >", start_measure)),
        ];

        let mut row_y = (modal_y + 55.0) as i32;
        for (i, (label, val)) in rows.iter().enumerate() {
            let is_sel = i == selected_row;
            let (text_color, bg_color) = if is_sel {
                (
                    ColorRgba::new(255, 255, 255, 255),
                    Some(ColorRgba::new(40, 70, 140, 255)),
                )
            } else {
                (ColorRgba::new(160, 170, 190, 255), None)
            };

            if let Some(bg) = bg_color {
                self.draw_rect(modal_x + 16.0, row_y as f32 - 4.0, modal_w - 32.0, 26.0, bg);
            }

            BitmapFont::draw_text(&mut self.pixmap.as_mut(), label, (modal_x + 30.0) as i32, row_y, 1, text_color);
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), val, (modal_x + 230.0) as i32, row_y, 1, if is_sel { ColorRgba::new(255, 230, 80, 255) } else { text_color });

            row_y += 30;
        }

        // Instructions Footer
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            "[Up/Down]: Select   [Left/Right]: Change   [Tab/Esc]: Close",
            center_x,
            (modal_y + modal_h - 25.0) as i32,
            1,
            ColorRgba::new(130, 140, 160, 255),
        );
    }

    /// Renders the in-game pause overlay modal with options (Resume, Restart, Quit).
    pub fn render_pause_modal(
        &mut self,
        title: &str,
        artist: &str,
        current_time_sec: f64,
        total_time_sec: f64,
        selected_option: usize,
    ) {
        let w = self.width() as f32;
        let h = self.height() as f32;

        // 1. Semi-transparent dark overlay dimming the gameplay field
        self.draw_rect(0.0, 0.0, w, h, ColorRgba::new(0, 0, 0, 190));

        // 2. Center Glassmorphic Modal Box
        let modal_w = 420.0f32;
        let modal_h = 300.0f32;
        let modal_x = (w - modal_w) / 2.0;
        let modal_y = (h - modal_h) / 2.0;

        self.draw_rect(modal_x, modal_y, modal_w, modal_h, ColorRgba::new(16, 20, 32, 255));
        self.draw_rect(modal_x, modal_y, modal_w, 1.0, ColorRgba::new(80, 180, 255, 255));
        self.draw_rect(modal_x, modal_y + modal_h - 1.0, modal_w, 1.0, ColorRgba::new(80, 180, 255, 255));
        self.draw_rect(modal_x, modal_y, 1.0, modal_h, ColorRgba::new(80, 180, 255, 255));
        self.draw_rect(modal_x + modal_w - 1.0, modal_y, 1.0, modal_h, ColorRgba::new(80, 180, 255, 255));

        // 3. Pause Header
        let center_x = (w / 2.0) as i32;
        let mut cur_y = modal_y + 20.0;
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            "GAME PAUSED",
            center_x,
            cur_y as i32,
            2,
            ColorRgba::new(255, 220, 80, 255),
        );
        cur_y += 32.0;

        // Song Title & Artist
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            &truncate_str(title, 26),
            center_x,
            cur_y as i32,
            1,
            ColorRgba::new(255, 255, 255, 255),
        );
        cur_y += 18.0;

        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            &truncate_str(artist, 28),
            center_x,
            cur_y as i32,
            1,
            ColorRgba::new(160, 170, 195, 255),
        );
        cur_y += 24.0;

        // Progress Bar
        let bar_w = modal_w - 60.0;
        let bar_x = modal_x + 30.0;
        let bar_h = 6.0;
        let ratio = if total_time_sec > 0.0 {
            (current_time_sec / total_time_sec).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };

        self.draw_rect(bar_x, cur_y, bar_w, bar_h, ColorRgba::new(30, 36, 52, 255));
        self.draw_rect(bar_x, cur_y, bar_w * ratio, bar_h, ColorRgba::new(80, 210, 255, 255));
        cur_y += bar_h + 8.0;

        let time_disp = format!(
            "{:02}:{:02} / {:02}:{:02}",
            (current_time_sec / 60.0) as u32,
            (current_time_sec % 60.0) as u32,
            (total_time_sec / 60.0) as u32,
            (total_time_sec % 60.0) as u32,
        );
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            &time_disp,
            center_x,
            cur_y as i32,
            1,
            ColorRgba::new(140, 150, 175, 255),
        );
        cur_y += 26.0;

        // Menu Options
        let menu_items = [
            ("RESUME", "Continue playing"),
            ("RESTART", "Retry from start (R)"),
            ("SELECT SONG", "Quit to song select (Esc)"),
        ];

        let item_w = modal_w - 40.0;
        let item_x = modal_x + 20.0;
        let item_h = 32.0;

        for (idx, (label, sub)) in menu_items.iter().enumerate() {
            let is_sel = idx == selected_option;
            let item_y = cur_y;

            if is_sel {
                self.draw_rect(item_x, item_y, item_w, item_h, ColorRgba::new(35, 65, 135, 255));
                self.draw_rect(item_x, item_y, item_w, 1.0, ColorRgba::new(90, 190, 255, 255));
                self.draw_rect(item_x, item_y + item_h - 1.0, item_w, 1.0, ColorRgba::new(90, 190, 255, 255));
                self.draw_rect(item_x, item_y, 1.0, item_h, ColorRgba::new(90, 190, 255, 255));
                self.draw_rect(item_x + item_w - 1.0, item_y, 1.0, item_h, ColorRgba::new(90, 190, 255, 255));
            } else {
                self.draw_rect(item_x, item_y, item_w, item_h, ColorRgba::new(20, 25, 38, 200));
                self.draw_rect(item_x, item_y, item_w, 1.0, ColorRgba::new(40, 48, 68, 255));
                self.draw_rect(item_x, item_y + item_h - 1.0, item_w, 1.0, ColorRgba::new(40, 48, 68, 255));
            }

            let text_col = if is_sel { ColorRgba::new(255, 255, 255, 255) } else { ColorRgba::new(180, 190, 215, 255) };
            let sub_col = if is_sel { ColorRgba::new(140, 210, 255, 255) } else { ColorRgba::new(100, 110, 135, 255) };

            BitmapFont::draw_text(&mut self.pixmap.as_mut(), label, (item_x + 16.0) as i32, (item_y + 8.0) as i32, 1, text_col);
            let sub_x = (item_x + item_w - BitmapFont::text_width(sub, 1) as f32 - 16.0) as i32;
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), sub, sub_x, (item_y + 8.0) as i32, 1, sub_col);

            cur_y += item_h + 8.0;
        }
    }

    /// Renders the interactive key configuration screen.
    pub fn render_key_config(
        &mut self,
        key_names: &[(&'static str, &'static str)],
        selected_lane_idx: usize,
    ) {
        self.clear();

        let center_x = (self.width() / 2) as i32;
        let mut y = 40;

        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            "KEY CONFIGURATION",
            center_x,
            y,
            2,
            ColorRgba::new(255, 255, 255, 255),
        );
        y += 30;

        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            "Preset Layout Mapping for 7K + 1S",
            center_x,
            y,
            1,
            ColorRgba::new(160, 160, 180, 255),
        );
        y += 40;

        let box_w = 400.0;
        let box_x = (self.width() as f32 - box_w) / 2.0;

        for (i, (lane_name, key_name)) in key_names.iter().enumerate() {
            let is_sel = i == selected_lane_idx;
            let (text_color, bg_color) = if is_sel {
                (
                    ColorRgba::new(255, 255, 255, 255),
                    Some(ColorRgba::new(40, 70, 140, 255)),
                )
            } else {
                (ColorRgba::new(180, 180, 200, 255), None)
            };

            if let Some(bg) = bg_color {
                self.draw_rect(box_x, y as f32 - 4.0, box_w, 28.0, bg);
            }

            BitmapFont::draw_text(&mut self.pixmap.as_mut(), lane_name, (box_x + 20.0) as i32, y, 1, text_color);
            let val_str = format!("[ {} ]", key_name);
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &val_str,
                (box_x + 200.0) as i32,
                y,
                1,
                if is_sel { ColorRgba::new(255, 220, 80, 255) } else { text_color },
            );

            y += 34;
        }

        let footer_y = (self.height() - 40) as i32;
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            "[Up/Down]: Select Lane   [F1]: Toggle Default Preset   [Enter/Esc]: Return",
            center_x,
            footer_y,
            1,
            ColorRgba::new(140, 140, 160, 255),
        );
    }

    /// Renders the transition loading screen while soundbanks and charts are decoded in the background.
    pub fn render_loading_screen(
        &mut self,
        title: &str,
        artist: &str,
        genre: &str,
        stage_image: Option<&ImageBuffer>,
        spinner_frame: usize,
        progress_msg: &str,
    ) {
        self.clear();

        let w = self.width() as f32;
        let _h = self.height() as f32;
        let center_x = (w / 2.0) as i32;

        // Background subtle grid/lines
        for line_y in (0..self.height()).step_by(24) {
            self.draw_rect(0.0, line_y as f32, w, 1.0, ColorRgba::new(20, 20, 30, 255));
        }

        let mut y = 60.0;

        // Top Header
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            "PREPARING TRACK",
            center_x,
            y as i32,
            1,
            ColorRgba::new(120, 160, 220, 255),
        );
        y += 36.0;

        // Stage Image / Artwork Box
        let art_w = (w * 0.45).clamp(240.0, 480.0);
        let art_h = art_w * (9.0 / 16.0);
        let art_x = (w - art_w) / 2.0;

        self.draw_rect(art_x - 2.0, y - 2.0, art_w + 4.0, art_h + 4.0, ColorRgba::new(50, 60, 90, 255));
        self.draw_rect(art_x, y, art_w, art_h, ColorRgba::new(12, 12, 18, 255));

        if let Some(img) = stage_image {
            img.draw_scaled(&mut self.pixmap, art_x as i32, y as i32, art_w as u32, art_h as u32);
        } else {
            BitmapFont::draw_text_centered(
                &mut self.pixmap.as_mut(),
                "[ NO STAGE IMAGE ]",
                center_x,
                (y + art_h / 2.0 - 4.0) as i32,
                1,
                ColorRgba::new(70, 70, 90, 255),
            );
        }
        y += art_h + 24.0;

        // Track Title
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            title,
            center_x,
            y as i32,
            2,
            ColorRgba::new(255, 255, 255, 255),
        );
        y += 32.0;

        // Artist & Genre
        let sub_info = if !genre.is_empty() {
            format!("{} / {}", artist, genre)
        } else {
            artist.to_string()
        };
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            &sub_info,
            center_x,
            y as i32,
            1,
            ColorRgba::new(170, 180, 210, 255),
        );
        y += 44.0;

        // Animated Loading Progress Indicator
        let bar_w = 320.0;
        let bar_x = (w - bar_w) / 2.0;
        self.draw_rect(bar_x, y, bar_w, 4.0, ColorRgba::new(30, 30, 45, 255));

        // Pulsing / moving progress highlight
        let pulse_pos = ((spinner_frame * 12) % (bar_w as usize)) as f32;
        let seg_w = 60.0f32;
        let seg_x = (bar_x + pulse_pos).min(bar_x + bar_w - seg_w);
        self.draw_rect(seg_x, y, seg_w, 4.0, ColorRgba::new(80, 200, 255, 255));
        y += 18.0;

        let spinner_chars = ['|', '/', '-', '\\'];
        let spinner = spinner_chars[spinner_frame % 4];
        let loading_disp = format!("[{}] {}", spinner, progress_msg);
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            &loading_disp,
            center_x,
            y as i32,
            1,
            ColorRgba::new(255, 220, 90, 255),
        );
    }
}

fn level_color(level: u32) -> ColorRgba {
    match level {
        1..=4 => ColorRgba::new(80, 220, 130, 255),  // Mint Green (Normal)
        5..=8 => ColorRgba::new(60, 180, 255, 255),  // Cyan (Hyper)
        9..=10 => ColorRgba::new(255, 200, 50, 255), // Amber/Yellow (Another)
        11..=12 => ColorRgba::new(255, 70, 70, 255), // Crimson Red (Insane)
        _ => ColorRgba::new(210, 90, 255, 255),      // Purple / Overjoy
    }
}

fn clear_lamp_color(clear_type: Option<beetle_core::ClearType>) -> (&'static str, ColorRgba) {
    match clear_type {
        Some(beetle_core::ClearType::Perfect) => ("PERFECT", ColorRgba::new(255, 230, 80, 255)),
        Some(beetle_core::ClearType::FullCombo) => ("FULL COMBO", ColorRgba::new(80, 255, 140, 255)),
        Some(beetle_core::ClearType::Clear) => ("CLEARED", ColorRgba::new(60, 190, 255, 255)),
        Some(beetle_core::ClearType::Failed) => ("FAILED", ColorRgba::new(240, 60, 60, 255)),
        None => ("NO PLAY", ColorRgba::new(70, 75, 95, 255)),
    }
}

fn accuracy_to_rank(acc: f64) -> (&'static str, ColorRgba) {
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

fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        truncated.push_str("...");
        truncated
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
}

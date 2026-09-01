use crate::bitmap_font::BitmapFont;
use crate::image::ImageBuffer;
use crate::renderer::{lane_index, SoftwareRenderer};
use crate::skin::ColorRgba;
use beetle_core::{BmsChart, GaugeType, JudgeGrade, NoteType, ScoreTracker, TimingModel};

impl SoftwareRenderer {
    /// Renders a single gameplay frame based on current audio time.
    pub fn render_gameplay(
        &mut self,
        chart: &BmsChart,
        notes: &[beetle_core::PlayNote],
        audio_time_seconds: f64,
        score: &ScoreTracker,
        visual_levels: &[f32; 16],
        bga_image: Option<&ImageBuffer>,
        layer_image: Option<&ImageBuffer>,
        track_bga_opacity: f32,
        timing: &TimingModel,
    ) {
        self.clear();

        let is_danger = (score.gauge < 30.0 && matches!(score.gauge_type, GaugeType::Hard | GaugeType::Groove))
            || (score.gauge_type == GaugeType::Hazard && score.gauge < 100.0);
        let danger_blink = is_danger && ((audio_time_seconds * 6.0).sin() > 0.0);

        self.draw_playfield_bg(score.current_combo, danger_blink, bga_image, layer_image, track_bga_opacity);
        self.draw_key_beams();
        self.draw_measure_lines(audio_time_seconds, timing, chart);
        self.draw_notes(notes, audio_time_seconds);
        self.draw_lane_cover();
        self.draw_judge_line(score.current_combo);
        self.draw_hit_bursts(audio_time_seconds);
        self.draw_gauge_bar(score, danger_blink);
        self.draw_combo_and_judge(score, audio_time_seconds);
        self.draw_hud_info(chart, score);
        self.draw_bga_and_visualizer(visual_levels, bga_image, layer_image);
    }

    fn draw_playfield_bg(
        &mut self,
        combo: u32,
        danger_blink: bool,
        bga_image: Option<&ImageBuffer>,
        layer_image: Option<&ImageBuffer>,
        track_bga_opacity: f32,
    ) {
        let s = self.viewport.scale;

        // Draw playfield main background box
        self.draw_rect(
            self.skin.playfield_x,
            self.skin.playfield_y,
            self.skin.playfield_width,
            self.skin.playfield_height,
            self.skin.playfield_bg_color,
        );

        // Draw playfield track BGA underlay if enabled
        if track_bga_opacity > 0.0 {
            if let Some(img) = bga_image {
                img.draw_fitted_with_opacity(
                    &mut self.pixmap,
                    self.skin.playfield_x as i32,
                    self.skin.playfield_y as i32,
                    self.skin.playfield_width as u32,
                    self.skin.playfield_height as u32,
                    crate::image::ImageFitMode::FillCrop,
                    track_bga_opacity,
                );
            }
            if let Some(layer) = layer_image {
                layer.draw_color_keyed_with_opacity(
                    &mut self.pixmap,
                    self.skin.playfield_x as i32,
                    self.skin.playfield_y as i32,
                    self.skin.playfield_width as u32,
                    self.skin.playfield_height as u32,
                    track_bga_opacity,
                );
            }
        }

        let line_color = if combo >= 100 {
            ColorRgba::new(50, 100, 180, 220) // Subtle ambient blue for active combo
        } else {
            self.skin.lane_line_color
        };

        let line_w = (1.0 * s).max(1.0);

        // Draw lane vertical separator lines
        for &lane in self.skin.active_lanes() {
            let x = self.skin.lane_x(lane);
            self.draw_rect(
                x,
                self.skin.playfield_y,
                line_w,
                self.skin.playfield_height,
                line_color,
            );
        }

        // Right boundary line
        let right_x = self.skin.playfield_x + self.skin.playfield_width;
        self.draw_rect(
            right_x,
            self.skin.playfield_y,
            line_w,
            self.skin.playfield_height,
            line_color,
        );

        // Danger pulsing border around entire playfield
        if danger_blink {
            let px = self.skin.playfield_x;
            let py = self.skin.playfield_y;
            let pw = self.skin.playfield_width;
            let ph = self.skin.playfield_height;
            let border_thickness = (2.0 * s).max(2.0);
            let danger_col = ColorRgba::new(255, 40, 40, 220);
            self.draw_rect(px, py, pw, border_thickness, danger_col);
            self.draw_rect(px, py + ph - border_thickness, pw, border_thickness, danger_col);
            self.draw_rect(px, py, border_thickness, ph, danger_col);
            self.draw_rect(px + pw - border_thickness, py, border_thickness, ph, danger_col);
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

    fn draw_measure_lines(
        &mut self,
        audio_time_seconds: f64,
        timing: &TimingModel,
        chart: &BmsChart,
    ) {
        let s = self.viewport.scale;
        let effective_speed = self.skin.hi_speed * s;
        let judge_y = self.skin.judge_line_y;
        let top_y = self.skin.playfield_y;
        let px = self.skin.playfield_x;
        let pw = self.skin.playfield_width;
        let line_h = (1.0 * s).max(1.0);
        let line_color = ColorRgba::new(255, 255, 255, 50);

        // Determine the visible time window
        let visible_duration = (judge_y - top_y + 50.0 * s) as f64 / effective_speed.max(1.0) as f64;
        let max_time = audio_time_seconds + visible_duration;

        // Find the highest measure number in the chart (from notes and bgm_notes)
        let max_measure = {
            let note_max = chart.notes.iter().map(|n| n.measure).max().unwrap_or(0);
            let bgm_max = chart.bgm_notes.iter().map(|b| b.0).max().unwrap_or(0);
            note_max.max(bgm_max) + 2 // +2 to cover trailing measures
        };

        // Find starting measure via binary-style scan (avoid iterating from 0 every frame)
        // We use audio_time_seconds to skip past measures that are already below judge line
        let start_measure = {
            let (m, _) = timing.time_to_beat(audio_time_seconds - 0.5);
            if m > 0 { m } else { 0 }
        };

        for measure in start_measure..=max_measure {
            let measure_time = timing.beat_to_time_seconds(measure, 0.0);

            // Skip measures that have already passed below judge line
            if measure_time < audio_time_seconds - 1.0 {
                continue;
            }

            // Stop once we're past the visible area
            if measure_time > max_time {
                break;
            }

            let delta_t = measure_time - audio_time_seconds;
            let bar_y = judge_y - (delta_t as f32 * effective_speed);

            // Only draw within the playfield vertical bounds
            if bar_y >= top_y && bar_y <= judge_y {
                self.draw_rect(px, bar_y - line_h * 0.5, pw, line_h, line_color);
            }
        }
    }

    fn draw_judge_line(&mut self, combo: u32) {
        let s = self.viewport.scale;
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
        self.draw_rect(px, jy - 3.0 * s, pw, 7.0 * s, glow_color);

        // Core bright judge line
        let line_h = (2.0 * s).max(2.0);
        self.draw_rect(
            px,
            jy,
            pw,
            line_h,
            self.skin.judge_line_color,
        );
    }

    fn draw_notes(&mut self, notes: &[beetle_core::PlayNote], audio_time_seconds: f64) {
        let s = self.viewport.scale;
        let effective_speed = self.skin.hi_speed * s;
        let judge_y = self.skin.judge_line_y;
        let top_y = self.skin.playfield_y;
        let note_h = self.skin.note_height;

        let visible_duration = (judge_y - top_y + 100.0 * s) as f64 / effective_speed.max(1.0) as f64;
        let min_time = audio_time_seconds - 2.0;
        let max_time = audio_time_seconds + visible_duration;

        let start_idx = notes.partition_point(|n| n.end_target_time_seconds < min_time);

        for note in &notes[start_idx..] {
            if note.target_time_seconds > max_time {
                break;
            }

            let delta_t = note.target_time_seconds - audio_time_seconds;
            let note_y = judge_y - (delta_t as f32 * effective_speed);

            let lane = note.note_event.lane;
            let lane_x = self.skin.lane_x(lane) + 1.0;
            let lane_w = self.skin.lane_width(lane) - 2.0;
            let note_color = self.skin.lane_color(lane);

            match note.note_event.note_type {
                NoteType::Tap => {
                    // Only draw if within visible playfield vertical range
                    if note_y + note_h >= top_y && note_y - note_h <= judge_y + 40.0 * s {
                        self.draw_rect(lane_x, note_y - note_h, lane_w, note_h, note_color);
                    }
                }
                NoteType::LongNoteStart => {
                    let end_delta = note.end_target_time_seconds - audio_time_seconds;
                    let end_y = judge_y - (end_delta as f32 * effective_speed);

                    let body_top = end_y.max(top_y);
                    let body_bottom = note_y.min(judge_y);

                    // Draw LN body
                    if body_bottom > body_top {
                        let body_color = note_color.with_alpha(140);
                        self.draw_rect(
                            lane_x + 3.0 * s,
                            body_top,
                            lane_w - 6.0 * s,
                            body_bottom - body_top,
                            body_color,
                        );
                    }

                    // Draw start head
                    if note_y + note_h >= top_y && note_y <= judge_y + 40.0 * s {
                        self.draw_rect(lane_x, note_y - note_h, lane_w, note_h, note_color);
                    }

                    // Draw end tail
                    if end_y + note_h >= top_y && end_y <= judge_y + 40.0 * s {
                        self.draw_rect(lane_x, end_y - note_h, lane_w, note_h, note_color);
                    }
                }
                _ => (),
            }
        }
    }

    fn draw_gauge_bar(&mut self, score: &ScoreTracker, danger_blink: bool) {
        let s = self.viewport.scale;
        let gauge_x = self.skin.playfield_x + self.skin.playfield_width + 16.0 * s;
        let gauge_y = self.skin.playfield_y;
        let gauge_w = 22.0 * s;
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
        let b_line = (1.0 * s).max(1.0);
        self.draw_rect(gauge_x, gauge_y, gauge_w, b_line, border_color);
        self.draw_rect(gauge_x, gauge_y + gauge_h - b_line, gauge_w, b_line, border_color);
        self.draw_rect(gauge_x, gauge_y, b_line, gauge_h, border_color);
        self.draw_rect(gauge_x + gauge_w - b_line, gauge_y, b_line, gauge_h, border_color);

        // Gauge 80% threshold line for Easy / Groove gauge
        if matches!(score.gauge_type, GaugeType::Easy | GaugeType::Groove) {
            let line_y = gauge_y + gauge_h * 0.2;
            self.draw_rect(gauge_x - 3.0 * s, line_y, gauge_w + 6.0 * s, 2.0 * s, ColorRgba::new(255, 220, 50, 255));
        }

        // Percentage text below gauge
        let gauge_str = format!("{:.1}%", score.gauge);
        let text_color = if danger_blink {
            ColorRgba::new(255, 80, 80, 255)
        } else {
            ColorRgba::new(220, 220, 240, 255)
        };
        let font_scale = (s * 0.9).round().max(1.0) as u32;
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &gauge_str,
            (gauge_x - 4.0 * s) as i32,
            (gauge_y + gauge_h + 8.0 * s) as i32,
            font_scale,
            text_color,
        );
    }

    fn draw_hit_bursts(&mut self, audio_time_seconds: f64) {
        let s = self.viewport.scale;
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
            let flare_size = ((1.0 - progress) * 26.0 + 4.0) * s;
            self.draw_rect(
                lx - flare_size / 2.0,
                judge_y - flare_size / 2.0,
                flare_size,
                flare_size,
                ColorRgba::new(r, g, b, alpha),
            );

            // 3. Radiating particle sparks
            let dist = progress * 32.0 * s;
            let spark_size = ((1.0 - progress) * 4.0 + 1.0) * s;
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
        let s = self.viewport.scale;
        let center_x = (self.skin.playfield_x + (self.skin.playfield_width / 2.0)) as i32;
        let judge_center_y = (self.skin.judge_line_y - 120.0 * s) as i32;
        let font_scale = (s * 0.9).round().max(1.0) as u32;

        // 1. Draw Combo with bounce pulse
        if score.current_combo > 0 {
            let combo_num = format!("{}", score.current_combo);
            let pulse_offset = if let Some((_, judge_time, _)) = self.last_judge {
                let elapsed = audio_time_seconds - judge_time;
                if elapsed >= 0.0 && elapsed < 0.12 {
                    (((1.0 - (elapsed / 0.12)) * 6.0) * s as f64) as i32
                } else {
                    0
                }
            } else {
                0
            };

            let combo_font_scale = (3.0 * s).round().max(2.0) as u32;
            let combo_y = judge_center_y - (34.0 * s) as i32 - pulse_offset;

            BitmapFont::draw_text_centered(
                &mut self.pixmap.as_mut(),
                &combo_num,
                center_x,
                combo_y,
                combo_font_scale,
                ColorRgba::new(255, 255, 255, 255),
            );

            BitmapFont::draw_text_centered(
                &mut self.pixmap.as_mut(),
                "COMBO",
                center_x,
                combo_y + (24.0 * s) as i32,
                font_scale,
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

                let judge_font_scale = (2.0 * s).round().max(1.0) as u32;
                BitmapFont::draw_text_centered(
                    &mut self.pixmap.as_mut(),
                    text,
                    center_x,
                    judge_center_y + (8.0 * s) as i32,
                    judge_font_scale,
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
                        judge_center_y + (28.0 * s) as i32,
                        font_scale,
                        fs_color,
                    );
                }
            }
        }
    }

    fn draw_lane_cover(&mut self) {
        if self.skin.lane_cover_ratio > 0.0 {
            let s = self.viewport.scale;
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
                self.skin.playfield_y + cover_h - 2.0 * s,
                self.skin.playfield_width,
                2.0 * s,
                ColorRgba::new(80, 140, 255, 255),
            );
        }
    }

    fn draw_hud_info(&mut self, chart: &BmsChart, score: &ScoreTracker) {
        let s = self.viewport.scale;
        let hud_x = (self.skin.playfield_x + self.skin.playfield_width + 48.0 * s) as i32;
        let mut hud_y = self.skin.playfield_y as i32;
        let title_scale = (2.0 * s).round().max(1.0) as u32;
        let font_scale = (s * 0.9).round().max(1.0) as u32;

        // Title & Artist
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &chart.header.title,
            hud_x,
            hud_y,
            title_scale,
            ColorRgba::new(255, 255, 255, 255),
        );
        hud_y += (22.0 * s) as i32;

        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &chart.header.artist,
            hud_x,
            hud_y,
            font_scale,
            ColorRgba::new(160, 160, 180, 255),
        );
        hud_y += (28.0 * s) as i32;

        // BPM & Play Level
        let bpm_str = format!("BPM: {:.1}", chart.header.bpm);
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &bpm_str,
            hud_x,
            hud_y,
            font_scale,
            ColorRgba::new(200, 200, 220, 255),
        );
        hud_y += (16.0 * s) as i32;

        let lvl_str = format!("LEVEL: {}", chart.header.play_level);
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &lvl_str,
            hud_x,
            hud_y,
            font_scale,
            ColorRgba::new(200, 200, 220, 255),
        );
        hud_y += (26.0 * s) as i32;

        // EX-Score and Accuracy Rate
        let ex_str = format!("EX SCORE: {} / {}", score.ex_score, score.max_ex_score());
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &ex_str,
            hud_x,
            hud_y,
            font_scale,
            ColorRgba::new(255, 230, 100, 255),
        );
        hud_y += (16.0 * s) as i32;

        let acc_str = format!("ACCURACY: {:.2}%", score.accuracy_rate());
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &acc_str,
            hud_x,
            hud_y,
            font_scale,
            ColorRgba::new(100, 220, 255, 255),
        );
        hud_y += (18.0 * s) as i32;

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
            font_scale,
            pace_color,
        );
        hud_y += (22.0 * s) as i32;

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
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &row, hud_x, hud_y, font_scale, color);
            hud_y += (14.0 * s) as i32;
        }
    }

    fn draw_bga_and_visualizer(
        &mut self,
        levels: &[f32; 16],
        bga_image: Option<&ImageBuffer>,
        layer_image: Option<&ImageBuffer>,
    ) {
        let s = self.viewport.scale;
        let side_x = self.skin.playfield_x + self.skin.playfield_width + 48.0 * s;
        let bga_y = self.skin.playfield_y + 240.0 * s;
        let max_w = (self.viewport.x + self.viewport.width - side_x - 24.0 * s).max(100.0);
        let bga_w = (520.0 * s).min(max_w);
        let bga_h = (bga_w * 9.0 / 16.0).round();

        // BGA frame
        self.draw_rect(side_x - 2.0 * s, bga_y - 2.0 * s, bga_w + 4.0 * s, bga_h + 4.0 * s, ColorRgba::new(50, 60, 80, 255));
        self.draw_rect(side_x, bga_y, bga_w, bga_h, ColorRgba::new(8, 8, 12, 255));

        if let Some(img) = bga_image {
            img.draw_fitted(&mut self.pixmap, side_x as i32, bga_y as i32, bga_w as u32, bga_h as u32, crate::image::ImageFitMode::FillCrop);
        } else {
            let font_scale = (s * 0.9).round().max(1.0) as u32;
            BitmapFont::draw_text_centered(
                &mut self.pixmap.as_mut(),
                "[ BGA / STAGE IMAGE ]",
                (side_x + bga_w / 2.0) as i32,
                (bga_y + bga_h / 2.0 - 4.0 * s) as i32,
                font_scale,
                ColorRgba::new(80, 90, 110, 255),
            );
        }

        // Draw Layer BGA (Channel 07) overlay with color-key transparency
        if let Some(layer) = layer_image {
            layer.draw_color_keyed(&mut self.pixmap, side_x as i32, bga_y as i32, bga_w as u32, bga_h as u32);
        }

        let vis_y = bga_y + bga_h + 16.0 * s;
        let vis_h = 95.0 * s;
        let b_line = (1.0 * s).max(1.0);

        // Visualizer background
        self.draw_rect(side_x, vis_y, bga_w, vis_h, ColorRgba::new(12, 14, 20, 255));
        self.draw_rect(side_x, vis_y, bga_w, b_line, ColorRgba::new(60, 70, 90, 255));
        self.draw_rect(side_x, vis_y + vis_h - b_line, bga_w, b_line, ColorRgba::new(60, 70, 90, 255));
        self.draw_rect(side_x, vis_y, b_line, vis_h, ColorRgba::new(60, 70, 90, 255));
        self.draw_rect(side_x + bga_w - b_line, vis_y, b_line, vis_h, ColorRgba::new(60, 70, 90, 255));

        let font_scale = (s * 0.9).round().max(1.0) as u32;
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "SPECTRUM VISUALIZER",
            (side_x + 12.0 * s) as i32,
            (vis_y + 8.0 * s) as i32,
            font_scale,
            ColorRgba::new(140, 150, 180, 255),
        );

        let bar_count = 16;
        let padding = 12.0 * s;
        let usable_w = bga_w - (padding * 2.0);
        let bar_w = ((usable_w / bar_count as f32) - 4.0 * s).max(1.0);
        let max_h = 60.0 * s;
        let base_y = vis_y + vis_h - 10.0 * s;

        for (i, &lvl) in levels.iter().enumerate() {
            let clamped_lvl = lvl.clamp(0.0, 1.0);
            let h = (clamped_lvl * max_h).max(2.0 * s);
            let x = side_x + padding + (i as f32 * (bar_w + 4.0 * s));
            let y = base_y - h;

            // Dynamic frequency-based color gradient
            let r = ((60.0 + i as f32 * 10.0 + clamped_lvl * 50.0).min(255.0)) as u8;
            let g = ((160.0 - i as f32 * 4.0 + clamped_lvl * 40.0).clamp(40.0, 255.0)) as u8;
            let b = (255.0 - clamped_lvl * 40.0) as u8;

            self.draw_rect(x, y, bar_w, h, ColorRgba::new(r, g, b, 255));
        }
    }
}

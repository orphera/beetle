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
    last_judge: Option<(JudgeGrade, f64, f64)>, // (Grade, time_seconds, delta_ms)
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

    pub fn trigger_judge(&mut self, grade: JudgeGrade, time_seconds: f64, delta_ms: f64) {
        self.last_judge = Some((grade, time_seconds, delta_ms));
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
        self.draw_lane_cover();
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
            GaugeType::Hazard => {
                ColorRgba::new(240, 40, 80, 255) // Crimson Hazard
            }
        };

        self.draw_rect(gauge_x, fill_y, gauge_w, fill_h, fill_color);

        // Border
        self.draw_rect(gauge_x, gauge_y, gauge_w, 1.0, ColorRgba::new(80, 80, 100, 255));
        self.draw_rect(gauge_x, gauge_y + gauge_h, gauge_w, 1.0, ColorRgba::new(80, 80, 100, 255));
        self.draw_rect(gauge_x, gauge_y, 1.0, gauge_h, ColorRgba::new(80, 80, 100, 255));
        self.draw_rect(gauge_x + gauge_w, gauge_y, 1.0, gauge_h, ColorRgba::new(80, 80, 100, 255));

        // Gauge 80% threshold line for Easy / Groove gauge
        if matches!(score.gauge_type, GaugeType::Easy | GaugeType::Groove) {
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
    ) {
        self.clear();

        // Top Header
        let header_str = format!("BEETLE BMS PLAYER - SONG SELECT  [SORT: {} (F2)]", sort_mode_str);
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &header_str,
            40,
            30,
            2,
            ColorRgba::new(255, 255, 255, 255),
        );

        let total_songs = songs.len();
        if total_songs == 0 {
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                "No BMS songs found in songs/ directory.",
                40,
                100,
                1,
                ColorRgba::new(200, 200, 220, 255),
            );
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                "Place .bms / .bme files into songs/ or run with: cargo run -p beetle-app -- <path.bms>",
                40,
                125,
                1,
                ColorRgba::new(140, 140, 160, 255),
            );
            return;
        }

        // Left Song List Panel
        let list_x = 40;
        let mut list_y = 80;
        let max_visible = 14;
        let start_idx = if selected_idx >= max_visible / 2 {
            (selected_idx + 1).saturating_sub(max_visible / 2).min(total_songs.saturating_sub(max_visible))
        } else {
            0
        };
        let end_idx = (start_idx + max_visible).min(total_songs);

        for i in start_idx..end_idx {
            let song = &songs[i];
            let is_selected = i == selected_idx;

            let (prefix, text_color, bg_color) = if is_selected {
                (
                    "> ",
                    ColorRgba::new(255, 255, 255, 255),
                    Some(ColorRgba::new(40, 60, 120, 255)),
                )
            } else {
                ("  ", ColorRgba::new(160, 160, 180, 255), None)
            };

            if let Some(bg) = bg_color {
                self.draw_rect(list_x as f32 - 8.0, list_y as f32 - 4.0, 420.0, 24.0, bg);
            }

            let line = format!("{}{:<28} [LV.{:>2}]", prefix, truncate_str(&song.title, 26), song.play_level);
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &line, list_x, list_y, 1, text_color);
            list_y += 26;
        }

        // Right Detail Panel
        let detail_x = 500;
        let mut detail_y = 80;

        self.draw_rect(detail_x as f32 - 10.0, 70.0, 260.0, 400.0, ColorRgba::new(18, 18, 26, 255));
        self.draw_rect(detail_x as f32 - 10.0, 70.0, 260.0, 1.0, ColorRgba::new(60, 60, 80, 255));
        self.draw_rect(detail_x as f32 - 10.0, 470.0, 260.0, 1.0, ColorRgba::new(60, 60, 80, 255));
        self.draw_rect(detail_x as f32 - 10.0, 70.0, 1.0, 400.0, ColorRgba::new(60, 60, 80, 255));
        self.draw_rect(detail_x as f32 + 250.0, 70.0, 1.0, 400.0, ColorRgba::new(60, 60, 80, 255));

        if let Some(selected_song) = songs.get(selected_idx) {
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), "CHART DETAILS", detail_x, detail_y, 1, ColorRgba::new(100, 200, 255, 255));
            detail_y += 24;

            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &truncate_str(&selected_song.title, 20), detail_x, detail_y, 1, ColorRgba::new(255, 255, 255, 255));
            detail_y += 18;

            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &truncate_str(&selected_song.artist, 20), detail_x, detail_y, 1, ColorRgba::new(160, 160, 180, 255));
            detail_y += 26;

            let bpm_str = format!("BPM:   {:.1}", selected_song.bpm);
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &bpm_str, detail_x, detail_y, 1, ColorRgba::new(200, 200, 220, 255));
            detail_y += 18;

            let lvl_str = format!("LEVEL: {}", selected_song.play_level);
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &lvl_str, detail_x, detail_y, 1, ColorRgba::new(200, 200, 220, 255));
            detail_y += 18;

            let notes_str = format!("NOTES: {}", selected_song.notes_count);
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &notes_str, detail_x, detail_y, 1, ColorRgba::new(200, 200, 220, 255));
            detail_y += 32;

            // Personal Best Record
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), "PERSONAL BEST", detail_x, detail_y, 1, ColorRgba::new(255, 220, 80, 255));
            detail_y += 20;

            if let Some(best) = score_store.get(selected_song.hash) {
                let lamp_color = match best.clear_type {
                    beetle_core::ClearType::Perfect => ColorRgba::new(255, 230, 80, 255),
                    beetle_core::ClearType::FullCombo => ColorRgba::new(100, 255, 120, 255),
                    beetle_core::ClearType::Clear => ColorRgba::new(60, 180, 255, 255),
                    beetle_core::ClearType::Failed => ColorRgba::new(220, 60, 60, 255),
                };

                let lamp_str = format!("STATUS: {}", best.clear_type.as_str());
                BitmapFont::draw_text(&mut self.pixmap.as_mut(), &lamp_str, detail_x, detail_y, 1, lamp_color);
                detail_y += 18;

                let score_str = format!("EX:     {}", best.ex_score);
                BitmapFont::draw_text(&mut self.pixmap.as_mut(), &score_str, detail_x, detail_y, 1, ColorRgba::new(255, 255, 255, 255));
                detail_y += 18;

                let acc_str = format!("ACC:    {:.2}%", best.accuracy_rate);
                BitmapFont::draw_text(&mut self.pixmap.as_mut(), &acc_str, detail_x, detail_y, 1, ColorRgba::new(100, 220, 255, 255));
                detail_y += 18;

                let combo_str = format!("COMBO:  {}", best.max_combo);
                BitmapFont::draw_text(&mut self.pixmap.as_mut(), &combo_str, detail_x, detail_y, 1, ColorRgba::new(220, 220, 240, 255));
            } else {
                BitmapFont::draw_text(&mut self.pixmap.as_mut(), "NO RECORD", detail_x, detail_y, 1, ColorRgba::new(120, 120, 140, 255));
            }
        }

        // Bottom Footer Bar
        let footer_y = (self.height() - 40) as i32;
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "[Up/Down or J/K]: Select  [Enter/Space]: Play  [F5]: Rescan  [F1/Tab]: Key Layout",
            40,
            footer_y,
            1,
            ColorRgba::new(140, 140, 160, 255),
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
        selected_row: usize,
    ) {
        let modal_w = 440.0;
        let modal_h = 280.0;
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
        renderer.render_gameplay(&chart, &timing, 1.0, &score);

        // Validate buffer is not all blank
        let has_content = renderer.data().chunks_exact(4).any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0);
        assert!(has_content);
    }
}

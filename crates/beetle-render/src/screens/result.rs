use crate::bitmap_font::BitmapFont;
use crate::renderer::{truncate_str, SoftwareRenderer};
use crate::skin::ColorRgba;
use beetle_core::{BmsChart, ScoreTracker};

impl SoftwareRenderer {
    /// Renders the rich Stage Result screen with rank emblem, stats comparison, timing histogram, and badges.
    pub fn render_result(
        &mut self,
        chart: &BmsChart,
        score: &ScoreTracker,
        is_new_record: bool,
        previous_best: Option<&beetle_core::ScoreRecord>,
    ) {
        self.clear();

        let w = self.width() as f32;
        let h = self.height() as f32;

        // 1. Top Header Bar
        self.draw_rect(0.0, 0.0, w, 48.0, ColorRgba::new(14, 18, 28, 255));
        self.draw_rect(0.0, 47.0, w, 1.0, ColorRgba::new(40, 55, 85, 255));

        BitmapFont::draw_badge(
            &mut self.pixmap.as_mut(),
            "STAGE RESULT",
            24,
            12,
            1,
            ColorRgba::new(80, 200, 255, 255),
            ColorRgba::new(20, 35, 65, 255),
            ColorRgba::new(50, 120, 220, 255),
            10,
            4,
        );

        let title_str = truncate_str(&chart.header.title, 32);
        let artist_str = truncate_str(&chart.header.artist, 28);
        let right_title_x = (w - BitmapFont::text_width(&title_str, 1) as f32 - 24.0) as i32;
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), &title_str, right_title_x, 10, 1, ColorRgba::new(255, 255, 255, 255));
        let right_artist_x = (w - BitmapFont::text_width(&artist_str, 1) as f32 - 24.0) as i32;
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), &artist_str, right_artist_x, 26, 1, ColorRgba::new(140, 150, 175, 255));

        // 2. Stage Status Banner & New Record Banner
        let banner_y = 65.0;
        let (status_text, status_color, status_bg) = if score.is_cleared() {
            if score.miss_count == 0 && score.poor_count == 0 && score.bad_count == 0 {
                if score.great_count == 0 && score.good_count == 0 {
                    ("PERFECT CLEAR!", ColorRgba::new(255, 220, 50, 255), ColorRgba::new(60, 50, 10, 255))
                } else {
                    ("FULL COMBO CLEAR!", ColorRgba::new(60, 255, 140, 255), ColorRgba::new(10, 50, 25, 255))
                }
            } else {
                ("STAGE CLEARED!", ColorRgba::new(60, 220, 255, 255), ColorRgba::new(12, 40, 65, 255))
            }
        } else {
            ("STAGE FAILED", ColorRgba::new(255, 70, 70, 255), ColorRgba::new(60, 15, 15, 255))
        };

        let banner_w = 400.0;
        let banner_x = (w - banner_w) / 2.0;
        self.draw_rect(banner_x, banner_y, banner_w, 36.0, status_bg);
        self.draw_rect(banner_x, banner_y, banner_w, 1.0, status_color);
        self.draw_rect(banner_x, banner_y + 35.0, banner_w, 1.0, status_color);
        BitmapFont::draw_text_centered(&mut self.pixmap.as_mut(), status_text, (w / 2.0) as i32, (banner_y + 8.0) as i32, 2, status_color);

        if is_new_record {
            let nrec_x = (banner_x + banner_w + 20.0) as i32;
            BitmapFont::draw_badge(
                &mut self.pixmap.as_mut(),
                "NEW RECORD!",
                nrec_x,
                (banner_y + 4.0) as i32,
                1,
                ColorRgba::new(255, 255, 255, 255),
                ColorRgba::new(180, 130, 10, 255),
                ColorRgba::new(255, 220, 50, 255),
                10,
                4,
            );
        }

        // 3. Main 3-Column Card Layout
        let card_y = 115.0;
        let card_h = h - card_y - 65.0;

        // Left Column: Large Rank Emblem & Core Performance Score
        let left_x = 30.0;
        let left_w = 340.0;
        self.draw_rect(left_x, card_y, left_w, card_h, ColorRgba::new(14, 18, 28, 255));
        self.draw_rect(left_x, card_y, left_w, 1.0, ColorRgba::new(35, 48, 75, 255));
        self.draw_rect(left_x, card_y + card_h - 1.0, left_w, 1.0, ColorRgba::new(35, 48, 75, 255));
        self.draw_rect(left_x, card_y, 1.0, card_h, ColorRgba::new(35, 48, 75, 255));
        self.draw_rect(left_x + left_w - 1.0, card_y, 1.0, card_h, ColorRgba::new(35, 48, 75, 255));

        // Large Rank Emblem Box
        let rank_str = score.rank();
        let (rank_color, rank_glow) = match rank_str {
            "MAX" => (ColorRgba::new(255, 240, 120, 255), ColorRgba::new(255, 215, 0, 255)),
            "AAA" => (ColorRgba::new(255, 220, 50, 255), ColorRgba::new(200, 160, 20, 255)),
            "AA" => (ColorRgba::new(60, 230, 255, 255), ColorRgba::new(20, 140, 200, 255)),
            "A" => (ColorRgba::new(80, 240, 150, 255), ColorRgba::new(20, 150, 80, 255)),
            "B" => (ColorRgba::new(255, 175, 40, 255), ColorRgba::new(180, 100, 15, 255)),
            "C" => (ColorRgba::new(245, 140, 60, 255), ColorRgba::new(160, 80, 20, 255)),
            "D" => (ColorRgba::new(230, 90, 90, 255), ColorRgba::new(140, 40, 40, 255)),
            _ => (ColorRgba::new(160, 70, 70, 255), ColorRgba::new(80, 30, 30, 255)),
        };

        let emblem_w = 160.0;
        let emblem_h = 75.0;
        let emblem_x = left_x + (left_w - emblem_w) / 2.0;
        let emblem_y = card_y + 16.0;

        self.draw_rect(emblem_x, emblem_y, emblem_w, emblem_h, ColorRgba::new(20, 26, 42, 255));
        self.draw_rect(emblem_x, emblem_y, emblem_w, 2.0, rank_glow);
        self.draw_rect(emblem_x, emblem_y + emblem_h - 2.0, emblem_w, 2.0, rank_glow);
        self.draw_rect(emblem_x, emblem_y, 2.0, emblem_h, rank_glow);
        self.draw_rect(emblem_x + emblem_w - 2.0, emblem_y, 2.0, emblem_h, rank_glow);

        BitmapFont::draw_text_centered(&mut self.pixmap.as_mut(), rank_str, (left_x + left_w / 2.0) as i32, (emblem_y + 16.0) as i32, 4, rank_color);

        // Core score lines
        let mut cur_y = emblem_y + emblem_h + 20.0;
        let pad_x = left_x + 24.0;

        // EX Score
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), "EX SCORE", pad_x as i32, cur_y as i32, 1, ColorRgba::new(140, 150, 175, 255));
        cur_y += 16.0;
        let ex_val = format!("{} / {}", score.ex_score, score.max_ex_score());
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), &ex_val, pad_x as i32, cur_y as i32, 2, ColorRgba::new(255, 230, 80, 255));
        if let Some(prev) = previous_best {
            let diff = score.ex_score as i32 - prev.ex_score as i32;
            let diff_str = if diff >= 0 { format!("(+{}) BEST: {}", diff, prev.ex_score) } else { format!("({}) BEST: {}", diff, prev.ex_score) };
            let diff_col = if diff > 0 { ColorRgba::new(80, 255, 140, 255) } else { ColorRgba::new(140, 150, 170, 255) };
            let diff_x = (left_x + left_w - BitmapFont::text_width(&diff_str, 1) as f32 - 24.0) as i32;
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &diff_str, diff_x, (cur_y + 4.0) as i32, 1, diff_col);
        }
        cur_y += 30.0;

        // Accuracy
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), "ACCURACY", pad_x as i32, cur_y as i32, 1, ColorRgba::new(140, 150, 175, 255));
        cur_y += 16.0;
        let acc_val = format!("{:.2}%", score.accuracy_rate());
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), &acc_val, pad_x as i32, cur_y as i32, 2, ColorRgba::new(80, 220, 255, 255));
        if let Some(prev) = previous_best {
            let diff = score.accuracy_rate() - prev.accuracy_rate;
            let diff_str = if diff >= 0.0 { format!("(+{:.2}%)", diff) } else { format!("({:.2}%)", diff) };
            let diff_col = if diff > 0.0 { ColorRgba::new(80, 255, 140, 255) } else { ColorRgba::new(140, 150, 170, 255) };
            let diff_x = (left_x + left_w - BitmapFont::text_width(&diff_str, 1) as f32 - 24.0) as i32;
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &diff_str, diff_x, (cur_y + 4.0) as i32, 1, diff_col);
        }
        cur_y += 30.0;

        // Max Combo
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), "MAX COMBO", pad_x as i32, cur_y as i32, 1, ColorRgba::new(140, 150, 175, 255));
        cur_y += 16.0;
        let combo_val = format!("{} / {}", score.max_combo, score.total_notes);
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), &combo_val, pad_x as i32, cur_y as i32, 2, ColorRgba::new(255, 255, 255, 255));
        if let Some(prev) = previous_best {
            let diff = score.max_combo as i32 - prev.max_combo as i32;
            let diff_str = if diff >= 0 { format!("(+{}) BEST: {}", diff, prev.max_combo) } else { format!("({}) BEST: {}", diff, prev.max_combo) };
            let diff_col = if diff > 0 { ColorRgba::new(80, 255, 140, 255) } else { ColorRgba::new(140, 150, 170, 255) };
            let diff_x = (left_x + left_w - BitmapFont::text_width(&diff_str, 1) as f32 - 24.0) as i32;
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &diff_str, diff_x, (cur_y + 4.0) as i32, 1, diff_col);
        }

        // Center Column: Detailed Judge Breakdown & Fast/Slow
        let mid_x = left_x + left_w + 20.0;
        let mid_w = 300.0;
        self.draw_rect(mid_x, card_y, mid_w, card_h, ColorRgba::new(14, 18, 28, 255));
        self.draw_rect(mid_x, card_y, mid_w, 1.0, ColorRgba::new(35, 48, 75, 255));
        self.draw_rect(mid_x, card_y + card_h - 1.0, mid_w, 1.0, ColorRgba::new(35, 48, 75, 255));
        self.draw_rect(mid_x, card_y, 1.0, card_h, ColorRgba::new(35, 48, 75, 255));
        self.draw_rect(mid_x + mid_w - 1.0, card_y, 1.0, card_h, ColorRgba::new(35, 48, 75, 255));

        let mut mid_y = card_y + 20.0;
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), "JUDGE BREAKDOWN", (mid_x + 20.0) as i32, mid_y as i32, 1, ColorRgba::new(160, 175, 205, 255));
        mid_y += 24.0;

        let judge_counts = [
            ("PERFECT GREAT", score.pgreat_count, ColorRgba::new(255, 230, 80, 255)),
            ("GREAT", score.great_count, ColorRgba::new(255, 170, 50, 255)),
            ("GOOD", score.good_count, ColorRgba::new(60, 220, 120, 255)),
            ("BAD", score.bad_count, ColorRgba::new(180, 70, 240, 255)),
            ("POOR", score.poor_count, ColorRgba::new(240, 50, 50, 255)),
            ("MISS", score.miss_count, ColorRgba::new(140, 140, 140, 255)),
        ];

        for (label, count, color) in judge_counts {
            self.draw_rect(mid_x + 20.0, mid_y, mid_w - 40.0, 26.0, ColorRgba::new(20, 25, 38, 200));
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), label, (mid_x + 30.0) as i32, (mid_y + 6.0) as i32, 1, color);
            let cnt_str = format!("{:>5}", count);
            let cnt_x = (mid_x + mid_w - BitmapFont::text_width(&cnt_str, 1) as f32 - 30.0) as i32;
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &cnt_str, cnt_x, (mid_y + 6.0) as i32, 1, ColorRgba::new(255, 255, 255, 255));
            mid_y += 32.0;
        }

        mid_y += 10.0;
        // Fast / Slow stats box
        let fs_w = (mid_w - 48.0) / 2.0;
        // Fast box
        self.draw_rect(mid_x + 20.0, mid_y, fs_w, 40.0, ColorRgba::new(15, 30, 50, 255));
        self.draw_rect(mid_x + 20.0, mid_y, fs_w, 1.0, ColorRgba::new(40, 100, 180, 255));
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), "FAST", (mid_x + 28.0) as i32, (mid_y + 6.0) as i32, 1, ColorRgba::new(80, 200, 255, 255));
        let fast_str = format!("{}", score.fast_count);
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), &fast_str, (mid_x + 28.0) as i32, (mid_y + 20.0) as i32, 1, ColorRgba::new(255, 255, 255, 255));

        // Slow box
        let slow_box_x = mid_x + 20.0 + fs_w + 8.0;
        self.draw_rect(slow_box_x, mid_y, fs_w, 40.0, ColorRgba::new(50, 25, 15, 255));
        self.draw_rect(slow_box_x, mid_y, fs_w, 1.0, ColorRgba::new(180, 80, 40, 255));
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), "SLOW", (slow_box_x + 10.0) as i32, (mid_y + 6.0) as i32, 1, ColorRgba::new(255, 140, 60, 255));
        let slow_str = format!("{}", score.slow_count);
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), &slow_str, (slow_box_x + 10.0) as i32, (mid_y + 20.0) as i32, 1, ColorRgba::new(255, 255, 255, 255));

        // Right Column: Timing Offset Histogram Distribution
        let right_x = mid_x + mid_w + 20.0;
        let right_w = w - right_x - 30.0;
        self.draw_rect(right_x, card_y, right_w, card_h, ColorRgba::new(14, 18, 28, 255));
        self.draw_rect(right_x, card_y, right_w, 1.0, ColorRgba::new(35, 48, 75, 255));
        self.draw_rect(right_x, card_y + card_h - 1.0, right_w, 1.0, ColorRgba::new(35, 48, 75, 255));
        self.draw_rect(right_x, card_y, 1.0, card_h, ColorRgba::new(35, 48, 75, 255));
        self.draw_rect(right_x + right_w - 1.0, card_y, 1.0, card_h, ColorRgba::new(35, 48, 75, 255));

        let mut right_y = card_y + 20.0;
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), "TIMING OFFSET DISTRIBUTION", (right_x + 20.0) as i32, right_y as i32, 1, ColorRgba::new(160, 175, 205, 255));
        right_y += 30.0;

        // Histogram Graph Area
        let hist_x = right_x + 24.0;
        let hist_w = right_w - 48.0;
        let hist_h = 220.0;
        let hist_y = right_y;

        self.draw_rect(hist_x, hist_y, hist_w, hist_h, ColorRgba::new(18, 22, 34, 255));
        self.draw_rect(hist_x, hist_y, hist_w, 1.0, ColorRgba::new(35, 45, 68, 255));
        self.draw_rect(hist_x, hist_y + hist_h, hist_w, 1.0, ColorRgba::new(35, 45, 68, 255));

        // Center line (0ms target)
        let center_hist_x = hist_x + hist_w / 2.0;
        self.draw_rect(center_hist_x, hist_y, 1.0, hist_h, ColorRgba::new(80, 200, 255, 120));

        let max_bucket_val = score.timing_histogram.iter().copied().max().unwrap_or(1).max(1) as f32;
        let num_bars = score.timing_histogram.len();
        let bar_width = (hist_w / num_bars as f32) - 2.0;

        for (b_idx, &count) in score.timing_histogram.iter().enumerate() {
            let bx = hist_x + (b_idx as f32 * (hist_w / num_bars as f32)) + 1.0;
            let bar_h = (count as f32 / max_bucket_val) * (hist_h - 20.0);
            let by = hist_y + hist_h - bar_h;

            let bar_color = if b_idx == 8 {
                ColorRgba::new(255, 230, 80, 255) // Center Gold
            } else if b_idx < 8 {
                ColorRgba::new(60, 180, 255, 230) // Fast Blue
            } else {
                ColorRgba::new(255, 140, 50, 230) // Slow Orange
            };

            if bar_h > 0.0 {
                self.draw_rect(bx, by, bar_width, bar_h, bar_color);
            }
        }

        // Labels under histogram
        let label_y = (hist_y + hist_h + 8.0) as i32;
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), "-40ms (FAST)", hist_x as i32, label_y, 1, ColorRgba::new(80, 180, 240, 255));
        BitmapFont::draw_text_centered(&mut self.pixmap.as_mut(), "0ms (PERFECT)", center_hist_x as i32, label_y, 1, ColorRgba::new(255, 230, 80, 255));
        let slow_lbl_x = (hist_x + hist_w - BitmapFont::text_width("+40ms (SLOW)", 1) as f32) as i32;
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), "+40ms (SLOW)", slow_lbl_x, label_y, 1, ColorRgba::new(255, 140, 60, 255));

        // 4. Bottom Footer Navigation Bar
        let footer_y = (h - 36.0) as i32;
        self.draw_rect(0.0, footer_y as f32, w, 36.0, ColorRgba::new(12, 16, 24, 255));
        self.draw_rect(0.0, footer_y as f32, w, 1.0, ColorRgba::new(40, 50, 75, 255));

        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            "[Enter / Space / Esc]: Return to Song Select     [R]: Retry Stage",
            (w / 2.0) as i32,
            footer_y + 10,
            1,
            ColorRgba::new(160, 175, 205, 255),
        );
    }
}

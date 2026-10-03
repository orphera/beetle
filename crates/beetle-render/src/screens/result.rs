use crate::bitmap_font::BitmapFont;
use crate::components::Corner;
use crate::design_tokens::ColorToken;
use crate::renderer::{truncate_str, SoftwareRenderer};
use crate::skin::ColorRgba;
use beetle_core::{BmsChart, ScoreTracker};

impl SoftwareRenderer {
    /// Renders the Stage Result screen: rank emblem, score headline, a single
    /// proportional judge-breakdown bar, and the timing offset histogram.
    ///
    /// PULSE direction: hairline-separated unboxed columns instead of three
    /// bordered panels, a diagonal-cut NEW RECORD badge, and the same
    /// segmented judge bar used on the Gameplay HUD instead of a boxed list.
    /// See docs/plans/2026-10-03-pulse-redesign.md and sketches/pulse-redesign/.
    pub fn render_result(
        &mut self,
        chart: &BmsChart,
        score: &ScoreTracker,
        is_new_record: bool,
        previous_best: Option<&beetle_core::ScoreRecord>,
        elapsed_seconds: f64,
    ) {
        self.clear();

        let vp = self.viewport;
        let s = vp.scale;
        let font_scale = (s * 0.9).round().max(2.0) as u32;
        let label_scale = (s * 0.72).round().max(2.0) as u32;
        let cyan = ColorToken::PULSE_CYAN.rgba();
        let muted = ColorToken::TEXT_TERTIARY.rgba();
        let hairline = ColorRgba::new(0x17, 0x1b, 0x27, 255);

        // Reveal timing: rank letter pops/settles into place first (spring
        // overshoot via ease_out_back reads as "slamming down" with weight),
        // then the EX score counts up from zero while the rank is still
        // settling. Both finish well within RESULT_REVEAL_DURATION_SECONDS
        // so the caller knows when it can stop forcing redraws. See
        // docs/plans/2026-10-03-pulse-redesign.md.
        let rank_pop_t = (elapsed_seconds / crate::RANK_POP_SECONDS).clamp(0.0, 1.0) as f32;
        let rank_eased_pos = crate::motion::ease_out_back(rank_pop_t);
        let rank_alpha = (crate::motion::ease_out_cubic(rank_pop_t) * 255.0) as u8;
        let rank_offset_y = ((1.0 - rank_eased_pos) * -36.0 * s) as i32;

        let score_t = (elapsed_seconds / crate::SCORE_COUNT_SECONDS).clamp(0.0, 1.0) as f32;
        let score_eased = crate::motion::ease_out_cubic(score_t);
        let displayed_ex = (score.ex_score as f32 * score_eased).round() as u32;

        // -----------------------------------------------------------------
        // 1. Header: title/artist right-aligned, thin accent underline
        // -----------------------------------------------------------------
        let header_h = 48.0 * s;
        self.draw_rect(
            vp.x,
            vp.y,
            vp.width,
            header_h,
            ColorToken::SURFACE_BASE.rgba(),
        );
        self.draw_rect(vp.x, vp.y + header_h - s, vp.width, s, hairline);
        self.draw_gradient_rect(
            vp.x,
            vp.y + header_h - 2.0 * s,
            160.0 * s,
            2.0 * s,
            cyan,
            cyan.with_alpha(0),
            true,
        );

        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "STAGE RESULT",
            (vp.x + 24.0 * s) as i32,
            (vp.y + 16.0 * s) as i32,
            label_scale,
            cyan,
        );

        let title_str = truncate_str(&chart.header.title, 32);
        let artist_str = truncate_str(&chart.header.artist, 28);
        let right_title_x = (vp.x + vp.width
            - BitmapFont::text_width(&title_str, font_scale) as f32
            - 24.0 * s) as i32;
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &title_str,
            right_title_x,
            (vp.y + 10.0 * s) as i32,
            font_scale,
            ColorRgba::new(255, 255, 255, 255),
        );
        let right_artist_x = (vp.x + vp.width
            - BitmapFont::text_width(&artist_str, font_scale) as f32
            - 24.0 * s) as i32;
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &artist_str,
            right_artist_x,
            (vp.y + 26.0 * s) as i32,
            font_scale,
            muted,
        );

        let body_y = vp.y + header_h + 16.0 * s;
        let footer_h = 36.0 * s;
        let body_h = vp.y + vp.height - footer_h - body_y;

        // -----------------------------------------------------------------
        // 2. Left: rank emblem (unboxed, glow via text shadow) + status
        // -----------------------------------------------------------------
        let left_x = vp.x + 24.0 * s;
        let left_w = 300.0 * s;
        self.draw_rect(left_x + left_w, body_y, s, body_h, hairline);

        let rank_str = score.rank();
        let (rank_color, rank_glow) = match rank_str {
            "MAX" => (
                ColorRgba::new(255, 240, 120, 255),
                ColorRgba::new(255, 215, 0, 255),
            ),
            "AAA" => (
                ColorRgba::new(255, 220, 50, 255),
                ColorRgba::new(200, 160, 20, 255),
            ),
            "AA" => (
                ColorRgba::new(60, 230, 255, 255),
                ColorRgba::new(20, 140, 200, 255),
            ),
            "A" => (
                ColorRgba::new(80, 240, 150, 255),
                ColorRgba::new(20, 150, 80, 255),
            ),
            "B" => (
                ColorRgba::new(255, 175, 40, 255),
                ColorRgba::new(180, 100, 15, 255),
            ),
            "C" => (
                ColorRgba::new(245, 140, 60, 255),
                ColorRgba::new(160, 80, 20, 255),
            ),
            "D" => (
                ColorRgba::new(230, 90, 90, 255),
                ColorRgba::new(140, 40, 40, 255),
            ),
            _ => (
                ColorRgba::new(160, 70, 70, 255),
                ColorRgba::new(80, 30, 30, 255),
            ),
        };

        self.draw_gradient_rect(
            left_x,
            body_y,
            left_w,
            body_h,
            rank_glow.with_alpha(20),
            rank_glow.with_alpha(0),
            false,
        );

        let rank_font_scale = (6.0 * s).round().max(2.0) as u32;
        let rank_cy = body_y + 110.0 * s + rank_offset_y as f32;
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            rank_str,
            (left_x + left_w / 2.0) as i32,
            rank_cy as i32,
            rank_font_scale,
            rank_glow.with_alpha(((rank_glow.a as u16 * rank_alpha as u16) / 255) as u8 / 3),
        );
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            rank_str,
            (left_x + left_w / 2.0) as i32,
            rank_cy as i32,
            rank_font_scale,
            rank_color.with_alpha(rank_alpha),
        );

        let (status_text, status_color) = if score.is_cleared() {
            if score.miss_count == 0 && score.poor_count == 0 && score.bad_count == 0 {
                if score.great_count == 0 && score.good_count == 0 {
                    ("PERFECT CLEAR", ColorRgba::new(255, 220, 50, 255))
                } else {
                    ("FULL COMBO CLEAR", ColorRgba::new(60, 255, 140, 255))
                }
            } else {
                ("STAGE CLEARED", cyan)
            }
        } else {
            ("STAGE FAILED", ColorRgba::new(255, 70, 70, 255))
        };
        let status_y = rank_cy + 60.0 * s;
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            status_text,
            (left_x + left_w / 2.0) as i32,
            status_y as i32,
            label_scale,
            status_color,
        );

        if is_new_record {
            let badge_w = 150.0 * s;
            let badge_x = left_x + (left_w - badge_w) / 2.0;
            let badge_y = status_y + 26.0 * s;
            self.draw_cut_quad(
                badge_x,
                badge_y,
                badge_w,
                26.0 * s,
                10.0 * s,
                ColorToken::PULSE_MAGENTA.rgba(),
                ColorToken::PULSE_MAGENTA.rgba(),
            );
            BitmapFont::draw_bold_text_centered(
                &mut self.pixmap.as_mut(),
                "NEW RECORD",
                (badge_x + badge_w / 2.0 + 5.0 * s) as i32,
                (badge_y + 6.0 * s) as i32,
                font_scale,
                ColorRgba::new(255, 255, 255, 255),
            );
        }

        // -----------------------------------------------------------------
        // 3. Center: score headline + judge breakdown bar + FAST/SLOW
        // -----------------------------------------------------------------
        let mid_x = left_x + left_w + 32.0 * s;
        let mid_w = 340.0 * s;
        self.draw_rect(mid_x + mid_w, body_y, s, body_h, hairline);

        let mut mid_y = body_y;
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "EX SCORE",
            mid_x as i32,
            mid_y as i32,
            label_scale,
            muted,
        );
        mid_y += 14.0 * s;
        let ex_val = format!("{} / {}", displayed_ex, score.max_ex_score());
        BitmapFont::draw_bold_text(
            &mut self.pixmap.as_mut(),
            &ex_val,
            mid_x as i32,
            mid_y as i32,
            (font_scale as f32 * 2.0) as u32,
            ColorToken::ACCENT_YELLOW.rgba(),
        );
        if let Some(prev) = previous_best {
            let diff = score.ex_score as i32 - prev.ex_score as i32;
            let diff_str = if diff >= 0 {
                format!("+{} vs best", diff)
            } else {
                format!("{} vs best", diff)
            };
            let diff_col = if diff > 0 {
                ColorToken::ACCENT_GREEN.rgba()
            } else {
                muted
            };
            let dx = (mid_x + mid_w - BitmapFont::text_width(&diff_str, label_scale) as f32) as i32;
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &diff_str,
                dx,
                (mid_y + 6.0 * s) as i32,
                label_scale,
                diff_col,
            );
        }
        mid_y += 46.0 * s;
        self.draw_rect(mid_x, mid_y, mid_w, s, hairline);
        mid_y += 16.0 * s;

        // Stat grid: ACCURACY / MAX COMBO, unboxed label+value pairs.
        let col_w = mid_w / 2.0;
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "ACCURACY",
            mid_x as i32,
            mid_y as i32,
            label_scale,
            muted,
        );
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "MAX COMBO",
            (mid_x + col_w) as i32,
            mid_y as i32,
            label_scale,
            muted,
        );
        mid_y += 14.0 * s;
        let acc_val = format!("{:.2}%", score.accuracy_rate());
        let combo_val = format!("{} / {}", score.max_combo, score.total_notes);
        BitmapFont::draw_bold_text(
            &mut self.pixmap.as_mut(),
            &acc_val,
            mid_x as i32,
            mid_y as i32,
            font_scale,
            cyan,
        );
        BitmapFont::draw_bold_text(
            &mut self.pixmap.as_mut(),
            &combo_val,
            (mid_x + col_w) as i32,
            mid_y as i32,
            font_scale,
            ColorRgba::new(255, 255, 255, 255),
        );
        mid_y += 36.0 * s;
        self.draw_rect(mid_x, mid_y, mid_w, s, hairline);
        mid_y += 16.0 * s;

        // Judge breakdown: single segmented bar + 2-column legend.
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "JUDGE BREAKDOWN",
            mid_x as i32,
            mid_y as i32,
            label_scale,
            muted,
        );
        mid_y += 16.0 * s;
        let counts = [
            (
                "PGREAT",
                score.pgreat_count,
                ColorToken::ACCENT_YELLOW.rgba(),
            ),
            ("GREAT", score.great_count, ColorToken::ACCENT_ORANGE.rgba()),
            ("GOOD", score.good_count, ColorToken::ACCENT_GREEN.rgba()),
            ("BAD", score.bad_count, ColorToken::DIFF_ANOTHER.rgba()),
            ("POOR", score.poor_count, ColorToken::ACCENT_RED.rgba()),
            ("MISS", score.miss_count, ColorToken::TEXT_TERTIARY.rgba()),
        ];
        let total: u32 = counts.iter().map(|&(_, c, _)| c).sum::<u32>().max(1);
        let bar_h = 10.0 * s;
        let mut bar_x = mid_x;
        for &(_, count, color) in &counts {
            let seg_w = mid_w * (count as f32 / total as f32);
            if seg_w > 0.0 {
                self.draw_rect(bar_x, mid_y, seg_w, bar_h, color);
            }
            bar_x += seg_w;
        }
        mid_y += bar_h + 12.0 * s;
        let legend_col_w = mid_w / 2.0;
        for (i, &(label, count, color)) in counts.iter().enumerate() {
            let col = i % 2;
            let row = i / 2;
            let lx = mid_x + col as f32 * legend_col_w;
            let ly = mid_y + row as f32 * 16.0 * s;
            self.draw_rect(lx, ly + 3.0 * s, 6.0 * s, 6.0 * s, color);
            let legend_str = format!("{} {}", label, count);
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &legend_str,
                (lx + 11.0 * s) as i32,
                ly as i32,
                label_scale,
                color,
            );
        }
        mid_y += 54.0 * s;
        self.draw_rect(mid_x, mid_y, mid_w, s, hairline);
        mid_y += 16.0 * s;

        // FAST / SLOW unboxed pair.
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "FAST",
            mid_x as i32,
            mid_y as i32,
            label_scale,
            ColorRgba::new(80, 200, 255, 255),
        );
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "SLOW",
            (mid_x + col_w) as i32,
            mid_y as i32,
            label_scale,
            ColorRgba::new(255, 140, 60, 255),
        );
        mid_y += 14.0 * s;
        BitmapFont::draw_bold_text(
            &mut self.pixmap.as_mut(),
            &format!("{}", score.fast_count),
            mid_x as i32,
            mid_y as i32,
            font_scale,
            ColorRgba::new(255, 255, 255, 255),
        );
        BitmapFont::draw_bold_text(
            &mut self.pixmap.as_mut(),
            &format!("{}", score.slow_count),
            (mid_x + col_w) as i32,
            mid_y as i32,
            font_scale,
            ColorRgba::new(255, 255, 255, 255),
        );

        // -----------------------------------------------------------------
        // 4. Right: timing offset histogram
        // -----------------------------------------------------------------
        let right_x = mid_x + mid_w + 32.0 * s;
        let right_w = vp.x + vp.width - right_x - 24.0 * s;

        let mut right_y = body_y;
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "TIMING OFFSET DISTRIBUTION",
            right_x as i32,
            right_y as i32,
            label_scale,
            muted,
        );
        right_y += 24.0 * s;

        let hist_x = right_x;
        let hist_w = right_w;
        let hist_h = (body_h - 90.0 * s).max(140.0 * s);
        let hist_y = right_y;

        self.draw_rect(hist_x, hist_y + hist_h, hist_w, s, hairline);
        let center_hist_x = hist_x + hist_w / 2.0;
        self.draw_rect(center_hist_x, hist_y, s, hist_h, cyan.with_alpha(110));

        let max_bucket_val = score
            .timing_histogram
            .iter()
            .copied()
            .max()
            .unwrap_or(1)
            .max(1) as f32;
        let num_bars = score.timing_histogram.len();
        let bar_width = ((hist_w / num_bars as f32) - 2.0 * s).max(1.0);
        for (b_idx, &count) in score.timing_histogram.iter().enumerate() {
            let bx = hist_x + (b_idx as f32 * (hist_w / num_bars as f32)) + 1.0 * s;
            let h = (count as f32 / max_bucket_val) * (hist_h - 20.0 * s);
            let by = hist_y + hist_h - h;
            let bar_color = if b_idx == 8 {
                ColorToken::ACCENT_YELLOW.rgba()
            } else if b_idx < 8 {
                cyan
            } else {
                ColorToken::PULSE_MAGENTA.rgba()
            };
            if h > 0.0 {
                self.draw_rect(bx, by, bar_width, h, bar_color);
            }
        }

        let label_y = (hist_y + hist_h + 8.0 * s) as i32;
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "-40ms",
            hist_x as i32,
            label_y,
            label_scale,
            cyan,
        );
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            "0ms",
            center_hist_x as i32,
            label_y,
            label_scale,
            ColorToken::ACCENT_YELLOW.rgba(),
        );
        let slow_lbl_x =
            (hist_x + hist_w - BitmapFont::text_width("+40ms", label_scale) as f32) as i32;
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "+40ms",
            slow_lbl_x,
            label_y,
            label_scale,
            ColorToken::PULSE_MAGENTA.rgba(),
        );

        // Corner accent tying the Result screen back into the PULSE identity.
        self.draw_corner_triangle(
            right_x + right_w,
            body_y,
            24.0 * s,
            Corner::TopRight,
            cyan.with_alpha(80),
        );

        // -----------------------------------------------------------------
        // 5. Footer
        // -----------------------------------------------------------------
        let footer_y = vp.y + vp.height - footer_h;
        self.draw_rect(
            vp.x,
            footer_y,
            vp.width,
            footer_h,
            ColorRgba::new(0, 0, 0, 255),
        );
        self.draw_rect(vp.x, footer_y, vp.width, s, hairline);
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            "[Enter / Space / Esc]: Return to Song Select     [R]: Retry Stage",
            (vp.x + vp.width / 2.0) as i32,
            (footer_y + 11.0 * s) as i32,
            label_scale,
            muted,
        );
    }
}

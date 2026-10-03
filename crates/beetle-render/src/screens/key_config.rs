use crate::bitmap_font::BitmapFont;
use crate::renderer::SoftwareRenderer;
use crate::skin::ColorRgba;

/// Color of the small lane indicator strip, derived from the lane's label
/// text (e.g. "SCRATCH (1S)", "KEY 3 (2P)") rather than its row position -
/// a column's first row isn't always Scratch (PMS/Keys9 has none), so a
/// position-based rule would mislabel it.
fn lane_indicator_color(lane_name: &str) -> ColorRgba {
    if lane_name.starts_with("SCRATCH") {
        return ColorRgba::new(255, 70, 70, 255); // Scratch: Red
    }
    let digit = lane_name
        .trim_start_matches("KEY ")
        .chars()
        .next()
        .and_then(|c| c.to_digit(10))
        .unwrap_or(1);
    if digit % 2 == 1 {
        ColorRgba::new(255, 255, 255, 255) // White keys (odd)
    } else {
        ColorRgba::new(60, 140, 255, 255) // Blue keys (even)
    }
}

impl SoftwareRenderer {
    /// Renders the interactive 1:1 key configuration screen.
    ///
    /// `key_names` lists every lane for the chart's current PlayMode, in
    /// `SkinConfig::active_lanes()` order. Modes with more than 8 lanes
    /// (Double Play 10K/14K) render as two side-by-side columns - the first
    /// half (1P) on the left, the second half (2P) on the right - since a
    /// single column can't fit 12-16 rows in the available height.
    pub fn render_key_config(
        &mut self,
        key_names: &[(&'static str, String)],
        selected_lane_idx: usize,
        preset_name: &str,
        is_rebinding: bool,
    ) {
        self.clear();

        let vp = self.viewport;
        let s = vp.scale;
        let font_scale = (s * 0.9).round().max(2.0) as u32;
        let center_x = (vp.x + vp.width / 2.0) as i32;

        // Header (token colors)
        self.draw_rect(
            vp.x,
            vp.y,
            vp.width,
            48.0 * s,
            crate::design_tokens::ColorToken::SURFACE_BASE.rgba(),
        );
        self.draw_rect(
            vp.x,
            vp.y + 47.0 * s,
            vp.width,
            1.0 * s,
            ColorRgba::new(40, 55, 85, 255),
        );

        BitmapFont::draw_badge(
            &mut self.pixmap.as_mut(),
            "KEY CONFIGURATION",
            (vp.x + 24.0 * s) as i32,
            (vp.y + 12.0 * s) as i32,
            font_scale,
            ColorRgba::new(80, 200, 255, 255),
            ColorRgba::new(20, 35, 65, 255),
            ColorRgba::new(50, 120, 220, 255),
            (10.0 * s) as i32,
            (4.0 * s) as i32,
        );

        let preset_badge = format!("LAYOUT: {}", preset_name);
        let right_preset_x = (vp.x + vp.width
            - BitmapFont::text_width(&preset_badge, font_scale) as f32
            - 24.0 * s) as i32;
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &preset_badge,
            right_preset_x,
            (vp.y + 16.0 * s) as i32,
            font_scale,
            ColorRgba::new(255, 220, 80, 255),
        );

        // Subtitle prompt
        let prompt_y = (vp.y + 65.0 * s) as i32;
        let (prompt_str, prompt_color) = if is_rebinding {
            (
                ">> PRESS ANY KEYBOARD KEY TO BIND <<",
                ColorRgba::new(255, 230, 80, 255),
            )
        } else {
            (
                "Select lane with [Up/Down] and press [Enter] to remap",
                ColorRgba::new(160, 175, 205, 255),
            )
        };
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            prompt_str,
            center_x,
            prompt_y,
            font_scale,
            prompt_color,
        );

        // Main table: a single column up to 8 lanes; Double Play (>8 lanes,
        // 12 for 10K or 16 for 14K) splits into a 1P/2P column pair so rows
        // stay readable instead of overflowing the box.
        let two_column = key_names.len() > 8;
        let mid = key_names.len() / 2;
        let columns: Vec<(&[(&'static str, String)], usize)> = if two_column {
            vec![(&key_names[..mid], 0), (&key_names[mid..], mid)]
        } else {
            (!key_names.is_empty())
                .then(|| (key_names, 0))
                .into_iter()
                .collect()
        };

        let row_step = 40.0 * s;
        let rows_in_tallest_column = columns
            .iter()
            .map(|(names, _)| names.len())
            .max()
            .unwrap_or(0) as f32;
        let box_h = (rows_in_tallest_column * row_step + 36.0 * s).min(vp.height - 150.0 * s);
        let box_y = vp.y + 95.0 * s;

        let single_box_w = 540.0 * s;
        let col_box_w = 440.0 * s;
        let gap = 20.0 * s;
        let total_w = if two_column {
            col_box_w * 2.0 + gap
        } else {
            single_box_w
        };
        let start_x = vp.x + (vp.width - total_w) / 2.0;

        for (col_idx, (col_names, start_idx)) in columns.iter().enumerate() {
            let box_w = if two_column { col_box_w } else { single_box_w };
            let box_x = start_x + (col_idx as f32) * (box_w + gap);

            self.draw_rect(box_x, box_y, box_w, box_h, ColorRgba::new(17, 21, 33, 255));

            if two_column {
                let label = if col_idx == 0 { "1P SIDE" } else { "2P SIDE" };
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    label,
                    (box_x + 4.0 * s) as i32,
                    (box_y - 18.0 * s) as i32,
                    font_scale,
                    ColorRgba::new(140, 160, 200, 255),
                );
            }

            let mut row_y = box_y + 18.0 * s;

            for (local_i, (lane_name, key_name)) in col_names.iter().enumerate() {
                let i = start_idx + local_i;
                let is_sel = i == selected_lane_idx;

                if is_sel {
                    let row_bg = if is_rebinding {
                        ColorRgba::new(70, 50, 20, 255)
                    } else {
                        ColorRgba::new(30, 55, 110, 255)
                    };
                    let border_col = if is_rebinding {
                        ColorRgba::new(255, 200, 50, 255)
                    } else {
                        ColorRgba::new(80, 160, 255, 255)
                    };
                    self.draw_rect(
                        box_x + 12.0 * s,
                        row_y - 4.0 * s,
                        box_w - 24.0 * s,
                        36.0 * s,
                        row_bg,
                    );
                    self.draw_rect(
                        box_x + 12.0 * s,
                        row_y - 4.0 * s,
                        box_w - 24.0 * s,
                        1.0 * s,
                        border_col,
                    );
                    self.draw_rect(
                        box_x + 12.0 * s,
                        row_y + 31.0 * s,
                        box_w - 24.0 * s,
                        1.0 * s,
                        border_col,
                    );
                } else {
                    self.draw_rect(
                        box_x + 12.0 * s,
                        row_y - 4.0 * s,
                        box_w - 24.0 * s,
                        36.0 * s,
                        ColorRgba::new(22, 27, 42, 255),
                    );
                }

                // Lane icon / color indicator
                self.draw_rect(
                    box_x + 24.0 * s,
                    row_y + 2.0 * s,
                    6.0 * s,
                    24.0 * s,
                    lane_indicator_color(lane_name),
                );

                // Lane Name
                let name_color = if is_sel {
                    ColorRgba::new(255, 255, 255, 255)
                } else {
                    ColorRgba::new(180, 190, 210, 255)
                };
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    lane_name,
                    (box_x + 40.0 * s) as i32,
                    (row_y + 8.0 * s) as i32,
                    font_scale,
                    name_color,
                );

                // Key value / Rebinding status
                let val_str = if is_sel && is_rebinding {
                    "< PRESS ANY KEY >".to_string()
                } else {
                    format!("[  {}  ]", key_name)
                };

                let val_color = if is_sel && is_rebinding {
                    ColorRgba::new(255, 230, 80, 255)
                } else if is_sel {
                    ColorRgba::new(100, 230, 255, 255)
                } else {
                    ColorRgba::new(220, 225, 240, 255)
                };

                let val_x = (box_x + box_w
                    - BitmapFont::text_width(&val_str, font_scale) as f32
                    - 30.0 * s) as i32;
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    &val_str,
                    val_x,
                    (row_y + 8.0 * s) as i32,
                    font_scale,
                    val_color,
                );

                row_y += row_step;
            }
        }

        // Footer instructions
        let footer_y = (vp.y + vp.height - 36.0 * s) as i32;
        self.draw_rect(
            vp.x,
            footer_y as f32,
            vp.width,
            36.0 * s,
            ColorRgba::new(12, 16, 24, 255),
        );
        self.draw_rect(
            vp.x,
            footer_y as f32,
            vp.width,
            1.0 * s,
            ColorRgba::new(40, 50, 75, 255),
        );

        let help_text = if is_rebinding {
            "Press any key to assign to this lane      [Esc]: Cancel"
        } else {
            "[Up/Down]: Select Lane   [Enter]: Rebind Key   [F1]: Toggle Preset   [Del]: Reset   [Esc]: Save & Return"
        };

        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            help_text,
            center_x,
            footer_y + (10.0 * s) as i32,
            font_scale,
            ColorRgba::new(160, 175, 205, 255),
        );
    }
}

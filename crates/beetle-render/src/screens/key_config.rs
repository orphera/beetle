use crate::bitmap_font::BitmapFont;
use crate::renderer::SoftwareRenderer;
use crate::skin::ColorRgba;

impl SoftwareRenderer {
    /// Renders the interactive 1:1 key configuration screen.
    pub fn render_key_config(
        &mut self,
        key_names: &[(&'static str, String)],
        selected_lane_idx: usize,
        preset_name: &str,
        is_rebinding: bool,
    ) {
        self.clear();

        let w = self.width() as f32;
        let h = self.height() as f32;
        let center_x = (w / 2.0) as i32;

        // Header
        self.draw_rect(0.0, 0.0, w, 48.0, ColorRgba::new(14, 18, 28, 255));
        self.draw_rect(0.0, 47.0, w, 1.0, ColorRgba::new(40, 55, 85, 255));

        BitmapFont::draw_badge(
            &mut self.pixmap.as_mut(),
            "KEY CONFIGURATION",
            24,
            12,
            1,
            ColorRgba::new(80, 200, 255, 255),
            ColorRgba::new(20, 35, 65, 255),
            ColorRgba::new(50, 120, 220, 255),
            10,
            4,
        );

        let preset_badge = format!("LAYOUT: {}", preset_name);
        let right_preset_x = (w - BitmapFont::text_width(&preset_badge, 1) as f32 - 24.0) as i32;
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), &preset_badge, right_preset_x, 16, 1, ColorRgba::new(255, 220, 80, 255));

        // Subtitle prompt
        let prompt_y = 65;
        let (prompt_str, prompt_color) = if is_rebinding {
            (">> PRESS ANY KEYBOARD KEY TO BIND <<", ColorRgba::new(255, 230, 80, 255))
        } else {
            ("Select lane with [Up/Down] and press [Enter] to remap", ColorRgba::new(160, 175, 205, 255))
        };
        BitmapFont::draw_text_centered(&mut self.pixmap.as_mut(), prompt_str, center_x, prompt_y, 1, prompt_color);

        // Main table card
        let box_w = 540.0;
        let box_h = 360.0;
        let box_x = (w - box_w) / 2.0;
        let box_y = 95.0;

        self.draw_rect(box_x, box_y, box_w, box_h, ColorRgba::new(14, 18, 28, 255));
        self.draw_rect(box_x, box_y, box_w, 1.0, ColorRgba::new(35, 48, 75, 255));
        self.draw_rect(box_x, box_y + box_h - 1.0, box_w, 1.0, ColorRgba::new(35, 48, 75, 255));
        self.draw_rect(box_x, box_y, 1.0, box_h, ColorRgba::new(35, 48, 75, 255));
        self.draw_rect(box_x + box_w - 1.0, box_y, 1.0, box_h, ColorRgba::new(35, 48, 75, 255));

        let mut row_y = box_y + 18.0;

        for (i, (lane_name, key_name)) in key_names.iter().enumerate() {
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
                self.draw_rect(box_x + 12.0, row_y - 4.0, box_w - 24.0, 36.0, row_bg);
                self.draw_rect(box_x + 12.0, row_y - 4.0, box_w - 24.0, 1.0, border_col);
                self.draw_rect(box_x + 12.0, row_y + 31.0, box_w - 24.0, 1.0, border_col);
            } else {
                self.draw_rect(box_x + 12.0, row_y - 4.0, box_w - 24.0, 36.0, ColorRgba::new(18, 24, 38, 180));
            }

            // Lane icon / color indicator
            let lane_color = match i {
                0 => ColorRgba::new(255, 70, 70, 255), // Scratch: Red
                1 | 3 | 5 | 7 => ColorRgba::new(255, 255, 255, 255), // White keys
                _ => ColorRgba::new(60, 140, 255, 255), // Blue keys
            };
            self.draw_rect(box_x + 24.0, row_y + 2.0, 6.0, 24.0, lane_color);

            // Lane Name
            let name_color = if is_sel { ColorRgba::new(255, 255, 255, 255) } else { ColorRgba::new(180, 190, 210, 255) };
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), lane_name, (box_x + 40.0) as i32, (row_y + 8.0) as i32, 1, name_color);

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

            let val_x = (box_x + box_w - BitmapFont::text_width(&val_str, 1) as f32 - 30.0) as i32;
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &val_str, val_x, (row_y + 8.0) as i32, 1, val_color);

            row_y += 40.0;
        }

        // Footer instructions
        let footer_y = (h - 36.0) as i32;
        self.draw_rect(0.0, footer_y as f32, w, 36.0, ColorRgba::new(12, 16, 24, 255));
        self.draw_rect(0.0, footer_y as f32, w, 1.0, ColorRgba::new(40, 50, 75, 255));

        let help_text = if is_rebinding {
            "Press any key to assign to this lane      [Esc]: Cancel"
        } else {
            "[Up/Down]: Select Lane   [Enter]: Rebind Key   [F1]: Toggle Preset   [Del]: Reset   [Esc]: Save & Return"
        };

        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            help_text,
            center_x,
            footer_y + 10,
            1,
            ColorRgba::new(160, 175, 205, 255),
        );
    }
}

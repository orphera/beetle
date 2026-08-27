use beetle_render::bitmap_font::BitmapFont;
use beetle_render::image::ImageBuffer;
use beetle_render::skin::ColorRgba;
use bms_package_manager::PackageRecord;
use tiny_skia::{Color, Paint, Pixmap, Rect, Shader, Transform};

pub struct GuiRenderer {
    pub pixmap: Pixmap,
}

impl GuiRenderer {
    pub fn new(width: u32, height: u32) -> Option<Self> {
        let pixmap = Pixmap::new(width.max(1), height.max(1))?;
        Some(Self { pixmap })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 && (self.pixmap.width() != width || self.pixmap.height() != height) {
            if let Some(new_pixmap) = Pixmap::new(width, height) {
                self.pixmap = new_pixmap;
            }
        }
    }

    pub fn clear(&mut self, color: ColorRgba) {
        self.pixmap.fill(Color::from_rgba8(color.r, color.g, color.b, color.a));
    }

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

    pub fn render_frame(
        &mut self,
        packages: &[&PackageRecord],
        selected_idx: usize,
        selected_ver_idx: usize,
        search_query: &str,
        is_search_active: bool,
        status_msg: &str,
        preview_img: Option<&ImageBuffer>,
        modal_info: Option<(&str, &str)>, // (Prompt, input)
    ) {
        let w = self.pixmap.width() as f32;
        let h = self.pixmap.height() as f32;

        // 1. Background
        self.clear(ColorRgba::new(14, 14, 20, 255));

        // 2. Top Header Bar
        self.draw_rect(0.0, 0.0, w, 56.0, ColorRgba::new(22, 22, 32, 255));
        self.draw_rect(0.0, 55.0, w, 1.0, ColorRgba::new(45, 45, 65, 255));

        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "BMS PACKAGE MANAGER (BPM)",
            20,
            12,
            2,
            ColorRgba::new(255, 220, 90, 255),
        );

        // Search Input Box
        let search_box_x = w - 340.0;
        let search_border_col = if is_search_active {
            ColorRgba::new(80, 180, 255, 255)
        } else {
            ColorRgba::new(50, 50, 70, 255)
        };
        self.draw_rect(search_box_x, 12.0, 320.0, 32.0, ColorRgba::new(28, 28, 40, 255));
        self.draw_rect(search_box_x, 12.0, 320.0, 1.0, search_border_col);
        self.draw_rect(search_box_x, 43.0, 320.0, 1.0, search_border_col);

        let search_disp = if search_query.is_empty() && !is_search_active {
            "[/] Search packages...".to_string()
        } else {
            format!("Search: {}{}", search_query, if is_search_active { "_" } else { "" })
        };
        let search_text_col = if is_search_active {
            ColorRgba::new(255, 255, 255, 255)
        } else {
            ColorRgba::new(140, 140, 160, 255)
        };
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &search_disp,
            search_box_x as i32 + 10,
            22,
            1,
            search_text_col,
        );

        // 3. Left Panel: Package List
        let list_w = (w * 0.52).max(380.0);
        let content_y = 66.0;
        let content_h = h - content_y - 48.0;

        self.draw_rect(16.0, content_y, list_w, content_h, ColorRgba::new(18, 18, 26, 255));
        self.draw_rect(16.0, content_y, list_w, 28.0, ColorRgba::new(26, 26, 38, 255));

        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "INSTALLED PACKAGES",
            26,
            (content_y + 8.0) as i32,
            1,
            ColorRgba::new(170, 170, 190, 255),
        );

        let count_str = format!("Total: {}", packages.len());
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &count_str,
            (16.0 + list_w - 90.0) as i32,
            (content_y + 8.0) as i32,
            1,
            ColorRgba::new(140, 140, 160, 255),
        );

        let row_h = 44.0;
        let visible_count = ((content_h - 32.0) / row_h) as usize;
        let scroll_offset = if selected_idx >= visible_count {
            selected_idx - visible_count + 1
        } else {
            0
        };

        let mut row_y = content_y + 32.0;
        for (i, &pkg) in packages.iter().enumerate().skip(scroll_offset).take(visible_count) {
            let is_selected = i == selected_idx;
            if is_selected {
                self.draw_rect(18.0, row_y, list_w - 4.0, row_h - 2.0, ColorRgba::new(38, 48, 70, 255));
                self.draw_rect(18.0, row_y, 4.0, row_h - 2.0, ColorRgba::new(255, 210, 70, 255));
            } else if i % 2 == 1 {
                self.draw_rect(18.0, row_y, list_w - 4.0, row_h - 2.0, ColorRgba::new(22, 22, 30, 255));
            }

            // Name
            let name_col = if is_selected {
                ColorRgba::new(255, 255, 255, 255)
            } else {
                ColorRgba::new(210, 210, 225, 255)
            };
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &pkg.name, 30, (row_y + 6.0) as i32, 1, name_col);

            // ID & Author & Version
            let author = pkg.author.as_deref().unwrap_or("Unknown");
            let sub_info = format!("{} | by {} | v{} ({} ver)", pkg.id, author, pkg.active_version, pkg.versions.len());
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &sub_info,
                30,
                (row_y + 24.0) as i32,
                1,
                ColorRgba::new(120, 130, 150, 255),
            );

            row_y += row_h;
        }

        // 4. Right Panel: Package Detail View
        let detail_x = 16.0 + list_w + 16.0;
        let detail_w = w - detail_x - 16.0;

        self.draw_rect(detail_x, content_y, detail_w, content_h, ColorRgba::new(18, 18, 26, 255));
        self.draw_rect(detail_x, content_y, detail_w, 28.0, ColorRgba::new(26, 26, 38, 255));

        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "PACKAGE DETAILS",
            detail_x as i32 + 12,
            (content_y + 8.0) as i32,
            1,
            ColorRgba::new(170, 170, 190, 255),
        );

        if let Some(&selected_pkg) = packages.get(selected_idx) {
            let mut dy = content_y + 38.0;

            // Artwork Frame (if preview image exists)
            let art_w = (detail_w - 24.0).min(320.0);
            let art_h = art_w * (9.0 / 16.0);
            let art_x = detail_x + (detail_w - art_w) / 2.0;

            self.draw_rect(art_x, dy, art_w, art_h, ColorRgba::new(10, 10, 16, 255));
            if let Some(img) = preview_img {
                img.draw_scaled(&mut self.pixmap, art_x as i32, dy as i32, art_w as u32, art_h as u32);
            } else {
                BitmapFont::draw_text_centered(
                    &mut self.pixmap.as_mut(),
                    "[NO ARTWORK PREVIEW]",
                    (art_x + art_w / 2.0) as i32,
                    (dy + art_h / 2.0 - 4.0) as i32,
                    1,
                    ColorRgba::new(80, 80, 100, 255),
                );
            }
            dy += art_h + 16.0;

            // Metadata Lines
            let title_line = format!("Title: {}", selected_pkg.name);
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &title_line, detail_x as i32 + 14, dy as i32, 1, ColorRgba::new(240, 240, 250, 255));
            dy += 20.0;

            let id_line = format!("ID:    {}", selected_pkg.id);
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &id_line, detail_x as i32 + 14, dy as i32, 1, ColorRgba::new(180, 180, 200, 255));
            dy += 20.0;

            let author_line = format!("Author: {}", selected_pkg.author.as_deref().unwrap_or("Unknown"));
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &author_line, detail_x as i32 + 14, dy as i32, 1, ColorRgba::new(180, 180, 200, 255));
            dy += 26.0;

            // Installed Versions Management Box
            self.draw_rect(detail_x + 10.0, dy, detail_w - 20.0, 1.0, ColorRgba::new(45, 45, 60, 255));
            dy += 8.0;

            BitmapFont::draw_text(&mut self.pixmap.as_mut(), "Installed Versions (Use [<-/->] to select):", detail_x as i32 + 14, dy as i32, 1, ColorRgba::new(255, 210, 80, 255));
            dy += 20.0;

            let version_keys: Vec<&String> = selected_pkg.versions.keys().collect();
            for (v_idx, &ver) in version_keys.iter().enumerate() {
                let is_ver_selected = v_idx == selected_ver_idx;
                let is_active = ver == &selected_pkg.active_version;

                let ver_tag = format!(
                    "{} v{} {}{}",
                    if is_ver_selected { ">" } else { " " },
                    ver,
                    if is_active { "[ACTIVE]" } else { "" },
                    if is_ver_selected { " (Selected)" } else { "" }
                );

                let ver_col = if is_active {
                    ColorRgba::new(80, 220, 130, 255)
                } else if is_ver_selected {
                    ColorRgba::new(255, 230, 120, 255)
                } else {
                    ColorRgba::new(150, 150, 170, 255)
                };

                BitmapFont::draw_text(&mut self.pixmap.as_mut(), &ver_tag, detail_x as i32 + 20, dy as i32, 1, ver_col);
                dy += 18.0;
            }

            dy += 12.0;
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                "[A]: Activate  [U]: Uninstall Version",
                detail_x as i32 + 14,
                dy as i32,
                1,
                ColorRgba::new(140, 180, 255, 255),
            );
        }

        // 5. Bottom Status / Footer Bar
        let footer_y = h - 40.0;
        self.draw_rect(0.0, footer_y, w, 40.0, ColorRgba::new(16, 16, 24, 255));
        self.draw_rect(0.0, footer_y, w, 1.0, ColorRgba::new(35, 35, 50, 255));

        // Help shortcuts
        let help_text = "[↑/↓]: Navigate  [I]: Import Folder  [P]: Pack Folder  [F5]: Refresh  [Esc]: Exit";
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), help_text, 16, (footer_y + 14.0) as i32, 1, ColorRgba::new(160, 160, 180, 255));

        // Status message
        if !status_msg.is_empty() {
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                status_msg,
                (w - 380.0) as i32,
                (footer_y + 14.0) as i32,
                1,
                ColorRgba::new(80, 220, 140, 255),
            );
        }

        // 6. Input Modal (if active)
        if let Some((prompt, input)) = modal_info {
            let modal_w = 520.0;
            let modal_h = 160.0;
            let modal_x = (w - modal_w) / 2.0;
            let modal_y = (h - modal_h) / 2.0;

            // Backdrop dimming
            self.draw_rect(0.0, 0.0, w, h, ColorRgba::new(0, 0, 0, 160));

            // Modal box
            self.draw_rect(modal_x, modal_y, modal_w, modal_h, ColorRgba::new(26, 26, 38, 255));
            self.draw_rect(modal_x, modal_y, modal_w, 2.0, ColorRgba::new(255, 210, 80, 255));

            BitmapFont::draw_text(&mut self.pixmap.as_mut(), prompt, (modal_x + 20.0) as i32, (modal_y + 24.0) as i32, 1, ColorRgba::new(255, 255, 255, 255));

            // Input line box
            let inp_box_y = modal_y + 60.0;
            self.draw_rect(modal_x + 20.0, inp_box_y, modal_w - 40.0, 36.0, ColorRgba::new(16, 16, 24, 255));
            self.draw_rect(modal_x + 20.0, inp_box_y, modal_w - 40.0, 1.0, ColorRgba::new(80, 180, 255, 255));

            let input_display = format!("{}_", input);
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &input_display, (modal_x + 30.0) as i32, (inp_box_y + 12.0) as i32, 1, ColorRgba::new(255, 255, 255, 255));

            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                "[Enter]: Confirm   [Esc]: Cancel",
                (modal_x + 20.0) as i32,
                (modal_y + 118.0) as i32,
                1,
                ColorRgba::new(140, 140, 160, 255),
            );
        }
    }
}

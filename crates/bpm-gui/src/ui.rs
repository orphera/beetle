use beetle_render::bitmap_font::BitmapFont;
use beetle_render::image::ImageBuffer;
use beetle_render::skin::ColorRgba;
use bms_package_manager::PackageRecord;
use tiny_skia::{Color, Paint, Pixmap, Rect, Shader, Transform};

#[derive(Debug, Clone)]
pub struct TaskProgressInfo<'a> {
    pub message: &'a str,
    pub phase: &'a str,
    pub current: usize,
    pub total: usize,
    pub detail: &'a str,
    pub spinner_frame: usize,
}

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
        bg_task_info: Option<TaskProgressInfo>,
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
            ColorRgba::new(255, 220, 80, 255)
        } else {
            ColorRgba::new(60, 60, 80, 255)
        };
        self.draw_rect(search_box_x, 14.0, 320.0, 28.0, ColorRgba::new(16, 16, 24, 255));
        self.draw_rect(search_box_x, 14.0, 320.0, 1.0, search_border_col);
        self.draw_rect(search_box_x, 41.0, 320.0, 1.0, search_border_col);
        self.draw_rect(search_box_x, 14.0, 1.0, 28.0, search_border_col);
        self.draw_rect(search_box_x + 319.0, 14.0, 1.0, 28.0, search_border_col);

        let search_display = if search_query.is_empty() {
            if is_search_active { "Type to search..._" } else { "Search (press [/])..." }
        } else {
            search_query
        };
        let search_text_col = if is_search_active {
            ColorRgba::new(255, 255, 255, 255)
        } else {
            ColorRgba::new(120, 120, 140, 255)
        };
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), search_display, (search_box_x + 10.0) as i32, 22, 1, search_text_col);

        // 3. Left Panel: Package List View
        let list_w = 420.0;
        let content_y = 68.0;
        let content_h = h - content_y - 48.0;

        self.draw_rect(16.0, content_y, list_w, content_h, ColorRgba::new(18, 18, 26, 255));
        self.draw_rect(16.0, content_y, list_w, 28.0, ColorRgba::new(26, 26, 38, 255));

        let list_title = format!("INSTALLED PACKAGES ({})", packages.len());
        BitmapFont::draw_text(&mut self.pixmap.as_mut(), &list_title, 26, (content_y + 8.0) as i32, 1, ColorRgba::new(170, 170, 190, 255));

        let row_h = 44.0;
        let max_visible_rows = ((content_h - 32.0) / row_h) as usize;
        let scroll_offset = if selected_idx >= max_visible_rows {
            selected_idx - max_visible_rows + 1
        } else {
            0
        };

        let mut row_y = content_y + 32.0;
        for (i, &pkg) in packages.iter().skip(scroll_offset).take(max_visible_rows).enumerate() {
            let actual_idx = scroll_offset + i;
            let is_selected = actual_idx == selected_idx;

            if is_selected {
                self.draw_rect(18.0, row_y, list_w - 4.0, row_h - 2.0, ColorRgba::new(35, 45, 70, 255));
                self.draw_rect(18.0, row_y, 4.0, row_h - 2.0, ColorRgba::new(255, 210, 80, 255));
            } else if actual_idx % 2 == 1 {
                self.draw_rect(18.0, row_y, list_w - 4.0, row_h - 2.0, ColorRgba::new(22, 22, 30, 255));
            }

            // Name
            let name_col = if is_selected {
                ColorRgba::new(255, 255, 255, 255)
            } else {
                ColorRgba::new(210, 210, 225, 255)
            };
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &pkg.name, 30, (row_y + 6.0) as i32, 1, name_col);

            // ID & Author & State
            let author = pkg.author.as_deref().unwrap_or("Unknown");
            let short_active = if pkg.active_state.len() > 10 { &pkg.active_state[..10] } else { &pkg.active_state };
            let sub_info = format!("{} | by {} | #{} ({} states)", pkg.id, author, short_active, pkg.state_hashes.len());
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

            // Installed States Management Box
            self.draw_rect(detail_x + 10.0, dy, detail_w - 20.0, 1.0, ColorRgba::new(45, 45, 60, 255));
            dy += 8.0;

            BitmapFont::draw_text(&mut self.pixmap.as_mut(), "Installed States (Use [<-/->] to select):", detail_x as i32 + 14, dy as i32, 1, ColorRgba::new(255, 210, 80, 255));
            dy += 20.0;

            let state_keys: Vec<&String> = selected_pkg.state_hashes.keys().collect();
            for (v_idx, &st) in state_keys.iter().enumerate() {
                let is_state_selected = v_idx == selected_ver_idx;
                let is_active = st == &selected_pkg.active_state;
                let short_st = if st.len() > 12 { &st[..12] } else { st };

                let ver_tag = format!(
                    "{} {} {}{}",
                    if is_state_selected { ">" } else { " " },
                    short_st,
                    if is_active { "[ACTIVE]" } else { "" },
                    if is_state_selected { " (Selected)" } else { "" }
                );

                let ver_col = if is_active {
                    ColorRgba::new(80, 220, 130, 255)
                } else if is_state_selected {
                    ColorRgba::new(255, 230, 120, 255)
                } else {
                    ColorRgba::new(150, 150, 170, 255)
                };

                BitmapFont::draw_text(&mut self.pixmap.as_mut(), &ver_tag, detail_x as i32 + 20, dy as i32, 1, ver_col);
                dy += 18.0;
            }

            dy += 12.0;

            // Actions box
            self.draw_rect(detail_x + 10.0, dy, detail_w - 20.0, 1.0, ColorRgba::new(45, 45, 60, 255));
            dy += 8.0;

            let action_text = "[A]: Set Active State   [U]/[Del]: Uninstall Selected State";
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                action_text,
                detail_x as i32 + 14,
                dy as i32,
                1,
                ColorRgba::new(130, 170, 220, 255),
            );
        }

        // 5. Bottom Status / Footer Bar
        let footer_y = h - 40.0;
        self.draw_rect(0.0, footer_y, w, 40.0, ColorRgba::new(16, 16, 24, 255));
        self.draw_rect(0.0, footer_y, w, 1.0, ColorRgba::new(35, 35, 50, 255));

        // Help shortcuts
        let help_text = "[↑/↓]: Move  [I]: Import  [P]: Pack  [D]: Apply Delta  [C]: Create Delta  [F5]: Refresh";
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
            let modal_w = 540.0;
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
                "[Enter]: Confirm   [Ctrl+V]: Paste   [Esc]: Cancel",
                (modal_x + 20.0) as i32,
                (modal_y + 118.0) as i32,
                1,
                ColorRgba::new(140, 140, 160, 255),
            );
        }

        // 7. Background Task Running Banner (if active)
        if let Some(info) = bg_task_info {
            let banner_w = (w - 60.0).min(520.0);
            let banner_h = if info.total > 0 { 86.0 } else { 48.0 };
            let banner_x = (w - banner_w) / 2.0;
            let banner_y = 66.0;

            self.draw_rect(banner_x, banner_y, banner_w, banner_h, ColorRgba::new(20, 28, 44, 250));
            self.draw_rect(banner_x, banner_y, banner_w, 2.0, ColorRgba::new(80, 180, 255, 255));
            self.draw_rect(banner_x, banner_y + banner_h - 1.0, banner_w, 1.0, ColorRgba::new(60, 140, 200, 255));

            let spinner_chars = ['|', '/', '-', '\\'];
            let spinner = spinner_chars[info.spinner_frame % 4];

            if info.total > 0 {
                // Title & counts
                let pct = ((info.current as f32 / info.total.max(1) as f32) * 100.0) as u32;
                let phase_disp = if !info.phase.is_empty() {
                    format!("[{}] {} ({}% - {}/{})", spinner, info.phase, pct, info.current, info.total)
                } else {
                    format!("[{}] {} ({}% - {}/{})", spinner, info.message, pct, info.current, info.total)
                };
                BitmapFont::draw_text(&mut self.pixmap.as_mut(), &phase_disp, (banner_x + 16.0) as i32, (banner_y + 12.0) as i32, 1, ColorRgba::new(255, 230, 90, 255));

                // Progress Bar
                let bar_x = banner_x + 16.0;
                let bar_y = banner_y + 36.0;
                let bar_w = banner_w - 32.0;
                let bar_h = 14.0;
                self.draw_rect(bar_x, bar_y, bar_w, bar_h, ColorRgba::new(12, 16, 26, 255));
                self.draw_rect(bar_x, bar_y, bar_w, 1.0, ColorRgba::new(40, 60, 90, 255));

                let ratio = (info.current as f32 / info.total.max(1) as f32).clamp(0.0, 1.0);
                let fill_w = bar_w * ratio;
                if fill_w > 0.0 {
                    self.draw_rect(bar_x, bar_y, fill_w, bar_h, ColorRgba::new(40, 180, 240, 255));
                }

                // Detail filename & Cancel text
                let detail_str = if info.detail.len() > 36 {
                    format!("...{}", &info.detail[info.detail.len() - 33..])
                } else {
                    info.detail.to_string()
                };
                BitmapFont::draw_text(&mut self.pixmap.as_mut(), &detail_str, (banner_x + 16.0) as i32, (banner_y + 60.0) as i32, 1, ColorRgba::new(170, 190, 215, 255));

                let cancel_hint = "[ESC] Cancel";
                BitmapFont::draw_text(&mut self.pixmap.as_mut(), cancel_hint, (banner_x + banner_w - 110.0) as i32, (banner_y + 60.0) as i32, 1, ColorRgba::new(255, 120, 120, 255));
            } else {
                let disp = format!("[{}] {}", spinner, info.message);
                BitmapFont::draw_text(&mut self.pixmap.as_mut(), &disp, (banner_x + 16.0) as i32, (banner_y + 16.0) as i32, 1, ColorRgba::new(255, 230, 90, 255));
                let cancel_hint = "[ESC] Cancel";
                BitmapFont::draw_text(&mut self.pixmap.as_mut(), cancel_hint, (banner_x + banner_w - 110.0) as i32, (banner_y + 16.0) as i32, 1, ColorRgba::new(255, 120, 120, 255));
            }
        }
    }
}

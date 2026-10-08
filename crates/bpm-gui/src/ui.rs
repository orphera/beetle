use crate::bitmap_font::BitmapFont;
use beetle_render::image::ImageBuffer;
use beetle_render::skin::ColorRgba;
use bms_package_manager::PackageRecord;
use tiny_skia::{Color, Paint, Pixmap, Rect, Shader, Transform};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Installed,
    OnlineHub,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemotePackageStatus {
    Available,
    Installed,
    UpdateAvailable,
}

#[derive(Debug, Clone)]
pub struct RemotePackageDisplayInfo {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub genre: Option<String>,
    pub bpm: Option<f64>,
    pub play_levels: Vec<u32>,
    pub size_bytes: u64,
    pub sha256: String,
    pub status: RemotePackageStatus,
    pub has_companion_bga: bool,
    pub download_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackModalOptionsDisplay {
    pub is_turbo: bool,
    pub bga_mode: bms_package_manager::BgaPackMode,
}

#[derive(Debug, Clone)]
pub struct ModalDisplayInfo<'a> {
    pub prompt: &'a str,
    pub input: &'a str,
    pub pack_options: Option<PackModalOptionsDisplay>,
    /// Extra lines under the hints (the legacy folder list).
    pub list: &'a [String],
}

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
        if width > 0
            && height > 0
            && (self.pixmap.width() != width || self.pixmap.height() != height)
        {
            if let Some(new_pixmap) = Pixmap::new(width, height) {
                self.pixmap = new_pixmap;
            }
        }
    }

    pub fn clear(&mut self, color: ColorRgba) {
        self.pixmap
            .fill(Color::from_rgba8(color.r, color.g, color.b, color.a));
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

    #[allow(clippy::too_many_arguments)]
    pub fn render_frame(
        &mut self,
        active_tab: ActiveTab,
        packages: &[&PackageRecord],
        selected_idx: usize,
        selected_ver_idx: usize,
        preview_img: Option<&ImageBuffer>,
        remote_packages: &[RemotePackageDisplayInfo],
        remote_selected_idx: usize,
        remote_level_filter: u8,
        search_query: &str,
        is_search_active: bool,
        status_msg: &str,
        modal_info: Option<ModalDisplayInfo>,
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
            "BEETLE BPM",
            20,
            16,
            2,
            ColorRgba::new(255, 220, 90, 255),
        );

        // Top Navigation Tabs
        let tab1_x = 210.0;
        let tab1_w = 140.0;
        let tab1_active = active_tab == ActiveTab::Installed;
        let tab1_bg = if tab1_active {
            ColorRgba::new(35, 45, 70, 255)
        } else {
            ColorRgba::new(20, 20, 28, 255)
        };
        let tab1_border = if tab1_active {
            ColorRgba::new(255, 220, 80, 255)
        } else {
            ColorRgba::new(50, 50, 70, 255)
        };
        let tab1_fg = if tab1_active {
            ColorRgba::new(255, 235, 120, 255)
        } else {
            ColorRgba::new(140, 140, 160, 255)
        };

        self.draw_rect(tab1_x, 14.0, tab1_w, 28.0, tab1_bg);
        self.draw_rect(tab1_x, 14.0, tab1_w, 1.0, tab1_border);
        self.draw_rect(tab1_x, 41.0, tab1_w, 1.0, tab1_border);
        self.draw_rect(tab1_x, 14.0, 1.0, 28.0, tab1_border);
        self.draw_rect(tab1_x + tab1_w - 1.0, 14.0, 1.0, 28.0, tab1_border);
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            "[1] Installed",
            (tab1_x + tab1_w / 2.0) as i32,
            22,
            1,
            tab1_fg,
        );

        let tab2_x = 360.0;
        let tab2_w = 150.0;
        let tab2_active = active_tab == ActiveTab::OnlineHub;
        let tab2_bg = if tab2_active {
            ColorRgba::new(20, 48, 70, 255)
        } else {
            ColorRgba::new(20, 20, 28, 255)
        };
        let tab2_border = if tab2_active {
            ColorRgba::new(80, 210, 255, 255)
        } else {
            ColorRgba::new(50, 50, 70, 255)
        };
        let tab2_fg = if tab2_active {
            ColorRgba::new(100, 225, 255, 255)
        } else {
            ColorRgba::new(140, 140, 160, 255)
        };

        self.draw_rect(tab2_x, 14.0, tab2_w, 28.0, tab2_bg);
        self.draw_rect(tab2_x, 14.0, tab2_w, 1.0, tab2_border);
        self.draw_rect(tab2_x, 41.0, tab2_w, 1.0, tab2_border);
        self.draw_rect(tab2_x, 14.0, 1.0, 28.0, tab2_border);
        self.draw_rect(tab2_x + tab2_w - 1.0, 14.0, 1.0, 28.0, tab2_border);
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            "[2] Online Hub",
            (tab2_x + tab2_w / 2.0) as i32,
            22,
            1,
            tab2_fg,
        );

        // Search Input Box
        let search_box_x = (w - 320.0).max(520.0);
        let s_w = w - search_box_x - 16.0;
        let search_border_col = if is_search_active {
            ColorRgba::new(255, 220, 80, 255)
        } else {
            ColorRgba::new(60, 60, 80, 255)
        };
        self.draw_rect(
            search_box_x,
            14.0,
            s_w,
            28.0,
            ColorRgba::new(16, 16, 24, 255),
        );
        self.draw_rect(search_box_x, 14.0, s_w, 1.0, search_border_col);
        self.draw_rect(search_box_x, 41.0, s_w, 1.0, search_border_col);
        self.draw_rect(search_box_x, 14.0, 1.0, 28.0, search_border_col);
        self.draw_rect(search_box_x + s_w - 1.0, 14.0, 1.0, 28.0, search_border_col);

        let search_display = if search_query.is_empty() {
            if is_search_active {
                "Type to search..._"
            } else {
                "Search (press [/])..."
            }
        } else {
            search_query
        };
        let search_text_col = if is_search_active {
            ColorRgba::new(255, 255, 255, 255)
        } else {
            ColorRgba::new(120, 120, 140, 255)
        };
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            search_display,
            (search_box_x + 10.0) as i32,
            22,
            1,
            search_text_col,
        );

        let content_y = 68.0;
        let content_h = h - content_y - 48.0;

        if active_tab == ActiveTab::Installed {
            // 3. Left Panel: Package List View
            let list_w = 420.0;

        self.draw_rect(
            16.0,
            content_y,
            list_w,
            content_h,
            ColorRgba::new(18, 18, 26, 255),
        );
        self.draw_rect(
            16.0,
            content_y,
            list_w,
            28.0,
            ColorRgba::new(26, 26, 38, 255),
        );

        let list_title = format!("INSTALLED PACKAGES ({})", packages.len());
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &list_title,
            26,
            (content_y + 8.0) as i32,
            1,
            ColorRgba::new(170, 170, 190, 255),
        );

        let row_h = 44.0;
        let max_visible_rows = ((content_h - 32.0) / row_h) as usize;
        let scroll_offset = if selected_idx >= max_visible_rows {
            selected_idx - max_visible_rows + 1
        } else {
            0
        };

        let mut row_y = content_y + 32.0;
        for (i, &pkg) in packages
            .iter()
            .skip(scroll_offset)
            .take(max_visible_rows)
            .enumerate()
        {
            let actual_idx = scroll_offset + i;
            let is_selected = actual_idx == selected_idx;

            if is_selected {
                self.draw_rect(
                    18.0,
                    row_y,
                    list_w - 4.0,
                    row_h - 2.0,
                    ColorRgba::new(35, 45, 70, 255),
                );
                self.draw_rect(
                    18.0,
                    row_y,
                    4.0,
                    row_h - 2.0,
                    ColorRgba::new(255, 210, 80, 255),
                );
            } else if actual_idx % 2 == 1 {
                self.draw_rect(
                    18.0,
                    row_y,
                    list_w - 4.0,
                    row_h - 2.0,
                    ColorRgba::new(22, 22, 30, 255),
                );
            }

            // Name
            // Name
            let name_col = if is_selected {
                ColorRgba::new(255, 255, 255, 255)
            } else {
                ColorRgba::new(210, 210, 225, 255)
            };
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &pkg.name,
                30,
                (row_y + 6.0) as i32,
                1,
                name_col,
            );

            // BGA Status Badge
            let (bga_tag, bga_bg, bga_fg) = match pkg.bga_status {
                bms_package_manager::BgaStatus::Embedded => (
                    "EMBED",
                    ColorRgba::new(20, 50, 40, 255),
                    ColorRgba::new(80, 220, 140, 255),
                ),
                bms_package_manager::BgaStatus::Companion => (
                    "COMPANION",
                    ColorRgba::new(20, 45, 75, 255),
                    ColorRgba::new(90, 190, 255, 255),
                ),
                bms_package_manager::BgaStatus::None => (
                    "NO-BGA",
                    ColorRgba::new(32, 32, 42, 255),
                    ColorRgba::new(130, 130, 150, 255),
                ),
            };
            let badge_w = 72.0;
            let badge_x = 18.0 + list_w - badge_w - 8.0;
            self.draw_rect(badge_x, row_y + 5.0, badge_w, 14.0, bga_bg);
            self.draw_rect(badge_x, row_y + 5.0, badge_w, 1.0, bga_fg);
            BitmapFont::draw_text_centered(
                &mut self.pixmap.as_mut(),
                bga_tag,
                (badge_x + badge_w / 2.0) as i32,
                (row_y + 8.0) as i32,
                1,
                bga_fg,
            );

            // ID & Author & State
            let author = pkg.author.as_deref().unwrap_or("Unknown");
            let short_active = if pkg.active_state.len() > 10 {
                &pkg.active_state[..10]
            } else {
                &pkg.active_state
            };
            let sub_info = format!(
                "{} | by {} | #{} ({} states)",
                pkg.id,
                author,
                short_active,
                pkg.state_hashes.len()
            );
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

        self.draw_rect(
            detail_x,
            content_y,
            detail_w,
            content_h,
            ColorRgba::new(18, 18, 26, 255),
        );
        self.draw_rect(
            detail_x,
            content_y,
            detail_w,
            28.0,
            ColorRgba::new(26, 26, 38, 255),
        );

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
                crate::image_draw::draw_scaled(
                    img,
                    &mut self.pixmap,
                    art_x as i32,
                    dy as i32,
                    art_w as u32,
                    art_h as u32,
                );
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
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &title_line,
                detail_x as i32 + 14,
                dy as i32,
                1,
                ColorRgba::new(240, 240, 250, 255),
            );
            dy += 20.0;

            let id_line = format!("ID:    {}", selected_pkg.id);
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &id_line,
                detail_x as i32 + 14,
                dy as i32,
                1,
                ColorRgba::new(180, 180, 200, 255),
            );
            dy += 20.0;

            let author_line = format!(
                "Author: {}",
                selected_pkg.author.as_deref().unwrap_or("Unknown")
            );
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &author_line,
                detail_x as i32 + 14,
                dy as i32,
                1,
                ColorRgba::new(180, 180, 200, 255),
            );
            dy += 20.0;

            let (bga_label, bga_col) = match selected_pkg.bga_status {
                bms_package_manager::BgaStatus::Embedded => (
                    "Embedded in package.bmsp (All-in-one)",
                    ColorRgba::new(80, 220, 140, 255),
                ),
                bms_package_manager::BgaStatus::Companion => (
                    "Decoupled Companion (.bga.bmsp installed)",
                    ColorRgba::new(90, 190, 255, 255),
                ),
                bms_package_manager::BgaStatus::None => (
                    "None (Audio & charts only)",
                    ColorRgba::new(150, 150, 170, 255),
                ),
            };
            let bga_line = format!("BGA:    {}", bga_label);
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &bga_line,
                detail_x as i32 + 14,
                dy as i32,
                1,
                bga_col,
            );
            dy += 20.0;

            if let Some(ref comp_path) = selected_pkg.bga_companion_path {
                let comp_line = format!("Path:   {}", comp_path);
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    &comp_line,
                    detail_x as i32 + 14,
                    dy as i32,
                    1,
                    ColorRgba::new(130, 150, 180, 255),
                );
                dy += 20.0;
            }
            dy += 6.0;

            // Installed States Management Box
            self.draw_rect(
                detail_x + 10.0,
                dy,
                detail_w - 20.0,
                1.0,
                ColorRgba::new(45, 45, 60, 255),
            );
            dy += 8.0;

            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                "Installed States (Use [<-/->] to select):",
                detail_x as i32 + 14,
                dy as i32,
                1,
                ColorRgba::new(255, 210, 80, 255),
            );
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

                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    &ver_tag,
                    detail_x as i32 + 20,
                    dy as i32,
                    1,
                    ver_col,
                );
                dy += 18.0;
            }

            dy += 12.0;

            // Actions box
            self.draw_rect(
                detail_x + 10.0,
                dy,
                detail_w - 20.0,
                1.0,
                ColorRgba::new(45, 45, 60, 255),
            );
            dy += 8.0;

            let action_text =
                if selected_pkg.bga_status == bms_package_manager::BgaStatus::Companion {
                    "[A]: Set Active   [U]/[Del]: Uninstall   [B]: Diet (Remove BGA)"
                } else {
                    "[A]: Set Active State   [U]/[Del]: Uninstall Selected State"
                };
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                action_text,
                detail_x as i32 + 14,
                dy as i32,
                1,
                ColorRgba::new(130, 170, 220, 255),
            );
        } // ends if let Some(&selected_pkg)
        } else {
            self.render_online_hub(
                w,
                h,
                content_y,
                content_h,
                remote_packages,
                remote_selected_idx,
                remote_level_filter,
            );
        }

        // 5. Bottom Status / Footer Bar
        let footer_y = h - 40.0;
        self.draw_rect(0.0, footer_y, w, 40.0, ColorRgba::new(16, 16, 24, 255));
        self.draw_rect(0.0, footer_y, w, 1.0, ColorRgba::new(35, 35, 50, 255));

        // Help shortcuts
        let help_text = if active_tab == ActiveTab::Installed {
            "[↑/↓]: Move  [I]: Import  [L]: Legacy folders  [P]: Pack  [T]: Turbo  [S]: Split BGA  [B]: Diet BGA  [F5]: Refresh  [Tab]: Online Hub"
        } else {
            "[↑/↓]: Move  [Enter]/[I]: Install  [U]: Upgrade  [B]: With BGA  [0-4]: Filter  [F5]: Refresh  [Tab]: Installed"
        };
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            help_text,
            16,
            (footer_y + 14.0) as i32,
            1,
            ColorRgba::new(160, 160, 180, 255),
        );

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
        if let Some(modal) = modal_info {
            let is_pack = modal.pack_options.is_some();
            let modal_w = if is_pack { 580.0 } else { 540.0 };
            let list_rows = modal.list.len().min(8) as f32;
            let modal_h = if is_pack {
                226.0
            } else if modal.list.is_empty() {
                160.0
            } else {
                160.0 + 8.0 + list_rows * 16.0
            };
            let modal_x = (w - modal_w) / 2.0;
            let modal_y = (h - modal_h) / 2.0;

            // Backdrop dimming
            self.draw_rect(0.0, 0.0, w, h, ColorRgba::new(0, 0, 0, 160));

            // Modal box
            self.draw_rect(
                modal_x,
                modal_y,
                modal_w,
                modal_h,
                ColorRgba::new(26, 26, 38, 255),
            );
            self.draw_rect(
                modal_x,
                modal_y,
                modal_w,
                2.0,
                ColorRgba::new(255, 210, 80, 255),
            );

            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                modal.prompt,
                (modal_x + 20.0) as i32,
                (modal_y + 18.0) as i32,
                1,
                ColorRgba::new(255, 255, 255, 255),
            );

            // Input line box
            let inp_box_y = modal_y + 46.0;
            self.draw_rect(
                modal_x + 20.0,
                inp_box_y,
                modal_w - 40.0,
                32.0,
                ColorRgba::new(16, 16, 24, 255),
            );
            self.draw_rect(
                modal_x + 20.0,
                inp_box_y,
                modal_w - 40.0,
                1.0,
                ColorRgba::new(80, 180, 255, 255),
            );

            let input_display = format!("{}_", modal.input);
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &input_display,
                (modal_x + 28.0) as i32,
                (inp_box_y + 10.0) as i32,
                1,
                ColorRgba::new(255, 255, 255, 255),
            );

            if let Some(pack_opts) = modal.pack_options {
                let opts_y = inp_box_y + 40.0;
                self.draw_rect(
                    modal_x + 20.0,
                    opts_y,
                    modal_w - 40.0,
                    1.0,
                    ColorRgba::new(45, 45, 65, 255),
                );

                // Turbo Option Row
                let turbo_check = if pack_opts.is_turbo { "[X]" } else { "[ ]" };
                let (turbo_label, turbo_col) = if pack_opts.is_turbo {
                    (
                        "Turbo Dual Atlas (Pre-decoded audio & GPU texture atlas)",
                        ColorRgba::new(255, 220, 80, 255),
                    )
                } else {
                    (
                        "Classic Packaging (Standard WAV/OGG files)",
                        ColorRgba::new(150, 150, 170, 255),
                    )
                };
                let turbo_line = format!("{} [Tab/F2]  Profile: {}", turbo_check, turbo_label);
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    &turbo_line,
                    (modal_x + 22.0) as i32,
                    (opts_y + 10.0) as i32,
                    1,
                    turbo_col,
                );

                // BGA Option Row
                let (bga_check, bga_label, bga_col) = match pack_opts.bga_mode {
                    bms_package_manager::BgaPackMode::Split => (
                        "[X]",
                        "Split BGA Companion (.bga.bmsp - diet friendly)",
                        ColorRgba::new(80, 200, 255, 255),
                    ),
                    bms_package_manager::BgaPackMode::Embed => (
                        "[ ]",
                        "Embed Video (All-in-one .bmsp)",
                        ColorRgba::new(130, 210, 150, 255),
                    ),
                    bms_package_manager::BgaPackMode::NoVideo => (
                        "[ ]",
                        "No Video (Pure audio/charts, minimal size)",
                        ColorRgba::new(160, 160, 180, 255),
                    ),
                };
                let bga_line = format!("{} [Ctrl+S/F3] BGA: {}", bga_check, bga_label);
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    &bga_line,
                    (modal_x + 22.0) as i32,
                    (opts_y + 30.0) as i32,
                    1,
                    bga_col,
                );

                // Combined Mode Badge
                let profile_tag = if pack_opts.is_turbo {
                    "TURBO DUAL ATLAS"
                } else {
                    "CLASSIC"
                };
                let bga_tag = match pack_opts.bga_mode {
                    bms_package_manager::BgaPackMode::Split => "SPLIT BGA COMPANION",
                    bms_package_manager::BgaPackMode::Embed => "EMBEDDED VIDEO",
                    bms_package_manager::BgaPackMode::NoVideo => "NO VIDEO",
                };
                let combo_disp = format!("Output: [{}] + [{}]", profile_tag, bga_tag);
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    &combo_disp,
                    (modal_x + 22.0) as i32,
                    (opts_y + 50.0) as i32,
                    1,
                    ColorRgba::new(255, 255, 255, 255),
                );

                // Hints line
                let hint_y = modal_y + modal_h - 22.0;
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    "[Enter]: Pack   [Tab]: Turbo   [Ctrl+S]: BGA Mode   [Ctrl+V]: Paste   [Esc]: Cancel",
                    (modal_x + 20.0) as i32,
                    hint_y as i32,
                    1,
                    ColorRgba::new(140, 150, 175, 255),
                );
            } else {
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    "[Enter]: Confirm   [Ctrl+V]: Paste   [Esc]: Cancel",
                    (modal_x + 20.0) as i32,
                    (modal_y + 118.0) as i32,
                    1,
                    ColorRgba::new(140, 140, 160, 255),
                );
                for (i, line) in modal.list.iter().take(8).enumerate() {
                    BitmapFont::draw_text(
                        &mut self.pixmap.as_mut(),
                        line,
                        (modal_x + 20.0) as i32,
                        (modal_y + 142.0 + i as f32 * 16.0) as i32,
                        1,
                        ColorRgba::new(190, 200, 225, 255),
                    );
                }
            }
        }

        // 7. Background Task Running Banner (if active)
        if let Some(info) = bg_task_info {
            let banner_w = (w - 60.0).min(520.0);
            let banner_h = if info.total > 0 { 86.0 } else { 48.0 };
            let banner_x = (w - banner_w) / 2.0;
            let banner_y = 66.0;

            self.draw_rect(
                banner_x,
                banner_y,
                banner_w,
                banner_h,
                ColorRgba::new(20, 28, 44, 250),
            );
            self.draw_rect(
                banner_x,
                banner_y,
                banner_w,
                2.0,
                ColorRgba::new(80, 180, 255, 255),
            );
            self.draw_rect(
                banner_x,
                banner_y + banner_h - 1.0,
                banner_w,
                1.0,
                ColorRgba::new(60, 140, 200, 255),
            );

            let spinner_chars = ['|', '/', '-', '\\'];
            let spinner = spinner_chars[info.spinner_frame % 4];

            if info.total > 0 {
                // Title & counts
                let pct = ((info.current as f32 / info.total.max(1) as f32) * 100.0) as u32;
                let phase_disp = if !info.phase.is_empty() {
                    format!(
                        "[{}] {} ({}% - {}/{})",
                        spinner, info.phase, pct, info.current, info.total
                    )
                } else {
                    format!(
                        "[{}] {} ({}% - {}/{})",
                        spinner, info.message, pct, info.current, info.total
                    )
                };
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    &phase_disp,
                    (banner_x + 16.0) as i32,
                    (banner_y + 12.0) as i32,
                    1,
                    ColorRgba::new(255, 230, 90, 255),
                );

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
                    self.draw_rect(
                        bar_x,
                        bar_y,
                        fill_w,
                        bar_h,
                        ColorRgba::new(40, 180, 240, 255),
                    );
                }

                // Detail filename & Cancel text
                let detail_str = if info.detail.len() > 36 {
                    format!("...{}", &info.detail[info.detail.len() - 33..])
                } else {
                    info.detail.to_string()
                };
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    &detail_str,
                    (banner_x + 16.0) as i32,
                    (banner_y + 60.0) as i32,
                    1,
                    ColorRgba::new(170, 190, 215, 255),
                );

                let cancel_hint = "[ESC] Cancel";
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    cancel_hint,
                    (banner_x + banner_w - 110.0) as i32,
                    (banner_y + 60.0) as i32,
                    1,
                    ColorRgba::new(255, 120, 120, 255),
                );
            } else {
                let disp = format!("[{}] {}", spinner, info.message);
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    &disp,
                    (banner_x + 16.0) as i32,
                    (banner_y + 16.0) as i32,
                    1,
                    ColorRgba::new(255, 230, 90, 255),
                );
                let cancel_hint = "[ESC] Cancel";
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    cancel_hint,
                    (banner_x + banner_w - 110.0) as i32,
                    (banner_y + 16.0) as i32,
                    1,
                    ColorRgba::new(255, 120, 120, 255),
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_online_hub(
        &mut self,
        w: f32,
        _h: f32,
        content_y: f32,
        content_h: f32,
        remote_packages: &[RemotePackageDisplayInfo],
        selected_idx: usize,
        level_filter: u8,
    ) {
        let list_w = 460.0_f32.min(w * 0.52);

        // Catalog Container
        self.draw_rect(
            16.0,
            content_y,
            list_w,
            content_h,
            ColorRgba::new(18, 18, 26, 255),
        );
        self.draw_rect(
            16.0,
            content_y,
            list_w,
            28.0,
            ColorRgba::new(26, 26, 38, 255),
        );

        let list_title = format!("ONLINE SONG HUB ({})", remote_packages.len());
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &list_title,
            26,
            (content_y + 8.0) as i32,
            1,
            ColorRgba::new(140, 200, 255, 255),
        );

        // Level Filter Bar
        let filter_y = content_y + 32.0;
        let filter_labels = ["[0] All", "[1] 1-4", "[2] 5-8", "[3] 9-11", "[4] 12+"];
        let pill_w = (list_w - 20.0) / 5.0;
        for (i, label) in filter_labels.iter().enumerate() {
            let px = 18.0 + (i as f32 * pill_w);
            let is_active_filter = level_filter == (i as u8);
            let pill_bg = if is_active_filter {
                ColorRgba::new(20, 50, 75, 255)
            } else {
                ColorRgba::new(24, 24, 34, 255)
            };
            let pill_border = if is_active_filter {
                ColorRgba::new(80, 210, 255, 255)
            } else {
                ColorRgba::new(45, 45, 60, 255)
            };
            let pill_fg = if is_active_filter {
                ColorRgba::new(100, 230, 255, 255)
            } else {
                ColorRgba::new(140, 140, 160, 255)
            };

            self.draw_rect(px, filter_y, pill_w - 2.0, 22.0, pill_bg);
            self.draw_rect(px, filter_y, pill_w - 2.0, 1.0, pill_border);
            self.draw_rect(px, filter_y + 21.0, pill_w - 2.0, 1.0, pill_border);
            self.draw_rect(px, filter_y, 1.0, 22.0, pill_border);
            self.draw_rect(px + pill_w - 3.0, filter_y, 1.0, 22.0, pill_border);
            BitmapFont::draw_text_centered(
                &mut self.pixmap.as_mut(),
                label,
                (px + (pill_w - 2.0) / 2.0) as i32,
                (filter_y + 6.0) as i32,
                1,
                pill_fg,
            );
        }

        // Virtual Scrolling Catalog List (Viewport Culling - INV-5)
        let catalog_y = filter_y + 26.0;
        let catalog_h = content_h - (catalog_y - content_y) - 6.0;
        let row_h = 50.0;
        let max_visible_rows = (catalog_h / row_h) as usize;
        let scroll_offset = if selected_idx >= max_visible_rows {
            selected_idx - max_visible_rows + 1
        } else {
            0
        };

        if remote_packages.is_empty() {
            BitmapFont::draw_text_centered(
                &mut self.pixmap.as_mut(),
                "No online packages found.",
                (16.0 + list_w / 2.0) as i32,
                (catalog_y + catalog_h / 2.0 - 10.0) as i32,
                1,
                ColorRgba::new(140, 140, 160, 255),
            );
            BitmapFont::draw_text_centered(
                &mut self.pixmap.as_mut(),
                "Press [F5] to sync remote registries or clear filter.",
                (16.0 + list_w / 2.0) as i32,
                (catalog_y + catalog_h / 2.0 + 10.0) as i32,
                1,
                ColorRgba::new(100, 100, 120, 255),
            );
        } else {
            for (i, pkg) in remote_packages
                .iter()
                .skip(scroll_offset)
                .take(max_visible_rows)
                .enumerate()
            {
                let actual_idx = scroll_offset + i;
                let is_selected = actual_idx == selected_idx;
                let row_y = catalog_y + (i as f32 * row_h);

                if is_selected {
                    self.draw_rect(
                        18.0,
                        row_y,
                        list_w - 4.0,
                        row_h - 2.0,
                        ColorRgba::new(25, 45, 75, 255),
                    );
                    self.draw_rect(
                        18.0,
                        row_y,
                        4.0,
                        row_h - 2.0,
                        ColorRgba::new(80, 210, 255, 255),
                    );
                } else if actual_idx % 2 == 1 {
                    self.draw_rect(
                        18.0,
                        row_y,
                        list_w - 4.0,
                        row_h - 2.0,
                        ColorRgba::new(22, 22, 30, 255),
                    );
                }

                // Title
                let title_col = if is_selected {
                    ColorRgba::new(255, 255, 255, 255)
                } else {
                    ColorRgba::new(220, 230, 245, 255)
                };
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    &pkg.title,
                    28,
                    (row_y + 6.0) as i32,
                    1,
                    title_col,
                );

                // Status Badge on right
                let (status_text, status_bg, status_border, status_fg) = match pkg.status {
                    RemotePackageStatus::Available => (
                        "[INSTALL]",
                        ColorRgba::new(15, 45, 55, 255),
                        ColorRgba::new(70, 200, 220, 255),
                        ColorRgba::new(90, 225, 245, 255),
                    ),
                    RemotePackageStatus::Installed => (
                        "[INSTALLED]",
                        ColorRgba::new(20, 45, 30, 255),
                        ColorRgba::new(60, 160, 100, 255),
                        ColorRgba::new(90, 220, 140, 255),
                    ),
                    RemotePackageStatus::UpdateAvailable => (
                        "[UPDATE AVAIL]",
                        ColorRgba::new(55, 45, 15, 255),
                        ColorRgba::new(230, 180, 40, 255),
                        ColorRgba::new(255, 210, 70, 255),
                    ),
                };
                let badge_w = 90.0;
                let badge_x = 18.0 + list_w - badge_w - 8.0;
                self.draw_rect(badge_x, row_y + 5.0, badge_w, 15.0, status_bg);
                self.draw_rect(badge_x, row_y + 5.0, badge_w, 1.0, status_border);
                self.draw_rect(badge_x, row_y + 19.0, badge_w, 1.0, status_border);
                self.draw_rect(badge_x, row_y + 5.0, 1.0, 15.0, status_border);
                self.draw_rect(badge_x + badge_w - 1.0, row_y + 5.0, 1.0, 15.0, status_border);
                BitmapFont::draw_text_centered(
                    &mut self.pixmap.as_mut(),
                    status_text,
                    (badge_x + badge_w / 2.0) as i32,
                    (row_y + 8.0) as i32,
                    1,
                    status_fg,
                );

                // Sub line 1: Artist, Genre, BPM
                let genre_str = pkg.genre.as_deref().unwrap_or("Unknown");
                let bpm_str = pkg
                    .bpm
                    .map(|b| format!("{:.0}", b))
                    .unwrap_or_else(|| "---".to_string());
                let sub_str = format!("{} | {} | BPM {}", pkg.artist, genre_str, bpm_str);
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    &sub_str,
                    28,
                    (row_y + 22.0) as i32,
                    1,
                    ColorRgba::new(135, 145, 165, 255),
                );

                // Sub line 2: Levels, Size, Companion BGA badge
                let levels_str = if pkg.play_levels.is_empty() {
                    "Lv: -".to_string()
                } else {
                    let lv_items: Vec<String> =
                        pkg.play_levels.iter().map(|l| l.to_string()).collect();
                    format!("Lv: {}", lv_items.join(", "))
                };
                let size_str = format_bytes(pkg.size_bytes);
                let bga_tag = if pkg.has_companion_bga {
                    " | [+BGA]"
                } else {
                    ""
                };
                let meta_line = format!("{} | {}{}", levels_str, size_str, bga_tag);
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    &meta_line,
                    28,
                    (row_y + 36.0) as i32,
                    1,
                    ColorRgba::new(110, 125, 145, 255),
                );
            }
        }

        // Right Panel: Remote Package Details
        let detail_x = 16.0 + list_w + 16.0;
        let detail_w = w - detail_x - 16.0;

        self.draw_rect(
            detail_x,
            content_y,
            detail_w,
            content_h,
            ColorRgba::new(18, 18, 26, 255),
        );
        self.draw_rect(
            detail_x,
            content_y,
            detail_w,
            28.0,
            ColorRgba::new(26, 26, 38, 255),
        );

        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "ONLINE SONG DETAILS",
            detail_x as i32 + 12,
            (content_y + 8.0) as i32,
            1,
            ColorRgba::new(170, 170, 190, 255),
        );

        if let Some(selected_pkg) = remote_packages.get(selected_idx) {
            let mut dy = content_y + 40.0;

            // Title (Header)
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &selected_pkg.title,
                detail_x as i32 + 16,
                dy as i32,
                2,
                ColorRgba::new(255, 255, 255, 255),
            );
            dy += 28.0;

            // Artist
            let artist_str = format!("Artist: {}", selected_pkg.artist);
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &artist_str,
                detail_x as i32 + 16,
                dy as i32,
                1,
                ColorRgba::new(180, 200, 230, 255),
            );
            dy += 18.0;

            // Genre & BPM
            let genre_bpm = format!(
                "Genre: {}   |   BPM: {}",
                selected_pkg.genre.as_deref().unwrap_or("Unknown"),
                selected_pkg
                    .bpm
                    .map(|b| format!("{:.1}", b))
                    .unwrap_or_else(|| "Variable".to_string())
            );
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &genre_bpm,
                detail_x as i32 + 16,
                dy as i32,
                1,
                ColorRgba::new(150, 165, 185, 255),
            );
            dy += 22.0;

            // Status Banner Box
            let (status_banner, banner_bg, banner_border, banner_fg) = match selected_pkg.status {
                RemotePackageStatus::Available => (
                    "STATUS: AVAILABLE FOR DOWNLOAD",
                    ColorRgba::new(18, 42, 52, 255),
                    ColorRgba::new(60, 180, 210, 255),
                    ColorRgba::new(90, 220, 250, 255),
                ),
                RemotePackageStatus::Installed => (
                    "STATUS: INSTALLED & UP TO DATE",
                    ColorRgba::new(20, 45, 30, 255),
                    ColorRgba::new(60, 170, 95, 255),
                    ColorRgba::new(90, 230, 140, 255),
                ),
                RemotePackageStatus::UpdateAvailable => (
                    "STATUS: UPDATE AVAILABLE! (Newer state in registry)",
                    ColorRgba::new(55, 45, 15, 255),
                    ColorRgba::new(230, 180, 40, 255),
                    ColorRgba::new(255, 215, 75, 255),
                ),
            };
            self.draw_rect(detail_x + 14.0, dy, detail_w - 28.0, 26.0, banner_bg);
            self.draw_rect(detail_x + 14.0, dy, detail_w - 28.0, 1.0, banner_border);
            self.draw_rect(detail_x + 14.0, dy + 25.0, detail_w - 28.0, 1.0, banner_border);
            self.draw_rect(detail_x + 14.0, dy, 1.0, 26.0, banner_border);
            self.draw_rect(detail_x + detail_w - 15.0, dy, 1.0, 26.0, banner_border);
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                status_banner,
                detail_x as i32 + 26,
                (dy + 8.0) as i32,
                1,
                banner_fg,
            );
            dy += 36.0;

            // Details section
            let id_str = format!("Package ID:   {}", selected_pkg.id);
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &id_str,
                detail_x as i32 + 16,
                dy as i32,
                1,
                ColorRgba::new(200, 205, 220, 255),
            );
            dy += 18.0;

            let size_str = format!(
                "Package Size: {} ({} bytes)",
                format_bytes(selected_pkg.size_bytes),
                selected_pkg.size_bytes
            );
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &size_str,
                detail_x as i32 + 16,
                dy as i32,
                1,
                ColorRgba::new(170, 180, 195, 255),
            );
            dy += 18.0;

            let short_hash = if selected_pkg.sha256.len() > 16 {
                format!("{}...", &selected_pkg.sha256[..16])
            } else {
                selected_pkg.sha256.clone()
            };
            let hash_str = format!("SHA-256:      {}", short_hash);
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &hash_str,
                detail_x as i32 + 16,
                dy as i32,
                1,
                ColorRgba::new(150, 160, 180, 255),
            );
            dy += 18.0;

            let bga_str = if selected_pkg.has_companion_bga {
                "Companion BGA: Available (.bga.bmsp ready to split/diet)"
            } else {
                "Companion BGA: None (Audio and charts only)"
            };
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                bga_str,
                detail_x as i32 + 16,
                dy as i32,
                1,
                if selected_pkg.has_companion_bga {
                    ColorRgba::new(100, 200, 255, 255)
                } else {
                    ColorRgba::new(130, 135, 150, 255)
                },
            );
            dy += 18.0;

            let short_url = if selected_pkg.download_url.len() > 36 {
                format!(
                    "...{}",
                    &selected_pkg.download_url[selected_pkg.download_url.len() - 33..]
                )
            } else {
                selected_pkg.download_url.clone()
            };
            let url_str = format!("Remote URL:    {}", short_url);
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &url_str,
                detail_x as i32 + 16,
                dy as i32,
                1,
                ColorRgba::new(125, 140, 165, 255),
            );
            dy += 18.0;

            let levels_detail = if selected_pkg.play_levels.is_empty() {
                "Play Levels:   None specified".to_string()
            } else {
                let lv_str: Vec<String> = selected_pkg
                    .play_levels
                    .iter()
                    .map(|l| format!("Lv.{}", l))
                    .collect();
                format!("Play Levels:   {}", lv_str.join("  "))
            };
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &levels_detail,
                detail_x as i32 + 16,
                dy as i32,
                1,
                ColorRgba::new(240, 210, 120, 255),
            );
            dy += 24.0;

            // Actions Box at bottom of detail panel
            let actions_box_y = (content_y + content_h - 76.0).max(dy + 10.0);
            self.draw_rect(
                detail_x + 10.0,
                actions_box_y,
                detail_w - 20.0,
                1.0,
                ColorRgba::new(45, 45, 60, 255),
            );

            let action_main = match selected_pkg.status {
                RemotePackageStatus::UpdateAvailable => {
                    "[U] / [Enter]: Upgrade to Latest Version"
                }
                RemotePackageStatus::Available => "[I] / [Enter]: 1-Click Download & Install",
                RemotePackageStatus::Installed => "[Enter]: Re-download / Reinstall Package",
            };
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                action_main,
                detail_x as i32 + 14,
                (actions_box_y + 10.0) as i32,
                1,
                ColorRgba::new(100, 230, 255, 255),
            );

            let action_sub = if selected_pkg.has_companion_bga {
                "[B]: Install with BGA   [0-4]: Filter   [F5]: Sync Sources"
            } else {
                "[0-4]: Filter   [F5]: Sync Sources   [Tab]: Installed"
            };
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                action_sub,
                detail_x as i32 + 14,
                (actions_box_y + 28.0) as i32,
                1,
                ColorRgba::new(150, 160, 180, 255),
            );
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

use crate::bitmap_font::BitmapFont;
use crate::image::ImageBuffer;
use crate::renderer::{truncate_str, SoftwareRenderer};
use crate::skin::ColorRgba;

impl SoftwareRenderer {
    /// Renders the option modal overlay centered on the screen.
    pub fn render_option_modal(
        &mut self,
        options: &beetle_core::PlayOptions,
        key_preset_str: &str,
        is_auto_play: bool,
        start_measure: u32,
        master_volume: f32,
        display_mode_str: &str,
        resolution_str: &str,
        gpu_backend_str: &str,
        target_fps: u32,
        selected_row: usize,
    ) {
        let vp = self.viewport;
        let s = vp.scale;
        let font_scale = (s * 0.9).round().max(1.0) as u32;

        let modal_w = 500.0 * s;
        let modal_h = (480.0 * s).min(vp.height - 40.0 * s);
        let modal_x = vp.x + (vp.width - modal_w) / 2.0;
        let modal_y = vp.y + (vp.height - modal_h) / 2.0;

        // Background shadow / dim overlay (draw dark background)
        self.draw_rect(modal_x, modal_y, modal_w, modal_h, ColorRgba::new(12, 14, 20, 255));

        // Glowing border
        self.draw_rect(modal_x, modal_y, modal_w, 2.0 * s, ColorRgba::new(80, 140, 255, 255));
        self.draw_rect(modal_x, modal_y + modal_h - 2.0 * s, modal_w, 2.0 * s, ColorRgba::new(80, 140, 255, 255));
        self.draw_rect(modal_x, modal_y, 2.0 * s, modal_h, ColorRgba::new(80, 140, 255, 255));
        self.draw_rect(modal_x + modal_w - 2.0 * s, modal_y, 2.0 * s, modal_h, ColorRgba::new(80, 140, 255, 255));

        // Header Title
        let center_x = (vp.x + vp.width / 2.0) as i32;
        let title_scale = (2.0 * s).round().max(1.0) as u32;
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            "PLAY OPTIONS",
            center_x,
            (modal_y + 14.0 * s) as i32,
            title_scale,
            ColorRgba::new(255, 255, 255, 255),
        );

        let fps_str = if target_fps == 0 {
            "<  UNLIMITED  >".to_string()
        } else {
            format!("<  {} FPS  >", target_fps)
        };

        let rows = [
            ("HI-SPEED", format!("<  {:.0} px/s  >", options.hi_speed)),
            ("MODIFIER", format!("<  {}  >", options.lane_modifier.as_str())),
            ("GAUGE", format!("<  {}  >", options.gauge_type.as_str())),
            ("JUDGE OFFSET", format!("<  {:+.0} ms  >", options.judge_offset_ms)),
            ("MASTER VOLUME", format!("<  {:.0}%  >", master_volume * 100.0)),
            ("DISPLAY MODE", format!("<  {}  >", display_mode_str)),
            ("RESOLUTION", format!("<  {}  >", resolution_str)),
            ("GRAPHICS GPU", format!("<  {}  >", gpu_backend_str)),
            ("TARGET FPS", fps_str),
            ("KEY LAYOUT", format!("<  {}  >", key_preset_str)),
            ("AUTO PLAY", if is_auto_play { "<  ON  >".to_string() } else { "<  OFF  >".to_string() }),
            ("START MEASURE", format!("<  M.{}  >", start_measure)),
        ];

        let row_step = 28.0 * s;
        let mut row_y = (modal_y + 46.0 * s) as i32;
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
                self.draw_rect(modal_x + 16.0 * s, row_y as f32 - 3.0 * s, modal_w - 32.0 * s, 24.0 * s, bg);
            }

            BitmapFont::draw_text(&mut self.pixmap.as_mut(), label, (modal_x + 28.0 * s) as i32, row_y, font_scale, text_color);
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), val, (modal_x + 240.0 * s) as i32, row_y, font_scale, if is_sel { ColorRgba::new(255, 230, 80, 255) } else { text_color });

            row_y += row_step as i32;
        }

        // Instructions Footer
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            "[Up/Down]: Select   [Left/Right]: Change   [Tab/Esc]: Close",
            center_x,
            (modal_y + modal_h - 22.0 * s) as i32,
            font_scale,
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
        let vp = self.viewport;
        let s = vp.scale;
        let font_scale = (s * 0.9).round().max(1.0) as u32;

        // 1. Semi-transparent dark overlay dimming the whole window
        self.draw_rect(0.0, 0.0, w, h, ColorRgba::new(0, 0, 0, 190));

        // 2. Center Glassmorphic Modal Box
        let modal_w = 420.0 * s;
        let modal_h = 300.0 * s;
        let modal_x = vp.x + (vp.width - modal_w) / 2.0;
        let modal_y = vp.y + (vp.height - modal_h) / 2.0;

        self.draw_rect(modal_x, modal_y, modal_w, modal_h, ColorRgba::new(16, 20, 32, 255));
        self.draw_rect(modal_x, modal_y, modal_w, 1.0 * s, ColorRgba::new(80, 180, 255, 255));
        self.draw_rect(modal_x, modal_y + modal_h - 1.0 * s, modal_w, 1.0 * s, ColorRgba::new(80, 180, 255, 255));
        self.draw_rect(modal_x, modal_y, 1.0 * s, modal_h, ColorRgba::new(80, 180, 255, 255));
        self.draw_rect(modal_x + modal_w - 1.0 * s, modal_y, 1.0 * s, modal_h, ColorRgba::new(80, 180, 255, 255));

        // 3. Pause Header
        let center_x = (vp.x + vp.width / 2.0) as i32;
        let mut cur_y = modal_y + 20.0 * s;
        let title_scale = (2.0 * s).round().max(1.0) as u32;
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            "GAME PAUSED",
            center_x,
            cur_y as i32,
            title_scale,
            ColorRgba::new(255, 220, 80, 255),
        );
        cur_y += 32.0 * s;

        // Song Title & Artist
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            &truncate_str(title, 26),
            center_x,
            cur_y as i32,
            font_scale,
            ColorRgba::new(255, 255, 255, 255),
        );
        cur_y += 18.0 * s;

        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            &truncate_str(artist, 28),
            center_x,
            cur_y as i32,
            font_scale,
            ColorRgba::new(160, 170, 195, 255),
        );
        cur_y += 24.0 * s;

        // Progress Bar
        let bar_w = modal_w - 60.0 * s;
        let bar_x = modal_x + 30.0 * s;
        let bar_h = 6.0 * s;
        let ratio = if total_time_sec > 0.0 {
            (current_time_sec / total_time_sec).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };

        self.draw_rect(bar_x, cur_y, bar_w, bar_h, ColorRgba::new(30, 36, 52, 255));
        self.draw_rect(bar_x, cur_y, bar_w * ratio, bar_h, ColorRgba::new(80, 210, 255, 255));
        cur_y += bar_h + 8.0 * s;

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
            font_scale,
            ColorRgba::new(140, 150, 175, 255),
        );
        cur_y += 26.0 * s;

        // Menu Options
        let menu_items = [
            ("RESUME", "Continue playing"),
            ("RESTART", "Retry from start (R)"),
            ("SELECT SONG", "Quit to song select (Esc)"),
        ];

        let item_w = modal_w - 40.0 * s;
        let item_x = modal_x + 20.0 * s;
        let item_h = 32.0 * s;

        for (idx, (label, sub)) in menu_items.iter().enumerate() {
            let is_sel = idx == selected_option;
            let item_y = cur_y;

            if is_sel {
                self.draw_rect(item_x, item_y, item_w, item_h, ColorRgba::new(35, 65, 135, 255));
                self.draw_rect(item_x, item_y, item_w, 1.0 * s, ColorRgba::new(90, 190, 255, 255));
                self.draw_rect(item_x, item_y + item_h - 1.0 * s, item_w, 1.0 * s, ColorRgba::new(90, 190, 255, 255));
                self.draw_rect(item_x, item_y, 1.0 * s, item_h, ColorRgba::new(90, 190, 255, 255));
                self.draw_rect(item_x + item_w - 1.0 * s, item_y, 1.0 * s, item_h, ColorRgba::new(90, 190, 255, 255));
            } else {
                self.draw_rect(item_x, item_y, item_w, item_h, ColorRgba::new(20, 25, 38, 200));
                self.draw_rect(item_x, item_y, item_w, 1.0 * s, ColorRgba::new(40, 48, 68, 255));
                self.draw_rect(item_x, item_y + item_h - 1.0 * s, item_w, 1.0 * s, ColorRgba::new(40, 48, 68, 255));
            }

            let text_col = if is_sel { ColorRgba::new(255, 255, 255, 255) } else { ColorRgba::new(180, 190, 215, 255) };
            let sub_col = if is_sel { ColorRgba::new(140, 210, 255, 255) } else { ColorRgba::new(100, 110, 135, 255) };

            BitmapFont::draw_text(&mut self.pixmap.as_mut(), label, (item_x + 16.0 * s) as i32, (item_y + 8.0 * s) as i32, font_scale, text_col);
            let sub_x = (item_x + item_w - BitmapFont::text_width(sub, font_scale) as f32 - 16.0 * s) as i32;
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), sub, sub_x, (item_y + 8.0 * s) as i32, font_scale, sub_col);

            cur_y += item_h + 8.0 * s;
        }
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

        let vp = self.viewport;
        let s = vp.scale;
        let font_scale = (s * 0.9).round().max(1.0) as u32;
        let center_x = (vp.x + vp.width / 2.0) as i32;

        // Background subtle grid/lines inside viewport
        let mut line_y = vp.y;
        while line_y < vp.y + vp.height {
            self.draw_rect(vp.x, line_y, vp.width, 1.0 * s, ColorRgba::new(20, 20, 30, 255));
            line_y += 24.0 * s;
        }

        let mut y = vp.y + 50.0 * s;

        // Top Header
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            "PREPARING TRACK",
            center_x,
            y as i32,
            font_scale,
            ColorRgba::new(120, 160, 220, 255),
        );
        y += 32.0 * s;

        // Stage Image / Artwork Box
        let art_w = (vp.width * 0.45).clamp(240.0 * s, 480.0 * s);
        let art_h = art_w * (9.0 / 16.0);
        let art_x = vp.x + (vp.width - art_w) / 2.0;

        self.draw_rect(art_x - 2.0 * s, y - 2.0 * s, art_w + 4.0 * s, art_h + 4.0 * s, ColorRgba::new(50, 60, 90, 255));
        self.draw_rect(art_x, y, art_w, art_h, ColorRgba::new(12, 12, 18, 255));

        if let Some(img) = stage_image {
            img.draw_fitted(&mut self.pixmap, art_x as i32, y as i32, art_w as u32, art_h as u32, crate::image::ImageFitMode::FillCrop);
        } else {
            BitmapFont::draw_text_centered(
                &mut self.pixmap.as_mut(),
                "[ NO STAGE IMAGE ]",
                center_x,
                (y + art_h / 2.0 - 4.0 * s) as i32,
                font_scale,
                ColorRgba::new(70, 70, 90, 255),
            );
        }
        y += art_h + 20.0 * s;

        // Track Title
        let title_scale = (2.0 * s).round().max(1.0) as u32;
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            title,
            center_x,
            y as i32,
            title_scale,
            ColorRgba::new(255, 255, 255, 255),
        );
        y += 28.0 * s;

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
            font_scale,
            ColorRgba::new(170, 180, 210, 255),
        );
        y += 38.0 * s;

        // Animated Loading Progress Indicator
        let bar_w = 320.0 * s;
        let bar_x = vp.x + (vp.width - bar_w) / 2.0;
        self.draw_rect(bar_x, y, bar_w, 4.0 * s, ColorRgba::new(30, 30, 45, 255));

        // Pulsing / moving progress highlight
        let pulse_pos = ((spinner_frame * 12) % (bar_w as usize).max(1)) as f32;
        let seg_w = 60.0 * s;
        let seg_x = (bar_x + pulse_pos).min(bar_x + bar_w - seg_w);
        self.draw_rect(seg_x, y, seg_w, 4.0 * s, ColorRgba::new(80, 200, 255, 255));
        y += 18.0 * s;

        let spinner_chars = ['|', '/', '-', '\\'];
        let spinner = spinner_chars[spinner_frame % 4];
        let loading_disp = format!("[{}] {}", spinner, progress_msg);
        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            &loading_disp,
            center_x,
            y as i32,
            font_scale,
            ColorRgba::new(255, 220, 90, 255),
        );
    }
}

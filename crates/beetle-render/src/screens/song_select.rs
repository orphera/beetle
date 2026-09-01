use crate::bitmap_font::BitmapFont;
use crate::image::ImageBuffer;
use crate::renderer::{accuracy_to_rank, clear_lamp_color, level_color, truncate_str, SoftwareRenderer};
use crate::skin::ColorRgba;

impl SoftwareRenderer {
    /// Renders the song select screen with list and metadata panel.
    pub fn render_song_select(
        &mut self,
        songs: &[beetle_core::SongMetadata],
        selected_idx: usize,
        score_store: &beetle_core::ScoreStore,
        sort_mode_str: &str,
        category_str: &str,
        search_query: &str,
        is_search_active: bool,
        stage_image: Option<&ImageBuffer>,
        total_library_count: usize,
    ) {
        self.clear();

        let vp = self.viewport;
        let s = vp.scale;
        let font_scale = (s * 0.9).round().max(1.0) as u32;

        // 1. Top Header Bar
        self.draw_rect(vp.x, vp.y, vp.width, 54.0 * s, ColorRgba::new(14, 16, 26, 255));
        self.draw_rect(vp.x, vp.y + 53.0 * s, vp.width, 1.0 * s, ColorRgba::new(45, 60, 95, 255));

        // Header Title
        let title_scale = (2.0 * s).round().max(1.0) as u32;
        BitmapFont::draw_text_with_shadow(
            &mut self.pixmap.as_mut(),
            "BEETLE BMS PLAYER",
            (vp.x + 24.0 * s) as i32,
            (vp.y + 12.0 * s) as i32,
            title_scale,
            ColorRgba::new(255, 255, 255, 255),
            ColorRgba::new(10, 10, 20, 255),
            1,
            1,
        );

        // Category & Sort Badges
        let folder_badge_text = format!("FOLDER: {}", category_str);
        let badge_x = (vp.x + 240.0 * s) as i32;
        BitmapFont::draw_badge(
            &mut self.pixmap.as_mut(),
            &folder_badge_text,
            badge_x,
            (vp.y + 14.0 * s) as i32,
            font_scale,
            ColorRgba::new(80, 220, 255, 255),
            ColorRgba::new(20, 35, 60, 255),
            ColorRgba::new(60, 140, 240, 255),
            (8.0 * s) as i32,
            (4.0 * s) as i32,
        );

        let sort_badge_text = format!("SORT: {}", sort_mode_str);
        let sort_x = badge_x + BitmapFont::text_width(&folder_badge_text, font_scale) as i32 + (20.0 * s) as i32;
        BitmapFont::draw_badge(
            &mut self.pixmap.as_mut(),
            &sort_badge_text,
            sort_x,
            (vp.y + 14.0 * s) as i32,
            font_scale,
            ColorRgba::new(255, 220, 90, 255),
            ColorRgba::new(45, 40, 20, 255),
            ColorRgba::new(180, 150, 40, 255),
            (8.0 * s) as i32,
            (4.0 * s) as i32,
        );

        // Search Input Box (Top Right)
        let search_box_w = 260.0 * s;
        let search_box_x = (vp.x + vp.width - search_box_w - 24.0 * s).max(sort_x as f32 + 120.0 * s);
        let search_border_col = if is_search_active {
            ColorRgba::new(80, 210, 255, 255)
        } else {
            ColorRgba::new(50, 55, 75, 255)
        };

        self.draw_rect(search_box_x, vp.y + 11.0 * s, search_box_w, 32.0 * s, ColorRgba::new(22, 24, 36, 255));
        self.draw_rect(search_box_x, vp.y + 11.0 * s, search_box_w, 1.0 * s, search_border_col);
        self.draw_rect(search_box_x, vp.y + 42.0 * s, search_box_w, 1.0 * s, search_border_col);
        self.draw_rect(search_box_x, vp.y + 11.0 * s, 1.0 * s, 32.0 * s, search_border_col);
        self.draw_rect(search_box_x + search_box_w - 1.0 * s, vp.y + 11.0 * s, 1.0 * s, 32.0 * s, search_border_col);

        let search_disp = if search_query.is_empty() && !is_search_active {
            "[/] Search title/artist...".to_string()
        } else {
            format!("Search: {}{}", search_query, if is_search_active { "_" } else { "" })
        };
        let search_text_col = if is_search_active {
            ColorRgba::new(255, 255, 255, 255)
        } else {
            ColorRgba::new(140, 145, 170, 255)
        };
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &search_disp,
            search_box_x as i32 + (10.0 * s) as i32,
            (vp.y + 18.0 * s) as i32,
            font_scale,
            search_text_col,
        );

        let total_songs = songs.len();
        if total_songs == 0 {
            let msg = if !search_query.is_empty() {
                format!("No songs match search query: \"{}\"", search_query)
            } else {
                "No BMS songs found in songs/ directory.".to_string()
            };
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), &msg, (vp.x + 40.0 * s) as i32, (vp.y + 100.0 * s) as i32, font_scale, ColorRgba::new(220, 200, 200, 255));
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                "Press [Esc] or [/] to reset search, or place .bms files into songs/ directory.",
                (vp.x + 40.0 * s) as i32,
                (vp.y + 130.0 * s) as i32,
                font_scale,
                ColorRgba::new(140, 145, 170, 255),
            );
            return;
        }

        // 2. Left Song List Panel (Carousel view)
        let list_x = (vp.x + 24.0 * s) as i32;
        let mut list_y = (vp.y + 68.0 * s) as i32;
        let list_w = ((vp.width * 0.50).min(650.0 * s)).max(340.0 * s);
        let card_step = (34.0 * s) as i32;
        let max_visible = (((vp.height - 120.0 * s) / card_step as f32).max(6.0)) as usize;

        let start_idx = if selected_idx >= max_visible / 2 {
            (selected_idx + 1).saturating_sub(max_visible / 2).min(total_songs.saturating_sub(max_visible))
        } else {
            0
        };
        let end_idx = (start_idx + max_visible).min(total_songs);

        for i in start_idx..end_idx {
            let song = &songs[i];
            let is_selected = i == selected_idx;

            let best_record = score_store.get(song.hash);
            let (lamp_str, lamp_color) = clear_lamp_color(best_record.map(|b| b.clear_type));
            let lvl_color = level_color(song.play_level);

            // Card Dimensions
            let card_y = list_y as f32;
            let card_h = (30.0 * s).max(24.0);
            let card_w = if is_selected { list_w + 12.0 * s } else { list_w };

            // Card Background & Glowing Border
            if is_selected {
                self.draw_rect(list_x as f32, card_y, card_w, card_h, ColorRgba::new(32, 54, 110, 255));
                self.draw_rect(list_x as f32, card_y, card_w, 1.0 * s, ColorRgba::new(90, 190, 255, 255));
                self.draw_rect(list_x as f32, card_y + card_h - 1.0 * s, card_w, 1.0 * s, ColorRgba::new(90, 190, 255, 255));
                self.draw_rect(list_x as f32 + card_w - 1.0 * s, card_y, 1.0 * s, card_h, ColorRgba::new(90, 190, 255, 255));
            } else {
                let bg_col = if i % 2 == 0 { ColorRgba::new(16, 18, 26, 220) } else { ColorRgba::new(20, 22, 32, 220) };
                self.draw_rect(list_x as f32, card_y, card_w, card_h, bg_col);
                self.draw_rect(list_x as f32, card_y, card_w, 1.0 * s, ColorRgba::new(35, 40, 55, 255));
                self.draw_rect(list_x as f32, card_y + card_h - 1.0 * s, card_w, 1.0 * s, ColorRgba::new(35, 40, 55, 255));
            }

            // Left Clear Lamp Bar
            self.draw_rect(list_x as f32, card_y, 5.0 * s, card_h, lamp_color);

            // Level Pill Badge
            let lvl_text = format!("LV.{:>2}", song.play_level);
            BitmapFont::draw_badge(
                &mut self.pixmap.as_mut(),
                &lvl_text,
                list_x + (12.0 * s) as i32,
                list_y + (4.0 * s) as i32,
                font_scale,
                ColorRgba::new(255, 255, 255, 255),
                ColorRgba::new(20, 22, 30, 255),
                lvl_color,
                (4.0 * s) as i32,
                (2.0 * s) as i32,
            );

            // Song Title (Multilingual BitmapFont!)
            let title_x = list_x + (76.0 * s) as i32;
            let title_color = if is_selected {
                ColorRgba::new(255, 255, 255, 255)
            } else {
                ColorRgba::new(190, 195, 215, 255)
            };

            let truncated_title = truncate_str(&song.title, 26);
            if is_selected {
                BitmapFont::draw_text_with_shadow(
                    &mut self.pixmap.as_mut(),
                    &truncated_title,
                    title_x,
                    list_y + (7.0 * s) as i32,
                    font_scale,
                    title_color,
                    ColorRgba::new(10, 15, 30, 255),
                    1,
                    1,
                );
            } else {
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    &truncated_title,
                    title_x,
                    list_y + (7.0 * s) as i32,
                    font_scale,
                    title_color,
                );
            }

            // Right Mini Lamp Status Tag
            let status_x = (list_x as f32 + card_w - 78.0 * s) as i32;
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                lamp_str,
                status_x,
                list_y + (8.0 * s) as i32,
                font_scale,
                lamp_color,
            );

            list_y += card_step;
        }

        // List Scrollbar
        let scroll_track_x = list_x as f32 + list_w + 16.0 * s;
        let scroll_track_y = vp.y + 68.0 * s;
        let scroll_track_h = (max_visible as i32 * card_step) as f32;
        self.draw_rect(scroll_track_x, scroll_track_y, 4.0 * s, scroll_track_h, ColorRgba::new(25, 30, 45, 255));

        if total_songs > 0 {
            let thumb_h = ((max_visible as f32 / total_songs as f32) * scroll_track_h).clamp(16.0 * s, scroll_track_h);
            let thumb_y = scroll_track_y + (selected_idx as f32 / total_songs as f32) * (scroll_track_h - thumb_h);
            self.draw_rect(scroll_track_x, thumb_y, 4.0 * s, thumb_h, ColorRgba::new(80, 180, 255, 255));
        }

        // 3. Right Song Detail Card
        let detail_x = scroll_track_x + 18.0 * s;
        let detail_y = vp.y + 68.0 * s;
        let detail_w = (vp.x + vp.width - detail_x - 24.0 * s).max(280.0 * s);
        let detail_h = (vp.height - 110.0 * s).max(440.0 * s);

        // Detail Glass Panel
        self.draw_rect(detail_x, detail_y, detail_w, detail_h, ColorRgba::new(14, 16, 26, 255));
        self.draw_rect(detail_x, detail_y, detail_w, 1.0 * s, ColorRgba::new(45, 55, 80, 255));
        self.draw_rect(detail_x, detail_y + detail_h - 1.0 * s, detail_w, 1.0 * s, ColorRgba::new(45, 55, 80, 255));
        self.draw_rect(detail_x, detail_y, 1.0 * s, detail_h, ColorRgba::new(45, 55, 80, 255));
        self.draw_rect(detail_x + detail_w - 1.0 * s, detail_y, 1.0 * s, detail_h, ColorRgba::new(45, 55, 80, 255));

        if let Some(selected_song) = songs.get(selected_idx) {
            let mut cur_y = detail_y + 16.0 * s;

            // Artwork Frame
            let art_w = detail_w - 32.0 * s;
            let art_h = (art_w * 9.0 / 16.0).clamp(100.0 * s, 180.0 * s);
            let art_x = detail_x + 16.0 * s;

            self.draw_rect(art_x - 1.0 * s, cur_y - 1.0 * s, art_w + 2.0 * s, art_h + 2.0 * s, ColorRgba::new(60, 80, 120, 255));
            self.draw_rect(art_x, cur_y, art_w, art_h, ColorRgba::new(10, 12, 18, 255));

            if let Some(img) = stage_image {
                img.draw_fitted(&mut self.pixmap, art_x as i32, cur_y as i32, art_w as u32, art_h as u32, crate::image::ImageFitMode::FillCrop);
            } else {
                BitmapFont::draw_text_centered(
                    &mut self.pixmap.as_mut(),
                    "[ STAGE IMAGE ]",
                    (art_x + art_w / 2.0) as i32,
                    (cur_y + art_h / 2.0 - 4.0 * s) as i32,
                    font_scale,
                    ColorRgba::new(75, 85, 110, 255),
                );
            }
            cur_y += art_h + 16.0 * s;

            // Song Title with Drop Shadow
            BitmapFont::draw_text_with_shadow(
                &mut self.pixmap.as_mut(),
                &truncate_str(&selected_song.title, 26),
                art_x as i32,
                cur_y as i32,
                title_scale,
                ColorRgba::new(255, 255, 255, 255),
                ColorRgba::new(10, 10, 20, 255),
                1,
                1,
            );
            cur_y += 24.0 * s;

            // Artist & Genre
            let artist_genre = if !selected_song.genre.is_empty() {
                format!("{} / {}", truncate_str(&selected_song.artist, 18), truncate_str(&selected_song.genre, 14))
            } else {
                truncate_str(&selected_song.artist, 26)
            };
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &artist_genre,
                art_x as i32,
                cur_y as i32,
                font_scale,
                ColorRgba::new(150, 160, 190, 255),
            );
            cur_y += 24.0 * s;

            // Attribute 2x2 Grid
            let grid_box_w = (art_w - 12.0 * s) / 2.0;
            let grid_box_h = 44.0 * s;

            // Box 1: BPM
            self.draw_rect(art_x, cur_y, grid_box_w, grid_box_h, ColorRgba::new(20, 24, 36, 255));
            self.draw_rect(art_x, cur_y, grid_box_w, 1.0 * s, ColorRgba::new(40, 50, 75, 255));
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), "BPM", (art_x + 8.0 * s) as i32, (cur_y + 6.0 * s) as i32, font_scale, ColorRgba::new(120, 130, 160, 255));
            let bpm_val = format!("{:.1}", selected_song.bpm);
            BitmapFont::draw_bold_text(&mut self.pixmap.as_mut(), &bpm_val, (art_x + 8.0 * s) as i32, (cur_y + 20.0 * s) as i32, font_scale, ColorRgba::new(255, 220, 90, 255));

            // Box 2: Total Notes
            let box2_x = art_x + grid_box_w + 12.0 * s;
            self.draw_rect(box2_x, cur_y, grid_box_w, grid_box_h, ColorRgba::new(20, 24, 36, 255));
            self.draw_rect(box2_x, cur_y, grid_box_w, 1.0 * s, ColorRgba::new(40, 50, 75, 255));
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), "NOTES", (box2_x + 8.0 * s) as i32, (cur_y + 6.0 * s) as i32, font_scale, ColorRgba::new(120, 130, 160, 255));
            let notes_val = format!("{}", selected_song.notes_count);
            BitmapFont::draw_bold_text(&mut self.pixmap.as_mut(), &notes_val, (box2_x + 8.0 * s) as i32, (cur_y + 20.0 * s) as i32, font_scale, ColorRgba::new(100, 220, 255, 255));
            cur_y += grid_box_h + 16.0 * s;

            // Personal Best Card
            let pb_h = 135.0 * s;
            self.draw_rect(art_x, cur_y, art_w, pb_h, ColorRgba::new(18, 22, 34, 255));
            self.draw_rect(art_x, cur_y, art_w, 1.0 * s, ColorRgba::new(55, 65, 95, 255));
            self.draw_rect(art_x, cur_y + pb_h - 1.0 * s, art_w, 1.0 * s, ColorRgba::new(55, 65, 95, 255));
            self.draw_rect(art_x, cur_y, 1.0 * s, pb_h, ColorRgba::new(55, 65, 95, 255));
            self.draw_rect(art_x + art_w - 1.0 * s, cur_y, 1.0 * s, pb_h, ColorRgba::new(55, 65, 95, 255));

            let pb_header_y = cur_y + 8.0 * s;
            BitmapFont::draw_text(&mut self.pixmap.as_mut(), "PERSONAL BEST", (art_x + 10.0 * s) as i32, pb_header_y as i32, font_scale, ColorRgba::new(255, 210, 80, 255));

            if let Some(best) = score_store.get(selected_song.hash) {
                let (lamp_title, lamp_color) = clear_lamp_color(Some(best.clear_type));
                let (rank_str, rank_color) = accuracy_to_rank(best.accuracy_rate);

                // Lamp badge on PB card
                BitmapFont::draw_badge(
                    &mut self.pixmap.as_mut(),
                    lamp_title,
                    (art_x + art_w - 110.0 * s) as i32,
                    (pb_header_y - 2.0 * s) as i32,
                    font_scale,
                    ColorRgba::new(255, 255, 255, 255),
                    ColorRgba::new(15, 18, 25, 255),
                    lamp_color,
                    (6.0 * s) as i32,
                    (2.0 * s) as i32,
                );

                let mut row_y = cur_y + 36.0 * s;
                let ex_disp = format!("EX SCORE:   {:>4} pts", best.ex_score);
                BitmapFont::draw_text(&mut self.pixmap.as_mut(), &ex_disp, (art_x + 10.0 * s) as i32, row_y as i32, font_scale, ColorRgba::new(255, 255, 255, 255));
                row_y += 20.0 * s;

                let acc_disp = format!("ACCURACY:   {:.2}%  [{}]", best.accuracy_rate, rank_str);
                BitmapFont::draw_text(&mut self.pixmap.as_mut(), &acc_disp, (art_x + 10.0 * s) as i32, row_y as i32, font_scale, rank_color);
                row_y += 20.0 * s;

                let combo_disp = format!("MAX COMBO:  {:>4} / {}", best.max_combo, selected_song.notes_count);
                BitmapFont::draw_text(&mut self.pixmap.as_mut(), &combo_disp, (art_x + 10.0 * s) as i32, row_y as i32, font_scale, ColorRgba::new(180, 210, 255, 255));
            } else {
                BitmapFont::draw_text_centered(
                    &mut self.pixmap.as_mut(),
                    "NO RECORD REGISTERED",
                    (art_x + art_w / 2.0) as i32,
                    (cur_y + 65.0 * s) as i32,
                    font_scale,
                    ColorRgba::new(100, 110, 135, 255),
                );
            }
        }

        // 4. Bottom Footer Keybindings Guide
        let footer_y = (vp.y + vp.height - 36.0 * s) as i32;
        self.draw_rect(vp.x, footer_y as f32 - 4.0 * s, vp.width, 40.0 * s, ColorRgba::new(12, 14, 22, 255));
        self.draw_rect(vp.x, footer_y as f32 - 4.0 * s, vp.width, 1.0 * s, ColorRgba::new(35, 40, 60, 255));

        let match_info = format!("[TOTAL: {}/{}]", total_songs, total_library_count);
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &match_info,
            (vp.x + 24.0 * s) as i32,
            footer_y + (4.0 * s) as i32,
            font_scale,
            ColorRgba::new(80, 200, 255, 255),
        );

        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "[Up/Down]: Move  [Enter]: Play  [/]: Search  [F2]: Sort  [F3]: Folder  [Tab]: Options  [F12]: KeyConfig",
            (vp.x + 160.0 * s) as i32,
            footer_y + (4.0 * s) as i32,
            font_scale,
            ColorRgba::new(150, 155, 175, 255),
        );
    }
}

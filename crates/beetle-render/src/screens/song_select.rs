use crate::bitmap_font::BitmapFont;
use crate::components::Corner;
use crate::design_tokens::ColorToken;
use crate::image::ImageBuffer;
use crate::renderer::{
    accuracy_to_rank, clear_lamp_color, level_color, level_tier_label, truncate_str,
    SoftwareRenderer,
};
use crate::skin::ColorRgba;

impl SoftwareRenderer {
    /// Renders the song select screen with list and metadata panel.
    ///
    /// PULSE direction: single cyan/magenta duotone for all UI chrome (judgment
    /// colors stay reserved for judgment data), hairline dividers instead of
    /// per-row boxes, a glowing gradient edge for the selected row, and a
    /// diagonal-cut gradient CTA instead of a plain rectangle button. See
    /// docs/plans/2026-10-03-pulse-redesign.md and sketches/pulse-redesign/
    /// for the full direction and the HTML mockup it was approved from.
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
        opt_bar: &str,
    ) {
        self.clear();

        let vp = self.viewport;
        let s = vp.scale;
        let font_scale = (s * 0.9).round().max(2.0) as u32;
        let label_scale = (s * 0.72).round().max(2.0) as u32;
        let cyan = ColorToken::PULSE_CYAN.rgba();
        let magenta = ColorToken::PULSE_MAGENTA.rgba();
        let muted = ColorRgba::new(0x7b, 0x82, 0x99, 255);
        let muted2 = ColorRgba::new(0x4c, 0x53, 0x68, 255);
        let hairline = ColorRgba::new(0x17, 0x1b, 0x27, 255);

        // Ambient art-color bleed: sample the selected jacket's dominant
        // color and wash it softly across the upper-right of the screen,
        // fading into the base background. The flat near-black canvas this
        // used to sit on read "unfinished"; this one trick (the same used
        // by Spotify/Apple Music/PS5 and most modern rhythm game menus) is
        // disproportionately cheap for how much more "produced" it makes the
        // whole screen feel. See docs/plans/2026-10-03-pulse-redesign.md.
        if let Some(img) = stage_image {
            let ambient = img.average_color_sampled(6);
            let wash_h = vp.height * 0.55;
            self.draw_gradient_rect(
                vp.x,
                vp.y,
                vp.width,
                wash_h,
                ambient.with_alpha(46),
                ambient.with_alpha(0),
                false,
            );
        }

        // -----------------------------------------------------------------
        // 1. Top bar: wordmark + underline tabs + borderless search
        // -----------------------------------------------------------------
        let topbar_h = 56.0 * s;
        self.draw_rect(
            vp.x,
            vp.y,
            vp.width,
            topbar_h,
            ColorToken::SURFACE_BASE.rgba(),
        );
        self.draw_rect(vp.x, vp.y + topbar_h - s, vp.width, s, hairline);
        // PULSE identity accent: short gradient underline beneath the wordmark.
        self.draw_gradient_rect(
            vp.x,
            vp.y + topbar_h - 2.0 * s,
            180.0 * s,
            2.0 * s,
            cyan,
            magenta.with_alpha(0),
            true,
        );

        let title_scale = (2.0 * s).round().max(1.0) as u32;
        let logo_x = (vp.x + 24.0 * s) as i32;
        let logo_y = (vp.y + 16.0 * s) as i32;
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "BEE",
            logo_x,
            logo_y,
            title_scale,
            ColorRgba::new(255, 255, 255, 255),
        );
        let bee_w = BitmapFont::text_width("BEE", title_scale) as i32;
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "TLE",
            logo_x + bee_w,
            logo_y,
            title_scale,
            cyan,
        );

        // Mode tabs (underline style, no boxes)
        let tabs_x = logo_x
            + bee_w
            + (BitmapFont::text_width("TLE", title_scale) as i32)
            + 24.0 as i32 * s as i32;
        let mut tx = tabs_x;
        let ty = (vp.y + 22.0 * s) as i32;
        let tab_all = "ALL SONGS";
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            tab_all,
            tx,
            ty,
            label_scale,
            cyan,
        );
        self.draw_rect(
            tx as f32,
            ty as f32 + 14.0 * s,
            BitmapFont::text_width(tab_all, label_scale) as f32,
            s,
            cyan,
        );
        tx += BitmapFont::text_width(tab_all, label_scale) as i32 + (18.0 * s) as i32;

        let folder_label = format!("FOLDER: {}", category_str);
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &folder_label,
            tx,
            ty,
            label_scale,
            muted,
        );
        self.draw_rect(
            tx as f32,
            ty as f32 + 14.0 * s,
            BitmapFont::text_width(&folder_label, label_scale) as f32,
            s,
            muted2,
        );
        tx += BitmapFont::text_width(&folder_label, label_scale) as i32 + (18.0 * s) as i32;

        let sort_label = format!("SORT: {}", sort_mode_str);
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &sort_label,
            tx,
            ty,
            label_scale,
            muted,
        );
        self.draw_rect(
            tx as f32,
            ty as f32 + 14.0 * s,
            BitmapFont::text_width(&sort_label, label_scale) as f32,
            s,
            muted2,
        );

        // Search box (right side, borderless — just a bottom hairline that lights
        // up cyan while active, matching the search box in sketches/pulse-redesign)
        let search_w = 260.0 * s;
        let search_x = vp.x + vp.width - search_w - 24.0 * s;
        let search_y = vp.y + 20.0 * s;
        let search_col = if is_search_active { cyan } else { muted2 };
        self.draw_rect(search_x, search_y + 16.0 * s, search_w, s, search_col);
        let search_disp = if search_query.is_empty() && !is_search_active {
            "/ Search title / artist...".to_string()
        } else {
            format!(
                "Search: {}{}",
                search_query,
                if is_search_active { "_" } else { "" }
            )
        };
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &search_disp,
            search_x as i32,
            search_y as i32,
            font_scale,
            if is_search_active {
                ColorRgba::new(255, 255, 255, 255)
            } else {
                muted
            },
        );

        let total_songs = songs.len();
        if total_songs == 0 {
            let msg = if !search_query.is_empty() {
                format!("No songs match search query: \"{}\"", search_query)
            } else {
                "No BMS songs found in songs/ directory.".to_string()
            };
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &msg,
                (vp.x + 24.0 * s) as i32,
                (vp.y + 100.0 * s) as i32,
                font_scale,
                ColorRgba::new(220, 200, 200, 255),
            );
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                "Press [Esc] or [/] to reset search, or place .bms files into songs/ directory.",
                (vp.x + 24.0 * s) as i32,
                (vp.y + 128.0 * s) as i32,
                font_scale,
                muted,
            );
            return;
        }

        // -----------------------------------------------------------------
        // 2. Song list — hairline rows, no per-row boxes. The selected row
        // gets a glowing cyan→magenta gradient edge bar and a left-fading
        // gradient wash instead of a full bordered card.
        // -----------------------------------------------------------------
        let list_x = vp.x + 24.0 * s;
        let list_y0 = vp.y + topbar_h + 12.0 * s;
        let list_w = ((vp.width * 0.50).min(640.0 * s)).max(340.0 * s);
        let row_h = (28.0 * s).max(22.0);
        let max_visible = (((vp.height - topbar_h - 100.0 * s) / row_h).max(6.0)) as usize;

        let start_idx = if selected_idx >= max_visible / 2 {
            (selected_idx + 1)
                .saturating_sub(max_visible / 2)
                .min(total_songs.saturating_sub(max_visible))
        } else {
            0
        };
        let end_idx = (start_idx + max_visible).min(total_songs);

        let mut row_y = list_y0;
        for i in start_idx..end_idx {
            let song = &songs[i];
            let is_selected = i == selected_idx;
            let best_record = score_store.get(song.hash);
            let (lamp_str, lamp_color) = clear_lamp_color(best_record.map(|b| b.clear_type));
            let lvl_color = level_color(song.play_level);

            if is_selected {
                self.draw_gradient_rect(
                    list_x,
                    row_y,
                    list_w,
                    row_h,
                    cyan.with_alpha(26),
                    cyan.with_alpha(0),
                    true,
                );
                self.draw_gradient_rect(list_x, row_y, 3.0 * s, row_h, cyan, magenta, false);
            }
            self.draw_rect(list_x, row_y + row_h - s, list_w, s, hairline);

            let lvl_text = format!("Lv.{:>2}", song.play_level);
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &lvl_text,
                (list_x + 16.0 * s) as i32,
                (row_y + (row_h - BitmapFont::CJK_HEIGHT as f32 * font_scale as f32) / 2.0) as i32,
                font_scale,
                lvl_color,
            );

            let title_x = (list_x + 76.0 * s) as i32;
            let title_color = if is_selected {
                ColorRgba::new(255, 255, 255, 255)
            } else {
                ColorRgba::new(199, 203, 218, 255)
            };
            let title_budget = (list_w - 150.0 * s) as usize;
            let truncated_title = truncate_str(&song.title, (title_budget / 9).max(12));
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &truncated_title,
                title_x,
                (row_y + (row_h - BitmapFont::CJK_HEIGHT as f32 * font_scale as f32) / 2.0) as i32,
                font_scale,
                title_color,
            );

            let status_x = (list_x + list_w - 78.0 * s) as i32;
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                lamp_str,
                status_x,
                (row_y + (row_h - BitmapFont::CJK_HEIGHT as f32 * label_scale as f32) / 2.0) as i32,
                label_scale,
                lamp_color,
            );

            row_y += row_h;
        }

        // Scrollbar (thin, no track box)
        let scroll_x = list_x + list_w + 14.0 * s;
        let scroll_h = max_visible as f32 * row_h;
        if total_songs > 0 {
            let thumb_h =
                ((max_visible as f32 / total_songs as f32) * scroll_h).clamp(16.0 * s, scroll_h);
            let thumb_y =
                list_y0 + (selected_idx as f32 / total_songs as f32) * (scroll_h - thumb_h);
            self.draw_rect(scroll_x, thumb_y, 2.0 * s, thumb_h, cyan.with_alpha(200));
        }

        // -----------------------------------------------------------------
        // 3. Detail panel — jacket with corner accent + bottom gradient
        // scrim, unboxed label/value stat columns, diagonal-cut PLAY CTA.
        // -----------------------------------------------------------------
        let detail_x = scroll_x + 18.0 * s;
        let detail_y = list_y0;
        let detail_w = (vp.x + vp.width - detail_x - 24.0 * s).max(280.0 * s);
        self.draw_rect(
            detail_x - s,
            detail_y,
            s,
            vp.y + vp.height - 46.0 * s - detail_y,
            hairline,
        );

        if let Some(selected_song) = songs.get(selected_idx) {
            let art_x = detail_x + 24.0 * s;
            let art_w = detail_w - 24.0 * s;
            let art_h = (art_w * 9.0 / 16.0).clamp(130.0 * s, 230.0 * s);

            // Jacket: duotone diagonal wash standing in for cover art, corner
            // accent slash, bottom gradient scrim so the title reads on top.
            self.draw_gradient_rect(
                art_x,
                detail_y,
                art_w,
                art_h,
                ColorRgba::new(0x14, 0x1a, 0x2a, 255),
                ColorRgba::new(0x24, 0x18, 0x2a, 255),
                true,
            );
            if let Some(img) = stage_image {
                img.draw_fitted(
                    &mut self.pixmap,
                    art_x as i32,
                    detail_y as i32,
                    art_w as u32,
                    art_h as u32,
                    crate::image::ImageFitMode::FillCrop,
                );
            }
            self.draw_corner_triangle(
                art_x + art_w,
                detail_y,
                56.0 * s,
                Corner::TopRight,
                cyan.with_alpha(230),
            );
            self.draw_gradient_rect(
                art_x,
                detail_y + art_h - 90.0 * s,
                art_w,
                90.0 * s,
                ColorToken::SURFACE_BASE.rgba().with_alpha(0),
                ColorToken::SURFACE_BASE.rgba(),
                false,
            );

            let mut cur_y = detail_y + art_h - 54.0 * s;
            BitmapFont::draw_text_with_shadow(
                &mut self.pixmap.as_mut(),
                &truncate_str(&selected_song.title, 26),
                art_x as i32,
                cur_y as i32,
                title_scale,
                ColorRgba::new(255, 255, 255, 255),
                ColorRgba::new(5, 6, 10, 255),
                1,
                1,
            );
            cur_y = detail_y + art_h + 14.0 * s;

            let artist_genre = if !selected_song.genre.is_empty() {
                format!(
                    "{} / {}",
                    truncate_str(&selected_song.artist, 18),
                    truncate_str(&selected_song.genre, 14)
                )
            } else {
                truncate_str(&selected_song.artist, 26)
            };
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &artist_genre,
                art_x as i32,
                cur_y as i32,
                font_scale,
                muted,
            );
            cur_y += 36.0 * s;

            // Stat columns: micro-label + big value, no boxes.
            let col_w = art_w / 3.0;
            let stats: [(&str, String, ColorRgba); 3] = [
                (
                    "BPM",
                    format!("{:.1}", selected_song.bpm),
                    ColorRgba::new(255, 212, 0, 255),
                ),
                (
                    level_tier_label(selected_song.play_level),
                    format!("Lv.{}", selected_song.play_level),
                    level_color(selected_song.play_level),
                ),
                (
                    "NOTES",
                    format!("{}", selected_song.notes_count),
                    ColorRgba::new(255, 255, 255, 255),
                ),
            ];
            for (i, (label, value, color)) in stats.iter().enumerate() {
                let cx = art_x + col_w * i as f32;
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    label,
                    cx as i32,
                    cur_y as i32,
                    label_scale,
                    muted,
                );
                BitmapFont::draw_bold_text(
                    &mut self.pixmap.as_mut(),
                    value,
                    cx as i32,
                    (cur_y + 14.0 * s) as i32,
                    font_scale,
                    *color,
                );
            }
            cur_y += 50.0 * s;
            self.draw_rect(art_x, cur_y, art_w, s, hairline);
            cur_y += 18.0 * s;

            // Personal best — unboxed, hairline-separated label/value rows.
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                "PERSONAL BEST",
                art_x as i32,
                cur_y as i32,
                label_scale,
                ColorRgba::new(230, 230, 240, 255),
            );
            if let Some(best) = score_store.get(selected_song.hash) {
                let (lamp_title, lamp_color) = clear_lamp_color(Some(best.clear_type));
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    lamp_title,
                    (art_x + art_w - BitmapFont::text_width(lamp_title, label_scale) as f32) as i32,
                    cur_y as i32,
                    label_scale,
                    lamp_color,
                );
                cur_y += 24.0 * s;

                let (rank_str, rank_color) = accuracy_to_rank(best.accuracy_rate);
                let rows: [(&str, String, ColorRgba); 3] = [
                    (
                        "EX SCORE",
                        format!("{} pts", best.ex_score),
                        ColorRgba::new(255, 255, 255, 255),
                    ),
                    (
                        "ACCURACY",
                        format!("{:.2}% [{}]", best.accuracy_rate, rank_str),
                        rank_color,
                    ),
                    (
                        "MAX COMBO",
                        format!("{} / {}", best.max_combo, selected_song.notes_count),
                        ColorRgba::new(180, 210, 255, 255),
                    ),
                ];
                for (label, value, color) in rows {
                    BitmapFont::draw_text(
                        &mut self.pixmap.as_mut(),
                        label,
                        art_x as i32,
                        cur_y as i32,
                        label_scale,
                        muted,
                    );
                    let vw = BitmapFont::bold_text_width(&value, font_scale) as f32;
                    BitmapFont::draw_bold_text(
                        &mut self.pixmap.as_mut(),
                        &value,
                        (art_x + art_w - vw) as i32,
                        cur_y as i32,
                        font_scale,
                        color,
                    );
                    cur_y += 20.0 * s;
                }
            } else {
                cur_y += 30.0 * s;
                BitmapFont::draw_text(
                    &mut self.pixmap.as_mut(),
                    "NO RECORD REGISTERED",
                    art_x as i32,
                    cur_y as i32,
                    font_scale,
                    muted2,
                );
            }

            // Diagonal-cut gradient PLAY CTA, bottom-right of the panel.
            let cta_w = 150.0 * s;
            let cta_h = 44.0 * s;
            let cta_x = vp.x + vp.width - 24.0 * s - cta_w;
            let cta_y = vp.y + vp.height - 46.0 * s - cta_h - 16.0 * s;
            self.draw_cut_quad(
                cta_x,
                cta_y,
                cta_w,
                cta_h,
                14.0 * s,
                cyan,
                ColorRgba::new(0x5a, 0xd9, 0xff, 255),
            );
            BitmapFont::draw_bold_text_centered(
                &mut self.pixmap.as_mut(),
                "PLAY",
                (cta_x + cta_w / 2.0 + 6.0 * s) as i32,
                (cta_y + (cta_h - BitmapFont::CJK_HEIGHT as f32 * font_scale as f32) / 2.0) as i32,
                font_scale,
                ColorRgba::new(6, 19, 26, 255),
            );
        }

        // -----------------------------------------------------------------
        // 4. Footer
        // -----------------------------------------------------------------
        let footer_h = 46.0 * s;
        let footer_top = vp.y + vp.height - footer_h;
        self.draw_rect(
            vp.x,
            footer_top,
            vp.width,
            footer_h,
            ColorRgba::new(0, 0, 0, 255),
        );
        self.draw_rect(vp.x, footer_top, vp.width, s, hairline);

        let line1_y = (footer_top + 8.0 * s) as i32;
        let line2_y = (footer_top + 26.0 * s) as i32;
        let match_info = format!("[TOTAL: {}/{}]", total_songs, total_library_count);
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &match_info,
            (vp.x + 24.0 * s) as i32,
            line1_y,
            label_scale,
            cyan,
        );
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            "[Up/Down]: Move  [Enter]: Play  [/]: Search  [F2]: Sort  [F3]: Folder  [Tab]: Options  [F12]: KeyConfig",
            (vp.x + 160.0 * s) as i32,
            line1_y,
            label_scale,
            muted,
        );
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            opt_bar,
            (vp.x + 24.0 * s) as i32,
            line2_y,
            label_scale,
            muted,
        );
    }
}

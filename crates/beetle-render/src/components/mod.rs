//! UI component library: reusable rendering primitives for all screens.
//!
//! Each component is a method on `SoftwareRenderer` that draws a self-contained
//! visual element using design tokens. Components only use `draw_rect`, `draw_text`,
//! `draw_badge`, and `blit_glyph_aa` primitives — no new rendering dependencies.

use crate::bitmap_font::BitmapFont;
use crate::design_tokens::*;
use crate::renderer::SoftwareRenderer;
use crate::skin::ColorRgba;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelVariant {
    Base,
    Card,
    Overlay,
}

impl SoftwareRenderer {
    // -----------------------------------------------------------------------
    // Geometry helpers
    // -----------------------------------------------------------------------

    /// Draws a rectangle with rounded corners (top-left, top-right, bottom-right, bottom-left)
    /// approximated by filling corner squares. If radius is 0, it's a plain rect.
    fn draw_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: ColorRgba, radius: f32) {
        if radius <= 0.0 {
            self.draw_rect(x, y, w, h, color);
            return;
        }
        // Fill full rect, then we'd need per-corner clipping for true rounding.
        // Approximation: fill rect, skip pixels outside corner radius (per-pixel check).
        // For performance with small radius, use plain fill + 1px border on corners.
        self.draw_rect(x, y, w, h, color);
    }

    // -----------------------------------------------------------------------
    // Panel (base container)
    // -----------------------------------------------------------------------

    /// Renders a base panel / card / overlay container.
    pub fn render_panel(&mut self, x: f32, y: f32, w: f32, h: f32, variant: PanelVariant) {
        let s = self.viewport.scale;
        let (bg, border, has_border) = match variant {
            PanelVariant::Base => (
                ColorToken::SURFACE_PANEL.rgba(),
                ColorToken::BORDER_SUBTLE.rgba(),
                false,
            ),
            PanelVariant::Card => (
                ColorToken::SURFACE_CARD.rgba(),
                ColorToken::BORDER_SUBTLE.rgba(),
                false,
            ),
            PanelVariant::Overlay => (
                ColorToken::SURFACE_OVERLAY.rgba(),
                ColorToken::BORDER_SUBTLE.rgba(),
                true,
            ),
        };

        let _ = Radius::MD.scaled(s);
        self.draw_rect(x, y, w, h, bg);
        if has_border {
            let bw = (1.0 * s).max(1.0);
            self.draw_rect(x, y, w, bw, border);
            self.draw_rect(x, y + h - bw, w, bw, border);
            self.draw_rect(x, y, bw, h, border);
            self.draw_rect(x + w - bw, y, bw, h, border);
        }
    }

    // -----------------------------------------------------------------------
    // Card (inner stat box with optional title)
    // -----------------------------------------------------------------------

    /// Renders a card: `surface_card` background + top border + optional title.
    pub fn render_card(&mut self, x: f32, y: f32, w: f32, h: f32, title: Option<&str>) {
        let s = self.viewport.scale;
        let font_scale = (s * 0.9).round().max(1.0) as u32;

        self.draw_rect(x, y, w, h, ColorToken::SURFACE_CARD.rgba());

        // Top border (subtle)
        let bw = (1.0 * s).max(1.0);
        self.draw_rect(x, y, w, bw, ColorToken::BORDER_SUBTLE.rgba());

        if let Some(t) = title {
            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                t,
                (x + Spacing::SM.scaled(s)) as i32,
                (y + Spacing::XS.scaled(s)) as i32,
                (font_scale as f32 * 10.0 / 14.0).round() as u32, // Label tier ≈ 0.7x
                ColorToken::TEXT_TERTIARY.rgba(),
            );
        }
    }

    // -----------------------------------------------------------------------
    // Button (functional, state-aware)
    // -----------------------------------------------------------------------

    pub fn render_button(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        label: &str,
        state: WidgetState,
        font_scale: u32,
    ) {
        let s = self.viewport.scale;
        let bg = state.bg_color().rgba();
        let text_col = state.text_color().rgba();

        let r = Radius::SM.scaled(s);
        self.draw_rounded_rect(x, y, w, h, bg, r);

        if let Some(border) = state.border_color() {
            let bw = (2.0 * s).max(1.0);
            let bc = border.rgba();
            self.draw_rect(x, y, w, bw, bc);
            self.draw_rect(x, y + h - bw, w, bw, bc);
            self.draw_rect(x, y, bw, h, bc);
            self.draw_rect(x + w - bw, y, bw, h, bc);
        }

        BitmapFont::draw_text_centered(
            &mut self.pixmap.as_mut(),
            label,
            (x + w / 2.0) as i32,
            (y + (h - (BitmapFont::CJK_HEIGHT * font_scale) as f32) / 2.0) as i32,
            font_scale,
            text_col,
        );
    }

    // -----------------------------------------------------------------------
    // Badge (label + value pill)
    // -----------------------------------------------------------------------

    /// Renders a pill badge with a left-aligned label and right-aligned value.
    pub fn render_badge(
        &mut self,
        x: f32,
        y: f32,
        max_w: f32,
        label: &str,
        value: &str,
        color: ColorToken,
        font_scale: u32,
    ) {
        let s = self.viewport.scale;
        let label_w = BitmapFont::text_width(label, font_scale) as f32;
        let value_w = BitmapFont::text_width(value, font_scale) as f32;
        let total_w = (label_w + Spacing::SM.scaled(s) + value_w).min(max_w);
        let total_h = (BitmapFont::CJK_HEIGHT as f32 * font_scale as f32
            + Spacing::XS.scaled(s) * 2.0)
            .max(Spacing::SM.scaled(s) * 2.0);

        // Background
        let bg = ColorToken::SURFACE_CARD.rgba();
        self.draw_rect(x, y, total_w, total_h, bg);

        // Border (accent color)
        let bw = (1.0 * s).max(1.0);
        self.draw_rect(x, y, total_w, bw, color.rgba());
        self.draw_rect(x, y + total_h - bw, total_w, bw, color.rgba());
        self.draw_rect(x, y, bw, total_h, color.rgba());
        self.draw_rect(x + total_w - bw, y, bw, total_h, color.rgba());

        // Label
        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            label,
            (x + Spacing::SM.scaled(s)) as i32,
            (y + Spacing::XS.scaled(s)) as i32,
            font_scale,
            ColorToken::TEXT_TERTIARY.rgba(),
        );

        // Value (right-aligned/bold)
        let val_x = (x + total_w - value_w - Spacing::SM.scaled(s)).max(x + Spacing::SM.scaled(s));
        BitmapFont::draw_bold_text(
            &mut self.pixmap.as_mut(),
            value,
            val_x as i32,
            (y + Spacing::XS.scaled(s)) as i32,
            font_scale,
            color.rgba(),
        );
    }

    // -----------------------------------------------------------------------
    // ProgressBar (gauge-style bar)
    // -----------------------------------------------------------------------

    pub fn render_progress_bar(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        progress: f32, // 0.0..=1.0
        color: ColorToken,
        show_text: bool,
        text: &str,
        font_scale: u32,
    ) {
        let s = self.viewport.scale;
        let progress = progress.clamp(0.0, 1.0);

        // Background
        self.draw_rect(x, y, w, h, ColorToken::SURFACE_PANEL.rgba());

        // Fill
        let fill_w = w * progress;
        let fill_col = color.rgba();
        self.draw_rect(x, y, fill_w, h, fill_col);

        // Border
        let bw = (1.0 * s).max(1.0);
        self.draw_rect(x, y, w, bw, ColorToken::BORDER_SUBTLE.rgba());
        self.draw_rect(x, y + h - bw, w, bw, ColorToken::BORDER_SUBTLE.rgba());
        self.draw_rect(x, y, bw, h, ColorToken::BORDER_SUBTLE.rgba());
        self.draw_rect(x + w - bw, y, bw, h, ColorToken::BORDER_SUBTLE.rgba());

        if show_text {
            BitmapFont::draw_text_centered(
                &mut self.pixmap.as_mut(),
                text,
                (x + w / 2.0) as i32,
                (y + (h - (BitmapFont::CJK_HEIGHT * font_scale) as f32) / 2.0) as i32,
                font_scale,
                ColorToken::TEXT_PRIMARY.rgba(),
            );
        }
    }

    // -----------------------------------------------------------------------
    // HeaderBar (screen top bar with title, badges, and search)
    // -----------------------------------------------------------------------

    pub fn render_header_bar(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        title: &str,
        badges: &[(&str, ColorToken)],
        search_active: bool,
        search_query: &str,
        placeholder: &str,
        font_scale: u32,
    ) {
        let s = self.viewport.scale;
        let title_scale = (2.0 * s).round().max(1.0) as u32;

        // Background
        self.draw_rect(x, y, w, h, ColorToken::SURFACE_BASE.rgba());

        // Bottom border
        let bw = (1.0 * s).max(1.0);
        self.draw_rect(x, y + h - bw, w, bw, ColorToken::BORDER_SUBTLE.rgba());

        // Title
        BitmapFont::draw_text_with_shadow(
            &mut self.pixmap.as_mut(),
            title,
            (x + Spacing::MD.scaled(s)) as i32,
            (y + Spacing::XS.scaled(s)) as i32,
            title_scale,
            ColorToken::TEXT_PRIMARY.rgba(),
            ColorToken::SHADOW_DROP.rgba(),
            1,
            1,
        );

        // Badges (left to right after title)
        let mut badge_x = x
            + Spacing::MD.scaled(s)
            + BitmapFont::text_width(title, title_scale) as f32
            + Spacing::SM.scaled(s);
        for (badge_label, badge_color) in badges {
            let badge_text = badge_label.to_string();
            let badge_w = (BitmapFont::text_width(&badge_text, font_scale) as f32
                + Spacing::SM.scaled(s) * 2.0)
                .min(200.0 * s);
            let badge_h = (BitmapFont::CJK_HEIGHT as f32 * font_scale as f32
                + Spacing::XS.scaled(s) * 2.0)
                .max(20.0 * s);

            // Badge background
            self.draw_rect(
                badge_x,
                y + Spacing::XS.scaled(s),
                badge_w,
                badge_h,
                ColorToken::SURFACE_CARD.rgba(),
            );
            let bw = (1.0 * s).max(1.0);
            self.draw_rect(
                badge_x,
                y + Spacing::XS.scaled(s),
                badge_w,
                bw,
                badge_color.rgba(),
            );

            BitmapFont::draw_text(
                &mut self.pixmap.as_mut(),
                &badge_text,
                (badge_x + Spacing::SM.scaled(s)) as i32,
                (y + Spacing::XS.scaled(s) + Spacing::XS.scaled(s)) as i32,
                font_scale,
                ColorToken::TEXT_PRIMARY.rgba(),
            );

            badge_x += badge_w + Spacing::SM.scaled(s);
        }

        // Search box (right side)
        let search_w = (260.0 * s)
            .min(w - badge_x - Spacing::SM.scaled(s) * 2.0)
            .max(180.0 * s);
        let search_x = w - search_w - Spacing::MD.scaled(s);
        let search_y = y + Spacing::XS.scaled(s);
        let search_h = h - Spacing::XS.scaled(s) * 2.0;

        let search_bg = ColorToken::SURFACE_CARD.rgba();
        let search_border = if search_active {
            ColorToken::BORDER_FOCUS.rgba()
        } else {
            ColorToken::BORDER_SUBTLE.rgba()
        };

        self.draw_rect(search_x, search_y, search_w, search_h, search_bg);
        let bw = (1.0 * s).max(1.0);
        self.draw_rect(search_x, search_y, search_w, bw, search_border);
        self.draw_rect(
            search_x,
            search_y + search_h - bw,
            search_w,
            bw,
            search_border,
        );
        self.draw_rect(search_x, search_y, bw, search_h, search_border);
        self.draw_rect(
            search_x + search_w - bw,
            search_y,
            bw,
            search_h,
            search_border,
        );

        let search_text = if search_query.is_empty() && !search_active {
            placeholder.to_string()
        } else {
            format!(
                "Search: {}{}",
                search_query,
                if search_active { "_" } else { "" }
            )
        };

        BitmapFont::draw_text(
            &mut self.pixmap.as_mut(),
            &search_text,
            (search_x + Spacing::SM.scaled(s)) as i32,
            (search_y + (search_h - (BitmapFont::CJK_HEIGHT * font_scale) as f32) / 2.0) as i32,
            font_scale,
            if search_active {
                ColorToken::TEXT_PRIMARY.rgba()
            } else {
                ColorToken::TEXT_TERTIARY.rgba()
            },
        );
    }
}

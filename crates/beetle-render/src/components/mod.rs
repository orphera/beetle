//! UI component library: reusable rendering primitives for all screens.
//!
//! Each component is a method on `SoftwareRenderer` that draws a self-contained
//! visual element using design tokens. Components only use `draw_rect`, `draw_text`,
//! `draw_badge`, and `blit_glyph_aa` primitives — no new rendering dependencies.

use crate::bitmap_font::BitmapFont;
use crate::design_tokens::*;
use crate::renderer::SoftwareRenderer;
use crate::skin::ColorRgba;

use tiny_skia::{
    FillRule, GradientStop, LinearGradient, Paint, PathBuilder, Point, Shader, SpreadMode,
    Transform,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelVariant {
    Base,
    Card,
    Overlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    TopRight,
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
    // PULSE identity primitives: linear gradients + diagonal-cut quads.
    // Uses tiny-skia's own Path/Shader pipeline (already a core dependency,
    // see AGENTS.md allowed crates) instead of the hand-rolled flat-fill
    // `draw_rect` fast path, since gradients/non-axis-aligned shapes need
    // tiny-skia's rasterizer. Reserved for PULSE chrome (CTAs, scrims,
    // selection edges) — the high-frequency gameplay draw loop keeps using
    // the raw `draw_rect` fast path untouched.
    // -----------------------------------------------------------------------

    fn color_to_premul(c: ColorRgba) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a)
    }

    /// Fills an axis-aligned rect with a 2-stop linear gradient.
    /// `horizontal = true` goes left→right, otherwise top→bottom.
    pub fn draw_gradient_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color_start: ColorRgba,
        color_end: ColorRgba,
        horizontal: bool,
    ) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let (start, end) = if horizontal {
            (Point::from_xy(x, y), Point::from_xy(x + w, y))
        } else {
            (Point::from_xy(x, y), Point::from_xy(x, y + h))
        };
        let Some(shader) = LinearGradient::new(
            start,
            end,
            vec![
                GradientStop::new(0.0, Self::color_to_premul(color_start)),
                GradientStop::new(1.0, Self::color_to_premul(color_end)),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        ) else {
            self.draw_rect(x, y, w, h, color_start);
            return;
        };
        let paint = Paint {
            shader,
            anti_alias: false,
            ..Default::default()
        };
        if let Some(rect) = tiny_skia::Rect::from_xywh(x, y, w, h) {
            self.pixmap
                .fill_rect(rect, &paint, Transform::identity(), None);
        }
    }

    /// Fills a quad with the top-left corner diagonally cut by `cut` pixels
    /// (the "arcade cabinet" CTA/panel shape used throughout PULSE), with an
    /// optional horizontal gradient fill.
    pub fn draw_cut_quad(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        cut: f32,
        color_start: ColorRgba,
        color_end: ColorRgba,
    ) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let cut = cut.min(w).max(0.0);
        let mut pb = PathBuilder::new();
        pb.move_to(x + cut, y);
        pb.line_to(x + w, y);
        pb.line_to(x + w, y + h);
        pb.line_to(x, y + h);
        pb.close();
        let Some(path) = pb.finish() else { return };

        let Some(shader) = LinearGradient::new(
            Point::from_xy(x, y),
            Point::from_xy(x + w, y),
            vec![
                GradientStop::new(0.0, Self::color_to_premul(color_start)),
                GradientStop::new(1.0, Self::color_to_premul(color_end)),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        ) else {
            return;
        };
        let paint = Paint {
            shader,
            anti_alias: true,
            ..Default::default()
        };
        self.pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    /// Fills a right-angle triangle in one corner of a box (used for the
    /// jacket "corner slash" accent). `corner` selects which of the box's
    /// four corners the right angle sits in.
    pub fn draw_corner_triangle(
        &mut self,
        x: f32,
        y: f32,
        size: f32,
        corner: Corner,
        color: ColorRgba,
    ) {
        if size <= 0.0 {
            return;
        }
        let mut pb = PathBuilder::new();
        match corner {
            Corner::TopRight => {
                pb.move_to(x - size, y);
                pb.line_to(x, y);
                pb.line_to(x, y + size);
            }
        }
        pb.close();
        let Some(path) = pb.finish() else { return };
        let paint = Paint {
            shader: Shader::SolidColor(Self::color_to_premul(color)),
            anti_alias: true,
            ..Default::default()
        };
        self.pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
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

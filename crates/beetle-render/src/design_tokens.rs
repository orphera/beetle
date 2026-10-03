//! Design tokens: semantic colors, spacing, typography, shadows, border radii.
//! All values scale with Viewport::scale (s).

use crate::skin::ColorRgba;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorToken(pub ColorRgba);

impl ColorToken {
    /// Semantic surface colors
    pub const SURFACE_BASE: Self = Self(ColorRgba::new(0x08, 0x08, 0x0C, 0xFF));
    pub const SURFACE_PANEL: Self = Self(ColorRgba::new(0x0E, 0x10, 0x1A, 0xFF));
    pub const SURFACE_CARD: Self = Self(ColorRgba::new(0x14, 0x18, 0x23, 0xFF));
    pub const SURFACE_OVERLAY: Self = Self(ColorRgba::new(0x10, 0x14, 0x20, 0xCC));
    pub const SURFACE_ROW_ALT_EVEN: Self = Self(ColorRgba::new(0x0F, 0x11, 0x18, 0xDC));
    pub const SURFACE_ROW_ALT_ODD: Self = Self(ColorRgba::new(0x16, 0x18, 0x23, 0xDC));

    /// Border colors
    pub const BORDER_SUBTLE: Self = Self(ColorRgba::new(0x23, 0x28, 0x3C, 0xFF));
    pub const BORDER_FOCUS: Self = Self(ColorRgba::new(0x50, 0xBC, 0xFF, 0xFF));
    pub const BORDER_DANGER: Self = Self(ColorRgba::new(0xF0, 0x3A, 0x3A, 0xFF));

    /// Text colors
    pub const TEXT_PRIMARY: Self = Self(ColorRgba::new(0xFF, 0xFF, 0xFF, 0xFF));
    pub const TEXT_SECONDARY: Self = Self(ColorRgba::new(0xBE, 0xC3, 0xD7, 0xFF));
    pub const TEXT_TERTIARY: Self = Self(ColorRgba::new(0x78, 0x82, 0xA0, 0xFF));
    pub const TEXT_HINT: Self = Self(ColorRgba::new(0x64, 0x6E, 0x87, 0xFF));

    /// Accent colors
    pub const ACCENT_CYAN: Self = Self(ColorRgba::new(0x50, 0xBC, 0xFF, 0xFF));
    pub const ACCENT_YELLOW: Self = Self(ColorRgba::new(0xFF, 0xE6, 0x32, 0xFF));
    pub const ACCENT_RED: Self = Self(ColorRgba::new(0xF0, 0x46, 0x46, 0xFF));
    pub const ACCENT_GREEN: Self = Self(ColorRgba::new(0x3C, 0xDC, 0x96, 0xFF));
    pub const ACCENT_ORANGE: Self = Self(ColorRgba::new(0xFF, 0x8C, 0x32, 0xFF));
    pub const ACCENT_MINT: Self = Self(ColorRgba::new(0x50, 0xFF, 0xB8, 0xFF));

    /// Difficulty tier colors
    pub const DIFF_NORMAL: Self = Self(ColorRgba::new(0x60, 0xE0, 0x60, 0xFF));
    pub const DIFF_HYPER: Self = Self(ColorRgba::new(0x50, 0xB0, 0xFF, 0xFF));
    pub const DIFF_ANOTHER: Self = Self(ColorRgba::new(0xE0, 0x50, 0xFF, 0xFF));
    pub const DIFF_INSANE: Self = Self(ColorRgba::new(0xFF, 0x50, 0x50, 0xFF));
    pub const DIFF_OVERJOY: Self = Self(ColorRgba::new(0xFF, 0xD0, 0x30, 0xFF));

    /// Glow colors
    pub const GLOW_RANK_MAX: Self = Self(ColorRgba::new(0xFF, 0xD7, 0x00, 0xFF));
    pub const GLOW_RANK_AAA: Self = Self(ColorRgba::new(0xFF, 0xDC, 0x32, 0xFF));

    /// Shadow helpers
    pub const SHADOW_DROP: Self = Self(ColorRgba::new(0x00, 0x00, 0x00, 0x99));
    pub const SHADOW_CARD: Self = Self(ColorRgba::new(0x00, 0x00, 0x00, 0x66));

    #[inline(always)]
    pub fn rgba(self) -> ColorRgba {
        self.0
    }

    #[inline(always)]
    pub fn with_alpha(self, a: u8) -> Self {
        Self(self.0.with_alpha(a))
    }

    /// Blend with surface_card for tinted difficulty backgrounds.
    /// `channel / 6` ≈ 16.7% opacity blend.
    pub fn blend_card_tinted(self) -> ColorRgba {
        let card = Self::SURFACE_CARD.0;
        ColorRgba::new(
            (card.r as u16 + self.0.r as u16 / 6).min(255) as u8,
            (card.g as u16 + self.0.g as u16 / 6).min(255) as u8,
            (card.b as u16 + self.0.b as u16 / 6).min(255) as u8,
            255,
        )
    }
}

/// Spacing tokens (multiplied by viewport scale `s` at render time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spacing(pub i32); // base pixels at s=1

impl Spacing {
    pub const XS: Self = Self(4);
    pub const SM: Self = Self(8);
    pub const MD: Self = Self(16);
    pub const LG: Self = Self(24);
    pub const XL: Self = Self(32);

    #[inline(always)]
    pub fn scaled(self, s: f32) -> f32 {
        self.0 as f32 * s
    }
}

/// Typography scale tiers (multiplied by viewport scale `s` at render time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypographyTier {
    Title,
    Value,
    Body,
    Label,
    Hint,
}

impl TypographyTier {
    /// Scale multiplier for this tier.
    pub fn multiplier(self) -> f32 {
        match self {
            Self::Title => 2.0,
            Self::Value => 1.1,
            Self::Body => 0.9,
            Self::Label => 0.7,
            Self::Hint => 0.9, // same as Body, only color differs
        }
    }

    /// Base glyph size for this tier (before scaling by `s`).
    pub fn base_glyph_size(self) -> (u32, u32) {
        match self {
            Self::Title | Self::Value | Self::Body => (16, 14), // ASCII
            Self::Label => (12, 10),
            Self::Hint => (16, 14),
        }
    }

    /// CJK glyph size for this tier.
    pub fn base_cjk_size(self) -> (u32, u32) {
        match self {
            Self::Title | Self::Value | Self::Body => (20, 16),
            Self::Label => (16, 12),
            Self::Hint => (20, 16),
        }
    }

    /// Recommended draw helper for this tier.
    pub fn draw_helper(self) -> &'static str {
        match self {
            Self::Title => "draw_text_with_shadow",
            Self::Value => "draw_bold_text",
            Self::Body => "draw_text",
            Self::Label => "draw_text",
            Self::Hint => "draw_text",
        }
    }
}

/// Border radius tokens (multiplied by `s` at render time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Radius(pub i32);

impl Radius {
    pub const NONE: Self = Self(0);
    pub const SM: Self = Self(4);
    pub const MD: Self = Self(8);
    pub const LG: Self = Self(12);

    #[inline(always)]
    pub fn scaled(self, s: f32) -> f32 {
        self.0 as f32 * s
    }
}

/// Button / interactive state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetState {
    Default,
    Hover,
    Selected,
    Disabled,
}

impl WidgetState {
    pub fn bg_color(self) -> ColorToken {
        match self {
            Self::Default => ColorToken::SURFACE_PANEL,
            Self::Hover => ColorToken::SURFACE_CARD,
            Self::Selected => ColorToken::SURFACE_CARD,
            Self::Disabled => ColorToken::SURFACE_PANEL,
        }
    }

    pub fn text_color(self) -> ColorToken {
        match self {
            Self::Default => ColorToken::TEXT_SECONDARY,
            Self::Hover => ColorToken::TEXT_PRIMARY,
            Self::Selected => ColorToken::TEXT_PRIMARY,
            Self::Disabled => ColorToken::TEXT_HINT,
        }
    }

    pub fn border_color(self) -> Option<ColorToken> {
        match self {
            Self::Selected => Some(ColorToken::BORDER_FOCUS),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_token_rgba() {
        // Verify to_u32 produces ARGB; compare against computed value from fields
        let base = ColorToken::SURFACE_BASE.0;
        assert_eq!(
            base.to_u32(),
            ((base.a as u32) << 24)
                | ((base.r as u32) << 16)
                | ((base.g as u32) << 8)
                | (base.b as u32)
        );
    }

    #[test]
    fn spacing_scaled() {
        assert!((Spacing::MD.scaled(1.0) - 16.0).abs() < f32::EPSILON);
        assert!((Spacing::MD.scaled(2.0) - 32.0).abs() < f32::EPSILON);
    }

    #[test]
    fn typography_tier_sizes() {
        assert_eq!(TypographyTier::Title.base_glyph_size(), (16, 14));
        assert_eq!(TypographyTier::Title.base_cjk_size(), (20, 16));
        assert_eq!(TypographyTier::Label.base_glyph_size(), (12, 10));
    }

    #[test]
    fn radius_scaled() {
        assert!((Radius::MD.scaled(1.0) - 8.0).abs() < f32::EPSILON);
    }

    #[test]
    fn widget_state_colors() {
        assert_eq!(WidgetState::Selected.bg_color(), ColorToken::SURFACE_CARD);
        assert_eq!(
            WidgetState::Selected.border_color(),
            Some(ColorToken::BORDER_FOCUS)
        );
        assert_eq!(WidgetState::Disabled.text_color(), ColorToken::TEXT_HINT);
    }
}

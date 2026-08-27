use crate::skin::ColorRgba;
use tiny_skia::PixmapMut;

/// Embedded 5x7 ASCII bitmap font representation for minimal binary size.
/// No vector font rasterizers or TrueType loaders required.
pub struct BitmapFont;

impl BitmapFont {
    /// Draws a number or ASCII string onto a raw pixel buffer or Pixmap.
    pub fn draw_text(
        _pixmap: &mut PixmapMut,
        _text: &str,
        _x: i32,
        _y: i32,
        _scale: u32,
        _color: ColorRgba,
    ) {
        // TODO: Embedded glyph table and blit logic in Phase 3
    }
}

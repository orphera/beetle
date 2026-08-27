//! # beetle-render
//!
//! Software 2D rendering pipeline utilizing tiny-skia and embedded bitmap fonts.
//! Direct output to softbuffer with zero GPU runtime requirements.

pub mod bitmap_font;
pub mod image;
pub mod renderer;
pub mod skin;

pub use bitmap_font::BitmapFont;
pub use image::ImageBuffer;
pub use renderer::SoftwareRenderer;
pub use skin::{ColorRgba, SkinConfig};

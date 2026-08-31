//! # beetle-render
//!
//! Software 2D rendering pipeline utilizing tiny-skia and embedded bitmap fonts.
//! Direct output to softbuffer with zero GPU runtime requirements.

pub mod backend;
pub mod bitmap_font;
pub mod image;
pub mod renderer;
pub mod screens;
pub mod skin;
pub mod video;

pub use backend::{BlendMode, GpuBackend, GpuTexturePool, SoftBackend, SpriteBatcher, TextureId, Vertex2D};
#[cfg(target_os = "windows")]
pub use backend::D3d11Backend;
pub use bitmap_font::BitmapFont;
pub use image::{ImageBuffer, ImageFitMode};
pub use renderer::SoftwareRenderer;
pub use skin::{ColorRgba, SkinConfig};
pub use video::{is_video_path, BgaVideoPlayer};

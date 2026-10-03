//! # beetle-render
//!
//! Software 2D rendering pipeline utilizing tiny-skia and embedded bitmap fonts.
//! Direct output to softbuffer with zero GPU runtime requirements.

pub mod backend;
pub mod bitmap_font;
pub mod design_tokens;
pub mod image;
pub mod renderer;
pub mod screens;
pub mod skin;
pub mod video;

#[cfg(target_os = "windows")]
pub use backend::D3d11Backend;
pub use backend::{
    BgaAtlasBuilder, BlendMode, FontAtlas, GpuBackend, GpuBgaAtlas, GpuTexturePool, SoftBackend,
    SpriteBatcher, TextureId, Vertex2D,
};
pub use bitmap_font::BitmapFont;
pub use image::{ImageBuffer, ImageFitMode};
pub use renderer::{HitBurst, SoftwareRenderer, Viewport};
pub use screens::render_gameplay_gpu;
pub use skin::{ColorRgba, SkinConfig};
pub use video::{is_video_path, BgaVideoPlayer, VIDEO_EXTENSIONS};

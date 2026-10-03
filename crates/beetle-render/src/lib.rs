//! # beetle-render
//!
//! Software 2D rendering pipeline utilizing tiny-skia and embedded bitmap fonts.
//! Direct output to softbuffer with zero GPU runtime requirements.

pub mod art;
pub mod backend;
pub mod canvas;
pub mod bitmap_font;
pub mod components;
pub mod design_tokens;
pub mod image;
pub mod motion;
pub mod renderer;
pub mod screens;
pub mod skin;
pub mod text;
pub mod theme;
pub mod ui;
pub mod video;

#[cfg(target_os = "windows")]
pub use backend::D3d11Backend;
pub use backend::{
    BgaAtlasBuilder, BlendMode, FontAtlas, GpuBackend, GpuBgaAtlas, GpuTexturePool, SoftBackend,
    SpriteBatcher, TextureId, Vertex2D,
};
pub use bitmap_font::BitmapFont;
pub use art::{Halo, NineSlice, Skin};
pub use canvas::{AtlasRegion, Canvas, Insets, Rect};
pub use text::{Align, FontMetrics, TextEngine, TextStyle, Weight};
pub use ui::Ui;
pub use image::{ImageBuffer, ImageFitMode};
pub use renderer::{HitBurst, SoftwareRenderer, Viewport};
pub use screens::{draw_gameplay, render_gameplay_gpu, PlayFrame, SizedTexture};
pub use skin::{ColorRgba, SkinConfig};
pub use video::{is_video_path, BgaVideoPlayer, VIDEO_EXTENSIONS};

/// Result screen rank-letter pop-in duration (ease_out_back settle).
pub const RANK_POP_SECONDS: f64 = 0.35;
/// Result screen EX score count-up duration (ease_out_cubic).
pub const SCORE_COUNT_SECONDS: f64 = 0.9;
/// Longest of the Result screen's reveal animations — callers (the app's
/// render loop) use this to know how long to keep forcing redraws past the
/// initial dirty-flag render before the screen is fully settled and static.
pub const RESULT_REVEAL_DURATION_SECONDS: f64 = SCORE_COUNT_SECONDS;

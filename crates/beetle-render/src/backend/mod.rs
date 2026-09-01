pub mod atlas;
pub mod batcher;
#[cfg(target_os = "windows")]
pub mod d3d11;
pub mod soft;
pub mod texture_pool;

pub use atlas::FontAtlas;
pub use batcher::SpriteBatcher;
#[cfg(target_os = "windows")]
pub use d3d11::D3d11Backend;
pub use soft::SoftBackend;
pub use texture_pool::GpuTexturePool;

/// Unique identifier for an uploaded GPU / HAL texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureId(pub u32);

/// 2D Vertex structure for GPU batched sprite & primitive rendering.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex2D {
    /// 2D Screen Coordinate (X, Y) in pixel units
    pub position: [f32; 2],
    /// Normalized Texture Coordinates (U, V) [0.0 ~ 1.0]
    pub uv: [f32; 2],
    /// Normalized RGBA Color Tint & Alpha [0.0 ~ 1.0]
    pub color: [f32; 4],
}

impl Vertex2D {
    pub const fn new(x: f32, y: f32, u: f32, v: f32, color: [f32; 4]) -> Self {
        Self {
            position: [x, y],
            uv: [u, v],
            color,
        }
    }
}

/// Color blend mode for batch rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlendMode {
    /// Standard Premultiplied Alpha Blending (SrcAlpha + InvSrcAlpha)
    #[default]
    Alpha,
    /// Additive Blending for judgment laser beams, note glows, and audio visualizer
    Additive,
}

/// Lightweight Hardware Abstraction Layer (HAL) for 2D rhythm game graphics backends.
///
/// Designed to be minimal (< 6 core APIs) so that native backends (Direct3D 11, OpenGL, Vulkan, Metal, Soft)
/// can be implemented in a few hundred lines with zero external crate overhead.
pub trait GpuBackend {
    /// Prepares backend for a new frame, resetting draw call counters and clearing framebuffer if needed.
    fn begin_frame(&mut self, width: u32, height: u32, clear_color: [f32; 4]);

    /// Creates an immutable or mutable 2D RGBA8 texture on the GPU / device memory.
    fn create_texture(&mut self, width: u32, height: u32, pixels: &[u8]) -> Option<TextureId>;

    /// Updates dynamic texture contents (such as live BGA video frames or animated sprites).
    fn update_texture(&mut self, id: TextureId, width: u32, height: u32, pixels: &[u8]);

    /// Destroys a texture and reclaims GPU / system memory.
    fn destroy_texture(&mut self, id: TextureId);

    /// Submits a batch of indexed 2D vertices with an optional bound texture and blend mode.
    fn draw_batch(
        &mut self,
        vertices: &[Vertex2D],
        indices: &[u16],
        texture: Option<TextureId>,
        blend_mode: BlendMode,
    );

    /// Finalizes the current frame and presents the swapchain to the display output.
    fn end_frame(&mut self);

    /// Resizes the internal swapchain / backbuffer to match window viewport dimensions.
    fn resize(&mut self, width: u32, height: u32);

    /// Human-readable name of the active graphics backend (e.g. "Direct3D 11", "Software (tiny-skia)").
    fn backend_name(&self) -> &'static str;
}

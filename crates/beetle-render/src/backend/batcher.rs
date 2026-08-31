use super::{BlendMode, GpuBackend, TextureId, Vertex2D};

const MAX_VERTICES: usize = 8192;
const MAX_INDICES: usize = 12288; // 6 indices per 4 vertices (quad)

/// High-performance 2D Quad / Sprite batcher.
///
/// Collects 2D quads, rectangles, and sprites and flushes them in bulk
/// to the underlying `GpuBackend`, minimizing GPU draw calls to 1~3 per frame.
pub struct SpriteBatcher {
    vertices: Vec<Vertex2D>,
    indices: Vec<u16>,
    current_texture: Option<TextureId>,
    current_blend: BlendMode,
    draw_call_count: usize,
}

impl Default for SpriteBatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl SpriteBatcher {
    pub fn new() -> Self {
        Self {
            vertices: Vec::with_capacity(MAX_VERTICES),
            indices: Vec::with_capacity(MAX_INDICES),
            current_texture: None,
            current_blend: BlendMode::Alpha,
            draw_call_count: 0,
        }
    }

    /// Resets the batcher state at the beginning of a frame.
    pub fn begin(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.current_texture = None;
        self.current_blend = BlendMode::Alpha;
        self.draw_call_count = 0;
    }

    /// Total number of draw call batches submitted in the current frame.
    pub fn draw_call_count(&self) -> usize {
        self.draw_call_count
    }

    /// Flushes any pending vertices to the backend.
    pub fn flush(&mut self, backend: &mut dyn GpuBackend) {
        if self.indices.is_empty() {
            return;
        }

        backend.draw_batch(
            &self.vertices,
            &self.indices,
            self.current_texture,
            self.current_blend,
        );
        self.draw_call_count += 1;

        self.vertices.clear();
        self.indices.clear();
    }

    /// Ensures that the current batch matches the requested texture and blend mode,
    /// flushing if either changes or if the buffer is nearly full.
    fn prepare_batch(
        &mut self,
        backend: &mut dyn GpuBackend,
        texture: Option<TextureId>,
        blend: BlendMode,
        needed_vertices: usize,
        needed_indices: usize,
    ) {
        let state_changed = self.current_texture != texture || self.current_blend != blend;
        let buffer_full = (self.vertices.len() + needed_vertices > MAX_VERTICES)
            || (self.indices.len() + needed_indices > MAX_INDICES);

        if state_changed || buffer_full {
            self.flush(backend);
            self.current_texture = texture;
            self.current_blend = blend;
        }
    }

    /// Appends a solid untextured colored rectangle.
    pub fn draw_rect(
        &mut self,
        backend: &mut dyn GpuBackend,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
    ) {
        self.draw_rect_with_blend(backend, x, y, w, h, color, BlendMode::Alpha);
    }

    /// Appends a solid untextured colored rectangle with a specific blend mode.
    pub fn draw_rect_with_blend(
        &mut self,
        backend: &mut dyn GpuBackend,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
        blend: BlendMode,
    ) {
        self.prepare_batch(backend, None, blend, 4, 6);

        let base_idx = self.vertices.len() as u16;

        self.vertices.push(Vertex2D::new(x, y, 0.0, 0.0, color));
        self.vertices.push(Vertex2D::new(x + w, y, 1.0, 0.0, color));
        self.vertices.push(Vertex2D::new(x + w, y + h, 1.0, 1.0, color));
        self.vertices.push(Vertex2D::new(x, y + h, 0.0, 1.0, color));

        self.indices.push(base_idx);
        self.indices.push(base_idx + 1);
        self.indices.push(base_idx + 2);
        self.indices.push(base_idx + 2);
        self.indices.push(base_idx + 3);
        self.indices.push(base_idx);
    }

    /// Appends a textured sprite rectangle using the entire texture.
    pub fn draw_sprite(
        &mut self,
        backend: &mut dyn GpuBackend,
        texture: TextureId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
    ) {
        self.draw_sub_sprite(backend, texture, x, y, w, h, 0.0, 0.0, 1.0, 1.0, color, BlendMode::Alpha);
    }

    /// Appends a textured sub-sprite rectangle with custom UV coordinates and blend mode.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_sub_sprite(
        &mut self,
        backend: &mut dyn GpuBackend,
        texture: TextureId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        u0: f32,
        v0: f32,
        u1: f32,
        v1: f32,
        color: [f32; 4],
        blend: BlendMode,
    ) {
        self.prepare_batch(backend, Some(texture), blend, 4, 6);

        let base_idx = self.vertices.len() as u16;

        self.vertices.push(Vertex2D::new(x, y, u0, v0, color));
        self.vertices.push(Vertex2D::new(x + w, y, u1, v0, color));
        self.vertices.push(Vertex2D::new(x + w, y + h, u1, v1, color));
        self.vertices.push(Vertex2D::new(x, y + h, u0, v1, color));

        self.indices.push(base_idx);
        self.indices.push(base_idx + 1);
        self.indices.push(base_idx + 2);
        self.indices.push(base_idx + 2);
        self.indices.push(base_idx + 3);
        self.indices.push(base_idx);
    }
}

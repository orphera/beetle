use super::{BlendMode, GpuBackend, TextureId, Vertex2D};
use crate::image::ImageBuffer;
use crate::skin::ColorRgba;
use std::collections::HashMap;
use tiny_skia::Pixmap;

/// Software rasterizer implementation of `GpuBackend` using `tiny-skia`.
///
/// Functions as the bulletproof, zero-GPU-dependency fallback on all platforms.
pub struct SoftBackend {
    pixmap: Pixmap,
    textures: HashMap<TextureId, ImageBuffer>,
    next_texture_id: u32,
    width: u32,
    height: u32,
}

impl SoftBackend {
    pub fn new(width: u32, height: u32) -> Self {
        let w = width.max(1);
        let h = height.max(1);
        let pixmap = Pixmap::new(w, h).unwrap_or_else(|| Pixmap::new(1, 1).unwrap());
        Self {
            pixmap,
            textures: HashMap::new(),
            next_texture_id: 1,
            width: w,
            height: h,
        }
    }

    /// Access the underlying `tiny-skia` pixmap.
    pub fn pixmap(&self) -> &Pixmap {
        &self.pixmap
    }

    /// Access mutable underlying `tiny-skia` pixmap.
    pub fn pixmap_mut(&mut self) -> &mut Pixmap {
        &mut self.pixmap
    }

    /// Raw RGBA8 pixel data slice of the rendered frame.
    pub fn data(&self) -> &[u8] {
        self.pixmap.data()
    }
}

impl GpuBackend for SoftBackend {
    fn begin_frame(&mut self, width: u32, height: u32, clear_color: [f32; 4]) {
        if self.width != width || self.height != height {
            self.resize(width, height);
        }

        let r = (clear_color[0].clamp(0.0, 1.0) * 255.0) as u8;
        let g = (clear_color[1].clamp(0.0, 1.0) * 255.0) as u8;
        let b = (clear_color[2].clamp(0.0, 1.0) * 255.0) as u8;
        let a = (clear_color[3].clamp(0.0, 1.0) * 255.0) as u8;

        let data = self.pixmap.data_mut();
        for chunk in data.chunks_exact_mut(4) {
            chunk[0] = r;
            chunk[1] = g;
            chunk[2] = b;
            chunk[3] = a;
        }
    }

    fn create_texture(&mut self, width: u32, height: u32, pixels: &[u8]) -> Option<TextureId> {
        let id = TextureId(self.next_texture_id);
        self.next_texture_id += 1;

        let pixel_count = (width * height) as usize;
        let mut img_pixels = Vec::with_capacity(pixel_count);

        for chunk in pixels.chunks_exact(4) {
            img_pixels.push(ColorRgba::new(chunk[0], chunk[1], chunk[2], chunk[3]));
        }

        self.textures.insert(
            id,
            ImageBuffer {
                width,
                height,
                pixels: img_pixels,
            },
        );

        Some(id)
    }

    fn update_texture(&mut self, id: TextureId, width: u32, height: u32, pixels: &[u8]) {
        if let Some(tex) = self.textures.get_mut(&id) {
            tex.width = width;
            tex.height = height;
            tex.pixels.clear();
            tex.pixels.reserve((width * height) as usize);
            for chunk in pixels.chunks_exact(4) {
                tex.pixels.push(ColorRgba::new(chunk[0], chunk[1], chunk[2], chunk[3]));
            }
        }
    }

    fn destroy_texture(&mut self, id: TextureId) {
        self.textures.remove(&id);
    }

    fn draw_batch(
        &mut self,
        vertices: &[Vertex2D],
        indices: &[u16],
        texture: Option<TextureId>,
        blend_mode: BlendMode,
    ) {
        let bound_tex = texture.and_then(|id| self.textures.get(&id));
        let target_w = self.width as i32;
        let target_h = self.height as i32;
        let data = self.pixmap.data_mut();

        // Process quads (each quad has 6 indices: 0, 1, 2, 2, 3, 0)
        for quad_indices in indices.chunks_exact(6) {
            let v0 = &vertices[quad_indices[0] as usize];
            let v1 = &vertices[quad_indices[1] as usize];
            let v2 = &vertices[quad_indices[2] as usize];
            let v3 = &vertices[quad_indices[4] as usize]; // 4th vertex of quad

            let min_x = v0.position[0].min(v1.position[0]).min(v2.position[0]).min(v3.position[0]);
            let max_x = v0.position[0].max(v1.position[0]).max(v2.position[0]).max(v3.position[0]);
            let min_y = v0.position[1].min(v1.position[1]).min(v2.position[1]).min(v3.position[1]);
            let max_y = v0.position[1].max(v1.position[1]).max(v2.position[1]).max(v3.position[1]);

            let start_x = (min_x.floor() as i32).clamp(0, target_w);
            let end_x = (max_x.ceil() as i32).clamp(0, target_w);
            let start_y = (min_y.floor() as i32).clamp(0, target_h);
            let end_y = (max_y.ceil() as i32).clamp(0, target_h);

            let quad_w = (max_x - min_x).max(1.0);
            let quad_h = (max_y - min_y).max(1.0);

            let tint_r = v0.color[0];
            let tint_g = v0.color[1];
            let tint_b = v0.color[2];
            let tint_a = v0.color[3];

            if let Some(tex) = bound_tex {
                let u0 = v0.uv[0];
                let v_top = v0.uv[1];
                let u1 = v1.uv[0];
                let v_bottom = v2.uv[1];

                let tex_w = tex.width as f32;
                let tex_h = tex.height as f32;

                for y in start_y..end_y {
                    let ty = (y as f32 - min_y) / quad_h;
                    let tex_v = (v_top + ty * (v_bottom - v_top)).clamp(0.0, 1.0);
                    let src_y = ((tex_v * (tex_h - 1.0)).round() as usize).min(tex.height as usize - 1);
                    let row_offset = src_y * tex.width as usize;

                    for x in start_x..end_x {
                        let tx = (x as f32 - min_x) / quad_w;
                        let tex_u = (u0 + tx * (u1 - u0)).clamp(0.0, 1.0);
                        let src_x = ((tex_u * (tex_w - 1.0)).round() as usize).min(tex.width as usize - 1);

                        let src_color = tex.pixels[row_offset + src_x];
                        let alpha = (src_color.a as f32 / 255.0) * tint_a;
                        if alpha <= 0.001 {
                            continue;
                        }

                        let sr = (src_color.r as f32 * tint_r) as u8;
                        let sg = (src_color.g as f32 * tint_g) as u8;
                        let sb = (src_color.b as f32 * tint_b) as u8;

                        let dst_idx = (y as usize * target_w as usize + x as usize) * 4;
                        if dst_idx + 3 < data.len() {
                            match blend_mode {
                                BlendMode::Alpha => {
                                    if alpha >= 0.999 {
                                        data[dst_idx] = sr;
                                        data[dst_idx + 1] = sg;
                                        data[dst_idx + 2] = sb;
                                        data[dst_idx + 3] = 255;
                                    } else {
                                        let inv_a = 1.0 - alpha;
                                        data[dst_idx] = (sr as f32 * alpha + data[dst_idx] as f32 * inv_a) as u8;
                                        data[dst_idx + 1] = (sg as f32 * alpha + data[dst_idx + 1] as f32 * inv_a) as u8;
                                        data[dst_idx + 2] = (sb as f32 * alpha + data[dst_idx + 2] as f32 * inv_a) as u8;
                                        data[dst_idx + 3] = 255;
                                    }
                                }
                                BlendMode::Additive => {
                                    data[dst_idx] = (data[dst_idx] as f32 + sr as f32 * alpha).min(255.0) as u8;
                                    data[dst_idx + 1] = (data[dst_idx + 1] as f32 + sg as f32 * alpha).min(255.0) as u8;
                                    data[dst_idx + 2] = (data[dst_idx + 2] as f32 + sb as f32 * alpha).min(255.0) as u8;
                                    data[dst_idx + 3] = 255;
                                }
                            }
                        }
                    }
                }
            } else {
                // Untextured colored rect
                let sr = (tint_r * 255.0) as u8;
                let sg = (tint_g * 255.0) as u8;
                let sb = (tint_b * 255.0) as u8;
                let alpha = tint_a;

                if alpha <= 0.001 {
                    continue;
                }

                for y in start_y..end_y {
                    for x in start_x..end_x {
                        let dst_idx = (y as usize * target_w as usize + x as usize) * 4;
                        if dst_idx + 3 < data.len() {
                            match blend_mode {
                                BlendMode::Alpha => {
                                    if alpha >= 0.999 {
                                        data[dst_idx] = sr;
                                        data[dst_idx + 1] = sg;
                                        data[dst_idx + 2] = sb;
                                        data[dst_idx + 3] = 255;
                                    } else {
                                        let inv_a = 1.0 - alpha;
                                        data[dst_idx] = (sr as f32 * alpha + data[dst_idx] as f32 * inv_a) as u8;
                                        data[dst_idx + 1] = (sg as f32 * alpha + data[dst_idx + 1] as f32 * inv_a) as u8;
                                        data[dst_idx + 2] = (sb as f32 * alpha + data[dst_idx + 2] as f32 * inv_a) as u8;
                                        data[dst_idx + 3] = 255;
                                    }
                                }
                                BlendMode::Additive => {
                                    data[dst_idx] = (data[dst_idx] as f32 + sr as f32 * alpha).min(255.0) as u8;
                                    data[dst_idx + 1] = (data[dst_idx + 1] as f32 + sg as f32 * alpha).min(255.0) as u8;
                                    data[dst_idx + 2] = (data[dst_idx + 2] as f32 + sb as f32 * alpha).min(255.0) as u8;
                                    data[dst_idx + 3] = 255;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn end_frame(&mut self) {
        // Frame complete in memory
    }

    fn resize(&mut self, width: u32, height: u32) {
        let w = width.max(1);
        let h = height.max(1);
        self.width = w;
        self.height = h;
        if self.pixmap.width() != w || self.pixmap.height() != h {
            if let Some(new_pixmap) = Pixmap::new(w, h) {
                self.pixmap = new_pixmap;
            }
        }
    }

    fn backend_name(&self) -> &'static str {
        "Software (tiny-skia)"
    }
}

#[cfg(test)]
mod tests {
    use super::super::batcher::SpriteBatcher;
    use super::*;

    #[test]
    fn test_soft_backend_solid_rect_rendering() {
        let mut backend = SoftBackend::new(100, 100);
        let mut batcher = SpriteBatcher::new();

        backend.begin_frame(100, 100, [0.0, 0.0, 0.0, 1.0]);
        batcher.begin();

        // Draw red rectangle at (10, 10, 20, 20)
        batcher.draw_rect(&mut backend, 10.0, 10.0, 20.0, 20.0, [1.0, 0.0, 0.0, 1.0]);
        batcher.flush(&mut backend);
        backend.end_frame();

        let data = backend.data();
        // Check pixel at (20, 20)
        let idx = (20 * 100 + 20) * 4;
        assert_eq!(data[idx], 255); // R
        assert_eq!(data[idx + 1], 0); // G
        assert_eq!(data[idx + 2], 0); // B
        assert_eq!(data[idx + 3], 255); // A
    }

    #[test]
    fn test_soft_backend_textured_sprite_and_additive_blend() {
        let mut backend = SoftBackend::new(100, 100);
        let mut batcher = SpriteBatcher::new();

        // 2x2 Texture: Top-left Red, Top-right Green, Bottom-left Blue, Bottom-right White
        let tex_raw = [
            255, 0, 0, 255,   0, 255, 0, 255,
            0, 0, 255, 255,   255, 255, 255, 255,
        ];
        let tex_id = backend.create_texture(2, 2, &tex_raw).expect("create texture");

        backend.begin_frame(100, 100, [0.0, 0.0, 0.0, 1.0]);
        batcher.begin();

        // Draw textured sprite at (0, 0, 50, 50)
        batcher.draw_sprite(&mut backend, tex_id, 0.0, 0.0, 50.0, 50.0, [1.0, 1.0, 1.0, 1.0]);

        // Draw additive beam at (0, 0, 50, 50)
        batcher.draw_rect_with_blend(
            &mut backend,
            0.0,
            0.0,
            50.0,
            50.0,
            [0.5, 0.5, 0.5, 1.0],
            BlendMode::Additive,
        );

        batcher.flush(&mut backend);
        backend.end_frame();

        let data = backend.data();
        let idx = (10 * 100 + 10) * 4; // Top-left quad (Red + additive gray)
        assert!(data[idx] > 200); // High red
        assert!(data[idx + 1] > 100); // Additive green
        assert!(data[idx + 2] > 100); // Additive blue
    }
}

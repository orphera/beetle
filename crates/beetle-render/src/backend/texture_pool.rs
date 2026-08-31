use super::{GpuBackend, TextureId};
use crate::image::ImageBuffer;
use std::collections::HashMap;

/// Manages caching and streaming of static and dynamic textures (BGA images, video frames, jacket art).
pub struct GpuTexturePool {
    textures: HashMap<u32, TextureId>,
    dynamic_video_tex: Option<TextureId>,
    dynamic_video_dims: (u32, u32),
}

impl Default for GpuTexturePool {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuTexturePool {
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
            dynamic_video_tex: None,
            dynamic_video_dims: (0, 0),
        }
    }

    /// Uploads or retrieves a static image from the texture pool.
    pub fn get_or_upload_image(
        &mut self,
        backend: &mut dyn GpuBackend,
        key: u32,
        img: &ImageBuffer,
    ) -> Option<TextureId> {
        if let Some(&id) = self.textures.get(&key) {
            return Some(id);
        }

        let mut raw_rgba = Vec::with_capacity(img.pixels.len() * 4);
        for p in &img.pixels {
            raw_rgba.push(p.r);
            raw_rgba.push(p.g);
            raw_rgba.push(p.b);
            raw_rgba.push(p.a);
        }

        if let Some(id) = backend.create_texture(img.width, img.height, &raw_rgba) {
            self.textures.insert(key, id);
            Some(id)
        } else {
            None
        }
    }

    /// Uploads or updates a streaming video/BGA frame dynamically in VRAM without re-allocation.
    pub fn update_video_frame(
        &mut self,
        backend: &mut dyn GpuBackend,
        width: u32,
        height: u32,
        rgba_pixels: &[u8],
    ) -> Option<TextureId> {
        if width == 0 || height == 0 || rgba_pixels.is_empty() {
            return None;
        }

        if let Some(id) = self.dynamic_video_tex {
            if self.dynamic_video_dims == (width, height) {
                backend.update_texture(id, width, height, rgba_pixels);
                return Some(id);
            } else {
                backend.destroy_texture(id);
                self.dynamic_video_tex = None;
            }
        }

        if let Some(id) = backend.create_texture(width, height, rgba_pixels) {
            self.dynamic_video_tex = Some(id);
            self.dynamic_video_dims = (width, height);
            Some(id)
        } else {
            None
        }
    }

    /// Clears all textures and releases GPU memory.
    pub fn clear(&mut self, backend: &mut dyn GpuBackend) {
        for (_, id) in self.textures.drain() {
            backend.destroy_texture(id);
        }
        if let Some(id) = self.dynamic_video_tex.take() {
            backend.destroy_texture(id);
        }
        self.dynamic_video_dims = (0, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::SoftBackend;
    use crate::skin::ColorRgba;

    #[test]
    fn test_gpu_texture_pool_caching_and_video_updates() {
        let mut backend = SoftBackend::new(100, 100);
        let mut pool = GpuTexturePool::new();

        let img = ImageBuffer {
            width: 2,
            height: 2,
            pixels: vec![ColorRgba::new(255, 0, 0, 255); 4],
        };

        // 1. Initial upload
        let id1 = pool.get_or_upload_image(&mut backend, 10, &img);
        assert!(id1.is_some());

        // 2. Cache hit returns same texture ID
        let id2 = pool.get_or_upload_image(&mut backend, 10, &img);
        assert_eq!(id1, id2);

        // 3. Dynamic video frame streaming
        let video_frame1 = [0u8; 16]; // 2x2 RGBA
        let vid_id1 = pool.update_video_frame(&mut backend, 2, 2, &video_frame1);
        assert!(vid_id1.is_some());

        let video_frame2 = [255u8; 16];
        let vid_id2 = pool.update_video_frame(&mut backend, 2, 2, &video_frame2);
        assert_eq!(vid_id1, vid_id2, "Same dimensions should reuse dynamic texture");

        // 4. Clear releases memory
        pool.clear(&mut backend);
        assert!(pool.textures.is_empty());
        assert!(pool.dynamic_video_tex.is_none());
    }
}

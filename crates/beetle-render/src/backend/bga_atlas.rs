use super::{BlendMode, GpuBackend, SpriteBatcher, TextureId};
use crate::image::ImageBuffer;
use crate::skin::ColorRgba;
use bms_package::{BgaAtlasMeta, BgaFrame, GuillotineBinPacker};
use std::collections::BTreeMap;

/// Ahead-Of-Time or runtime builder to assemble multiple individual BGA images into a single Texture Atlas.
pub struct BgaAtlasBuilder {
    padding: u32,
    frames: Vec<(String, ImageBuffer, Option<String>)>, // (key, image, original_filename)
}

impl BgaAtlasBuilder {
    pub fn new(padding: u32) -> Self {
        Self {
            padding,
            frames: Vec::new(),
        }
    }

    /// Adds an image frame to be packed into the atlas.
    pub fn add_frame(
        &mut self,
        key: impl Into<String>,
        image: &ImageBuffer,
        original_filename: Option<String>,
    ) {
        self.frames.push((key.into(), image.clone(), original_filename));
    }

    /// Packs all added images using the 2D Guillotine bin packer and blits them into a single canvas.
    pub fn build(mut self, file_path: impl Into<String>) -> Option<(BgaAtlasMeta, ImageBuffer)> {
        if self.frames.is_empty() {
            let meta = BgaAtlasMeta::new(file_path, 1, 1, BTreeMap::new());
            let canvas = ImageBuffer::new(1, 1, ColorRgba::transparent());
            return Some((meta, canvas));
        }

        // Sort items deterministically for reproducible packaging (INV-6)
        self.frames.sort_by(|a, b| a.0.cmp(&b.0));

        let pack_items: Vec<(String, u32, u32)> = self
            .frames
            .iter()
            .map(|(key, img, _)| (key.clone(), img.width, img.height))
            .collect();

        let packer = GuillotineBinPacker::new(self.padding);
        let packed = packer.pack(&pack_items)?;

        let mut canvas = ImageBuffer::new(packed.width, packed.height, ColorRgba::transparent());
        let mut bga_frames = BTreeMap::new();

        for (key, img, orig_name) in self.frames {
            let Some(rect) = packed.rects.get(&key) else {
                continue;
            };

            img.blit_into(&mut canvas, rect.x, rect.y);

            let frame = BgaFrame::new(rect.x, rect.y, rect.width, rect.height, orig_name);
            bga_frames.insert(key, frame);
        }

        let meta = BgaAtlasMeta::new(file_path, packed.width, packed.height, bga_frames);
        if meta.validate().is_err() {
            return None;
        }

        Some((meta, canvas))
    }
}

/// GPU-accelerated BGA Texture Atlas.
///
/// Holds a single VRAM texture handle containing all BGA frames, stagefile, and banners.
/// Enables 0-texture-switch BGA animations via UV remapped quad sub-sprites with `SpriteBatcher`.
#[derive(Debug, Clone)]
pub struct GpuBgaAtlas {
    pub texture_id: TextureId,
    pub meta: BgaAtlasMeta,
}

impl GpuBgaAtlas {
    /// Uploads an assembled BGA Atlas image and metadata to the GPU backend.
    pub fn new(
        backend: &mut dyn GpuBackend,
        meta: BgaAtlasMeta,
        atlas_image: &ImageBuffer,
    ) -> Option<Self> {
        let raw_bytes = atlas_image.to_raw_rgba_bytes();
        let texture_id = backend.create_texture(atlas_image.width, atlas_image.height, &raw_bytes)?;

        Some(Self { texture_id, meta })
    }

    /// Checks if a frame key (e.g. "01", "stagefile", "banner") exists in the atlas.
    pub fn contains_frame(&self, key: &str) -> bool {
        self.meta.frames.contains_key(key)
    }

    /// Returns the normalized UV coordinates `[u0, v0, u1, v1]` for a given frame key.
    pub fn get_frame_uv(&self, key: &str) -> Option<[f32; 4]> {
        let frame = self.meta.frames.get(key)?;
        Some(frame.uv_rect(self.meta.width, self.meta.height))
    }

    /// Appends a hardware-accelerated BGA quad to the `SpriteBatcher` without texture switching.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_bga_frame(
        &self,
        batcher: &mut SpriteBatcher,
        backend: &mut dyn GpuBackend,
        key: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
        blend: BlendMode,
    ) -> bool {
        let Some(uv) = self.get_frame_uv(key) else {
            return false;
        };

        batcher.draw_sub_sprite(
            backend,
            self.texture_id,
            x,
            y,
            w,
            h,
            uv[0],
            uv[1],
            uv[2],
            uv[3],
            color,
            blend,
        );

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::SoftBackend;

    #[test]
    fn test_bga_atlas_builder_and_gpu_rendering() {
        let mut builder = BgaAtlasBuilder::new(1);

        let img1 = ImageBuffer::new(64, 64, ColorRgba::new(255, 0, 0, 255));
        let img2 = ImageBuffer::new(32, 32, ColorRgba::new(0, 255, 0, 255));
        let stage = ImageBuffer::new(128, 64, ColorRgba::new(0, 0, 255, 255));

        builder.add_frame("01", &img1, Some("01.bmp".to_string()));
        builder.add_frame("02", &img2, Some("02.bmp".to_string()));
        builder.add_frame("stagefile", &stage, Some("stage.png".to_string()));

        let (meta, canvas) = builder.build("visual/atlas.png").expect("build failed");

        assert!(meta.validate().is_ok());
        assert!(canvas.width >= 128);
        assert!(canvas.height >= 128);
        assert_eq!(meta.frames.len(), 3);

        // Upload to SoftBackend (CPU GpuBackend implementation)
        let mut backend = SoftBackend::new(800, 600);
        let gpu_atlas = GpuBgaAtlas::new(&mut backend, meta, &canvas).expect("gpu upload failed");

        assert!(gpu_atlas.contains_frame("01"));
        assert!(gpu_atlas.contains_frame("02"));
        assert!(gpu_atlas.contains_frame("stagefile"));
        assert!(!gpu_atlas.contains_frame("nonexistent"));

        let mut batcher = SpriteBatcher::new();
        let drew = gpu_atlas.draw_bga_frame(
            &mut batcher,
            &mut backend,
            "01",
            100.0,
            100.0,
            200.0,
            200.0,
            [1.0, 1.0, 1.0, 1.0],
            BlendMode::Alpha,
        );

        assert!(drew);
        batcher.flush(&mut backend);
        assert_eq!(batcher.draw_call_count(), 1);
    }
}

use crate::image::ImageBuffer;
use crate::skin::ColorRgba;
use bms_package::{BgaAtlasMeta, BgaFrame, GuillotineBinPacker};
use std::collections::BTreeMap;

/// Packaging-time builder (used by `bms-package-manager`) to assemble multiple individual BGA images into a single Texture Atlas.
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
        self.frames
            .push((key.into(), image.clone(), original_filename));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bga_atlas_builder() {
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

        let f01 = &meta.frames["01"];
        assert_eq!((f01.width, f01.height), (64, 64));
        // The frame's pixels landed where the packer put them.
        let px = canvas.pixels[(f01.y * canvas.width + f01.x) as usize];
        assert_eq!(px, ColorRgba::new(255, 0, 0, 255));
    }
}

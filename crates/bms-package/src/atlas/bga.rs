use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Placement and bounding box of a single BGA image frame or UI artwork inside the BGA Texture Atlas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BgaFrame {
    /// Top-left X pixel coordinate in the atlas canvas.
    pub x: u32,
    /// Top-left Y pixel coordinate in the atlas canvas.
    pub y: u32,
    /// Width of the frame in pixels.
    pub width: u32,
    /// Height of the frame in pixels.
    pub height: u32,
    /// Original image filename before packing (e.g. "bga01.bmp", "stagefile.png"), preserved for VFS and unpacking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_filename: Option<String>,
}

impl BgaFrame {
    pub fn new(x: u32, y: u32, width: u32, height: u32, original_filename: Option<String>) -> Self {
        Self {
            x,
            y,
            width,
            height,
            original_filename,
        }
    }

    /// Computes normalized texture UV coordinates `[u0, v0, u1, v1]` for this frame.
    #[inline(always)]
    pub fn uv_rect(&self, atlas_width: u32, atlas_height: u32) -> [f32; 4] {
        if atlas_width == 0 || atlas_height == 0 {
            return [0.0, 0.0, 1.0, 1.0];
        }
        let w = atlas_width as f32;
        let h = atlas_height as f32;
        [
            (self.x as f32) / w,
            (self.y as f32) / h,
            ((self.x + self.width) as f32) / w,
            ((self.y + self.height) as f32) / h,
        ]
    }
}

/// Metadata describing the BGA Texture Atlas inside a package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BgaAtlasMeta {
    /// Relative path to the atlas image file within the package (e.g. "visual/atlas.png" or "visual/atlas.bmp").
    pub file: String,
    /// Canvas width in pixels (power of two recommended, e.g. 1024, 2048, 4096).
    pub width: u32,
    /// Canvas height in pixels.
    pub height: u32,
    /// Mapping from BmpId or special keys ("01", "ZZ", "stagefile", "banner", "title") to frame rectangles.
    pub frames: BTreeMap<String, BgaFrame>,
}

impl BgaAtlasMeta {
    pub fn new(
        file: impl Into<String>,
        width: u32,
        height: u32,
        frames: BTreeMap<String, BgaFrame>,
    ) -> Self {
        Self {
            file: file.into(),
            width,
            height,
            frames,
        }
    }

    /// Validates that all frames reside strictly within the canvas boundaries.
    pub fn validate(&self) -> Result<(), String> {
        if self.width == 0 || self.height == 0 {
            return Err("Atlas dimensions must be greater than 0".to_string());
        }

        for (key, frame) in &self.frames {
            if frame.x + frame.width > self.width || frame.y + frame.height > self.height {
                return Err(format!(
                    "Frame '{}' ([{}, {}] {}x{}) exceeds atlas boundaries ({}x{})",
                    key, frame.x, frame.y, frame.width, frame.height, self.width, self.height
                ));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bga_atlas_meta_serialization_and_uv() {
        let mut frames = BTreeMap::new();
        frames.insert(
            "01".to_string(),
            BgaFrame::new(0, 0, 256, 256, Some("01.bmp".to_string())),
        );
        frames.insert(
            "stagefile".to_string(),
            BgaFrame::new(256, 0, 640, 480, Some("stage.png".to_string())),
        );

        let meta = BgaAtlasMeta::new("visual/atlas.png", 1024, 1024, frames);
        assert!(meta.validate().is_ok());

        let frame_01 = meta.frames.get("01").unwrap();
        let uv = frame_01.uv_rect(meta.width, meta.height);
        assert_eq!(uv, [0.0, 0.0, 0.25, 0.25]);

        let json = serde_json::to_string_pretty(&meta).expect("serialize failed");
        let deserialized: BgaAtlasMeta = serde_json::from_str(&json).expect("deserialize failed");

        assert_eq!(meta, deserialized);
        assert_eq!(deserialized.frames.len(), 2);
    }
}

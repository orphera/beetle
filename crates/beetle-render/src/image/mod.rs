pub mod bga_atlas;
pub mod bmp;
pub mod jpeg;
pub mod png;

use crate::skin::ColorRgba;
use std::fs;
use std::path::Path;

pub use bga_atlas::BgaAtlasBuilder;
pub use bmp::{decode_bmp, encode_bmp};
#[cfg(feature = "bga-enhanced")]
pub use jpeg::decode_jpeg;
#[cfg(feature = "bga-enhanced")]
pub use png::decode_png;

/// Decoded RGBA image buffer.
#[derive(Debug, Clone)]
pub struct ImageBuffer {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<ColorRgba>, // Row-major: y * width + x
}

impl ImageBuffer {
    pub fn new(width: u32, height: u32, color: ColorRgba) -> Self {
        Self {
            width,
            height,
            pixels: vec![color; (width * height) as usize],
        }
    }

    /// Loads and decodes an image file from disk (BMP, or PNG/JPEG if bga-enhanced feature is enabled).
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Option<Self> {
        let data = fs::read(path).ok()?;
        Self::from_bytes(&data)
    }

    /// Automatically detects the image format from magic bytes and decodes it.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() >= 2 && data[0] == b'B' && data[1] == b'M' {
            return decode_bmp(data);
        }

        #[cfg(feature = "bga-enhanced")]
        {
            if data.len() >= 8 && &data[0..8] == b"\x89PNG\r\n\x1a\n" {
                return decode_png(data);
            }
            if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
                return decode_jpeg(data);
            }
        }

        // Fallback try BMP
        decode_bmp(data)
    }

    /// Decodes a PNG image from byte slice (requires bga-enhanced feature).
    #[cfg(feature = "bga-enhanced")]
    pub fn from_png_bytes(data: &[u8]) -> Option<Self> {
        decode_png(data)
    }

    /// Decodes a JPEG image from byte slice (requires bga-enhanced feature).
    #[cfg(feature = "bga-enhanced")]
    pub fn from_jpeg_bytes(data: &[u8]) -> Option<Self> {
        decode_jpeg(data)
    }

    /// Decodes a 24-bit or 32-bit uncompressed Windows BMP image without external crates.
    pub fn from_bmp_bytes(data: &[u8]) -> Option<Self> {
        decode_bmp(data)
    }

    /// Encodes this image buffer as a 24-bit uncompressed Windows BMP byte vector without external dependencies.
    pub fn encode_bmp_bytes(&self) -> Vec<u8> {
        encode_bmp(self)
    }

    /// Returns a flat RGBA8 byte buffer of this image.
    pub fn to_raw_rgba_bytes(&self) -> Vec<u8> {
        let mut raw = Vec::with_capacity((self.width * self.height * 4) as usize);
        for p in &self.pixels {
            raw.push(p.r);
            raw.push(p.g);
            raw.push(p.b);
            raw.push(p.a);
        }
        raw
    }

    /// Creates a new scaled ImageBuffer.
    pub fn create_scaled(&self, dst_w: u32, dst_h: u32) -> Self {
        if dst_w == 0 || dst_h == 0 || self.width == 0 || self.height == 0 {
            return Self::new(dst_w, dst_h, ColorRgba::transparent());
        }

        let mut pixels = Vec::with_capacity((dst_w * dst_h) as usize);
        for dy in 0..dst_h {
            let src_y = (dy as f32 / dst_h as f32 * self.height as f32) as usize;
            let src_y = src_y.min(self.height as usize - 1);
            let row_offset = src_y * self.width as usize;

            for dx in 0..dst_w {
                let src_x = (dx as f32 / dst_w as f32 * self.width as f32) as usize;
                let src_x = src_x.min(self.width as usize - 1);
                pixels.push(self.pixels[row_offset + src_x]);
            }
        }

        Self {
            width: dst_w,
            height: dst_h,
            pixels,
        }
    }

    /// Samples a coarse grid (`grid` x `grid` points) across the image and
    /// averages them into a single representative color, boosted toward a
    /// richer, slightly darker tone so it reads well as an ambient UI wash
    /// rather than washing out text contrast. Used for the "art color bleed"
    /// background treatment on SongSelect (see
    /// docs/plans/2026-10-03-pulse-redesign.md) — deliberately cheap (at
    /// most grid*grid samples, independent of image resolution) since it can
    /// run every time the selection changes rather than needing a cache.
    pub fn average_color_sampled(&self, grid: u32) -> ColorRgba {
        if self.width == 0 || self.height == 0 || self.pixels.is_empty() {
            return ColorRgba::new(0, 0, 0, 255);
        }
        let grid = grid.max(1);
        let mut sum_r: u64 = 0;
        let mut sum_g: u64 = 0;
        let mut sum_b: u64 = 0;
        let mut count: u64 = 0;

        for gy in 0..grid {
            let y = ((gy as f32 + 0.5) / grid as f32 * self.height as f32) as u32;
            let y = y.min(self.height - 1);
            for gx in 0..grid {
                let x = ((gx as f32 + 0.5) / grid as f32 * self.width as f32) as u32;
                let x = x.min(self.width - 1);
                let p = self.pixels[(y * self.width + x) as usize];
                sum_r += p.r as u64;
                sum_g += p.g as u64;
                sum_b += p.b as u64;
                count += 1;
            }
        }

        if count == 0 {
            return ColorRgba::new(0, 0, 0, 255);
        }

        ColorRgba::new(
            (sum_r / count) as u8,
            (sum_g / count) as u8,
            (sum_b / count) as u8,
            255,
        )
    }

    /// Copies this image directly into another destination ImageBuffer at the specified (dst_x, dst_y) coordinates.
    pub fn blit_into(&self, dst: &mut ImageBuffer, dst_x: u32, dst_y: u32) {
        let src_w = self.width;
        let src_h = self.height;
        let dst_w = dst.width;
        let dst_h = dst.height;

        for dy in 0..src_h {
            let py = dst_y + dy;
            if py >= dst_h {
                break;
            }
            let src_row = (dy * src_w) as usize;
            let dst_row = (py * dst_w) as usize;

            let copy_w = src_w.min(dst_w.saturating_sub(dst_x)) as usize;
            if copy_w == 0 {
                continue;
            }
            let src_start = src_row;
            let src_end = src_start + copy_w;
            let dst_start = dst_row + dst_x as usize;
            let dst_end = dst_start + copy_w;

            dst.pixels[dst_start..dst_end].copy_from_slice(&self.pixels[src_start..src_end]);
        }
    }

    /// Extracts a sub-rectangle from this image as a new ImageBuffer.
    /// Returns None if the requested rectangle is out of bounds or has zero width/height.
    pub fn crop(&self, x: u32, y: u32, w: u32, h: u32) -> Option<Self> {
        if w == 0 || h == 0 || x.saturating_add(w) > self.width || y.saturating_add(h) > self.height
        {
            return None;
        }

        let mut pixels = Vec::with_capacity((w * h) as usize);
        for row in 0..h {
            let src_row = (y + row) * self.width;
            let start = (src_row + x) as usize;
            let end = start + w as usize;
            pixels.extend_from_slice(&self.pixels[start..end]);
        }

        Some(Self {
            width: w,
            height: h,
            pixels,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_synthetic_24bit_bmp(w: u32, h: u32, bgr_color: (u8, u8, u8)) -> Vec<u8> {
        let img = ImageBuffer::new(
            w,
            h,
            ColorRgba::new(bgr_color.2, bgr_color.1, bgr_color.0, 255),
        );
        img.encode_bmp_bytes()
    }

    #[test]
    fn test_bmp_decoder_24bit() {
        let bmp_data = create_synthetic_24bit_bmp(2, 3, (255, 128, 64)); // B=255, G=128, R=64
        let img = ImageBuffer::from_bmp_bytes(&bmp_data).expect("Failed to decode synthetic BMP");

        assert_eq!(img.width, 2);
        assert_eq!(img.height, 3);
        assert_eq!(img.pixels.len(), 6);
        assert_eq!(img.pixels[0], ColorRgba::new(64, 128, 255, 255));
    }

    #[test]
    fn test_bmp_decoder_8bit_paletted() {
        let mut data = Vec::new();
        data.extend_from_slice(b"BM");
        let file_size: u32 = 14 + 40 + (256 * 4) + 8;
        data.extend_from_slice(&file_size.to_le_bytes());
        data.extend_from_slice(&[0; 4]);
        let data_offset: u32 = 14 + 40 + (256 * 4);
        data.extend_from_slice(&data_offset.to_le_bytes());

        // DIB Header
        data.extend_from_slice(&40u32.to_le_bytes());
        data.extend_from_slice(&2i32.to_le_bytes()); // w
        data.extend_from_slice(&2i32.to_le_bytes()); // h
        data.extend_from_slice(&1u16.to_le_bytes()); // planes
        data.extend_from_slice(&8u16.to_le_bytes()); // bpp = 8
        data.extend_from_slice(&0u32.to_le_bytes()); // compression = 0
        data.extend_from_slice(&8u32.to_le_bytes()); // image size
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&256u32.to_le_bytes()); // colors used
        data.extend_from_slice(&0u32.to_le_bytes());

        // Palette: color 0 = Red (B=0, G=0, R=255), color 1 = Blue (B=255, G=0, R=0)
        data.extend_from_slice(&[0, 0, 255, 0]);
        data.extend_from_slice(&[255, 0, 0, 0]);
        for _ in 2..256 {
            data.extend_from_slice(&[0, 0, 0, 0]);
        }

        // Pixel data (bottom-up: bottom row first, top row second)
        data.extend_from_slice(&[0, 1, 0, 0]); // bottom row: Red, Blue
        data.extend_from_slice(&[1, 0, 0, 0]); // top row: Blue, Red

        let img = ImageBuffer::from_bmp_bytes(&data).expect("Failed to decode 8-bit paletted BMP");
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 2);
        assert_eq!(img.pixels[0], ColorRgba::new(0, 0, 255, 255)); // Top-left: Blue
        assert_eq!(img.pixels[1], ColorRgba::new(255, 0, 0, 255)); // Top-right: Red
        assert_eq!(img.pixels[2], ColorRgba::new(255, 0, 0, 255)); // Bottom-left: Red
        assert_eq!(img.pixels[3], ColorRgba::new(0, 0, 255, 255)); // Bottom-right: Blue
    }

    #[test]
    fn test_from_bytes_magic_detection() {
        let bmp_data = create_synthetic_24bit_bmp(2, 2, (10, 20, 30));
        let img = ImageBuffer::from_bytes(&bmp_data).expect("Should decode BMP from generic bytes");
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 2);
    }

    #[cfg(feature = "bga-enhanced")]
    #[test]
    fn test_png_decoder_under_bga_enhanced() {
        let png_bytes: &[u8] = &[
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x00, 0x00,
            0x00, 0x3a, 0x7e, 0x9b, 0x55, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9c, 0x63, 0x60, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0x48, 0xaf, 0xa4, 0x71, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
        ];
        let img = ImageBuffer::from_bytes(png_bytes).expect("Should decode PNG via from_bytes");
        assert_eq!(img.width, 1);
        assert_eq!(img.height, 1);
        assert_eq!(img.pixels.len(), 1);
        assert_eq!(img.pixels[0], ColorRgba::new(0, 0, 0, 255));
    }

    #[test]
    fn test_image_buffer_crop() {
        let mut img = ImageBuffer::new(10, 10, ColorRgba::new(0, 0, 0, 255));
        img.pixels[2 * 10 + 3] = ColorRgba::new(255, 128, 64, 255);

        let cropped = img.crop(3, 2, 4, 4).expect("crop failed");
        assert_eq!(cropped.width, 4);
        assert_eq!(cropped.height, 4);
        assert_eq!(cropped.pixels[0], ColorRgba::new(255, 128, 64, 255));

        assert!(img.crop(8, 8, 4, 4).is_none());
        assert!(img.crop(0, 0, 0, 5).is_none());
    }
}

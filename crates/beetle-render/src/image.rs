use crate::skin::ColorRgba;
use std::fs;
use std::path::Path;
use tiny_skia::Pixmap;

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

    /// Loads and decodes a BMP image file from disk.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Option<Self> {
        let data = fs::read(path).ok()?;
        Self::from_bmp_bytes(&data)
    }

    /// Decodes a 24-bit or 32-bit uncompressed Windows BMP image without external crates.
    pub fn from_bmp_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 54 {
            return None;
        }

        // 1. Magic check 'BM'
        if data[0] != b'B' || data[1] != b'M' {
            return None;
        }

        // 2. Pixel data offset
        let data_offset = u32::from_le_bytes(data[10..14].try_into().ok()?) as usize;

        // 3. DIB Header
        let width_raw = i32::from_le_bytes(data[18..22].try_into().ok()?);
        let height_raw = i32::from_le_bytes(data[22..26].try_into().ok()?);

        if width_raw <= 0 || height_raw == 0 {
            return None;
        }

        let width = width_raw as u32;
        let height = height_raw.unsigned_abs();
        let is_top_down = height_raw < 0;

        let planes = u16::from_le_bytes(data[26..28].try_into().ok()?);
        if planes != 1 {
            return None;
        }

        let bpp = u16::from_le_bytes(data[28..30].try_into().ok()?);
        if bpp != 24 && bpp != 32 {
            return None;
        }

        if data.len() < data_offset {
            return None;
        }

        let mut pixels = vec![ColorRgba::transparent(); (width * height) as usize];
        let bytes_per_pixel = (bpp / 8) as usize;
        let row_stride_unpadded = width as usize * bytes_per_pixel;
        let row_padding = (4 - (row_stride_unpadded % 4)) % 4;
        let row_stride = row_stride_unpadded + row_padding;

        let raw_pixels = &data[data_offset..];

        for y in 0..height as usize {
            let src_y = if is_top_down {
                y
            } else {
                (height as usize - 1) - y
            };

            let row_start = src_y * row_stride;
            if row_start + row_stride_unpadded > raw_pixels.len() {
                break;
            }

            let row = &raw_pixels[row_start..row_start + row_stride_unpadded];

            for x in 0..width as usize {
                let px_idx = x * bytes_per_pixel;
                let b = row[px_idx];
                let g = row[px_idx + 1];
                let r = row[px_idx + 2];
                let a = if bytes_per_pixel == 4 { row[px_idx + 3] } else { 255 };

                pixels[y * width as usize + x] = ColorRgba::new(r, g, b, a);
            }
        }

        Some(Self {
            width,
            height,
            pixels,
        })
    }

    /// Blits and scales the image into a target area on the tiny-skia Pixmap.
    pub fn draw_scaled(
        &self,
        pixmap: &mut Pixmap,
        dst_x: i32,
        dst_y: i32,
        dst_w: u32,
        dst_h: u32,
    ) {
        if dst_w == 0 || dst_h == 0 || self.width == 0 || self.height == 0 {
            return;
        }

        let target_w = pixmap.width() as i32;
        let target_h = pixmap.height() as i32;
        let data = pixmap.data_mut();

        for dy in 0..dst_h as i32 {
            let py = dst_y + dy;
            if py < 0 || py >= target_h {
                continue;
            }

            let src_y = (dy as f32 / dst_h as f32 * self.height as f32) as usize;
            let src_y = src_y.min(self.height as usize - 1);

            for dx in 0..dst_w as i32 {
                let px = dst_x + dx;
                if px < 0 || px >= target_w {
                    continue;
                }

                let src_x = (dx as f32 / dst_w as f32 * self.width as f32) as usize;
                let src_x = src_x.min(self.width as usize - 1);

                let color = self.pixels[src_y * self.width as usize + src_x];
                if color.a == 0 {
                    continue;
                }

                let dst_idx = (py as usize * target_w as usize + px as usize) * 4;
                if dst_idx + 3 < data.len() {
                    if color.a == 255 {
                        data[dst_idx] = color.r;
                        data[dst_idx + 1] = color.g;
                        data[dst_idx + 2] = color.b;
                        data[dst_idx + 3] = 255;
                    } else {
                        // Alpha blend
                        let alpha = color.a as f32 / 255.0;
                        let inv_a = 1.0 - alpha;
                        data[dst_idx] = (color.r as f32 * alpha + data[dst_idx] as f32 * inv_a) as u8;
                        data[dst_idx + 1] = (color.g as f32 * alpha + data[dst_idx + 1] as f32 * inv_a) as u8;
                        data[dst_idx + 2] = (color.b as f32 * alpha + data[dst_idx + 2] as f32 * inv_a) as u8;
                        data[dst_idx + 3] = 255;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_synthetic_24bit_bmp(w: u32, h: u32, bgr_color: (u8, u8, u8)) -> Vec<u8> {
        let row_stride_unpadded = (w * 3) as usize;
        let row_padding = (4 - (row_stride_unpadded % 4)) % 4;
        let row_stride = row_stride_unpadded + row_padding;
        let pixel_bytes_len = row_stride * h as usize;
        let total_size = 54 + pixel_bytes_len;

        let mut data = Vec::with_capacity(total_size);
        // Header
        data.extend_from_slice(b"BM");
        data.extend_from_slice(&(total_size as u32).to_le_bytes()); // File size
        data.extend_from_slice(&[0, 0, 0, 0]); // Reserved
        data.extend_from_slice(&54u32.to_le_bytes()); // Offset to pixels

        // DIB Header
        data.extend_from_slice(&40u32.to_le_bytes()); // Header size
        data.extend_from_slice(&(w as i32).to_le_bytes());
        data.extend_from_slice(&(h as i32).to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes()); // Planes
        data.extend_from_slice(&24u16.to_le_bytes()); // BPP
        data.extend_from_slice(&0u32.to_le_bytes()); // Compression
        data.extend_from_slice(&(pixel_bytes_len as u32).to_le_bytes());
        data.extend_from_slice(&2835u32.to_le_bytes());
        data.extend_from_slice(&2835u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());

        // Pixels
        for _ in 0..h {
            for _ in 0..w {
                data.push(bgr_color.0); // B
                data.push(bgr_color.1); // G
                data.push(bgr_color.2); // R
            }
            for _ in 0..row_padding {
                data.push(0);
            }
        }

        data
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
}

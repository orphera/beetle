pub mod bmp;
pub mod jpeg;
pub mod png;

use crate::skin::ColorRgba;
use std::fs;
use std::path::Path;
use tiny_skia::Pixmap;

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

/// Image fitting mode when drawing into a target viewport rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageFitMode {
    /// Stretches to fill the entire target area ignoring original aspect ratio.
    Stretch,
    /// Centers and crops the image to fill the target rectangle without aspect ratio distortion.
    #[default]
    FillCrop,
    /// Fits the entire image within the target area with dark padding / letterbox.
    FitLetterbox,
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

    /// Blits this image (without scaling) directly into the target pixmap.
    pub fn blit_to(&self, pixmap: &mut Pixmap, dst_x: i32, dst_y: i32) {
        let target_w = pixmap.width() as i32;
        let target_h = pixmap.height() as i32;
        let data = pixmap.data_mut();
        let src_w = self.width as i32;
        let src_h = self.height as i32;

        for dy in 0..src_h {
            let py = dst_y + dy;
            if py < 0 || py >= target_h {
                continue;
            }

            let row_src_idx = (dy as usize) * (self.width as usize);

            for dx in 0..src_w {
                let px = dst_x + dx;
                if px < 0 || px >= target_w {
                    continue;
                }

                let color = self.pixels[row_src_idx + dx as usize];
                if color.a == 0 {
                    continue;
                }

                let dst_idx = (py as usize * target_w as usize + px as usize) * 4;
                if dst_idx + 3 < data.len() {
                    data[dst_idx] = color.r;
                    data[dst_idx + 1] = color.g;
                    data[dst_idx + 2] = color.b;
                    data[dst_idx + 3] = 255;
                }
            }
        }
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

        // Precompute horizontal sample mapping to eliminate per-pixel floating point division
        let mut src_x_table = Vec::with_capacity(dst_w as usize);
        for dx in 0..dst_w {
            let sx = (dx as f32 / dst_w as f32 * self.width as f32) as usize;
            src_x_table.push(sx.min(self.width as usize - 1));
        }

        for dy in 0..dst_h as i32 {
            let py = dst_y + dy;
            if py < 0 || py >= target_h {
                continue;
            }

            let src_y = (dy as f32 / dst_h as f32 * self.height as f32) as usize;
            let src_y = src_y.min(self.height as usize - 1);
            let row_offset = src_y * self.width as usize;

            for dx in 0..dst_w as i32 {
                let px = dst_x + dx;
                if px < 0 || px >= target_w {
                    continue;
                }

                let src_x = src_x_table[dx as usize];
                let color = self.pixels[row_offset + src_x];
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
                        // Fast integer alpha blend
                        let a = color.a as u32;
                        let inv_a = 255 - a;
                        data[dst_idx] = ((color.r as u32 * a + data[dst_idx] as u32 * inv_a) / 255) as u8;
                        data[dst_idx + 1] = ((color.g as u32 * a + data[dst_idx + 1] as u32 * inv_a) / 255) as u8;
                        data[dst_idx + 2] = ((color.b as u32 * a + data[dst_idx + 2] as u32 * inv_a) / 255) as u8;
                        data[dst_idx + 3] = 255;
                    }
                }
            }
        }
    }

    /// Blits the image fitted into a target rectangle according to `ImageFitMode`.
    pub fn draw_fitted(
        &self,
        pixmap: &mut Pixmap,
        dst_x: i32,
        dst_y: i32,
        dst_w: u32,
        dst_h: u32,
        fit_mode: ImageFitMode,
    ) {
        if dst_w == 0 || dst_h == 0 || self.width == 0 || self.height == 0 {
            return;
        }

        match fit_mode {
            ImageFitMode::Stretch => {
                self.draw_scaled(pixmap, dst_x, dst_y, dst_w, dst_h);
            }
            ImageFitMode::FillCrop => {
                let target_w = pixmap.width() as i32;
                let target_h = pixmap.height() as i32;
                let data = pixmap.data_mut();

                let scale_x = dst_w as f32 / self.width as f32;
                let scale_y = dst_h as f32 / self.height as f32;
                let scale = scale_x.max(scale_y);

                let src_view_w = dst_w as f32 / scale;
                let src_view_h = dst_h as f32 / scale;
                let src_origin_x = (self.width as f32 - src_view_w) / 2.0;
                let src_origin_y = (self.height as f32 - src_view_h) / 2.0;

                for dy in 0..dst_h as i32 {
                    let py = dst_y + dy;
                    if py < 0 || py >= target_h {
                        continue;
                    }

                    let sy = (src_origin_y + (dy as f32 / dst_h as f32) * src_view_h) as i32;
                    if sy < 0 || sy >= self.height as i32 {
                        continue;
                    }
                    let row_src_idx = (sy as usize) * (self.width as usize);

                    for dx in 0..dst_w as i32 {
                        let px = dst_x + dx;
                        if px < 0 || px >= target_w {
                            continue;
                        }

                        let sx = (src_origin_x + (dx as f32 / dst_w as f32) * src_view_w) as i32;
                        if sx < 0 || sx >= self.width as i32 {
                            continue;
                        }

                        let color = self.pixels[row_src_idx + sx as usize];
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
            ImageFitMode::FitLetterbox => {
                let scale_x = dst_w as f32 / self.width as f32;
                let scale_y = dst_h as f32 / self.height as f32;
                let scale = scale_x.min(scale_y);

                let fit_w = (self.width as f32 * scale).round() as u32;
                let fit_h = (self.height as f32 * scale).round() as u32;
                let offset_x = (dst_w.saturating_sub(fit_w)) / 2;
                let offset_y = (dst_h.saturating_sub(fit_h)) / 2;

                self.draw_scaled(
                    pixmap,
                    dst_x + offset_x as i32,
                    dst_y + offset_y as i32,
                    fit_w,
                    fit_h,
                );
            }
        }
    }

    /// Draws this image fitted into the destination box, treating RGB(0,0,0) black or transparent pixels as transparent color-key.
    /// Used for BMS Layer BGA (Channel 07) overlays on top of background video or images.
    pub fn draw_color_keyed(
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

        let scale_x = dst_w as f32 / self.width as f32;
        let scale_y = dst_h as f32 / self.height as f32;
        let scale = scale_x.max(scale_y);

        let src_view_w = dst_w as f32 / scale;
        let src_view_h = dst_h as f32 / scale;
        let src_origin_x = (self.width as f32 - src_view_w) / 2.0;
        let src_origin_y = (self.height as f32 - src_view_h) / 2.0;

        for dy in 0..dst_h as i32 {
            let py = dst_y + dy;
            if py < 0 || py >= target_h {
                continue;
            }

            let sy = (src_origin_y + (dy as f32 / dst_h as f32) * src_view_h) as i32;
            if sy < 0 || sy >= self.height as i32 {
                continue;
            }
            let row_src_idx = (sy as usize) * (self.width as usize);

            for dx in 0..dst_w as i32 {
                let px = dst_x + dx;
                if px < 0 || px >= target_w {
                    continue;
                }

                let sx = (src_origin_x + (dx as f32 / dst_w as f32) * src_view_w) as i32;
                if sx < 0 || sx >= self.width as i32 {
                    continue;
                }

                let color = self.pixels[row_src_idx + sx as usize];
                if color.a == 0 || (color.r == 0 && color.g == 0 && color.b == 0) {
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

    /// Draws this image fitted into the destination box with a specified global opacity (0.0 .. 1.0).
    pub fn draw_fitted_with_opacity(
        &self,
        pixmap: &mut Pixmap,
        dst_x: i32,
        dst_y: i32,
        dst_w: u32,
        dst_h: u32,
        fit_mode: ImageFitMode,
        global_opacity: f32,
    ) {
        if global_opacity <= 0.0 || dst_w == 0 || dst_h == 0 || self.width == 0 || self.height == 0 {
            return;
        }

        let opacity = global_opacity.clamp(0.0, 1.0);
        let target_w = pixmap.width() as i32;
        let target_h = pixmap.height() as i32;
        let data = pixmap.data_mut();

        match fit_mode {
            ImageFitMode::Stretch | ImageFitMode::FillCrop => {
                let (src_view_w, src_view_h, src_origin_x, src_origin_y) = if fit_mode == ImageFitMode::Stretch {
                    (self.width as f32, self.height as f32, 0.0, 0.0)
                } else {
                    let scale_x = dst_w as f32 / self.width as f32;
                    let scale_y = dst_h as f32 / self.height as f32;
                    let scale = scale_x.max(scale_y);

                    let src_view_w = dst_w as f32 / scale;
                    let src_view_h = dst_h as f32 / scale;
                    let src_origin_x = (self.width as f32 - src_view_w) / 2.0;
                    let src_origin_y = (self.height as f32 - src_view_h) / 2.0;
                    (src_view_w, src_view_h, src_origin_x, src_origin_y)
                };

                for dy in 0..dst_h as i32 {
                    let py = dst_y + dy;
                    if py < 0 || py >= target_h {
                        continue;
                    }

                    let sy = (src_origin_y + (dy as f32 / dst_h as f32) * src_view_h) as i32;
                    if sy < 0 || sy >= self.height as i32 {
                        continue;
                    }
                    let row_src_idx = (sy as usize) * (self.width as usize);

                    for dx in 0..dst_w as i32 {
                        let px = dst_x + dx;
                        if px < 0 || px >= target_w {
                            continue;
                        }

                        let sx = (src_origin_x + (dx as f32 / dst_w as f32) * src_view_w) as i32;
                        if sx < 0 || sx >= self.width as i32 {
                            continue;
                        }

                        let color = self.pixels[row_src_idx + sx as usize];
                        if color.a == 0 {
                            continue;
                        }

                        let dst_idx = (py as usize * target_w as usize + px as usize) * 4;
                        if dst_idx + 3 < data.len() {
                            let alpha = (color.a as f32 / 255.0) * opacity;
                            let inv_a = 1.0 - alpha;
                            data[dst_idx] = (color.r as f32 * alpha + data[dst_idx] as f32 * inv_a) as u8;
                            data[dst_idx + 1] = (color.g as f32 * alpha + data[dst_idx + 1] as f32 * inv_a) as u8;
                            data[dst_idx + 2] = (color.b as f32 * alpha + data[dst_idx + 2] as f32 * inv_a) as u8;
                            data[dst_idx + 3] = 255;
                        }
                    }
                }
            }
            ImageFitMode::FitLetterbox => {
                let scale_x = dst_w as f32 / self.width as f32;
                let scale_y = dst_h as f32 / self.height as f32;
                let scale = scale_x.min(scale_y);

                let fit_w = (self.width as f32 * scale).round() as u32;
                let fit_h = (self.height as f32 * scale).round() as u32;
                let offset_x = (dst_w.saturating_sub(fit_w)) / 2;
                let offset_y = (dst_h.saturating_sub(fit_h)) / 2;

                self.draw_fitted_with_opacity(
                    pixmap,
                    dst_x + offset_x as i32,
                    dst_y + offset_y as i32,
                    fit_w,
                    fit_h,
                    ImageFitMode::FillCrop,
                    opacity,
                );
            }
        }
    }

    /// Draws this image fitted into the destination box with color-key transparency and opacity.
    pub fn draw_color_keyed_with_opacity(
        &self,
        pixmap: &mut Pixmap,
        dst_x: i32,
        dst_y: i32,
        dst_w: u32,
        dst_h: u32,
        global_opacity: f32,
    ) {
        if global_opacity <= 0.0 || dst_w == 0 || dst_h == 0 || self.width == 0 || self.height == 0 {
            return;
        }

        let opacity = global_opacity.clamp(0.0, 1.0);
        let target_w = pixmap.width() as i32;
        let target_h = pixmap.height() as i32;
        let data = pixmap.data_mut();

        let scale_x = dst_w as f32 / self.width as f32;
        let scale_y = dst_h as f32 / self.height as f32;
        let scale = scale_x.max(scale_y);

        let src_view_w = dst_w as f32 / scale;
        let src_view_h = dst_h as f32 / scale;
        let src_origin_x = (self.width as f32 - src_view_w) / 2.0;
        let src_origin_y = (self.height as f32 - src_view_h) / 2.0;

        for dy in 0..dst_h as i32 {
            let py = dst_y + dy;
            if py < 0 || py >= target_h {
                continue;
            }

            let sy = (src_origin_y + (dy as f32 / dst_h as f32) * src_view_h) as i32;
            if sy < 0 || sy >= self.height as i32 {
                continue;
            }
            let row_src_idx = (sy as usize) * (self.width as usize);

            for dx in 0..dst_w as i32 {
                let px = dst_x + dx;
                if px < 0 || px >= target_w {
                    continue;
                }

                let sx = (src_origin_x + (dx as f32 / dst_w as f32) * src_view_w) as i32;
                if sx < 0 || sx >= self.width as i32 {
                    continue;
                }

                let color = self.pixels[row_src_idx + sx as usize];
                if color.a == 0 || (color.r == 0 && color.g == 0 && color.b == 0) {
                    continue;
                }

                let dst_idx = (py as usize * target_w as usize + px as usize) * 4;
                if dst_idx + 3 < data.len() {
                    let alpha = (color.a as f32 / 255.0) * opacity;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_synthetic_24bit_bmp(w: u32, h: u32, bgr_color: (u8, u8, u8)) -> Vec<u8> {
        let img = ImageBuffer::new(w, h, ColorRgba::new(bgr_color.2, bgr_color.1, bgr_color.0, 255));
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
    fn test_draw_fitted_modes() {
        let mut pixmap = Pixmap::new(100, 100).unwrap();
        let img = ImageBuffer::new(200, 100, ColorRgba::new(255, 0, 0, 255));

        // 1. FillCrop: Should completely fill target 100x100
        pixmap.fill(tiny_skia::Color::BLACK);
        img.draw_fitted(&mut pixmap, 0, 0, 100, 100, ImageFitMode::FillCrop);
        let non_black_count = pixmap.data().chunks_exact(4).filter(|p| p[0] == 255).count();
        assert_eq!(non_black_count, 10000);

        // 2. FitLetterbox: 2:1 image in 1:1 box should be 100x50 centered
        pixmap.fill(tiny_skia::Color::BLACK);
        img.draw_fitted(&mut pixmap, 0, 0, 100, 100, ImageFitMode::FitLetterbox);
        let filled_count = pixmap.data().chunks_exact(4).filter(|p| p[0] == 255).count();
        assert_eq!(filled_count, 100 * 50);
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
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a,
            0x00, 0x00, 0x00, 0x0d,
            0x49, 0x48, 0x44, 0x52,
            0x00, 0x00, 0x00, 0x01,
            0x00, 0x00, 0x00, 0x01,
            0x08, 0x00, 0x00, 0x00, 0x00,
            0x3a, 0x7e, 0x9b, 0x55,
            0x00, 0x00, 0x00, 0x0a,
            0x49, 0x44, 0x41, 0x54,
            0x78, 0x9c, 0x63, 0x60, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01,
            0x48, 0xaf, 0xa4, 0x71,
            0x00, 0x00, 0x00, 0x00,
            0x49, 0x45, 0x4e, 0x44,
            0xae, 0x42, 0x60, 0x82,
        ];
        let img = ImageBuffer::from_bytes(png_bytes).expect("Should decode PNG via from_bytes");
        assert_eq!(img.width, 1);
        assert_eq!(img.height, 1);
        assert_eq!(img.pixels.len(), 1);
        assert_eq!(img.pixels[0], ColorRgba::new(0, 0, 0, 255));
    }
}

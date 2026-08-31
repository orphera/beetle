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
            return Self::from_bmp_bytes(data);
        }

        #[cfg(feature = "bga-enhanced")]
        {
            if data.len() >= 8 && &data[0..8] == b"\x89PNG\r\n\x1a\n" {
                return Self::from_png_bytes(data);
            }
            if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
                return Self::from_jpeg_bytes(data);
            }
        }

        // Fallback try BMP
        Self::from_bmp_bytes(data)
    }

    #[cfg(feature = "bga-enhanced")]
    /// Decodes a PNG image from byte slice (requires bga-enhanced feature).
    pub fn from_png_bytes(data: &[u8]) -> Option<Self> {
        let mut decoder = png::Decoder::new(data);
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().ok()?;
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).ok()?;
        let width = info.width;
        let height = info.height;
        let (color_type, _) = reader.output_color_type();
        let mut pixels = Vec::with_capacity((width * height) as usize);

        match color_type {
            png::ColorType::Rgba => {
                for chunk in buf[..info.buffer_size()].chunks_exact(4) {
                    pixels.push(ColorRgba::new(chunk[0], chunk[1], chunk[2], chunk[3]));
                }
            }
            png::ColorType::Rgb => {
                for chunk in buf[..info.buffer_size()].chunks_exact(3) {
                    pixels.push(ColorRgba::new(chunk[0], chunk[1], chunk[2], 255));
                }
            }
            png::ColorType::Grayscale => {
                for &b in &buf[..info.buffer_size()] {
                    pixels.push(ColorRgba::new(b, b, b, 255));
                }
            }
            png::ColorType::GrayscaleAlpha => {
                for chunk in buf[..info.buffer_size()].chunks_exact(2) {
                    pixels.push(ColorRgba::new(chunk[0], chunk[0], chunk[0], chunk[1]));
                }
            }
            _ => return None,
        }

        if pixels.len() == (width * height) as usize {
            Some(Self {
                width,
                height,
                pixels,
            })
        } else {
            None
        }
    }

    #[cfg(feature = "bga-enhanced")]
    /// Decodes a JPEG image from byte slice (requires bga-enhanced feature).
    pub fn from_jpeg_bytes(data: &[u8]) -> Option<Self> {
        let mut decoder = jpeg_decoder::Decoder::new(data);
        let pixels_raw = decoder.decode().ok()?;
        let metadata = decoder.info()?;
        let width = metadata.width as u32;
        let height = metadata.height as u32;
        let mut pixels = Vec::with_capacity((width * height) as usize);

        match metadata.pixel_format {
            jpeg_decoder::PixelFormat::RGB24 => {
                for chunk in pixels_raw.chunks_exact(3) {
                    pixels.push(ColorRgba::new(chunk[0], chunk[1], chunk[2], 255));
                }
            }
            jpeg_decoder::PixelFormat::L8 => {
                for &b in &pixels_raw {
                    pixels.push(ColorRgba::new(b, b, b, 255));
                }
            }
            jpeg_decoder::PixelFormat::L16 => {
                for chunk in pixels_raw.chunks_exact(2) {
                    pixels.push(ColorRgba::new(chunk[0], chunk[0], chunk[0], 255));
                }
            }
            jpeg_decoder::PixelFormat::CMYK32 => {
                for chunk in pixels_raw.chunks_exact(4) {
                    let c = chunk[0] as f32 / 255.0;
                    let m = chunk[1] as f32 / 255.0;
                    let y = chunk[2] as f32 / 255.0;
                    let k = chunk[3] as f32 / 255.0;
                    let r = ((1.0 - c) * (1.0 - k) * 255.0).round() as u8;
                    let g = ((1.0 - m) * (1.0 - k) * 255.0).round() as u8;
                    let b = ((1.0 - y) * (1.0 - k) * 255.0).round() as u8;
                    pixels.push(ColorRgba::new(r, g, b, 255));
                }
            }
        }

        if pixels.len() == (width * height) as usize {
            Some(Self {
                width,
                height,
                pixels,
            })
        } else {
            None
        }
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

    /// Encodes this image buffer as a 24-bit uncompressed Windows BMP byte vector without external dependencies.
    pub fn encode_bmp_bytes(&self) -> Vec<u8> {
        let w = self.width;
        let h = self.height;
        let row_stride_unpadded = (w * 3) as usize;
        let row_padding = (4 - (row_stride_unpadded % 4)) % 4;
        let row_stride = row_stride_unpadded + row_padding;
        let pixel_bytes_len = row_stride * h as usize;
        let total_size = 54 + pixel_bytes_len;

        let mut data = Vec::with_capacity(total_size);
        // Header
        data.extend_from_slice(b"BM");
        data.extend_from_slice(&(total_size as u32).to_le_bytes());
        data.extend_from_slice(&[0, 0, 0, 0]);
        data.extend_from_slice(&54u32.to_le_bytes());

        // DIB Header
        data.extend_from_slice(&40u32.to_le_bytes());
        data.extend_from_slice(&(w as i32).to_le_bytes());
        data.extend_from_slice(&(h as i32).to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&24u16.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(pixel_bytes_len as u32).to_le_bytes());
        data.extend_from_slice(&2835u32.to_le_bytes());
        data.extend_from_slice(&2835u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());

        // Pixels: bottom-up row order
        for y in (0..h as usize).rev() {
            for x in 0..w as usize {
                let px = self.pixels[y * w as usize + x];
                data.push(px.b);
                data.push(px.g);
                data.push(px.r);
            }
            for _ in 0..row_padding {
                data.push(0);
            }
        }

        data
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
        let mut png_bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_bytes, 2, 2);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("write header");
            let data = [
                255, 0, 0, 255,  0, 255, 0, 255,
                0, 0, 255, 255,  255, 255, 255, 255,
            ];
            writer.write_image_data(&data).expect("write data");
        }
        let img = ImageBuffer::from_bytes(&png_bytes).expect("Should decode PNG via from_bytes");
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 2);
        assert_eq!(img.pixels.len(), 4);
        assert_eq!(img.pixels[0], ColorRgba::new(255, 0, 0, 255));
        assert_eq!(img.pixels[1], ColorRgba::new(0, 255, 0, 255));
    }
}

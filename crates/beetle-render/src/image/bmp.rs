use crate::skin::ColorRgba;
use super::ImageBuffer;

/// Decodes a 24-bit or 32-bit uncompressed Windows BMP image without external crates.
pub fn decode_bmp(data: &[u8]) -> Option<ImageBuffer> {
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

    Some(ImageBuffer {
        width,
        height,
        pixels,
    })
}

/// Encodes an ImageBuffer as a 24-bit uncompressed Windows BMP byte vector without external dependencies.
pub fn encode_bmp(image: &ImageBuffer) -> Vec<u8> {
    let w = image.width;
    let h = image.height;
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
            let px = image.pixels[y * w as usize + x];
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

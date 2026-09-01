use crate::skin::ColorRgba;
use super::ImageBuffer;

/// Decodes a 1-bit, 4-bit, 8-bit paletted, 24-bit, or 32-bit uncompressed Windows BMP image without external crates.
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
    let dib_header_size = u32::from_le_bytes(data[14..18].try_into().ok()?) as usize;
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
    if bpp != 1 && bpp != 4 && bpp != 8 && bpp != 24 && bpp != 32 {
        return None;
    }

    // Compression: 0 = BI_RGB (uncompressed)
    let compression = if data.len() >= 34 {
        u32::from_le_bytes(data[30..34].try_into().unwrap_or([0; 4]))
    } else {
        0
    };
    if compression != 0 {
        return None;
    }

    if data.len() < data_offset {
        return None;
    }

    // 4. Color Palette for indexed color modes (1, 4, 8 bpp)
    let mut palette: Vec<ColorRgba> = Vec::new();
    if bpp <= 8 {
        let palette_offset = 14 + dib_header_size;
        let num_colors = if data.len() >= 50 {
            let colors_used = u32::from_le_bytes(data[46..50].try_into().unwrap_or([0; 4])) as usize;
            if colors_used > 0 && colors_used <= (1 << bpp) {
                colors_used
            } else {
                1 << bpp
            }
        } else {
            1 << bpp
        };

        if palette_offset + num_colors * 4 <= data.len() {
            for i in 0..num_colors {
                let entry_idx = palette_offset + i * 4;
                let b = data[entry_idx];
                let g = data[entry_idx + 1];
                let r = data[entry_idx + 2];
                palette.push(ColorRgba::new(r, g, b, 255));
            }
        } else {
            // Fallback grayscale ramp if palette block is truncated
            for i in 0..num_colors {
                let v = (i * 255 / (num_colors - 1).max(1)) as u8;
                palette.push(ColorRgba::new(v, v, v, 255));
            }
        }
    }

    let mut pixels = vec![ColorRgba::transparent(); (width * height) as usize];
    let raw_pixels = &data[data_offset..];

    // Calculate row stride padded to 4-byte boundary
    let row_stride = match bpp {
        1 => ((width as usize + 31) / 32) * 4,
        4 => ((width as usize * 4 + 31) / 32) * 4,
        8 => ((width as usize + 3) / 4) * 4,
        24 => ((width as usize * 3 + 3) / 4) * 4,
        32 => width as usize * 4,
        _ => return None,
    };

    for y in 0..height as usize {
        let src_y = if is_top_down {
            y
        } else {
            (height as usize - 1) - y
        };

        let row_start = src_y * row_stride;
        if row_start >= raw_pixels.len() {
            break;
        }

        let row = &raw_pixels[row_start..];
        let dst_row_start = y * width as usize;

        match bpp {
            1 => {
                for x in 0..width as usize {
                    let byte_idx = x / 8;
                    if byte_idx >= row.len() {
                        break;
                    }
                    let bit = (row[byte_idx] >> (7 - (x % 8))) & 1;
                    if (bit as usize) < palette.len() {
                        pixels[dst_row_start + x] = palette[bit as usize];
                    }
                }
            }
            4 => {
                for x in 0..width as usize {
                    let byte_idx = x / 2;
                    if byte_idx >= row.len() {
                        break;
                    }
                    let nibble = if x % 2 == 0 {
                        (row[byte_idx] >> 4) & 0x0F
                    } else {
                        row[byte_idx] & 0x0F
                    };
                    if (nibble as usize) < palette.len() {
                        pixels[dst_row_start + x] = palette[nibble as usize];
                    }
                }
            }
            8 => {
                for x in 0..width as usize {
                    if x >= row.len() {
                        break;
                    }
                    let idx = row[x] as usize;
                    if idx < palette.len() {
                        pixels[dst_row_start + x] = palette[idx];
                    }
                }
            }
            24 => {
                for x in 0..width as usize {
                    let px_idx = x * 3;
                    if px_idx + 2 >= row.len() {
                        break;
                    }
                    let b = row[px_idx];
                    let g = row[px_idx + 1];
                    let r = row[px_idx + 2];
                    pixels[dst_row_start + x] = ColorRgba::new(r, g, b, 255);
                }
            }
            32 => {
                for x in 0..width as usize {
                    let px_idx = x * 4;
                    if px_idx + 3 >= row.len() {
                        break;
                    }
                    let b = row[px_idx];
                    let g = row[px_idx + 1];
                    let r = row[px_idx + 2];
                    let a = row[px_idx + 3];
                    pixels[dst_row_start + x] = ColorRgba::new(r, g, b, a);
                }
            }
            _ => (),
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

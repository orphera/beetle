//! CPU image drawing onto the GUI's tiny-skia pixmap (moved here from
//! beetle-render when the player went GPU-only, ADR-026).

use beetle_render::ImageBuffer;
use tiny_skia::Pixmap;

/// Blits and scales the image into a target area on the tiny-skia Pixmap.
pub fn draw_scaled(
    img: &ImageBuffer,
    pixmap: &mut Pixmap,
    dst_x: i32,
    dst_y: i32,
    dst_w: u32,
    dst_h: u32,
) {
    if dst_w == 0 || dst_h == 0 || img.width == 0 || img.height == 0 {
        return;
    }

    let target_w = pixmap.width() as i32;
    let target_h = pixmap.height() as i32;
    let data = pixmap.data_mut();

    // Precompute horizontal sample mapping to eliminate per-pixel floating point division
    let mut src_x_table = Vec::with_capacity(dst_w as usize);
    for dx in 0..dst_w {
        let sx = (dx as f32 / dst_w as f32 * img.width as f32) as usize;
        src_x_table.push(sx.min(img.width as usize - 1));
    }

    for dy in 0..dst_h as i32 {
        let py = dst_y + dy;
        if py < 0 || py >= target_h {
            continue;
        }

        let src_y = (dy as f32 / dst_h as f32 * img.height as f32) as usize;
        let src_y = src_y.min(img.height as usize - 1);
        let row_offset = src_y * img.width as usize;

        for dx in 0..dst_w as i32 {
            let px = dst_x + dx;
            if px < 0 || px >= target_w {
                continue;
            }

            let src_x = src_x_table[dx as usize];
            let color = img.pixels[row_offset + src_x];
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
                    data[dst_idx] =
                        ((color.r as u32 * a + data[dst_idx] as u32 * inv_a) / 255) as u8;
                    data[dst_idx + 1] =
                        ((color.g as u32 * a + data[dst_idx + 1] as u32 * inv_a) / 255) as u8;
                    data[dst_idx + 2] =
                        ((color.b as u32 * a + data[dst_idx + 2] as u32 * inv_a) / 255) as u8;
                    data[dst_idx + 3] = 255;
                }
            }
        }
    }
}

use super::{BlendMode, GpuBackend, SpriteBatcher, TextureId};
use crate::bitmap_font::{ascii::ASCII_5X7, bold_digits::BOLD_DIGITS_8X12};
use crate::skin::ColorRgba;

pub const ATLAS_WIDTH: u32 = 128;
pub const ATLAS_HEIGHT: u32 = 128;

/// GPU Texture Atlas containing baked bitmap font glyphs (Bold Digits 8x12 and ASCII 5x7).
///
/// Enables zero-CPU-allocation hardware-accelerated text rendering with a single draw batch.
#[derive(Debug, Clone, Copy)]
pub struct FontAtlas {
    pub texture_id: TextureId,
}

impl FontAtlas {
    /// Bakes bitmap glyphs into a 128x128 RGBA8 texture and uploads it to the given `GpuBackend`.
    pub fn new(backend: &mut dyn GpuBackend) -> Option<Self> {
        let mut pixels = vec![0u8; (ATLAS_WIDTH * ATLAS_HEIGHT * 4) as usize];

        // 1. Bake 8x12 Bold Digits across row 0..12 (16 characters: '0'..='9', '+', '-', '.', ':', '%', '/')
        for (i, glyph) in BOLD_DIGITS_8X12.iter().enumerate() {
            let base_x = (i as u32) * 8;
            let base_y = 0;
            for row in 0..12 {
                let row_bits = glyph[row];
                for col in 0..8 {
                    if (row_bits & (0x80 >> col)) != 0 {
                        let px = base_x + col as u32;
                        let py = base_y + row as u32;
                        let idx = ((py * ATLAS_WIDTH + px) * 4) as usize;
                        pixels[idx] = 255;
                        pixels[idx + 1] = 255;
                        pixels[idx + 2] = 255;
                        pixels[idx + 3] = 255;
                    }
                }
            }
        }

        // 2. Bake 5x7 ASCII glyphs across grid starting at Y=16 (16 cols x 6 rows of 8x8 cells)
        for (i, glyph) in ASCII_5X7.iter().enumerate() {
            let col_cell = (i % 16) as u32;
            let row_cell = (i / 16) as u32;
            let base_x = col_cell * 8;
            let base_y = 16 + (row_cell * 8);

            for col in 0..5 {
                let col_bits = glyph[col];
                for row in 0..7 {
                    if (col_bits & (1 << row)) != 0 {
                        let px = base_x + col as u32;
                        let py = base_y + row as u32;
                        let idx = ((py * ATLAS_WIDTH + px) * 4) as usize;
                        pixels[idx] = 255;
                        pixels[idx + 1] = 255;
                        pixels[idx + 2] = 255;
                        pixels[idx + 3] = 255;
                    }
                }
            }
        }

        // 3. Bake a pure white 4x4 pixel block at bottom right (x: 124..128, y: 124..128)
        // for optional untextured quad drawing without switching textures.
        for py in 124..128 {
            for px in 124..128 {
                let idx = ((py * ATLAS_WIDTH + px) * 4) as usize;
                pixels[idx] = 255;
                pixels[idx + 1] = 255;
                pixels[idx + 2] = 255;
                pixels[idx + 3] = 255;
            }
        }

        let texture_id = backend.create_texture(ATLAS_WIDTH, ATLAS_HEIGHT, &pixels)?;
        Some(Self { texture_id })
    }

    /// Returns normalized UV coordinates `[u0, v0, u1, v1]` for 8x12 bold digits and symbols.
    #[inline(always)]
    pub fn get_bold_digit_uv(&self, c: char) -> Option<[f32; 4]> {
        let idx = match c {
            '0'..='9' => c as usize - '0' as usize,
            '+' => 10,
            '-' => 11,
            '.' => 12,
            ':' => 13,
            '%' => 14,
            '/' => 15,
            _ => return None,
        };

        let x0 = (idx as f32) * 8.0;
        let y0 = 0.0;
        let x1 = x0 + 8.0;
        let y1 = 12.0;

        Some([
            x0 / (ATLAS_WIDTH as f32),
            y0 / (ATLAS_HEIGHT as f32),
            x1 / (ATLAS_WIDTH as f32),
            y1 / (ATLAS_HEIGHT as f32),
        ])
    }

    /// Returns normalized UV coordinates `[u0, v0, u1, v1]` for 5x7 ASCII characters.
    #[inline(always)]
    pub fn get_ascii_uv(&self, c: char) -> Option<[f32; 4]> {
        let code = c as u32;
        if !(32..=126).contains(&code) {
            return None;
        }
        let idx = (code - 32) as usize;
        let col = (idx % 16) as f32;
        let row = (idx / 16) as f32;

        let x0 = col * 8.0;
        let y0 = 16.0 + (row * 8.0);
        let x1 = x0 + 5.0;
        let y1 = y0 + 7.0;

        Some([
            x0 / (ATLAS_WIDTH as f32),
            y0 / (ATLAS_HEIGHT as f32),
            x1 / (ATLAS_WIDTH as f32),
            y1 / (ATLAS_HEIGHT as f32),
        ])
    }

    /// Draws a string of bold digits using batched textured sub-sprites.
    pub fn draw_bold_text(
        &self,
        batcher: &mut SpriteBatcher,
        backend: &mut dyn GpuBackend,
        text: &str,
        mut x: f32,
        y: f32,
        scale: f32,
        color: ColorRgba,
    ) {
        let s = scale.max(1.0);
        let char_w = 8.0 * s;
        let char_h = 12.0 * s;
        let spacing = 2.0 * s;
        let color_f32 = color.to_f32_array();

        for c in text.chars() {
            if c == ' ' {
                x += char_w + spacing;
                continue;
            }
            if let Some([u0, v0, u1, v1]) = self.get_bold_digit_uv(c) {
                batcher.draw_sub_sprite(
                    backend,
                    self.texture_id,
                    x,
                    y,
                    char_w,
                    char_h,
                    u0,
                    v0,
                    u1,
                    v1,
                    color_f32,
                    BlendMode::Alpha,
                );
            }
            x += char_w + spacing;
        }
    }

    /// Draws horizontally centered bold digits.
    pub fn draw_bold_text_centered(
        &self,
        batcher: &mut SpriteBatcher,
        backend: &mut dyn GpuBackend,
        text: &str,
        center_x: f32,
        y: f32,
        scale: f32,
        color: ColorRgba,
    ) {
        let count = text.chars().count() as f32;
        if count == 0.0 {
            return;
        }
        let s = scale.max(1.0);
        let total_w = count * 8.0 * s + (count - 1.0) * 2.0 * s;
        let x = center_x - (total_w / 2.0);
        self.draw_bold_text(batcher, backend, text, x, y, scale, color);
    }

    /// Draws a string of ASCII characters using batched textured sub-sprites.
    pub fn draw_ascii_text(
        &self,
        batcher: &mut SpriteBatcher,
        backend: &mut dyn GpuBackend,
        text: &str,
        mut x: f32,
        y: f32,
        scale: f32,
        color: ColorRgba,
    ) {
        let s = scale.max(1.0);
        let char_w = 5.0 * s;
        let char_h = 7.0 * s;
        let spacing = 1.0 * s;
        let color_f32 = color.to_f32_array();

        for c in text.chars() {
            if c == ' ' {
                x += char_w + spacing;
                continue;
            }
            if let Some([u0, v0, u1, v1]) = self.get_ascii_uv(c) {
                batcher.draw_sub_sprite(
                    backend,
                    self.texture_id,
                    x,
                    y,
                    char_w,
                    char_h,
                    u0,
                    v0,
                    u1,
                    v1,
                    color_f32,
                    BlendMode::Alpha,
                );
            }
            x += char_w + spacing;
        }
    }

    /// Draws horizontally centered ASCII text.
    pub fn draw_ascii_text_centered(
        &self,
        batcher: &mut SpriteBatcher,
        backend: &mut dyn GpuBackend,
        text: &str,
        center_x: f32,
        y: f32,
        scale: f32,
        color: ColorRgba,
    ) {
        let count = text.chars().count() as f32;
        if count == 0.0 {
            return;
        }
        let s = scale.max(1.0);
        let total_w = count * 5.0 * s + (count - 1.0) * 1.0 * s;
        let x = center_x - (total_w / 2.0);
        self.draw_ascii_text(batcher, backend, text, x, y, scale, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::SoftBackend;

    #[test]
    fn test_font_atlas_creation_and_uv_mapping() {
        let mut backend = SoftBackend::new(256, 256);
        let atlas = FontAtlas::new(&mut backend).expect("Failed to create font atlas");

        // Test bold digit UVs
        let uv_0 = atlas.get_bold_digit_uv('0').expect("Valid digit 0");
        assert_eq!(uv_0[0], 0.0);
        assert_eq!(uv_0[1], 0.0);
        assert_eq!(uv_0[2], 8.0 / 128.0);
        assert_eq!(uv_0[3], 12.0 / 128.0);

        let uv_plus = atlas.get_bold_digit_uv('+').expect("Valid symbol +");
        assert_eq!(uv_plus[0], 80.0 / 128.0);

        assert!(atlas.get_bold_digit_uv('Z').is_none());

        // Test ASCII UVs
        let uv_space = atlas.get_ascii_uv(' ').expect("Valid space");
        assert_eq!(uv_space[0], 0.0);
        assert_eq!(uv_space[1], 16.0 / 128.0);

        let uv_a = atlas.get_ascii_uv('A').expect("Valid char A");
        assert!(uv_a[0] >= 0.0 && uv_a[2] <= 1.0);

        // Test batched rendering with SoftBackend
        let mut batcher = SpriteBatcher::new();
        batcher.begin();
        atlas.draw_bold_text(
            &mut batcher,
            &mut backend,
            "1234",
            10.0,
            10.0,
            2.0,
            ColorRgba::new(255, 255, 255, 255),
        );
        atlas.draw_ascii_text(
            &mut batcher,
            &mut backend,
            "PERFECT",
            10.0,
            50.0,
            1.5,
            ColorRgba::new(255, 220, 50, 255),
        );
        batcher.flush(&mut backend);
        assert_eq!(batcher.draw_call_count(), 1);
    }
}

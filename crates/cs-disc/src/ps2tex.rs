//! PS2 textures as the IIDX 16 discs store their pictures: 8-bit indexed
//! (PSMT8) with a 256-colour CLUT, LZSS-compressed.
//!
//! Ported from iidx-ps2tools (`animtool/sprites.decode_lzss`,
//! `animtool/ps2textures.py`). A decompressed texture is the CLUT (256
//! little-endian `u32` RGBA, 0x400 bytes) then `width * height` indices, laid
//! out as the GS swizzles PSMT8 when it is uploaded as PSMCT32.

/// An RGBA8 picture, row-major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}

const CLUT: usize = 0x400;

/// The LZSS of the texture containers (a 4 KiB ring buffer starting at
/// 0xfee). Decodes until `size` bytes are out; `None` when the input runs out first.
pub fn decode_lzss(input: &[u8], size: usize) -> Option<Vec<u8>> {
    const RING: usize = 0x1000;
    let mut ring = [0u8; RING];
    let mut at = 0xfee;
    let mut out = Vec::with_capacity(size);
    let mut i = 0;
    let mut control: u32 = 0;
    while out.len() < size {
        control >>= 1;
        if control & 0x100 == 0 {
            control = *input.get(i)? as u32 | 0xff00;
            i += 1;
        }
        if control & 1 != 0 {
            let b = *input.get(i)?;
            i += 1;
            out.push(b);
            ring[at] = b;
            at = (at + 1) % RING;
        } else {
            let (lo, hi) = (*input.get(i)? as usize, *input.get(i + 1)? as usize);
            i += 2;
            let mut from = ((hi & 0xf0) << 4) | lo;
            for _ in 0..(hi & 0x0f) + 3 {
                let b = ring[from];
                out.push(b);
                ring[at] = b;
                at = (at + 1) % RING;
                from = (from + 1) % RING;
            }
        }
    }
    out.truncate(size);
    Some(out)
}

/// The size of an 8-bit texture from its decompressed length (CLUT plus one
/// byte a pixel); the wider one when two sizes fit.
pub fn texture_size(decompressed: usize) -> Option<(u32, u32)> {
    const WIDTHS: [u32; 4] = [640, 512, 384, 256];
    const HEIGHTS: [u32; 6] = [640, 512, 448, 384, 256, 128];
    let pixels = decompressed.checked_sub(CLUT).filter(|&p| p > 0)?;
    WIDTHS.iter().find_map(|&w| {
        HEIGHTS
            .iter()
            .find(|&&h| (w * h) as usize == pixels)
            .map(|&h| (w, h))
    })
}

/// The CLUT position of colour index `i`: the GS stores the 256 colours in
/// blocks of 8 with blocks 1 and 2 of every 32 swapped.
fn clut_index(i: u8) -> usize {
    ((i & 0xe7) | ((i & 0x08) << 1) | ((i & 0x10) >> 1)) as usize
}

/// Where pixel (x, y) of a `width`-wide PSMT8 texture is in the swizzled data.
fn swizzled(x: usize, y: usize, width: usize) -> usize {
    let block = (y & !0xf) * width + (x & !0xf) * 2;
    let swap = (((y + 2) >> 2) & 1) * 4;
    let row = (((y & !3) >> 1) + (y & 1)) & 7;
    let column = row * width * 2 + ((x + swap) & 7) * 4;
    let byte = ((y >> 1) & 1) + ((x >> 2) & 2);
    block + column + byte
}

/// A decompressed PSMT8 texture as RGBA. Alpha is stored halved (0x80 is
/// opaque), as the GS uses it.
pub fn decode_psmt8(data: &[u8], width: u32, height: u32) -> Option<Rgba> {
    let (w, h) = (width as usize, height as usize);
    let indices = data.get(CLUT..CLUT + w * h)?;
    let clut: Vec<[u8; 4]> = data[..CLUT]
        .chunks_exact(4)
        .map(|c| [c[0], c[1], c[2], c[3].saturating_mul(2)])
        .collect();
    let mut pixels = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let index = *indices.get(swizzled(x, y, w))?;
            pixels.push(clut[clut_index(index)]);
        }
    }
    Some(Rgba {
        width,
        height,
        pixels,
    })
}

impl Rgba {
    /// A picture wider than 4:3 in the middle of a black 4:3 canvas of its
    /// width (a pop'n banner, 244x58, on 244x183), so a 4:3 frame shows all
    /// of it; a narrower one as it is.
    pub fn letterboxed_4_3(&self) -> Rgba {
        let height = (self.width * 3).div_ceil(4);
        if height <= self.height {
            return self.clone();
        }
        let top = (height - self.height) / 2;
        let mut pixels = vec![[0, 0, 0, 255]; (self.width * height) as usize];
        for y in 0..self.height {
            for x in 0..self.width {
                let [r, g, b, a] = self.pixels[(y * self.width + x) as usize];
                // Over black, by its alpha (0x80 is opaque once doubled).
                let over = |c: u8| (c as u32 * a as u32 / 255) as u8;
                pixels[((y + top) * self.width + x) as usize] = [over(r), over(g), over(b), 255];
            }
        }
        Rgba {
            width: self.width,
            height,
            pixels,
        }
    }

    /// The picture on a black 4:3 canvas of its height (or wider, for a
    /// wide picture), moved so the box around its visible content (any
    /// channel above 40) sits in the middle without cutting anything off: the
    /// stage card layout of the iidx16-bms extraction, without its rescale.
    pub fn centered_4_3(&self) -> Rgba {
        let (w, h) = (self.width as i64, self.height as i64);
        let cw = (h * 4 / 3).max(w);
        let lit = |p: &[u8; 4]| p[0].max(p[1]).max(p[2]) > 40;
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0i64, 0i64);
        for y in 0..h {
            for x in 0..w {
                if lit(&self.pixels[(y * w + x) as usize]) {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
        }
        if x1 <= x0 {
            (x0, y0, x1, y1) = (0, 0, w, h);
        }
        let ox = (cw / 2 - (x0 + x1) / 2).clamp(-x0, cw - x1);
        let oy = (h / 2 - (y0 + y1) / 2).clamp(-y0, h - y1);
        let mut pixels = vec![[0, 0, 0, 255]; (cw * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let (tx, ty) = (x + ox, y + oy);
                if (0..cw).contains(&tx) && (0..h).contains(&ty) {
                    let [r, g, b, _] = self.pixels[(y * w + x) as usize];
                    pixels[(ty * cw + tx) as usize] = [r, g, b, 255];
                }
            }
        }
        Rgba {
            width: cw as u32,
            height: h as u32,
            pixels,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lzss_literals_and_a_ring_copy() {
        // control 0b0000_0111: three literals, then a copy of 3 from ring 0xfee.
        let input = [0b0000_0111, b'a', b'b', b'c', 0xee, 0xf0];
        assert_eq!(decode_lzss(&input, 6).unwrap(), b"abcabc");
        assert_eq!(decode_lzss(&input[..3], 6), None);
    }

    #[test]
    fn sizes_come_from_the_length() {
        assert_eq!(texture_size(0x400 + 384 * 384), Some((384, 384)));
        assert_eq!(texture_size(0x50400), Some((640, 512)));
        assert_eq!(texture_size(0x400), None);
    }

    #[test]
    fn the_clut_swaps_blocks_one_and_two() {
        assert_eq!(clut_index(0x08), 0x10);
        assert_eq!(clut_index(0x10), 0x08);
        assert_eq!(clut_index(0x07), 0x07);
        assert_eq!(clut_index(0x28), 0x30);
    }

    #[test]
    fn the_swizzle_visits_every_byte_once() {
        let (w, h) = (128, 64);
        let mut seen = vec![false; w * h];
        for y in 0..h {
            for x in 0..w {
                let i = swizzled(x, y, w);
                assert!(!seen[i], "({x}, {y}) repeats {i}");
                seen[i] = true;
            }
        }
    }

    #[test]
    fn a_card_is_centered_on_a_black_4_3_canvas() {
        // A 4x3 picture lit only in its left column.
        let mut pixels = vec![[0, 0, 0, 0x80]; 12];
        for y in 0..3 {
            pixels[y * 4] = [255, 255, 255, 0x80];
        }
        let card = Rgba {
            width: 4,
            height: 3,
            pixels,
        }
        .centered_4_3();
        assert_eq!((card.width, card.height), (4, 3));
        assert_eq!(card.pixels[2], [255, 255, 255, 255]);
        assert_eq!(card.pixels[0], [0, 0, 0, 255]);
    }
}

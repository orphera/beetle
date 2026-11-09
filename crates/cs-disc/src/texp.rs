//! Konami `TEXP` texture files (little-endian, so `PXET` on disc) as the
//! GuitarFreaks & DrumMania PS2 disc keeps its pictures, song banners among
//! them (`IMG_CO{id:04}`, 200x80).
//!
//! Layout (`docs/specs/gfdm_ps2_disc.md`): a 0x40 head (`u32` texture count
//! at 0x14, data at 0x1c, the `TCER` rect section at 0x20), a `PMAN` name
//! table of the textures at 0x40, the textures, then `TCER` with its own
//! `PMAN` and one rect per name. A texture is a big-endian decompressed and
//! compressed size, then LZSS (`ps2tex::decode_lzss`) of a `TDXT`, or the
//! `TDXT` as it is when the compressed size is 0. `TDXT`: 0x40 head (`u16`
//! width and height at 0x10, pixel format at 0x14: 0x12 eight bits, 0x11
//! four bits a pixel, low nibble first), the pixels in rows, then a `PALD`
//! (`DLAP` on disc; 0x14 head with the colour count at 0x10, then the RGBA
//! colours in plain order, not the GS's CSM1 order).

use crate::ps2tex::{decode_lzss, Rgba};

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

/// The names of a `PMAN` table at `at`, by their index.
fn names(b: &[u8], at: usize) -> Option<Vec<String>> {
    if b.get(at..at + 4)? != b"PMAN" {
        return None;
    }
    let count = u32_at(b, at + 0x10)? as usize;
    let mut out = vec![String::new(); count];
    for k in 0..count {
        let entry = at + 0x1c + k * 12;
        let index = u32_at(b, entry + 4)? as usize;
        let start = at + u32_at(b, entry + 8)? as usize;
        let len = b.get(start..)?.iter().position(|&c| c == 0)?;
        *out.get_mut(index)? = b[start..start + len].iter().map(|&c| c as char).collect();
    }
    Some(out)
}

/// A part of a texture with a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rect {
    pub name: String,
    pub texture: usize,
    pub x1: u16,
    pub x2: u16,
    pub y1: u16,
    pub y2: u16,
}

/// A parsed texture file; textures are decompressed on demand.
#[derive(Debug, Clone)]
pub struct Texp<'a> {
    bytes: &'a [u8],
    /// Each texture's name and where its compressed data starts.
    pub textures: Vec<(String, usize)>,
    pub rects: Vec<Rect>,
}

impl<'a> Texp<'a> {
    pub fn parse(bytes: &'a [u8]) -> Option<Self> {
        if !bytes.starts_with(b"PXET") {
            return None;
        }
        let count = u32_at(bytes, 0x14)? as usize;
        let tex_names = names(bytes, 0x40)?;
        let mut at = u32_at(bytes, 0x1c)? as usize;
        let mut textures = Vec::with_capacity(count);
        for k in 0..count {
            let size = u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?);
            let packed = u32::from_be_bytes(bytes.get(at + 4..at + 8)?.try_into().ok()?);
            textures.push((tex_names.get(k).cloned().unwrap_or_default(), at));
            let stored = if packed == 0 { size } else { packed };
            at = (at + 8 + stored as usize + 3) & !3;
        }
        let mut rects = Vec::new();
        let tcer = u32_at(bytes, 0x20)? as usize;
        if tcer != 0 && bytes.get(tcer..tcer + 4)? == b"TCER" {
            let rect_names = names(bytes, tcer + 0x1c)?;
            let data = tcer + u32_at(bytes, tcer + 0x18)? as usize;
            for (k, name) in rect_names.into_iter().enumerate() {
                let r = data + k * 12;
                rects.push(Rect {
                    name,
                    texture: u32_at(bytes, r)? as usize,
                    x1: u16_at(bytes, r + 4)?,
                    x2: u16_at(bytes, r + 6)?,
                    y1: u16_at(bytes, r + 8)?,
                    y2: u16_at(bytes, r + 10)?,
                });
            }
        }
        Some(Self {
            bytes,
            textures,
            rects,
        })
    }

    /// Texture `n` as RGBA.
    pub fn texture(&self, n: usize) -> Option<Rgba> {
        let at = self.textures.get(n)?.1;
        let b = self.bytes;
        let size = u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?) as usize;
        let packed = u32::from_be_bytes(b.get(at + 4..at + 8)?.try_into().ok()?);
        let t = if packed == 0 {
            b.get(at + 8..at + 8 + size)?.to_vec()
        } else {
            decode_lzss(b.get(at + 8..)?, size)?
        };
        if t.get(..4)? != b"TDXT" {
            return None;
        }
        let (w, h) = (u16_at(&t, 0x10)? as usize, u16_at(&t, 0x12)? as usize);
        let bits = match t.get(0x14)? {
            0x12 => 8,
            0x11 => 4,
            _ => return None,
        };
        let px = t.get(0x40..0x40 + w * h * bits / 8)?;
        let pald = 0x40 + px.len();
        if t.get(pald..pald + 4)? != b"DLAP" {
            return None;
        }
        let colours = (u16_at(&t, pald + 0x10)? as usize).min(256);
        let mut clut = vec![[0u8; 4]; 256];
        for (k, c) in t
            .get(pald + 0x14..pald + 0x14 + colours * 4)?
            .chunks_exact(4)
            .enumerate()
        {
            clut[k] = [c[0], c[1], c[2], c[3]];
        }
        let pixels = if bits == 8 {
            px.iter().map(|&i| clut[i as usize]).collect()
        } else {
            px.iter()
                .flat_map(|&b| [clut[(b & 0xf) as usize], clut[(b >> 4) as usize]])
                .collect()
        };
        Some(Rgba {
            width: w as u32,
            height: h as u32,
            pixels,
        })
    }

    /// The rect called `name` cut out of its texture.
    pub fn image(&self, name: &str) -> Option<Rgba> {
        let r = self.rects.iter().find(|r| r.name == name)?;
        let t = self.texture(r.texture)?;
        let (x1, x2, y1, y2) = (r.x1 as u32, r.x2 as u32, r.y1 as u32, r.y2 as u32);
        if x2 > t.width || y2 > t.height || x1 >= x2 || y1 >= y2 {
            return None;
        }
        let mut pixels = Vec::with_capacity(((x2 - x1) * (y2 - y1)) as usize);
        for y in y1..y2 {
            let row = (y * t.width) as usize;
            pixels.extend_from_slice(&t.pixels[row + x1 as usize..row + x2 as usize]);
        }
        Some(Rgba {
            width: x2 - x1,
            height: y2 - y1,
            pixels,
        })
    }
}

/// The banner rect name of song `id`.
pub fn banner_name(id: u32) -> String {
    format!("IMG_CO{id:04}")
}

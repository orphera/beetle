//! Last-resort glyph rasterizer for characters outside the embedded font
//! subsets (rare kanji, hanzi, symbols in song titles), via Windows GDI.
//!
//! Loading a full system CJK font into fontdue costs ~300–400 ms and tens of
//! MB; GDI rasterizes single glyphs on demand with no load time, and the
//! result goes into the shared atlas like any other glyph.

#![allow(non_snake_case, clippy::upper_case_acronyms)]

use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr;

use super::RasterGlyph;

type HDC = *mut c_void;
type HGDIOBJ = *mut c_void;

const GDI_ERROR: u32 = 0xFFFF_FFFF;
const GGO_GRAY8_BITMAP: u32 = 6;
const GGI_MARK_NONEXISTING_GLYPHS: u32 = 1;
const FW_NORMAL: i32 = 400;
const FW_BOLD: i32 = 700;
const DEFAULT_CHARSET: u32 = 1;
const ANTIALIASED_QUALITY: u32 = 4;

/// Tried in order; the first face that actually contains the glyph wins.
/// Japanese faces first (BMS titles are mostly Japanese), then Korean and
/// Chinese for anything they lack. All ship with Windows 10+.
const FACES: &[&str] = &[
    "Yu Gothic",
    "Meiryo",
    "MS Gothic",
    "Malgun Gothic",
    "Microsoft YaHei",
    "SimSun",
];

#[repr(C)]
#[derive(Default)]
struct GLYPHMETRICS {
    gmBlackBoxX: u32,
    gmBlackBoxY: u32,
    gmptGlyphOrigin: [i32; 2],
    gmCellIncX: i16,
    gmCellIncY: i16,
}

/// 16.16 fixed point (`fract`, `value`).
#[repr(C)]
struct FIXED {
    fract: u16,
    value: i16,
}

#[repr(C)]
struct MAT2 {
    m: [FIXED; 4],
}

const IDENTITY: MAT2 = MAT2 {
    m: [
        FIXED { fract: 0, value: 1 },
        FIXED { fract: 0, value: 0 },
        FIXED { fract: 0, value: 0 },
        FIXED { fract: 0, value: 1 },
    ],
};

// The legacy bitmap_font::gdi_fallback declares GetGlyphOutlineW with its own
// struct types; it is deleted with the software renderer (P4).
#[allow(clashing_extern_declarations)]
#[link(name = "gdi32")]
extern "system" {
    fn CreateCompatibleDC(hdc: HDC) -> HDC;
    fn DeleteDC(hdc: HDC) -> i32;
    fn CreateFontW(
        height: i32,
        width: i32,
        escapement: i32,
        orientation: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strike: u32,
        charset: u32,
        out_precision: u32,
        clip_precision: u32,
        quality: u32,
        pitch_family: u32,
        face: *const u16,
    ) -> HGDIOBJ;
    fn SelectObject(hdc: HDC, obj: HGDIOBJ) -> HGDIOBJ;
    fn DeleteObject(obj: HGDIOBJ) -> i32;
    fn GetGlyphIndicesW(hdc: HDC, s: *const u16, n: i32, out: *mut u16, flags: u32) -> u32;
    fn GetGlyphOutlineW(
        hdc: HDC,
        ch: u32,
        format: u32,
        gm: *mut GLYPHMETRICS,
        buf_size: u32,
        buf: *mut c_void,
        mat: *const MAT2,
    ) -> u32;
}

pub(super) struct GdiRasterizer {
    hdc: HDC,
    original_font: HGDIOBJ,
    /// (face index, px, bold) → HFONT
    fonts: HashMap<(usize, u16, bool), HGDIOBJ>,
}

// The DC and fonts are only ever touched by the thread that owns the
// TextEngine (the render thread); GDI objects are not thread-affine.
unsafe impl Send for GdiRasterizer {}

impl GdiRasterizer {
    pub(super) fn new() -> Option<Self> {
        let hdc = unsafe { CreateCompatibleDC(ptr::null_mut()) };
        (!hdc.is_null()).then(|| Self {
            hdc,
            original_font: ptr::null_mut(),
            fonts: HashMap::new(),
        })
    }

    fn select(&mut self, face: usize, px: u16, bold: bool) -> bool {
        let hdc = self.hdc;
        let font = *self.fonts.entry((face, px, bold)).or_insert_with(|| {
            let name: Vec<u16> = FACES[face].encode_utf16().chain(Some(0)).collect();
            unsafe {
                CreateFontW(
                    -(px as i32), // negative = em height, same meaning as fontdue px
                    0,
                    0,
                    0,
                    if bold { FW_BOLD } else { FW_NORMAL },
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET,
                    0,
                    0,
                    ANTIALIASED_QUALITY,
                    0,
                    name.as_ptr(),
                )
            }
        });
        if font.is_null() {
            return false;
        }
        let prev = unsafe { SelectObject(hdc, font) };
        if self.original_font.is_null() {
            self.original_font = prev;
        }
        true
    }

    fn has_glyph(&self, c: char) -> bool {
        let mut buf = [0u16; 2];
        let units = c.encode_utf16(&mut buf);
        if units.len() != 1 {
            return false; // GetGlyphIndicesW cannot map surrogate pairs
        }
        let mut index = 0u16;
        let n = unsafe {
            GetGlyphIndicesW(
                self.hdc,
                units.as_ptr(),
                1,
                &mut index,
                GGI_MARK_NONEXISTING_GLYPHS,
            )
        };
        n != GDI_ERROR && index != 0xFFFF
    }

    /// Rasterizes `c` at `px` with the first fallback face containing it.
    pub(super) fn rasterize(&mut self, c: char, px: u16, bold: bool) -> Option<RasterGlyph> {
        for face in 0..FACES.len() {
            if self.select(face, px, bold) && self.has_glyph(c) {
                return self.rasterize_selected(c);
            }
        }
        None
    }

    fn rasterize_selected(&mut self, c: char) -> Option<RasterGlyph> {
        let mut gm = GLYPHMETRICS::default();
        let size = unsafe {
            GetGlyphOutlineW(
                self.hdc,
                c as u32,
                GGO_GRAY8_BITMAP,
                &mut gm,
                0,
                ptr::null_mut(),
                &IDENTITY,
            )
        };
        if size == GDI_ERROR {
            return None;
        }
        let advance = gm.gmCellIncX as f32;
        if size == 0 || gm.gmBlackBoxX == 0 || gm.gmBlackBoxY == 0 {
            // Whitespace-like glyph: advance only.
            return Some(RasterGlyph::empty(advance));
        }
        let mut buf = vec![0u8; size as usize];
        let res = unsafe {
            GetGlyphOutlineW(
                self.hdc,
                c as u32,
                GGO_GRAY8_BITMAP,
                &mut gm,
                size,
                buf.as_mut_ptr() as *mut c_void,
                &IDENTITY,
            )
        };
        if res == GDI_ERROR {
            return None;
        }
        let (w, h) = (gm.gmBlackBoxX as usize, gm.gmBlackBoxY as usize);
        let pitch = w.div_ceil(4) * 4; // DWORD-aligned rows
        if pitch * h > buf.len() {
            return None;
        }
        // GGO_GRAY8 coverage is 0..=64
        let coverage = (0..h)
            .flat_map(|y| buf[y * pitch..y * pitch + w].iter())
            .map(|&g| ((g as u16 * 255 + 32) / 64).min(255) as u8)
            .collect();
        Some(RasterGlyph {
            width: w as u32,
            height: h as u32,
            left: gm.gmptGlyphOrigin[0],
            top: -gm.gmptGlyphOrigin[1], // origin.y is distance *above* baseline
            advance,
            coverage,
        })
    }
}

impl Drop for GdiRasterizer {
    fn drop(&mut self) {
        unsafe {
            if !self.original_font.is_null() {
                SelectObject(self.hdc, self.original_font);
            }
            for &font in self.fonts.values() {
                if !font.is_null() {
                    DeleteObject(font);
                }
            }
            DeleteDC(self.hdc);
        }
    }
}

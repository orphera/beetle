//! Windows GDI runtime glyph fallback for CJK Kanji rendering.
//! Zero external crate dependencies; links directly to system `gdi32.dll`.

#![cfg(target_os = "windows")]
#![allow(non_snake_case, non_camel_case_types, dead_code)]

use crate::skin::ColorRgba;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::os::raw::c_int;
use std::ptr;
use tiny_skia::PixmapMut;

type HDC = *mut c_void;
type HFONT = *mut c_void;
type HGDIOBJ = *mut c_void;
type LPVOID = *mut c_void;
type LPCWSTR = *const u16;
type UINT = u32;
type DWORD = u32;
type BOOL = c_int;
type LONG = i32;
type WORD = u16;

const GDI_ERROR: DWORD = 0xFFFFFFFF;
const GGO_GRAY8_BITMAP: UINT = 6;
const FW_NORMAL: c_int = 400;
const DEFAULT_CHARSET: DWORD = 1;
const OUT_DEFAULT_PRECIS: DWORD = 0;
const CLIP_DEFAULT_PRECIS: DWORD = 0;
const CLEARTYPE_QUALITY: DWORD = 5;
const DEFAULT_PITCH: DWORD = 0;
const FF_DONTCARE: DWORD = 0;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct POINT {
    pub x: LONG,
    pub y: LONG,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct GLYPHMETRICS {
    pub gmBlackBoxX: UINT,
    pub gmBlackBoxY: UINT,
    pub gmptGlyphOrigin: POINT,
    pub gmCellIncX: i16,
    pub gmCellIncY: i16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FIXED {
    pub fract: WORD,
    pub value: i16,
}

impl FIXED {
    pub const fn from_i16(v: i16) -> Self {
        Self { fract: 0, value: v }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MAT2 {
    pub eM11: FIXED,
    pub eM12: FIXED,
    pub eM21: FIXED,
    pub eM22: FIXED,
}

impl MAT2 {
    pub const fn identity() -> Self {
        Self {
            eM11: FIXED { fract: 0, value: 1 },
            eM12: FIXED { fract: 0, value: 0 },
            eM21: FIXED { fract: 0, value: 0 },
            eM22: FIXED { fract: 0, value: 1 },
        }
    }
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateCompatibleDC(hdc: HDC) -> HDC;
    fn DeleteDC(hdc: HDC) -> BOOL;
    fn CreateFontW(
        cHeight: c_int,
        cWidth: c_int,
        cEscapement: c_int,
        cOrientation: c_int,
        cWeight: c_int,
        bItalic: DWORD,
        bUnderline: DWORD,
        bStrikeOut: DWORD,
        iCharSet: DWORD,
        iOutPrecision: DWORD,
        iClipPrecision: DWORD,
        iQuality: DWORD,
        iPitchAndFamily: DWORD,
        pszFaceName: LPCWSTR,
    ) -> HFONT;
    fn SelectObject(hdc: HDC, h: HGDIOBJ) -> HGDIOBJ;
    fn DeleteObject(ho: HGDIOBJ) -> BOOL;
    fn GetGlyphOutlineW(
        hdc: HDC,
        uChar: UINT,
        fuFormat: UINT,
        lpgm: *mut GLYPHMETRICS,
        cjBuffer: DWORD,
        pvBuffer: LPVOID,
        lpmat2: *const MAT2,
    ) -> DWORD;
}

/// An antialiased 8bpp rasterized glyph bitmap.
#[derive(Clone, Debug)]
pub struct GlyphBitmap {
    pub width: u32,
    pub height: u32,
    pub origin_x: i32,
    pub origin_y: i32,
    pub cell_inc_x: i32,
    /// Grayscale coverage values normalized to 0..=255.
    pub pixels: Vec<u8>,
}

/// GDI font rasterizer and glyph cache.
pub struct GdiFontFallback {
    hdc: HDC,
    hfont: HFONT,
    old_font: HGDIOBJ,
    cache: HashMap<char, Option<GlyphBitmap>>,
    rasterize_count: usize,
}

impl GdiFontFallback {
    /// Creates a new GDI font fallback instance using the default system GUI font.
    pub fn new() -> Option<Self> {
        unsafe {
            let hdc = CreateCompatibleDC(ptr::null_mut());
            if hdc.is_null() {
                return None;
            }

            // Height -10 creates ~10px EM square matching BitmapFont's 10x8 CJK cell size.
            let hfont = CreateFontW(
                -10,
                0,
                0,
                0,
                FW_NORMAL,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                DEFAULT_PITCH | FF_DONTCARE,
                ptr::null(),
            );

            if hfont.is_null() {
                DeleteDC(hdc);
                return None;
            }

            let old_font = SelectObject(hdc, hfont as HGDIOBJ);

            Some(Self {
                hdc,
                hfont,
                old_font,
                cache: HashMap::new(),
                rasterize_count: 0,
            })
        }
    }

    /// Looks up a character in the cache, or rasterizes it via GDI if not already cached.
    pub fn get_or_rasterize(&mut self, c: char) -> Option<&GlyphBitmap> {
        if self.cache.contains_key(&c) {
            return self.cache.get(&c).and_then(|opt| opt.as_ref());
        }

        self.rasterize_count += 1;

        let mut gm = GLYPHMETRICS::default();
        let mat = MAT2::identity();

        let needed = unsafe {
            GetGlyphOutlineW(
                self.hdc,
                c as u32,
                GGO_GRAY8_BITMAP,
                &mut gm,
                0,
                ptr::null_mut(),
                &mat,
            )
        };

        if needed == GDI_ERROR || needed == 0 || gm.gmBlackBoxX == 0 || gm.gmBlackBoxY == 0 {
            self.cache.insert(c, None);
            return None;
        }

        let mut buffer = vec![0u8; needed as usize];
        let res = unsafe {
            GetGlyphOutlineW(
                self.hdc,
                c as u32,
                GGO_GRAY8_BITMAP,
                &mut gm,
                needed,
                buffer.as_mut_ptr() as LPVOID,
                &mat,
            )
        };

        if res == GDI_ERROR || res == 0 {
            self.cache.insert(c, None);
            return None;
        }

        let pitch = ((gm.gmBlackBoxX + 3) / 4) * 4;
        let w = gm.gmBlackBoxX as usize;
        let h = gm.gmBlackBoxY as usize;

        if (h * pitch as usize) > buffer.len() {
            self.cache.insert(c, None);
            return None;
        }

        let mut tight_pixels = Vec::with_capacity(w * h);

        for row in 0..h {
            let row_start = row * pitch as usize;
            for col in 0..w {
                let gray = buffer[row_start + col];
                // GGO_GRAY8_BITMAP pixel values range from 0 to 64
                let alpha = ((gray as u16 * 255) / 64).min(255) as u8;
                tight_pixels.push(alpha);
            }
        }

        let glyph = GlyphBitmap {
            width: gm.gmBlackBoxX,
            height: gm.gmBlackBoxY,
            origin_x: gm.gmptGlyphOrigin.x,
            origin_y: gm.gmptGlyphOrigin.y,
            cell_inc_x: gm.gmCellIncX as i32,
            pixels: tight_pixels,
        };

        self.cache.insert(c, Some(glyph));
        self.cache.get(&c).and_then(|opt| opt.as_ref())
    }

    /// Number of times GetGlyphOutline was called to rasterize a new glyph.
    pub fn rasterize_count(&self) -> usize {
        self.rasterize_count
    }

    /// Number of cached glyph entries (both successes and failures).
    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }

    /// Clears the glyph cache.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.rasterize_count = 0;
    }
}

impl Drop for GdiFontFallback {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.hdc, self.old_font);
            DeleteObject(self.hfont as HGDIOBJ);
            DeleteDC(self.hdc);
        }
    }
}

thread_local! {
    static LOCAL_FALLBACK: RefCell<Option<GdiFontFallback>> = RefCell::new(None);
}

/// Fallback draw function called from `BitmapFont::draw_char`.
/// Returns `true` if the glyph was found and drawn, or `false` if it should fall through to square box.
pub fn draw_char_fallback(
    pixmap: &mut PixmapMut,
    c: char,
    x: i32,
    y: i32,
    scale: u32,
    color: ColorRgba,
) -> bool {
    LOCAL_FALLBACK.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = GdiFontFallback::new();
        }

        let fallback = match slot.as_mut() {
            Some(f) => f,
            None => return false,
        };

        if let Some(glyph) = fallback.get_or_rasterize(c) {
            blit_glyph_aa(pixmap, glyph, x, y, scale, color);
            true
        } else {
            false
        }
    })
}

/// Clears the cached glyphs on the current thread.
pub fn clear_cache() {
    LOCAL_FALLBACK.with(|cell| {
        if let Some(f) = cell.borrow_mut().as_mut() {
            f.clear_cache();
        }
    });
}

/// Returns the number of times GDI GetGlyphOutline was called for rasterization on the current thread.
pub fn rasterize_count() -> usize {
    LOCAL_FALLBACK.with(|cell| {
        cell.borrow().as_ref().map(|f| f.rasterize_count()).unwrap_or(0)
    })
}

/// Returns the number of cached glyph entries on the current thread.
pub fn cache_len() -> usize {
    LOCAL_FALLBACK.with(|cell| {
        cell.borrow().as_ref().map(|f| f.cache_len()).unwrap_or(0)
    })
}

/// Blits an antialiased 8bpp glyph bitmap onto a tiny-skia pixmap with scaling.
pub fn blit_glyph_aa(
    pixmap: &mut PixmapMut,
    glyph: &GlyphBitmap,
    x: i32,
    y: i32,
    scale: u32,
    color: ColorRgba,
) {
    if color.a == 0 || glyph.width == 0 || glyph.height == 0 {
        return;
    }

    let scale = scale.max(1);
    let pw = pixmap.width() as i32;
    let ph = pixmap.height() as i32;

    // Baseline is at row 8 to match the 8-row height of Hangul & Kana glyphs.
    let base_x = x + glyph.origin_x * scale as i32;
    let base_y = y + (8 - glyph.origin_y) * scale as i32;

    let data = pixmap.data_mut();
    let u32_slice: &mut [u32] = unsafe {
        std::slice::from_raw_parts_mut(data.as_mut_ptr() as *mut u32, data.len() / 4)
    };

    let gw = glyph.width as usize;
    let gh = glyph.height as usize;

    for row in 0..gh {
        for col in 0..gw {
            let cov = glyph.pixels[row * gw + col];
            if cov == 0 {
                continue;
            }

            let effective_a = (color.a as u32 * cov as u32) / 255;
            if effective_a == 0 {
                continue;
            }

            let px = base_x + (col as i32 * scale as i32);
            let py = base_y + (row as i32 * scale as i32);

            fill_pixel_block_aa(u32_slice, pw, ph, px, py, scale, color, effective_a);
        }
    }
}

#[inline(always)]
fn fill_pixel_block_aa(
    u32_slice: &mut [u32],
    pw: i32,
    ph: i32,
    px: i32,
    py: i32,
    scale: u32,
    color: ColorRgba,
    effective_a: u32,
) {
    if px < 0 || py < 0 || px >= pw || py >= ph {
        return;
    }

    if effective_a == 255 && scale == 1 {
        let idx = (py as usize) * (pw as usize) + (px as usize);
        if idx < u32_slice.len() {
            u32_slice[idx] = u32::from_ne_bytes([color.r, color.g, color.b, 255]);
        }
        return;
    }

    let inv_a = 255 - effective_a;
    let sr = (color.r as u32 * effective_a) / 255;
    let sg = (color.g as u32 * effective_a) / 255;
    let sb = (color.b as u32 * effective_a) / 255;

    let x_end = (px + scale as i32).min(pw);
    let y_end = (py + scale as i32).min(ph);
    let row_len = (x_end - px) as usize;

    for y in py..y_end {
        let row_start = (y as usize) * (pw as usize) + (px as usize);
        for pixel in &mut u32_slice[row_start..row_start + row_len] {
            let p = *pixel;
            let dr = p & 0xFF;
            let dg = (p >> 8) & 0xFF;
            let db = (p >> 16) & 0xFF;
            let nr = sr + (dr * inv_a) / 255;
            let ng = sg + (dg * inv_a) / 255;
            let nb = sb + (db * inv_a) / 255;
            *pixel = (255 << 24) | (nb << 16) | (ng << 8) | nr;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gdi_fallback_cache_hit_and_miss() {
        let mut fallback = GdiFontFallback::new().expect("Failed to create GdiFontFallback");
        assert_eq!(fallback.rasterize_count(), 0);
        assert_eq!(fallback.cache_len(), 0);

        // First lookup: misses cache, calls GDI
        let glyph1 = fallback.get_or_rasterize('桜');
        assert!(glyph1.is_some());
        assert_eq!(fallback.rasterize_count(), 1);
        assert_eq!(fallback.cache_len(), 1);

        // Second lookup: hits cache, no GDI call
        let glyph2 = fallback.get_or_rasterize('桜');
        assert!(glyph2.is_some());
        assert_eq!(fallback.rasterize_count(), 1);
        assert_eq!(fallback.cache_len(), 1);

        // Third lookup with a different character
        let glyph3 = fallback.get_or_rasterize('龍');
        assert!(glyph3.is_some());
        assert_eq!(fallback.rasterize_count(), 2);
        assert_eq!(fallback.cache_len(), 2);

        // Repeat third lookup: hit
        let glyph4 = fallback.get_or_rasterize('龍');
        assert!(glyph4.is_some());
        assert_eq!(fallback.rasterize_count(), 2);
        assert_eq!(fallback.cache_len(), 2);
    }

    #[test]
    fn test_gdi_unmapped_character_caching() {
        let mut fallback = GdiFontFallback::new().expect("Failed to create GdiFontFallback");
        let initial_count = fallback.rasterize_count();
        let initial_len = fallback.cache_len();

        // Null character has no blackbox glyph outline
        let missing_char = '\0';
        let res1 = fallback.get_or_rasterize(missing_char);
        assert!(res1.is_none());
        assert_eq!(fallback.rasterize_count(), initial_count + 1);
        assert_eq!(fallback.cache_len(), initial_len + 1);

        // Second query must hit cache and return None without incrementing rasterize_count
        let res2 = fallback.get_or_rasterize(missing_char);
        assert!(res2.is_none());
        assert_eq!(fallback.rasterize_count(), initial_count + 1);
        assert_eq!(fallback.cache_len(), initial_len + 1);
    }

    #[test]
    fn test_thread_local_draw_char_fallback() {
        clear_cache();
        let count_before = rasterize_count();

        let mut pixmap = tiny_skia::Pixmap::new(40, 40).unwrap();
        pixmap.fill(tiny_skia::Color::BLACK);

        let color = ColorRgba::new(255, 255, 255, 255);

        // Draw Kanji '桜'
        let drawn = draw_char_fallback(&mut pixmap.as_mut(), '桜', 5, 5, 1, color);
        assert!(drawn);
        assert_eq!(rasterize_count(), count_before + 1);

        // Check that pixels were actually rendered
        let has_bright_pixel = pixmap.data().chunks_exact(4).any(|p| p[0] > 100);
        assert!(has_bright_pixel);

        // Draw same Kanji again: must be a cache hit
        let drawn2 = draw_char_fallback(&mut pixmap.as_mut(), '桜', 5, 5, 1, color);
        assert!(drawn2);
        assert_eq!(rasterize_count(), count_before + 1);
    }

    #[test]
    fn test_draw_char_fallback_scale_2() {
        let mut pixmap1 = tiny_skia::Pixmap::new(50, 50).unwrap();
        let mut pixmap2 = tiny_skia::Pixmap::new(50, 50).unwrap();
        pixmap1.fill(tiny_skia::Color::BLACK);
        pixmap2.fill(tiny_skia::Color::BLACK);

        let color = ColorRgba::new(255, 255, 255, 255);
        let drawn1 = draw_char_fallback(&mut pixmap1.as_mut(), '東', 0, 0, 1, color);
        let drawn2 = draw_char_fallback(&mut pixmap2.as_mut(), '東', 0, 0, 2, color);

        assert!(drawn1);
        assert!(drawn2);

        let count1 = pixmap1.data().chunks_exact(4).filter(|p| p[0] > 50).count();
        let count2 = pixmap2.data().chunks_exact(4).filter(|p| p[0] > 50).count();

        // Scale 2 should render roughly 4x as many pixels
        assert!(count2 > count1 * 2);
    }

    #[test]
    fn test_multiple_instance_lifecycle() {
        // Creating and dropping instances to verify no GDI resource leaks or crashes
        for _ in 0..10 {
            let mut fb = GdiFontFallback::new().expect("Failed to create GdiFontFallback");
            let _ = fb.get_or_rasterize('曲');
            assert_eq!(fb.rasterize_count(), 1);
        }
    }
}

//! Shared helpers for real-device D3D11 tests.
#![allow(dead_code)]

use std::ffi::c_void;

#[link(name = "user32")]
extern "system" {
    fn CreateWindowExW(
        ex_style: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: *mut c_void,
        menu: *mut c_void,
        instance: *mut c_void,
        param: *mut c_void,
    ) -> *mut c_void;
    fn DestroyWindow(hwnd: *mut c_void) -> i32;
}

const WS_POPUP: u32 = 0x8000_0000;
pub const W: u32 = 256;
pub const H: u32 = 64;

pub struct HiddenWindow(pub *mut c_void);

impl HiddenWindow {
    pub fn new() -> Self {
        Self::with_size(W, H)
    }

    pub fn with_size(w: u32, h: u32) -> Self {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                [0u16].as_ptr(),
                WS_POPUP,
                0,
                0,
                w as i32,
                h as i32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert!(!hwnd.is_null(), "CreateWindowExW failed");
        Self(hwnd)
    }
}

impl Drop for HiddenWindow {
    fn drop(&mut self) {
        unsafe { DestroyWindow(self.0) };
    }
}

/// Writes tightly packed RGBA8 as a top-down 32-bit BMP (for eyeballing).
pub fn write_bmp(path: &std::path::Path, w: u32, h: u32, px: &[u8]) {
    let row = w * 4;
    let size = 54 + row * h;
    let mut out = Vec::with_capacity(size as usize);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(-(h as i32)).to_le_bytes()); // top-down
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    for p in px.chunks_exact(4) {
        out.extend_from_slice(&[p[2], p[1], p[0], 255]);
    }
    let _ = std::fs::write(path, out);
}

/// Where a render helper writes its capture: `target/<file>`.
pub fn capture_path(file: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(file)
}

/// Draws one frame three times (twice on one `Ui`, then once on a fresh
/// `Ui`) and asserts that every capture is byte-identical, so a capture can
/// serve as a regression check. `draw` must write its capture to
/// `capture_path(file)`. Returns the draw-call count of the first pass.
#[cfg(target_os = "windows")]
pub fn assert_repeatable(
    gpu: &mut beetle_render::D3d11Backend,
    file: &str,
    scale: f32,
    mut draw: impl FnMut(&mut beetle_render::D3d11Backend, &mut beetle_render::Ui) -> usize,
) -> usize {
    let path = capture_path(file);
    let mut ui = beetle_render::Ui::new(scale);
    let mut captures = Vec::with_capacity(3);
    let mut calls = 0;
    for pass in 0..3 {
        if pass == 2 {
            ui = beetle_render::Ui::new(scale);
        }
        let _ = std::fs::remove_file(&path);
        let n = draw(gpu, &mut ui);
        if pass == 0 {
            calls = n;
        }
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("{file}: pass {pass} wrote no capture: {e}"));
        captures.push(bytes);
    }
    assert!(
        captures[0] == captures[1],
        "{file}: a second draw on the same Ui differs from the first"
    );
    assert!(
        captures[0] == captures[2],
        "{file}: a draw on a fresh Ui differs from the first"
    );
    calls
}

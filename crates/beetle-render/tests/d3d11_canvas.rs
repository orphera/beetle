//! End-to-end check of Canvas → D3D11 on a real device: draws a test pattern
//! into a hidden window, reads the backbuffer back and asserts pixel values.
//! Runs on the hardware adapter and on WARP (the low-end fallback, ADR-026).
//! A BMP of each capture is written to `target/canvas-test-<driver>.bmp`.
#![cfg(target_os = "windows")]

use beetle_render::backend::d3d11::com::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use beetle_render::{Canvas, ColorRgba, D3d11Backend, GpuBackend, Insets, Rect};
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
const W: u32 = 256;
const H: u32 = 64;

struct HiddenWindow(*mut c_void);

impl HiddenWindow {
    fn new() -> Self {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                [0u16].as_ptr(),
                WS_POPUP,
                0,
                0,
                W as i32,
                H as i32,
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

fn render_pattern(driver: u32) -> Option<(String, Vec<u8>)> {
    let window = HiddenWindow::new();
    let mut gpu = match D3d11Backend::with_driver_types(window.0, W, H, &[driver]) {
        Ok(gpu) => gpu,
        // No hardware adapter on this machine (CI VM): nothing to test there.
        Err(e) if driver == D3D_DRIVER_TYPE_HARDWARE => {
            eprintln!("skipping hardware pass: {e}");
            return None;
        }
        Err(e) => panic!("WARP must always be available: {e}"),
    };

    let mut canvas = Canvas::new(256);
    let mask = canvas.atlas_mut().alloc(32, 32).unwrap();
    canvas.atlas_mut().write_alpha(mask, &[255; 32 * 32]);

    gpu.begin_frame(W, H, [0.0, 0.0, 0.0, 1.0]);
    canvas.begin(W, H);
    // 0: opaque red
    canvas.fill_rect(Rect::new(0.0, 0.0, 32.0, 32.0), ColorRgba::new(255, 0, 0, 255));
    // 1: 50% white over black
    canvas.fill_rect(
        Rect::new(32.0, 0.0, 32.0, 32.0),
        ColorRgba::new(255, 255, 255, 128),
    );
    // 2: red + additive green = yellow
    canvas.fill_rect(Rect::new(64.0, 0.0, 32.0, 32.0), ColorRgba::new(255, 0, 0, 255));
    canvas.set_additive(true);
    canvas.fill_rect(Rect::new(64.0, 0.0, 32.0, 32.0), ColorRgba::new(0, 255, 0, 255));
    canvas.set_additive(false);
    // 3: atlas sprite tinted blue
    canvas.sprite(mask, Rect::new(96.0, 0.0, 32.0, 32.0), ColorRgba::new(0, 0, 255, 255));
    // 4: clipped to its left half
    canvas.push_clip(Rect::new(128.0, 0.0, 16.0, 32.0));
    canvas.fill_rect(Rect::new(128.0, 0.0, 32.0, 32.0), ColorRgba::new(255, 255, 0, 255));
    canvas.pop_clip();
    // 5: 9-slice white
    canvas.nine_slice(
        mask,
        Insets::uniform(4.0),
        1.0,
        Rect::new(160.0, 0.0, 32.0, 32.0),
        ColorRgba::new(255, 255, 255, 255),
    );
    // bottom: black → white horizontal gradient
    canvas.fill_rect_hgradient(
        Rect::new(0.0, 40.0, 256.0, 16.0),
        ColorRgba::new(0, 0, 0, 255),
        ColorRgba::new(255, 255, 255, 255),
    );
    let calls = canvas.end(&mut gpu);
    assert_eq!(calls, 1, "whole pattern must be a single draw call");

    let (w, h, px) = gpu.capture_frame().expect("backbuffer readback");
    gpu.end_frame();
    assert_eq!((w, h), (W, H));
    Some((gpu.backend_name().to_string(), px))
}

fn rgb(px: &[u8], x: u32, y: u32) -> [u8; 3] {
    let i = ((y * W + x) * 4) as usize;
    [px[i], px[i + 1], px[i + 2]]
}

fn assert_near(name: &str, got: [u8; 3], want: [u8; 3]) {
    let ok = got
        .iter()
        .zip(want)
        .all(|(&g, w)| (g as i32 - w as i32).abs() <= 3);
    assert!(ok, "{name}: got {got:?}, want {want:?}");
}

fn write_bmp(path: &std::path::Path, px: &[u8]) {
    let row = W * 4;
    let size = 54 + row * H;
    let mut out = Vec::with_capacity(size as usize);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(W as i32).to_le_bytes());
    out.extend_from_slice(&(-(H as i32)).to_le_bytes()); // top-down
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    for p in px.chunks_exact(4) {
        out.extend_from_slice(&[p[2], p[1], p[0], 255]);
    }
    let _ = std::fs::write(path, out);
}

fn check(driver: u32, tag: &str) {
    let Some((name, px)) = render_pattern(driver) else {
        return;
    };
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    write_bmp(&target.join(format!("canvas-test-{tag}.bmp")), &px);

    assert_near(&format!("{name} red"), rgb(&px, 16, 16), [255, 0, 0]);
    assert_near(&format!("{name} 50% white"), rgb(&px, 48, 16), [128, 128, 128]);
    assert_near(&format!("{name} additive"), rgb(&px, 80, 16), [255, 255, 0]);
    assert_near(&format!("{name} sprite"), rgb(&px, 112, 16), [0, 0, 255]);
    assert_near(&format!("{name} clip in"), rgb(&px, 136, 16), [255, 255, 0]);
    assert_near(&format!("{name} clip out"), rgb(&px, 152, 16), [0, 0, 0]);
    assert_near(&format!("{name} 9-slice"), rgb(&px, 176, 16), [255, 255, 255]);
    let mid = rgb(&px, 128, 48)[0];
    assert!((120..=136).contains(&mid), "{name} gradient midpoint {mid}");
}

#[test]
fn canvas_renders_correctly_on_d3d11_hardware_and_warp() {
    // Sequential on purpose: one device at a time on the test thread.
    check(D3D_DRIVER_TYPE_HARDWARE, "hardware");
    check(D3D_DRIVER_TYPE_WARP, "warp");
}

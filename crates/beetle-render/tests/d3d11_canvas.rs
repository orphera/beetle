//! End-to-end check of Canvas → D3D11 on a real device: draws a test pattern
//! into a hidden window, reads the backbuffer back and asserts pixel values.
//! Runs on the hardware adapter and on WARP (the low-end fallback, ADR-026).
//! A BMP of each capture is written to `target/canvas-test-<driver>.bmp`.
#![cfg(target_os = "windows")]

use beetle_render::backend::d3d11::com::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use beetle_render::{Canvas, ColorRgba, D3d11Backend, GpuBackend, Insets, Rect};

mod common;
use common::{write_bmp, HiddenWindow, H, W};

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
    canvas.fill_rect(
        Rect::new(0.0, 0.0, 32.0, 32.0),
        ColorRgba::new(255, 0, 0, 255),
    );
    // 1: 50% white over black
    canvas.fill_rect(
        Rect::new(32.0, 0.0, 32.0, 32.0),
        ColorRgba::new(255, 255, 255, 128),
    );
    // 2: red + additive green = yellow
    canvas.fill_rect(
        Rect::new(64.0, 0.0, 32.0, 32.0),
        ColorRgba::new(255, 0, 0, 255),
    );
    canvas.set_additive(true);
    canvas.fill_rect(
        Rect::new(64.0, 0.0, 32.0, 32.0),
        ColorRgba::new(0, 255, 0, 255),
    );
    canvas.set_additive(false);
    // 3: atlas sprite tinted blue
    canvas.sprite(
        mask,
        Rect::new(96.0, 0.0, 32.0, 32.0),
        ColorRgba::new(0, 0, 255, 255),
    );
    // 4: clipped to its left half
    canvas.push_clip(Rect::new(128.0, 0.0, 16.0, 32.0));
    canvas.fill_rect(
        Rect::new(128.0, 0.0, 32.0, 32.0),
        ColorRgba::new(255, 255, 0, 255),
    );
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

fn check(driver: u32, tag: &str) {
    let Some((name, px)) = render_pattern(driver) else {
        return;
    };
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    write_bmp(&target.join(format!("canvas-test-{tag}.bmp")), W, H, &px);

    assert_near(&format!("{name} red"), rgb(&px, 16, 16), [255, 0, 0]);
    assert_near(
        &format!("{name} 50% white"),
        rgb(&px, 48, 16),
        [128, 128, 128],
    );
    assert_near(&format!("{name} additive"), rgb(&px, 80, 16), [255, 255, 0]);
    assert_near(&format!("{name} sprite"), rgb(&px, 112, 16), [0, 0, 255]);
    assert_near(&format!("{name} clip in"), rgb(&px, 136, 16), [255, 255, 0]);
    assert_near(&format!("{name} clip out"), rgb(&px, 152, 16), [0, 0, 0]);
    assert_near(
        &format!("{name} 9-slice"),
        rgb(&px, 176, 16),
        [255, 255, 255],
    );
    let mid = rgb(&px, 128, 48)[0];
    assert!((120..=136).contains(&mid), "{name} gradient midpoint {mid}");
}

#[test]
fn canvas_renders_correctly_on_d3d11_hardware_and_warp() {
    // Sequential on purpose: one device at a time on the test thread.
    check(D3D_DRIVER_TYPE_HARDWARE, "hardware");
    check(D3D_DRIVER_TYPE_WARP, "warp");
}

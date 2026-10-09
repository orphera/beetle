//! Boot screen on a real D3D11 device (WARP): mid-entrance and settled.
//! Captures go to `target/boot-*.bmp`.
#![cfg(target_os = "windows")]

mod common;

use beetle_render::backend::d3d11::com::D3D_DRIVER_TYPE_WARP;
use beetle_render::{draw_boot, BootFrame, D3d11Backend, GpuBackend, Ui, Viewport};
use common::{write_bmp, HiddenWindow};

const W: u32 = 1280;
const H: u32 = 720;

fn render(gpu: &mut D3d11Backend, ui: &mut Ui, title: &str, elapsed: f64, name: &str) -> usize {
    let vp = Viewport::new(W, H);
    gpu.begin_frame(W, H, [0.0, 0.0, 0.0, 1.0]);
    ui.begin(W, H, vp.scale);
    draw_boot(
        ui,
        &BootFrame {
            viewport: &vp,
            elapsed,
            title,
            status: "Reading song library",
        },
    );
    let calls = ui.end(gpu);
    let (w, h, px) = gpu.capture_frame().expect("readback");
    gpu.end_frame();
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    write_bmp(&target.join(format!("boot-{name}.bmp")), w, h, &px);
    calls
}

#[test]
fn boot_layouts() {
    let window = HiddenWindow::with_size(W, H);
    let mut gpu = D3d11Backend::with_driver_types(window.0, W, H, &[D3D_DRIVER_TYPE_WARP])
        .expect("WARP device");
    let mut ui = Ui::new(1.0);
    assert_eq!(render(&mut gpu, &mut ui, "STARTING UP", 0.12, "enter"), 1);
    assert_eq!(
        render(&mut gpu, &mut ui, "RESCANNING LIBRARY", 1.9, "rescan"),
        1
    );
}

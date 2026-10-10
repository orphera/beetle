//! Judge offset calibration on a real D3D11 device (WARP): the test in
//! progress (beat indicator lit, presses on the mark line, one press left out
//! of the mean) and the finished test with its suggestion.
//! Captures go to `target/settings-calibrate*.bmp` for layout review.
#![cfg(target_os = "windows")]

mod common;

use beetle_render::backend::d3d11::com::D3D_DRIVER_TYPE_WARP;
use beetle_render::{
    draw_calibrate, CalibrateFrame, CalibratePhase, D3d11Backend, GpuBackend, Ui, Viewport,
};
use common::{write_bmp, HiddenWindow};

const W: u32 = 1280;
const H: u32 = 720;

fn render(gpu: &mut D3d11Backend, ui: &mut Ui, frame: &CalibrateFrame, name: &str) -> usize {
    gpu.begin_frame(W, H, [0.0, 0.0, 0.0, 1.0]);
    ui.begin(W, H, frame.viewport.scale);
    draw_calibrate(ui, frame);
    let calls = ui.end(gpu);
    let (w, h, px) = gpu.capture_frame().expect("readback");
    gpu.end_frame();
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    write_bmp(&target.join(format!("settings-{name}.bmp")), w, h, &px);
    calls
}

#[test]
fn calibrate_captures() {
    let window = HiddenWindow::with_size(W, H);
    let mut gpu = D3d11Backend::with_driver_types(window.0, W, H, &[D3D_DRIVER_TYPE_WARP])
        .expect("WARP device");
    let mut ui = Ui::new(1.0);
    let vp = Viewport::new(W, H);

    // Mid-test: 9 counted presses, one of them far off and left out.
    let marks = [
        (12.0, true),
        (-8.0, true),
        (25.0, true),
        (18.0, true),
        (-3.0, true),
        (21.0, true),
        (14.0, true),
        (-120.0, false),
        (16.0, true),
    ];
    let measuring = CalibrateFrame {
        viewport: &vp,
        phase: CalibratePhase::Measuring,
        pulse: 0.7,
        required: 16,
        collected: marks.len(),
        window_ms: 150.0,
        marks: &marks,
        mean_ms: Some(12.7),
        std_ms: Some(10.4),
        suggestion_ms: None,
    };
    assert_eq!(render(&mut gpu, &mut ui, &measuring, "calibrate"), 1);

    // Finished: the suggestion with 적용 / 다시 / 취소.
    let done = CalibrateFrame {
        phase: CalibratePhase::Done,
        pulse: 0.0,
        collected: 16,
        suggestion_ms: Some(-13.0),
        ..measuring
    };
    assert_eq!(render(&mut gpu, &mut ui, &done, "calibrate-done"), 1);
}

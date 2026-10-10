//! The swap chain is flip model with a frame latency of one on Windows 10
//! and later, on the hardware adapter and on WARP, and frames still go
//! through: a hidden window must not stall `begin_frame` past its bound.
#![cfg(target_os = "windows")]

use std::time::{Duration, Instant};

use beetle_render::backend::d3d11::com::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use beetle_render::{D3d11Backend, GpuBackend};

mod common;
use common::{HiddenWindow, H, W};

#[test]
fn the_swap_chain_is_flip_model_with_one_frame_of_latency() {
    for driver in [D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP] {
        let window = HiddenWindow::new();
        let mut gpu = match D3d11Backend::with_driver_types(window.0, W, H, &[driver]) {
            Ok(gpu) => gpu,
            Err(e) if driver == D3D_DRIVER_TYPE_HARDWARE => {
                eprintln!("skipping hardware pass: {e}");
                continue;
            }
            Err(e) => panic!("WARP must always be available: {e}"),
        };
        assert!(
            gpu.present_mode().starts_with("flip, frame latency 1"),
            "{}: {}",
            gpu.backend_name(),
            gpu.present_mode()
        );

        // Unlocked and vsync frames, and a resize in between.
        let started = Instant::now();
        for (i, vsync) in [false, false, true, true, false].into_iter().enumerate() {
            gpu.set_vsync(vsync);
            let (w, h) = if i == 2 { (W * 2, H * 2) } else { (W, H) };
            gpu.begin_frame(w, h, [0.0, 0.0, 0.0, 1.0]);
            gpu.end_frame();
        }
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}

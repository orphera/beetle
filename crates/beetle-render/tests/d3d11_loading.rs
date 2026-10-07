//! Loading screen on a real D3D11 device (WARP): mid-entrance and settled,
//! with a long title. Captures go to `target/loading-*.bmp`.
#![cfg(target_os = "windows")]

mod common;

use beetle_core::{PlayMode, SongMetadata};
use beetle_render::backend::d3d11::com::D3D_DRIVER_TYPE_WARP;
use beetle_render::{draw_loading, D3d11Backend, GpuBackend, LoadingFrame, Ui, Viewport};
use common::{write_bmp, HiddenWindow};

const W: u32 = 1280;
const H: u32 = 720;

fn render(gpu: &mut D3d11Backend, ui: &mut Ui, title: &str, elapsed: f64, name: &str) -> usize {
    let vp = Viewport::new(W, H);
    let song = SongMetadata {
        hash: 1,
        file_path: String::new(),
        title: title.into(),
        subtitle: String::new(),
        artist: "モリモリあつし".into(),
        genre: "Future Bass".into(),
        bpm: 174.0,
        bpm_min: 174.0,
        bpm_max: 174.0,
        play_level: 10,
        notes_count: 1873,
        play_mode: PlayMode::Keys7,
    };
    let chips = vec!["HI-SPEED 1100".to_string(), "REGULAR".into(), "GROOVE".into()];
    gpu.begin_frame(W, H, [0.0, 0.0, 0.0, 1.0]);
    ui.begin(W, H, vp.scale);
    draw_loading(
        ui,
        &LoadingFrame {
            viewport: &vp,
            song: &song,
            jacket: None,
            ambient: None,
            elapsed,
            status: "Decoding keysounds",
            option_chips: &chips,
            badge: None,
        },
    );
    let calls = ui.end(gpu);
    let (w, h, px) = gpu.capture_frame().expect("readback");
    gpu.end_frame();
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    write_bmp(&target.join(format!("loading-{name}.bmp")), w, h, &px);
    calls
}

#[test]
fn loading_layouts() {
    let window = HiddenWindow::with_size(W, H);
    let mut gpu = D3d11Backend::with_driver_types(window.0, W, H, &[D3D_DRIVER_TYPE_WARP]).expect("WARP device");
    let mut ui = Ui::new(1.0);
    assert_eq!(render(&mut gpu, &mut ui, "MilK", 0.12, "enter"), 1);
    assert_eq!(render(&mut gpu, &mut ui, "Love & Justice ~Endless Summer Night Extended Mix~", 1.9, "long"), 1);
}

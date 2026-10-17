//! The blurred jacket backdrop on a real D3D11 device (WARP): the loading
//! screen with and without the picture. The picture must not make the frame
//! brighter (mean luma), and the frame stays one batch. Captures go to
//! `target/backdrop-*.bmp`.
#![cfg(target_os = "windows")]

mod common;

use std::time::{Duration, Instant};

use beetle_core::{ChartId, PlayMode, SongMetadata};
use beetle_render::backdrop::blur_backdrop;
use beetle_render::backend::d3d11::com::D3D_DRIVER_TYPE_WARP;
use beetle_render::jacket::JacketKey;
use beetle_render::skin::ColorRgba;
use beetle_render::{
    draw_loading, strings, D3d11Backend, GpuBackend, ImageBuffer, LoadingFrame, Ui, Viewport,
};
use common::{write_bmp, HiddenWindow};

const W: u32 = 1280;
const H: u32 = 720;

fn song() -> SongMetadata {
    SongMetadata {
        id: ChartId::synthetic(42),
        md5: [0; 16],
        ln_count: 0,
        ln_mode: None,
        legacy_hash: 42,
        file_path: String::new(),
        title: "Backdrop Probe".into(),
        subtitle: String::new(),
        artist: "Probe Artist".into(),
        genre: "GENRE".into(),
        bpm: 150.0,
        bpm_min: 150.0,
        bpm_max: 150.0,
        play_level: 12,
        notes_count: 1500,
        play_mode: PlayMode::Keys7,
    }
}

/// A bright, colourful stand-in for a jacket: mean luma about 127, the
/// worst case for the scrim.
fn bright_art() -> ImageBuffer {
    let (w, h) = (1280u32, 720u32);
    let mut img = ImageBuffer::new(w, h, ColorRgba::new(0, 0, 0, 255));
    for y in 0..h {
        for x in 0..w {
            let r = (x * 255 / w) as u8;
            let g = (y * 255 / h) as u8;
            let b = 128;
            img.pixels[(y * w + x) as usize] = ColorRgba::new(r, g, b, 255);
        }
    }
    img
}

fn mean_luma(px: &[u8]) -> f64 {
    let total: f64 = px
        .chunks_exact(4)
        .map(|p| 0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64)
        .sum();
    total / (px.len() / 4) as f64
}

/// Draws the loading screen once; `art` shows the blurred picture of `song`
/// (already settled, so no crossfade is in progress). Returns the draw-call
/// count and the mean luma of the captured frame.
fn frame(gpu: &mut D3d11Backend, ui: &mut Ui, art: bool, name: &str) -> (usize, f64) {
    let vp = Viewport::new(W, H);
    let song = song();
    let chips = vec![strings::fill(strings::CHIP_GREEN, &["500"])];
    ui.lite = false;
    gpu.begin_frame(W, H, [0.0, 0.0, 0.0, 1.0]);
    ui.backdrop.reset();
    ui.begin(W, H, vp.scale);
    if art {
        // Shown a second ago, so the crossfade has finished.
        let settled = Instant::now() - Duration::from_secs(1);
        let px = blur_backdrop(&bright_art()).expect("pixels");
        assert!(ui.backdrop.show(
            ui.canvas.atlas_mut(),
            JacketKey::Chart(song.id),
            settled,
            || px.clone(),
        ));
    }
    draw_loading(
        ui,
        &LoadingFrame {
            viewport: &vp,
            song: &song,
            notes: 1500,
            jacket: None,
            ambient: None,
            elapsed: 3.3,
            status: strings::LOADING,
            option_chips: &chips,
            badge: None,
        },
    );
    let calls = ui.end(gpu);
    let (w, h, px) = gpu.capture_frame().expect("readback");
    gpu.end_frame();
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    write_bmp(&target.join(format!("backdrop-{name}.bmp")), w, h, &px);
    (calls, mean_luma(&px))
}

#[test]
fn blurred_backdrop_is_one_batch_and_not_brighter() {
    let window = HiddenWindow::with_size(W, H);
    let mut gpu = D3d11Backend::with_driver_types(window.0, W, H, &[D3D_DRIVER_TYPE_WARP])
        .expect("WARP device");
    let mut ui = Ui::new(1.0);

    let (calls_plain, luma_plain) = frame(&mut gpu, &mut ui, false, "plain");
    let (calls_art, luma_art) = frame(&mut gpu, &mut ui, true, "art");
    eprintln!(
        "backdrop: batches {calls_plain} -> {calls_art}, mean luma {luma_plain:.2} -> {luma_art:.2}"
    );
    assert_eq!(calls_plain, 1);
    assert_eq!(calls_art, 1, "the picture rides in the main batch");
    assert!(
        luma_art <= luma_plain + 0.5,
        "backdrop brightens the frame: {luma_plain:.2} -> {luma_art:.2}"
    );
}

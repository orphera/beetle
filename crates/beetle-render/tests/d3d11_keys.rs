//! Key config screen on a real D3D11 device (WARP): 7K, 14K double play
//! and 9K while rebinding. Captures go to `target/keys-*.bmp`.
#![cfg(target_os = "windows")]

mod common;

use beetle_core::{Lane, PlayMode};
use beetle_render::backend::d3d11::com::D3D_DRIVER_TYPE_WARP;
use beetle_render::{
    draw_key_config, D3d11Backend, GpuBackend, KeyBinding, KeyConfigFrame, Rebind, SkinConfig, Ui,
    Viewport,
};
use common::{write_bmp, HiddenWindow};

const W: u32 = 1280;
const H: u32 = 720;

fn keys_for(lane: Lane) -> &'static [&'static str] {
    match lane {
        Lane::Scratch => &["LShift", "LCtrl"],
        Lane::Key4 => &["Space", "B", "N", "M"],
        Lane::Key9 => &[],
        Lane::P2Scratch => &["RShift", "RCtrl"],
        other => std::slice::from_ref(key_for(other)),
    }
}

fn key_for(lane: Lane) -> &'static &'static str {
    match lane {
        Lane::Scratch => &"LShift",
        Lane::Key1 => &"Z",
        Lane::Key2 => &"S",
        Lane::Key3 => &"X",
        Lane::Key4 => &"Space",
        Lane::Key5 => &"C",
        Lane::Key6 => &"F",
        Lane::Key7 => &"V",
        Lane::Key8 => &";",
        Lane::Key9 => &"None",
        Lane::P2Scratch => &"RShift",
        Lane::P2Key1 => &"U",
        Lane::P2Key2 => &"I",
        Lane::P2Key3 => &"O",
        Lane::P2Key4 => &"P",
        Lane::P2Key5 => &"[",
        Lane::P2Key6 => &"]",
        Lane::P2Key7 => &"\\",
    }
}

fn render(gpu: &mut D3d11Backend, ui: &mut Ui, mode: PlayMode, selected: usize, rebinding: Option<Rebind>, name: &str) -> usize {
    let vp = Viewport::new(W, H);
    let mut layout = SkinConfig::default();
    layout.set_play_mode(mode);
    let labels: Vec<String> = layout.active_lanes().iter().map(|l| format!("{l:?}").to_uppercase()).collect();
    let lanes: Vec<KeyBinding> = layout
        .active_lanes()
        .iter()
        .zip(&labels)
        .map(|(&lane, label)| KeyBinding { lane, label, keys: keys_for(lane) })
        .collect();
    gpu.begin_frame(W, H, [0.0, 0.0, 0.0, 1.0]);
    ui.begin(W, H, vp.scale);
    draw_key_config(ui, &KeyConfigFrame { viewport: &vp, mode, lanes: &lanes, selected, rebinding, layout: "ArcadeZx (Z S X D C F V)" });
    let calls = ui.end(gpu);
    let (w, h, px) = gpu.capture_frame().expect("readback");
    gpu.end_frame();
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    write_bmp(&target.join(format!("keys-{name}.bmp")), w, h, &px);
    calls
}

#[test]
fn key_config_layouts() {
    let window = HiddenWindow::with_size(W, H);
    let mut gpu = D3d11Backend::with_driver_types(window.0, W, H, &[D3D_DRIVER_TYPE_WARP]).expect("WARP device");
    let mut ui = Ui::new(1.0);
    assert_eq!(render(&mut gpu, &mut ui, PlayMode::Keys7, 4, None, "7k"), 1);
    assert_eq!(render(&mut gpu, &mut ui, PlayMode::Keys14, 0, None, "14k"), 1);
    assert_eq!(render(&mut gpu, &mut ui, PlayMode::Keys9, 3, Some(Rebind::Add), "9k-add"), 1);
}

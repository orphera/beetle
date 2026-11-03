//! Settings screen on a real D3D11 device (WARP): the page tabs and a page,
//! a restart-required value, and the help card for two rows.
//! Captures go to `target/settings-*.bmp` for layout review.
#![cfg(target_os = "windows")]

mod common;

use beetle_render::backend::d3d11::com::D3D_DRIVER_TYPE_WARP;
use beetle_render::strings;
use beetle_render::{
    draw_settings, D3d11Backend, GpuBackend, OptionLine, SettingsFrame, Ui, Viewport,
};
use common::{assert_repeatable, write_bmp, HiddenWindow};

const W: u32 = 1280;
const H: u32 = 720;

/// Settings rows in table order; a section starts a page.
fn lines(graphics_after_restart: bool) -> Vec<OptionLine<'static>> {
    let graphics = if graphics_after_restart {
        strings::fill(strings::VALUE_AFTER_RESTART(), &[strings::GPU_WARP()])
    } else {
        strings::GPU_AUTO().to_string()
    };
    let rows: [(Option<&'static str>, &'static str, String); 11] = [
        (
            Some(strings::GROUP_DISPLAY()),
            strings::ROW_DISPLAY_MODE(),
            strings::DISPLAY_WINDOWED().into(),
        ),
        (
            None,
            strings::ROW_RESOLUTION(),
            "1280 x 720 (16:9 HD)".into(),
        ),
        (None, strings::ROW_GRAPHICS(), graphics),
        (
            None,
            strings::ROW_TARGET_FPS(),
            strings::VALUE_UNLIMITED().into(),
        ),
        (
            Some(strings::GROUP_AUDIO()),
            strings::ROW_MASTER_VOLUME(),
            "80%".into(),
        ),
        (
            Some(strings::GROUP_JUDGE_INPUT()),
            strings::ROW_JUDGE_OFFSET(),
            "+0 ms".into(),
        ),
        (
            Some(strings::GROUP_PLAY_SCREEN()),
            strings::ROW_PLAYFIELD(),
            strings::VALUE_CENTER().into(),
        ),
        (None, strings::ROW_SCRATCH(), strings::SIDE_RIGHT().into()),
        (None, strings::ROW_BGA(), strings::VALUE_ON().into()),
        (
            None,
            strings::ROW_TRACK_BGA(),
            strings::TRACK_BGA_OFF().into(),
        ),
        (None, strings::ROW_KEY_LAYOUT(), "7K  HomeRow".into()),
    ];
    rows.into_iter()
        .map(|(section, label, value)| OptionLine {
            section,
            label,
            value,
            stepper: true,
        })
        .collect()
}

/// Help sentence per row, in the same order as `lines`.
fn helps() -> [&'static str; 11] {
    [
        strings::HELP_DISPLAY_MODE(),
        strings::HELP_RESOLUTION(),
        strings::HELP_GRAPHICS(),
        strings::HELP_TARGET_FPS(),
        strings::HELP_MASTER_VOLUME(),
        strings::HELP_JUDGE_OFFSET(),
        strings::HELP_PLAYFIELD(),
        strings::HELP_SCRATCH(),
        strings::HELP_BGA(),
        strings::HELP_TRACK_BGA(),
        strings::HELP_KEY_LAYOUT(),
    ]
}

fn render(
    gpu: &mut D3d11Backend,
    ui: &mut Ui,
    selected: usize,
    restart: bool,
    name: &str,
) -> usize {
    let vp = Viewport::new(W, H);
    let lines = lines(restart);
    gpu.begin_frame(W, H, [0.0, 0.0, 0.0, 1.0]);
    ui.begin(W, H, vp.scale);
    draw_settings(
        ui,
        &SettingsFrame {
            viewport: &vp,
            lines: &lines,
            selected,
            help: helps()[selected],
            scroll: 0.0,
        },
    );
    let calls = ui.end(gpu);
    let (w, h, px) = gpu.capture_frame().expect("readback");
    gpu.end_frame();
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    write_bmp(&target.join(format!("settings-{name}.bmp")), w, h, &px);
    calls
}

#[test]
fn settings_layouts() {
    let window = HiddenWindow::with_size(W, H);
    let mut gpu = D3d11Backend::with_driver_types(window.0, W, H, &[D3D_DRIVER_TYPE_WARP])
        .expect("WARP device");
    // Graphics row selected, with a value that waits for a restart.
    assert_eq!(
        assert_repeatable(&mut gpu, "settings-display.bmp", 1.0, |gpu, ui| {
            render(gpu, ui, 2, true, "display")
        }),
        1
    );
    // Key layout row (the last page, the last row) selected.
    assert_eq!(
        assert_repeatable(&mut gpu, "settings-keys.bmp", 1.0, |gpu, ui| {
            render(gpu, ui, 10, false, "keys")
        }),
        1
    );
}

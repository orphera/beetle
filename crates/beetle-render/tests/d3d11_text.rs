//! Renders a text specimen through TextEngine → Canvas → D3D11 (WARP, so it
//! runs anywhere) and checks basic properties of the result. The capture is
//! written to `target/text-specimen.bmp` for visual review.
#![cfg(target_os = "windows")]

mod common;

use beetle_render::backend::d3d11::com::D3D_DRIVER_TYPE_WARP;
use beetle_render::{
    Align, Canvas, ColorRgba, D3d11Backend, GpuBackend, Rect, TextEngine, TextStyle,
};
use common::{write_bmp, HiddenWindow};

const SW: u32 = 960;
const SH: u32 = 540;

#[test]
fn text_specimen_renders_in_one_draw_call() {
    let window = HiddenWindow::with_size(SW, SH);
    let mut gpu = D3d11Backend::with_driver_types(window.0, SW, SH, &[D3D_DRIVER_TYPE_WARP])
        .expect("WARP device");
    let mut canvas = Canvas::default();
    let mut text = TextEngine::new();

    let white = ColorRgba::new(0xF2, 0xF4, 0xF8, 255);
    let muted = ColorRgba::new(0x8A, 0x92, 0xA6, 255);
    let cyan = ColorRgba::new(0x3D, 0xE0, 0xFF, 255);
    let panel = ColorRgba::new(0x14, 0x17, 0x22, 255);

    gpu.begin_frame(SW, SH, [0.03, 0.035, 0.05, 1.0]);
    canvas.begin(SW, SH);

    let mut y = 40.0;
    for px in [11.0, 12.0, 14.0, 16.0, 20.0, 24.0] {
        let s = TextStyle::new(px).color(white);
        let adv = text.draw(
            &mut canvas,
            &format!("{px}px  Beetle — Song Select  AVATAR To Ty 0123456789"),
            24.0,
            y,
            &s,
        );
        text.draw(
            &mut canvas,
            "한글 제목  日本語のタイトル",
            24.0 + adv + 16.0,
            y,
            &s.color(muted),
        );
        y += px * 1.6;
    }

    y += 16.0;
    text.draw(
        &mut canvas,
        "EX SCORE",
        24.0,
        y,
        &TextStyle::new(12.0).bold().color(muted).tracking(2.0),
    );
    text.draw(
        &mut canvas,
        "2,847",
        24.0,
        y + 64.0,
        &TextStyle::new(64.0).bold().color(white),
    );
    text.draw(
        &mut canvas,
        "AAA",
        260.0,
        y + 64.0,
        &TextStyle::new(64.0).bold().color(cyan),
    );
    text.draw_scaled(
        &mut canvas,
        "COMBO",
        480.0,
        y + 64.0,
        &TextStyle::new(32.0).bold().color(white),
        1.5,
    );

    // Panel with interleaved shape/text draws and ellipsis fitting.
    let card = Rect::new(24.0, 420.0, 420.0, 48.0);
    canvas.fill_rect(card, panel);
    canvas.fill_rect(Rect::new(card.x, card.y, 3.0, card.h), cyan);
    text.draw_in(
        &mut canvas,
        "Rare glyphs 龍 ★ via GDI — and a title far too long to fit in this card",
        card.inset(14.0),
        Align::Left,
        &TextStyle::new(16.0).color(white),
    );
    text.draw_in(
        &mut canvas,
        "Lv.12",
        Rect::new(460.0, 420.0, 120.0, 48.0),
        Align::Center,
        &TextStyle::new(20.0).bold().color(cyan),
    );

    let calls = canvas.end(&mut gpu);
    let (w, h, px) = gpu.capture_frame().expect("readback");
    gpu.end_frame();

    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    write_bmp(&target.join("text-specimen.bmp"), w, h, &px);

    assert_eq!(calls, 1, "all text and shapes in one draw call");
    // The 64px bold "2,847" must have produced bright, fully covered pixels.
    let lit = px
        .chunks_exact(4)
        .filter(|p| p[0] > 230 && p[1] > 230 && p[2] > 230)
        .count();
    assert!(lit > 2000, "expected solid text pixels, got {lit}");
}

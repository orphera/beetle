//! Generated-skin showcase on a real D3D11 device (WARP): a song-select
//! composition and a gameplay composition built only from `Ui` (Canvas +
//! TextEngine + Skin). Captures go to `target/skin-menu.bmp` and
//! `target/skin-gameplay.bmp` for art-direction review.
#![cfg(target_os = "windows")]

mod common;

use beetle_render::backend::d3d11::com::D3D_DRIVER_TYPE_WARP;
use beetle_render::{Align, ColorRgba, D3d11Backend, GpuBackend, Rect, TextStyle, Ui};
use common::{write_bmp, HiddenWindow};
use std::time::Instant;

const W: u32 = 1280;
const H: u32 = 720;

const fn rgb(hex: u32) -> ColorRgba {
    ColorRgba::new((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, 255)
}
const fn rgba(hex: u32, a: u8) -> ColorRgba {
    ColorRgba::new((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, a)
}

const BG: ColorRgba = rgb(0x05060a);
const SURF1: ColorRgba = rgb(0x0b0e16);
const SURF2: ColorRgba = rgb(0x11151f);
const SURF3: ColorRgba = rgb(0x171c29);
const LINE: ColorRgba = rgb(0x232a3c);
const CYAN: ColorRgba = rgb(0x00e5ff);
const MAGENTA: ColorRgba = rgb(0xff2d6a);
const GOLD: ColorRgba = rgb(0xffd400);
const TEXT: ColorRgba = rgb(0xf2f4fa);
const MUTED: ColorRgba = rgb(0x7b8299);
const MUTED2: ColorRgba = rgb(0x4c5368);

fn backdrop(ui: &mut Ui, ambient: ColorRgba) {
    let c = &mut ui.canvas;
    let full = Rect::new(0.0, 0.0, W as f32, H as f32);
    c.fill_rect_vgradient(full, BG, SURF1);
    c.set_additive(true);
    let glow = ui.skin.glow;
    c.sprite_centered(glow, 1050.0, 120.0, 1100.0, 760.0, ambient.with_alpha(70));
    c.sprite_centered(glow, 120.0, 700.0, 900.0, 500.0, CYAN.with_alpha(22));
    c.tile(ui.skin.noise, full, ColorRgba::new(255, 255, 255, 7));
    c.set_additive(false);
    // Vignette darkens the backdrop only; UI drawn afterwards stays crisp.
    c.sprite(ui.skin.vignette, full, ColorRgba::new(255, 255, 255, 200));
}

fn render_menu(ui: &mut Ui) {
    backdrop(ui, MAGENTA);
    let Ui { canvas: c, text: t, skin } = ui;

    // Top bar
    c.fill_rect(Rect::new(0.0, 0.0, W as f32, 64.0), rgba(0x05060a, 200));
    c.fill_rect(Rect::new(0.0, 63.0, W as f32, 1.0), LINE);
    let adv = t.draw(c, "BEETLE", 32.0, 41.0, &TextStyle::new(22.0).bold().color(TEXT).tracking(3.0));
    c.fill_rect_hgradient(Rect::new(32.0, 62.0, adv, 2.0), CYAN, MAGENTA.with_alpha(0));
    let mut x = 32.0 + adv + 48.0;
    for (i, tab) in ["ALL SONGS", "FAVORITES", "RECENT", "COURSES"].iter().enumerate() {
        let st = TextStyle::new(13.0).bold().tracking(1.5).color(if i == 0 { TEXT } else { MUTED });
        let w = t.draw(c, tab, x, 38.0, &st);
        if i == 0 {
            c.fill_rect(Rect::new(x, 61.0, w, 3.0), CYAN);
        }
        x += w + 32.0;
    }
    let search = Rect::new(W as f32 - 32.0 - 280.0, 16.0, 280.0, 32.0);
    c.nine(&skin.panel_lg, search, SURF2);
    t.draw_in(c, "Search title, artist, tag", search.inset(16.0), Align::Left, &TextStyle::new(13.0).color(MUTED2));

    // Song list
    let songs = [
        ("12", "Absolute Zero — 絶対零度の夜明け", "kanone feat. 初音ミク", MAGENTA, "AAA", 0.92),
        ("11", "Lost Vector", "Shiraishi Ren", rgb(0xc06bff), "AA", 0.81),
        ("12", "冥 -MEI- (Original Mix)", "Amuro vs Killer", MAGENTA, "AAA", 0.95),
        ("10", "가을밤의 신호등", "모래시계 사운드", rgb(0x3ce07a), "A", 0.72),
        ("9", "Chrono Diver -PENDULUMs-", "Sound Holic", rgb(0xff9a3c), "AA", 0.84),
        ("12", "Concertino in Blue", "Tatsh", MAGENTA, "B", 0.61),
        ("8", "Neon Rain Parade", "VOID", rgb(0x50b0ff), "AAA", 0.9),
    ];
    let selected = 2;
    let (lx, lw, rh) = (32.0, 640.0, 60.0);
    let mut y = 96.0;
    for (i, (lv, title, artist, diff, rank, rate)) in songs.iter().enumerate() {
        let row = Rect::new(lx, y, lw, rh);
        if i == selected {
            c.halo(&skin.shadow, row, ColorRgba::new(255, 255, 255, 200));
            c.nine(&skin.panel, row, SURF3);
            c.push_clip(row);
            c.fill_rect_hgradient(Rect::new(row.x, row.y, 360.0, row.h), CYAN.with_alpha(40), CYAN.with_alpha(0));
            c.pop_clip();
            c.nine(&skin.panel_outline, row, CYAN.with_alpha(200));
            c.set_additive(true);
            c.sprite_centered(skin.glow, row.x, row.y + row.h / 2.0, 80.0, 120.0, CYAN.with_alpha(120));
            c.set_additive(false);
            c.fill_rect(Rect::new(row.x, row.y + 12.0, 3.0, row.h - 24.0), CYAN);
        } else {
            c.nine(&skin.panel, row, rgba(0x0b0e16, 220));
        }
        let badge = Rect::new(row.x + 16.0, row.y + 14.0, 40.0, 32.0);
        c.nine(&skin.panel_sm, badge, diff.with_alpha(40));
        c.nine(&skin.panel_sm, Rect::new(badge.x, badge.bottom() - 3.0, badge.w, 3.0), *diff);
        t.draw_in(c, lv, badge, Align::Center, &TextStyle::new(17.0).bold().color(*diff));
        let tx = badge.right() + 16.0;
        let title_w = lw - (tx - lx) - 150.0;
        let title = t.fit(c, title, title_w, &TextStyle::new(17.0).bold()).into_owned();
        t.draw(c, &title, tx, row.y + 28.0, &TextStyle::new(17.0).bold().color(TEXT));
        t.draw(c, artist, tx, row.y + 47.0, &TextStyle::new(12.0).color(MUTED));
        // rank + score bar
        let rx = row.right() - 120.0;
        let bar = Rect::new(rx, row.y + 38.0, 96.0, 4.0);
        c.nine(&skin.panel_sm, bar, LINE);
        c.nine(&skin.panel_sm, Rect::new(bar.x, bar.y, bar.w * rate, bar.h), if *rank == "AAA" { GOLD } else { CYAN });
        t.draw_in(c, rank, Rect::new(rx, row.y + 8.0, 96.0, 26.0), Align::Right, &TextStyle::new(16.0).bold().color(if *rank == "AAA" { GOLD } else { TEXT }));
        y += rh + 8.0;
    }

    // Detail panel
    let panel = Rect::new(704.0, 96.0, 544.0, 468.0);
    c.halo(&skin.shadow, panel, ColorRgba::new(255, 255, 255, 160));
    c.nine(&skin.panel_lg, panel, rgba(0x0b0e16, 235));
    let jacket = Rect::new(panel.x + 24.0, panel.y + 24.0, 200.0, 200.0);
    c.nine(&skin.panel, jacket, MAGENTA);
    c.push_clip(jacket);
    c.fill_rect_corners(jacket, [MAGENTA, rgb(0x6a1b9a), rgb(0x1a0b3a), rgb(0xc0185a)]);
    c.set_additive(true);
    c.sprite_centered(skin.flare, jacket.x + 140.0, jacket.y + 60.0, 180.0, 180.0, ColorRgba::new(255, 255, 255, 140));
    c.set_additive(false);
    c.pop_clip();
    let ix = jacket.right() + 24.0;
    t.draw(c, "ANOTHER", ix, panel.y + 44.0, &TextStyle::new(11.0).bold().tracking(2.0).color(MAGENTA));
    t.draw(c, "冥 -MEI-", ix, panel.y + 80.0, &TextStyle::new(28.0).bold().color(TEXT));
    t.draw(c, "Amuro vs Killer", ix, panel.y + 104.0, &TextStyle::new(14.0).color(MUTED));
    let stats = [("BPM", "190"), ("NOTES", "1,880"), ("LEVEL", "12")];
    for (i, (k, v)) in stats.iter().enumerate() {
        let sx = ix + i as f32 * 92.0;
        t.draw(c, k, sx, panel.y + 150.0, &TextStyle::new(10.0).bold().tracking(1.5).color(MUTED2));
        t.draw(c, v, sx, panel.y + 178.0, &TextStyle::new(22.0).bold().color(TEXT));
    }
    c.fill_rect(Rect::new(panel.x + 24.0, panel.y + 248.0, panel.w - 48.0, 1.0), LINE);
    t.draw(c, "PERSONAL BEST", panel.x + 24.0, panel.y + 280.0, &TextStyle::new(10.0).bold().tracking(1.5).color(MUTED2));
    t.draw(c, "3,452", panel.x + 24.0, panel.y + 330.0, &TextStyle::new(44.0).bold().color(TEXT));
    t.draw(c, "AAA", panel.x + 190.0, panel.y + 330.0, &TextStyle::new(44.0).bold().color(GOLD));
    // clear lamp chips
    let mut cx = panel.x + 24.0;
    for (label, col, on) in [("EX-HARD", rgb(0xffd400), true), ("HARD", MAGENTA, true), ("FC", CYAN, false)] {
        let st = TextStyle::new(11.0).bold().tracking(1.0);
        let w = t.measure(c, label, &st) + 24.0;
        let chip = Rect::new(cx, panel.y + 352.0, w, 24.0);
        c.nine(&skin.panel_lg, chip, if on { col.with_alpha(36) } else { SURF2 });
        if on {
            c.nine(&skin.panel_outline, chip, col.with_alpha(160));
        }
        t.draw_in(c, label, chip, Align::Center, &st.color(if on { col } else { MUTED2 }));
        cx += w + 8.0;
    }

    // PLAY CTA
    let cta = Rect::new(panel.x + 24.0, panel.bottom() - 72.0, panel.w - 48.0, 52.0);
    c.set_additive(true);
    c.sprite_centered(skin.glow, cta.x + cta.w / 2.0, cta.y + cta.h / 2.0, cta.w * 1.1, 140.0, CYAN.with_alpha(60));
    c.set_additive(false);
    c.nine_hgradient(&skin.cut_panel, cta, CYAN, rgb(0x7fb4ff));
    let play = skin.icons.play;
    c.sprite(play, Rect::new(cta.x + 24.0, cta.y + 10.0, 32.0, 32.0), BG);
    t.draw_in(c, "PLAY", Rect::new(cta.x + 64.0, cta.y, 200.0, cta.h), Align::Left, &TextStyle::new(20.0).bold().tracking(4.0).color(BG));
    t.draw_in(c, "ENTER", Rect::new(cta.right() - 140.0, cta.y, 116.0, cta.h), Align::Right, &TextStyle::new(12.0).bold().tracking(2.0).color(rgba(0x05060a, 160)));

    // Footer hints
    let mut fx = 32.0;
    for (icon, label) in [(skin.icons.chevron_left, "SORT"), (skin.icons.chevron_right, "DIFFICULTY"), (skin.icons.star, "FAVORITE"), (skin.icons.check, "OPTIONS")] {
        c.sprite(icon, Rect::new(fx, 672.0, 20.0, 20.0), MUTED);
        let w = t.draw(c, label, fx + 26.0, 687.0, &TextStyle::new(11.0).bold().tracking(1.5).color(MUTED));
        fx += w + 26.0 + 28.0;
    }
}

fn render_gameplay(ui: &mut Ui) {
    backdrop(ui, rgb(0x2a5bff));
    let Ui { canvas: c, text: t, skin } = ui;

    // Playfield: scratch + 7 keys
    let lanes: [(f32, ColorRgba); 8] = [
        (68.0, MAGENTA),
        (44.0, TEXT),
        (36.0, rgb(0x4aa8ff)),
        (44.0, TEXT),
        (36.0, rgb(0x4aa8ff)),
        (44.0, TEXT),
        (36.0, rgb(0x4aa8ff)),
        (44.0, TEXT),
    ];
    let (px0, judge_y) = (96.0, 600.0);
    let pw: f32 = lanes.iter().map(|l| l.0).sum();
    let field = Rect::new(px0, 0.0, pw, judge_y + 40.0);
    c.fill_rect(field, rgba(0x020306, 235));
    c.fill_rect_vgradient(Rect::new(px0, 0.0, pw, 200.0), rgba(0x020306, 255), rgba(0x020306, 0));
    let mut x = px0;
    let mut lane_x = Vec::new();
    for (i, (w, _)) in lanes.iter().enumerate() {
        lane_x.push(x);
        if i > 0 {
            c.fill_rect(Rect::new(x, 0.0, 1.0, judge_y), rgba(0x232a3c, 160));
        }
        x += w;
    }
    c.fill_rect(Rect::new(px0 - 2.0, 0.0, 2.0, judge_y + 40.0), LINE);
    c.fill_rect(Rect::new(px0 + pw, 0.0, 2.0, judge_y + 40.0), LINE);

    // Key beams on pressed lanes
    c.set_additive(true);
    for &i in &[2usize, 5] {
        let (w, col) = lanes[i];
        c.sprite(skin.beam, Rect::new(lane_x[i], judge_y - 300.0, w, 300.0), col.with_alpha(150));
    }
    c.set_additive(false);

    // Notes
    let note_h = skin.note.region.h as f32;
    let notes = [(1, 120.0), (3, 180.0), (5, 260.0), (7, 260.0), (2, 330.0), (0, 380.0), (4, 430.0), (6, 470.0), (1, 520.0), (3, 560.0)];
    for (lane, y) in notes {
        let (w, col) = lanes[lane];
        let r = Rect::new(lane_x[lane] + 2.0, y, w - 4.0, note_h);
        c.nine(&skin.note, r, col);
        c.set_additive(true);
        c.nine(&skin.note_gloss, r, ColorRgba::new(255, 255, 255, 255));
        c.set_additive(false);
    }
    // Long note
    let (w, col) = lanes[5];
    let ln = Rect::new(lane_x[5] + 4.0, 140.0, w - 8.0, judge_y - 140.0);
    c.nine(&skin.ln_body, ln, col.with_alpha(110));
    c.nine(&skin.note, Rect::new(lane_x[5] + 2.0, 140.0, w - 4.0, note_h), col);

    // Judge line + glow
    c.fill_rect(Rect::new(px0, judge_y, pw, 3.0), MAGENTA);
    c.set_additive(true);
    c.sprite(skin.glow, Rect::new(px0 - 20.0, judge_y - 14.0, pw + 40.0, 31.0), MAGENTA.with_alpha(90));
    // Hit burst on lane 2 and 5
    for &i in &[2usize, 5] {
        let cx = lane_x[i] + lanes[i].0 / 2.0;
        c.sprite_centered(skin.ring, cx, judge_y + 1.0, 84.0, 84.0, GOLD.with_alpha(220));
        c.sprite_centered(skin.flare, cx, judge_y + 1.0, 120.0, 120.0, ColorRgba::new(255, 240, 200, 230));
        for k in 0..5 {
            let a = k as f32 * 1.25 + 0.4;
            let (sx, sy) = (cx + a.cos() * 40.0, judge_y - a.sin().abs() * 34.0);
            c.sprite_centered(skin.spark, sx, sy, 28.0, 6.0, GOLD.with_alpha(200));
        }
    }
    c.set_additive(false);
    // Key area
    c.fill_rect_vgradient(Rect::new(px0, judge_y + 4.0, pw, 36.0), SURF2, BG);

    // Judgement + combo
    let mid = px0 + pw / 2.0;
    let judge_st = TextStyle::new(36.0).bold().tracking(3.0);
    let jw = t.measure(c, "PGREAT", &judge_st);
    c.set_additive(true);
    c.sprite_centered(skin.glow, mid, 372.0, jw * 1.4, 90.0, CYAN.with_alpha(110));
    c.set_additive(false);
    t.draw(c, "PGREAT", mid - jw / 2.0, 386.0, &judge_st.color(rgb(0xbff8ff)));
    let combo_st = TextStyle::new(56.0).bold();
    let cw = t.measure(c, "1,284", &combo_st);
    t.draw(c, "1,284", mid - cw / 2.0, 300.0, &combo_st.color(TEXT));
    let lw = t.measure(c, "COMBO", &TextStyle::new(11.0).bold().tracking(3.0));
    t.draw(c, "COMBO", mid - lw / 2.0, 318.0, &TextStyle::new(11.0).bold().tracking(3.0).color(MUTED));

    // HUD panel
    let hud = Rect::new(px0 + pw + 48.0, 48.0, 420.0, 230.0);
    c.halo(&skin.shadow, hud, ColorRgba::new(255, 255, 255, 180));
    c.nine(&skin.cut_panel, hud, rgba(0x0b0e16, 240));
    c.nine(&skin.cut_outline, hud, LINE);
    t.draw(c, "EX SCORE", hud.x + 24.0, hud.y + 36.0, &TextStyle::new(10.0).bold().tracking(2.0).color(MUTED2));
    t.draw(c, "2,847", hud.x + 24.0, hud.y + 88.0, &TextStyle::new(48.0).bold().color(TEXT));
    t.draw(c, "+12", hud.x + 190.0, hud.y + 88.0, &TextStyle::new(18.0).bold().color(CYAN));
    t.draw_in(c, "AAA", Rect::new(hud.right() - 140.0, hud.y + 40.0, 116.0, 56.0), Align::Right, &TextStyle::new(40.0).bold().color(GOLD));
    // judge distribution bar
    let bar = Rect::new(hud.x + 24.0, hud.y + 116.0, hud.w - 48.0, 8.0);
    let parts = [(0.72, GOLD), (0.19, rgb(0xff9a3c)), (0.05, rgb(0x3ce07a)), (0.02, rgb(0x50b0ff)), (0.02, MAGENTA)];
    let mut bx = bar.x;
    for (f, col) in parts {
        let w = bar.w * f;
        c.fill_rect(Rect::new(bx, bar.y, w - 2.0, bar.h), col);
        bx += w;
    }
    let labels = [("PGREAT", "1,342", GOLD), ("GREAT", "356", rgb(0xff9a3c)), ("GOOD", "94", rgb(0x3ce07a)), ("BAD", "31", rgb(0x50b0ff)), ("POOR", "28", MAGENTA)];
    for (i, (k, v, col)) in labels.iter().enumerate() {
        let lx = hud.x + 24.0 + (i % 3) as f32 * 128.0;
        let ly = hud.y + 156.0 + (i / 3) as f32 * 30.0;
        c.sprite(skin.icons.dot, Rect::new(lx - 4.0, ly - 12.0, 16.0, 16.0), *col);
        t.draw(c, k, lx + 14.0, ly, &TextStyle::new(11.0).bold().tracking(1.0).color(MUTED));
        t.draw_in(c, v, Rect::new(lx + 14.0, ly - 14.0, 100.0, 16.0), Align::Right, &TextStyle::new(13.0).bold().color(TEXT));
    }

    // Groove gauge
    let gauge = Rect::new(hud.x, hud.bottom() + 24.0, hud.w, 22.0);
    c.nine(&skin.panel_sm, gauge, SURF2);
    let segs = 50;
    let filled = 41;
    let sw = (gauge.w - 8.0) / segs as f32;
    for i in 0..segs {
        let col = if i >= 40 { MAGENTA } else { CYAN };
        let a = if i < filled { 255 } else { 40 };
        c.fill_rect(Rect::new(gauge.x + 4.0 + i as f32 * sw, gauge.y + 4.0, sw - 2.0, gauge.h - 8.0), col.with_alpha(a));
    }
    t.draw(c, "82%", gauge.right() - 40.0, gauge.bottom() + 22.0, &TextStyle::new(14.0).bold().color(TEXT));
    t.draw(c, "GROOVE GAUGE", gauge.x, gauge.bottom() + 22.0, &TextStyle::new(10.0).bold().tracking(2.0).color(MUTED2));

    // Song info
    t.draw(c, "冥 -MEI-", hud.x, 600.0, &TextStyle::new(24.0).bold().color(TEXT));
    t.draw(c, "Amuro vs Killer  ·  ANOTHER 12", hud.x, 624.0, &TextStyle::new(13.0).color(MUTED));
    let prog = Rect::new(hud.x, 644.0, hud.w, 3.0);
    c.fill_rect(prog, LINE);
    c.fill_rect_hgradient(Rect::new(prog.x, prog.y, prog.w * 0.63, prog.h), CYAN, MAGENTA);
}

fn capture(ui: &mut Ui, gpu: &mut D3d11Backend, name: &str, draw: fn(&mut Ui)) -> (usize, Vec<u8>) {
    gpu.begin_frame(W, H, [0.0, 0.0, 0.0, 1.0]);
    ui.begin(W, H, 1.0);
    draw(ui);
    let calls = ui.end(gpu);
    let (w, h, px) = gpu.capture_frame().expect("readback");
    gpu.end_frame();
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    write_bmp(&target.join(format!("skin-{name}.bmp")), w, h, &px);
    (calls, px)
}

#[test]
fn generated_skin_showcase() {
    let window = HiddenWindow::with_size(W, H);
    let mut gpu = D3d11Backend::with_driver_types(window.0, W, H, &[D3D_DRIVER_TYPE_WARP])
        .expect("WARP device");
    let t0 = Instant::now();
    let mut ui = Ui::new(1.0);
    eprintln!("Ui::new (fonts + skin): {:?}", t0.elapsed());

    let (menu_calls, menu) = capture(&mut ui, &mut gpu, "menu", render_menu);
    let (play_calls, play) = capture(&mut ui, &mut gpu, "gameplay", render_gameplay);
    eprintln!("draw calls: menu {menu_calls}, gameplay {play_calls}");
    assert_eq!(menu_calls, 1);
    assert_eq!(play_calls, 1);
    for px in [&menu, &play] {
        let non_black = px.chunks_exact(4).filter(|p| p[0] as u32 + p[1] as u32 + p[2] as u32 > 60).count();
        assert!(non_black > 20_000, "composition rendered ({non_black} lit px)");
    }
}

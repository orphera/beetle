//! Key configuration screen on the `Ui` (Canvas + TextEngine + generated
//! Skin). Replaces `key_config.rs`.
//!
//! Lanes are drawn as a controller (IIDX layout: turntable + keys in two
//! staggered rows, odd keys low / even keys high) with the bound key on each
//! button, instead of a table. Double play shows the 1P and 2P controllers
//! side by side; lane order (and the selection index) is
//! `SkinConfig::active_lanes()`, i.e. left to right on screen.

use super::widgets::{self, FOOTER_H, PAD, TOPBAR_H};
use crate::art::Skin;
use crate::canvas::{Canvas, Rect};
use crate::view::Viewport;
use crate::skin::SkinConfig;
use crate::text::{Align, TextEngine, TextStyle};
use crate::theme::{self, caption};
use crate::ui::Ui;
use beetle_core::{Lane, PlayMode};

/// One configurable lane.
pub struct KeyBinding<'a> {
    pub lane: Lane,
    /// Long name, e.g. "KEY 3 (1P)".
    pub label: &'a str,
    /// Bound key, e.g. "LShift" ("None" when unbound).
    pub key: &'a str,
}

pub struct KeyConfigFrame<'a> {
    pub viewport: &'a Viewport,
    pub mode: PlayMode,
    /// Left-to-right lane order.
    pub lanes: &'a [KeyBinding<'a>],
    pub selected: usize,
    pub rebinding: bool,
    /// Current preset / layout name.
    pub layout: &'a str,
}

const HINTS: [(&str, &str); 5] = [
    (widgets::LEFT_RIGHT, "SELECT"),
    ("ENTER", "REBIND"),
    ("F1", "LAYOUT"),
    ("DEL", "RESET"),
    ("ESC", "BACK"),
];
const REBIND_HINTS: [(&str, &str); 2] = [("ANY KEY", "BIND"), ("ESC", "CANCEL")];

pub fn draw_key_config(ui: &mut Ui, f: &KeyConfigFrame) {
    let sk = ui.skin;
    let lite = ui.lite;
    let (c, t) = (&mut ui.canvas, &mut ui.text);
    let vp = f.viewport;
    let s = vp.scale;

    widgets::backdrop(c, &sk, vp, theme::BLUE, lite);
    let content = Rect::from_ltrb(
        vp.x + PAD * s,
        vp.y + (TOPBAR_H + 24.0) * s,
        vp.x + vp.width - PAD * s,
        vp.y + vp.height - (FOOTER_H + 24.0) * s,
    );

    // Mode heading
    let mode_name = format!("{} KEYS", theme::mode_label(f.mode).trim_end_matches('K'));
    let head = TextStyle::new(28.0 * s).bold().tracking(4.0 * s).color(theme::TEXT);
    t.draw_in(c, &mode_name, Rect::new(content.x, content.y, content.w, 36.0 * s), Align::Center, &head);
    let sub = "Lanes of the selected chart's key mode";
    t.draw_in(c, sub, Rect::new(content.x, content.y + 36.0 * s, content.w, 20.0 * s), Align::Center, &TextStyle::new(13.0 * s).color(theme::MUTED));

    let card = Rect::new(content.x + (content.w - 640.0 * s) / 2.0, content.bottom() - 136.0 * s, 640.0 * s, 136.0 * s);
    controllers(c, t, &sk, f, Rect::from_ltrb(content.x, content.y + 72.0 * s, content.right(), card.y - 16.0 * s), s);
    detail_card(c, t, &sk, f, card, s);

    // Top bar: layout chip on the right
    widgets::top_bar(c, t, vp, "KEY CONFIG", s);
    let st = TextStyle::new(13.0 * s).bold().color(theme::TEXT);
    let cap = caption(10.0, s);
    let right = vp.x + vp.width - PAD * s;
    let name_w = t.measure(c, f.layout, &st).min(460.0 * s);
    let name = t.fit(c, f.layout, name_w, &st).into_owned();
    t.draw(c, &name, right - name_w, vp.y + 37.0 * s, &st);
    let cw = t.measure(c, "LAYOUT", &cap);
    t.draw(c, "LAYOUT", right - name_w - 12.0 * s - cw, vp.y + 37.0 * s, &cap);

    let bar = widgets::footer_bar(c, vp, s);
    if f.rebinding {
        widgets::footer_hints(c, t, &sk, &REBIND_HINTS, bar, s);
    } else {
        widgets::footer_hints(c, t, &sk, &HINTS, bar, s);
    }
}

/// Button geometry in 1280×720 units.
const KEY_W: f32 = 62.0;
const KEY_STEP: f32 = 70.0;
const KEY_H: f32 = 104.0;
const ROW_OFFSET: f32 = 64.0;
const TABLE: f32 = 160.0;
const TABLE_GAP: f32 = 28.0;
const SIDE_GAP: f32 = 64.0;

/// Even-numbered keys sit in the upper row (the black keys of the cabinet).
fn upper_row(lane: Lane) -> bool {
    matches!(lane, Lane::Key2 | Lane::Key4 | Lane::Key6 | Lane::Key8 | Lane::P2Key2 | Lane::P2Key4 | Lane::P2Key6)
}

fn is_scratch(lane: Lane) -> bool {
    matches!(lane, Lane::Scratch | Lane::P2Scratch)
}

fn is_2p(lane: Lane) -> bool {
    matches!(
        lane,
        Lane::P2Scratch | Lane::P2Key1 | Lane::P2Key2 | Lane::P2Key3 | Lane::P2Key4 | Lane::P2Key5 | Lane::P2Key6 | Lane::P2Key7
    )
}

/// Width of one side (scratch + keys) in layout units.
fn side_width(lanes: &[&KeyBinding]) -> f32 {
    let keys = lanes.iter().filter(|b| !is_scratch(b.lane)).count() as f32;
    let scratch = if lanes.iter().any(|b| is_scratch(b.lane)) { TABLE + TABLE_GAP } else { 0.0 };
    scratch + (keys - 1.0).max(0.0) * KEY_STEP + KEY_W
}

fn controllers(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, f: &KeyConfigFrame, area: Rect, s: f32) {
    let mut layout = SkinConfig::default();
    layout.set_play_mode(f.mode);
    let sides: Vec<Vec<(usize, &KeyBinding)>> = {
        let p1: Vec<_> = f.lanes.iter().enumerate().filter(|(_, b)| !is_2p(b.lane)).collect();
        let p2: Vec<_> = f.lanes.iter().enumerate().filter(|(_, b)| is_2p(b.lane)).collect();
        [p1, p2].into_iter().filter(|v| !v.is_empty()).collect()
    };
    let widths: Vec<f32> = sides
        .iter()
        .map(|side| side_width(&side.iter().map(|(_, b)| *b).collect::<Vec<_>>()))
        .collect();
    let total = widths.iter().sum::<f32>() + SIDE_GAP * (sides.len() as f32 - 1.0).max(0.0);
    // Shrink to fit (14K is the widest).
    let k = (area.w / (total * s)).min(1.0) * s;
    let mut x = area.x + (area.w - total * k) / 2.0;
    let top = area.y + (area.h - (KEY_H + ROW_OFFSET) * k) / 2.0;

    for (side, w) in sides.iter().zip(&widths) {
        // Controller body
        let body = Rect::new(x - 20.0 * k, top - 24.0 * k, (w + 40.0) * k, (KEY_H + ROW_OFFSET + 48.0) * k);
        c.halo(&sk.shadow, body, theme::WHITE.with_alpha(150));
        c.nine(&sk.panel_lg, body, theme::SURF1.with_alpha(235));
        c.nine(&sk.panel_outline, body, theme::LINE);
        if sides.len() > 1 {
            let label = if is_2p(side[0].1.lane) { "2P" } else { "1P" };
            t.draw(c, label, body.x + 12.0 * k, body.y - 8.0 * k, &caption(11.0, s).color(theme::MUTED));
        }

        let mut kx = x;
        for &(idx, b) in side {
            let col = layout.lane_color(b.lane);
            let on = idx == f.selected;
            if is_scratch(b.lane) {
                let r = Rect::new(kx, top + (KEY_H + ROW_OFFSET - TABLE) * k / 2.0, TABLE * k, TABLE * k);
                button(c, t, sk, b, r, col, on, f.rebinding, true, s);
                kx += (TABLE + TABLE_GAP) * k;
            } else {
                let y = if upper_row(b.lane) { top } else { top + ROW_OFFSET * k };
                button(c, t, sk, b, Rect::new(kx, y, KEY_W * k, KEY_H * k), col, on, f.rebinding, false, s);
                kx += KEY_STEP * k;
            }
        }
        x += (w + SIDE_GAP) * k;
    }
}

#[allow(clippy::too_many_arguments)]
fn button(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    b: &KeyBinding,
    r: Rect,
    col: crate::skin::ColorRgba,
    on: bool,
    rebinding: bool,
    scratch: bool,
    s: f32,
) {
    let unbound = b.key == "None";
    let accent = if on && rebinding { theme::MAGENTA } else { theme::CYAN };
    if on {
        c.set_additive(true);
        c.sprite_centered(sk.glow, r.x + r.w / 2.0, r.y + r.h / 2.0, r.w * 2.4, r.h * 1.8, accent.with_alpha(90));
        c.set_additive(false);
    }
    c.nine(if scratch { &sk.panel_lg } else { &sk.panel }, r, if on { theme::SURF3 } else { theme::SURF2 });
    if scratch {
        // Turntable: lane-colored ring
        let d = r.w * 0.78;
        c.set_additive(true);
        c.sprite_centered(sk.ring, r.x + r.w / 2.0, r.y + r.h / 2.0, d, d, col.with_alpha(if on { 230 } else { 120 }));
        c.set_additive(false);
    } else {
        // Key cap: lane color fill fading down, color bar at the bottom
        let face = r.inset(4.0 * s);
        c.push_clip(face);
        c.fill_rect_vgradient(face, col.with_alpha(if on { 70 } else { 34 }), col.with_alpha(6));
        c.pop_clip();
        c.nine(&sk.panel_sm, Rect::new(r.x + 6.0 * s, r.bottom() - 8.0 * s, r.w - 12.0 * s, 3.0 * s), col);
    }
    c.nine(&sk.panel_outline, r, if on { accent } else { theme::LINE });

    let (txt, st) = if on && rebinding {
        ("?", TextStyle::new(22.0 * s).bold().color(theme::MAGENTA))
    } else if unbound {
        ("—", TextStyle::new(14.0 * s).bold().color(theme::MUTED2))
    } else {
        (b.key, TextStyle::new(if b.key.chars().count() > 2 { 13.0 } else { 20.0 } * s).bold().color(theme::TEXT))
    };
    let label_r = if scratch { r } else { Rect::new(r.x, r.y, r.w, r.h - 10.0 * s) };
    t.draw_in(c, txt, label_r.inset(3.0 * s), Align::Center, &st);
}

fn detail_card(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, f: &KeyConfigFrame, r: Rect, s: f32) {
    c.halo(&sk.shadow, r, theme::WHITE.with_alpha(160));
    c.nine(&sk.panel_lg, r, theme::SURF1.with_alpha(235));
    if f.rebinding {
        c.nine(&sk.panel_outline, r, theme::MAGENTA.with_alpha(200));
    }
    let inner = r.inset(24.0 * s);
    let Some(b) = f.lanes.get(f.selected) else { return };

    t.draw(c, if f.rebinding { "REBINDING" } else { "SELECTED LANE" }, inner.x, inner.y + 14.0 * s, &caption(10.0, s).color(if f.rebinding { theme::MAGENTA } else { theme::MUTED2 }));
    t.draw(c, b.label, inner.x, inner.y + 48.0 * s, &TextStyle::new(24.0 * s).bold().color(theme::TEXT));
    let msg = if f.rebinding {
        "Press the key for this lane. If another lane uses it, that lane is cleared."
    } else {
        "Enter to assign a new key. F1 cycles the preset layouts."
    };
    let st = TextStyle::new(13.0 * s).color(theme::MUTED);
    let msg = t.fit(c, msg, inner.w, &st).into_owned();
    t.draw(c, &msg, inner.x, inner.bottom() - 4.0 * s, &st);

    // Current key as a large keycap on the right
    let key = if f.rebinding { "?" } else if b.key == "None" { "—" } else { b.key };
    let st = TextStyle::new(22.0 * s).bold().color(if f.rebinding { theme::MAGENTA } else { theme::TEXT });
    let w = (t.measure(c, key, &st) + 40.0 * s).max(72.0 * s);
    let cap = Rect::new(inner.right() - w, inner.y, w, 52.0 * s);
    c.nine(&sk.panel, cap, theme::SURF3);
    c.nine(&sk.panel_sm, Rect::new(cap.x + 4.0 * s, cap.bottom() - 4.0 * s, cap.w - 8.0 * s, 3.0 * s), theme::LINE);
    t.draw_in(c, key, Rect::new(cap.x, cap.y, cap.w, cap.h - 4.0 * s), Align::Center, &st);
    t.draw_in(c, "KEY", Rect::new(cap.x - 60.0 * s, cap.y, 48.0 * s, cap.h), Align::Right, &caption(10.0, s));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_is_one_batch() {
        let vp = Viewport::new(1280, 720);
        let mut ui = Ui::new(vp.scale);
        for mode in [PlayMode::Keys5, PlayMode::Keys7, PlayMode::Keys9, PlayMode::Keys10, PlayMode::Keys14] {
            let mut layout = SkinConfig::default();
            layout.set_play_mode(mode);
            let lanes: Vec<KeyBinding> = layout.active_lanes().iter().map(|&lane| KeyBinding { lane, label: "KEY", key: "S" }).collect();
            for rebinding in [false, true] {
                ui.begin(1280, 720, vp.scale);
                draw_key_config(&mut ui, &KeyConfigFrame { viewport: &vp, mode, lanes: &lanes, selected: 1, rebinding, layout: "HomeRow" });
                assert_eq!(ui.canvas.debug_batches().len(), 1, "{mode:?}");
            }
        }
    }

    #[test]
    fn sides_fit_the_screen() {
        // 14K at full size would be wider than the content area; it must be
        // scaled down rather than overflow.
        let lanes: Vec<KeyBinding> = (0..8).map(|i| KeyBinding { lane: if i == 0 { Lane::Scratch } else { Lane::Key1 }, label: "", key: "" }).collect();
        let w = side_width(&lanes.iter().collect::<Vec<_>>());
        assert!((w - (TABLE + TABLE_GAP + 6.0 * KEY_STEP + KEY_W)).abs() < 1e-3);
    }
}

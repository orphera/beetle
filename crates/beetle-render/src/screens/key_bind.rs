//! Key configuration screen on the `Ui` (Canvas + TextEngine + generated
//! Skin). Replaces `key_config.rs`.
//!
//! Lanes are drawn as a controller (IIDX layout: turntable + keys in two
//! staggered rows, odd keys low / even keys high) with the bound key on each
//! button, instead of a table. Double play shows the 1P and 2P controllers
//! side by side; lane order (and the selection index) is
//! `SkinConfig::screen_lanes()`, i.e. left to right on screen. Tabs on top
//! switch between the key modes, which each keep their own layout; a lane
//! can have several keys.

use super::widgets::{self, Hint, FOOTER_H, PAD, TOPBAR_H};
use crate::art::Skin;
use crate::canvas::{Canvas, Rect};
use crate::hit::{HitId, HitSink};
use crate::skin::{scratch_side_applies, EightKForm, ScratchSide, SkinConfig};
use crate::strings;
use crate::text::{Align, TextEngine, TextStyle};
use crate::theme::{self, caption};
use crate::ui::Ui;
use crate::view::Viewport;
use beetle_core::{Lane, PlayMode};

/// One configurable lane.
pub struct KeyBinding<'a> {
    pub lane: Lane,
    /// Long name, e.g. "KEY 3 (1P)".
    pub label: &'a str,
    /// Bound keys, e.g. ["LShift", "LCtrl"] (empty when unbound).
    pub keys: &'a [&'a str],
}

/// What the next key press does while waiting for one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rebind {
    /// Becomes the lane's only key.
    Replace,
    /// Is added to the lane's keys.
    Add,
}

/// Key modes in tab order.
pub const KEY_MODES: [PlayMode; 8] = [
    PlayMode::Keys4,
    PlayMode::Keys5,
    PlayMode::Keys6,
    PlayMode::Keys7,
    PlayMode::Keys8,
    PlayMode::Keys9,
    PlayMode::Keys10,
    PlayMode::Keys14,
];

pub struct KeyConfigFrame<'a> {
    pub viewport: &'a Viewport,
    pub mode: PlayMode,
    /// Left-to-right lane order.
    pub lanes: &'a [KeyBinding<'a>],
    pub selected: usize,
    /// `Some` while waiting for a key press.
    pub rebinding: Option<Rebind>,
    /// Current key preset name.
    pub layout: &'a str,
    /// Which edge the mode's scratch lane is on (5K / 7K / 8K).
    pub scratch: ScratchSide,
    /// How 8K is arranged.
    pub form: EightKForm,
}

/// Footer hints; F2 (scratch side) and F3 (8K form) only where they apply.
fn hints_for(mode: PlayMode) -> Vec<Hint> {
    let mut hints: Vec<Hint> = vec![
        (widgets::LEFT_RIGHT, strings::KEY_LANE, None),
        ("↑↓", strings::MODE, None),
        ("ENTER", strings::SET_KEY, Some(HitId::KeySet)),
        ("A", strings::ADD_KEY, Some(HitId::KeyAdd)),
        ("BKSP", strings::CLEAR_LANE, Some(HitId::KeyClear)),
        ("F1", strings::PRESET, Some(HitId::KeyPreset)),
    ];
    if scratch_side_applies(mode) {
        hints.push(("F2", strings::SCRATCH, Some(HitId::KeyScratch)));
    }
    if mode == PlayMode::Keys8 {
        hints.push(("F3", strings::FORM_8K, Some(HitId::KeyForm)));
    }
    hints.push(("DEL", strings::RESET, Some(HitId::KeyReset)));
    hints.push(("ESC", strings::BACK, Some(HitId::KeyBack)));
    hints
}
const REBIND_HINTS: [Hint; 2] = [
    (strings::ANY_KEY, strings::BIND, None),
    ("ESC", strings::CANCEL, None),
];

pub fn draw_key_config(ui: &mut Ui, f: &KeyConfigFrame) {
    let sk = ui.skin;
    let lite = ui.lite;
    let mut hs = HitSink::new(&mut ui.hits, ui.pointer);
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

    mode_tabs(
        c,
        t,
        &sk,
        f.mode,
        Rect::new(content.x, content.y, content.w, 36.0 * s),
        s,
        &mut hs,
    );
    let sub = layout_summary(f);
    t.draw_in(
        c,
        &sub,
        Rect::new(content.x, content.y + 40.0 * s, content.w, 20.0 * s),
        Align::Center,
        &TextStyle::new(13.0 * s).color(theme::MUTED),
    );

    let card = Rect::new(
        content.x + (content.w - 640.0 * s) / 2.0,
        content.bottom() - 136.0 * s,
        640.0 * s,
        136.0 * s,
    );
    controllers(
        c,
        t,
        &sk,
        f,
        Rect::from_ltrb(
            content.x,
            content.y + 72.0 * s,
            content.right(),
            card.y - 16.0 * s,
        ),
        s,
        &mut hs,
    );
    detail_card(c, t, &sk, f, card, s);

    // Top bar: layout chip on the right
    widgets::top_bar(c, t, vp, strings::KEY_CONFIG, s);
    let st = TextStyle::new(13.0 * s).bold().color(theme::TEXT);
    let cap = caption(12.0, s);
    let right = vp.x + vp.width - PAD * s;
    let name_w = t.measure(c, f.layout, &st).min(460.0 * s);
    let name = t.fit(c, f.layout, name_w, &st).into_owned();
    t.draw(c, &name, right - name_w, vp.y + 37.0 * s, &st);
    let cw = t.measure(c, strings::PRESET, &cap);
    t.draw(
        c,
        strings::PRESET,
        right - name_w - 12.0 * s - cw,
        vp.y + 37.0 * s,
        &cap,
    );

    let bar = widgets::footer_bar(c, vp, s);
    if f.rebinding.is_some() {
        widgets::footer_buttons(c, t, &sk, &REBIND_HINTS, bar, s, &mut hs);
    } else {
        widgets::footer_buttons(c, t, &sk, &hints_for(f.mode), bar, s, &mut hs);
    }
}

/// The line under the tabs: what is special about the mode's layout.
fn layout_summary(f: &KeyConfigFrame) -> String {
    let side = match f.scratch {
        ScratchSide::Left => strings::SIDE_LEFT,
        ScratchSide::Right => strings::SIDE_RIGHT,
    };
    match f.mode {
        PlayMode::Keys8 if f.form == EightKForm::Triggers => strings::LAYOUT_TRIGGERS.to_string(),
        PlayMode::Keys8 => strings::fill(strings::LAYOUT_8K, &[side]),
        PlayMode::Keys4 | PlayMode::Keys6 => strings::LAYOUT_STRAIGHT.to_string(),
        mode if scratch_side_applies(mode) => strings::fill(strings::LAYOUT_SCRATCH, &[side]),
        _ => strings::LAYOUT_PER_MODE.to_string(),
    }
}

/// Tabs for the key modes, the current one highlighted.
fn mode_tabs(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    mode: PlayMode,
    area: Rect,
    s: f32,
    hs: &mut HitSink,
) {
    let (tab_w, gap) = (84.0 * s, 8.0 * s);
    let total = KEY_MODES.len() as f32 * (tab_w + gap) - gap;
    let mut x = area.x + (area.w - total) / 2.0;
    for (i, m) in KEY_MODES.into_iter().enumerate() {
        let on = m == mode;
        let r = Rect::new(x, area.y, tab_w, area.h);
        hs.add(r, HitId::KeyModeTab(i));
        let hot = !on && hs.hovered(r);
        if on {
            c.set_additive(true);
            c.sprite_centered(
                sk.glow,
                r.x + r.w / 2.0,
                r.y + r.h / 2.0,
                r.w * 1.8,
                r.h * 2.4,
                theme::CYAN.with_alpha(70),
            );
            c.set_additive(false);
            c.nine(&sk.panel_lg, r, theme::CYAN);
        } else {
            c.nine(
                &sk.panel_lg,
                r,
                if hot { theme::SURF3 } else { theme::SURF2 },
            );
            c.nine(&sk.panel_outline, r, theme::LINE);
        }
        let st = TextStyle::new(13.0 * s)
            .bold()
            .tracking(1.5 * s)
            .color(if on { theme::ON_ACCENT } else { theme::MUTED });
        t.draw_in(c, theme::mode_label(m), r, Align::Center, &st);
        x += tab_w + gap;
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
    matches!(
        lane,
        Lane::Key2
            | Lane::Key4
            | Lane::Key6
            | Lane::Key8
            | Lane::P2Key2
            | Lane::P2Key4
            | Lane::P2Key6
    )
}

fn is_scratch(lane: Lane) -> bool {
    matches!(lane, Lane::Scratch | Lane::P2Scratch)
}

fn is_2p(lane: Lane) -> bool {
    matches!(
        lane,
        Lane::P2Scratch
            | Lane::P2Key1
            | Lane::P2Key2
            | Lane::P2Key3
            | Lane::P2Key4
            | Lane::P2Key5
            | Lane::P2Key6
            | Lane::P2Key7
    )
}

/// 8K in its trigger form: the outer two lanes are L / R triggers.
const TRIGGER_W: f32 = 84.0;
const TRIGGER_GAP: f32 = 20.0;

fn is_trigger(lane: Lane) -> bool {
    matches!(lane, Lane::Scratch | Lane::Key7)
}

/// Width of one side (scratch + keys) in layout units.
fn side_width(lanes: &[&KeyBinding], triggers: bool, straight: bool) -> f32 {
    if straight {
        return (lanes.len() as f32 - 1.0).max(0.0) * KEY_STEP + KEY_W;
    }
    if triggers {
        let keys = lanes.iter().filter(|b| !is_trigger(b.lane)).count() as f32;
        return 2.0 * (TRIGGER_W + TRIGGER_GAP) + (keys - 1.0).max(0.0) * KEY_STEP + KEY_W;
    }
    let keys = lanes.iter().filter(|b| !is_scratch(b.lane)).count() as f32;
    let scratch = if lanes.iter().any(|b| is_scratch(b.lane)) {
        TABLE + TABLE_GAP
    } else {
        0.0
    };
    scratch + (keys - 1.0).max(0.0) * KEY_STEP + KEY_W
}

fn controllers(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    f: &KeyConfigFrame,
    area: Rect,
    s: f32,
    hs: &mut HitSink,
) {
    let mut layout = SkinConfig::default();
    layout.set_play_mode(f.mode);
    layout.set_scratch_side(f.mode, f.scratch);
    layout.set_eight_k_form(f.form);
    let triggers = f.mode == PlayMode::Keys8 && f.form == EightKForm::Triggers;
    // Straight 4K/6K (no scratch lane) and 8K without the trigger form: every
    // lane is the same square button, all in one row.
    let straight = matches!(f.mode, PlayMode::Keys4 | PlayMode::Keys6)
        || (f.mode == PlayMode::Keys8 && !triggers);
    let sides: Vec<Vec<(usize, &KeyBinding)>> = {
        let p1: Vec<_> = f
            .lanes
            .iter()
            .enumerate()
            .filter(|(_, b)| !is_2p(b.lane))
            .collect();
        let p2: Vec<_> = f
            .lanes
            .iter()
            .enumerate()
            .filter(|(_, b)| is_2p(b.lane))
            .collect();
        [p1, p2].into_iter().filter(|v| !v.is_empty()).collect()
    };
    let widths: Vec<f32> = sides
        .iter()
        .map(|side| {
            side_width(
                &side.iter().map(|(_, b)| *b).collect::<Vec<_>>(),
                triggers,
                straight,
            )
        })
        .collect();
    let total = widths.iter().sum::<f32>() + SIDE_GAP * (sides.len() as f32 - 1.0).max(0.0);
    // Shrink to fit (14K is the widest).
    let k = (area.w / (total * s)).min(1.0) * s;
    let mut x = area.x + (area.w - total * k) / 2.0;
    let top = area.y + (area.h - (KEY_H + ROW_OFFSET) * k) / 2.0;

    for (side, w) in sides.iter().zip(&widths) {
        // Controller body
        let body = Rect::new(
            x - 20.0 * k,
            top - 24.0 * k,
            (w + 40.0) * k,
            (KEY_H + ROW_OFFSET + 48.0) * k,
        );
        c.halo(&sk.shadow, body, theme::WHITE.with_alpha(150));
        c.nine(&sk.panel_lg, body, theme::SURF1.with_alpha(235));
        c.nine(&sk.panel_outline, body, theme::LINE);
        if sides.len() > 1 {
            let label = if is_2p(side[0].1.lane) { "2P" } else { "1P" };
            t.draw(
                c,
                label,
                body.x + 12.0 * k,
                body.y - 8.0 * k,
                &caption(12.0, s).color(theme::MUTED),
            );
        }

        let mut kx = x;
        for &(idx, b) in side {
            let col = layout.lane_color(b.lane);
            let on = idx == f.selected;
            if triggers {
                // Triggers stand full height at the edges, the keys sit in one row between.
                let full = KEY_H + ROW_OFFSET;
                if is_trigger(b.lane) {
                    button(
                        c,
                        t,
                        sk,
                        b,
                        Rect::new(kx, top, TRIGGER_W * k, full * k),
                        col,
                        on,
                        f.rebinding.is_some(),
                        false,
                        s,
                        idx,
                        hs,
                    );
                    kx += (TRIGGER_W + TRIGGER_GAP) * k;
                } else {
                    button(
                        c,
                        t,
                        sk,
                        b,
                        Rect::new(kx, top + ROW_OFFSET * k / 2.0, KEY_W * k, KEY_H * k),
                        col,
                        on,
                        f.rebinding.is_some(),
                        false,
                        s,
                        idx,
                        hs,
                    );
                    kx += KEY_STEP * k;
                }
            } else if straight {
                button(
                    c,
                    t,
                    sk,
                    b,
                    Rect::new(kx, top + ROW_OFFSET * k / 2.0, KEY_W * k, KEY_H * k),
                    col,
                    on,
                    f.rebinding.is_some(),
                    false,
                    s,
                    idx,
                    hs,
                );
                kx += KEY_STEP * k;
            } else if is_scratch(b.lane) {
                let r = Rect::new(
                    kx,
                    top + (KEY_H + ROW_OFFSET - TABLE) * k / 2.0,
                    TABLE * k,
                    TABLE * k,
                );
                button(
                    c,
                    t,
                    sk,
                    b,
                    r,
                    col,
                    on,
                    f.rebinding.is_some(),
                    true,
                    s,
                    idx,
                    hs,
                );
                kx += (TABLE + TABLE_GAP) * k;
            } else {
                let y = if upper_row(b.lane) {
                    top
                } else {
                    top + ROW_OFFSET * k
                };
                button(
                    c,
                    t,
                    sk,
                    b,
                    Rect::new(kx, y, KEY_W * k, KEY_H * k),
                    col,
                    on,
                    f.rebinding.is_some(),
                    false,
                    s,
                    idx,
                    hs,
                );
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
    idx: usize,
    hs: &mut HitSink,
) {
    hs.add(r, HitId::KeyLane(idx));
    let accent = if on && rebinding {
        theme::MAGENTA
    } else {
        theme::CYAN
    };
    if on {
        c.set_additive(true);
        c.sprite_centered(
            sk.glow,
            r.x + r.w / 2.0,
            r.y + r.h / 2.0,
            r.w * 2.4,
            r.h * 1.8,
            accent.with_alpha(90),
        );
        c.set_additive(false);
    }
    c.nine(
        if scratch { &sk.panel_lg } else { &sk.panel },
        r,
        if on { theme::SURF3 } else { theme::SURF2 },
    );
    if !on && hs.hovered(r) {
        c.nine(&sk.panel_outline, r, theme::CYAN.with_alpha(120));
    }
    if scratch {
        // Turntable: lane-colored ring
        let d = r.w * 0.78;
        c.set_additive(true);
        c.sprite_centered(
            sk.ring,
            r.x + r.w / 2.0,
            r.y + r.h / 2.0,
            d,
            d,
            col.with_alpha(if on { 230 } else { 120 }),
        );
        c.set_additive(false);
    } else {
        // Key cap: lane color fill fading down, color bar at the bottom
        let face = r.inset(4.0 * s);
        c.push_clip(face);
        c.fill_rect_vgradient(
            face,
            col.with_alpha(if on { 70 } else { 34 }),
            col.with_alpha(6),
        );
        c.pop_clip();
        c.nine(
            &sk.panel_sm,
            Rect::new(r.x + 6.0 * s, r.bottom() - 8.0 * s, r.w - 12.0 * s, 3.0 * s),
            col,
        );
    }
    c.nine(&sk.panel_outline, r, if on { accent } else { theme::LINE });

    let label_r = if scratch {
        r
    } else {
        Rect::new(r.x, r.y, r.w, r.h - 10.0 * s)
    };
    let label_r = label_r.inset(3.0 * s);
    if on && rebinding {
        t.draw_in(
            c,
            "?",
            label_r,
            Align::Center,
            &TextStyle::new(22.0 * s).bold().color(theme::MAGENTA),
        );
    } else if b.keys.is_empty() {
        t.draw_in(
            c,
            "—",
            label_r,
            Align::Center,
            &TextStyle::new(14.0 * s).bold().color(theme::MUTED2),
        );
    } else if b.keys.len() == 1 {
        let key = b.keys[0];
        let size = if key.chars().count() > 2 { 13.0 } else { 20.0 };
        t.draw_in(
            c,
            key,
            label_r,
            Align::Center,
            &TextStyle::new(size * s).bold().color(theme::TEXT),
        );
    } else {
        // Several keys: stacked small, "+N" past three.
        let shown = b.keys.len().min(3);
        let line = 16.0 * s;
        let y0 = label_r.y + (label_r.h - line * shown as f32) / 2.0;
        let st = TextStyle::new(11.0 * s).bold().color(theme::TEXT);
        for (i, key) in b.keys.iter().take(shown).enumerate() {
            let txt = if i == 2 && b.keys.len() > 3 {
                format!("+{}", b.keys.len() - 2)
            } else {
                key.to_string()
            };
            t.draw_in(
                c,
                &txt,
                Rect::new(label_r.x, y0 + i as f32 * line, label_r.w, line),
                Align::Center,
                &st,
            );
        }
    }
}

fn detail_card(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, f: &KeyConfigFrame, r: Rect, s: f32) {
    c.halo(&sk.shadow, r, theme::WHITE.with_alpha(160));
    c.nine(&sk.panel_lg, r, theme::SURF1.with_alpha(235));
    if f.rebinding.is_some() {
        c.nine(&sk.panel_outline, r, theme::MAGENTA.with_alpha(200));
    }
    let inner = r.inset(24.0 * s);
    let Some(b) = f.lanes.get(f.selected) else {
        return;
    };

    let (title, msg) = match f.rebinding {
        Some(Rebind::Replace) => (strings::SET_KEY, strings::REBIND_REPLACE),
        Some(Rebind::Add) => (strings::ADD_KEY, strings::REBIND_ADD),
        None => (strings::SELECTED_LANE, strings::REBIND_SELECTED),
    };
    let accent = if f.rebinding.is_some() {
        theme::MAGENTA
    } else {
        theme::MUTED2
    };
    t.draw(
        c,
        title,
        inner.x,
        inner.y + 14.0 * s,
        &caption(12.0, s).color(accent),
    );
    let label_st = TextStyle::new(24.0 * s).bold().color(theme::TEXT);
    let label_w = t.draw(c, b.label, inner.x, inner.y + 48.0 * s, &label_st);
    let st = TextStyle::new(13.0 * s).color(theme::MUTED);
    let msg = t.fit(c, msg, inner.w, &st).into_owned();
    t.draw(c, &msg, inner.x, inner.bottom() - 4.0 * s, &st);

    // The lane's keys as keycaps, right-aligned ("?" for the one being added).
    let mut caps: Vec<&str> = match f.rebinding {
        Some(Rebind::Replace) => vec![],
        _ => b.keys.to_vec(),
    };
    if f.rebinding.is_some() {
        caps.push("?");
    } else if caps.is_empty() {
        caps.push("—");
    }
    let st = TextStyle::new(18.0 * s).bold();
    let widths: Vec<f32> = caps
        .iter()
        .map(|k| (t.measure(c, k, &st) + 28.0 * s).max(52.0 * s))
        .collect();
    let gap = 8.0 * s;
    let mut x = inner.right() - (widths.iter().sum::<f32>() + gap * (caps.len() as f32 - 1.0));
    let min_x = inner.x + label_w + 80.0 * s;
    let first = caps.len() - caps.len().min(5);
    t.draw_in(
        c,
        strings::KEYS_CAPTION,
        Rect::new(x.max(min_x) - 60.0 * s, inner.y, 48.0 * s, 44.0 * s),
        Align::Right,
        &caption(12.0, s),
    );
    for (key, w) in caps.iter().zip(&widths).skip(first) {
        if x < min_x {
            x += w + gap;
            continue;
        }
        let cap = Rect::new(x, inner.y, *w, 44.0 * s);
        let pending = *key == "?";
        c.nine(&sk.panel, cap, theme::SURF3);
        if pending {
            c.nine(&sk.panel_outline, cap, theme::MAGENTA.with_alpha(200));
        }
        c.nine(
            &sk.panel_sm,
            Rect::new(
                cap.x + 4.0 * s,
                cap.bottom() - 4.0 * s,
                cap.w - 8.0 * s,
                3.0 * s,
            ),
            theme::LINE,
        );
        let col = if pending {
            theme::MAGENTA
        } else if *key == "—" {
            theme::MUTED2
        } else {
            theme::TEXT
        };
        t.draw_in(
            c,
            key,
            Rect::new(cap.x, cap.y, cap.w, cap.h - 4.0 * s),
            Align::Center,
            &st.color(col),
        );
        x += w + gap;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_is_one_batch() {
        let vp = Viewport::new(1280, 720);
        let mut ui = Ui::new(vp.scale);
        let variants = [
            (PlayMode::Keys5, ScratchSide::Left, EightKForm::Inline),
            (PlayMode::Keys7, ScratchSide::Right, EightKForm::Inline),
            (PlayMode::Keys8, ScratchSide::Left, EightKForm::Inline),
            (PlayMode::Keys8, ScratchSide::Right, EightKForm::Inline),
            (PlayMode::Keys8, ScratchSide::Left, EightKForm::Triggers),
            (PlayMode::Keys9, ScratchSide::Left, EightKForm::Inline),
            (PlayMode::Keys10, ScratchSide::Left, EightKForm::Inline),
            (PlayMode::Keys14, ScratchSide::Left, EightKForm::Inline),
        ];
        for (mode, scratch, form) in variants {
            let mut layout = SkinConfig::default();
            layout.set_play_mode(mode);
            layout.set_scratch_side(mode, scratch);
            layout.set_eight_k_form(form);
            let keys = ["S", "LCtrl", "Q", "W"];
            let lanes: Vec<KeyBinding> = layout
                .screen_lanes()
                .into_iter()
                .enumerate()
                .map(|(i, lane)| KeyBinding {
                    lane,
                    label: "KEY",
                    keys: &keys[..i % 5],
                })
                .collect();
            for rebinding in [None, Some(Rebind::Replace), Some(Rebind::Add)] {
                ui.begin(1280, 720, vp.scale);
                draw_key_config(
                    &mut ui,
                    &KeyConfigFrame {
                        viewport: &vp,
                        mode,
                        lanes: &lanes,
                        selected: 1,
                        rebinding,
                        layout: "HomeRow",
                        scratch,
                        form,
                    },
                );
                assert_eq!(
                    ui.canvas.debug_batches().len(),
                    1,
                    "{mode:?} {scratch:?} {form:?}"
                );
            }
        }
    }

    #[test]
    fn hit_regions_follow_the_drawn_layout() {
        use crate::hit::{hit_at, HitId};
        let vp = Viewport::new(1280, 720);
        let mut ui = Ui::new(vp.scale);
        let mut layout = SkinConfig::default();
        layout.set_play_mode(PlayMode::Keys7);
        layout.set_scratch_side(PlayMode::Keys7, ScratchSide::Right);
        let keys = ["S", "LCtrl"];
        let lanes: Vec<KeyBinding> = layout
            .screen_lanes()
            .into_iter()
            .map(|lane| KeyBinding {
                lane,
                label: "KEY",
                keys: &keys,
            })
            .collect();
        let frame = KeyConfigFrame {
            viewport: &vp,
            mode: PlayMode::Keys7,
            lanes: &lanes,
            selected: 1,
            rebinding: None,
            layout: "HomeRow",
            scratch: ScratchSide::Right,
            form: EightKForm::Inline,
        };
        ui.begin(1280, 720, vp.scale);
        draw_key_config(&mut ui, &frame);
        let center = |hits: &[crate::hit::Hit], id: HitId| {
            let r = hits.iter().find(|h| h.id == id).expect("recorded").rect;
            (r.x + r.w / 2.0, r.y + r.h / 2.0)
        };

        // Every mode tab sits where it was drawn and maps to its own mode.
        for i in 0..KEY_MODES.len() {
            let (x, y) = center(&ui.hits, HitId::KeyModeTab(i));
            assert_eq!(hit_at(&ui.hits, x, y), Some(HitId::KeyModeTab(i)));
        }
        // Each lane button is recorded once and found at its centre.
        let lane_hits: Vec<_> = ui
            .hits
            .iter()
            .filter(|h| matches!(h.id, HitId::KeyLane(_)))
            .collect();
        assert_eq!(lane_hits.len(), lanes.len());
        for i in 0..lanes.len() {
            let (x, y) = center(&ui.hits, HitId::KeyLane(i));
            assert_eq!(hit_at(&ui.hits, x, y), Some(HitId::KeyLane(i)));
        }

        // Footer buttons of 7K (scratch side, no 8K form) lie in the footer.
        let footer_top = vp.y + vp.height - FOOTER_H * vp.scale;
        for id in [
            HitId::KeySet,
            HitId::KeyAdd,
            HitId::KeyClear,
            HitId::KeyPreset,
            HitId::KeyScratch,
            HitId::KeyReset,
            HitId::KeyBack,
        ] {
            let (x, y) = center(&ui.hits, id);
            assert_eq!(hit_at(&ui.hits, x, y), Some(id));
            let r = ui.hits.iter().find(|h| h.id == id).expect("footer").rect;
            assert!(r.y >= footer_top, "{id:?} is outside the footer");
        }
        assert!(!ui.hits.iter().any(|h| h.id == HitId::KeyForm));

        // While a key is awaited the footer shows only the rebind prompt.
        ui.begin(1280, 720, vp.scale);
        draw_key_config(
            &mut ui,
            &KeyConfigFrame {
                rebinding: Some(Rebind::Replace),
                ..frame
            },
        );
        assert!(!ui
            .hits
            .iter()
            .any(|h| matches!(h.id, HitId::KeyBack | HitId::KeySet)));
        assert!(ui.hits.iter().any(|h| matches!(h.id, HitId::KeyLane(_))));
    }

    #[test]
    fn sides_fit_the_screen() {
        // 14K at full size would be wider than the content area; it must be
        // scaled down rather than overflow.
        let lanes: Vec<KeyBinding> = (0..8)
            .map(|i| KeyBinding {
                lane: if i == 0 { Lane::Scratch } else { Lane::Key1 },
                label: "",
                keys: &[],
            })
            .collect();
        let w = side_width(&lanes.iter().collect::<Vec<_>>(), false, false);
        assert!((w - (TABLE + TABLE_GAP + 6.0 * KEY_STEP + KEY_W)).abs() < 1e-3);
    }
}

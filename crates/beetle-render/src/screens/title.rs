//! Title screen: the first screen after the library is read. A wordmark with
//! an outline and a cyan-to-magenta gradient, a slow light sweep across it and
//! a breathing glow behind it; a tagline and the song count; a vertical menu of
//! four entries; and a pulsing ENTER prompt over the living backdrop.
//!
//! Every period divides the ambient loop (`ui::AMBIENT_LOOP_SECS`), so the
//! clock wraps without a visible jump. Nothing here is judged or timed against
//! the audio clock.

use super::widgets::{self, keycap, keycap_width};
use crate::art::Skin;
use crate::canvas::{Canvas, Rect};
use crate::hit::HitId;
use crate::skin::ColorRgba;
use crate::strings;
use crate::text::{Align, Fx, TextStyle};
use crate::theme::{self, caption};
use crate::ui::Ui;
use crate::view::Viewport;

/// Seconds for one light sweep, including its dark rest. Divides the ambient loop.
pub const SWEEP_SECS: f32 = 6.0;
/// Seconds for one breath of the wordmark glow. Divides the ambient loop.
pub const BREATH_SECS: f32 = 4.0;
/// Seconds for one pulse of the ENTER prompt. Divides the ambient loop.
pub const PROMPT_SECS: f32 = 3.0;
/// Share of a sweep period the light takes to cross the wordmark; the rest is dark.
const SWEEP_SHARE: f32 = 0.5;
/// Lowest alpha of the ENTER prompt's text.
const PROMPT_MIN: f32 = 0.45;

/// The menu's labels, top to bottom (the app maps each index to an action).
pub const MENU_LABELS: [&str; 4] = [
    strings::TITLE_START,
    strings::SETTINGS,
    strings::KEY_CONFIG,
    strings::QUIT,
];

pub struct TitleFrame<'a> {
    pub viewport: &'a Viewport,
    /// Highlighted menu entry (an index into `MENU_LABELS`).
    pub selected: usize,
    /// Songs in the library.
    pub songs: usize,
    /// The app version, for the tagline.
    pub version: &'a str,
    /// Whether the quit prompt is open over the title (it takes the clicks).
    pub modal: bool,
}

/// A cosine wave of `time` (seconds) with `period` seconds per cycle: 0 at
/// the start of a cycle, 1 halfway through.
fn wave(time: f32, period: f32) -> f32 {
    0.5 - 0.5 * (std::f32::consts::TAU * time / period).cos()
}

/// Where the light sweep is across the wordmark, as a fraction of its width
/// (from just left of it to just right), or `None` during the dark rest.
pub fn sweep_center(time: f32) -> Option<f32> {
    let p = (time / SWEEP_SECS).rem_euclid(1.0);
    (p < SWEEP_SHARE).then(|| -0.1 + 1.2 * p / SWEEP_SHARE)
}

/// Strength of the wordmark's glow, 0 to 1.
pub fn breath(time: f32) -> f32 {
    wave(time, BREATH_SECS)
}

/// Alpha of the ENTER prompt's text, from `PROMPT_MIN` up to 1.
pub fn prompt_alpha(time: f32) -> f32 {
    PROMPT_MIN + (1.0 - PROMPT_MIN) * wave(time, PROMPT_SECS)
}

/// The four menu rows in viewport pixels, top to bottom.
pub fn menu_rects(vp: &Viewport) -> [Rect; 4] {
    let s = vp.scale;
    let (w, h, pitch) = (300.0 * s, 40.0 * s, 50.0 * s);
    let x = vp.x + (vp.width - w) / 2.0;
    let top = vp.y + 402.0 * s;
    std::array::from_fn(|i| Rect::new(x, top + pitch * i as f32, w, h))
}

/// Draws the title screen as one batch and records its menu rows as hits.
pub fn draw_title(ui: &mut Ui, f: &TitleFrame) {
    let sk = ui.skin;
    let lite = ui.lite;
    let time = ui.clock;
    let vp = f.viewport;
    let s = vp.scale;
    let cx = vp.x + vp.width / 2.0;

    // Hover and hits are read before the canvas and text are borrowed.
    let rows = menu_rects(vp);
    let hot: [bool; 4] = std::array::from_fn(|i| !f.modal && ui.hovered(rows[i]));
    if !f.modal {
        for (i, r) in rows.iter().enumerate() {
            ui.hit(*r, HitId::TitleItem(i));
        }
    }

    let (c, t) = (&mut ui.canvas, &mut ui.text);
    widgets::backdrop(c, &sk, vp, theme::CYAN, time, lite);

    // Wordmark: outline, cyan-to-magenta gradient, letter-spaced.
    let base = vp.y + 292.0 * s;
    let size = 128.0 * s;
    let mark = TextStyle::new(size)
        .bold()
        .tracking(16.0 * s)
        .color(theme::TEXT);
    let width = t.measure(c, strings::WORDMARK, &mark);
    let x0 = cx - width / 2.0;
    let cap = size * 0.72;
    if !lite {
        let b = breath(time);
        c.set_additive(true);
        c.sprite_centered(
            sk.glow,
            cx,
            base - cap / 2.0,
            width * 1.25,
            cap * 3.0,
            theme::CYAN.with_alpha((30.0 + 40.0 * b) as u8),
        );
        c.sprite_centered(
            sk.glow,
            cx + width * 0.32,
            base - cap / 2.0,
            width * 0.7,
            cap * 2.2,
            theme::MAGENTA.with_alpha((12.0 + 22.0 * (1.0 - b)) as u8),
        );
        c.set_additive(false);
    }
    let fx = Fx {
        outline: Some((theme::BLACK, 3.0 * s)),
        gradient: Some((theme::CYAN, theme::MAGENTA)),
    };
    t.draw_fx(c, strings::WORDMARK, x0, base, &mark, 1.0, fx);
    if !lite {
        if let Some(p) = sweep_center(time) {
            // The slanted band of the menu backdrop, laid over the letters.
            // Confined to the capitals, so its ends are the letters' own tops and baseline.
            let bx = x0 + width * p;
            let half = width * 0.06;
            let slant = cap * 0.6;
            let (top, bot) = (base - cap, base);
            let edge = theme::WHITE.with_alpha(0);
            let mid = theme::WHITE.with_alpha(96);
            c.set_additive(true);
            c.fill_quad(
                [
                    (bx + slant - half, top),
                    (bx + slant, top),
                    (bx, bot),
                    (bx - half, bot),
                ],
                [edge, mid, mid, edge],
            );
            c.fill_quad(
                [
                    (bx + slant, top),
                    (bx + slant + half, top),
                    (bx + half, bot),
                    (bx, bot),
                ],
                [mid, edge, edge, mid],
            );
            c.set_additive(false);
        }
    }

    // Tagline and song count.
    let tagline = strings::fill(strings::TITLE_TAGLINE, &[f.version]);
    t.draw_in(
        c,
        &tagline,
        Rect::new(vp.x, vp.y + 312.0 * s, vp.width, 26.0 * s),
        Align::Center,
        &caption(15.0, s),
    );
    let songs = strings::fill(strings::TITLE_SONGS, &[&f.songs.to_string()]);
    t.draw_in(
        c,
        &songs,
        Rect::new(vp.x, vp.y + 336.0 * s, vp.width, 24.0 * s),
        Align::Center,
        &caption(14.0, s),
    );

    // Menu: the selected row has a cyan rail; a hovered row a lighter panel.
    for (i, r) in rows.iter().enumerate() {
        let on = i == f.selected;
        if on {
            panel(c, &sk, *r, theme::CYAN.with_alpha(40));
            c.fill_rect(
                Rect::new(r.x, r.y + 6.0 * s, 4.0 * s, r.h - 12.0 * s),
                theme::CYAN,
            );
        } else if hot[i] {
            panel(c, &sk, *r, theme::SURF3.with_alpha(200));
        }
        let color = if on || hot[i] {
            theme::TEXT
        } else {
            theme::MUTED
        };
        t.draw_in(
            c,
            MENU_LABELS[i],
            *r,
            Align::Center,
            &TextStyle::new(20.0 * s).bold().color(color),
        );
    }

    // Prompt: PRESS [ENTER] 시작, the text pulsing.
    let a = (prompt_alpha(time) * 255.0) as u8;
    let label = TextStyle::new(14.0 * s)
        .bold()
        .tracking(3.0 * s)
        .color(theme::TEXT.with_alpha(a));
    let press_w = t.measure(c, strings::TITLE_PRESS, &label);
    let key_w = keycap_width(c, t, strings::TITLE_ENTER, s);
    let start_w = t.measure(c, strings::TITLE_START, &label);
    let gap = 10.0 * s;
    let total = press_w + gap + key_w + gap + start_w;
    let py = vp.y + 640.0 * s;
    let mut x = cx - total / 2.0;
    t.draw(c, strings::TITLE_PRESS, x, py, &label);
    x += press_w + gap;
    keycap(
        c,
        t,
        &sk,
        strings::TITLE_ENTER,
        Rect::new(x, py - 17.0 * s, key_w, 22.0 * s),
        s,
    );
    x += key_w + gap;
    t.draw(c, strings::TITLE_START, x, py, &label);
}

/// A rounded menu panel in `tint`.
fn panel(c: &mut Canvas, sk: &Skin, r: Rect, tint: ColorRgba) {
    c.nine(&sk.panel, r, tint);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::AMBIENT_LOOP_SECS;

    const SIZES: [(u32, u32); 3] = [(1280, 720), (1920, 1080), (1024, 768)];

    fn frame(vp: &Viewport, selected: usize, modal: bool) -> TitleFrame<'_> {
        TitleFrame {
            viewport: vp,
            selected,
            songs: 42,
            version: "0.1.0",
            modal,
        }
    }

    #[test]
    fn periods_divide_the_ambient_loop() {
        for period in [SWEEP_SECS, BREATH_SECS, PROMPT_SECS] {
            let n = AMBIENT_LOOP_SECS / period;
            assert!(
                (n - n.round()).abs() < 1e-4,
                "{period} does not divide the loop"
            );
        }
    }

    #[test]
    fn sweep_is_periodic_and_only_crosses_the_wordmark() {
        for i in 0..600 {
            let t = i as f32 * 0.37;
            match (sweep_center(t), sweep_center(t + SWEEP_SECS)) {
                (Some(a), Some(b)) => assert!((a - b).abs() < 1e-3, "t={t}"),
                (a, b) => assert_eq!(a.is_some(), b.is_some(), "t={t}"),
            }
            if let Some(p) = sweep_center(t) {
                assert!((-0.1..=1.1).contains(&p), "t={t} p={p}");
            }
        }
        // The light is dark for the rest of each period.
        assert!(sweep_center(SWEEP_SECS * 0.75).is_none());
        assert!(sweep_center(0.0).is_some());
    }

    #[test]
    fn sweep_and_pulse_wrap_without_a_jump_at_the_loop_end() {
        let end = AMBIENT_LOOP_SECS - 1e-3;
        assert!((breath(end) - breath(0.0)).abs() < 1e-2);
        assert!((prompt_alpha(end) - prompt_alpha(0.0)).abs() < 1e-2);
        assert_eq!(sweep_center(0.0), sweep_center(AMBIENT_LOOP_SECS));
    }

    #[test]
    fn pulses_stay_within_bounds() {
        for i in 0..1000 {
            let t = i as f32 * 0.6;
            let b = breath(t);
            assert!((0.0..=1.0).contains(&b), "breath {b}");
            let a = prompt_alpha(t);
            assert!((PROMPT_MIN - 1e-6..=1.0 + 1e-6).contains(&a), "prompt {a}");
        }
        assert!((prompt_alpha(0.0) - PROMPT_MIN).abs() < 1e-6);
        assert!((prompt_alpha(PROMPT_SECS / 2.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn every_frame_is_one_batch() {
        for (w, h) in SIZES {
            let vp = Viewport::new(w, h);
            let mut ui = Ui::new(vp.scale);
            for clock in [0.0_f32, 0.9, 2.2, 3.7, 4.9, 6.5, 599.5] {
                for selected in 0..4 {
                    ui.begin(w, h, vp.scale);
                    ui.clock = clock;
                    draw_title(&mut ui, &frame(&vp, selected, false));
                    assert_eq!(
                        ui.canvas.debug_batches().len(),
                        1,
                        "{w}x{h} clock={clock} selected={selected}"
                    );
                }
            }
        }
    }

    #[test]
    fn hovered_and_modal_frames_are_one_batch() {
        let vp = Viewport::new(1280, 720);
        let mut ui = Ui::new(vp.scale);
        for r in menu_rects(&vp) {
            ui.begin(1280, 720, vp.scale);
            ui.pointer = Some((r.x + r.w / 2.0, r.y + r.h / 2.0));
            draw_title(&mut ui, &frame(&vp, 0, false));
            assert_eq!(ui.canvas.debug_batches().len(), 1);
        }
        ui.begin(1280, 720, vp.scale);
        draw_title(&mut ui, &frame(&vp, 1, true));
        assert_eq!(ui.canvas.debug_batches().len(), 1);
    }

    #[test]
    fn menu_rows_are_four_disjoint_rects_inside_the_viewport() {
        for (w, h) in SIZES {
            let vp = Viewport::new(w, h);
            let rows = menu_rects(&vp);
            for (i, r) in rows.iter().enumerate() {
                assert!(
                    r.x >= vp.x && r.right() <= vp.x + vp.width,
                    "{w}x{h} row {i}"
                );
                assert!(
                    r.y >= vp.y && r.bottom() <= vp.y + vp.height,
                    "{w}x{h} row {i}"
                );
                for other in &rows[i + 1..] {
                    let apart = r.bottom() <= other.y || other.bottom() <= r.y;
                    assert!(apart, "{w}x{h} rows overlap");
                }
            }
        }
    }

    #[test]
    fn draw_records_one_hit_per_entry_and_none_under_the_modal() {
        let vp = Viewport::new(1280, 720);
        let mut ui = Ui::new(vp.scale);
        ui.begin(1280, 720, vp.scale);
        draw_title(&mut ui, &frame(&vp, 0, false));
        let items: Vec<HitId> = ui
            .hits
            .iter()
            .map(|h| h.id)
            .filter(|id| matches!(id, HitId::TitleItem(_)))
            .collect();
        assert_eq!(items, (0..4).map(HitId::TitleItem).collect::<Vec<_>>());

        ui.begin(1280, 720, vp.scale);
        draw_title(&mut ui, &frame(&vp, 0, true));
        assert!(!ui.hits.iter().any(|h| matches!(h.id, HitId::TitleItem(_))));
    }

    #[test]
    fn the_menu_starts_with_start_and_ends_with_quit() {
        assert_eq!(MENU_LABELS.len(), 4);
        assert_eq!(MENU_LABELS[0], strings::TITLE_START);
        assert_eq!(MENU_LABELS[3], strings::QUIT);
    }
}

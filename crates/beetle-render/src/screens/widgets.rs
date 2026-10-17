//! Building blocks shared by the Canvas-UI screens: backdrop, top bar,
//! footer with keycap hints, score-rate bar.

use crate::art::sdf::hash_noise;
use crate::art::Skin;
use crate::backdrop::Mix;
use crate::canvas::{Canvas, Rect};
use crate::hit::{HitId, HitSink};
use crate::skin::ColorRgba;
use crate::strings;
use crate::text::{Align, TextEngine, TextStyle};
use crate::theme::{self, caption};
use crate::view::Viewport;

// Screen frame grid (1280×720 units).
pub(crate) const PAD: f32 = 32.0;
pub(crate) const TOPBAR_H: f32 = 64.0;
pub(crate) const FOOTER_H: f32 = 40.0;

/// Every drift, sweep, sway and twinkle period below divides the ambient
/// loop (`ui::AMBIENT_LOOP_SECS`), so the clock wraps without a visible jump.
const TAU: f32 = std::f32::consts::TAU;
/// Seconds for the light band to cross the viewport once.
const BAND_PERIOD: f32 = 60.0;
/// Peak alpha of the diagonal light band (additive, so it lifts the
/// backdrop without covering text).
const BAND_ALPHA: u8 = 10;
/// Motes drifting upward behind the menus.
const MOTES: u32 = 32;
/// Peak alpha of one mote (additive).
const MOTE_ALPHA: f32 = 40.0;

/// Center of one drifting glow, as offsets in -1..=1 along a Lissajous
/// path. Periods are in seconds.
pub(crate) fn drift(time: f32, period_x: f32, period_y: f32, phase: f32) -> (f32, f32) {
    (
        (TAU * time / period_x + phase).sin(),
        (TAU * time / period_y + phase).cos(),
    )
}

/// Horizontal center of the light band, in viewport widths. It starts and
/// ends just outside the viewport, so the wrap is never visible.
pub(crate) fn band_center(time: f32) -> f32 {
    -0.3 + 1.6 * (time / BAND_PERIOD).fract()
}

/// One mote at a point in time: position in viewport fractions (0..=1,
/// top-left origin), twinkle and edge fade, both 0..=1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Mote {
    pub x: f32,
    pub y: f32,
    pub twinkle: f32,
    pub fade: f32,
}

/// Mote `i` at `time`. Pure: the same index and time always give the same
/// mote, so no per-mote state is kept between frames.
pub(crate) fn mote(i: u32, time: f32) -> Mote {
    const RISE: [f32; 5] = [12.0, 15.0, 20.0, 24.0, 30.0];
    const SWAY: [f32; 4] = [6.0, 10.0, 12.0, 15.0];
    const TWINKLE: [f32; 5] = [4.0, 5.0, 6.0, 8.0, 10.0];
    // `climb` goes 0 → 1 over one rise period, so the mote travels from
    // the bottom edge to the top edge.
    let climb = (time / RISE[(i % 5) as usize] + hash(i, 1)).fract();
    // Fade in from the bottom edge and out at the top, so no mote pops in.
    let fade = (climb / 0.12).min((1.0 - climb) / 0.12).clamp(0.0, 1.0);
    let base = 0.02 + 0.96 * hash(i, 2);
    let sway = 0.02 * (TAU * time / SWAY[(i % 4) as usize] + hash(i, 3) * TAU).sin();
    let twinkle = 0.5 + 0.5 * (TAU * time / TWINKLE[(i % 5) as usize] + hash(i, 4) * TAU).sin();
    Mote {
        x: base + sway,
        y: 1.0 - climb,
        twinkle,
        fade,
    }
}

/// Deterministic per-index value in 0..=1.
fn hash(i: u32, salt: u32) -> f32 {
    hash_noise(i, salt, 0xBA5D)
}

/// Menu-screen backdrop: dark gradient, ambient light in `ambient`, a slow
/// light band, drifting glows and motes, grain and vignette. `time` is the
/// ambient clock (`Ui::clock`). Skipped in lite mode (see `Ui::lite`); lite
/// stays static, whatever `time` is.
pub(crate) fn backdrop(
    c: &mut Canvas,
    sk: &Skin,
    vp: &Viewport,
    ambient: ColorRgba,
    time: f32,
    lite: bool,
) {
    backdrop_art(c, sk, vp, ambient, time, lite, Mix::default());
}

/// Peak opacity of the blurred jacket picture (`crate::backdrop`).
const ART_ALPHA: f32 = 0.85;
/// Opacity of the dark scrim laid over the picture, so text keeps its contrast.
const ART_SCRIM: u8 = 60;

/// `backdrop` with the blurred jacket picture `art` under the glows. The
/// picture sits on the gradient, under a scrim, and the glows and grain go on
/// top, as before. Lite mode skips all of it.
pub(crate) fn backdrop_art(
    c: &mut Canvas,
    sk: &Skin,
    vp: &Viewport,
    ambient: ColorRgba,
    time: f32,
    lite: bool,
    art: Mix,
) {
    if lite {
        return; // the frame clear color is the backdrop
    }
    let full = Rect::new(vp.x, vp.y, vp.width, vp.height);
    c.fill_rect_vgradient(full, theme::BG, theme::SURF1);
    for layer in [art.outgoing, art.incoming].into_iter().flatten() {
        let a = (255.0 * ART_ALPHA * layer.alpha).round() as u8;
        c.sprite(layer.region, full, theme::WHITE.with_alpha(a));
    }
    if art.incoming.is_some() {
        c.fill_rect(full, theme::BG.with_alpha(ART_SCRIM));
    }
    c.set_additive(true);

    let (dx, dy) = drift(time, 40.0, 30.0, 0.0);
    c.sprite_centered(
        sk.glow,
        vp.x + vp.width * (0.82 + 0.07 * dx),
        vp.y + vp.height * (0.17 + 0.08 * dy),
        vp.width * 0.86,
        vp.height * 1.05,
        ambient.with_alpha(64),
    );
    let (dx, dy) = drift(time, 30.0, 20.0, 2.1);
    c.sprite_centered(
        sk.glow,
        vp.x + vp.width * (0.1 + 0.07 * dx),
        vp.y + vp.height * (0.97 + 0.08 * dy),
        vp.width * 0.7,
        vp.height * 0.7,
        theme::CYAN.with_alpha(20),
    );

    light_band(c, vp, time);
    for i in 0..MOTES {
        let m = mote(i, time);
        let size = (6.0 + 8.0 * hash(i, 5)) * vp.scale;
        let alpha = (MOTE_ALPHA * m.twinkle * m.fade) as u8;
        let tint = if i % 3 == 0 {
            theme::CYAN
        } else {
            theme::WHITE
        };
        c.sprite_centered(
            sk.glow,
            vp.x + vp.width * m.x,
            vp.y + vp.height * m.y,
            size,
            size,
            tint.with_alpha(alpha),
        );
    }

    c.tile(sk.noise, full, theme::WHITE.with_alpha(6));
    c.set_additive(false);
    c.sprite(sk.vignette, full, theme::WHITE.with_alpha(200));
}

/// A faint slanted band of cyan light sweeping across the viewport, echoing
/// the slanted motif of the UI. Two quads, each fading from the edges
/// to the center.
fn light_band(c: &mut Canvas, vp: &Viewport, time: f32) {
    let (w, h) = (vp.width, vp.height);
    let cx = vp.x + w * band_center(time);
    let half = w * 0.06;
    let slant = h * 0.35;
    let (top, bot) = (vp.y, vp.y + h);
    let edge = theme::CYAN.with_alpha(0);
    let mid = theme::CYAN.with_alpha(BAND_ALPHA);
    c.fill_quad(
        [
            (cx + slant - half, top),
            (cx + slant, top),
            (cx, bot),
            (cx - half, bot),
        ],
        [edge, mid, mid, edge],
    );
    c.fill_quad(
        [
            (cx + slant, top),
            (cx + slant + half, top),
            (cx + half, bot),
            (cx, bot),
        ],
        [mid, edge, edge, mid],
    );
}

/// Top bar with a wordmark-style screen title; returns the title's width.
pub(crate) fn top_bar(
    c: &mut Canvas,
    t: &mut TextEngine,
    vp: &Viewport,
    title: &str,
    s: f32,
) -> f32 {
    let bar = Rect::new(vp.x, vp.y, vp.width, TOPBAR_H * s);
    c.fill_rect(bar, theme::BG.with_alpha(200));
    c.fill_rect(Rect::new(bar.x, bar.bottom() - s, bar.w, s), theme::LINE);
    let x0 = vp.x + PAD * s;
    // Only the English wordmark is letter-spaced; screen titles are Korean.
    let logo = TextStyle::new(22.0 * s).bold().color(theme::TEXT);
    let logo = if title == strings::WORDMARK {
        logo.tracking(3.0 * s)
    } else {
        logo
    };
    let adv = t.draw(c, title, x0, vp.y + 41.0 * s, &logo);
    c.fill_rect_hgradient(
        Rect::new(x0, bar.bottom() - 2.0 * s, adv, 2.0 * s),
        theme::CYAN,
        theme::MAGENTA.with_alpha(0),
    );
    adv
}

/// Footer strip; returns its rect.
pub(crate) fn footer_bar(c: &mut Canvas, vp: &Viewport, s: f32) -> Rect {
    let bar = Rect::new(
        vp.x,
        vp.y + vp.height - FOOTER_H * s,
        vp.width,
        FOOTER_H * s,
    );
    c.fill_rect(bar, theme::BG.with_alpha(220));
    c.fill_rect(Rect::new(bar.x, bar.y, bar.w, s.max(1.0)), theme::LINE);
    bar
}

/// Key hints right-aligned in the footer `bar`.
pub(crate) fn footer_hints(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    hints: &[(&str, &str)],
    bar: Rect,
    s: f32,
) {
    let w = hint_row(c, t, sk, hints, 0.0, 0.0, s, false);
    hint_row(
        c,
        t,
        sk,
        hints,
        bar.right() - PAD * s - w,
        bar.y + 10.0 * s,
        s,
        true,
    );
}

/// A footer key hint: key, label, and the action a click on it runs (`None`: not clickable).
pub(crate) type Hint = (&'static str, &'static str, Option<HitId>);

/// Width of a footer row of hints, laid out as `footer_buttons` draws them.
pub(crate) fn hints_width(c: &mut Canvas, t: &mut TextEngine, hints: &[Hint], s: f32) -> f32 {
    let label_st = caption(12.0, s).color(theme::MUTED);
    let mut w = 0.0;
    for (i, (key, label, _)) in hints.iter().enumerate() {
        if i > 0 {
            w += 20.0 * s;
        }
        w += keycap_width(c, t, key, s) + 6.0 * s + t.measure(c, label, &label_st);
    }
    w
}

/// Key hints right-aligned in the footer `bar`, each clickable hint recorded
/// as a region (hover tints it).
pub(crate) fn footer_buttons(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    hints: &[Hint],
    bar: Rect,
    s: f32,
    hs: &mut HitSink,
) {
    let label_st = caption(12.0, s).color(theme::MUTED);
    let y = bar.y + 10.0 * s;
    let mut hx = bar.right() - PAD * s - hints_width(c, t, hints, s);
    for (i, (key, label, action)) in hints.iter().enumerate() {
        if i > 0 {
            hx += 20.0 * s;
        }
        let kw = keycap_width(c, t, key, s);
        let lw = t.measure(c, label, &label_st);
        if let Some(id) = action {
            let hit = Rect::new(hx - 6.0 * s, bar.y + 4.0 * s, kw + lw + 12.0 * s, 32.0 * s);
            hs.add(hit, *id);
            if hs.hovered(hit) {
                c.nine(&sk.panel_sm, hit, theme::WHITE.with_alpha(16));
            }
        }
        keycap(c, t, sk, key, Rect::new(hx, y, kw, 20.0 * s), s);
        t.draw(c, label, hx + kw + 6.0 * s, y + 14.0 * s, &label_st);
        hx += kw + 6.0 * s + lw;
    }
}

/// Height of the help card under an option list (1280×720 units).
pub(crate) const HELP_CARD_H: f32 = 88.0;

/// One line of an option list. A list is drawn in `column`s; `section`, when
/// set, is a header drawn above this line.
pub struct OptionLine<'a> {
    pub column: usize,
    pub section: Option<&'a str>,
    pub label: &'a str,
    /// The value as shown ("500 ms", "GROOVE", "<" arrows added by the drawing).
    pub value: String,
}

/// A section header: caption and a hairline to its right, top at `y`.
pub(crate) fn section_header(
    c: &mut Canvas,
    t: &mut TextEngine,
    label: &str,
    x: f32,
    y: f32,
    w: f32,
    s: f32,
) {
    let cap = caption(12.0, s).color(theme::CYAN.with_alpha(200));
    let lw = t.draw(c, label, x, y + 20.0 * s, &cap);
    c.fill_rect(
        Rect::new(
            x + lw + 12.0 * s,
            y + 16.0 * s,
            w - lw - 12.0 * s,
            s.max(1.0),
        ),
        theme::LINE,
    );
}

/// One option row: label on the left, value in the middle, `<` `>` arrows
/// around the value. The row and both arrows record hits (`OptionRow`,
/// `OptionPrev`, `OptionNext`) with `index`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn option_row(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    hs: &mut HitSink,
    row: Rect,
    index: usize,
    line: &OptionLine,
    on: bool,
    label_w: f32,
    s: f32,
) {
    let hot = !on && hs.hovered(row);
    if on {
        c.nine(&sk.panel, row, theme::CYAN.with_alpha(30));
        c.nine(&sk.panel_outline, row, theme::CYAN.with_alpha(200));
    } else if hot {
        c.nine(&sk.panel, row, theme::SURF2);
    }
    let fg = if on { theme::TEXT } else { theme::MUTED };
    t.draw_in(
        c,
        line.label,
        Rect::new(row.x + 14.0 * s, row.y, label_w, row.h),
        Align::Left,
        &TextStyle::new(13.0 * s).bold().color(fg),
    );

    let icon = 16.0 * s;
    let step = icon + 8.0 * s;
    // The value sits in the right part of the row, so the middle of the row
    // (where a click selects it) stays outside the arrows.
    let value_w = (row.w * 0.4).min(260.0 * s);
    let area = Rect::new(row.right() - value_w - 12.0 * s, row.y, value_w, row.h);
    let prev = Rect::new(area.x, row.y, step, row.h);
    let next = Rect::new(area.right() - step, row.y, step, row.h);
    hs.add(row, HitId::OptionRow(index));
    hs.add(prev, HitId::OptionPrev(index));
    hs.add(next, HitId::OptionNext(index));

    let text = Rect::new(area.x + step, row.y, area.w - 2.0 * step, row.h);
    let st = TextStyle::new(13.0 * s).bold().color(fg);
    let value = t.fit(c, &line.value, text.w - 4.0 * s, &st).into_owned();
    t.draw_in(c, &value, text, Align::Center, &st);
    if on || hot {
        let iy = row.y + (row.h - icon) / 2.0;
        c.sprite(
            sk.icons.chevron_left,
            Rect::new(area.x + 4.0 * s, iy, icon, icon),
            theme::CYAN,
        );
        c.sprite(
            sk.icons.chevron_right,
            Rect::new(area.right() - icon - 4.0 * s, iy, icon, icon),
            theme::CYAN,
        );
    }
}

/// The card that explains the highlighted option: its name, then up to two
/// lines of help.
pub(crate) fn help_card(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    r: Rect,
    title: &str,
    body: &str,
    s: f32,
) {
    c.halo(&sk.shadow, r, theme::WHITE.with_alpha(120));
    c.nine(&sk.panel_lg, r, theme::SURF1.with_alpha(235));
    let inner = r.inset(16.0 * s);
    t.draw(
        c,
        title,
        inner.x,
        inner.y + 12.0 * s,
        &TextStyle::new(14.0 * s).bold().color(theme::TEXT),
    );
    let st = TextStyle::new(13.0 * s).color(theme::MUTED);
    let (first, rest) = wrap2(c, t, body, inner.w, &st);
    t.draw(c, &first, inner.x, inner.y + 34.0 * s, &st);
    if let Some(rest) = rest {
        t.draw(c, &rest, inner.x, inner.y + 52.0 * s, &st);
    }
}

/// Score-rate bar (0..1) with A / AA / AAA marks at the IIDX ninths.
pub(crate) fn rate_bar(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    bar: Rect,
    rate: f32,
    color: ColorRgba,
    s: f32,
) {
    c.nine(&sk.panel_sm, bar, theme::LINE);
    c.nine(
        &sk.panel_sm,
        Rect::new(bar.x, bar.y, bar.w * rate.clamp(0.0, 1.0), bar.h),
        color,
    );
    for (ninths, label) in [(6.0, "A"), (7.0, "AA"), (8.0, "AAA")] {
        let mx = bar.x + bar.w * ninths / 9.0;
        c.fill_rect(
            Rect::new(mx - s / 2.0, bar.y - 3.0 * s, s.max(1.0), bar.h + 6.0 * s),
            theme::MUTED2,
        );
        let lw = t.measure(c, label, &TextStyle::new(11.0 * s).color(theme::MUTED2));
        t.draw(
            c,
            label,
            mx - lw / 2.0,
            bar.bottom() + 14.0 * s,
            &TextStyle::new(11.0 * s).color(theme::MUTED2),
        );
    }
}

/// A small key label ("ENTER", "/") drawn as a keycap.
/// `"←→"` is drawn with chevron icons (the arrow glyphs are too thin at
/// keycap size).
pub(crate) fn keycap(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, key: &str, r: Rect, s: f32) {
    c.nine(&sk.panel_sm, r, theme::SURF3);
    c.nine(
        &sk.panel_sm,
        Rect::new(r.x, r.bottom() - 2.0 * s, r.w, 2.0 * s),
        theme::LINE,
    );
    if key == LEFT_RIGHT {
        let icon = 14.0 * s;
        let (cx, iy) = (r.x + r.w / 2.0, r.y + (r.h - 2.0 * s - icon) / 2.0);
        c.sprite(
            sk.icons.chevron_left,
            Rect::new(cx - icon + 2.0 * s, iy, icon, icon),
            theme::MUTED,
        );
        c.sprite(
            sk.icons.chevron_right,
            Rect::new(cx - 2.0 * s, iy, icon, icon),
            theme::MUTED,
        );
        return;
    }
    t.draw_in(
        c,
        key,
        Rect::new(r.x, r.y, r.w, r.h - 2.0 * s),
        Align::Center,
        &TextStyle::new(11.0 * s).bold().color(theme::MUTED),
    );
}

pub(crate) const LEFT_RIGHT: &str = "←→";

/// A row of "[key] LABEL" hints starting at `x`, keycaps `y`..`y + 20`.
/// Returns the total width.
#[allow(clippy::too_many_arguments)]
pub(crate) fn hint_row(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    hints: &[(&str, &str)],
    x: f32,
    y: f32,
    s: f32,
    draw: bool,
) -> f32 {
    let label_st = caption(12.0, s).color(theme::MUTED);
    let mut hx = x;
    for (i, (key, label)) in hints.iter().enumerate() {
        if i > 0 {
            hx += 20.0 * s;
        }
        let kw = keycap_width(c, t, key, s);
        if draw {
            keycap(c, t, sk, key, Rect::new(hx, y, kw, 20.0 * s), s);
            t.draw(c, label, hx + kw + 6.0 * s, y + 14.0 * s, &label_st);
        }
        hx += kw + 6.0 * s + t.measure(c, label, &label_st);
    }
    hx - x
}

pub(crate) fn keycap_width(c: &mut Canvas, t: &mut TextEngine, key: &str, s: f32) -> f32 {
    if key == LEFT_RIGHT {
        return 28.0 * s;
    }
    (t.measure(c, key, &TextStyle::new(11.0 * s).bold()) + 12.0 * s).max(20.0 * s)
}

/// Splits `text` into at most two lines that fit `max_w`, preferring to
/// break after a space (CJK titles without spaces break between glyphs).
/// The second line is ellipsized if the rest still does not fit.
pub(crate) fn wrap2(
    c: &mut Canvas,
    t: &mut TextEngine,
    text: &str,
    max_w: f32,
    st: &TextStyle,
) -> (String, Option<String>) {
    if t.measure(c, text, st) <= max_w {
        return (text.to_string(), None);
    }
    let mut cut = 0;
    let mut last_space = None;
    for (i, ch) in text.char_indices() {
        let end = i + ch.len_utf8();
        if t.measure(c, &text[..end], st) > max_w {
            break;
        }
        cut = end;
        if ch == ' ' {
            last_space = Some(end);
        }
    }
    let cut = match last_space {
        Some(sp) if sp * 2 > cut => sp, // do not leave a tiny first line
        _ => cut,
    };
    let (first, rest) = text.split_at(cut);
    let rest = t.fit(c, rest.trim_start(), max_w, st).into_owned();
    (first.trim_end().to_string(), Some(rest))
}

/// Indeterminate progress track: a highlight sweeping along `track`, looping
/// every 1.4 s of `elapsed`.
pub(crate) fn sweep_bar(c: &mut Canvas, sk: &Skin, track: Rect, elapsed: f64, s: f32) {
    c.nine(&sk.panel_sm, track, theme::LINE);
    let period = 1.4;
    let ph = ((elapsed % period) / period) as f32;
    let seg = track.w * 0.28;
    let head = track.x - seg + (track.w + seg) * ease_in_out(ph);
    c.push_clip(Rect::new(
        track.x,
        track.y - 8.0 * s,
        track.w,
        track.h + 16.0 * s,
    ));
    c.fill_rect_hgradient(
        Rect::new(head, track.y, seg, track.h),
        theme::CYAN.with_alpha(0),
        theme::CYAN,
    );
    c.set_additive(true);
    c.sprite_centered(
        sk.glow,
        head + seg,
        track.y + track.h / 2.0,
        80.0 * s,
        26.0 * s,
        theme::CYAN.with_alpha(140),
    );
    c.set_additive(false);
    c.pop_clip();
}

fn ease_in_out(p: f32) -> f32 {
    if p < 0.5 {
        4.0 * p * p * p
    } else {
        1.0 - (-2.0 * p + 2.0).powi(3) / 2.0
    }
}

#[cfg(test)]
mod backdrop_tests {
    use super::*;
    use crate::theme::{BLUE, CYAN, MAGENTA};
    use crate::ui::{Ui, AMBIENT_LOOP_SECS};

    /// Samples across one full ambient loop, including the wrap point.
    fn samples() -> impl Iterator<Item = f32> {
        (0..=600).map(|i| i as f32 + 0.37)
    }

    #[test]
    fn drift_and_motes_stay_inside_the_viewport() {
        for t in samples() {
            let (dx, dy) = drift(t, 40.0, 30.0, 0.0);
            assert!((-1.0..=1.0).contains(&dx) && (-1.0..=1.0).contains(&dy));
            for i in 0..MOTES {
                let m = mote(i, t);
                assert!((0.0..=1.0).contains(&m.x), "x={} i={i} t={t}", m.x);
                assert!((0.0..=1.0).contains(&m.y), "y={} i={i} t={t}", m.y);
                assert!((0.0..=1.0).contains(&m.twinkle));
                assert!((0.0..=1.0).contains(&m.fade));
            }
        }
    }

    #[test]
    fn motion_repeats_every_loop() {
        // The clock wraps at the loop length, so every position must match
        // itself one loop later (and the band must, too).
        let loop_len = AMBIENT_LOOP_SECS;
        for t in samples() {
            let (ax, ay) = drift(t, 30.0, 20.0, 2.1);
            let (bx, by) = drift(t + loop_len, 30.0, 20.0, 2.1);
            assert!((ax - bx).abs() < 1e-3 && (ay - by).abs() < 1e-3, "t={t}");
            assert!((band_center(t) - band_center(t + loop_len)).abs() < 1e-3);
            for i in 0..MOTES {
                let (a, b) = (mote(i, t), mote(i, t + loop_len));
                assert!((a.x - b.x).abs() < 1e-3, "x i={i} t={t}");
                assert!((a.y - b.y).abs() < 1e-3, "y i={i} t={t}");
                assert!((a.twinkle - b.twinkle).abs() < 1e-3);
                assert!((a.fade - b.fade).abs() < 1e-3);
            }
        }
    }

    #[test]
    fn motes_rise_over_time() {
        // Mote 0 rises 12 s per crossing: a later time is higher on screen.
        let early = mote(0, 1.0).y;
        let later = mote(0, 2.0).y;
        assert!(later < early, "early={early} later={later}");
    }

    #[test]
    fn backdrop_is_one_batch_at_several_times() {
        let mut ui = Ui::new(1.0);
        let vp = Viewport::new(1280, 720);
        for tint in [CYAN, BLUE, MAGENTA] {
            for time in [0.0, 1.7, 33.3, 599.9] {
                ui.begin(1280, 720, vp.scale);
                backdrop(&mut ui.canvas, &ui.skin, &vp, tint, time, false);
                assert_eq!(ui.canvas.debug_batches().len(), 1, "time={time}");
            }
        }
    }

    #[test]
    fn lite_backdrop_is_static_at_every_time() {
        let mut ui = Ui::new(1.0);
        let vp = Viewport::new(1280, 720);
        let mut frames = Vec::new();
        for time in [0.0, 12.5, 377.0] {
            ui.begin(1280, 720, vp.scale);
            backdrop(&mut ui.canvas, &ui.skin, &vp, CYAN, time, true);
            frames.push(ui.canvas.debug_vertices().len());
        }
        assert!(frames.iter().all(|&n| n == 0), "lite drew {frames:?}");
    }

    #[test]
    fn full_backdrop_changes_over_time() {
        let mut ui = Ui::new(1.0);
        let vp = Viewport::new(1280, 720);
        ui.begin(1280, 720, vp.scale);
        backdrop(&mut ui.canvas, &ui.skin, &vp, CYAN, 0.0, false);
        let a = ui.canvas.debug_vertices().to_vec();
        ui.begin(1280, 720, vp.scale);
        backdrop(&mut ui.canvas, &ui.skin, &vp, CYAN, 9.0, false);
        let b = ui.canvas.debug_vertices().to_vec();
        assert_ne!(a.len(), 0);
        assert!(a.len() == b.len() && a.iter().zip(&b).any(|(p, q)| p.position != q.position));
    }
}

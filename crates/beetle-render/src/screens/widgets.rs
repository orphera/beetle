//! Building blocks shared by the Canvas-UI screens: backdrop, top bar,
//! footer with keycap hints, score-rate bar.

use crate::art::Skin;
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

/// Menu-screen backdrop: dark gradient, ambient light in `ambient`, grain
/// and vignette. Skipped in lite mode (see `Ui::lite`).
pub(crate) fn backdrop(c: &mut Canvas, sk: &Skin, vp: &Viewport, ambient: ColorRgba, lite: bool) {
    if lite {
        return; // the frame clear color is the backdrop
    }
    let full = Rect::new(vp.x, vp.y, vp.width, vp.height);
    c.fill_rect_vgradient(full, theme::BG, theme::SURF1);
    c.set_additive(true);
    c.sprite_centered(
        sk.glow,
        vp.x + vp.width * 0.82,
        vp.y + vp.height * 0.17,
        vp.width * 0.86,
        vp.height * 1.05,
        ambient.with_alpha(64),
    );
    c.sprite_centered(
        sk.glow,
        vp.x + vp.width * 0.1,
        vp.y + vp.height * 0.97,
        vp.width * 0.7,
        vp.height * 0.7,
        theme::CYAN.with_alpha(20),
    );
    c.tile(sk.noise, full, theme::WHITE.with_alpha(6));
    c.set_additive(false);
    c.sprite(sk.vignette, full, theme::WHITE.with_alpha(200));
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
    let label_st = caption(10.0, s).color(theme::MUTED);
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
    let label_st = caption(10.0, s).color(theme::MUTED);
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
    let cap = caption(10.0, s).color(theme::CYAN.with_alpha(200));
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
        let lw = t.measure(c, label, &caption(9.0, s));
        t.draw(
            c,
            label,
            mx - lw / 2.0,
            bar.bottom() + 14.0 * s,
            &caption(9.0, s),
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
        &TextStyle::new(10.0 * s).bold().color(theme::MUTED),
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
    let label_st = caption(10.0, s).color(theme::MUTED);
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
    (t.measure(c, key, &TextStyle::new(10.0 * s).bold()) + 12.0 * s).max(20.0 * s)
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

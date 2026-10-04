//! Building blocks shared by the Canvas-UI screens: backdrop, top bar,
//! footer with keycap hints, score-rate bar.

use crate::art::Skin;
use crate::canvas::{Canvas, Rect};
use crate::renderer::Viewport;
use crate::skin::ColorRgba;
use crate::text::{Align, TextEngine, TextStyle};
use crate::theme::{self, caption};

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
    c.sprite_centered(sk.glow, vp.x + vp.width * 0.82, vp.y + vp.height * 0.17, vp.width * 0.86, vp.height * 1.05, ambient.with_alpha(64));
    c.sprite_centered(sk.glow, vp.x + vp.width * 0.1, vp.y + vp.height * 0.97, vp.width * 0.7, vp.height * 0.7, theme::CYAN.with_alpha(20));
    c.tile(sk.noise, full, theme::WHITE.with_alpha(6));
    c.set_additive(false);
    c.sprite(sk.vignette, full, theme::WHITE.with_alpha(200));
}

/// Top bar with a wordmark-style screen title; returns the title's width.
pub(crate) fn top_bar(c: &mut Canvas, t: &mut TextEngine, vp: &Viewport, title: &str, s: f32) -> f32 {
    let bar = Rect::new(vp.x, vp.y, vp.width, TOPBAR_H * s);
    c.fill_rect(bar, theme::BG.with_alpha(200));
    c.fill_rect(Rect::new(bar.x, bar.bottom() - s, bar.w, s), theme::LINE);
    let x0 = vp.x + PAD * s;
    let logo = TextStyle::new(22.0 * s).bold().tracking(3.0 * s).color(theme::TEXT);
    let adv = t.draw(c, title, x0, vp.y + 41.0 * s, &logo);
    c.fill_rect_hgradient(Rect::new(x0, bar.bottom() - 2.0 * s, adv, 2.0 * s), theme::CYAN, theme::MAGENTA.with_alpha(0));
    adv
}

/// Footer strip; returns its rect.
pub(crate) fn footer_bar(c: &mut Canvas, vp: &Viewport, s: f32) -> Rect {
    let bar = Rect::new(vp.x, vp.y + vp.height - FOOTER_H * s, vp.width, FOOTER_H * s);
    c.fill_rect(bar, theme::BG.with_alpha(220));
    c.fill_rect(Rect::new(bar.x, bar.y, bar.w, s.max(1.0)), theme::LINE);
    bar
}

/// Key hints right-aligned in the footer `bar`.
pub(crate) fn footer_hints(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, hints: &[(&str, &str)], bar: Rect, s: f32) {
    let w = hint_row(c, t, sk, hints, 0.0, 0.0, s, false);
    hint_row(c, t, sk, hints, bar.right() - PAD * s - w, bar.y + 10.0 * s, s, true);
}

/// Score-rate bar (0..1) with A / AA / AAA marks at the IIDX ninths.
pub(crate) fn rate_bar(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, bar: Rect, rate: f32, color: ColorRgba, s: f32) {
    c.nine(&sk.panel_sm, bar, theme::LINE);
    c.nine(&sk.panel_sm, Rect::new(bar.x, bar.y, bar.w * rate.clamp(0.0, 1.0), bar.h), color);
    for (ninths, label) in [(6.0, "A"), (7.0, "AA"), (8.0, "AAA")] {
        let mx = bar.x + bar.w * ninths / 9.0;
        c.fill_rect(Rect::new(mx - s / 2.0, bar.y - 3.0 * s, s.max(1.0), bar.h + 6.0 * s), theme::MUTED2);
        let lw = t.measure(c, label, &caption(9.0, s));
        t.draw(c, label, mx - lw / 2.0, bar.bottom() + 14.0 * s, &caption(9.0, s));
    }
}

/// A small key label ("ENTER", "/") drawn as a keycap.
/// `"←→"` is drawn with chevron icons (the arrow glyphs are too thin at
/// keycap size).
pub(crate) fn keycap(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, key: &str, r: Rect, s: f32) {
    c.nine(&sk.panel_sm, r, theme::SURF3);
    c.nine(&sk.panel_sm, Rect::new(r.x, r.bottom() - 2.0 * s, r.w, 2.0 * s), theme::LINE);
    if key == LEFT_RIGHT {
        let icon = 14.0 * s;
        let (cx, iy) = (r.x + r.w / 2.0, r.y + (r.h - 2.0 * s - icon) / 2.0);
        c.sprite(sk.icons.chevron_left, Rect::new(cx - icon + 2.0 * s, iy, icon, icon), theme::MUTED);
        c.sprite(sk.icons.chevron_right, Rect::new(cx - 2.0 * s, iy, icon, icon), theme::MUTED);
        return;
    }
    t.draw_in(c, key, Rect::new(r.x, r.y, r.w, r.h - 2.0 * s), Align::Center, &TextStyle::new(10.0 * s).bold().color(theme::MUTED));
}

pub(crate) const LEFT_RIGHT: &str = "←→";

/// A row of "[key] LABEL" hints starting at `x`, keycaps `y`..`y + 20`.
/// Returns the total width.
#[allow(clippy::too_many_arguments)]
pub(crate) fn hint_row(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, hints: &[(&str, &str)], x: f32, y: f32, s: f32, draw: bool) -> f32 {
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
pub(crate) fn wrap2(c: &mut Canvas, t: &mut TextEngine, text: &str, max_w: f32, st: &TextStyle) -> (String, Option<String>) {
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

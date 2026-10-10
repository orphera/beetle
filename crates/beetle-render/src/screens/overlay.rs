//! Presentation layers drawn on top of a finished menu frame: the fade-in
//! from the background after a screen change, and the toast notification.
//! Both take their timing from the app (`beetle-app/src/transition.rs`); they
//! only draw what they are given.

use crate::canvas::Rect;
use crate::text::{Align, TextStyle};
use crate::theme;
use crate::ui::Ui;
use crate::view::Viewport;

use super::widgets::FOOTER_H;

/// What a toast says about itself; it picks the accent colour and icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

/// Where a toast sits, chosen per screen so it never covers a control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastAnchor {
    /// The footer strip, left of the key hints (empty on the menus).
    Footer,
    /// The band under Key Config's mode tabs (above the keyboard).
    BelowTabs,
}

/// One toast at a point in its life. `alpha` and `slide` come from the app's
/// timeline: `slide` 0 is raised by a few pixels, 1 is in place.
#[derive(Debug, Clone, Copy)]
pub struct ToastFrame<'a> {
    pub text: &'a str,
    pub kind: ToastKind,
    pub alpha: f32,
    pub slide: f32,
    pub anchor: ToastAnchor,
}

/// Left edge of a footer toast, in 720 units: clears the footer count on the
/// song list ("N / M곡"), which is the only text left of the hints.
const FOOTER_TOAST_X: f32 = 140.0;
/// Top of a Key Config toast, in 720 units: under the tabs and their note
/// line (ends at y 148), above the keyboard (starts at y 224 on 7K).
const BELOW_TABS_TOAST_Y: f32 = 152.0;

/// Fills the whole frame with the background colour at `alpha` (1 = covered,
/// 0 = nothing drawn). Used for the fade-in after a screen change.
pub fn draw_screen_fade(ui: &mut Ui, alpha: f32) {
    let a = alpha8(alpha);
    if a == 0 {
        return;
    }
    let (w, h) = (ui.canvas.width() as f32, ui.canvas.height() as f32);
    ui.canvas
        .fill_rect(Rect::new(0.0, 0.0, w, h), theme::BG.with_alpha(a));
}

/// A small rounded panel with an icon and one line of text, centred under the
/// top bar. Not clickable: it records no hit.
pub fn draw_toast(ui: &mut Ui, vp: &Viewport, toast: &ToastFrame) {
    let a = toast.alpha.clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    let sk = ui.skin;
    let s = vp.scale;
    let accent = match toast.kind {
        ToastKind::Info => theme::CYAN,
        ToastKind::Success => theme::GREEN,
        ToastKind::Error => theme::RED,
    };
    let style = TextStyle::new(15.0 * s).color(theme::TEXT.with_alpha(alpha8(a)));
    let (c, t) = (&mut ui.canvas, &mut ui.text);
    let text_w = t.measure(c, toast.text, &style);

    const PAD_X: f32 = 18.0;
    const ICON: f32 = 10.0;
    const GAP: f32 = 12.0;
    const H: f32 = 40.0;
    const SLIDE: f32 = 10.0;
    let w = (PAD_X * 2.0 + ICON + GAP) * s + text_w;
    let h = H * s;
    let rise = (1.0 - toast.slide.clamp(0.0, 1.0)) * SLIDE * s;
    let (x, top) = match toast.anchor {
        // The toast is as tall as the footer, so it fills the strip.
        ToastAnchor::Footer => (
            vp.x + FOOTER_TOAST_X * s,
            vp.y + vp.height - FOOTER_H * s - rise,
        ),
        ToastAnchor::BelowTabs => (
            vp.x + (vp.width - w) / 2.0,
            vp.y + BELOW_TABS_TOAST_Y * s - rise,
        ),
    };
    let rect = Rect::new(x, top, w, h);

    c.halo(&sk.shadow, rect, theme::BLACK.with_alpha(alpha8(a * 0.6)));
    c.nine(&sk.panel, rect, theme::SURF3.with_alpha(alpha8(a * 0.96)));
    c.nine(&sk.panel_outline, rect, accent.with_alpha(alpha8(a * 0.6)));

    let icon = ICON * s;
    let icon_rect = Rect::new(x + PAD_X * s, top + (h - icon) / 2.0, icon, icon);
    let icon_tint = accent.with_alpha(alpha8(a));
    match toast.kind {
        ToastKind::Success => c.sprite(sk.icons.check, icon_rect, icon_tint),
        ToastKind::Info | ToastKind::Error => c.sprite(sk.icons.dot, icon_rect, icon_tint),
    }

    let text_x = x + (PAD_X + ICON + GAP) * s;
    let text_rect = Rect::new(text_x, top, text_w + 2.0, h);
    t.draw_in(c, toast.text, text_rect, Align::Left, &style);
}

fn alpha8(a: f32) -> u8 {
    (a.clamp(0.0, 1.0) * 255.0).round() as u8
}

//! Presentation layers drawn on top of a finished menu frame: the fade-in
//! from the background after a screen change, and the toast notification.
//! Both take their timing from the app (`beetle-app/src/transition.rs`); they
//! only draw what they are given.

use crate::canvas::Rect;
use crate::motion::ease_in_out_cubic;
use crate::skin::ColorRgba;
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

/// One step of the shutter wipe, as a linear fraction of its phase.
/// `Cover` sweeps the panels across the frame (0 clear, 1 covered); `Reveal`
/// sweeps them off again (0 covered, 1 clear). The easing happens here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShutterPhase {
    Cover(f32),
    Reveal(f32),
}

/// Edge slant, in 720-line units: the top of a panel edge sits this far right
/// of its bottom (the cut-corner angle of the top bar buttons).
const SHUTTER_SLANT: f32 = 40.0;
/// How far the leading panel runs ahead of the trailing one, in 720-line units.
const SHUTTER_LEAD: f32 = 56.0;
/// Thickness of the cyan edge line, in 720-line units.
const SHUTTER_EDGE: f32 = 2.0;

/// A slanted panel: its four corners in winding order, and its colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShutterPanel {
    pub quad: [(f32, f32); 4],
    pub color: ColorRgba,
}

/// What a shutter draws at one phase, back to front: `lead` (SURF2) first,
/// then `trail` (BG) on top, so the lead shows as a band ahead of the trail
/// with its cyan `edge` on the leading side. A panel that misses the
/// viewport is `None`, so a clear phase draws nothing at all.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShutterGeometry {
    pub lead: Option<ShutterPanel>,
    pub trail: Option<ShutterPanel>,
    /// The cyan line along the lead's leading edge, as a quad.
    pub edge: Option<[(f32, f32); 4]>,
}

/// Panel geometry for one phase of the shutter. Pure: no GPU, no timing.
/// Within a phase the edges move monotonically, so the covered area only
/// grows during `Cover` and only shrinks during `Reveal`. The trail covers the
/// whole viewport at `Cover(1)` and at `Reveal(0)`, and the trail and lead are
/// both off-screen at `Cover(0)` and `Reveal(1)`.
pub fn shutter_geometry(vp: &Viewport, phase: ShutterPhase) -> ShutterGeometry {
    let s = SHUTTER_SLANT * vp.scale;
    let lead_w = SHUTTER_LEAD * vp.scale;
    let (top, bottom) = (vp.y, vp.y + vp.height);
    let right = vp.x + vp.width;
    // Edge positions at the frame's vertical centre: `start` is just off the
    // left edge (with the slant), `end` just off the right edge.
    let (start, end) = (vp.x - s, right + s);
    let (cover, lead_x, trail_x) = match phase {
        ShutterPhase::Cover(t) => {
            let e = ease_in_out_cubic(t);
            (
                true,
                start + (end - start + lead_w) * e,
                start + (end - start) * e,
            )
        }
        ShutterPhase::Reveal(t) => {
            let e = ease_in_out_cubic(t);
            (
                false,
                start - lead_w + (end - start + lead_w) * e,
                start + (end - start) * e,
            )
        }
    };
    // Cover panels hang off the left edge, reveal panels off the right one.
    let far = if cover {
        vp.x - 2.0 * s
    } else {
        right + 2.0 * s
    };
    let panel = |x: f32, color: ColorRgba| {
        let quad = if cover {
            [
                (far, top),
                (x + s / 2.0, top),
                (x - s / 2.0, bottom),
                (far, bottom),
            ]
        } else {
            [
                (x + s / 2.0, top),
                (far, top),
                (far, bottom),
                (x - s / 2.0, bottom),
            ]
        };
        ShutterPanel { quad, color }
    };
    // A shape is visible when its horizontal extent overlaps the viewport.
    let seen = |xs: &[f32]| {
        let lo = xs.iter().copied().fold(f32::MAX, f32::min);
        let hi = xs.iter().copied().fold(f32::MIN, f32::max);
        lo < right && hi > vp.x
    };

    let lead = Some(panel(lead_x, theme::SURF2)).filter(|p| seen(&p.quad.map(|q| q.0)));
    let trail = Some(panel(trail_x, theme::BG)).filter(|p| seen(&p.quad.map(|q| q.0)));
    // The cyan line is a quad wound like the panels: the D3D11 default
    // rasterizer culls the other winding, so it would silently vanish. The
    // same rule holds for `Canvas::line`, which is wound like `fill_rect`.
    let h = SHUTTER_EDGE * vp.scale / 2.0;
    let (xt, xb) = (lead_x + s / 2.0, lead_x - s / 2.0);
    let edge_quad = [
        (xt - h, top),
        (xt + h, top),
        (xb + h, bottom),
        (xb - h, bottom),
    ];
    let edge = lead
        .is_some()
        .then_some(edge_quad)
        .filter(|q| seen(&q.map(|p| p.0)));
    ShutterGeometry { lead, trail, edge }
}

/// Draws the shutter over a finished frame: at most three quads, all on the
/// atlas, so it adds no batch of its own beyond the frame's. Nothing is drawn
/// when the phase is clear. Records no hit regions, so input is unaffected.
pub fn draw_shutter(ui: &mut Ui, vp: &Viewport, phase: ShutterPhase) {
    let g = shutter_geometry(vp, phase);
    for panel in [g.lead, g.trail].into_iter().flatten() {
        ui.canvas.fill_quad(panel.quad, [panel.color; 4]);
    }
    if let Some(quad) = g.edge {
        ui.canvas.fill_quad(quad, [theme::CYAN; 4]);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether `p` lies inside the convex quad `q` (either winding).
    fn inside(q: &[(f32, f32); 4], p: (f32, f32)) -> bool {
        let mut sign = 0.0f32;
        for i in 0..4 {
            let (a, b) = (q[i], q[(i + 1) % 4]);
            let cross = (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
            if cross == 0.0 {
                continue;
            }
            if sign == 0.0 {
                sign = cross.signum();
            } else if sign != cross.signum() {
                return false;
            }
        }
        true
    }

    /// Share of a 32 x 18 grid of points over the viewport that the shutter covers.
    fn covered(vp: &Viewport, phase: ShutterPhase) -> f32 {
        let g = shutter_geometry(vp, phase);
        let quads: Vec<[(f32, f32); 4]> = [g.lead, g.trail]
            .into_iter()
            .flatten()
            .map(|p| p.quad)
            .collect();
        let (nx, ny) = (32, 18);
        let mut hit = 0;
        for iy in 0..ny {
            for ix in 0..nx {
                let p = (
                    vp.x + (ix as f32 + 0.5) * vp.width / nx as f32,
                    vp.y + (iy as f32 + 0.5) * vp.height / ny as f32,
                );
                if quads.iter().any(|q| inside(q, p)) {
                    hit += 1;
                }
            }
        }
        hit as f32 / (nx * ny) as f32
    }

    fn vp() -> Viewport {
        Viewport::new(1280, 720)
    }

    /// Corners in the order `Canvas::fill_rect` emits them.
    const FULL_RECT: [(f32, f32); 4] = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];

    #[test]
    fn shutter_is_clear_at_both_ends() {
        let vp = vp();
        let start = shutter_geometry(&vp, ShutterPhase::Cover(0.0));
        assert_eq!((start.lead, start.trail, start.edge), (None, None, None));
        let end = shutter_geometry(&vp, ShutterPhase::Reveal(1.0));
        assert_eq!((end.lead, end.trail, end.edge), (None, None, None));
        assert_eq!(covered(&vp, ShutterPhase::Cover(0.0)), 0.0);
        assert_eq!(covered(&vp, ShutterPhase::Reveal(1.0)), 0.0);
    }

    #[test]
    fn shutter_covers_the_frame_at_the_midpoint() {
        let vp = vp();
        assert_eq!(covered(&vp, ShutterPhase::Cover(1.0)), 1.0);
        assert_eq!(covered(&vp, ShutterPhase::Reveal(0.0)), 1.0);
    }

    #[test]
    fn cover_grows_and_reveal_shrinks_monotonically() {
        let vp = vp();
        let steps: Vec<f32> = (0..=40).map(|i| i as f32 / 40.0).collect();
        let cover: Vec<f32> = steps
            .iter()
            .map(|&t| covered(&vp, ShutterPhase::Cover(t)))
            .collect();
        assert!(cover.windows(2).all(|w| w[1] >= w[0]), "{cover:?}");
        let reveal: Vec<f32> = steps
            .iter()
            .map(|&t| covered(&vp, ShutterPhase::Reveal(t)))
            .collect();
        assert!(reveal.windows(2).all(|w| w[1] <= w[0]), "{reveal:?}");
    }

    #[test]
    fn panels_are_slanted_with_the_cyan_edge_on_the_lead() {
        let vp = vp();
        let g = shutter_geometry(&vp, ShutterPhase::Cover(0.5));
        let lead = g.lead.expect("lead visible mid-cover");
        assert_eq!(lead.color, theme::SURF2);
        assert_eq!(g.trail.expect("trail visible").color, theme::BG);
        let edge = g.edge.expect("edge visible mid-cover");
        // Top-left, top-right, bottom-right, bottom-left: the top leans right.
        assert!(edge[0].0 > edge[3].0, "top leans right of bottom (slant)");
        assert_eq!((edge[0].1, edge[3].1), (vp.y, vp.y + vp.height));
    }

    /// Signed area of a quad; the sign is its winding.
    fn winding(q: &[(f32, f32); 4]) -> f32 {
        (0..4)
            .map(|i| {
                let (a, b) = (q[i], q[(i + 1) % 4]);
                a.0 * b.1 - b.0 * a.1
            })
            .sum::<f32>()
            .signum()
    }

    #[test]
    fn every_shutter_quad_shares_one_winding() {
        // The backend culls back faces, so the panels and the edge must all
        // be wound the same way, in every phase, or some would not show.
        let vp = vp();
        let mut seen_quads = 0;
        for i in 0..=20 {
            let t = i as f32 / 20.0;
            for phase in [ShutterPhase::Cover(t), ShutterPhase::Reveal(t)] {
                let g = shutter_geometry(&vp, phase);
                let quads = [g.lead.map(|p| p.quad), g.trail.map(|p| p.quad), g.edge];
                for q in quads.iter().flatten() {
                    assert_eq!(winding(q), winding(&FULL_RECT), "{phase:?}");
                    seen_quads += 1;
                }
            }
        }
        assert!(seen_quads > 40, "the phases drew {seen_quads} quads");
    }

    /// The overlay for overall progress `t` of the shutter (0.0 start, 1.0 end),
    /// as the app maps it: cover over the first 45 %, reveal over the rest.
    /// Each must add at most one batch to the frame it sits on.
    #[test]
    fn shutter_overlay_adds_at_most_one_batch() {
        let vp = vp();
        let cases = [
            (0.0, ShutterPhase::Cover(0.0)),
            (0.25, ShutterPhase::Cover(0.556)),
            (0.5, ShutterPhase::Reveal(0.09)),
            (0.75, ShutterPhase::Reveal(0.545)),
            (1.0, ShutterPhase::Reveal(1.0)),
        ];
        let mut ui = Ui::new(1.0);
        for (t, phase) in cases {
            ui.begin(1280, 720, 1.0);
            let before = ui.canvas.debug_batches().len();
            draw_shutter(&mut ui, &vp, phase);
            let added = ui.canvas.debug_batches().len() - before;
            assert!(added <= 1, "t={t}: {added} batches");
            if t == 1.0 {
                assert_eq!(added, 0, "a clear shutter draws nothing");
            }
        }
    }
}

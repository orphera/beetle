//! Judge offset calibration on the `Ui`: a sub-screen of Settings. A beat
//! indicator flashes on each click, the counted presses sit on a line from
//! early to late, and once the test is done the suggested offset is shown
//! with 적용 (ENTER), 다시 (R) and 취소 (ESC).

use super::widgets::{self, Hint, FOOTER_H, HELP_CARD_H, PAD, TOPBAR_H};
use crate::canvas::Rect;
use crate::hit::{HitId, HitSink};
use crate::strings;
use crate::text::{Align, TextStyle};
use crate::theme::{self, caption};
use crate::ui::Ui;
use crate::view::Viewport;

/// Gap between the stat cards and the mark line (1280×720 units).
const GAP: f32 = 16.0;
const STAT_H: f32 = 80.0;

/// Where the test is, as the screen shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibratePhase {
    /// No output device: nothing can be measured.
    Unavailable,
    /// The first clicks, whose presses do not count.
    CountIn,
    Measuring,
    /// Enough presses: the suggestion is shown.
    Done,
}

/// Everything the calibration screen shows for one frame.
pub struct CalibrateFrame<'a> {
    pub viewport: &'a Viewport,
    pub phase: CalibratePhase,
    /// Beat indicator strength: 1 on a click, fading to 0.
    pub pulse: f32,
    pub required: usize,
    pub collected: usize,
    /// Half-width of the mark line in ms (the match window).
    pub window_ms: f64,
    /// Counted presses in order: error in ms (late positive), and whether
    /// the mean uses it.
    pub marks: &'a [(f64, bool)],
    pub mean_ms: Option<f64>,
    pub std_ms: Option<f64>,
    /// Suggested judge offset in ms, once the test is done.
    pub suggestion_ms: Option<f64>,
}

const HINTS_RUNNING: [Hint; 3] = [
    ("SPACE", strings::CALIBRATE_HINT_TAP, None),
    ("R", strings::CALIBRATE_RETRY, Some(HitId::CalibrateRetry)),
    (
        "ESC",
        strings::CALIBRATE_CANCEL,
        Some(HitId::CalibrateCancel),
    ),
];
const HINTS_DONE: [Hint; 3] = [
    (
        "ENTER",
        strings::CALIBRATE_APPLY,
        Some(HitId::CalibrateApply),
    ),
    ("R", strings::CALIBRATE_RETRY, Some(HitId::CalibrateRetry)),
    (
        "ESC",
        strings::CALIBRATE_CANCEL,
        Some(HitId::CalibrateCancel),
    ),
];
const HINTS_UNAVAILABLE: [Hint; 1] = [(
    "ESC",
    strings::CALIBRATE_CANCEL,
    Some(HitId::CalibrateCancel),
)];

pub fn draw_calibrate(ui: &mut Ui, f: &CalibrateFrame) {
    let sk = ui.skin;
    let lite = ui.lite;
    let mut hs = HitSink::new(&mut ui.hits, ui.pointer);
    let (c, t) = (&mut ui.canvas, &mut ui.text);
    let vp = f.viewport;
    let s = vp.scale;

    widgets::backdrop(c, &sk, vp, theme::BLUE, lite);
    widgets::top_bar(c, t, vp, strings::CALIBRATE_TITLE, s);

    let content = Rect::from_ltrb(
        vp.x + PAD * s,
        vp.y + (TOPBAR_H + 24.0) * s,
        vp.x + vp.width - PAD * s,
        vp.y + vp.height - (FOOTER_H + 24.0) * s,
    );
    let card = Rect::new(
        content.x,
        content.bottom() - HELP_CARD_H * s,
        content.w,
        HELP_CARD_H * s,
    );
    let area = Rect::from_ltrb(content.x, content.y, content.right(), card.y - GAP * s);
    let cx = area.x + area.w / 2.0;

    // Headline and help, by phase.
    let (headline, help, hints): (&str, &str, &[Hint]) = match f.phase {
        CalibratePhase::Unavailable => (
            strings::CALIBRATE_NO_AUDIO,
            strings::CALIBRATE_HELP_NO_AUDIO,
            &HINTS_UNAVAILABLE,
        ),
        CalibratePhase::CountIn | CalibratePhase::Measuring => (
            strings::CALIBRATE_INSTRUCTION,
            strings::CALIBRATE_HELP,
            &HINTS_RUNNING,
        ),
        CalibratePhase::Done => (
            strings::CALIBRATE_DONE_TITLE,
            strings::CALIBRATE_HELP_DONE,
            &HINTS_DONE,
        ),
    };
    let head = TextStyle::new(20.0 * s).bold().color(theme::TEXT);
    t.draw_in(
        c,
        headline,
        Rect::new(area.x, area.y, area.w, 36.0 * s),
        Align::Center,
        &head,
    );

    // Centre: the beat indicator while measuring, the suggestion when done.
    match f.phase {
        CalibratePhase::CountIn | CalibratePhase::Measuring => {
            let label = if f.phase == CalibratePhase::CountIn {
                strings::CALIBRATE_COUNT_IN
            } else {
                strings::CALIBRATE_MEASURING
            };
            let cy = area.y + 100.0 * s;
            let glow = (120.0 + 40.0 * f.pulse) * s;
            c.sprite_centered(
                sk.glow,
                cx,
                cy,
                glow,
                glow,
                theme::CYAN.with_alpha(alpha8(0.25 + 0.6 * f.pulse)),
            );
            c.sprite_centered(
                sk.icons.dot,
                cx,
                cy,
                28.0 * s,
                28.0 * s,
                theme::WHITE.with_alpha(alpha8(0.5 + 0.5 * f.pulse)),
            );
            t.draw_in(
                c,
                label,
                Rect::new(area.x, area.y + 150.0 * s, area.w, 24.0 * s),
                Align::Center,
                &caption(14.0, s).color(theme::TEXT),
            );
        }
        CalibratePhase::Done => {
            t.draw_in(
                c,
                strings::CALIBRATE_SUGGEST,
                Rect::new(area.x, area.y + 44.0 * s, area.w, 20.0 * s),
                Align::Center,
                &caption(12.0, s).color(theme::MUTED),
            );
            let value = f.suggestion_ms.map_or_else(
                || "--".to_string(),
                |v| strings::fill(strings::VALUE_MS, &[&format!("{v:+.0}")]),
            );
            t.draw_in(
                c,
                &value,
                Rect::new(area.x, area.y + 66.0 * s, area.w, 64.0 * s),
                Align::Center,
                &TextStyle::new(44.0 * s).bold().color(theme::CYAN),
            );
        }
        CalibratePhase::Unavailable => {}
    }

    // Stat cards: progress, mean error, spread.
    let stat_y = area.y + 200.0 * s;
    let card_w = (area.w - 2.0 * GAP * s) / 3.0;
    let stats = [
        (
            strings::CALIBRATE_PROGRESS,
            format!("{}/{}", f.collected, f.required),
        ),
        (
            strings::CALIBRATE_MEAN,
            f.mean_ms.map_or_else(
                || "--".to_string(),
                |m| strings::fill(strings::VALUE_MS, &[&format!("{m:+.1}")]),
            ),
        ),
        (
            strings::CALIBRATE_SPREAD,
            f.std_ms.map_or_else(
                || "--".to_string(),
                |v| strings::fill(strings::VALUE_MS, &[&format!("{v:.1}")]),
            ),
        ),
    ];
    for (i, (label, value)) in stats.iter().enumerate() {
        let r = Rect::new(
            area.x + i as f32 * (card_w + GAP * s),
            stat_y,
            card_w,
            STAT_H * s,
        );
        c.nine(&sk.panel, r, theme::SURF2.with_alpha(230));
        t.draw_in(
            c,
            label,
            Rect::new(r.x, r.y + 8.0 * s, r.w, 22.0 * s),
            Align::Center,
            &caption(11.0, s).color(theme::MUTED),
        );
        t.draw_in(
            c,
            value,
            Rect::new(r.x, r.y + 30.0 * s, r.w, 44.0 * s),
            Align::Center,
            &TextStyle::new(26.0 * s).bold().color(theme::TEXT),
        );
    }

    // The mark line: every counted press, early on the left, late on the right.
    let line_y = area.y + 340.0 * s;
    let x0 = area.x + 40.0 * s;
    let x1 = area.right() - 40.0 * s;
    let mid = (x0 + x1) / 2.0;
    let half = (x1 - x0) / 2.0;
    c.fill_rect(Rect::new(x0, line_y, x1 - x0, s.max(1.0)), theme::LINE);
    c.fill_rect(
        Rect::new(
            mid - s.max(1.0),
            line_y - 16.0 * s,
            2.0 * s.max(1.0),
            32.0 * s,
        ),
        theme::MUTED2,
    );
    for &(ms, counted) in f.marks {
        let u = (ms / f.window_ms).clamp(-1.0, 1.0) as f32;
        let x = mid + u * half;
        let color = if counted { theme::CYAN } else { theme::MUTED2 };
        c.fill_rect(
            Rect::new(x - 2.0 * s, line_y - 14.0 * s, 4.0 * s, 28.0 * s),
            color,
        );
    }
    let dir = caption(11.0, s).color(theme::MUTED);
    t.draw_in(
        c,
        strings::CALIBRATE_EARLY,
        Rect::new(x0, line_y - 40.0 * s, 120.0 * s, 18.0 * s),
        Align::Left,
        &dir,
    );
    t.draw_in(
        c,
        strings::CALIBRATE_LATE,
        Rect::new(x1 - 120.0 * s, line_y - 40.0 * s, 120.0 * s, 18.0 * s),
        Align::Right,
        &dir,
    );
    let w = f.window_ms;
    let tick = |v: String| strings::fill(strings::VALUE_MS, &[&v]);
    for (x, text) in [
        (x0, tick(format!("-{w:.0}"))),
        (mid, tick("0".to_string())),
        (x1, tick(format!("+{w:.0}"))),
    ] {
        t.draw_in(
            c,
            &text,
            Rect::new(x - 60.0 * s, line_y + 18.0 * s, 120.0 * s, 16.0 * s),
            Align::Center,
            &caption(10.0, s).color(theme::MUTED2),
        );
    }
    if f.marks.iter().any(|&(_, counted)| !counted) {
        t.draw_in(
            c,
            strings::CALIBRATE_EXCLUDED,
            Rect::new(area.x, line_y + 44.0 * s, area.w, 18.0 * s),
            Align::Center,
            &caption(10.0, s).color(theme::MUTED2),
        );
    }

    widgets::help_card(c, t, &sk, card, strings::CALIBRATE_TITLE, help, s);
    let bar = widgets::footer_bar(c, vp, s);
    widgets::footer_buttons(c, t, &sk, hints, bar, s, &mut hs);
}

fn alpha8(a: f32) -> u8 {
    (a.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hit::hit_at;

    fn frame<'a>(
        vp: &'a Viewport,
        phase: CalibratePhase,
        marks: &'a [(f64, bool)],
    ) -> CalibrateFrame<'a> {
        CalibrateFrame {
            viewport: vp,
            phase,
            pulse: 0.6,
            required: 16,
            collected: marks.len(),
            window_ms: 150.0,
            marks,
            mean_ms: Some(18.4),
            std_ms: Some(3.2),
            suggestion_ms: (phase == CalibratePhase::Done).then_some(-18.0),
        }
    }

    #[test]
    fn done_screen_records_apply_retry_and_cancel() {
        let vp = Viewport::new(1280, 720);
        let mut ui = Ui::new(vp.scale);
        let marks = [(12.0, true), (-30.0, true), (140.0, false)];
        ui.begin(1280, 720, vp.scale);
        draw_calibrate(&mut ui, &frame(&vp, CalibratePhase::Done, &marks));
        for id in [
            HitId::CalibrateApply,
            HitId::CalibrateRetry,
            HitId::CalibrateCancel,
        ] {
            let r = ui.hits.iter().find(|h| h.id == id).expect("hit").rect;
            let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
            assert_eq!(hit_at(&ui.hits, x, y), Some(id));
        }
    }

    #[test]
    fn running_screen_offers_retry_and_cancel_only() {
        let vp = Viewport::new(1280, 720);
        let mut ui = Ui::new(vp.scale);
        ui.begin(1280, 720, vp.scale);
        draw_calibrate(&mut ui, &frame(&vp, CalibratePhase::Measuring, &[]));
        assert!(!ui.hits.iter().any(|h| h.id == HitId::CalibrateApply));
        assert!(ui.hits.iter().any(|h| h.id == HitId::CalibrateRetry));
        assert!(ui.hits.iter().any(|h| h.id == HitId::CalibrateCancel));
    }

    #[test]
    fn every_phase_is_one_batch() {
        let vp = Viewport::new(1280, 720);
        let mut ui = Ui::new(vp.scale);
        let marks = [(5.0, true), (-150.0, false)];
        for phase in [
            CalibratePhase::Unavailable,
            CalibratePhase::CountIn,
            CalibratePhase::Measuring,
            CalibratePhase::Done,
        ] {
            ui.begin(1280, 720, vp.scale);
            draw_calibrate(&mut ui, &frame(&vp, phase, &marks));
            assert_eq!(ui.canvas.debug_batches().len(), 1, "{phase:?}");
        }
    }
}

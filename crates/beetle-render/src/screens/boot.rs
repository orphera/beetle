//! Boot screen: shown while the song library is read (at startup and on a
//! rescan) so the window paints from its first frame instead of hanging.
//!
//! The scan reports no progress, so the bar is indeterminate. Everything
//! animates from `elapsed`; the app redraws every frame meanwhile (INV-5).

use super::widgets;
use crate::canvas::Rect;
use crate::motion::ease_out_cubic;
use crate::strings;
use crate::text::{Align, TextStyle};
use crate::theme::{self, caption};
use crate::ui::Ui;
use crate::view::Viewport;

pub struct BootFrame<'a> {
    pub viewport: &'a Viewport,
    /// Seconds since the library load started.
    pub elapsed: f64,
    /// Title: "시작하는 중" / "서재를 다시 읽는 중" (see `strings`).
    pub title: &'a str,
    /// What is happening ("곡 목록을 읽는 중").
    pub status: &'a str,
}

const ENTER_SECONDS: f64 = 0.35;

pub fn draw_boot(ui: &mut Ui, f: &BootFrame) {
    let sk = ui.skin;
    let lite = ui.lite;
    let (c, t) = (&mut ui.canvas, &mut ui.text);
    let vp = f.viewport;
    let s = vp.scale;

    widgets::backdrop(c, &sk, vp, theme::CYAN, lite);

    let p = ease_out_cubic((f.elapsed / ENTER_SECONDS).clamp(0.0, 1.0) as f32);
    let a = |max: u8| (max as f32 * p) as u8;
    let slide = (1.0 - p) * 32.0 * s;

    let x = vp.x + 160.0 * s - slide;
    let y = vp.y + 270.0 * s;
    // Korean caption: no letter-spacing (tracking splits the syllables).
    let label = caption(12.0, s).color(theme::CYAN.with_alpha(a(255)));
    t.draw(c, f.title, x, y, &label);
    let mark = TextStyle::new(72.0 * s)
        .bold()
        .tracking(6.0 * s)
        .color(theme::TEXT.with_alpha(a(255)));
    let adv = t.draw(c, strings::WORDMARK, x, y + 92.0 * s, &mark);
    c.fill_rect_hgradient(
        Rect::new(x, y + 108.0 * s, adv, 3.0 * s),
        theme::CYAN.with_alpha(a(255)),
        theme::MAGENTA.with_alpha(0),
    );

    let track = Rect::new(
        vp.x + 160.0 * s,
        vp.y + 540.0 * s,
        vp.width - 320.0 * s,
        4.0 * s,
    );
    widgets::sweep_bar(c, &sk, track, f.elapsed, s);
    let dots = ((f.elapsed * 3.0) as usize) % 4;
    let status = format!("{}{}", f.status, ".".repeat(dots));
    t.draw(
        c,
        &status,
        track.x,
        track.y - 16.0 * s,
        &TextStyle::new(13.0 * s).color(theme::MUTED),
    );
    let elapsed = format!("{:.1}s", f.elapsed);
    t.draw_in(
        c,
        &elapsed,
        Rect::new(track.x, track.y - 30.0 * s, track.w, 20.0 * s),
        Align::Right,
        &TextStyle::new(13.0 * s).bold().color(theme::MUTED2),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_frame_is_one_batch() {
        let vp = Viewport::new(1280, 720);
        let mut ui = Ui::new(vp.scale);
        for elapsed in [0.0, 0.1, 0.7, 3.3] {
            ui.begin(1280, 720, vp.scale);
            draw_boot(
                &mut ui,
                &BootFrame {
                    viewport: &vp,
                    elapsed,
                    title: strings::BOOT_TITLE_STARTUP,
                    status: strings::BOOT_STATUS_STARTUP,
                },
            );
            assert_eq!(ui.canvas.debug_batches().len(), 1, "elapsed={elapsed}");
        }
    }
}

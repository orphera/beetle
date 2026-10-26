//! Course result: shown once after the last song of a course (or the song
//! its gauge emptied in). Songs of a course show no result of their own; this
//! screen lists each song, then the course added up.

use super::widgets;
use crate::canvas::Rect;
use crate::strings;
use crate::text::{Align, TextStyle};
use crate::theme::{self, caption};
use crate::ui::Ui;
use crate::view::Viewport;

/// One song row of the result. `None` numbers: the course ended before it.
pub struct CourseStageLine<'a> {
    pub title: &'a str,
    pub played: Option<CourseNumbers>,
}

/// What a song (or the whole course) scored.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CourseNumbers {
    pub ex_score: u32,
    pub max_ex_score: u32,
    /// BAD + POOR + MISS.
    pub misses: u32,
    pub max_combo: u32,
    /// The gauge when the song ended.
    pub gauge: f64,
}

pub struct CourseResultFrame<'a> {
    pub viewport: &'a Viewport,
    pub name: &'a str,
    /// `단위인정` or `코스`.
    pub kind: &'a str,
    /// The dan gauge played (`CLASS`).
    pub gauge_label: &'a str,
    pub passed: bool,
    /// The trophy earned, as the table names it.
    pub trophy: Option<&'a str>,
    pub stages: &'a [CourseStageLine<'a>],
    pub total: CourseNumbers,
    /// Why nothing was saved (auto play), if so.
    pub unsaved_reason: Option<&'a str>,
}

pub fn draw_course_result(ui: &mut Ui, f: &CourseResultFrame) {
    let sk = ui.skin;
    let lite = ui.lite;
    let (c, t) = (&mut ui.canvas, &mut ui.text);
    let fitted = f.viewport.fit_to_design();
    let vp = &fitted;
    let s = vp.scale;
    let accent = if f.passed { theme::GOLD } else { theme::RED };
    widgets::backdrop(c, &sk, vp, accent, ui.clock, lite);

    let panel = Rect::new(
        vp.x + 120.0 * s,
        vp.y + 60.0 * s,
        vp.width - 240.0 * s,
        vp.height - 140.0 * s,
    );
    c.halo(&sk.shadow, panel, theme::WHITE.with_alpha(160));
    c.nine(&sk.panel_lg, panel, theme::SURF1.with_alpha(235));
    let inner = panel.inset(32.0 * s);

    // Heading: kind and gauge, the course name, then the verdict on the right.
    let tag = format!("{}  /  {}", f.kind, f.gauge_label);
    t.draw(
        c,
        &tag,
        inner.x,
        inner.y + 14.0 * s,
        &caption(12.0, s).color(theme::MUTED),
    );
    let verdict = if f.passed {
        strings::COURSE_PASSED()
    } else {
        strings::COURSE_FAILED()
    };
    let verdict_st = TextStyle::new(40.0 * s).bold().color(accent);
    let verdict_w = t.measure(c, verdict, &verdict_st);
    t.draw(
        c,
        verdict,
        inner.right() - verdict_w,
        inner.y + 52.0 * s,
        &verdict_st,
    );
    let name_st = TextStyle::new(30.0 * s).bold().color(theme::TEXT);
    let name = t
        .fit(c, f.name, inner.w - verdict_w - 24.0 * s, &name_st)
        .into_owned();
    t.draw(c, &name, inner.x, inner.y + 52.0 * s, &name_st);
    if let Some(trophy) = f.trophy {
        t.draw_in(
            c,
            trophy,
            Rect::new(inner.x, inner.y + 66.0 * s, inner.w, 20.0 * s),
            Align::Right,
            &TextStyle::new(14.0 * s).bold().color(theme::GOLD),
        );
    }

    // Columns: STAGE, title, EX, misses, combo, gauge.
    let cols = [0.0, 0.10, 0.62, 0.74, 0.84, 0.93].map(|p| inner.x + inner.w * p);
    let head_y = inner.y + 116.0 * s;
    let head = caption(12.0, s).color(theme::MUTED);
    for (x, label) in cols
        .iter()
        .zip(["STAGE", "", "EX SCORE", "MISS", "COMBO", "GAUGE"])
    {
        t.draw(c, label, *x, head_y, &head);
    }
    c.fill_rect(
        Rect::new(inner.x, head_y + 8.0 * s, inner.w, s.max(1.0)),
        theme::LINE,
    );

    let row_h = 40.0 * s;
    let text = TextStyle::new(16.0 * s).color(theme::TEXT);
    let mut y = head_y + 8.0 * s;
    let row = |c: &mut crate::canvas::Canvas,
               t: &mut crate::text::TextEngine,
               y: f32,
               lead: &str,
               title: &str,
               numbers: Option<CourseNumbers>,
               st: &TextStyle| {
        let base = y + row_h / 2.0 + 6.0 * s;
        t.draw(c, lead, cols[0], base, st);
        let title = t
            .fit(c, title, cols[2] - cols[1] - 16.0 * s, st)
            .into_owned();
        t.draw(c, &title, cols[1], base, st);
        match numbers {
            Some(n) => {
                let ex = format!("{} / {}", n.ex_score, n.max_ex_score);
                t.draw(c, &ex, cols[2], base, st);
                t.draw(c, &n.misses.to_string(), cols[3], base, st);
                t.draw(c, &n.max_combo.to_string(), cols[4], base, st);
                t.draw(c, &format!("{:.1}%", n.gauge), cols[5], base, st);
            }
            None => {
                t.draw(
                    c,
                    strings::COURSE_NOT_PLAYED(),
                    cols[2],
                    base,
                    &st.clone().color(theme::MUTED),
                );
            }
        }
    };
    for (i, stage) in f.stages.iter().enumerate() {
        let st = if stage.played.is_some() {
            text.clone()
        } else {
            text.clone().color(theme::MUTED)
        };
        row(
            c,
            t,
            y,
            &(i + 1).to_string(),
            stage.title,
            stage.played,
            &st,
        );
        y += row_h;
    }
    c.fill_rect(
        Rect::new(inner.x, y + 4.0 * s, inner.w, s.max(1.0)),
        theme::LINE,
    );
    let total_st = TextStyle::new(18.0 * s).bold().color(accent);
    row(
        c,
        t,
        y + 8.0 * s,
        "",
        strings::COURSE_TOTAL(),
        Some(f.total),
        &total_st,
    );

    let foot = Rect::new(inner.x, inner.bottom() - 20.0 * s, inner.w, 20.0 * s);
    t.draw_in(
        c,
        strings::COURSE_RESULT_HINT(),
        foot,
        Align::Left,
        &TextStyle::new(13.0 * s).color(theme::MUTED),
    );
    if let Some(reason) = f.unsaved_reason {
        t.draw_in(
            c,
            reason,
            foot,
            Align::Right,
            &TextStyle::new(13.0 * s).bold().color(theme::ORANGE),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_course_with_unplayed_songs_draws_in_one_batch() {
        let vp = Viewport::new(1280, 720);
        let mut ui = Ui::new(vp.scale);
        ui.begin(1280, 720, vp.scale);
        let played = CourseNumbers {
            ex_score: 1500,
            max_ex_score: 2000,
            misses: 30,
            max_combo: 400,
            gauge: 0.0,
        };
        let stages = [
            CourseStageLine {
                title: "First",
                played: Some(played),
            },
            CourseStageLine {
                title: "Second",
                played: None,
            },
        ];
        draw_course_result(
            &mut ui,
            &CourseResultFrame {
                viewport: &vp,
                name: "発狂初段",
                kind: strings::COURSE_DAN(),
                gauge_label: "CLASS",
                passed: false,
                trophy: None,
                stages: &stages,
                total: played,
                unsaved_reason: Some(strings::AUTO_PLAY()),
            },
        );
        assert_eq!(ui.canvas.debug_batches().len(), 1);
    }
}

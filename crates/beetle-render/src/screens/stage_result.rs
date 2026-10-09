//! Stage result screen on the `Ui` (Canvas + TextEngine + generated Skin).
//! Replaces `result.rs`.
//!
//! Three panels: the outcome (rank, clear lamp, final gauge), the score
//! (EX score vs. personal best, judge breakdown) and timing (FAST / SLOW,
//! offset histogram). The rank pops in and the numbers count up during the
//! first `RESULT_REVEAL_DURATION_SECONDS`.

use super::widgets::{self, FOOTER_H, PAD, TOPBAR_H};
use crate::art::Skin;
use crate::canvas::{Canvas, Rect};
use crate::motion::{ease_out_back, ease_out_cubic};
use crate::screens::play::{cover_uv, SizedTexture};
use crate::skin::ColorRgba;
use crate::text::{Align, TextEngine, TextStyle};
use crate::theme::{self, caption, thousands};
use crate::ui::Ui;
use crate::view::Viewport;
use crate::{RANK_POP_SECONDS, SCORE_COUNT_SECONDS};
use beetle_core::{
    BmsChart, ClearType, GaugeType, JudgeGrade, ScoreRecord, ScoreTracker, ScoreUpdate,
};

/// Everything the result screen shows for one frame.
pub struct ResultFrame<'a> {
    pub viewport: &'a Viewport,
    pub chart: &'a BmsChart,
    pub score: &'a ScoreTracker,
    /// Best record before this play (`None` = first play).
    pub previous_best: Option<&'a ScoreRecord>,
    /// Which of the chart's bests this play beat (all false when nothing was saved).
    pub update: ScoreUpdate,
    /// Seconds since the screen was entered (drives the reveal).
    pub elapsed: f64,
    pub jacket: Option<SizedTexture>,
    /// Why the score was not saved ("AUTO PLAY", "REPLAY"), if so.
    pub unsaved_reason: Option<&'a str>,
    /// The long note rule the play was judged under, for a chart with long notes (`LN`, `CN`, `CN (HCN)`).
    pub ln_label: Option<&'a str>,
}

const HINTS: [(&str, &str); 3] = [
    ("ENTER", "SONG SELECT"),
    ("R", "RETRY"),
    ("P", "SCREENSHOT"),
];

pub fn draw_result(ui: &mut Ui, f: &ResultFrame) {
    let sk = ui.skin;
    let lite = ui.lite;
    let (c, t) = (&mut ui.canvas, &mut ui.text);
    let vp = f.viewport;
    let s = vp.scale;
    let clear = f.score.clear_type();
    let (_, lamp_col) = theme::clear_lamp(Some(clear));

    widgets::backdrop(c, &sk, vp, lamp_col, lite);

    let content = Rect::from_ltrb(
        vp.x + PAD * s,
        vp.y + (TOPBAR_H + 24.0) * s,
        vp.x + vp.width - PAD * s,
        vp.y + vp.height - (FOOTER_H + 24.0) * s,
    );
    let gap = 24.0 * s;
    let hero = Rect::new(content.x, content.y, 400.0 * s, content.h);
    let score_panel = Rect::new(hero.right() + gap, content.y, 392.0 * s, content.h);
    let timing = Rect::from_ltrb(
        score_panel.right() + gap,
        content.y,
        content.right(),
        content.bottom(),
    );

    let reveal = ease_out_cubic((f.elapsed / SCORE_COUNT_SECONDS).clamp(0.0, 1.0) as f32);
    outcome_panel(c, t, &sk, f, clear, hero, s);
    score_panel_draw(c, t, &sk, f, score_panel, reveal, s);
    timing_panel(c, t, &sk, f, timing, reveal, s);

    widgets::top_bar(c, t, vp, "RESULT", s);
    let bar = widgets::footer_bar(c, vp, s);
    if let Some(reason) = f.unsaved_reason {
        let st = caption(10.0, s).color(theme::ON_ACCENT);
        let w = t.measure(c, reason, &st) + 20.0 * s;
        let chip = Rect::new(bar.x + PAD * s, bar.y + 10.0 * s, w, 20.0 * s);
        c.nine(&sk.panel_lg, chip, theme::CYAN);
        t.draw_in(c, reason, chip, Align::Center, &st);
        t.draw(
            c,
            "SCORE NOT SAVED",
            chip.right() + 10.0 * s,
            bar.y + 25.0 * s,
            &caption(10.0, s).color(theme::MUTED),
        );
    }
    widgets::footer_hints(c, t, &sk, &HINTS, bar, s);
}

fn panel(c: &mut Canvas, sk: &Skin, r: Rect) {
    c.halo(&sk.shadow, r, theme::WHITE.with_alpha(160));
    c.nine(&sk.panel_lg, r, theme::SURF1.with_alpha(235));
}

/// Rank shown on the result: MAX for a perfect EX score, else the ninths.
fn result_rank(score: &ScoreTracker) -> (&'static str, ColorRgba) {
    if score.max_ex_score() > 0 && score.ex_score >= score.max_ex_score() {
        ("MAX", theme::GOLD)
    } else {
        theme::rank(score.accuracy_rate())
    }
}

fn clear_title(clear: ClearType) -> &'static str {
    match clear {
        ClearType::Perfect => "PERFECT",
        ClearType::FullCombo => "FULL COMBO",
        ClearType::Easy | ClearType::Clear | ClearType::Hard => "STAGE CLEAR",
        ClearType::Failed => "STAGE FAILED",
    }
}

fn gauge_color(score: &ScoreTracker) -> ColorRgba {
    match score.gauge_type {
        _ if !score.is_cleared() => theme::RED,
        GaugeType::Easy => theme::GREEN,
        GaugeType::Groove => theme::CYAN,
        GaugeType::Hard => theme::ORANGE,
        GaugeType::Hazard => theme::MAGENTA,
    }
}

// ---------------------------------------------------------------------------
// Outcome: song, rank, clear lamp, gauge
// ---------------------------------------------------------------------------

fn outcome_panel(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    f: &ResultFrame,
    clear: ClearType,
    p: Rect,
    s: f32,
) {
    panel(c, sk, p);
    let inner = p.inset(24.0 * s);
    let header = &f.chart.header;
    let (tier, tier_col) = theme::level_tier(header.play_level);

    // Song header: jacket thumbnail + tier / title / artist
    let jacket = Rect::new(inner.x, inner.y, 96.0 * s, 72.0 * s);
    match f.jacket {
        Some(tex) => {
            c.fill_rect(jacket, theme::BG);
            c.image(tex.id, jacket, cover_uv(tex, jacket), theme::WHITE);
        }
        None => c.fill_rect_corners(
            jacket,
            [tier_col, theme::SURF3, theme::BG, tier_col.with_alpha(160)],
        ),
    }
    c.stroke_rect(jacket, s.max(1.0), theme::LINE);
    let tx = jacket.right() + 16.0 * s;
    let tw = inner.right() - tx;
    t.draw(
        c,
        &format!("{tier} {}", header.play_level),
        tx,
        inner.y + 14.0 * s,
        &caption(10.0, s).color(tier_col),
    );
    let title_st = TextStyle::new(17.0 * s).bold().color(theme::TEXT);
    let title = t.fit(c, &header.title, tw, &title_st).into_owned();
    t.draw(c, &title, tx, inner.y + 40.0 * s, &title_st);
    let artist_st = TextStyle::new(12.0 * s).color(theme::MUTED);
    let artist = t.fit(c, &header.artist, tw, &artist_st).into_owned();
    t.draw(c, &artist, tx, inner.y + 60.0 * s, &artist_st);
    c.fill_rect(
        Rect::new(inner.x, jacket.bottom() + 24.0 * s, inner.w, s.max(1.0)),
        theme::LINE,
    );

    // Rank: pops in with a little overshoot.
    let (rank, rank_col) = result_rank(f.score);
    let p_rank = (f.elapsed / RANK_POP_SECONDS).clamp(0.0, 1.0) as f32;
    let pop = 1.0 + 0.5 * (1.0 - ease_out_back(p_rank));
    let alpha = ease_out_cubic(p_rank);
    let cx = inner.x + inner.w / 2.0;
    let baseline = p.y + 300.0 * s;
    c.set_additive(true);
    c.sprite_centered(
        sk.glow,
        cx,
        baseline - 44.0 * s,
        340.0 * s,
        220.0 * s,
        rank_col.with_alpha((alpha * 90.0) as u8),
    );
    c.set_additive(false);
    let rank_st = TextStyle::new(120.0 * s)
        .bold()
        .color(rank_col.with_alpha((alpha * 255.0) as u8));
    let w = t.measure(c, rank, &rank_st) * pop;
    t.draw_scaled(c, rank, cx - w / 2.0, baseline, &rank_st, pop);

    // Clear lamp + new record
    let (_, lamp_col) = theme::clear_lamp(Some(clear));
    let status = clear_title(clear);
    let st = TextStyle::new(18.0 * s)
        .bold()
        .tracking(4.0 * s)
        .color(lamp_col);
    let sw = t.measure(c, status, &st);
    t.draw(c, status, cx - sw / 2.0, baseline + 44.0 * s, &st);
    if f.update.lamp {
        new_tag(c, t, cx + sw / 2.0 + 10.0 * s, baseline + 44.0 * s, s);
    }
    if f.update.any() {
        let st = caption(11.0, s).tracking(3.0 * s).color(theme::WHITE);
        let w = t.measure(c, "NEW RECORD", &st) + 40.0 * s;
        let chip = Rect::new(cx - w / 2.0, baseline + 64.0 * s, w, 28.0 * s);
        c.set_additive(true);
        c.sprite_centered(
            sk.glow,
            cx,
            chip.y + chip.h / 2.0,
            w * 1.4,
            70.0 * s,
            theme::MAGENTA.with_alpha(70),
        );
        c.set_additive(false);
        c.nine(&sk.cut_panel, chip, theme::MAGENTA);
        t.draw_in(c, "NEW RECORD", chip, Align::Center, &st);
    }

    // Final gauge
    let score = f.score;
    let gcol = gauge_color(score);
    let bar = Rect::new(inner.x, inner.bottom() - 18.0 * s, inner.w, 18.0 * s);
    let gauge_caption = match f.ln_label {
        Some(rule) => format!("{} GAUGE  ·  {rule}", score.gauge_type.as_str()),
        None => format!("{} GAUGE", score.gauge_type.as_str()),
    };
    t.draw(
        c,
        &gauge_caption,
        inner.x,
        bar.y - 12.0 * s,
        &caption(10.0, s),
    );
    let pct = format!("{:.1}%", score.gauge);
    t.draw_in(
        c,
        &pct,
        Rect::new(inner.x, bar.y - 30.0 * s, inner.w, 24.0 * s),
        Align::Right,
        &TextStyle::new(16.0 * s).bold().color(gcol),
    );
    c.nine(&sk.panel_sm, bar, theme::SURF2);
    let segs = 50;
    let inner_bar = bar.inset(3.0 * s);
    let seg_w = inner_bar.w / segs as f32;
    let lit = ((score.gauge / 100.0).clamp(0.0, 1.0) * segs as f64).round() as usize;
    for i in 0..segs {
        let on = i < lit;
        c.fill_rect(
            Rect::new(
                inner_bar.x + i as f32 * seg_w,
                inner_bar.y,
                seg_w - 1.0,
                inner_bar.h,
            ),
            if on { gcol } else { gcol.with_alpha(28) },
        );
    }
    if matches!(score.gauge_type, GaugeType::Easy | GaugeType::Groove) {
        let x = inner_bar.x + inner_bar.w * 0.8;
        c.fill_rect(
            Rect::new(x - s, bar.y - 4.0 * s, 2.0 * s, bar.h + 8.0 * s),
            theme::GOLD,
        );
    }
}

// ---------------------------------------------------------------------------
// Score: EX score, vs best, stats, judge breakdown
// ---------------------------------------------------------------------------

/// Small "NEW" tag marking a best this play beat; `baseline` is the label's baseline.
fn new_tag(c: &mut Canvas, t: &mut TextEngine, x: f32, baseline: f32, s: f32) {
    let st = caption(8.0, s).color(theme::WHITE);
    let w = t.measure(c, "NEW", &st) + 10.0 * s;
    let tag = Rect::new(x, baseline - 11.0 * s, w, 14.0 * s);
    c.fill_rect(tag, theme::MAGENTA);
    t.draw_in(c, "NEW", tag, Align::Center, &st);
}

fn score_panel_draw(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    f: &ResultFrame,
    p: Rect,
    reveal: f32,
    s: f32,
) {
    panel(c, sk, p);
    let inner = p.inset(24.0 * s);
    let score = f.score;
    let y = inner.y;

    let label_w = t.draw(c, "EX SCORE", inner.x, y + 14.0 * s, &caption(10.0, s));
    if f.update.ex {
        new_tag(c, t, inner.x + label_w + 8.0 * s, y + 14.0 * s, s);
    }
    let shown = (score.ex_score as f32 * reveal).round() as u32;
    let ex_w = t.draw(
        c,
        &thousands(shown),
        inner.x,
        y + 72.0 * s,
        &TextStyle::new(56.0 * s).bold().color(theme::TEXT),
    );
    let max = format!("/ {}", thousands(score.max_ex_score()));
    t.draw(
        c,
        &max,
        inner.x + ex_w + 10.0 * s,
        y + 72.0 * s,
        &TextStyle::new(13.0 * s).color(theme::MUTED2),
    );

    // Versus the previous best
    let right = Rect::new(inner.right() - 140.0 * s, y, 140.0 * s, 24.0 * s);
    match f.previous_best {
        Some(best) => {
            let diff = score.ex_score as i64 - best.ex_score as i64;
            let (txt, col) = match diff {
                d if d > 0 => (format!("+{}", thousands(d as u32)), theme::GREEN),
                0 => ("±0".to_string(), theme::MUTED),
                d => (format!("-{}", thousands((-d) as u32)), theme::RED),
            };
            t.draw_in(
                c,
                &txt,
                Rect::new(right.x, y + 28.0 * s, right.w, 28.0 * s),
                Align::Right,
                &TextStyle::new(22.0 * s).bold().color(col),
            );
            let cap = format!("BEST {}", thousands(best.ex_score));
            t.draw_in(
                c,
                &cap,
                Rect::new(right.x, y + 58.0 * s, right.w, 16.0 * s),
                Align::Right,
                &caption(10.0, s),
            );
        }
        None => {
            t.draw_in(
                c,
                "FIRST PLAY",
                Rect::new(right.x, y + 36.0 * s, right.w, 24.0 * s),
                Align::Right,
                &caption(11.0, s).color(theme::CYAN),
            );
        }
    }

    let (_, rank_col) = result_rank(score);
    let rate = (score.accuracy_rate() / 100.0) as f32 * reveal;
    widgets::rate_bar(
        c,
        t,
        sk,
        Rect::new(inner.x, y + 96.0 * s, inner.w, 4.0 * s),
        rate,
        rank_col,
        s,
    );

    let breaks = score.bad_count + score.poor_count + score.miss_count;
    // (label, value, "/ total" suffix)
    let stats = [
        ("ACCURACY", format!("{:.2}%", score.accuracy_rate()), None),
        (
            "MAX COMBO",
            thousands(score.max_combo),
            Some(format!("/ {}", thousands(score.total_notes))),
        ),
        ("MISS COUNT", thousands(breaks), None),
    ];
    let col_w = inner.w / 3.0;
    for (i, (k, v, suffix)) in stats.iter().enumerate() {
        let sx = inner.x + i as f32 * col_w;
        t.draw(c, k, sx, y + 152.0 * s, &caption(10.0, s));
        let beaten = match i {
            1 => f.update.combo,
            2 => f.update.bp,
            _ => false,
        };
        if beaten {
            // Above the label: the columns are too narrow to fit it beside one.
            new_tag(c, t, sx, y + 138.0 * s, s);
        }
        let vw = t.draw(
            c,
            v,
            sx,
            y + 176.0 * s,
            &TextStyle::new(17.0 * s).bold().color(theme::TEXT),
        );
        if let Some(suffix) = suffix {
            t.draw(
                c,
                suffix,
                sx + vw + 4.0 * s,
                y + 176.0 * s,
                &TextStyle::new(11.0 * s).color(theme::MUTED2),
            );
        }
    }
    c.fill_rect(
        Rect::new(inner.x, y + 200.0 * s, inner.w, s.max(1.0)),
        theme::LINE,
    );

    // Judge breakdown table
    let counts = [
        (JudgeGrade::PerfectGreat, score.pgreat_count),
        (JudgeGrade::Great, score.great_count),
        (JudgeGrade::Good, score.good_count),
        (JudgeGrade::Bad, score.bad_count),
        (JudgeGrade::Poor, score.poor_count),
        (JudgeGrade::Miss, score.miss_count),
    ];
    let total = counts.iter().map(|c| c.1).sum::<u32>().max(1);
    t.draw(c, "JUDGE", inner.x, y + 232.0 * s, &caption(10.0, s));
    let row_h = (inner.bottom() - (y + 248.0 * s)) / counts.len() as f32;
    for (i, (g, n)) in counts.iter().enumerate() {
        let ry = y + 248.0 * s + i as f32 * row_h;
        let col = theme::judge_color(*g);
        let mid = ry + row_h / 2.0;
        c.sprite(
            sk.icons.dot,
            Rect::new(inner.x - 3.0 * s, mid - 6.0 * s, 12.0 * s, 12.0 * s),
            col,
        );
        t.draw_in(
            c,
            theme::judge_label(*g),
            Rect::new(inner.x + 14.0 * s, ry, 80.0 * s, row_h),
            Align::Left,
            &caption(11.0, s).color(theme::MUTED),
        );
        let bar = Rect::new(
            inner.x + 96.0 * s,
            mid - 3.0 * s,
            inner.w - 176.0 * s,
            6.0 * s,
        );
        c.nine(&sk.panel_sm, bar, theme::SURF2);
        let frac = *n as f32 / total as f32 * reveal;
        if frac > 0.0 {
            c.nine(
                &sk.panel_sm,
                Rect::new(bar.x, bar.y, (bar.w * frac).max(bar.h), bar.h),
                col,
            );
        }
        let shown = (*n as f32 * reveal).round() as u32;
        t.draw_in(
            c,
            &thousands(shown),
            Rect::new(inner.right() - 72.0 * s, ry, 72.0 * s, row_h),
            Align::Right,
            &TextStyle::new(16.0 * s).bold().color(theme::TEXT),
        );
    }
}

// ---------------------------------------------------------------------------
// Timing: FAST / SLOW and offset histogram
// ---------------------------------------------------------------------------

fn timing_panel(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    f: &ResultFrame,
    p: Rect,
    reveal: f32,
    s: f32,
) {
    panel(c, sk, p);
    let inner = p.inset(24.0 * s);
    let score = f.score;
    let y = inner.y;

    t.draw(c, "TIMING", inner.x, y + 14.0 * s, &caption(10.0, s));
    let half = Rect::new(inner.x, y + 28.0 * s, inner.w / 2.0, 52.0 * s);
    t.draw(
        c,
        "FAST",
        half.x,
        half.y + 12.0 * s,
        &caption(10.0, s).color(theme::FAST),
    );
    t.draw(
        c,
        &thousands(score.fast_count),
        half.x,
        half.y + 46.0 * s,
        &TextStyle::new(28.0 * s).bold().color(theme::TEXT),
    );
    let rhalf = Rect::new(half.right(), half.y, half.w, half.h);
    t.draw_in(
        c,
        "SLOW",
        Rect::new(rhalf.x, rhalf.y, rhalf.w, 16.0 * s),
        Align::Right,
        &caption(10.0, s).color(theme::SLOW),
    );
    t.draw_in(
        c,
        &thousands(score.slow_count),
        Rect::new(rhalf.x, rhalf.y + 22.0 * s, rhalf.w, 30.0 * s),
        Align::Right,
        &TextStyle::new(28.0 * s).bold().color(theme::TEXT),
    );

    // FAST : SLOW ratio
    let ratio_bar = Rect::new(inner.x, y + 96.0 * s, inner.w, 6.0 * s);
    let fs = (score.fast_count + score.slow_count) as f32;
    if fs > 0.0 {
        let fw = ratio_bar.w * score.fast_count as f32 / fs;
        c.fill_rect(
            Rect::new(ratio_bar.x, ratio_bar.y, fw, ratio_bar.h),
            theme::FAST,
        );
        c.fill_rect(
            Rect::from_ltrb(
                ratio_bar.x + fw,
                ratio_bar.y,
                ratio_bar.right(),
                ratio_bar.bottom(),
            ),
            theme::SLOW,
        );
    } else {
        c.nine(&sk.panel_sm, ratio_bar, theme::SURF2);
    }

    // Offset histogram: 17 buckets, -40 ms (early) .. +40 ms (late).
    t.draw(c, "OFFSET", inner.x, y + 140.0 * s, &caption(10.0, s));
    let hist = Rect::from_ltrb(
        inner.x,
        y + 156.0 * s,
        inner.right(),
        inner.bottom() - 28.0 * s,
    );
    for k in 1..=3 {
        let gy = hist.y + hist.h * k as f32 / 4.0;
        c.fill_rect(
            Rect::new(hist.x, gy, hist.w, s.max(1.0)),
            theme::LINE.with_alpha(110),
        );
    }
    c.fill_rect(
        Rect::new(hist.x, hist.bottom(), hist.w, s.max(1.0)),
        theme::LINE,
    );
    let buckets = &score.timing_histogram;
    let n = buckets.len();
    let center = n / 2;
    let peak = buckets.iter().copied().max().unwrap_or(0).max(1) as f32;
    let slot = hist.w / n as f32;
    for (i, &count) in buckets.iter().enumerate() {
        let h = hist.h * count as f32 / peak * reveal;
        if h <= 0.0 {
            continue;
        }
        let col = match i.cmp(&center) {
            std::cmp::Ordering::Less => theme::FAST,
            std::cmp::Ordering::Equal => theme::GOLD,
            std::cmp::Ordering::Greater => theme::SLOW,
        };
        let r = Rect::new(
            hist.x + i as f32 * slot + 2.0 * s,
            hist.bottom() - h,
            slot - 4.0 * s,
            h,
        );
        c.fill_rect_vgradient(r, col, col.with_alpha(90));
    }
    let cx = hist.x + slot * (center as f32 + 0.5);
    c.fill_rect(
        Rect::new(cx - s / 2.0, hist.y, s.max(1.0), hist.h),
        theme::GOLD.with_alpha(120),
    );
    let label_y = hist.bottom() + 20.0 * s;
    let st = caption(10.0, s);
    t.draw(c, "-40 ms", hist.x, label_y, &st.color(theme::FAST));
    let zw = t.measure(c, "0", &st);
    t.draw(c, "0", cx - zw / 2.0, label_y, &st.color(theme::GOLD));
    t.draw_in(
        c,
        "+40 ms",
        Rect::new(hist.x, label_y - 12.0 * s, hist.w, 16.0 * s),
        Align::Right,
        &st.color(theme::SLOW),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use beetle_core::BmsHeader;

    #[test]
    fn whole_screen_is_one_batch() {
        let vp = Viewport::new(1280, 720);
        let chart = BmsChart {
            header: BmsHeader {
                title: "Test".into(),
                artist: "Tester".into(),
                play_level: 12,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut score = ScoreTracker::new(500, 260.0, GaugeType::Groove);
        for i in 0..500 {
            score.record_hit(if i % 9 == 0 {
                JudgeGrade::Great
            } else {
                JudgeGrade::PerfectGreat
            });
        }
        let mut ui = Ui::new(vp.scale);
        let beaten = ScoreUpdate {
            lamp: true,
            ex: true,
            combo: true,
            bp: true,
        };
        for (elapsed, update) in [(0.0, ScoreUpdate::default()), (0.2, beaten), (5.0, beaten)] {
            ui.begin(1280, 720, vp.scale);
            draw_result(
                &mut ui,
                &ResultFrame {
                    viewport: &vp,
                    chart: &chart,
                    score: &score,
                    previous_best: None,
                    update,
                    elapsed,
                    jacket: None,
                    unsaved_reason: Some("AUTO PLAY"),
                    ln_label: Some("CN (HCN)"),
                },
            );
            assert_eq!(ui.canvas.debug_batches().len(), 1, "elapsed={elapsed}");
        }
    }

    #[test]
    fn perfect_score_ranks_max() {
        let mut score = ScoreTracker::new(10, 200.0, GaugeType::Groove);
        for _ in 0..10 {
            score.record_hit(JudgeGrade::PerfectGreat);
        }
        assert_eq!(result_rank(&score).0, "MAX");
        assert_eq!(score.clear_type(), ClearType::Perfect);
    }
}

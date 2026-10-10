//! Result screen on a real D3D11 device (WARP): a cleared new record with the
//! gauge trend, a failed Hard run with its failure marker, a cleared Hard run,
//! an auto-play (unsaved) perfect, and the play options panel opened over the
//! result. Captures go to `target/result-*.bmp` for layout review.
#![cfg(target_os = "windows")]

mod common;

use beetle_core::{
    BmsChart, BmsHeader, ClearType, GaugeTrend, GaugeType, JudgeGrade, ScoreRecord, ScoreTracker,
    ScoreUpdate,
};
use beetle_render::backend::d3d11::com::D3D_DRIVER_TYPE_WARP;
use beetle_render::strings;
use beetle_render::{
    draw_options_panel, draw_result, D3d11Backend, GpuBackend, OptionLine, OptionsFooter,
    ResultFrame, Ui, Viewport,
};
use common::{write_bmp, HiddenWindow};

const W: u32 = 1280;
const H: u32 = 720;
const SONG_SECONDS: f64 = 240.0;

fn chart() -> BmsChart {
    BmsChart {
        header: BmsHeader {
            title: "冥 -MEI- (Original Mix)".into(),
            artist: "Amuro vs Killer".into(),
            bpm: 190.0,
            play_level: 12,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// `pattern(i)` picks the judgement of note `i`; offsets fill the histogram.
fn play(notes: u32, gauge: GaugeType, pattern: impl Fn(u32) -> JudgeGrade) -> ScoreTracker {
    let mut score = ScoreTracker::new(notes, 260.0, gauge);
    for i in 0..notes {
        let g = pattern(i);
        score.record_hit(g);
        if g != JudgeGrade::Miss {
            let offset = ((i * 7919) % 61) as f64 - 30.0 + if i % 3 == 0 { -6.0 } else { 3.0 };
            let bucket = (((offset + 42.5) / 5.0) as usize).min(16);
            score.timing_histogram[bucket] += 1;
            if offset < -1.0 {
                score.fast_count += 1;
            } else if offset > 1.0 {
                score.slow_count += 1;
            }
        }
    }
    score
}

/// A gauge trend sampled every 50 ms of song time: `gauge_at(t)` until the
/// stage fails at `fail_at` (the gauge reads 0 from then on).
fn trend(gauge_at: impl Fn(f64) -> f64, fail_at: Option<f64>) -> GaugeTrend {
    let mut trend = GaugeTrend::new(SONG_SECONDS);
    let mut t = 0.0;
    while t <= SONG_SECONDS {
        let failed = fail_at.is_some_and(|f| t >= f);
        trend.sample(t, if failed { 0.0 } else { gauge_at(t) }, failed);
        t += 0.05;
    }
    trend
}

fn save(gpu: &mut D3d11Backend, name: &str) {
    let (w, h, px) = gpu.capture_frame().expect("readback");
    gpu.end_frame();
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    write_bmp(&target.join(format!("result-{name}.bmp")), w, h, &px);
}

fn new_record() -> ScoreUpdate {
    ScoreUpdate {
        lamp: true,
        ex: true,
        combo: true,
        bp: false,
    }
}

/// Draws one result frame and returns its draw call count.
#[allow(clippy::too_many_arguments)]
fn render(
    gpu: &mut D3d11Backend,
    ui: &mut Ui,
    score: &ScoreTracker,
    gauge_trend: &GaugeTrend,
    best: Option<&ScoreRecord>,
    update: ScoreUpdate,
    unsaved: Option<&str>,
    name: &str,
) -> usize {
    let vp = Viewport::new(W, H);
    let chart = chart();
    gpu.begin_frame(W, H, [0.0, 0.0, 0.0, 1.0]);
    ui.begin(W, H, vp.scale);
    draw_result(
        ui,
        &ResultFrame {
            viewport: &vp,
            chart: &chart,
            score,
            previous_best: best,
            update,
            elapsed: 3.0,
            jacket: None,
            unsaved_reason: unsaved,
            ln_label: None,
            gauge_trend,
        },
    );
    let calls = ui.end(gpu);
    save(gpu, name);
    calls
}

#[test]
fn result_layouts() {
    let window = HiddenWindow::with_size(W, H);
    let mut gpu = D3d11Backend::with_driver_types(window.0, W, H, &[D3D_DRIVER_TYPE_WARP])
        .expect("WARP device");
    let mut ui = Ui::new(1.0);

    // Groove clear: the trend climbs above the clear line.
    let clear = play(1585, GaugeType::Groove, |i| match i % 37 {
        0..=3 => JudgeGrade::Great,
        4 => JudgeGrade::Good,
        5 if i % 5 == 0 => JudgeGrade::Miss,
        6 if i % 7 == 0 => JudgeGrade::Bad,
        _ => JudgeGrade::PerfectGreat,
    });
    assert_eq!(clear.clear_type(), ClearType::Clear);
    let clear_trend = trend(
        |t| 20.0 + 70.0 * (t / SONG_SECONDS) + 4.0 * (t * 0.3).sin(),
        None,
    );
    let best = ScoreRecord {
        ex_score: clear.ex_score - 37,
        max_combo: 400,
        total_notes: clear.total_notes,
        clear_type: ClearType::Clear,
        ..ScoreRecord::default()
    };
    assert_eq!(
        render(
            &mut gpu,
            &mut ui,
            &clear,
            &clear_trend,
            Some(&best),
            new_record(),
            None,
            "clear"
        ),
        1
    );

    // Hard run that failed at 60 %: the line stops at the red marker.
    let mut failed = play(1585, GaugeType::Hard, |i| {
        if i % 4 == 0 {
            JudgeGrade::Miss
        } else {
            JudgeGrade::Great
        }
    });
    failed.is_failed = true;
    failed.gauge = 0.0;
    let fail_at = 0.6 * SONG_SECONDS;
    let failed_trend = trend(|t| (100.0 - 100.0 * t / fail_at).max(0.0), Some(fail_at));
    // The failure is kept at the first sample at or after the fail time.
    let at = failed_trend.failed_at().expect("failed");
    assert!((at - fail_at).abs() < 0.05, "{at}");
    assert_eq!(
        render(
            &mut gpu,
            &mut ui,
            &failed,
            &failed_trend,
            Some(&best),
            ScoreUpdate::default(),
            None,
            "failed"
        ),
        1
    );

    // Hard run that cleared: the gauge never reached 0, so the fail line is
    // the bottom edge of the graph.
    let hard = play(1585, GaugeType::Hard, |i| match i % 60 {
        0 => JudgeGrade::Bad,
        1 | 2 => JudgeGrade::Great,
        _ => JudgeGrade::PerfectGreat,
    });
    assert!(!hard.is_failed);
    assert_eq!(hard.clear_type(), ClearType::Hard);
    let hard_trend = trend(
        |t| 100.0 - 30.0 * t / SONG_SECONDS + 3.0 * (t * 0.5).cos(),
        None,
    );
    assert_eq!(
        render(
            &mut gpu,
            &mut ui,
            &hard,
            &hard_trend,
            None,
            ScoreUpdate::default(),
            None,
            "hard-clear"
        ),
        1
    );

    let perfect = play(1000, GaugeType::Groove, |_| JudgeGrade::PerfectGreat);
    let perfect_trend = trend(|_| 100.0, None);
    assert_eq!(
        render(
            &mut gpu,
            &mut ui,
            &perfect,
            &perfect_trend,
            None,
            ScoreUpdate::default(),
            Some(strings::AUTO_PLAY),
            "auto"
        ),
        1
    );
}

#[test]
fn options_panel_over_the_result() {
    let window = HiddenWindow::with_size(W, H);
    let mut gpu = D3d11Backend::with_driver_types(window.0, W, H, &[D3D_DRIVER_TYPE_WARP])
        .expect("WARP device");
    let mut ui = Ui::new(1.0);
    let vp = Viewport::new(W, H);
    let clear = play(1585, GaugeType::Groove, |i| {
        if i % 6 == 0 {
            JudgeGrade::Great
        } else {
            JudgeGrade::PerfectGreat
        }
    });
    let clear_trend = trend(|t| 20.0 + 70.0 * (t / SONG_SECONDS), None);
    let chart = chart();
    let line = |section, label, value: &str| OptionLine {
        column: 0,
        section,
        label,
        value: value.to_string(),
    };
    let lines = [
        line(None, strings::ROW_LANE_COVER, "25%"),
        line(None, strings::ROW_MODIFIER, "REGULAR"),
        line(None, strings::ROW_GAUGE, "HARD"),
        line(None, strings::ROW_LN_MODE, "자동 (LN)"),
        line(
            Some(strings::GROUP_SESSION),
            strings::ROW_AUTO_PLAY,
            strings::VALUE_OFF,
        ),
        line(None, strings::ROW_START_MEASURE, "M.0"),
    ];

    gpu.begin_frame(W, H, [0.0, 0.0, 0.0, 1.0]);
    ui.begin(W, H, vp.scale);
    draw_result(
        &mut ui,
        &ResultFrame {
            viewport: &vp,
            chart: &chart,
            score: &clear,
            previous_best: None,
            update: new_record(),
            elapsed: 3.0,
            jacket: None,
            unsaved_reason: None,
            ln_label: None,
            gauge_trend: &clear_trend,
        },
    );
    draw_options_panel(
        &mut ui,
        &vp,
        &lines,
        2,
        (strings::ROW_GAUGE, strings::HELP_GAUGE),
        OptionsFooter::Retry,
    );
    let calls = ui.end(&mut gpu);
    save(&mut gpu, "options");
    assert_eq!(calls, 1, "the panel joins the result's one batch");
}

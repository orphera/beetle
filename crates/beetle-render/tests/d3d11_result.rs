//! Result screen on a real D3D11 device (WARP): a cleared new record, a
//! failed run mid-reveal, and an auto-play (unsaved) perfect. Captures go
//! to `target/result-*.bmp` for layout review.
#![cfg(target_os = "windows")]

mod common;

use beetle_core::{BmsChart, BmsHeader, ClearType, GaugeType, JudgeGrade, ScoreRecord, ScoreTracker};
use beetle_render::backend::d3d11::com::D3D_DRIVER_TYPE_WARP;
use beetle_render::{draw_result, D3d11Backend, GpuBackend, ResultFrame, Ui, Viewport};
use common::{write_bmp, HiddenWindow};

const W: u32 = 1280;
const H: u32 = 720;

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

#[allow(clippy::too_many_arguments)]
fn render(gpu: &mut D3d11Backend, ui: &mut Ui, score: &ScoreTracker, best: Option<&ScoreRecord>, new_record: bool, elapsed: f64, unsaved: Option<&str>, name: &str) -> usize {
    let vp = Viewport::new(W, H);
    let chart = chart();
    gpu.begin_frame(W, H, [0.0, 0.0, 0.0, 1.0]);
    ui.begin(W, H, vp.scale);
    draw_result(
        ui,
        &ResultFrame { viewport: &vp, chart: &chart, score, previous_best: best, new_record, elapsed, jacket: None, unsaved_reason: unsaved },
    );
    let calls = ui.end(gpu);
    let (w, h, px) = gpu.capture_frame().expect("readback");
    gpu.end_frame();
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    write_bmp(&target.join(format!("result-{name}.bmp")), w, h, &px);
    calls
}

#[test]
fn result_layouts() {
    let window = HiddenWindow::with_size(W, H);
    let mut gpu = D3d11Backend::with_driver_types(window.0, W, H, &[D3D_DRIVER_TYPE_WARP]).expect("WARP device");
    let mut ui = Ui::new(1.0);

    let clear = play(1585, GaugeType::Groove, |i| match i % 37 {
        0..=3 => JudgeGrade::Great,
        4 => JudgeGrade::Good,
        5 if i % 5 == 0 => JudgeGrade::Miss,
        6 if i % 7 == 0 => JudgeGrade::Bad,
        _ => JudgeGrade::PerfectGreat,
    });
    assert_eq!(clear.clear_type(), ClearType::Clear);
    let best = ScoreRecord { chart_hash: 1, ex_score: clear.ex_score - 37, max_combo: 400, total_notes: clear.total_notes, clear_type: ClearType::Clear, ..ScoreRecord::default() };
    assert_eq!(render(&mut gpu, &mut ui, &clear, Some(&best), true, 3.0, None, "clear"), 1);

    let mut failed = play(1585, GaugeType::Hard, |i| if i % 4 == 0 { JudgeGrade::Miss } else { JudgeGrade::Great });
    failed.is_failed = true;
    failed.gauge = 0.0;
    assert_eq!(render(&mut gpu, &mut ui, &failed, Some(&best), false, 0.25, None, "failed"), 1);

    let perfect = play(1000, GaugeType::Groove, |_| JudgeGrade::PerfectGreat);
    assert_eq!(render(&mut gpu, &mut ui, &perfect, None, false, 3.0, Some("AUTO PLAY"), "auto"), 1);
}

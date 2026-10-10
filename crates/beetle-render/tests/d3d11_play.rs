//! Gameplay screen on a real D3D11 device (WARP): 7K mid-song (left, center
//! and right playfield, scratch on either side), 7K paused and
//! 14K double play. Captures go to `target/play-*.bmp` for layout review.
#![cfg(target_os = "windows")]

mod common;

use beetle_core::{
    BmsChart, BmsHeader, ClearType, GaugeType, JudgeEngine, JudgeGrade, Lane, NoteEvent, NoteType,
    PlayMode, ScoreTracker, TimingModel,
};
use beetle_render::backend::d3d11::com::D3D_DRIVER_TYPE_WARP;
use beetle_render::{
    draw_gameplay, D3d11Backend, FieldPosition, GpuBackend, HitBurst, JudgeMark, PlayFrame,
    ScratchSide, SkinConfig, Ui, Viewport,
};
use common::{write_bmp, HiddenWindow};

const W: u32 = 1280;
const H: u32 = 720;

/// The same chart with its first `delay` measures empty (about 1.26 s each
/// at 190 BPM), so READY has time to show before the first note.
fn chart_delayed(mode: PlayMode, delay: u32) -> BmsChart {
    let lanes: &[Lane] = match mode {
        PlayMode::Keys14 => &[
            Lane::Scratch,
            Lane::Key1,
            Lane::Key2,
            Lane::Key3,
            Lane::Key4,
            Lane::Key5,
            Lane::Key6,
            Lane::Key7,
            Lane::P2Key1,
            Lane::P2Key3,
            Lane::P2Key5,
            Lane::P2Key7,
            Lane::P2Scratch,
        ],
        PlayMode::Keys5 => &[
            Lane::Scratch,
            Lane::Key1,
            Lane::Key2,
            Lane::Key3,
            Lane::Key4,
            Lane::Key5,
        ],
        PlayMode::Keys4 | PlayMode::Keys6 => mode.restricted_lanes().unwrap(),
        _ => &[
            Lane::Scratch,
            Lane::Key1,
            Lane::Key2,
            Lane::Key3,
            Lane::Key4,
            Lane::Key5,
            Lane::Key6,
            Lane::Key7,
        ],
    };
    let mut notes = Vec::new();
    for m in delay..delay + 40 {
        for k in 0..8u32 {
            let lane = lanes[((m * 3 + k * 5) as usize) % lanes.len()];
            let note_type = if k == 3 && m % 4 == 1 {
                NoteType::LongNoteStart
            } else {
                NoteType::Tap
            };
            notes.push(NoteEvent {
                measure: m,
                fraction: k as f64 / 8.0,
                lane,
                wav_id: None,
                note_type,
            });
            if k == 2 && m % 3 == 2 {
                // A landmine on a lane the tap above does not use.
                let mine_lane = lanes[((m * 3 + k * 5 + 1) as usize) % lanes.len()];
                notes.push(NoteEvent {
                    measure: m,
                    fraction: k as f64 / 8.0,
                    lane: mine_lane,
                    wav_id: Some(beetle_core::WavId(10)),
                    note_type: NoteType::Landmine,
                });
            }
            if note_type == NoteType::LongNoteStart {
                notes.push(NoteEvent {
                    measure: m,
                    fraction: (k as f64 + 2.5) / 8.0,
                    lane,
                    wav_id: None,
                    note_type: NoteType::LongNoteEnd,
                });
            }
        }
    }
    BmsChart {
        header: BmsHeader {
            title: "冥 -MEI- (Original Mix)".into(),
            artist: "Amuro vs Killer".into(),
            bpm: 190.0,
            play_level: 12,
            ..Default::default()
        },
        total_notes_count: notes.len(),
        notes,
        max_measure: 40 + delay,
        has_scratch: true,
        ..Default::default()
    }
}

type Placement = (FieldPosition, ScratchSide);
const LEFT: Placement = (FieldPosition::Left, ScratchSide::Left);

/// HUD pieces beyond the plain frame: a delayed first note (READY), a
/// readout, an end banner, judge marks, and the key hint's opacity.
#[derive(Default)]
struct Extra {
    delay: u32,
    audio_time: Option<f64>,
    readout: Option<(&'static str, f64)>,
    banner: Option<(ClearType, f32)>,
    marks: bool,
    hint_alpha: Option<f32>,
}

fn render(
    gpu: &mut D3d11Backend,
    ui: &mut Ui,
    mode: PlayMode,
    at: Placement,
    pause: Option<usize>,
    name: &str,
) -> usize {
    render_with(gpu, ui, mode, at, pause, name, &Extra::default())
}

fn render_with(
    gpu: &mut D3d11Backend,
    ui: &mut Ui,
    mode: PlayMode,
    at: Placement,
    pause: Option<usize>,
    name: &str,
    extra: &Extra,
) -> usize {
    let vp = Viewport::new(W, H);
    let mut layout = SkinConfig::default();
    layout.update_layout(&vp);
    layout.set_play_mode(mode);
    layout.set_field_layout(at.0, at.1);
    if name.ends_with("triggers") {
        layout.set_eight_k_form(beetle_render::EightKForm::Triggers);
    }
    layout.hi_speed = 700.0;
    let chart = chart_delayed(mode, extra.delay);
    let timing = TimingModel::from_chart(&chart);
    let judge = JudgeEngine::new(&chart, &timing, GaugeType::Groove, beetle_core::Ruleset::CN);
    let mut score = ScoreTracker::new(chart.total_notes_count as u32, 260.0, GaugeType::Groove);
    for i in 0..412 {
        score.record_hit(match i % 23 {
            0 => JudgeGrade::Great,
            7 => JudgeGrade::Good,
            _ => JudgeGrade::PerfectGreat,
        });
    }
    let audio_time = extra.audio_time.unwrap_or(6.0);
    let marks = [
        JudgeMark {
            time: audio_time - 0.4,
            grade: JudgeGrade::PerfectGreat,
            delta_ms: -4.0,
        },
        JudgeMark {
            time: audio_time - 1.1,
            grade: JudgeGrade::Good,
            delta_ms: 55.0,
        },
        JudgeMark {
            time: audio_time - 2.3,
            grade: JudgeGrade::Bad,
            delta_ms: -120.0,
        },
        JudgeMark {
            time: audio_time - 3.0,
            grade: JudgeGrade::Miss,
            delta_ms: 0.0,
        },
        JudgeMark {
            time: audio_time - 4.6,
            grade: JudgeGrade::Great,
            delta_ms: 22.0,
        },
        JudgeMark {
            time: audio_time - 6.2,
            grade: JudgeGrade::PerfectGreat,
            delta_ms: 3.0,
        },
    ];
    let mut keys = [false; 18];
    keys[0] = name.ends_with("triggers");
    keys[2] = true;
    keys[5] = true;
    let bursts = [
        HitBurst {
            lane: Lane::Key2,
            spawn_time: audio_time - 0.05,
            grade: JudgeGrade::PerfectGreat,
        },
        HitBurst {
            lane: Lane::Key5,
            spawn_time: audio_time - 0.12,
            grade: JudgeGrade::Great,
        },
    ];

    gpu.begin_frame(W, H, [0.0, 0.0, 0.0, 1.0]);
    ui.begin(W, H, vp.scale);
    draw_gameplay(
        ui,
        &PlayFrame {
            viewport: &vp,
            layout: &layout,
            chart: &chart,
            notes: judge.notes(),
            timing: &timing,
            score: &score,
            audio_time,
            song_length: 95.0,
            visual_levels: &[
                0.2, 0.5, 0.8, 0.6, 0.9, 0.4, 0.3, 0.7, 0.5, 0.2, 0.6, 0.85, 0.4, 0.3, 0.2, 0.1,
            ],
            bga: None,
            layer: None,
            track_bga_opacity: 0.0,
            key_pressed: &keys,
            hit_bursts: &bursts,
            last_judge: Some((JudgeGrade::PerfectGreat, audio_time - 0.05, -3.0)),
            hint: "키  Shift+S D F Space J K L    1/2 그린 넘버    F10/F11 커버    ESC 일시정지",
            badge: None,
            pause,
            has_bga: false,
            key_hint_alpha: extra.hint_alpha.unwrap_or(1.0),
            readout: extra.readout,
            banner: extra.banner,
            judge_marks: if extra.marks { &marks } else { &[] },
        },
    );
    let calls = ui.end(gpu);
    let (w, h, px) = gpu.capture_frame().expect("readback");
    gpu.end_frame();
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    write_bmp(&target.join(format!("play-{name}.bmp")), w, h, &px);
    calls
}

#[test]
fn gameplay_layouts() {
    let window = HiddenWindow::with_size(W, H);
    let mut gpu = D3d11Backend::with_driver_types(window.0, W, H, &[D3D_DRIVER_TYPE_WARP])
        .expect("WARP device");
    let mut ui = Ui::new(1.0);
    assert_eq!(
        render(&mut gpu, &mut ui, PlayMode::Keys7, LEFT, None, "7k"),
        1
    );
    assert_eq!(
        render(
            &mut gpu,
            &mut ui,
            PlayMode::Keys7,
            (FieldPosition::Center, ScratchSide::Left),
            None,
            "7k-center"
        ),
        1
    );
    assert_eq!(
        render(
            &mut gpu,
            &mut ui,
            PlayMode::Keys7,
            (FieldPosition::Right, ScratchSide::Right),
            None,
            "7k-right"
        ),
        1
    );
    assert_eq!(
        render(
            &mut gpu,
            &mut ui,
            PlayMode::Keys7,
            (FieldPosition::Center, ScratchSide::Right),
            None,
            "7k-center-sr"
        ),
        1
    );
    assert_eq!(
        render(
            &mut gpu,
            &mut ui,
            PlayMode::Keys5,
            (FieldPosition::Right, ScratchSide::Left),
            None,
            "5k-right"
        ),
        1
    );
    assert_eq!(
        render(&mut gpu, &mut ui, PlayMode::Keys7, LEFT, Some(1), "paused"),
        1
    );
    assert_eq!(
        render(
            &mut gpu,
            &mut ui,
            PlayMode::Keys14,
            (FieldPosition::Center, ScratchSide::Right),
            None,
            "14k"
        ),
        1
    );
    assert_eq!(
        render(
            &mut gpu,
            &mut ui,
            PlayMode::Keys4,
            (FieldPosition::Center, ScratchSide::Left),
            None,
            "4k"
        ),
        1
    );
    assert_eq!(
        render(
            &mut gpu,
            &mut ui,
            PlayMode::Keys6,
            (FieldPosition::Center, ScratchSide::Left),
            None,
            "6k"
        ),
        1
    );
    assert_eq!(
        render(
            &mut gpu,
            &mut ui,
            PlayMode::Keys8,
            (FieldPosition::Center, ScratchSide::Left),
            None,
            "8k"
        ),
        1
    );
    assert_eq!(
        render(
            &mut gpu,
            &mut ui,
            PlayMode::Keys8,
            (FieldPosition::Center, ScratchSide::Left),
            None,
            "8k-triggers"
        ),
        1
    );
}

#[test]
fn gameplay_hud_variants() {
    let window = HiddenWindow::with_size(W, H);
    let mut gpu = D3d11Backend::with_driver_types(window.0, W, H, &[D3D_DRIVER_TYPE_WARP])
        .expect("WARP device");
    let mut ui = Ui::new(1.0);
    let center = (FieldPosition::Center, ScratchSide::Left);
    let mut draw =
        |gpu: &mut D3d11Backend, ui: &mut Ui, at: Placement, name: &str, extra: Extra| {
            assert_eq!(
                render_with(gpu, ui, PlayMode::Keys7, at, None, name, &extra),
                1,
                "{name}"
            );
        };
    // No BGA in the chart: the left column becomes the timeline and the
    // score panel, and the centered layout keeps everything on one side.
    draw(
        &mut gpu,
        &mut ui,
        LEFT,
        "7k-nobga-left-timeline",
        Extra {
            marks: true,
            ..Default::default()
        },
    );
    draw(
        &mut gpu,
        &mut ui,
        center,
        "7k-nobga-center",
        Extra {
            marks: true,
            ..Default::default()
        },
    );
    // READY: first note at about 5 s, two seconds in.
    draw(
        &mut gpu,
        &mut ui,
        LEFT,
        "7k-ready",
        Extra {
            delay: 4,
            audio_time: Some(2.0),
            ..Default::default()
        },
    );
    // Readout a third of a second after a green change.
    draw(
        &mut gpu,
        &mut ui,
        LEFT,
        "7k-readout-green",
        Extra {
            readout: Some(("500 ms", 5.7)),
            ..Default::default()
        },
    );
    draw(
        &mut gpu,
        &mut ui,
        LEFT,
        "7k-readout-cover",
        Extra {
            readout: Some(("커버 25%", 5.7)),
            ..Default::default()
        },
    );
    // End banners.
    draw(
        &mut gpu,
        &mut ui,
        LEFT,
        "7k-banner-fullcombo",
        Extra {
            banner: Some((ClearType::FullCombo, 0.35)),
            ..Default::default()
        },
    );
    draw(
        &mut gpu,
        &mut ui,
        LEFT,
        "7k-banner-failed",
        Extra {
            banner: Some((ClearType::Failed, 0.6)),
            ..Default::default()
        },
    );
    // Key hint gone.
    draw(
        &mut gpu,
        &mut ui,
        LEFT,
        "7k-hint-off",
        Extra {
            hint_alpha: Some(0.0),
            ..Default::default()
        },
    );
}

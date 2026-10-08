//! Gameplay screen on a real D3D11 device (WARP): 7K mid-song (left, center
//! and right playfield, scratch on either side), 7K paused and
//! 14K double play. Captures go to `target/play-*.bmp` for layout review.
#![cfg(target_os = "windows")]

mod common;

use beetle_core::{
    BmsChart, BmsHeader, GaugeType, JudgeEngine, JudgeGrade, Lane, NoteEvent, NoteType, PlayMode,
    ScoreTracker, TimingModel,
};
use beetle_render::backend::d3d11::com::D3D_DRIVER_TYPE_WARP;
use beetle_render::{
    draw_gameplay, D3d11Backend, FieldPosition, GpuBackend, HitBurst, PlayFrame, ScratchSide,
    SkinConfig, Ui, Viewport,
};
use common::{write_bmp, HiddenWindow};

const W: u32 = 1280;
const H: u32 = 720;

fn chart(mode: PlayMode) -> BmsChart {
    let lanes: &[Lane] = match mode {
        PlayMode::Keys14 => &[
            Lane::Scratch, Lane::Key1, Lane::Key2, Lane::Key3, Lane::Key4, Lane::Key5, Lane::Key6,
            Lane::Key7, Lane::P2Key1, Lane::P2Key3, Lane::P2Key5, Lane::P2Key7, Lane::P2Scratch,
        ],
        PlayMode::Keys5 => &[Lane::Scratch, Lane::Key1, Lane::Key2, Lane::Key3, Lane::Key4, Lane::Key5],
        PlayMode::Keys4 | PlayMode::Keys6 => mode.restricted_lanes().unwrap(),
        _ => &[Lane::Scratch, Lane::Key1, Lane::Key2, Lane::Key3, Lane::Key4, Lane::Key5, Lane::Key6, Lane::Key7],
    };
    let mut notes = Vec::new();
    for m in 0..40u32 {
        for k in 0..8u32 {
            let lane = lanes[((m * 3 + k * 5) as usize) % lanes.len()];
            let note_type = if k == 3 && m % 4 == 1 { NoteType::LongNoteStart } else { NoteType::Tap };
            notes.push(NoteEvent { measure: m, fraction: k as f64 / 8.0, lane, wav_id: None, note_type });
            if k == 2 && m % 3 == 2 {
                // A landmine on a lane the tap above does not use.
                let mine_lane = lanes[((m * 3 + k * 5 + 1) as usize) % lanes.len()];
                notes.push(NoteEvent { measure: m, fraction: k as f64 / 8.0, lane: mine_lane, wav_id: Some(beetle_core::WavId(10)), note_type: NoteType::Landmine });
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
        max_measure: 40,
        has_scratch: true,
        ..Default::default()
    }
}

type Placement = (FieldPosition, ScratchSide);
const LEFT: Placement = (FieldPosition::Left, ScratchSide::Left);

fn render(gpu: &mut D3d11Backend, ui: &mut Ui, mode: PlayMode, at: Placement, pause: Option<usize>, name: &str) -> usize {
    let vp = Viewport::new(W, H);
    let mut layout = SkinConfig::default();
    layout.update_layout(&vp);
    layout.set_play_mode(mode);
    layout.set_field_layout(at.0, at.1);
    layout.hi_speed = 700.0;
    let chart = chart(mode);
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
    let audio_time = 6.0;
    let mut keys = [false; 18];
    keys[2] = true;
    keys[5] = true;
    let bursts = [
        HitBurst { lane: Lane::Key2, spawn_time: audio_time - 0.05, grade: JudgeGrade::PerfectGreat },
        HitBurst { lane: Lane::Key5, spawn_time: audio_time - 0.12, grade: JudgeGrade::Great },
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
            visual_levels: &[0.2, 0.5, 0.8, 0.6, 0.9, 0.4, 0.3, 0.7, 0.5, 0.2, 0.6, 0.85, 0.4, 0.3, 0.2, 0.1],
            bga: None,
            layer: None,
            track_bga_opacity: 0.0,
            key_pressed: &keys,
            hit_bursts: &bursts,
            last_judge: Some((JudgeGrade::PerfectGreat, audio_time - 0.05, -3.0)),
            hint: "KEYS  Shift+S D F Space J K L    1/2 SPEED    F10/F11 COVER    ESC PAUSE",
            badge: None,
            pause,
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
    assert_eq!(render(&mut gpu, &mut ui, PlayMode::Keys7, LEFT, None, "7k"), 1);
    assert_eq!(render(&mut gpu, &mut ui, PlayMode::Keys7, (FieldPosition::Center, ScratchSide::Left), None, "7k-center"), 1);
    assert_eq!(render(&mut gpu, &mut ui, PlayMode::Keys7, (FieldPosition::Right, ScratchSide::Right), None, "7k-right"), 1);
    assert_eq!(render(&mut gpu, &mut ui, PlayMode::Keys7, (FieldPosition::Center, ScratchSide::Right), None, "7k-center-sr"), 1);
    assert_eq!(render(&mut gpu, &mut ui, PlayMode::Keys5, (FieldPosition::Right, ScratchSide::Left), None, "5k-right"), 1);
    assert_eq!(render(&mut gpu, &mut ui, PlayMode::Keys7, LEFT, Some(1), "paused"), 1);
    assert_eq!(render(&mut gpu, &mut ui, PlayMode::Keys14, (FieldPosition::Center, ScratchSide::Right), None, "14k"), 1);
    assert_eq!(render(&mut gpu, &mut ui, PlayMode::Keys4, (FieldPosition::Center, ScratchSide::Left), None, "4k"), 1);
    assert_eq!(render(&mut gpu, &mut ui, PlayMode::Keys6, (FieldPosition::Center, ScratchSide::Left), None, "6k"), 1);
    assert_eq!(render(&mut gpu, &mut ui, PlayMode::Keys8, (FieldPosition::Center, ScratchSide::Left), None, "8k"), 1);
}

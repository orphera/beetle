//! Song select screen on a real D3D11 device (WARP): the list with a
//! personal best, an empty search result, and both modals. Captures go to
//! `target/select-*.bmp` for layout review.
#![cfg(target_os = "windows")]

mod common;

use beetle_core::{
    ClearType, GaugeType, LaneModifier, PlayMode, PlayResult, ScoreStore, SongMetadata,
};
use beetle_render::backend::d3d11::com::D3D_DRIVER_TYPE_WARP;
use beetle_render::strings;
use beetle_render::{
    draw_exit_modal, draw_options_modal, draw_song_select, D3d11Backend, GpuBackend, SelectFrame,
    Ui, Viewport,
};
use common::{write_bmp, HiddenWindow};

const W: u32 = 1280;
const H: u32 = 720;

fn library() -> Vec<SongMetadata> {
    let entries: [(&str, &str, &str, u32, PlayMode); 15] = [
        ("Aci-L", "裏吉川", "TRANCE", 7, PlayMode::Keys7),
        ("Aci-L -EX-", "裏吉川", "TRANCE", 11, PlayMode::Keys7),
        (
            "AIRSHAVER [7key, Another]",
            "Unknown",
            "HARDCORE",
            12,
            PlayMode::Keys7,
        ),
        (
            "AIRSHAVER [14key, Another]",
            "Unknown",
            "HARDCORE",
            12,
            PlayMode::Keys14,
        ),
        (
            "aliceblue (Radio Edit) (SP ANOTHER)",
            "Ym1024 feat. lamie*",
            "POP",
            10,
            PlayMode::Keys7,
        ),
        (
            "冥 -MEI- (Original Mix)",
            "Amuro vs Killer",
            "HARD TRANCE",
            12,
            PlayMode::Keys7,
        ),
        (
            "가을밤의 신호등",
            "모래시계 사운드",
            "K-POP",
            4,
            PlayMode::Keys5,
        ),
        (
            "Chrono Diver -PENDULUMs-",
            "Sound Holic",
            "DRUM'N'BASS",
            9,
            PlayMode::Keys7,
        ),
        (
            "Concertino in Blue",
            "Tatsh",
            "CLASSICAL",
            8,
            PlayMode::Keys7,
        ),
        (
            "Love & Justice ~Endless Summer Night Extended Mix~",
            "BACO",
            "SPEEDCORE",
            13,
            PlayMode::Keys7,
        ),
        ("MilK", "moe", "ELECTRO", 6, PlayMode::Keys7),
        (
            "Blue-White Crazystars [6K]",
            "beta",
            "HAPPY HARDCORE",
            12,
            PlayMode::Keys6,
        ),
        ("Black Lotus [8K]", "Wa.", "TRANCE", 10, PlayMode::Keys8),
        ("Bahamut [4K]", "UE", "TRANCE", 11, PlayMode::Keys4),
        (
            "PrayStation (HD Edit)",
            "Ras",
            "BREAKCORE",
            11,
            PlayMode::Keys14,
        ),
    ];
    entries
        .iter()
        .enumerate()
        .map(|(i, (title, artist, genre, level, mode))| {
            // Tempo ranges as the real library has them: an ordinary wide one (5)
            // and the extreme one that a BPM-change command can produce (9).
            let (lo, hi) = match i {
                5 => (158.0, 246.0),
                9 => (197.0, 19_700_197.0),
                _ => {
                    let bpm = 140.0 + i as f64 * 7.0;
                    (bpm, bpm)
                }
            };
            SongMetadata {
                id: beetle_core::ChartId::synthetic(i as u64 + 1),
                md5: [0; 16],
                ln_count: 0,
                ln_mode: None,
                legacy_hash: i as u64 + 1,
                file_path: format!("songs/{i}.bms"),
                title: (*title).into(),
                subtitle: String::new(),
                artist: (*artist).into(),
                genre: (*genre).into(),
                bpm: lo,
                bpm_min: lo,
                bpm_max: hi,
                play_level: *level,
                notes_count: 900 + i * 137,
                play_mode: *mode,
            }
        })
        .collect()
}

fn scores() -> ScoreStore {
    let mut store = ScoreStore::default();
    for (hash, rate, clear) in [
        (1, 72.0, ClearType::Failed),
        (3, 81.0, ClearType::Clear),
        (5, 91.5, ClearType::FullCombo),
        (6, 93.4, ClearType::Clear),
        (8, 100.0, ClearType::Perfect),
    ] {
        let notes = 900 + (hash as u32 - 1) * 137;
        store.update(PlayResult {
            chart: beetle_core::ChartId::synthetic(hash),
            ln: None,
            lamp: clear,
            ex_score: (notes as f64 * 2.0 * rate / 100.0) as u32,
            max_combo: notes * 2 / 3,
            pgreat_count: notes * 8 / 10,
            great_count: notes / 10,
            good_count: 12,
            bad_count: 4,
            poor_count: 3,
            miss_count: 9,
            total_notes: notes,
            modifier: LaneModifier::Regular,
            gauge: GaugeType::Groove,
            random_seed: None,
            played_at: 0,
        });
    }
    store
}

#[derive(Clone, Copy)]
enum Overlay {
    None,
    Options,
    Exit,
}

fn render(
    gpu: &mut D3d11Backend,
    ui: &mut Ui,
    selected: usize,
    search: &str,
    preedit: &str,
    overlay: Overlay,
    name: &str,
) -> usize {
    let vp = Viewport::new(W, H);
    let songs = library();
    // Two tables: song 6 (the highlighted one) is in both, a few others in the first.
    let entry = |n: u64, level: &str| beetle_core::TableEntry {
        level: level.into(),
        sha256: Some(beetle_core::ChartId::synthetic(n)),
        ..Default::default()
    };
    let table = |name: &str, symbol: &str, entries: Vec<beetle_core::TableEntry>| {
        beetle_core::DifficultyTable {
            name: name.into(),
            symbol: symbol.into(),
            entries,
            ..Default::default()
        }
    };
    let mut tables = beetle_core::TableIndex::new(vec![
        table(
            "Satellite",
            "sl",
            vec![entry(6, "3"), entry(2, "1"), entry(4, "12")],
        ),
        table("Stella", "st", vec![entry(6, "5")]),
    ]);
    tables.match_songs(songs.iter().map(|s| (s.id, s.md5)));
    let visible: Vec<usize> = (0..songs.len())
        .filter(|&i| search.is_empty() || songs[i].title.contains(search))
        .collect();
    let scores = scores();
    let chips = vec![
        format!("{} 1100", strings::ROW_HI_SPEED),
        "REGULAR".into(),
        "GROOVE".into(),
    ];

    gpu.begin_frame(W, H, [0.0, 0.0, 0.0, 1.0]);
    ui.begin(W, H, vp.scale);
    draw_song_select(
        ui,
        &SelectFrame {
            viewport: &vp,
            songs: &songs,
            visible: &visible,
            selected,
            scores: &scores,
            tables: &tables,
            ln_option: beetle_core::LnOption::Auto,
            // The longest kind of folder title: a difficulty table with its owned count.
            folder: if name == "noplay" {
                "A TABLE WITH A VERY…  1,234 / 12,345"
            } else {
                strings::FOLDER_ALL
            },
            sort: strings::SORT_TITLE,
            search,
            search_active: !search.is_empty() || !preedit.is_empty(),
            preedit,
            jacket: None,
            ambient: None,
            option_chips: &chips,
            auto_play: false,
            has_replay: selected == 5,
            preview_secs: (selected == 5 && search.is_empty()).then_some(0.4),
        },
    );
    match overlay {
        Overlay::None => {}
        Overlay::Options => {
            let rows: Vec<(&str, String)> = [
                (strings::ROW_HI_SPEED, "1100 px/s"),
                (strings::ROW_MODIFIER, "REGULAR"),
                (strings::ROW_GAUGE, "GROOVE"),
                (strings::ROW_LN_MODE, "AUTO (LN)"),
                (strings::ROW_JUDGE_OFFSET, "+0 ms"),
                (strings::ROW_MASTER_VOLUME, "80%"),
                (strings::ROW_PLAYFIELD, strings::VALUE_CENTER),
                (strings::SCRATCH, strings::SIDE_RIGHT),
                (strings::ROW_TRACK_BGA, strings::TRACK_BGA_OFF),
                (strings::ROW_DISPLAY_MODE, strings::DISPLAY_WINDOWED),
                (strings::ROW_RESOLUTION, "1280 x 720"),
                (strings::ROW_GRAPHICS, "WARP (CPU) (재시작 후 적용)"),
                (strings::ROW_TARGET_FPS, strings::VALUE_UNLIMITED),
                (strings::ROW_KEY_LAYOUT, "7K  HOME ROW"),
                (strings::ROW_AUTO_PLAY, strings::VALUE_OFF),
                (strings::ROW_START_MEASURE, "M.0"),
            ]
            .iter()
            .map(|(k, v)| (*k, v.to_string()))
            .collect();
            draw_options_modal(ui, &vp, &rows, 2);
        }
        Overlay::Exit => draw_exit_modal(ui, &vp),
    }
    let calls = ui.end(gpu);
    let (w, h, px) = gpu.capture_frame().expect("readback");
    gpu.end_frame();
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    write_bmp(&target.join(format!("select-{name}.bmp")), w, h, &px);
    calls
}

#[test]
fn song_select_layouts() {
    let window = HiddenWindow::with_size(W, H);
    let mut gpu = D3d11Backend::with_driver_types(window.0, W, H, &[D3D_DRIVER_TYPE_WARP])
        .expect("WARP device");
    let mut ui = Ui::new(1.0);
    assert_eq!(
        render(&mut gpu, &mut ui, 5, "", "", Overlay::None, "list"),
        1
    );
    assert_eq!(
        render(&mut gpu, &mut ui, 9, "", "", Overlay::None, "noplay"),
        1
    );
    assert_eq!(
        render(&mut gpu, &mut ui, 0, "zzz", "", Overlay::None, "empty"),
        1
    );
    assert_eq!(
        render(&mut gpu, &mut ui, 13, "", "", Overlay::None, "ue-modes"),
        1
    );
    // Korean query with an IME composition in progress (underlined, caret after it).
    assert_eq!(
        render(
            &mut gpu,
            &mut ui,
            0,
            "가을",
            "밤",
            Overlay::None,
            "ime-preedit"
        ),
        1
    );
    assert_eq!(
        render(&mut gpu, &mut ui, 5, "", "", Overlay::Options, "options"),
        1
    );
    assert_eq!(
        render(&mut gpu, &mut ui, 5, "", "", Overlay::Exit, "exit"),
        1
    );

    // Frame cost on WARP (the low-end fallback), full and lite.
    for lite in [false, true] {
        ui.lite = lite;
        let t0 = std::time::Instant::now();
        for _ in 0..60 {
            render(&mut gpu, &mut ui, 5, "", "", Overlay::None, "perf");
        }
        eprintln!(
            "WARP lite={lite}: {:.2} ms/frame (incl. readback)",
            t0.elapsed().as_secs_f64() * 1000.0 / 60.0
        );
    }
}

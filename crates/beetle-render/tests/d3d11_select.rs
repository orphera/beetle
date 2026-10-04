//! Song select screen on a real D3D11 device (WARP): the list with a
//! personal best, an empty search result, and both modals. Captures go to
//! `target/select-*.bmp` for layout review.
#![cfg(target_os = "windows")]

mod common;

use beetle_core::{ClearType, PlayMode, ScoreRecord, ScoreStore, SongMetadata};
use beetle_render::backend::d3d11::com::D3D_DRIVER_TYPE_WARP;
use beetle_render::{
    draw_exit_modal, draw_options_modal, draw_song_select, D3d11Backend, GpuBackend, SelectFrame,
    Ui, Viewport,
};
use common::{write_bmp, HiddenWindow};

const W: u32 = 1280;
const H: u32 = 720;

fn library() -> Vec<SongMetadata> {
    let entries: [(&str, &str, &str, u32, PlayMode); 12] = [
        ("Aci-L", "裏吉川", "TRANCE", 7, PlayMode::Keys7),
        ("Aci-L -EX-", "裏吉川", "TRANCE", 11, PlayMode::Keys7),
        ("AIRSHAVER [7key, Another]", "Unknown", "HARDCORE", 12, PlayMode::Keys7),
        ("AIRSHAVER [14key, Another]", "Unknown", "HARDCORE", 12, PlayMode::Keys14),
        ("aliceblue (Radio Edit) (SP ANOTHER)", "Ym1024 feat. lamie*", "POP", 10, PlayMode::Keys7),
        ("冥 -MEI- (Original Mix)", "Amuro vs Killer", "HARD TRANCE", 12, PlayMode::Keys7),
        ("가을밤의 신호등", "모래시계 사운드", "K-POP", 4, PlayMode::Keys5),
        ("Chrono Diver -PENDULUMs-", "Sound Holic", "DRUM'N'BASS", 9, PlayMode::Keys7),
        ("Concertino in Blue", "Tatsh", "CLASSICAL", 8, PlayMode::Keys7),
        ("Love & Justice ~Endless Summer Night Extended Mix~", "BACO", "SPEEDCORE", 13, PlayMode::Keys7),
        ("MilK", "moe", "ELECTRO", 6, PlayMode::Keys7),
        ("PrayStation (HD Edit)", "Ras", "BREAKCORE", 11, PlayMode::Keys14),
    ];
    entries
        .iter()
        .enumerate()
        .map(|(i, (title, artist, genre, level, mode))| SongMetadata {
            hash: i as u64 + 1,
            file_path: format!("songs/{i}.bms"),
            title: (*title).into(),
            subtitle: String::new(),
            artist: (*artist).into(),
            genre: (*genre).into(),
            bpm: 140.0 + i as f64 * 7.0,
            play_level: *level,
            notes_count: 900 + i * 137,
            play_mode: *mode,
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
        store.update(ScoreRecord {
            chart_hash: hash,
            ex_score: (notes as f64 * 2.0 * rate / 100.0) as u32,
            max_combo: notes * 2 / 3,
            accuracy_rate: rate,
            clear_type: clear,
            pgreat_count: notes * 8 / 10,
            great_count: notes / 10,
            good_count: 12,
            bad_count: 4,
            poor_count: 3,
            miss_count: 9,
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

fn render(gpu: &mut D3d11Backend, ui: &mut Ui, selected: usize, search: &str, overlay: Overlay, name: &str) -> usize {
    let vp = Viewport::new(W, H);
    let songs = library();
    let visible: Vec<usize> = if search.is_empty() {
        (0..songs.len()).collect()
    } else {
        Vec::new()
    };
    let scores = scores();
    let chips = vec!["HI-SPEED 1100".to_string(), "REGULAR".into(), "GROOVE".into()];

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
            folder: "ALL SONGS",
            sort: "TITLE",
            search,
            search_active: !search.is_empty(),
            jacket: None,
            ambient: None,
            option_chips: &chips,
            auto_play: false,
            has_replay: selected == 5,
        },
    );
    match overlay {
        Overlay::None => {}
        Overlay::Options => {
            let rows: Vec<(&str, String)> = [
                ("HI-SPEED", "1100 px/s"),
                ("MODIFIER", "REGULAR"),
                ("GAUGE", "GROOVE"),
                ("JUDGE OFFSET", "+0 ms"),
                ("MASTER VOLUME", "80%"),
                ("DISPLAY MODE", "WINDOWED"),
                ("RESOLUTION", "1280 x 720"),
                ("GRAPHICS GPU", "AUTO"),
                ("TARGET FPS", "UNLIMITED"),
                ("KEY LAYOUT", "HOME ROW"),
                ("AUTO PLAY", "OFF"),
                ("START MEASURE", "M.0"),
                ("TRACK BGA", "OFF (0%)"),
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
    assert_eq!(render(&mut gpu, &mut ui, 5, "", Overlay::None, "list"), 1);
    assert_eq!(render(&mut gpu, &mut ui, 9, "", Overlay::None, "noplay"), 1);
    assert_eq!(render(&mut gpu, &mut ui, 0, "zzz", Overlay::None, "empty"), 1);
    assert_eq!(render(&mut gpu, &mut ui, 5, "", Overlay::Options, "options"), 1);
    assert_eq!(render(&mut gpu, &mut ui, 5, "", Overlay::Exit, "exit"), 1);

    // Frame cost on WARP (the low-end fallback), full and lite.
    for lite in [false, true] {
        ui.lite = lite;
        let t0 = std::time::Instant::now();
        for _ in 0..60 {
            render(&mut gpu, &mut ui, 5, "", Overlay::None, "perf");
        }
        eprintln!("WARP lite={lite}: {:.2} ms/frame (incl. readback)", t0.elapsed().as_secs_f64() * 1000.0 / 60.0);
    }
}

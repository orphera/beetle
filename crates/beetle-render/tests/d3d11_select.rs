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
    draw_drop_overlay, draw_exit_modal, draw_help_overlay, draw_options_modal, draw_screen_fade,
    draw_song_select, draw_toast, D3d11Backend, FilterBar, GpuBackend, OptionLine, SelectFrame,
    SelectRow, SortMenu, ToastAnchor, ToastFrame, ToastKind, Ui, Viewport,
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
    /// A success toast, fully shown.
    Toast,
    /// The info toast for one uninstalled package, fully shown.
    UninstalledOne,
    /// The info toast for several uninstalled packages, fully shown.
    UninstalledMany,
    /// The fade-in halfway through: the background covers half of the frame.
    Fade,
    /// The help overlay (?) over the list.
    Help,
    /// The drag overlay: a file is held over the window.
    Drop,
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
    render_with(gpu, ui, selected, search, preedit, overlay, name, None)
}

/// What the filter row, the result count and the sort menu show on top of the list.
#[derive(Default)]
struct Extra<'a> {
    filter: FilterBar<'a>,
    result_count: Option<usize>,
    sort_menu: Option<SortMenu<'a>>,
    /// The library has no songs of its own: the first-run guide shows.
    library_empty: bool,
}

/// `folder` replaces the flat song list with a folder view: the rows and the
/// breadcrumb the app would build for it (the tree itself lives in beetle-app).
#[allow(clippy::too_many_arguments)]
fn render_with(
    gpu: &mut D3d11Backend,
    ui: &mut Ui,
    selected: usize,
    search: &str,
    preedit: &str,
    overlay: Overlay,
    name: &str,
    folder: Option<(Vec<SelectRow<'static>>, Vec<String>)>,
) -> usize {
    render_ex(
        gpu,
        ui,
        selected,
        search,
        preedit,
        overlay,
        name,
        folder,
        Extra::default(),
    )
}

#[allow(clippy::too_many_arguments)]
fn render_ex(
    gpu: &mut D3d11Backend,
    ui: &mut Ui,
    selected: usize,
    search: &str,
    preedit: &str,
    overlay: Overlay,
    name: &str,
    folder: Option<(Vec<SelectRow<'static>>, Vec<String>)>,
    extra: Extra<'_>,
) -> usize {
    let vp = Viewport::new(W, H);
    // An empty library has no songs at all (the first-run guide case).
    let songs = if extra.library_empty {
        Vec::new()
    } else {
        library()
    };
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
    let (rows, crumbs): (Vec<SelectRow>, Vec<String>) = match folder {
        Some((rows, crumbs)) => (rows, crumbs),
        None => {
            let rows = (0..songs.len())
                .filter(|&i| search.is_empty() || songs[i].title.contains(search))
                .map(SelectRow::Song)
                .collect();
            (
                rows,
                vec![strings::FOLDER_ROOT.into(), strings::FOLDER_ALL.into()],
            )
        }
    };
    let scores = scores();
    let chips = vec![
        strings::fill(strings::CHIP_GREEN, &["500"]),
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
            rows: &rows,
            selected,
            scroll: 0,
            scores: &scores,
            tables: &tables,
            ln_option: beetle_core::LnOption::Auto,
            crumbs: &crumbs,
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
            filter: extra.filter,
            result_count: extra.result_count,
            sort_menu: extra.sort_menu,
            library_empty: extra.library_empty,
        },
    );
    match overlay {
        Overlay::None => {}
        Overlay::Options => {
            let line = |section, label, value: &str| OptionLine {
                column: 0,
                section,
                label,
                value: value.to_string(),
            };
            let lines = [
                line(Some(strings::GROUP_PLAY), strings::ROW_GREEN, "500 ms"),
                line(None, strings::ROW_LANE_COVER, "25%"),
                line(None, strings::ROW_MODIFIER, "REGULAR"),
                line(None, strings::ROW_GAUGE, "GROOVE"),
                line(None, strings::ROW_LN_MODE, "자동 (LN)"),
                line(
                    Some(strings::GROUP_SESSION),
                    strings::ROW_AUTO_PLAY,
                    strings::VALUE_OFF,
                ),
                line(None, strings::ROW_START_MEASURE, "M.0"),
            ];
            draw_options_modal(
                ui,
                &vp,
                &lines,
                2,
                (strings::ROW_GAUGE, strings::HELP_GAUGE),
            );
        }
        Overlay::Exit => draw_exit_modal(ui, &vp),
        Overlay::Toast => {
            let text = strings::fill(strings::TOAST_RESCAN_DONE, &["412"]);
            draw_toast(
                ui,
                &vp,
                &ToastFrame {
                    text: &text,
                    kind: ToastKind::Success,
                    alpha: 1.0,
                    slide: 1.0,
                    anchor: ToastAnchor::Footer,
                },
            )
        }
        Overlay::UninstalledOne | Overlay::UninstalledMany => {
            let text = if matches!(overlay, Overlay::UninstalledOne) {
                strings::fill(strings::TOAST_UNINSTALLED_ONE, &["AIRSHAVER.bmsp"])
            } else {
                strings::fill(strings::TOAST_UNINSTALLED_MANY, &["3", "AIRSHAVER.bmsp"])
            };
            draw_toast(
                ui,
                &vp,
                &ToastFrame {
                    text: &text,
                    kind: ToastKind::Info,
                    alpha: 1.0,
                    slide: 1.0,
                    anchor: ToastAnchor::Footer,
                },
            )
        }
        Overlay::Fade => draw_screen_fade(ui, 0.5),
        Overlay::Help => draw_help_overlay(ui, &vp),
        Overlay::Drop => draw_drop_overlay(ui, &vp),
    }
    let calls = ui.end(gpu);
    let (w, h, px) = gpu.capture_frame().expect("readback");
    gpu.end_frame();
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    write_bmp(&target.join(format!("select-{name}.bmp")), w, h, &px);
    calls
}

/// The folder views: the root, a branch with a breadcrumb, and a leaf of songs with one.
#[test]
fn song_select_folder_views() {
    let window = HiddenWindow::with_size(W, H);
    let mut gpu = D3d11Backend::with_driver_types(window.0, W, H, &[D3D_DRIVER_TYPE_WARP])
        .expect("WARP device");
    let mut ui = Ui::new(1.0);
    let total = library().len();
    let folder = |label: &'static str, count: usize| SelectRow::Folder {
        label,
        count,
        lamps: [0; beetle_render::LAMP_COUNT],
    };

    // Root: the top-level folders (난이도표 is listed last). The highlight is on 레벨.
    let root = vec![
        folder(strings::FOLDER_ALL, total),
        folder(strings::FOLDER_MODE, total),
        folder(strings::FOLDER_LEVEL, total),
        folder(strings::FOLDER_LAMP, total),
        folder(strings::FOLDER_TABLE, 4),
    ];
    assert_eq!(
        render_with(
            &mut gpu,
            &mut ui,
            2,
            "",
            "",
            Overlay::None,
            "folder-root",
            Some((root, vec![strings::FOLDER_ROOT.into()])),
        ),
        1
    );

    // A branch under 레벨 with a breadcrumb; the highlight is on 12.
    let levels = vec![
        folder("5", 1),
        folder("9", 1),
        folder("12", 4),
        folder("13", 2),
    ];
    assert_eq!(
        render_with(
            &mut gpu,
            &mut ui,
            2,
            "",
            "",
            Overlay::None,
            "folder-level",
            Some((
                levels,
                vec![strings::FOLDER_ROOT.into(), strings::FOLDER_LEVEL.into()],
            )),
        ),
        1
    );

    // A leaf: its songs under a three-part breadcrumb (전체 > 레벨 > 12).
    let songs: Vec<SelectRow<'static>> = (0..total).map(SelectRow::Song).collect();
    assert_eq!(
        render_with(
            &mut gpu,
            &mut ui,
            5,
            "",
            "",
            Overlay::None,
            "folder-leaf",
            Some((
                songs,
                vec![
                    strings::FOLDER_ROOT.into(),
                    strings::FOLDER_LEVEL.into(),
                    "12".into(),
                ],
            )),
        ),
        1
    );
    // A leaf with a group: the two AIRSHAVER charts share one row (charts by level,
    // then title: 14K first). The chosen chart is the 7K one, then the 14K one.
    for (pos, name) in [(1, "select-group-7k"), (0, "select-group-14k")] {
        let rows: Vec<SelectRow<'static>> = vec![
            SelectRow::Group {
                title: "AIRSHAVER",
                charts: &[3, 2],
                selected: pos,
            },
            SelectRow::Song(0),
            SelectRow::Song(1),
            SelectRow::Song(4),
            SelectRow::Song(5),
        ];
        assert_eq!(
            render_with(
                &mut gpu,
                &mut ui,
                0,
                "",
                "",
                Overlay::None,
                name,
                Some((
                    rows,
                    vec![
                        strings::FOLDER_ROOT.into(),
                        strings::FOLDER_LEVEL.into(),
                        "12".into(),
                    ],
                )),
            ),
            1
        );
    }
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

    // Toast over the list (success), and the fade-in halfway through.
    assert_eq!(
        render(&mut gpu, &mut ui, 5, "", "", Overlay::Toast, "toast"),
        1
    );
    assert_eq!(
        render(&mut gpu, &mut ui, 5, "", "", Overlay::Fade, "fade-mid"),
        1
    );

    // Uninstalled-package notices (info), one file and several.
    assert_eq!(
        render(
            &mut gpu,
            &mut ui,
            5,
            "",
            "",
            Overlay::UninstalledOne,
            "uninstalled-one"
        ),
        1
    );
    assert_eq!(
        render(
            &mut gpu,
            &mut ui,
            5,
            "",
            "",
            Overlay::UninstalledMany,
            "uninstalled-many"
        ),
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

#[test]
fn song_select_filter_and_sort_views() {
    let window = HiddenWindow::with_size(W, H);
    let mut gpu = D3d11Backend::with_driver_types(window.0, W, H, &[D3D_DRIVER_TYPE_WARP])
        .expect("WARP device");
    let mut ui = Ui::new(1.0);
    let songs = library();
    let crumbs = vec![
        strings::FOLDER_ROOT.to_string(),
        strings::FOLDER_ALL.to_string(),
    ];
    let sort_names = [
        strings::SORT_TITLE,
        strings::SORT_LEVEL,
        strings::SORT_CLEAR_LAMP,
        strings::SORT_SCORE_RATE,
        strings::SORT_BPM,
    ];

    // The sort menu open over the list; the highlight is on 클리어 and the mode in use is 제목.
    let menu = SortMenu {
        options: &sort_names,
        current: 0,
        highlight: 2,
    };
    assert_eq!(
        render_ex(
            &mut gpu,
            &mut ui,
            5,
            "",
            "",
            Overlay::None,
            "sort-menu",
            None,
            Extra {
                sort_menu: Some(menu),
                ..Default::default()
            },
        ),
        1
    );

    // Filter on: 7K only, level 12 and up, unplayed only. The list is the 7K songs that pass.
    let chips = [("5K", false), ("7K", true), ("14K", false)];
    let passing: Vec<SelectRow<'static>> = (0..songs.len())
        .filter(|&i| songs[i].play_mode == PlayMode::Keys7 && songs[i].play_level >= 12)
        .map(SelectRow::Song)
        .collect();
    let passing_count = passing.len();
    for (focus, name) in [(None, "filter-chips"), (Some(1), "filter-focus")] {
        let filter = FilterBar {
            modes: &chips,
            level_min: Some(12),
            level_max: None,
            unplayed: true,
            uncleared: false,
            active: true,
            focus,
        };
        assert_eq!(
            render_ex(
                &mut gpu,
                &mut ui,
                0,
                "",
                "",
                Overlay::None,
                name,
                Some((passing.clone(), crumbs.clone())),
                Extra {
                    filter,
                    result_count: Some(passing_count),
                    ..Default::default()
                },
            ),
            1
        );
    }

    // A search with the count of its matches, no filter.
    let hits = songs.iter().filter(|s| s.title.contains('a')).count();
    assert_eq!(
        render_ex(
            &mut gpu,
            &mut ui,
            0,
            "a",
            "",
            Overlay::None,
            "search-count",
            None,
            Extra {
                result_count: Some(hits),
                ..Default::default()
            },
        ),
        1
    );

    // A filter that leaves nothing: the empty state names the filter and offers 초기화.
    let empty_filter = FilterBar {
        modes: &chips,
        level_min: None,
        level_max: Some(1),
        unplayed: false,
        uncleared: true,
        active: true,
        focus: None,
    };
    assert_eq!(
        render_ex(
            &mut gpu,
            &mut ui,
            0,
            "",
            "",
            Overlay::None,
            "empty-filter",
            Some((Vec::new(), crumbs.clone())),
            Extra {
                filter: empty_filter,
                result_count: Some(0),
                ..Default::default()
            },
        ),
        1
    );
}

/// U3d: the slim footer with the ? key, the help overlay, the drag overlay, the
/// first-run guide of an empty library, and a folder's clear lamp breakdown.
#[test]
fn song_select_u3d_views() {
    let window = HiddenWindow::with_size(W, H);
    let mut gpu = D3d11Backend::with_driver_types(window.0, W, H, &[D3D_DRIVER_TYPE_WARP])
        .expect("WARP device");
    let mut ui = Ui::new(1.0);
    let crumbs = vec![
        strings::FOLDER_ROOT.to_string(),
        strings::FOLDER_ALL.to_string(),
    ];

    // The help overlay (? over the list). Over a full list its text passes the
    // backend's per-batch vertex limit, so the frame takes two draw calls.
    let help = render(&mut gpu, &mut ui, 5, "", "", Overlay::Help, "help");
    assert!((1..=2).contains(&help), "help: {help} draw calls");
    // A file dragged over the window.
    assert_eq!(
        render(&mut gpu, &mut ui, 5, "", "", Overlay::Drop, "drop"),
        1
    );
    // The first-run guide: the library has no songs.
    let guide = render_ex(
        &mut gpu,
        &mut ui,
        0,
        "",
        "",
        Overlay::None,
        "guide",
        None,
        Extra {
            library_empty: true,
            ..Default::default()
        },
    );
    assert_eq!(guide, 1);
    // A folder's detail with the clear lamp breakdown. The lamps add up to the
    // folder's 216 songs: PERFECT, FULL COMBO, HARD, CLEAR, EASY, FAILED, no record.
    let lamps = [12, 3, 20, 58, 41, 17, 65];
    assert_eq!(lamps.iter().sum::<usize>(), 216);
    let folders = vec![
        SelectRow::Folder {
            label: strings::FOLDER_ALL,
            count: 216,
            lamps,
        },
        SelectRow::Folder {
            label: strings::FOLDER_MODE,
            count: 216,
            lamps,
        },
    ];
    assert_eq!(
        render_ex(
            &mut gpu,
            &mut ui,
            0,
            "",
            "",
            Overlay::None,
            "folder-lamps",
            Some((folders, crumbs)),
            Extra::default(),
        ),
        1
    );
}

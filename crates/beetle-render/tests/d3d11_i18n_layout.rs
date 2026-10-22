//! The main screens in every UI language at 1280x720 and 1024x768, on a real
//! D3D11 device (WARP). Each frame must be one batch and must not panic.
//! The language is process-wide, so this binary has a single test function.
#![cfg(target_os = "windows")]

mod common;

use std::time::Instant;

use beetle_core::{Lane, LnOption, PlayMode, ScoreStore, TableIndex};
use beetle_render::backend::d3d11::com::D3D_DRIVER_TYPE_WARP;
use beetle_render::strings::{self, Lang};
use beetle_render::{
    draw_key_config, draw_options_modal, draw_settings, draw_song_select, draw_title, D3d11Backend,
    EightKForm, FilterBar, GpuBackend, Hit, HitId, KeyBinding, KeyConfigFrame, KeyConfigRects,
    OptionLine, Rebind, Rect, ScratchSide, SelectFrame, SelectMotion, SettingsFrame, SkinConfig,
    TitleFrame, Ui, Viewport,
};
use common::HiddenWindow;

const SIZES: [(u32, u32); 2] = [(1280, 720), (1024, 768)];

/// Draws one frame with `draw` and returns its batch count.
fn one_frame(
    gpu: &mut D3d11Backend,
    ui: &mut Ui,
    (w, h): (u32, u32),
    draw: impl FnOnce(&mut Ui, &Viewport),
) -> usize {
    let vp = Viewport::new(w, h);
    gpu.begin_frame(w, h, [0.0, 0.0, 0.0, 1.0]);
    ui.begin(w, h, vp.scale);
    draw(ui, &vp);
    let calls = ui.end(gpu);
    gpu.end_frame();
    calls
}

/// The Settings rows in table order, the way the app lists them.
fn settings_lines() -> Vec<OptionLine<'static>> {
    let rows: [(usize, Option<&'static str>, &'static str, &str); 10] = [
        (
            0,
            Some(strings::GROUP_SYSTEM()),
            strings::ROW_LANGUAGE(),
            strings::LANG_NAME_EN(),
        ),
        (
            0,
            Some(strings::GROUP_DISPLAY()),
            strings::ROW_DISPLAY_MODE(),
            strings::DISPLAY_WINDOWED(),
        ),
        (0, None, strings::ROW_RESOLUTION(), "1280 x 720"),
        (0, None, strings::ROW_GRAPHICS(), strings::GPU_AUTO()),
        (
            0,
            None,
            strings::ROW_TARGET_FPS(),
            strings::VALUE_UNLIMITED(),
        ),
        (
            0,
            Some(strings::GROUP_AUDIO()),
            strings::ROW_MASTER_VOLUME(),
            "80%",
        ),
        (
            0,
            Some(strings::GROUP_JUDGE()),
            strings::ROW_JUDGE_OFFSET(),
            "+0 ms",
        ),
        (
            1,
            Some(strings::GROUP_LAYOUT()),
            strings::ROW_PLAYFIELD(),
            strings::VALUE_CENTER(),
        ),
        (
            1,
            None,
            strings::ROW_TRACK_BGA(),
            strings::TRACK_BGA_MEDIUM(),
        ),
        (
            1,
            Some(strings::GROUP_INPUT()),
            strings::ROW_KEY_LAYOUT(),
            "5K HomeRow",
        ),
    ];
    rows.into_iter()
        .map(|(column, section, label, value)| OptionLine {
            column,
            section,
            label,
            value: value.to_string(),
            stepper: true,
        })
        .collect()
}

/// The play options rows, for the options modal.
fn option_lines() -> Vec<OptionLine<'static>> {
    [
        (strings::GROUP_PLAY(), strings::ROW_GREEN(), "1480 ms"),
        (strings::GROUP_PLAY(), strings::ROW_LANE_COVER(), "0%"),
        (strings::GROUP_PLAY(), strings::ROW_MODIFIER(), "REGULAR"),
        (strings::GROUP_PLAY(), strings::ROW_GAUGE(), "GROOVE"),
        (strings::GROUP_PLAY(), strings::ROW_LN_MODE(), "AUTO"),
        (
            strings::GROUP_SESSION(),
            strings::ROW_AUTO_PLAY(),
            strings::VALUE_OFF(),
        ),
        (
            strings::GROUP_SESSION(),
            strings::ROW_START_MEASURE(),
            "M.0",
        ),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (section, label, value))| OptionLine {
        column: 0,
        section: (i == 0 || i == 5).then_some(section),
        label,
        value: value.to_string(),
        stepper: true,
    })
    .collect()
}

#[test]
fn main_screens_draw_one_batch_in_every_language() {
    for (w, h) in SIZES {
        let window = HiddenWindow::with_size(w, h);
        let mut gpu = D3d11Backend::with_driver_types(window.0, w, h, &[D3D_DRIVER_TYPE_WARP])
            .expect("WARP device");
        let mut ui = Ui::new(1.0);
        for lang in Lang::ALL {
            strings::set_lang(lang);
            let at = format!("{lang:?} {w}x{h}");

            let calls = one_frame(&mut gpu, &mut ui, (w, h), |ui, vp| {
                draw_title(
                    ui,
                    &TitleFrame {
                        viewport: vp,
                        selected: 0,
                        songs: 21,
                        version: "0.1.0",
                        modal: false,
                    },
                );
            });
            assert_eq!(calls, 1, "title {at}");

            let lines = settings_lines();
            let help = strings::HELP_LANGUAGE();
            let calls = one_frame(&mut gpu, &mut ui, (w, h), |ui, vp| {
                draw_settings(
                    ui,
                    &SettingsFrame {
                        viewport: vp,
                        lines: &lines,
                        selected: 0,
                        help,
                        scroll: [0.0; 2],
                    },
                );
            });
            assert_eq!(calls, 1, "settings {at}");

            let lines = option_lines();
            let help = (strings::ROW_GREEN(), strings::HELP_GREEN());
            let calls = one_frame(&mut gpu, &mut ui, (w, h), |ui, vp| {
                draw_options_modal(ui, vp, &lines, 0, help);
            });
            assert_eq!(calls, 1, "options modal {at}");

            let crumbs = vec![strings::FOLDER_ROOT().to_string()];
            let scores = ScoreStore::default();
            let tables = TableIndex::default();
            let calls = one_frame(&mut gpu, &mut ui, (w, h), |ui, vp| {
                draw_song_select(
                    ui,
                    &SelectFrame {
                        viewport: vp,
                        now: Instant::now(),
                        songs: &[],
                        rows: &[],
                        selected: 0,
                        motion: SelectMotion::at_rest(0),
                        library_empty: false,
                        scores: &scores,
                        ln_option: LnOption::Auto,
                        tables: &tables,
                        crumbs: &crumbs,
                        sort: strings::SORT_TITLE(),
                        search: "",
                        search_active: false,
                        preedit: "",
                        jacket: None,
                        ambient: None,
                        option_chips: &[],
                        auto_play: false,
                        has_replay: false,
                        preview_secs: None,
                        filter: FilterBar::default(),
                        result_count: None,
                        sort_menu: None,
                        show_help: false,
                    },
                );
            });
            assert_eq!(calls, 1, "song select {at}");
        }
        ui_scales_keep_the_layout_apart(&mut gpu, &mut ui, (w, h));
    }
    strings::set_lang(Lang::Ko);
}

/// Whether `r` lies inside the window's 16:9 rectangle.
fn inside(r: Rect, vp: &Viewport) -> bool {
    r.x >= vp.x - 0.5
        && r.y >= vp.y - 0.5
        && r.right() <= vp.x + vp.width + 0.5
        && r.bottom() <= vp.y + vp.height + 0.5
}

/// Whether two rectangles share area.
fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.right() - 0.5
        && b.x < a.right() - 0.5
        && a.y < b.bottom() - 0.5
        && b.y < a.bottom() - 0.5
}

/// Every click target picked by `pick` lies in the window and no two overlap.
fn assert_apart(hits: &[Hit], vp: &Viewport, at: &str, pick: impl Fn(&HitId) -> bool) {
    let picked: Vec<&Hit> = hits.iter().filter(|h| pick(&h.id)).collect();
    for (i, a) in picked.iter().enumerate() {
        assert!(inside(a.rect, vp), "{at}: {:?} leaves the window", a.id);
        for b in &picked[i + 1..] {
            assert!(
                !overlaps(a.rect, b.rect),
                "{at}: {:?} overlaps {:?}",
                a.id,
                b.id
            );
        }
    }
}

fn hit_rect(hits: &[Hit], id: HitId) -> Rect {
    hits.iter()
        .find(|h| h.id == id)
        .unwrap_or_else(|| panic!("{id:?} not drawn"))
        .rect
}

/// Key Config and the Song Select top bar at UI scales 80, 100 and 150 in
/// every language and both window sizes. Click targets stay inside the window
/// and do not overlap, the Key Config parts stack in order (tabs, lanes, card,
/// footer) and the top bar runs left to right (folder, sort, search, buttons).
fn ui_scales_keep_the_layout_apart(gpu: &mut D3d11Backend, ui: &mut Ui, (w, h): (u32, u32)) {
    let modes = [
        (PlayMode::Keys5, ScratchSide::Left, EightKForm::Inline),
        (PlayMode::Keys7, ScratchSide::Right, EightKForm::Inline),
        (PlayMode::Keys8, ScratchSide::Left, EightKForm::Triggers),
    ];
    for lang in Lang::ALL {
        strings::set_lang(lang);
        for percent in [80, 100, 150] {
            let vp = Viewport::new(w, h).with_ui_scale(percent);
            let at = format!("{lang:?} {w}x{h} ui {percent}");

            for (mode, scratch, form) in modes {
                let mut layout = SkinConfig::default();
                layout.set_play_mode(mode);
                layout.set_scratch_side(mode, scratch);
                layout.set_eight_k_form(form);
                let screen = layout.screen_lanes();
                let labels: Vec<String> = screen
                    .iter()
                    .map(|l| format!("{l:?}").to_uppercase())
                    .collect();
                let lanes: Vec<KeyBinding> = screen
                    .iter()
                    .zip(&labels)
                    .map(|(&lane, label)| KeyBinding {
                        lane,
                        label,
                        keys: if lane == Lane::Scratch {
                            &["LShift"]
                        } else {
                            &["S"]
                        },
                    })
                    .collect();
                let at = format!("{at} {mode:?}");
                gpu.begin_frame(w, h, [0.0, 0.0, 0.0, 1.0]);
                ui.begin(w, h, vp.scale);
                let rects: KeyConfigRects = draw_key_config(
                    ui,
                    &KeyConfigFrame {
                        viewport: &vp,
                        mode,
                        lanes: &lanes,
                        selected: 1,
                        rebinding: Some(Rebind::Replace),
                        layout: "HomeRow",
                        scratch,
                        form,
                    },
                );
                ui.end(gpu);
                gpu.end_frame();
                assert!(
                    rects.tabs.bottom() <= rects.controllers.y + 0.5,
                    "tabs/lanes {at}"
                );
                assert!(
                    rects.controllers.bottom() <= rects.card.y + 0.5,
                    "lanes/card {at}"
                );
                assert!(
                    rects.card.bottom() <= rects.footer.y + 0.5,
                    "card/footer {at}"
                );
                assert!(inside(rects.footer, &vp), "footer {at}");
                assert_apart(&ui.hits, &vp, &format!("key config {at}"), |id| {
                    matches!(
                        id,
                        HitId::KeyModeTab(_)
                            | HitId::KeyLane(_)
                            | HitId::KeySet
                            | HitId::KeyAdd
                            | HitId::KeyClear
                            | HitId::KeyPreset
                            | HitId::KeyScratch
                            | HitId::KeyReset
                            | HitId::KeyBack
                            | HitId::KeyForm
                    )
                });
                for h in ui.hits.iter().filter(|h| matches!(h.id, HitId::KeyLane(_))) {
                    let c = rects.controllers;
                    assert!(
                        h.rect.x >= c.x - 0.5
                            && h.rect.right() <= c.right() + 0.5
                            && h.rect.y >= c.y - 0.5
                            && h.rect.bottom() <= c.bottom() + 0.5,
                        "lane {:?} leaves its area {at}",
                        h.id
                    );
                }
            }

            let crumbs = vec![strings::FOLDER_ROOT().to_string()];
            let scores = ScoreStore::default();
            let tables = TableIndex::default();
            gpu.begin_frame(w, h, [0.0, 0.0, 0.0, 1.0]);
            ui.begin(w, h, vp.scale);
            draw_song_select(
                ui,
                &SelectFrame {
                    viewport: &vp,
                    now: Instant::now(),
                    songs: &[],
                    rows: &[],
                    selected: 0,
                    motion: SelectMotion::at_rest(0),
                    library_empty: false,
                    scores: &scores,
                    ln_option: LnOption::Auto,
                    tables: &tables,
                    crumbs: &crumbs,
                    sort: strings::SORT_TITLE(),
                    search: "",
                    search_active: false,
                    preedit: "",
                    jacket: None,
                    ambient: None,
                    option_chips: &[],
                    auto_play: false,
                    has_replay: false,
                    preview_secs: None,
                    filter: FilterBar::default(),
                    result_count: None,
                    sort_menu: None,
                    show_help: false,
                },
            );
            ui.end(gpu);
            gpu.end_frame();
            assert_apart(&ui.hits, &vp, &format!("song select {at}"), |id| {
                matches!(
                    id,
                    HitId::FolderNext
                        | HitId::Sort
                        | HitId::Search
                        | HitId::Auto
                        | HitId::ExitQuit
                        | HitId::OpenSettings
                        | HitId::KeyConfig
                        | HitId::PlayOptions
                )
            });
            let folder_next = hit_rect(&ui.hits, HitId::FolderNext);
            let sort = hit_rect(&ui.hits, HitId::Sort);
            let search = hit_rect(&ui.hits, HitId::Search);
            let auto = hit_rect(&ui.hits, HitId::Auto);
            assert!(folder_next.right() <= sort.x + 0.5, "folder/sort {at}");
            assert!(sort.right() <= search.x + 0.5, "sort/search {at}");
            assert!(search.right() <= auto.x + 0.5, "search/buttons {at}");
        }
    }
}

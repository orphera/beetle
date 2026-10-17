//! The main screens in every UI language at 1280x720 and 1024x768, on a real
//! D3D11 device (WARP). Each frame must be one batch and must not panic.
//! The language is process-wide, so this binary has a single test function.
#![cfg(target_os = "windows")]

mod common;

use std::time::Instant;

use beetle_core::{LnOption, ScoreStore, TableIndex};
use beetle_render::backend::d3d11::com::D3D_DRIVER_TYPE_WARP;
use beetle_render::strings::{self, Lang};
use beetle_render::{
    draw_options_modal, draw_settings, draw_song_select, draw_title, D3d11Backend, FilterBar,
    GpuBackend, OptionLine, SelectFrame, SelectMotion, SettingsFrame, TitleFrame, Ui, Viewport,
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
    }
    strings::set_lang(Lang::Ko);
}

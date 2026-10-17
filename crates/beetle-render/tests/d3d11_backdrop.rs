//! The blurred jacket backdrop on a real D3D11 device (WARP): the loading
//! and song select screens with and without the picture. The picture must
//! show (mean luma and the bare left margin rise), keep the list readable
//! (a pure white picture stays dim behind the rows), and the frame stays
//! one batch. Captures go to `target/backdrop-*.bmp`.
#![cfg(target_os = "windows")]

mod common;

use std::time::{Duration, Instant};

use beetle_core::{ChartId, LnOption, PlayMode, ScoreStore, SongMetadata, TableIndex};
use beetle_render::backdrop::blur_backdrop;
use beetle_render::backend::d3d11::com::D3D_DRIVER_TYPE_WARP;
use beetle_render::jacket::{jacket_seed, JacketKey};
use beetle_render::skin::ColorRgba;
use beetle_render::{
    draw_loading, draw_song_select, strings, D3d11Backend, FilterBar, GpuBackend, ImageBuffer,
    LoadingFrame, SelectFrame, SelectMotion, SelectRow, Ui, Viewport,
};
use common::{write_bmp, HiddenWindow};

const W: u32 = 1280;
const H: u32 = 720;

/// The picture must lift the frame by at least this much mean luma.
const MIN_RISE: f64 = 8.0;
/// A pure white picture must leave the song list at most this dim.
const LIST_MAX_LUMA: f64 = 26.0;
/// Whole-frame rise required on the song select screen, where panels cover
/// most of the picture.
const SELECT_MIN_RISE: f64 = 3.0;

fn song(n: u64) -> SongMetadata {
    SongMetadata {
        id: ChartId::synthetic(n),
        md5: [0; 16],
        ln_count: 0,
        ln_mode: None,
        legacy_hash: n,
        file_path: String::new(),
        title: format!("Backdrop Probe {n}"),
        subtitle: String::new(),
        artist: "Probe Artist".into(),
        genre: "GENRE".into(),
        bpm: 150.0,
        bpm_min: 150.0,
        bpm_max: 150.0,
        play_level: 12,
        notes_count: 1500,
        play_mode: PlayMode::Keys7,
    }
}

/// A bright, colourful stand-in for a jacket: mean luma about 127.
fn bright_art() -> ImageBuffer {
    let (w, h) = (1280u32, 720u32);
    let mut img = ImageBuffer::new(w, h, ColorRgba::new(0, 0, 0, 255));
    for y in 0..h {
        for x in 0..w {
            let r = (x * 255 / w) as u8;
            let g = (y * 255 / h) as u8;
            img.pixels[(y * w + x) as usize] = ColorRgba::new(r, g, 128, 255);
        }
    }
    img
}

/// A pure white stage image: the worst case for the text behind it.
fn white_art() -> ImageBuffer {
    ImageBuffer::new(1280, 720, ColorRgba::new(255, 255, 255, 255))
}

/// Rec. 709 luma of the `x0..x1` × `y0..y1` part of a captured RGBA frame.
fn region_luma(px: &[u8], x0: usize, y0: usize, x1: usize, y1: usize) -> f64 {
    let mut total = 0.0;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = &px[(y * W as usize + x) * 4..][..3];
            total += 0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64;
        }
    }
    total / ((x1 - x0) * (y1 - y0)) as f64
}

fn frame_luma(px: &[u8]) -> f64 {
    region_luma(px, 0, 0, W as usize, H as usize)
}

/// The left margin, left of every panel and the jacket: where only the
/// picture can show.
fn left_margin_luma(px: &[u8]) -> f64 {
    region_luma(px, 0, 200, 24, 500)
}

/// Shows `art` (settled a second ago, so no crossfade runs) as the backdrop
/// of `key`, or nothing when `art` is `None`.
fn show_art(ui: &mut Ui, key: JacketKey, art: Option<&ImageBuffer>) {
    let Some(art) = art else {
        return;
    };
    let settled = Instant::now() - Duration::from_secs(1);
    let px = blur_backdrop(art).expect("pixels");
    assert!(ui
        .backdrop
        .show(ui.canvas.atlas_mut(), key, settled, || px.clone()));
}

fn capture(gpu: &mut D3d11Backend, name: &str) -> Vec<u8> {
    let (w, h, px) = gpu.capture_frame().expect("readback");
    gpu.end_frame();
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    write_bmp(&target.join(format!("backdrop-{name}.bmp")), w, h, &px);
    px
}

/// Draws the loading screen once. Returns the draw-call count and the pixels.
fn loading_frame(
    gpu: &mut D3d11Backend,
    ui: &mut Ui,
    art: Option<&ImageBuffer>,
    name: &str,
) -> (usize, Vec<u8>) {
    let vp = Viewport::new(W, H);
    let song = song(42);
    let chips = vec![strings::fill(strings::CHIP_GREEN, &["500"])];
    ui.lite = false;
    gpu.begin_frame(W, H, [0.0, 0.0, 0.0, 1.0]);
    ui.backdrop.reset();
    ui.begin(W, H, vp.scale);
    show_art(ui, JacketKey::Chart(song.id), art);
    draw_loading(
        ui,
        &LoadingFrame {
            viewport: &vp,
            song: &song,
            notes: 1500,
            jacket: None,
            ambient: None,
            elapsed: 3.3,
            status: strings::LOADING,
            option_chips: &chips,
            badge: None,
        },
    );
    let calls = ui.end(gpu);
    (calls, capture(gpu, name))
}

/// Draws the song select screen once at `size` with a 12-song list, every
/// jacket resident as a placeholder. Returns the draw-call count.
fn select_draw(
    gpu: &mut D3d11Backend,
    ui: &mut Ui,
    (w, h): (u32, u32),
    art: Option<&ImageBuffer>,
) -> usize {
    let vp = Viewport::new(w, h);
    let songs: Vec<SongMetadata> = (1..=12).map(song).collect();
    let rows: Vec<SelectRow> = (0..songs.len()).map(SelectRow::Song).collect();
    let tables = TableIndex::new(Vec::new());
    let scores = ScoreStore::default();
    let crumbs = vec![
        strings::FOLDER_ROOT.to_string(),
        strings::FOLDER_ALL.to_string(),
    ];
    let chips = vec![strings::fill(strings::CHIP_GREEN, &["500"])];
    ui.lite = false;
    gpu.begin_frame(w, h, [0.0, 0.0, 0.0, 1.0]);
    ui.backdrop.reset();
    ui.begin(w, h, vp.scale);
    for s in &songs {
        let seed = jacket_seed(&s.title, &s.artist);
        assert!(ui.jackets.ensure_placeholder(ui.canvas.atlas_mut(), seed));
    }
    show_art(ui, JacketKey::Chart(songs[0].id), art);
    draw_song_select(
        ui,
        &SelectFrame {
            viewport: &vp,
            songs: &songs,
            rows: &rows,
            selected: 0,
            motion: SelectMotion::at_rest(0),
            library_empty: false,
            scores: &scores,
            ln_option: LnOption::Auto,
            tables: &tables,
            crumbs: &crumbs,
            sort: strings::SORT_TITLE,
            search: "",
            search_active: false,
            preedit: "",
            jacket: None,
            ambient: None,
            option_chips: &chips,
            auto_play: false,
            has_replay: false,
            preview_secs: None,
            filter: FilterBar::default(),
            result_count: None,
            sort_menu: None,
            show_help: false,
        },
    );
    ui.end(gpu)
}

/// `select_draw` at the captured size, with the frame saved to `target`.
fn select_frame(
    gpu: &mut D3d11Backend,
    ui: &mut Ui,
    art: Option<&ImageBuffer>,
    name: &str,
) -> (usize, Vec<u8>) {
    let calls = select_draw(gpu, ui, (W, H), art);
    (calls, capture(gpu, name))
}

#[test]
fn loading_backdrop_shows_and_stays_one_batch() {
    let window = HiddenWindow::with_size(W, H);
    let mut gpu = D3d11Backend::with_driver_types(window.0, W, H, &[D3D_DRIVER_TYPE_WARP])
        .expect("WARP device");
    let mut ui = Ui::new(1.0);
    let art = bright_art();

    let (calls_plain, plain) = loading_frame(&mut gpu, &mut ui, None, "loading-plain");
    let (calls_art, with_art) = loading_frame(&mut gpu, &mut ui, Some(&art), "loading-art");
    let (luma_plain, luma_art) = (frame_luma(&plain), frame_luma(&with_art));
    let (margin_plain, margin_art) = (left_margin_luma(&plain), left_margin_luma(&with_art));
    eprintln!(
        "loading: batches {calls_plain} -> {calls_art}, mean luma {luma_plain:.2} -> {luma_art:.2}, \
         left margin {margin_plain:.2} -> {margin_art:.2}"
    );
    assert_eq!(calls_plain, 1);
    assert_eq!(calls_art, 1, "the picture rides in the main batch");
    assert!(
        luma_art >= luma_plain + MIN_RISE,
        "picture too faint: {luma_plain:.2} -> {luma_art:.2}"
    );
    // A broken sub-rect upload leaves the picture black: the bare margin
    // must show the picture, not just the gradient.
    assert!(
        margin_art >= margin_plain + MIN_RISE,
        "the picture is missing at the margin: {margin_plain:.2} -> {margin_art:.2}"
    );
}

#[test]
fn select_backdrop_shows_and_the_list_stays_readable() {
    let window = HiddenWindow::with_size(W, H);
    let mut gpu = D3d11Backend::with_driver_types(window.0, W, H, &[D3D_DRIVER_TYPE_WARP])
        .expect("WARP device");
    let mut ui = Ui::new(1.0);
    let bright = bright_art();
    let white = white_art();

    let (calls_plain, plain) = select_frame(&mut gpu, &mut ui, None, "select-plain");
    let (calls_bright, with_bright) = select_frame(&mut gpu, &mut ui, Some(&bright), "select-art");
    let (calls_white, with_white) = select_frame(&mut gpu, &mut ui, Some(&white), "select-white");

    // The list rows sit in the middle of the left part of the frame.
    let list = |px: &[u8]| region_luma(px, 200, 300, 560, 360);
    let (luma_plain, luma_bright) = (frame_luma(&plain), frame_luma(&with_bright));
    let (margin_plain, margin_bright) = (left_margin_luma(&plain), left_margin_luma(&with_bright));
    let list_white = list(&with_white);
    eprintln!(
        "select: batches {calls_plain}/{calls_bright}/{calls_white}, mean luma {luma_plain:.2} -> \
         {luma_bright:.2}, left margin {margin_plain:.2} -> {margin_bright:.2}, \
         list under a white picture {list_white:.2}"
    );
    assert_eq!(calls_bright, calls_plain, "the picture adds no batch");
    assert_eq!(calls_white, calls_plain, "the picture adds no batch");
    // The list and the detail panel cover most of this frame, so the whole
    // frame rises less than the loading screen does (measured about 4 luma).
    assert!(
        luma_bright >= luma_plain + SELECT_MIN_RISE,
        "picture too faint: {luma_plain:.2} -> {luma_bright:.2}"
    );
    assert!(
        margin_bright >= margin_plain + MIN_RISE,
        "the picture is missing at the margin: {margin_plain:.2} -> {margin_bright:.2}"
    );
    assert!(
        list_white <= LIST_MAX_LUMA,
        "a white picture lights the list too much: {list_white:.2}"
    );
}

/// The select frame at every UI scale from 0.75 to 3.0 (step 0.25) is one
/// batch, with the grid on page 0 and the twelve jackets resident.
#[test]
fn select_frame_is_one_batch_with_twelve_jackets_at_every_ui_scale() {
    let (max_w, max_h) = (3840, 2160);
    let window = HiddenWindow::with_size(max_w, max_h);
    let mut gpu = D3d11Backend::with_driver_types(window.0, max_w, max_h, &[D3D_DRIVER_TYPE_WARP])
        .expect("WARP device");
    let white = white_art();
    for step in 0..=9 {
        let scale = 0.75 + 0.25 * step as f32;
        let size = ((W as f32 * scale) as u32, (H as f32 * scale) as u32);
        let mut ui = Ui::new(scale);
        let calls = select_draw(&mut gpu, &mut ui, size, Some(&white));
        gpu.end_frame();
        eprintln!(
            "scale {scale}: batches {calls}, pages {}, grid page {:?}",
            ui.canvas.atlas().page_count(),
            ui.jackets.page()
        );
        assert_eq!(
            calls, 1,
            "scale {scale}: the select frame splits into batches"
        );
        assert_eq!(ui.jackets.page(), Some(0), "scale {scale}: grid off page 0");
        for n in 1..=12 {
            let s = song(n);
            let seed = jacket_seed(&s.title, &s.artist);
            assert!(
                ui.jackets.contains(JacketKey::Placeholder(seed)),
                "scale {scale}: jacket {n} not resident"
            );
        }
    }
}

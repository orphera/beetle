//! Draws and presents one frame of the current screen with the Canvas UI on
//! Direct3D 11 (ADR-026: the only renderer; WARP when there is no GPU).
//!
//! Each function gathers what its screen shows from `AppState`, draws it
//! with `beetle_render`, and presents. Screens render on every
//! `RedrawRequested`; the event loop decides how often that is.

use std::path::Path;
use std::sync::OnceLock;
use std::time::Instant;

use beetle_core::{ChartId, SongMetadata, SortMode};
use beetle_render::backdrop::placeholder_backdrop;
use beetle_render::jacket::{jacket_seed, JacketKey};
use beetle_render::{
    strings, GpuBackend, OptionLine, SettingsFrame, ToastAnchor, ToastFrame, ToastKind, Ui,
};
use winit::dpi::PhysicalSize;

use crate::devtools;
use crate::gameplay::END_BANNER_SECONDS;
use crate::gpu_ui::{bga_texture, gameplay_bga_texture, ImageKey};
use crate::input::{lane_label, screen_lanes_for, KeyPreset};
use crate::options_table::{self, play_options, settings_options, OptionDesc};
use crate::state::{replay_path, AppScreen, AppState, LibraryJob};
use crate::transition::{self, show_toast};

/// The decorative clock for menu backdrops: seconds since the app started,
/// wrapped into the ambient loop. Purely visual; the audio clock never reads it.
fn ambient_clock() -> f32 {
    static START: OnceLock<Instant> = OnceLock::new();
    beetle_render::ambient_time(START.get_or_init(Instant::now).elapsed().as_secs_f64())
}

/// Starts a frame on the backbuffer and the UI.
fn begin(state: &mut AppState, size: PhysicalSize<u32>) {
    state.sync_screen_entry();
    state.d3d11.begin_frame(
        size.width,
        size.height,
        beetle_render::theme::clear_color(state.screen == AppScreen::Gameplay),
    );
    // Menus draw at the UI scale, gameplay at the window scale; each has its own UI.
    let gameplay = state.screen == AppScreen::Gameplay;
    let warp = state.d3d11.is_warp();
    // Reduce Motion freezes the decorative clock at 0: the backdrop drift,
    // particles, light band and title sweep all read it.
    let clock = if state.reduce_motion {
        0.0
    } else {
        ambient_clock()
    };
    let ui = state.gpu_ui.ui.begin(
        gameplay,
        size.width,
        size.height,
        state.view.ui.scale,
        state.view.viewport.scale,
    );
    ui.lite = warp;
    ui.clock = clock;
    ui.pointer = state.cursor;
}

/// Shows a song's blurred jacket as the backdrop of the screen, crossfading
/// from the one on show. `song` is the song's id and its placeholder seed. A
/// song whose artwork is still loading, or no song at all (a folder row), keeps
/// the backdrop on show; a song without artwork gets its placeholder. Lite mode
/// draws no backdrop.
fn show_backdrop(state: &mut AppState, song: Option<(ChartId, u64)>, now: Instant) {
    let Some((id, seed)) = song else {
        return;
    };
    if state.gpu_ui.ui.current().lite {
        return;
    }
    let ui = state.gpu_ui.ui.current();
    match state.stage_image_cache.get(&id) {
        Some(Some(_)) => {
            if let Some(px) = state.stage_backdrop_cache.get(&id) {
                ui.backdrop
                    .show(ui.canvas.atlas_mut(), JacketKey::Chart(id), now, || {
                        px.clone()
                    });
            }
        }
        Some(None) => {
            ui.backdrop.show(
                ui.canvas.atlas_mut(),
                JacketKey::Placeholder(seed),
                now,
                || placeholder_backdrop(seed),
            );
        }
        None => {}
    }
}

/// The Loading screen's backdrop: the song's own blur when it is ready, else its
/// placeholder. Unlike song select it never keeps the previous song's backdrop
/// on show. Lite mode draws no backdrop.
fn show_loading_backdrop(state: &mut AppState, song: Option<(ChartId, u64)>, now: Instant) {
    let Some((id, seed)) = song else {
        return;
    };
    if state.gpu_ui.ui.current().lite {
        return;
    }
    let ui = state.gpu_ui.ui.current();
    match state.stage_backdrop_cache.get(&id) {
        Some(px) => {
            ui.backdrop
                .show(ui.canvas.atlas_mut(), JacketKey::Chart(id), now, || {
                    px.clone()
                });
        }
        None => {
            ui.backdrop.show(
                ui.canvas.atlas_mut(),
                JacketKey::Placeholder(seed),
                now,
                || placeholder_backdrop(seed),
            );
        }
    }
}

/// Submits the UI, serves screenshot / devtools capture requests (they read
/// the backbuffer, so before present) and presents.
fn finish(state: &mut AppState) {
    state.gpu_ui.ui.current().end(&mut state.d3d11);
    if let Some(path) = state.pending_screenshot.take() {
        match devtools::save_backbuffer(&mut state.d3d11, &path) {
            Ok(_) => {
                let name = Path::new(&path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                show_toast(
                    state,
                    ToastKind::Success,
                    strings::fill(strings::TOAST_SCREENSHOT_SAVED(), &[&name]),
                );
            }
            Err(e) => show_toast(
                state,
                ToastKind::Error,
                strings::fill(strings::TOAST_SCREENSHOT_FAILED(), &[&e.to_string()]),
            ),
        }
    }
    if let Some(cap) = &mut state.capture {
        if cap.on_frame(state.screen, &mut state.d3d11) {
            state.should_exit_app = true;
        }
    }
    state.d3d11.end_frame();
}

/// The layers above a frame, drawn last: the screen-change transition (a
/// fade, the shutter, or the Gameplay reveal), then the toast (`toasts` is
/// false on Loading and Gameplay). Overlays record no hits and touch no clock,
/// so they never change what a screen does with input or judgement.
fn overlays(state: &mut AppState, toasts: bool) {
    let now = Instant::now();
    let overlay = match devtools::frozen_shutter_progress() {
        Some(p) => transition::shutter_phase_at(p * transition::SHUTTER_SECONDS)
            .map(transition::Overlay::Shutter),
        None => state.screen_entry.overlay(now),
    };
    match overlay {
        Some(transition::Overlay::Fade(alpha)) => {
            beetle_render::draw_screen_fade(state.gpu_ui.ui.current(), alpha);
        }
        Some(transition::Overlay::Shutter(phase) | transition::Overlay::GameplayReveal(phase)) => {
            beetle_render::draw_shutter(
                state.gpu_ui.ui.current(),
                &state
                    .view
                    .frame_viewport(state.screen == AppScreen::Gameplay),
                phase,
            );
        }
        None => {}
    }
    if toasts {
        // Key Config keeps the tabs clear; every other menu uses the footer strip.
        let anchor = if state.screen == AppScreen::KeyConfig {
            ToastAnchor::BelowTabs
        } else {
            ToastAnchor::Footer
        };
        if let (Some(pose), Some(toast)) = (state.toast_pose_now(now), state.toast.as_ref()) {
            beetle_render::draw_toast(
                state.gpu_ui.ui.current(),
                &state
                    .view
                    .frame_viewport(state.screen == AppScreen::Gameplay),
                &ToastFrame {
                    text: &toast.text,
                    kind: toast.kind,
                    alpha: pose.alpha,
                    slide: pose.slide,
                    anchor,
                    clickable: toast.clickable,
                },
            );
        }
    }
}

/// "500", "REGULAR", "GROOVE", and for a chart with long notes
/// "LN" / "CN" / "CN (HCN)": the options a play of `song` will use.
fn option_chips(state: &AppState, song: Option<&SongMetadata>) -> Vec<String> {
    let mut chips = vec![
        strings::fill(
            strings::CHIP_GREEN(),
            &[&state.play_options.green_ms.to_string()],
        ),
        state.play_options.lane_modifier.as_str().to_string(),
        state.play_options.gauge_type.as_str().to_string(),
    ];
    if let Some(song) = song.filter(|s| s.ln_count > 0) {
        chips.push(state.play_ruleset(song).label());
    }
    chips
}

pub fn gameplay(
    state: &mut AppState,
    size: PhysicalSize<u32>,
    audio_time: f64,
    visual_levels: &[f32; 16],
) {
    // BGA not visible (option off, or track BGA at 0%): no texture lookups or
    // uploads at all.
    let bga = state.bga_visible().then(|| {
        gameplay_bga_texture(
            &mut state.gpu_ui,
            &mut state.d3d11,
            &state.bga_bank,
            &state.video_players,
            state.poor_until_time,
            state.poor_bga_bmp,
            state.current_bga_bmp,
            state.active_bga_image.as_ref(),
            state.active_chart_id,
            audio_time,
        )
    });
    let bga = bga.flatten();
    let layer = state
        .current_layer_bmp
        .filter(|_| state.bga_visible())
        .and_then(|id| {
            bga_texture(
                &mut state.gpu_ui,
                &mut state.d3d11,
                &state.bga_bank,
                &state.video_players,
                id,
                true,
            )
        });
    state.view.clean_expired_hit_bursts(audio_time);
    let (badge, hint) = gameplay_badge_and_hint(
        state.is_replay_playback,
        state.is_auto_play,
        state.key_bindings.get(state.view.skin.play_mode).preset,
    );

    begin(state, size);

    if let (Some(chart), Some(judge), Some(timing)) = (
        &state.active_chart,
        &state.active_judge,
        &state.active_timing,
    ) {
        beetle_render::draw_gameplay(
            state.gpu_ui.ui.current(),
            &beetle_render::PlayFrame {
                first_note: state.first_note_time,
                viewport: &state.view.viewport,
                layout: &state.view.skin,
                chart,
                notes: judge.notes(),
                timing,
                score: judge.score(),
                audio_time,
                song_length: state.song_end_time,
                visual_levels,
                bga,
                layer,
                track_bga_opacity: if state.bga_enabled {
                    state.track_bga.opacity()
                } else {
                    0.0
                },
                key_pressed: state.view.key_pressed(),
                hit_bursts: state.view.hit_bursts(),
                last_judge: state.view.last_judge(),
                hint: &hint,
                badge,
                pause: state
                    .is_gameplay_paused
                    .then_some(state.pause_selected_option),
                key_hint_alpha: state.key_hint.alpha(audio_time),
                readout: state
                    .gameplay_readout
                    .as_ref()
                    .map(|(text, at)| (text.as_str(), *at)),
                banner: state.gameplay_end.map(|end| {
                    let progress = end.started.elapsed().as_secs_f64() / END_BANNER_SECONDS;
                    (end.lamp, progress.min(1.0) as f32)
                }),
                judge_marks: state.view.judge_marks(),
                visual_offset_ms: state.visual_offset_ms,
                reduce_flashes: state.reduce_flashes,
                high_contrast: state.high_contrast,
            },
        );
    }
    // The Loading -> Gameplay reveal: drawn over this frame only. Gameplay
    // time, READY and judgement were already advanced above.
    overlays(state, false);
    finish(state);
}

/// Mode badge and key-hint line for the gameplay HUD. The key names stay
/// English (they are what the player presses).
fn gameplay_badge_and_hint(
    is_replay: bool,
    is_auto: bool,
    preset: KeyPreset,
) -> (Option<&'static str>, String) {
    if is_replay {
        return (
            Some(strings::REPLAY()),
            strings::HUD_BACK_TO_SELECT().to_string(),
        );
    }
    if is_auto {
        return (
            Some(strings::AUTO_PLAY()),
            strings::HUD_BACK_TO_SELECT().to_string(),
        );
    }
    let keys = match preset {
        KeyPreset::HomeRow => "Shift+S D F Space J K L",
        KeyPreset::ArcadeZx => "Shift+Z S X D C F V",
        KeyPreset::Pms9K => "S D F Space J K L ; '",
        KeyPreset::Ue4K => "S D L ;",
        KeyPreset::Ue6K => "A S D L ; '",
        KeyPreset::Ue8K => "A S D F K L ; '",
        KeyPreset::Ue8KTriggers => "LShift + S D F J K L + RShift",
        KeyPreset::DoublePlay => {
            let keys = "Shift+ZSXDCFV / RShift+UIOP[]\\";
            return (None, strings::fill(strings::HUD_KEYS_NO_COVER(), &[keys]));
        }
        KeyPreset::Custom => strings::KEYS_CUSTOM(),
    };
    (None, strings::fill(strings::HUD_KEYS(), &[keys]))
}

/// The sort menu's option names, in `SortMode::ALL` order.
fn sort_names() -> [&'static str; 5] {
    SortMode::ALL.map(sort_label)
}

/// Display names of the options that are stored under English enum names.
fn sort_label(mode: SortMode) -> &'static str {
    match mode {
        SortMode::Title => strings::SORT_TITLE(),
        SortMode::Level => strings::SORT_LEVEL(),
        SortMode::ClearLamp => strings::SORT_CLEAR_LAMP(),
        SortMode::ScoreRate => strings::SORT_SCORE_RATE(),
        SortMode::Bpm => strings::SORT_BPM(),
    }
}

/// The rows of an option table as the screens draw them: a section header
/// where the group changes.
fn option_lines(state: &AppState, table: &'static [OptionDesc]) -> Vec<OptionLine<'static>> {
    let mut last: Option<&'static str> = None;
    table
        .iter()
        .map(|d| {
            let section = (last != Some(d.group)).then_some(d.group);
            last = Some(d.group);
            OptionLine {
                column: d.column,
                section,
                label: d.label,
                value: options_table::value(state, d.id),
                stepper: options_table::steps(d.id),
            }
        })
        .collect()
}

/// Song select plus its option / quit modals.
pub fn song_select(state: &mut AppState, size: PhysicalSize<u32>) {
    let selected_id = state.current_selected_song().map(|s| s.id);
    let ln_option = state.ln_option();
    let has_replay = state
        .current_selected_song()
        .is_some_and(|s| Path::new(&replay_path(s.id, s.score_rule(ln_option))).exists());
    let chips = option_chips(state, state.current_selected_song());
    let option_panel = state
        .show_option_modal
        .then(|| option_lines(state, play_options()));

    let stage_img = selected_id
        .and_then(|id| state.stage_image_cache.get(&id))
        .and_then(|img| img.as_ref());
    let (jacket, ambient) = match (selected_id, stage_img) {
        (Some(id), Some(img)) => {
            state.gpu_ui.trim_stage_textures(&mut state.d3d11, id);
            (
                state
                    .gpu_ui
                    .image(&mut state.d3d11, ImageKey::Stage(id), img),
                Some(img.average_color_sampled(6)),
            )
        }
        _ => (None, None),
    };

    begin(state, size);
    let backdrop_song = state
        .current_selected_song()
        .map(|s| (s.id, jacket_seed(&s.title, &s.artist)));
    let now = Instant::now();
    show_backdrop(state, backdrop_song, now);
    let crumbs = crate::folders::crumbs(&state.folder_tree, &state.folder_path);
    let mode_chips: Vec<(&str, bool)> = state
        .present_modes
        .iter()
        .map(|&m| {
            (
                beetle_render::theme::mode_label(m),
                state.filter.modes.contains(&m),
            )
        })
        .collect();
    let filter = beetle_render::FilterBar {
        modes: &mode_chips,
        level_min: state.filter.level_min,
        level_max: state.filter.level_max,
        unplayed: state.filter.only_unplayed,
        uncleared: state.filter.only_uncleared,
        active: state.filter.is_active(),
        focus: state.filter_focus,
    };
    let names = sort_names();
    let sort_menu = state.sort_menu.map(|highlight| beetle_render::SortMenu {
        options: &names,
        current: SortMode::ALL
            .iter()
            .position(|&m| m == state.sort_mode)
            .unwrap_or(0),
        highlight,
    });
    let motion = state.song_select_motion(Instant::now());
    // Jacket thumbnails for the rows near the list (see `thumbs`).
    let visible = beetle_render::visible_rows(&state.view.ui);
    let settled = state.cursor_settle_time.elapsed() >= crate::thumbs::SETTLE;
    state.jacket_busy = crate::thumbs::pump(state, motion.scroll, visible, settled);
    let rows: Vec<beetle_render::SelectRow> = state
        .entries
        .iter()
        .map(|e| match e {
            crate::folders::ListEntry::Song(i) => beetle_render::SelectRow::Song(*i),
            crate::folders::ListEntry::Folder {
                label,
                count,
                lamps,
                ..
            } => beetle_render::SelectRow::Folder {
                label,
                count: *count,
                lamps: *lamps,
            },
            crate::folders::ListEntry::Group {
                title,
                charts,
                selected,
                labels,
                ..
            } => beetle_render::SelectRow::Group {
                title: title.as_str(),
                charts,
                selected: *selected,
                labels,
            },
        })
        .collect();
    let vp = state.view.ui;
    let ui: &mut Ui = state.gpu_ui.ui.current();
    if let Some(r) = devtools::capture_hover()
        .and_then(|i| beetle_render::top_bar_buttons(ui, &vp).get(i).copied())
    {
        ui.pointer = Some((r.x + r.w / 2.0, r.y + r.h / 2.0));
    }
    beetle_render::draw_song_select(
        ui,
        &beetle_render::SelectFrame {
            viewport: &vp,
            songs: &state.songs,
            rows: &rows,
            selected: state.selected_entry,
            motion,
            library_empty: crate::folders::is_first_run(&state.songs),
            scores: &state.score_store,
            tables: &state.tables,
            ln_option,
            crumbs: &crumbs,
            sort: sort_label(state.sort_mode),
            search: &state.search_query,
            search_active: state.is_search_active,
            preedit: &state.search_preedit,
            jacket,
            ambient,
            option_chips: &chips,
            auto_play: state.is_auto_play,
            has_replay,
            preview_secs: state.preview.playing_for(),
            filter,
            result_count: state.result_count,
            sort_menu,
            show_help: state.show_help,
            now,
        },
    );
    if let Some(lines) = &option_panel {
        let d = &play_options()[state.modal_row.min(play_options().len() - 1)];
        beetle_render::draw_options_modal(ui, &vp, lines, state.modal_row, (d.label, d.help));
    }
    if state.show_exit_modal {
        beetle_render::draw_exit_modal(ui, &vp);
    }
    if state.show_help {
        beetle_render::draw_help_overlay(ui, &vp);
    }
    if let Some(menu) = &state.song_menu {
        let items: Vec<&str> = menu.items.iter().map(|i| i.label.as_str()).collect();
        let danger = menu
            .items
            .iter()
            .position(|i| i.action == crate::song_menu::Action::Delete);
        beetle_render::draw_song_menu(
            ui,
            &vp,
            &menu.title,
            menu.note.as_deref(),
            &items,
            menu.row,
            danger,
        );
    }
    if state.installs.show_skipped {
        let rows: Vec<(String, String)> = state
            .installs
            .skipped
            .iter()
            .map(|r| (r.name.clone(), r.reason.clone()))
            .collect();
        beetle_render::draw_skipped_overlay(ui, &vp, &rows);
    }
    if state.drop_hover {
        beetle_render::draw_drop_overlay(ui, &vp);
    }
    let caret = ui.ime_caret;
    crate::ime::sync_caret(state, caret);
    overlays(state, true);
    finish(state);
}

/// The Settings screen: the values that are set once. While the judge offset
/// calibration is open, its test screen instead.
pub fn settings(state: &mut AppState, size: PhysicalSize<u32>) {
    if state.calibration.is_some() {
        calibrate(state, size);
        return;
    }
    let lines = option_lines(state, settings_options());
    let row = state.settings_row.min(settings_options().len() - 1);
    let scroll =
        beetle_render::follow_settings_scroll(&state.view.ui, &lines, row, state.settings_scroll);
    state.settings_scroll = scroll;
    begin(state, size);
    beetle_render::draw_settings(
        state.gpu_ui.ui.current(),
        &SettingsFrame {
            viewport: &state.view.ui,
            lines: &lines,
            selected: row,
            help: settings_options()[row].help,
            scroll,
        },
    );
    if let Some(list) = &state.song_folders {
        beetle_render::draw_folders_overlay(
            state.gpu_ui.ui.current(),
            &state.view.ui,
            &list.paths,
            list.row,
        );
    }
    overlays(state, true);
    finish(state);
}

/// The judge offset calibration screen. What it shows is read from the
/// session first, so the borrow ends before the frame is drawn.
fn calibrate(state: &mut AppState, size: PhysicalSize<u32>) {
    let Some(session) = &state.calibration else {
        return;
    };
    let test = session.test();
    let now = session.now();
    let phase = if !session.has_audio() {
        beetle_render::CalibratePhase::Unavailable
    } else if test.is_done() {
        beetle_render::CalibratePhase::Done
    } else if test.counting_in(now) {
        beetle_render::CalibratePhase::CountIn
    } else {
        beetle_render::CalibratePhase::Measuring
    };
    let summary = test.summary();
    let kept: Vec<bool> = summary
        .as_ref()
        .map_or_else(|| vec![true; test.presses().len()], |s| s.kept.clone());
    let marks: Vec<(f64, bool)> = test
        .presses()
        .iter()
        .zip(kept)
        .map(|(&ms, counted)| (ms, counted))
        .collect();
    let (pulse, collected) = (test.pulse(now), test.presses().len());
    let (mean_ms, std_ms) = summary.map_or((None, None), |s| (Some(s.mean_ms), Some(s.std_ms)));
    let suggestion_ms = test.suggestion();

    begin(state, size);
    beetle_render::draw_calibrate(
        state.gpu_ui.ui.current(),
        &beetle_render::CalibrateFrame {
            viewport: &state.view.ui,
            phase,
            pulse,
            required: crate::calibration::REQUIRED_PRESSES,
            collected,
            window_ms: crate::calibration::MATCH_WINDOW_MS,
            marks: &marks,
            mean_ms,
            std_ms,
            suggestion_ms,
        },
    );
    overlays(state, true);
    finish(state);
}

/// The title screen: the menu over the living backdrop, and the quit prompt
/// when it is open.
pub fn title(state: &mut AppState, size: PhysicalSize<u32>) {
    begin(state, size);
    let vp = state.view.ui;
    beetle_render::draw_title(
        state.gpu_ui.ui.current(),
        &beetle_render::TitleFrame {
            viewport: &vp,
            selected: state.title_row,
            songs: state.songs.len(),
            version: env!("CARGO_PKG_VERSION"),
            modal: state.show_exit_modal,
        },
    );
    if state.show_exit_modal {
        beetle_render::draw_exit_modal(state.gpu_ui.ui.current(), &vp);
    }
    overlays(state, false);
    finish(state);
}

pub fn boot(state: &mut AppState, size: PhysicalSize<u32>) {
    let (title, status) = match state.library_job {
        LibraryJob::Startup => (
            strings::BOOT_TITLE_STARTUP(),
            strings::BOOT_STATUS_STARTUP(),
        ),
        LibraryJob::Rescan => (strings::BOOT_TITLE_RESCAN(), strings::BOOT_STATUS_RESCAN()),
    };
    begin(state, size);
    beetle_render::draw_boot(
        state.gpu_ui.ui.current(),
        &beetle_render::BootFrame {
            viewport: &state.view.ui,
            elapsed: state.library_started_at.elapsed().as_secs_f64(),
            title,
            status,
        },
    );
    finish(state);
}

pub fn loading(state: &mut AppState, size: PhysicalSize<u32>) {
    let chips = option_chips(state, state.loading_song.as_ref());
    let badge = if state.is_replay_playback {
        Some(strings::REPLAY())
    } else if state.is_auto_play {
        Some(strings::AUTO_PLAY())
    } else {
        None
    };
    let elapsed = state.loading_started_at.elapsed().as_secs_f64();
    let (jacket, ambient) = match state.loading_song.as_ref() {
        Some(song) => match state
            .stage_image_cache
            .get(&song.id)
            .and_then(|img| img.as_ref())
        {
            Some(img) => (
                state
                    .gpu_ui
                    .image(&mut state.d3d11, ImageKey::Stage(song.id), img),
                Some(img.average_color_sampled(6)),
            ),
            None => (None, None),
        },
        None => (None, None),
    };

    let notes = state
        .loading_song
        .as_ref()
        .map_or(0, |song| song.notes_count_for(state.play_ruleset(song).ln));
    begin(state, size);
    let backdrop_song = state
        .loading_song
        .as_ref()
        .map(|s| (s.id, jacket_seed(&s.title, &s.artist)));
    let now = Instant::now();
    show_loading_backdrop(state, backdrop_song, now);
    if let Some(song) = state.loading_song.as_ref() {
        beetle_render::draw_loading(
            state.gpu_ui.ui.current(),
            &beetle_render::LoadingFrame {
                viewport: &state.view.ui,
                song,
                notes,
                jacket,
                ambient,
                elapsed,
                status: strings::LOADING_STATUS(),
                option_chips: &chips,
                badge,
                now,
            },
        );
    }
    overlays(state, false);
    finish(state);
}

/// The backdrop of the result screen: the played chart's artwork, blurred
/// once (from the stage image the play already holds) and kept in
/// `stage_backdrop_cache`. A chart without artwork gets its placeholder.
fn show_result_backdrop(state: &mut AppState, now: Instant) {
    if state.gpu_ui.ui.current().lite {
        return;
    }
    let played = state.active_chart_id;
    if !state.stage_backdrop_cache.contains_key(&played) {
        let fresh = state
            .active_bga_image
            .as_ref()
            .and_then(beetle_render::backdrop::blur_backdrop);
        if let Some(px) = fresh {
            state.stage_backdrop_cache.insert(played, px);
        }
    }
    let ui = state.gpu_ui.ui.current();
    if let Some(px) = state.stage_backdrop_cache.get(&played) {
        ui.backdrop
            .show(ui.canvas.atlas_mut(), JacketKey::Chart(played), now, || {
                px.clone()
            });
    } else if state.active_bga_image.is_none() {
        if let Some(chart) = state.active_chart.as_ref() {
            let title = if chart.header.title.is_empty() {
                "Unknown Title"
            } else {
                chart.header.title.as_str()
            };
            let seed = jacket_seed(title, &chart.header.artist);
            ui.backdrop.show(
                ui.canvas.atlas_mut(),
                JacketKey::Placeholder(seed),
                now,
                || placeholder_backdrop(seed),
            );
        }
    }
}

pub fn result(state: &mut AppState, size: PhysicalSize<u32>) {
    crate::result_reveal::advance(state);
    let elapsed = crate::result_reveal::reveal_time(state);
    let unsaved = if state.is_replay_playback {
        Some(strings::REPLAY())
    } else if state.is_auto_play {
        Some(strings::AUTO_PLAY())
    } else if state.start_measure > 0 {
        Some(strings::PRACTICE())
    } else {
        None
    };
    let ln_label = state.active_ln.map(|rule| match state.active_hcn {
        true => format!("{} (HCN)", rule.as_str()),
        false => rule.as_str().to_string(),
    });
    let jacket = state.active_bga_image.as_ref().and_then(|img| {
        state.gpu_ui.image(
            &mut state.d3d11,
            ImageKey::Stage(state.active_chart_id),
            img,
        )
    });

    begin(state, size);
    let now = Instant::now();
    show_result_backdrop(state, now);
    if let (Some(chart), Some(judge)) = (&state.active_chart, &state.active_judge) {
        beetle_render::draw_result(
            state.gpu_ui.ui.current(),
            &beetle_render::ResultFrame {
                viewport: &state.view.ui,
                chart,
                score: judge.score(),
                previous_best: state.previous_best.as_ref(),
                update: state.score_update,
                elapsed,
                // Reduce Motion implies the flash-free result (shake and flash off).
                reduce_flashes: state.reduce_flashes || state.reduce_motion,
                jacket,
                unsaved_reason: unsaved,
                ln_label: ln_label.as_deref(),
                gauge_trend: &state.gauge_trend,
                now,
            },
        );
    }
    // TAB on the result opens the play options over it (the same panel as on
    // song select); ENTER there plays again with the options as changed.
    if state.show_option_modal {
        let lines = option_lines(state, play_options());
        let row = state.modal_row.min(play_options().len() - 1);
        let d = &play_options()[row];
        beetle_render::draw_options_panel(
            state.gpu_ui.ui.current(),
            &state.view.ui,
            &lines,
            row,
            (d.label, d.help),
            beetle_render::OptionsFooter::Retry,
        );
    }
    overlays(state, true);
    finish(state);
}

pub fn key_config(state: &mut AppState, size: PhysicalSize<u32>) {
    let mode = state.key_config_edit_mode;
    let layout = state.key_bindings.get(mode);
    let (scratch, form) = (
        state.view.skin.scratch_side_of(mode),
        state.view.skin.eight_k_form,
    );
    let keys: Vec<(beetle_core::Lane, Vec<&'static str>)> =
        screen_lanes_for(&state.view.skin, mode)
            .into_iter()
            .map(|lane| (lane, layout.key_names_for_lane(lane)))
            .collect();
    let lanes: Vec<beetle_render::KeyBinding> = keys
        .iter()
        .map(|(lane, keys)| beetle_render::KeyBinding {
            lane: *lane,
            label: lane_label(*lane),
            keys,
        })
        .collect();
    let preset = layout.preset.as_str();

    begin(state, size);
    beetle_render::draw_key_config(
        state.gpu_ui.ui.current(),
        &beetle_render::KeyConfigFrame {
            viewport: &state.view.ui,
            mode,
            lanes: &lanes,
            selected: state.selected_key_idx,
            rebinding: state.rebinding,
            layout: preset,
            scratch,
            form,
        },
    );
    overlays(state, true);
    finish(state);
}

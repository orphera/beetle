//! Draws and presents one frame of the current screen with the Canvas UI on
//! Direct3D 11 (ADR-026: the only renderer; WARP when there is no GPU).
//!
//! Each function gathers what its screen shows from `AppState`, draws it
//! with `beetle_render`, and presents. Screens render on every
//! `RedrawRequested`; the event loop decides how often that is.

use std::path::Path;
use std::time::Instant;

use beetle_core::{SongMetadata, SortMode};
use beetle_render::{
    strings, GpuBackend, OptionLine, SettingsFrame, ToastAnchor, ToastFrame, ToastKind, Ui,
};
use winit::dpi::PhysicalSize;

use crate::devtools;
use crate::gameplay::END_BANNER_SECONDS;
use crate::gpu_ui::{bga_texture, gameplay_bga_texture, ImageKey};
use crate::input::{lane_label, screen_lanes_for, KeyPreset};
use crate::options_table::{self, OptionDesc, PLAY_OPTIONS, SETTINGS};
use crate::state::{replay_path, AppScreen, AppState, LibraryJob};
use crate::transition::show_toast;

/// Starts a frame on the backbuffer and the UI.
fn begin(state: &mut AppState, size: PhysicalSize<u32>) {
    state.sync_screen_entry();
    let scale = state.view.viewport.scale;
    state
        .d3d11
        .begin_frame(size.width, size.height, [0.0, 0.0, 0.0, 1.0]);
    state.gpu_ui.ui.lite = state.d3d11.is_warp();
    state.gpu_ui.ui.pointer = state.cursor;
    state.gpu_ui.ui.begin(size.width, size.height, scale);
}

/// Submits the UI, serves screenshot / devtools capture requests (they read
/// the backbuffer, so before present) and presents.
fn finish(state: &mut AppState) {
    state.gpu_ui.ui.end(&mut state.d3d11);
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
                    strings::fill(strings::TOAST_SCREENSHOT_SAVED, &[&name]),
                );
            }
            Err(e) => show_toast(
                state,
                ToastKind::Error,
                strings::fill(strings::TOAST_SCREENSHOT_FAILED, &[&e.to_string()]),
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

/// The layers above a menu frame, drawn last: the fade-in after a screen
/// change, then the toast (`toasts` is false on Loading). Never on Gameplay.
fn overlays(state: &mut AppState, toasts: bool) {
    let now = Instant::now();
    if let Some(alpha) = state.screen_entry.fade_alpha(now) {
        beetle_render::draw_screen_fade(&mut state.gpu_ui.ui, alpha);
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
                &mut state.gpu_ui.ui,
                &state.view.viewport,
                &ToastFrame {
                    text: &toast.text,
                    kind: toast.kind,
                    alpha: pose.alpha,
                    slide: pose.slide,
                    anchor,
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
            strings::CHIP_GREEN,
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
    // BGA OFF: no texture lookups or uploads at all.
    let bga = state.bga_enabled.then(|| {
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
        .filter(|_| state.bga_enabled)
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
    // A song without BGA events or videos has no BGA box: the HUD uses the room.
    let has_bga = state.bga_enabled
        && (state
            .active_chart
            .as_ref()
            .is_some_and(|c| !c.bga_events.is_empty())
            || !state.video_players.is_empty());
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
            &mut state.gpu_ui.ui,
            &beetle_render::PlayFrame {
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
                has_bga,
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
            },
        );
    }
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
            Some(strings::REPLAY),
            strings::HUD_BACK_TO_SELECT.to_string(),
        );
    }
    if is_auto {
        return (
            Some(strings::AUTO_PLAY),
            strings::HUD_BACK_TO_SELECT.to_string(),
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
            return (None, strings::fill(strings::HUD_KEYS_NO_COVER, &[keys]));
        }
        KeyPreset::Custom => strings::KEYS_CUSTOM,
    };
    (None, strings::fill(strings::HUD_KEYS, &[keys]))
}

/// The sort menu's option names, in `SortMode::ALL` order.
fn sort_names() -> [&'static str; 5] {
    SortMode::ALL.map(sort_label)
}

/// Display names of the options that are stored under English enum names.
fn sort_label(mode: SortMode) -> &'static str {
    match mode {
        SortMode::Title => strings::SORT_TITLE,
        SortMode::Level => strings::SORT_LEVEL,
        SortMode::ClearLamp => strings::SORT_CLEAR_LAMP,
        SortMode::ScoreRate => strings::SORT_SCORE_RATE,
        SortMode::Bpm => strings::SORT_BPM,
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
        .then(|| option_lines(state, PLAY_OPTIONS));

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
                ..
            } => beetle_render::SelectRow::Group {
                title: title.as_str(),
                charts,
                selected: *selected,
            },
        })
        .collect();
    let vp = state.view.viewport;
    let ui: &mut Ui = &mut state.gpu_ui.ui;
    beetle_render::draw_song_select(
        ui,
        &beetle_render::SelectFrame {
            viewport: &vp,
            songs: &state.songs,
            rows: &rows,
            selected: state.selected_entry,
            scroll: state.list_scroll,
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
        },
    );
    if let Some(lines) = &option_panel {
        let d = &PLAY_OPTIONS[state.modal_row.min(PLAY_OPTIONS.len() - 1)];
        beetle_render::draw_options_modal(ui, &vp, lines, state.modal_row, (d.label, d.help));
    }
    if state.show_exit_modal {
        beetle_render::draw_exit_modal(ui, &vp);
    }
    if state.show_help {
        beetle_render::draw_help_overlay(ui, &vp);
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
    let lines = option_lines(state, SETTINGS);
    let row = state.settings_row.min(SETTINGS.len() - 1);
    begin(state, size);
    beetle_render::draw_settings(
        &mut state.gpu_ui.ui,
        &SettingsFrame {
            viewport: &state.view.viewport,
            lines: &lines,
            selected: row,
            help: SETTINGS[row].help,
        },
    );
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
        &mut state.gpu_ui.ui,
        &beetle_render::CalibrateFrame {
            viewport: &state.view.viewport,
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

pub fn boot(state: &mut AppState, size: PhysicalSize<u32>) {
    let (title, status) = match state.library_job {
        LibraryJob::Startup => (strings::BOOT_TITLE_STARTUP, strings::BOOT_STATUS_STARTUP),
        LibraryJob::Rescan => (strings::BOOT_TITLE_RESCAN, strings::BOOT_STATUS_RESCAN),
    };
    begin(state, size);
    beetle_render::draw_boot(
        &mut state.gpu_ui.ui,
        &beetle_render::BootFrame {
            viewport: &state.view.viewport,
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
        Some(strings::REPLAY)
    } else if state.is_auto_play {
        Some(strings::AUTO_PLAY)
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
    if let Some(song) = state.loading_song.as_ref() {
        beetle_render::draw_loading(
            &mut state.gpu_ui.ui,
            &beetle_render::LoadingFrame {
                viewport: &state.view.viewport,
                song,
                notes,
                jacket,
                ambient,
                elapsed,
                status: strings::LOADING_STATUS,
                option_chips: &chips,
                badge,
            },
        );
    }
    overlays(state, false);
    finish(state);
}

pub fn result(state: &mut AppState, size: PhysicalSize<u32>) {
    let elapsed = state.result_entered_at.elapsed().as_secs_f64();
    let unsaved = if state.is_replay_playback {
        Some(strings::REPLAY)
    } else if state.is_auto_play {
        Some(strings::AUTO_PLAY)
    } else if state.start_measure > 0 {
        Some(strings::PRACTICE)
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
    if let (Some(chart), Some(judge)) = (&state.active_chart, &state.active_judge) {
        beetle_render::draw_result(
            &mut state.gpu_ui.ui,
            &beetle_render::ResultFrame {
                viewport: &state.view.viewport,
                chart,
                score: judge.score(),
                previous_best: state.previous_best.as_ref(),
                update: state.score_update,
                elapsed,
                jacket,
                unsaved_reason: unsaved,
                ln_label: ln_label.as_deref(),
                gauge_trend: &state.gauge_trend,
            },
        );
    }
    // TAB on the result opens the play options over it (the same panel as on
    // song select); ENTER there plays again with the options as changed.
    if state.show_option_modal {
        let lines = option_lines(state, PLAY_OPTIONS);
        let row = state.modal_row.min(PLAY_OPTIONS.len() - 1);
        let d = &PLAY_OPTIONS[row];
        beetle_render::draw_options_panel(
            &mut state.gpu_ui.ui,
            &state.view.viewport,
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
        &mut state.gpu_ui.ui,
        &beetle_render::KeyConfigFrame {
            viewport: &state.view.viewport,
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

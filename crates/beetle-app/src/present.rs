//! Draws and presents one frame of the current screen with the Canvas UI on
//! Direct3D 11 (ADR-026: the only renderer; WARP when there is no GPU).
//!
//! Each function gathers what its screen shows from `AppState`, draws it
//! with `beetle_render`, and presents. Screens render on every
//! `RedrawRequested`; the event loop decides how often that is.

use std::path::Path;

use beetle_core::{LnOption, Ruleset, SongMetadata};
use beetle_render::{GpuBackend, Ui};
use winit::dpi::PhysicalSize;

use crate::devtools;
use crate::gpu_ui::{bga_texture, gameplay_bga_texture, ImageKey};
use crate::input::{lane_label, screen_lanes_for, KeyPreset};
use crate::state::{replay_path, AppState, LibraryJob};

/// Starts a frame on the backbuffer and the UI.
fn begin(state: &mut AppState, size: PhysicalSize<u32>) {
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
        let _ = devtools::save_backbuffer(&mut state.d3d11, &path);
    }
    if let Some(cap) = &mut state.capture {
        if cap.on_frame(state.screen, &mut state.d3d11) {
            state.should_exit_app = true;
        }
    }
    state.d3d11.end_frame();
}

/// "HI-SPEED 1100", "REGULAR", "GROOVE", and for a chart with long notes
/// "LN" / "CN" / "CN (HCN)": the options a play of `song` will use.
fn option_chips(state: &AppState, song: Option<&SongMetadata>) -> Vec<String> {
    let mut chips = vec![
        format!("HI-SPEED {:.0}", state.play_options.hi_speed),
        state.play_options.lane_modifier.as_str().to_string(),
        state.play_options.gauge_type.as_str().to_string(),
    ];
    if let Some(song) = song.filter(|s| s.ln_count > 0) {
        chips.push(Ruleset::resolve(song.ln_mode, state.ln_option()).label());
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
                hint,
                badge,
                pause: state
                    .is_gameplay_paused
                    .then_some(state.pause_selected_option),
            },
        );
    }
    finish(state);
}

/// Mode badge and key-hint line for the gameplay HUD.
fn gameplay_badge_and_hint(
    is_replay: bool,
    is_auto: bool,
    preset: KeyPreset,
) -> (Option<&'static str>, &'static str) {
    if is_replay {
        return (Some("REPLAY"), "ESC  Return to song select");
    }
    if is_auto {
        return (Some("AUTO PLAY"), "ESC  Return to song select");
    }
    let hint = match preset {
        KeyPreset::HomeRow => {
            "KEYS  Shift+S D F Space J K L    1/2 SPEED    F10/F11 COVER    ESC PAUSE"
        }
        KeyPreset::ArcadeZx => {
            "KEYS  Shift+Z S X D C F V    1/2 SPEED    F10/F11 COVER    ESC PAUSE"
        }
        KeyPreset::Pms9K => {
            "KEYS  S D F Space J K L ; '    1/2 SPEED    F10/F11 COVER    ESC PAUSE"
        }
        KeyPreset::Ue4K => "KEYS  S D L ;    1/2 SPEED    F10/F11 COVER    ESC PAUSE",
        KeyPreset::Ue6K => "KEYS  A S D L ; '    1/2 SPEED    F10/F11 COVER    ESC PAUSE",
        KeyPreset::Ue8K => "KEYS  A S D F K L ; '    1/2 SPEED    F10/F11 COVER    ESC PAUSE",
        KeyPreset::Ue8KTriggers => {
            "KEYS  LShift + S D F J K L + RShift    1/2 SPEED    F10/F11 COVER    ESC PAUSE"
        }
        KeyPreset::DoublePlay => "KEYS  Shift+ZSXDCFV / RShift+UIOP[]\\    1/2 SPEED    ESC PAUSE",
        KeyPreset::Custom => "KEYS  Custom layout    1/2 SPEED    F10/F11 COVER    ESC PAUSE",
    };
    (None, hint)
}

/// Song select plus its option / quit modals.
pub fn song_select(state: &mut AppState, size: PhysicalSize<u32>) {
    let selected_id = state.current_selected_song().map(|s| s.id);
    let ln_option = state.ln_option();
    let has_replay = state
        .current_selected_song()
        .is_some_and(|s| Path::new(&replay_path(s.id, s.score_rule(ln_option))).exists());
    let chips = option_chips(state, state.current_selected_song());
    let option_rows = state.show_option_modal.then(|| option_modal_rows(state));

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

    let folder = state.category_mode.title(&state.tables);

    begin(state, size);
    let vp = state.view.viewport;
    let ui: &mut Ui = &mut state.gpu_ui.ui;
    beetle_render::draw_song_select(
        ui,
        &beetle_render::SelectFrame {
            viewport: &vp,
            songs: &state.songs,
            visible: &state.filtered_indices,
            selected: state.selected_song_idx,
            scores: &state.score_store,
            tables: &state.tables,
            ln_option,
            folder: &folder,
            sort: state.sort_mode.as_str(),
            search: &state.search_query,
            search_active: state.is_search_active,
            jacket,
            ambient,
            option_chips: &chips,
            auto_play: state.is_auto_play,
            has_replay,
            preview_secs: state.preview.playing_for(),
        },
    );
    if let Some(rows) = &option_rows {
        beetle_render::draw_options_modal(ui, &vp, rows, state.modal_row);
    }
    if state.show_exit_modal {
        beetle_render::draw_exit_modal(ui, &vp);
    }
    finish(state);
}

/// (label, value) rows of the play options modal, in the order the option
/// handler indexes them (`state.modal_row`).
fn option_modal_rows(state: &AppState) -> Vec<(&'static str, String)> {
    let o = &state.play_options;
    vec![
        ("HI-SPEED", format!("{:.0} px/s", o.hi_speed)),
        ("MODIFIER", o.lane_modifier.as_str().to_string()),
        ("GAUGE", o.gauge_type.as_str().to_string()),
        (
            "LN MODE",
            match state.current_selected_song().filter(|s| s.ln_count > 0) {
                // AUTO says what it comes to for the highlighted song.
                Some(song) if o.ln == LnOption::Auto => {
                    format!("AUTO ({})", Ruleset::resolve(song.ln_mode, o.ln).label())
                }
                _ => o.ln.as_str().to_string(),
            },
        ),
        ("JUDGE OFFSET", format!("{:+.0} ms", o.judge_offset_ms)),
        (
            "MASTER VOLUME",
            format!("{:.0}%", state.master_volume * 100.0),
        ),
        (
            "PLAYFIELD",
            state.view.skin.field_position.as_str().to_string(),
        ),
        (
            "BGA",
            if state.bga_enabled { "ON" } else { "OFF" }.to_string(),
        ),
        ("TRACK BGA", state.track_bga.as_str().to_string()),
        ("DISPLAY MODE", state.display_mode.as_str().to_string()),
        ("RESOLUTION", state.current_resolution_label().to_string()),
        (
            "GRAPHICS",
            if state.gpu_backend == state.gpu_backend_at_start {
                state.gpu_backend.as_str().to_string()
            } else {
                format!("{} (AFTER RESTART)", state.gpu_backend.as_str())
            },
        ),
        (
            "TARGET FPS",
            if state.target_fps == 0 {
                "UNLIMITED".to_string()
            } else {
                format!("{} FPS", state.target_fps)
            },
        ),
        ("KEY LAYOUT", {
            // Layouts are per key mode; this row edits the selected song's.
            let mode = state.key_config_mode();
            format!(
                "{}  {}",
                beetle_render::theme::mode_label(mode),
                state.key_bindings.get(mode).preset.as_str()
            )
        }),
        (
            "AUTO PLAY",
            if state.is_auto_play { "ON" } else { "OFF" }.to_string(),
        ),
        ("START MEASURE", format!("M.{}", state.start_measure)),
    ]
}

pub fn boot(state: &mut AppState, size: PhysicalSize<u32>) {
    let (title, status) = match state.library_job {
        LibraryJob::Startup => ("STARTING UP", "Reading song library"),
        LibraryJob::Rescan => ("RESCANNING LIBRARY", "Scanning song folders"),
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
        Some("REPLAY")
    } else if state.is_auto_play {
        Some("AUTO PLAY")
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

    begin(state, size);
    if let Some(song) = state.loading_song.as_ref() {
        beetle_render::draw_loading(
            &mut state.gpu_ui.ui,
            &beetle_render::LoadingFrame {
                viewport: &state.view.viewport,
                song,
                jacket,
                ambient,
                elapsed,
                status: "Decoding keysounds and preparing audio",
                option_chips: &chips,
                badge,
            },
        );
    }
    finish(state);
}

pub fn result(state: &mut AppState, size: PhysicalSize<u32>) {
    let elapsed = state.result_entered_at.elapsed().as_secs_f64();
    let unsaved = if state.is_replay_playback {
        Some("REPLAY")
    } else if state.is_auto_play {
        Some("AUTO PLAY")
    } else if state.start_measure > 0 {
        Some("PRACTICE")
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
            },
        );
    }
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
    finish(state);
}

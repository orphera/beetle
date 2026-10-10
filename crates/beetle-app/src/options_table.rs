//! The option lists: the play options panel (per play, song select TAB / O)
//! and the Settings screen (set once, F4). One table per screen drives both
//! drawing (`present.rs`) and input (`handlers/options.rs`,
//! `handlers/settings.rs`). A row is an index into its table; nothing
//! numbers the rows.

use beetle_core::{GaugeType, LaneModifier, LnOption, Ruleset};
use beetle_render::{scratch_side_applies, strings, FieldPosition, ScratchSide};

use crate::config::{DisplayMode, GpuBackendSetting, TrackBgaSetting};
use crate::state::AppState;

/// Frame rates the target FPS row cycles through; 0 is unlimited.
const FPS_PRESETS: [u32; 6] = [60, 120, 144, 240, 360, 0];
const LANE_ORDER: [LaneModifier; 5] = [
    LaneModifier::Regular,
    LaneModifier::Mirror,
    LaneModifier::Random,
    LaneModifier::RRandom,
    LaneModifier::SRandom,
];
const GAUGE_ORDER: [GaugeType; 4] = [
    GaugeType::Easy,
    GaugeType::Groove,
    GaugeType::Hard,
    GaugeType::Hazard,
];
const LN_ORDER: [LnOption; 3] = [LnOption::Auto, LnOption::Ln, LnOption::Cn];

/// Which option a row is. Ids are unique across both tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionId {
    // Play options panel.
    HiSpeed,
    LaneCover,
    Modifier,
    Gauge,
    LnMode,
    AutoPlay,
    StartMeasure,
    // Settings screen.
    DisplayMode,
    Resolution,
    Graphics,
    TargetFps,
    MasterVolume,
    JudgeOffset,
    Playfield,
    Scratch,
    Bga,
    TrackBga,
    KeyLayout,
}

/// One row of an option table.
#[derive(Debug, Clone, Copy)]
pub struct OptionDesc {
    pub id: OptionId,
    /// Section the row sits under; a header is drawn where it changes.
    pub group: &'static str,
    /// Column of the Settings screen (the panel has one column).
    pub column: usize,
    pub label: &'static str,
    /// One sentence on what the option does (shown for the highlighted row).
    pub help: &'static str,
}

/// The per-play options, shown by the play options panel.
pub const PLAY_OPTIONS: &[OptionDesc] = &[
    OptionDesc {
        id: OptionId::HiSpeed,
        group: strings::GROUP_PLAY,
        column: 0,
        label: strings::ROW_HI_SPEED,
        help: strings::HELP_HI_SPEED,
    },
    OptionDesc {
        id: OptionId::LaneCover,
        group: strings::GROUP_PLAY,
        column: 0,
        label: strings::ROW_LANE_COVER,
        help: strings::HELP_LANE_COVER,
    },
    OptionDesc {
        id: OptionId::Modifier,
        group: strings::GROUP_PLAY,
        column: 0,
        label: strings::ROW_MODIFIER,
        help: strings::HELP_MODIFIER,
    },
    OptionDesc {
        id: OptionId::Gauge,
        group: strings::GROUP_PLAY,
        column: 0,
        label: strings::ROW_GAUGE,
        help: strings::HELP_GAUGE,
    },
    OptionDesc {
        id: OptionId::LnMode,
        group: strings::GROUP_PLAY,
        column: 0,
        label: strings::ROW_LN_MODE,
        help: strings::HELP_LN_MODE,
    },
    OptionDesc {
        id: OptionId::AutoPlay,
        group: strings::GROUP_SESSION,
        column: 0,
        label: strings::ROW_AUTO_PLAY,
        help: strings::HELP_AUTO_PLAY,
    },
    OptionDesc {
        id: OptionId::StartMeasure,
        group: strings::GROUP_SESSION,
        column: 0,
        label: strings::ROW_START_MEASURE,
        help: strings::HELP_START_MEASURE,
    },
];

/// The Settings screen, in table order (the two columns follow `column`).
pub const SETTINGS: &[OptionDesc] = &[
    OptionDesc {
        id: OptionId::DisplayMode,
        group: strings::GROUP_DISPLAY,
        column: 0,
        label: strings::ROW_DISPLAY_MODE,
        help: strings::HELP_DISPLAY_MODE,
    },
    OptionDesc {
        id: OptionId::Resolution,
        group: strings::GROUP_DISPLAY,
        column: 0,
        label: strings::ROW_RESOLUTION,
        help: strings::HELP_RESOLUTION,
    },
    OptionDesc {
        id: OptionId::Graphics,
        group: strings::GROUP_DISPLAY,
        column: 0,
        label: strings::ROW_GRAPHICS,
        help: strings::HELP_GRAPHICS,
    },
    OptionDesc {
        id: OptionId::TargetFps,
        group: strings::GROUP_DISPLAY,
        column: 0,
        label: strings::ROW_TARGET_FPS,
        help: strings::HELP_TARGET_FPS,
    },
    OptionDesc {
        id: OptionId::MasterVolume,
        group: strings::GROUP_AUDIO,
        column: 0,
        label: strings::ROW_MASTER_VOLUME,
        help: strings::HELP_MASTER_VOLUME,
    },
    OptionDesc {
        id: OptionId::JudgeOffset,
        group: strings::GROUP_JUDGE,
        column: 0,
        label: strings::ROW_JUDGE_OFFSET,
        help: strings::HELP_JUDGE_OFFSET,
    },
    OptionDesc {
        id: OptionId::Playfield,
        group: strings::GROUP_LAYOUT,
        column: 1,
        label: strings::ROW_PLAYFIELD,
        help: strings::HELP_PLAYFIELD,
    },
    OptionDesc {
        id: OptionId::Scratch,
        group: strings::GROUP_LAYOUT,
        column: 1,
        label: strings::ROW_SCRATCH,
        help: strings::HELP_SCRATCH,
    },
    OptionDesc {
        id: OptionId::Bga,
        group: strings::GROUP_LAYOUT,
        column: 1,
        label: strings::ROW_BGA,
        help: strings::HELP_BGA,
    },
    OptionDesc {
        id: OptionId::TrackBga,
        group: strings::GROUP_LAYOUT,
        column: 1,
        label: strings::ROW_TRACK_BGA,
        help: strings::HELP_TRACK_BGA,
    },
    OptionDesc {
        id: OptionId::KeyLayout,
        group: strings::GROUP_INPUT,
        column: 1,
        label: strings::ROW_KEY_LAYOUT,
        help: strings::HELP_KEY_LAYOUT,
    },
];

/// What ENTER does on a row that opens something instead of changing a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activation {
    /// Opens the key configuration for the selected song's mode.
    KeyConfig,
}

/// The activation of `id`, if the row has one. Other rows change their value
/// on ENTER, as the arrows do (forward).
pub fn activation(id: OptionId) -> Option<Activation> {
    match id {
        OptionId::KeyLayout => Some(Activation::KeyConfig),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Value steps (pure, so the ranges can be tested without a window)
// ---------------------------------------------------------------------------

/// Hi-speed in px/s: 25 steps between 100 and 1200.
pub fn hi_speed_next(v: f32, forward: bool) -> f32 {
    (v + if forward { 25.0 } else { -25.0 }).clamp(100.0, 1200.0)
}

/// Lane cover ratio: 5 % steps between 0 and 80 %, the range of the cover
/// keys in gameplay (F10 / F11).
pub fn lane_cover_next(ratio: f32, forward: bool) -> f32 {
    (ratio + if forward { 0.05 } else { -0.05 }).clamp(0.0, 0.80)
}

/// Master volume: 5 % steps between 0 and 200 %.
pub fn volume_next(v: f32, forward: bool) -> f32 {
    (v + if forward { 0.05 } else { -0.05 }).clamp(0.0, 2.0)
}

/// Judge offset in ms: 1 ms steps between -100 and +100.
pub fn judge_offset_next(v: f64, forward: bool) -> f64 {
    (v + if forward { 1.0 } else { -1.0 }).clamp(-100.0, 100.0)
}

/// Start measure: 1 step up to 200, down to 0.
pub fn start_measure_next(v: u32, forward: bool) -> u32 {
    if forward {
        (v + 1).min(200)
    } else {
        v.saturating_sub(1)
    }
}

/// The value after (or before) `cur` in `order`, wrapping around. A value
/// not in the list (a hand-edited FPS) counts as the 4th entry (240 FPS).
pub fn cycle<T: Copy + PartialEq>(order: &[T], cur: T, forward: bool) -> T {
    let n = order.len();
    let i = order.iter().position(|&v| v == cur).unwrap_or(3.min(n - 1));
    order[if forward {
        (i + 1) % n
    } else {
        (i + n - 1) % n
    }]
}

// ---------------------------------------------------------------------------
// Reading and changing the values in AppState
// ---------------------------------------------------------------------------

/// The row's value as the panel or the Settings screen shows it.
pub fn value(state: &AppState, id: OptionId) -> String {
    let o = &state.play_options;
    let on_off = |on: bool| {
        if on {
            strings::VALUE_ON
        } else {
            strings::VALUE_OFF
        }
    };
    match id {
        OptionId::HiSpeed => {
            strings::fill(strings::VALUE_PX_PER_SEC, &[&format!("{:.0}", o.hi_speed)])
        }
        OptionId::LaneCover => format!("{:.0}%", state.view.skin.lane_cover_ratio * 100.0),
        OptionId::Modifier => o.lane_modifier.as_str().to_string(),
        OptionId::Gauge => o.gauge_type.as_str().to_string(),
        OptionId::LnMode => match state.current_selected_song().filter(|s| s.ln_count > 0) {
            // AUTO says what it comes to for the highlighted song.
            Some(song) if o.ln == LnOption::Auto => strings::fill(
                strings::VALUE_AUTO_RESOLVED,
                &[&Ruleset::resolve(song.ln_mode, o.ln).label()],
            ),
            _ => o.ln.as_str().to_string(),
        },
        OptionId::AutoPlay => on_off(state.is_auto_play).to_string(),
        OptionId::StartMeasure => format!("M.{}", state.start_measure),
        OptionId::DisplayMode => display_mode_label(state.display_mode).to_string(),
        OptionId::Resolution => state.current_resolution_label().to_string(),
        OptionId::Graphics => {
            if state.gpu_backend == state.gpu_backend_at_start {
                gpu_label(state.gpu_backend).to_string()
            } else {
                strings::fill(
                    strings::VALUE_AFTER_RESTART,
                    &[gpu_label(state.gpu_backend)],
                )
            }
        }
        OptionId::TargetFps => {
            if state.target_fps == 0 {
                strings::VALUE_UNLIMITED.to_string()
            } else {
                strings::fill(strings::VALUE_FPS, &[&state.target_fps.to_string()])
            }
        }
        OptionId::MasterVolume => format!("{:.0}%", state.master_volume * 100.0),
        OptionId::JudgeOffset => format!("{:+.0} ms", o.judge_offset_ms),
        OptionId::Playfield => field_label(state.view.skin.field_position).to_string(),
        OptionId::Scratch => {
            let mode = state.key_config_mode();
            if scratch_side_applies(mode) {
                scratch_label(state.view.skin.scratch_side_of(mode)).to_string()
            } else {
                strings::VALUE_NOT_APPLICABLE.to_string()
            }
        }
        OptionId::Bga => on_off(state.bga_enabled).to_string(),
        OptionId::TrackBga => track_bga_label(state.track_bga).to_string(),
        OptionId::KeyLayout => {
            // Layouts are per key mode; this row shows the selected song's.
            let mode = state.key_config_mode();
            format!(
                "{}  {}",
                beetle_render::theme::mode_label(mode),
                state.key_bindings.get(mode).preset.as_str()
            )
        }
    }
}

/// Changes the row's value one step (`forward`: the right arrow, or ENTER on
/// a row without an activation). The caller saves the config.
pub fn step(state: &mut AppState, id: OptionId, forward: bool) {
    match id {
        OptionId::HiSpeed => {
            let o = &mut state.play_options;
            o.hi_speed = hi_speed_next(o.hi_speed, forward);
            state.view.skin.hi_speed = o.hi_speed;
        }
        OptionId::LaneCover => {
            let skin = &mut state.view.skin;
            skin.lane_cover_ratio = lane_cover_next(skin.lane_cover_ratio, forward);
        }
        OptionId::Modifier => {
            state.play_options.lane_modifier =
                cycle(&LANE_ORDER, state.play_options.lane_modifier, forward);
        }
        OptionId::Gauge => {
            state.play_options.gauge_type =
                cycle(&GAUGE_ORDER, state.play_options.gauge_type, forward);
        }
        OptionId::LnMode => {
            state.play_options.ln = cycle(&LN_ORDER, state.play_options.ln, forward);
            state.resort_songs();
        }
        OptionId::AutoPlay => state.is_auto_play = !state.is_auto_play,
        OptionId::StartMeasure => {
            state.start_measure = start_measure_next(state.start_measure, forward);
        }
        OptionId::DisplayMode => {
            state.display_mode = if forward {
                state.display_mode.next()
            } else {
                state.display_mode.prev()
            };
            state.apply_display_mode();
        }
        OptionId::Resolution => state.cycle_resolution(forward),
        OptionId::Graphics => {
            // Takes effect on the next start (see AppState::d3d11).
            state.gpu_backend = if forward {
                state.gpu_backend.next()
            } else {
                state.gpu_backend.prev()
            };
        }
        OptionId::TargetFps => {
            state.target_fps = cycle(&FPS_PRESETS, state.target_fps, forward);
        }
        OptionId::MasterVolume => {
            state.master_volume = volume_next(state.master_volume, forward);
            if let Some(audio) = &mut state.audio_engine {
                let _ = audio.set_master_volume(state.master_volume);
            }
        }
        OptionId::JudgeOffset => {
            let o = &mut state.play_options;
            o.judge_offset_ms = judge_offset_next(o.judge_offset_ms, forward);
        }
        OptionId::Playfield => {
            let skin = &mut state.view.skin;
            let position = if forward {
                skin.field_position.next()
            } else {
                skin.field_position.prev()
            };
            skin.set_field_layout(position, skin.scratch_side_of(skin.play_mode));
        }
        OptionId::Scratch => toggle_scratch(state),
        OptionId::Bga => state.bga_enabled = !state.bga_enabled,
        OptionId::TrackBga => {
            state.track_bga = if forward {
                state.track_bga.next()
            } else {
                state.track_bga.prev()
            };
        }
        OptionId::KeyLayout => {
            // The preset cycles in either direction.
            let mode = state.key_config_mode();
            state.key_bindings.get_mut(mode).cycle_preset(mode);
            state.sync_eight_k_form();
        }
    }
}

/// Puts the scratch lane of the selected song's mode on the other edge.
/// Modes without a scratch lane (4K, 6K, 9K, ...) do nothing.
fn toggle_scratch(state: &mut AppState) {
    let mode = state.key_config_mode();
    if !scratch_side_applies(mode) {
        return;
    }
    let side = state.view.skin.scratch_side_of(mode).toggle();
    state.view.skin.set_scratch_side(mode, side);
}

fn display_mode_label(mode: DisplayMode) -> &'static str {
    match mode {
        DisplayMode::Windowed => strings::DISPLAY_WINDOWED,
        DisplayMode::Borderless => strings::DISPLAY_BORDERLESS,
        DisplayMode::ExclusiveFullscreen => strings::DISPLAY_FULLSCREEN,
    }
}

fn gpu_label(backend: GpuBackendSetting) -> &'static str {
    match backend {
        GpuBackendSetting::Auto => strings::GPU_AUTO,
        GpuBackendSetting::Warp => strings::GPU_WARP,
    }
}

fn track_bga_label(bga: TrackBgaSetting) -> &'static str {
    match bga {
        TrackBgaSetting::Off => strings::TRACK_BGA_OFF,
        TrackBgaSetting::Low => strings::TRACK_BGA_LOW,
        TrackBgaSetting::Medium => strings::TRACK_BGA_MEDIUM,
        TrackBgaSetting::High => strings::TRACK_BGA_HIGH,
    }
}

fn field_label(position: FieldPosition) -> &'static str {
    match position {
        FieldPosition::Left => strings::SIDE_LEFT,
        FieldPosition::Center => strings::VALUE_CENTER,
        FieldPosition::Right => strings::SIDE_RIGHT,
    }
}

fn scratch_label(side: ScratchSide) -> &'static str {
    match side {
        ScratchSide::Left => strings::SIDE_LEFT,
        ScratchSide::Right => strings::SIDE_RIGHT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> impl Iterator<Item = &'static OptionDesc> {
        PLAY_OPTIONS.iter().chain(SETTINGS.iter())
    }

    #[test]
    fn ids_are_unique_across_both_tables() {
        let mut seen = Vec::new();
        for d in all() {
            assert!(!seen.contains(&d.id), "{:?} listed twice", d.id);
            seen.push(d.id);
        }
        assert_eq!(seen.len(), PLAY_OPTIONS.len() + SETTINGS.len());
    }

    #[test]
    fn every_row_has_a_label_and_a_help_sentence() {
        for d in all() {
            assert!(!d.label.is_empty(), "{:?}", d.id);
            assert!(
                d.help.chars().count() > 10,
                "{:?} needs a help sentence",
                d.id
            );
        }
    }

    #[test]
    fn play_options_and_settings_do_not_share_rows() {
        for p in PLAY_OPTIONS {
            assert!(!SETTINGS.iter().any(|s| s.id == p.id), "{:?}", p.id);
        }
        for s in SETTINGS {
            assert!(!PLAY_OPTIONS.iter().any(|p| p.id == s.id), "{:?}", s.id);
        }
    }

    #[test]
    fn play_options_are_one_column_and_settings_two() {
        assert!(PLAY_OPTIONS.iter().all(|d| d.column == 0));
        assert!(SETTINGS.iter().any(|d| d.column == 1));
    }

    #[test]
    fn groups_are_contiguous() {
        // A section header is drawn where the group changes, so a group must
        // not come back after another one started.
        for table in [PLAY_OPTIONS, SETTINGS] {
            let mut seen: Vec<&str> = Vec::new();
            for d in table {
                if seen.last() != Some(&d.group) {
                    assert!(!seen.contains(&d.group), "{} split up", d.group);
                    seen.push(d.group);
                }
            }
        }
    }

    #[test]
    fn only_key_layout_has_an_activation() {
        for d in all() {
            let expected = (d.id == OptionId::KeyLayout).then_some(Activation::KeyConfig);
            assert_eq!(activation(d.id), expected, "{:?}", d.id);
        }
    }

    #[test]
    fn hi_speed_steps_are_reversible_inside_the_range() {
        assert_eq!(hi_speed_next(1100.0, true), 1125.0);
        assert_eq!(hi_speed_next(1125.0, false), 1100.0);
        assert_eq!(hi_speed_next(1200.0, true), 1200.0);
        assert_eq!(hi_speed_next(100.0, false), 100.0);
    }

    #[test]
    fn lane_cover_is_five_percent_steps_from_0_to_80() {
        let mut r = 0.0;
        for _ in 0..20 {
            r = lane_cover_next(r, true);
        }
        assert!((r - 0.80).abs() < 1e-6, "top is 80 %, got {r}");
        assert!((lane_cover_next(r, false) - 0.75).abs() < 1e-6);
        assert!(lane_cover_next(0.0, false).abs() < 1e-6);
        // Forward then back returns to where it started.
        let start = 0.25;
        assert!((lane_cover_next(lane_cover_next(start, true), false) - start).abs() < 1e-6);
    }

    #[test]
    fn volume_and_offset_steps_are_reversible() {
        let v = volume_next(0.8, true);
        assert!((v - 0.85).abs() < 1e-6);
        assert!((volume_next(v, false) - 0.8).abs() < 1e-6);
        assert!((volume_next(2.0, true) - 2.0).abs() < 1e-6);
        assert_eq!(judge_offset_next(0.0, true), 1.0);
        assert_eq!(judge_offset_next(1.0, false), 0.0);
        assert_eq!(judge_offset_next(100.0, true), 100.0);
        assert_eq!(judge_offset_next(-100.0, false), -100.0);
    }

    #[test]
    fn start_measure_steps_and_clamps() {
        assert_eq!(start_measure_next(0, false), 0);
        assert_eq!(start_measure_next(4, true), 5);
        assert_eq!(start_measure_next(5, false), 4);
        assert_eq!(start_measure_next(200, true), 200);
    }

    #[test]
    fn cycle_wraps_both_ways() {
        assert_eq!(cycle(&[1, 2, 3], 3, true), 1);
        assert_eq!(cycle(&[1, 2, 3], 1, false), 3);
        assert_eq!(cycle(&[60, 120, 144, 240, 360, 0], 75, true), 360);
    }

    #[test]
    fn fps_presets_cycle_through_unlimited() {
        assert_eq!(cycle(&FPS_PRESETS, 360, true), 0);
        assert_eq!(cycle(&FPS_PRESETS, 0, true), 60);
        assert_eq!(cycle(&FPS_PRESETS, 60, false), 0);
    }
}

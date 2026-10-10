//! Screens drawn with `Ui` (Canvas + TextEngine + generated Skin).

pub mod boot;
pub mod calibrate;
pub mod key_bind;
pub mod loading;
mod overlay;
pub mod play;
pub mod select;
pub mod settings;
pub mod stage_result;
mod widgets;

pub use boot::{draw_boot, BootFrame};
pub use calibrate::{draw_calibrate, CalibrateFrame, CalibratePhase};
pub use key_bind::{draw_key_config, KeyBinding, KeyConfigFrame, Rebind, KEY_MODES};
pub use loading::{draw_loading, LoadingFrame};
pub use overlay::{draw_screen_fade, draw_toast, ToastFrame, ToastKind};
pub use play::{draw_gameplay, PlayFrame, SizedTexture};
pub use select::{
    centred_start, clamp_into_window, draw_drop_overlay, draw_exit_modal, draw_help_overlay,
    draw_options_modal, draw_song_select, filter_items, scroll_by, visible_rows, window_start,
    FilterBar, FilterItem, SelectFrame, SelectRow, SortMenu, LAMP_COUNT,
};
pub use settings::{draw_settings, SettingsFrame};
pub use stage_result::{draw_result, ResultFrame};
pub use widgets::OptionLine;

#[cfg(test)]
mod tests {
    /// Screens ported to the Canvas UI take colors only from `theme.rs`
    /// (docs/plans/2026-10-04-d3d11-ui-rebuild.md, P4 rules).
    #[test]
    fn ported_screens_use_theme_colors_only() {
        for (name, src) in [
            ("play.rs", include_str!("play.rs")),
            ("select.rs", include_str!("select.rs")),
            ("stage_result.rs", include_str!("stage_result.rs")),
            ("widgets.rs", include_str!("widgets.rs")),
            ("key_bind.rs", include_str!("key_bind.rs")),
            ("loading.rs", include_str!("loading.rs")),
            ("boot.rs", include_str!("boot.rs")),
            ("overlay.rs", include_str!("overlay.rs")),
            ("settings.rs", include_str!("settings.rs")),
            ("calibrate.rs", include_str!("calibrate.rs")),
        ] {
            assert!(
                !src.contains(concat!("ColorRgba", "::new")),
                "{name} builds colors inline; add them to theme.rs"
            );
        }
    }
}

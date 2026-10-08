//! Screens drawn with `Ui` (Canvas + TextEngine + generated Skin).

pub mod key_bind;
pub mod loading;
pub mod play;
pub mod select;
pub mod stage_result;
mod widgets;

pub use key_bind::{draw_key_config, KeyBinding, KeyConfigFrame, Rebind, KEY_MODES};
pub use loading::{draw_loading, LoadingFrame};
pub use play::{draw_gameplay, PlayFrame, SizedTexture};
pub use stage_result::{draw_result, ResultFrame};
pub use select::{draw_exit_modal, draw_options_modal, draw_song_select, SelectFrame, OPTION_COLUMN_BREAK, OPTION_SECTIONS};

#[cfg(test)]
mod tests {
    /// Screens ported to the Canvas UI take colors only from `theme.rs`
    /// (docs/plans/2026-10-04-d3d11-ui-rebuild.md, P4 rules).
    #[test]
    fn ported_screens_use_theme_colors_only() {
        for (name, src) in [("play.rs", include_str!("play.rs")), ("select.rs", include_str!("select.rs")), ("stage_result.rs", include_str!("stage_result.rs")), ("widgets.rs", include_str!("widgets.rs")), ("key_bind.rs", include_str!("key_bind.rs")), ("loading.rs", include_str!("loading.rs"))] {
            assert!(!src.contains(concat!("ColorRgba", "::new")), "{name} builds colors inline; add them to theme.rs");
        }
    }
}

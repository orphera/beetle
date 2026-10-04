//! Screen rendering implementations for SoftwareRenderer.

pub mod gameplay;
pub mod gameplay_gpu;
pub mod key_config;
pub mod modals;
pub mod play;
pub mod result;
pub mod select;
pub mod song_select;

pub use gameplay_gpu::render_gameplay_gpu;
pub use play::{draw_gameplay, PlayFrame, SizedTexture};
pub use select::{draw_exit_modal, draw_options_modal, draw_song_select, SelectFrame, OPTION_SECTIONS};

#[cfg(test)]
mod tests {
    /// Screens ported to the Canvas UI take colors only from `theme.rs`
    /// (docs/plans/2026-10-04-d3d11-ui-rebuild.md, P4 rules).
    #[test]
    fn ported_screens_use_theme_colors_only() {
        for (name, src) in [("play.rs", include_str!("play.rs")), ("select.rs", include_str!("select.rs"))] {
            assert!(!src.contains(concat!("ColorRgba", "::new")), "{name} builds colors inline; add them to theme.rs");
        }
    }
}

//! `Ui`: everything a screen needs to draw — Canvas, text engine and the
//! generated skin — kept consistent with the current UI scale.

use crate::art::Skin;
use crate::backdrop::BlurBackdrop;
use crate::backend::GpuBackend;
use crate::canvas::{Canvas, Rect};
use crate::hit::{self, Hit, HitId};
use crate::jacket::JacketAtlas;
use crate::text::TextEngine;

pub struct Ui {
    pub canvas: Canvas,
    pub text: TextEngine,
    pub skin: Skin,
    /// Low-fill mode: screens skip purely decorative full-screen layers
    /// (ambient glow, grain, vignette). Set when running on WARP, where every
    /// full-screen layer is CPU-rasterized.
    pub lite: bool,
    /// Click regions of the frame being drawn (cleared by `begin`). Hit-tested
    /// by the app against the last presented frame.
    pub hits: Vec<Hit>,
    /// Cursor position in physical pixels, set by the app before drawing;
    /// `None` when the cursor is outside the window.
    pub pointer: Option<(f32, f32)>,
    /// Where the song select search caret was drawn this frame (set while the
    /// search is open, cleared by `begin`). The app passes it to the OS as the
    /// IME candidate position so the candidate list opens under the caret.
    pub ime_caret: Option<Rect>,
    /// Ambient clock for decorative motion (menu backdrop), in seconds within
    /// `AMBIENT_LOOP_SECS`. Set by the app before drawing; it is not the audio
    /// clock and nothing that is judged or timed reads it.
    pub clock: f32,
    /// Song jacket thumbnails, in a slot grid of the shared atlas (`jacket`).
    pub jackets: JacketAtlas,
    /// The blurred jacket backdrop of the menu screens (`backdrop`), two
    /// slots in the shared atlas with a crossfade between them.
    pub backdrop: BlurBackdrop,
}

/// Length of the ambient clock's loop, in seconds. Every decorative period
/// divides it, so the wrap is invisible, and f32 keeps full precision
/// however long the app runs.
pub const AMBIENT_LOOP_SECS: f32 = 600.0;

/// Maps seconds since the app started onto the ambient loop.
pub fn ambient_time(app_elapsed_secs: f64) -> f32 {
    app_elapsed_secs.rem_euclid(AMBIENT_LOOP_SECS as f64) as f32
}

impl Ui {
    /// Builds the atlas, fonts and skin for `scale` (viewport height / 720).
    pub fn new(scale: f32) -> Self {
        let mut canvas = Canvas::default();
        let skin = Skin::generate(canvas.atlas_mut(), scale);
        let jackets = JacketAtlas::new(canvas.atlas_mut());
        Self {
            canvas,
            text: TextEngine::new(),
            skin,
            lite: false,
            hits: Vec::new(),
            pointer: None,
            ime_caret: None,
            clock: 0.0,
            jackets,
            backdrop: BlurBackdrop::new(),
        }
    }

    /// Starts a frame. If the UI scale changed (window resized), the atlas is
    /// rebuilt so sprites and glyphs stay pixel-exact at the new size.
    pub fn begin(&mut self, width: u32, height: u32, scale: f32) {
        if (scale - self.skin.scale).abs() > 0.01 {
            self.canvas.reset_atlas();
            self.text.clear_cache();
            self.skin = Skin::generate(self.canvas.atlas_mut(), scale);
            // The reset emptied the atlas: the jacket grid is reserved again
            // and its residents are gone.
            self.jackets = JacketAtlas::new(self.canvas.atlas_mut());
            self.backdrop.reset();
        }
        self.canvas.begin(width, height);
        self.hits.clear();
        self.ime_caret = None;
    }

    /// Records a click region for this frame.
    pub fn hit(&mut self, rect: Rect, id: HitId) {
        self.hits.push(Hit { rect, id });
    }

    /// Whether the cursor is over `rect` in this frame.
    pub fn hovered(&self, rect: Rect) -> bool {
        hit::hovered(self.pointer, rect)
    }

    /// Submits the frame; returns the number of draw calls.
    pub fn end(&mut self, backend: &mut dyn GpuBackend) -> usize {
        self.canvas.end(backend)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jacket::JacketKey;

    #[test]
    fn scale_change_regenerates_without_leaking_atlas_space() {
        let mut ui = Ui::new(1.0);
        let style = crate::text::TextStyle::new(16.0);
        for i in 0..20 {
            let scale = if i % 2 == 0 { 1.0 } else { 1.5 };
            ui.begin(1280, 720, scale);
            let (canvas, text) = (&mut ui.canvas, &mut ui.text);
            text.draw(canvas, "Beetle 한글 テスト", 0.0, 20.0, &style);
        }
        assert_eq!(ui.canvas.atlas().page_count(), 1);
        assert_eq!(ui.skin.scale, 1.5);
    }

    #[test]
    fn jacket_grid_and_twelve_thumbs_stay_on_page_zero_at_every_ui_scale() {
        // 0.75 to 3.0 in steps of 0.25: the grid is reserved on page 0, the
        // placeholders and the text land there too, so no draw splits.
        for step in 0..=9 {
            let scale = 0.75 + 0.25 * step as f32;
            let mut ui = Ui::new(scale);
            ui.begin(1280, 720, scale);
            assert_eq!(ui.jackets.page(), Some(0), "grid at scale {scale}");
            for seed in 1..=12u64 {
                assert!(ui.jackets.ensure_placeholder(ui.canvas.atlas_mut(), seed));
            }
            let style = crate::text::TextStyle::new(16.0 * scale);
            let (canvas, text) = (&mut ui.canvas, &mut ui.text);
            text.draw(canvas, "Beetle 한글 テスト", 0.0, 20.0, &style);
            for seed in 1..=12u64 {
                let r = ui
                    .jackets
                    .get(JacketKey::Placeholder(seed))
                    .expect("resident");
                assert_eq!(r.page, 0, "thumb {seed} at scale {scale}");
            }
            assert_eq!(ui.canvas.atlas().page_count(), 1, "scale {scale}");
        }
    }
}

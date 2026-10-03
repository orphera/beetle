//! `Ui`: everything a screen needs to draw — Canvas, text engine and the
//! generated skin — kept consistent with the current UI scale.

use crate::art::Skin;
use crate::backend::GpuBackend;
use crate::canvas::Canvas;
use crate::text::TextEngine;

pub struct Ui {
    pub canvas: Canvas,
    pub text: TextEngine,
    pub skin: Skin,
}

impl Ui {
    /// Builds the atlas, fonts and skin for `scale` (viewport height / 720).
    pub fn new(scale: f32) -> Self {
        let mut canvas = Canvas::default();
        let skin = Skin::generate(canvas.atlas_mut(), scale);
        Self {
            canvas,
            text: TextEngine::new(),
            skin,
        }
    }

    /// Starts a frame. If the UI scale changed (window resized), the atlas is
    /// rebuilt so sprites and glyphs stay pixel-exact at the new size.
    pub fn begin(&mut self, width: u32, height: u32, scale: f32) {
        if (scale - self.skin.scale).abs() > 0.01 {
            self.canvas.reset_atlas();
            self.text.clear_cache();
            self.skin = Skin::generate(self.canvas.atlas_mut(), scale);
        }
        self.canvas.begin(width, height);
    }

    /// Submits the frame; returns the number of draw calls.
    pub fn end(&mut self, backend: &mut dyn GpuBackend) -> usize {
        self.canvas.end(backend)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

use crate::skin::SkinConfig;
use beetle_core::{BmsChart, ScoreTracker, TimingModel};
use tiny_skia::{Color, Pixmap};

/// Software 2D renderer powered by tiny-skia.
pub struct SoftwareRenderer {
    pixmap: Pixmap,
    skin: SkinConfig,
}

impl SoftwareRenderer {
    pub fn new(width: u32, height: u32, skin: SkinConfig) -> Option<Self> {
        let pixmap = Pixmap::new(width.max(1), height.max(1))?;
        Some(Self { pixmap, skin })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 && (self.pixmap.width() != width || self.pixmap.height() != height) {
            if let Some(new_pixmap) = Pixmap::new(width, height) {
                self.pixmap = new_pixmap;
            }
        }
    }

    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    /// Access the raw RGBA8 pixel data for blitting via softbuffer.
    pub fn data(&self) -> &[u8] {
        self.pixmap.data()
    }

    /// Clear frame with background color.
    pub fn clear(&mut self) {
        let bg = self.skin.bg_color;
        self.pixmap.fill(Color::from_rgba8(bg.r, bg.g, bg.b, bg.a));
    }

    /// Renders a single gameplay frame based on current audio time.
    pub fn render_gameplay(
        &mut self,
        _chart: &BmsChart,
        _timing: &TimingModel,
        _audio_time_seconds: f64,
        _score: &ScoreTracker,
    ) {
        self.clear();
        // TODO: Full software 2D note rendering in Phase 3
    }
}

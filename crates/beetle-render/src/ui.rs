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
    /// How many times `begin` rebuilt the atlas for a new scale.
    pub rebuilds: u32,
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
        let mut canvas = Canvas::new(crate::canvas::atlas_page_size_for_scale(scale));
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
            rebuilds: 0,
        }
    }

    /// Starts a frame. If the UI scale changed (window resized), the atlas is
    /// rebuilt so sprites and glyphs stay pixel-exact at the new size.
    pub fn begin(&mut self, width: u32, height: u32, scale: f32) {
        // A new scale, or a page size the canvas no longer wants (the backend
        // refused a large page), rebuilds everything at the page size it wants.
        let page_changed = self.canvas.page_size_for(scale) != self.canvas.atlas().page_size();
        if (scale - self.skin.scale).abs() > 0.01 || page_changed {
            self.rebuilds += 1;
            self.canvas.reset_atlas_for_scale(scale);
            self.text.clear_cache();
            self.skin = Skin::generate(self.canvas.atlas_mut(), scale);
            // The reset emptied the atlas: the jacket grid is reserved again
            // and its residents are uploaded again from their CPU copies.
            let residents = self.jackets.residents();
            self.jackets = JacketAtlas::new(self.canvas.atlas_mut());
            self.jackets.restore(self.canvas.atlas_mut(), residents);
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

/// The app's UIs: one for the menus and, when its scale differs, one for
/// gameplay. Menus draw at the window scale times the UI scale, gameplay at the
/// window scale alone, so a single atlas would be rebuilt on every move between
/// them (skin, glyph cache, jacket grid, and possibly the page size). Each
/// keeps its own atlas instead, so a screen change rebuilds nothing; only a
/// resize or a UI scale change does. When the scales match, gameplay draws with
/// the menu UI and there is one atlas, as before.
pub struct UiSet {
    menu: Ui,
    play: Option<Ui>,
    in_play: bool,
}

impl UiSet {
    /// The menu UI at `menu_scale`; the gameplay UI is made on first use.
    pub fn new(menu_scale: f32) -> Self {
        Self {
            menu: Ui::new(menu_scale),
            play: None,
            in_play: false,
        }
    }

    /// Starts a frame on the UI of the screen kind and returns it. `menu_scale`
    /// and `play_scale` are the scales the menus and gameplay draw at.
    pub fn begin(
        &mut self,
        gameplay: bool,
        width: u32,
        height: u32,
        menu_scale: f32,
        play_scale: f32,
    ) -> &mut Ui {
        self.in_play = gameplay && (play_scale - menu_scale).abs() > 0.01;
        if self.in_play {
            let play = self.play.get_or_insert_with(|| Ui::new(play_scale));
            play.begin(width, height, play_scale);
            play
        } else {
            self.menu.begin(width, height, menu_scale);
            &mut self.menu
        }
    }

    /// The UI of the frame in progress, or of the last frame.
    pub fn current(&mut self) -> &mut Ui {
        match (self.in_play, self.play.as_mut()) {
            (true, Some(play)) => play,
            _ => &mut self.menu,
        }
    }

    /// The menu UI, whatever the frame is: jacket and backdrop state belongs to it.
    pub fn menu(&mut self) -> &mut Ui {
        &mut self.menu
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jacket::{JacketKey, THUMB_PX};

    /// A backend that refuses page textures wider than `max` texels.
    struct SizeLimited {
        max: u32,
    }

    impl GpuBackend for SizeLimited {
        fn begin_frame(&mut self, _: u32, _: u32, _: [f32; 4]) {}
        fn create_texture(
            &mut self,
            w: u32,
            _: u32,
            _: &[u8],
        ) -> Option<crate::backend::TextureId> {
            (w <= self.max).then_some(crate::backend::TextureId(1))
        }
        fn update_texture(&mut self, _: crate::backend::TextureId, _: u32, _: u32, _: &[u8]) {}
        fn update_texture_rect(
            &mut self,
            _: crate::backend::TextureId,
            _: u32,
            _: u32,
            _: [u32; 4],
            _: &[u8],
        ) {
        }
        fn destroy_texture(&mut self, _: crate::backend::TextureId) {}
        fn draw_batch(
            &mut self,
            _: &[crate::backend::Vertex2D],
            _: &[u16],
            _: Option<crate::backend::TextureId>,
            _: crate::backend::BlendMode,
        ) {
        }
        fn end_frame(&mut self) {}
        fn resize(&mut self, _: u32, _: u32) {}
        fn backend_name(&self) -> &'static str {
            "size-limited"
        }
    }

    #[test]
    fn a_refused_large_page_falls_back_to_the_default_size() {
        // Scale 4.0 is above LARGE_PAGE_SCALE: the UI starts on 4096 pages.
        let mut ui = Ui::new(4.0);
        assert_eq!(ui.canvas.atlas().page_size(), 4096);
        let mut gpu = SizeLimited { max: 2048 };

        ui.begin(1280, 720, 4.0);
        ui.canvas
            .fill_rect(Rect::new(0.0, 0.0, 10.0, 10.0), crate::theme::WHITE);
        assert_eq!(ui.end(&mut gpu), 0, "the refused frame draws nothing");

        // The next frame rebuilds at the default page size and draws.
        ui.begin(1280, 720, 4.0);
        assert_eq!(ui.canvas.atlas().page_size(), 2048);
        assert_eq!(ui.rebuilds, 1);
        ui.canvas
            .fill_rect(Rect::new(0.0, 0.0, 10.0, 10.0), crate::theme::WHITE);
        assert!(ui.end(&mut gpu) > 0, "the fallback frame draws");
    }

    #[test]
    fn switching_between_menus_and_gameplay_rebuilds_no_atlas() {
        // UI 120 percent in a 1280x720 window: menus draw at 1.2, gameplay at 1.0.
        let mut set = UiSet::new(1.2);
        for _ in 0..5 {
            set.begin(false, 1280, 720, 1.2, 1.0);
            set.begin(true, 1280, 720, 1.2, 1.0);
        }
        assert_eq!(set.menu().rebuilds, 0);
        assert_eq!(set.play.as_ref().map(|p| p.rebuilds), Some(0));

        // The same frames on one shared UI rebuild on every switch, which is
        // what the separate gameplay UI avoids.
        let mut shared = Ui::new(1.2);
        for _ in 0..5 {
            shared.begin(1280, 720, 1.2);
            shared.begin(1280, 720, 1.0);
        }
        // The first call matches the starting scale; each of the other nine switches.
        assert_eq!(shared.rebuilds, 9);
    }

    #[test]
    fn a_ui_scale_change_rebuilds_only_the_menu_atlas() {
        let mut set = UiSet::new(1.2);
        set.begin(true, 1280, 720, 1.2, 1.0);
        set.begin(false, 1280, 720, 1.5, 1.0);
        set.begin(true, 1280, 720, 1.5, 1.0);
        assert_eq!(set.menu().rebuilds, 1);
        assert_eq!(set.play.as_ref().map(|p| p.rebuilds), Some(0));
    }

    #[test]
    fn matching_scales_share_one_atlas() {
        let mut set = UiSet::new(1.0);
        set.begin(true, 1280, 720, 1.0, 1.0);
        set.begin(false, 1280, 720, 1.0, 1.0);
        assert!(set.play.is_none(), "no second atlas at equal scales");
        assert_eq!(set.menu().rebuilds, 0);
    }
    use crate::view::{UI_SCALE_MAX, UI_SCALE_MIN, UI_SCALE_STEP};

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
    fn a_rebuild_uploads_the_resident_thumbnails_again_without_a_decode() {
        let mut ui = Ui::new(1.0);
        ui.begin(1280, 720, 1.0);
        let key = JacketKey::Chart(beetle_core::ChartId::synthetic(5));
        let n = (THUMB_PX * THUMB_PX * 4) as usize;
        let px: Vec<u8> = (0..n).map(|i| (i % 251) as u8).collect();
        assert!(ui.jackets.insert(ui.canvas.atlas_mut(), key, &px));
        // A resize rebuilds the atlas: the thumbnail is still there, same pixels.
        ui.begin(1920, 1080, 1.5);
        let r = ui.jackets.get(key).expect("still resident");
        assert_eq!(ui.canvas.atlas().read_rgba(r), px);
    }

    #[test]
    fn jacket_grid_and_twelve_thumbs_stay_on_page_zero_at_every_ui_scale() {
        // Window scale 0.75 to 3.0 (steps of 0.25) times UI scale 80 % to
        // 150 % (steps of 10 %): the grid is reserved on page 0, the
        // placeholders and the text land there too, so no draw splits.
        for window_step in 0..=9 {
            for ui_percent in (UI_SCALE_MIN..=UI_SCALE_MAX).step_by(UI_SCALE_STEP as usize) {
                let window = 0.75 + 0.25 * window_step as f32;
                let scale = window * ui_percent as f32 / 100.0;
                check_thumbs_on_page_zero(scale);
            }
        }
    }

    fn check_thumbs_on_page_zero(scale: f32) {
        {
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

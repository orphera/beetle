//! Display state shared by the app and the screens: the 16:9 viewport, the
//! gameplay lane layout, and per-frame feedback (keys held, last judgement,
//! hit bursts) that the gameplay screen animates from.

use crate::skin::SkinConfig;
use beetle_core::{JudgeGrade, Lane};

/// A visual particle burst spawned when hitting a note on a lane.
#[derive(Debug, Clone, Copy)]
pub struct HitBurst {
    pub lane: Lane,
    pub spawn_time: f64,
    pub grade: JudgeGrade,
}

/// 16:9 Viewport mapping within the physical window surface.
///
/// Automatically computes pillarbox / letterbox bounds and reference scale (relative to 1280x720).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// X offset in window surface pixels (pillarboxing)
    pub x: f32,
    /// Y offset in window surface pixels (letterboxing)
    pub y: f32,
    /// Width of active 16:9 rendering viewport
    pub width: f32,
    /// Height of active 16:9 rendering viewport
    pub height: f32,
    /// Proportional scale factor relative to standard 720p (1280x720) reference height (height / 720.0)
    pub scale: f32,
}

impl Viewport {
    pub const BASE_WIDTH: f32 = 1280.0;
    pub const BASE_HEIGHT: f32 = 720.0;
    pub const TARGET_ASPECT: f32 = 16.0 / 9.0;

    pub fn new(window_width: u32, window_height: u32) -> Self {
        let w = window_width.max(1) as f32;
        let h = window_height.max(1) as f32;
        let aspect = w / h;

        let (vp_w, vp_h, vp_x, vp_y) = if (aspect - Self::TARGET_ASPECT).abs() < 0.005 {
            (w, h, 0.0, 0.0)
        } else if aspect > Self::TARGET_ASPECT {
            // Window is wider than 16:9 -> Pillarbox (bars on left and right)
            let vp_h = h;
            let vp_w = (h * Self::TARGET_ASPECT).round();
            let vp_x = ((w - vp_w) / 2.0).round();
            let vp_y = 0.0;
            (vp_w, vp_h, vp_x, vp_y)
        } else {
            // Window is taller than 16:9 -> Letterbox (bars on top and bottom)
            let vp_w = w;
            let vp_h = (w / Self::TARGET_ASPECT).round();
            let vp_x = 0.0;
            let vp_y = ((h - vp_h) / 2.0).round();
            (vp_w, vp_h, vp_x, vp_y)
        };

        let scale = vp_h / Self::BASE_HEIGHT;

        Self {
            x: vp_x,
            y: vp_y,
            width: vp_w,
            height: vp_h,
            scale,
        }
    }

    /// Whether the window pixel (`x`, `y`) lies inside the 16:9 rectangle
    /// (not in a bar). Window pixels are what the cursor and the hit regions use.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    /// Checks if this viewport has active letterbox (top/bottom bars)
    pub fn is_letterboxed(&self) -> bool {
        self.y > 0.5
    }

    /// Checks if this viewport has active pillarbox (left/right bars)
    pub fn is_pillarboxed(&self) -> bool {
        self.x > 0.5
    }

    /// The same 16:9 rectangle with the scale multiplied by the UI scale
    /// (`percent`, 100 = no change). Menus draw with this; gameplay keeps the
    /// window scale.
    pub fn with_ui_scale(self, percent: u32) -> Self {
        let factor = percent.clamp(UI_SCALE_MIN, UI_SCALE_MAX) as f32 / 100.0;
        Self {
            scale: self.scale * factor,
            ..self
        }
    }

    /// The same rectangle with the scale capped at what the whole 1280x720
    /// design fits in: the scale that makes the rectangle exactly 1280 wide
    /// and 720 tall at most. Screens whose layout is a fixed stack of design
    /// units (Key Config, Song Select) draw with this, so a UI scale that the
    /// window cannot hold is capped instead of spilling off the window.
    pub fn fit_to_design(self) -> Self {
        let fit = (self.width / Self::BASE_WIDTH).min(self.height / Self::BASE_HEIGHT);
        Self {
            scale: self.scale.min(fit),
            ..self
        }
    }
}

/// The UI scale option, in percent: 80 to 150 in steps of 10 (100 = default).
pub const UI_SCALE_MIN: u32 = 80;
pub const UI_SCALE_MAX: u32 = 150;
pub const UI_SCALE_STEP: u32 = 10;
pub const UI_SCALE_DEFAULT: u32 = 100;

/// Snaps a UI scale percent onto the option's steps and range.
pub fn snap_ui_scale(percent: u32) -> u32 {
    let steps = (percent + UI_SCALE_STEP / 2) / UI_SCALE_STEP * UI_SCALE_STEP;
    steps.clamp(UI_SCALE_MIN, UI_SCALE_MAX)
}

/// Total number of `Lane` variants (see `beetle_core::Lane`); sizes every
/// fixed per-lane array (key-press state, etc.).
pub const LANE_COUNT: usize = 18;

/// How long a hit burst stays alive (the screen's own animation is shorter).
const BURST_LIFETIME_SECONDS: f64 = 0.3;
/// How far back the judge timeline reaches (audio seconds).
pub const JUDGE_TIMELINE_SECONDS: f64 = 8.0;

/// One judgement on the judge timeline: when it happened on the audio clock,
/// its grade, and its timing (negative = early, positive = late).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JudgeMark {
    pub time: f64,
    pub grade: JudgeGrade,
    pub delta_ms: f64,
}

/// Viewport + lane layout + gameplay feedback, owned by the app.
pub struct ViewState {
    /// The window's 16:9 rectangle at the window scale: gameplay draws with it.
    pub viewport: Viewport,
    /// The same rectangle with the UI scale applied: every menu draws with it.
    pub ui: Viewport,
    /// The UI scale in percent (`UI_SCALE_MIN..=UI_SCALE_MAX`).
    pub ui_scale: u32,
    pub skin: SkinConfig,
    key_pressed: [bool; LANE_COUNT],
    /// (grade, time in seconds, delta in ms)
    last_judge: Option<(JudgeGrade, f64, f64)>,
    hit_bursts: Vec<HitBurst>,
    /// The judgements of the last `JUDGE_TIMELINE_SECONDS`, oldest first.
    judge_marks: Vec<JudgeMark>,
}

impl ViewState {
    pub fn new(width: u32, height: u32, mut skin: SkinConfig) -> Self {
        let viewport = Viewport::new(width, height);
        skin.update_layout(&viewport);
        Self {
            viewport,
            ui: viewport.with_ui_scale(UI_SCALE_DEFAULT),
            ui_scale: UI_SCALE_DEFAULT,
            skin,
            key_pressed: [false; LANE_COUNT],
            last_judge: None,
            hit_bursts: Vec::with_capacity(32),
            judge_marks: Vec::with_capacity(256),
        }
    }

    /// Recomputes the viewport and lane layout for a new window size.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.viewport = Viewport::new(width, height);
            self.ui = self.viewport.with_ui_scale(self.ui_scale);
            self.skin.update_layout(&self.viewport);
        }
    }

    /// Sets the UI scale (percent, snapped to the option's steps). Applies
    /// from the next frame: the UI rebuilds its atlas when the scale changes.
    pub fn set_ui_scale(&mut self, percent: u32) {
        self.ui_scale = snap_ui_scale(percent);
        self.ui = self.viewport.with_ui_scale(self.ui_scale);
    }

    /// The rectangle a frame draws with: the window scale in gameplay, the UI
    /// scale on every other screen.
    pub fn frame_viewport(&self, gameplay: bool) -> Viewport {
        if gameplay {
            self.viewport
        } else {
            self.ui
        }
    }

    pub fn set_key_state(&mut self, lane: Lane, pressed: bool) {
        self.key_pressed[lane_index(lane)] = pressed;
    }

    pub fn key_pressed(&self) -> &[bool; LANE_COUNT] {
        &self.key_pressed
    }

    pub fn hit_bursts(&self) -> &[HitBurst] {
        &self.hit_bursts
    }

    pub fn clean_expired_hit_bursts(&mut self, audio_time_seconds: f64) {
        self.hit_bursts.retain(|b| {
            let elapsed = audio_time_seconds - b.spawn_time;
            (0.0..BURST_LIFETIME_SECONDS).contains(&elapsed)
        });
        self.judge_marks.retain(|m| {
            let elapsed = audio_time_seconds - m.time;
            (0.0..JUDGE_TIMELINE_SECONDS).contains(&elapsed)
        });
    }

    /// The judgements shown on the judge timeline, oldest first.
    pub fn judge_marks(&self) -> &[JudgeMark] {
        &self.judge_marks
    }

    pub fn last_judge(&self) -> Option<(JudgeGrade, f64, f64)> {
        self.last_judge
    }

    /// Records a judgement that has no lane to burst on (e.g. a miss).
    pub fn trigger_judge(&mut self, grade: JudgeGrade, time_seconds: f64, delta_ms: f64) {
        self.last_judge = Some((grade, time_seconds, delta_ms));
        self.push_judge_mark(grade, time_seconds, delta_ms);
    }

    fn push_judge_mark(&mut self, grade: JudgeGrade, time: f64, delta_ms: f64) {
        self.judge_marks.push(JudgeMark {
            time,
            grade,
            delta_ms,
        });
    }

    /// Records a judgement; hits (not POOR / MISS) also spawn a burst on `lane`.
    pub fn trigger_judge_with_lane(
        &mut self,
        lane: Lane,
        grade: JudgeGrade,
        time_seconds: f64,
        delta_ms: f64,
    ) {
        self.last_judge = Some((grade, time_seconds, delta_ms));
        self.push_judge_mark(grade, time_seconds, delta_ms);
        if grade != JudgeGrade::Miss && grade != JudgeGrade::Poor {
            self.hit_bursts.push(HitBurst {
                lane,
                spawn_time: time_seconds,
                grade,
            });
        }
    }

    /// Clears feedback from a previous song.
    pub fn reset_feedback(&mut self) {
        self.key_pressed = [false; LANE_COUNT];
        self.last_judge = None;
        self.hit_bursts.clear();
        self.judge_marks.clear();
    }
}

pub fn lane_index(lane: Lane) -> usize {
    match lane {
        Lane::Scratch => 0,
        Lane::Key1 => 1,
        Lane::Key2 => 2,
        Lane::Key3 => 3,
        Lane::Key4 => 4,
        Lane::Key5 => 5,
        Lane::Key6 => 6,
        Lane::Key7 => 7,
        Lane::Key8 => 8,
        Lane::Key9 => 9,
        Lane::P2Scratch => 10,
        Lane::P2Key1 => 11,
        Lane::P2Key2 => 12,
        Lane::P2Key3 => 13,
        Lane::P2Key4 => 14,
        Lane::P2Key5 => 15,
        Lane::P2Key6 => 16,
        Lane::P2Key7 => 17,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_relayouts_and_bursts_expire() {
        let mut view = ViewState::new(1280, 720, SkinConfig::default());
        view.resize(1920, 1080);
        assert!((view.viewport.scale - 1.5).abs() < 1e-6);
        view.trigger_judge_with_lane(Lane::Key1, JudgeGrade::PerfectGreat, 1.0, 0.0);
        view.trigger_judge_with_lane(Lane::Key2, JudgeGrade::Miss, 1.0, 0.0);
        assert_eq!(view.hit_bursts().len(), 1);
        view.clean_expired_hit_bursts(1.1);
        assert_eq!(view.hit_bursts().len(), 1);
        view.clean_expired_hit_bursts(2.0);
        assert!(view.hit_bursts().is_empty());
    }

    #[test]
    fn ui_scale_applies_to_menus_and_leaves_the_window_scale_for_gameplay() {
        let mut view = ViewState::new(1280, 720, SkinConfig::default());
        view.set_ui_scale(150);
        assert_eq!(view.ui_scale, 150);
        assert!((view.ui.scale - 1.5).abs() < 1e-6);
        assert!(
            (view.viewport.scale - 1.0).abs() < 1e-6,
            "gameplay keeps 1.0"
        );
        assert_eq!(view.frame_viewport(true), view.viewport);
        assert_eq!(view.frame_viewport(false), view.ui);
        // A resize keeps the chosen UI scale on the new window.
        view.resize(1920, 1080);
        assert!((view.ui.scale - 2.25).abs() < 1e-6);
        // Off-range and off-step values are snapped onto the option.
        view.set_ui_scale(200);
        assert_eq!(view.ui_scale, UI_SCALE_MAX);
        view.set_ui_scale(84);
        assert_eq!(view.ui_scale, 80);
        view.set_ui_scale(86);
        assert_eq!(view.ui_scale, 90);
    }

    #[test]
    fn ui_scale_snaps_to_ten_percent_steps_inside_the_range() {
        assert_eq!(snap_ui_scale(0), UI_SCALE_MIN);
        assert_eq!(snap_ui_scale(100), 100);
        assert_eq!(snap_ui_scale(114), 110);
        assert_eq!(snap_ui_scale(116), 120);
        assert_eq!(snap_ui_scale(999), UI_SCALE_MAX);
    }

    #[test]
    fn ultrawide_and_4_3_windows_centre_the_16_9_rectangle() {
        // 21:9 (2560x1080) and 32:9-ish 3440x1440 pillarbox; 4:3 letterboxes.
        let cases = [
            (2560, 1080, (1920.0, 1080.0), (320.0, 0.0), 1.5),
            (3440, 1440, (2560.0, 1440.0), (440.0, 0.0), 2.0),
            (1024, 768, (1024.0, 576.0), (0.0, 96.0), 0.8),
        ];
        for (w, h, (vw, vh), (x, y), scale) in cases {
            let vp = Viewport::new(w, h);
            assert_eq!((vp.width, vp.height), (vw, vh), "{w}x{h} size");
            assert_eq!((vp.x, vp.y), (x, y), "{w}x{h} offset");
            assert!(
                (vp.scale - scale).abs() < 1e-6,
                "{w}x{h} scale {}",
                vp.scale
            );
            // The rectangle is centred in the window.
            assert_eq!(vp.x * 2.0 + vp.width, w as f32, "{w}x{h} centred across");
            assert_eq!(vp.y * 2.0 + vp.height, h as f32, "{w}x{h} centred down");
            // The window's corners are outside the rectangle (bars), its centre inside.
            assert!(!vp.contains(0.0, 0.0), "{w}x{h} corner is in a bar");
            assert!(
                vp.contains(w as f32 / 2.0, h as f32 / 2.0),
                "{w}x{h} centre"
            );
            // Mouse mapping: the rectangle's own top-left pixel is inside.
            assert!(vp.contains(vp.x, vp.y), "{w}x{h} top-left");
        }
    }

    #[test]
    fn viewport_letterboxes_to_16_9() {
        let vp = Viewport::new(1280, 1024);
        assert!(vp.is_letterboxed() && !vp.is_pillarboxed());
        assert_eq!((vp.width, vp.height), (1280.0, 720.0));
    }
}

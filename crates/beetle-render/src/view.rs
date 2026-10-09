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

    /// Checks if this viewport has active letterbox (top/bottom bars)
    pub fn is_letterboxed(&self) -> bool {
        self.y > 0.5
    }

    /// Checks if this viewport has active pillarbox (left/right bars)
    pub fn is_pillarboxed(&self) -> bool {
        self.x > 0.5
    }
}

/// Total number of `Lane` variants (see `beetle_core::Lane`); sizes every
/// fixed per-lane array (key-press state, etc.).
pub const LANE_COUNT: usize = 18;

/// How long a hit burst stays alive (the screen's own animation is shorter).
const BURST_LIFETIME_SECONDS: f64 = 0.3;

/// Viewport + lane layout + gameplay feedback, owned by the app.
pub struct ViewState {
    pub viewport: Viewport,
    pub skin: SkinConfig,
    key_pressed: [bool; LANE_COUNT],
    /// (grade, time in seconds, delta in ms)
    last_judge: Option<(JudgeGrade, f64, f64)>,
    hit_bursts: Vec<HitBurst>,
}

impl ViewState {
    pub fn new(width: u32, height: u32, mut skin: SkinConfig) -> Self {
        let viewport = Viewport::new(width, height);
        skin.update_layout(&viewport);
        Self {
            viewport,
            skin,
            key_pressed: [false; LANE_COUNT],
            last_judge: None,
            hit_bursts: Vec::with_capacity(32),
        }
    }

    /// Recomputes the viewport and lane layout for a new window size.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.viewport = Viewport::new(width, height);
            self.skin.update_layout(&self.viewport);
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
    }

    pub fn last_judge(&self) -> Option<(JudgeGrade, f64, f64)> {
        self.last_judge
    }

    /// Records a judgement that has no lane to burst on (e.g. a miss).
    pub fn trigger_judge(&mut self, grade: JudgeGrade, time_seconds: f64, delta_ms: f64) {
        self.last_judge = Some((grade, time_seconds, delta_ms));
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
    fn viewport_letterboxes_to_16_9() {
        let vp = Viewport::new(1280, 1024);
        assert!(vp.is_letterboxed() && !vp.is_pillarboxed());
        assert_eq!((vp.width, vp.height), (1280.0, 720.0));
    }
}

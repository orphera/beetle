//! Presentation-only polish for the menus (plan U1e): a fade-in after a
//! screen change and one toast notification at a time. Nothing here reads or
//! advances the audio clock or the judge (INV-1), and Gameplay never gets an
//! overlay.

use std::time::{Duration, Instant};

use beetle_render::motion::{ease_out_cubic, ease_out_quad};
pub use beetle_render::ToastKind;

use crate::state::{AppScreen, AppState};

/// Length of the fade-in from the background after a screen change.
pub const FADE_SECONDS: f32 = 0.2;
/// Toast: slide and fade in, stay, then fade out.
const TOAST_IN: f32 = 0.15;
const TOAST_STAY: f32 = 2.5;
const TOAST_OUT: f32 = 0.3;
/// Frame pacing while something animates (about 60 Hz).
pub const FRAME: Duration = Duration::from_millis(16);

/// Screens the fade-in plays on. Boot is the first screen and has no fade;
/// Gameplay never fades (Loading -> Gameplay is a cut).
pub fn fades_in(screen: AppScreen) -> bool {
    matches!(
        screen,
        AppScreen::SongSelect
            | AppScreen::KeyConfig
            | AppScreen::Settings
            | AppScreen::Loading
            | AppScreen::Result
    )
}

/// Screens that draw toasts. Gameplay, Loading and Boot never do.
pub fn shows_toasts(screen: AppScreen) -> bool {
    matches!(
        screen,
        AppScreen::SongSelect | AppScreen::KeyConfig | AppScreen::Settings | AppScreen::Result
    )
}

/// Overlay alpha `elapsed` seconds after a fade-in started: 1 (the background
/// covers the screen) easing to 0, `None` once it is over.
pub fn fade_alpha_at(elapsed: f32) -> Option<f32> {
    if elapsed >= FADE_SECONDS {
        return None;
    }
    let t = (elapsed / FADE_SECONDS).max(0.0);
    Some(1.0 - ease_out_cubic(t))
}

/// Where a toast is in its life.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToastPose {
    pub alpha: f32,
    /// 0 raised a little, 1 in place.
    pub slide: f32,
}

/// The toast's pose `elapsed` seconds after it was shown; `None` once gone.
pub fn toast_pose(elapsed: f32) -> Option<ToastPose> {
    let elapsed = elapsed.max(0.0);
    let stay_end = TOAST_IN + TOAST_STAY;
    if elapsed < TOAST_IN {
        let t = elapsed / TOAST_IN;
        Some(ToastPose {
            alpha: ease_out_quad(t),
            slide: ease_out_cubic(t),
        })
    } else if elapsed < stay_end {
        Some(ToastPose {
            alpha: 1.0,
            slide: 1.0,
        })
    } else if elapsed < stay_end + TOAST_OUT {
        // Quadratic falloff so the last frame drawn before the end is already invisible.
        let t = (elapsed - stay_end) / TOAST_OUT;
        Some(ToastPose {
            alpha: (1.0 - t) * (1.0 - t),
            slide: 1.0,
        })
    } else {
        None
    }
}

/// The screen the app is on and when it was entered. The only place entry
/// times are recorded: `sync` notices a changed screen.
#[derive(Debug, Clone, Copy)]
pub struct ScreenEntry {
    screen: AppScreen,
    entered_at: Instant,
}

impl ScreenEntry {
    pub fn new(screen: AppScreen, now: Instant) -> Self {
        Self {
            screen,
            entered_at: now,
        }
    }

    /// Records `screen` as the current one. Returns `true` when it changed.
    pub fn sync(&mut self, screen: AppScreen, now: Instant) -> bool {
        if screen == self.screen {
            return false;
        }
        self.screen = screen;
        self.entered_at = now;
        true
    }

    /// The fade-in overlay alpha now, if the current screen fades in.
    pub fn fade_alpha(&self, now: Instant) -> Option<f32> {
        if !fades_in(self.screen) {
            return None;
        }
        fade_alpha_at(now.saturating_duration_since(self.entered_at).as_secs_f32())
    }
}

/// The one toast on screen, if any.
#[derive(Debug, Clone)]
pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub shown_at: Instant,
}

/// Puts `text` in the toast slot; a toast already there is replaced.
pub fn set_toast(slot: &mut Option<Toast>, kind: ToastKind, text: String, now: Instant) {
    *slot = Some(Toast {
        text,
        kind,
        shown_at: now,
    });
}

/// Shows a toast over the menus (replacing one that is showing) and asks for a frame.
pub fn show_toast(state: &mut AppState, kind: ToastKind, text: impl Into<String>) {
    set_toast(&mut state.toast, kind, text.into(), Instant::now());
    state.window.request_redraw();
}

impl AppState {
    /// Records a change of screen. Called before drawing and every loop turn.
    pub fn sync_screen_entry(&mut self) {
        self.screen_entry.sync(self.screen, Instant::now());
    }

    /// The toast's pose now, if a toast is showing on the current screen.
    pub fn toast_pose_now(&self, now: Instant) -> Option<ToastPose> {
        if !shows_toasts(self.screen) {
            return None;
        }
        let toast = self.toast.as_ref()?;
        toast_pose(now.saturating_duration_since(toast.shown_at).as_secs_f32())
    }

    /// Whether a fade or a toast is on screen now (frames keep coming).
    pub fn presentation_animating(&self, now: Instant) -> bool {
        self.screen_entry.fade_alpha(now).is_some() || self.toast_pose_now(now).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(from: f32, to: f32, step: f32, f: impl Fn(f32) -> Option<f32>) -> Vec<f32> {
        let mut out = Vec::new();
        let mut t = from;
        while t <= to + 1e-6 {
            out.push(f(t).unwrap_or(-1.0));
            t += step;
        }
        out
    }

    #[test]
    fn fade_starts_covered_eases_out_and_ends() {
        assert_eq!(fade_alpha_at(0.0), Some(1.0));
        assert_eq!(fade_alpha_at(FADE_SECONDS), None);
        assert_eq!(fade_alpha_at(FADE_SECONDS + 1.0), None);
        // Ease-out: well past half the alpha is gone at the midpoint.
        let mid = fade_alpha_at(FADE_SECONDS / 2.0).unwrap();
        assert!(mid < 0.5 && mid > 0.0, "mid = {mid}");
        // Monotone falling until the end.
        let alphas = sample(0.0, FADE_SECONDS, 0.005, fade_alpha_at);
        let visible: Vec<f32> = alphas.iter().copied().filter(|a| *a >= 0.0).collect();
        assert!(visible.windows(2).all(|w| w[1] <= w[0]), "{visible:?}");
        // The last drawn frame is essentially transparent.
        assert!(*visible.last().unwrap() < 0.01);
    }

    #[test]
    fn fades_play_on_menus_but_never_on_gameplay() {
        assert!(!fades_in(AppScreen::Gameplay));
        assert!(!fades_in(AppScreen::Boot));
        for screen in [
            AppScreen::SongSelect,
            AppScreen::KeyConfig,
            AppScreen::Settings,
            AppScreen::Loading,
            AppScreen::Result,
        ] {
            assert!(fades_in(screen), "{screen:?} should fade in");
        }

        // Entering Gameplay: no overlay at any point. Entering Result: covered at once.
        let t0 = Instant::now();
        let mut entry = ScreenEntry::new(AppScreen::Loading, t0);
        assert!(entry.sync(AppScreen::Gameplay, t0));
        assert_eq!(entry.fade_alpha(t0), None);
        assert_eq!(entry.fade_alpha(t0 + Duration::from_millis(5)), None);
        assert!(entry.sync(AppScreen::Result, t0));
        assert_eq!(entry.fade_alpha(t0), Some(1.0));
    }

    #[test]
    fn screen_entry_resets_only_when_the_screen_changes() {
        let t0 = Instant::now();
        let later = t0 + Duration::from_millis(300);
        let mut entry = ScreenEntry::new(AppScreen::Boot, t0);
        assert!(
            !entry.sync(AppScreen::Boot, later),
            "same screen: no change"
        );
        assert_eq!(entry.fade_alpha(later), None, "Boot does not fade");
        assert!(entry.sync(AppScreen::SongSelect, later));
        assert_eq!(entry.fade_alpha(later), Some(1.0));
        let mid = later + Duration::from_millis(100);
        assert!(!entry.sync(AppScreen::SongSelect, mid));
        assert!(entry.fade_alpha(mid).unwrap() < 1.0, "entry time kept");
    }

    #[test]
    fn toast_rises_holds_then_fades_out() {
        assert_eq!(toast_pose(0.0).unwrap().alpha, 0.0);
        assert_eq!(toast_pose(0.0).unwrap().slide, 0.0);
        let full = toast_pose(TOAST_IN).unwrap();
        assert_eq!((full.alpha, full.slide), (1.0, 1.0));

        // Rising phase: alpha and slide grow.
        let rise = sample(0.0, TOAST_IN, 0.01, |t| toast_pose(t).map(|p| p.alpha));
        assert!(rise.windows(2).all(|w| w[1] >= w[0]), "{rise:?}");
        let slide = sample(0.0, TOAST_IN, 0.01, |t| toast_pose(t).map(|p| p.slide));
        assert!(slide.windows(2).all(|w| w[1] >= w[0]), "{slide:?}");

        // Hold: fully visible and in place for about 2.5 s.
        for t in [TOAST_IN, 1.0, TOAST_IN + TOAST_STAY - 0.01] {
            let p = toast_pose(t).unwrap();
            assert_eq!((p.alpha, p.slide), (1.0, 1.0), "at {t}");
        }

        // Fade-out: falling until gone.
        let end = TOAST_IN + TOAST_STAY + TOAST_OUT;
        let fall = sample(TOAST_IN + TOAST_STAY, end, 0.01, |t| {
            toast_pose(t).map(|p| p.alpha)
        });
        let visible: Vec<f32> = fall.iter().copied().filter(|a| *a >= 0.0).collect();
        assert!(visible.windows(2).all(|w| w[1] <= w[0]), "{visible:?}");
        assert!(*visible.last().unwrap() < 0.01, "last frame invisible");
        assert_eq!(toast_pose(end), None);
        assert_eq!(toast_pose(end + 1.0), None);
    }

    #[test]
    fn a_new_toast_replaces_the_one_showing() {
        let t0 = Instant::now();
        let mut slot = None;
        set_toast(&mut slot, ToastKind::Info, "first".into(), t0);
        let t1 = t0 + Duration::from_millis(500);
        set_toast(&mut slot, ToastKind::Error, "second".into(), t1);
        let toast = slot.as_ref().unwrap();
        assert_eq!(toast.text, "second");
        assert_eq!(toast.kind, ToastKind::Error);
        assert_eq!(toast.shown_at, t1, "its own timer restarts");
    }

    #[test]
    fn toasts_show_only_on_menus() {
        assert!(shows_toasts(AppScreen::SongSelect));
        assert!(shows_toasts(AppScreen::KeyConfig));
        assert!(shows_toasts(AppScreen::Settings));
        assert!(shows_toasts(AppScreen::Result));
        assert!(!shows_toasts(AppScreen::Gameplay));
        assert!(!shows_toasts(AppScreen::Loading));
        assert!(!shows_toasts(AppScreen::Boot));
    }
}

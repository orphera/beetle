//! Presentation-only polish for the menus and the Loading -> Gameplay hand-off:
//! screen transitions (a shutter wipe, or a plain fade under the reduced-motion
//! style), the reveal over the first Gameplay frames, and one toast at a time.
//! Nothing here reads or advances the audio clock or the judge (INV-1): these
//! timings only choose what is drawn over a frame that is already rendered.
//! The Gameplay reveal never delays gameplay time, READY, judgement or input.

use std::time::{Duration, Instant};

pub use beetle_render::ToastKind;
use beetle_render::{
    motion::{ease_out_cubic, ease_out_quad},
    ShutterPhase,
};

use crate::state::{AppScreen, AppState};

/// Length of the plain fade-in from the background after a screen change.
pub const FADE_SECONDS: f32 = 0.2;
/// Shutter: the panels cover the frame, then uncover it.
pub const SHUTTER_COVER_SECONDS: f32 = 0.18;
pub const SHUTTER_REVEAL_SECONDS: f32 = 0.22;
pub const SHUTTER_SECONDS: f32 = SHUTTER_COVER_SECONDS + SHUTTER_REVEAL_SECONDS;
/// Loading -> Gameplay: the panels uncover the first frames of Gameplay.
pub const GAMEPLAY_REVEAL_SECONDS: f32 = 0.25;
/// Toast: slide and fade in, stay, then fade out.
const TOAST_IN: f32 = 0.15;
const TOAST_STAY: f32 = 2.5;
const TOAST_OUT: f32 = 0.3;
/// Frame pacing while something animates (about 60 Hz).
pub const FRAME: Duration = Duration::from_millis(16);

/// How screen changes are presented. `Shutter` is the default; `Fade` is the
/// plain background fade a future "reduce flashes" option would select.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransitionStyle {
    #[default]
    Shutter,
    // Chosen by the reduce-flashes option, which is not in the config yet.
    #[allow(dead_code)]
    Fade,
}

/// The overlay a screen change plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    /// Plain fade-in from the background.
    Fade,
    /// Shutter wipe: cover, then reveal.
    Shutter,
    /// Reveal only, over the first frames of Gameplay.
    GameplayReveal,
}

/// What is drawn over the current frame, if anything.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Overlay {
    /// Background alpha: 1 covers the frame, 0 is clear.
    Fade(f32),
    Shutter(ShutterPhase),
    GameplayReveal(ShutterPhase),
}

/// Screens the fade-in plays on. Boot is the first screen and has no fade;
/// Gameplay never fades in on its own (its reveal comes from Loading).
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

/// The overlay for a change from `from` to `to`, or `None` for a plain cut.
/// The shutter joins SongSelect with Settings, Key Config, Result and Loading
/// (both ways for the first three). Loading -> Gameplay reveals the first
/// frames, and under `Fade` that is a plain fade instead. Every other change
/// keeps the menu fade-in on its destination.
pub fn transition_for(
    from: AppScreen,
    to: AppScreen,
    style: TransitionStyle,
) -> Option<Transition> {
    if from == AppScreen::Loading && to == AppScreen::Gameplay {
        return Some(match style {
            TransitionStyle::Shutter => Transition::GameplayReveal,
            TransitionStyle::Fade => Transition::Fade,
        });
    }
    let shutter_pair = matches!(
        (from, to),
        (
            AppScreen::SongSelect,
            AppScreen::Settings | AppScreen::KeyConfig | AppScreen::Result | AppScreen::Loading,
        ) | (
            AppScreen::Settings | AppScreen::KeyConfig | AppScreen::Result,
            AppScreen::SongSelect,
        )
    );
    if shutter_pair && style == TransitionStyle::Shutter {
        return Some(Transition::Shutter);
    }
    fades_in(to).then_some(Transition::Fade)
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

/// The shutter's phase `elapsed` seconds after it started: covering for the
/// first `SHUTTER_COVER_SECONDS`, then revealing; `None` once it is over.
/// Cover ends at 45 % of the whole, where the frame is fully covered.
pub fn shutter_phase_at(elapsed: f32) -> Option<ShutterPhase> {
    let elapsed = elapsed.max(0.0);
    if elapsed < SHUTTER_COVER_SECONDS {
        Some(ShutterPhase::Cover(elapsed / SHUTTER_COVER_SECONDS))
    } else if elapsed < SHUTTER_SECONDS {
        Some(ShutterPhase::Reveal(
            (elapsed - SHUTTER_COVER_SECONDS) / SHUTTER_REVEAL_SECONDS,
        ))
    } else {
        None
    }
}

/// The Gameplay reveal's phase `elapsed` seconds after Gameplay was entered:
/// starts fully covered and uncovers; `None` once it is over.
pub fn gameplay_reveal_phase_at(elapsed: f32) -> Option<ShutterPhase> {
    let elapsed = elapsed.max(0.0);
    (elapsed < GAMEPLAY_REVEAL_SECONDS)
        .then(|| ShutterPhase::Reveal(elapsed / GAMEPLAY_REVEAL_SECONDS))
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

/// The screen the app is on, when it was entered, and the transition that
/// entry plays. The only place entry times are recorded: `sync` notices a
/// changed screen.
#[derive(Debug, Clone, Copy)]
pub struct ScreenEntry {
    screen: AppScreen,
    entered_at: Instant,
    transition: Option<Transition>,
    style: TransitionStyle,
}

impl ScreenEntry {
    pub fn new(screen: AppScreen, now: Instant) -> Self {
        Self {
            screen,
            entered_at: now,
            transition: None,
            style: TransitionStyle::default(),
        }
    }

    /// Records `screen` as the current one. Returns `true` when it changed.
    pub fn sync(&mut self, screen: AppScreen, now: Instant) -> bool {
        if screen == self.screen {
            return false;
        }
        self.transition = transition_for(self.screen, screen, self.style);
        self.screen = screen;
        self.entered_at = now;
        true
    }

    /// The overlay to draw on the current frame, if its entry transition is still running.
    pub fn overlay(&self, now: Instant) -> Option<Overlay> {
        let elapsed = now.saturating_duration_since(self.entered_at).as_secs_f32();
        match self.transition? {
            Transition::Fade => fade_alpha_at(elapsed).map(Overlay::Fade),
            Transition::Shutter => shutter_phase_at(elapsed).map(Overlay::Shutter),
            Transition::GameplayReveal => {
                gameplay_reveal_phase_at(elapsed).map(Overlay::GameplayReveal)
            }
        }
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
        if self.screen_entry.sync(self.screen, Instant::now()) {
            crate::menu_audio::sync_with_screen(self);
        }
    }

    /// The toast's pose now, if a toast is showing on the current screen.
    pub fn toast_pose_now(&self, now: Instant) -> Option<ToastPose> {
        if !shows_toasts(self.screen) {
            return None;
        }
        let toast = self.toast.as_ref()?;
        toast_pose(now.saturating_duration_since(toast.shown_at).as_secs_f32())
    }

    /// Whether a transition or a toast is on screen now (frames keep coming).
    pub fn presentation_animating(&self, now: Instant) -> bool {
        self.screen_entry.overlay(now).is_some() || self.toast_pose_now(now).is_some()
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
    fn shutter_lasts_about_0_4_seconds_and_covers_at_its_midpoint() {
        assert!((SHUTTER_SECONDS - 0.4).abs() < 1e-6);
        assert_eq!(shutter_phase_at(0.0), Some(ShutterPhase::Cover(0.0)));
        // Cover completes at 45 % of the whole: the frame is covered there.
        let cover_end = shutter_phase_at(SHUTTER_COVER_SECONDS - 1e-4);
        assert!(matches!(cover_end, Some(ShutterPhase::Cover(t)) if t > 0.999));
        assert_eq!(
            shutter_phase_at(SHUTTER_COVER_SECONDS),
            Some(ShutterPhase::Reveal(0.0))
        );
        let ratio = SHUTTER_COVER_SECONDS / SHUTTER_SECONDS;
        assert!((ratio - 0.45).abs() < 1e-6, "ratio = {ratio}");
        assert_eq!(shutter_phase_at(SHUTTER_SECONDS), None);
        assert_eq!(shutter_phase_at(SHUTTER_SECONDS + 1.0), None);
        // Reveal climbs monotonically from covered to clear.
        let reveal: Vec<f32> = (0..=20)
            .map(|i| {
                let t = SHUTTER_COVER_SECONDS + SHUTTER_REVEAL_SECONDS * i as f32 / 20.0;
                match shutter_phase_at(t.min(SHUTTER_SECONDS - 1e-4)) {
                    Some(ShutterPhase::Reveal(v)) => v,
                    other => panic!("not revealing: {other:?}"),
                }
            })
            .collect();
        assert!(reveal.windows(2).all(|w| w[1] >= w[0]), "{reveal:?}");
    }

    #[test]
    fn gameplay_reveal_starts_covered_and_lasts_250_ms() {
        assert_eq!(
            gameplay_reveal_phase_at(0.0),
            Some(ShutterPhase::Reveal(0.0))
        );
        assert!(gameplay_reveal_phase_at(GAMEPLAY_REVEAL_SECONDS / 2.0).is_some());
        assert_eq!(gameplay_reveal_phase_at(GAMEPLAY_REVEAL_SECONDS), None);
        assert!((GAMEPLAY_REVEAL_SECONDS - 0.25).abs() < 1e-6);
    }

    #[test]
    fn style_selection_per_screen_pair() {
        use AppScreen::*;
        let shutter = TransitionStyle::Shutter;
        for (from, to) in [
            (SongSelect, Settings),
            (SongSelect, KeyConfig),
            (SongSelect, Result),
            (SongSelect, Loading),
            (Settings, SongSelect),
            (KeyConfig, SongSelect),
            (Result, SongSelect),
        ] {
            assert_eq!(
                transition_for(from, to, shutter),
                Some(Transition::Shutter),
                "{from:?} -> {to:?}"
            );
        }
        assert_eq!(
            transition_for(Loading, Gameplay, shutter),
            Some(Transition::GameplayReveal)
        );
        // Boot -> SongSelect stays a plain fade; so do the other changes.
        assert_eq!(
            transition_for(Boot, SongSelect, shutter),
            Some(Transition::Fade)
        );
        assert_eq!(
            transition_for(Gameplay, Result, shutter),
            Some(Transition::Fade)
        );
        assert_eq!(
            transition_for(Result, Loading, shutter),
            Some(Transition::Fade)
        );
        assert_eq!(
            transition_for(Gameplay, Settings, shutter),
            Some(Transition::Fade)
        );
        // Reduced-motion style: plain fades everywhere a shutter or reveal would play.
        let fade = TransitionStyle::Fade;
        assert_eq!(
            transition_for(SongSelect, Loading, fade),
            Some(Transition::Fade)
        );
        assert_eq!(
            transition_for(Loading, Gameplay, fade),
            Some(Transition::Fade)
        );
        // A cut where nothing fades in (into Gameplay from a menu).
        assert_eq!(transition_for(SongSelect, Gameplay, shutter), None);
    }

    #[test]
    fn overlays_follow_the_entry_transition() {
        let t0 = Instant::now();
        let mut entry = ScreenEntry::new(AppScreen::SongSelect, t0);
        assert!(entry.sync(AppScreen::Settings, t0));
        assert_eq!(
            entry.overlay(t0),
            Some(Overlay::Shutter(ShutterPhase::Cover(0.0)))
        );
        let end = t0 + Duration::from_secs_f32(SHUTTER_SECONDS + 0.01);
        assert_eq!(entry.overlay(end), None);

        assert!(entry.sync(AppScreen::Loading, end));
        assert_eq!(entry.overlay(end), Some(Overlay::Fade(1.0)));

        let t1 = end + Duration::from_secs(1);
        assert!(entry.sync(AppScreen::Gameplay, t1));
        assert_eq!(
            entry.overlay(t1),
            Some(Overlay::GameplayReveal(ShutterPhase::Reveal(0.0)))
        );
        // Gameplay reveal ends; no other overlay ever plays on Gameplay.
        assert_eq!(entry.overlay(t1 + Duration::from_millis(300)), None);
    }

    #[test]
    fn boot_fades_and_gameplay_never_fades_in_on_its_own() {
        let t0 = Instant::now();
        let mut entry = ScreenEntry::new(AppScreen::Boot, t0);
        assert_eq!(entry.overlay(t0), None, "Boot does not fade");
        assert!(entry.sync(AppScreen::SongSelect, t0));
        assert_eq!(entry.overlay(t0), Some(Overlay::Fade(1.0)));
        assert!(entry.sync(AppScreen::Gameplay, t0));
        assert_eq!(entry.overlay(t0), None);
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
        assert_eq!(entry.overlay(later), None, "Boot does not fade");
        assert!(entry.sync(AppScreen::SongSelect, later));
        assert_eq!(entry.overlay(later), Some(Overlay::Fade(1.0)));
        let mid = later + Duration::from_millis(100);
        assert!(!entry.sync(AppScreen::SongSelect, mid));
        assert!(
            matches!(entry.overlay(mid), Some(Overlay::Fade(a)) if a < 1.0),
            "entry time kept"
        );
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

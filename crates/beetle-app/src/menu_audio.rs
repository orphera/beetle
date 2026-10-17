//! Menu sound effects: the procedural UI sounds of `beetle_audio::sfx`, played
//! on their own short-lived audio stream while a menu screen is up.
//!
//! The stream is closed on Gameplay, so the gameplay engine has the output
//! device to itself, and reopened when a menu screen comes back. Sounds
//! go through the engine's lock-free trigger queue only (INV-2 applies to the
//! audio thread; this module never blocks on it).

use std::time::{Duration, Instant};

use beetle_audio::{ui_sound_bank, AudioEngine, SampleTrigger, UiSound};

use crate::state::{AppScreen, AppState};

/// Rate the sounds are synthesized at; the mixer resamples them to the device.
const SYNTH_RATE: u32 = 48_000;
/// Menu sounds are secondary to the song preview: they play at this share of the master volume.
const SFX_GAIN: f32 = 0.6;
/// Fastest repeat of `Move` (held arrow keys) and of `Tick`.
const MOVE_GAP: Duration = Duration::from_millis(35);
const TICK_GAP: Duration = Duration::from_millis(20);

/// The menu's sound effects. Closed (silent) until `open` succeeds.
#[derive(Default)]
pub struct MenuAudio {
    engine: Option<AudioEngine>,
    trigger: Option<SampleTrigger>,
    last_move: Option<Instant>,
    last_tick: Option<Instant>,
}

impl MenuAudio {
    /// Opens the output stream at `master` volume (the Settings value). If a
    /// stream is already open only the volume changes. When no device can be
    /// opened the menus stay silent; nothing is reported.
    pub fn open(&mut self, master: f32) {
        if self.engine.is_some() {
            self.set_volume(master);
            return;
        }
        let Ok(mut engine) = AudioEngine::new(ui_sound_bank(SYNTH_RATE)) else {
            return;
        };
        let _ = engine.set_master_volume(master * SFX_GAIN);
        self.trigger = engine.take_trigger();
        self.engine = Some(engine);
    }

    /// Follows a change of the master volume (`master`, not yet scaled).
    pub fn set_volume(&mut self, master: f32) {
        if let Some(engine) = self.engine.as_mut() {
            let _ = engine.set_master_volume(master * SFX_GAIN);
        }
    }

    /// Drops the stream, freeing the output device.
    pub fn close(&mut self) {
        self.trigger = None;
        self.engine = None;
        self.last_move = None;
        self.last_tick = None;
    }

    /// Plays `sound` now, unless the stream is closed or the sound is rate-limited.
    pub fn play(&mut self, sound: UiSound) {
        self.play_at(sound, Instant::now());
    }

    fn play_at(&mut self, sound: UiSound, now: Instant) {
        let Some(trigger) = self.trigger.as_mut() else {
            return;
        };
        match sound {
            UiSound::Move => {
                if !allow(self.last_move, now, MOVE_GAP) {
                    return;
                }
                self.last_move = Some(now);
            }
            UiSound::Tick => {
                if !allow(self.last_tick, now, TICK_GAP) {
                    return;
                }
                self.last_tick = Some(now);
            }
            _ => {}
        }
        trigger.play(sound.wav_id());
    }
}

/// Whether a sound last played at `last` may play again at `now`, given at least `min` between two.
pub fn allow(last: Option<Instant>, now: Instant, min: Duration) -> bool {
    match last {
        None => true,
        Some(last) => now.saturating_duration_since(last) >= min,
    }
}

/// The one way a call site plays a menu sound.
pub fn play_ui(state: &mut AppState, sound: UiSound) {
    state.menu_audio.play(sound);
}

/// What the menu stream does when the screen changes: `Some(true)` opens it (or
/// only updates its volume when already open), `Some(false)` closes it, and
/// `None` leaves it as it is. Loading keeps the stream: a Confirm sound is
/// queued in the same turn that enters Loading, and the gameplay engine closes
/// the stream itself right before it opens the device.
fn stream_policy(screen: AppScreen) -> Option<bool> {
    match screen {
        AppScreen::Gameplay => Some(false),
        AppScreen::Loading => None,
        _ => Some(true),
    }
}

/// Opens the menu stream on a menu screen and closes it on Gameplay.
/// Called when the screen changes, and once at start-up.
pub fn sync_with_screen(state: &mut AppState) {
    match stream_policy(state.screen) {
        Some(true) => state.menu_audio.open(state.master_volume),
        Some(false) => state.menu_audio.close(),
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_sound_is_always_allowed() {
        assert!(allow(None, Instant::now(), MOVE_GAP));
    }

    #[test]
    fn repeats_inside_the_gap_are_dropped() {
        let t0 = Instant::now();
        let just_after = t0 + Duration::from_millis(34);
        assert!(!allow(Some(t0), just_after, MOVE_GAP));
        assert!(!allow(Some(t0), t0, TICK_GAP));
    }

    #[test]
    fn a_sound_is_allowed_again_once_the_gap_has_passed() {
        let t0 = Instant::now();
        assert!(allow(Some(t0), t0 + MOVE_GAP, MOVE_GAP));
        assert!(allow(Some(t0), t0 + TICK_GAP, TICK_GAP));
        assert!(!allow(Some(t0), t0 + Duration::from_millis(19), TICK_GAP));
    }

    #[test]
    fn a_clock_going_backwards_does_not_panic_or_allow_a_burst() {
        let t0 = Instant::now();
        let earlier = t0.checked_sub(Duration::from_millis(5)).unwrap_or(t0);
        assert!(!allow(Some(t0), earlier, MOVE_GAP));
    }

    #[test]
    fn play_on_a_closed_menu_audio_does_nothing() {
        let mut menu = MenuAudio::default();
        let t0 = Instant::now();
        menu.play_at(UiSound::Move, t0);
        menu.play_at(UiSound::Tick, t0);
        menu.play(UiSound::Confirm);
        assert!(menu.last_move.is_none(), "a closed stream records nothing");
        assert!(menu.last_tick.is_none());
    }

    #[test]
    fn the_stream_policy_follows_the_screen() {
        assert_eq!(stream_policy(AppScreen::SongSelect), Some(true));
        assert_eq!(stream_policy(AppScreen::Result), Some(true));
        assert_eq!(stream_policy(AppScreen::Settings), Some(true));
        assert_eq!(stream_policy(AppScreen::KeyConfig), Some(true));
        assert_eq!(
            stream_policy(AppScreen::Loading),
            None,
            "Loading keeps the stream"
        );
        assert_eq!(stream_policy(AppScreen::Gameplay), Some(false));
    }

    #[test]
    fn close_on_a_closed_menu_audio_is_harmless() {
        let mut menu = MenuAudio::default();
        menu.close();
        menu.set_volume(0.5);
        menu.play(UiSound::Move);
        assert!(menu.engine.is_none());
    }
}

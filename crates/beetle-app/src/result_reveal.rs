//! The clock and the sounds of the Stage Result reveal. The timeline itself is
//! `beetle_render::reveal`; this module decides which of its sounds are due
//! on a frame, so each one plays exactly once however the frames fall.
//!
//! The reveal starts when the screen is entered. A key press or a click
//! during it skips to the final state at once: the Stamp sounds once (if it
//! has not yet) and the Toast too on a new best, but no ticks.

use std::time::Instant;

use beetle_audio::UiSound;
use beetle_render::reveal::{self, REVEAL_SECONDS};

use crate::devtools;
use crate::menu_audio::play_ui;
use crate::state::AppState;

/// What the reveal has already sounded, and the time of the last frame.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct RevealSounds {
    last_t: f64,
    stamp_fired: bool,
    toast_fired: bool,
}

/// The sounds due on one frame. At most one tick: ticks are pitch-less, and
/// a frame that covers several ticks sounds one of them.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Due {
    pub tick: bool,
    pub stamp: bool,
    pub toast: bool,
}

/// The sounds due at reveal time `t` (the frame's), given the last frame's
/// time in `sounds`. `new_best` is whether the play set a best (the Toast);
/// `skipped` is whether the reveal was skipped (then `t` is the final time).
pub fn due(sounds: &mut RevealSounds, t: f64, new_best: bool, skipped: bool) -> Due {
    let prev = sounds.last_t;
    sounds.last_t = t;
    let mut due = Due::default();
    if skipped {
        // Skipping lands on the final state: the impact sounds that have not
        // played yet play now, and no count-up ticks do.
        due.stamp = !sounds.stamp_fired;
        due.toast = new_best && !sounds.toast_fired;
    } else {
        due.tick = reveal::ticks_between(prev, t) > 0;
        due.stamp = !sounds.stamp_fired && reveal::stamp_due(prev, t);
        due.toast = new_best && !sounds.toast_fired && reveal::toast_due(prev, t);
    }
    sounds.stamp_fired |= due.stamp;
    sounds.toast_fired |= due.toast;
    due
}

/// Starts the reveal for a new Result screen (called when the screen opens).
pub fn begin(state: &mut AppState) {
    state.result_entered_at = Instant::now();
    state.result_reveal_skipped = false;
    state.result_sounds = RevealSounds::default();
}

/// Seconds of the reveal at this moment, from 0 to `REVEAL_SECONDS` (and
/// past it, the final state). A skip is the final state at once.
pub fn reveal_time(state: &AppState) -> f64 {
    if state.result_reveal_skipped {
        return REVEAL_SECONDS;
    }
    if let Some(t) = devtools::frozen_reveal_seconds() {
        return t;
    }
    state
        .result_entered_at
        .elapsed()
        .as_secs_f64()
        .min(REVEAL_SECONDS)
}

/// Whether the reveal is still running (the screen needs frames at 60 fps).
pub fn running(state: &AppState) -> bool {
    !state.result_reveal_skipped && reveal_time(state) < REVEAL_SECONDS
}

/// Whether a key or click at reveal time `t` skips the reveal. Only the part
/// before the footer appears skips; after that the footer's keys work at once.
pub fn skip_consumes_key(t: f64, skipped: bool) -> bool {
    !skipped && t < reveal::FOOTER_AT
}

/// Skips the reveal if the key or click is one that skips it (see
/// `skip_consumes_key`). Returns whether it did: that input does nothing else.
pub fn skip(state: &mut AppState) -> bool {
    if !skip_consumes_key(reveal_time(state), state.result_reveal_skipped) {
        return false;
    }
    state.result_reveal_skipped = true;
    state.window.request_redraw();
    true
}

/// Plays the reveal's sounds that are due on this frame. Called by the
/// Result screen's draw, before it draws the frame's time.
pub fn advance(state: &mut AppState) {
    let t = reveal_time(state);
    let skipped = state.result_reveal_skipped;
    let new_best = state.score_update.any();
    let sounds = due(&mut state.result_sounds, t, new_best, skipped);
    if sounds.tick {
        play_ui(state, UiSound::Tick);
    }
    if sounds.stamp {
        play_ui(state, UiSound::Stamp);
    }
    if sounds.toast {
        play_ui(state, UiSound::Toast);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_skips_the_reveal_only_until_the_footer_appears() {
        assert!(skip_consumes_key(0.0, false));
        assert!(skip_consumes_key(reveal::FOOTER_AT - 0.01, false));
        assert!(!skip_consumes_key(reveal::FOOTER_AT, false));
        assert!(!skip_consumes_key(REVEAL_SECONDS, false));
        // Once skipped there is nothing left to skip.
        assert!(!skip_consumes_key(0.5, true));
    }

    /// Plays one frame at each time in `times`, returning the sounds per frame.
    fn run(times: &[f64], new_best: bool) -> Vec<Due> {
        let mut sounds = RevealSounds::default();
        times
            .iter()
            .map(|&t| due(&mut sounds, t, new_best, false))
            .collect()
    }

    fn count(dues: &[Due], pick: fn(&Due) -> bool) -> usize {
        dues.iter().filter(|d| pick(d)).count()
    }

    #[test]
    fn stamp_plays_once_at_60fps() {
        let times: Vec<f64> = (0..=200).map(|i| i as f64 / 60.0).collect();
        let dues = run(&times, false);
        assert_eq!(count(&dues, |d| d.stamp), 1);
    }

    #[test]
    fn stamp_plays_once_on_a_long_frame_across_the_impact() {
        // The frame before the impact at 1.0 s, then one 1.2 s frame over it.
        let dues = run(&[0.0, 1.0, 2.2, 2.3, 4.0], false);
        assert_eq!(count(&dues, |d| d.stamp), 1);
        assert!(dues[2].stamp, "the frame that crosses the impact sounds it");
    }

    #[test]
    fn stamp_is_not_repeated_after_the_reveal_ends() {
        let dues = run(&[1.6, 1.8, 3.2, 3.2, 9.0, 30.0], false);
        assert_eq!(count(&dues, |d| d.stamp), 1);
    }

    #[test]
    fn toast_plays_once_only_on_a_new_best() {
        let times: Vec<f64> = (0..=300).map(|i| i as f64 / 60.0).collect();
        assert_eq!(count(&run(&times, true), |d| d.toast), 1);
        assert_eq!(count(&run(&times, false), |d| d.toast), 0);
    }

    #[test]
    fn ticks_stay_within_twenty_per_second() {
        let times: Vec<f64> = (0..=240).map(|i| i as f64 / 240.0 * 3.2).collect();
        let dues = run(&times, false);
        // One tick sound per frame at most, and none outside the count-up.
        let ticks = count(&dues, |d| d.tick);
        assert!(ticks <= 24, "{ticks} tick frames");
        assert!(ticks >= 20, "{ticks} tick frames");
        for (i, d) in dues.iter().enumerate() {
            if d.tick {
                assert!(times[i] >= reveal::COUNT_START, "tick at {}", times[i]);
            }
        }
    }

    #[test]
    fn a_huge_frame_still_sounds_one_tick_and_no_double_stamp() {
        let dues = run(&[0.0, 0.1, 3.0, 3.2], true);
        assert_eq!(count(&dues, |d| d.stamp), 1);
        assert_eq!(count(&dues, |d| d.toast), 1);
        assert!(dues[2].tick, "the covered ticks sound as one");
    }

    #[test]
    fn skip_plays_the_stamp_once_and_no_ticks() {
        let mut sounds = RevealSounds::default();
        let early = due(&mut sounds, 0.3, true, false);
        assert_eq!(early, Due::default());
        let skipped = due(&mut sounds, REVEAL_SECONDS, true, true);
        assert!(skipped.stamp && skipped.toast && !skipped.tick);
        let again = due(&mut sounds, REVEAL_SECONDS, true, true);
        assert_eq!(again, Due::default());
    }

    #[test]
    fn skip_after_the_stamp_does_not_repeat_it() {
        let mut sounds = RevealSounds::default();
        due(&mut sounds, 1.8, false, false);
        assert!(due(&mut sounds, REVEAL_SECONDS, false, true) == Due::default());
    }

    #[test]
    fn skip_before_the_impact_plays_stamp_and_toast_on_a_new_best() {
        let mut sounds = RevealSounds::default();
        due(&mut sounds, 1.0, true, false);
        let skipped = due(&mut sounds, REVEAL_SECONDS, true, true);
        assert!(skipped.stamp && skipped.toast && !skipped.tick);
        // Without a new best the same skip plays the stamp but no toast.
        let mut plain = RevealSounds::default();
        due(&mut plain, 1.0, false, false);
        let skipped = due(&mut plain, REVEAL_SECONDS, false, true);
        assert!(skipped.stamp && !skipped.toast);
    }
}

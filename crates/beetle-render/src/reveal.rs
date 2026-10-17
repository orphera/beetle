//! The Stage Result reveal timeline: one pure function of the seconds since
//! the screen was entered, shared by the renderer (`screens::stage_result`)
//! and the app's sound triggers (`beetle-app/src/result_reveal.rs`). Nothing
//! here reads a clock, so a frame at any time draws and sounds the same way.
//!
//! Order of the reveal (seconds):
//! 1. `0.0 .. 0.5`  the jacket / title panel slides in over a dim scrim.
//! 2. `0.4 .. 1.6`  EX SCORE and the judgement numbers count up; the judgement
//!    rows slide in one after another, and one Tick sound per tick.
//! 3. `1.7`         RANK STAMP: the rank letter slams in, with a white flash,
//!    a shake, a ring and sparks, and the Stamp sound once at impact.
//! 4. `2.4`         the clear lamp plate wipes in (no sound: the Clear jingle
//!    already played at the end of the play).
//! 5. `2.8`         the NEW RECORD tags pop in, and Toast once on a new best.
//! 6. `3.0`         the buttons and footer fade in last.

use crate::motion::{ease_out_back, ease_out_cubic, ease_out_quad};

/// Length of the whole reveal. After it the screen is in its final state.
pub const REVEAL_SECONDS: f64 = 3.2;

/// Judgement rows that slide in (PGREAT .. MISS).
pub const ROW_COUNT: usize = 6;

const PANEL_START: f64 = 0.0;
const PANEL_SECONDS: f64 = 0.5;
/// Count-up of the EX score and the judgement numbers.
pub const COUNT_START: f64 = 0.4;
pub const COUNT_END: f64 = 1.6;
const ROW_STAGGER: f64 = 0.08;
const ROW_SECONDS: f64 = 0.3;
/// Ticks per second while the numbers count (the sound's cap: never more).
pub const TICK_RATE: f64 = 20.0;

/// The rank stamp lands here: the slam, flash, shake, burst and Stamp sound.
pub const STAMP_AT: f64 = 1.7;
const SLAM_SECONDS: f64 = 0.25;
/// The slam of the reduced-flash variant (shorter, no overshoot wobble).
const SLAM_SECONDS_REDUCED: f64 = 0.12;
const RANK_FADE_SECONDS: f64 = 0.08;
/// Scale the rank letter starts at before it settles to 1.0.
pub const RANK_START_SCALE: f32 = 2.2;
const FLASH_SECONDS: f64 = 0.15;
/// Peak alpha of the white flash.
pub const FLASH_PEAK: f32 = 0.5;
const SHAKE_SECONDS: f64 = 0.25;
/// Shake amplitude in pixels (before the scale), and the heavier one for a failed play.
pub const SHAKE_PX: f32 = 7.0;
pub const SHAKE_PX_FAILED: f32 = 9.0;
const SHAKE_HZ: f64 = 28.0;
const BURST_SECONDS: f64 = 0.45;

pub const LAMP_WIPE_AT: f64 = 2.4;
const LAMP_WIPE_SECONDS: f64 = 0.35;
pub const TAGS_AT: f64 = 2.8;
const TAGS_SECONDS: f64 = 0.3;
const FLARE_SECONDS: f64 = 0.4;
pub const FOOTER_AT: f64 = 3.0;
const FOOTER_SECONDS: f64 = 0.2;

/// Per-element progress of the reveal at one time. Every field is 0 or 1 at
/// the two ends it names and is constant outside its window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RevealStages {
    /// Jacket / title panel slide-in, 0 = off to the side, 1 = in place.
    pub panel: f32,
    /// Strength of the scrim over the backdrop: 1 at the start, 0 once the panel is in.
    pub dim: f32,
    /// Count-up of the numbers, 0 = zero, 1 = final value.
    pub count: f32,
    /// Slide-in of each judgement row, 0 = offset and transparent, 1 = in place.
    pub rows: [f32; ROW_COUNT],
    /// Scale of the rank letter: `RANK_START_SCALE` before it lands, 1.0 when settled.
    pub rank_scale: f32,
    /// Alpha of the rank letter. 0 until the stamp lands.
    pub rank_alpha: f32,
    /// Progress of the ring and sparks: 0 and 1 draw nothing, in between they spread out and fade.
    pub burst: f32,
    /// Clear lamp plate wipe, 0 = hidden, 1 = fully shown.
    pub lamp: f32,
    /// NEW RECORD tag pop-in, 0 = hidden, 1 = settled. Overshoots a little past 1 on the way.
    pub tags: f32,
    /// Flare behind the tags: 0 = none, 1 = at the pop, back to 0 as it fades.
    pub flare: f32,
    /// Opacity of the buttons and footer, 0 = hidden, 1 = shown.
    pub footer: f32,
    /// Opacity of the score and timing panels, 0 = hidden, 1 = shown.
    pub side: f32,
}

impl RevealStages {
    /// The settled screen: what every element looks like after `REVEAL_SECONDS`.
    pub const FINAL: RevealStages = RevealStages {
        panel: 1.0,
        dim: 0.0,
        count: 1.0,
        rows: [1.0; ROW_COUNT],
        rank_scale: 1.0,
        rank_alpha: 1.0,
        burst: 1.0,
        lamp: 1.0,
        tags: 1.0,
        flare: 0.0,
        footer: 1.0,
        side: 1.0,
    };
}

/// `t` moved from `start` over `dur` seconds, clamped to 0..=1.
#[inline]
fn progress(t: f64, start: f64, dur: f64) -> f32 {
    if dur <= 0.0 {
        return if t >= start { 1.0 } else { 0.0 };
    }
    ((t - start) / dur).clamp(0.0, 1.0) as f32
}

/// The slam length: shorter with reduced flashes.
pub fn slam_seconds(reduce_flashes: bool) -> f64 {
    if reduce_flashes {
        SLAM_SECONDS_REDUCED
    } else {
        SLAM_SECONDS
    }
}

/// Every element's progress at `t` seconds after the screen was entered.
/// Negative times count as the start; any time past `REVEAL_SECONDS` is the
/// final state. With `reduce_flashes` the stamp slam is shorter (the flash
/// and shake are handled by `flash_alpha` and `shake_px`).
pub fn reveal_stage(t: f64, reduce_flashes: bool) -> RevealStages {
    let t = if t.is_finite() { t } else { REVEAL_SECONDS };
    let panel = ease_out_cubic(progress(t, PANEL_START, PANEL_SECONDS));
    let count = ease_out_cubic(progress(t, COUNT_START, COUNT_END - COUNT_START));
    let mut rows = [0.0; ROW_COUNT];
    for (i, row) in rows.iter_mut().enumerate() {
        let start = COUNT_START + ROW_STAGGER * i as f64;
        *row = ease_out_cubic(progress(t, start, ROW_SECONDS));
    }

    let slam = progress(t, STAMP_AT, slam_seconds(reduce_flashes) as f64);
    // The overshoot runs the letter a little below 1.0 and it settles back.
    let rank_scale = 1.0 + (RANK_START_SCALE - 1.0) * (1.0 - ease_out_back(slam));
    let rank_alpha = ease_out_quad(progress(t, STAMP_AT, RANK_FADE_SECONDS));
    let burst = progress(t, STAMP_AT, BURST_SECONDS);

    let tags_pop = ease_out_back(progress(t, TAGS_AT, TAGS_SECONDS));
    let flare = if t >= TAGS_AT {
        1.0 - ease_out_cubic(progress(t, TAGS_AT, FLARE_SECONDS))
    } else {
        0.0
    };

    RevealStages {
        panel,
        dim: 1.0 - panel,
        count,
        rows,
        rank_scale,
        rank_alpha,
        burst,
        lamp: ease_out_cubic(progress(t, LAMP_WIPE_AT, LAMP_WIPE_SECONDS)),
        tags: tags_pop,
        flare,
        footer: ease_out_quad(progress(t, FOOTER_AT, FOOTER_SECONDS)),
        side: ease_out_cubic(progress(t, 0.25, 0.35)),
    }
}

/// Vertical shake of the content in pixels (before the UI scale) at `t`. An impulse at the stamp that decays over `SHAKE_SECONDS`.
/// A failed play shakes harder; reduced flashes turn it off.
pub fn shake_px(t: f64, failed: bool, reduce_flashes: bool) -> f32 {
    if reduce_flashes {
        return 0.0;
    }
    let d = t - STAMP_AT;
    if !(0.0..SHAKE_SECONDS).contains(&d) {
        return 0.0;
    }
    let amp = if failed { SHAKE_PX_FAILED } else { SHAKE_PX };
    let decay = (1.0 - d / SHAKE_SECONDS) as f32;
    amp * decay * (std::f64::consts::TAU * SHAKE_HZ * d).sin() as f32
}

/// Alpha of the white flash over the whole screen at `t`: `FLASH_PEAK` at
/// the stamp, fading to 0 over `FLASH_SECONDS`. None for a failed play and
/// with reduced flashes.
pub fn flash_alpha(t: f64, failed: bool, reduce_flashes: bool) -> f32 {
    if failed || reduce_flashes {
        return 0.0;
    }
    let d = t - STAMP_AT;
    if !(0.0..FLASH_SECONDS).contains(&d) {
        return 0.0;
    }
    FLASH_PEAK * (1.0 - (d / FLASH_SECONDS) as f32)
}

/// Whether the Stamp sound is due when time moves from `prev` to `t`: it
/// fires once, on the frame that first reaches the impact, however long
/// that frame is.
pub fn stamp_due(prev: f64, t: f64) -> bool {
    prev < STAMP_AT && t >= STAMP_AT
}

/// Whether the new-record Toast is due moving from `prev` to `t`.
pub fn toast_due(prev: f64, t: f64) -> bool {
    prev < TAGS_AT && t >= TAGS_AT
}

/// How many ticks have sounded by `t`: one per `1 / TICK_RATE` second of
/// the count-up, never more than the count's `(COUNT_END - COUNT_START) * TICK_RATE`.
pub fn tick_count_at(t: f64) -> u32 {
    if !t.is_finite() {
        return 0;
    }
    let max = ((COUNT_END - COUNT_START) * TICK_RATE).round() as u32;
    let n = ((t - COUNT_START) * TICK_RATE).ceil();
    n.clamp(0.0, max as f64) as u32
}

/// Ticks due when time moves from `prev` to `t`.
pub fn ticks_between(prev: f64, t: f64) -> u32 {
    tick_count_at(t).saturating_sub(tick_count_at(prev))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples() -> Vec<f64> {
        (0..=400).map(|i| i as f64 * 0.01).collect()
    }

    #[test]
    fn panel_and_count_are_monotonic_and_end_final() {
        let mut last = RevealStages::FINAL;
        let mut prev: Option<RevealStages> = None;
        for t in samples() {
            let s = reveal_stage(t, false);
            if let Some(p) = prev {
                assert!(
                    s.panel >= p.panel && s.count >= p.count && s.lamp >= p.lamp,
                    "t={t}"
                );
                assert!(s.footer >= p.footer && s.side >= p.side, "t={t}");
                for i in 0..ROW_COUNT {
                    assert!(s.rows[i] >= p.rows[i], "row {i} at t={t}");
                }
                assert!(s.dim <= p.dim, "dim at t={t}");
            }
            prev = Some(s);
            last = s;
        }
        assert_eq!(last, RevealStages::FINAL);
        assert_eq!(reveal_stage(REVEAL_SECONDS, false), RevealStages::FINAL);
    }

    #[test]
    fn rows_are_staggered_in_order() {
        let t = 0.9;
        let s = reveal_stage(t, false);
        for i in 1..ROW_COUNT {
            assert!(
                s.rows[i] <= s.rows[i - 1],
                "row {i} is ahead of row {}",
                i - 1
            );
        }
    }

    #[test]
    fn rank_lands_at_the_stamp_with_overshoot() {
        let before = reveal_stage(STAMP_AT - 0.01, false);
        assert_eq!(before.rank_alpha, 0.0);
        assert_eq!(before.burst, 0.0);
        let at = reveal_stage(STAMP_AT, false);
        assert_eq!(at.rank_scale, RANK_START_SCALE);
        // Somewhere in the slam the letter dips under 1.0 (the overshoot).
        let min = samples()
            .into_iter()
            .map(|t| reveal_stage(t, false).rank_scale)
            .fold(f32::MAX, f32::min);
        assert!(min < 1.0, "min scale {min}");
        assert_eq!(
            reveal_stage(STAMP_AT + SLAM_SECONDS + 0.5, false).rank_scale,
            1.0
        );
    }

    #[test]
    fn reduced_flashes_shorten_the_slam() {
        let end = STAMP_AT + SLAM_SECONDS_REDUCED;
        assert_eq!(reveal_stage(end, true).rank_scale, 1.0);
        // The normal slam is still moving at that time (it overshoots below 1.0 first).
        assert_ne!(reveal_stage(end, false).rank_scale, 1.0);
        assert_eq!(slam_seconds(true), SLAM_SECONDS_REDUCED);
    }

    #[test]
    fn flash_is_half_then_gone() {
        assert_eq!(flash_alpha(STAMP_AT - 0.001, false, false), 0.0);
        assert!((flash_alpha(STAMP_AT, false, false) - FLASH_PEAK).abs() < 1e-6);
        let mut last = f32::MAX;
        for i in 0..=15 {
            let a = flash_alpha(STAMP_AT + i as f64 * 0.01, false, false);
            assert!(a <= last, "flash rose at step {i}");
            last = a;
        }
        assert_eq!(flash_alpha(STAMP_AT + FLASH_SECONDS, false, false), 0.0);
    }

    #[test]
    fn failed_and_reduced_have_no_flash() {
        for t in samples() {
            assert_eq!(flash_alpha(t, true, false), 0.0);
            assert_eq!(flash_alpha(t, false, true), 0.0);
            assert_eq!(shake_px(t, false, true), 0.0);
        }
    }

    #[test]
    fn shake_is_bounded_and_decays() {
        let mut max_normal = 0.0f32;
        let mut max_failed = 0.0f32;
        for t in samples() {
            max_normal = max_normal.max(shake_px(t, false, false).abs());
            max_failed = max_failed.max(shake_px(t, true, false).abs());
        }
        assert!(max_normal > 0.0 && max_normal <= SHAKE_PX);
        assert!(max_failed > max_normal && max_failed <= SHAKE_PX_FAILED);
        assert_eq!(shake_px(STAMP_AT - 0.01, false, false), 0.0);
        assert_eq!(shake_px(STAMP_AT + SHAKE_SECONDS, false, false), 0.0);
        // The late part of the shake is smaller than the early part.
        let early = (0..10)
            .map(|i| shake_px(STAMP_AT + i as f64 * 0.002, false, false).abs())
            .fold(0.0, f32::max);
        let late = (0..10)
            .map(|i| shake_px(STAMP_AT + 0.2 + i as f64 * 0.002, false, false).abs())
            .fold(0.0, f32::max);
        assert!(late < early);
    }

    #[test]
    fn stamp_fires_exactly_once_for_any_frame_pattern() {
        // Fixed 60 fps, irregular frames, and one huge frame that skips the impact.
        for step in [1.0 / 60.0, 0.013, 0.07, 0.4, 2.0] {
            let mut t = 0.0;
            let mut prev = 0.0;
            let mut fired = 0;
            while t < REVEAL_SECONDS + step {
                let now = t.min(REVEAL_SECONDS + 1.0);
                if stamp_due(prev, now) {
                    fired += 1;
                }
                prev = now;
                t += step;
            }
            assert_eq!(fired, 1, "step {step}");
        }
    }

    #[test]
    fn toast_fires_once_too() {
        let mut fired = 0;
        let mut prev = 0.0;
        for t in [0.5, 2.7, 2.79, 2.81, 3.0, 9.0] {
            if toast_due(prev, t) {
                fired += 1;
            }
            prev = t;
        }
        assert_eq!(fired, 1);
    }

    #[test]
    fn ticks_are_capped_at_twenty_per_second() {
        // The whole count-up holds 24 ticks at 20/s, never more.
        let total = tick_count_at(REVEAL_SECONDS);
        assert_eq!(total, 24);
        for start in samples() {
            let in_one_second = tick_count_at(start + 1.0) - tick_count_at(start);
            assert!(
                in_one_second <= TICK_RATE as u32,
                "{in_one_second} ticks from {start}"
            );
        }
        assert_eq!(tick_count_at(COUNT_START - 0.1), 0);
    }

    #[test]
    fn ticks_are_not_missed_on_large_frames() {
        // One 0.5 s frame across the count counts every tick in it.
        let all = tick_count_at(COUNT_END);
        assert_eq!(ticks_between(0.0, COUNT_END), all);
        // Frame by frame adds up to the same.
        let mut sum = 0;
        let mut prev = 0.0;
        let mut t = 0.0;
        while t < REVEAL_SECONDS {
            t += 1.0 / 60.0;
            let now = t.min(REVEAL_SECONDS);
            sum += ticks_between(prev, now);
            prev = now;
        }
        assert_eq!(sum, tick_count_at(REVEAL_SECONDS));
    }

    #[test]
    fn tags_overshoot_only_a_little_and_settle() {
        let peak = samples()
            .into_iter()
            .map(|t| reveal_stage(t, false).tags)
            .fold(0.0, f32::max);
        assert!(peak > 1.0 && peak < 1.2, "peak {peak}");
        assert_eq!(reveal_stage(REVEAL_SECONDS, false).tags, 1.0);
        assert_eq!(reveal_stage(REVEAL_SECONDS, false).flare, 0.0);
    }

    #[test]
    fn skipping_is_the_final_state() {
        // The app's skip moves the clock to the end: that is the final state.
        assert_eq!(reveal_stage(REVEAL_SECONDS, false), RevealStages::FINAL);
        assert_eq!(reveal_stage(REVEAL_SECONDS, true), RevealStages::FINAL);
        assert_eq!(reveal_stage(1e6, false), RevealStages::FINAL);
        assert_eq!(reveal_stage(f64::NAN, false), RevealStages::FINAL);
    }

    #[test]
    fn reveal_before_zero_is_the_start() {
        let s = reveal_stage(-1.0, false);
        assert_eq!(s.panel, 0.0);
        assert_eq!(s.count, 0.0);
        assert_eq!(s.footer, 0.0);
        assert_eq!(tick_count_at(-1.0), 0);
    }
}

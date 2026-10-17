//! The turntable: a scratch axis of a controller, played as the scratch lane.
//!
//! A turntable is an axis that turns freely. Each report's change of the
//! axis is taken the short way round (`hid::wrap_delta`), so an 8- or 16-bit
//! counter that wraps reads as a small move. A move of at least
//! `MOVE_COUNTS` raw counts in either direction is scratching: it presses
//! the scratch lane, which stays pressed while the axis keeps moving, and
//! lets go `RELEASE_AFTER` after the last move. Both directions press it, so
//! a change of direction holds the lane without a new press.
//!
//! This is a pure state machine; the raw input thread feeds it the axis of
//! each report and calls `tick` on its miss-check timer during a play.

use std::time::{Duration, Instant};

use crate::hid::wrap_delta;

/// Raw counts per report that count as a move (both directions).
pub const MOVE_COUNTS: i32 = 2;
/// The press lets go this long after the last move.
pub const RELEASE_AFTER: Duration = Duration::from_millis(60);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Turntable {
    /// The axis value of the last report, once there was one.
    prev: Option<u32>,
    held: bool,
    last_move: Option<Instant>,
}

impl Turntable {
    /// Forgets the axis and any press (a new play starts from rest).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// One report of the axis (`raw`, `bits` wide) read at `at`. `Some(true)`
    /// when a press starts, `None` otherwise.
    pub fn report(&mut self, raw: u32, bits: u8, at: Instant) -> Option<bool> {
        let Some(prev) = self.prev.replace(raw) else {
            return None;
        };
        if wrap_delta(prev, raw, bits).abs() < MOVE_COUNTS {
            return None;
        }
        self.last_move = Some(at);
        if self.held {
            None
        } else {
            self.held = true;
            Some(true)
        }
    }

    /// The release timer at `at`: `Some(false)` when the held press lets go.
    pub fn tick(&mut self, at: Instant) -> Option<bool> {
        if !self.held {
            return None;
        }
        let last = self.last_move?;
        if at.saturating_duration_since(last) < RELEASE_AFTER {
            return None;
        }
        self.held = false;
        Some(false)
    }

    pub fn is_held(&self) -> bool {
        self.held
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn the_first_report_only_sets_the_baseline() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        assert_eq!(tt.report(100, 8, t), None);
        assert!(!tt.is_held());
    }

    #[test]
    fn a_move_of_one_count_is_not_scratching() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        tt.report(100, 8, t);
        assert_eq!(tt.report(101, 8, t + ms(1)), None);
        assert_eq!(tt.report(100, 8, t + ms(2)), None);
        assert!(!tt.is_held());
    }

    #[test]
    fn a_move_presses_once_and_holds_while_it_keeps_moving() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        tt.report(100, 8, t);
        assert_eq!(tt.report(103, 8, t + ms(5)), Some(true));
        // Still moving: no second press.
        assert_eq!(tt.report(106, 8, t + ms(10)), None);
        assert_eq!(tt.report(109, 8, t + ms(20)), None);
        assert!(tt.is_held());
        // Quiet for 59 ms after the last move: still held.
        assert_eq!(tt.tick(t + ms(79)), None);
        assert!(tt.is_held());
        // 60 ms after the last move: released.
        assert_eq!(tt.tick(t + ms(80)), Some(false));
        assert!(!tt.is_held());
        assert_eq!(tt.tick(t + ms(200)), None);
    }

    #[test]
    fn both_directions_press_and_a_reversal_keeps_the_press() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        tt.report(100, 8, t);
        assert_eq!(tt.report(95, 8, t + ms(1)), Some(true));
        assert_eq!(tt.report(105, 8, t + ms(30)), None);
        assert_eq!(tt.report(100, 8, t + ms(60)), None);
        assert!(tt.is_held());
        assert_eq!(tt.tick(t + ms(119)), None);
        assert_eq!(tt.tick(t + ms(120)), Some(false));
    }

    #[test]
    fn an_eight_bit_wrap_is_a_small_move_not_a_long_one() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        tt.report(250, 8, t);
        // 250 -> 3 is +9 across the wrap: a press, not a jump backwards.
        assert_eq!(tt.report(3, 8, t + ms(1)), Some(true));
        tt.tick(t + ms(100));
        // A sixteen-bit wrap works the same.
        let mut tt = Turntable::default();
        tt.report(65530, 16, t);
        assert_eq!(tt.report(4, 16, t + ms(1)), Some(true));
    }

    #[test]
    fn a_press_can_start_again_after_a_release() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        tt.report(0, 16, t);
        assert_eq!(tt.report(10, 16, t + ms(1)), Some(true));
        assert_eq!(tt.tick(t + ms(61)), Some(false));
        assert_eq!(tt.report(20, 16, t + ms(100)), Some(true));
    }

    #[test]
    fn nothing_is_released_without_a_press() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        assert_eq!(tt.tick(t + ms(1000)), None);
    }

    #[test]
    fn reset_forgets_the_axis_and_the_press() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        tt.report(0, 8, t);
        tt.report(9, 8, t + ms(1));
        tt.reset();
        assert!(!tt.is_held());
        // The next report is a baseline again, not a move from 9.
        assert_eq!(tt.report(50, 8, t + ms(2)), None);
    }
}

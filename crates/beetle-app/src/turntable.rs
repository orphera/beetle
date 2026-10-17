//! The turntable: a scratch axis of a controller, played as the scratch lane.
//!
//! A turntable is an axis that turns freely. Each report's change of the
//! axis is taken the short way round (`hid::wrap_delta`), so an 8- or 16-bit
//! counter that wraps reads as a small move. The direction of a move is
//! `Dir::Cw` or `Dir::Ccw`, and each direction is its own press (see
//! `raw_input`, which plays them on the scratch lane).
//!
//! - A press starts once `MOVE_COUNTS` counts have piled up in one direction.
//!   The counts add up across reports, so a slow turn (one count per report)
//!   presses too. A move the other way starts the count again.
//! - While the press is down, any move in its direction refreshes the last
//!   move, even by one count, and keeps the press down.
//! - A move the other way (by at least one count) lets the press go at once
//!   and starts counting the new direction (IIDX / LR2 convention: a reversal
//!   is a new press once it has its counts).
//! - The press lets go `RELEASE_AFTER` after the last move, from `tick`.
//!
//! This is a pure state machine; the raw input thread feeds it the axis of
//! each report of its device and calls `tick` on its miss-check timer during
//! a play. Each device has its own `Turntable`.

use std::time::{Duration, Instant};

use crate::hid::wrap_delta;

/// Counts of one direction that make a press.
pub const MOVE_COUNTS: i32 = 2;
/// The press lets go this long after the last move.
pub const RELEASE_AFTER: Duration = Duration::from_millis(60);

/// The direction of a move: `Cw` is the axis counting up, `Ccw` down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Cw,
    Ccw,
}

/// The presses and releases one report (or timer tick) makes, in the order
/// to play them: the release first, then the press.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Change {
    pub release: Option<Dir>,
    pub press: Option<Dir>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Turntable {
    /// The axis value of the last report, once there was one.
    prev: Option<u32>,
    /// The direction of the press that is down.
    held: Option<Dir>,
    /// Movement counted toward a press that is not down: its sign is the
    /// direction, 0 when nothing is counted.
    counted: i32,
    /// When the axis last moved (while it counts, or while a press is down).
    last_move: Option<Instant>,
}

impl Turntable {
    /// Forgets the axis, the count and any press (a new play starts from rest).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// One report of the axis (`raw`, `bits` wide) read at `at`.
    pub fn report(&mut self, raw: u32, bits: u8, at: Instant) -> Change {
        let Some(prev) = self.prev.replace(raw) else {
            return Change::default();
        };
        let delta = wrap_delta(prev, raw, bits);
        if delta == 0 {
            return Change::default();
        }
        let dir = if delta > 0 { Dir::Cw } else { Dir::Ccw };
        self.last_move = Some(at);

        let mut change = Change::default();
        match self.held {
            // Turning on in the held direction keeps the press.
            Some(held) if held == dir => return change,
            // A reversal lets the old direction go and starts counting anew.
            Some(held) => {
                change.release = Some(held);
                self.held = None;
                self.counted = 0;
            }
            None => {}
        }

        if self.counted != 0 && (self.counted > 0) != (delta > 0) {
            self.counted = 0;
        }
        self.counted += delta;
        if self.counted.abs() >= MOVE_COUNTS {
            self.counted = 0;
            self.held = Some(dir);
            change.press = Some(dir);
        }
        change
    }

    /// The release timer at `at`: the direction of a press that lets go once
    /// `RELEASE_AFTER` has passed since the last move.
    pub fn tick(&mut self, at: Instant) -> Option<Dir> {
        let held = self.held?;
        let last = self.last_move?;
        if at.saturating_duration_since(last) < RELEASE_AFTER {
            return None;
        }
        self.held = None;
        Some(held)
    }

    /// The direction of the press that is down, if any.
    pub fn held(&self) -> Option<Dir> {
        self.held
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// Feeds `raws` one per millisecond from `t` and returns the changes that
    /// were not empty, with the millisecond they came at.
    fn feed(tt: &mut Turntable, t: Instant, bits: u8, raws: &[u32]) -> Vec<(u64, Change)> {
        raws.iter()
            .enumerate()
            .filter_map(|(i, &raw)| {
                let c = tt.report(raw, bits, t + ms(i as u64));
                (c != Change::default()).then_some((i as u64, c))
            })
            .collect()
    }

    fn press(dir: Dir) -> Change {
        Change {
            release: None,
            press: Some(dir),
        }
    }

    fn release(dir: Dir) -> Change {
        Change {
            release: Some(dir),
            press: None,
        }
    }

    #[test]
    fn the_first_report_only_sets_the_baseline() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        assert_eq!(tt.report(100, 8, t), Change::default());
        assert_eq!(tt.held(), None);
    }

    #[test]
    fn a_slow_turn_of_one_count_per_report_presses_on_the_second() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        let changes = feed(&mut tt, t, 16, &[100, 101, 102, 103, 104, 105]);
        // Counts add up across reports: the second count presses, and the
        // rest keep the press down without a new one.
        assert_eq!(changes, vec![(2, press(Dir::Cw))]);
        assert_eq!(tt.held(), Some(Dir::Cw));
    }

    #[test]
    fn a_slow_turn_at_1000_hz_is_held_while_it_turns_and_lets_go_after_60_ms() {
        // One count every millisecond for 500 ms, then still.
        let t = Instant::now();
        let mut tt = Turntable::default();
        let raws: Vec<u32> = (0..=500).collect();
        let changes = feed(&mut tt, t, 16, &raws);
        assert_eq!(
            changes,
            vec![(2, press(Dir::Cw))],
            "one press, no re-presses"
        );
        // Last move at 500 ms: held until 60 ms after it.
        assert_eq!(tt.tick(t + ms(559)), None);
        assert_eq!(tt.tick(t + ms(560)), Some(Dir::Cw));
        assert_eq!(tt.tick(t + ms(900)), None);
    }

    #[test]
    fn a_one_count_move_keeps_a_held_press_past_60_ms() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        tt.report(0, 16, t);
        assert_eq!(tt.report(5, 16, t + ms(1)), press(Dir::Cw));
        // One count every 50 ms: never 60 ms without a move.
        let mut raw = 5;
        for step in 1..=10u64 {
            raw += 1;
            assert_eq!(tt.report(raw, 16, t + ms(1 + 50 * step)), Change::default());
            assert_eq!(tt.tick(t + ms(1 + 50 * step + 59)), None);
        }
        // Last move at 501 ms: released 60 ms later.
        assert_eq!(tt.tick(t + ms(561)), Some(Dir::Cw));
    }

    #[test]
    fn a_fast_spin_presses_at_once_and_holds() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        let raws: Vec<u32> = (0..50u32).map(|i| 100 + i * 5).collect();
        let changes = feed(&mut tt, t, 16, &raws);
        assert_eq!(changes, vec![(1, press(Dir::Cw))]);
        assert_eq!(tt.tick(t + ms(49 + 59)), None);
        assert_eq!(tt.tick(t + ms(49 + 60)), Some(Dir::Cw));
    }

    #[test]
    fn a_reversal_releases_then_presses_the_new_direction_after_its_counts() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        tt.report(100, 8, t);
        assert_eq!(tt.report(103, 8, t + ms(1)), press(Dir::Cw));
        // One count back: the old direction lets go, the new one has 1 of 2.
        assert_eq!(tt.report(102, 8, t + ms(2)), release(Dir::Cw));
        assert_eq!(tt.held(), None);
        // The second count back presses the new direction.
        assert_eq!(tt.report(101, 8, t + ms(3)), press(Dir::Ccw));
        assert_eq!(tt.held(), Some(Dir::Ccw));
    }

    #[test]
    fn a_fast_reversal_is_a_release_and_a_press_in_one_report() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        tt.report(100, 8, t);
        assert_eq!(tt.report(104, 8, t + ms(1)), press(Dir::Cw));
        assert_eq!(
            tt.report(96, 8, t + ms(2)),
            Change {
                release: Some(Dir::Cw),
                press: Some(Dir::Ccw),
            }
        );
    }

    #[test]
    fn reversing_counts_restart_at_the_reversal() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        tt.report(100, 8, t);
        // Two counts up, then one down: the count restarts at the reversal, so
        // the down count is 1 and the next down count presses.
        assert_eq!(tt.report(101, 8, t + ms(1)), Change::default());
        assert_eq!(tt.report(100, 8, t + ms(2)), Change::default());
        assert_eq!(tt.report(99, 8, t + ms(3)), press(Dir::Ccw));
    }

    #[test]
    fn jitter_of_one_count_around_rest_never_presses() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        // Strictly alternating one count each way: every reversal restarts the count.
        let raws: Vec<u32> = (0..200)
            .map(|i| if i % 2 == 0 { 100 } else { 101 })
            .collect();
        assert_eq!(feed(&mut tt, t, 8, &raws), vec![]);
        // A sensor that repeats a value now and then, still alternating.
        let raws = [100, 101, 101, 100, 101, 100, 101, 100];
        assert_eq!(feed(&mut tt, t + ms(300), 8, &raws), vec![]);
        assert_eq!(tt.held(), None);
    }

    #[test]
    fn jitter_while_a_press_is_down_releases_it_and_does_not_re_press_alone() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        tt.report(100, 8, t);
        assert_eq!(tt.report(103, 8, t + ms(1)), press(Dir::Cw));
        assert_eq!(tt.report(102, 8, t + ms(2)), release(Dir::Cw));
        assert_eq!(tt.report(103, 8, t + ms(3)), Change::default());
        assert_eq!(tt.report(102, 8, t + ms(4)), Change::default());
        assert_eq!(tt.held(), None);
    }

    #[test]
    fn an_eight_bit_wrap_is_a_small_move_not_a_long_one() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        tt.report(250, 8, t);
        // 250 -> 252 -> 255 -> 1 -> 3 is a steady +2 each, across the wrap.
        let changes = feed(&mut tt, t + ms(1), 8, &[252, 255, 1, 3]);
        assert_eq!(changes, vec![(0, press(Dir::Cw))]);
        assert_eq!(tt.held(), Some(Dir::Cw), "no release across the wrap");
        // Backwards across the wrap: 3 -> 1 is -2, a reversal that presses at
        // once; 1 -> 255 -> 253 is more of the same direction, across the wrap.
        assert_eq!(
            tt.report(1, 8, t + ms(10)),
            Change {
                release: Some(Dir::Cw),
                press: Some(Dir::Ccw),
            }
        );
        assert_eq!(tt.report(255, 8, t + ms(11)), Change::default());
        assert_eq!(tt.report(253, 8, t + ms(12)), Change::default());
        assert_eq!(tt.held(), Some(Dir::Ccw));
    }

    #[test]
    fn a_sixteen_bit_wrap_works_the_same() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        tt.report(65530, 16, t);
        assert_eq!(tt.report(4, 16, t + ms(1)), press(Dir::Cw));
        assert_eq!(tt.report(9, 16, t + ms(2)), Change::default());
        assert_eq!(tt.held(), Some(Dir::Cw));
    }

    #[test]
    fn a_press_can_start_again_after_a_release() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        tt.report(0, 16, t);
        assert_eq!(tt.report(10, 16, t + ms(1)), press(Dir::Cw));
        assert_eq!(tt.tick(t + ms(61)), Some(Dir::Cw));
        assert_eq!(tt.report(20, 16, t + ms(100)), press(Dir::Cw));
    }

    #[test]
    fn nothing_is_released_without_a_press() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        assert_eq!(tt.tick(t + ms(1000)), None);
        tt.report(100, 8, t);
        tt.report(101, 8, t + ms(1));
        assert_eq!(tt.tick(t + ms(1000)), None);
    }

    #[test]
    fn reset_forgets_the_axis_the_count_and_the_press() {
        let t = Instant::now();
        let mut tt = Turntable::default();
        tt.report(0, 8, t);
        tt.report(9, 8, t + ms(1));
        tt.reset();
        assert_eq!(tt.held(), None);
        // The next report is a baseline again, not a move from 9.
        assert_eq!(tt.report(50, 8, t + ms(2)), Change::default());
        assert_eq!(tt.report(51, 8, t + ms(3)), Change::default());
        assert_eq!(tt.report(52, 8, t + ms(4)), press(Dir::Cw));
    }
}

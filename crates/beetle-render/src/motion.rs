//! Shared easing/motion curves for juicing up otherwise-linear animations
//! (combo punches, hit bursts, score reveals, screen transitions).
//!
//! Every animation driver in this codebase previously interpolated with raw
//! linear `(1.0 - progress) * amplitude` math, which reads as stiff/robotic
//! next to commercial rhythm game UI. These are the standard Penner-style
//! easing curves, kept tiny (no `f32` transcendental beyond `powi`/`sqrt`) so
//! they're safe to call every frame from render code.
//!
//! All functions take and return `t` clamped to `[0.0, 1.0]` representing
//! normalized animation progress.

/// Starts fast, decelerates into the end value. Good for "settling" motion
/// (combo digit drifting back down, hit-burst shrink).
#[inline]
pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    1.0 - inv * inv * inv
}

/// Gentler deceleration than cubic; good for fades where linear alpha reads
/// as an abrupt cutoff near the end.
#[inline]
pub fn ease_out_quad(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    1.0 - inv * inv
}

/// Overshoots past 1.0 then springs back — the "pop" used for combo
/// milestones, rank-letter reveals, and anything that should feel like it
/// has weight and snaps into place rather than just arriving.
#[inline]
pub fn ease_out_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    const C1: f32 = 1.70158;
    const C3: f32 = C1 + 1.0;
    let inv = t - 1.0;
    1.0 + C3 * inv * inv * inv + C1 * inv * inv
}

/// Accelerates into the end value — used where a fade should linger near
/// full strength before dropping away quickly (burst alpha).
#[inline]
pub fn ease_in_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t
}

/// Largest time step (seconds) a menu animation takes in one frame. A stall
/// (a window drag, a hitch) advances the motion by this much at most.
pub const MOTION_MAX_DT: f32 = 0.1;

/// Distance under which an eased value snaps onto its target.
pub const SNAP_EPSILON: f32 = 0.005;

/// Rate (per second) of the song list's scroll easing.
pub const LIST_SCROLL_RATE: f32 = 22.0;

/// A list jump of more rows than this does not travel the whole list: it
/// starts this many rows short of the target and eases in at `LIST_JUMP_RATE`.
pub const LIST_JUMP_ROWS: f32 = 12.0;

/// Rate (per second) of the song list's easing after a long jump.
pub const LIST_JUMP_RATE: f32 = 40.0;

/// How long the selected row takes to indent after it becomes selected.
pub const SELECT_EMPHASIS_SECS: f32 = 0.12;

/// How long the detail panel takes to fade in after the selection changes.
pub const DETAIL_FADE_SECS: f32 = 0.15;

/// Exponential smoothing of `current` toward `target`: each call closes a
/// fixed share of the remaining distance, so the motion decelerates and
/// never overshoots. `dt` is the frame's real elapsed seconds (clamped to
/// `MOTION_MAX_DT`, non-finite treated as zero); `rate` is per second. The
/// result snaps onto `target` once it is within `SNAP_EPSILON`.
#[inline]
pub fn ease_toward(current: f32, target: f32, dt: f32, rate: f32) -> f32 {
    if (target - current).abs() <= SNAP_EPSILON {
        return target;
    }
    let dt = if dt.is_finite() {
        dt.clamp(0.0, MOTION_MAX_DT)
    } else {
        0.0
    };
    let share = 1.0 - (-rate * dt).exp();
    let next = current + (target - current) * share;
    if (target - next).abs() <= SNAP_EPSILON {
        target
    } else {
        next
    }
}

/// One frame of the song list's scroll: `visual` (fractional first visible
/// row) eases toward `target`. A jump longer than `LIST_JUMP_ROWS` starts
/// `LIST_JUMP_ROWS` short of the target and eases in faster.
#[inline]
pub fn list_scroll_step(visual: f32, target: f32, dt: f32) -> f32 {
    let gap = target - visual;
    if gap.abs() > LIST_JUMP_ROWS {
        let from = target - gap.signum() * LIST_JUMP_ROWS;
        ease_toward(from, target, dt, LIST_JUMP_RATE)
    } else {
        ease_toward(visual, target, dt, LIST_SCROLL_RATE)
    }
}

/// Share of a `span` second animation that `elapsed` seconds have run, in
/// `[0, 1]`. A non-positive span is already finished.
#[inline]
pub fn span_progress(elapsed: f32, span: f32) -> f32 {
    if span <= 0.0 || !elapsed.is_finite() {
        return 1.0;
    }
    (elapsed / span).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_out_cubic_bounds_and_monotonic() {
        assert!((ease_out_cubic(0.0) - 0.0).abs() < f32::EPSILON);
        assert!((ease_out_cubic(1.0) - 1.0).abs() < f32::EPSILON);
        assert!(ease_out_cubic(0.5) > 0.5); // decelerating curve is ahead of linear at midpoint
    }

    #[test]
    fn ease_out_quad_bounds() {
        assert!((ease_out_quad(0.0) - 0.0).abs() < f32::EPSILON);
        assert!((ease_out_quad(1.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn ease_out_back_overshoots_past_one() {
        // Characteristic signature of a "back" ease: it exceeds 1.0 before
        // t=1.0, which is what produces the spring/pop sensation.
        let mut max = 0.0f32;
        let mut t = 0.0;
        while t <= 1.0 {
            max = max.max(ease_out_back(t));
            t += 0.01;
        }
        assert!(max > 1.0, "expected overshoot, got max={}", max);
        assert!((ease_out_back(1.0) - 1.0).abs() < 0.001);
    }

    #[test]
    fn ease_in_cubic_bounds() {
        assert!((ease_in_cubic(0.0) - 0.0).abs() < f32::EPSILON);
        assert!((ease_in_cubic(1.0) - 1.0).abs() < f32::EPSILON);
        assert!(ease_in_cubic(0.5) < 0.5); // accelerating curve lags linear at midpoint
    }

    #[test]
    fn clamps_out_of_range_input() {
        assert!((ease_out_cubic(-1.0) - 0.0).abs() < f32::EPSILON);
        assert!((ease_out_cubic(2.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn ease_toward_converges_on_the_target() {
        let mut v = 0.0f32;
        for _ in 0..200 {
            v = ease_toward(v, 10.0, 1.0 / 60.0, LIST_SCROLL_RATE);
        }
        assert_eq!(v, 10.0, "it lands exactly on the target");
    }

    #[test]
    fn ease_toward_never_overshoots_or_reverses() {
        let (mut up, mut down) = (0.0f32, 10.0f32);
        let mut last_up = up;
        let mut last_down = down;
        for i in 0..400 {
            // Uneven frame times, including a stall, must not change that.
            let dt = if i % 17 == 0 {
                0.5
            } else {
                0.004 + (i % 5) as f32 * 0.003
            };
            up = ease_toward(up, 10.0, dt, LIST_SCROLL_RATE);
            down = ease_toward(down, 0.0, dt, LIST_SCROLL_RATE);
            assert!(up >= last_up && up <= 10.0, "up={up} last={last_up}");
            assert!(
                down <= last_down && down >= 0.0,
                "down={down} last={last_down}"
            );
            last_up = up;
            last_down = down;
        }
    }

    #[test]
    fn ease_toward_clamps_the_frame_time() {
        let stalled = ease_toward(0.0, 10.0, 5.0, LIST_SCROLL_RATE);
        let capped = ease_toward(0.0, 10.0, MOTION_MAX_DT, LIST_SCROLL_RATE);
        assert_eq!(
            stalled, capped,
            "a stall advances no more than one capped step"
        );
        assert!(stalled < 10.0, "a stall does not teleport the list");
        assert_eq!(ease_toward(3.0, 9.0, 0.0, LIST_SCROLL_RATE), 3.0);
        assert_eq!(ease_toward(3.0, 9.0, f32::NAN, LIST_SCROLL_RATE), 3.0);
        assert!(ease_toward(3.0, 9.0, -1.0, LIST_SCROLL_RATE) >= 3.0);
    }

    #[test]
    fn ease_toward_snaps_within_epsilon() {
        assert_eq!(ease_toward(4.996, 5.0, 0.016, LIST_SCROLL_RATE), 5.0);
        assert_eq!(ease_toward(4.997, 5.0, 0.0, 1.0), 5.0);
        // Farther away it still moves rather than snapping.
        let v = ease_toward(4.0, 5.0, 0.016, LIST_SCROLL_RATE);
        assert!(v > 4.0 && v < 5.0, "v={v}");
    }

    #[test]
    fn list_scroll_step_eases_short_moves_and_caps_long_jumps() {
        // One row: eased, not snapped, and it arrives within a second.
        let one = list_scroll_step(0.0, 1.0, 0.016);
        assert!(one > 0.0 && one < 1.0, "one={one}");
        let mut v = 0.0;
        let mut frames = 0;
        while v != 1.0 && frames < 120 {
            v = list_scroll_step(v, 1.0, 1.0 / 60.0);
            frames += 1;
        }
        assert_eq!(v, 1.0);
        assert!(
            frames < 60,
            "a one-row move settles within a second: {frames}"
        );

        // A Home/End jump of 90 rows starts within LIST_JUMP_ROWS of its target
        // rather than sweeping through the whole list.
        let jumped = list_scroll_step(0.0, 90.0, 0.016);
        assert!(jumped >= 90.0 - LIST_JUMP_ROWS - 1e-3, "jumped={jumped}");
        assert!(jumped < 90.0);
        // And going up the list works the same way.
        let back = list_scroll_step(90.0, 0.0, 0.016);
        assert!(back <= LIST_JUMP_ROWS + 1e-3 && back > 0.0, "back={back}");
    }

    #[test]
    fn span_progress_runs_zero_to_one() {
        assert_eq!(span_progress(0.0, 0.15), 0.0);
        assert!((span_progress(0.075, 0.15) - 0.5).abs() < 1e-6);
        assert_eq!(span_progress(1.0, 0.15), 1.0);
        assert_eq!(span_progress(-1.0, 0.15), 0.0);
        assert_eq!(span_progress(0.0, 0.0), 1.0);
    }
}

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
}

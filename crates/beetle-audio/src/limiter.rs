//! The mixer's last stage: keeps the summed voices inside full scale without
//! the harsh distortion of cutting them off.
//!
//! A gain rides the louder channel's level, falling within about a
//! millisecond when the mix would pass `THRESHOLD` and rising back over
//! `RELEASE_SECONDS`. It has no look-ahead, so it adds no latency (a key
//! sound still starts on its frame); the overshoot a 1 ms fall lets through
//! is rounded off by a soft clip above the threshold instead of cut. Below
//! the threshold the mix passes untouched.

/// Where limiting starts, of full scale (about -0.9 dBFS).
const THRESHOLD: f32 = 0.9;
const ATTACK_SECONDS: f32 = 0.001;
const RELEASE_SECONDS: f32 = 0.08;

pub struct Limiter {
    gain: f32,
    attack: f32,
    release: f32,
}

impl Limiter {
    pub fn new(sample_rate: u32) -> Self {
        let per_frame = |seconds: f32| 1.0 - (-1.0 / (seconds * sample_rate.max(1) as f32)).exp();
        Self {
            gain: 1.0,
            attack: per_frame(ATTACK_SECONDS),
            release: per_frame(RELEASE_SECONDS),
        }
    }

    /// Limits interleaved stereo `frames` in place.
    pub fn process(&mut self, frames: &mut [f32]) {
        for frame in frames.chunks_exact_mut(2) {
            let peak = frame[0].abs().max(frame[1].abs());
            let wanted = if peak > THRESHOLD {
                THRESHOLD / peak
            } else {
                1.0
            };
            let rate = if wanted < self.gain {
                self.attack
            } else {
                self.release
            };
            self.gain += (wanted - self.gain) * rate;
            if self.gain > 0.9999 {
                self.gain = 1.0;
            }
            for s in frame.iter_mut() {
                *s = soft_clip(*s * self.gain);
            }
        }
    }
}

/// Passes `x` below the threshold; above it, bends toward full scale and
/// never goes past it.
fn soft_clip(x: f32) -> f32 {
    let a = x.abs();
    if a <= THRESHOLD {
        return x;
    }
    let knee = 1.0 - THRESHOLD;
    (THRESHOLD + knee * ((a - THRESHOLD) / knee).tanh()).copysign(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quiet_mix_passes_untouched() {
        let mut limiter = Limiter::new(44100);
        let mut frames: Vec<f32> = (0..2000).map(|i| (i as f32 * 0.01).sin() * 0.8).collect();
        let before = frames.clone();
        limiter.process(&mut frames);
        assert_eq!(frames, before);
    }

    #[test]
    fn a_loud_mix_stays_inside_full_scale_and_recovers() {
        let mut limiter = Limiter::new(44100);
        // A tone at three times full scale for 0.1 s, then a quiet one.
        let mut loud: Vec<f32> = (0..8820).map(|i| (i as f32 * 0.05).sin() * 3.0).collect();
        limiter.process(&mut loud);
        assert!(loud.iter().all(|s| s.abs() <= 1.0));
        // After the attack, the tone's peaks sit just past the threshold, in
        // the soft clip's bend.
        let settled = loud[4000..].iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!(settled > THRESHOLD && settled < 0.97, "{settled}");
        let mut quiet: Vec<f32> = (0..88200).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        limiter.process(&mut quiet);
        let tail = quiet[80000..].iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!((tail - 0.5).abs() < 0.01, "{tail}");
    }

    #[test]
    fn the_soft_clip_bends_above_the_threshold() {
        assert_eq!(soft_clip(0.5), 0.5);
        assert_eq!(soft_clip(-0.9), -0.9);
        assert!(soft_clip(0.95) > 0.9 && soft_clip(0.95) < 0.95);
        assert!(soft_clip(5.0) <= 1.0);
        assert!(soft_clip(-5.0) >= -1.0);
    }
}

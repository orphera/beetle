//! Procedurally synthesized UI sound effects.
//!
//! Every UI sound is generated at load time from sines, harmonic partials,
//! exponential envelopes and a tiny xorshift noise source, so no audio files
//! ship with the binary. Output is interleaved stereo like every other
//! [`PcmBuffer`] (mono is duplicated to both channels). The mixer resamples
//! voices whose rate differs from the device rate, so buffers can be built at
//! the device rate or at any other rate.

use crate::sample::{PcmBuffer, SampleBank};
use beetle_core::WavId;
use std::f64::consts::TAU;

/// Short interface sounds played by the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UiSound {
    Move,
    Confirm,
    Cancel,
    Open,
    Close,
    Toast,
    Tick,
    Stamp,
    Clear,
    Fail,
}

impl UiSound {
    pub const ALL: [UiSound; 10] = [
        UiSound::Move,
        UiSound::Confirm,
        UiSound::Cancel,
        UiSound::Open,
        UiSound::Close,
        UiSound::Toast,
        UiSound::Tick,
        UiSound::Stamp,
        UiSound::Clear,
        UiSound::Fail,
    ];

    /// Stable soundbank id (1..=10) for this sound.
    pub const fn wav_id(self) -> WavId {
        WavId::new(match self {
            UiSound::Move => 1,
            UiSound::Confirm => 2,
            UiSound::Cancel => 3,
            UiSound::Open => 4,
            UiSound::Close => 5,
            UiSound::Toast => 6,
            UiSound::Tick => 7,
            UiSound::Stamp => 8,
            UiSound::Clear => 9,
            UiSound::Fail => 10,
        })
    }

    /// Intended length in seconds.
    pub const fn seconds(self) -> f64 {
        match self {
            UiSound::Move => 0.035,
            UiSound::Confirm => 0.14,
            UiSound::Cancel => 0.12,
            UiSound::Open => 0.12,
            UiSound::Close => 0.10,
            UiSound::Toast => 0.22,
            UiSound::Tick => 0.015,
            UiSound::Stamp => 0.35,
            UiSound::Clear => 0.9,
            UiSound::Fail => 0.8,
        }
    }

    /// Peak level after normalization; kept well below full scale.
    const fn peak(self) -> f32 {
        match self {
            UiSound::Move | UiSound::Tick => 0.5,
            UiSound::Toast | UiSound::Fail => 0.6,
            UiSound::Stamp => 0.8,
            _ => 0.65,
        }
    }
}

/// Deterministic xorshift32 noise source (no `rand` dependency).
struct XorShift32(u32);

impl XorShift32 {
    fn new(seed: u32) -> Self {
        Self(if seed == 0 { 0x2545_F491 } else { seed })
    }

    /// Uniform value in `[-1.0, 1.0]`.
    fn next_unit(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Adds one sine note with optional harmonic partials to `buf`.
///
/// The pitch glides linearly from `f0` to `f1` over `len` seconds. Each
/// partial is `(ratio, amplitude)` relative to the fundamental phase. The
/// envelope is a 2 ms attack followed by an exponential decay with time
/// constant `tau`.
#[allow(clippy::too_many_arguments)]
fn note(
    buf: &mut [f32],
    sr: f64,
    start: f64,
    len: f64,
    (f0, f1): (f64, f64),
    amp: f64,
    tau: f64,
    partials: &[(f64, f64)],
) {
    let first = (start * sr) as usize;
    let count = ((len * sr) as usize).max(1);
    let mut phase = 0.0f64;
    for i in 0..count {
        let idx = first + i;
        if idx >= buf.len() {
            break;
        }
        let t = i as f64 / sr;
        let k = i as f64 / count as f64;
        phase += TAU * (f0 + (f1 - f0) * k) / sr;
        let env = (t / 0.002).min(1.0) * (-t / tau).exp();
        let mut s = phase.sin();
        for &(ratio, a) in partials {
            s += a * (phase * ratio).sin();
        }
        buf[idx] += (amp * env * s) as f32;
    }
}

/// Adds a short burst of white noise with a 1 ms attack and decay `tau`.
fn noise(buf: &mut [f32], sr: f64, start: f64, len: f64, amp: f64, tau: f64, rng: &mut XorShift32) {
    let first = (start * sr) as usize;
    let count = (len * sr) as usize;
    for i in 0..count {
        let idx = first + i;
        if idx >= buf.len() {
            break;
        }
        let t = i as f64 / sr;
        let env = (t / 0.001).min(1.0) * (-t / tau).exp();
        buf[idx] += (amp * env) as f32 * rng.next_unit();
    }
}

fn render(sound: UiSound, buf: &mut [f32], sr: f64, rng: &mut XorShift32) {
    match sound {
        UiSound::Move => note(buf, sr, 0.0, 0.035, (1200.0, 1200.0), 0.6, 0.006, &[]),
        UiSound::Confirm => {
            note(
                buf,
                sr,
                0.0,
                0.07,
                (880.0, 880.0),
                0.5,
                0.025,
                &[(2.0, 0.25)],
            );
            note(
                buf,
                sr,
                0.05,
                0.09,
                (1320.0, 1320.0),
                0.5,
                0.04,
                &[(2.0, 0.25)],
            );
        }
        UiSound::Cancel => {
            note(buf, sr, 0.0, 0.06, (698.46, 698.46), 0.5, 0.025, &[]);
            note(buf, sr, 0.05, 0.07, (466.16, 466.16), 0.5, 0.03, &[]);
        }
        UiSound::Open => note(buf, sr, 0.0, 0.12, (400.0, 900.0), 0.45, 0.05, &[]),
        UiSound::Close => note(buf, sr, 0.0, 0.10, (900.0, 420.0), 0.4, 0.04, &[]),
        UiSound::Toast => {
            // Bell: fundamental plus a 1.5x partial, the 2x partial slightly quieter.
            note(
                buf,
                sr,
                0.0,
                0.22,
                (880.0, 880.0),
                0.5,
                0.09,
                &[(1.5, 0.35), (2.0, 0.1)],
            );
        }
        UiSound::Tick => {
            note(buf, sr, 0.0, 0.015, (1800.0, 1800.0), 0.35, 0.003, &[]);
            noise(buf, sr, 0.0, 0.015, 0.5, 0.003, rng);
        }
        UiSound::Stamp => {
            // Low thump with a falling pitch, a noise transient and a bright ring.
            note(buf, sr, 0.0, 0.12, (120.0, 45.0), 0.9, 0.04, &[]);
            noise(buf, sr, 0.0, 0.03, 0.45, 0.008, rng);
            note(
                buf,
                sr,
                0.02,
                0.33,
                (1760.0, 1760.0),
                0.3,
                0.08,
                &[(2.4, 0.3)],
            );
        }
        UiSound::Clear => {
            // Rising C major arpeggio, then a held C6-E6-G6 chord.
            note(buf, sr, 0.0, 0.14, (523.25, 523.25), 0.35, 0.08, &[]);
            note(buf, sr, 0.12, 0.14, (659.25, 659.25), 0.35, 0.08, &[]);
            note(buf, sr, 0.24, 0.14, (783.99, 783.99), 0.35, 0.08, &[]);
            note(buf, sr, 0.36, 0.54, (1046.5, 1046.5), 0.2, 0.25, &[]);
            note(buf, sr, 0.36, 0.54, (1318.5, 1318.5), 0.2, 0.25, &[]);
            note(buf, sr, 0.36, 0.54, (1567.98, 1567.98), 0.2, 0.25, &[]);
        }
        UiSound::Fail => {
            // Descending minor sag: A3, F3, D3 with a slight downward glide.
            note(buf, sr, 0.0, 0.28, (220.0, 200.0), 0.45, 0.15, &[]);
            note(buf, sr, 0.22, 0.28, (174.6, 160.0), 0.45, 0.15, &[]);
            note(buf, sr, 0.44, 0.36, (146.8, 130.0), 0.45, 0.2, &[]);
        }
    }
}

/// Applies a 3 ms fade-out so the sound ends at silence, then normalizes the
/// peak to `peak`.
fn finish(buf: &mut [f32], sr: f64, peak: f32) {
    let len = buf.len();
    let fade = ((0.003 * sr) as usize).min(len);
    for i in 0..fade {
        buf[len - fade + i] *= 1.0 - (i + 1) as f32 / fade as f32;
    }
    let max = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if max > 0.0 {
        let gain = peak / max;
        for s in buf.iter_mut() {
            *s *= gain;
        }
    }
}

/// Synthesizes one UI sound as interleaved stereo PCM at `sample_rate`.
///
/// Pure and deterministic: the same arguments always produce identical samples.
pub fn synthesize(sound: UiSound, sample_rate: u32) -> PcmBuffer {
    let sample_rate = sample_rate.max(1);
    let sr = f64::from(sample_rate);
    let len = (sound.seconds() * sr).round() as usize;
    let mut mono = vec![0.0f32; len];
    let mut rng = XorShift32::new(0x9E37_79B9 ^ u32::from(sound.wav_id().as_u16()));
    render(sound, &mut mono, sr, &mut rng);
    finish(&mut mono, sr, sound.peak());

    let mut stereo = Vec::with_capacity(len * 2);
    for s in mono {
        stereo.push(s);
        stereo.push(s);
    }
    PcmBuffer::new(sample_rate, stereo)
}

/// Builds a soundbank containing every [`UiSound`] under its [`UiSound::wav_id`].
pub fn ui_sound_bank(sample_rate: u32) -> SampleBank {
    let mut bank = SampleBank::new();
    for sound in UiSound::ALL {
        bank.insert(sound.wav_id(), synthesize(sound, sample_rate));
    }
    bank
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sound_is_nonempty_in_range_and_finite() {
        for sound in UiSound::ALL {
            for sr in [44_100u32, 48_000] {
                let buf = synthesize(sound, sr);
                assert!(!buf.samples.is_empty(), "{sound:?} is empty");
                assert_eq!(buf.samples.len(), buf.frame_count() * 2);
                assert_eq!(buf.sample_rate, sr);
                let want = sound.seconds();
                assert!(
                    (buf.duration_seconds() - want).abs() <= 1.0 / f64::from(sr),
                    "{sound:?} duration {} not near {want}",
                    buf.duration_seconds()
                );
                assert!(
                    buf.samples.iter().all(|s| s.is_finite()),
                    "{sound:?} has NaN/inf"
                );
                let peak = buf.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
                assert!(peak > 0.05 && peak <= 0.95, "{sound:?} peak {peak}");
            }
        }
    }

    #[test]
    fn every_sound_is_stereo_centered_and_ends_in_silence() {
        for sound in UiSound::ALL {
            let buf = synthesize(sound, 44_100);
            for frame in buf.samples.chunks_exact(2) {
                assert_eq!(frame[0], frame[1], "{sound:?} channels differ");
            }
            let n = buf.samples.len();
            assert!(
                buf.samples[n - 16..].iter().all(|s| s.abs() < 0.01),
                "{sound:?} does not end near silence"
            );
        }
    }

    #[test]
    fn synthesis_is_deterministic() {
        for sound in UiSound::ALL {
            let a = synthesize(sound, 44_100);
            let b = synthesize(sound, 44_100);
            assert_eq!(a.samples.as_ref(), b.samples.as_ref(), "{sound:?}");
        }
    }

    #[test]
    fn wav_ids_are_distinct_and_one_to_ten() {
        let mut ids: Vec<u16> = UiSound::ALL.iter().map(|s| s.wav_id().as_u16()).collect();
        ids.sort_unstable();
        assert_eq!(ids, (1..=10).collect::<Vec<u16>>());
    }

    #[test]
    fn ui_sound_bank_has_all_ten_ids() {
        let bank = ui_sound_bank(44_100);
        assert_eq!(bank.len(), 10);
        for sound in UiSound::ALL {
            assert!(bank.contains_key(sound.wav_id()), "{sound:?} missing");
            let pcm = bank.get(sound.wav_id()).unwrap();
            assert_eq!(
                pcm.samples.as_ref(),
                synthesize(sound, 44_100).samples.as_ref()
            );
        }
    }
}

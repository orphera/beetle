use std::sync::atomic::{fence, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// High-water positions are kept in 1/256 frame steps under a 16-bit reset
/// generation, so a reader that saw the clock before a reset cannot hold the
/// clock back after it.
const POS_BITS: u32 = 48;
const POS_MASK: u64 = (1 << POS_BITS) - 1;
const GEN_MASK: u64 = 0xFFFF;
const POS_SCALE: f64 = 256.0;

/// What the mixer last told the readers: how many frames it has handed to the
/// device, when it did so, and how far the clock may run on before the next
/// callback. Written by the audio callback only, under a sequence lock.
#[derive(Debug)]
pub(crate) struct ClockState {
    seq: AtomicU64,
    generation: AtomicU64,
    rendered: AtomicU64,
    anchor_ns: AtomicU64,
    span: AtomicU64,
    high_water: AtomicU64,
    epoch: Instant,
}

#[derive(Debug, Clone, Copy)]
struct Snapshot {
    generation: u64,
    rendered: u64,
    anchor_ns: i64,
    span: u64,
}

impl ClockState {
    fn new() -> Self {
        Self {
            seq: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            rendered: AtomicU64::new(0),
            anchor_ns: AtomicU64::new(0),
            span: AtomicU64::new(0),
            high_water: AtomicU64::new(0),
            epoch: Instant::now(),
        }
    }

    /// Signed, so a moment before the clock was made still reads correctly.
    fn nanos_since_epoch(&self, t: Instant) -> i64 {
        match t.checked_duration_since(self.epoch) {
            Some(d) => d.as_nanos() as i64,
            None => -(self.epoch.duration_since(t).as_nanos() as i64),
        }
    }

    /// Writer side (the audio callback only). Lock-free and allocation-free.
    fn publish(&self, generation: u64, rendered: u64, anchor: Instant, span: u64) {
        let seq = self.seq.load(Ordering::Relaxed);
        self.seq.store(seq.wrapping_add(1), Ordering::Relaxed);
        fence(Ordering::Release);
        self.generation.store(generation, Ordering::Relaxed);
        self.rendered.store(rendered, Ordering::Relaxed);
        self.anchor_ns
            .store(self.nanos_since_epoch(anchor) as u64, Ordering::Relaxed);
        self.span.store(span, Ordering::Relaxed);
        self.seq.store(seq.wrapping_add(2), Ordering::Release);
    }

    fn snapshot(&self) -> Snapshot {
        loop {
            let before = self.seq.load(Ordering::Acquire);
            if before & 1 == 1 {
                std::hint::spin_loop();
                continue;
            }
            let snap = Snapshot {
                generation: self.generation.load(Ordering::Relaxed),
                rendered: self.rendered.load(Ordering::Relaxed),
                anchor_ns: self.anchor_ns.load(Ordering::Relaxed) as i64,
                span: self.span.load(Ordering::Relaxed),
            };
            fence(Ordering::Acquire);
            if self.seq.load(Ordering::Relaxed) == before {
                return snap;
            }
        }
    }
}

/// Master audio clock tracking the sample frames rendered by the audio device.
/// All judgment and visual note positions are derived from this clock.
///
/// The mixer only reports in whole buffers, so between callbacks the clock
/// runs on at the sample rate from the last callback's time, never further
/// than that callback's buffer, which keeps it from stepping in buffer-sized
/// jumps.
#[derive(Debug, Clone)]
pub struct AudioClock {
    state: Arc<ClockState>,
    sample_rate: u32,
}

impl AudioClock {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            state: Arc::new(ClockState::new()),
            sample_rate,
        }
    }

    /// The playback position at `t` in frames, with no monotonic guard: `t`
    /// may lie a little in the past (an input event stamped when it arrived).
    fn position_at(&self, snap: Snapshot, t: Instant) -> f64 {
        let rendered = snap.rendered as f64;
        // Paused (or not started): the clock stands still.
        if snap.span == 0 || self.sample_rate == 0 {
            return rendered;
        }
        let elapsed = (self.state.nanos_since_epoch(t) - snap.anchor_ns) as f64 / 1e9;
        let pos = rendered + elapsed * self.sample_rate as f64;
        pos.min(rendered + snap.span as f64).max(0.0)
    }

    /// The position now, never below one handed out before (since the last
    /// clock reset).
    fn position_now(&self) -> f64 {
        let snap = self.state.snapshot();
        let pos = self.position_at(snap, Instant::now());
        let generation = snap.generation & GEN_MASK;
        let fixed = ((pos * POS_SCALE) as u64).min(POS_MASK);
        let prev = self
            .state
            .high_water
            .fetch_max((generation << POS_BITS) | fixed, Ordering::AcqRel);
        if prev >> POS_BITS == generation {
            pos.max((prev & POS_MASK) as f64 / POS_SCALE)
        } else {
            pos
        }
    }

    /// Audio frames played since playback started.
    pub fn current_samples(&self) -> u64 {
        self.position_now() as u64
    }

    /// Frames the mixer has handed to the device so far, without the
    /// between-callback interpolation.
    pub fn rendered_samples(&self) -> u64 {
        self.state.snapshot().rendered
    }

    /// Current audio playback time in seconds.
    pub fn current_time_seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.position_now() / self.sample_rate as f64
        }
    }

    /// The audio playback time at `t` in seconds. `t` may be a moment shortly
    /// in the past, such as when an input event arrived; unlike
    /// [`Self::current_time_seconds`] this is not held monotonic.
    pub fn time_at(&self, t: Instant) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.position_at(self.state.snapshot(), t) / self.sample_rate as f64
        }
    }

    /// Returns the audio playback time with a user-configured offset (e.g. visual/audio latency calibration).
    pub fn current_compensated_time_seconds(&self, offset_seconds: f64) -> f64 {
        (self.current_time_seconds() + offset_seconds).max(0.0)
    }

    /// Configured audio output sample rate in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Mixer side: `frames` more were handed to the device in the callback
    /// that ran at `at` (none while paused). Called from the audio callback only.
    pub(crate) fn advance(&self, frames: u64, at: Instant) {
        let snap = self.state.snapshot();
        self.state
            .publish(snap.generation, snap.rendered + frames, at, frames);
    }

    /// Mixer side: back to zero. Called from the audio callback only.
    pub(crate) fn reset(&self, at: Instant) {
        let snap = self.state.snapshot();
        self.state
            .publish(snap.generation.wrapping_add(1), 0, at, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const RATE: u32 = 48000;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn stands_at_zero_until_the_first_callback() {
        let clock = AudioClock::new(RATE);
        assert_eq!(clock.current_samples(), 0);
        assert_eq!(clock.current_time_seconds(), 0.0);
        assert_eq!(clock.time_at(Instant::now() + ms(50)), 0.0);
    }

    #[test]
    fn runs_on_between_callbacks_up_to_one_buffer() {
        let clock = AudioClock::new(RATE);
        let t0 = Instant::now();
        // 24000 frames out, then a 480-frame (10 ms) buffer at t0.
        clock.advance(24000, t0 - ms(500));
        clock.advance(480, t0);
        assert_eq!(clock.rendered_samples(), 24480);

        // 5 ms on: half a buffer further.
        let at5 = clock.time_at(t0 + ms(5));
        assert!((at5 - (24480.0 + 240.0) / RATE as f64).abs() < 1e-9);
        // 30 ms on with no callback: held at one buffer past the last one.
        let at30 = clock.time_at(t0 + ms(30));
        assert!((at30 - (24480.0 + 480.0) / RATE as f64).abs() < 1e-9);
    }

    #[test]
    fn a_moment_before_the_last_callback_reads_back_in_time() {
        let clock = AudioClock::new(RATE);
        let t0 = Instant::now() + ms(100);
        clock.advance(24000, t0 - ms(10));
        clock.advance(480, t0);
        // An event stamped 2 ms before the callback that just ran.
        let t = clock.time_at(t0 - ms(2));
        assert!((t - (24480.0 - 96.0) / RATE as f64).abs() < 1e-9);
    }

    #[test]
    fn stands_still_while_paused() {
        let clock = AudioClock::new(RATE);
        let t0 = Instant::now();
        clock.advance(4800, t0);
        clock.advance(0, t0 + ms(10));
        assert_eq!(clock.time_at(t0 + ms(500)), 0.1);
    }

    #[test]
    fn now_never_goes_back() {
        let clock = AudioClock::new(RATE);
        // A buffer that was due long ago: the clock has run to its end.
        clock.advance(480, Instant::now() - ms(100));
        assert_eq!(clock.current_samples(), 960);
        // Then a pause publishes the same frame count with no run-on: the
        // clock holds where it was seen rather than dropping back to 480.
        clock.advance(0, Instant::now());
        assert_eq!(clock.current_samples(), 960);
        assert_eq!(clock.rendered_samples(), 480);
    }

    #[test]
    fn reset_starts_over_from_zero() {
        let clock = AudioClock::new(RATE);
        clock.advance(48000, Instant::now() - ms(100));
        assert!(clock.current_time_seconds() >= 1.0);

        clock.reset(Instant::now());
        assert_eq!(clock.current_samples(), 0);
        assert_eq!(clock.current_time_seconds(), 0.0);

        clock.advance(4800, Instant::now() - ms(100));
        assert_eq!(clock.current_samples(), 9600);
    }

    #[test]
    fn compensated_time_adds_the_offset() {
        let clock = AudioClock::new(RATE);
        clock.advance(24000, Instant::now());
        clock.advance(0, Instant::now());
        assert_eq!(clock.current_compensated_time_seconds(0.05), 0.55);
        assert_eq!(clock.current_compensated_time_seconds(-1.0), 0.0);
    }

    #[test]
    fn readers_on_other_threads_see_whole_snapshots() {
        let clock = AudioClock::new(RATE);
        let reader = clock.clone();
        let handle = std::thread::spawn(move || {
            let mut last = 0.0;
            for _ in 0..20000 {
                let now = reader.current_time_seconds();
                assert!(now >= last, "clock went back: {now} < {last}");
                last = now;
            }
        });
        for _ in 0..20000 {
            clock.advance(480, Instant::now());
        }
        handle.join().unwrap();
    }
}

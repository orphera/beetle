use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Master audio clock tracking the actual number of sample frames rendered by the audio device.
/// All judgment and visual note positions are derived from this clock.
#[derive(Debug, Clone)]
pub struct AudioClock {
    samples_played: Arc<AtomicU64>,
    sample_rate: u32,
}

impl AudioClock {
    pub fn new(samples_played: Arc<AtomicU64>, sample_rate: u32) -> Self {
        Self {
            samples_played,
            sample_rate,
        }
    }

    /// Total audio frames/samples played since playback started.
    pub fn current_samples(&self) -> u64 {
        self.samples_played.load(Ordering::Relaxed)
    }

    /// Current audio playback time in seconds.
    pub fn current_time_seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.current_samples() as f64 / self.sample_rate as f64
        }
    }

    /// Configured audio output sample rate in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

use beetle_core::WavId;

/// Lock-free commands sent from the logic thread to the audio callback thread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioCommand {
    /// Trigger playback of a preloaded sample.
    PlaySample {
        sample_id: WavId,
        volume: f32,
        pan: f32,
    },
    /// Start a preloaded sample at an exact frame of the audio clock
    /// (`AudioClock::current_samples`). A frame already past plays at once.
    PlaySampleAt {
        sample_id: WavId,
        start_sample: u64,
        volume: f32,
        pan: f32,
    },
    /// Stop all active voices (and pending starts) for a specific sample.
    StopSample { sample_id: WavId },
    /// Stop all currently playing voices and drop pending starts.
    StopAll,
    /// Pause audio playback and clock advancement.
    Pause,
    /// Resume audio playback and clock advancement.
    Resume,
    /// Set master volume multiplier (0.0 ~ 1.0).
    SetMasterVolume(f32),
    /// Reset the audio clock counter to zero.
    ResetClock,
}

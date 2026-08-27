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
    /// Stop all active voices for a specific sample.
    StopSample {
        sample_id: WavId,
    },
    /// Set master volume multiplier (0.0 ~ 1.0).
    SetMasterVolume(f32),
    /// Reset the audio clock counter to zero.
    ResetClock,
}

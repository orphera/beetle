use beetle_core::WavId;

/// A mixer bus: the gain group a voice belongs to, set apart from the master
/// volume. A start on the chart's clock (`PlaySampleAt`) is BGM; a start
/// from the player's keys (`PlaySample`) is a key sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bus {
    Bgm = 0,
    Key = 1,
}

/// Number of buses (the size of the mixer's gain table).
pub const BUS_COUNT: usize = 2;

/// Lock-free commands sent from the logic thread to the audio callback thread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioCommand {
    /// Trigger playback of a preloaded sample (key bus).
    PlaySample {
        sample_id: WavId,
        volume: f32,
        pan: f32,
    },
    /// Start a preloaded sample at an exact frame of the audio clock
    /// (`AudioClock::current_samples`). A frame already past plays at once.
    /// Chart sounds, so the BGM bus.
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
    /// Set master volume multiplier (0.0 ~ 2.0).
    SetMasterVolume(f32),
    /// Set one bus's gain (0.0 ~ 1.0), applied on top of the master volume.
    SetBusVolume { bus: Bus, volume: f32 },
    /// Reset the audio clock counter to zero.
    ResetClock,
}

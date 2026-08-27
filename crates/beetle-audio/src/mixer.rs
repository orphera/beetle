use crate::command::AudioCommand;
use crate::sample::SampleBank;
use beetle_core::WavId;
use rtrb::Consumer;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub const MAX_VOICES: usize = 128;

/// A single active voice being mixed.
#[derive(Debug, Clone, Copy)]
pub struct ActiveVoice {
    pub sample_id: WavId,
    pub cursor: usize,
    pub volume_left: f32,
    pub volume_right: f32,
    pub is_active: bool,
}

impl Default for ActiveVoice {
    fn default() -> Self {
        Self {
            sample_id: WavId(0),
            cursor: 0,
            volume_left: 1.0,
            volume_right: 1.0,
            is_active: false,
        }
    }
}

/// Realtime audio mixer running exclusively inside the audio callback thread.
/// Guaranteed zero heap allocation and zero locking.
pub struct Mixer {
    voices: [ActiveVoice; MAX_VOICES],
    sample_bank: SampleBank,
    command_rx: Consumer<AudioCommand>,
    samples_played: Arc<AtomicU64>,
    master_volume: f32,
}

impl Mixer {
    pub fn new(
        sample_bank: SampleBank,
        command_rx: Consumer<AudioCommand>,
        samples_played: Arc<AtomicU64>,
    ) -> Self {
        Self {
            voices: [ActiveVoice::default(); MAX_VOICES],
            sample_bank,
            command_rx,
            samples_played,
            master_volume: 1.0,
        }
    }

    /// Process incoming commands and mix audio samples into the output buffer.
    pub fn process_buffer(&mut self, output: &mut [f32]) {
        // 1. Drain lock-free commands
        while let Ok(cmd) = self.command_rx.pop() {
            match cmd {
                AudioCommand::PlaySample { sample_id, volume, pan } => {
                    self.spawn_voice(sample_id, volume, pan);
                }
                AudioCommand::StopSample { sample_id } => {
                    self.kill_voice(sample_id);
                }
                AudioCommand::SetMasterVolume(vol) => {
                    self.master_volume = vol;
                }
                AudioCommand::ResetClock => {
                    self.samples_played.store(0, Ordering::Relaxed);
                }
            }
        }

        // 2. Clear output buffer
        output.fill(0.0);

        // 3. Mix active voices (skeleton - detailed mixing logic in Phase 2)
        // Note: channels are assumed stereo (2)
        let frame_count = output.len() / 2;
        self.samples_played.fetch_add(frame_count as u64, Ordering::Relaxed);
    }

    fn spawn_voice(&mut self, sample_id: WavId, volume: f32, pan: f32) {
        if self.sample_bank.get(sample_id).is_none() {
            return;
        }
        let pan_clamped = pan.clamp(-1.0, 1.0);
        let vol_l = volume * (1.0 - pan_clamped.max(0.0));
        let vol_r = volume * (1.0 + pan_clamped.min(0.0));

        // Find inactive voice or oldest voice
        for voice in &mut self.voices {
            if !voice.is_active {
                *voice = ActiveVoice {
                    sample_id,
                    cursor: 0,
                    volume_left: vol_l,
                    volume_right: vol_r,
                    is_active: true,
                };
                return;
            }
        }
    }

    fn kill_voice(&mut self, sample_id: WavId) {
        for voice in &mut self.voices {
            if voice.is_active && voice.sample_id == sample_id {
                voice.is_active = false;
            }
        }
    }
}

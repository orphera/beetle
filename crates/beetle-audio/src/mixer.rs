use crate::command::AudioCommand;
use crate::sample::SampleBank;
use beetle_core::WavId;
use rtrb::Consumer;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

pub const MAX_VOICES: usize = 128;

/// A single active voice being mixed.
#[derive(Debug, Clone, Copy)]
pub struct ActiveVoice {
    pub sample_id: WavId,
    pub cursor: f64,
    pub volume_left: f32,
    pub volume_right: f32,
    pub is_active: bool,
}

impl Default for ActiveVoice {
    fn default() -> Self {
        Self {
            sample_id: WavId(0),
            cursor: 0.0,
            volume_left: 1.0,
            volume_right: 1.0,
            is_active: false,
        }
    }
}

/// Realtime audio mixer running exclusively inside the audio callback thread.
/// Guaranteed zero heap allocation and zero blocking synchronization.
pub struct Mixer {
    voices: [ActiveVoice; MAX_VOICES],
    sample_bank: SampleBank,
    command_rx: Consumer<AudioCommand>,
    samples_played: Arc<AtomicU64>,
    visual_levels: Arc<[AtomicU32; 16]>,
    output_sample_rate: u32,
    master_volume: f32,
}

impl Mixer {
    pub fn new(
        sample_bank: SampleBank,
        command_rx: Consumer<AudioCommand>,
        samples_played: Arc<AtomicU64>,
        visual_levels: Arc<[AtomicU32; 16]>,
        output_sample_rate: u32,
    ) -> Self {
        Self {
            voices: [ActiveVoice::default(); MAX_VOICES],
            sample_bank,
            command_rx,
            samples_played,
            visual_levels,
            output_sample_rate: output_sample_rate.max(1),
            master_volume: 1.0,
        }
    }

    /// Process incoming commands and mix audio samples into the interleaved stereo output buffer.
    pub fn process_buffer(&mut self, output: &mut [f32]) {
        // 1. Drain lock-free commands
        while let Ok(cmd) = self.command_rx.pop() {
            match cmd {
                AudioCommand::PlaySample {
                    sample_id,
                    volume,
                    pan,
                } => {
                    self.spawn_voice(sample_id, volume, pan);
                }
                AudioCommand::StopSample { sample_id } => {
                    self.kill_voice(sample_id);
                }
                AudioCommand::SetMasterVolume(vol) => {
                    self.master_volume = vol.clamp(0.0, 2.0);
                }
                AudioCommand::ResetClock => {
                    self.samples_played.store(0, Ordering::Relaxed);
                }
            }
        }

        // 2. Clear output buffer
        output.fill(0.0);

        let frame_count = output.len() / 2;
        if frame_count == 0 {
            return;
        }

        let out_sr = self.output_sample_rate as f64;

        // 3. Mix all active voices with linear interpolation
        for voice in &mut self.voices {
            if !voice.is_active {
                continue;
            }

            let Some(pcm) = self.sample_bank.get(voice.sample_id) else {
                voice.is_active = false;
                continue;
            };

            let total_pcm_frames = pcm.frame_count();
            if total_pcm_frames == 0 {
                voice.is_active = false;
                continue;
            }

            let step = pcm.sample_rate as f64 / out_sr;
            let vol_l = voice.volume_left * self.master_volume;
            let vol_r = voice.volume_right * self.master_volume;

            for i in 0..frame_count {
                let frame_idx = voice.cursor;
                let f0 = frame_idx as usize;

                if f0 >= total_pcm_frames {
                    voice.is_active = false;
                    break;
                }

                let alpha = (frame_idx - f0 as f64) as f32;
                let f1 = (f0 + 1).min(total_pcm_frames - 1);

                let l0 = pcm.samples[f0 * 2];
                let r0 = pcm.samples[f0 * 2 + 1];
                let l1 = pcm.samples[f1 * 2];
                let r1 = pcm.samples[f1 * 2 + 1];

                let sample_l = l0 + alpha * (l1 - l0);
                let sample_r = r0 + alpha * (r1 - r0);

                output[i * 2] += sample_l * vol_l;
                output[i * 2 + 1] += sample_r * vol_r;

                voice.cursor += step;
                if voice.cursor >= total_pcm_frames as f64 {
                    voice.is_active = false;
                    break;
                }
            }
        }

        // 4. Soft limiter / clamp
        for s in output.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }

        // 5. Update visualizer snapshot (16 bands)
        let chunk = (output.len() / 16).max(1);
        for (i, slot) in self.visual_levels.iter().enumerate() {
            let start = i * chunk;
            if start >= output.len() {
                slot.store(0, Ordering::Relaxed);
                continue;
            }
            let end = (start + chunk).min(output.len());
            let mut peak: f32 = 0.0;
            for &sample in &output[start..end] {
                let a = sample.abs();
                if a > peak {
                    peak = a;
                }
            }
            slot.store((peak * 1000.0) as u32, Ordering::Relaxed);
        }

        // 6. Update master audio clock
        self.samples_played.fetch_add(frame_count as u64, Ordering::Relaxed);
    }

    fn spawn_voice(&mut self, sample_id: WavId, volume: f32, pan: f32) {
        if self.sample_bank.get(sample_id).is_none() {
            return;
        }

        let pan_clamped = pan.clamp(-1.0, 1.0);
        let vol_l = volume * (1.0 - pan_clamped.max(0.0));
        let vol_r = volume * (1.0 + pan_clamped.min(0.0));

        // 1. Look for inactive voice slot
        for voice in &mut self.voices {
            if !voice.is_active {
                *voice = ActiveVoice {
                    sample_id,
                    cursor: 0.0,
                    volume_left: vol_l,
                    volume_right: vol_r,
                    is_active: true,
                };
                return;
            }
        }

        // 2. Voice stealing: overwrite oldest voice with largest cursor progress
        let mut max_cursor = -1.0;
        let mut steal_idx = 0;
        for (i, voice) in self.voices.iter().enumerate() {
            if voice.cursor > max_cursor {
                max_cursor = voice.cursor;
                steal_idx = i;
            }
        }

        self.voices[steal_idx] = ActiveVoice {
            sample_id,
            cursor: 0.0,
            volume_left: vol_l,
            volume_right: vol_r,
            is_active: true,
        };
    }

    fn kill_voice(&mut self, sample_id: WavId) {
        for voice in &mut self.voices {
            if voice.is_active && voice.sample_id == sample_id {
                voice.is_active = false;
            }
        }
    }

    /// Number of active voices currently playing.
    pub fn active_voice_count(&self) -> usize {
        self.voices.iter().filter(|v| v.is_active).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample::PcmBuffer;
    use rtrb::RingBuffer;

    fn make_visual_levels() -> Arc<[AtomicU32; 16]> {
        Arc::new(std::array::from_fn(|_| AtomicU32::new(0)))
    }

    #[test]
    fn test_mixer_playback_and_panning() {
        let mut sample_bank = SampleBank::new();
        // 4 stereo frames of constant DC value 0.5
        let pcm = PcmBuffer::new(44100, vec![0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5]);
        sample_bank.insert(WavId(1), pcm);

        let (mut producer, consumer) = RingBuffer::new(32);
        let samples_played = Arc::new(AtomicU64::new(0));
        let visual_levels = make_visual_levels();
        let mut mixer = Mixer::new(
            sample_bank,
            consumer,
            Arc::clone(&samples_played),
            visual_levels,
            44100,
        );

        // Pan center
        producer
            .push(AudioCommand::PlaySample {
                sample_id: WavId(1),
                volume: 1.0,
                pan: 0.0,
            })
            .unwrap();

        let mut output = [0.0f32; 4]; // 2 stereo frames
        mixer.process_buffer(&mut output);

        assert_eq!(samples_played.load(Ordering::Relaxed), 2);
        assert!((output[0] - 0.5).abs() < 0.001);
        assert!((output[1] - 0.5).abs() < 0.001);
        assert!((output[2] - 0.5).abs() < 0.001);
        assert!((output[3] - 0.5).abs() < 0.001);

        // Next buffer: voice ends after remaining 2 frames
        let mut output2 = [0.0f32; 4];
        mixer.process_buffer(&mut output2);
        assert_eq!(samples_played.load(Ordering::Relaxed), 4);
        assert_eq!(mixer.active_voice_count(), 0);
    }

    #[test]
    fn test_mixer_stop_sample() {
        let mut sample_bank = SampleBank::new();
        let pcm = PcmBuffer::new(44100, vec![0.8; 1000]);
        sample_bank.insert(WavId(1), pcm);

        let (mut producer, consumer) = RingBuffer::new(32);
        let samples_played = Arc::new(AtomicU64::new(0));
        let visual_levels = make_visual_levels();
        let mut mixer = Mixer::new(
            sample_bank,
            consumer,
            samples_played,
            visual_levels,
            44100,
        );

        producer
            .push(AudioCommand::PlaySample {
                sample_id: WavId(1),
                volume: 1.0,
                pan: 0.0,
            })
            .unwrap();

        let mut output = [0.0f32; 10];
        mixer.process_buffer(&mut output);
        assert_eq!(mixer.active_voice_count(), 1);

        producer
            .push(AudioCommand::StopSample {
                sample_id: WavId(1),
            })
            .unwrap();

        mixer.process_buffer(&mut output);
        assert_eq!(mixer.active_voice_count(), 0);
    }
}

use crate::clock::AudioClock;
use crate::command::AudioCommand;
use crate::sample::SampleBank;
use beetle_core::WavId;
use rtrb::Consumer;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub const MAX_VOICES: usize = 128;
/// Starts waiting for their frame (`AudioCommand::PlaySampleAt`). When all
/// are taken, a further one starts at once rather than being lost.
pub const MAX_SCHEDULED: usize = 256;

/// A single active voice being mixed.
#[derive(Debug, Clone, Copy)]
pub struct ActiveVoice {
    pub sample_id: WavId,
    pub cursor: f64,
    pub volume_left: f32,
    pub volume_right: f32,
    pub is_active: bool,
    /// Silent frames left before the voice starts (a start inside a buffer).
    pub delay: u32,
}

impl Default for ActiveVoice {
    fn default() -> Self {
        Self {
            sample_id: WavId(0),
            cursor: 0.0,
            volume_left: 1.0,
            volume_right: 1.0,
            is_active: false,
            delay: 0,
        }
    }
}

/// A start waiting for its frame.
#[derive(Debug, Clone, Copy)]
struct ScheduledStart {
    sample_id: WavId,
    start_sample: u64,
    volume: f32,
    pan: f32,
    is_set: bool,
}

impl Default for ScheduledStart {
    fn default() -> Self {
        Self {
            sample_id: WavId(0),
            start_sample: 0,
            volume: 1.0,
            pan: 0.0,
            is_set: false,
        }
    }
}

/// Realtime audio mixer running exclusively inside the audio callback thread.
/// Guaranteed zero heap allocation and zero blocking synchronization.
pub struct Mixer {
    voices: [ActiveVoice; MAX_VOICES],
    scheduled: [ScheduledStart; MAX_SCHEDULED],
    sample_bank: SampleBank,
    command_rx: Consumer<AudioCommand>,
    clock: AudioClock,
    visual_levels: Arc<[AtomicU32; 16]>,
    output_sample_rate: u32,
    master_volume: f32,
    is_paused: bool,
}

impl Mixer {
    pub fn new(
        sample_bank: SampleBank,
        command_rx: Consumer<AudioCommand>,
        clock: &AudioClock,
        visual_levels: Arc<[AtomicU32; 16]>,
    ) -> Self {
        Self {
            voices: [ActiveVoice::default(); MAX_VOICES],
            scheduled: [ScheduledStart::default(); MAX_SCHEDULED],
            sample_bank,
            command_rx,
            clock: clock.clone(),
            visual_levels,
            output_sample_rate: clock.sample_rate().max(1),
            master_volume: 1.0,
            is_paused: false,
        }
    }

    /// Process incoming commands and mix audio samples into the interleaved stereo output buffer.
    pub fn process_buffer(&mut self, output: &mut [f32]) {
        // The clock runs on from when the device asked for this buffer.
        let now = Instant::now();

        // 1. Drain lock-free commands
        while let Ok(cmd) = self.command_rx.pop() {
            match cmd {
                AudioCommand::PlaySample {
                    sample_id,
                    volume,
                    pan,
                } => {
                    self.spawn_voice(sample_id, volume, pan, 0);
                }
                AudioCommand::PlaySampleAt {
                    sample_id,
                    start_sample,
                    volume,
                    pan,
                } => {
                    self.schedule(sample_id, start_sample, volume, pan);
                }
                AudioCommand::StopSample { sample_id } => {
                    self.kill_voice(sample_id);
                }
                AudioCommand::StopAll => {
                    for v in &mut self.voices {
                        v.is_active = false;
                    }
                    for s in &mut self.scheduled {
                        s.is_set = false;
                    }
                }
                AudioCommand::Pause => {
                    self.is_paused = true;
                }
                AudioCommand::Resume => {
                    self.is_paused = false;
                }
                AudioCommand::SetMasterVolume(vol) => {
                    self.master_volume = vol.clamp(0.0, 2.0);
                }
                AudioCommand::ResetClock => {
                    // Pending frames counted from the old zero mean nothing now.
                    for s in &mut self.scheduled {
                        s.is_set = false;
                    }
                    self.clock.reset(now);
                }
            }
        }

        // 2. Clear output buffer
        output.fill(0.0);

        if self.is_paused {
            for slot in self.visual_levels.iter() {
                slot.store(0, Ordering::Relaxed);
            }
            self.clock.advance(0, now);
            return;
        }

        let frame_count = output.len() / 2;
        if frame_count == 0 {
            return;
        }

        // 3. Start the scheduled voices whose frame falls in this buffer
        // (or has already passed).
        let first = self.clock.rendered_samples();
        let end = first + frame_count as u64;
        for i in 0..MAX_SCHEDULED {
            let s = self.scheduled[i];
            if s.is_set && s.start_sample < end {
                self.scheduled[i].is_set = false;
                let delay = s.start_sample.saturating_sub(first) as u32;
                self.spawn_voice(s.sample_id, s.volume, s.pan, delay);
            }
        }

        let out_sr = self.output_sample_rate as f64;

        // 4. Mix all active voices with linear interpolation
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

            let start = voice.delay as usize;
            if start >= frame_count {
                voice.delay -= frame_count as u32;
                continue;
            }
            voice.delay = 0;

            let step = pcm.sample_rate as f64 / out_sr;
            let vol_l = voice.volume_left * self.master_volume;
            let vol_r = voice.volume_right * self.master_volume;

            for i in start..frame_count {
                let frame_idx = voice.cursor;
                let f0 = frame_idx as usize;

                if f0 >= total_pcm_frames {
                    voice.is_active = false;
                    break;
                }

                let alpha = (frame_idx - f0 as f64) as f32;
                let f1 = (f0 + 1).min(total_pcm_frames - 1);

                let base = pcm.offset;
                let l0 = pcm.samples[base + f0 * 2];
                let r0 = pcm.samples[base + f0 * 2 + 1];
                let l1 = pcm.samples[base + f1 * 2];
                let r1 = pcm.samples[base + f1 * 2 + 1];

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

        // 5. Soft limiter / clamp
        for s in output.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }

        // 6. Update visualizer snapshot (16 bands)
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

        // 7. Update master audio clock
        self.clock.advance(frame_count as u64, now);
    }

    /// Holds a start for its frame; with every slot taken it starts at once.
    fn schedule(&mut self, sample_id: WavId, start_sample: u64, volume: f32, pan: f32) {
        if self.sample_bank.get(sample_id).is_none() {
            return;
        }
        match self.scheduled.iter_mut().find(|s| !s.is_set) {
            Some(slot) => {
                *slot = ScheduledStart {
                    sample_id,
                    start_sample,
                    volume,
                    pan,
                    is_set: true,
                }
            }
            None => self.spawn_voice(sample_id, volume, pan, 0),
        }
    }

    fn spawn_voice(&mut self, sample_id: WavId, volume: f32, pan: f32, delay: u32) {
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
                    delay,
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
            delay,
        };
    }

    fn kill_voice(&mut self, sample_id: WavId) {
        for voice in &mut self.voices {
            if voice.is_active && voice.sample_id == sample_id {
                voice.is_active = false;
            }
        }
        for s in &mut self.scheduled {
            if s.is_set && s.sample_id == sample_id {
                s.is_set = false;
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
        let clock = AudioClock::new(44100);
        let visual_levels = make_visual_levels();
        let mut mixer = Mixer::new(sample_bank, consumer, &clock, visual_levels);

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

        assert_eq!(clock.rendered_samples(), 2);
        assert!((output[0] - 0.5).abs() < 0.001);
        assert!((output[1] - 0.5).abs() < 0.001);
        assert!((output[2] - 0.5).abs() < 0.001);
        assert!((output[3] - 0.5).abs() < 0.001);

        // Next buffer: voice ends after remaining 2 frames
        let mut output2 = [0.0f32; 4];
        mixer.process_buffer(&mut output2);
        assert_eq!(clock.rendered_samples(), 4);
        assert_eq!(mixer.active_voice_count(), 0);
    }

    #[test]
    fn test_mixer_stop_sample() {
        let mut sample_bank = SampleBank::new();
        let pcm = PcmBuffer::new(44100, vec![0.8; 1000]);
        sample_bank.insert(WavId(1), pcm);

        let (mut producer, consumer) = RingBuffer::new(32);
        let clock = AudioClock::new(44100);
        let visual_levels = make_visual_levels();
        let mut mixer = Mixer::new(sample_bank, consumer, &clock, visual_levels);

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

    #[test]
    fn test_mixer_pause_and_resume() {
        let mut sample_bank = SampleBank::new();
        let pcm = PcmBuffer::new(44100, vec![0.5; 1000]);
        sample_bank.insert(WavId(1), pcm);

        let (mut producer, consumer) = RingBuffer::new(32);
        let clock = AudioClock::new(44100);
        let visual_levels = make_visual_levels();
        let mut mixer = Mixer::new(sample_bank, consumer, &clock, visual_levels);

        producer
            .push(AudioCommand::PlaySample {
                sample_id: WavId(1),
                volume: 1.0,
                pan: 0.0,
            })
            .unwrap();

        let mut output = [0.0f32; 10];
        mixer.process_buffer(&mut output);
        assert_eq!(clock.rendered_samples(), 5);

        // Pause
        producer.push(AudioCommand::Pause).unwrap();
        let mut pause_out = [1.0f32; 10];
        mixer.process_buffer(&mut pause_out);

        // While paused: output is 0.0 and clock does not advance
        assert_eq!(clock.rendered_samples(), 5);
        assert!(pause_out.iter().all(|&s| s == 0.0));

        // Resume
        producer.push(AudioCommand::Resume).unwrap();
        mixer.process_buffer(&mut output);
        assert_eq!(clock.rendered_samples(), 10);
    }

    /// A mixer over one 1.0-valued sample of `frames` frames (id 1).
    fn scheduling_mixer(frames: usize) -> (Mixer, rtrb::Producer<AudioCommand>, AudioClock) {
        let mut sample_bank = SampleBank::new();
        sample_bank.insert(WavId(1), PcmBuffer::new(44100, vec![1.0; frames * 2]));
        let (producer, consumer) = RingBuffer::new(512);
        let clock = AudioClock::new(44100);
        let mixer = Mixer::new(sample_bank, consumer, &clock, make_visual_levels());
        (mixer, producer, clock)
    }

    fn play_at(producer: &mut rtrb::Producer<AudioCommand>, start_sample: u64) {
        producer
            .push(AudioCommand::PlaySampleAt {
                sample_id: WavId(1),
                start_sample,
                volume: 1.0,
                pan: 0.0,
            })
            .unwrap();
    }

    /// Left-channel values of one `frames`-frame buffer.
    fn mix(mixer: &mut Mixer, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0f32; frames * 2];
        mixer.process_buffer(&mut out);
        out.iter().step_by(2).copied().collect()
    }

    #[test]
    fn a_scheduled_start_begins_on_its_frame_inside_the_buffer() {
        let (mut mixer, mut producer, _) = scheduling_mixer(100);
        play_at(&mut producer, 3);
        assert_eq!(mix(&mut mixer, 6), [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn a_scheduled_start_waits_for_a_later_buffer() {
        let (mut mixer, mut producer, _) = scheduling_mixer(100);
        play_at(&mut producer, 10);
        assert_eq!(mix(&mut mixer, 4), [0.0; 4]);
        assert_eq!(mix(&mut mixer, 4), [0.0; 4]);
        assert_eq!(mix(&mut mixer, 4), [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn a_start_already_past_plays_at_once() {
        let (mut mixer, mut producer, clock) = scheduling_mixer(100);
        mix(&mut mixer, 8);
        assert_eq!(clock.rendered_samples(), 8);
        play_at(&mut producer, 2);
        assert_eq!(mix(&mut mixer, 3), [1.0, 1.0, 1.0]);
    }

    #[test]
    fn a_pending_start_waits_out_a_pause() {
        let (mut mixer, mut producer, _) = scheduling_mixer(100);
        play_at(&mut producer, 5);
        producer.push(AudioCommand::Pause).unwrap();
        assert_eq!(mix(&mut mixer, 4), [0.0; 4]);
        assert_eq!(mix(&mut mixer, 4), [0.0; 4]);
        producer.push(AudioCommand::Resume).unwrap();
        // The clock stood still while paused, so frame 5 is still ahead.
        assert_eq!(mix(&mut mixer, 4), [0.0; 4]);
        assert_eq!(mix(&mut mixer, 4), [0.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn stopping_drops_pending_starts() {
        let (mut mixer, mut producer, _) = scheduling_mixer(100);
        play_at(&mut producer, 6);
        producer.push(AudioCommand::StopAll).unwrap();
        mix(&mut mixer, 4);
        play_at(&mut producer, 6);
        producer
            .push(AudioCommand::StopSample {
                sample_id: WavId(1),
            })
            .unwrap();
        assert_eq!(mix(&mut mixer, 4), [0.0; 4]);
        assert_eq!(mix(&mut mixer, 4), [0.0; 4]);
        assert_eq!(mixer.active_voice_count(), 0);
    }

    #[test]
    fn with_every_slot_taken_a_further_start_plays_at_once() {
        let (mut mixer, mut producer, _) = scheduling_mixer(100);
        for _ in 0..MAX_SCHEDULED {
            play_at(&mut producer, 1_000_000);
        }
        play_at(&mut producer, 1_000_000);
        assert_eq!(mix(&mut mixer, 2), [1.0, 1.0]);
        assert_eq!(mixer.active_voice_count(), 1);
    }
}

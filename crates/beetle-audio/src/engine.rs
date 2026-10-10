use crate::clock::AudioClock;
use crate::command::AudioCommand;
use crate::mixer::Mixer;
use crate::sample::SampleBank;
use beetle_core::WavId;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig};
use rtrb::{Producer, RingBuffer};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

pub const COMMAND_QUEUE_CAPACITY: usize = 512;

/// Errors when initializing the audio engine.
#[derive(Debug)]
pub enum AudioEngineError {
    NoOutputDevice,
    DefaultStreamConfigError(String),
    BuildStreamError(String),
    PlayStreamError(String),
}

impl std::fmt::Display for AudioEngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoOutputDevice => write!(f, "No audio output device found"),
            Self::DefaultStreamConfigError(e) => {
                write!(f, "Failed to get default stream config: {e}")
            }
            Self::BuildStreamError(e) => write!(f, "Failed to build audio stream: {e}"),
            Self::PlayStreamError(e) => write!(f, "Failed to start audio stream: {e}"),
        }
    }
}

impl std::error::Error for AudioEngineError {}

/// Starts samples from a thread other than the one holding the
/// `AudioEngine`, on its own lock-free queue into the mixer.
pub struct SampleTrigger {
    tx: Producer<AudioCommand>,
}

impl SampleTrigger {
    /// Starts a sample in the next buffer (wait-free).
    pub fn play(&mut self, sample_id: WavId) {
        let _ = self.tx.push(AudioCommand::PlaySample {
            sample_id,
            volume: 1.0,
            pan: 0.0,
        });
    }
}

/// Main audio engine holding the playback stream, command producer, and visual levels.
pub struct AudioEngine {
    _stream: Stream,
    command_tx: Producer<AudioCommand>,
    trigger: Option<SampleTrigger>,
    clock: AudioClock,
    visual_levels: Arc<[AtomicU32; 16]>,
}

impl AudioEngine {
    /// Initializes cpal audio stream, lock-free ring buffer, and audio clock.
    pub fn new(sample_bank: SampleBank) -> Result<Self, AudioEngineError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioEngineError::NoOutputDevice)?;

        let supported_config = device
            .default_output_config()
            .map_err(|e| AudioEngineError::DefaultStreamConfigError(e.to_string()))?;

        let sample_rate = supported_config.sample_rate().0;
        let config: StreamConfig = supported_config.into();

        let clock = AudioClock::new(sample_rate);

        let visual_levels: Arc<[AtomicU32; 16]> =
            Arc::new(std::array::from_fn(|_| AtomicU32::new(0)));
        let (producer, consumer) = RingBuffer::new(COMMAND_QUEUE_CAPACITY);
        let (trigger_tx, trigger_rx) = RingBuffer::new(COMMAND_QUEUE_CAPACITY);
        let mut mixer = Mixer::new(sample_bank, consumer, &clock, Arc::clone(&visual_levels))
            .with_trigger_queue(trigger_rx);

        let err_fn = |err| eprintln!("Audio stream error: {err}");

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    mixer.process_buffer(data);
                },
                err_fn,
                None,
            )
            .map_err(|e| AudioEngineError::BuildStreamError(e.to_string()))?;

        stream
            .play()
            .map_err(|e| AudioEngineError::PlayStreamError(e.to_string()))?;

        Ok(Self {
            _stream: stream,
            command_tx: producer,
            trigger: Some(SampleTrigger { tx: trigger_tx }),
            clock,
            visual_levels,
        })
    }

    /// The engine's second sample queue, for another thread (once).
    pub fn take_trigger(&mut self) -> Option<SampleTrigger> {
        self.trigger.take()
    }

    /// Access the lock-free audio clock.
    pub fn clock(&self) -> &AudioClock {
        &self.clock
    }

    /// Reads the latest 16-band peak levels (0.0 .. 1.0) without lock contention.
    pub fn get_visual_levels(&self, out: &mut [f32; 16]) {
        for (i, slot) in self.visual_levels.iter().enumerate() {
            out[i] = slot.load(Ordering::Relaxed) as f32 / 1000.0;
        }
    }

    /// Enqueue a command to the audio thread (wait-free, lock-free).
    pub fn send_command(&mut self, cmd: AudioCommand) -> Result<(), AudioCommand> {
        self.command_tx.push(cmd).map_err(|e| match e {
            rtrb::PushError::Full(val) => val,
        })
    }

    /// Starts a sample at `time_seconds` on the audio clock, to the frame.
    /// A time already past plays at once.
    pub fn play_at(&mut self, sample_id: WavId, time_seconds: f64) -> Result<(), AudioCommand> {
        let rate = self.clock.sample_rate() as f64;
        self.send_command(AudioCommand::PlaySampleAt {
            sample_id,
            start_sample: (time_seconds.max(0.0) * rate).round() as u64,
            volume: 1.0,
            pan: 0.0,
        })
    }

    /// Pause audio playback and master clock advancement.
    pub fn pause(&mut self) -> Result<(), AudioCommand> {
        self.send_command(AudioCommand::Pause)
    }

    /// Resume audio playback and master clock advancement.
    pub fn resume(&mut self) -> Result<(), AudioCommand> {
        self.send_command(AudioCommand::Resume)
    }

    /// Stop all active playing voices immediately.
    pub fn stop_all(&mut self) -> Result<(), AudioCommand> {
        self.send_command(AudioCommand::StopAll)
    }

    /// Set master volume multiplier (0.0 ~ 2.0).
    pub fn set_master_volume(&mut self, vol: f32) -> Result<(), AudioCommand> {
        self.send_command(AudioCommand::SetMasterVolume(vol))
    }
}

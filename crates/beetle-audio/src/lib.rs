//! # beetle-audio
//!
//! Realtime audio engine, lock-free mixer, master audio clock, and pre-decoded PCM soundbank.

pub mod atlas;
pub mod clock;
pub mod command;
pub mod engine;
pub mod mixer;
pub mod sample;

pub use atlas::{
    load_sample_bank_from_sound_atlas, load_sample_bank_from_sound_atlas_for_chart,
    resample_to_44k_stereo, SoundAtlasBuilder,
};
pub use clock::AudioClock;
pub use command::AudioCommand;
pub use engine::{AudioEngine, AudioEngineError, SampleTrigger};
pub use mixer::Mixer;
pub use sample::{AudioDecodeError, PcmBuffer, SampleBank};

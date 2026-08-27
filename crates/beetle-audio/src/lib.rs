//! # beetle-audio
//!
//! Realtime audio engine, lock-free mixer, master audio clock, and pre-decoded PCM soundbank.

pub mod clock;
pub mod command;
pub mod engine;
pub mod mixer;
pub mod sample;

pub use clock::AudioClock;
pub use command::AudioCommand;
pub use engine::{AudioEngine, AudioEngineError};
pub use mixer::Mixer;
pub use sample::{AudioDecodeError, PcmBuffer, SampleBank};

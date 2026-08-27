use beetle_core::WavId;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Decoded interleaved 32-bit floating point stereo/mono PCM audio buffer.
#[derive(Debug, Clone)]
pub struct PcmBuffer {
    pub channels: u16,
    pub sample_rate: u32,
    pub samples: Arc<[f32]>,
}

/// Errors that can occur when decoding audio samples.
#[derive(Debug)]
pub enum AudioDecodeError {
    IoError(std::io::Error),
    WavDecodeError(String),
    UnsupportedFormat(String),
}

impl std::fmt::Display for AudioDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "I/O error: {e}"),
            Self::WavDecodeError(msg) => write!(f, "WAV decode error: {msg}"),
            Self::UnsupportedFormat(msg) => write!(f, "Unsupported audio format: {msg}"),
        }
    }
}

impl std::error::Error for AudioDecodeError {}

/// Preloaded soundbank holding pre-decoded PCM data for all keysounds.
#[derive(Debug, Default, Clone)]
pub struct SampleBank {
    samples: HashMap<WavId, PcmBuffer>,
}

impl SampleBank {
    pub fn new() -> Self {
        Self {
            samples: HashMap::new(),
        }
    }

    pub fn insert(&mut self, id: WavId, buffer: PcmBuffer) {
        self.samples.insert(id, buffer);
    }

    pub fn get(&self, id: WavId) -> Option<&PcmBuffer> {
        self.samples.get(&id)
    }

    /// Load a WAV file from disk and pre-decode to PCM.
    pub fn load_wav_file<P: AsRef<Path>>(_path: P) -> Result<PcmBuffer, AudioDecodeError> {
        // TODO: Full WAV loader via hound in Phase 2
        Ok(PcmBuffer {
            channels: 2,
            sample_rate: 44100,
            samples: Arc::new([]),
        })
    }
}

use beetle_core::{BmsChart, WavId};
use hound::{SampleFormat, WavReader};
use lewton::inside_ogg::OggStreamReader;
use std::collections::HashMap;
use std::io::{Cursor, Read, Seek};
use std::path::Path;
use std::sync::Arc;

/// Decoded interleaved 32-bit floating point stereo PCM audio buffer.
/// Always normalized to stereo (2 channels) for zero-branching inner mixing loops.
#[derive(Debug, Clone)]
pub struct PcmBuffer {
    pub sample_rate: u32,
    pub samples: Arc<[f32]>, // Interleaved [L0, R0, L1, R1, ...]
}

impl PcmBuffer {
    pub fn new(sample_rate: u32, samples: Vec<f32>) -> Self {
        Self {
            sample_rate,
            samples: samples.into(),
        }
    }

    /// Total number of stereo frames (sample count / 2).
    pub fn frame_count(&self) -> usize {
        self.samples.len() / 2
    }

    /// Duration of audio buffer in seconds.
    pub fn duration_seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.frame_count() as f64 / self.sample_rate as f64
        }
    }
}

/// Errors that can occur when decoding audio samples.
#[derive(Debug)]
pub enum AudioDecodeError {
    IoError(std::io::Error),
    WavDecodeError(String),
    OggDecodeError(String),
    UnsupportedFormat(String),
}

impl std::fmt::Display for AudioDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "I/O error: {e}"),
            Self::WavDecodeError(msg) => write!(f, "WAV decode error: {msg}"),
            Self::OggDecodeError(msg) => write!(f, "OGG decode error: {msg}"),
            Self::UnsupportedFormat(msg) => write!(f, "Unsupported audio format: {msg}"),
        }
    }
}

impl std::error::Error for AudioDecodeError {}

impl From<std::io::Error> for AudioDecodeError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

impl From<hound::Error> for AudioDecodeError {
    fn from(e: hound::Error) -> Self {
        Self::WavDecodeError(e.to_string())
    }
}

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

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Decode WAV from any `Read + Seek` stream into stereo normalized `PcmBuffer`.
    pub fn load_wav_from_reader<R: Read + Seek>(reader: R) -> Result<PcmBuffer, AudioDecodeError> {
        let mut wav_reader = WavReader::new(reader)?;
        let spec = wav_reader.spec();

        let channels = spec.channels as usize;
        let sample_rate = spec.sample_rate;

        if channels == 0 || channels > 2 {
            return Err(AudioDecodeError::UnsupportedFormat(format!(
                "Channels count {channels} not supported (only mono or stereo)"
            )));
        }

        let raw_samples: Vec<f32> = match spec.sample_format {
            SampleFormat::Float => wav_reader
                .samples::<f32>()
                .filter_map(|s| s.ok())
                .collect(),
            SampleFormat::Int => match spec.bits_per_sample {
                8 => wav_reader
                    .samples::<i8>()
                    .filter_map(|s| s.ok())
                    .map(|s| s as f32 / 128.0)
                    .collect(),
                16 => wav_reader
                    .samples::<i16>()
                    .filter_map(|s| s.ok())
                    .map(|s| s as f32 / 32768.0)
                    .collect(),
                24 => wav_reader
                    .samples::<i32>()
                    .filter_map(|s| s.ok())
                    .map(|s| s as f32 / 8388608.0)
                    .collect(),
                32 => wav_reader
                    .samples::<i32>()
                    .filter_map(|s| s.ok())
                    .map(|s| s as f32 / 2147483648.0)
                    .collect(),
                bits => {
                    return Err(AudioDecodeError::UnsupportedFormat(format!(
                        "Unsupported bit depth: {bits}"
                    )))
                }
            },
        };

        // Normalize to stereo
        let stereo_samples = if channels == 1 {
            let mut stereo = Vec::with_capacity(raw_samples.len() * 2);
            for s in raw_samples {
                stereo.push(s);
                stereo.push(s);
            }
            stereo
        } else {
            raw_samples
        };

        Ok(PcmBuffer::new(sample_rate, stereo_samples))
    }

    /// Decode OGG Vorbis from any `Read + Seek` stream into stereo normalized `PcmBuffer`.
    pub fn load_ogg_from_reader<R: Read + Seek>(reader: R) -> Result<PcmBuffer, AudioDecodeError> {
        let mut ogg_reader =
            OggStreamReader::new(reader).map_err(|e| AudioDecodeError::OggDecodeError(e.to_string()))?;

        let channels = ogg_reader.ident_hdr.audio_channels as usize;
        let sample_rate = ogg_reader.ident_hdr.audio_sample_rate;

        if channels == 0 || channels > 2 {
            return Err(AudioDecodeError::UnsupportedFormat(format!(
                "Channels count {channels} not supported (only mono or stereo)"
            )));
        }

        let mut raw_samples = Vec::new();
        while let Some(packet) = ogg_reader
            .read_dec_packet_itl()
            .map_err(|e| AudioDecodeError::OggDecodeError(e.to_string()))?
        {
            for s in packet {
                raw_samples.push(s as f32 / 32768.0);
            }
        }

        let stereo_samples = if channels == 1 {
            let mut stereo = Vec::with_capacity(raw_samples.len() * 2);
            for s in raw_samples {
                stereo.push(s);
                stereo.push(s);
            }
            stereo
        } else {
            raw_samples
        };

        Ok(PcmBuffer::new(sample_rate, stereo_samples))
    }

    /// Decodes an audio file (WAV or OGG) from an in-memory byte buffer into stereo normalized PCM.
    pub fn load_audio_from_bytes(data: &[u8]) -> Result<PcmBuffer, AudioDecodeError> {
        let cursor = Cursor::new(data);
        match Self::load_wav_from_reader(cursor.clone()) {
            Ok(pcm) => Ok(pcm),
            Err(_) => Self::load_ogg_from_reader(cursor),
        }
    }

    /// Load an audio file (WAV or OGG) from disk and pre-decode to PCM.
    pub fn load_audio_file<P: AsRef<Path>>(path: P) -> Result<PcmBuffer, AudioDecodeError> {
        let p = path.as_ref();
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");

        if ext.eq_ignore_ascii_case("ogg") {
            let file = std::fs::File::open(p)?;
            Self::load_ogg_from_reader(std::io::BufReader::new(file))
        } else {
            let file = std::fs::File::open(p)?;
            // Attempt WAV first, fallback to OGG if format header mismatches
            match Self::load_wav_from_reader(std::io::BufReader::new(file)) {
                Ok(pcm) => Ok(pcm),
                Err(_) => {
                    let file = std::fs::File::open(p)?;
                    Self::load_ogg_from_reader(std::io::BufReader::new(file))
                }
            }
        }
    }

    /// Pre-decodes and loads all `#WAVxx` audio files referenced in a chart from the song directory.
    pub fn load_chart_soundbank<P: AsRef<Path>>(chart: &BmsChart, chart_dir: P) -> (Self, usize) {
        let dir = chart_dir.as_ref();
        let mut bank = Self::new();
        let mut loaded_count = 0;

        for (&wav_id, filename) in &chart.header.wav_table {
            let file_path = dir.join(filename);
            let mut resolved_path = None;

            if file_path.exists() {
                resolved_path = Some(file_path);
            } else {
                // Try smart alternate extensions (.wav, .ogg, .WAV, .OGG)
                let stem = Path::new(filename).file_stem().unwrap_or_default();
                let wav_alt = dir.join(format!("{}.wav", stem.to_string_lossy()));
                let ogg_alt = dir.join(format!("{}.ogg", stem.to_string_lossy()));
                let wav_upper = dir.join(format!("{}.WAV", stem.to_string_lossy()));
                let ogg_upper = dir.join(format!("{}.OGG", stem.to_string_lossy()));

                if wav_alt.exists() {
                    resolved_path = Some(wav_alt);
                } else if ogg_alt.exists() {
                    resolved_path = Some(ogg_alt);
                } else if wav_upper.exists() {
                    resolved_path = Some(wav_upper);
                } else if ogg_upper.exists() {
                    resolved_path = Some(ogg_upper);
                }
            }

            if let Some(path) = resolved_path {
                if let Ok(pcm) = Self::load_audio_file(&path) {
                    bank.insert(wav_id, pcm);
                    loaded_count += 1;
                }
            }
        }

        (bank, loaded_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn create_synthetic_wav(channels: u16, sample_rate: u32, bits: u16, samples: &[i16]) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        let spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: bits,
            sample_format: SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::new(&mut buffer, spec).unwrap();
        for &s in samples {
            writer.write_sample(s).unwrap();
        }
        writer.finalize().unwrap();
        buffer.into_inner()
    }

    #[test]
    fn test_load_mono_16bit_wav() {
        let samples = vec![0, 16384, 32767, -16384, -32768];
        let wav_data = create_synthetic_wav(1, 44100, 16, &samples);
        let pcm = SampleBank::load_wav_from_reader(Cursor::new(wav_data)).expect("Failed to load WAV");

        assert_eq!(pcm.sample_rate, 44100);
        // Mono duplicated to stereo: 5 frames * 2 channels = 10 samples
        assert_eq!(pcm.samples.len(), 10);
        assert_eq!(pcm.frame_count(), 5);

        // Check values normalized to -1.0 .. 1.0
        assert!((pcm.samples[0] - 0.0).abs() < 0.001);
        assert!((pcm.samples[1] - 0.0).abs() < 0.001);
        assert!((pcm.samples[2] - 0.5).abs() < 0.001);
        assert!((pcm.samples[3] - 0.5).abs() < 0.001);
        assert!((pcm.samples[4] - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_load_stereo_16bit_wav() {
        let samples = vec![0, 16384, -16384, 0]; // 2 frames
        let wav_data = create_synthetic_wav(2, 48000, 16, &samples);
        let pcm = SampleBank::load_wav_from_reader(Cursor::new(wav_data)).expect("Failed to load WAV");

        assert_eq!(pcm.sample_rate, 48000);
        assert_eq!(pcm.samples.len(), 4);
        assert_eq!(pcm.frame_count(), 2);
    }
}

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
    pub offset: usize,       // Start sample index (even number for stereo alignment)
    pub length: usize,       // Number of interleaved samples in this slice
}

impl PcmBuffer {
    pub fn new(sample_rate: u32, samples: Vec<f32>) -> Self {
        let len = samples.len();
        Self {
            sample_rate,
            samples: samples.into(),
            offset: 0,
            length: len,
        }
    }

    /// Creates a sub-slice view over a shared PCM buffer with zero copy.
    pub fn from_slice(sample_rate: u32, samples: Arc<[f32]>, offset: usize, length: usize) -> Self {
        assert!(
            offset + length <= samples.len(),
            "PcmBuffer slice bounds out of range: offset {} + len {} > total {}",
            offset,
            length,
            samples.len()
        );
        Self {
            sample_rate,
            samples,
            offset,
            length,
        }
    }

    /// Total number of stereo frames in this buffer/slice (sample count / 2).
    pub fn frame_count(&self) -> usize {
        self.length / 2
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
    FlacDecodeError(String),
    UnsupportedFormat(String),
}

impl std::fmt::Display for AudioDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "I/O error: {e}"),
            Self::WavDecodeError(msg) => write!(f, "WAV decode error: {msg}"),
            Self::OggDecodeError(msg) => write!(f, "OGG decode error: {msg}"),
            Self::FlacDecodeError(msg) => write!(f, "FLAC decode error: {msg}"),
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

impl From<claxon::Error> for AudioDecodeError {
    fn from(e: claxon::Error) -> Self {
        Self::FlacDecodeError(e.to_string())
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

    /// Loads an entire soundbank from a pre-compiled Sound Atlas metadata and binary buffer with zero-copy slice references.
    pub fn load_from_sound_atlas(
        meta: &bms_package::SoundAtlasMeta,
        atlas_data: &[u8],
    ) -> Result<Self, AudioDecodeError> {
        crate::atlas::load_sample_bank_from_sound_atlas(meta, atlas_data)
    }

    /// Loads keysounds from a Sound Atlas mapped specifically for the provided BMS chart.
    pub fn load_from_sound_atlas_for_chart(
        chart: &beetle_core::BmsChart,
        meta: &bms_package::SoundAtlasMeta,
        atlas_data: &[u8],
    ) -> Result<Self, AudioDecodeError> {
        crate::atlas::load_sample_bank_from_sound_atlas_for_chart(Some(chart), meta, atlas_data)
    }

    pub fn insert(&mut self, id: WavId, buffer: PcmBuffer) {
        self.samples.insert(id, buffer);
    }

    pub fn get(&self, id: WavId) -> Option<&PcmBuffer> {
        self.samples.get(&id)
    }

    pub fn contains_key(&self, id: WavId) -> bool {
        self.samples.contains_key(&id)
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Decode WAV from any `Read + Seek` stream into stereo normalized `PcmBuffer`.
    pub fn load_wav_from_reader<R: Read + Seek>(
        mut reader: R,
    ) -> Result<PcmBuffer, AudioDecodeError> {
        match WavReader::new(&mut reader) {
            Ok(wav_reader) => Self::decode_hound_wav(wav_reader),
            Err(e) => {
                // Early-2000s BMS keysounds often carry a data chunk length that
                // doesn't match the file (odd, or past EOF); hound rejects them
                // outright. Players tolerate that, so retry with a lenient parse.
                reader.rewind()?;
                let mut bytes = Vec::new();
                reader.read_to_end(&mut bytes)?;
                Self::decode_wav_lenient(&bytes).ok_or_else(|| e.into())
            }
        }
    }

    /// Minimal tolerant RIFF/WAVE reader: PCM 8/16/24/32-bit and float32, with
    /// the `data` length clamped to the bytes actually present and whole frames.
    fn decode_wav_lenient(b: &[u8]) -> Option<PcmBuffer> {
        if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
            return None;
        }
        let u16_at = |p: usize| u16::from_le_bytes([b[p], b[p + 1]]);
        let u32_at = |p: usize| u32::from_le_bytes([b[p], b[p + 1], b[p + 2], b[p + 3]]);
        let (mut fmt, mut data) = (None, None);
        let mut p = 12;
        while p + 8 <= b.len() {
            let size = u32_at(p + 4) as usize;
            let body = p + 8;
            match &b[p..p + 4] {
                b"fmt " if size >= 16 && body + 16 <= b.len() => {
                    fmt = Some((u16_at(body), u16_at(body + 2), u32_at(body + 4), u16_at(body + 14)));
                }
                b"data" => {
                    data = Some(&b[body..body.saturating_add(size).min(b.len())]);
                    break;
                }
                _ => {}
            }
            p = body.saturating_add(size).saturating_add(size & 1);
        }
        let ((tag, channels, sample_rate, bits), data) = (fmt?, data?);
        let channels = channels as usize;
        if channels == 0 || channels > 2 {
            return None;
        }
        let bytes_per = (bits as usize / 8).max(1);
        let data = &data[..data.len() / (bytes_per * channels) * (bytes_per * channels)];
        let raw: Vec<f32> = match (tag, bits) {
            (1 | 0xFFFE, 8) => data.iter().map(|&s| (s as f32 - 128.0) / 128.0).collect(),
            (1 | 0xFFFE, 16) => data
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
                .collect(),
            (1 | 0xFFFE, 24) => data
                .chunks_exact(3)
                .map(|c| (i32::from_le_bytes([0, c[0], c[1], c[2]]) >> 8) as f32 / 8388608.0)
                .collect(),
            (1 | 0xFFFE, 32) => data
                .chunks_exact(4)
                .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f32 / 2147483648.0)
                .collect(),
            (3, 32) => data
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect(),
            _ => return None,
        };
        let samples = if channels == 1 {
            raw.iter().flat_map(|&s| [s, s]).collect()
        } else {
            raw
        };
        Some(PcmBuffer::new(sample_rate, samples))
    }

    fn decode_hound_wav<R: Read>(
        mut wav_reader: WavReader<R>,
    ) -> Result<PcmBuffer, AudioDecodeError> {
        let spec = wav_reader.spec();

        let channels = spec.channels as usize;
        let sample_rate = spec.sample_rate;

        if channels == 0 || channels > 2 {
            return Err(AudioDecodeError::UnsupportedFormat(format!(
                "Channels count {channels} not supported (only mono or stereo)"
            )));
        }

        let raw_samples: Vec<f32> = match spec.sample_format {
            SampleFormat::Float => wav_reader.samples::<f32>().filter_map(|s| s.ok()).collect(),
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
        let mut ogg_reader = OggStreamReader::new(reader)
            .map_err(|e| AudioDecodeError::OggDecodeError(e.to_string()))?;

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

    /// Decode FLAC from any `Read` stream into stereo normalized `PcmBuffer`.
    pub fn load_flac_from_reader<R: Read>(reader: R) -> Result<PcmBuffer, AudioDecodeError> {
        let mut flac_reader = claxon::FlacReader::new(reader)?;
        let info = flac_reader.streaminfo();
        let channels = info.channels as usize;
        let sample_rate = info.sample_rate;
        let bps = info.bits_per_sample;

        if channels == 0 || channels > 2 {
            return Err(AudioDecodeError::UnsupportedFormat(format!(
                "Channels count {channels} not supported (only mono or stereo)"
            )));
        }

        let divisor = match bps {
            8 => 128.0,
            16 => 32768.0,
            24 => 8388608.0,
            32 => 2147483648.0,
            bits => (1i64 << (bits - 1)) as f32,
        };

        let mut raw_samples = Vec::new();
        for sample in flac_reader.samples() {
            let s = sample.map_err(|e| AudioDecodeError::FlacDecodeError(e.to_string()))?;
            raw_samples.push(s as f32 / divisor);
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

    /// Decodes an audio file (WAV, FLAC, or OGG) from an in-memory byte buffer into stereo normalized PCM.
    pub fn load_audio_from_bytes(data: &[u8]) -> Result<PcmBuffer, AudioDecodeError> {
        if data.starts_with(b"fLaC") {
            // Claxon strictly requires STREAMINFO min_block_size >= 16.
            // Some encoders (such as flacenc) set min_block_size < 16 when the last block has < 16 samples.
            // In that case, clamp min_block_size to 16 to ensure compatibility and avoid spurious decode errors.
            if data.len() >= 10 && (data[4] & 0x7F) == 0 {
                let min_bs = u16::from_be_bytes([data[8], data[9]]);
                if min_bs < 16 {
                    let mut patched = data.to_vec();
                    patched[8] = 0;
                    patched[9] = 16;
                    return Self::load_flac_from_reader(Cursor::new(patched));
                }
            }
            return Self::load_flac_from_reader(Cursor::new(data));
        }
        if data.starts_with(b"RIFF") {
            return Self::load_wav_from_reader(Cursor::new(data));
        }
        if data.starts_with(b"OggS") {
            return Self::load_ogg_from_reader(Cursor::new(data));
        }

        let cursor = Cursor::new(data);
        if let Ok(pcm) = Self::load_wav_from_reader(cursor.clone()) {
            return Ok(pcm);
        }
        if let Ok(pcm) = Self::load_flac_from_reader(cursor.clone()) {
            return Ok(pcm);
        }
        Self::load_ogg_from_reader(cursor)
    }

    /// Load an audio file (WAV, FLAC, or OGG) from disk and pre-decode to PCM.
    pub fn load_audio_file<P: AsRef<Path>>(path: P) -> Result<PcmBuffer, AudioDecodeError> {
        let p = path.as_ref();
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");

        if ext.eq_ignore_ascii_case("flac") {
            let file = std::fs::File::open(p)?;
            Self::load_flac_from_reader(std::io::BufReader::new(file))
        } else if ext.eq_ignore_ascii_case("ogg") {
            let file = std::fs::File::open(p)?;
            Self::load_ogg_from_reader(std::io::BufReader::new(file))
        } else {
            let file = std::fs::File::open(p)?;
            // Attempt WAV first, fallback to FLAC then OGG if format header mismatches
            let mut buf = std::io::BufReader::new(file);
            match Self::load_wav_from_reader(&mut buf) {
                Ok(pcm) => Ok(pcm),
                Err(_) => {
                    let file = std::fs::File::open(p)?;
                    let buf = std::io::BufReader::new(file);
                    match Self::load_flac_from_reader(buf) {
                        Ok(pcm) => Ok(pcm),
                        Err(_) => {
                            let file = std::fs::File::open(p)?;
                            Self::load_ogg_from_reader(std::io::BufReader::new(file))
                        }
                    }
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
                // Try smart alternate extensions (.flac, .wav, .ogg, .FLAC, .WAV, .OGG)
                let stem = Path::new(filename).file_stem().unwrap_or_default();
                let stem_str = stem.to_string_lossy();
                let candidates = [
                    dir.join(format!("{stem_str}.flac")),
                    dir.join(format!("{stem_str}.wav")),
                    dir.join(format!("{stem_str}.ogg")),
                    dir.join(format!("{stem_str}.FLAC")),
                    dir.join(format!("{stem_str}.WAV")),
                    dir.join(format!("{stem_str}.OGG")),
                ];

                for cand in candidates {
                    if cand.exists() {
                        resolved_path = Some(cand);
                        break;
                    }
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

    fn create_synthetic_wav(
        channels: u16,
        sample_rate: u32,
        bits: u16,
        samples: &[i16],
    ) -> Vec<u8> {
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
        let pcm =
            SampleBank::load_wav_from_reader(Cursor::new(wav_data)).expect("Failed to load WAV");

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
        let pcm =
            SampleBank::load_wav_from_reader(Cursor::new(wav_data)).expect("Failed to load WAV");

        assert_eq!(pcm.sample_rate, 48000);
        assert_eq!(pcm.samples.len(), 4);
        assert_eq!(pcm.frame_count(), 2);
    }

    #[test]
    fn test_load_8bit_unsigned_wav() {
        // Create synthetic 8-bit mono WAV manually:
        // Format: RIFF WAV, 1 channel, 44100 Hz, 8-bit
        // Samples: 128 (silence / 0.0), 0 (-1.0), 255 (+0.992)
        let mut wav_bytes = Vec::new();
        wav_bytes.extend_from_slice(b"RIFF");
        let chunk_size: u32 = 36 + 3;
        wav_bytes.extend_from_slice(&chunk_size.to_le_bytes());
        wav_bytes.extend_from_slice(b"WAVE");
        wav_bytes.extend_from_slice(b"fmt ");
        wav_bytes.extend_from_slice(&16u32.to_le_bytes()); // subchunk1 size
        wav_bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav_bytes.extend_from_slice(&1u16.to_le_bytes()); // 1 channel (mono)
        wav_bytes.extend_from_slice(&44100u32.to_le_bytes()); // sample rate
        wav_bytes.extend_from_slice(&44100u32.to_le_bytes()); // byte rate
        wav_bytes.extend_from_slice(&1u16.to_le_bytes()); // block align
        wav_bytes.extend_from_slice(&8u16.to_le_bytes()); // bits per sample
        wav_bytes.extend_from_slice(b"data");
        wav_bytes.extend_from_slice(&3u32.to_le_bytes()); // data size
        wav_bytes.push(128); // center / silence -> 0.0
        wav_bytes.push(0); // min -> -1.0
        wav_bytes.push(255); // max -> ~+0.992

        let pcm = SampleBank::load_wav_from_reader(Cursor::new(wav_bytes))
            .expect("Failed to load 8-bit WAV");
        assert_eq!(pcm.sample_rate, 44100);
        assert_eq!(pcm.frame_count(), 3);
        // Mono duplicated to stereo
        assert!(
            (pcm.samples[0] - 0.0).abs() < 0.001,
            "Expected 0.0 for 128 silence, got {}",
            pcm.samples[0]
        );
        assert!(
            (pcm.samples[1] - 0.0).abs() < 0.001,
            "Expected 0.0 for 128 silence, got {}",
            pcm.samples[1]
        );
        assert!(
            (pcm.samples[2] - (-1.0)).abs() < 0.001,
            "Expected -1.0 for 0 min, got {}",
            pcm.samples[2]
        );
        assert!(
            (pcm.samples[3] - (-1.0)).abs() < 0.001,
            "Expected -1.0 for 0 min, got {}",
            pcm.samples[3]
        );
        assert!(
            (pcm.samples[4] - 0.9921875).abs() < 0.01,
            "Expected ~0.992 for 255 max, got {}",
            pcm.samples[4]
        );
    }

    #[test]
    fn lenient_wav_accepts_overlong_and_odd_data_chunk() {
        // 8-bit stereo @22050 with a data length claiming more than the file
        // holds, and an odd byte count: hound rejects it, we keep whole frames.
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF    WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        for v in [1u16, 2] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&22050u32.to_le_bytes());
        b.extend_from_slice(&44100u32.to_le_bytes());
        for v in [2u16, 8] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(b"data");
        b.extend_from_slice(&1001u32.to_le_bytes());
        b.extend_from_slice(&[255, 0, 128, 128, 255]);
        assert!(hound::WavReader::new(Cursor::new(&b)).is_err());
        let pcm = SampleBank::load_wav_from_reader(Cursor::new(&b)).unwrap();
        assert_eq!(pcm.sample_rate, 22050);
        assert_eq!(pcm.frame_count(), 2);
        assert!((pcm.samples[0] - 127.0 / 128.0).abs() < 1e-6);
        assert!((pcm.samples[1] + 1.0).abs() < 1e-6);
    }
}

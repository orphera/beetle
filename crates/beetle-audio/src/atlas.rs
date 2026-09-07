use crate::sample::{AudioDecodeError, PcmBuffer, SampleBank};
use beetle_core::bms::{decode_base36, encode_base36};
use beetle_core::WavId;
use bms_package::{SoundAtlasCodec, SoundAtlasMeta, SoundSlice};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

pub const STANDARD_SAMPLE_RATE: u32 = 44100;
pub const STANDARD_CHANNELS: u8 = 2;
pub const DEFAULT_PADDING_FRAMES: u32 = 128;

/// Resamples and normalizes any `PcmBuffer` to 44,100 Hz interleaved stereo.
pub fn resample_to_44k_stereo(buffer: &PcmBuffer) -> PcmBuffer {
    let src_sr = buffer.sample_rate;
    let src_frames = buffer.frame_count();

    if src_frames == 0 {
        return PcmBuffer::new(STANDARD_SAMPLE_RATE, Vec::new());
    }

    // Fast path: already 44.1kHz stereo and offset == 0
    if src_sr == STANDARD_SAMPLE_RATE && buffer.offset == 0 && buffer.length == buffer.samples.len()
    {
        return buffer.clone();
    }

    let target_sr = STANDARD_SAMPLE_RATE;
    let ratio = src_sr as f64 / target_sr as f64;
    let target_frames = ((src_frames as f64) / ratio).round() as usize;

    let mut out_samples = Vec::with_capacity(target_frames * 2);
    let base = buffer.offset;

    for i in 0..target_frames {
        let src_idx = i as f64 * ratio;
        let f0 = (src_idx as usize).min(src_frames - 1);
        let f1 = (f0 + 1).min(src_frames - 1);
        let alpha = (src_idx - f0 as f64) as f32;

        let l0 = buffer.samples[base + f0 * 2];
        let r0 = buffer.samples[base + f0 * 2 + 1];
        let l1 = buffer.samples[base + f1 * 2];
        let r1 = buffer.samples[base + f1 * 2 + 1];

        let l = l0 + alpha * (l1 - l0);
        let r = r0 + alpha * (r1 - r0);

        out_samples.push(l);
        out_samples.push(r);
    }

    PcmBuffer::new(STANDARD_SAMPLE_RATE, out_samples)
}

/// Parses a WavId from a slice key (Base36 or decimal numeric string).
pub fn parse_wav_id(key: &str) -> Option<WavId> {
    let bytes = key.as_bytes();
    if bytes.len() == 2 {
        if let Some(id) = decode_base36(bytes[0], bytes[1]) {
            return Some(id);
        }
    }
    key.parse::<u16>().ok().map(WavId)
}

/// Ahead-Of-Time (AOT) Sound Atlas Builder that packs multiple keysounds into a continuous stream.
pub struct SoundAtlasBuilder {
    codec: SoundAtlasCodec,
    sample_rate: u32,
    padding_frames: u32,
    entries: Vec<(String, PcmBuffer, Option<String>)>, // for PCM (key, pcm, original_filename)
    raw_entries: Vec<(String, Vec<u8>, Option<String>)>, // for OggBundle (key, raw_bytes, original_filename)
}

impl SoundAtlasBuilder {
    pub fn new(codec: SoundAtlasCodec) -> Self {
        Self {
            codec,
            sample_rate: STANDARD_SAMPLE_RATE,
            padding_frames: DEFAULT_PADDING_FRAMES,
            entries: Vec::new(),
            raw_entries: Vec::new(),
        }
    }

    pub fn with_padding_frames(mut self, padding: u32) -> Self {
        self.padding_frames = padding;
        self
    }

    /// Adds a keysound sample to be packed into the atlas (for PCM codecs).
    pub fn add_sample(
        &mut self,
        key: impl Into<String>,
        buffer: &PcmBuffer,
        original_filename: Option<String>,
    ) {
        let normalized = resample_to_44k_stereo(buffer);
        self.entries
            .push((key.into(), normalized, original_filename));
    }

    /// Adds a raw keysound audio byte stream (e.g. Vorbis OGG) to be bundled.
    pub fn add_raw(
        &mut self,
        key: impl Into<String>,
        raw_bytes: Vec<u8>,
        original_filename: Option<String>,
    ) {
        self.raw_entries
            .push((key.into(), raw_bytes, original_filename));
    }

    /// Compiles all added samples into a single continuous byte buffer and metadata.
    pub fn build(
        mut self,
        file_path: impl Into<String>,
    ) -> Result<(SoundAtlasMeta, Vec<u8>), AudioDecodeError> {
        if self.codec.is_bundle() {
            // Sort entries deterministically by key for reproducible packaging (INV-6)
            self.raw_entries.sort_by(|a, b| a.0.cmp(&b.0));

            let mut slices = BTreeMap::new();
            let mut total_bytes: Vec<u8> = Vec::new();
            let mut current_offset: u64 = 0;

            for (key, raw_data, orig_name) in self.raw_entries {
                let len = raw_data.len() as u64;
                if len == 0 {
                    continue;
                }

                let slice = SoundSlice::new(current_offset, len, orig_name);
                slices.insert(key, slice);

                total_bytes.extend_from_slice(&raw_data);
                current_offset += len;
            }

            let total_frames = current_offset; // represents total byte length
            let meta = SoundAtlasMeta::new(
                file_path,
                self.codec,
                self.sample_rate,
                STANDARD_CHANNELS,
                total_frames,
                0,
                slices,
            );

            if let Err(e) = meta.validate() {
                return Err(AudioDecodeError::UnsupportedFormat(e));
            }

            return Ok((meta, total_bytes));
        }

        // Sort entries deterministically by key for reproducible packaging (INV-6)
        self.entries.sort_by(|a, b| a.0.cmp(&b.0));

        let mut slices = BTreeMap::new();
        let mut total_f32_samples: Vec<f32> = Vec::new();
        let mut current_frame: u64 = 0;

        for (key, pcm, orig_name) in self.entries {
            let frames = pcm.frame_count() as u64;
            if frames == 0 {
                continue;
            }

            let start_frame = current_frame;
            let slice = SoundSlice::new(start_frame, frames, orig_name);
            slices.insert(key, slice);

            // Copy audio frames
            let base = pcm.offset;
            let len = pcm.length;
            total_f32_samples.extend_from_slice(&pcm.samples[base..base + len]);

            current_frame += frames;

            // Append zero-padding frames between keysounds to prevent filter bleeding
            if self.padding_frames > 0 {
                for _ in 0..(self.padding_frames * 2) {
                    total_f32_samples.push(0.0);
                }
                current_frame += self.padding_frames as u64;
            }
        }

        let total_frames = current_frame;

        // Encode f32 samples to raw byte stream according to chosen codec
        let byte_data = match self.codec {
            SoundAtlasCodec::Pcm16 => {
                let mut bytes = Vec::with_capacity(total_f32_samples.len() * 2);
                for &sample in &total_f32_samples {
                    let clamped = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
                    bytes.extend_from_slice(&clamped.to_le_bytes());
                }
                bytes
            }
            SoundAtlasCodec::PcmF32 => {
                let mut bytes = Vec::with_capacity(total_f32_samples.len() * 4);
                for &sample in &total_f32_samples {
                    bytes.extend_from_slice(&sample.to_le_bytes());
                }
                bytes
            }
            SoundAtlasCodec::OggBundle
            | SoundAtlasCodec::WavBundle
            | SoundAtlasCodec::FlacBundle => unreachable!(),
        };

        let meta = SoundAtlasMeta::new(
            file_path,
            self.codec,
            self.sample_rate,
            STANDARD_CHANNELS,
            total_frames,
            self.padding_frames,
            slices,
        );

        if let Err(e) = meta.validate() {
            return Err(AudioDecodeError::UnsupportedFormat(e));
        }

        Ok((meta, byte_data))
    }
}

/// Decodes a Sound Atlas byte stream into a `SampleBank` where each keysound is a pre-decoded PCM buffer.
pub fn load_sample_bank_from_sound_atlas(
    meta: &SoundAtlasMeta,
    atlas_data: &[u8],
) -> Result<SampleBank, AudioDecodeError> {
    load_sample_bank_from_sound_atlas_for_chart(None, meta, atlas_data)
}

/// Decodes a Sound Atlas byte stream into a `SampleBank` mapped directly for a specific BMS chart.
///
/// Resolves keysounds by chart `#WAV` table filename, stem, and `WavId` fallbacks,
/// ensuring that duplicate files across different `#WAV` IDs and multiple charts within
/// the same package are properly populated without missing notes.
pub fn load_sample_bank_from_sound_atlas_for_chart(
    chart: Option<&beetle_core::BmsChart>,
    meta: &SoundAtlasMeta,
    atlas_data: &[u8],
) -> Result<SampleBank, AudioDecodeError> {
    if let Err(e) = meta.validate() {
        return Err(AudioDecodeError::UnsupportedFormat(e));
    }

    // Build filename, stem, and key lookups for slice matching
    let mut slices_by_key: HashMap<&str, &SoundSlice> = HashMap::new();
    let mut slices_by_filename: HashMap<String, &SoundSlice> = HashMap::new();

    for (key, slice) in &meta.slices {
        slices_by_key.insert(key.as_str(), slice);
        if let Some(ref orig) = slice.original_filename {
            let norm = orig.replace('\\', "/").to_ascii_lowercase();
            let file_only = std::path::Path::new(&norm)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&norm)
                .to_string();
            slices_by_filename.entry(norm.clone()).or_insert(slice);
            slices_by_filename.entry(file_only.clone()).or_insert(slice);

            if norm.ends_with(".wav") {
                let base = &norm[..norm.len() - 4];
                slices_by_filename
                    .entry(format!("{}.ogg", base))
                    .or_insert(slice);
                let fbase = &file_only[..file_only.len() - 4];
                slices_by_filename
                    .entry(format!("{}.ogg", fbase))
                    .or_insert(slice);
            } else if norm.ends_with(".ogg") {
                let base = &norm[..norm.len() - 4];
                slices_by_filename
                    .entry(format!("{}.wav", base))
                    .or_insert(slice);
                let fbase = &file_only[..file_only.len() - 4];
                slices_by_filename
                    .entry(format!("{}.wav", fbase))
                    .or_insert(slice);
            }
        }
    }

    let find_slice_for = |wav_id: WavId, filename: &str| -> Option<&SoundSlice> {
        let norm = filename.replace('\\', "/").to_ascii_lowercase();
        let file_only = std::path::Path::new(&norm)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&norm);
        if let Some(s) = slices_by_filename.get(&norm).copied() {
            return Some(s);
        }
        if let Some(s) = slices_by_filename.get(file_only).copied() {
            return Some(s);
        }
        // New atlases are keyed by normalized file path and must resolve via
        // the chart's filename. Keep the ID lookup only for legacy atlases.
        let base36_key = encode_base36(wav_id);
        if let Some(s) = slices_by_key.get(base36_key.as_str()).copied() {
            return Some(s);
        }
        let dec_key = wav_id.0.to_string();
        if let Some(s) = slices_by_key.get(dec_key.as_str()).copied() {
            return Some(s);
        }
        None
    };

    if meta.codec.is_bundle() {
        let mut bank = SampleBank::new();
        let mut decoded_cache: HashMap<u64, PcmBuffer> = HashMap::new();

        let mut decode_slice = |slice: &SoundSlice| -> Result<PcmBuffer, AudioDecodeError> {
            if let Some(cached) = decoded_cache.get(&slice.start_frame) {
                return Ok(cached.clone());
            }
            let offset = slice.start_frame as usize;
            let length = slice.frame_count as usize;

            if offset + length <= atlas_data.len() {
                let slice_data = &atlas_data[offset..offset + length];
                let pcm = SampleBank::load_audio_from_bytes(slice_data)?;
                let normalized = resample_to_44k_stereo(&pcm);
                decoded_cache.insert(slice.start_frame, normalized.clone());
                Ok(normalized)
            } else {
                Err(AudioDecodeError::UnsupportedFormat(
                    "Sound slice offset/length out of atlas bounds".to_string(),
                ))
            }
        };

        // 1. Chart-specific mapping
        if let Some(c) = chart {
            for (&wav_id, filename) in &c.header.wav_table {
                if let Some(slice) = find_slice_for(wav_id, filename) {
                    if let Ok(pcm) = decode_slice(slice) {
                        bank.insert(wav_id, pcm);
                    }
                }
            }
        }

        // 2. Direct key mapping for any slices not yet mapped
        for (key, slice) in &meta.slices {
            if let Some(wav_id) = parse_wav_id(key) {
                if !bank.contains_key(wav_id) {
                    if let Ok(pcm) = decode_slice(slice) {
                        bank.insert(wav_id, pcm);
                    }
                }
            }
        }

        return Ok(bank);
    }

    // Single-shot decode of raw byte stream into continuous stereo f32 PCM buffer
    let shared_samples: Arc<[f32]> = match meta.codec {
        SoundAtlasCodec::Pcm16 => {
            let sample_count = atlas_data.len() / 2;
            let mut samples = Vec::with_capacity(sample_count);
            for chunk in atlas_data.chunks_exact(2) {
                let i = i16::from_le_bytes([chunk[0], chunk[1]]);
                samples.push(i as f32 / 32768.0);
            }
            samples.into()
        }
        SoundAtlasCodec::PcmF32 => {
            let sample_count = atlas_data.len() / 4;
            let mut samples = Vec::with_capacity(sample_count);
            for chunk in atlas_data.chunks_exact(4) {
                let f = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                samples.push(f);
            }
            samples.into()
        }
        SoundAtlasCodec::OggBundle
        | SoundAtlasCodec::WavBundle
        | SoundAtlasCodec::FlacBundle => unreachable!(),
    };

    let mut bank = SampleBank::new();

    let make_slice_pcm = |slice: &SoundSlice| -> Option<PcmBuffer> {
        let offset = (slice.start_frame * 2) as usize;
        let length = (slice.frame_count * 2) as usize;

        if offset + length <= shared_samples.len() {
            Some(PcmBuffer::from_slice(
                meta.sample_rate,
                Arc::clone(&shared_samples),
                offset,
                length,
            ))
        } else {
            None
        }
    };

    // 1. Chart-specific mapping
    if let Some(c) = chart {
        for (&wav_id, filename) in &c.header.wav_table {
            if let Some(slice) = find_slice_for(wav_id, filename) {
                if let Some(pcm) = make_slice_pcm(slice) {
                    bank.insert(wav_id, pcm);
                }
            }
        }
    }

    // 2. Direct key mapping for any slices not yet mapped
    for (key, slice) in &meta.slices {
        if let Some(wav_id) = parse_wav_id(key) {
            if !bank.contains_key(wav_id) {
                if let Some(pcm) = make_slice_pcm(slice) {
                    bank.insert(wav_id, pcm);
                }
            }
        }
    }

    Ok(bank)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sound_atlas_builder_roundtrip_pcm16() {
        let mut builder = SoundAtlasBuilder::new(SoundAtlasCodec::Pcm16).with_padding_frames(64);

        // Sound 1: 100 stereo frames (200 samples)
        let sound1 = PcmBuffer::new(44100, vec![0.5f32; 200]);
        builder.add_sample("01", &sound1, Some("kick.wav".to_string()));

        // Sound 2: 200 stereo frames (400 samples) at 22050Hz (will be resampled to 44100)
        let sound2 = PcmBuffer::new(22050, vec![0.25f32; 400]);
        builder.add_sample("02", &sound2, Some("snare.wav".to_string()));

        let (meta, bytes) = builder.build("audio/atlas.bin").expect("build failed");

        assert_eq!(meta.codec, SoundAtlasCodec::Pcm16);
        assert_eq!(meta.slices.len(), 2);
        assert!(meta.validate().is_ok());

        // Load into SampleBank
        let bank = load_sample_bank_from_sound_atlas(&meta, &bytes).expect("load failed");

        let pcm1 = bank.get(WavId(1)).expect("sound 1 missing");
        assert_eq!(pcm1.frame_count(), 100);
        // Verify amplitude preserved within 16-bit precision
        let sample_val = pcm1.samples[pcm1.offset];
        assert!((sample_val - 0.5).abs() < 0.001);

        let pcm2 = bank.get(WavId(2)).expect("sound 2 missing");
        // 200 frames at 22050Hz resampled to 44100Hz should be ~400 frames
        assert_eq!(pcm2.frame_count(), 400);

        // Verify that both sounds share the exact same underlying Arc allocation
        assert!(Arc::ptr_eq(&pcm1.samples, &pcm2.samples));
    }

    #[test]
    fn test_sound_atlas_builder_roundtrip_pcm_f32() {
        let mut builder = SoundAtlasBuilder::new(SoundAtlasCodec::PcmF32);
        let sound = PcmBuffer::new(44100, vec![0.123456f32; 100]);
        builder.add_sample("0A", &sound, None);

        let (meta, bytes) = builder.build("audio/atlas.bin").expect("build failed");
        assert_eq!(meta.codec, SoundAtlasCodec::PcmF32);

        let bank = load_sample_bank_from_sound_atlas(&meta, &bytes).expect("load failed");
        let id_0a = parse_wav_id("0A").unwrap();
        let pcm = bank.get(id_0a).expect("sound 0A missing");
        assert_eq!(pcm.frame_count(), 50);
        assert_eq!(pcm.samples[pcm.offset], 0.123456f32);
    }

    #[test]
    fn test_sound_atlas_builder_roundtrip_ogg_bundle() {
        let mut builder = SoundAtlasBuilder::new(SoundAtlasCodec::OggBundle);
        // Create valid tiny WAV bytes to test raw stream bundle decoding
        let mut wav_bytes = Vec::new();
        {
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 44100,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut writer =
                hound::WavWriter::new(std::io::Cursor::new(&mut wav_bytes), spec).unwrap();
            for _ in 0..100 {
                writer.write_sample(1000i16).unwrap();
            }
            writer.finalize().unwrap();
        }

        builder.add_raw("01", wav_bytes.clone(), Some("kick.wav".to_string()));
        builder.add_raw("02", wav_bytes.clone(), Some("snare.wav".to_string()));

        let (meta, bytes) = builder.build("audio/atlas.bin").expect("build failed");
        assert_eq!(meta.codec, SoundAtlasCodec::OggBundle);
        assert_eq!(meta.slices.len(), 2);
        assert!(meta.validate().is_ok());

        let bank = load_sample_bank_from_sound_atlas(&meta, &bytes).expect("load failed");
        let pcm1 = bank.get(WavId(1)).expect("sound 1 missing");
        assert_eq!(pcm1.frame_count(), 100);
        let pcm2 = bank.get(WavId(2)).expect("sound 2 missing");
        assert_eq!(pcm2.frame_count(), 100);
    }

    #[test]
    fn test_sound_atlas_builder_roundtrip_wav_bundle() {
        let mut builder = SoundAtlasBuilder::new(SoundAtlasCodec::WavBundle);
        let mut wav_bytes = Vec::new();
        {
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 22050,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut writer =
                hound::WavWriter::new(std::io::Cursor::new(&mut wav_bytes), spec).unwrap();
            for _ in 0..50 {
                writer.write_sample(500i16).unwrap();
            }
            writer.finalize().unwrap();
        }

        builder.add_raw("01", wav_bytes.clone(), Some("kick.wav".to_string()));

        let (meta, bytes) = builder.build("audio/atlas.bin").expect("build failed");
        assert_eq!(meta.codec, SoundAtlasCodec::WavBundle);
        assert_eq!(meta.slices.len(), 1);
        assert!(meta.validate().is_ok());

        let bank = load_sample_bank_from_sound_atlas(&meta, &bytes).expect("load failed");
        let pcm1 = bank.get(WavId(1)).expect("sound 1 missing");
        // Resampled from 22050 1-channel to 44100 stereo -> frame count doubled to 100
        assert_eq!(pcm1.frame_count(), 100);
    }
}

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Supported audio codecs for Sound Atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundAtlasCodec {
    /// 16-bit signed integer little-endian interleaved stereo PCM.
    Pcm16,
    /// 32-bit floating point little-endian interleaved stereo PCM.
    PcmF32,
    /// Bundled raw Vorbis OGG bitstreams indexed by byte offsets.
    OggBundle,
}

impl SoundAtlasCodec {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pcm16 => "pcm16",
            Self::PcmF32 => "pcm_f32",
            Self::OggBundle => "ogg_bundle",
        }
    }
}

impl Default for SoundAtlasCodec {
    fn default() -> Self {
        Self::Pcm16
    }
}

/// A slice referencing a single keysound within a continuous Sound Atlas audio stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoundSlice {
    /// Start frame index (for PCM) or byte offset (for OggBundle) in the atlas stream.
    pub start_frame: u64,
    /// Number of audio frames (for PCM) or byte length (for OggBundle).
    pub frame_count: u64,
    /// Original audio filename before packing (e.g. "kick.wav"), preserved for unpacking/VFS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_filename: Option<String>,
}

impl SoundSlice {
    pub fn new(start_frame: u64, frame_count: u64, original_filename: Option<String>) -> Self {
        Self {
            start_frame,
            frame_count,
            original_filename,
        }
    }

    /// End frame / byte index (exclusive).
    pub fn end_frame(&self) -> u64 {
        self.start_frame + self.frame_count
    }

    /// Byte offset within the atlas binary (alias for start_frame).
    pub fn byte_offset(&self) -> u64 {
        self.start_frame
    }

    /// Byte length within the atlas binary (alias for frame_count).
    pub fn byte_len(&self) -> u64 {
        self.frame_count
    }
}

/// Metadata describing the Sound Atlas contained within the package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoundAtlasMeta {
    /// Relative path to the atlas audio binary within the package (e.g. "audio/atlas.bin").
    pub file: String,
    /// Audio encoding codec.
    #[serde(default)]
    pub codec: SoundAtlasCodec,
    /// Standardized sampling rate in Hz (default 44100).
    pub sample_rate: u32,
    /// Standardized channel count (default 2 for stereo).
    pub channels: u8,
    /// Total audio frames (for PCM) or total bytes (for OggBundle) in the atlas.
    pub total_frames: u64,
    /// Zero-padding frames placed between keysound slices to prevent bleeding (default 128 for PCM).
    #[serde(default = "default_padding_frames")]
    pub padding_frames: u32,
    /// Mapping from WavId / key (e.g. "01", "0A", "ZZ") to its sound slice.
    pub slices: BTreeMap<String, SoundSlice>,
}

fn default_padding_frames() -> u32 {
    128
}

impl SoundAtlasMeta {
    pub fn new(
        file: impl Into<String>,
        codec: SoundAtlasCodec,
        sample_rate: u32,
        channels: u8,
        total_frames: u64,
        padding_frames: u32,
        slices: BTreeMap<String, SoundSlice>,
    ) -> Self {
        Self {
            file: file.into(),
            codec,
            sample_rate,
            channels,
            total_frames,
            padding_frames,
            slices,
        }
    }

    /// Validates that all slices fit within total_frames and do not overlap illegally.
    pub fn validate(&self) -> Result<(), String> {
        if self.codec != SoundAtlasCodec::OggBundle && self.channels != 2 {
            return Err(format!("SoundAtlas must be stereo (2 channels), got {}", self.channels));
        }
        if self.sample_rate == 0 {
            return Err("SoundAtlas sample_rate must be greater than 0".to_string());
        }

        for (key, slice) in &self.slices {
            if slice.end_frame() > self.total_frames {
                return Err(format!(
                    "Slice '{}' ends at {} which exceeds total_frames/bytes {}",
                    key,
                    slice.end_frame(),
                    self.total_frames
                ));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sound_atlas_meta_serialization_roundtrip() {
        let mut slices = BTreeMap::new();
        slices.insert(
            "01".to_string(),
            SoundSlice::new(0, 22050, Some("kick.wav".to_string())),
        );
        slices.insert(
            "02".to_string(),
            SoundSlice::new(22178, 44100, Some("snare.wav".to_string())),
        );

        let meta = SoundAtlasMeta::new(
            "audio/atlas.bin",
            SoundAtlasCodec::Pcm16,
            44100,
            2,
            66278,
            128,
            slices,
        );

        assert!(meta.validate().is_ok());

        let json = serde_json::to_string_pretty(&meta).expect("serialization failed");
        let deserialized: SoundAtlasMeta =
            serde_json::from_str(&json).expect("deserialization failed");

        assert_eq!(meta, deserialized);
        assert_eq!(deserialized.codec, SoundAtlasCodec::Pcm16);
        assert_eq!(deserialized.slices.len(), 2);
    }

    #[test]
    fn test_sound_atlas_meta_validation_failure() {
        let mut slices = BTreeMap::new();
        slices.insert("01".to_string(), SoundSlice::new(0, 500, None));

        let meta = SoundAtlasMeta::new(
            "audio/atlas.bin",
            SoundAtlasCodec::PcmF32,
            44100,
            2,
            400, // smaller than slice end!
            128,
            slices,
        );

        assert!(meta.validate().is_err());
    }

    #[test]
    fn test_sound_atlas_meta_ogg_bundle_roundtrip() {
        let mut slices = BTreeMap::new();
        slices.insert(
            "01".to_string(),
            SoundSlice::new(0, 1024, Some("kick.ogg".to_string())),
        );
        slices.insert(
            "02".to_string(),
            SoundSlice::new(1024, 2048, Some("snare.ogg".to_string())),
        );

        let meta = SoundAtlasMeta::new(
            "audio/atlas.bin",
            SoundAtlasCodec::OggBundle,
            44100,
            2,
            3072,
            0,
            slices,
        );

        assert!(meta.validate().is_ok());

        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("\"codec\":\"ogg_bundle\""));

        let deserialized: SoundAtlasMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(meta, deserialized);
        assert_eq!(deserialized.codec, SoundAtlasCodec::OggBundle);
        assert_eq!(deserialized.slices.get("01").unwrap().byte_offset(), 0);
        assert_eq!(deserialized.slices.get("01").unwrap().byte_len(), 1024);
    }
}

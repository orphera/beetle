use crate::atlas::{BgaAtlasMeta, SoundAtlasMeta};
use crate::error::PackageError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CURRENT_FORMAT_VERSION: u32 = 2;
pub const MANIFEST_FILENAME: &str = "manifest.json";

/// Package metadata and identity structure (`manifest.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Format specification version (currently 1 or 2).
    pub format: u32,
    /// Stable, persistent package identifier (e.g. `example.song`).
    pub id: String,
    /// Display name of the package.
    pub name: String,
    /// Optional author or creator name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Optional Sound Atlas metadata for Turbo profile packages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound_atlas: Option<SoundAtlasMeta>,
    /// Optional BGA Texture Atlas metadata for Turbo profile packages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bga_atlas: Option<BgaAtlasMeta>,
    /// Additional optional fields preserved for forward-compatibility.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Manifest {
    /// Creates a new minimal Manifest with the current format version.
    pub fn new<I: Into<String>, N: Into<String>>(id: I, name: N) -> Self {
        Self {
            format: CURRENT_FORMAT_VERSION,
            id: id.into(),
            name: name.into(),
            author: None,
            sound_atlas: None,
            bga_atlas: None,
            extra: BTreeMap::new(),
        }
    }

    /// Builder method to attach an author.
    pub fn with_author<A: Into<String>>(mut self, author: A) -> Self {
        self.author = Some(author.into());
        self
    }

    /// Builder method to attach a sound atlas.
    pub fn with_sound_atlas(mut self, sound_atlas: SoundAtlasMeta) -> Self {
        self.sound_atlas = Some(sound_atlas);
        self.format = 2;
        self
    }

    /// Builder method to attach a BGA texture atlas.
    pub fn with_bga_atlas(mut self, bga_atlas: BgaAtlasMeta) -> Self {
        self.bga_atlas = Some(bga_atlas);
        self.format = 2;
        self
    }

    /// Builder method to attach an extra key-value pair.
    pub fn with_extra<K: Into<String>>(mut self, key: K, value: serde_json::Value) -> Self {
        self.extra.insert(key.into(), value);
        self
    }

    /// Deserializes a manifest from a UTF-8 JSON string and validates its constraints.
    pub fn from_json_str(json: &str) -> Result<Self, PackageError> {
        let manifest: Self = serde_json::from_str(json)
            .map_err(|e| PackageError::InvalidManifest(format!("JSON parse error: {e}")))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Serializes the manifest to a pretty-printed, deterministic JSON string.
    pub fn to_json_string(&self) -> Result<String, PackageError> {
        self.validate()?;
        serde_json::to_string_pretty(self)
            .map_err(|e| PackageError::InvalidManifest(format!("JSON serialization error: {e}")))
    }

    /// Validates all required fields and format constraints.
    pub fn validate(&self) -> Result<(), PackageError> {
        // 1. Format version
        if self.format == 0 || self.format > CURRENT_FORMAT_VERSION {
            return Err(PackageError::UnsupportedFormat(self.format));
        }

        // 2. Id
        if self.id.trim().is_empty() {
            return Err(PackageError::InvalidManifest("Field 'id' cannot be empty".to_string()));
        }

        // 3. Name
        if self.name.trim().is_empty() {
            return Err(PackageError::InvalidManifest("Field 'name' cannot be empty".to_string()));
        }

        // 4. Sound Atlas validation if present
        if let Some(ref sa) = self.sound_atlas {
            sa.validate()
                .map_err(|e| PackageError::InvalidManifest(format!("Invalid sound_atlas: {e}")))?;
        }

        // 5. BGA Atlas validation if present
        if let Some(ref ba) = self.bga_atlas {
            ba.validate()
                .map_err(|e| PackageError::InvalidManifest(format!("Invalid bga_atlas: {e}")))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_serialization_and_validation() {
        let manifest = Manifest::new("test.song", "Test Song")
            .with_author("Beetle Dev")
            .with_extra("license", serde_json::json!("MIT"));

        let json = manifest.to_json_string().unwrap();
        let parsed = Manifest::from_json_str(&json).unwrap();

        assert_eq!(manifest.id, parsed.id);
        assert_eq!(manifest.name, parsed.name);
        assert_eq!(manifest.author, Some("Beetle Dev".to_string()));
        assert_eq!(parsed.extra.get("license"), Some(&serde_json::json!("MIT")));
    }

    #[test]
    fn test_invalid_manifest_rejection() {
        // Empty id
        let m = Manifest::new("", "Name");
        assert!(m.validate().is_err());

        // Empty name
        let m = Manifest::new("id", "");
        assert!(m.validate().is_err());

        // Unsupported format
        let mut m = Manifest::new("id", "Name");
        m.format = 99;
        assert!(m.validate().is_err());
    }

    #[test]
    fn test_manifest_v2_with_atlases_roundtrip() {
        use crate::atlas::{BgaFrame, SoundAtlasCodec, SoundSlice};

        let mut sound_slices = BTreeMap::new();
        sound_slices.insert("01".to_string(), SoundSlice::new(0, 44100, Some("01.wav".to_string())));
        let sound_atlas = SoundAtlasMeta::new("audio/atlas.bin", SoundAtlasCodec::Pcm16, 44100, 2, 44100, 128, sound_slices);

        let mut bga_frames = BTreeMap::new();
        bga_frames.insert("stage".to_string(), BgaFrame::new(0, 0, 640, 480, Some("stage.png".to_string())));
        let bga_atlas = BgaAtlasMeta::new("visual/atlas.png", 1024, 1024, bga_frames);

        let manifest = Manifest::new("turbo.song", "Turbo Song")
            .with_sound_atlas(sound_atlas)
            .with_bga_atlas(bga_atlas);

        assert_eq!(manifest.format, 2);
        let json = manifest.to_json_string().unwrap();
        let parsed = Manifest::from_json_str(&json).unwrap();

        assert_eq!(parsed.format, 2);
        assert!(parsed.sound_atlas.is_some());
        assert!(parsed.bga_atlas.is_some());
        assert_eq!(parsed.sound_atlas.unwrap().file, "audio/atlas.bin");
        assert_eq!(parsed.bga_atlas.unwrap().file, "visual/atlas.png");
    }
}
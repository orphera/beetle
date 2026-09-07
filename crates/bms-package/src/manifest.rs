use crate::atlas::{BgaAtlasMeta, BgaDeltaMeta, SoundAtlasMeta};
use crate::error::PackageError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CURRENT_FORMAT_VERSION: u32 = 2;
pub const MANIFEST_FILENAME: &str = "manifest.json";

/// Type of BMS package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageType {
    /// Standard song package containing charts and audio keysounds.
    #[default]
    Standard,
    /// BGA companion package containing video or visual assets associated with a base package.
    BgaCompanion,
}

impl PackageType {
    pub fn is_standard(&self) -> bool {
        matches!(self, Self::Standard)
    }

    pub fn is_bga_companion(&self) -> bool {
        matches!(self, Self::BgaCompanion)
    }
}

/// Metadata describing a BGA companion package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BgaCompanionInfo {
    /// Whether this companion package is strictly required (defaults to false).
    #[serde(default)]
    pub required: bool,
    /// Recommended filename when downloaded/stored (e.g. `junk_g2r2018_ogg.bga.bmsp`).
    pub recommended_filename: String,
    /// Uncompressed or archive size in bytes.
    pub size_bytes: u64,
    /// SHA-256 hex digest of the companion package.
    pub sha256: String,
}

impl BgaCompanionInfo {
    pub fn new<F: Into<String>, H: Into<String>>(
        recommended_filename: F,
        size_bytes: u64,
        sha256: H,
    ) -> Self {
        Self {
            required: false,
            recommended_filename: recommended_filename.into(),
            size_bytes,
            sha256: sha256.into(),
        }
    }

    pub fn with_required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn validate(&self) -> Result<(), PackageError> {
        if self.recommended_filename.trim().is_empty() {
            return Err(PackageError::InvalidManifest(
                "BGA companion 'recommended_filename' cannot be empty".to_string(),
            ));
        }
        if self.sha256.trim().is_empty() {
            return Err(PackageError::InvalidManifest(
                "BGA companion 'sha256' cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}

/// Collection of companion packages associated with a base package.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionPackages {
    /// Optional BGA companion package information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bga: Option<BgaCompanionInfo>,
}

impl CompanionPackages {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_bga(mut self, bga: BgaCompanionInfo) -> Self {
        self.bga = Some(bga);
        self
    }

    pub fn validate(&self) -> Result<(), PackageError> {
        if let Some(ref bga) = self.bga {
            bga.validate()?;
        }
        Ok(())
    }
}

/// Package metadata and identity structure (`manifest.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Format specification version (currently 1 or 2).
    #[serde(alias = "format_version")]
    pub format: u32,
    /// Stable, persistent package identifier (e.g. `example.song`).
    pub id: String,
    /// Display name of the package.
    pub name: String,
    /// Optional author or creator name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Package type: standard song package or bga companion package.
    #[serde(default, rename = "type")]
    pub package_type: PackageType,
    /// Target package ID for companion packages (when `package_type == PackageType::BgaCompanion`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_package_id: Option<String>,
    /// Optional companion package metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_packages: Option<CompanionPackages>,
    /// Optional Sound Atlas metadata for Turbo profile packages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound_atlas: Option<SoundAtlasMeta>,
    /// Optional BGA Texture Atlas metadata for Turbo profile packages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bga_atlas: Option<BgaAtlasMeta>,
    /// Optional BGA Delta sequence bundle metadata for Turbo profile packages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bga_delta: Option<BgaDeltaMeta>,
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
            package_type: PackageType::Standard,
            target_package_id: None,
            companion_packages: None,
            sound_atlas: None,
            bga_atlas: None,
            bga_delta: None,
            extra: BTreeMap::new(),
        }
    }

    /// Creates a new BGA Companion Manifest for a specified target package.
    pub fn new_bga_companion<I: Into<String>, N: Into<String>, T: Into<String>>(
        id: I,
        name: N,
        target_package_id: T,
    ) -> Self {
        Self {
            format: CURRENT_FORMAT_VERSION,
            id: id.into(),
            name: name.into(),
            author: None,
            package_type: PackageType::BgaCompanion,
            target_package_id: Some(target_package_id.into()),
            companion_packages: None,
            sound_atlas: None,
            bga_atlas: None,
            bga_delta: None,
            extra: BTreeMap::new(),
        }
    }

    /// Builder method to set package type.
    pub fn with_package_type(mut self, package_type: PackageType) -> Self {
        self.package_type = package_type;
        self
    }

    /// Builder method to attach target package ID.
    pub fn with_target_package_id<T: Into<String>>(mut self, target_package_id: T) -> Self {
        self.target_package_id = Some(target_package_id.into());
        self
    }

    /// Builder method to attach companion packages metadata.
    pub fn with_companion_packages(mut self, companion_packages: CompanionPackages) -> Self {
        self.companion_packages = Some(companion_packages);
        self
    }

    /// Builder method to attach a BGA companion info directly.
    pub fn with_bga_companion(mut self, bga_companion: BgaCompanionInfo) -> Self {
        let mut comps = self.companion_packages.unwrap_or_default();
        comps.bga = Some(bga_companion);
        self.companion_packages = Some(comps);
        self
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

    /// Builder method to attach a BGA delta sequence bundle.
    pub fn with_bga_delta(mut self, bga_delta: BgaDeltaMeta) -> Self {
        self.bga_delta = Some(bga_delta);
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
            return Err(PackageError::InvalidManifest(
                "Field 'id' cannot be empty".to_string(),
            ));
        }

        // 3. Name
        if self.name.trim().is_empty() {
            return Err(PackageError::InvalidManifest(
                "Field 'name' cannot be empty".to_string(),
            ));
        }

        // 4. Package type and target_package_id
        match self.package_type {
            PackageType::BgaCompanion => {
                if let Some(ref target) = self.target_package_id {
                    if target.trim().is_empty() {
                        return Err(PackageError::InvalidManifest(
                            "Field 'target_package_id' cannot be empty for BGA companion package"
                                .to_string(),
                        ));
                    }
                } else {
                    return Err(PackageError::InvalidManifest(
                        "Field 'target_package_id' is required for BGA companion package"
                            .to_string(),
                    ));
                }
            }
            PackageType::Standard => {}
        }

        // 5. Companion packages validation if present
        if let Some(ref companions) = self.companion_packages {
            companions.validate()?;
        }

        // 6. Sound Atlas validation if present
        if let Some(ref sa) = self.sound_atlas {
            sa.validate()
                .map_err(|e| PackageError::InvalidManifest(format!("Invalid sound_atlas: {e}")))?;
        }

        // 7. BGA Atlas validation if present
        if let Some(ref ba) = self.bga_atlas {
            ba.validate()
                .map_err(|e| PackageError::InvalidManifest(format!("Invalid bga_atlas: {e}")))?;
        }

        // 8. BGA Delta validation if present
        if let Some(ref bd) = self.bga_delta {
            bd.validate()
                .map_err(|e| PackageError::InvalidManifest(format!("Invalid bga_delta: {e}")))?;
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
        sound_slices.insert(
            "01".to_string(),
            SoundSlice::new(0, 44100, Some("01.wav".to_string())),
        );
        let sound_atlas = SoundAtlasMeta::new(
            "audio/atlas.bin",
            SoundAtlasCodec::Pcm16,
            44100,
            2,
            44100,
            128,
            sound_slices,
        );

        let mut bga_frames = BTreeMap::new();
        bga_frames.insert(
            "stage".to_string(),
            BgaFrame::new(0, 0, 640, 480, Some("stage.png".to_string())),
        );
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

    #[test]
    fn test_bga_companion_manifest_roundtrip_and_validation() {
        let bga_manifest = Manifest::new_bga_companion(
            "junk_g2r2018_ogg_bga",
            "Junk G2R 2018 (BGA Pack)",
            "junk_g2r2018_ogg",
        );

        assert_eq!(bga_manifest.package_type, PackageType::BgaCompanion);
        assert_eq!(
            bga_manifest.target_package_id.as_deref(),
            Some("junk_g2r2018_ogg")
        );

        let json = bga_manifest.to_json_string().unwrap();
        assert!(json.contains("\"type\": \"bga_companion\""));
        assert!(json.contains("\"target_package_id\": \"junk_g2r2018_ogg\""));

        let parsed = Manifest::from_json_str(&json).unwrap();
        assert_eq!(parsed.package_type, PackageType::BgaCompanion);
        assert_eq!(
            parsed.target_package_id.as_deref(),
            Some("junk_g2r2018_ogg")
        );

        // Invalid: missing target_package_id for bga_companion
        let mut invalid_bga =
            Manifest::new("id", "name").with_package_type(PackageType::BgaCompanion);
        assert!(invalid_bga.validate().is_err());

        // Invalid: empty target_package_id
        invalid_bga.target_package_id = Some("  ".to_string());
        assert!(invalid_bga.validate().is_err());
    }

    #[test]
    fn test_manifest_with_companion_packages_roundtrip() {
        let comp_info = BgaCompanionInfo::new(
            "junk.bga.bmsp",
            92_460_000,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        );
        let manifest =
            Manifest::new("junk_g2r2018_ogg", "Junk G2R 2018").with_bga_companion(comp_info);

        let json = manifest.to_json_string().unwrap();
        assert!(json.contains("\"companion_packages\""));
        assert!(json.contains("\"recommended_filename\": \"junk.bga.bmsp\""));

        let parsed = Manifest::from_json_str(&json).unwrap();
        assert_eq!(parsed.package_type, PackageType::Standard);
        let bga_comp = parsed
            .companion_packages
            .as_ref()
            .unwrap()
            .bga
            .as_ref()
            .unwrap();
        assert_eq!(bga_comp.recommended_filename, "junk.bga.bmsp");
        assert_eq!(bga_comp.size_bytes, 92_460_000);
        assert_eq!(
            bga_comp.sha256,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert!(!bga_comp.required);
    }

    #[test]
    fn test_backward_compatibility_v1_and_v2_legacy_json() {
        // Old JSON without 'type' or with 'format_version'
        let legacy_json = r#"{
            "format": 2,
            "id": "legacy.song",
            "name": "Legacy Song",
            "author": "Composer"
        }"#;

        let parsed = Manifest::from_json_str(legacy_json).unwrap();
        assert_eq!(parsed.package_type, PackageType::Standard);
        assert!(parsed.target_package_id.is_none());
        assert!(parsed.companion_packages.is_none());

        // With format_version alias
        let format_version_json = r#"{
            "format_version": 2,
            "id": "legacy2.song",
            "name": "Legacy 2"
        }"#;
        let parsed2 = Manifest::from_json_str(format_version_json).unwrap();
        assert_eq!(parsed2.format, 2);
    }
}

use crate::error::PackageError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CURRENT_FORMAT_VERSION: u32 = 1;
pub const MANIFEST_FILENAME: &str = "manifest.json";

/// Package metadata and identity structure (`manifest.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Format specification version (currently 1).
    pub format: u32,
    /// Stable, persistent package identifier (e.g. `example.song`).
    pub id: String,
    /// Semantic version string (e.g. `1.0.0`, `1.2.0-beta.1`).
    pub version: String,
    /// Display name of the package.
    pub name: String,
    /// Optional author or creator name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Additional optional fields preserved for forward-compatibility.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Manifest {
    /// Creates a new minimal Manifest with the current format version.
    pub fn new<I: Into<String>, V: Into<String>, N: Into<String>>(
        id: I,
        version: V,
        name: N,
    ) -> Self {
        Self {
            format: CURRENT_FORMAT_VERSION,
            id: id.into(),
            version: version.into(),
            name: name.into(),
            author: None,
            extra: BTreeMap::new(),
        }
    }

    /// Builder method to attach an author.
    pub fn with_author<A: Into<String>>(mut self, author: A) -> Self {
        self.author = Some(author.into());
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

        // 4. Version (basic SemVer validation)
        if self.version.trim().is_empty() {
            return Err(PackageError::InvalidManifest("Field 'version' cannot be empty".to_string()));
        }

        if !is_valid_semver(&self.version) {
            return Err(PackageError::InvalidManifest(format!(
                "Field 'version' ('{}') is not a valid SemVer format (MAJOR.MINOR.PATCH)",
                self.version
            )));
        }

        Ok(())
    }
}

/// Simple, robust SemVer string verification (X.Y.Z[-prerelease][+build]).
fn is_valid_semver(ver: &str) -> bool {
    let mut parts = ver.splitn(2, |c| c == '-' || c == '+');
    let core = parts.next().unwrap_or("");
    let numbers: Vec<&str> = core.split('.').collect();
    if numbers.len() != 3 {
        return false;
    }
    numbers.iter().all(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_serialization_and_validation() {
        let manifest = Manifest::new("test.song", "1.0.0", "Test Song")
            .with_author("Beetle Dev")
            .with_extra("license", serde_json::json!("MIT"));

        let json = manifest.to_json_string().unwrap();
        let parsed = Manifest::from_json_str(&json).unwrap();

        assert_eq!(manifest.id, parsed.id);
        assert_eq!(manifest.version, parsed.version);
        assert_eq!(manifest.name, parsed.name);
        assert_eq!(manifest.author, Some("Beetle Dev".to_string()));
        assert_eq!(parsed.extra.get("license"), Some(&serde_json::json!("MIT")));
    }

    #[test]
    fn test_invalid_manifest_rejection() {
        // Empty id
        let m = Manifest::new("", "1.0.0", "Name");
        assert!(m.validate().is_err());

        // Empty name
        let m = Manifest::new("id", "1.0.0", "");
        assert!(m.validate().is_err());

        // Invalid version
        let m = Manifest::new("id", "invalid_ver", "Name");
        assert!(m.validate().is_err());

        // Unsupported format
        let mut m = Manifest::new("id", "1.0.0", "Name");
        m.format = 99;
        assert!(m.validate().is_err());
    }

    #[test]
    fn test_semver_formats() {
        assert!(is_valid_semver("1.0.0"));
        assert!(is_valid_semver("0.1.2-alpha.1"));
        assert!(is_valid_semver("2.3.4+build.2026"));
        assert!(!is_valid_semver("1.0"));
        assert!(!is_valid_semver("v1.0.0"));
        assert!(!is_valid_semver("1.a.0"));
    }
}

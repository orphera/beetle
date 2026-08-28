use serde::{Deserialize, Serialize};

use crate::error::PackageError;
use crate::manifest::Manifest;
use crate::path::validate_entry_path;

pub const CURRENT_DELTA_FORMAT_VERSION: u32 = 1;
pub const DELTA_MANIFEST_FILENAME: &str = "delta_manifest.json";

/// The operation applied to a specific resource path in a delta transformation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeltaOpKind {
    /// Resource is newly introduced in target state (payload present in delta archive).
    Added,
    /// Resource content changed in target state (payload present in delta archive).
    Modified,
    /// Resource was deleted in target state (no payload in delta archive).
    Removed,
    /// Resource is identical in target state (carried forward from base state).
    Unchanged,
}

/// Description of a single resource's delta operation and target integrity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeltaResourceEntry {
    pub path: String,
    pub op: DeltaOpKind,
    pub sha256: String,
    pub size_bytes: u64,
}

/// Manifest describing a transformation from Base State to Target State.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeltaManifest {
    pub format: u32,
    pub package_id: String,
    pub base_hash: String,
    pub target_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_package_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_package_sha256: Option<String>,
    pub target_manifest: Manifest,
    pub resources: Vec<DeltaResourceEntry>,
}

impl DeltaManifest {
    pub fn new(
        package_id: impl Into<String>,
        base_hash: impl Into<String>,
        target_hash: impl Into<String>,
        target_manifest: Manifest,
    ) -> Self {
        Self {
            format: CURRENT_DELTA_FORMAT_VERSION,
            package_id: package_id.into(),
            base_hash: base_hash.into(),
            target_hash: target_hash.into(),
            base_package_sha256: None,
            target_package_sha256: None,
            target_manifest,
            resources: Vec::new(),
        }
    }

    /// Validates the structure, paths, and hashes of this delta manifest.
    pub fn validate(&self) -> Result<(), PackageError> {
        if self.format != CURRENT_DELTA_FORMAT_VERSION {
            return Err(PackageError::UnsupportedFormat(self.format));
        }

        if self.package_id.trim().is_empty() {
            return Err(PackageError::InvalidDeltaManifest("Empty package_id".to_string()));
        }

        if self.base_hash.trim().is_empty() || self.target_hash.trim().is_empty() {
            return Err(PackageError::InvalidDeltaManifest("Empty base or target hash".to_string()));
        }

        if self.base_hash == self.target_hash {
            return Err(PackageError::InvalidDeltaManifest(
                "Base hash and target hash cannot be identical in delta".to_string(),
            ));
        }

        if self.package_id != self.target_manifest.id {
            return Err(PackageError::InvalidDeltaManifest(format!(
                "Delta package_id '{}' does not match target manifest id '{}'",
                self.package_id, self.target_manifest.id
            )));
        }

        self.target_manifest.validate()?;

        for entry in &self.resources {
            validate_entry_path(&entry.path)?;
            if entry.sha256.trim().len() != 64 {
                return Err(PackageError::InvalidDeltaManifest(format!(
                    "Invalid SHA-256 for entry '{}'",
                    entry.path
                )));
            }
        }

        Ok(())
    }

    /// Serializes to normalized JSON bytes.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, PackageError> {
        serde_json::to_vec_pretty(self).map_err(|e| PackageError::InvalidDeltaManifest(e.to_string()))
    }

    /// Parses from JSON bytes.
    pub fn from_json_slice(bytes: &[u8]) -> Result<Self, PackageError> {
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|e| PackageError::InvalidDeltaManifest(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Manifest;

    #[test]
    fn test_delta_manifest_roundtrip() {
        let target_man = Manifest::new("com.example.song", "Example Song");
        let mut delta = DeltaManifest::new(
            "com.example.song",
            "a3f8c2d1...",
            "7b1d0e9f...",
            target_man,
        );
        delta.resources.push(DeltaResourceEntry {
            path: "bms/insane.bms".to_string(),
            op: DeltaOpKind::Added,
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            size_bytes: 1024,
        });

        delta.validate().unwrap();
        let bytes = delta.to_json_bytes().unwrap();
        let parsed = DeltaManifest::from_json_slice(&bytes).unwrap();
        assert_eq!(delta, parsed);
    }
}
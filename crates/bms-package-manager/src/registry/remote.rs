use serde::{Deserialize, Serialize};

pub const CURRENT_REMOTE_INDEX_FORMAT_VERSION: &str = "1.0.0";

/// Remote package registry index hosted on a static web server or CDN.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteRegistryIndex {
    pub format_version: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub url: String,
    pub updated_at: String,
    #[serde(default)]
    pub packages: Vec<RemotePackageMetadata>,
}

impl RemoteRegistryIndex {
    pub fn new(
        name: impl Into<String>,
        url: impl Into<String>,
        updated_at: impl Into<String>,
    ) -> Self {
        Self {
            format_version: CURRENT_REMOTE_INDEX_FORMAT_VERSION.to_string(),
            name: name.into(),
            description: None,
            url: url.into(),
            updated_at: updated_at.into(),
            packages: Vec::new(),
        }
    }

    /// Parses remote index from JSON string and validates basic integrity.
    pub fn from_json_str(json: &str) -> Result<Self, String> {
        let index: Self = serde_json::from_str(json)
            .map_err(|e| format!("Failed to parse RemoteRegistryIndex JSON: {e}"))?;
        index.validate()?;
        Ok(index)
    }

    /// Serializes to compact JSON string.
    pub fn to_json_str(&self) -> Result<String, String> {
        serde_json::to_string(self)
            .map_err(|e| format!("Failed to serialize RemoteRegistryIndex: {e}"))
    }

    /// Serializes to pretty-printed JSON string.
    pub fn to_json_pretty(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize RemoteRegistryIndex: {e}"))
    }

    /// Validates format version and package checksum formats.
    pub fn validate(&self) -> Result<(), String> {
        if self.format_version.is_empty() {
            return Err("RemoteRegistryIndex 'format_version' cannot be empty".to_string());
        }
        if self.name.is_empty() {
            return Err("RemoteRegistryIndex 'name' cannot be empty".to_string());
        }
        if self.url.is_empty() {
            return Err("RemoteRegistryIndex 'url' cannot be empty".to_string());
        }

        for pkg in &self.packages {
            pkg.validate()?;
        }
        Ok(())
    }

    /// Finds package metadata by exact package ID.
    pub fn find_package(&self, id: &str) -> Option<&RemotePackageMetadata> {
        self.packages.iter().find(|p| p.id.eq_ignore_ascii_case(id))
    }

    /// Searches packages matching query in title, artist, or genre (case-insensitive).
    pub fn search(&self, query: &str) -> Vec<&RemotePackageMetadata> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return self.packages.iter().collect();
        }

        self.packages
            .iter()
            .filter(|p| {
                p.id.to_lowercase().contains(&q)
                    || p.title.to_lowercase().contains(&q)
                    || p.artist.to_lowercase().contains(&q)
                    || p.genre
                        .as_deref()
                        .map(|g| g.to_lowercase().contains(&q))
                        .unwrap_or(false)
            })
            .collect()
    }
}

/// Metadata for a single package hosted in the remote registry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemotePackageMetadata {
    pub id: String,
    pub version: String,
    pub state_hash: String,
    pub title: String,
    pub artist: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bpm: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub play_levels: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keysounds_count: Option<u32>,
    pub size_bytes: u64,
    pub sha256: String,
    pub download_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_audio_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner_image_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_bga: Option<CompanionBgaMetadata>,
}

impl RemotePackageMetadata {
    /// Validates required fields, SHA-256 format (64-char hex), and positive size.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err("Package id cannot be empty".to_string());
        }
        if self.version.is_empty() {
            return Err(format!("Package '{}' version cannot be empty", self.id));
        }
        if self.title.is_empty() {
            return Err(format!("Package '{}' title cannot be empty", self.id));
        }
        if self.artist.is_empty() {
            return Err(format!("Package '{}' artist cannot be empty", self.id));
        }
        if self.download_url.is_empty() {
            return Err(format!(
                "Package '{}' download_url cannot be empty",
                self.id
            ));
        }
        if self.size_bytes == 0 {
            return Err(format!(
                "Package '{}' size_bytes must be greater than 0",
                self.id
            ));
        }
        if self.sha256.len() != 64 || !self.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!(
                "Package '{}' sha256 must be a 64-character lowercase hex string, found '{}'",
                self.id, self.sha256
            ));
        }

        if let Some(bga) = &self.companion_bga {
            bga.validate()?;
        }

        Ok(())
    }

    /// Resolves an absolute download URL using the registry base URL if relative.
    pub fn resolve_download_url(&self, base_registry_url: &str) -> String {
        resolve_url(base_registry_url, &self.download_url)
    }

    /// Resolves an absolute preview audio URL using the registry base URL if relative.
    pub fn resolve_preview_url(&self, base_registry_url: &str) -> Option<String> {
        self.preview_audio_url
            .as_ref()
            .map(|u| resolve_url(base_registry_url, u))
    }

    /// Resolves an absolute banner image URL using the registry base URL if relative.
    pub fn resolve_banner_url(&self, base_registry_url: &str) -> Option<String> {
        self.banner_image_url
            .as_ref()
            .map(|u| resolve_url(base_registry_url, u))
    }
}

/// Metadata for an optional decoupled large BGA companion package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionBgaMetadata {
    pub id: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub download_url: String,
}

impl CompanionBgaMetadata {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err("Companion BGA id cannot be empty".to_string());
        }
        if self.download_url.is_empty() {
            return Err("Companion BGA download_url cannot be empty".to_string());
        }
        if self.size_bytes == 0 {
            return Err("Companion BGA size_bytes must be greater than 0".to_string());
        }
        if self.sha256.len() != 64 || !self.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!(
                "Companion BGA sha256 must be a 64-character hex string, found '{}'",
                self.sha256
            ));
        }
        Ok(())
    }
}

/// Helper to resolve relative URL against a base URL.
pub fn resolve_url(base: &str, relative_or_absolute: &str) -> String {
    if relative_or_absolute.starts_with("http://") || relative_or_absolute.starts_with("https://") {
        return relative_or_absolute.to_string();
    }

    let base_trimmed = base.trim_end_matches('/');
    let path_trimmed = relative_or_absolute.trim_start_matches('/');
    format!("{base_trimmed}/{path_trimmed}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_registry_index_roundtrip() {
        let mut index = RemoteRegistryIndex::new(
            "Beetle Official",
            "https://packages.beetle-engine.org",
            "2026-09-14T00:00:00Z",
        );
        index.description = Some("Official BMS packages".to_string());

        let pkg = RemotePackageMetadata {
            id: "conflict".to_string(),
            version: "1.0.0".to_string(),
            state_hash: "a3f8c2d1e4b5".to_string(),
            title: "Conflict".to_string(),
            artist: "siqlo + cranky".to_string(),
            genre: Some("HARMONIC HARDCORE".to_string()),
            bpm: Some(160.0),
            play_levels: vec![5, 9, 11],
            keysounds_count: Some(480),
            size_bytes: 14500000,
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            download_url: "conflict-1.0.0.bmsp".to_string(),
            preview_audio_url: Some("conflict-preview.ogg".to_string()),
            banner_image_url: Some("conflict-banner.bmp".to_string()),
            companion_bga: Some(CompanionBgaMetadata {
                id: "conflict-bga".to_string(),
                size_bytes: 45000000,
                sha256: "8f4a124298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                    .to_string(),
                download_url: "conflict-bga-1.0.0.bmsp".to_string(),
            }),
        };
        index.packages.push(pkg);

        let json = index.to_json_pretty().expect("must serialize");
        let parsed = RemoteRegistryIndex::from_json_str(&json).expect("must parse");

        assert_eq!(parsed.name, "Beetle Official");
        assert_eq!(parsed.packages.len(), 1);
        let p = &parsed.packages[0];
        assert_eq!(p.id, "conflict");
        assert_eq!(
            p.resolve_download_url(&parsed.url),
            "https://packages.beetle-engine.org/conflict-1.0.0.bmsp"
        );
        assert_eq!(
            p.resolve_preview_url(&parsed.url).unwrap(),
            "https://packages.beetle-engine.org/conflict-preview.ogg"
        );
    }

    #[test]
    fn test_search_and_find_package() {
        let mut index = RemoteRegistryIndex::new(
            "Test Registry",
            "https://test.example.com",
            "2026-09-14T00:00:00Z",
        );
        index.packages.push(RemotePackageMetadata {
            id: "halcyon".to_string(),
            version: "1.0.0".to_string(),
            state_hash: "112233".to_string(),
            title: "Halcyon".to_string(),
            artist: "xi".to_string(),
            genre: Some("Renaissance Hardcore".to_string()),
            bpm: Some(191.0),
            play_levels: vec![12],
            keysounds_count: Some(1200),
            size_bytes: 25000000,
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            download_url: "https://test.example.com/halcyon.bmsp".to_string(),
            preview_audio_url: None,
            banner_image_url: None,
            companion_bga: None,
        });

        assert!(index.find_package("halcyon").is_some());
        assert!(index.find_package("HALCYON").is_some());
        assert!(index.find_package("nonexistent").is_none());

        let results = index.search("xi");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Halcyon");

        let results = index.search("renaissance");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_invalid_package_validation() {
        let mut index = RemoteRegistryIndex::new("Test", "http://test", "2026-09-14T00:00:00Z");
        index.packages.push(RemotePackageMetadata {
            id: "invalid-sha".to_string(),
            version: "1.0.0".to_string(),
            state_hash: "112233".to_string(),
            title: "Bad".to_string(),
            artist: "None".to_string(),
            genre: None,
            bpm: None,
            play_levels: vec![],
            keysounds_count: None,
            size_bytes: 100,
            sha256: "too_short".to_string(),
            download_url: "bad.bmsp".to_string(),
            preview_audio_url: None,
            banner_image_url: None,
            companion_bga: None,
        });

        assert!(index.validate().is_err());
    }
}

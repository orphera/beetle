use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::remote::{RemotePackageMetadata, RemoteRegistryIndex};

pub const DEFAULT_OFFICIAL_SOURCE_ID: &str = "official";
pub const DEFAULT_OFFICIAL_SOURCE_NAME: &str = "Beetle Official Registry";
pub const DEFAULT_OFFICIAL_SOURCE_URL: &str = "https://packages.beetle-engine.org/index.json";

/// Configuration for a remote registry source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistrySource {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_priority")]
    pub priority: u32,
}

fn default_enabled() -> bool {
    true
}

fn default_priority() -> u32 {
    100
}

impl RegistrySource {
    pub fn new(id: impl Into<String>, name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            url: url.into(),
            enabled: true,
            priority: 100,
        }
    }

    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }
}

/// Collection of configured remote registry sources (saved in sources.json).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcesConfig {
    #[serde(default)]
    pub sources: Vec<RegistrySource>,
}

impl Default for SourcesConfig {
    fn default() -> Self {
        Self {
            sources: vec![RegistrySource::new(
                DEFAULT_OFFICIAL_SOURCE_ID,
                DEFAULT_OFFICIAL_SOURCE_NAME,
                DEFAULT_OFFICIAL_SOURCE_URL,
            )
            .with_priority(100)],
        }
    }
}

impl SourcesConfig {
    /// Loads sources configuration from file or initializes default.
    pub fn load_or_init(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            let default_cfg = Self::default();
            default_cfg.save_to_file(path)?;
            return Ok(default_cfg);
        }

        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read sources file '{}': {e}", path.display()))?;
        let cfg: Self = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse sources file '{}': {e}", path.display()))?;
        Ok(cfg)
    }

    /// Saves sources configuration to file.
    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create parent directory for sources '{}': {e}",
                    parent.display()
                )
            })?;
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize sources configuration: {e}"))?;
        fs::write(path, json)
            .map_err(|e| format!("Failed to write sources file '{}': {e}", path.display()))
    }

    /// Adds or updates a registry source.
    pub fn add_or_update(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        url: impl Into<String>,
        priority: u32,
    ) {
        let id_str = id.into();
        if let Some(existing) = self
            .sources
            .iter_mut()
            .find(|s| s.id.eq_ignore_ascii_case(&id_str))
        {
            existing.name = name.into();
            existing.url = url.into();
            existing.priority = priority;
            existing.enabled = true;
        } else {
            self.sources.push(RegistrySource {
                id: id_str,
                name: name.into(),
                url: url.into(),
                enabled: true,
                priority,
            });
        }
    }

    /// Removes a registry source by ID. Returns true if removed.
    pub fn remove(&mut self, id: &str) -> bool {
        let initial_len = self.sources.len();
        self.sources.retain(|s| !s.id.eq_ignore_ascii_case(id));
        self.sources.len() < initial_len
    }

    /// Finds a source by ID.
    pub fn find(&self, id: &str) -> Option<&RegistrySource> {
        self.sources.iter().find(|s| s.id.eq_ignore_ascii_case(id))
    }

    /// Returns active enabled sources sorted by priority descending.
    pub fn active_sources_by_priority(&self) -> Vec<&RegistrySource> {
        let mut active: Vec<&RegistrySource> = self.sources.iter().filter(|s| s.enabled).collect();
        active.sort_by_key(|b| std::cmp::Reverse(b.priority));
        active
    }

    /// Merges packages from multiple remote index snapshots.
    /// When the same package ID exists in multiple registries, the higher priority source wins.
    pub fn merge_packages(
        indexes: &[(&RegistrySource, &RemoteRegistryIndex)],
    ) -> Vec<RemotePackageMetadata> {
        // Sort pairs by source priority descending
        let mut sorted = indexes.to_vec();
        sorted.sort_by_key(|b| std::cmp::Reverse(b.0.priority));

        let mut seen_packages: HashMap<String, RemotePackageMetadata> = HashMap::new();

        for (source, index) in sorted {
            if !source.enabled {
                continue;
            }
            for pkg in &index.packages {
                let key = pkg.id.to_lowercase();
                seen_packages.entry(key).or_insert_with(|| {
                    let mut resolved = pkg.clone();
                    // Ensure download URLs are resolved against the source/index URL if relative
                    resolved.download_url = pkg.resolve_download_url(&index.url);
                    if let Some(preview) = pkg.resolve_preview_url(&index.url) {
                        resolved.preview_audio_url = Some(preview);
                    }
                    if let Some(banner) = pkg.resolve_banner_url(&index.url) {
                        resolved.banner_image_url = Some(banner);
                    }
                    resolved
                });
            }
        }

        let mut list: Vec<RemotePackageMetadata> = seen_packages.into_values().collect();
        list.sort_by_key(|a| a.title.to_lowercase());
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sources_config_defaults_and_mutation() {
        let mut config = SourcesConfig::default();
        assert_eq!(config.sources.len(), 1);
        assert_eq!(config.sources[0].id, "official");

        config.add_or_update(
            "community",
            "Community Hub",
            "https://bms.org/index.json",
            150,
        );
        assert_eq!(config.sources.len(), 2);

        let active = config.active_sources_by_priority();
        assert_eq!(active[0].id, "community"); // priority 150 > 100
        assert_eq!(active[1].id, "official");

        assert!(config.remove("official"));
        assert_eq!(config.sources.len(), 1);
        assert!(!config.remove("nonexistent"));
    }

    #[test]
    fn test_merge_packages_priority_deduplication() {
        let official_src =
            RegistrySource::new("official", "Official", "https://packages.org").with_priority(50);
        let community_src = RegistrySource::new("community", "Community", "https://community.org")
            .with_priority(100);

        let mut official_index =
            RemoteRegistryIndex::new("Official", "https://packages.org", "2026-09-14");
        official_index.packages.push(RemotePackageMetadata {
            id: "conflict".to_string(),
            version: "1.0.0".to_string(),
            state_hash: "official_hash".to_string(),
            title: "Conflict (Official)".to_string(),
            artist: "cranky".to_string(),
            genre: None,
            bpm: None,
            play_levels: vec![10],
            keysounds_count: None,
            size_bytes: 1000,
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            download_url: "conflict.bmsp".to_string(),
            preview_audio_url: None,
            banner_image_url: None,
            companion_bga: None,
        });

        let mut community_index =
            RemoteRegistryIndex::new("Community", "https://community.org", "2026-09-14");
        community_index.packages.push(RemotePackageMetadata {
            id: "conflict".to_string(),
            version: "1.1.0".to_string(),
            state_hash: "community_hash".to_string(),
            title: "Conflict (Community Remaster)".to_string(),
            artist: "cranky".to_string(),
            genre: None,
            bpm: None,
            play_levels: vec![11],
            keysounds_count: None,
            size_bytes: 1200,
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            download_url: "conflict-remaster.bmsp".to_string(),
            preview_audio_url: None,
            banner_image_url: None,
            companion_bga: None,
        });

        let merged = SourcesConfig::merge_packages(&[
            (&official_src, &official_index),
            (&community_src, &community_index),
        ]);

        assert_eq!(merged.len(), 1);
        // community priority (100) > official priority (50)
        assert_eq!(merged[0].title, "Conflict (Community Remaster)");
        assert_eq!(
            merged[0].download_url,
            "https://community.org/conflict-remaster.bmsp"
        );
    }
}

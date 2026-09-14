use std::fs;
use std::path::{Path, PathBuf};

use crate::net::http::HttpClient;
use crate::registry::remote::RemoteRegistryIndex;
use crate::registry::sources::RegistrySource;

/// Manages local disk caching of remote registry index snapshots.
#[derive(Debug, Clone)]
pub struct RegistryCacheManager {
    cache_dir: PathBuf,
}

impl RegistryCacheManager {
    pub fn new(base_dir: &Path) -> Self {
        Self {
            cache_dir: base_dir.join(".cache").join("registry"),
        }
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Gets cache file path for a specific source ID.
    pub fn cache_path_for_source(&self, source_id: &str) -> PathBuf {
        let safe_id = source_id.replace(['/', '\\', ':', '.'], "_");
        self.cache_dir.join(format!("{safe_id}.json"))
    }

    /// Saves a remote registry index to local cache file.
    pub fn save_index(
        &self,
        source_id: &str,
        index: &RemoteRegistryIndex,
    ) -> Result<PathBuf, String> {
        fs::create_dir_all(&self.cache_dir).map_err(|e| {
            format!(
                "Failed to create registry cache directory '{}': {e}",
                self.cache_dir.display()
            )
        })?;

        let path = self.cache_path_for_source(source_id);
        let json = index.to_json_pretty()?;
        fs::write(&path, json).map_err(|e| {
            format!(
                "Failed to write registry cache file '{}': {e}",
                path.display()
            )
        })?;
        Ok(path)
    }

    /// Loads a cached remote registry index from local disk.
    pub fn load_index(&self, source_id: &str) -> Result<Option<RemoteRegistryIndex>, String> {
        let path = self.cache_path_for_source(source_id);
        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path).map_err(|e| {
            format!(
                "Failed to read cached registry file '{}': {e}",
                path.display()
            )
        })?;

        let index = RemoteRegistryIndex::from_json_str(&content)?;
        Ok(Some(index))
    }

    /// Updates registry index from remote URL.
    /// If network request fails, automatically and silently falls back to local cache (Offline Resilience).
    pub fn update_or_fallback(
        &self,
        client: &HttpClient,
        source: &RegistrySource,
    ) -> Result<(RemoteRegistryIndex, bool), String> {
        match client.fetch_index(&source.url) {
            Ok(remote_index) => {
                let _ = self.save_index(&source.id, &remote_index);
                Ok((remote_index, false)) // false = fresh from network
            }
            Err(network_err) => {
                // Fallback to local cache
                if let Ok(Some(cached)) = self.load_index(&source.id) {
                    Ok((cached, true)) // true = fallback from cache
                } else {
                    Err(format!(
                        "Failed to fetch remote registry '{}' from '{}' and no local cache available: {network_err}",
                        source.id, source.url
                    ))
                }
            }
        }
    }

    /// Loads all available cached indexes for the given active sources.
    pub fn load_all_cached(&self, sources: &[&RegistrySource]) -> Vec<(RegistrySource, RemoteRegistryIndex)> {
        let mut results = Vec::new();
        for &source in sources {
            if !source.enabled {
                continue;
            }
            if let Ok(Some(idx)) = self.load_index(&source.id) {
                results.push((source.clone(), idx));
            }
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_cache_save_and_load() {
        let temp_dir = std::env::temp_dir().join(format!(
            "bpm_cache_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let cache = RegistryCacheManager::new(&temp_dir);

        let mut index = RemoteRegistryIndex::new("Test", "http://test.org", "2026-09-14");
        index.packages.push(crate::registry::remote::RemotePackageMetadata {
            id: "test-song".to_string(),
            version: "1.0.0".to_string(),
            state_hash: "abcdef".to_string(),
            title: "Test Song".to_string(),
            artist: "Tester".to_string(),
            genre: None,
            bpm: None,
            play_levels: vec![5],
            keysounds_count: None,
            size_bytes: 500,
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            download_url: "song.bmsp".to_string(),
            preview_audio_url: None,
            banner_image_url: None,
            companion_bga: None,
        });

        // 1. Initially not cached
        assert!(cache.load_index("official").unwrap().is_none());

        // 2. Save
        cache.save_index("official", &index).unwrap();

        // 3. Load
        let loaded = cache.load_index("official").unwrap().unwrap();
        assert_eq!(loaded.name, "Test");
        assert_eq!(loaded.packages.len(), 1);
        assert_eq!(loaded.packages[0].id, "test-song");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}

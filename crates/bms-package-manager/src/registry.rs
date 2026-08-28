use crate::error::PackageManagerError;
use bms_package::Manifest;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Metadata record for a specific installed state of a package.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PackageStateRecord {
    pub path: String,
    pub installed_at: String,
}

/// Metadata record for an installed package grouping all its installed states.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PackageRecord {
    pub id: String,
    pub name: String,
    pub author: Option<String>,
    pub active_state: String,
    pub state_hashes: BTreeMap<String, PackageStateRecord>,
}

impl PackageRecord {
    /// Checks if the package has the given state installed.
    pub fn has_state(&self, state_hash: &str) -> bool {
        self.state_hashes.contains_key(state_hash)
    }
}

/// The recommended storage format for the registry.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Registry {
    pub packages: BTreeMap<String, PackageRecord>,
}

pub const REGISTRY_FILENAME: &str = "registry.json";

impl Registry {
    /// Loads the registry from a file, or returns default empty registry if file does not exist.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, PackageManagerError> {
        let p = path.as_ref();
        if !p.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(p)?;
        let registry: Self = serde_json::from_str(&content)
            .map_err(|e| PackageManagerError::RegistryError(e.to_string()))?;
        Ok(registry)
    }

    /// Atomically saves the registry to a file as formatted JSON.
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), PackageManagerError> {
        let p = path.as_ref();
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| PackageManagerError::RegistryError(e.to_string()))?;
        let tmp_path = p.with_extension("tmp");
        fs::write(&tmp_path, content)?;
        fs::rename(tmp_path, p)?;
        Ok(())
    }

    /// Registers a new installed package state and sets it as the active state.
    pub fn register(
        &mut self,
        manifest: &Manifest,
        state_hash: &str,
        rel_path: &str,
        installed_at: &str,
    ) -> Result<(), PackageManagerError> {
        let package_id = manifest.id.clone();
        let package_name = manifest.name.clone();
        let package_author = manifest.author.clone();

        let entry = self.packages.entry(package_id.clone()).or_insert_with(|| PackageRecord {
            id: package_id,
            name: package_name.clone(),
            author: package_author.clone(),
            active_state: state_hash.to_string(),
            state_hashes: BTreeMap::new(),
        });

        entry.name = package_name;
        entry.author = package_author;
        entry.active_state = state_hash.to_string();

        entry.state_hashes.insert(
            state_hash.to_string(),
            PackageStateRecord {
                path: rel_path.to_string(),
                installed_at: installed_at.to_string(),
            },
        );

        Ok(())
    }

    /// Unregisters an installed package state. If no states remain, removes the package record completely.
    pub fn unregister(&mut self, package_id: &str, state_hash: &str) -> Result<bool, PackageManagerError> {
        let pkg = self
            .packages
            .get_mut(package_id)
            .ok_or_else(|| PackageManagerError::PackageNotFound(package_id.to_string()))?;

        if pkg.state_hashes.remove(state_hash).is_none() {
            return Err(PackageManagerError::StateNotFound {
                id: package_id.to_string(),
                state_hash: state_hash.to_string(),
            });
        }

        if pkg.state_hashes.is_empty() {
            self.packages.remove(package_id);
            return Ok(true);
        }

        // If the removed state was the active state, choose the latest remaining state
        if pkg.active_state == state_hash {
            if let Some(last_state) = pkg.state_hashes.keys().last() {
                pkg.active_state = last_state.clone();
            }
        }

        Ok(false)
    }

    /// Sets the active state for a multi-state package.
    pub fn set_active(&mut self, id: &str, state_hash: &str) -> Result<(), PackageManagerError> {
        let pkg = self
            .packages
            .get_mut(id)
            .ok_or_else(|| PackageManagerError::PackageNotFound(id.to_string()))?;

        if !pkg.state_hashes.contains_key(state_hash) {
            return Err(PackageManagerError::StateNotFound {
                id: id.to_string(),
                state_hash: state_hash.to_string(),
            });
        }

        pkg.active_state = state_hash.to_string();
        Ok(())
    }

    /// Looks up a package record in the registry by ID.
    pub fn get_package(&self, id: &str) -> Option<&PackageRecord> {
        self.packages.get(id)
    }

    /// Returns a list of all packages in the registry.
    pub fn list_packages(&self) -> Vec<&PackageRecord> {
        self.packages.values().collect()
    }

    /// Returns all installed state hashes for a package ID.
    pub fn get_installed_states(&self, id: &str) -> Vec<String> {
        self.packages
            .get(id)
            .map(|r| r.state_hashes.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Gets a specific installed package state record.
    pub fn get_package_state(&self, id: &str, state_hash: &str) -> Option<&PackageStateRecord> {
        self.packages
            .get(id)
            .and_then(|r| r.state_hashes.get(state_hash))
    }

    /// Gets the active installed package state record for a package ID.
    pub fn get_active_state(&self, id: &str) -> Option<(&PackageRecord, &PackageStateRecord)> {
        self.packages.get(id).and_then(|record| {
            record
                .state_hashes
                .get(&record.active_state)
                .map(|state_record| (record, state_record))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_registration_and_switching() {
        let mut reg = Registry::default();
        let manifest = Manifest::new("com.example.song", "Test Song");

        reg.register(&manifest, "hash_v1", "packages/com.example.song/hash_v1", "2026-08-28T00:00:00Z").unwrap();
        assert_eq!(reg.get_installed_states("com.example.song"), vec!["hash_v1".to_string()]);

        let (pkg, state) = reg.get_active_state("com.example.song").unwrap();
        assert_eq!(pkg.active_state, "hash_v1");
        assert_eq!(state.path, "packages/com.example.song/hash_v1");

        // Register state 2
        reg.register(&manifest, "hash_v2", "packages/com.example.song/hash_v2", "2026-08-28T01:00:00Z").unwrap();
        assert_eq!(reg.get_installed_states("com.example.song").len(), 2);
        assert_eq!(reg.get_package("com.example.song").unwrap().active_state, "hash_v2");

        // Switch active state back to hash_v1
        reg.set_active("com.example.song", "hash_v1").unwrap();
        assert_eq!(reg.get_package("com.example.song").unwrap().active_state, "hash_v1");

        // Unregister hash_v1 -> active switches to hash_v2
        let removed_all = reg.unregister("com.example.song", "hash_v1").unwrap();
        assert!(!removed_all);
        assert_eq!(reg.get_package("com.example.song").unwrap().active_state, "hash_v2");

        // Unregister hash_v2 -> completely removed
        let removed_all = reg.unregister("com.example.song", "hash_v2").unwrap();
        assert!(removed_all);
        assert!(reg.get_package("com.example.song").is_none());
    }
}
use crate::error::PackageManagerError;
use crate::storage::PackageStorage;
use bms_package::{Manifest, Package, MANIFEST_FILENAME};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Metadata record for a specific installed state of a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageStateRecord {
    pub path: String,
    pub installed_at: String,
}

/// Metadata record for an installed package grouping all its installed states.
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Registry {
    pub packages: BTreeMap<String, PackageRecord>,
}

const REGISTRY_FILENAME: &str = "registry.json";

impl Registry {
    /// Loads the registry from a file.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, PackageManagerError> {
        let content = fs::read_to_string(path)?;
        let registry: Self = serde_json::from_str(&content)
            .map_err(|e| PackageManagerError::RegistryError(e.to_string()))?;
        Ok(registry)
    }

    /// Saves the registry to a file.
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), PackageManagerError> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| PackageManagerError::RegistryError(e.to_string()))?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Registers a new installed package state.
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

        let entry = self.packages.entry(package_id).or_insert(PackageRecord {
            id: package_id,
            name: package_name,
            author: package_author,
            active_state: state_hash.to_string(),
            state_hashes: BTreeMap::new(),
        });

        entry.state_hashes.insert(
            state_hash.to_string(),
            PackageStateRecord {
                path: rel_path.to_string(),
                installed_at: installed_at.to_string(),
            },
        );

        // Update active state if this is the first state or if we want to keep the existing active state?
        // For simplicity, we set the active state to the newly registered state.
        entry.active_state = state_hash.to_string();

        Ok(())
    }

    /// Unregisters an installed package state. If no states remain, removes the package record completely.
    pub fn unregister(&mut self, package_id: &str, state_hash: &str) -> Result<bool, PackageManagerError> {
        if let Some(package_record) = self.packages.get_mut(package_id) {
            package_record.state_hashes.remove(state_hash);
            if package_record.state_hashes.is_empty() {
                self.packages.remove(package_id);
                Ok(true)
            } else {
                // If the removed state was the active state, we need to choose a new active state.
                if package_record.active_state == state_hash {
                    // Choose the first remaining state as active (arbitrary)
                    if let Some((new_active_state, _)) = package_record.state_hashes.iter().next() {
                        package_record.active_state = new_active_state.clone();
                    }
                }
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    /// Looks up a package record in the registry by ID.
    pub fn get_package(&self, id: &str) -> Option<&PackageRecord> {
        self.packages.get(id)
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
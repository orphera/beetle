use crate::error::PackageManagerError;
use bms_package::Manifest;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub const REGISTRY_FILENAME: &str = "registry.json";

/// Metadata record for a specific installed version of a package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageVersionRecord {
    pub version: String,
    pub path: String,
    pub installed_at: String,
}

/// Metadata record for an installed package grouping all its installed versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageRecord {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub active_version: String,
    pub versions: BTreeMap<String, PackageVersionRecord>,
}

/// In-memory and persisted registry of all managed BMS packages.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registry {
    pub packages: BTreeMap<String, PackageRecord>,
}

impl Registry {
    /// Loads registry from disk, or returns default empty registry if file does not exist.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, PackageManagerError> {
        let p = path.as_ref();
        if !p.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(p)?;
        let reg: Self = serde_json::from_str(&content)?;
        Ok(reg)
    }

    /// Atomically saves registry to disk as formatted JSON.
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), PackageManagerError> {
        let p = path.as_ref();
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)?;
        let tmp_path = p.with_extension("tmp");
        fs::write(&tmp_path, json)?;
        fs::rename(tmp_path, p)?;
        Ok(())
    }

    /// Registers an installed package version and sets it as the active version.
    pub fn register(
        &mut self,
        manifest: &Manifest,
        rel_path: String,
        installed_at: String,
    ) -> Result<(), PackageManagerError> {
        let id = manifest.id.clone();
        let version = manifest.version.clone();

        let pkg_record = self.packages.entry(id.clone()).or_insert_with(|| PackageRecord {
            id: id.clone(),
            name: manifest.name.clone(),
            author: manifest.author.clone(),
            active_version: version.clone(),
            versions: BTreeMap::new(),
        });

        pkg_record.name = manifest.name.clone();
        pkg_record.author = manifest.author.clone();
        pkg_record.active_version = version.clone();

        pkg_record.versions.insert(
            version.clone(),
            PackageVersionRecord {
                version,
                path: rel_path,
                installed_at,
            },
        );

        Ok(())
    }

    /// Unregisters an installed version. If no versions remain, removes the package record completely.
    pub fn unregister(&mut self, id: &str, version: &str) -> Result<bool, PackageManagerError> {
        let pkg = self
            .packages
            .get_mut(id)
            .ok_or_else(|| PackageManagerError::PackageNotFound(id.to_string()))?;

        if pkg.versions.remove(version).is_none() {
            return Err(PackageManagerError::VersionNotFound {
                id: id.to_string(),
                version: version.to_string(),
            });
        }

        if pkg.versions.is_empty() {
            self.packages.remove(id);
            return Ok(true);
        }

        // If the uninstalled version was active, switch active version to the newest remaining version
        if pkg.active_version == version {
            if let Some(last_ver) = pkg.versions.keys().last() {
                pkg.active_version = last_ver.clone();
            }
        }

        Ok(false)
    }

    /// Sets the active version for a package.
    pub fn set_active(&mut self, id: &str, version: &str) -> Result<(), PackageManagerError> {
        let pkg = self
            .packages
            .get_mut(id)
            .ok_or_else(|| PackageManagerError::PackageNotFound(id.to_string()))?;

        if !pkg.versions.contains_key(version) {
            return Err(PackageManagerError::VersionNotFound {
                id: id.to_string(),
                version: version.to_string(),
            });
        }

        pkg.active_version = version.to_string();
        Ok(())
    }

    /// Looks up a package record by ID.
    pub fn get_package(&self, id: &str) -> Option<&PackageRecord> {
        self.packages.get(id)
    }

    /// Looks up the active version record for a package.
    pub fn get_active_version(&self, id: &str) -> Option<(&PackageRecord, &PackageVersionRecord)> {
        let pkg = self.packages.get(id)?;
        let ver = pkg.versions.get(&pkg.active_version)?;
        Some((pkg, ver))
    }

    /// Returns a list of all packages in the registry.
    pub fn list_packages(&self) -> Vec<&PackageRecord> {
        self.packages.values().collect()
    }
}

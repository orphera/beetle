use crate::error::PackageManagerError;
use crate::registry::{PackageRecord, Registry, REGISTRY_FILENAME};
use crate::storage::PackageStorage;
use bms_package::{Manifest, Package, MANIFEST_FILENAME};
use std::fs;
use std::path::{Path, PathBuf};

/// High-level handle representing an installed package ready for Beetle consumption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPackage {
    pub id: String,
    pub version: String,
    pub name: String,
    pub author: Option<String>,
    pub location: PathBuf,
    pub is_active: bool,
}

impl InstalledPackage {
    /// Opens the installed package as a `bms_package::Package` instance.
    pub fn open(&self) -> Result<Package, PackageManagerError> {
        let bmsp_file = self.location.join("package.bmsp");
        if bmsp_file.exists() {
            let pkg = Package::open(&bmsp_file)?;
            return Ok(pkg);
        }

        // Fallback: read manifest and construct package from files
        let manifest_path = self.location.join(MANIFEST_FILENAME);
        let manifest_content = fs::read_to_string(manifest_path)?;
        let manifest = Manifest::from_json_str(&manifest_content)?;

        let mut builder = bms_package::PackageBuilder::new(manifest);
        self.collect_files_recursive(&self.location, &self.location, &mut builder)?;
        let bytes = builder.build_to_bytes()?;
        let pkg = Package::from_bytes(bytes)?;
        Ok(pkg)
    }

    /// Reads and returns the package manifest from the installed directory.
    pub fn manifest(&self) -> Result<Manifest, PackageManagerError> {
        let manifest_path = self.location.join(MANIFEST_FILENAME);
        let manifest_content = fs::read_to_string(manifest_path)?;
        let manifest = Manifest::from_json_str(&manifest_content)?;
        Ok(manifest)
    }

    fn collect_files_recursive(
        &self,
        base_dir: &Path,
        current_dir: &Path,
        builder: &mut bms_package::PackageBuilder,
    ) -> Result<(), PackageManagerError> {
        for entry in fs::read_dir(current_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                self.collect_files_recursive(base_dir, &path, builder)?;
            } else if path.is_file() {
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if file_name == MANIFEST_FILENAME || file_name == "package.bmsp" {
                    continue;
                }
                if let Ok(rel) = path.strip_prefix(base_dir) {
                    let rel_str = rel.to_string_lossy().replace('\\', "/");
                    let content = fs::read(&path)?;
                    builder.add_file(rel_str, content)?;
                }
            }
        }
        Ok(())
    }
}

/// Central BMS Package Manager coordinating installation, uninstallation, active versions, and discovery.
#[derive(Debug)]
pub struct PackageManager {
    root_dir: PathBuf,
    storage: PackageStorage,
    registry: Registry,
}

impl PackageManager {
    /// Initializes a package manager rooted at the specified directory.
    pub fn new<P: Into<PathBuf>>(root_dir: P) -> Result<Self, PackageManagerError> {
        let root = root_dir.into();
        fs::create_dir_all(&root)?;

        let storage = PackageStorage::new(&root);
        let registry_path = root.join(REGISTRY_FILENAME);
        let registry = Registry::load_from_file(&registry_path)?;

        Ok(Self {
            root_dir: root,
            storage,
            registry,
        })
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    fn save_registry(&self) -> Result<(), PackageManagerError> {
        let registry_path = self.root_dir.join(REGISTRY_FILENAME);
        self.registry.save_to_file(&registry_path)
    }

    /// Installs a package from a `.bmsp` file on disk.
    pub fn install<P: AsRef<Path>>(&mut self, bmsp_path: P) -> Result<InstalledPackage, PackageManagerError> {
        let bytes = fs::read(bmsp_path)?;
        self.install_from_bytes(bytes)
    }

    /// Installs a package from raw `.bmsp` binary bytes.
    pub fn install_from_bytes(&mut self, bytes: Vec<u8>) -> Result<InstalledPackage, PackageManagerError> {
        // 1. Validate package structure using bms-package
        let pkg = Package::from_bytes(bytes.clone())?;
        let manifest = pkg.manifest().clone();
        let id = manifest.id.clone();
        let version = manifest.version.clone();

        // 2. Check if already installed
        if let Some(record) = self.registry.get_package(&id) {
            if record.versions.contains_key(&version) {
                return Err(PackageManagerError::AlreadyInstalled {
                    id: id.clone(),
                    version: version.clone(),
                });
            }
        }

        // 3. Atomically extract and install files into managed storage
        let (location, rel_path) = self.storage.install_package(&pkg, &bytes)?;

        // 4. Update registry
        let now_str = "2026-08-28T02:00:00Z".to_string(); // Or ISO timestamp
        self.registry.register(&manifest, rel_path, now_str)?;
        self.save_registry()?;

        Ok(InstalledPackage {
            id,
            version,
            name: manifest.name,
            author: manifest.author,
            location,
            is_active: true,
        })
    }

    /// Packs a local BMS directory into `.bmsp` bytes.
    pub fn pack_folder<P: AsRef<Path>>(
        &self,
        folder_path: P,
        manifest_override: Option<Manifest>,
    ) -> Result<Vec<u8>, PackageManagerError> {
        crate::pack::pack_bms_folder(folder_path, manifest_override)
    }

    /// Ingests and installs an existing local BMS directory directly into managed storage.
    pub fn import_folder<P: AsRef<Path>>(
        &mut self,
        folder_path: P,
        manifest_override: Option<Manifest>,
    ) -> Result<InstalledPackage, PackageManagerError> {
        let bytes = self.pack_folder(folder_path, manifest_override)?;
        self.install_from_bytes(bytes)
    }

    /// Applies a delta `.bmdp` package file on disk onto the installed base version.
    pub fn apply_delta<P: AsRef<Path>>(
        &mut self,
        delta_path: P,
    ) -> Result<InstalledPackage, PackageManagerError> {
        crate::updater::PackageUpdater::apply_delta_file(self, delta_path)
    }

    /// Applies raw delta `.bmdp` bytes onto the installed base version.
    pub fn apply_delta_bytes(
        &mut self,
        delta_bytes: &[u8],
    ) -> Result<InstalledPackage, PackageManagerError> {
        crate::updater::PackageUpdater::apply_delta_bytes(self, delta_bytes)
    }

    /// Uninstalls a specific package version.
    pub fn uninstall(&mut self, id: &str, version: &str) -> Result<(), PackageManagerError> {
        // 1. Remove from storage
        self.storage.remove_package(id, version)?;

        // 2. Update registry
        self.registry.unregister(id, version)?;
        self.save_registry()?;

        Ok(())
    }

    /// Sets the active version for a multi-version package.
    pub fn set_active(&mut self, id: &str, version: &str) -> Result<(), PackageManagerError> {
        self.registry.set_active(id, version)?;
        self.save_registry()?;
        Ok(())
    }

    /// Returns a list of all active installed packages for discovery by Beetle.
    pub fn list_active_packages(&self) -> Vec<InstalledPackage> {
        let mut result = Vec::new();
        for record in self.registry.list_packages() {
            if let Some(ver_record) = record.versions.get(&record.active_version) {
                let location = self.root_dir.join(&ver_record.path);
                result.push(InstalledPackage {
                    id: record.id.clone(),
                    version: record.active_version.clone(),
                    name: record.name.clone(),
                    author: record.author.clone(),
                    location,
                    is_active: true,
                });
            }
        }
        result
    }

    /// Returns all installed package versions across all packages.
    pub fn list_all_installed(&self) -> Vec<InstalledPackage> {
        let mut result = Vec::new();
        for record in self.registry.list_packages() {
            for (ver, ver_record) in &record.versions {
                let location = self.root_dir.join(&ver_record.path);
                result.push(InstalledPackage {
                    id: record.id.clone(),
                    version: ver.clone(),
                    name: record.name.clone(),
                    author: record.author.clone(),
                    location,
                    is_active: ver == &record.active_version,
                });
            }
        }
        result
    }

    /// Gets the active installed package handle for a package ID.
    pub fn get_active_package(&self, id: &str) -> Option<InstalledPackage> {
        let (record, ver_record) = self.registry.get_active_version(id)?;
        let location = self.root_dir.join(&ver_record.path);
        Some(InstalledPackage {
            id: record.id.clone(),
            version: record.active_version.clone(),
            name: record.name.clone(),
            author: record.author.clone(),
            location,
            is_active: true,
        })
    }

    /// Gets a specific installed package version.
    pub fn get_installed_package(&self, id: &str, version: &str) -> Option<InstalledPackage> {
        let record = self.registry.get_package(id)?;
        let ver_record = record.versions.get(version)?;
        let location = self.root_dir.join(&ver_record.path);
        Some(InstalledPackage {
            id: record.id.clone(),
            version: version.to_string(),
            name: record.name.clone(),
            author: record.author.clone(),
            location,
            is_active: version == &record.active_version,
        })
    }

    /// Looks up a package record in the registry by ID.
    pub fn get_package(&self, id: &str) -> Option<&PackageRecord> {
        self.registry.get_package(id)
    }

    /// Gets all installed versions for a package ID.
    pub fn get_installed_versions(&self, id: &str) -> Vec<String> {
        self.registry
            .get_package(id)
            .map(|r| r.versions.keys().cloned().collect())
            .unwrap_or_default()
    }
}

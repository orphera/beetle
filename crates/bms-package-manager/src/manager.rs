use crate::error::PackageManagerError;
use crate::registry::{BgaStatus, PackageRecord, Registry, REGISTRY_FILENAME};
use crate::storage::PackageStorage;
use bms_package::{Manifest, Package, MANIFEST_FILENAME};
use std::fs;
use std::path::{Path, PathBuf};

/// High-level handle representing an installed package ready for Beetle consumption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPackage {
    pub id: String,
    pub state_hash: String,
    pub name: String,
    pub author: Option<String>,
    pub location: PathBuf,
    pub is_active: bool,
    pub bga_status: BgaStatus,
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
        Ok(Package::from_bytes(bytes)?)
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

/// Central BMS Package Manager coordinating installation, uninstallation, active states, and discovery.
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

    pub fn storage(&self) -> &PackageStorage {
        &self.storage
    }

    fn save_registry(&self) -> Result<(), PackageManagerError> {
        let registry_path = self.root_dir.join(REGISTRY_FILENAME);
        self.registry.save_to_file(&registry_path)
    }

    /// Installs a package from a `.bmsp` file on disk.
    pub fn install<P: AsRef<Path>>(&mut self, bmsp_path: P) -> Result<InstalledPackage, PackageManagerError> {
        self.install_with_progress(bmsp_path, None, |_, _, _, _| {})
    }

    /// Installs a package from a `.bmsp` file on disk with progress reporting and cancellation.
    pub fn install_with_progress<P: AsRef<Path>, F>(
        &mut self,
        bmsp_path: P,
        cancel_flag: Option<&std::sync::atomic::AtomicBool>,
        on_progress: F,
    ) -> Result<InstalledPackage, PackageManagerError>
    where
        F: FnMut(&str, usize, usize, &str),
    {
        let bytes = fs::read(bmsp_path)?;
        self.install_from_bytes_with_progress(bytes, cancel_flag, on_progress)
    }

    /// Installs a package from raw `.bmsp` binary bytes.
    pub fn install_from_bytes(&mut self, bytes: Vec<u8>) -> Result<InstalledPackage, PackageManagerError> {
        self.install_from_bytes_with_progress(bytes, None, |_, _, _, _| {})
    }

    /// Installs a package from raw `.bmsp` binary bytes with cancellation and progress reporting.
    pub fn install_from_bytes_with_progress<F>(
        &mut self,
        bytes: Vec<u8>,
        cancel_flag: Option<&std::sync::atomic::AtomicBool>,
        on_progress: F,
    ) -> Result<InstalledPackage, PackageManagerError>
    where
        F: FnMut(&str, usize, usize, &str),
    {
        // 1. Validate package structure using bms-package
        let pkg = Package::from_bytes(bytes.clone())?;
        let manifest = pkg.manifest().clone();
        let id = manifest.id.clone();
        let state_hash = pkg.state_hash();

        // 2. Check if already installed
        if let Some(record) = self.registry.get_package(&id) {
            if record.state_hashes.contains_key(&state_hash) {
                return Err(PackageManagerError::AlreadyInstalled {
                    id: id.clone(),
                    state_hash: state_hash.clone(),
                });
            }
        }

        // 3. Atomically extract and install files into managed storage
        let (location, rel_path) = self.storage.install_package_with_progress(&pkg, &bytes, cancel_flag, on_progress)?;

        // 4. Determine initial BGA status
        let has_video = pkg.entries().iter().any(|e| beetle_render::is_video_path(&e.path));
        let bga_status = if has_video {
            BgaStatus::Embedded
        } else {
            BgaStatus::None
        };

        // 5. Update registry
        let now_str = "2026-08-28T02:00:00Z".to_string(); // Or ISO timestamp
        self.registry.register_with_bga(&manifest, &state_hash, &rel_path, &now_str, bga_status, None)?;
        self.save_registry()?;

        Ok(InstalledPackage {
            id,
            state_hash,
            name: manifest.name,
            author: manifest.author,
            location,
            is_active: true,
            bga_status,
        })
    }

    /// Packs a local BMS directory into `.bmsp` bytes.
    pub fn pack_folder<P: AsRef<Path>>(
        &self,
        folder_path: P,
        manifest_override: Option<Manifest>,
    ) -> Result<Vec<u8>, PackageManagerError> {
        self.pack_folder_with_progress(folder_path, manifest_override, None, |_, _, _, _| {})
    }

    /// Packs a local BMS directory with cancellation and progress reporting (Classic profile).
    pub fn pack_folder_with_progress<P: AsRef<Path>, F>(
        &self,
        folder_path: P,
        manifest_override: Option<Manifest>,
        cancel_flag: Option<&std::sync::atomic::AtomicBool>,
        on_progress: F,
    ) -> Result<Vec<u8>, PackageManagerError>
    where
        F: FnMut(&str, usize, usize, &str),
    {
        crate::pack::pack_bms_folder_profile_with_progress(folder_path, manifest_override, crate::pack::PackProfile::Classic, cancel_flag, on_progress)
    }

    /// Packs a local BMS directory using a specified packaging profile.
    pub fn pack_folder_profile<P: AsRef<Path>>(
        &self,
        folder_path: P,
        manifest_override: Option<Manifest>,
        profile: crate::pack::PackProfile,
    ) -> Result<Vec<u8>, PackageManagerError> {
        self.pack_folder_profile_with_progress(folder_path, manifest_override, profile, None, |_, _, _, _| {})
    }

    /// Packs a local BMS directory using a specified packaging profile with cancellation and progress reporting.
    pub fn pack_folder_profile_with_progress<P: AsRef<Path>, F>(
        &self,
        folder_path: P,
        manifest_override: Option<Manifest>,
        profile: crate::pack::PackProfile,
        cancel_flag: Option<&std::sync::atomic::AtomicBool>,
        on_progress: F,
    ) -> Result<Vec<u8>, PackageManagerError>
    where
        F: FnMut(&str, usize, usize, &str),
    {
        crate::pack::pack_bms_folder_profile_with_progress(folder_path, manifest_override, profile, cancel_flag, on_progress)
    }

    /// Packs a local BMS directory with advanced options (profile and BGA mode), cancellation, and progress reporting.
    pub fn pack_folder_advanced_with_progress<P: AsRef<Path>, F>(
        &self,
        folder_path: P,
        manifest_override: Option<Manifest>,
        options: crate::pack::PackOptions,
        cancel_flag: Option<&std::sync::atomic::AtomicBool>,
        on_progress: F,
    ) -> Result<crate::pack::PackOutput, PackageManagerError>
    where
        F: FnMut(&str, usize, usize, &str),
    {
        crate::pack::pack_bms_folder_advanced_with_progress(folder_path, manifest_override, options, cancel_flag, on_progress)
    }

    /// Ingests and installs an existing local BMS directory directly into managed storage.
    pub fn import_folder<P: AsRef<Path>>(
        &mut self,
        folder_path: P,
        manifest_override: Option<Manifest>,
    ) -> Result<InstalledPackage, PackageManagerError> {
        self.import_folder_with_progress(folder_path, manifest_override, None, |_, _, _, _| {})
    }

    /// Ingests and installs a local BMS directory with cancellation and progress reporting.
    pub fn import_folder_with_progress<P: AsRef<Path>, F>(
        &mut self,
        folder_path: P,
        manifest_override: Option<Manifest>,
        cancel_flag: Option<&std::sync::atomic::AtomicBool>,
        mut on_progress: F,
    ) -> Result<InstalledPackage, PackageManagerError>
    where
        F: FnMut(&str, usize, usize, &str),
    {
        let bytes = self.pack_folder_with_progress(folder_path, manifest_override, cancel_flag, &mut on_progress)?;
        self.install_from_bytes_with_progress(bytes, cancel_flag, on_progress)
    }

    /// Applies a delta `.bmdp` package file on disk onto the installed base state.
    pub fn apply_delta<P: AsRef<Path>>(
        &mut self,
        delta_path: P,
    ) -> Result<InstalledPackage, PackageManagerError> {
        crate::updater::PackageUpdater::apply_delta_file(self, delta_path)
    }

    /// Applies raw delta `.bmdp` bytes onto the installed base state.
    pub fn apply_delta_bytes(
        &mut self,
        delta_bytes: &[u8],
    ) -> Result<InstalledPackage, PackageManagerError> {
        crate::updater::PackageUpdater::apply_delta_bytes(self, delta_bytes)
    }

    /// Exports an installed package or a package file into a traditional BMS folder.
    pub fn export_package<P: AsRef<Path>>(
        &self,
        package_id_or_path: &str,
        destination_dir: P,
    ) -> Result<crate::export::ExportStats, PackageManagerError> {
        let pkg_path = if Path::new(package_id_or_path).exists() {
            PathBuf::from(package_id_or_path)
        } else {
            let record = self
                .registry
                .packages
                .get(package_id_or_path)
                .ok_or_else(|| PackageManagerError::PackageNotFound(package_id_or_path.to_string()))?;
            let bmsp_file = self.storage.state_dir(&record.id, &record.active_state).join("package.bmsp");
            if !bmsp_file.exists() {
                return Err(PackageManagerError::PackageNotFound(format!(
                    "{}: package file not found at {}",
                    package_id_or_path,
                    bmsp_file.display()
                )));
            }
            bmsp_file
        };

        crate::export::export_package_to_folder(pkg_path, destination_dir)
    }

    /// Creates a Virtual BMS File System (VFS) mounting all active packages in storage,
    /// including associated BGA companions if present.
    pub fn create_vfs(&self) -> Result<crate::vfs::VirtualBmsFs, PackageManagerError> {
        let mut vfs = crate::vfs::VirtualBmsFs::new();
        for record in self.registry.packages.values() {
            let state_dir = self.storage.state_dir(&record.id, &record.active_state);
            let pkg_path = state_dir.join("package.bmsp");
            if pkg_path.exists() {
                let _ = vfs.mount_package(&record.id, &pkg_path);
            }
            let companion_path = state_dir.join(format!("{}.bga.bmsp", record.id));
            if companion_path.exists() {
                let _ = vfs.mount_package(&record.id, &companion_path);
            }
        }
        Ok(vfs)
    }

    /// Installs a decoupled BGA companion package (.bga.bmsp) for an existing installed package.
    pub fn install_bga_companion<P: AsRef<Path>>(
        &mut self,
        bga_bmsp_path: P,
    ) -> Result<String, PackageManagerError> {
        let bytes = fs::read(bga_bmsp_path)?;
        self.install_bga_companion_from_bytes(bytes)
    }

    /// Installs a decoupled BGA companion package from raw bytes for an existing installed package.
    pub fn install_bga_companion_from_bytes(
        &mut self,
        bytes: Vec<u8>,
    ) -> Result<String, PackageManagerError> {
        let bga_pkg = Package::from_bytes(bytes.clone())?;
        let bga_manifest = bga_pkg.manifest();

        let target_id = bga_manifest
            .target_package_id
            .as_ref()
            .ok_or_else(|| PackageManagerError::InvalidPackage("BGA companion package must specify target_package_id".to_string()))?;

        let record = self
            .registry
            .get_package(target_id)
            .ok_or_else(|| PackageManagerError::PackageNotFound(target_id.clone()))?;

        let active_state = record.active_state.clone();
        let companion_path = self.storage.install_companion(target_id, &active_state, &bga_pkg, &bytes)?;

        self.registry.update_bga_status(target_id, BgaStatus::Companion, Some(companion_path.to_string_lossy().to_string()))?;
        self.save_registry()?;

        Ok(target_id.clone())
    }

    /// Removes the BGA companion files from an installed package, reclaiming disk space while preserving song charts and keysounds.
    pub fn remove_bga_companion(&mut self, package_id: &str) -> Result<u64, PackageManagerError> {
        let record = self
            .registry
            .get_package(package_id)
            .ok_or_else(|| PackageManagerError::PackageNotFound(package_id.to_string()))?;

        let active_state = record.active_state.clone();
        let reclaimed = self.storage.remove_companion(package_id, &active_state)?;

        self.registry.update_bga_status(package_id, BgaStatus::None, None)?;
        self.save_registry()?;

        Ok(reclaimed)
    }

    /// Uninstalls a specific package state.
    pub fn uninstall(&mut self, id: &str, state_hash: &str) -> Result<(), PackageManagerError> {
        // 1. Remove from storage
        self.storage.remove_package(id, state_hash)?;

        // 2. Update registry
        self.registry.unregister(id, state_hash)?;
        self.save_registry()?;

        Ok(())
    }

    /// Sets the active state for a multi-state package.
    pub fn set_active(&mut self, id: &str, state_hash: &str) -> Result<(), PackageManagerError> {
        self.registry.set_active(id, state_hash)?;
        self.save_registry()?;
        Ok(())
    }

    /// Returns a list of all active installed packages for discovery by Beetle.
    pub fn list_active_packages(&self) -> Vec<InstalledPackage> {
        let mut result = Vec::new();
        for record in self.registry.list_packages() {
            if let Some(state_record) = record.state_hashes.get(&record.active_state) {
                let location = self.root_dir.join(&state_record.path);
                result.push(InstalledPackage {
                    id: record.id.clone(),
                    state_hash: record.active_state.clone(),
                    name: record.name.clone(),
                    author: record.author.clone(),
                    location,
                    is_active: true,
                    bga_status: record.bga_status,
                });
            }
        }
        result
    }

    /// Returns all installed package states across all packages.
    pub fn list_all_installed(&self) -> Vec<InstalledPackage> {
        let mut result = Vec::new();
        for record in self.registry.list_packages() {
            for (state_hash, state_record) in &record.state_hashes {
                let location = self.root_dir.join(&state_record.path);
                result.push(InstalledPackage {
                    id: record.id.clone(),
                    state_hash: state_hash.clone(),
                    name: record.name.clone(),
                    author: record.author.clone(),
                    location,
                    is_active: state_hash == &record.active_state,
                    bga_status: record.bga_status,
                });
            }
        }
        result
    }

    /// Looks up a package record in the registry by ID.
    pub fn get_package(&self, id: &str) -> Option<&PackageRecord> {
        self.registry.get_package(id)
    }

    /// Gets a specific installed package state.
    pub fn get_installed_package(&self, id: &str, state_hash: &str) -> Option<InstalledPackage> {
        let record = self.registry.get_package(id)?;
        let state_record = record.state_hashes.get(state_hash)?;
        let location = self.root_dir.join(&state_record.path);
        Some(InstalledPackage {
            id: record.id.clone(),
            state_hash: state_hash.to_string(),
            name: record.name.clone(),
            author: record.author.clone(),
            location,
            is_active: state_hash == &record.active_state,
            bga_status: record.bga_status,
        })
    }

    /// Gets the active installed package handle for a package ID.
    pub fn get_active_package(&self, id: &str) -> Option<InstalledPackage> {
        let (record, state_record) = self.registry.get_active_state(id)?;
        let location = self.root_dir.join(&state_record.path);
        Some(InstalledPackage {
            id: record.id.clone(),
            state_hash: record.active_state.clone(),
            name: record.name.clone(),
            author: record.author.clone(),
            location,
            is_active: true,
            bga_status: record.bga_status,
        })
    }

    /// Gets all installed state hashes for a package ID.
    pub fn get_installed_states(&self, id: &str) -> Vec<String> {
        self.registry
            .get_package(id)
            .map(|r| r.state_hashes.keys().cloned().collect())
            .unwrap_or_default()
    }
}
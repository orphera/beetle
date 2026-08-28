use std::fs;
use std::path::Path;

use bms_package::{sha256_hex, DeltaApplicator, DeltaBuilder, DeltaPackage, Package};

use crate::error::PackageManagerError;
use crate::manager::{InstalledPackage, PackageManager};
use crate::pack::pack_bms_folder;

/// Core update engine for generating and applying delta packages atomically.
pub struct PackageUpdater;

impl PackageUpdater {
    /// Generates a `.bmdp` delta package between two raw `.bmsp` packages.
    pub fn create_delta_between_packages(
        base_bmsp_bytes: &[u8],
        target_bmsp_bytes: &[u8],
    ) -> Result<Vec<u8>, PackageManagerError> {
        let base_pkg = Package::from_bytes(base_bmsp_bytes.to_vec())?;
        let target_pkg = Package::from_bytes(target_bmsp_bytes.to_vec())?;

        let base_sha256 = sha256_hex(base_bmsp_bytes);
        let target_sha256 = sha256_hex(target_bmsp_bytes);

        let delta_builder = DeltaBuilder::from_packages(&base_pkg, &target_pkg)?
            .with_checksums(Some(base_sha256), Some(target_sha256));

        let delta_bytes = delta_builder.build_to_bytes()?;
        Ok(delta_bytes)
    }

    /// Generates a `.bmdp` delta package between two files or directories.
    pub fn create_delta_between_paths<P1: AsRef<Path>, P2: AsRef<Path>>(
        base_path: P1,
        target_path: P2,
    ) -> Result<Vec<u8>, PackageManagerError> {
        let base_p = base_path.as_ref();
        let target_p = target_path.as_ref();

        let base_bytes = if base_p.is_dir() {
            pack_bms_folder(base_p, None)?
        } else {
            fs::read(base_p)?
        };

        let target_bytes = if target_p.is_dir() {
            pack_bms_folder(target_p, None)?
        } else {
            fs::read(target_p)?
        };

        Self::create_delta_between_packages(&base_bytes, &target_bytes)
    }

    /// Applies a delta package in memory onto the base package in the manager and installs the target version.
    pub fn apply_delta_bytes(
        manager: &mut PackageManager,
        delta_bytes: &[u8],
    ) -> Result<InstalledPackage, PackageManagerError> {
        let mut delta_pkg = DeltaPackage::from_bytes(delta_bytes.to_vec())?;
        let delta_man = delta_pkg.manifest().clone();

        // 1. Locate installed base package version
        let base_installed = manager
            .get_installed_package(&delta_man.package_id, &delta_man.base_version)
            .ok_or_else(|| PackageManagerError::BaseVersionNotInstalled {
                id: delta_man.package_id.clone(),
                base_version: delta_man.base_version.clone(),
            })?;

        // 2. Open base package
        let base_pkg = base_installed.open()?;

        // 3. Read raw base bytes if present (for exact hash verification)
        let base_bmsp_path = base_installed.location.join("package.bmsp");
        let base_raw_bytes = if base_bmsp_path.exists() {
            fs::read(&base_bmsp_path).ok()
        } else {
            None
        };

        // 4. Apply delta and reconstruct target package bytes
        let target_bytes = DeltaApplicator::apply_to_bytes(
            &base_pkg,
            &mut delta_pkg,
            base_raw_bytes.as_deref(),
        )?;

        // 5. Install reconstructed target package into manager
        manager.install_from_bytes(target_bytes)
    }

    /// Applies a delta package file from disk.
    pub fn apply_delta_file<P: AsRef<Path>>(
        manager: &mut PackageManager,
        delta_path: P,
    ) -> Result<InstalledPackage, PackageManagerError> {
        let delta_bytes = fs::read(delta_path)?;
        Self::apply_delta_bytes(manager, &delta_bytes)
    }
}

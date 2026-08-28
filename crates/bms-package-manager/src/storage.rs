use crate::error::PackageManagerError;
use bms_package::{Package, MANIFEST_FILENAME};
use std::fs;
use std::path::{Path, PathBuf};

/// Storage manager handling atomic filesystem layout, extraction, and deletion of packages.
#[derive(Debug, Clone)]
pub struct PackageStorage {
    root_dir: PathBuf,
}

impl PackageStorage {
    pub fn new<P: Into<PathBuf>>(root_dir: P) -> Self {
        Self {
            root_dir: root_dir.into(),
        }
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn packages_dir(&self) -> PathBuf {
        self.root_dir.join("packages")
    }

    /// Gets the destination directory for a specific package ID and state hash.
    pub fn state_dir(&self, id: &str, state_hash: &str) -> PathBuf {
        self.packages_dir().join(id).join(state_hash)
    }

    /// Checks if a package state directory already exists on disk.
    pub fn exists(&self, id: &str, state_hash: &str) -> bool {
        self.state_dir(id, state_hash).exists()
    }

    /// Atomically extracts and installs a validated package into the managed storage directory.
    pub fn install_package(
        &self,
        pkg: &Package,
        raw_bytes: &[u8],
    ) -> Result<(PathBuf, String), PackageManagerError> {
        self.install_package_with_progress(pkg, raw_bytes, None, |_, _, _, _| {})
    }

    /// Atomically extracts and installs a validated package with optional cancellation and progress reporting.
    pub fn install_package_with_progress<F>(
        &self,
        pkg: &Package,
        raw_bytes: &[u8],
        cancel_flag: Option<&std::sync::atomic::AtomicBool>,
        mut on_progress: F,
    ) -> Result<(PathBuf, String), PackageManagerError>
    where
        F: FnMut(&str, usize, usize, &str),
    {
        let id = &pkg.manifest().id;
        let state_hash = pkg.state_hash();
        let target_dir = self.state_dir(id, &state_hash);

        if target_dir.exists() {
            return Err(PackageManagerError::AlreadyInstalled {
                id: id.clone(),
                state_hash: state_hash.clone(),
            });
        }

        // Create a unique temporary directory for atomic installation
        let temp_base = self.root_dir.join(".tmp_install");
        fs::create_dir_all(&temp_base)?;

        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let temp_dir = temp_base.join(format!("{}_{}_{}", id, &state_hash[..8.min(state_hash.len())], nonce));
        fs::create_dir_all(&temp_dir)?;

        // Ensure temp_dir is cleaned up if any step fails
        let extract_result = (|| -> Result<(), PackageManagerError> {
            // 1. Write manifest.json
            let manifest_json = pkg.manifest().to_json_string()?;
            fs::write(temp_dir.join(MANIFEST_FILENAME), manifest_json)?;

            let entries = pkg.entries();
            let total = entries.len();

            // 2. Extract each entry safely
            for (i, entry) in entries.iter().enumerate() {
                if let Some(flag) = cancel_flag {
                    if flag.load(std::sync::atomic::Ordering::Relaxed) {
                        return Err(PackageManagerError::Cancelled);
                    }
                }

                if entry.path == MANIFEST_FILENAME {
                    continue;
                }

                on_progress("Extracting files", i + 1, total, &entry.path);

                let dest_path = temp_dir.join(&entry.path);

                // Security check: ensure path does not escape temp_dir
                if let Some(parent) = dest_path.parent() {
                    fs::create_dir_all(parent)?;
                }

                let content = pkg.read_entry(&entry.path)?;
                fs::write(&dest_path, content)?;
            }

            // 3. Save the intact .bmsp archive for fast package opens
            fs::write(temp_dir.join("package.bmsp"), raw_bytes)?;

            Ok(())
        })();

        if let Err(e) = extract_result {
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(e);
        }

        // Atomic move from temp directory to final destination
        if let Some(parent) = target_dir.parent() {
            fs::create_dir_all(parent)?;
        }

        if let Err(e) = fs::rename(&temp_dir, &target_dir) {
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(PackageManagerError::InstallationFailed(format!(
                "Failed to finalize package directory: {e}"
            )));
        }

        let rel_path = format!("packages/{}/{}", id, state_hash);
        Ok((target_dir, rel_path))
    }

    /// Removes an installed package state from storage.
    pub fn remove_package(&self, id: &str, state_hash: &str) -> Result<(), PackageManagerError> {
        let dir = self.state_dir(id, state_hash);
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }

        // Clean up parent package directory if empty
        let parent_dir = self.packages_dir().join(id);
        if parent_dir.exists() {
            if let Ok(mut entries) = fs::read_dir(&parent_dir) {
                if entries.next().is_none() {
                    let _ = fs::remove_dir(&parent_dir);
                }
            }
        }

        Ok(())
    }
}

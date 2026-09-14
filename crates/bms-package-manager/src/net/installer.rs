use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::manager::{InstalledPackage, PackageManager};
use crate::net::http::{DownloadProgressCallback, HttpClient, GLOBAL_MAX_PACKAGE_SIZE};
use crate::registry::remote::{resolve_url, CompanionBgaMetadata, RemotePackageMetadata};

/// RAII Drop Guard that ensures an in-progress temporary download file is automatically
/// deleted from disk if the download fails, cancels, or panics before being committed.
#[derive(Debug)]
pub struct DownloadTempFile {
    path: PathBuf,
    committed: bool,
}

impl DownloadTempFile {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            committed: false,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Marks the download as completed and verified.
    /// Disarms the drop guard and returns the final file path.
    pub fn commit(mut self) -> PathBuf {
        self.committed = true;
        self.path.clone()
    }
}

impl Drop for DownloadTempFile {
    fn drop(&mut self) {
        if !self.committed && self.path.exists() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Coordinates remote package downloads, verification, and atomic installation into PackageManager.
pub struct RemotePackageInstaller {
    downloads_dir: PathBuf,
}

impl RemotePackageInstaller {
    pub fn new(base_dir: &Path) -> Self {
        Self {
            downloads_dir: base_dir.join(".cache").join("downloads"),
        }
    }

    pub fn downloads_dir(&self) -> &Path {
        &self.downloads_dir
    }

    /// Generates a unique temporary download file path for the package.
    pub fn temp_download_path(&self, pkg: &RemotePackageMetadata) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let safe_id = pkg.id.replace(['/', '\\', ':', '.'], "_");
        self.downloads_dir
            .join(format!("{safe_id}-{timestamp}.tmp"))
    }

    /// Cleans up orphaned temporary download files older than `max_age` (Sweeping).
    pub fn cleanup_stale_downloads(&self, max_age: Duration) {
        if !self.downloads_dir.exists() {
            return;
        }

        let now = SystemTime::now();
        if let Ok(entries) = fs::read_dir(&self.downloads_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "tmp") {
                    if let Ok(meta) = entry.metadata() {
                        if let Ok(modified) = meta.modified() {
                            if let Ok(age) = now.duration_since(modified) {
                                if age > max_age {
                                    let _ = fs::remove_file(path);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Downloads a remote package into a guarded temporary file with streaming SHA-256 verification.
    pub fn download_package<C: DownloadProgressCallback>(
        &self,
        client: &HttpClient,
        pkg: &RemotePackageMetadata,
        base_registry_url: &str,
        callback: C,
    ) -> Result<DownloadTempFile, String> {
        fs::create_dir_all(&self.downloads_dir).map_err(|e| {
            format!(
                "Failed to create downloads directory '{}': {e}",
                self.downloads_dir.display()
            )
        })?;

        let full_url = pkg.resolve_download_url(base_registry_url);
        let temp_path = self.temp_download_path(pkg);
        let guard = DownloadTempFile::new(temp_path);

        // Safety Hard-Cap: Allow up to 105% of declared size or +1 MB margin, capped at 2 GB
        let safety_cap = ((pkg.size_bytes as f64 * 1.05) as u64)
            .max(pkg.size_bytes.saturating_add(1024 * 1024))
            .min(GLOBAL_MAX_PACKAGE_SIZE);

        client.download_file_with_checksum(
            &full_url,
            guard.path(),
            &pkg.sha256,
            Some(safety_cap),
            callback,
        )?;

        Ok(guard)
    }

    /// Downloads, verifies, and atomically installs a remote package into PackageManager.
    ///
    /// Reuses the existing, battle-tested `PackageManager::install_package` pipeline.
    pub fn install_remote_package<C: DownloadProgressCallback>(
        &self,
        manager: &mut PackageManager,
        client: &HttpClient,
        pkg: &RemotePackageMetadata,
        base_registry_url: &str,
        callback: C,
    ) -> Result<InstalledPackage, String> {
        let temp_file = self.download_package(client, pkg, base_registry_url, callback)?;
        let bmsp_path = temp_file.commit();

        let install_result = manager
            .install(&bmsp_path)
            .map_err(|e| format!("Failed to install downloaded package '{}': {e}", pkg.id));

        // Always clean up the committed temporary .tmp/.bmsp file after installation
        let _ = fs::remove_file(&bmsp_path);

        install_result
    }

    /// Downloads a companion BGA package into a guarded temporary file with streaming SHA-256 verification.
    pub fn download_bga_companion<C: DownloadProgressCallback>(
        &self,
        client: &HttpClient,
        bga: &CompanionBgaMetadata,
        base_registry_url: &str,
        callback: C,
    ) -> Result<DownloadTempFile, String> {
        fs::create_dir_all(&self.downloads_dir).map_err(|e| {
            format!(
                "Failed to create downloads directory '{}': {e}",
                self.downloads_dir.display()
            )
        })?;

        let full_url = resolve_url(base_registry_url, &bga.download_url);
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let safe_id = bga.id.replace(['/', '\\', ':', '.'], "_");
        let temp_path = self.downloads_dir.join(format!("{safe_id}-bga-{timestamp}.tmp"));
        let guard = DownloadTempFile::new(temp_path);

        let safety_cap = ((bga.size_bytes as f64 * 1.05) as u64)
            .max(bga.size_bytes.saturating_add(1024 * 1024))
            .min(GLOBAL_MAX_PACKAGE_SIZE);

        client.download_file_with_checksum(
            &full_url,
            guard.path(),
            &bga.sha256,
            Some(safety_cap),
            callback,
        )?;

        Ok(guard)
    }

    /// Downloads, verifies, and atomically installs a remote companion BGA package into PackageManager.
    pub fn install_remote_bga_companion<C: DownloadProgressCallback>(
        &self,
        manager: &mut PackageManager,
        client: &HttpClient,
        bga: &CompanionBgaMetadata,
        base_registry_url: &str,
        callback: C,
    ) -> Result<String, String> {
        let temp_file = self.download_bga_companion(client, bga, base_registry_url, callback)?;
        let bmsp_path = temp_file.commit();

        let install_result = manager
            .install_bga_companion(&bmsp_path)
            .map_err(|e| format!("Failed to install companion BGA package '{}': {e}", bga.id));

        let _ = fs::remove_file(&bmsp_path);

        install_result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_temp_file_raii_guard_removes_on_drop() {
        let temp_dir = std::env::temp_dir().join(format!(
            "bpm_raii_test_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        let file_path = temp_dir.join("download.tmp");
        fs::write(&file_path, b"test content").unwrap();
        assert!(file_path.exists());

        {
            let _guard = DownloadTempFile::new(file_path.clone());
            // Drops here without commit()
        }

        // Must be automatically removed on drop!
        assert!(!file_path.exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_download_temp_file_commit_preserves_file() {
        let temp_dir = std::env::temp_dir().join(format!(
            "bpm_raii_test2_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        let file_path = temp_dir.join("download2.tmp");
        fs::write(&file_path, b"test content").unwrap();

        let committed_path = {
            let guard = DownloadTempFile::new(file_path.clone());
            guard.commit()
        };

        // Must still exist because commit() disarmed the guard!
        assert!(committed_path.exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }
}

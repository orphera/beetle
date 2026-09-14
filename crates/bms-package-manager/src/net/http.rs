use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use bms_package::Sha256Hasher;

use crate::registry::remote::RemoteRegistryIndex;

pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
pub const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEFAULT_USER_AGENT: &str = "Beetle-Package-Manager/0.1.0";
pub const DEFAULT_STREAM_CHUNK_SIZE: usize = 64 * 1024; // 64 KB
pub const GLOBAL_MAX_PACKAGE_SIZE: u64 = 2 * 1024 * 1024 * 1024; // 2 GB

/// Callback trait for monitoring download progress.
pub trait DownloadProgressCallback {
    fn on_progress(&mut self, downloaded_bytes: u64, total_bytes: Option<u64>);

    /// Returns true if the download should be cancelled.
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// Simple closure adapter for `DownloadProgressCallback`.
impl<F> DownloadProgressCallback for F
where
    F: FnMut(u64, Option<u64>),
{
    fn on_progress(&mut self, downloaded_bytes: u64, total_bytes: Option<u64>) {
        (self)(downloaded_bytes, total_bytes);
    }
}

/// No-op callback implementation.
pub struct NoopProgressCallback;
impl DownloadProgressCallback for NoopProgressCallback {
    fn on_progress(&mut self, _downloaded_bytes: u64, _total_bytes: Option<u64>) {}
}

/// Lightweight blocking HTTP client built on `ureq`.
pub struct HttpClient {
    agent: ureq::Agent,
    user_agent: String,
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient {
    /// Creates a new HTTP client with default timeouts and user agent.
    pub fn new() -> Self {
        Self::with_timeouts(DEFAULT_CONNECT_TIMEOUT, DEFAULT_READ_TIMEOUT)
    }

    /// Creates a new HTTP client with custom timeouts.
    pub fn with_timeouts(connect_timeout: Duration, read_timeout: Duration) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(connect_timeout)
            .timeout_read(read_timeout)
            .build();

        Self {
            agent,
            user_agent: DEFAULT_USER_AGENT.to_string(),
        }
    }

    /// Fetches and parses a remote registry index (`index.json`).
    pub fn fetch_index(&self, url: &str) -> Result<RemoteRegistryIndex, String> {
        let response = self
            .agent
            .get(url)
            .set("User-Agent", &self.user_agent)
            .set("Accept", "application/json")
            .call()
            .map_err(|e| format!("HTTP request to '{url}' failed: {e}"))?;

        let status = response.status();
        if status != 200 {
            return Err(format!(
                "HTTP request to '{url}' returned non-200 status: {status}"
            ));
        }

        let body = response
            .into_string()
            .map_err(|e| format!("Failed to read HTTP response body from '{url}': {e}"))?;

        RemoteRegistryIndex::from_json_str(&body)
    }

    /// Downloads a remote file to `target_path` while streaming SHA-256 verification and progress updates.
    ///
    /// If `expected_sha256` does not match or size exceeds `max_bytes`, the file is removed and an error returned.
    pub fn download_file_with_checksum<P: AsRef<Path>, C: DownloadProgressCallback>(
        &self,
        url: &str,
        target_path: P,
        expected_sha256: &str,
        max_bytes: Option<u64>,
        mut callback: C,
    ) -> Result<u64, String> {
        let target = target_path.as_ref();
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create target parent directory '{}': {e}",
                    parent.display()
                )
            })?;
        }

        let response = self
            .agent
            .get(url)
            .set("User-Agent", &self.user_agent)
            .call()
            .map_err(|e| format!("HTTP download request to '{url}' failed: {e}"))?;

        let status = response.status();
        if status != 200 {
            return Err(format!(
                "HTTP download from '{url}' returned non-200 status: {status}"
            ));
        }

        let total_bytes: Option<u64> = response
            .header("Content-Length")
            .and_then(|h| h.parse::<u64>().ok());

        // Check against hard safety caps if Content-Length is provided
        let hard_cap = max_bytes.unwrap_or(GLOBAL_MAX_PACKAGE_SIZE);
        if let Some(len) = total_bytes {
            if len > hard_cap {
                return Err(format!(
                    "Content-Length ({len} bytes) exceeds safety maximum cap ({hard_cap} bytes)"
                ));
            }
        }

        let mut reader = response.into_reader();
        let mut file = File::create(target)
            .map_err(|e| format!("Failed to create download file '{}': {e}", target.display()))?;

        let mut hasher = Sha256Hasher::new();
        let mut downloaded: u64 = 0;
        let mut buffer = [0u8; DEFAULT_STREAM_CHUNK_SIZE];

        loop {
            if callback.is_cancelled() {
                let _ = fs::remove_file(target);
                return Err("Download cancelled by user".to_string());
            }

            let bytes_read = match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => {
                    let _ = fs::remove_file(target);
                    return Err(format!(
                        "Network read error during download from '{url}': {e}"
                    ));
                }
            };

            downloaded += bytes_read as u64;

            // Stream size safety cap check
            if downloaded > hard_cap {
                let _ = fs::remove_file(target);
                return Err(format!(
                    "Download stream exceeded safety maximum cap ({hard_cap} bytes)"
                ));
            }

            hasher.update(&buffer[..bytes_read]);

            if let Err(e) = file.write_all(&buffer[..bytes_read]) {
                let _ = fs::remove_file(target);
                return Err(format!("Disk write error to '{}': {e}", target.display()));
            }

            callback.on_progress(downloaded, total_bytes);
        }

        file.flush().map_err(|e| {
            format!(
                "Failed to flush downloaded file '{}': {e}",
                target.display()
            )
        })?;
        drop(file);

        // Verify SHA-256
        let actual_sha256 = hasher.finalize_hex();
        let expected_normalized = expected_sha256.trim().to_ascii_lowercase();

        if actual_sha256 != expected_normalized {
            let _ = fs::remove_file(target);
            return Err(format!(
                "SHA-256 checksum mismatch for '{url}': expected '{}', computed '{}'",
                expected_normalized, actual_sha256
            ));
        }

        Ok(downloaded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_progress_callback() {
        let mut noop = NoopProgressCallback;
        noop.on_progress(100, Some(200));
    }

    #[test]
    fn test_closure_progress_callback() {
        let mut recorded = 0;
        let mut cb = |cur: u64, _total: Option<u64>| {
            recorded = cur;
        };
        cb.on_progress(512, None);
        assert_eq!(recorded, 512);
    }
}

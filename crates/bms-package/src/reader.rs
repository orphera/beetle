use crate::entry::PackageEntry;
use crate::error::PackageError;
use crate::manifest::{Manifest, MANIFEST_FILENAME};
use crate::path::validate_entry_path;
use crate::checksum::sha256_hex;
use std::collections::HashSet;
use std::fs::File;
use std::io::{Cursor, Read, Seek};
use std::path::Path;
use zip::ZipArchive;

/// Default maximum allowed uncompressed size for a single entry (2 GB safety limit).
pub const DEFAULT_MAX_ENTRY_SIZE: u64 = 2 * 1024 * 1024 * 1024;

/// Represents an opened, validated, read-only BMS package (`.bmsp`).
#[derive(Debug, Clone)]
pub struct Package {
    manifest: Manifest,
    entries: Vec<PackageEntry>,
    data: Vec<u8>,
}

impl Package {
    /// Opens and validates a `.bmsp` package file from disk.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, PackageError> {
        let mut file = File::open(path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        Self::from_bytes(buffer)
    }

    /// Opens and validates a `.bmsp` package from an in-memory byte buffer.
    pub fn from_bytes(data: Vec<u8>) -> Result<Self, PackageError> {
        let (manifest, entries) = Self::inspect(&data)?;
        Ok(Self {
            manifest,
            entries,
            data,
        })
    }

    /// Opens and validates a `.bmsp` package from any seekable reader.
    pub fn from_reader<R: Read + Seek>(mut reader: R) -> Result<Self, PackageError> {
        let mut data = Vec::new();
        reader.read_to_end(&mut data)?;
        Self::from_bytes(data)
    }

    /// Inspects and validates raw package bytes without transferring ownership.
    pub fn inspect(data: &[u8]) -> Result<(Manifest, Vec<PackageEntry>), PackageError> {
        let reader = Cursor::new(data);
        let mut zip = ZipArchive::new(reader)?;

        // 1. Check and read manifest.json
        let manifest = {
            let mut manifest_file = zip
                .by_name(MANIFEST_FILENAME)
                .map_err(|_| PackageError::MissingManifest)?;

            if manifest_file.size() > 1024 * 1024 {
                return Err(PackageError::InvalidManifest(
                    "manifest.json exceeds size limit of 1MB".to_string(),
                ));
            }

            let mut manifest_json = String::new();
            manifest_file.read_to_string(&mut manifest_json)?;
            Manifest::from_json_str(&manifest_json)?
        };

        // 2. Validate all entries in archive
        let mut entries = Vec::with_capacity(zip.len());
        let mut seen_paths = HashSet::new();

        for i in 0..zip.len() {
            let zip_entry = zip.by_index(i)?;
            let raw_name = zip_entry.name();

            // Ignore directory markers if any (though builder doesn't create them)
            if raw_name.ends_with('/') {
                continue;
            }

            // Path security validation
            validate_entry_path(raw_name)?;

            // Duplicate entry check
            if !seen_paths.insert(raw_name.to_string()) {
                return Err(PackageError::DuplicateEntry(raw_name.to_string()));
            }

            // Decompression safety check
            let uncompressed_size = zip_entry.size();
            if uncompressed_size > DEFAULT_MAX_ENTRY_SIZE {
                return Err(PackageError::DecompressionLimitExceeded(uncompressed_size));
            }

            entries.push(PackageEntry::new(
                raw_name,
                uncompressed_size,
                zip_entry.crc32(),
            ));
        }

        // Sort entries alphabetically for consistent representation
        entries.sort_by(|a, b| a.path.cmp(&b.path));

        Ok((manifest, entries))
    }

    /// Access the parsed and verified package manifest.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Returns the list of all file entries contained in the package.
    pub fn entries(&self) -> &[PackageEntry] {
        &self.entries
    }

    /// Checks if an entry with the given relative path exists in the package.
    pub fn contains(&self, path: &str) -> bool {
        self.entries.iter().any(|e| e.path == path)
    }

    /// Reads and decompresses the full content of an entry by path.
    pub fn read_entry(&self, path: &str) -> Result<Vec<u8>, PackageError> {
        let cursor = Cursor::new(&self.data);
        let mut zip = ZipArchive::new(cursor)?;
        let mut entry_file = zip
            .by_name(path)
            .map_err(|_| PackageError::EntryNotFound(path.to_string()))?;
        let mut content = Vec::with_capacity(entry_file.size() as usize);
        entry_file.read_to_end(&mut content)?;
        Ok(content)
    }

    /// Opens an entry as a streaming reader.
    pub fn open_entry(&self, path: &str) -> Result<Box<dyn Read>, PackageError> {
        let bytes = self.read_entry(path)?;
        Ok(Box::new(Cursor::new(bytes)))
    }

    /// Returns the SHA-256 hash of the entire package data (the canonical archive bytes).
    pub fn state_hash(&self) -> String {
        sha256_hex(&self.data)
    }
}

/// Streaming, zero-allocation package reader backed by a file stream without loading the whole archive into memory.
pub struct PackageReader {
    manifest: Manifest,
    entries: Vec<PackageEntry>,
    zip: ZipArchive<std::io::BufReader<File>>,
}

impl PackageReader {
    /// Opens and validates package metadata and entry table from a .bmsp file on disk.
    pub fn open_file<P: AsRef<Path>>(path: P) -> Result<Self, PackageError> {
        let file = File::open(path)?;
        let reader = std::io::BufReader::new(file);
        let mut zip = ZipArchive::new(reader)?;

        // 1. Check and read manifest.json
        let manifest = {
            let mut manifest_file = zip
                .by_name(MANIFEST_FILENAME)
                .map_err(|_| PackageError::MissingManifest)?;

            if manifest_file.size() > 1024 * 1024 {
                return Err(PackageError::InvalidManifest(
                    "manifest.json exceeds size limit of 1MB".to_string(),
                ));
            }

            let mut manifest_json = String::new();
            manifest_file.read_to_string(&mut manifest_json)?;
            Manifest::from_json_str(&manifest_json)?
        };

        // 2. Validate all entries in archive
        let mut entries = Vec::with_capacity(zip.len());
        let mut seen_paths = HashSet::new();

        for i in 0..zip.len() {
            let zip_entry = zip.by_index(i)?;
            let raw_name = zip_entry.name();

            if raw_name.ends_with('/') {
                continue;
            }

            validate_entry_path(raw_name)?;

            if !seen_paths.insert(raw_name.to_string()) {
                return Err(PackageError::DuplicateEntry(raw_name.to_string()));
            }

            let uncompressed_size = zip_entry.size();
            if uncompressed_size > DEFAULT_MAX_ENTRY_SIZE {
                return Err(PackageError::DecompressionLimitExceeded(uncompressed_size));
            }

            entries.push(PackageEntry::new(
                raw_name,
                uncompressed_size,
                zip_entry.crc32(),
            ));
        }

        entries.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(Self {
            manifest,
            entries,
            zip,
        })
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn entries(&self) -> &[PackageEntry] {
        &self.entries
    }

    pub fn contains(&self, path: &str) -> bool {
        self.entries.iter().any(|e| e.path == path)
    }

    /// Resolves an entry path within the package given a base directory and relative name.
    /// Handles backslashes, case-insensitivity, and alternate audio extensions (.wav <-> .ogg).
    /// Scopes searches strictly to `base_dir` to prevent cross-song asset leakage in multi-song packages.
    pub fn find_entry_path(&self, base_dir: &str, relative_name: &str) -> Option<String> {
        let normalized_name = relative_name.replace('\\', "/");
        let base_trimmed = base_dir.trim_matches('/').trim_matches('\\');
        let has_base = !base_trimmed.is_empty() && base_trimmed != ".";

        let combined = if has_base {
            format!("{}/{}", base_trimmed, normalized_name.trim_start_matches('/'))
        } else {
            normalized_name.clone()
        };

        // 1. Exact match with combined path
        if self.contains(&combined) {
            return Some(combined);
        }

        // 2. Case-insensitive match with combined
        let combined_lower = combined.to_lowercase();
        if let Some(entry) = self.entries.iter().find(|e| e.path.to_lowercase() == combined_lower) {
            return Some(entry.path.clone());
        }

        let filename_only = normalized_name.rsplit('/').next().unwrap_or(&normalized_name).to_lowercase();

        // 3. Basename match strictly within base_dir (or anywhere if no base_dir)
        if has_base {
            let base_lower = base_trimmed.to_lowercase();
            let base_prefix = format!("{}/", base_lower);

            if let Some(entry) = self.entries.iter().find(|e| {
                let e_lower = e.path.to_lowercase();
                if e_lower.starts_with(&base_prefix) {
                    let e_file = e.path.rsplit('/').next().unwrap_or(&e.path).to_lowercase();
                    e_file == filename_only
                } else {
                    false
                }
            }) {
                return Some(entry.path.clone());
            }

            // 4. Alternate audio extensions within base_dir
            let stem = match filename_only.rfind('.') {
                Some(pos) => &filename_only[..pos],
                None => &filename_only,
            };

            for ext in &["wav", "ogg", "flac"] {
                let alt_target = format!("{}.{}", stem, ext).to_lowercase();
                if let Some(entry) = self.entries.iter().find(|e| {
                    let e_lower = e.path.to_lowercase();
                    if e_lower.starts_with(&base_prefix) {
                        let e_file = e.path.rsplit('/').next().unwrap_or(&e.path).to_lowercase();
                        e_file == alt_target
                    } else {
                        false
                    }
                }) {
                    return Some(entry.path.clone());
                }
            }
        } else {
            // No base_dir: match anywhere or at root
            if self.contains(&normalized_name) {
                return Some(normalized_name);
            }
            let rel_lower = normalized_name.to_lowercase();
            if let Some(entry) = self.entries.iter().find(|e| e.path.to_lowercase() == rel_lower) {
                return Some(entry.path.clone());
            }
            if let Some(entry) = self.entries.iter().find(|e| {
                let e_file = e.path.rsplit('/').next().unwrap_or(&e.path).to_lowercase();
                e_file == filename_only
            }) {
                return Some(entry.path.clone());
            }

            let stem = match filename_only.rfind('.') {
                Some(pos) => &filename_only[..pos],
                None => &filename_only,
            };
            for ext in &["wav", "ogg", "flac"] {
                let alt_target = format!("{}.{}", stem, ext).to_lowercase();
                if let Some(entry) = self.entries.iter().find(|e| {
                    let e_file = e.path.rsplit('/').next().unwrap_or(&e.path).to_lowercase();
                    e_file == alt_target
                }) {
                    return Some(entry.path.clone());
                }
            }
        }

        None
    }

    pub fn read_entry(&mut self, path: &str) -> Result<Vec<u8>, PackageError> {
        let mut entry_file = self
            .zip
            .by_name(path)
            .map_err(|_| PackageError::EntryNotFound(path.to_string()))?;
        let mut content = Vec::with_capacity(entry_file.size() as usize);
        entry_file.read_to_end(&mut content)?;
        Ok(content)
    }
}
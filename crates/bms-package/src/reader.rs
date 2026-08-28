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

/// Default maximum allowed uncompressed size for a single entry (100 MB safety limit).
pub const DEFAULT_MAX_ENTRY_SIZE: u64 = 100 * 1024 * 1024;

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
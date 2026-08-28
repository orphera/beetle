use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Cursor, Seek, Write};
use std::path::Path;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use crate::checksum::sha256_hex;
use crate::delta::manifest::{DeltaManifest, DeltaOpKind, DeltaResourceEntry, DELTA_MANIFEST_FILENAME};
use crate::error::PackageError;
use crate::path::validate_entry_path;
use crate::reader::Package;

/// Builder for creating standardized, deterministic `.bmdp` delta packages.
#[derive(Debug, Clone)]
pub struct DeltaBuilder {
    pub manifest: DeltaManifest,
    pub(crate) payload_files: BTreeMap<String, Vec<u8>>,
}

impl DeltaBuilder {
    /// Creates a DeltaBuilder manually from a DeltaManifest.
    pub fn new(manifest: DeltaManifest) -> Self {
        Self {
            manifest,
            payload_files: BTreeMap::new(),
        }
    }

    /// Automatically computes the diff between a Base Package and a Target Package.
    pub fn from_packages(base: &Package, target: &Package) -> Result<Self, PackageError> {
        let base_man = base.manifest();
        let target_man = target.manifest();

        if base_man.id != target_man.id {
            return Err(PackageError::DeltaBaseMismatch {
                expected_id: base_man.id.clone(),
                expected_version: base_man.version.clone(),
                actual_id: target_man.id.clone(),
                actual_version: target_man.version.clone(),
            });
        }

        let mut manifest = DeltaManifest::new(
            &base_man.id,
            &base_man.version,
            &target_man.version,
            target_man.clone(),
        );

        let mut payload_files = BTreeMap::new();

        // 1. Scan target entries to find Added, Modified, or Unchanged
        for target_entry in target.entries() {
            let path = &target_entry.path;
            if path == crate::manifest::MANIFEST_FILENAME {
                continue;
            }
            let target_bytes = target.read_entry(path)?;
            let target_hash = sha256_hex(&target_bytes);
            let size = target_bytes.len() as u64;

            if base.contains(path) {
                let base_bytes = base.read_entry(path)?;
                let base_hash = sha256_hex(&base_bytes);

                if base_hash == target_hash && base_bytes.len() == target_bytes.len() {
                    manifest.resources.push(DeltaResourceEntry {
                        path: path.clone(),
                        op: DeltaOpKind::Unchanged,
                        sha256: target_hash,
                        size_bytes: size,
                    });
                } else {
                    manifest.resources.push(DeltaResourceEntry {
                        path: path.clone(),
                        op: DeltaOpKind::Modified,
                        sha256: target_hash,
                        size_bytes: size,
                    });
                    payload_files.insert(path.clone(), target_bytes);
                }
            } else {
                manifest.resources.push(DeltaResourceEntry {
                    path: path.clone(),
                    op: DeltaOpKind::Added,
                    sha256: target_hash,
                    size_bytes: size,
                });
                payload_files.insert(path.clone(), target_bytes);
            }
        }

        // 2. Scan base entries to find Removed
        for base_entry in base.entries() {
            let path = &base_entry.path;
            if path == crate::manifest::MANIFEST_FILENAME {
                continue;
            }
            if !target.contains(path) {
                let base_bytes = base.read_entry(path)?;
                let base_hash = sha256_hex(&base_bytes);
                manifest.resources.push(DeltaResourceEntry {
                    path: path.clone(),
                    op: DeltaOpKind::Removed,
                    sha256: base_hash,
                    size_bytes: base_bytes.len() as u64,
                });
            }
        }

        // Sort resource list deterministically by path
        manifest.resources.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(Self {
            manifest,
            payload_files,
        })
    }

    /// Sets the expected SHA-256 checksums of the base package and target package.
    pub fn with_checksums(
        mut self,
        base_sha256: Option<String>,
        target_sha256: Option<String>,
    ) -> Self {
        self.manifest.base_package_sha256 = base_sha256;
        self.manifest.target_package_sha256 = target_sha256;
        self
    }

    /// Adds payload data for an Added or Modified entry.
    pub fn add_payload<S: Into<String>, D: Into<Vec<u8>>>(
        &mut self,
        path: S,
        data: D,
    ) -> Result<&mut Self, PackageError> {
        let path_str = path.into();
        if path_str == DELTA_MANIFEST_FILENAME {
            return Err(PackageError::InvalidEntryPath(
                "delta_manifest.json is managed automatically by DeltaBuilder".to_string(),
            ));
        }

        validate_entry_path(&path_str)?;

        if self.payload_files.contains_key(&path_str) {
            return Err(PackageError::DuplicateEntry(path_str));
        }

        self.payload_files.insert(path_str, data.into());
        Ok(self)
    }

    /// Access the delta manifest.
    pub fn manifest(&self) -> &DeltaManifest {
        &self.manifest
    }

    /// Access mutable delta manifest.
    pub fn manifest_mut(&mut self) -> &mut DeltaManifest {
        &mut self.manifest
    }

    /// Builds and serializes the delta package into a seekable writer with deterministic ordering and timestamps.
    pub fn build_to_writer<W: Write + Seek>(&self, writer: W) -> Result<(), PackageError> {
        self.manifest.validate()?;

        let mut zip = ZipWriter::new(writer);

        // Deterministic fixed timestamp: 1980-01-01 00:00:00 (INV-6)
        let fixed_time = DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0)
            .unwrap_or_else(|_| DateTime::default());

        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .last_modified_time(fixed_time);

        // 1. Write delta_manifest.json first
        let manifest_bytes = self.manifest.to_json_bytes()?;
        zip.start_file(DELTA_MANIFEST_FILENAME, options)?;
        zip.write_all(&manifest_bytes)?;

        // 2. Write payload file entries in alphabetical order (BTreeMap guarantees sorted keys)
        for (path, content) in &self.payload_files {
            zip.start_file(path, options)?;
            zip.write_all(content)?;
        }

        zip.finish()?;
        Ok(())
    }

    /// Builds and saves the delta package to a `.bmdp` file on disk.
    pub fn build_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), PackageError> {
        let file = File::create(path)?;
        self.build_to_writer(file)
    }

    /// Builds and returns the delta package binary data as an in-memory byte buffer.
    pub fn build_to_bytes(&self) -> Result<Vec<u8>, PackageError> {
        let mut cursor = Cursor::new(Vec::new());
        self.build_to_writer(&mut cursor)?;
        Ok(cursor.into_inner())
    }
}

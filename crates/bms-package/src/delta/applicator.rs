use std::io::{Cursor, Read, Seek};
use std::path::Path;

use zip::ZipArchive;

use crate::builder::PackageBuilder;
use crate::checksum::sha256_hex;
use crate::delta::manifest::{DeltaManifest, DeltaOpKind, DELTA_MANIFEST_FILENAME};
use crate::error::PackageError;
use crate::path::validate_entry_path;
use crate::reader::{Package, DEFAULT_MAX_ENTRY_SIZE};

/// Reader for inspecting and accessing a `.bmdp` delta package.
pub struct DeltaPackage<R: Read + Seek> {
    archive: ZipArchive<R>,
    manifest: DeltaManifest,
    entries: Vec<String>,
}

impl<R: Read + Seek> DeltaPackage<R> {
    /// Opens and validates a delta package from a seekable reader.
    pub fn new(reader: R) -> Result<Self, PackageError> {
        let mut archive = ZipArchive::new(reader)?;

        let manifest_bytes = {
            let mut file = archive
                .by_name(DELTA_MANIFEST_FILENAME)
                .map_err(|_| PackageError::MissingDeltaManifest)?;

            if file.size() > DEFAULT_MAX_ENTRY_SIZE {
                return Err(PackageError::DecompressionLimitExceeded(file.size()));
            }

            let mut buffer = Vec::with_capacity(file.size() as usize);
            file.read_to_end(&mut buffer)?;
            buffer
        };

        let manifest = DeltaManifest::from_json_slice(&manifest_bytes)?;

        let mut entries = Vec::with_capacity(archive.len());
        for i in 0..archive.len() {
            let file = archive.by_index(i)?;
            let name = file.name();
            if name != DELTA_MANIFEST_FILENAME {
                validate_entry_path(name)?;
                entries.push(name.to_string());
            }
        }

        Ok(Self {
            archive,
            manifest,
            entries,
        })
    }

    pub fn manifest(&self) -> &DeltaManifest {
        &self.manifest
    }

    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    pub fn contains_payload(&mut self, path: &str) -> bool {
        self.archive.by_name(path).is_ok()
    }

    /// Reads an uncompressed payload entry into memory.
    pub fn read_payload(&mut self, path: &str) -> Result<Vec<u8>, PackageError> {
        let mut file = self
            .archive
            .by_name(path)
            .map_err(|_| PackageError::EntryNotFound(path.to_string()))?;

        if file.size() > DEFAULT_MAX_ENTRY_SIZE {
            return Err(PackageError::DecompressionLimitExceeded(file.size()));
        }

        let mut buffer = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut buffer)?;
        Ok(buffer)
    }
}

impl DeltaPackage<Cursor<Vec<u8>>> {
    /// Opens a delta package from an in-memory byte buffer.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, PackageError> {
        Self::new(Cursor::new(bytes))
    }
}

impl DeltaPackage<std::fs::File> {
    /// Opens a delta package from a file path.
    pub fn open_file<P: AsRef<Path>>(path: P) -> Result<Self, PackageError> {
        let file = std::fs::File::open(path)?;
        Self::new(file)
    }
}

/// Applies a Delta Package to a Base Package to reconstruct the Target Package:
/// `Apply(Base, Delta) -> Target`
pub struct DeltaApplicator;

impl DeltaApplicator {
    /// Applies `delta` onto `base` and produces a reconstructed target `PackageBuilder`.
    pub fn apply<R: Read + Seek>(
        base: &Package,
        delta: &mut DeltaPackage<R>,
        base_raw_bytes: Option<&[u8]>,
    ) -> Result<PackageBuilder, PackageError> {
        let base_man = base.manifest();
        let delta_man = delta.manifest().clone();

        // 1. Verify Identity and Base Hash
        if base_man.id != delta_man.package_id || base.state_hash() != delta_man.base_hash {
            return Err(PackageError::DeltaBaseMismatch {
                expected_id: delta_man.package_id,
                expected_hash: delta_man.base_hash.clone(),
                actual_id: base_man.id.clone(),
                actual_hash: base.state_hash(),
            });
        }

        // 2. If Base Package Checksum is specified, verify it
        if let (Some(expected_base_hash), Some(raw_bytes)) = (&delta_man.base_package_sha256, base_raw_bytes) {
            let actual_base_hash = sha256_hex(raw_bytes);
            if actual_base_hash != *expected_base_hash {
                return Err(PackageError::DeltaChecksumMismatch {
                    expected: expected_base_hash.clone(),
                    actual: actual_base_hash,
                });
            }
        }

        // 3. Construct Target Package
        let mut target_builder = PackageBuilder::new(delta_man.target_manifest.clone());

        for res in &delta_man.resources {
            match res.op {
                DeltaOpKind::Added | DeltaOpKind::Modified => {
                    let payload = delta.read_payload(&res.path)?;
                    let hash = sha256_hex(&payload);
                    if hash != res.sha256 {
                        return Err(PackageError::DeltaChecksumMismatch {
                            expected: res.sha256.clone(),
                            actual: hash,
                        });
                    }
                    target_builder.add_file(&res.path, payload)?;
                }
                DeltaOpKind::Unchanged => {
                    // For unchanged resources, we need to copy them from the base package.
                    // We don't have the payload in the delta, so we read from the base.
                    let payload = base.read_entry(&res.path)?;
                    target_builder.add_file(&res.path, payload)?;
                }
                DeltaOpKind::Removed => {
                    // Do nothing, the resource is not in the target.
                }
            }
        }

        Ok(target_builder)
    }

    /// Applies delta and returns the reconstructed target package bytes, verifying target checksum if specified.
    pub fn apply_to_bytes<R: Read + Seek>(
        base: &Package,
        delta: &mut DeltaPackage<R>,
        base_raw_bytes: Option<&[u8]>,
    ) -> Result<Vec<u8>, PackageError> {
        let builder = Self::apply(base, delta, base_raw_bytes)?;
        let target_bytes = builder.build_to_bytes()?;

        if let Some(expected_target_hash) = &delta.manifest().target_package_sha256 {
            let actual_target_hash = sha256_hex(&target_bytes);
            if actual_target_hash != *expected_target_hash {
                return Err(PackageError::DeltaChecksumMismatch {
                    expected: expected_target_hash.clone(),
                    actual: actual_target_hash,
                });
            }
        }

        Ok(target_bytes)
    }
}
use crate::error::PackageError;
use crate::manifest::{Manifest, MANIFEST_FILENAME};
use crate::path::validate_entry_path;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Cursor, Read, Seek, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

/// Builder for creating standardized, deterministic `.bmsp` packages.
#[derive(Debug, Clone)]
pub struct PackageBuilder {
    manifest: Manifest,
    files: BTreeMap<String, Vec<u8>>,
}

impl PackageBuilder {
    /// Creates a new PackageBuilder with the specified manifest.
    pub fn new(manifest: Manifest) -> Self {
        Self {
            manifest,
            files: BTreeMap::new(),
        }
    }

    /// Adds a file entry with in-memory byte contents to the package.
    pub fn add_file<S: Into<String>, D: Into<Vec<u8>>>(
        &mut self,
        path: S,
        data: D,
    ) -> Result<&mut Self, PackageError> {
        let path_str = path.into();

        if path_str == MANIFEST_FILENAME {
            return Err(PackageError::InvalidEntryPath(
                "manifest.json is managed automatically by PackageBuilder".to_string(),
            ));
        }

        validate_entry_path(&path_str)?;

        if self.files.contains_key(&path_str) {
            return Err(PackageError::DuplicateEntry(path_str));
        }

        self.files.insert(path_str, data.into());
        Ok(self)
    }

    /// Adds a file entry by reading its content from a disk path.
    pub fn add_file_from_disk<S: Into<String>, P: AsRef<Path>>(
        &mut self,
        entry_path: S,
        file_path: P,
    ) -> Result<&mut Self, PackageError> {
        let mut file = File::open(file_path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        self.add_file(entry_path, buffer)
    }

    /// Builds and serializes the package into a seekable writer with deterministic ordering and timestamps.
    pub fn build_to_writer<W: Write + Seek>(&self, writer: W) -> Result<(), PackageError> {
        self.manifest.validate()?;

        let mut zip = ZipWriter::new(writer);

        // Deterministic fixed timestamp: 1980-01-01 00:00:00
        let fixed_time = DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0)
            .unwrap_or_else(|_| DateTime::default());

        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .last_modified_time(fixed_time);

        // 1. Write manifest.json first
        let manifest_json = self.manifest.to_json_string()?;
        zip.start_file(MANIFEST_FILENAME, options)?;
        zip.write_all(manifest_json.as_bytes())?;

        // 2. Write file entries in alphabetical order (BTreeMap guarantees sorted keys)
        for (path, content) in &self.files {
            zip.start_file(path, options)?;
            zip.write_all(content)?;
        }

        zip.finish()?;
        Ok(())
    }

    /// Builds and saves the package to a `.bmsp` file on disk.
    pub fn build_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), PackageError> {
        let file = File::create(path)?;
        self.build_to_writer(file)
    }

    /// Builds and returns the package binary data as an in-memory byte buffer.
    pub fn build_to_bytes(&self) -> Result<Vec<u8>, PackageError> {
        let mut cursor = Cursor::new(Vec::new());
        self.build_to_writer(&mut cursor)?;
        Ok(cursor.into_inner())
    }
}

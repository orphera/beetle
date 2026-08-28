use std::fmt;

/// Errors that can occur during BMS package reading, creation, or validation.
#[derive(Debug)]
pub enum PackageError {
    /// Underlying I/O error.
    Io(std::io::Error),
    /// The container is not a valid ZIP file or has invalid headers.
    InvalidZip(String),
    /// The required `manifest.json` file is missing in the package root.
    MissingManifest,
    /// `manifest.json` is malformed JSON or has missing/invalid required fields.
    InvalidManifest(String),
    /// The package format version is unsupported by this version of bms-package.
    UnsupportedFormat(u32),
    /// An entry path violates security rules (traversal, absolute path, backslashes, etc.).
    InvalidEntryPath(String),
    /// The package contains duplicate entry paths.
    DuplicateEntry(String),
    /// The requested entry was not found in the package.
    EntryNotFound(String),
    /// The uncompressed size of an entry or package exceeds safety limits.
    DecompressionLimitExceeded(u64),
    /// General corruption or integrity error in package data.
    CorruptedPackage(String),
    /// The required `delta_manifest.json` file is missing in the delta package root.
    MissingDeltaManifest,
    /// `delta_manifest.json` is malformed JSON or has missing/invalid required fields.
    InvalidDeltaManifest(String),
    /// The base package ID or version does not match what the delta requires.
    DeltaBaseMismatch {
        expected_id: String,
        expected_version: String,
        actual_id: String,
        actual_version: String,
    },
    /// The calculated checksum of the base or target package did not match the manifest.
    DeltaChecksumMismatch {
        expected: String,
        actual: String,
    },
    /// Delta patch application failed.
    DeltaApplyFailed(String),
}

impl fmt::Display for PackageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::InvalidZip(msg) => write!(f, "Invalid ZIP container: {msg}"),
            Self::MissingManifest => write!(f, "Missing manifest.json in package root"),
            Self::InvalidManifest(msg) => write!(f, "Invalid manifest: {msg}"),
            Self::UnsupportedFormat(ver) => write!(f, "Unsupported package format version: {ver}"),
            Self::InvalidEntryPath(path) => write!(f, "Invalid entry path: '{path}'"),
            Self::DuplicateEntry(path) => write!(f, "Duplicate entry path in package: '{path}'"),
            Self::EntryNotFound(path) => write!(f, "Entry not found in package: '{path}'"),
            Self::DecompressionLimitExceeded(size) => {
                write!(f, "Decompression safety limit exceeded: {size} bytes")
            }
            Self::CorruptedPackage(msg) => write!(f, "Corrupted package: {msg}"),
            Self::MissingDeltaManifest => write!(f, "Missing delta_manifest.json in delta archive root"),
            Self::InvalidDeltaManifest(msg) => write!(f, "Invalid delta manifest: {msg}"),
            Self::DeltaBaseMismatch {
                expected_id,
                expected_version,
                actual_id,
                actual_version,
            } => write!(
                f,
                "Delta base mismatch: expected {expected_id}@{expected_version}, got {actual_id}@{actual_version}"
            ),
            Self::DeltaChecksumMismatch { expected, actual } => write!(
                f,
                "Delta integrity checksum mismatch: expected '{expected}', calculated '{actual}'"
            ),
            Self::DeltaApplyFailed(msg) => write!(f, "Delta apply failed: {msg}"),
        }
    }
}

impl std::error::Error for PackageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for PackageError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<serde_json::Error> for PackageError {
    fn from(err: serde_json::Error) -> Self {
        Self::InvalidManifest(err.to_string())
    }
}

impl From<zip::result::ZipError> for PackageError {
    fn from(err: zip::result::ZipError) -> Self {
        match err {
            zip::result::ZipError::Io(e) => Self::Io(e),
            zip::result::ZipError::InvalidArchive(msg) => Self::InvalidZip(msg.to_string()),
            zip::result::ZipError::UnsupportedArchive(msg) => Self::InvalidZip(msg.to_string()),
            zip::result::ZipError::FileNotFound => Self::MissingManifest,
            other => Self::CorruptedPackage(other.to_string()),
        }
    }
}

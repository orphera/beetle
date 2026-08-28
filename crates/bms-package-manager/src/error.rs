use std::fmt;

/// Errors that can occur during package manager operations.
#[derive(Debug)]
pub enum PackageManagerError {
    /// Package with given ID was not found in registry.
    PackageNotFound(String),
    /// Specific version of a package was not found.
    VersionNotFound { id: String, version: String },
    /// Package version is already installed.
    AlreadyInstalled { id: String, version: String },
    /// Package is not installed.
    NotInstalled(String),
    /// Package validation failed.
    InvalidPackage(String),
    /// Installation process failed during extraction or verification.
    InstallationFailed(String),
    /// Package verification failed after installation.
    VerificationFailed(String),
    /// Base version required by a delta package is not installed.
    BaseStateNotInstalled { id: String, base_hash: String },
    /// Delta creation or application error.
    DeltaError(String),
    /// Registry storage or serialization error.
    RegistryError(String),
    /// Storage error during file operations.
    StorageError(String),
    /// Error originating from the bms-package library.
    Package(bms_package::PackageError),
    /// Underlying I/O error.
    Io(std::io::Error),
}

impl fmt::Display for PackageManagerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PackageNotFound(id) => write!(f, "Package not found: '{id}'"),
            Self::VersionNotFound { id, version } => {
                write!(f, "Package '{id}' version '{version}' not found")
            }
            Self::AlreadyInstalled { id, version } => {
                write!(f, "Package '{id}@{version}' is already installed")
            }
            Self::BaseStateNotInstalled { id, base_hash } => write!(
                f,
                "Base package '{id}@{base_hash}' is not installed (required for delta update)"
            ),
            Self::DeltaError(msg) => write!(f, "Delta error: {msg}"),
            Self::NotInstalled(id) => write!(f, "Package '{id}' is not installed"),
            Self::InvalidPackage(msg) => write!(f, "Invalid package: {msg}"),
            Self::InstallationFailed(msg) => write!(f, "Installation failed: {msg}"),
            Self::VerificationFailed(msg) => write!(f, "Verification failed: {msg}"),
            Self::RegistryError(msg) => write!(f, "Registry error: {msg}"),
            Self::StorageError(msg) => write!(f, "Storage error: {msg}"),
            Self::Package(e) => write!(f, "Package error: {e}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for PackageManagerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Package(e) => Some(e),
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<bms_package::PackageError> for PackageManagerError {
    fn from(err: bms_package::PackageError) -> Self {
        Self::Package(err)
    }
}

impl From<std::io::Error> for PackageManagerError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<serde_json::Error> for PackageManagerError {
    fn from(err: serde_json::Error) -> Self {
        Self::RegistryError(err.to_string())
    }
}

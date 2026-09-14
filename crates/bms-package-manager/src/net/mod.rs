pub mod cache;
pub mod http;
pub mod installer;
pub mod updater;

pub use cache::RegistryCacheManager;
pub use http::{
    DownloadProgressCallback, HttpClient, NoopProgressCallback, DEFAULT_CONNECT_TIMEOUT,
    DEFAULT_READ_TIMEOUT, DEFAULT_STREAM_CHUNK_SIZE, DEFAULT_USER_AGENT, GLOBAL_MAX_PACKAGE_SIZE,
};
pub use installer::{DownloadTempFile, RemotePackageInstaller};
pub use updater::{find_available_updates, upgrade_packages, PackageUpdateInfo};

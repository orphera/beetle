/// Metadata describing a file entry within a BMS package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageEntry {
    /// Relative path using `/` separator (e.g. `bms/main.bms`, `audio/01.wav`).
    pub path: String,
    /// Uncompressed size of the entry in bytes.
    pub size: u64,
    /// CRC32 checksum of the uncompressed entry data.
    pub crc32: u32,
}

impl PackageEntry {
    pub fn new<P: Into<String>>(path: P, size: u64, crc32: u32) -> Self {
        Self {
            path: path.into(),
            size,
            crc32,
        }
    }
}

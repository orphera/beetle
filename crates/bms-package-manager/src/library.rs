//! Reading and writing `library.dat`, the list of legacy BMS folders the
//! player scans in place (shared by `bpm library` and `bpm-gui`).

use beetle_core::LibraryPaths;
use std::fs;
use std::path::PathBuf;

/// `library.dat` in the working directory, or `$BEETLE_LIBRARY_FILE`. The
/// player reads the same file.
pub fn library_file() -> PathBuf {
    std::env::var("BEETLE_LIBRARY_FILE")
        .map_or_else(|_| PathBuf::from("library.dat"), PathBuf::from)
}

pub fn load_library() -> LibraryPaths {
    LibraryPaths::parse(&fs::read_to_string(library_file()).unwrap_or_default())
}

pub fn save_library(list: &LibraryPaths) -> std::io::Result<()> {
    fs::write(library_file(), list.serialize())
}

/// Absolute form of an existing folder, as the collection stores it (`beetle_core::display_path`).
pub fn absolute_dir(path: &str) -> Result<String, String> {
    let p = fs::canonicalize(path).map_err(|e| format!("'{path}': {e}"))?;
    if !p.is_dir() {
        return Err(format!("'{path}' is not a folder"));
    }
    Ok(beetle_core::display_path(&p.to_string_lossy()).to_string())
}

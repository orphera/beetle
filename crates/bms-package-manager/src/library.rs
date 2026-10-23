//! Reading and writing `library.dat`, the list of legacy BMS folders the
//! player scans in place, and of game discs it reads (`.iso` images and
//! drives; shared by `bpm library` and `bpm-gui`).

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

/// What `library.dat` stores for `path`: a drive as `E:`, a disc image or a
/// folder as its absolute path. A drive need not hold a disc yet.
pub fn library_entry(path: &str) -> Result<String, String> {
    if let Some(letter) = cs_disc::drive::as_drive_letter(path) {
        return Ok(format!("{letter}:"));
    }
    if cs_disc::is_disc_target(path) {
        let p = fs::canonicalize(path).map_err(|e| format!("'{path}': {e}"))?;
        if !p.is_file() {
            return Err(format!("'{path}' is not a disc image"));
        }
        return Ok(beetle_core::display_path(&p.to_string_lossy()).to_string());
    }
    absolute_dir(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drive_is_stored_by_its_letter() {
        assert_eq!(library_entry("e:\\").unwrap(), "E:");
        assert_eq!(library_entry("E:").unwrap(), "E:");
    }

    #[test]
    fn a_missing_disc_image_is_refused() {
        assert!(library_entry("no-such-disc.iso").is_err());
    }
}

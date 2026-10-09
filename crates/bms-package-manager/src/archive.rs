//! Extraction of plain zip archives (for example, a zip of several BMS song folders).

use crate::PackageManagerError;
use std::fs::{self, File};
use std::io;
use std::path::Path;

/// Extracts every entry of the zip archive at `zip_path` under `dest_dir`.
///
/// Entries whose names would escape `dest_dir` (absolute paths, `..` components) are skipped.
/// Returns the number of regular files written.
pub fn extract_zip_archive<P: AsRef<Path>, Q: AsRef<Path>>(
    zip_path: P,
    dest_dir: Q,
) -> Result<usize, PackageManagerError> {
    let dest_dir = dest_dir.as_ref();
    let file = File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| PackageManagerError::InvalidPackage(format!("not a valid zip: {e}")))?;
    fs::create_dir_all(dest_dir)?;

    let mut written = 0;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| PackageManagerError::InvalidPackage(format!("corrupt zip entry: {e}")))?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let out_path = dest_dir.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = File::create(&out_path)?;
        io::copy(&mut entry, &mut out)?;
        written += 1;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    #[test]
    fn test_extract_skips_entries_escaping_destination() {
        let root = std::env::temp_dir().join(format!(
            "bpm_archive_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let zip_path = root.join("pack.zip");
        {
            let mut zip = zip::ZipWriter::new(File::create(&zip_path).unwrap());
            let opts = SimpleFileOptions::default();
            zip.start_file("song/main.bms", opts).unwrap();
            zip.write_all(b"#TITLE Song\n").unwrap();
            zip.start_file("../escape.bms", opts).unwrap();
            zip.write_all(b"#TITLE Escape\n").unwrap();
            zip.finish().unwrap();
        }

        let dest = root.join("out");
        let written = extract_zip_archive(&zip_path, &dest).unwrap();

        assert_eq!(written, 1);
        assert!(dest.join("song").join("main.bms").exists());
        assert!(!root.join("escape.bms").exists());
        let _ = fs::remove_dir_all(&root);
    }
}

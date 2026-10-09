//! Extraction of plain zip archives (for example, a zip of several BMS song folders).

use crate::PackageManagerError;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

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
        // `name()` decodes names without the UTF-8 flag as CP437, which turns
        // Korean and Japanese names from Windows-made zips into mojibake.
        // Decode the raw bytes the way chart text is decoded instead.
        let name = beetle_core::decode_bms_text(entry.name_raw());
        let Some(relative) = safe_relative_path(&name) else {
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

/// Turns an archive entry name into a path under the destination, or `None`
/// if it would escape it (absolute, drive-prefixed, or containing `..`).
fn safe_relative_path(name: &str) -> Option<PathBuf> {
    let mut path = PathBuf::new();
    for part in name.split(['/', '\\']) {
        match part {
            "" | "." => continue,
            ".." => return None,
            _ if part.contains(':') => return None,
            _ => path.push(part),
        }
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    #[test]
    fn test_safe_relative_path_rejects_escapes() {
        assert_eq!(
            safe_relative_path("song/main.bms"),
            Some(PathBuf::from("song").join("main.bms"))
        );
        assert_eq!(safe_relative_path("../x.bms"), None);
        assert_eq!(safe_relative_path("a/../../x.bms"), None);
        assert_eq!(safe_relative_path("C:/evil.bms"), None);
        assert_eq!(
            safe_relative_path("/abs.bms"),
            Some(PathBuf::from("abs.bms"))
        );
    }

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

    /// Builds a one-entry stored zip whose name is raw bytes with the UTF-8
    /// flag cleared, the way older Windows tools wrote Korean/Japanese names.
    #[cfg(target_os = "windows")]
    fn legacy_zip(name: &[u8], content: &[u8]) -> Vec<u8> {
        fn crc32(data: &[u8]) -> u32 {
            let mut crc = 0xFFFF_FFFFu32;
            for &b in data {
                crc ^= b as u32;
                for _ in 0..8 {
                    crc = if crc & 1 != 0 {
                        (crc >> 1) ^ 0xEDB8_8320
                    } else {
                        crc >> 1
                    };
                }
            }
            !crc
        }
        let crc = crc32(content);
        let mut out = Vec::new();
        let push16 = |out: &mut Vec<u8>, v: u16| out.extend_from_slice(&v.to_le_bytes());
        // Local file header
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        push16(&mut out, 20);
        push16(&mut out, 0); // flags: no UTF-8
        push16(&mut out, 0); // stored
        push16(&mut out, 0);
        push16(&mut out, 0);
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(content.len() as u32).to_le_bytes());
        out.extend_from_slice(&(content.len() as u32).to_le_bytes());
        push16(&mut out, name.len() as u16);
        push16(&mut out, 0);
        out.extend_from_slice(name);
        out.extend_from_slice(content);
        let cd_offset = out.len() as u32;
        // Central directory entry
        out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        push16(&mut out, 20);
        push16(&mut out, 20);
        push16(&mut out, 0);
        push16(&mut out, 0);
        push16(&mut out, 0);
        push16(&mut out, 0);
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(content.len() as u32).to_le_bytes());
        out.extend_from_slice(&(content.len() as u32).to_le_bytes());
        push16(&mut out, name.len() as u16);
        push16(&mut out, 0);
        push16(&mut out, 0);
        push16(&mut out, 0);
        push16(&mut out, 0);
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(name);
        let cd_size = out.len() as u32 - cd_offset;
        // End of central directory
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        push16(&mut out, 0);
        push16(&mut out, 0);
        push16(&mut out, 1);
        push16(&mut out, 1);
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        push16(&mut out, 0);
        out
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_extract_decodes_legacy_korean_entry_names() {
        // "가나/main.bms" with the Korean part in CP949 (EUC-KR) bytes.
        let name: &[u8] = &[
            0xB0, 0xA1, 0xB3, 0xAA, b'/', b'm', b'a', b'i', b'n', b'.', b'b', b'm', b's',
        ];
        let root = std::env::temp_dir().join(format!(
            "bpm_legacy_zip_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let zip_path = root.join("legacy.zip");
        fs::write(&zip_path, legacy_zip(name, b"#TITLE x\n")).unwrap();

        let dest = root.join("out");
        let written = extract_zip_archive(&zip_path, &dest).unwrap();

        assert_eq!(written, 1);
        assert!(dest.join("가나").join("main.bms").exists());
        let _ = fs::remove_dir_all(&root);
    }
}

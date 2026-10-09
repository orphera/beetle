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

/// Extracts a zip with the built-in reader, and a RAR or 7z with an installed
/// 7-Zip (see ADR-027). Returns the number of regular files written.
pub fn extract_archive<P: AsRef<Path>, Q: AsRef<Path>>(
    archive: P,
    dest_dir: Q,
) -> Result<usize, PackageManagerError> {
    let archive = archive.as_ref();
    let dest_dir = dest_dir.as_ref();
    let ext = archive
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "zip" => extract_zip_archive(archive, dest_dir),
        "rar" | "7z" => {
            let seven_zip = find_seven_zip().ok_or_else(|| {
                PackageManagerError::InvalidPackage(format!(
                    "'{}' needs 7-Zip. Install it, or extract the archive yourself and pass the folder.",
                    archive.display()
                ))
            })?;
            extract_with_seven_zip(&seven_zip, archive, dest_dir)
        }
        _ => Err(PackageManagerError::InvalidPackage(format!(
            "unsupported archive type: '{}'",
            archive.display()
        ))),
    }
}

/// The 7-Zip command line tool, found on `PATH` or in the default install folders.
pub fn find_seven_zip() -> Option<PathBuf> {
    let on_path = std::process::Command::new("7z")
        .arg("i")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if on_path {
        return Some(PathBuf::from("7z"));
    }
    [
        r"C:\Program Files\7-Zip\7z.exe",
        r"C:\Program Files (x86)\7-Zip\7z.exe",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|path| path.is_file())
}

/// Runs 7-Zip to extract `archive` into `dest_dir`, then checks that every
/// extracted file is inside `dest_dir`.
fn extract_with_seven_zip(
    seven_zip: &Path,
    archive: &Path,
    dest_dir: &Path,
) -> Result<usize, PackageManagerError> {
    fs::create_dir_all(dest_dir)?;
    let status = std::process::Command::new(seven_zip)
        .arg("x")
        .arg("-y")
        .arg(format!("-o{}", dest_dir.display()))
        .arg(archive)
        .stdout(std::process::Stdio::null())
        .status()?;
    if !status.success() {
        return Err(PackageManagerError::InvalidPackage(format!(
            "7-Zip could not extract '{}' (exit {status})",
            archive.display()
        )));
    }
    let root = fs::canonicalize(dest_dir)?;
    let mut count = 0;
    let mut pending = vec![root.clone()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir)?.flatten() {
            let path = fs::canonicalize(entry.path())?;
            if !path.starts_with(&root) {
                return Err(PackageManagerError::InvalidPackage(format!(
                    "7-Zip wrote outside the folder: '{}'",
                    path.display()
                )));
            }
            if path.is_dir() {
                pending.push(path);
            } else {
                count += 1;
            }
        }
    }
    Ok(count)
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

#[cfg(test)]
mod archive_dispatch_tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bpm_archive_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn unknown_archive_types_are_refused() {
        let root = scratch("unknown");
        let file = root.join("pack.lzh");
        fs::write(&file, b"x").unwrap();
        assert!(extract_archive(&file, root.join("out")).is_err());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_zip_is_extracted_without_seven_zip() {
        let root = scratch("zip");
        let zip_path = root.join("pack.zip");
        {
            use std::io::Write;
            use zip::write::SimpleFileOptions;
            let mut zip = zip::ZipWriter::new(File::create(&zip_path).unwrap());
            zip.start_file("song/main.bms", SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"#TITLE Song\n").unwrap();
            zip.finish().unwrap();
        }
        let written = extract_archive(&zip_path, root.join("out")).unwrap();
        assert_eq!(written, 1);
        assert!(root.join("out/song/main.bms").is_file());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_seven_zip_archive_is_extracted_when_seven_zip_is_installed() {
        let Some(seven_zip) = find_seven_zip() else {
            return; // 7-Zip is optional; nothing to check on a machine without it.
        };
        let root = scratch("seven");
        let source = root.join("src");
        fs::create_dir_all(source.join("song")).unwrap();
        fs::write(source.join("song/main.bms"), b"#TITLE Song\n").unwrap();
        let archive = root.join("pack.7z");
        let made = std::process::Command::new(&seven_zip)
            .arg("a")
            .arg(&archive)
            .arg(source.join("song"))
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(made.success());
        let written = extract_archive(&archive, root.join("out")).unwrap();
        assert_eq!(written, 1);
        fs::remove_dir_all(&root).ok();
    }
}

//! Fetching the difference (差分) packs that a difficulty table links directly.
//!
//! Only links that answer with a zip file are fetched (see `is_direct_pack`).
//! Every other link is a page, and is left to the user. A pack is unpacked in a
//! scratch folder, and it is kept only when one of its charts has the hash that
//! the table entry names. The whole pack is kept, so its key sounds come with it.

use crate::archive::extract_zip_archive;
use beetle_core::{md5_of_bytes, ChartId, TableEntry};
use std::fs;
use std::path::{Path, PathBuf};

/// Largest pack accepted, compressed. Packs in the Satellite and Stella tables are far smaller.
pub const MAX_PACK_BYTES: u64 = 100 * 1024 * 1024;

const DIRECT_PACK_PREFIXES: &[&str] = &["https://stellabms.xyz/upload/"];
const CHART_EXTENSIONS: &[&str] = &["bms", "bme", "bml", "pms"];

/// True when the link is known to answer with a zip file rather than a page.
pub fn is_direct_pack(url: &str) -> bool {
    DIRECT_PACK_PREFIXES
        .iter()
        .any(|prefix| url.starts_with(prefix))
}

/// True when `bytes` is the chart the entry names: by SHA-256 when the entry has
/// one, otherwise by MD5. An entry with neither matches nothing.
pub fn chart_matches(entry: &TableEntry, bytes: &[u8]) -> bool {
    match (entry.sha256, entry.md5) {
        (Some(sha), _) => ChartId::of_bytes(bytes) == sha,
        (None, Some(md5)) => md5_of_bytes(bytes) == md5,
        (None, None) => false,
    }
}

/// Unpacks the zip at `zip_path` into `scratch`, and, when one of its charts
/// matches `entry`, copies the whole unpacked pack into `pack_dir`. Returns how
/// many charts matched. Nothing is copied when none match.
pub fn keep_pack(
    zip_path: &Path,
    entry: &TableEntry,
    scratch: &Path,
    pack_dir: &Path,
) -> Result<usize, String> {
    let unpacked = scratch.join("unpacked");
    let _ = fs::remove_dir_all(&unpacked);
    extract_zip_archive(zip_path, &unpacked).map_err(|e| format!("cannot unpack: {e}"))?;

    let mut matching = 0;
    for file in chart_files(&unpacked) {
        let bytes = fs::read(&file).map_err(|e| format!("cannot read {}: {e}", file.display()))?;
        if chart_matches(entry, &bytes) {
            matching += 1;
        }
    }
    if matching == 0 {
        return Ok(0);
    }
    copy_dir(&unpacked, pack_dir)?;
    Ok(matching)
}

/// Every chart file under `dir`, in sorted order.
pub fn chart_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_charts(dir, &mut out);
    out.sort();
    out
}

fn collect_charts(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|e| e.path()) {
        if path.is_dir() {
            collect_charts(&path, out);
        } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
            CHART_EXTENSIONS
                .iter()
                .any(|known| e.eq_ignore_ascii_case(known))
        }) {
            out.push(path);
        }
    }
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("cannot create {}: {e}", to.display()))?;
    let entries = fs::read_dir(from).map_err(|e| format!("cannot read {}: {e}", from.display()))?;
    for path in entries.flatten().map(|e| e.path()) {
        let target = to.join(path.file_name().unwrap_or_default());
        if path.is_dir() {
            copy_dir(&path, &target)?;
        } else {
            fs::copy(&path, &target).map_err(|e| format!("cannot copy {}: {e}", path.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    const CHART: &[u8] = b"#TITLE Diff\n#WAV01 kick.wav\n#00111:01\n";

    fn entry_for(bytes: &[u8]) -> TableEntry {
        TableEntry {
            level: "1".into(),
            md5: Some(md5_of_bytes(bytes)),
            sha256: Some(ChartId::of_bytes(bytes)),
            title: "Diff".into(),
            ..TableEntry::default()
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("bpm_table_fetch_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_zip(path: &Path, files: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for (name, bytes) in files {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn only_the_known_zip_hosts_are_direct() {
        assert!(is_direct_pack("https://stellabms.xyz/upload/7967"));
        assert!(!is_direct_pack("https://stellabms.xyz/sl/table.html"));
        assert!(!is_direct_pack("https://drive.google.com/file/d/x/view"));
        assert!(!is_direct_pack(
            "https://evil.example/stellabms.xyz/upload/1"
        ));
    }

    #[test]
    fn a_chart_matches_by_sha256_then_by_md5() {
        let mut entry = entry_for(CHART);
        assert!(chart_matches(&entry, CHART));
        assert!(!chart_matches(&entry, b"other"));
        entry.sha256 = None;
        assert!(chart_matches(&entry, CHART));
        entry.md5 = None;
        assert!(!chart_matches(&entry, CHART));
    }

    #[test]
    fn a_pack_with_the_entry_chart_is_kept_whole() {
        let root = scratch("keep");
        let zip_path = root.join("pack.zip");
        write_zip(
            &zip_path,
            &[
                ("pack/diff.bme", CHART),
                ("pack/kick.wav", b"sound"),
                ("../escape.bme", CHART),
            ],
        );
        let pack_dir = root.join("out");
        let matched =
            keep_pack(&zip_path, &entry_for(CHART), &root.join("work"), &pack_dir).unwrap();
        assert_eq!(matched, 1, "the escaping entry is skipped before matching");
        assert!(pack_dir.join("pack").join("kick.wav").is_file());
        assert!(!root.join("escape.bme").exists());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_pack_without_the_entry_chart_is_not_kept() {
        let root = scratch("miss");
        let zip_path = root.join("pack.zip");
        write_zip(&zip_path, &[("other.bme", b"#TITLE Other\n")]);
        let pack_dir = root.join("out");
        let matched =
            keep_pack(&zip_path, &entry_for(CHART), &root.join("work"), &pack_dir).unwrap();
        assert_eq!(matched, 0);
        assert!(!pack_dir.exists());
        fs::remove_dir_all(&root).ok();
    }
}

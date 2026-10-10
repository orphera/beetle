//! Fetching the difference (差分) packs that a difficulty table links directly.
//!
//! Only links that answer with a zip file are fetched (see `is_direct_pack`).
//! The Stella IR page (`ir.rs`) also lists archives, which may be rar or 7z.
//! Every other link is a page, and is left to the user. A pack is unpacked in a
//! scratch folder, and it is kept only when one of its charts has the hash that
//! the table entry names. The whole pack is kept, so its key sounds come with it.

use crate::archive::extract_archive;
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

/// The archive type a link names by the extension of its last path segment: `zip`,
/// `rar` or `7z`, in lower case. The query string is ignored. `None` for any other name.
pub fn pack_extension(url: &str) -> Option<&'static str> {
    let (_, after_scheme) = url.split_once("://")?;
    let (_, path) = after_scheme.split_once('/')?;
    let path = path.split(['?', '#']).next()?;
    let name = path.rsplit('/').next()?;
    let (_, ext) = name.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    ["zip", "rar", "7z"].into_iter().find(|known| *known == ext)
}

/// True for `http://` and `https://` addresses. Only these are opened in a browser.
pub fn is_web_url(url: &str) -> bool {
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

/// Opens a web address in the default browser. Other schemes are refused.
///
/// On Windows this goes through `rundll32`, not `cmd /C start`, so the `&` in
/// query strings is not read as a command separator.
pub fn open_in_browser(url: &str) -> Result<(), String> {
    if !is_web_url(url) {
        return Err(format!("not a web address: {url}"));
    }
    let status = if cfg!(target_os = "windows") {
        std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .status()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).status()
    } else {
        std::process::Command::new("xdg-open").arg(url).status()
    };
    match status {
        Ok(code) if code.success() => Ok(()),
        Ok(code) => Err(format!("the browser command exited with {code}")),
        Err(e) => Err(format!("cannot start the browser: {e}")),
    }
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

/// What `keep_pack` did with a pack.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct KeptPack {
    /// Charts in the pack whose hash is the entry's.
    pub matching: usize,
    /// Files copied into the target folder.
    pub copied: usize,
    /// Files left alone because the target already had a file of that name.
    pub skipped: usize,
}

/// Unpacks the archive at `archive_path` into `scratch`, and, when one of its charts
/// matches `entry`, copies the whole unpacked pack into `pack_dir`. Files that
/// already exist in `pack_dir` are never overwritten. Nothing is copied when no
/// chart matches.
pub fn keep_pack(
    archive_path: &Path,
    entry: &TableEntry,
    scratch: &Path,
    pack_dir: &Path,
) -> Result<KeptPack, String> {
    let unpacked = scratch.join("unpacked");
    let _ = fs::remove_dir_all(&unpacked);
    extract_archive(archive_path, &unpacked).map_err(|e| format!("cannot unpack: {e}"))?;

    let mut matching = 0;
    for file in chart_files(&unpacked) {
        let bytes = fs::read(&file).map_err(|e| format!("cannot read {}: {e}", file.display()))?;
        if chart_matches(entry, &bytes) {
            matching += 1;
        }
    }
    if matching == 0 {
        return Ok(KeptPack::default());
    }
    let mut kept = KeptPack {
        matching,
        ..KeptPack::default()
    };
    copy_new_files(&unpacked, pack_dir, &mut kept)?;
    Ok(kept)
}

/// How many key sounds a chart declares that are not next to it, as the
/// collection counts them. `None` when the chart cannot be read or parsed.
pub fn missing_key_sounds(chart: &Path) -> Option<u32> {
    let bytes = fs::read(chart).ok()?;
    let names = beetle_core::key_sounds::declared_key_sounds(&bytes)?;
    let dir = chart.parent().unwrap_or_else(|| Path::new("."));
    Some(beetle_core::key_sounds::count_missing(&names, |name| {
        beetle_core::key_sounds::folder_sound_candidates(name)
            .iter()
            .any(|candidate| dir.join(candidate).is_file())
    }))
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

/// Copies every file under `from` into `to`, keeping its relative path, and
/// skips any file whose target already exists.
fn copy_new_files(from: &Path, to: &Path, kept: &mut KeptPack) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("cannot create {}: {e}", to.display()))?;
    let entries = fs::read_dir(from).map_err(|e| format!("cannot read {}: {e}", from.display()))?;
    for path in entries.flatten().map(|e| e.path()) {
        let target = to.join(path.file_name().unwrap_or_default());
        if path.is_dir() {
            copy_new_files(&path, &target, kept)?;
        } else if target.exists() {
            kept.skipped += 1;
        } else {
            fs::copy(&path, &target).map_err(|e| format!("cannot copy {}: {e}", path.display()))?;
            kept.copied += 1;
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
    fn a_link_names_its_archive_type_by_the_last_path_segment() {
        assert_eq!(
            pack_extension("https://web.archive.org/web/2016/junk_qualia.rar"),
            Some("rar")
        );
        assert_eq!(pack_extension("https://a.test/x.ZIP?dl=1"), Some("zip"));
        assert_eq!(pack_extension("https://a.test/x.7z#top"), Some("7z"));
        assert_eq!(pack_extension("https://a.test/page"), None);
        assert_eq!(pack_extension("https://a.zip"), None);
        assert_eq!(pack_extension("https://a.test/x.exe"), None);
    }

    #[test]
    fn only_http_and_https_addresses_open() {
        assert!(is_web_url(
            "https://manbow.nothing.sh/event/event.cgi?a=1&b=2"
        ));
        assert!(is_web_url("HTTP://example.test/x"));
        assert!(!is_web_url("file:///C:/secret.txt"));
        assert!(!is_web_url("javascript:alert(1)"));
        assert!(!is_web_url(""));
        assert!(open_in_browser("file:///C:/secret.txt").is_err());
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
        let kept = keep_pack(&zip_path, &entry_for(CHART), &root.join("work"), &pack_dir).unwrap();
        assert_eq!(
            kept.matching, 1,
            "the escaping entry is skipped before matching"
        );
        assert_eq!(kept.copied, 2);
        assert!(pack_dir.join("pack").join("kick.wav").is_file());
        assert!(!root.join("escape.bme").exists());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn existing_files_are_never_overwritten() {
        let root = scratch("skip");
        let zip_path = root.join("pack.zip");
        write_zip(&zip_path, &[("diff.bme", CHART), ("kick.wav", b"new")]);
        let target = root.join("body");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("kick.wav"), b"body's own").unwrap();
        let kept = keep_pack(&zip_path, &entry_for(CHART), &root.join("work"), &target).unwrap();
        assert_eq!((kept.copied, kept.skipped), (1, 1));
        assert_eq!(fs::read(target.join("kick.wav")).unwrap(), b"body's own");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_key_sounds_counts_only_the_absent_ones() {
        let root = scratch("sounds");
        let chart = root.join("diff.bme");
        fs::write(
            &chart,
            b"#TITLE D
#WAV01 kick.wav
#WAV02 snare.wav
#00111:0102
",
        )
        .unwrap();
        fs::write(root.join("kick.ogg"), b"x").unwrap();
        assert_eq!(
            missing_key_sounds(&chart),
            Some(1),
            "kick.wav is found as kick.ogg"
        );
        fs::write(root.join("snare.wav"), b"x").unwrap();
        assert_eq!(missing_key_sounds(&chart), Some(0));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_pack_without_the_entry_chart_is_not_kept() {
        let root = scratch("miss");
        let zip_path = root.join("pack.zip");
        write_zip(&zip_path, &[("other.bme", b"#TITLE Other\n")]);
        let pack_dir = root.join("out");
        let kept = keep_pack(&zip_path, &entry_for(CHART), &root.join("work"), &pack_dir).unwrap();
        assert_eq!(kept, KeptPack::default());
        assert!(!pack_dir.exists());
        fs::remove_dir_all(&root).ok();
    }
}

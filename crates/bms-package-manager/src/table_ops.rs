//! The difficulty-table actions the CLI and `bpm-gui` share: listing the charts a
//! collection lacks, downloading a difference pack, and importing a body folder.
//!
//! Nothing here prints or asks. Callers decide how to show progress and when to
//! confirm, so the same code runs in a terminal and in the window.

use crate::archive::extract_archive;
use crate::collection::Index;
use crate::manager::{InstalledPackage, PackageManager};
use crate::table_fetch::{self, KeptPack};
use crate::HttpClient;
use beetle_core::{DifficultyTable, TableEntry, TableIndex};
use std::fs;
use std::path::{Path, PathBuf};

/// One entry of a table that the collection does not own (with an intact copy).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingRow {
    /// Position in the table, counted from 1, as the CLI and window show it.
    pub number: usize,
    pub level: String,
    pub title: String,
    pub artist: String,
    pub url: String,
    pub url_diff: String,
    /// A copy exists, but its key sounds are not all next to it: the body is needed.
    pub body_needed: bool,
}

/// The entries of `table` that the collection does not own, in table order.
///
/// A chart counts as owned only with an intact copy (every key sound next to it).
/// An entry whose only copy lacks key sounds is listed with `body_needed` set.
pub fn missing_rows(table: &DifficultyTable, index: &Index) -> Vec<MissingRow> {
    let mut any_copy = TableIndex::new(vec![table.clone()]);
    any_copy.match_songs(index.locations.iter().map(|l| (l.chart, l.md5)));
    let mut intact = TableIndex::new(vec![table.clone()]);
    intact.match_songs(
        index
            .locations
            .iter()
            .filter(|l| l.is_intact())
            .map(|l| (l.chart, l.md5)),
    );
    let with_copy: std::collections::HashSet<usize> =
        any_copy.missing_entries(0).into_iter().collect();
    intact
        .missing_entries(0)
        .into_iter()
        .map(|entry_index| {
            let entry = &table.entries[entry_index];
            MissingRow {
                number: entry_index + 1,
                level: entry.level.clone(),
                title: entry.title.clone(),
                artist: entry.artist.clone(),
                url: entry.url.clone(),
                url_diff: entry.url_diff.clone(),
                body_needed: !with_copy.contains(&entry_index),
            }
        })
        .collect()
}

/// Downloads a difference pack and keeps its charts that match `entry`, copying
/// the pack into `folder` beside its key sounds. `scratch` is a folder this call
/// may use and clear. `folder` must exist.
pub fn fetch_diff(
    client: &HttpClient,
    entry: &TableEntry,
    scratch: &Path,
    folder: &Path,
) -> Result<KeptPack, String> {
    if !table_fetch::is_direct_pack(&entry.url_diff) {
        return Err(format!("not a direct pack link: {}", entry.url_diff));
    }
    let bytes = client
        .get_bytes(&entry.url_diff, table_fetch::MAX_PACK_BYTES)
        .map_err(|e| format!("cannot download the difference pack: {e}"))?;
    fs::create_dir_all(scratch).map_err(|e| e.to_string())?;
    let zip = scratch.join("diff.zip");
    fs::write(&zip, bytes).map_err(|e| e.to_string())?;
    table_fetch::keep_pack(&zip, entry, &scratch.join("diff-work"), folder)
}

/// Unpacks a body archive (zip, or rar and 7z through 7-Zip) into `scratch` and
/// returns the folder that holds the charts: the single folder the archive
/// unpacks to, or the scratch folder itself.
pub fn unpack_body(body_file: &Path, scratch: &Path) -> Result<PathBuf, String> {
    if !body_file.is_file() {
        return Err(format!("'{}' is not a file", body_file.display()));
    }
    let dir = scratch.join("body");
    extract_archive(body_file, &dir).map_err(|e| e.to_string())?;
    Ok(single_top_folder(&dir))
}

/// The first chart under `folder` whose hash is the entry's, if any.
pub fn find_matching_chart(entry: &TableEntry, folder: &Path) -> Option<PathBuf> {
    table_fetch::chart_files(folder)
        .into_iter()
        .find(|file| fs::read(file).is_ok_and(|bytes| table_fetch::chart_matches(entry, &bytes)))
}

/// Imports `folder` as one package, or each song folder under it when there are
/// several. Returns what was installed. Fails when nothing was.
pub fn import_body_folder(
    manager: &mut PackageManager,
    folder: &Path,
) -> Result<Vec<InstalledPackage>, String> {
    let roots = crate::find_bms_song_roots(folder);
    if roots.is_empty() {
        return Err(format!(
            "no BMS chart files found in '{}'",
            folder.display()
        ));
    }
    let mut installed = Vec::new();
    let mut failures = Vec::new();
    for root in &roots {
        match manager.import_folder(root, None) {
            Ok(package) => installed.push(package),
            Err(e) => failures.push(format!("{}: {e}", root.display())),
        }
    }
    if installed.is_empty() {
        return Err(failures.join("; "));
    }
    Ok(installed)
}

/// The only folder inside `dir` when it holds exactly one folder and nothing else.
fn single_top_folder(dir: &Path) -> PathBuf {
    let entries: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    match entries.as_slice() {
        [only] if only.is_dir() => only.clone(),
        _ => dir.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{Index, Location, LocationKind};
    use beetle_core::{md5_of_bytes, ChartId};

    fn chart_entry(n: u8) -> (ChartId, [u8; 16]) {
        let bytes = [n; 8];
        (ChartId::of_bytes(&bytes), md5_of_bytes(&bytes))
    }

    fn table_with(entries: Vec<TableEntry>) -> DifficultyTable {
        DifficultyTable {
            name: "T".into(),
            symbol: "t".into(),
            entries,
            ..DifficultyTable::default()
        }
    }

    fn entry(n: u8) -> TableEntry {
        let (id, md5) = chart_entry(n);
        TableEntry {
            level: format!("{n}"),
            md5: Some(md5),
            sha256: Some(id),
            title: format!("Chart {n}"),
            ..TableEntry::default()
        }
    }

    fn location(n: u8, missing_keys: u32) -> Location {
        let (chart, md5) = chart_entry(n);
        Location {
            chart,
            md5,
            kind: LocationKind::Folder,
            source: "D:/x".into(),
            path: format!("{n}.bms"),
            title: String::new(),
            artist: String::new(),
            play_level: 0,
            mode: String::new(),
            missing_keys,
            key_stems: Vec::new(),
        }
    }

    #[test]
    fn owned_intact_copies_are_not_listed_and_incomplete_ones_need_their_body() {
        let table = table_with(vec![entry(1), entry(2), entry(3)]);
        let index = Index {
            locations: vec![location(1, 0), location(2, 4)],
            ..Index::default()
        };
        let rows = missing_rows(&table, &index);
        let numbers: Vec<usize> = rows.iter().map(|r| r.number).collect();
        assert_eq!(numbers, vec![2, 3]);
        assert!(
            rows[0].body_needed,
            "#2 has a copy, but without its key sounds"
        );
        assert!(!rows[1].body_needed, "#3 has no copy at all");
    }

    #[test]
    fn a_folder_with_one_subfolder_is_the_chart_root() {
        let root = std::env::temp_dir().join(format!("bpm_table_ops_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("only/inner")).unwrap();
        assert_eq!(single_top_folder(&root), root.join("only"));
        fs::create_dir_all(root.join("second")).unwrap();
        assert_eq!(single_top_folder(&root), root);
        fs::remove_dir_all(&root).ok();
    }
}

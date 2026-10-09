//! The difficulty-table actions the CLI and `bpm-gui` share: listing the charts a
//! collection lacks, downloading a difference pack, and importing a body folder.
//!
//! Nothing here prints or asks. Callers decide how to show progress and when to
//! confirm, so the same code runs in a terminal and in the window.

use crate::archive::extract_archive;
use crate::base_match::{self, EntryMeta, Verdict};
use crate::collection::{self, Index, Location};
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

/// What `base_report` found for one table entry.
#[derive(Debug, PartialEq)]
pub struct BaseReport {
    pub verdict: Verdict,
    /// The difference chart, when the index has a copy of it.
    pub diff_copy: Option<beetle_core::ChartId>,
    /// Every place holding a copy of the difference. They are left out of the candidates.
    pub diff_places: Vec<String>,
    /// Sounds the original must supply (the difference's declared sounds minus those its folder has).
    /// Empty when the difference is already complete where it sits.
    pub required: std::collections::BTreeSet<String>,
}

/// Estimates the original of the entry at `number` (counted from 1). Without a
/// local copy of the difference, only the title and artist are used.
pub fn base_report(
    table: &DifficultyTable,
    number: usize,
    index: &Index,
) -> Result<BaseReport, String> {
    base_report_in(table, number, index, &base_match::places_from_index(index))
}

/// `base_report` with the places already built from `index`, for many entries in a row.
/// `places` must come from this same `index` (`base_match::places_from_index`).
pub fn base_report_in(
    table: &DifficultyTable,
    number: usize,
    index: &Index,
    places: &[base_match::Place],
) -> Result<BaseReport, String> {
    let entry = number
        .checked_sub(1)
        .and_then(|i| table.entries.get(i))
        .ok_or_else(|| {
            format!(
                "no entry #{number} in '{}' ({} entries)",
                table.name,
                table.entries.len()
            )
        })?;
    let meta = EntryMeta {
        title: &entry.title,
        artist: &entry.artist,
    };

    let copies: Vec<&Location> = index
        .locations
        .iter()
        .filter(|location| match (entry.sha256, entry.md5) {
            (Some(sha), _) => location.chart == sha,
            (None, Some(md5)) => location.md5 == md5,
            (None, None) => false,
        })
        .collect();
    // Several copies can exist (a folder and a package). The sounds come from an
    // intact one when there is one, since a copy missing sounds declares more than it has.
    let Some(copy) = copies
        .iter()
        .find(|location| location.is_intact())
        .or(copies.first())
    else {
        return Ok(BaseReport {
            verdict: base_match::estimate(meta, &Default::default(), places),
            diff_copy: None,
            diff_places: Vec::new(),
            required: Default::default(),
        });
    };

    // Every place with a copy of the difference is left out, so a copy never
    // matches itself. Sounds its folders already have came with the pack, so the
    // original does not have to supply them; everything else is required.
    let diff_places: std::collections::BTreeSet<String> = copies
        .iter()
        .map(|location| base_match::place_id(location))
        .collect();
    let folder_keys: std::collections::BTreeSet<String> = places
        .iter()
        .filter(|place| diff_places.contains(&place.id))
        .flat_map(|place| place.keys.iter().cloned())
        .collect();
    let required: std::collections::BTreeSet<String> = copy
        .key_stems
        .iter()
        .filter(|stem| !folder_keys.contains(*stem))
        .cloned()
        .collect();
    let verdict = base_match::estimate_excluding(meta, &required, places, &diff_places);
    Ok(BaseReport {
        verdict,
        diff_copy: Some(copy.chart),
        diff_places: diff_places.into_iter().collect(),
        required,
    })
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

/// What a scan found, for the CLI and the window to report.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ScanReport {
    pub folders: usize,
    pub package_states: usize,
    /// Copies of charts, one per file or package entry.
    pub copies: usize,
    /// Distinct charts (by hash) across all copies.
    pub charts: usize,
    /// Charts that have more than one copy.
    pub duplicate_groups: usize,
    pub skipped: u32,
}

/// Scans the library folders, the songs folder, and the active packages in
/// `packages_root` into the collection index, and drops the game's song cache
/// so the game rebuilds it from these sources.
pub fn scan_collection(packages_root: &Path) -> Result<ScanReport, String> {
    let songs_dir = crate::absolute_dir("songs").ok();
    let folders = beetle_core::collection_folders(&crate::load_library(), songs_dir.as_deref());
    let manager = PackageManager::new(packages_root).map_err(|e| e.to_string())?;
    let installed = manager.list_active_packages();
    let (index, counts) = collection::build_index(&folders, &installed);
    let path = collection::index_file();
    fs::write(&path, index.serialize())
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    for cache in [
        PathBuf::from(beetle_core::SONGS_CACHE_FILE),
        songs_dir
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(beetle_core::SONGS_CACHE_FILE),
    ] {
        let _ = fs::remove_file(cache);
    }
    let charts = index
        .locations
        .iter()
        .map(|l| l.chart)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    Ok(ScanReport {
        folders: counts.folders,
        package_states: counts.package_states,
        copies: index.locations.len(),
        charts,
        duplicate_groups: index.duplicate_groups().len(),
        skipped: counts.skipped,
    })
}

/// Gets one table entry from a body archive or folder, without asking: the
/// difference pack goes beside the body's key sounds when its link is direct, and
/// the body folder is imported as a package. Refuses when key sounds are still
/// missing, so nothing half-working is installed. Returns a summary line.
pub fn get_from_body(
    client: &HttpClient,
    entry: &TableEntry,
    body_file: &Path,
    scratch: &Path,
    packages_root: &Path,
) -> Result<String, String> {
    let body_root = unpack_body(body_file, scratch)?;
    if table_fetch::is_direct_pack(&entry.url_diff) {
        let kept = fetch_diff(client, entry, &scratch.join("diff"), &body_root)?;
        if kept.matching == 0 {
            return Err("the difference pack has no chart with this entry's hash".into());
        }
    }
    let chart = find_matching_chart(entry, &body_root)
        .ok_or_else(|| "the body has no chart with this entry's hash".to_string())?;
    let missing = table_fetch::missing_key_sounds(&chart).unwrap_or(0);
    if missing > 0 {
        return Err(format!(
            "{missing} key sound(s) still missing from the body; nothing imported"
        ));
    }
    let mut manager = PackageManager::new(packages_root).map_err(|e| e.to_string())?;
    let installed = import_body_folder(&mut manager, &body_root)?;
    let names: Vec<&str> = installed.iter().map(|p| p.name.as_str()).collect();
    Ok(format!("Imported {}", names.join(", ")))
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

    fn copy_with(
        n: u8,
        source: &str,
        path: &str,
        title: &str,
        missing: u32,
        keys: &[&str],
    ) -> Location {
        let (chart, md5) = chart_entry(n);
        Location {
            chart,
            md5,
            kind: LocationKind::Folder,
            source: source.into(),
            path: path.into(),
            title: title.into(),
            artist: String::new(),
            play_level: 0,
            mode: String::new(),
            missing_keys: missing,
            key_stems: keys.iter().map(|k| k.to_string()).collect(),
        }
    }

    #[test]
    fn base_report_without_a_local_copy_uses_only_metadata() {
        let table = table_with(vec![entry(1)]);
        let report = base_report(&table, 1, &Index::default()).unwrap();
        assert_eq!(report.diff_copy, None);
        assert_eq!(report.verdict, Verdict::MetadataOnly(vec![]));
    }

    #[test]
    fn base_report_leaves_the_difference_folder_out_and_finds_the_original() {
        let table = table_with(vec![entry(1)]);
        // The difference is not intact (its body is missing), so its folder keeps none of its sounds.
        let index = Index {
            locations: vec![
                copy_with(
                    1,
                    "D:/x",
                    "diff/d.bme",
                    "Chart 1",
                    3,
                    &["kick", "snare", "hat", "bass", "lead"],
                ),
                copy_with(
                    9,
                    "D:/x",
                    "orig/o.bme",
                    "Chart 1",
                    0,
                    &["kick", "snare", "hat", "bass", "lead", "pad"],
                ),
            ],
            ..Index::default()
        };
        let report = base_report(&table, 1, &index).unwrap();
        assert_eq!(report.diff_places, vec!["folder:D:/x:diff".to_string()]);
        assert_eq!(report.required.len(), 5);
        match report.verdict {
            Verdict::Confident(candidate) => assert_eq!(candidate.place, "folder:D:/x:orig"),
            other => panic!("expected confident, got {other:?}"),
        }
    }

    #[test]
    fn base_report_leaves_out_every_copy_of_the_difference() {
        let table = table_with(vec![entry(1)]);
        // An incomplete copy in one folder and an intact copy in a package: the
        // intact one supplies the sounds, and neither place may be a candidate.
        let mut packaged = copy_with(
            1,
            "pkg@1",
            "d.bme",
            "Chart 1",
            0,
            &["kick", "snare", "hat", "bass", "lead"],
        );
        packaged.kind = LocationKind::Package;
        let index = Index {
            locations: vec![
                copy_with(1, "D:/x", "diff/d.bme", "Chart 1", 3, &["kick", "snare"]),
                packaged,
                copy_with(
                    9,
                    "D:/x",
                    "orig/o.bme",
                    "Chart 1",
                    0,
                    &["kick", "snare", "hat", "bass", "lead", "pad"],
                ),
            ],
            ..Index::default()
        };
        let report = base_report(&table, 1, &index).unwrap();
        assert_eq!(
            report.diff_places,
            vec!["folder:D:/x:diff".to_string(), "package:pkg@1:".to_string()]
        );
        // The intact package copy already has every sound, so nothing is required from the original.
        assert!(report.required.is_empty());
        assert_eq!(
            report.verdict,
            Verdict::MetadataOnly(vec![base_match::MetadataCandidate {
                place: "folder:D:/x:orig".to_string(),
                evidence: base_match::Evidence::Weak,
            }])
        );
    }

    #[test]
    fn base_report_rejects_an_entry_number_out_of_range() {
        let table = table_with(vec![entry(1)]);
        assert!(base_report(&table, 0, &Index::default()).is_err());
        assert!(base_report(&table, 2, &Index::default()).is_err());
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

#[cfg(test)]
mod get_tests {
    use super::*;
    use beetle_core::{md5_of_bytes, ChartId};
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn entry_for(bytes: &[u8]) -> TableEntry {
        TableEntry {
            level: "1".into(),
            md5: Some(md5_of_bytes(bytes)),
            sha256: Some(ChartId::of_bytes(bytes)),
            title: "T".into(),
            ..TableEntry::default()
        }
    }

    fn zip_with(path: &Path, files: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for (name, bytes) in files {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bpm_get_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_body_without_the_entry_chart_is_refused() {
        let root = scratch("nochart");
        let body = root.join("body.zip");
        zip_with(&body, &[("other.bme", b"#TITLE Other\n#00111:01\n")]);
        let client = HttpClient::new();
        let entry = entry_for(b"#TITLE Wanted\n");
        let err =
            get_from_body(&client, &entry, &body, &root.join("w"), &root.join("p")).unwrap_err();
        assert!(err.contains("no chart"), "{err}");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_chart_with_key_sounds_missing_is_refused_before_any_import() {
        let root = scratch("sounds");
        let chart: &[u8] = b"#TITLE Wanted\n#WAV01 kick.wav\n#00111:01\n";
        let body = root.join("body.zip");
        zip_with(&body, &[("wanted.bme", chart)]);
        let client = HttpClient::new();
        let entry = entry_for(chart);
        let err =
            get_from_body(&client, &entry, &body, &root.join("w"), &root.join("p")).unwrap_err();
        assert!(err.contains("key sound"), "{err}");
        assert!(!root.join("p").exists(), "nothing was installed");
        fs::remove_dir_all(&root).ok();
    }
}

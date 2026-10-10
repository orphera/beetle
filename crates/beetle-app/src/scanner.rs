//! Finds the charts the game plays, and which copy of each chart it loads.
//!
//! Sources are the folders from `beetle_core::collection_folders` (library
//! folders and the songs folder) and the active package states in the
//! registry. A copy of a chart is a file (or package entry) whose bytes have the
//! same `ChartId`; the game lists each chart once, loading the copy that
//! `beetle_core::choose_load_index` picks among the copies in the same order
//! `bpm` uses: folders before packages, then source, then path.

use beetle_core::key_sounds::{count_missing, declared_key_sounds, folder_sound_candidates};
use beetle_core::{
    choose_load_index, collection_folders, deserialize_song_cache, display_path,
    serialize_song_cache, ChartId, LibraryPaths, SongMetadata, SONGS_CACHE_FILE,
};
use bms_package::installed::{self, REGISTRY_FILENAME};
use bms_package::PackageReader;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_SONGS_DIR: &str = "songs";

const SKIP_DIRS: &[&str] = &[".bpm-trash", ".tmp_install"];
const CHART_EXTENSIONS: &[&str] = &["bms", "bme", "bml", "pms"];

/// Where the source list lives for this process, as `bpm library` writes it.
fn library_file() -> PathBuf {
    std::env::var("BEETLE_LIBRARY_FILE")
        .map_or_else(|_| PathBuf::from("library.dat"), PathBuf::from)
}

/// Where a copy comes from. Folders sort before packages, as in `bpm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Folder,
    Package,
}

/// One copy of a chart found by a scan.
struct Copy {
    kind: Kind,
    /// Folder: the folder's absolute path. Package: `<id>@<state hash>`.
    source: String,
    /// Folder: the file's path inside the folder, with `/`. Package: the entry path.
    path: String,
    meta: SongMetadata,
    /// Declared key sounds missing next to this copy. Only worked out for
    /// charts that have more than one copy.
    missing: Option<u32>,
}

/// Lists the charts a scan of `folders` and the active packages finds, one
/// entry per chart, each loaded from the copy that `bpm` would load.
pub fn scan_sources(folders: &[String], packages_root: Option<&Path>) -> Vec<SongMetadata> {
    let mut copies: Vec<Copy> = Vec::new();
    let mut seen_files = HashSet::new();
    for folder in folders {
        let root = Path::new(folder);
        let mut files = Vec::new();
        collect_chart_files(root, &mut files);
        for file in files {
            let canonical = fs::canonicalize(&file).unwrap_or_else(|_| file.clone());
            if !seen_files.insert(canonical) {
                continue;
            }
            let Ok(bytes) = fs::read(&file) else {
                continue;
            };
            let Some(meta) = SongMetadata::from_bytes(&file.to_string_lossy(), &bytes) else {
                continue;
            };
            copies.push(Copy {
                kind: Kind::Folder,
                source: folder.clone(),
                path: relative_path(root, &file),
                meta,
                missing: None,
            });
        }
    }

    let mut readers: BTreeMap<String, PackageReader> = BTreeMap::new();
    if let Some(root) = packages_root {
        for state in installed::read_active_states(root).unwrap_or_default() {
            let Ok(mut reader) = PackageReader::open_file(&state.bmsp_path) else {
                continue;
            };
            let source = format!("{}@{}", state.id, state.state_hash);
            let package_path = state.bmsp_path.to_string_lossy().into_owned();
            let charts: Vec<String> = reader
                .entries()
                .iter()
                .map(|e| e.path.clone())
                .filter(|p| is_chart_path(p))
                .collect();
            for path in charts {
                let Ok(bytes) = reader.read_entry(&path) else {
                    continue;
                };
                let virtual_path = format!("{package_path}::{path}");
                let Some(meta) = SongMetadata::from_bytes(&virtual_path, &bytes) else {
                    continue;
                };
                copies.push(Copy {
                    kind: Kind::Package,
                    source: source.clone(),
                    path,
                    meta,
                    missing: None,
                });
            }
            readers.insert(source, reader);
        }
    }

    let mut by_chart: BTreeMap<ChartId, Vec<Copy>> = BTreeMap::new();
    for copy in copies {
        by_chart.entry(copy.meta.id).or_default().push(copy);
    }
    for group in by_chart.values_mut().filter(|g| g.len() > 1) {
        for copy in group.iter_mut() {
            copy.missing = missing_key_sounds(copy, &mut readers);
        }
    }

    let mut songs = select_copies(by_chart);
    songs.sort_by(|a, b| {
        a.title
            .cmp(&b.title)
            .then_with(|| a.file_path.cmp(&b.file_path))
    });
    songs
}

/// One entry per chart: the copy the game loads, chosen from each group in
/// placement order.
fn select_copies(by_chart: BTreeMap<ChartId, Vec<Copy>>) -> Vec<SongMetadata> {
    by_chart
        .into_values()
        .filter_map(|mut group| {
            group.sort_by(|a, b| (a.kind, &a.source, &a.path).cmp(&(b.kind, &b.source, &b.path)));
            let intact: Vec<bool> = group.iter().map(|c| c.missing == Some(0)).collect();
            let chosen = choose_load_index(&intact)?;
            group.into_iter().nth(chosen).map(|copy| copy.meta)
        })
        .collect()
}

/// Declared key sounds of a copy that are not next to it, as `bpm` counts them.
/// `None` when the copy cannot be read or does not parse.
fn missing_key_sounds(copy: &Copy, readers: &mut BTreeMap<String, PackageReader>) -> Option<u32> {
    match copy.kind {
        Kind::Folder => {
            let file = Path::new(&copy.meta.file_path);
            let bytes = fs::read(file).ok()?;
            let names = declared_key_sounds(&bytes)?;
            let dir = file.parent().unwrap_or_else(|| Path::new("."));
            Some(count_missing(&names, |name| {
                folder_sound_candidates(name)
                    .iter()
                    .any(|candidate| dir.join(candidate).is_file())
            }))
        }
        Kind::Package => {
            let reader = readers.get_mut(&copy.source)?;
            let bytes = reader.read_entry(&copy.path).ok()?;
            let names = declared_key_sounds(&bytes)?;
            let base = copy.path.rsplit_once('/').map_or("", |(dir, _)| dir);
            Some(count_missing(&names, |name| {
                reader.find_entry_path(base, name).is_some()
            }))
        }
    }
}

/// The folders to scan now: `library.dat` and the songs folder.
fn current_folders(songs_dir: &Path) -> Vec<String> {
    let library = LibraryPaths::parse(&fs::read_to_string(library_file()).unwrap_or_default());
    let songs = absolute_dir(songs_dir);
    collection_folders(&library, songs.as_deref())
}

/// Absolute form of an existing folder, as the collection stores it (`display_path`).
fn absolute_dir(path: &Path) -> Option<String> {
    let full = fs::canonicalize(path).ok()?;
    if !full.is_dir() {
        return None;
    }
    Some(display_path(&full.to_string_lossy()).to_string())
}

/// `.bmsp` files directly in the library folders that the game does not play:
/// the package id is not the id of an active registry state, or the manifest
/// cannot be read. Sorted file names. The registry is read only when a folder
/// has a `.bmsp` file, so a library without packages does no extra work.
pub fn uninstalled_packages(folders: &[String], packages_root: &Path) -> Vec<String> {
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    for folder in folders {
        let Ok(entries) = fs::read_dir(folder) else {
            continue;
        };
        for path in entries.flatten().map(|e| e.path()) {
            let is_bmsp = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("bmsp"));
            if is_bmsp && path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    found.push((name.to_string(), path.clone()));
                }
            }
        }
    }
    if found.is_empty() {
        return Vec::new();
    }
    let active: Vec<String> = installed::read_active_states(packages_root)
        .unwrap_or_default()
        .into_iter()
        .map(|state| state.id)
        .collect();
    let manifests: Vec<(String, Option<String>)> = found
        .into_iter()
        .map(|(name, path)| {
            let id = PackageReader::open_file(&path)
                .ok()
                .map(|reader| reader.manifest().id.clone());
            (name, id)
        })
        .collect();
    uninstalled_files(&manifests, &active)
}

/// The decision behind `uninstalled_packages`: each file with its manifest id
/// (`None` when unreadable) and the active registry ids. Returns the sorted,
/// de-duplicated names of the files that are not installed.
pub fn uninstalled_files(files: &[(String, Option<String>)], active_ids: &[String]) -> Vec<String> {
    let mut names: Vec<String> = files
        .iter()
        .filter(|(_, id)| !id.as_ref().is_some_and(|id| active_ids.contains(id)))
        .map(|(name, _)| name.clone())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// `uninstalled_packages` for the library folders the game scans now.
pub fn uninstalled_in_library<P: AsRef<Path>>(songs_dir: P) -> Vec<String> {
    let folders = current_folders(songs_dir.as_ref());
    uninstalled_packages(&folders, &installed::packages_root())
}

/// A cache older than the library list or the registry was built without some
/// folders or with states that are no longer active.
fn cache_is_stale(cache: &Path) -> bool {
    let modified = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).ok();
    let Some(cached) = modified(cache) else {
        return false;
    };
    let registry = installed::packages_root().join(REGISTRY_FILENAME);
    [library_file(), registry]
        .iter()
        .filter_map(|p| modified(p))
        .any(|changed| cached < changed)
}

/// Scans the song folders and packages, using `songs.cache` when it is current.
pub fn load_or_scan_songs<P: AsRef<Path>>(dir: P) -> Vec<SongMetadata> {
    let dir_path = dir.as_ref();
    for candidate in &[
        Path::new(SONGS_CACHE_FILE),
        &dir_path.join(SONGS_CACHE_FILE),
    ] {
        if candidate.exists() && !cache_is_stale(candidate) {
            if let Ok(cache_text) = fs::read_to_string(candidate) {
                let cached = deserialize_song_cache(&cache_text);
                if !cached.is_empty() {
                    return cached;
                }
            }
        }
    }
    force_rescan_songs(dir_path)
}

/// Force rescans the song folders and packages, overwriting `songs.cache`.
pub fn force_rescan_songs<P: AsRef<Path>>(dir: P) -> Vec<SongMetadata> {
    let dir_path = dir.as_ref();
    let folders = current_folders(dir_path);
    let packages = installed::packages_root();
    let songs = scan_sources(&folders, Some(&packages));
    if !songs.is_empty() {
        let cache_data = serialize_song_cache(&songs);
        let _ = fs::write(SONGS_CACHE_FILE, &cache_data);
        if dir_path.is_dir() {
            let _ = fs::write(dir_path.join(SONGS_CACHE_FILE), &cache_data);
        }
    }
    songs
}

fn collect_chart_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !SKIP_DIRS.contains(&name) {
                collect_chart_files(&path, out);
            }
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(is_chart_extension)
        {
            out.push(path);
        }
    }
}

fn is_chart_extension(ext: &str) -> bool {
    CHART_EXTENSIONS
        .iter()
        .any(|known| ext.eq_ignore_ascii_case(known))
}

fn is_chart_path(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, ext)| is_chart_extension(ext))
}

/// `file` relative to `root`, with `/` separators.
fn relative_path(root: &Path, file: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHART: &[u8] = b"#TITLE T\n#BPM 150\n#00111:01\n";

    fn copy(kind: Kind, source: &str, path: &str, file_path: &str, missing: u32) -> Copy {
        Copy {
            kind,
            source: source.to_string(),
            path: path.to_string(),
            meta: SongMetadata::from_bytes(file_path, CHART).unwrap(),
            missing: Some(missing),
        }
    }

    fn group(copies: Vec<Copy>) -> Vec<SongMetadata> {
        let mut by_chart = BTreeMap::new();
        by_chart.insert(copies[0].meta.id, copies);
        select_copies(by_chart)
    }

    #[test]
    fn first_intact_copy_in_placement_order_is_listed() {
        let kept = group(vec![
            copy(Kind::Folder, "D:/b", "x.bms", "D:/b/x.bms", 2),
            copy(Kind::Folder, "D:/a", "x.bms", "D:/a/x.bms", 1),
            copy(Kind::Folder, "D:/c", "x.bms", "D:/c/x.bms", 0),
        ]);
        assert_eq!(kept[0].file_path, "D:/c/x.bms");
    }

    #[test]
    fn folder_copy_beats_package_copy() {
        let kept = group(vec![
            copy(
                Kind::Package,
                "p@s",
                "x.bms",
                "packages/p/s/package.bmsp::x.bms",
                0,
            ),
            copy(Kind::Folder, "D:/a", "x.bms", "D:/a/x.bms", 0),
        ]);
        assert_eq!(kept[0].file_path, "D:/a/x.bms");
    }

    #[test]
    fn a_package_is_uninstalled_unless_its_id_is_active() {
        let files = vec![
            ("AIRSHAVER.bmsp".to_string(), Some("airshaver".to_string())),
            ("Kept.bmsp".to_string(), Some("kept".to_string())),
            ("broken.bmsp".to_string(), None),
            ("Other.bmsp".to_string(), Some("other".to_string())),
        ];
        let active = vec!["kept".to_string()];
        assert_eq!(
            uninstalled_files(&files, &active),
            vec![
                "AIRSHAVER.bmsp".to_string(),
                "Other.bmsp".to_string(),
                "broken.bmsp".to_string(),
            ]
        );
    }

    #[test]
    fn an_unreadable_package_counts_as_uninstalled_even_with_no_registry() {
        let files = vec![("broken.bmsp".to_string(), None)];
        assert_eq!(uninstalled_files(&files, &[]), vec!["broken.bmsp"]);
        assert!(uninstalled_files(&[], &[]).is_empty());
    }

    #[test]
    fn a_folder_without_packages_reports_nothing() {
        let dir = std::env::temp_dir().join(format!("beetle-nobmsp-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.bme"), CHART).unwrap();
        let folders = vec![dir.to_string_lossy().into_owned()];
        assert!(uninstalled_packages(&folders, &dir.join("no-packages")).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn falls_back_to_the_first_copy_when_none_is_intact() {
        let kept = group(vec![
            copy(Kind::Folder, "D:/b", "x.bms", "D:/b/x.bms", 1),
            copy(Kind::Folder, "D:/a", "x.bms", "D:/a/x.bms", 3),
        ]);
        assert_eq!(kept[0].file_path, "D:/a/x.bms");
    }

    #[test]
    fn game_and_bpm_choose_the_same_copy() {
        let dir = std::env::temp_dir().join(format!("beetle-cross-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let one = dir.join("one");
        let two = dir.join("two");
        fs::create_dir_all(&one).unwrap();
        fs::create_dir_all(&two).unwrap();
        let chart = b"#TITLE Same\n#WAV01 kick.wav\n#00111:01\n";
        fs::write(one.join("a.bme"), chart).unwrap();
        fs::write(two.join("a.bme"), chart).unwrap();
        fs::write(two.join("kick.wav"), b"x").unwrap();
        let folders = vec![
            one.to_string_lossy().into_owned(),
            two.to_string_lossy().into_owned(),
        ];

        let game = scan_sources(&folders, None);
        let (index, _) = bms_package_manager::collection::build_index(&folders, &[]);
        let chart_id = game[0].id;
        let copies: Vec<&bms_package_manager::collection::Location> = index
            .locations
            .iter()
            .filter(|l| l.chart == chart_id)
            .collect();
        let loaded =
            bms_package_manager::collection::Index::load_index(&copies).expect("bpm picks a copy");
        let bpm_file = Path::new(&copies[loaded].source).join(&copies[loaded].path);
        assert_eq!(Path::new(&game[0].file_path), bpm_file);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_is_scanned_in_the_same_order_bpm_uses() {
        let dir = std::env::temp_dir().join(format!("beetle-scan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let one = dir.join("one");
        let two = dir.join("two");
        fs::create_dir_all(&one).unwrap();
        fs::create_dir_all(&two).unwrap();
        // Same bytes in both folders. Only `two` has the declared key sound.
        let chart = b"#TITLE Same\n#WAV01 kick.wav\n#00111:01\n";
        fs::write(one.join("a.bme"), chart).unwrap();
        fs::write(two.join("a.bme"), chart).unwrap();
        fs::write(two.join("kick.wav"), b"x").unwrap();

        let folders = vec![
            one.to_string_lossy().into_owned(),
            two.to_string_lossy().into_owned(),
        ];
        let songs = scan_sources(&folders, None);
        assert_eq!(songs.len(), 1);
        assert_eq!(Path::new(&songs[0].file_path), two.join("a.bme"));
        let _ = fs::remove_dir_all(&dir);
    }
}

//! Finds the charts the game plays, and which copy of each chart it loads.
//!
//! Sources are the folders from `beetle_core::collection_folders` (library
//! folders and the songs folder), the active package states in the registry,
//! and the game discs listed in the library (`.iso` images and drives, read
//! with `cs_disc`). A copy of a chart is a file (or package entry) whose bytes have the
//! same `ChartId`; the game lists each chart once, loading the copy that
//! `beetle_core::choose_load_index` picks among the copies in the same order
//! `bpm` uses: folders before packages before discs, then source, then path.

use crate::boot_progress::{self, Stage};
use beetle_core::key_sounds::{count_missing, declared_key_sounds, folder_sound_candidates};
use beetle_core::{
    choose_load_index, collection_folders, deserialize_song_cache, disc_address, display_path,
    package_address, serialize_song_cache, ChartId, LibraryPaths, SongMetadata, SONGS_CACHE_FILE,
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
pub fn library_file() -> PathBuf {
    std::env::var("BEETLE_LIBRARY_FILE")
        .map_or_else(|_| PathBuf::from("library.dat"), PathBuf::from)
}

/// Where a copy comes from. Folders sort before packages, as in `bpm`, and
/// discs last: a chart that is also installed loads from the installed copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Folder,
    Package,
    Disc,
}

/// One copy of a chart found by a scan.
struct Copy {
    kind: Kind,
    /// Folder: the folder's absolute path. Package: `<id>@<state hash>`.
    /// Disc: the image or drive.
    source: String,
    /// Folder: the file's path inside the folder, with `/`. Package: the entry
    /// path. Disc: the chart's key on the disc.
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
    // Every chart file first (a file reached twice is read once), then read
    // and parsed on several threads; the copies keep this order.
    // A file's identity is its folder's canonical path plus its path inside:
    // a folder listed inside another one is read once, without asking the
    // file system for every file's canonical path.
    let mut jobs: Vec<(&String, PathBuf)> = Vec::new();
    let (discs, folders): (Vec<&String>, Vec<&String>) =
        folders.iter().partition(|f| cs_disc::is_disc_target(f));
    boot_progress::begin(Stage::Listing, 0);
    for folder in folders {
        let root = Path::new(folder);
        let canonical_root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let mut files = Vec::new();
        collect_chart_files(root, &mut files);
        for file in files {
            let inside = file.strip_prefix(root).unwrap_or(&file);
            let key = canonical_root.join(inside).to_string_lossy().to_lowercase();
            if seen_files.insert(key) {
                jobs.push((folder, file));
            }
        }
    }
    boot_progress::begin(Stage::Charts, jobs.len());
    let read = read_charts_parallel(&jobs);
    for ((folder, file), meta) in jobs.iter().zip(read) {
        if let Some(meta) = meta {
            copies.push(Copy {
                kind: Kind::Folder,
                source: (*folder).clone(),
                path: relative_path(Path::new(folder), file),
                meta,
                missing: None,
            });
        }
    }

    let mut readers: BTreeMap<String, PackageReader> = BTreeMap::new();
    if let Some(root) = packages_root {
        let states = installed::read_active_states(root).unwrap_or_default();
        boot_progress::begin(Stage::Packages, states.len());
        for (source, reader, found) in read_packages_parallel(&states).into_iter().flatten() {
            copies.extend(found);
            readers.insert(source, reader);
        }
    }

    boot_progress::begin(Stage::Discs, discs.len());
    copies.extend(read_discs_parallel(&discs));

    let mut by_chart: BTreeMap<ChartId, Vec<Copy>> = BTreeMap::new();
    for copy in copies {
        by_chart.entry(copy.meta.id).or_default().push(copy);
    }
    boot_progress::begin(Stage::Copies, 0);
    check_copies_parallel(&mut by_chart, readers);

    let mut songs = select_copies(by_chart);
    songs.sort_by(|a, b| {
        a.title
            .cmp(&b.title)
            .then_with(|| a.file_path.cmp(&b.file_path))
    });
    songs
}

/// How many threads a stage of the scan uses for `jobs` items.
fn workers_for(jobs: usize) -> usize {
    std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .clamp(1, 8)
        .min(jobs.max(1))
}

/// A package's source (`<id>@<state hash>`), its reader and its chart copies.
type PackageCopies = (String, PackageReader, Vec<Copy>);

/// Opens each active package state and reads its charts, on several threads.
/// The result is in `states` order; `None` for a package that cannot be opened.
fn read_packages_parallel(states: &[installed::ActiveState]) -> Vec<Option<PackageCopies>> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<PackageCopies>>> =
        Mutex::new((0..states.len()).map(|_| None).collect());
    std::thread::scope(|scope| {
        for _ in 0..workers_for(states.len()) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(state) = states.get(i) else {
                    break;
                };
                let read = read_package(state);
                if let Ok(mut out) = out.lock() {
                    out[i] = read;
                }
                boot_progress::step();
            });
        }
    });
    out.into_inner().unwrap_or_default()
}

fn read_package(state: &installed::ActiveState) -> Option<PackageCopies> {
    let mut reader = PackageReader::open_file(&state.bmsp_path).ok()?;
    let source = format!("{}@{}", state.id, state.state_hash);
    let package_path = state.bmsp_path.to_string_lossy().into_owned();
    let charts: Vec<String> = reader
        .entries()
        .iter()
        .map(|e| e.path.clone())
        .filter(|p| is_chart_path(p))
        .collect();
    let mut copies = Vec::new();
    for path in charts {
        let Ok(bytes) = reader.read_entry(&path) else {
            continue;
        };
        let virtual_path = package_address(&package_path, &path);
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
    Some((source, reader, copies))
}

/// Reads the charts of each disc, one disc per thread, in `discs` order.
fn read_discs_parallel(discs: &[&String]) -> Vec<Copy> {
    std::thread::scope(|scope| {
        let reads: Vec<_> = discs
            .iter()
            .map(|target| scope.spawn(move || read_disc(target)))
            .collect();
        reads
            .into_iter()
            .flat_map(|read| {
                boot_progress::step();
                read.join().unwrap_or_default()
            })
            .collect()
    })
}

/// The charts on a game disc, each a copy with nothing missing. Nothing for
/// a disc that cannot be read (an empty drive) or a game `cs_disc` does not know.
fn read_disc(target: &str) -> Vec<Copy> {
    // A drive is stored as `E:`, however the library spells it.
    let source = match cs_disc::drive::as_drive_letter(target) {
        Some(letter) => format!("{letter}:"),
        None => target.to_string(),
    };
    let Ok(mut disc) = cs_disc::open(&source) else {
        return Vec::new();
    };
    let Ok(Some(edition)) = cs_disc::identify(&mut disc) else {
        if let Ok(Some(title)) = cs_disc::identify_title(&mut disc) {
            println!(
                "[library] {source}: {} ({}) is not read yet",
                title.name, title.code
            );
        }
        return Vec::new();
    };
    read_disc_songs(source, disc, edition)
}

fn read_disc_songs(
    source: String,
    mut disc: cs_disc::SectorReader<std::fs::File>,
    edition: cs_disc::Edition,
) -> Vec<Copy> {
    use cs_disc::iidx::Disc;
    let Ok(volume) = cs_disc::Volume::open(&mut disc) else {
        return Vec::new();
    };
    let Ok(songs) = Disc::open(&mut disc, &volume, edition) else {
        return Vec::new();
    };
    let mut copies = Vec::new();
    let mut charts = Vec::new();
    for song in &songs.songs {
        for chart in &song.charts {
            // The identity comes from the disc alone.
            let Ok(bms) = songs.chart_bms(&mut disc, song, chart) else {
                continue;
            };
            let key = chart.key(song);
            let address = disc_address(&source, &key);
            if let Some(meta) = SongMetadata::from_bytes(&address, bms.as_bytes()) {
                charts.push((meta.id, song.id));
                copies.push(Copy {
                    kind: Kind::Disc,
                    source: source.clone(),
                    path: key,
                    meta,
                    missing: Some(0),
                });
            }
        }
    }
    crate::names::ensure(&mut disc, &songs, edition.slug(), &source, &charts);
    copies
}

/// Works out `missing` for every copy of the charts found more than once, on
/// several threads: the folder copies one at a time, the package copies one
/// package at a time (a reader is not shared).
fn check_copies_parallel(
    by_chart: &mut BTreeMap<ChartId, Vec<Copy>>,
    mut readers: BTreeMap<String, PackageReader>,
) {
    use std::sync::Mutex;

    enum Job<'a> {
        Folder(&'a mut Copy),
        Package(PackageReader, Vec<&'a mut Copy>),
    }
    let mut by_source: BTreeMap<String, Vec<&mut Copy>> = BTreeMap::new();
    let mut jobs: Vec<Job> = Vec::new();
    for copy in by_chart
        .values_mut()
        .filter(|g| g.len() > 1)
        .flat_map(|g| g.iter_mut())
    {
        match copy.kind {
            Kind::Folder => jobs.push(Job::Folder(copy)),
            Kind::Package => by_source.entry(copy.source.clone()).or_default().push(copy),
            // A disc chart's sounds are on the disc.
            Kind::Disc => {}
        }
    }
    for (source, copies) in by_source {
        if let Some(reader) = readers.remove(&source) {
            jobs.push(Job::Package(reader, copies));
        }
    }
    let workers = workers_for(jobs.len());
    let queue = Mutex::new(jobs);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                let mut lookups = Lookups::default();
                loop {
                    let job = match queue.lock() {
                        Ok(mut q) => q.pop(),
                        Err(_) => None,
                    };
                    match job {
                        None => break,
                        Some(Job::Folder(copy)) => {
                            copy.missing = folder_missing(copy, &mut lookups);
                        }
                        Some(Job::Package(mut reader, copies)) => {
                            let index = reader.entry_index();
                            for copy in copies {
                                copy.missing = package_missing(copy, &mut reader, &index);
                            }
                        }
                    }
                }
            });
        }
    });
}

/// Reads and parses each chart file on several threads. The result is in
/// `jobs` order; `None` for a file that cannot be read or is not a chart.
fn read_charts_parallel(jobs: &[(&String, PathBuf)]) -> Vec<Option<SongMetadata>> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<SongMetadata>>> = Mutex::new(vec![None; jobs.len()]);
    std::thread::scope(|scope| {
        for _ in 0..workers_for(jobs.len()) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some((_, file)) = jobs.get(i) else {
                    break;
                };
                let meta = fs::read(file)
                    .ok()
                    .and_then(|bytes| SongMetadata::from_bytes(&file.to_string_lossy(), &bytes));
                if let Ok(mut out) = out.lock() {
                    out[i] = meta;
                }
                boot_progress::step();
            });
        }
    });
    out.into_inner().unwrap_or_default()
}

/// One entry per chart: the copy the game loads, chosen from each group in
/// placement order, naming the chart's other disc copies (`also_on`).
fn select_copies(by_chart: BTreeMap<ChartId, Vec<Copy>>) -> Vec<SongMetadata> {
    by_chart
        .into_values()
        .filter_map(|mut group| {
            group.sort_by(|a, b| (a.kind, &a.source, &a.path).cmp(&(b.kind, &b.source, &b.path)));
            let intact: Vec<bool> = group.iter().map(|c| c.missing == Some(0)).collect();
            let chosen = choose_load_index(&intact)?;
            let also_on = group
                .iter()
                .enumerate()
                .filter(|&(i, c)| i != chosen && c.kind == Kind::Disc)
                .map(|(_, c)| c.meta.file_path.clone())
                .collect();
            let mut meta = group.into_iter().nth(chosen)?.meta;
            meta.also_on = also_on;
            Some(meta)
        })
        .collect()
}

/// The files directly in each folder the copy check has looked in, by
/// lowercased name, so a chart's key sounds are answered from memory.
#[derive(Default)]
struct Lookups {
    folders: BTreeMap<PathBuf, HashSet<String>>,
}

impl Lookups {
    /// Whether `candidate` (a path relative to `dir`) is a file, as
    /// `dir.join(candidate).is_file()` answers on Windows (any case).
    fn folder_has(&mut self, dir: &Path, candidate: &str) -> bool {
        if candidate.contains('/') {
            return dir.join(candidate).is_file();
        }
        let files = self.folders.entry(dir.to_path_buf()).or_insert_with(|| {
            fs::read_dir(dir)
                .map(|entries| {
                    entries
                        .flatten()
                        .filter(|e| e.file_type().is_ok_and(|t| !t.is_dir()))
                        .map(|e| e.file_name().to_string_lossy().to_lowercase())
                        .collect()
                })
                .unwrap_or_default()
        });
        files.contains(&candidate.to_lowercase())
    }
}

/// Declared key sounds of a folder copy that are not next to it, as `bpm`
/// counts them. `None` when the copy cannot be read or does not parse.
fn folder_missing(copy: &Copy, lookups: &mut Lookups) -> Option<u32> {
    let file = Path::new(&copy.meta.file_path);
    let bytes = fs::read(file).ok()?;
    let names = declared_key_sounds(&bytes)?;
    let dir = file.parent().unwrap_or_else(|| Path::new("."));
    Some(count_missing(&names, |name| {
        folder_sound_candidates(name)
            .iter()
            .any(|candidate| lookups.folder_has(dir, candidate))
    }))
}

/// `folder_missing` for a copy inside a package (`index` is the package's).
fn package_missing(
    copy: &Copy,
    reader: &mut PackageReader,
    index: &bms_package::EntryIndex,
) -> Option<u32> {
    let bytes = reader.read_entry(&copy.path).ok()?;
    let names = declared_key_sounds(&bytes)?;
    let base = copy.path.rsplit_once('/').map_or("", |(dir, _)| dir);
    Some(count_missing(&names, |name| index.has(base, name)))
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
    let stale_names = crate::names::any_unreadable();
    stale_names
        || [library_file(), registry]
            .into_iter()
            .filter_map(|p| modified(&p))
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
    // The entry's own type: no second look at the disk for each file.
    let mut entries: Vec<(PathBuf, bool)> = entries
        .flatten()
        .map(|e| {
            let is_dir = match e.file_type() {
                // A link to a folder is followed, as before.
                Ok(t) if t.is_symlink() => e.path().is_dir(),
                Ok(t) => t.is_dir(),
                Err(_) => false,
            };
            (e.path(), is_dir)
        })
        .collect();
    entries.sort();
    for (path, is_dir) in entries {
        if is_dir {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !SKIP_DIRS.contains(&name) {
                collect_chart_files(&path, out);
            }
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(is_chart_extension)
        {
            boot_progress::step();
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

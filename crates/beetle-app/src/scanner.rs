//! Finds the charts the game plays, and which copy of each chart it loads.
//!
//! Sources are the folders from `beetle_core::collection_folders` (library
//! folders and the songs folder), the active package states in the registry,
//! and the game discs listed in the library (`.iso` images and drives, read
//! with `cs_disc`). A copy of a chart is a file (or package entry) whose bytes have the
//! same `ChartId`; the game lists each chart once, loading the copy that
//! `beetle_core::choose_load_index` picks among the copies in the same order
//! `bpm` uses: folders before packages, then source, then path. Each game
//! disc a chart is on lists it again, as that disc's song.

use crate::boot_progress::{self, Stage};
use beetle_core::key_sounds::{count_missing, declared_key_sounds, folder_sound_candidates};
use beetle_core::{
    choose_load_index, collection_folders, deserialize_song_cache, disc_address, display_path,
    package_address, serialize_song_cache, ChartId, LibraryPaths, SongAddress, SongMetadata,
    SONGS_CACHE_FILE,
};
use bms_package::installed::{self, REGISTRY_FILENAME};
use bms_package::PackageReader;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub const DEFAULT_SONGS_DIR: &str = "songs";

const SKIP_DIRS: &[&str] = &[".bpm-trash", ".tmp_install"];
const CHART_EXTENSIONS: &[&str] = &["bms", "bme", "bml", "pms"];
/// Per-source metadata and file stamps used to update only changed charts.
const SOURCE_CACHE_FILE: &str = "songs.sources";

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    length: u64,
    modified_ns: u128,
}

#[derive(Debug, Clone, PartialEq)]
struct CachedCopy {
    kind: Kind,
    source: String,
    path: String,
    meta: SongMetadata,
    missing: Option<u32>,
    stamp: Option<FileStamp>,
}

#[derive(Debug, Default, PartialEq)]
struct SourceCache {
    folder_fingerprints: BTreeMap<String, u64>,
    copies: Vec<CachedCopy>,
}

impl CachedCopy {
    fn from_copy(copy: &Copy) -> Self {
        let stamp = match copy.kind {
            Kind::Folder => file_stamp(Path::new(&copy.meta.file_path)),
            Kind::Package => file_stamp(Path::new(
                SongAddress::parse(&copy.meta.file_path).container(),
            )),
            Kind::Disc => None,
        };
        Self {
            kind: copy.kind,
            source: copy.source.clone(),
            path: copy.path.clone(),
            meta: copy.meta.clone(),
            missing: copy.missing,
            stamp,
        }
    }

    fn into_copy(self) -> Copy {
        Copy {
            kind: self.kind,
            source: self.source,
            path: self.path,
            meta: self.meta,
            missing: self.missing,
        }
    }
}

fn file_stamp(path: &Path) -> Option<FileStamp> {
    let metadata = fs::metadata(path).ok()?;
    let modified_ns = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(FileStamp {
        length: metadata.len(),
        modified_ns,
    })
}

fn source_cache_path() -> PathBuf {
    PathBuf::from(SOURCE_CACHE_FILE)
}

fn load_source_cache() -> Option<SourceCache> {
    let text = fs::read_to_string(source_cache_path()).ok()?;
    parse_source_cache(&text)
}

fn parse_source_cache(text: &str) -> Option<SourceCache> {
    let mut lines = text.lines();
    if lines.next()? != "#BEETLE_SOURCES_V1" {
        return None;
    }
    let mut cache = SourceCache::default();
    for line in lines {
        let fields: Vec<_> = line.split('\t').collect();
        match fields.as_slice() {
            ["dir", root, fingerprint] => {
                cache.folder_fingerprints.insert(
                    String::from_utf8(hex_decode(root)?).ok()?,
                    u64::from_str_radix(fingerprint, 16).ok()?,
                );
            }
            [kind, source, path, missing, length, modified, metadata] => {
                let kind = match *kind {
                    "F" => Kind::Folder,
                    "P" => Kind::Package,
                    _ => return None,
                };
                let source = String::from_utf8(hex_decode(source)?).ok()?;
                let path = String::from_utf8(hex_decode(path)?).ok()?;
                let metadata = String::from_utf8(hex_decode(metadata)?).ok()?;
                let meta = SongMetadata::parse_line(&metadata)?;
                let missing = if *missing == "-" {
                    None
                } else {
                    Some(missing.parse().ok()?)
                };
                let stamp = if kind != Kind::Disc {
                    Some(FileStamp {
                        length: length.parse().ok()?,
                        modified_ns: modified.parse().ok()?,
                    })
                } else {
                    None
                };
                cache.copies.push(CachedCopy {
                    kind,
                    source,
                    path,
                    meta,
                    missing,
                    stamp,
                });
            }
            _ => return None,
        }
    }
    Some(cache)
}

fn serialize_source_cache(cache: &SourceCache) -> String {
    use std::fmt::Write;

    let mut text = String::from("#BEETLE_SOURCES_V1\n");
    for (root, fingerprint) in &cache.folder_fingerprints {
        let _ = writeln!(
            text,
            "dir\t{}\t{fingerprint:016x}",
            hex_encode(root.as_bytes())
        );
    }
    for copy in &cache.copies {
        let kind = match copy.kind {
            Kind::Folder => "F",
            Kind::Package => "P",
            Kind::Disc => continue,
        };
        let (length, modified) = copy
            .stamp
            .map_or((0, 0), |stamp| (stamp.length, stamp.modified_ns));
        let missing = copy
            .missing
            .map_or_else(|| "-".to_string(), |n| n.to_string());
        let _ = writeln!(
            text,
            "{kind}\t{}\t{}\t{missing}\t{length}\t{modified}\t{}",
            hex_encode(copy.source.as_bytes()),
            hex_encode(copy.path.as_bytes()),
            hex_encode(copy.meta.serialize_line().as_bytes())
        );
    }
    text
}

fn write_source_cache(cache: &SourceCache) {
    let text = serialize_source_cache(cache);
    if let Err(error) = fs::write(source_cache_path(), text) {
        eprintln!("[library] failed to write source cache: {error}");
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 15) as usize] as char);
    }
    out
}

fn hex_decode(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |byte: u8| (byte as char).to_digit(16).map(|d| d as u8);
            Some(digit(pair[0])? * 16 + digit(pair[1])?)
        })
        .collect()
}

/// Lists the charts a scan of `folders` and the active packages finds, one
/// entry per chart, each loaded from the copy that `bpm` would load.
#[cfg(test)]
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

    finish_scan(copies, readers, None).0
}

fn scan_sources_incremental(
    folders: &[String],
    packages_root: Option<&Path>,
    previous: Option<SourceCache>,
) -> (Vec<SongMetadata>, SourceCache) {
    let previous = previous.unwrap_or_default();
    let mut old_folders = HashMap::new();
    let mut old_packages: BTreeMap<String, Vec<CachedCopy>> = BTreeMap::new();
    for cached in previous.copies {
        match cached.kind {
            Kind::Folder => {
                old_folders.insert((cached.source.clone(), cached.path.clone()), cached);
            }
            Kind::Package => old_packages
                .entry(cached.source.clone())
                .or_default()
                .push(cached),
            Kind::Disc => {}
        }
    }

    let mut copies = Vec::new();
    let mut changed_jobs = Vec::new();
    let mut changed_info = Vec::new();
    let mut seen_files = HashSet::new();
    let mut refresh_folders = HashSet::new();
    let mut current_stamps = HashMap::new();
    let mut source_cache = SourceCache::default();
    let (discs, folders): (Vec<&String>, Vec<&String>) =
        folders.iter().partition(|f| cs_disc::is_disc_target(f));
    boot_progress::begin(Stage::Listing, 0);
    for folder in folders {
        let root = Path::new(folder);
        let canonical_root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let mut files = Vec::new();
        let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
        collect_folder_files(root, root, &mut files, &mut fingerprint);
        let fingerprint = fingerprint.finish();
        source_cache
            .folder_fingerprints
            .insert(folder.clone(), fingerprint);
        if previous.folder_fingerprints.get(folder) != Some(&fingerprint) {
            refresh_folders.insert(folder.clone());
        }
        for file in files {
            let inside = file.strip_prefix(root).unwrap_or(&file);
            let key = canonical_root.join(inside).to_string_lossy().to_lowercase();
            if !seen_files.insert(key) {
                continue;
            }
            let path = relative_path(root, &file);
            let stamp = file_stamp(&file);
            current_stamps.insert((folder.clone(), path.clone()), stamp);
            let cache_key = (folder.clone(), path.clone());
            if let Some(cached) = old_folders.remove(&cache_key) {
                if cached.stamp == stamp && stamp.is_some() {
                    copies.push(cached.into_copy());
                    continue;
                }
            }
            changed_jobs.push((folder, file));
            changed_info.push((folder.clone(), path));
        }
    }
    boot_progress::begin(Stage::Charts, changed_jobs.len());
    for ((source, path), meta) in changed_info
        .into_iter()
        .zip(read_charts_parallel(&changed_jobs))
    {
        if let Some(meta) = meta {
            copies.push(Copy {
                kind: Kind::Folder,
                source: source.clone(),
                path: path.clone(),
                meta,
                missing: None,
            });
        }
    }

    let mut readers = BTreeMap::new();
    if let Some(root) = packages_root {
        let states = installed::read_active_states(root).unwrap_or_default();
        boot_progress::begin(Stage::Packages, states.len());
        let mut changed_states = Vec::new();
        for state in states {
            let source = format!("{}@{}", state.id, state.state_hash);
            if let Some(cached) = old_packages.remove(&source) {
                let package_path = state.bmsp_path.to_string_lossy();
                let current_stamp = file_stamp(&state.bmsp_path);
                let unchanged = cached.iter().all(|copy| {
                    copy.stamp == current_stamp
                        && copy.meta.file_path == package_address(&package_path, &copy.path)
                });
                match unchanged
                    .then(|| PackageReader::open_file(&state.bmsp_path))
                    .transpose()
                {
                    Ok(Some(reader)) => {
                        copies.extend(cached.into_iter().map(CachedCopy::into_copy));
                        readers.insert(source, reader);
                        boot_progress::step();
                    }
                    Ok(None) | Err(_) => changed_states.push(state),
                }
            } else {
                changed_states.push(state);
            }
        }
        for (source, reader, found) in read_packages_parallel(&changed_states)
            .into_iter()
            .flatten()
        {
            copies.extend(found);
            readers.insert(source, reader);
        }
    }

    boot_progress::begin(Stage::Discs, discs.len());
    copies.extend(read_discs_parallel(&discs));
    let (songs, mut cached_copies) = finish_scan(copies, readers, Some(&refresh_folders));
    for cached in &mut cached_copies {
        if cached.kind == Kind::Folder {
            let key = (cached.source.clone(), cached.path.clone());
            let captured = current_stamps.get(&key).copied().flatten();
            cached.stamp = captured
                .filter(|stamp| file_stamp(Path::new(&cached.meta.file_path)) == Some(*stamp));
        }
    }
    source_cache.copies = cached_copies;
    (songs, source_cache)
}

fn finish_scan(
    copies: Vec<Copy>,
    readers: BTreeMap<String, PackageReader>,
    refresh_folders: Option<&HashSet<String>>,
) -> (Vec<SongMetadata>, Vec<CachedCopy>) {
    let mut by_chart: BTreeMap<ChartId, Vec<Copy>> = BTreeMap::new();
    for copy in copies {
        by_chart.entry(copy.meta.id).or_default().push(copy);
    }
    boot_progress::begin(Stage::Copies, 0);
    check_copies_parallel(&mut by_chart, readers, refresh_folders);
    let cached = by_chart
        .values()
        .flatten()
        .map(CachedCopy::from_copy)
        .collect();
    let mut songs = select_copies(by_chart);
    songs.sort_by(|a, b| {
        a.title
            .cmp(&b.title)
            .then_with(|| a.file_path.cmp(&b.file_path))
    });
    (songs, cached)
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
    let trace_disc = std::env::var_os("BEETLE_DISC_TRACE").is_some();
    if trace_disc {
        disc.enable_trace();
    }
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
                charts.push((meta.picture_key(), song.id));
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
    if let Some(stats) = disc.read_stats() {
        println!(
            "[disc] scan seeks={} sectors={} locations={:?}",
            stats.seeks, stats.sectors, stats.locations
        );
    }
    copies
}

/// Works out `missing` for every copy of the charts found more than once, on
/// several threads: the folder copies one at a time, the package copies one
/// package at a time (a reader is not shared).
fn check_copies_parallel(
    by_chart: &mut BTreeMap<ChartId, Vec<Copy>>,
    mut readers: BTreeMap<String, PackageReader>,
    refresh_folders: Option<&HashSet<String>>,
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
        let needs_check = match copy.kind {
            Kind::Folder => {
                copy.missing.is_none()
                    || refresh_folders.is_none_or(|folders| folders.contains(&copy.source))
            }
            Kind::Package => copy.missing.is_none() || refresh_folders.is_none(),
            Kind::Disc => false,
        };
        if !needs_check {
            continue;
        }
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

/// One entry per chart, loaded from the copy `bpm` would load among its
/// folder and package copies, and one more for each game disc it is on: a
/// disc lists its own songs, with its own pictures, apart from other discs.
fn select_copies(by_chart: BTreeMap<ChartId, Vec<Copy>>) -> Vec<SongMetadata> {
    let mut out = Vec::new();
    for (_, mut group) in by_chart {
        group.sort_by(|a, b| (a.kind, &a.source, &a.path).cmp(&(b.kind, &b.source, &b.path)));
        let (discs, files): (Vec<Copy>, Vec<Copy>) =
            group.into_iter().partition(|c| c.kind == Kind::Disc);
        let intact: Vec<bool> = files.iter().map(|c| c.missing == Some(0)).collect();
        if let Some(chosen) = choose_load_index(&intact) {
            out.extend(files.into_iter().nth(chosen).map(|c| c.meta));
        }
        out.extend(discs.into_iter().map(|c| c.meta));
    }
    out
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

/// Rechecks folder and package sources against `songs.sources`, then overwrites
/// the visible song list cache. Discs are identified on every rescan.
pub fn force_rescan_songs<P: AsRef<Path>>(dir: P) -> Vec<SongMetadata> {
    let dir_path = dir.as_ref();
    let folders = current_folders(dir_path);
    let packages = installed::packages_root();
    let (songs, source_cache) =
        scan_sources_incremental(&folders, Some(&packages), load_source_cache());
    write_source_cache(&source_cache);
    if !songs.is_empty() {
        let cache_data = serialize_song_cache(&songs);
        let _ = fs::write(SONGS_CACHE_FILE, &cache_data);
        if dir_path.is_dir() {
            let _ = fs::write(dir_path.join(SONGS_CACHE_FILE), &cache_data);
        }
    }
    songs
}

fn collect_folder_files(
    dir: &Path,
    root: &Path,
    charts: &mut Vec<PathBuf>,
    fingerprint: &mut impl Hasher,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        let is_dir = match entry.file_type() {
            Ok(t) if t.is_symlink() => path.is_dir(),
            Ok(t) => t.is_dir(),
            Err(_) => false,
        };
        if is_dir {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !SKIP_DIRS.contains(&name) {
                collect_folder_files(&path, root, charts, fingerprint);
            }
            continue;
        }

        if path == root.join(SONGS_CACHE_FILE) || path == root.join(SOURCE_CACHE_FILE) {
            continue;
        }
        relative_path(root, &path).to_lowercase().hash(fingerprint);
        match file_stamp(&path) {
            Some(stamp) => {
                stamp.length.hash(fingerprint);
                stamp.modified_ns.hash(fingerprint);
            }
            None => 0u8.hash(fingerprint),
        }
        if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(is_chart_extension)
        {
            boot_progress::step();
            charts.push(path);
        }
    }
}

#[cfg(test)]
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

    #[test]
    fn incremental_rescan_matches_a_full_scan_after_source_changes() {
        let nonce = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("beetle-incremental-{nonce}"));
        let one = dir.join("one");
        let two = dir.join("two");
        fs::create_dir_all(&one).unwrap();
        fs::create_dir_all(&two).unwrap();
        let shared = b"#TITLE Shared\n#WAV01 kick.wav\n#00111:01\n";
        fs::write(one.join("shared.bms"), shared).unwrap();
        fs::write(two.join("shared.bms"), shared).unwrap();
        fs::write(one.join("unique.bms"), CHART).unwrap();
        fs::write(two.join("kick.wav"), b"sound").unwrap();
        let folders: Vec<_> = [&one, &two]
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();

        let (first, cache) = scan_sources_incremental(&folders, None, None);
        assert_eq!(first, scan_sources(&folders, None));
        let persisted = parse_source_cache(&serialize_source_cache(&cache)).unwrap();
        assert_eq!(persisted, cache);
        assert_eq!(
            Path::new(
                &first
                    .iter()
                    .find(|song| song.title == "Shared")
                    .unwrap()
                    .file_path
            ),
            two.join("shared.bms")
        );

        fs::write(one.join("kick.wav"), b"sound").unwrap();
        fs::write(
            one.join("unique.bms"),
            b"#TITLE Updated\n#BPM 160\n#00111:01\n",
        )
        .unwrap();
        fs::write(
            one.join("added.bms"),
            b"#TITLE Added\n#BPM 170\n#00111:01\n",
        )
        .unwrap();
        let (updated, cache) = scan_sources_incremental(&folders, None, Some(persisted));
        assert_eq!(updated, scan_sources(&folders, None));
        assert_eq!(
            Path::new(
                &updated
                    .iter()
                    .find(|song| song.title == "Shared")
                    .unwrap()
                    .file_path
            ),
            one.join("shared.bms")
        );

        fs::remove_file(two.join("shared.bms")).unwrap();
        let (removed, _) = scan_sources_incremental(&folders, None, Some(cache));
        assert_eq!(removed, scan_sources(&folders, None));
        assert_eq!(removed.len(), 3);
        let _ = fs::remove_dir_all(&dir);
    }
}

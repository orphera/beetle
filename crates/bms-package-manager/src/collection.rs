//! The collection index: every chart the user has, where each copy lives, and
//! which copies are identical (`bpm scan`, `bpm status`, `bpm dupes`).
//!
//! A chart is identified by its `ChartId` (SHA-256 of the file bytes), so a
//! copy in a folder and a copy in an installed package match without any
//! extra bookkeeping. The index is a cache that `bpm scan` rebuilds; the other
//! commands only read it. Locations are the folders registered with
//! `bpm library` and every installed package state.

use crate::manager::InstalledPackage;
use beetle_core::{
    choose_load_index, decode_bms_text, md5_from_hex, md5_to_hex, parse_bms, ChartId, SongMetadata,
};
use bms_package::PackageReader;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Version of the index layout and of the stored key-sound stems. Bump it when
/// either changes; an index with another version is rejected and must be rescanned.
pub const KEY_VERSION: u32 = 1;

/// Folder inside each registered folder where removed charts are moved.
/// `bpm scan` never reads it, so trashed copies are not reported as duplicates.
pub const TRASH_DIR: &str = ".bpm-trash";

const SKIP_DIRS: &[&str] = &[TRASH_DIR, ".tmp_install"];
const CHART_EXTENSIONS: &[&str] = &["bms", "bme", "bml", "pms"];
const SOUND_EXTENSIONS: &[&str] = &["wav", "ogg", "flac", "mp3"];
const HEADER_PREFIX: &str = "# bpm collection index";

/// Where the index is kept: `collection.idx` in the working directory, or
/// `$BEETLE_COLLECTION_INDEX`.
pub fn index_file() -> PathBuf {
    std::env::var("BEETLE_COLLECTION_INDEX")
        .map_or_else(|_| PathBuf::from("collection.idx"), PathBuf::from)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LocationKind {
    Folder,
    Package,
}

impl LocationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::Package => "package",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "folder" => Some(Self::Folder),
            "package" => Some(Self::Package),
            _ => None,
        }
    }
}

/// One copy of a chart: a file in a registered folder, or an entry in an
/// installed package state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub chart: ChartId,
    pub md5: [u8; 16],
    pub kind: LocationKind,
    /// Folder: the registered folder's absolute path. Package: `<id>@<state hash>`.
    pub source: String,
    /// Folder: the chart's path inside the folder. Package: the entry path inside the package.
    pub path: String,
    pub title: String,
    pub artist: String,
    pub play_level: u32,
    pub mode: String,
    /// Declared key sounds that cannot be found next to this copy.
    pub missing_keys: u32,
    /// Normalized key-sound names (lowercase, `/` separators, no extension), sorted and unique.
    pub key_stems: Vec<String>,
}

impl Location {
    /// The fixed order copies are listed and loaded in: kind, source, path.
    pub fn placement_key(&self) -> (LocationKind, &str, &str) {
        (self.kind, &self.source, &self.path)
    }

    /// A copy is intact when every declared key sound was found next to it.
    pub fn is_intact(&self) -> bool {
        self.missing_keys == 0
    }

    fn serialize_line(&self) -> String {
        let fields = [
            self.chart.to_hex(),
            md5_to_hex(&self.md5),
            self.kind.as_str().to_string(),
            escape(&self.source),
            escape(&self.path),
            self.play_level.to_string(),
            escape(&self.mode),
            self.missing_keys.to_string(),
            escape(&self.title),
            escape(&self.artist),
            self.key_stems
                .iter()
                .map(|stem| escape(stem))
                .collect::<Vec<_>>()
                .join("|"),
        ];
        fields.join("\t")
    }

    fn parse_line(line: &str) -> Option<Self> {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 11 {
            return None;
        }
        let key_stems = if fields[10].is_empty() {
            Vec::new()
        } else {
            fields[10].split('|').map(unescape).collect()
        };
        Some(Self {
            chart: ChartId::from_hex(fields[0])?,
            md5: md5_from_hex(fields[1])?,
            kind: LocationKind::parse(fields[2])?,
            source: unescape(fields[3]),
            path: unescape(fields[4]),
            play_level: fields[5].parse().ok()?,
            mode: unescape(fields[6]),
            missing_keys: fields[7].parse().ok()?,
            title: unescape(fields[8]),
            artist: unescape(fields[9]),
            key_stems,
        })
    }
}

/// The whole index as `bpm scan` leaves it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Index {
    /// Unix seconds when the scan finished.
    pub scanned_at: u64,
    /// Charts that could not be read or parsed.
    pub skipped: u32,
    pub locations: Vec<Location>,
}

impl Index {
    pub fn serialize(&self) -> String {
        let mut out = format!(
            "{HEADER_PREFIX}\tkey_v={KEY_VERSION}\tscanned={}\tskipped={}\n",
            self.scanned_at, self.skipped
        );
        for location in &self.locations {
            out.push_str(&location.serialize_line());
            out.push('\n');
        }
        out
    }

    /// Reads an index written by `serialize`. `None` when the file is not an
    /// index, has another `KEY_VERSION`, or has a malformed line: the caller
    /// should then ask for a rescan.
    pub fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        let header = lines.next()?;
        if !header.starts_with(HEADER_PREFIX) {
            return None;
        }
        let mut index = Self::default();
        let mut version = None;
        for field in header.split('\t').skip(1) {
            let (name, value) = field.split_once('=')?;
            match name {
                "key_v" => version = value.parse::<u32>().ok(),
                "scanned" => index.scanned_at = value.parse().ok()?,
                "skipped" => index.skipped = value.parse().ok()?,
                _ => {}
            }
        }
        if version? != KEY_VERSION {
            return None;
        }
        for line in lines.filter(|line| !line.is_empty()) {
            index.locations.push(Location::parse_line(line)?);
        }
        Some(index)
    }

    /// Charts that have more than one copy, with the copies in placement order.
    pub fn duplicate_groups(&self) -> Vec<DuplicateGroup<'_>> {
        let mut by_chart: BTreeMap<ChartId, Vec<&Location>> = BTreeMap::new();
        for location in &self.locations {
            by_chart.entry(location.chart).or_default().push(location);
        }
        by_chart
            .into_iter()
            .filter(|(_, copies)| copies.len() > 1)
            .map(|(chart, mut copies)| {
                copies.sort_by(|a, b| a.placement_key().cmp(&b.placement_key()));
                DuplicateGroup { chart, copies }
            })
            .collect()
    }

    /// Index of the copy the game loads for these copies, which must already be
    /// in placement order. See `beetle_core::choose_load_index`.
    pub fn load_index(copies: &[&Location]) -> Option<usize> {
        let intact: Vec<bool> = copies.iter().map(|c| c.is_intact()).collect();
        choose_load_index(&intact)
    }
}

/// One chart that exists in more than one place.
#[derive(Debug)]
pub struct DuplicateGroup<'a> {
    pub chart: ChartId,
    /// In placement order (folders first, then packages, then source and path).
    pub copies: Vec<&'a Location>,
}

impl DuplicateGroup<'_> {
    pub fn folder_copies(&self) -> usize {
        self.copies
            .iter()
            .filter(|c| c.kind == LocationKind::Folder)
            .count()
    }

    pub fn package_copies(&self) -> usize {
        self.copies.len() - self.folder_copies()
    }
}

/// Result of a scan, before it is written.
#[derive(Debug, Default)]
pub struct ScanCounts {
    pub folders: usize,
    pub package_states: usize,
    pub skipped: u32,
}

/// Scans every registered folder and every installed package state into a fresh index.
pub fn build_index(folders: &[String], installed: &[InstalledPackage]) -> (Index, ScanCounts) {
    let mut index = Index {
        scanned_at: now_secs(),
        ..Index::default()
    };
    let mut counts = ScanCounts {
        folders: folders.len(),
        package_states: installed.len(),
        skipped: 0,
    };
    // The same file can be reached through a folder and through a folder inside it.
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
            match fs::read(&file)
                .ok()
                .and_then(|bytes| folder_location(folder, root, &file, &bytes))
            {
                Some(location) => index.locations.push(location),
                None => counts.skipped += 1,
            }
        }
    }

    for package in installed {
        let source = format!("{}@{}", package.id, package.state_hash);
        match scan_package(&source, package) {
            Some((locations, skipped)) => {
                index.locations.extend(locations);
                counts.skipped += skipped;
            }
            None => counts.skipped += 1,
        }
    }

    index.skipped = counts.skipped;
    index.locations.sort_by(|a, b| {
        a.chart
            .cmp(&b.chart)
            .then_with(|| a.placement_key().cmp(&b.placement_key()))
    });
    (index, counts)
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

fn folder_location(source: &str, root: &Path, file: &Path, bytes: &[u8]) -> Option<Location> {
    let chart_dir = file.parent()?;
    let rel = file
        .strip_prefix(root)
        .ok()?
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");
    let meta = SongMetadata::from_bytes(&file.to_string_lossy(), bytes)?;
    let names = declared_sounds(bytes)?;
    let missing_keys = names
        .iter()
        .filter(|name| !folder_has_sound(chart_dir, name))
        .count() as u32;
    Some(location_from(
        meta,
        LocationKind::Folder,
        source.to_string(),
        rel,
        missing_keys,
        &names,
    ))
}

/// Locations of every chart inside one installed package state. `None` when the
/// package itself cannot be opened; otherwise the locations and how many entries were skipped.
fn scan_package(source: &str, package: &InstalledPackage) -> Option<(Vec<Location>, u32)> {
    let mut reader = PackageReader::open_file(package.location.join("package.bmsp")).ok()?;
    let charts: Vec<String> = reader
        .entries()
        .iter()
        .map(|e| e.path.clone())
        .filter(|p| is_chart_path(p))
        .collect();
    let mut locations = Vec::new();
    let mut skipped = 0;
    for path in charts {
        let Some(meta) = reader
            .read_entry(&path)
            .ok()
            .and_then(|bytes| SongMetadata::from_bytes(&path, &bytes).map(|m| (m, bytes)))
        else {
            skipped += 1;
            continue;
        };
        let (meta, bytes) = meta;
        let Some(names) = declared_sounds(&bytes) else {
            skipped += 1;
            continue;
        };
        let base_dir = path.rsplit_once('/').map_or("", |(dir, _)| dir);
        let missing_keys = names
            .iter()
            .filter(|name| reader.find_entry_path(base_dir, name).is_none())
            .count() as u32;
        locations.push(location_from(
            meta,
            LocationKind::Package,
            source.to_string(),
            path,
            missing_keys,
            &names,
        ));
    }
    Some((locations, skipped))
}

/// The key-sound file names a chart declares with `#WAVxx`.
fn declared_sounds(bytes: &[u8]) -> Option<Vec<String>> {
    let chart = parse_bms(&decode_bms_text(bytes)).ok()?;
    let mut names: Vec<String> = chart.header.wav_table.values().cloned().collect();
    names.sort();
    names.dedup();
    Some(names)
}

fn folder_has_sound(chart_dir: &Path, name: &str) -> bool {
    let rel = name.trim().replace('\\', "/");
    if chart_dir.join(&rel).is_file() {
        return true;
    }
    let stem = without_extension(&rel);
    SOUND_EXTENSIONS
        .iter()
        .any(|ext| chart_dir.join(format!("{stem}.{ext}")).is_file())
}

fn location_from(
    meta: SongMetadata,
    kind: LocationKind,
    source: String,
    path: String,
    missing_keys: u32,
    names: &[String],
) -> Location {
    let mut key_stems: Vec<String> = names
        .iter()
        .map(|name| normalize_stem(name))
        .filter(|stem| !stem.is_empty())
        .collect();
    key_stems.sort();
    key_stems.dedup();
    Location {
        chart: meta.id,
        md5: meta.md5,
        kind,
        source,
        path,
        title: meta.title,
        artist: meta.artist,
        play_level: meta.play_level,
        mode: meta.play_mode.as_str().to_string(),
        missing_keys,
        key_stems,
    }
}

/// Lowercase ASCII, `/` separators, no leading `./`, no extension.
pub fn normalize_stem(name: &str) -> String {
    let unified = name.trim().replace('\\', "/").to_ascii_lowercase();
    without_extension(unified.trim_start_matches("./")).to_string()
}

/// Drops the extension of the last path segment, if it has one.
fn without_extension(path: &str) -> &str {
    let segment_start = path.rfind('/').map_or(0, |slash| slash + 1);
    match path[segment_start..].rfind('.') {
        Some(dot) if dot > 0 => &path[..segment_start + dot],
        _ => path,
    }
}

/// Escapes the characters that separate index fields, so any text fits on one line.
fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('|', "\\p")
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('p') => out.push('|'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn location(
        chart: ChartId,
        kind: LocationKind,
        source: &str,
        path: &str,
        missing: u32,
    ) -> Location {
        Location {
            chart,
            md5: [7; 16],
            kind,
            source: source.to_string(),
            path: path.to_string(),
            title: "Aleph | 0\tx".to_string(),
            artist: "LeaF".to_string(),
            play_level: 12,
            mode: "7K".to_string(),
            missing_keys: missing,
            key_stems: vec!["kick".to_string(), "sub/snare".to_string()],
        }
    }

    #[test]
    fn index_round_trips_through_text() {
        let chart = ChartId::of_bytes(b"chart");
        let index = Index {
            scanned_at: 42,
            skipped: 1,
            locations: vec![
                location(chart, LocationKind::Folder, "D:\\BMS", "a\\b.bme", 0),
                location(chart, LocationKind::Package, "leaf@abc", "bms/main.bme", 2),
            ],
        };
        let parsed = Index::parse(&index.serialize()).expect("parses");
        assert_eq!(parsed, index);
    }

    #[test]
    fn index_with_another_key_version_is_rejected() {
        let text = format!(
            "{HEADER_PREFIX}\tkey_v={}\tscanned=0\tskipped=0\n",
            KEY_VERSION + 1
        );
        assert!(Index::parse(&text).is_none());
    }

    #[test]
    fn stems_drop_case_separators_and_extension() {
        assert_eq!(normalize_stem("Sub\\Kick.WAV"), "sub/kick");
        assert_eq!(normalize_stem("./a.b/c"), "a.b/c");
        assert_eq!(normalize_stem("01.ogg"), "01");
    }

    #[test]
    fn duplicate_groups_list_copies_in_placement_order() {
        let shared = ChartId::of_bytes(b"shared");
        let lone = ChartId::of_bytes(b"lone");
        let index = Index {
            locations: vec![
                location(shared, LocationKind::Package, "leaf@abc", "x.bme", 0),
                location(shared, LocationKind::Folder, "D:\\z", "b.bme", 0),
                location(shared, LocationKind::Folder, "D:\\a", "a.bme", 1),
                location(lone, LocationKind::Folder, "D:\\a", "c.bme", 0),
            ],
            ..Index::default()
        };
        let groups = index.duplicate_groups();
        assert_eq!(groups.len(), 1);
        let group = &groups[0];
        assert_eq!(group.chart, shared);
        assert_eq!(group.folder_copies(), 2);
        assert_eq!(group.package_copies(), 1);
        assert_eq!(group.copies[0].source, "D:\\a");
        assert_eq!(group.copies[2].kind, LocationKind::Package);
        // Placement order puts D:\a first, but it is missing a key sound, so
        // the first intact copy (D:\z) is the one loaded.
        assert_eq!(Index::load_index(&group.copies), Some(1));
    }
}

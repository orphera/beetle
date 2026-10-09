use beetle_core::{deserialize_song_cache, serialize_song_cache, LibraryPaths, SongMetadata};
use bms_package::PackageReader;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_SONGS_DIR: &str = "songs";
pub const DEFAULT_PACKAGES_DIR: &str = "packages";
pub const SONGS_CACHE_FILE: &str = "songs.cache";

/// Where `bpm library` keeps the extra BMS folders; the same file it writes to.
fn library_file() -> PathBuf {
    std::env::var("BEETLE_LIBRARY_FILE")
        .map_or_else(|_| PathBuf::from("library.dat"), PathBuf::from)
}

fn library_paths() -> Vec<PathBuf> {
    let text = fs::read_to_string(library_file()).unwrap_or_default();
    LibraryPaths::parse(&text)
        .paths()
        .iter()
        .map(PathBuf::from)
        .collect()
}

/// A cache older than the library list was built without some folders.
fn cache_is_stale(cache: &Path) -> bool {
    let modified = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).ok();
    match (modified(cache), modified(&library_file())) {
        (Some(c), Some(l)) => c < l,
        _ => false,
    }
}

/// Scans the target directory for BMS files, utilizing `songs.cache` when available.
pub fn load_or_scan_songs<P: AsRef<Path>>(dir: P) -> Vec<SongMetadata> {
    let dir_path = dir.as_ref();

    // 1. Try loading from cache in current dir, songs dir, or target/release
    for candidate in &[
        Path::new(SONGS_CACHE_FILE),
        &dir_path.join(SONGS_CACHE_FILE),
        Path::new("target/release/songs.cache"),
    ] {
        if candidate.exists() && !cache_is_stale(candidate) {
            if let Ok(cache_text) = fs::read_to_string(candidate) {
                let cached_songs = deserialize_song_cache(&cache_text);
                if !cached_songs.is_empty() {
                    // Caches written before deduplication may still list a chart twice.
                    return dedup_songs(cached_songs);
                }
            }
        }
    }

    // 2. Perform filesystem scan
    let songs = scan_directory(dir_path);

    // 3. Save cache to root songs.cache
    if !songs.is_empty() {
        let cache_data = serialize_song_cache(&songs);
        let _ = fs::write(SONGS_CACHE_FILE, &cache_data);
        if dir_path.exists() && dir_path.is_dir() {
            let _ = fs::write(dir_path.join(SONGS_CACHE_FILE), &cache_data);
        }
    }

    songs
}

/// Force rescans target directory and packages, invalidating and overwriting `songs.cache`.
pub fn force_rescan_songs<P: AsRef<Path>>(dir: P) -> Vec<SongMetadata> {
    let dir_path = dir.as_ref();
    let songs = scan_directory(dir_path);
    if !songs.is_empty() {
        let cache_data = serialize_song_cache(&songs);
        let _ = fs::write(SONGS_CACHE_FILE, &cache_data);
        if dir_path.exists() && dir_path.is_dir() {
            let _ = fs::write(dir_path.join(SONGS_CACHE_FILE), &cache_data);
        }
    }
    songs
}

/// Recursively scans target directory and packages directory for `.bms`, `.bme`, and `.bml` files.
pub fn scan_directory<P: AsRef<Path>>(dir: P) -> Vec<SongMetadata> {
    let mut songs = Vec::new();
    let dir_path = dir.as_ref();
    scan_recursive(dir_path, &mut songs);

    // Also check standard packages locations
    for extra_dir in &[
        DEFAULT_PACKAGES_DIR,
        "target/release/packages",
        "../packages",
    ] {
        let p = Path::new(extra_dir);
        if p.exists() && p != dir_path {
            scan_recursive(p, &mut songs);
        }
    }

    for p in library_paths() {
        if p != dir_path {
            scan_recursive(&p, &mut songs);
        }
    }

    if let Ok(env_dir) = std::env::var("BMS_DIR") {
        let p = Path::new(&env_dir);
        if p.exists() && p != dir_path {
            scan_recursive(p, &mut songs);
        }
    }

    // Scan order is kept until here, so a copy in the main folder wins over one in an extra folder.
    let mut songs = dedup_songs(songs);
    songs.sort_by(|a, b| {
        a.title
            .cmp(&b.title)
            .then_with(|| a.file_path.cmp(&b.file_path))
    });
    songs
}

/// Keeps the first entry of each chart. The same file can be reached through two
/// paths, and the same bytes can sit in two folders; both are one chart (same `id`).
fn dedup_songs(songs: Vec<SongMetadata>) -> Vec<SongMetadata> {
    let mut seen_paths = HashSet::new();
    let mut seen_ids = HashSet::new();
    songs
        .into_iter()
        .filter(|s| seen_paths.insert(s.file_path.clone()) && seen_ids.insert(s.id))
        .collect()
}

fn scan_recursive(dir: &Path, songs: &mut Vec<SongMetadata>) {
    if !dir.exists() || !dir.is_dir() {
        return;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Avoid recursion into temp folders
            if path
                .file_name()
                .map(|n| n == ".tmp_install")
                .unwrap_or(false)
            {
                continue;
            }
            scan_recursive(&path, songs);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if ext.eq_ignore_ascii_case("bms")
                || ext.eq_ignore_ascii_case("bme")
                || ext.eq_ignore_ascii_case("bml")
                || ext.eq_ignore_ascii_case("pms")
            {
                if let Ok(bytes) = fs::read(&path) {
                    if let Some(meta) = SongMetadata::from_bytes(&path.to_string_lossy(), &bytes) {
                        songs.push(meta);
                    }
                }
            } else if ext.eq_ignore_ascii_case("bmsp") {
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if file_name.ends_with(".bga.bmsp") {
                    continue;
                }
                // Low-memory streaming scan: reads only central directory without buffering gigabytes into RAM
                if let Ok(mut pkg) = PackageReader::open_file(&path) {
                    let path_str = path.to_string_lossy();
                    let chart_entries: Vec<String> = pkg
                        .entries()
                        .iter()
                        .filter_map(|e| {
                            let e_ext = e.path.rsplit('.').next().unwrap_or("");
                            if e_ext.eq_ignore_ascii_case("bms")
                                || e_ext.eq_ignore_ascii_case("bme")
                                || e_ext.eq_ignore_ascii_case("bml")
                                || e_ext.eq_ignore_ascii_case("pms")
                            {
                                Some(e.path.clone())
                            } else {
                                None
                            }
                        })
                        .collect();

                    for entry_path in chart_entries {
                        if let Ok(bytes) = pkg.read_entry(&entry_path) {
                            let virtual_path = format!("{}::{}", path_str, entry_path);
                            if let Some(meta) = SongMetadata::from_bytes(&virtual_path, &bytes) {
                                songs.push(meta);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHART: &[u8] = b"#TITLE T\n#BPM 150\n#00111:01\n";

    #[test]
    fn the_same_chart_in_two_folders_is_listed_once() {
        let main = SongMetadata::from_bytes("songs/a/t.bms", CHART).unwrap();
        let extra = SongMetadata::from_bytes("extra/b/t.bms", CHART).unwrap();
        let other = SongMetadata::from_bytes("songs/c/u.bms", b"#TITLE U\n#00111:01\n").unwrap();
        let kept = dedup_songs(vec![main.clone(), extra, other.clone()]);
        assert_eq!(kept, vec![main, other]);
    }

    #[test]
    fn the_same_path_twice_is_listed_once() {
        let song = SongMetadata::from_bytes("songs/a/t.bms", CHART).unwrap();
        assert_eq!(dedup_songs(vec![song.clone(), song.clone()]), vec![song]);
    }
}

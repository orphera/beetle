use beetle_core::{deserialize_song_cache, serialize_song_cache, SongMetadata};
use bms_package::PackageReader;
use std::fs;
use std::path::Path;

pub const DEFAULT_SONGS_DIR: &str = "songs";
pub const DEFAULT_PACKAGES_DIR: &str = "packages";
pub const SONGS_CACHE_FILE: &str = "songs.cache";

/// Scans the target directory for BMS files, utilizing `songs.cache` when available.
pub fn load_or_scan_songs<P: AsRef<Path>>(dir: P) -> Vec<SongMetadata> {
    let dir_path = dir.as_ref();

    // 1. Try loading from cache in current dir, songs dir, or target/release
    for candidate in &[
        Path::new(SONGS_CACHE_FILE),
        &dir_path.join(SONGS_CACHE_FILE),
        Path::new("target/release/songs.cache"),
    ] {
        if candidate.exists() {
            if let Ok(cache_text) = fs::read_to_string(candidate) {
                let cached_songs = deserialize_song_cache(&cache_text);
                if !cached_songs.is_empty() {
                    return cached_songs;
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

    // Deduplicate by file_path
    songs.sort_by(|a, b| a.file_path.cmp(&b.file_path));
    songs.dedup_by(|a, b| a.file_path == b.file_path);

    songs.sort_by(|a, b| a.title.cmp(&b.title));
    songs
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
            if path.file_name().map(|n| n == ".tmp_install").unwrap_or(false) {
                continue;
            }
            scan_recursive(&path, songs);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if ext.eq_ignore_ascii_case("bms")
                || ext.eq_ignore_ascii_case("bme")
                || ext.eq_ignore_ascii_case("bml")
            {
                if let Ok(bytes) = fs::read(&path) {
                    let content = String::from_utf8_lossy(&bytes);
                    if let Some(meta) = SongMetadata::from_content(&path.to_string_lossy(), &content) {
                        songs.push(meta);
                    }
                }
            } else if ext.eq_ignore_ascii_case("bmsp") {
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
                            {
                                Some(e.path.clone())
                            } else {
                                None
                            }
                        })
                        .collect();

                    for entry_path in chart_entries {
                        if let Ok(bytes) = pkg.read_entry(&entry_path) {
                            let content = String::from_utf8_lossy(&bytes);
                            let virtual_path = format!("{}::{}", path_str, entry_path);
                            if let Some(meta) = SongMetadata::from_content(&virtual_path, &content) {
                                songs.push(meta);
                            }
                        }
                    }
                }
            }
        }
    }
}

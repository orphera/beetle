use beetle_core::{deserialize_song_cache, serialize_song_cache, SongMetadata};
use std::fs;
use std::path::Path;

pub const DEFAULT_SONGS_DIR: &str = "songs";
pub const SONGS_CACHE_FILE: &str = "songs.cache";

/// Scans the target directory for BMS files, utilizing `songs.cache` when available.
pub fn load_or_scan_songs<P: AsRef<Path>>(dir: P) -> Vec<SongMetadata> {
    let dir_path = dir.as_ref();

    // 1. Try loading from cache
    let cache_path = dir_path.join(SONGS_CACHE_FILE);
    if cache_path.exists() {
        if let Ok(cache_text) = fs::read_to_string(&cache_path) {
            let cached_songs = deserialize_song_cache(&cache_text);
            if !cached_songs.is_empty() {
                return cached_songs;
            }
        }
    }

    // 2. Perform filesystem scan
    let songs = scan_directory(dir_path);

    // 3. Save cache
    if !songs.is_empty() {
        let cache_data = serialize_song_cache(&songs);
        let _ = fs::write(&cache_path, cache_data);
    }

    songs
}

/// Recursively scans a directory for `.bms`, `.bme`, and `.bml` files.
pub fn scan_directory<P: AsRef<Path>>(dir: P) -> Vec<SongMetadata> {
    let mut songs = Vec::new();
    scan_recursive(dir.as_ref(), &mut songs);
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
            scan_recursive(&path, songs);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if ext.eq_ignore_ascii_case("bms")
                || ext.eq_ignore_ascii_case("bme")
                || ext.eq_ignore_ascii_case("bml")
            {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Some(meta) = SongMetadata::from_content(&path.to_string_lossy(), &content) {
                        songs.push(meta);
                    }
                }
            }
        }
    }
}

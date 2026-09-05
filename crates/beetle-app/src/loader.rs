use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use beetle_audio::SampleBank;
use beetle_core::{parse_bms, BmpId, BmsChart, SongMetadata, TimingModel};
use beetle_render::{is_video_path, ImageBuffer};

use crate::demo;

pub const ARTWORKS_CACHE_DIR: &str = ".cache/artworks";

/// Unified video data source: either a filesystem path or in-memory byte buffer.
#[derive(Debug, Clone)]
pub enum VideoSource {
    File(PathBuf),
    Memory {
        bytes: Arc<[u8]>,
        filename_hint: Option<String>,
    },
}

const ARTWORK_CANDIDATE_FILENAMES: &[&str] = &[
    "stagefile.bmp", "stage.bmp", "banner.bmp", "title.bmp",
    "stagefile.png", "stage.png", "banner.png", "title.png",
    "stagefile.jpg", "stage.jpg", "banner.jpg", "title.jpg",
    "STAGEFILE.BMP", "STAGE.BMP", "BANNER.BMP", "TITLE.BMP",
];

fn resolve_file_case_insensitive(dir: &Path, relative: &str) -> Option<PathBuf> {
    let normalized = relative.replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    if parts.is_empty() {
        return None;
    }

    let mut current = dir.to_path_buf();
    for part in parts {
        let mut found = false;
        if let Ok(entries) = fs::read_dir(&current) {
            for entry in entries.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    if name.eq_ignore_ascii_case(part) {
                        current = entry.path();
                        found = true;
                        break;
                    }
                }
            }
        }
        if !found {
            return None;
        }
    }

    Some(current)
}

fn load_image_from_dir_or_case_insensitive(dir: &Path, filename: &str) -> Option<ImageBuffer> {
    let resolved = resolve_file_case_insensitive(dir, filename)?;
    ImageBuffer::load_from_file(&resolved)
}

fn parse_bmp_id(key: &str) -> Option<BmpId> {
    let bytes = key.as_bytes();
    if bytes.len() == 2 {
        if let Some(id) = beetle_core::decode_base36(bytes[0], bytes[1]) {
            return Some(BmpId(id.0));
        }
    }
    if let Ok(v) = u16::from_str_radix(key, 16) {
        return Some(BmpId(v));
    }
    key.parse::<u16>().ok().map(BmpId)
}

fn find_video_files_in_dir(dir: &Path, chart: &BmsChart) -> HashMap<BmpId, VideoSource> {
    let mut videos = HashMap::new();

    // 1. Check bmp_table for video files or files with matching stems
    for (&bmp_id, filename) in &chart.header.bmp_table {
        if is_video_path(filename) {
            if let Some(p) = resolve_file_case_insensitive(dir, filename) {
                videos.insert(bmp_id, VideoSource::File(p));
            }
        } else {
            let stem = match filename.rfind('.') {
                Some(pos) => &filename[..pos],
                None => filename.as_str(),
            };
            for ext in beetle_render::VIDEO_EXTENSIONS {
                let candidate = format!("{}.{}", stem, ext);
                if let Some(p) = resolve_file_case_insensitive(dir, &candidate) {
                    videos.insert(bmp_id, VideoSource::File(p));
                    break;
                }
            }
        }
    }

    // 2. Fallback: if no bmp_table entry matched a video, check stage/banner or common names
    if videos.is_empty() {
        let mut fallback_path = None;
        for filename in &[&chart.header.stage_file, &chart.header.banner] {
            if !filename.is_empty() && is_video_path(filename) {
                if let Some(p) = resolve_file_case_insensitive(dir, filename) {
                    fallback_path = Some(p);
                    break;
                }
            }
        }
        if fallback_path.is_none() {
            for name in &[
                "bga.mp4", "movie.mp4", "video.mp4", "bg.mp4", "pv.mp4",
                "bga.mpg", "movie.mpg", "video.mpg", "bg.mpg",
                "bga.wmv", "movie.wmv", "video.wmv", "bg.wmv",
                "bga.avi", "movie.avi", "video.avi", "bg.avi",
                "bga.webm", "movie.webm", "video.webm", "bg.webm",
                "bga.mkv", "movie.mkv", "video.mkv", "bg.mkv",
            ] {
                let p = dir.join(name);
                if p.exists() {
                    fallback_path = Some(p);
                    break;
                }
            }
        }

        if let Some(fp) = fallback_path {
            let base_ids: Vec<BmpId> = chart
                .bga_events
                .iter()
                .filter(|ev| ev.channel == beetle_core::BgaChannel::Base)
                .map(|ev| ev.bmp_id)
                .collect();
            let source = VideoSource::File(fp);
            if base_ids.is_empty() {
                videos.insert(BmpId(1), source);
            } else {
                for id in base_ids {
                    videos.entry(id).or_insert_with(|| source.clone());
                }
            }
        }
    }

    videos
}

fn load_videos_from_package_archive(
    pkg: &mut bms_package::PackageReader,
    base_dir: &str,
    chart: &BmsChart,
    video_sources: &mut HashMap<BmpId, VideoSource>,
) {
    for (&bmp_id, filename) in &chart.header.bmp_table {
        if is_video_path(filename) {
            if let Some(target_path) = pkg.find_entry_path(base_dir, filename) {
                if let Ok(bytes) = pkg.read_entry(&target_path) {
                    video_sources.insert(
                        bmp_id,
                        VideoSource::Memory {
                            bytes: Arc::from(bytes.into_boxed_slice()),
                            filename_hint: Some(filename.clone()),
                        },
                    );
                }
            }
        } else {
            let stem = match filename.rfind('.') {
                Some(pos) => &filename[..pos],
                None => filename.as_str(),
            };
            for ext in beetle_render::VIDEO_EXTENSIONS {
                let candidate = format!("{}.{}", stem, ext);
                if let Some(target_path) = pkg.find_entry_path(base_dir, &candidate) {
                    if let Ok(bytes) = pkg.read_entry(&target_path) {
                        video_sources.insert(
                            bmp_id,
                            VideoSource::Memory {
                                bytes: Arc::from(bytes.into_boxed_slice()),
                                filename_hint: Some(candidate),
                            },
                        );
                        break;
                    }
                }
            }
        }
    }

    // Fallback video inside package
    if video_sources.is_empty() {
        let mut fallback_entry = None;
        for filename in &[&chart.header.stage_file, &chart.header.banner] {
            if !filename.is_empty() && is_video_path(filename) {
                if let Some(target_path) = pkg.find_entry_path(base_dir, filename) {
                    fallback_entry = Some((target_path, filename.to_string()));
                    break;
                }
            }
        }
        if fallback_entry.is_none() {
            for name in &[
                "bga.mp4", "movie.mp4", "video.mp4", "bg.mp4", "pv.mp4",
                "bga.mpg", "movie.mpg", "video.mpg", "bg.mpg",
                "bga.wmv", "movie.wmv", "video.wmv", "bg.wmv",
                "bga.avi", "movie.avi", "video.avi", "bg.avi",
                "bga.webm", "movie.webm", "video.webm", "bg.webm",
                "bga.mkv", "movie.mkv", "video.mkv", "bg.mkv",
            ] {
                if let Some(target_path) = pkg.find_entry_path(base_dir, name) {
                    fallback_entry = Some((target_path, name.to_string()));
                    break;
                }
            }
        }
        if let Some((target_path, name)) = fallback_entry {
            if let Ok(bytes) = pkg.read_entry(&target_path) {
                let source = VideoSource::Memory {
                    bytes: Arc::from(bytes.into_boxed_slice()),
                    filename_hint: Some(name),
                };
                let base_ids: Vec<BmpId> = chart
                    .bga_events
                    .iter()
                    .filter(|ev| ev.channel == beetle_core::BgaChannel::Base)
                    .map(|ev| ev.bmp_id)
                    .collect();
                if base_ids.is_empty() {
                    video_sources.insert(BmpId(1), source);
                } else {
                    for id in base_ids {
                        video_sources.entry(id).or_insert_with(|| source.clone());
                    }
                }
            }
        }
    }
}

/// Loads stage artwork image for a song if available on disk, cache, or inside a .bmsp package.
pub fn load_stage_image(song: &SongMetadata) -> Option<ImageBuffer> {
    if song.file_path == ":demo:" {
        return None;
    }

    // 1. Check persistent on-disk artwork cache first (fastest)
    let cache_dir = Path::new(ARTWORKS_CACHE_DIR);
    let cache_file = cache_dir.join(format!("{:016x}.bmp", song.hash));
    if cache_file.exists() {
        if let Some(img) = ImageBuffer::load_from_file(&cache_file) {
            return Some(img);
        }
    }

    // 2. Check if song is packaged inside a .bmsp archive
    if let Some((pkg_path, entry_name)) = song.file_path.split_once("::") {
        if let Ok(mut pkg) = bms_package::PackageReader::open_file(pkg_path) {
            let base_dir = Path::new(entry_name)
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .to_string_lossy();

            // 2a. Check if package has BGA Atlas with stagefile/banner
            let bga_meta = pkg.manifest().bga_atlas.clone();
            if let Some(bga_meta) = bga_meta {
                let atlas_path = if pkg.contains(&bga_meta.file) {
                    Some(bga_meta.file.clone())
                } else {
                    pkg.find_entry_path(&base_dir, &bga_meta.file)
                };

                if let Some(path) = atlas_path {
                    if let Ok(atlas_bytes) = pkg.read_entry(&path) {
                        if let Some(atlas_img) = ImageBuffer::from_bytes(&atlas_bytes) {
                            let mut stage_frame = bga_meta.frames.get("stagefile")
                                .or_else(|| bga_meta.frames.get("banner"));

                            if stage_frame.is_none() {
                                if let Ok(bms_bytes) = pkg.read_entry(entry_name) {
                                    let content = beetle_core::decode_bms_text(&bms_bytes);
                                    if let Ok(chart) = parse_bms(&content) {
                                        for name in &[&chart.header.stage_file, &chart.header.banner] {
                                            if !name.is_empty() {
                                                let norm = name.replace('\\', "/");
                                                let file_name = Path::new(&norm).file_name().and_then(|n| n.to_str()).unwrap_or(&norm);
                                                if let Some(f) = bga_meta.frames.values().find(|f| {
                                                    f.original_filename.as_deref().map(|s| s.eq_ignore_ascii_case(file_name)).unwrap_or(false)
                                                }) {
                                                    stage_frame = Some(f);
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            if let Some(frame) = stage_frame {
                                if let Some(img) = atlas_img.crop(frame.x, frame.y, frame.width, frame.height) {
                                    let _ = fs::create_dir_all(cache_dir);
                                    let bmp_bytes = img.encode_bmp_bytes();
                                    let _ = fs::write(&cache_file, &bmp_bytes);
                                    return Some(img);
                                }
                            }
                        }
                    }
                }
            }

            if let Ok(bms_bytes) = pkg.read_entry(entry_name) {
                let content = beetle_core::decode_bms_text(&bms_bytes);
                if let Ok(chart) = parse_bms(&content) {
                    if !chart.header.stage_file.is_empty() {
                        if let Some(path) = pkg.find_entry_path(&base_dir, &chart.header.stage_file) {
                            if let Ok(img_bytes) = pkg.read_entry(&path) {
                                if let Some(img) = ImageBuffer::from_bytes(&img_bytes) {
                                    let _ = fs::create_dir_all(cache_dir);
                                    let _ = fs::write(&cache_file, &img_bytes);
                                    return Some(img);
                                }
                            }
                        }
                    }
                    if !chart.header.banner.is_empty() {
                        if let Some(path) = pkg.find_entry_path(&base_dir, &chart.header.banner) {
                            if let Ok(img_bytes) = pkg.read_entry(&path) {
                                if let Some(img) = ImageBuffer::from_bytes(&img_bytes) {
                                    let _ = fs::create_dir_all(cache_dir);
                                    let _ = fs::write(&cache_file, &img_bytes);
                                    return Some(img);
                                }
                            }
                        }
                    }
                }
            }

            for name in ARTWORK_CANDIDATE_FILENAMES {
                if let Some(path) = pkg.find_entry_path(&base_dir, name) {
                    if let Ok(img_bytes) = pkg.read_entry(&path) {
                        if let Some(img) = ImageBuffer::from_bytes(&img_bytes) {
                            let _ = fs::create_dir_all(cache_dir);
                            let _ = fs::write(&cache_file, &img_bytes);
                            return Some(img);
                        }
                    }
                }
            }
        }
        return None;
    }

    let song_path = Path::new(&song.file_path);
    let dir = song_path.parent().unwrap_or_else(|| Path::new("."));

    // Check parsed chart header for stagefile or banner
    if let Ok(bytes) = fs::read(song_path) {
        let content = beetle_core::decode_bms_text(&bytes);
        if let Ok(chart) = parse_bms(&content) {
            if !chart.header.stage_file.is_empty() {
                if let Some(img) = load_image_from_dir_or_case_insensitive(dir, &chart.header.stage_file) {
                    return Some(img);
                }
            }
            if !chart.header.banner.is_empty() {
                if let Some(img) = load_image_from_dir_or_case_insensitive(dir, &chart.header.banner) {
                    return Some(img);
                }
            }
        }
    }

    // Fallback file scanning for common artwork names
    for name in ARTWORK_CANDIDATE_FILENAMES {
        let p = dir.join(name);
        if let Some(img) = ImageBuffer::load_from_file(&p) {
            if let Ok(data) = fs::read(&p) {
                let _ = fs::create_dir_all(cache_dir);
                let _ = fs::write(&cache_file, data);
            }
            return Some(img);
        }
    }

    None
}

/// Loads and parses the BMS chart file and pre-decodes the entire keysound samplebank and BGA images into memory.
pub fn load_chart_and_audio(
    song: &SongMetadata,
) -> (BmsChart, TimingModel, SampleBank, HashMap<BmpId, ImageBuffer>, HashMap<BmpId, VideoSource>) {
    if song.file_path == ":demo:" {
        let chart = demo::create_demo_chart();
        let timing = TimingModel::from_chart(&chart);
        let soundbank = demo::create_demo_sample_bank();
        let bga_bank = HashMap::new();
        return (chart, timing, soundbank, bga_bank, HashMap::new());
    }

    // Check if song is inside a .bmsp package
    if let Some((pkg_path, entry_name)) = song.file_path.split_once("::") {
        if let Ok(mut pkg) = bms_package::PackageReader::open_file(pkg_path) {
            let base_dir = Path::new(entry_name)
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .to_string_lossy();

            if let Ok(bms_bytes) = pkg.read_entry(entry_name) {
                let content = beetle_core::decode_bms_text(&bms_bytes);
                if let Ok(chart) = parse_bms(&content) {
                    let timing = TimingModel::from_chart(&chart);
                    let mut soundbank = SampleBank::new();
                    let mut bga_bank = HashMap::new();
                    let mut video_sources = HashMap::new();
                    let mut loaded_count = 0;

                    // 1. Keysounds: FAST-PATH via Sound Atlas (1-pass zero-allocation decoding)
                    let mut loaded_sound_from_atlas = false;
                    let sound_meta = pkg.manifest().sound_atlas.clone();
                    if let Some(sound_meta) = sound_meta {
                        let atlas_path = if pkg.contains(&sound_meta.file) {
                            Some(sound_meta.file.clone())
                        } else {
                            pkg.find_entry_path(&base_dir, &sound_meta.file)
                        };

                        if let Some(path) = atlas_path {
                            if let Ok(atlas_bytes) = pkg.read_entry(&path) {
                                match SampleBank::load_from_sound_atlas_for_chart(&chart, &sound_meta, &atlas_bytes) {
                                    Ok(bank) => {
                                        loaded_count = bank.len();
                                        soundbank = bank;
                                        loaded_sound_from_atlas = true;
                                    }
                                    Err(e) => {
                                        eprintln!("[Loader] Sound Atlas decode error: {e}, falling back to individual file scan");
                                    }
                                }
                            }
                        }
                    }

                    // Fallback for sound: Classic file-by-file loading for any keysound missing from Sound Atlas
                    for (&wav_id, filename) in &chart.header.wav_table {
                        if !soundbank.contains_key(wav_id) {
                            if let Some(target_path) = pkg.find_entry_path(&base_dir, filename) {
                                if let Ok(bytes) = pkg.read_entry(&target_path) {
                                    if let Ok(pcm) = SampleBank::load_audio_from_bytes(&bytes) {
                                        soundbank.insert(wav_id, pcm);
                                        loaded_count += 1;
                                    }
                                }
                            }
                        }
                    }

                    // 2. BGA Frames: FAST-PATH via BGA Texture Atlas (1-pass decode and memory crop)
                    let mut loaded_bga_from_atlas = false;
                    let bga_meta = pkg.manifest().bga_atlas.clone();
                    if let Some(bga_meta) = bga_meta {
                        let atlas_path = if pkg.contains(&bga_meta.file) {
                            Some(bga_meta.file.clone())
                        } else {
                            pkg.find_entry_path(&base_dir, &bga_meta.file)
                        };

                        if let Some(path) = atlas_path {
                            if let Ok(atlas_bytes) = pkg.read_entry(&path) {
                                if let Some(atlas_img) = ImageBuffer::from_bytes(&atlas_bytes) {
                                    // A. Map hex/decimal key frames directly to BmpId
                                    for (key, frame) in &bga_meta.frames {
                                        if let Some(bmp_id) = parse_bmp_id(key) {
                                            if let Some(sub_img) = atlas_img.crop(frame.x, frame.y, frame.width, frame.height) {
                                                bga_bank.insert(bmp_id, sub_img);
                                            }
                                        }
                                    }
                                    // B. Map any chart bmp_table entry by original_file name if not yet mapped
                                    for (&bmp_id, filename) in &chart.header.bmp_table {
                                        if !bga_bank.contains_key(&bmp_id) {
                                            let norm = filename.replace('\\', "/");
                                            let file_name = Path::new(&norm).file_name().and_then(|n| n.to_str()).unwrap_or(&norm);
                                            if let Some(frame) = bga_meta.frames.values().find(|f| {
                                                f.original_filename.as_deref().map(|s| s.eq_ignore_ascii_case(file_name)).unwrap_or(false)
                                            }) {
                                                if let Some(sub_img) = atlas_img.crop(frame.x, frame.y, frame.width, frame.height) {
                                                    bga_bank.insert(bmp_id, sub_img);
                                                }
                                            }
                                        }
                                    }
                                    loaded_bga_from_atlas = true;
                                }
                            }
                        }
                    }

                    if !loaded_bga_from_atlas {
                        for (&bmp_id, filename) in &chart.header.bmp_table {
                            if !is_video_path(filename) {
                                if let Some(target_path) = pkg.find_entry_path(&base_dir, filename) {
                                    if let Ok(bytes) = pkg.read_entry(&target_path) {
                                        if let Some(img) = ImageBuffer::from_bytes(&bytes) {
                                            bga_bank.insert(bmp_id, img);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // 1. Search videos inside primary .bmsp package
                    load_videos_from_package_archive(&mut pkg, &base_dir, &chart, &mut video_sources);

                    // 2. Search companion BGA package if no video was found in primary package
                    if video_sources.is_empty() {
                        let pkg_file_path = Path::new(pkg_path);
                        let parent_dir = pkg_file_path.parent().unwrap_or_else(|| Path::new("."));
                        let pkg_stem = pkg_file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

                        let mut companion_candidates = Vec::new();

                        if let Some(ref companions) = pkg.manifest().companion_packages {
                            if let Some(ref bga_info) = companions.bga {
                                companion_candidates.push(parent_dir.join(&bga_info.recommended_filename));
                            }
                        }

                        if !pkg_stem.is_empty() {
                            companion_candidates.push(parent_dir.join(format!("{}.bga.bmsp", pkg_stem)));
                        }

                        companion_candidates.push(parent_dir.join(format!("{}.bga.bmsp", pkg.manifest().id)));
                        companion_candidates.push(parent_dir.join(&pkg.manifest().id).join("bga").join(format!("{}.bga.bmsp", pkg.manifest().id)));

                        for cand in companion_candidates {
                            if cand.is_file() {
                                if let Ok(mut bga_pkg) = bms_package::PackageReader::open_file(&cand) {
                                    load_videos_from_package_archive(&mut bga_pkg, "", &chart, &mut video_sources);
                                    if !video_sources.is_empty() {
                                        println!("[Loader] Successfully loaded BGA companion package: '{}'", cand.display());
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    println!(
                        "Loaded BMSP Chart: '{}' ({} / {} keysounds, {} BGA frames, {} BGA videos in-memory from archive, fast-atlas: sound={}, bga={})",
                        chart.header.title, loaded_count, chart.header.wav_table.len(), bga_bank.len(), video_sources.len(),
                        loaded_sound_from_atlas, loaded_bga_from_atlas
                    );
                    return (chart, timing, soundbank, bga_bank, video_sources);
                }
            }
        }
    }

    let path = Path::new(&song.file_path);
    if let Ok(bytes) = fs::read(path) {
        let content = beetle_core::decode_bms_text(&bytes);
        if let Ok(chart) = parse_bms(&content) {
            let timing = TimingModel::from_chart(&chart);
            let parent_dir = path.parent().unwrap_or_else(|| Path::new("."));
            let (soundbank, loaded) = SampleBank::load_chart_soundbank(&chart, parent_dir);
            let mut bga_bank = HashMap::new();

            for (&bmp_id, filename) in &chart.header.bmp_table {
                if let Some(img) = load_image_from_dir_or_case_insensitive(parent_dir, filename) {
                    bga_bank.insert(bmp_id, img);
                }
            }

            let mut video_sources = find_video_files_in_dir(parent_dir, &chart);

            // If folder has no videos, check for adjacent companion package
            if video_sources.is_empty() {
                let dir_name = parent_dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let candidates = [
                    parent_dir.join(format!("{}.bga.bmsp", dir_name)),
                    parent_dir.join("bga.bmsp"),
                ];
                for cand in candidates {
                    if cand.is_file() {
                        if let Ok(mut bga_pkg) = bms_package::PackageReader::open_file(&cand) {
                            load_videos_from_package_archive(&mut bga_pkg, "", &chart, &mut video_sources);
                            if !video_sources.is_empty() {
                                println!("[Loader] Successfully loaded BGA companion package for folder: '{}'", cand.display());
                                break;
                            }
                        }
                    }
                }
            }

            for vs in video_sources.values() {
                if let VideoSource::File(p) = vs {
                    println!("Detected BGA Video: '{}'", p.display());
                }
            }

            println!(
                "Loaded BMS: '{}' ({} keysounds, {} BGA frames loaded, {} BGA videos)",
                chart.header.title, loaded, bga_bank.len(), video_sources.len()
            );
            return (chart, timing, soundbank, bga_bank, video_sources);
        }
    }

    // Fallback demo
    let chart = demo::create_demo_chart();
    let timing = TimingModel::from_chart(&chart);
    let soundbank = demo::create_demo_sample_bank();
    (chart, timing, soundbank, HashMap::new(), HashMap::new())
}

/// Spawns a background thread to load and decode a song's chart, audio soundbank, BGA frames, and video sources.
pub fn spawn_background_song_loader(
    song: &SongMetadata,
) -> Receiver<Result<(BmsChart, TimingModel, SampleBank, HashMap<BmpId, ImageBuffer>, HashMap<BmpId, VideoSource>), String>> {
    let song_clone = song.clone();
    let (tx, rx): (
        Sender<Result<(BmsChart, TimingModel, SampleBank, HashMap<BmpId, ImageBuffer>, HashMap<BmpId, VideoSource>), String>>,
        Receiver<Result<(BmsChart, TimingModel, SampleBank, HashMap<BmpId, ImageBuffer>, HashMap<BmpId, VideoSource>), String>>,
    ) = channel();

    thread::spawn(move || {
        let (chart, timing, bank, bga_bank, video_sources) = load_chart_and_audio(&song_clone);
        let _ = tx.send(Ok((chart, timing, bank, bga_bank, video_sources)));
    });

    rx
}

/// Spawns a background thread to load a song's stage image without blocking the UI thread.
pub fn spawn_background_stage_image_loader(
    song: &SongMetadata,
) -> Receiver<(u64, Option<ImageBuffer>)> {
    let hash = song.hash;
    let song_clone = song.clone();
    let (tx, rx) = channel();

    thread::spawn(move || {
        let img = load_stage_image(&song_clone);
        let _ = tx.send((hash, img));
    });

    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bmsp_in_memory_video_loading() {
        let pkg_path = "../../songs/bms.bmsp";
        if !std::path::Path::new(pkg_path).exists() {
            return;
        }

        let meta = SongMetadata {
            hash: 12345,
            file_path: format!("{}::roop_dotm_ogg/01_roop_dotm7SPN.bms", pkg_path),
            title: "roop_dotm".to_string(),
            subtitle: "".to_string(),
            artist: "roop".to_string(),
            genre: "".to_string(),
            bpm: 150.0,
            play_level: 7,
            notes_count: 100,
            play_mode: beetle_core::PlayMode::Keys7,
        };

        let (_chart, _timing, _soundbank, _bga_bank, video_sources) = load_chart_and_audio(&meta);
        assert!(!video_sources.is_empty(), "Video sources should not be empty for roop_dotm BMSP");

        for (bmp_id, source) in video_sources {
            eprintln!("[TEST BMSP] Found video source for BMP ID: {:?}", bmp_id);
            match source {
                VideoSource::Memory { bytes, filename_hint } => {
                    eprintln!("[TEST BMSP] Memory video size: {} bytes, hint: {:?}", bytes.len(), filename_hint);
                    assert!(!bytes.is_empty());
                    let player = beetle_render::BgaVideoPlayer::open_from_memory(&bytes, filename_hint.as_deref());
                    assert!(player.is_some(), "In-memory video player should open successfully");
                    let pl = player.unwrap();
                    assert!(pl.current_frame().is_some(), "Initial video frame should be decoded");
                    eprintln!("[TEST BMSP] Video dimension: {}x{}", pl.width(), pl.height());
                }
                VideoSource::File(p) => {
                    panic!("BMSP package video should be in-memory, but got file: {}", p.display());
                }
            }
        }
    }

    #[test]
    fn test_turbo_bmsp_loading() {
        use beetle_render::skin::ColorRgba;

        let temp_dir = std::env::temp_dir().join(format!(
            "beetle_loader_test_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        // 1. Create a minimal BMS chart file
        let bms_content = "#TITLE Turbo Test\n#ARTIST Tester\n#BPM 130\n#STAGEFILE stage.bmp\n#WAV01 kick.wav\n#BMP01 bg.bmp\n#00111:01\n#00104:01\n";
        fs::write(temp_dir.join("test.bms"), bms_content).unwrap();

        // 2. Create minimal WAV file
        let wav_bytes = {
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 44100,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut cur = std::io::Cursor::new(Vec::new());
            {
                let mut w = hound::WavWriter::new(&mut cur, spec).unwrap();
                for &s in &[1000i16, 2000, 3000, -1000, -2000, 0] {
                    w.write_sample(s).unwrap();
                }
                w.finalize().unwrap();
            }
            cur.into_inner()
        };
        fs::write(temp_dir.join("kick.wav"), wav_bytes).unwrap();

        // 3. Create minimal BMP images
        let img1 = ImageBuffer::new(32, 32, ColorRgba::new(255, 0, 0, 255));
        let img2 = ImageBuffer::new(64, 48, ColorRgba::new(0, 255, 0, 255));
        fs::write(temp_dir.join("bg.bmp"), img1.encode_bmp_bytes()).unwrap();
        fs::write(temp_dir.join("stage.bmp"), img2.encode_bmp_bytes()).unwrap();

        // 4. Pack folder with Turbo profile
        let turbo_pkg_bytes = bms_package_manager::pack_bms_folder_profile(
            &temp_dir,
            None,
            bms_package_manager::PackProfile::Turbo,
        ).expect("turbo pack failed");
        let pkg_path = temp_dir.join("turbo.bmsp");
        fs::write(&pkg_path, turbo_pkg_bytes).unwrap();

        // 5. Test Turbo loading via load_chart_and_audio
        let meta = SongMetadata {
            hash: 99999,
            file_path: format!("{}::test.bms", pkg_path.to_string_lossy().replace('\\', "/")),
            title: "Turbo Test".to_string(),
            subtitle: "".to_string(),
            artist: "Tester".to_string(),
            genre: "".to_string(),
            bpm: 130.0,
            play_level: 5,
            notes_count: 1,
            play_mode: beetle_core::PlayMode::Keys7,
        };

        let (chart, _timing, soundbank, bga_bank, video_sources) = load_chart_and_audio(&meta);
        assert_eq!(chart.header.title, "Turbo Test");
        assert_eq!(soundbank.len(), 1, "Soundbank should load 1 keysound from Sound Atlas");
        assert!(soundbank.get(beetle_core::WavId(1)).is_some(), "WAV01 should be loaded");
        assert_eq!(bga_bank.len(), 1, "BGA bank should load 1 frame from BGA Atlas");
        assert!(bga_bank.get(&beetle_core::BmpId(1)).is_some(), "BMP01 should be loaded");
        assert!(video_sources.is_empty());

        // 6. Test Stage image loading from Turbo BGA Atlas
        let stage_img = load_stage_image(&meta);
        assert!(stage_img.is_some(), "Stage image should load from BGA Atlas");
        let stage = stage_img.unwrap();
        assert_eq!(stage.width, 64);
        assert_eq!(stage.height, 48);

        // 7. Test Classic fallback loading
        let classic_pkg_bytes = bms_package_manager::pack_bms_folder_profile(
            &temp_dir,
            None,
            bms_package_manager::PackProfile::Classic,
        ).expect("classic pack failed");
        let classic_pkg_path = temp_dir.join("classic.bmsp");
        fs::write(&classic_pkg_path, classic_pkg_bytes).unwrap();

        let classic_meta = SongMetadata {
            hash: 88888,
            file_path: format!("{}::test.bms", classic_pkg_path.to_string_lossy().replace('\\', "/")),
            title: "Turbo Test".to_string(),
            subtitle: "".to_string(),
            artist: "Tester".to_string(),
            genre: "".to_string(),
            bpm: 130.0,
            play_level: 5,
            notes_count: 1,
            play_mode: beetle_core::PlayMode::Keys7,
        };

        let (c_chart, _c_timing, c_soundbank, c_bga_bank, _c_videos) = load_chart_and_audio(&classic_meta);
        assert_eq!(c_chart.header.title, "Turbo Test");
        assert_eq!(c_soundbank.len(), 1, "Soundbank should load 1 keysound from Classic package");
        assert_eq!(c_bga_bank.len(), 1, "BGA bank should load 1 frame from Classic package");

        let c_stage = load_stage_image(&classic_meta);
        assert!(c_stage.is_some(), "Stage image should load from Classic package");

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_large_song_dual_atlas_benchmark() {
        use std::time::Instant;
        use beetle_render::skin::ColorRgba;

        let temp_dir = std::env::temp_dir().join(format!(
            "beetle_benchmark_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        // 1. Generate 250 keysounds (full 2-digit hex space) and 50 BGA frames
        let sound_count = 250;
        let bga_count = 50;

        let mut bms_lines = vec![
            "#TITLE Benchmark Song".to_string(),
            "#ARTIST Large Ensemble".to_string(),
            "#BPM 175".to_string(),
            "#STAGEFILE stage.bmp".to_string(),
        ];

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let wav_bytes = {
            let mut cur = std::io::Cursor::new(Vec::new());
            {
                let mut w = hound::WavWriter::new(&mut cur, spec).unwrap();
                for s in 0..50 {
                    w.write_sample(((s % 20) * 1000) as i16).unwrap();
                }
                w.finalize().unwrap();
            }
            cur.into_inner()
        };

        for i in 1..=sound_count {
            let wav_name = format!("snd_{:03}.wav", i);
            fs::write(temp_dir.join(&wav_name), &wav_bytes).unwrap();
            let key = beetle_core::encode_base36(beetle_core::WavId(i as u16));
            bms_lines.push(format!("#WAV{} {}", key, wav_name));
        }

        let dummy_bmp = ImageBuffer::new(32, 32, ColorRgba::new(100, 150, 200, 255)).encode_bmp_bytes();
        for i in 1..=bga_count {
            let bmp_name = format!("bga_{:02}.bmp", i);
            fs::write(temp_dir.join(&bmp_name), &dummy_bmp).unwrap();
            let key = beetle_core::encode_base36(beetle_core::WavId(i as u16));
            bms_lines.push(format!("#BMP{} {}", key, bmp_name));
        }
        fs::write(temp_dir.join("stage.bmp"), &dummy_bmp).unwrap();

        // Add note events
        bms_lines.push("#00111:01".to_string());
        bms_lines.push("#00104:01".to_string());
        fs::write(temp_dir.join("bench.bms"), bms_lines.join("\n")).unwrap();

        // 2. Pack Classic vs Turbo
        let t0 = Instant::now();
        let classic_bytes = bms_package_manager::pack_bms_folder_profile(
            &temp_dir,
            None,
            bms_package_manager::PackProfile::Classic,
        ).expect("classic pack failed");
        let classic_pack_time = t0.elapsed();

        let t1 = Instant::now();
        let turbo_bytes = bms_package_manager::pack_bms_folder_profile(
            &temp_dir,
            None,
            bms_package_manager::PackProfile::Turbo,
        ).expect("turbo pack failed");
        let turbo_pack_time = t1.elapsed();

        let classic_path = temp_dir.join("classic.bmsp");
        let turbo_path = temp_dir.join("turbo.bmsp");
        fs::write(&classic_path, &classic_bytes).unwrap();
        fs::write(&turbo_path, &turbo_bytes).unwrap();

        println!("[BENCHMARK] Packages created:");
        println!("  - Classic: {} bytes (pack time: {:?})", classic_bytes.len(), classic_pack_time);
        println!("  - Turbo:   {} bytes (pack time: {:?})", turbo_bytes.len(), turbo_pack_time);

        // 3. Measure Loading Time: Classic vs Turbo
        let classic_meta = SongMetadata {
            hash: 10001,
            file_path: format!("{}::bench.bms", classic_path.to_string_lossy().replace('\\', "/")),
            title: "Benchmark Song".to_string(),
            subtitle: "".to_string(),
            artist: "Large Ensemble".to_string(),
            genre: "".to_string(),
            bpm: 175.0,
            play_level: 10,
            notes_count: 500,
            play_mode: beetle_core::PlayMode::Keys7,
        };

        let turbo_meta = SongMetadata {
            hash: 10002,
            file_path: format!("{}::bench.bms", turbo_path.to_string_lossy().replace('\\', "/")),
            title: "Benchmark Song".to_string(),
            subtitle: "".to_string(),
            artist: "Large Ensemble".to_string(),
            genre: "".to_string(),
            bpm: 175.0,
            play_level: 10,
            notes_count: 250,
            play_mode: beetle_core::PlayMode::Keys7,
        };

        // Classic load
        let start_classic = Instant::now();
        let (_c_chart, _c_timing, c_bank, c_bga, _) = load_chart_and_audio(&classic_meta);
        let classic_load_time = start_classic.elapsed();

        // Turbo load
        let start_turbo = Instant::now();
        let (_t_chart, _t_timing, t_bank, t_bga, _) = load_chart_and_audio(&turbo_meta);
        let turbo_load_time = start_turbo.elapsed();

        println!("[BENCHMARK] 250 Keysounds & 50 BGAs In-Game Load Time:");
        println!("  - Classic: {:?}", classic_load_time);
        println!("  - Turbo:   {:?} (Speedup: {:.1}x)", turbo_load_time, classic_load_time.as_secs_f64() / turbo_load_time.as_secs_f64().max(0.0001));

        assert_eq!(c_bank.len(), sound_count, "Classic loaded all keysounds");
        assert_eq!(t_bank.len(), sound_count, "Turbo loaded all keysounds");
        assert_eq!(c_bga.len(), bga_count, "Classic loaded all BGA frames");
        assert_eq!(t_bga.len(), bga_count, "Turbo loaded all BGA frames");

        assert!(turbo_load_time.as_millis() < 80, "Turbo loading should be under 80ms even in debug profile");
        assert!(turbo_load_time < classic_load_time, "Turbo loading should be faster than Classic loading");

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_decoupled_bga_companion() {
        use bms_package_manager::{pack_bms_folder_advanced_with_progress, BgaPackMode, PackOptions};

        let temp_dir = std::env::temp_dir().join(format!(
            "beetle_bga_companion_load_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let src_dir = temp_dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();

        let bms_content = r#"
#TITLE Companion Test Song
#ARTIST Beetle Dev
#WAV01 01.wav
#BMP01 movie.mp4
#00111:01
#00104:01
"#;
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut cur = std::io::Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(&mut cur, spec).unwrap();
            w.write_sample(1000i16).unwrap();
            w.finalize().unwrap();
        }
        let valid_wav = cur.into_inner();

        fs::write(src_dir.join("main.bms"), bms_content).unwrap();
        fs::write(src_dir.join("01.wav"), &valid_wav).unwrap();
        let video_data = vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE];
        fs::write(src_dir.join("movie.mp4"), &video_data).unwrap();

        // 1. Pack with Split BGA mode (Turbo)
        let split_opts = PackOptions::turbo(BgaPackMode::Split);
        let out = pack_bms_folder_advanced_with_progress(&src_dir, None, split_opts, None, |_, _, _, _| {})
            .expect("pack failed");

        let target_dir = temp_dir.join("installed");
        fs::create_dir_all(&target_dir).unwrap();
        let base_bmsp = target_dir.join("test_song.bmsp");
        let bga_bmsp = target_dir.join("test_song.bga.bmsp");
        fs::write(&base_bmsp, &out.base_package).unwrap();
        fs::write(&bga_bmsp, out.bga_package.unwrap()).unwrap();

        let song_meta = SongMetadata {
            hash: 77777,
            file_path: format!("{}::main.bms", base_bmsp.to_string_lossy().replace('\\', "/")),
            title: "Companion Test Song".to_string(),
            subtitle: "".to_string(),
            artist: "Beetle Dev".to_string(),
            genre: "".to_string(),
            bpm: 150.0,
            play_level: 5,
            notes_count: 1,
            play_mode: beetle_core::PlayMode::Keys7,
        };

        // 2. Load with BGA companion present
        let (chart, _timing, soundbank, _bga_bank, video_sources) = load_chart_and_audio(&song_meta);
        assert_eq!(chart.header.title, "Companion Test Song");
        assert_eq!(soundbank.len(), 1);
        assert_eq!(video_sources.len(), 1, "Should load video from companion package");

        let vs = video_sources.values().next().unwrap();
        if let VideoSource::Memory { bytes, filename_hint } = vs {
            assert_eq!(bytes.as_ref(), &video_data);
            assert_eq!(filename_hint.as_deref(), Some("movie.mp4"));
        } else {
            panic!("Expected VideoSource::Memory");
        }

        // 3. Delete companion package -> Verify Graceful Fallback
        fs::remove_file(&bga_bmsp).unwrap();
        let (_chart2, _timing2, soundbank2, _bga_bank2, video_sources2) = load_chart_and_audio(&song_meta);
        assert_eq!(soundbank2.len(), 1, "Audio still loads perfectly");
        assert!(video_sources2.is_empty(), "Video is gracefully omitted when companion is missing");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_turbo_package_with_duplicate_wav_ids() {
        use bms_package_manager::{pack_bms_folder_advanced_with_progress, BgaPackMode, PackOptions};

        let temp_dir = std::env::temp_dir().join(format!("beetle_test_dup_wav_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let song_dir = temp_dir.join("src_song");
        fs::create_dir_all(&song_dir).unwrap();

        // 1. Write BMS with multiple #WAV ids pointing to the same file
        let bms_content = "\
#TITLE Duplicate WAV Test
#ARTIST Beetle Dev
#BPM 130
#PLAYER 1
#WAV01 kick.wav
#WAV02 kick.wav
#WAV03 snare.wav
#00111:01020300
";
        fs::write(song_dir.join("main.bms"), bms_content).unwrap();

        // Create synthetic kick and snare WAV files
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut cur1 = std::io::Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(&mut cur1, spec).unwrap();
            w.write_sample(2000i16).unwrap();
            w.finalize().unwrap();
        }
        let kick_wav = cur1.into_inner();

        let mut cur2 = std::io::Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(&mut cur2, spec).unwrap();
            w.write_sample(1500i16).unwrap();
            w.finalize().unwrap();
        }
        let snare_wav = cur2.into_inner();

        fs::write(song_dir.join("kick.wav"), kick_wav).unwrap();
        fs::write(song_dir.join("snare.wav"), snare_wav).unwrap();

        // Pack as Turbo package
        let turbo_opts = PackOptions::turbo(BgaPackMode::Embed);
        let out = pack_bms_folder_advanced_with_progress(
            &song_dir,
            None,
            turbo_opts,
            None,
            |_, _, _, _| {},
        ).unwrap();

        let out_bmsp = temp_dir.join("dup_test.bmsp");
        fs::write(&out_bmsp, &out.base_package).unwrap();

        let song_meta = SongMetadata {
            hash: 88888,
            file_path: format!("{}::main.bms", out_bmsp.to_string_lossy().replace('\\', "/")),
            title: "Duplicate WAV Test".to_string(),
            subtitle: "".to_string(),
            artist: "Beetle Dev".to_string(),
            genre: "".to_string(),
            bpm: 130.0,
            play_level: 5,
            notes_count: 3,
            play_mode: beetle_core::PlayMode::Keys7,
        };

        // Load chart and audio from the packed Turbo package
        let (_chart, _timing, soundbank, _bga_bank, _videos) = load_chart_and_audio(&song_meta);

        assert_eq!(soundbank.len(), 3, "All 3 #WAV entries must be populated in soundbank");
        assert!(soundbank.contains_key(beetle_core::WavId(1)), "WavId 1 (kick.wav) must exist");
        assert!(soundbank.contains_key(beetle_core::WavId(2)), "WavId 2 (kick.wav duplicate) must exist");
        assert!(soundbank.contains_key(beetle_core::WavId(3)), "WavId 3 (snare.wav) must exist");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_real_aliceblue_package() {
        println!("Current dir: {:?}", std::env::current_dir());
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
        let pkg_path = Path::new(&manifest_dir).join("../../songs/aliceblue.bmsp");
        println!("Checking pkg_path: {:?} (exists: {})", pkg_path, pkg_path.exists());
        if !pkg_path.exists() {
            return;
        }

        let song_meta = SongMetadata {
            hash: 11111,
            file_path: format!("{}::alice7-1.bme", pkg_path.to_string_lossy().replace('\\', "/")),
            title: "aliceblue (Radio Edit)".to_string(),
            subtitle: "".to_string(),
            artist: "nekodex".to_string(),
            genre: "".to_string(),
            bpm: 175.0,
            play_level: 10,
            notes_count: 1000,
            play_mode: beetle_core::PlayMode::Keys7,
        };

        let (chart, _timing, soundbank, _bga_bank, _videos) = load_chart_and_audio(&song_meta);
        println!("[VERIFICATION] alice7-1.bme wav_table count: {}, loaded in soundbank: {}", chart.header.wav_table.len(), soundbank.len());
        assert!(soundbank.len() >= 193, "Should load at least 193 keysounds, got {}", soundbank.len());
    }

    #[test]
    fn test_load_real_andromeda_package() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
        let pkg_path = Path::new(&manifest_dir).join("../../songs/星の器.bmsp");
        if !pkg_path.exists() {
            return;
        }

        let song_meta = SongMetadata {
            hash: 22222,
            file_path: format!("{}::marisa(NORMAL7).bme", pkg_path.to_string_lossy().replace('\\', "/")),
            title: "STAR OF ANDROMEDA".to_string(),
            subtitle: "".to_string(),
            artist: "D.Watt".to_string(),
            genre: "".to_string(),
            bpm: 170.0,
            play_level: 6,
            notes_count: 500,
            play_mode: beetle_core::PlayMode::Keys7,
        };

        let (chart, _timing, soundbank, _bga_bank, _videos) = load_chart_and_audio(&song_meta);
        println!("[VERIFICATION] marisa(NORMAL7).bme wav_table count: {}, loaded in soundbank: {}", chart.header.wav_table.len(), soundbank.len());
        assert!(soundbank.len() >= chart.header.wav_table.len(), "All keysounds in wav_table should be loaded");
        assert!(soundbank.contains_key(beetle_core::WavId(2)), "WavId 02 (bd.wav) must be loaded");
        assert!(soundbank.contains_key(beetle_core::decode_base36(b'0', b'W').unwrap()), "WavId 0W (bass-01.wav) must be loaded");
    }
}



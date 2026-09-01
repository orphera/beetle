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

                    for (&wav_id, filename) in &chart.header.wav_table {
                        if let Some(target_path) = pkg.find_entry_path(&base_dir, filename) {
                            if let Ok(bytes) = pkg.read_entry(&target_path) {
                                if let Ok(pcm) = SampleBank::load_audio_from_bytes(&bytes) {
                                    soundbank.insert(wav_id, pcm);
                                    loaded_count += 1;
                                }
                            }
                        }
                    }

                    for (&bmp_id, filename) in &chart.header.bmp_table {
                        if is_video_path(filename) {
                            if let Some(target_path) = pkg.find_entry_path(&base_dir, filename) {
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
                                if let Some(target_path) = pkg.find_entry_path(&base_dir, &candidate) {
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
                            // Always load still image if available as BGA fallback
                            if let Some(target_path) = pkg.find_entry_path(&base_dir, filename) {
                                if let Ok(bytes) = pkg.read_entry(&target_path) {
                                    if let Some(img) = ImageBuffer::from_bytes(&bytes) {
                                        bga_bank.insert(bmp_id, img);
                                    }
                                }
                            }
                        }
                    }

                    // Fallback video inside .bmsp
                    if video_sources.is_empty() {
                        let mut fallback_entry = None;
                        for filename in &[&chart.header.stage_file, &chart.header.banner] {
                            if !filename.is_empty() && is_video_path(filename) {
                                if let Some(target_path) = pkg.find_entry_path(&base_dir, filename) {
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
                                if let Some(target_path) = pkg.find_entry_path(&base_dir, name) {
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

                    println!(
                        "Loaded BMSP Chart: '{}' ({} / {} keysounds, {} BGA frames, {} BGA videos in-memory from archive)",
                        chart.header.title, loaded_count, chart.header.wav_table.len(), bga_bank.len(), video_sources.len()
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

            let video_sources = find_video_files_in_dir(parent_dir, &chart);
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
}

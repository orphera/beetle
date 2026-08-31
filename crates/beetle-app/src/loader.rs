use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use beetle_audio::SampleBank;
use beetle_core::{parse_bms, BmpId, BmsChart, SongMetadata, TimingModel};
use beetle_render::{is_video_path, ImageBuffer};

use crate::demo;

pub const ARTWORKS_CACHE_DIR: &str = ".cache/artworks";

const ARTWORK_CANDIDATE_FILENAMES: &[&str] = &[
    "stagefile.bmp", "stage.bmp", "banner.bmp", "title.bmp",
    "stagefile.png", "stage.png", "banner.png", "title.png",
    "stagefile.jpg", "stage.jpg", "banner.jpg", "title.jpg",
    "STAGEFILE.BMP", "STAGE.BMP", "BANNER.BMP", "TITLE.BMP",
];

fn load_image_from_dir_or_case_insensitive(dir: &Path, filename: &str) -> Option<ImageBuffer> {
    let direct = dir.join(filename);
    if let Some(img) = ImageBuffer::load_from_file(&direct) {
        return Some(img);
    }

    // Try case-insensitive matching and alternate extensions (.bmp, .png, .jpg, .jpeg)
    let stem = match filename.rfind('.') {
        Some(pos) => &filename[..pos],
        None => filename,
    };

    if let Ok(entries) = fs::read_dir(dir) {
        let entries_list: Vec<_> = entries.flatten().collect();

        // 1. Direct case-insensitive match
        for entry in &entries_list {
            if let Some(name) = entry.file_name().to_str() {
                if name.eq_ignore_ascii_case(filename) {
                    if let Some(img) = ImageBuffer::load_from_file(entry.path()) {
                        return Some(img);
                    }
                }
            }
        }

        // 2. Alternate extension match (.bmp, .png, .jpg, .jpeg)
        for ext in &["bmp", "png", "jpg", "jpeg"] {
            let alt_target = format!("{}.{}", stem, ext);
            for entry in &entries_list {
                if let Some(name) = entry.file_name().to_str() {
                    if name.eq_ignore_ascii_case(&alt_target) {
                        if let Some(img) = ImageBuffer::load_from_file(entry.path()) {
                            return Some(img);
                        }
                    }
                }
            }
        }
    }

    None
}

fn find_video_file_in_dir(dir: &Path, chart: &BmsChart) -> Option<PathBuf> {
    // 1. Check bmp_table for video files
    for filename in chart.header.bmp_table.values() {
        if is_video_path(filename) {
            let p = dir.join(filename);
            if p.exists() {
                return Some(p);
            }
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.eq_ignore_ascii_case(filename) {
                            return Some(entry.path());
                        }
                    }
                }
            }
        }
    }

    // 2. Check stage_file or banner
    for filename in &[&chart.header.stage_file, &chart.header.banner] {
        if !filename.is_empty() && is_video_path(filename) {
            let p = dir.join(filename);
            if p.exists() {
                return Some(p);
            }
        }
    }

    // 3. Fallback common video names in song directory
    for name in &[
        "bga.mp4", "movie.mp4", "video.mp4", "bg.mp4", "pv.mp4",
        "bga.mpg", "movie.mpg", "video.mpg", "bg.mpg",
        "bga.wmv", "movie.wmv", "video.wmv", "bg.wmv",
        "bga.avi", "movie.avi", "video.avi", "bg.avi",
        "bga.webm", "movie.webm", "video.webm", "bg.webm",
        "bga.mkv", "movie.mkv", "video.mkv", "bg.mkv",
        "BGA.MP4", "MOVIE.MP4", "VIDEO.MP4", "BG.MP4", "PV.MP4",
    ] {
        let p = dir.join(name);
        if p.exists() {
            return Some(p);
        }
    }

    None
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
                let content = String::from_utf8_lossy(&bms_bytes);
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
        let content = String::from_utf8_lossy(&bytes);
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
) -> (BmsChart, TimingModel, SampleBank, HashMap<BmpId, ImageBuffer>, Option<PathBuf>) {
    if song.file_path == ":demo:" {
        let chart = demo::create_demo_chart();
        let timing = TimingModel::from_chart(&chart);
        let soundbank = demo::create_demo_sample_bank();
        let bga_bank = HashMap::new();
        return (chart, timing, soundbank, bga_bank, None);
    }

    // Check if song is inside a .bmsp package
    if let Some((pkg_path, entry_name)) = song.file_path.split_once("::") {
        if let Ok(mut pkg) = bms_package::PackageReader::open_file(pkg_path) {
            let base_dir = Path::new(entry_name)
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .to_string_lossy();

            if let Ok(bms_bytes) = pkg.read_entry(entry_name) {
                let content = String::from_utf8_lossy(&bms_bytes);
                if let Ok(chart) = parse_bms(&content) {
                    let timing = TimingModel::from_chart(&chart);
                    let mut soundbank = SampleBank::new();
                    let mut bga_bank = HashMap::new();
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
                        if let Some(target_path) = pkg.find_entry_path(&base_dir, filename) {
                            if let Ok(bytes) = pkg.read_entry(&target_path) {
                                if let Some(img) = ImageBuffer::from_bytes(&bytes) {
                                    bga_bank.insert(bmp_id, img);
                                }
                            }
                        }
                    }

                    println!(
                        "Loaded BMSP Chart: '{}' ({} / {} keysounds, {} BGA frames decoded in-memory from archive)",
                        chart.header.title, loaded_count, chart.header.wav_table.len(), bga_bank.len()
                    );
                    return (chart, timing, soundbank, bga_bank, None);
                }
            }
        }
    }

    let path = Path::new(&song.file_path);
    if let Ok(bytes) = fs::read(path) {
        let content = String::from_utf8_lossy(&bytes);
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

            let video_path = find_video_file_in_dir(parent_dir, &chart);
            if let Some(ref vp) = video_path {
                println!("Detected BGA Video: '{}'", vp.display());
            }

            println!(
                "Loaded BMS: '{}' ({} keysounds, {} BGA frames loaded)",
                chart.header.title, loaded, bga_bank.len()
            );
            return (chart, timing, soundbank, bga_bank, video_path);
        }
    }

    // Fallback demo
    let chart = demo::create_demo_chart();
    let timing = TimingModel::from_chart(&chart);
    let soundbank = demo::create_demo_sample_bank();
    (chart, timing, soundbank, HashMap::new(), None)
}

/// Spawns a background thread to load and decode a song's chart, audio soundbank, BGA frames, and video path.
pub fn spawn_background_song_loader(
    song: &SongMetadata,
) -> Receiver<Result<(BmsChart, TimingModel, SampleBank, HashMap<BmpId, ImageBuffer>, Option<PathBuf>), String>> {
    let song_clone = song.clone();
    let (tx, rx): (
        Sender<Result<(BmsChart, TimingModel, SampleBank, HashMap<BmpId, ImageBuffer>, Option<PathBuf>), String>>,
        Receiver<Result<(BmsChart, TimingModel, SampleBank, HashMap<BmpId, ImageBuffer>, Option<PathBuf>), String>>,
    ) = channel();

    thread::spawn(move || {
        let (chart, timing, bank, bga_bank, video_path) = load_chart_and_audio(&song_clone);
        let _ = tx.send(Ok((chart, timing, bank, bga_bank, video_path)));
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

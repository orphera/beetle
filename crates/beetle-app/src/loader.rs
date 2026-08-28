use std::fs;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use beetle_audio::SampleBank;
use beetle_core::{parse_bms, BmsChart, SongMetadata, TimingModel};
use beetle_render::ImageBuffer;

use crate::demo;

pub const ARTWORKS_CACHE_DIR: &str = ".cache/artworks";

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
                                if let Some(img) = ImageBuffer::from_bmp_bytes(&img_bytes) {
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
                                if let Some(img) = ImageBuffer::from_bmp_bytes(&img_bytes) {
                                    let _ = fs::create_dir_all(cache_dir);
                                    let _ = fs::write(&cache_file, &img_bytes);
                                    return Some(img);
                                }
                            }
                        }
                    }
                }
            }

            for name in &[
                "stagefile.bmp", "stage.bmp", "banner.bmp", "title.bmp",
                "STAGEFILE.BMP", "STAGE.BMP", "BANNER.BMP", "TITLE.BMP",
            ] {
                if let Some(path) = pkg.find_entry_path(&base_dir, name) {
                    if let Ok(img_bytes) = pkg.read_entry(&path) {
                        if let Some(img) = ImageBuffer::from_bmp_bytes(&img_bytes) {
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
                let p = dir.join(&chart.header.stage_file);
                if let Some(img) = ImageBuffer::load_from_file(&p) {
                    if let Ok(data) = fs::read(&p) {
                        let _ = fs::create_dir_all(cache_dir);
                        let _ = fs::write(&cache_file, data);
                    }
                    return Some(img);
                }
            }
            if !chart.header.banner.is_empty() {
                let p = dir.join(&chart.header.banner);
                if let Some(img) = ImageBuffer::load_from_file(&p) {
                    if let Ok(data) = fs::read(&p) {
                        let _ = fs::create_dir_all(cache_dir);
                        let _ = fs::write(&cache_file, data);
                    }
                    return Some(img);
                }
            }
        }
    }

    // Fallback file scanning for common artwork names
    for name in &[
        "stagefile.bmp", "stage.bmp", "banner.bmp", "title.bmp",
        "STAGEFILE.BMP", "STAGE.BMP", "BANNER.BMP", "TITLE.BMP",
    ] {
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

/// Loads and parses the BMS chart file and pre-decodes the entire keysound samplebank into PCM memory.
pub fn load_chart_and_audio(song: &SongMetadata) -> (BmsChart, TimingModel, SampleBank) {
    if song.file_path == ":demo:" {
        let chart = demo::create_demo_chart();
        let timing = TimingModel::from_chart(&chart);
        let soundbank = demo::create_demo_sample_bank();
        return (chart, timing, soundbank);
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

                    println!(
                        "Loaded BMSP Chart: '{}' ({} / {} keysounds decoded in-memory from archive)",
                        chart.header.title, loaded_count, chart.header.wav_table.len()
                    );
                    return (chart, timing, soundbank);
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
            println!(
                "Loaded BMS: '{}' ({} keysounds loaded)",
                chart.header.title, loaded
            );
            return (chart, timing, soundbank);
        }
    }

    // Fallback demo
    let chart = demo::create_demo_chart();
    let timing = TimingModel::from_chart(&chart);
    let soundbank = demo::create_demo_sample_bank();
    (chart, timing, soundbank)
}

/// Spawns a background thread to load and decode a song's chart and audio soundbank.
pub fn spawn_background_song_loader(
    song: &SongMetadata,
) -> Receiver<Result<(BmsChart, TimingModel, SampleBank), String>> {
    let song_clone = song.clone();
    let (tx, rx): (
        Sender<Result<(BmsChart, TimingModel, SampleBank), String>>,
        Receiver<Result<(BmsChart, TimingModel, SampleBank), String>>,
    ) = channel();

    thread::spawn(move || {
        let (chart, timing, bank) = load_chart_and_audio(&song_clone);
        let _ = tx.send(Ok((chart, timing, bank)));
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

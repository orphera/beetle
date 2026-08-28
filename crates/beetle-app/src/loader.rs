use std::fs;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use beetle_audio::{PcmBuffer, SampleBank};
use beetle_core::{parse_bms, BmsChart, SongMetadata, TimingModel};
use beetle_render::ImageBuffer;

use crate::demo;

/// Loads stage artwork image for a song if available on disk.
pub fn load_stage_image(song: &SongMetadata) -> Option<ImageBuffer> {
    if song.file_path == ":demo:" {
        return None;
    }

    let song_path = Path::new(&song.file_path);
    let dir = song_path.parent().unwrap_or_else(|| Path::new("."));

    // Check parsed chart header for stagefile or banner
    if let Ok(content) = fs::read_to_string(song_path) {
        if let Ok(chart) = parse_bms(&content) {
            if !chart.header.stage_file.is_empty() {
                let p = dir.join(&chart.header.stage_file);
                if let Some(img) = ImageBuffer::load_from_file(&p) {
                    return Some(img);
                }
            }
            if !chart.header.banner.is_empty() {
                let p = dir.join(&chart.header.banner);
                if let Some(img) = ImageBuffer::load_from_file(&p) {
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
            return Some(img);
        }
    }

    None
}

/// Loads a preview audio sample (dedicated preview.ogg/wav or first keysound).
pub fn load_preview_sample(song: &SongMetadata) -> Option<PcmBuffer> {
    if song.file_path == ":demo:" {
        return None;
    }

    let song_path = Path::new(&song.file_path);
    let dir = song_path.parent().unwrap_or_else(|| Path::new("."));

    // 1. Common preview audio filenames
    for name in &[
        "preview.ogg", "preview.wav", "PREVIEW.OGG", "PREVIEW.WAV",
        "intro.ogg", "intro.wav", "INTRO.OGG", "INTRO.WAV",
    ] {
        let p = dir.join(name);
        if p.exists() {
            if let Ok(pcm) = SampleBank::load_audio_file(&p) {
                return Some(pcm);
            }
        }
    }

    // 2. Fallback: Parse chart, find first valid keysound longer than 0.4s
    if let Ok(content) = fs::read_to_string(song_path) {
        if let Ok(chart) = parse_bms(&content) {
            for filename in chart.header.wav_table.values() {
                let p = dir.join(filename);
                if let Ok(pcm) = SampleBank::load_audio_file(&p) {
                    if pcm.duration_seconds() > 0.4 {
                        return Some(pcm);
                    }
                }
            }
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

    let path = Path::new(&song.file_path);
    if let Ok(content) = fs::read_to_string(path) {
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

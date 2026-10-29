//! Research helper: one disc song laid out to look at and listen to. Writes
//! `info.txt` (its record, files and each chart's BGM start), its loading
//! picture, its word pictures, and as WAV the BGM stream its charts play
//! with the two streams on either side (`bgm_<file>.wav`, the chosen one
//! marked), for checking which stream is the song's by ear.
//!
//! cargo run -p cs-disc --release --example song_dump -- <image.iso> <song id> <out dir>
use std::fmt::Write as _;

use cs_disc::iidx::Disc;
use cs_disc::ply::Command;
use cs_disc::{identify, SectorReader, Volume};

fn wav(pcm: &cs_disc::wvb::Pcm) -> Vec<u8> {
    let channels = pcm.channels.len() as u32;
    let frames = pcm.channels[0].len();
    let bytes = frames as u32 * channels * 2;
    let mut out = b"RIFF".to_vec();
    out.extend((36 + bytes).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend((channels as u16).to_le_bytes());
    out.extend(pcm.rate.to_le_bytes());
    out.extend((pcm.rate * channels * 2).to_le_bytes());
    out.extend(((channels * 2) as u16).to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend(bytes.to_le_bytes());
    for i in 0..frames {
        for c in &pcm.channels {
            out.extend(c.get(i).copied().unwrap_or(0).to_le_bytes());
        }
    }
    out
}

fn png_like(img: &cs_disc::ps2tex::Rgba) -> Vec<u8> {
    let mut f = format!("P6 {} {} 255\n", img.width, img.height).into_bytes();
    for p in &img.pixels {
        f.extend(&p[..3]);
    }
    f
}

fn mask_pgm(m: &cs_disc::popn::Mask) -> Vec<u8> {
    let mut f = format!("P5 {} {} 255\n", m.width, m.height).into_bytes();
    f.extend(&m.coverage);
    f
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut d = SectorReader::open_image(&a[0]).unwrap();
    let edition = identify(&mut d).unwrap().unwrap();
    let volume = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &volume, edition).unwrap();
    let id: u32 = a[1].parse().unwrap();
    let out = std::path::Path::new(&a[2]);
    std::fs::create_dir_all(out).unwrap();
    let song = disc
        .songs
        .iter()
        .find(|s| s.id == id)
        .expect("no such song");
    let mut info = String::new();
    writeln!(info, "{edition:?} record {} \"{}\"", song.id, song.title).unwrap();
    writeln!(info, "sounds {:?}", song.sounds).unwrap();
    let mut streams = Vec::new();
    for chart in &song.charts {
        let (bank, bgm) = song.sound_files(chart.style, chart.difficulty);
        let ply = disc.chart_ply(&mut d, chart).unwrap();
        let start = ply
            .events
            .iter()
            .find(|e| matches!(e.command, Command::Auto { sound, .. } if sound == edition.bgm_sound()))
            .map(|e| ply.millis(e.time));
        let last = ply
            .events
            .iter()
            .filter(|e| matches!(e.command, Command::Note { .. }))
            .map(|e| ply.millis(e.time))
            .fold(0.0, f64::max);
        writeln!(
            info,
            "{:?} {:?} level {}: chart file {}, bank {bank:?}, BGM {bgm:?}, BGM start {start:?} ms, last note {last:.0} ms",
            chart.style, chart.difficulty, chart.level, chart.file
        )
        .unwrap();
        streams.extend(bgm);
    }
    streams.sort_unstable();
    streams.dedup();
    for &chosen in &streams {
        for f in chosen.saturating_sub(2)..=chosen + 2 {
            let Ok(bytes) = disc.read_file(&mut d, f) else {
                continue;
            };
            match cs_disc::wvb::decode_bgm(&bytes) {
                Ok(pcm) => {
                    let mark = if f == chosen { "_chosen" } else { "" };
                    writeln!(
                        info,
                        "stream file {f}{mark}: {} Hz, {} ch, {:.1} s",
                        pcm.rate,
                        pcm.channels.len(),
                        pcm.channels[0].len() as f64 / pcm.rate as f64
                    )
                    .unwrap();
                    std::fs::write(out.join(format!("bgm_{f}{mark}.wav")), wav(&pcm)).unwrap();
                }
                Err(e) => writeln!(info, "stream file {f}: not a stream ({e})").unwrap(),
            }
        }
    }
    if let Ok(Some(img)) = disc.stage_image(&mut d, song, false) {
        std::fs::write(out.join("loading.ppm"), png_like(&img)).unwrap();
    }
    if let Ok(Some(names)) = disc.song_names(&mut d, song) {
        for (name, m) in [
            ("title", &names.title),
            ("small_title", &names.small_title),
            ("artist", &names.artist),
            ("genre", &names.genre),
        ] {
            if let Some(m) = m {
                std::fs::write(out.join(format!("word_{name}.pgm")), mask_pgm(m)).unwrap();
            }
        }
    }
    std::fs::write(out.join("info.txt"), info).unwrap();
}

//! Research helper: each song's record index, chart file, key sound bank
//! and BGM file (SP HYPER's), title, the banks its charts play and its video
//! file, tab-separated.
//!
//! cargo run -p cs-disc --release --example song_files -- <image.iso>
use cs_disc::iidx::{Difficulty, Disc, Style};
use cs_disc::{identify, SectorReader, Volume};

fn main() {
    let iso = std::env::args().nth(1).unwrap();
    let mut d = SectorReader::open_image(&iso).unwrap();
    let edition = identify(&mut d).unwrap().unwrap();
    let volume = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &volume, edition).unwrap();
    for song in &disc.songs {
        let (bank, bgm) = song.sound_files(Style::Single, Difficulty::Hyper);
        let mut banks: Vec<u32> = song
            .charts
            .iter()
            .filter_map(|c| song.sound_files(c.style, c.difficulty).0)
            .collect();
        banks.sort_unstable();
        banks.dedup();
        println!(
            "{}\t{}\t{:?}\t{:?}\t{}\t{:?}\t{}",
            song.id,
            song.charts[0].file,
            bank,
            bgm,
            song.title,
            banks,
            song.video.map_or(String::new(), |v| v.to_string())
        );
    }
}

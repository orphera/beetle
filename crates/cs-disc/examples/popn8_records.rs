//! Research helper: pop'n 8's song records next to the chart files in
//! `SOUND/SD_01.DAT` order: each record's two file fields, the chart the
//! player picks for it and that chart's place among all chart files, and
//! the first tempo of that chart and of the chart in the record's place.
//!
//! cargo run -p cs-disc --release --example popn8_records -- <image.iso>
use cs_disc::iidx::Disc;
use cs_disc::{identify, SectorReader, Volume};

fn main() {
    let iso = std::env::args().nth(1).unwrap();
    let mut d = SectorReader::open_image(&iso).unwrap();
    let edition = identify(&mut d).unwrap().unwrap();
    let volume = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &volume, edition).unwrap();
    let charts: Vec<u32> = (0..245)
        .filter(|&f| {
            disc.read_file(&mut d, f)
                .is_ok_and(|b| b.first() == Some(&0x20))
        })
        .collect();
    let bpm = |d: &mut SectorReader<std::fs::File>, file: u32| -> Option<f64> {
        let raw = disc.read_chart(d, file).ok()?;
        let list = u32::from_le_bytes(raw.get(4..8)?.try_into().ok()?) as usize;
        raw.get(list..)?
            .chunks_exact(8)
            .find(|e| e[5] == 4)
            .map(|e| u16::from_le_bytes([e[6], e[7]]) as f64)
    };
    let exe = volume.read_file(&mut d, "SLPM_653.16").unwrap().unwrap();
    for r in 0..104usize {
        let at = 0xc6d30 + r * 0x38;
        let f = |k: usize| u32::from_le_bytes(exe[at + k..at + k + 4].try_into().unwrap());
        let song = disc.songs.iter().find(|s| s.id == r as u32);
        let chart = song.map(|s| s.charts[0].file);
        let rank = chart.and_then(|c| charts.iter().position(|&x| x == c));
        let now = chart.and_then(|c| bpm(&mut d, c));
        let placed = charts.get(r).and_then(|&c| bpm(&mut d, c));
        println!(
            "{r}	{}	{}	{chart:?}	{rank:?}	{:?}	{now:?}	{placed:?}	{}",
            f(0x10),
            f(0x14),
            charts.get(r),
            song.map_or("", |s| &s.title)
        );
    }
}

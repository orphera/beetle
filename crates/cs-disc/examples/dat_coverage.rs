//! Research helper: how much of each data file the file table covers, and
//! the gaps it leaves (offset, bytes), where unlisted data may sit.
//!
//! cargo run -p cs-disc --release --example dat_coverage -- <image.iso>
use cs_disc::iidx::Disc;
use cs_disc::{identify, SectorReader, Volume};

fn main() {
    let iso = std::env::args().nth(1).unwrap();
    let mut d = SectorReader::open_image(&iso).unwrap();
    let edition = identify(&mut d).unwrap().unwrap();
    let volume = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &volume, edition).unwrap();
    let mut spans: std::collections::BTreeMap<u8, Vec<(u64, u64)>> = Default::default();
    for n in 0.. {
        let Some(e) = disc.file(n) else { break };
        spans
            .entry(e.dat)
            .or_default()
            .push((e.offset, e.offset + e.size as u64));
    }
    for (dat, mut s) in spans {
        s.sort_unstable();
        let covered: u64 = s.iter().map(|(a, b)| b - a).sum();
        let end = s.iter().map(|&(_, b)| b).max().unwrap_or(0);
        println!(
            "dat{dat}: {} files, {covered} bytes listed, last byte {end}",
            s.len()
        );
        let mut at = 0;
        for (a, b) in s {
            if a > at + 0x10000 {
                println!("  gap {at:#x}..{a:#x} ({} bytes)", a - at);
            }
            at = at.max(b);
        }
    }
}

//! Research helper: which of a disc's files contain a byte string (hex),
//! with the first offsets. Skips MPEG videos.
//!
//! cargo run -p cs-disc --release --example find_bytes -- <image.iso> <hex> [<from file> <to file>]
use cs_disc::iidx::Disc;
use cs_disc::{identify, SectorReader, Volume};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut d = SectorReader::open_image(&a[0]).unwrap();
    let edition = identify(&mut d).unwrap().unwrap();
    let volume = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &volume, edition).unwrap();
    let needle: Vec<u8> = (0..a[1].len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&a[1][i..i + 2], 16).unwrap())
        .collect();
    let from: u32 = a.get(2).map_or(0, |s| s.parse().unwrap());
    let to: u32 = a.get(3).map_or(u32::MAX, |s| s.parse().unwrap());
    for n in from..=to {
        let Some(entry) = disc.file(n) else { break };
        if entry.size > 40_000_000 {
            continue;
        }
        let Ok(bytes) = disc.read_file(&mut d, n) else {
            continue;
        };
        if bytes.starts_with(&[0, 0, 1, 0xba]) {
            continue;
        }
        let hits: Vec<usize> = bytes
            .windows(needle.len())
            .enumerate()
            .filter(|(_, w)| *w == &needle[..])
            .map(|(i, _)| i)
            .take(4)
            .collect();
        if !hits.is_empty() {
            println!("{n} ({} bytes): {hits:x?}", entry.size);
        }
    }
}

//! Research helper: what each file of a disc's data files starts with, run
//! together by kind, to find where pictures are.
//!
//! cargo run -p cs-disc --release --example file_kinds -- <image.iso>
use cs_disc::iidx::Disc;
use cs_disc::{identify, SectorReader, Volume};

fn main() {
    let iso = std::env::args().nth(1).unwrap();
    let mut d = SectorReader::open_image(&iso).unwrap();
    let edition = identify(&mut d).unwrap().unwrap();
    let volume = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &volume, edition).unwrap();
    let mut last = String::new();
    let mut run = 0;
    for n in 0.. {
        let Some(entry) = disc.file(n) else { break };
        let head = disc
            .read_file(&mut d, n)
            .map(|b| b[..16.min(b.len())].to_vec())
            .unwrap_or_default();
        let kind = if head.starts_with(&[0, 0, 1, 0xba]) {
            "mpeg".to_string()
        } else {
            head.iter().take(8).map(|b| format!("{b:02x}")).collect()
        };
        let key = format!("{} {}", entry.dat, &kind[..kind.len().min(8)]);
        if key == last {
            run += 1;
            continue;
        }
        if run > 0 {
            println!("   ... x{run}");
        }
        println!("{n:5} dat{} {:>9} {kind}", entry.dat, entry.size);
        last = key;
        run = 0;
    }
}

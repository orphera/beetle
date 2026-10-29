//! Research helper: writes data file number <n> of a disc to <out>.
//!
//! cargo run -p cs-disc --release --example file_dump -- <image.iso> <n> <out>
use cs_disc::iidx::Disc;
use cs_disc::{identify, SectorReader, Volume};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut d = SectorReader::open_image(&a[0]).unwrap();
    let edition = identify(&mut d).unwrap().unwrap();
    let volume = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &volume, edition).unwrap();
    std::fs::write(
        &a[2],
        disc.read_file(&mut d, a[1].parse().unwrap()).unwrap(),
    )
    .unwrap();
}

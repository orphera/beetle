//! Research helper: writes one file of a disc's DAT files by its number.
//!
//! ```text
//! cargo run -p cs-disc --example song_file -- <image.iso> <file number> <out file>
//! cargo run -p cs-disc --example song_file -- <image.iso> c<file number> <out file>   (a chart, unpacked)
//! ```

use cs_disc::iidx::Disc;
use cs_disc::{identify, Volume};

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [image, file, out] = args.as_slice() else {
        eprintln!("usage: song_file <image.iso> <file number> <out file>");
        std::process::exit(2);
    };
    let mut disc = cs_disc::open(image)?;
    let volume = Volume::open(&mut disc)?;
    let edition = identify(&mut disc)?.expect("an IIDX disc");
    let songs = Disc::open(&mut disc, &volume, edition)?;
    let data = match file.strip_prefix('c') {
        Some(chart) => songs.read_chart(&mut disc, chart.parse().unwrap())?,
        None => songs.read_file(&mut disc, file.parse().unwrap())?,
    };
    std::fs::write(out, data)
}

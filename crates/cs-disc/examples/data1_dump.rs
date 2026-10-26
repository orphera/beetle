//! Research helper: writes every `DATA1.DAT` file of a disc to a folder as
//! `<number>.bin`.
//!
//! ```text
//! cargo run -p cs-disc --example data1_dump -- <image.iso> <out dir>
//! ```

use std::io;

use cs_disc::iidx::Disc;
use cs_disc::{identify, Volume};

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [image, out] = args.as_slice() else {
        eprintln!("usage: data1_dump <image.iso> <out dir>");
        std::process::exit(2);
    };
    let mut disc = cs_disc::open(image)?;
    let edition = identify(&mut disc)?.ok_or_else(|| io::Error::other("unknown disc"))?;
    let volume = Volume::open(&mut disc)?;
    let read = Disc::open(&mut disc, &volume, edition)?;
    std::fs::create_dir_all(out)?;
    let mut written = 0;
    for file in 0..read.data1_count() as u32 {
        if let Some(bytes) = read.read_data1(&mut disc, file)? {
            std::fs::write(format!("{out}/{file:04}.bin"), bytes)?;
            written += 1;
        }
    }
    println!("{written} of {} files", read.data1_count());
    Ok(())
}

//! Research helper: lists a disc's root, or writes one root file out.
//!
//! ```text
//! cargo run -p cs-disc --example disc_file -- <image.iso> [<name> <out file> | <directory>]
//! ```

use std::io;

use cs_disc::Volume;

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut disc = cs_disc::open(&args[0])?;
    let volume = Volume::open(&mut disc)?;
    match args.get(1..3) {
        Some([name, out]) => {
            let bytes = volume
                .read_file(&mut disc, name)?
                .ok_or_else(|| io::Error::other("no such file"))?;
            std::fs::write(out, bytes)
        }
        _ => {
            let entries = match args.get(1) {
                Some(dir) => {
                    let extent = volume
                        .find_dir(&mut disc, dir)?
                        .ok_or_else(|| io::Error::other("no such directory"))?;
                    volume.list(&mut disc, extent)?
                }
                None => volume.root(&mut disc)?,
            };
            for e in entries {
                println!("{}\t{}\t{}", e.name, e.extent.size, e.extent.lba);
            }
            Ok(())
        }
    }
}

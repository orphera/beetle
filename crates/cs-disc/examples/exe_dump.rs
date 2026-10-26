//! Research helper: writes a disc's boot executable and its song list
//! (`id<TAB>title<TAB>levels`) next to each other, for looking for tables the
//! player does not read yet (such as the dan courses).
//!
//! ```text
//! cargo run -p cs-disc --example exe_dump -- <image.iso> <out dir>
//! ```

use std::fmt::Write as _;
use std::io;

use cs_disc::iidx::Disc;
use cs_disc::{identify, Volume};

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [image, out] = args.as_slice() else {
        eprintln!("usage: exe_dump <image.iso> <out dir>");
        std::process::exit(2);
    };
    let mut disc = cs_disc::open(image)?;
    let volume = Volume::open(&mut disc)?;
    let cnf = volume
        .read_file(&mut disc, "SYSTEM.CNF")?
        .ok_or_else(|| io::Error::other("no SYSTEM.CNF"))?;
    let cnf = String::from_utf8_lossy(&cnf);
    let boot = cnf
        .lines()
        .find_map(|l| l.trim().strip_prefix("BOOT2"))
        .and_then(|l| l.rsplit(['\\', ':']).next())
        .map(|l| l.trim().trim_end_matches(";1").to_string())
        .ok_or_else(|| io::Error::other("no BOOT2"))?;
    let exe = volume
        .read_file(&mut disc, &boot)?
        .ok_or_else(|| io::Error::other("no boot executable"))?;
    std::fs::create_dir_all(out)?;
    std::fs::write(format!("{out}/{boot}"), &exe)?;

    let mut list = String::new();
    if let Some(edition) = identify(&mut disc)? {
        let songs = Disc::open(&mut disc, &volume, edition)?;
        for song in &songs.songs {
            let levels: Vec<String> = song.charts.iter().map(|c| c.level.to_string()).collect();
            let _ = writeln!(list, "{}\t{}\t{}", song.id, song.title, levels.join(","));
        }
    }
    std::fs::write(format!("{out}/{boot}.songs.tsv"), list)?;
    println!("{boot}: {} bytes", exe.len());
    Ok(())
}

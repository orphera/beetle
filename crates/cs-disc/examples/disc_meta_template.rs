//! Writes a blank song details table for a disc (format: `cs_disc::meta`):
//! every song's id and disc title, for a person to fill in. See docs/specs/disc_meta.md.
//!
//! cargo run -p cs-disc --release --example disc_meta_template -- <image.iso | E:> [out.tsv]
//! Without `out.tsv`, prints the table.
use cs_disc::iidx::Disc;
use cs_disc::{identify, Volume};
use std::fmt::Write;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut d = cs_disc::open(&a[0]).expect("disc opens");
    let Some(edition) = identify(&mut d).expect("disc reads") else {
        eprintln!("{}: not a disc this player reads", a[0]);
        std::process::exit(1);
    };
    let v = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &v, edition).unwrap();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# {} song details: fill in by hand. An empty cell keeps what the disc says.",
        edition.name()
    );
    let _ = writeln!(
        out,
        "# Save as disc_meta/{}.tsv next to library.dat.",
        edition.slug()
    );
    let _ = writeln!(out, "id\tdisc_title\ttitle\tsubtitle\tartist\tgenre\tnote");
    for song in &disc.songs {
        let _ = writeln!(out, "{}\t{}\t\t\t\t\t", song.id, song.title);
    }
    match a.get(1) {
        Some(path) => std::fs::write(path, out).unwrap(),
        None => print!("{out}"),
    }
}

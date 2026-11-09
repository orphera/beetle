//! Research helper: lists a GuitarFreaks & DrumMania disc's songs and the
//! tracks of their charts.
//!
//! cargo run -p cs-disc --release --example gfdm_songs -- <image.iso>
use cs_disc::gfdm::{Disc, Kind, Seq, EVENT_NOTE};
use cs_disc::Volume;

fn main() {
    let iso = std::env::args().nth(1).unwrap();
    let mut d = cs_disc::open(&iso).unwrap();
    let volume = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &volume)
        .unwrap()
        .expect("no PACKINFO.BIN");
    println!("{} files", disc.index.entries.len());
    let songs = disc.songs(&mut d).unwrap();
    for song in &songs {
        let mut line = format!("pack {:3}:", song.pack);
        for &(n, kind) in &song.files {
            match kind {
                Kind::Seq => {
                    let seq = Seq::parse(&disc.read(&mut d, n).unwrap()).unwrap();
                    for t in seq.tracks.iter().filter(|t| !t.is_meta) {
                        let notes = t.events.iter().filter(|e| e.kind == EVENT_NOTE).count();
                        let g = match t.game {
                            cs_disc::gfdm::Game::DrumMania => "DM",
                            cs_disc::gfdm::Game::GuitarFreaks => "GF",
                        };
                        line += &format!(" {g}{}.{}={notes}", t.difficulty, t.part);
                    }
                }
                Kind::SongInfo => {
                    let b = disc.read(&mut d, n).unwrap();
                    let v: Vec<u16> = b
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect();
                    line += &format!(" info={v:?}");
                }
                _ => {}
            }
        }
        println!("{line}");
    }
    println!("{} songs", songs.len());
}

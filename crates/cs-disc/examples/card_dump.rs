//! Writes songs' loading cards as raw RGBA (`<id>.rgba`: u32 width, u32
//! height, then pixels), to compare with another extraction.
//!
//! cargo run -p cs-disc --release --example card_dump -- <iso> <out dir> [<song id>:<container> ...]
//! Without pairs, every song's card by `Song::card`.
use cs_disc::iidx16::Disc;
use cs_disc::{identify, Game, Volume};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::Path::new(&a[1]);
    std::fs::create_dir_all(out).unwrap();
    let mut d = cs_disc::open(&a[0]).unwrap();
    let Some(Game::Iidx16(ed)) = identify(&mut d).unwrap() else {
        panic!("not IIDX 16")
    };
    let v = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &v, ed).unwrap();
    let pairs: Vec<(u32, u32)> = if a.len() > 2 {
        a[2..]
            .iter()
            .map(|p| {
                let (s, c) = p.split_once(':').unwrap();
                (s.parse().unwrap(), c.parse().unwrap())
            })
            .collect()
    } else {
        disc.songs
            .iter()
            .filter_map(|s| s.card.map(|c| (s.id, c)))
            .collect()
    };
    for (id, container) in pairs {
        match disc.card_image(&mut d, container).unwrap() {
            Some(img) => {
                let mut bytes = img.width.to_le_bytes().to_vec();
                bytes.extend(img.height.to_le_bytes());
                bytes.extend(img.pixels.iter().flatten());
                std::fs::write(out.join(format!("{id:04}.rgba")), bytes).unwrap();
            }
            None => println!("{id}: no picture in container {container}"),
        }
    }
}

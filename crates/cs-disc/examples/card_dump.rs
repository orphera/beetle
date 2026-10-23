//! Writes loading cards as BMP pictures, to look at or to compare with
//! another extraction. With `<song id>:<container>` pairs, those containers
//! (as `<id>.bmp`); with `--scan <from> <to>`, every container in that range
//! that holds a picture (as `c<container>.bmp`), to find a song's card by
//! eye; with neither, every song's card by `Song::card`.
//!
//! cargo run -p cs-disc --release --example card_dump -- <image.iso | E:> <out dir> [<id>:<container> ... | --scan <from> <to>]
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
    let pairs: Vec<(String, u32)> = if a.get(2).map(String::as_str) == Some("--scan") {
        let (from, to): (u32, u32) = (a[3].parse().unwrap(), a[4].parse().unwrap());
        (from..=to).map(|c| (format!("c{c}"), c)).collect()
    } else if a.len() > 2 {
        a[2..]
            .iter()
            .map(|p| {
                let (s, c) = p.split_once(':').unwrap();
                (
                    format!("{:04}", s.parse::<u32>().unwrap()),
                    c.parse().unwrap(),
                )
            })
            .collect()
    } else {
        disc.songs
            .iter()
            .filter_map(|s| s.card.map(|c| (format!("{:04}", s.id), c)))
            .collect()
    };
    for (id, container) in pairs {
        match disc.card_image(&mut d, container).unwrap() {
            Some(img) => std::fs::write(out.join(format!("{id}.bmp")), bmp(&img)).unwrap(),
            None if id.starts_with('c') => {}
            None => println!("{id}: no picture in container {container}"),
        }
    }
}

/// A 32-bit top-down BMP.
fn bmp(img: &cs_disc::ps2tex::Rgba) -> Vec<u8> {
    let size = 54 + img.pixels.len() as u32 * 4;
    let mut b = b"BM".to_vec();
    for v in [size, 0, 54, 40, img.width] {
        b.extend(v.to_le_bytes());
    }
    b.extend((-(img.height as i32)).to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(32u16.to_le_bytes());
    for v in [0u32, size - 54, 2835, 2835, 0, 0] {
        b.extend(v.to_le_bytes());
    }
    for [r, g, bl, _] in &img.pixels {
        b.extend([*bl, *g, *r, 255]);
    }
    b
}

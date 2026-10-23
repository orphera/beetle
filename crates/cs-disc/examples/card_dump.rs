//! Writes loading cards as BMP pictures, to look at or to compare with
//! another extraction. With `<song id>:<container>` pairs, those containers
//! (as `<id>.bmp`); with `--scan <from> <to>`, every container in that range
//! that holds a picture (as `c<container>.bmp`), to find a song's card by
//! eye; with neither, every song's card by `Song::card`.
//!
//! cargo run -p cs-disc --release --example card_dump -- <image.iso | E:> <out dir> [<id>:<container> ... | --scan <from> <to>]
use cs_disc::{identify, Game, Volume};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::Path::new(&a[1]);
    std::fs::create_dir_all(out).unwrap();
    let mut d = cs_disc::open(&a[0]).unwrap();
    let Some(game) = identify(&mut d).unwrap() else {
        panic!("unknown disc")
    };
    let v = Volume::open(&mut d).unwrap();
    let mut cards: Vec<(String, u32)> = Vec::new();
    let scan = a.get(2).map(String::as_str) == Some("--scan");
    match game {
        Game::Iidx15(ed) => {
            let disc = cs_disc::iidx15::Disc::open(&mut d, &v, ed).unwrap();
            if scan {
                let (from, to): (u32, u32) = (a[3].parse().unwrap(), a[4].parse().unwrap());
                cards.extend((from..=to).map(|c| (format!("c{c}"), c)));
            } else if a.len() > 2 {
                cards.extend(a[2..].iter().map(|p| {
                    let (s, c) = p.split_once(':').unwrap();
                    (
                        format!("{:04}", s.parse::<u32>().unwrap()),
                        c.parse().unwrap(),
                    )
                }));
            } else {
                cards.extend(
                    disc.songs
                        .iter()
                        .filter_map(|s| s.overlay.map(|c| (format!("{:04}", s.id), c))),
                );
            }
            dump(&mut d, &disc, &cards, out);
            return;
        }
        Game::Iidx16(ed) => {
            let disc = cs_disc::iidx16::Disc::open(&mut d, &v, ed).unwrap();
            if scan {
                let (from, to): (u32, u32) = (a[3].parse().unwrap(), a[4].parse().unwrap());
                cards.extend((from..=to).map(|c| (format!("c{c}"), c)));
            } else if a.len() > 2 {
                cards.extend(a[2..].iter().map(|p| {
                    let (s, c) = p.split_once(':').unwrap();
                    (
                        format!("{:04}", s.parse::<u32>().unwrap()),
                        c.parse().unwrap(),
                    )
                }));
            } else {
                cards.extend(
                    disc.songs
                        .iter()
                        .filter_map(|s| s.card.map(|c| (format!("{:04}", s.id), c))),
                );
            }
            dump(&mut d, &disc, &cards, out);
        }
    }
}

fn dump<R: std::io::Read + std::io::Seek, D: CardDisc<R>>(
    d: &mut cs_disc::SectorReader<R>,
    disc: &D,
    pairs: &[(String, u32)],
    out: &std::path::Path,
) {
    for (id, container) in pairs {
        match disc.card_image(d, *container).unwrap() {
            Some(img) => std::fs::write(out.join(format!("{id}.bmp")), bmp(&img)).unwrap(),
            None if id.starts_with('c') => {}
            None => println!("{id}: no picture in container {container}"),
        }
    }
}

trait CardDisc<R: std::io::Read + std::io::Seek> {
    fn card_image(
        &self,
        disc: &mut cs_disc::SectorReader<R>,
        container: u32,
    ) -> std::io::Result<Option<cs_disc::ps2tex::Rgba>>;
}

impl<R: std::io::Read + std::io::Seek> CardDisc<R> for cs_disc::iidx15::Disc {
    fn card_image(
        &self,
        disc: &mut cs_disc::SectorReader<R>,
        container: u32,
    ) -> std::io::Result<Option<cs_disc::ps2tex::Rgba>> {
        self.card_image(disc, container)
    }
}

impl<R: std::io::Read + std::io::Seek> CardDisc<R> for cs_disc::iidx16::Disc {
    fn card_image(
        &self,
        disc: &mut cs_disc::SectorReader<R>,
        container: u32,
    ) -> std::io::Result<Option<cs_disc::ps2tex::Rgba>> {
        self.card_image(disc, container)
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

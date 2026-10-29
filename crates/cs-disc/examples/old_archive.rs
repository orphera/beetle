//! Research helper: the pictures in 3rd to 8th style archives. An archive
//! is a `u32` part count, then from 0x10 the parts' offsets; each part is
//! LZ-packed as the charts are, and a picture part unpacks to the form
//! `ps2tex::decode_old_picture` reads (8 or 4 bits a pixel). Writes each as
//! PPM and lists the parts that are not pictures it knows.
//!
//! cargo run -p cs-disc --release --example old_archive -- <image.iso> <out dir> <from file> <to file>
use cs_disc::iidx::Disc;
use cs_disc::{identify, SectorReader, Volume};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut d = SectorReader::open_image(&a[0]).unwrap();
    let edition = identify(&mut d).unwrap().unwrap();
    let volume = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &volume, edition).unwrap();
    let out = std::path::Path::new(&a[1]);
    std::fs::create_dir_all(out).unwrap();
    let (from, to): (u32, u32) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    let (mut pictures, mut others) = (0, 0);
    for n in from..=to {
        let Some(entry) = disc.file(n) else { break };
        if entry.size > 8_000_000 {
            continue;
        }
        let Ok(b) = disc.read_file(&mut d, n) else {
            continue;
        };
        let w = |i: usize| {
            b.get(i..i + 4)
                .map(|s| u32::from_le_bytes(s.try_into().unwrap()) as usize)
        };
        let Some(count) = w(0).filter(|&c| (1..4096).contains(&c)) else {
            continue;
        };
        let Some(offs) = (0..count)
            .map(|k| w(0x10 + 4 * k))
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        if offs.windows(2).any(|p| p[0] >= p[1]) || offs.last().is_some_and(|&o| o >= b.len()) {
            continue;
        }
        let ends: Vec<usize> = offs.iter().skip(1).copied().chain([b.len()]).collect();
        for k in 0..count {
            let Some(p) = cs_disc::lz::decode(&b[offs[k]..ends[k]]) else {
                continue;
            };
            match cs_disc::ps2tex::decode_old_picture(&p) {
                Some(img) => {
                    let mut f = format!("P6 {} {} 255\n", img.width, img.height).into_bytes();
                    for px in &img.pixels {
                        f.extend(&px[..3]);
                    }
                    std::fs::write(out.join(format!("{n:04}_{k:02}.ppm")), f).unwrap();
                    pictures += 1;
                }
                None => {
                    let head: String = p.iter().take(12).map(|x| format!("{x:02x}")).collect();
                    println!("{n} part {k}: {} bytes, not a picture: {head}", p.len());
                    others += 1;
                }
            }
        }
    }
    println!("{pictures} pictures, {others} other parts");
}

//! Research helper: the pictures in 3rd to 8th style archives. An archive
//! is a `u32` part count, then from 0x10 the parts' offsets; each part is
//! LZ-packed as the charts are. A picture part unpacks to a 0x14-byte head
//! (`u32` 0x10, `u16`, `u16` width, ...), a 256-entry 16-bit palette (CSM1
//! order) and, from 0x220, one index byte a pixel. Writes each as PPM.
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
        let Some(count) = w(0).filter(|&c| (1..64).contains(&c)) else {
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
            if p.len() <= 0x220 || u32::from_le_bytes(p[..4].try_into().unwrap()) != 0x10 {
                continue;
            }
            let width = u16::from_le_bytes([p[6], p[7]]) as usize;
            let pixels = p.len() - 0x220;
            if width == 0 || pixels % width != 0 {
                continue;
            }
            let height = pixels / width;
            let swz = |i: usize| (i & 0xe7) | ((i & 8) << 1) | ((i & 16) >> 1);
            let pal: Vec<[u8; 3]> = (0..256)
                .map(|i| {
                    let v = u16::from_le_bytes([p[0x14 + 2 * i], p[0x15 + 2 * i]]);
                    [
                        ((v & 31) << 3) as u8,
                        (((v >> 5) & 31) << 3) as u8,
                        (((v >> 10) & 31) << 3) as u8,
                    ]
                })
                .collect();
            let mut f = format!("P6 {width} {height} 255\n").into_bytes();
            for &i in &p[0x220..] {
                f.extend(pal[swz(i as usize)]);
            }
            std::fs::write(out.join(format!("{n:04}_{k:02}.ppm")), f).unwrap();
        }
    }
}

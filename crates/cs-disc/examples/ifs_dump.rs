//! Research helper: the textures of 3rd to 8th style `IFs` files, as the
//! 8th style executable reads them (0x12df20): `"IFs" 05`, the first and the
//! last texture number, then from 0x0c one `u32` per number, the texture's
//! offset in words. A texture is `u32` kind (1 PSMT8, 0 PSMT4) and LZ-packed
//! pixels (`lz`), 128x128. The palette is not in the file, so the indices
//! are written as grey (PGM), with every file's textures side by side in
//! `<file>_sheet.pgm` as well.
//!
//! cargo run -p cs-disc --release --example ifs_dump -- <image.iso> <out dir>
use cs_disc::iidx::Disc;
use cs_disc::{identify, SectorReader, Volume};

const SIDE: usize = 128;

fn pgm(w: usize, h: usize, px: &[u8]) -> Vec<u8> {
    let mut f = format!("P5 {w} {h} 255\n").into_bytes();
    f.extend(px);
    f
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut d = SectorReader::open_image(&a[0]).unwrap();
    let edition = identify(&mut d).unwrap().unwrap();
    let volume = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &volume, edition).unwrap();
    let out = std::path::Path::new(&a[1]);
    std::fs::create_dir_all(out).unwrap();
    let u32_at = |b: &[u8], at: usize| {
        b.get(at..at + 4)
            .map(|s| u32::from_le_bytes(s.try_into().unwrap()) as usize)
    };
    let mut total = 0;
    for n in 0.. {
        let Some(entry) = disc.file(n) else { break };
        if entry.size > 8_000_000 {
            continue;
        }
        let Ok(b) = disc.read_file(&mut d, n) else {
            continue;
        };
        if !b.starts_with(b"IFs\x05") {
            continue;
        }
        let (Some(first), Some(last)) = (u32_at(&b, 4), u32_at(&b, 8)) else {
            continue;
        };
        let mut tiles = Vec::new();
        for id in first..=last {
            let Some(at) = u32_at(&b, 0x0c + (id - first) * 4).map(|w| w * 4) else {
                continue;
            };
            let (Some(kind), Some(packed)) = (u32_at(&b, at), b.get(at + 4..)) else {
                continue;
            };
            let Some(p) = cs_disc::lz::decode(packed) else {
                println!("{n} texture {id}: does not unpack");
                continue;
            };
            let grey: Vec<u8> = match kind {
                1 => p.iter().take(SIDE * SIDE).copied().collect(),
                0 => p
                    .iter()
                    .take(SIDE * SIDE / 2)
                    .flat_map(|&v| [(v & 15) * 17, (v >> 4) * 17])
                    .collect(),
                _ => {
                    println!("{n} texture {id}: kind {kind}");
                    continue;
                }
            };
            if grey.len() < SIDE * SIDE {
                println!("{n} texture {id}: {} pixels", grey.len());
                continue;
            }
            std::fs::write(
                out.join(format!("{n:04}_{id:04}.pgm")),
                pgm(SIDE, SIDE, &grey),
            )
            .unwrap();
            tiles.push(grey);
        }
        if tiles.is_empty() {
            continue;
        }
        total += tiles.len();
        let across = 8.min(tiles.len());
        let down = tiles.len().div_ceil(across);
        let (w, h) = (across * SIDE, down * SIDE);
        let mut sheet = vec![40u8; w * h];
        for (k, t) in tiles.iter().enumerate() {
            let (tx, ty) = ((k % across) * SIDE, (k / across) * SIDE);
            for y in 0..SIDE {
                sheet[(ty + y) * w + tx..(ty + y) * w + tx + SIDE]
                    .copy_from_slice(&t[y * SIDE..(y + 1) * SIDE]);
            }
        }
        std::fs::write(out.join(format!("{n:04}_sheet.pgm")), pgm(w, h, &sheet)).unwrap();
        println!("{n}: textures {first}..{last}, {} written", tiles.len());
    }
    println!("{total} textures");
}

//! Research helper: the `IMAGE.DAT` files of a pop'n disc that list
//! rectangles (part 0), with how many there are and how many are
//! banner-sized (about 244x58), to find a disc's banner file.
//!
//! With `RECTS=<file>` it lists that file's rectangles; with `DUMP=<dir>` it
//! also writes the first rectangles of every listed file there as PPM.
//!
//! cargo run -p cs-disc --release --example popn_rects -- <image.iso>
use cs_disc::iidx::Disc;
use cs_disc::{identify, SectorReader, Volume};

fn main() {
    let iso = std::env::args().nth(1).unwrap();
    let mut d = SectorReader::open_image(&iso).unwrap();
    let edition = identify(&mut d).unwrap().unwrap();
    let volume = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &volume, edition).unwrap();
    let u32_at = |b: &[u8], at: usize| {
        b.get(at..at + 4)
            .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
    };
    for n in 0..2000u32 {
        let Ok(Some(file)) = disc.read_data1(&mut d, n) else {
            if n > 0 && disc.read_data1(&mut d, n).is_err() {
                break;
            }
            continue;
        };
        let Some(at) = u32_at(&file, 0).filter(|&a| a != 0) else {
            continue;
        };
        let Some(size) = u32_at(&file, at as usize) else {
            continue;
        };
        if size > 1 << 16 {
            continue;
        }
        let Some(part) = file
            .get(at as usize + 4..)
            .and_then(|b| cs_disc::popn::unpack(b, size as usize))
        else {
            continue;
        };
        let field = |k: usize| part.get(k..k + 2).map(|b| i16::from_le_bytes([b[0], b[1]]));
        if field(0) != Some(0) || field(4) != Some(1) {
            continue;
        }
        let rects: Vec<(i16, i16)> = (0x10..part.len().saturating_sub(7))
            .step_by(8)
            .map(|k| (field(k + 4).unwrap(), field(k + 6).unwrap()))
            .collect();
        let banners = rects
            .iter()
            .filter(|&&(w, h)| (236..=248).contains(&w) && (56..=60).contains(&h))
            .count();
        if banners > 0 || rects.len() > 50 {
            println!(
                "{n}: {} rects, {banners} banner-sized, sheets {:?}",
                rects.len(),
                field(2)
            );
            if let Ok(dir) = std::env::var("DUMP") {
                std::fs::create_dir_all(&dir).unwrap();
                for k in 0..rects.len().min(6) as u32 {
                    if let Some(img) = cs_disc::popn::rect_picture(&file, k) {
                        let mut f = format!(
                            "P6 {} {} 255
",
                            img.width, img.height
                        )
                        .into_bytes();
                        for p in &img.pixels {
                            f.extend(&p[..3]);
                        }
                        std::fs::write(format!("{dir}/{n:04}_{k}.ppm"), f).unwrap();
                    }
                }
            }
            if std::env::var("RECTS").is_ok_and(|v| v == n.to_string()) {
                for (i, r) in rects.iter().enumerate() {
                    print!("{i}:{}x{} ", r.0, r.1);
                }
                println!();
            }
        }
    }
}

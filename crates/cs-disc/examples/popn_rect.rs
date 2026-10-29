//! Research helper: writes rectangles of one pop'n `IMAGE.DAT` file as PPM.
//!
//! cargo run -p cs-disc --release --example popn_rect -- <image.iso> <file> <out dir> <rect>...
use cs_disc::iidx::Disc;
use cs_disc::{identify, SectorReader, Volume};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut d = SectorReader::open_image(&a[0]).unwrap();
    let edition = identify(&mut d).unwrap().unwrap();
    let volume = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &volume, edition).unwrap();
    let file = disc
        .read_data1(&mut d, a[1].parse().unwrap())
        .unwrap()
        .unwrap();
    std::fs::create_dir_all(&a[2]).unwrap();
    for n in &a[3..] {
        match cs_disc::popn::rect_picture(&file, n.parse().unwrap()) {
            Some(img) => {
                let mut f = format!("P6 {} {} 255\n", img.width, img.height).into_bytes();
                for p in &img.pixels {
                    f.extend(&p[..3]);
                }
                std::fs::write(format!("{}/{n}.ppm", a[2]), f).unwrap();
            }
            None => println!("{n}: none"),
        }
    }
}

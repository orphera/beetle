//! Research helper: the LZ-packed pictures (`ps2tex::decode_old_picture`)
//! anywhere in a file such as a 3rd to 8th style executable, found by their
//! packed head (`?? 10 00 80`: a control byte, then the picture's `u32`
//! 0x10), written as PPM named by their offset.
//!
//! cargo run -p cs-disc --release --example exe_textures -- <file> <out dir>
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let b = std::fs::read(&a[0]).unwrap();
    let out = std::path::Path::new(&a[1]);
    std::fs::create_dir_all(out).unwrap();
    let mut found = 0;
    for at in (0..b.len() - 4).filter(|&i| b[i + 1..i + 4] == [0x10, 0x00, 0x80]) {
        let Some(p) = cs_disc::lz::decode(&b[at..]) else {
            continue;
        };
        let Some(img) = cs_disc::ps2tex::decode_old_picture(&p) else {
            continue;
        };
        let mut f = format!("P6 {} {} 255\n", img.width, img.height).into_bytes();
        for px in &img.pixels {
            f.extend(&px[..3]);
        }
        std::fs::write(out.join(format!("{at:07x}.ppm")), f).unwrap();
        found += 1;
    }
    println!("{found} pictures");
}

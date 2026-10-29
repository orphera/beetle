//! Research helper: the LZ-packed 8-bit textures (`old_archive`'s picture
//! form) anywhere in a file, found by their packed head (`04 10 00 80 09`),
//! written as PPM named by their offset.
//!
//! cargo run -p cs-disc --release --example exe_textures -- <file> <out dir>
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let b = std::fs::read(&a[0]).unwrap();
    let out = std::path::Path::new(&a[1]);
    std::fs::create_dir_all(out).unwrap();
    let head = [0x04, 0x10, 0x00, 0x80, 0x09];
    for at in (0..b.len() - 5).filter(|&i| b[i..i + 5] == head) {
        let Some(p) = cs_disc::lz::decode(&b[at..]) else { continue };
        if p.len() <= 0x220 {
            continue;
        }
        let width = u16::from_le_bytes([p[6], p[7]]) as usize;
        let pixels = p.len() - 0x220;
        if width == 0 || pixels % width != 0 {
            println!("{at:x}: {} bytes, width {width}?", p.len());
            continue;
        }
        let height = pixels / width;
        let swz = |i: usize| (i & 0xe7) | ((i & 8) << 1) | ((i & 16) >> 1);
        let pal: Vec<[u8; 3]> = (0..256)
            .map(|i| {
                let v = u16::from_le_bytes([p[0x14 + 2 * i], p[0x15 + 2 * i]]);
                [((v & 31) << 3) as u8, (((v >> 5) & 31) << 3) as u8, (((v >> 10) & 31) << 3) as u8]
            })
            .collect();
        let mut f = format!("P6 {width} {height} 255\n").into_bytes();
        for &i in &p[0x220..] {
            f.extend(pal[swz(i as usize)]);
        }
        std::fs::write(out.join(format!("{at:07x}.ppm")), f).unwrap();
        println!("{at:x}: {width}x{height}");
    }
}

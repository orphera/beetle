//! Research helper: an old IIDX archive's parts unpacked (`old_archive`),
//! with each part's size and first 0x24 bytes, and the parts written raw.
//!
//! cargo run -p cs-disc --release --example lz_parts -- <archive file> <out dir>
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let b = std::fs::read(&a[0]).unwrap();
    std::fs::create_dir_all(&a[1]).unwrap();
    let w = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap()) as usize;
    let count = w(0);
    let offs: Vec<usize> = (0..count)
        .map(|k| w(0x10 + 4 * k))
        .chain([b.len()])
        .collect();
    for k in 0..count {
        match cs_disc::lz::decode(&b[offs[k]..offs[k + 1]]) {
            Some(p) => {
                let head: String = p.iter().take(0x24).map(|x| format!("{x:02x} ")).collect();
                println!("{k}: {} bytes: {head}", p.len());
                std::fs::write(format!("{}/{k}.raw", a[1]), p).unwrap();
            }
            None => println!("{k}: does not unpack"),
        }
    }
}

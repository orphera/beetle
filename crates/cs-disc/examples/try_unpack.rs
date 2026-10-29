//! Research helper: tries the pop'n picture compression (`popn::unpack`) at
//! offsets of a file, printing how much comes out and its first bytes.
//!
//! cargo run -p cs-disc --release --example try_unpack -- <file> <hex offset>...
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let b = std::fs::read(&a[0]).unwrap();
    for o in &a[1..] {
        let at = usize::from_str_radix(o, 16).unwrap();
        for size in [0x100usize, 0x1000, 0x10000, 0x100000] {
            match cs_disc::popn::unpack(&b[at..], size) {
                Some(p) => {
                    let head: String = p.iter().take(24).map(|x| format!("{x:02x} ")).collect();
                    println!("{at:x} size {size:x}: ok {head}");
                }
                None => println!("{at:x} size {size:x}: no"),
            }
        }
    }
}

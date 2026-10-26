//! Research helper: unpacks every entry of every `DATA1.DAT` archive of a
//! disc into a folder as `<file>_<entry>.bin` (LZSS-decoded where stored
//! packed), for looking for data the player does not read yet.
//!
//! ```text
//! cargo run -p cs-disc --example data1_unpack -- <image.iso> <out dir>
//! ```

use std::io;

use cs_disc::iidx::Disc;
use cs_disc::ps2tex::decode_lzss;
use cs_disc::{identify, Volume};

fn u32_at(b: &[u8], at: usize) -> usize {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]) as usize
}

/// The entries of an archive: header bytes and count (u16 each), then one
/// (unpacked, packed) size pair per entry right before the data.
fn entries(archive: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    if archive.len() < 4 {
        return out;
    }
    let data = u16::from_le_bytes([archive[0], archive[1]]) as usize;
    let count = u16::from_le_bytes([archive[2], archive[3]]) as usize;
    let Some(sizes) = data.checked_sub((count + 1) * 8) else {
        return out;
    };
    let mut at = data;
    for k in 0..=count {
        let Some(record) = archive.get(sizes + k * 8..sizes + k * 8 + 8) else {
            break;
        };
        let (unpacked, packed) = (u32_at(record, 0), u32_at(record, 4));
        let Some(bytes) = archive.get(at..at + packed) else {
            break;
        };
        at += packed;
        let entry = if packed == unpacked {
            Some(bytes.to_vec())
        } else {
            decode_lzss(bytes, unpacked)
        };
        out.push(entry.unwrap_or_default());
    }
    out
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [image, out] = args.as_slice() else {
        eprintln!("usage: data1_unpack <image.iso> <out dir>");
        std::process::exit(2);
    };
    let mut disc = cs_disc::open(image)?;
    let edition = identify(&mut disc)?.ok_or_else(|| io::Error::other("unknown disc"))?;
    let volume = Volume::open(&mut disc)?;
    let read = Disc::open(&mut disc, &volume, edition)?;
    std::fs::create_dir_all(out)?;
    let mut written = 0;
    for file in 0..read.data1_count() as u32 {
        let Some(archive) = read.read_data1(&mut disc, file)? else {
            continue;
        };
        for (k, entry) in entries(&archive).into_iter().enumerate() {
            if !entry.is_empty() {
                std::fs::write(format!("{out}/{file:04}_{k:03}.bin"), entry)?;
                written += 1;
            }
        }
    }
    println!("{written} entries");
    Ok(())
}

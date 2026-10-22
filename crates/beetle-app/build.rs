//! Build script for the `beetle-app` executable.
//!
//! On MSVC, the exe's static imports (hid.dll, user32.dll, ...) are resolved
//! from the system directory only (`DEPENDENTLOADFLAG` with
//! `LOAD_LIBRARY_SEARCH_SYSTEM32`, 0x800), so a DLL planted beside the exe in
//! a user-writable folder is never loaded. XInput is loaded the same way
//! (`xinput.rs`). Other toolchains do not take the MSVC linker flag.
//!
//! The app icon is drawn here (no image file, like the skin) and linked as a
//! compiled resource (`.res`, which `link.exe` takes as an input file): icon
//! group 1, read back at run time for the window (`APP_ICON_ID` in `main.rs`).

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bin=beetle-app=/DEPENDENTLOADFLAG:0x800");
        let out = std::env::var("OUT_DIR").expect("OUT_DIR");
        let res = Path::new(&out).join("app_icon.res");
        std::fs::write(&res, icon_res(&[16, 24, 32, 48, 64])).expect("write app_icon.res");
        println!("cargo:rustc-link-arg-bin=beetle-app={}", res.display());
    }
}

/// The theme's cyan and magenta (`beetle_render::theme`), and the note white.
const CYAN: [f32; 3] = [0.0, 229.0, 255.0];
const MAGENTA: [f32; 3] = [255.0, 45.0, 106.0];
const WHITE: [f32; 3] = [255.0, 255.0, 255.0];

/// The icon at `size` px, RGBA rows top to bottom: a rounded square in a
/// cyan-to-magenta diagonal, two falling note bars and the judge line.
fn draw(size: u32) -> Vec<u8> {
    const SS: u32 = 4;
    let n = size as f32;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let mut acc = [0.0f32; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    // Sample point in 0..1 icon units.
                    let u = (x as f32 + (sx as f32 + 0.5) / SS as f32) / n;
                    let v = (y as f32 + (sy as f32 + 0.5) / SS as f32) / n;
                    if let Some(rgb) = sample(u, v) {
                        acc[0] += rgb[0];
                        acc[1] += rgb[1];
                        acc[2] += rgb[2];
                        acc[3] += 1.0;
                    }
                }
            }
            let k = (SS * SS) as f32;
            let a = acc[3] / k;
            if a > 0.0 {
                out.extend_from_slice(&[
                    (acc[0] / acc[3]) as u8,
                    (acc[1] / acc[3]) as u8,
                    (acc[2] / acc[3]) as u8,
                    (a * 255.0) as u8,
                ]);
            } else {
                out.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    out
}

/// The colour at (u, v) in 0..1 icon units, `None` outside the shape.
fn sample(u: f32, v: f32) -> Option<[f32; 3]> {
    // Rounded square with a small margin.
    let (m, r) = (0.04, 0.22);
    let (cx, cy) = (u.clamp(m + r, 1.0 - m - r), v.clamp(m + r, 1.0 - m - r));
    if (u - cx).powi(2) + (v - cy).powi(2) > r * r {
        return None;
    }
    let t = ((u + v) / 2.0).clamp(0.0, 1.0);
    let bg = [
        CYAN[0] + (MAGENTA[0] - CYAN[0]) * t,
        CYAN[1] + (MAGENTA[1] - CYAN[1]) * t,
        CYAN[2] + (MAGENTA[2] - CYAN[2]) * t,
    ];
    let inside = |x0: f32, y0: f32, x1: f32, y1: f32| u >= x0 && u <= x1 && v >= y0 && v <= y1;
    // Two notes in neighbouring lanes, then the judge line across.
    let mark = inside(0.22, 0.24, 0.48, 0.36)
        || inside(0.52, 0.46, 0.78, 0.58)
        || inside(0.16, 0.70, 0.84, 0.78);
    Some(if mark { WHITE } else { bg })
}

/// A `.res` file holding one icon group (id 1) with an image per size.
fn icon_res(sizes: &[u32]) -> Vec<u8> {
    let mut res = Vec::new();
    // Every .res starts with an empty entry.
    entry(&mut res, 0, 0, &[]);
    let mut group = Vec::new();
    group.extend_from_slice(&0u16.to_le_bytes()); // reserved
    group.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    group.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    for (i, &size) in sizes.iter().enumerate() {
        let image = dib(size, &draw(size));
        let id = (i + 1) as u16;
        entry(&mut res, 3, id, &image); // RT_ICON
        let side = if size >= 256 { 0 } else { size as u8 };
        group.extend_from_slice(&[side, side, 0, 0]);
        group.extend_from_slice(&1u16.to_le_bytes()); // planes
        group.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        group.extend_from_slice(&(image.len() as u32).to_le_bytes());
        group.extend_from_slice(&id.to_le_bytes());
    }
    entry(&mut res, 14, 1, &group); // RT_GROUP_ICON
    res
}

/// One resource: a header with a numeric type and name, then the data, each
/// padded to four bytes.
fn entry(res: &mut Vec<u8>, kind: u16, name: u16, data: &[u8]) {
    let mut header = Vec::new();
    header.extend_from_slice(&(data.len() as u32).to_le_bytes());
    header.extend_from_slice(&32u32.to_le_bytes()); // header size
    header.extend_from_slice(&0xFFFFu16.to_le_bytes());
    header.extend_from_slice(&kind.to_le_bytes());
    header.extend_from_slice(&0xFFFFu16.to_le_bytes());
    header.extend_from_slice(&name.to_le_bytes());
    header.extend_from_slice(&0u32.to_le_bytes()); // data version
    let flags: u16 = if kind == 0 { 0 } else { 0x1010 }; // moveable, discardable
    header.extend_from_slice(&flags.to_le_bytes());
    let lang: u16 = if kind == 0 { 0 } else { 0x0409 };
    header.extend_from_slice(&lang.to_le_bytes());
    header.extend_from_slice(&0u32.to_le_bytes()); // version
    header.extend_from_slice(&0u32.to_le_bytes()); // characteristics
    res.extend_from_slice(&header);
    res.extend_from_slice(data);
    while res.len() % 4 != 0 {
        res.push(0);
    }
}

/// An icon image as a 32-bit DIB: the header (height doubled for the mask),
/// BGRA rows bottom up, then an all-clear AND mask (alpha does the work).
fn dib(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&40u32.to_le_bytes());
    d.extend_from_slice(&(size as i32).to_le_bytes());
    d.extend_from_slice(&((size * 2) as i32).to_le_bytes());
    d.extend_from_slice(&1u16.to_le_bytes());
    d.extend_from_slice(&32u16.to_le_bytes());
    d.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    d.extend_from_slice(&(size * size * 4).to_le_bytes());
    d.extend_from_slice(&[0; 16]); // resolution, palette
    for y in (0..size).rev() {
        for x in 0..size {
            let p = ((y * size + x) * 4) as usize;
            d.extend_from_slice(&[rgba[p + 2], rgba[p + 1], rgba[p], rgba[p + 3]]);
        }
    }
    let mask_row = ((size + 31) / 32) * 4;
    d.extend(std::iter::repeat_n(0u8, (mask_row * size) as usize));
    d
}

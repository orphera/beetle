// The app icons, shared by the `beetle-app` and `bpm-gui` build scripts
// (`include!`d there, so there is one drawing). They are drawn here (no image
// file, like the skin) and linked as a compiled resource (`.res`, which
// `link.exe` takes as an input file). Icon group 1 is the app, which each app
// reads back at run time for its window; group 2 (the game only) is the
// `.bmsp` file icon the file association points at (`"<exe>",-2`).

/// What a binary carries: the app icon, and with `package_icon` the `.bmsp`
/// file icon too.
fn link_app_icon(bin: &str, package_icon: bool) {
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    let out = std::env::var("OUT_DIR").expect("OUT_DIR");
    let res = std::path::Path::new(&out).join("app_icon.res");
    let mut groups: Vec<(u16, fn(f32, f32) -> Option<[f32; 3]>)> = vec![(1, sample)];
    if package_icon {
        groups.push((2, sample_package));
    }
    std::fs::write(&res, icon_res(&[16, 24, 32, 48, 64], &groups)).expect("write app_icon.res");
    println!("cargo:rustc-link-arg-bin={bin}={}", res.display());
}

/// The theme's cyan and magenta (`beetle_render::theme`), and the note white.
const CYAN: [f32; 3] = [0.0, 229.0, 255.0];
const MAGENTA: [f32; 3] = [255.0, 45.0, 106.0];
const WHITE: [f32; 3] = [255.0, 255.0, 255.0];

/// An icon at `size` px from `sample`, RGBA rows top to bottom.
fn draw(size: u32, sample: fn(f32, f32) -> Option<[f32; 3]>) -> Vec<u8> {
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

/// The app icon at (u, v) in 0..1 icon units, `None` outside the shape: a
/// rounded square in a cyan-to-magenta diagonal, two falling note bars and
/// the judge line.
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

/// The `.bmsp` file icon at (u, v): a light page with a folded corner,
/// carrying the app's gradient panel with its note mark.
fn sample_package(u: f32, v: f32) -> Option<[f32; 3]> {
    const PAGE: [f32; 3] = [236.0, 240.0, 247.0];
    const EDGE: [f32; 3] = [140.0, 150.0, 170.0];
    const FOLD: [f32; 3] = [196.0, 204.0, 218.0];
    let (x0, y0, x1, y1, corner) = (0.14, 0.04, 0.86, 0.96, 0.24);
    if u < x0 || u > x1 || v < y0 || v > y1 {
        return None;
    }
    // The folded corner: cut above its diagonal, the flap below it.
    let (fx, fy) = (u - (x1 - corner), v - y0);
    if fx > 0.0 && fy < corner {
        if fx > fy + 0.01 {
            return None;
        }
        return Some(if fx > fy - 0.04 { EDGE } else { FOLD });
    }
    let edge = 0.035;
    if u < x0 + edge || u > x1 - edge || v < y0 + edge || v > y1 - edge {
        return Some(EDGE);
    }
    // The app's panel in the lower part of the page.
    let (px0, py0, px1, py1) = (0.24, 0.42, 0.76, 0.86);
    if u >= px0 && u <= px1 && v >= py0 && v <= py1 {
        let t = (((u - px0) / (px1 - px0) + (v - py0) / (py1 - py0)) / 2.0).clamp(0.0, 1.0);
        let inside = |a: f32, b: f32, c: f32, d: f32| u >= a && u <= c && v >= b && v <= d;
        let mark = inside(0.31, 0.50, 0.49, 0.58)
            || inside(0.51, 0.62, 0.69, 0.70)
            || inside(0.29, 0.76, 0.71, 0.80);
        return Some(if mark {
            WHITE
        } else {
            [
                CYAN[0] + (MAGENTA[0] - CYAN[0]) * t,
                CYAN[1] + (MAGENTA[1] - CYAN[1]) * t,
                CYAN[2] + (MAGENTA[2] - CYAN[2]) * t,
            ]
        });
    }
    Some(PAGE)
}

/// A `.res` file holding each icon group in `groups` (id, drawing) with an
/// image per size. Image ids run on across the groups.
fn icon_res(sizes: &[u32], groups: &[(u16, fn(f32, f32) -> Option<[f32; 3]>)]) -> Vec<u8> {
    let mut res = Vec::new();
    // Every .res starts with an empty entry.
    entry(&mut res, 0, 0, &[]);
    let mut next_id = 1u16;
    for &(group_id, sample) in groups {
        let mut group = Vec::new();
        group.extend_from_slice(&0u16.to_le_bytes()); // reserved
        group.extend_from_slice(&1u16.to_le_bytes()); // type: icon
        group.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
        for &size in sizes {
            let image = dib(size, &draw(size, sample));
            let id = next_id;
            next_id += 1;
            entry(&mut res, 3, id, &image); // RT_ICON
            let side = if size >= 256 { 0 } else { size as u8 };
            group.extend_from_slice(&[side, side, 0, 0]);
            group.extend_from_slice(&1u16.to_le_bytes()); // planes
            group.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
            group.extend_from_slice(&(image.len() as u32).to_le_bytes());
            group.extend_from_slice(&id.to_le_bytes());
        }
        entry(&mut res, 14, group_id, &group); // RT_GROUP_ICON
    }
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

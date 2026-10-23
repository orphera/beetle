//! Where one frame differs from a reference decode (see `mpeg_compare`):
//! luma PSNR of the frames around it, and the differing macroblocks.
//!
//! cargo run -p mpeg-video --release --example mpeg_frame_diff -- <video> <reference.yuv> <frame>
use mpeg_video::{ps, Decoder};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let file = std::fs::read(&a[0]).unwrap();
    let reference = std::fs::read(&a[1]).unwrap();
    let target: usize = a[2].parse().unwrap();
    let mut decoder = Decoder::new(ps::video_stream(&file));
    let (w, h) = decoder.size().unwrap();
    let (w, h) = (w as usize, h as usize);
    let frame_size = w * h + 2 * w.div_ceil(2) * h.div_ceil(2);
    let mut n = 0;
    while let Some(Ok(frame)) = decoder.next_frame() {
        if n + 3 >= target && n <= target + 3 {
            let ours = frame.to_yuv420();
            let theirs = &reference[n * frame_size..(n + 1) * frame_size];
            let bad: Vec<(usize, usize, i32)> = (0..h / 16)
                .flat_map(|my| (0..w / 16).map(move |mx| (mx, my)))
                .filter_map(|(mx, my)| {
                    let mut worst = 0;
                    for y in my * 16..my * 16 + 16 {
                        for x in mx * 16..mx * 16 + 16 {
                            worst = worst
                                .max((ours[y * w + x] as i32 - theirs[y * w + x] as i32).abs());
                        }
                    }
                    (worst > 3).then_some((mx, my, worst))
                })
                .collect();
            println!(
                "frame {n}: {} macroblocks differ, first {:?}",
                bad.len(),
                &bad[..bad.len().min(8)]
            );
            // Chroma, by 8x8 block (one macroblock each).
            let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
            for (plane, base) in [("Cb", w * h), ("Cr", w * h + cw * ch)] {
                let bad: Vec<(usize, usize, i32)> = (0..ch / 8)
                    .flat_map(|my| (0..cw / 8).map(move |mx| (mx, my)))
                    .filter_map(|(mx, my)| {
                        let mut worst = 0;
                        for y in my * 8..my * 8 + 8 {
                            for x in mx * 8..mx * 8 + 8 {
                                let i = base + y * cw + x;
                                worst = worst.max((ours[i] as i32 - theirs[i] as i32).abs());
                            }
                        }
                        (worst > 3).then_some((mx, my, worst))
                    })
                    .collect();
                if !bad.is_empty() {
                    println!(
                        "  {plane}: {} differ, first {:?}",
                        bad.len(),
                        &bad[..bad.len().min(8)]
                    );
                }
            }
            if n == target && !bad.is_empty() {
                let (mx, my, _) = bad[0];
                for y in my * 16..my * 16 + 4 {
                    let o: Vec<u8> = (mx * 16..mx * 16 + 8).map(|x| ours[y * w + x]).collect();
                    let t: Vec<u8> = (mx * 16..mx * 16 + 8).map(|x| theirs[y * w + x]).collect();
                    println!("  row {y}: ours {o:?} theirs {t:?}");
                }
            }
        }
        n += 1;
        if n > target + 3 {
            break;
        }
    }
}

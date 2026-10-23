//! Decodes a video and compares every frame with a reference decode:
//! `ffmpeg -i <video> -f rawvideo -pix_fmt yuv420p <reference.yuv>`.
//! Prints the PSNR (luma and chroma) of the worst frame and the average.
//!
//! cargo run -p mpeg-video --release --example mpeg_compare -- <video> <reference.yuv> [frames]
use mpeg_video::{ps, Decoder};

fn psnr(a: &[u8], b: &[u8]) -> f64 {
    let mse = a
        .iter()
        .zip(b)
        .map(|(&x, &y)| (x as f64 - y as f64).powi(2))
        .sum::<f64>()
        / a.len() as f64;
    if mse == 0.0 {
        99.0
    } else {
        10.0 * (255.0f64 * 255.0 / mse).log10()
    }
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let file = std::fs::read(&a[0]).unwrap();
    let reference = std::fs::read(&a[1]).unwrap();
    let limit: usize = a.get(2).map_or(usize::MAX, |n| n.parse().unwrap());
    let mut decoder = Decoder::new(ps::video_stream(&file));
    let (w, h) = decoder.size().expect("a sequence header");
    let frame_size = (w * h + 2 * w.div_ceil(2) * h.div_ceil(2)) as usize;
    let luma = (w * h) as usize;
    println!(
        "{w}x{h} at {:.3} fps, reference has {} frames",
        decoder.frame_rate().unwrap(),
        reference.len() / frame_size
    );
    let started = std::time::Instant::now();
    let (mut n, mut sum, mut worst, mut below) = (0usize, 0.0, (f64::MAX, 0usize, 0.0), 0usize);
    let mut decoding = std::time::Duration::ZERO;
    loop {
        let t = std::time::Instant::now();
        let Some(frame) = decoder.next_frame() else {
            break;
        };
        decoding += t.elapsed();
        let frame = match frame {
            Ok(f) => f,
            Err(e) => {
                println!("frame {n}: {e}");
                break;
            }
        };
        let ours = frame.to_yuv420();
        let Some(theirs) = reference.get(n * frame_size..(n + 1) * frame_size) else {
            println!("more frames than the reference ({n})");
            break;
        };
        let y = psnr(&ours[..luma], &theirs[..luma]);
        let c = psnr(&ours[luma..], &theirs[luma..]);
        sum += y;
        if y < 40.0 {
            below += 1;
            if std::env::var("MPEG_LIST").is_ok() {
                println!("  frame {n}: luma {y:.1} dB");
            }
        }
        if y < worst.0 {
            worst = (y, n, c);
        }
        n += 1;
        if n >= limit {
            break;
        }
    }
    println!(
        "{n} frames in {:.2} s (decoding {:.2} s); luma PSNR average {:.2} dB, worst {:.2} dB (frame {}, chroma {:.2} dB), {below} below 40 dB",
        started.elapsed().as_secs_f64(),
        decoding.as_secs_f64(),
        sum / n.max(1) as f64,
        worst.0,
        worst.1,
        worst.2
    );
}

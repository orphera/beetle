//! Decodes small ffmpeg-made clips (`testsrc`, so no one's footage) and
//! compares every frame with ffmpeg's own decode of them (`.yuv`,
//! `-pix_fmt yuv420p`). Decoders may round the IDCT differently, so frames
//! are compared by PSNR, not byte for byte.
//!
//! - `testsrc_mpeg1.mpg`: MPEG-1 program stream, B frames.
//! - `testsrc_mpeg2.mpg`: MPEG-2 program stream, B frames, field motion and
//!   field DCT, intra VLC table B.15, alternate scan.

use mpeg_video::{ps, Decoder};

fn psnr(a: &[u8], b: &[u8]) -> f64 {
    let mse = a
        .iter()
        .zip(b)
        .map(|(&x, &y)| (x as f64 - y as f64).powi(2))
        .sum::<f64>()
        / a.len() as f64;
    if mse == 0.0 {
        f64::INFINITY
    } else {
        10.0 * (255.0f64 * 255.0 / mse).log10()
    }
}

fn check(name: &str) {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/");
    let file = std::fs::read(format!("{dir}{name}.mpg")).unwrap();
    let reference = std::fs::read(format!("{dir}{name}.yuv")).unwrap();
    let mut decoder = Decoder::new(ps::video_stream(&file));
    let (w, h) = decoder.size().unwrap();
    assert_eq!((w, h), (96, 64));
    let size = (w * h * 3 / 2) as usize;
    let mut frames = 0;
    while let Some(frame) = decoder.next_frame() {
        let ours = frame.unwrap().to_yuv420();
        let theirs = &reference[frames * size..(frames + 1) * size];
        let p = psnr(&ours, theirs);
        assert!(p > 50.0, "{name} frame {frames}: {p:.1} dB");
        frames += 1;
    }
    assert_eq!(frames * size, reference.len(), "{name}: frame count");
}

#[test]
fn mpeg1_matches_ffmpeg() {
    check("testsrc_mpeg1");
}

#[test]
fn mpeg2_interlaced_matches_ffmpeg() {
    check("testsrc_mpeg2");
}

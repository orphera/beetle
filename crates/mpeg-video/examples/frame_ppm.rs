//! Research helper: writes frame <n> of an MPEG video as PPM.
//!
//! cargo run -p mpeg-video --release --example frame_ppm -- <video> <out.ppm> <n>
use mpeg_video::{ps, Decoder};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let file = std::fs::read(&a[0]).unwrap();
    let target: usize = a[2].parse().unwrap();
    let mut decoder = Decoder::new(ps::video_stream(&file));
    let (w, h) = decoder.size().unwrap();
    for n in 0.. {
        let Some(Ok(frame)) = decoder.next_frame() else {
            break;
        };
        if n == target {
            let mut out = format!("P6 {w} {h} 255\n").into_bytes();
            for p in frame.to_rgba() {
                out.extend(&p[..3]);
            }
            std::fs::write(&a[1], out).unwrap();
            break;
        }
    }
}

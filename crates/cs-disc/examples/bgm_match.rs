//! Temporary: how well BGM files line up with a song's chart (onsets of the
//! sounds the chart plays against the stream's loudness rises).
//! bgm_match <iso> <song id> <bgm file>...
use cs_disc::iidx::Disc;
use cs_disc::ply::Command;
use cs_disc::{identify, SectorReader, Volume};
const BIN: f64 = 0.02;
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut d = SectorReader::open_image(&a[0]).unwrap();
    let ed = identify(&mut d).unwrap().unwrap();
    let v = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &v, ed).unwrap();
    let id: u32 = a[1].parse().unwrap();
    let song = disc.songs.iter().find(|s| s.id == id).unwrap();
    let ply = disc.chart_ply(&mut d, &song.charts[0]).unwrap();
    let n = 20000;
    let mut onsets = vec![0f64; n];
    for e in &ply.events {
        if matches!(e.command, Command::Note { .. } | Command::Auto { .. }) {
            let b = (e.time as f64 * ply.tick_ms / 1000.0 / BIN) as usize;
            if b < n {
                onsets[b] += 1.0;
            }
        }
    }
    for path in &a[2..] {
        let pcm = cs_disc::wvb::decode_bgm(&std::fs::read(path).unwrap()).unwrap();
        let per = (pcm.rate as f64 * BIN) as usize;
        let env: Vec<f64> = pcm.channels[0]
            .chunks(per)
            .map(|c| c.iter().map(|&x| (x as f64).abs()).sum::<f64>() / per as f64)
            .collect();
        let mut flux = vec![0f64; n];
        for i in 1..env.len().min(n) {
            flux[i] = (env[i] - env[i - 1]).max(0.0);
        }
        let norm = |v: &[f64]| {
            let m = v.iter().sum::<f64>() / v.len() as f64;
            v.iter().map(|x| x - m).collect::<Vec<_>>()
        };
        let (o, f) = (norm(&onsets), norm(&flux));
        let (so, sf) = (
            o.iter().map(|x| x * x).sum::<f64>().sqrt(),
            f.iter().map(|x| x * x).sum::<f64>().sqrt(),
        );
        let mut best = (0.0, 0i32);
        for lag in -150i32..150 {
            let c: f64 = (0..n)
                .filter_map(|i| {
                    let j = i as i32 + lag;
                    (j >= 0 && (j as usize) < n).then(|| o[i] * f[j as usize])
                })
                .sum::<f64>()
                / (so * sf);
            if c > best.0 {
                best = (c, lag);
            }
        }
        println!(
            "{path}: {:.1} s, corr {:.3} at {} ms",
            pcm.channels[0].len() as f64 / pcm.rate as f64,
            best.0,
            best.1 as f64 * BIN * 1000.0
        );
    }
}

//! Research helper: for every song of a disc, where its BGM stream lines up
//! with its chart: the stream placed at chart time `lag` against when the
//! chart's sounds start, over -12 s to +12 s, next to what the chart's BGM
//! start (command 3) says. With a stream file per song (`files` argument
//! `<song>=<file>` pairs) it checks those instead of the chosen ones.
//!
//! cargo run -p cs-disc --release --example bgm_offset -- <image.iso> [<song>=<file> ...]
use cs_disc::iidx::Disc;
use cs_disc::ply::Command;
use cs_disc::{identify, SectorReader, Volume};

const BIN: f64 = 0.02;
const N: usize = 16000;

/// (correlation, lag in bins) of the chart's onsets against the stream's rises.
pub fn best_lag(onsets: &[f64], pcm: &cs_disc::wvb::Pcm, reach: i32) -> (f64, i32) {
    let per = (pcm.rate as f64 * BIN) as usize;
    let env: Vec<f64> = pcm.channels[0]
        .chunks(per)
        .map(|c| c.iter().map(|&x| (x as f64).abs()).sum::<f64>() / per as f64)
        .collect();
    let mut flux = vec![0f64; N];
    for i in 1..env.len().min(N) {
        flux[i] = (env[i] - env[i - 1]).max(0.0);
    }
    let norm = |v: &[f64]| {
        let m = v.iter().sum::<f64>() / v.len() as f64;
        v.iter().map(|x| x - m).collect::<Vec<_>>()
    };
    let (o, f) = (norm(onsets), norm(&flux));
    let size = |v: &[f64]| v.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-9);
    let (so, sf) = (size(&o), size(&f));
    let mut best = (0.0, 0);
    // The stream starts at chart time `lag`: its bin j is chart bin j + lag.
    for lag in -reach..reach {
        let c: f64 = (0..N)
            .filter_map(|j| {
                let i = j as i32 + lag;
                (i >= 0 && (i as usize) < N).then(|| o[i as usize] * f[j])
            })
            .sum::<f64>()
            / (so * sf);
        if c > best.0 {
            best = (c, lag);
        }
    }
    best
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut d = SectorReader::open_image(&a[0]).unwrap();
    let edition = identify(&mut d).unwrap().unwrap();
    let volume = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &volume, edition).unwrap();
    let given: Vec<(u32, u32)> = a[1..]
        .iter()
        .map(|p| {
            let (s, f) = p.split_once('=').unwrap();
            (s.parse().unwrap(), f.parse().unwrap())
        })
        .collect();
    println!("song\tfile\tcorr\tstream at ms\tcmd3 ms\tcmd3 value\ttitle");
    for song in &disc.songs {
        let chart = &song.charts[0];
        let file = match given.iter().find(|(s, _)| *s == song.id) {
            Some(&(_, f)) => Some(f),
            None if given.is_empty() => song.sound_files(chart.style, chart.difficulty).1,
            None => continue,
        };
        let Some(file) = file else { continue };
        let Ok(ply) = disc.chart_ply(&mut d, chart) else {
            continue;
        };
        let mut onsets = vec![0f64; N];
        for e in &ply.events {
            if let Command::Note { .. } | Command::Auto { .. } = e.command {
                if matches!(e.command, Command::Auto { sound, .. } if sound == edition.bgm_sound())
                {
                    continue;
                }
                let b = (ply.millis(e.time) / 1000.0 / BIN) as usize;
                if b < N {
                    onsets[b] += 1.0;
                }
            }
        }
        // The raw command 3 of the chart file (pop'n: 8-byte events).
        let raw = disc.read_chart(&mut d, chart.file).unwrap_or_default();
        let list = u32::from_le_bytes(raw[4..8].try_into().unwrap_or([0; 4])) as usize;
        let cmd3 = raw
            .get(list..)
            .unwrap_or_default()
            .chunks_exact(8)
            .find(|e| e[5] == 3)
            .map(|e| {
                (
                    u32::from_le_bytes(e[..4].try_into().unwrap()),
                    u16::from_le_bytes([e[6], e[7]]),
                )
            });
        let Ok(bytes) = disc.read_file(&mut d, file) else {
            continue;
        };
        let Ok(pcm) = cs_disc::wvb::decode_bgm(&bytes) else {
            continue;
        };
        let (c, lag) = best_lag(&onsets, &pcm, 600);
        println!(
            "{}\t{file}\t{c:.2}\t{:.0}\t{:?}\t{:?}\t{}",
            song.id,
            lag as f64 * BIN * 1000.0,
            cmd3.map(|c| c.0),
            cmd3.map(|c| c.1),
            song.title
        );
    }
}

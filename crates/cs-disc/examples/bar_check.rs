//! Research helper: a disc chart's bar lines, tempos and signatures as
//! stored, beside where the BMS layout (`ply_bms::Layout`) puts each bar line.
//!
//! cargo run -p cs-disc --release --example bar_check -- <image.iso> <title part> [chart index]
use cs_disc::iidx::Disc;
use cs_disc::ply::Command;
use cs_disc::ply_bms::Layout;
use cs_disc::{identify, SectorReader, Volume};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut d = SectorReader::open_image(&a[0]).unwrap();
    let edition = identify(&mut d).unwrap().unwrap();
    let volume = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &volume, edition).unwrap();
    let want = a[1].to_lowercase();
    let pick: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    if want == "*" {
        // Every chart: the furthest a stored line is from its fitted bar, and
        // how many notes are off the 1/12 and 1/16 beat grids.
        for song in &disc.songs {
            for chart in &song.charts {
                let Ok(ply) = disc.chart_ply(&mut d, chart) else {
                    continue;
                };
                let Ok(layout) = Layout::of(&ply) else {
                    continue;
                };
                let far = layout.line_offsets().fold(0.0f64, |m, o| m.max(o.abs()));
                let beats: Vec<f64> = layout.bar_beats().collect();
                let (mut notes, mut off) = (0, 0);
                for e in &ply.events {
                    if let Command::Note { .. } = e.command {
                        if e.time == 0 {
                            continue;
                        }
                        let (bar, f) = layout.position(ply.millis(e.time));
                        let beat = f * beats[bar];
                        notes += 1;
                        let on = |g: f64| ((beat * g).round() - beat * g).abs() < 1e-6;
                        if !on(12.0) && !on(16.0) {
                            off += 1;
                        }
                    }
                }
                let tempos = layout.tempo_changes().len();
                println!(
                    "{far:7.2} ms {off} {notes} {tempos} off  {} {}",
                    song.title,
                    chart.key(song)
                );
            }
        }
        return;
    }
    for song in &disc.songs {
        if !song.title.to_lowercase().contains(&want) {
            continue;
        }
        let Some(chart) = song.charts.get(pick) else {
            continue;
        };
        println!("== {} {}", song.title, chart.key(song));
        let ply = disc.chart_ply(&mut d, chart).unwrap();
        let layout = Layout::of(&ply).unwrap();
        println!("initial bpm {}", layout.initial_bpm());
        for (bar, f, bpm) in layout.tempo_changes() {
            println!("  tempo bar {bar} +{f:.4} -> {bpm:.4}");
        }
        let beats: Vec<f64> = layout.bar_beats().collect();
        let mut prev = 0.0;
        let mut n = 0;
        for e in &ply.events {
            let t = ply.millis(e.time);
            match e.command {
                Command::Measure => {
                    let (bar, f) = layout.position(t);
                    println!(
                        "line {n:3} t={t:9.2} d={:8.2} -> bar {bar} +{f:.4} (beats {})",
                        t - prev,
                        beats.get(bar).copied().unwrap_or(0.0)
                    );
                    prev = t;
                    n += 1;
                }
                Command::Bpm { bpm } => println!("   bpm {bpm} at {t:.2}"),
                Command::TimeSignature {
                    numerator,
                    denominator,
                } => println!("   sig {numerator}/{denominator} at {t:.2}"),
                Command::Note { lane, .. } if e.time > 0 => {
                    let (bar, f) = layout.position(t);
                    println!(
                        "      note lane {lane} t={t:9.2} +{:6.2}fr -> bar {bar} +{f:.4} ({:.3}/16)",
                        (t - prev) / (1000.0 / 59.94),
                        f * 16.0
                    );
                }
                _ => {}
            }
        }
    }
}

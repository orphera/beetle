//! Research helper: lists a GuitarFreaks & DrumMania disc's songs (from
//! `music_info.bin`), their files and chart tracks, and with an out
//! directory writes each song's banner (`IMG_CO{id}`) as `m{id}.tga`.
//!
//! cargo run -p cs-disc --release --example gfdm_songs -- <image.iso> [<out dir>]
use std::collections::HashMap;

use cs_disc::gfdm::{Disc, Game, Seq, EVENT_NOTE};
use cs_disc::ps2tex::Rgba;
use cs_disc::texp::{banner_name, Texp};
use cs_disc::Volume;

fn tga(p: &Rgba) -> Vec<u8> {
    let mut f = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    f.extend((p.width as u16).to_le_bytes());
    f.extend((p.height as u16).to_le_bytes());
    f.extend([32, 0x28]);
    for [r, g, b, a] in &p.pixels {
        f.extend([*b, *g, *r, *a]);
    }
    f
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut d = cs_disc::open(&args[0]).unwrap();
    let volume = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &volume)
        .unwrap()
        .expect("no PACKINFO.BIN");
    let songs = disc.songs(&mut d).unwrap();

    // Which texture file holds each banner.
    let mut banners: HashMap<String, usize> = HashMap::new();
    let mut files: HashMap<usize, Vec<u8>> = HashMap::new();
    if args.len() > 1 {
        for n in disc.texture_files(&mut d).unwrap() {
            let bytes = disc.read(&mut d, n).unwrap();
            if let Some(t) = Texp::parse(&bytes) {
                for r in &t.rects {
                    banners.entry(r.name.clone()).or_insert(n);
                }
            }
            files.insert(n, bytes);
        }
    }

    for song in &songs {
        let i = &song.info;
        let mut line = format!(
            "{:4} {:15} bpm {:3} gf {:?} dm {:?} len {:5}",
            i.id, i.name, i.bpm, i.gf_levels, i.dm_levels, i.length
        );
        for chart in [song.drum_chart, song.guitar_chart].into_iter().flatten() {
            let seq = Seq::parse(&disc.read(&mut d, chart).unwrap()).unwrap();
            for t in seq.tracks.iter().filter(|t| !t.is_meta) {
                let notes = t.events.iter().filter(|e| e.kind == EVENT_NOTE).count();
                let g = if t.game == Game::DrumMania {
                    "DM"
                } else {
                    "GF"
                };
                line += &format!(" {g}{}.{}={notes}", t.difficulty, t.part);
            }
        }
        let missing = [song.movie, song.drum_samples, song.guitar_samples]
            .iter()
            .filter(|f| f.is_none())
            .count();
        if missing > 0 {
            line += &format!(" ({missing} files missing)");
        }
        if let Some(out) = args.get(1) {
            let name = banner_name(i.id);
            let picture = banners
                .get(&name)
                .and_then(|n| Texp::parse(&files[n])?.image(&name));
            match picture {
                Some(p) => std::fs::write(format!("{out}/m{:04}.tga", i.id), tga(&p)).unwrap(),
                None => line += " (no banner)",
            }
        }
        println!("{line}");
    }
    println!("{} songs", songs.len());
}

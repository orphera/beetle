//! Dumps every Pop'n 8 BGM stream currently assigned by `Disc::sound_files`.
//!
//! Output WAV names include the SD_02 stream index, global disc file index,
//! and one matched title. `matches.tsv` records every matching song/chart.
//!
//! cargo run -p cs-disc --release --example popn8_bgm_dump -- <image.iso|E:> [out dir]
//! The default output directory is `scratch/popn8_bgm_slot`.

use cs_disc::iidx::Disc;
use cs_disc::{identify, open, Edition, Volume};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Default)]
struct SongMatch {
    title: String,
    charts: BTreeSet<String>,
}

fn clean_field(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if matches!(c, '\t' | '\r' | '\n') {
                ' '
            } else {
                c
            }
        })
        .collect()
}

fn safe_title(title: &str) -> String {
    let mut name = String::new();
    let mut separator = false;
    for c in title.chars() {
        if c.is_alphanumeric() {
            name.push(c);
            separator = false;
        } else if !separator && !name.is_empty() {
            name.push('_');
            separator = true;
        }
        if name.len() >= 80 {
            break;
        }
    }
    name.trim_matches('_').to_string()
}

fn wav(pcm: &cs_disc::wvb::Pcm) -> Vec<u8> {
    let channels = pcm.channels.len() as u32;
    let frames = pcm.channels.first().map_or(0, Vec::len);
    let data_bytes = (frames as u32) * channels * 2;
    let mut out = b"RIFF".to_vec();
    out.extend((36 + data_bytes).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend((channels as u16).to_le_bytes());
    out.extend(pcm.rate.to_le_bytes());
    out.extend((pcm.rate * channels * 2).to_le_bytes());
    out.extend(((channels * 2) as u16).to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend(data_bytes.to_le_bytes());
    for frame in 0..frames {
        for channel in &pcm.channels {
            out.extend(channel.get(frame).copied().unwrap_or(0).to_le_bytes());
        }
    }
    out
}

fn first_stream_file(disc: &Disc) -> io::Result<u32> {
    let mut file = 0;
    loop {
        match disc.file(file) {
            Some(entry) if entry.dat == 1 => return Ok(file),
            Some(_) => file += 1,
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "the disc has no SD_02 stream files",
                ));
            }
        }
    }
}

fn run(source: &str, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut disc = open(source)?;
    let edition = identify(&mut disc)?.ok_or("not a supported PS2 disc")?;
    if edition != Edition::Popn8 {
        return Err(format!("expected Pop'n music 8, found {}", edition.name()).into());
    }
    let volume = Volume::open(&mut disc)?;
    let songs = Disc::open(&mut disc, &volume, edition)?;
    let first_stream_file = first_stream_file(&songs)?;
    let mut matches: BTreeMap<u32, BTreeMap<u32, SongMatch>> = BTreeMap::new();

    for song in &songs.songs {
        for chart in &song.charts {
            let (_, Some(file)) = song.sound_files(chart.style, chart.difficulty) else {
                continue;
            };
            let entry = songs
                .file(file)
                .ok_or_else(|| format!("BGM file {file} is missing"))?;
            if entry.dat != 1 {
                return Err(format!("BGM file {file} is not in SD_02.DAT").into());
            }
            let stream_index = file
                .checked_sub(first_stream_file)
                .ok_or("BGM file precedes SD_02.DAT")?;
            let matched = matches
                .entry(stream_index)
                .or_default()
                .entry(song.id)
                .or_default();
            matched.title.clone_from(&song.title);
            matched
                .charts
                .insert(format!("{:?}_{:?}", chart.style, chart.difficulty));
        }
    }
    if matches.is_empty() {
        return Err("no BGM streams are assigned to Pop'n 8 charts".into());
    }

    std::fs::create_dir_all(output)?;
    let mut manifest = String::from(
        "stream_index\tdisc_file_index\ttitles\tchart_matches\trate_hz\tchannels\tduration_seconds\twav\n",
    );
    for (stream_index, songs_for_stream) in matches {
        let file = first_stream_file + stream_index;
        let bytes = songs.read_file(&mut disc, file)?;
        let pcm = cs_disc::wvb::decode_bgm(&bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        let first = songs_for_stream
            .values()
            .next()
            .ok_or("stream has no matched title")?;
        let stem = safe_title(&first.title);
        let suffix = if songs_for_stream.len() > 1 {
            format!("_plus{}", songs_for_stream.len() - 1)
        } else {
            String::new()
        };
        let filename = format!("stream_{stream_index:03}_file_{file:04}_{stem}{suffix}.wav");
        std::fs::write(output.join(&filename), wav(&pcm))?;

        let titles = songs_for_stream
            .values()
            .map(|matched| clean_field(&matched.title))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join("; ");
        let chart_matches = songs_for_stream
            .iter()
            .map(|(id, matched)| {
                format!(
                    "{:03}:{} [{}]",
                    id,
                    clean_field(&matched.title),
                    matched.charts.iter().cloned().collect::<Vec<_>>().join(",")
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        writeln!(
            manifest,
            "{stream_index}\t{file}\t{titles}\t{chart_matches}\t{}\t{}\t{:.3}\t{filename}",
            pcm.rate,
            pcm.channels.len(),
            pcm.channels
                .first()
                .map_or(0.0, |channel| { channel.len() as f64 / pcm.rate as f64 })
        )?;
        println!("stream {stream_index:03}: {filename}");
    }
    std::fs::write(output.join("matches.tsv"), manifest)?;
    println!("wrote Pop'n 8 BGM dump to {}", output.display());
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !(1..=2).contains(&args.len()) {
        return Err("usage: popn8_bgm_dump <image.iso|E:> [out dir]".into());
    }
    let output = args
        .get(1)
        .map_or_else(|| PathBuf::from("scratch/popn8_bgm_slot"), PathBuf::from);
    run(&args[0], &output)
}

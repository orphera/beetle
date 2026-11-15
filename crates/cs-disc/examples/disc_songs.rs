//! Lists the songs on a disc image or in an optical drive.
//!
//! ```text
//! cargo run -p cs-disc --example disc_songs -- <image.iso | E:>
//! cargo run -p cs-disc --example disc_songs          (every CD/DVD drive)
//! ```

use std::io;

use cs_disc::iidx::{Difficulty, Disc, Style};
use cs_disc::{drive, identify, Volume};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let targets: Vec<String> = if args.is_empty() {
        drive::optical_drives()
            .into_iter()
            .map(|d| format!("{d}:"))
            .collect()
    } else {
        args
    };
    if targets.is_empty() {
        eprintln!("no disc image given and no CD/DVD drive found");
    }
    for target in targets {
        if let Err(e) = list(&target) {
            eprintln!("{target}: {e}");
        }
    }
}

fn list(target: &str) -> io::Result<()> {
    let mut disc = cs_disc::open(target)?;
    let Some(game) = identify(&mut disc)? else {
        match cs_disc::identify_title(&mut disc)? {
            Some(title)
                if title.code == cs_disc::gfdm::EXE_PATH
                    || title.code == cs_disc::gf4dm3::EXE_PATH =>
            {
                return list_gfdm(target, &mut disc, title.code, title.name);
            }
            Some(title) => println!("{target}: {} ({}), not read yet", title.name, title.code),
            None => println!("{target}: not a disc this player reads"),
        }
        return Ok(());
    };
    let volume = Volume::open(&mut disc)?;
    let edition = game;
    let songs = Disc::open(&mut disc, &volume, edition)?;
    println!("{target}: {} ({} songs)", game.name(), songs.songs.len());
    for song in &songs.songs {
        let charts: Vec<String> = song
            .charts
            .iter()
            .map(|c| {
                let style = match c.style {
                    Style::Single => "SP",
                    Style::Double => "DP",
                };
                let diff = match c.difficulty {
                    Difficulty::Beginner => "B",
                    Difficulty::Normal => "N",
                    Difficulty::Hyper => "H",
                    Difficulty::Another => "A",
                    Difficulty::Black => "X",
                };
                format!("{style}{diff}{}", c.level)
            })
            .collect();
        println!("{:4}  {:40}  {}", song.id, song.title, charts.join(" "));
    }
    Ok(())
}

fn list_gfdm(
    target: &str,
    disc: &mut cs_disc::SectorReader<std::fs::File>,
    code: &str,
    name: &str,
) -> io::Result<()> {
    let volume = Volume::open(disc)?;
    if code == cs_disc::gfdm::EXE_PATH {
        let reader = cs_disc::gfdm::Disc::open(disc, &volume)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing GFDM index"))?;
        let songs = reader.songs(disc)?;
        print_gfdm_songs(
            target,
            name,
            code,
            songs.len(),
            songs.iter().map(|song| &song.info),
        );
    } else {
        let reader = cs_disc::gf4dm3::Disc::open(disc, &volume)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "not a GF4DM3 disc"))?;
        print_gfdm_songs(
            target,
            name,
            code,
            reader.songs.len(),
            reader.songs.iter().map(|song| &song.info),
        );
    }
    Ok(())
}

fn print_gfdm_songs<'a>(
    target: &str,
    name: &str,
    code: &str,
    count: usize,
    songs: impl Iterator<Item = &'a cs_disc::gfdm::MusicInfo>,
) {
    println!("{target}: {name} ({code}, {count} songs)");
    for song in songs {
        println!(
            "{:4}  {:40}  GF {:?}  OPEN {:?}  BASS {:?}  DM {:?}",
            song.id, song.name, song.gf_levels, song.open_levels, song.bass_levels, song.dm_levels,
        );
    }
}

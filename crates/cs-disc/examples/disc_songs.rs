//! Lists the songs on a disc image or in an optical drive.
//!
//! ```text
//! cargo run -p cs-disc --example disc_songs -- <image.iso | E:>
//! cargo run -p cs-disc --example disc_songs          (every CD/DVD drive)
//! ```

use std::fs::File;
use std::io;

use cs_disc::iidx16::{Difficulty, Disc, Style};
use cs_disc::{drive, identify, Game, SectorReader, Volume};

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

fn open(target: &str) -> io::Result<SectorReader<File>> {
    match drive::as_drive_letter(target) {
        Some(letter) => drive::open(letter),
        None => SectorReader::open_image(target),
    }
}

fn list(target: &str) -> io::Result<()> {
    let mut disc = open(target)?;
    let Some(game) = identify(&mut disc)? else {
        println!("{target}: not a disc this player reads");
        return Ok(());
    };
    let volume = Volume::open(&mut disc)?;
    let Game::Iidx16(edition) = game;
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

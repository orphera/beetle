//! Compares this crate's ADPCM decoding with vgmstream's for one song's key
//! sounds: writes each as a BMDX `.pcm` (the wrapper iidx-ps2tools feeds
//! vgmstream) and a WAV of our own decode, runs vgmstream, and reports the
//! largest sample difference.
//!
//! cargo run -p cs-disc --release --example sound_check -- <iso> <song title> <vgmstream_cli.exe> <work dir>
use cs_disc::iidx16::{Difficulty, Disc, Style};
use cs_disc::{identify, Game, SectorReader, Volume};
use std::process::Command;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (iso, title, vgm, work) = (&a[0], &a[1], &a[2], std::path::Path::new(&a[3]));
    std::fs::create_dir_all(work).unwrap();
    let mut d = SectorReader::open_image(iso).unwrap();
    let Some(Game::Iidx16(ed)) = identify(&mut d).unwrap() else {
        panic!("not IIDX 16")
    };
    let v = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &v, ed).unwrap();
    let song = disc.songs.iter().find(|s| &s.title == title).unwrap();
    let (wvb, _) = song.sound_files(Style::Single, Difficulty::Normal);
    let data = disc.read_file(&mut d, wvb.unwrap()).unwrap();
    let bank = cs_disc::wvb::Bank::parse(&data).unwrap();
    let (mut worst, mut checked) = (0i32, 0);
    for n in bank.numbers().take(400) {
        let ks = bank.sound(n).unwrap();
        let (rate, ours) = bank.decode_file(ks.file).unwrap();
        // BMDX wrapper of the raw file bytes.
        let (raw, scrambled) = bank.raw_file(ks.file).unwrap();
        let mut pcm = Vec::new();
        for w in [
            0x08640001u32,
            0,
            0x800,
            raw.len() as u32,
            0,
            0,
            rate,
            1,
            scrambled as u32,
            16,
            0,
            0,
        ] {
            pcm.extend(w.to_le_bytes());
        }
        pcm.resize(0x800, 0);
        pcm.extend(raw);
        let src = work.join(format!("{n:04}.pcm"));
        let out = work.join(format!("{n:04}.wav"));
        std::fs::write(&src, &pcm).unwrap();
        Command::new(vgm)
            .arg("-q")
            .arg("-o")
            .arg(&out)
            .arg(&src)
            .status()
            .unwrap();
        let wav = std::fs::read(&out).unwrap();
        let pos = wav.windows(4).position(|w| w == b"data").unwrap();
        let theirs: Vec<i16> = wav[pos + 8..]
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        let diff = ours
            .iter()
            .zip(&theirs)
            .map(|(a, b)| (*a as i32 - *b as i32).abs())
            .max()
            .unwrap_or(0);
        if diff > 0 || ours.len() != theirs.len() {
            println!(
                "{n:4}: ours {} theirs {} samples, max diff {diff}",
                ours.len(),
                theirs.len()
            );
        }
        worst = worst.max(diff);
        checked += 1;
    }
    println!("{checked} key sounds, worst sample difference {worst}");

    // The BGM stream is a BMDX file already.
    let (_, bgm) = song.sound_files(Style::Single, Difficulty::Normal);
    let data = disc.read_file(&mut d, bgm.unwrap()).unwrap();
    let ours = cs_disc::wvb::decode_bgm(&data).unwrap();
    let src = work.join("bgm.pcm");
    let out = work.join("bgm.wav");
    std::fs::write(&src, &data).unwrap();
    Command::new(vgm)
        .arg("-q")
        .arg("-o")
        .arg(&out)
        .arg(&src)
        .status()
        .unwrap();
    let wav = std::fs::read(&out).unwrap();
    let pos = wav.windows(4).position(|w| w == b"data").unwrap();
    let theirs: Vec<i16> = wav[pos + 8..]
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect();
    let channels = ours.channels.len();
    let diff = (0..ours.channels[0].len().min(theirs.len() / channels))
        .flat_map(|i| (0..channels).map(move |c| (i, c)))
        .map(|(i, c)| (ours.channels[c][i] as i32 - theirs[i * channels + c] as i32).abs())
        .max()
        .unwrap_or(0);
    println!(
        "BGM: {} Hz, {} channels, ours {} theirs {} frames, worst sample difference {diff}",
        ours.rate,
        channels,
        ours.channels[0].len(),
        theirs.len() / channels
    );
}

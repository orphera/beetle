//! Research helper: a GuitarFreaks & DrumMania song's sounds as WAV files.
//! For each song id: the movie's music streams (`m{id}_bgm{n}.wav`), the
//! previews, and each chart's first playable track rendered with its key
//! sounds (`m{id}_dm.wav`, `m{id}_gf.wav`). With no ids it checks every
//! song: stream count and length, and chart notes without a sound.
//!
//! cargo run -p cs-disc --release --example gfdm_audio -- <image.iso> <out dir> [<id>...]
use cs_disc::atrac3::Decoder;
use cs_disc::gfdm::{
    bank_samples, lane_notes, movie_audio, sample_banks, song_path, Disc, Game, Kits, Seq,
    SongFile, AUDIO_RATE, EVENT_NOTE, EXE_PATH, MOVIE_AUDIO_BLOCK, TICKS_PER_SECOND,
};
use cs_disc::Volume;

fn wav(channels: u16, rate: u32, pcm: &[i16]) -> Vec<u8> {
    let data = pcm.len() as u32 * 2;
    let mut f = b"RIFF".to_vec();
    f.extend((36 + data).to_le_bytes());
    f.extend(b"WAVEfmt ");
    f.extend(16u32.to_le_bytes());
    f.extend(1u16.to_le_bytes());
    f.extend(channels.to_le_bytes());
    f.extend(rate.to_le_bytes());
    f.extend((rate * channels as u32 * 2).to_le_bytes());
    f.extend((channels * 2).to_le_bytes());
    f.extend(16u16.to_le_bytes());
    f.extend(b"data");
    f.extend(data.to_le_bytes());
    for s in pcm {
        f.extend(s.to_le_bytes());
    }
    f
}

/// The first playable track of a chart, mixed at 44100 Hz mono.
fn render(
    kits: &Kits,
    id: u32,
    game: Game,
    chart: &[u8],
    samples: &[u8],
    lanes: &[u16],
) -> (Vec<i16>, usize) {
    let seq = Seq::parse(chart).unwrap();
    let Some(track) = seq.tracks.iter().find(|t| !t.is_meta) else {
        return (Vec::new(), 0);
    };
    let mut mix = vec![0f32; AUDIO_RATE as usize * 60 * 4];
    let mut missing = 0;
    for e in &track.events {
        if e.kind != EVENT_NOTE && e.kind != 1 {
            continue;
        }
        let Some(ks) = kits.event_sound(id, game, e, lanes) else {
            missing += 1;
            continue;
        };
        let Some(s) =
            bank_samples(samples, ks.bank).and_then(|b| b.get(ks.sound as usize).copied())
        else {
            missing += 1;
            continue;
        };
        let pcm = s.decode(samples);
        let step = s.rate() as f64 / AUDIO_RATE as f64;
        let start = (e.time as u64 * AUDIO_RATE as u64 / TICKS_PER_SECOND as u64) as usize;
        let gain = s.volume as f32 / 127.0;
        let mut t = 0f64;
        let mut at = start;
        while (t as usize) < pcm.len() && at < mix.len() {
            mix[at] += pcm[t as usize] as f32 * gain;
            t += step;
            at += 1;
        }
    }
    let end = mix.iter().rposition(|&v| v != 0.0).map_or(0, |p| p + 1);
    let pcm = mix[..end]
        .iter()
        .map(|&v| (v * 0.5).clamp(-32768.0, 32767.0) as i16)
        .collect();
    (pcm, missing)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut d = cs_disc::open(&args[0]).unwrap();
    let out = std::path::Path::new(&args[1]);
    let volume = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &volume)
        .unwrap()
        .expect("no PACKINFO.BIN");
    let exe = volume.read_file(&mut d, EXE_PATH).unwrap().expect("no exe");
    let kits = Kits::parse(&exe).expect("kit tables");
    let ids: Vec<u32> = args[2..].iter().map(|a| a.parse().unwrap()).collect();
    let read = |d: &mut _, id, f| disc.read_path(d, &song_path(id, f)).unwrap();

    if ids.is_empty() {
        for id in kits.ids().collect::<Vec<_>>() {
            let movie = read(&mut d, id, SongFile::Movie).unwrap_or_default();
            let streams: Vec<String> = movie_audio(&movie)
                .iter()
                .map(|(n, s)| {
                    let secs = s.len() / MOVIE_AUDIO_BLOCK * 1024 / AUDIO_RATE as usize;
                    format!("{n}:{secs}s")
                })
                .collect();
            let mut line = format!("{id:4} bgm {}", streams.join(" "));
            for (game, chart, bank) in [
                (Game::DrumMania, SongFile::DrumChart, SongFile::DrumSamples),
                (
                    Game::GuitarFreaks,
                    SongFile::GuitarChart,
                    SongFile::GuitarSamples,
                ),
            ] {
                let samples = read(&mut d, id, bank).unwrap_or_default();
                let lanes = lane_notes(&read(&mut d, id, SongFile::Fre).unwrap_or_default());
                let seq = Seq::parse(&read(&mut d, id, chart).unwrap()).unwrap();
                let mut missing = 0;
                let mut notes = 0;
                for t in seq.tracks.iter().filter(|t| !t.is_meta) {
                    for e in t
                        .events
                        .iter()
                        .filter(|e| e.kind == EVENT_NOTE || e.kind == 1)
                    {
                        notes += 1;
                        let ok = kits.event_sound(id, game, e, &lanes).is_some_and(|ks| {
                            bank_samples(&samples, ks.bank)
                                .is_some_and(|b| (ks.sound as usize) < b.len())
                        });
                        missing += !ok as usize;
                    }
                }
                line += &format!(
                    " {:?} banks {} notes {notes} silent {missing}",
                    game,
                    sample_banks(&samples).len()
                );
            }
            println!("{line}");
        }
        return;
    }

    for id in ids {
        let movie = read(&mut d, id, SongFile::Movie).unwrap_or_default();
        for (n, stream) in movie_audio(&movie) {
            let pcm = Decoder::decode_all(MOVIE_AUDIO_BLOCK, &stream);
            std::fs::write(
                out.join(format!("m{id}_bgm{n}.wav")),
                wav(2, AUDIO_RATE, &pcm),
            )
            .unwrap();
        }
        for (f, name) in [
            (SongFile::GuitarPreview, "gf"),
            (SongFile::DrumPreview, "dm"),
        ] {
            if let Some(at3) = read(&mut d, id, f) {
                let pcm = Decoder::decode_all(MOVIE_AUDIO_BLOCK, &at3);
                std::fs::write(
                    out.join(format!("m{id}_preview_{name}.wav")),
                    wav(2, AUDIO_RATE, &pcm),
                )
                .unwrap();
            }
        }
        for (game, chart, bank, name) in [
            (
                Game::DrumMania,
                SongFile::DrumChart,
                SongFile::DrumSamples,
                "dm",
            ),
            (
                Game::GuitarFreaks,
                SongFile::GuitarChart,
                SongFile::GuitarSamples,
                "gf",
            ),
        ] {
            let samples = read(&mut d, id, bank).unwrap();
            let chart = read(&mut d, id, chart).unwrap();
            let lanes = lane_notes(&read(&mut d, id, SongFile::Fre).unwrap_or_default());
            let (pcm, missing) = render(&kits, id, game, &chart, &samples, &lanes);
            println!(
                "{id} {name}: {} s, {missing} notes without a sound",
                pcm.len() / 44100
            );
            std::fs::write(
                out.join(format!("m{id}_{name}.wav")),
                wav(1, AUDIO_RATE, &pcm),
            )
            .unwrap();
        }
    }
}

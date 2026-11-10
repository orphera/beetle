//! Research helper: for every GuitarFreaks & DrumMania song, how late the
//! music streams run against the charts (`gfdm::MUSIC_LEAD`, spec 4.2).
//! Stream 3 (the mix without drums) is matched against the guitar chart
//! rendered with its key sounds, stream 5 (the mix with drums) against the
//! drum chart: onset envelopes (1 ms) cross-correlated over ±200 ms. Prints
//! per song the lag in ms (positive: the music is late) and the peak
//! correlation; `MUSIC_LEAD` is 1600 frames = 33.3 ms.
//!
//! cargo run -p cs-disc --release --example gfdm_lead -- <image.iso> [<id>...]
use cs_disc::atrac3::Decoder;
use cs_disc::gfdm::{
    bank_samples, lane_notes, movie_audio, song_path, Disc, Game, Kits, Seq, SongFile, EVENT_NOTE,
    EXE_PATH, MOVIE_AUDIO_BLOCK, MUSIC_RATE, TICKS_PER_SECOND,
};
use cs_disc::Volume;

/// Envelope steps a second (1 ms).
const STEPS: usize = 1000;
const REACH_MS: i64 = 200;

/// Every playable track of a chart file mixed into a 1 ms envelope (mean
/// absolute level per ms), at the samples' own rates.
fn rendered_envelope(
    kits: &Kits,
    id: u32,
    game: Game,
    chart: &[u8],
    samples: &[u8],
    lanes: &[u16],
) -> Vec<f64> {
    let seq = Seq::parse(chart).unwrap();
    // The first playable track, as the game's charts all share times.
    let Some(track) = seq.tracks.iter().find(|t| !t.is_meta) else {
        return Vec::new();
    };
    const RATE: usize = 48000;
    let mut mix = vec![0f32; RATE * 60 * 5];
    for e in &track.events {
        if e.kind != EVENT_NOTE && e.kind != 1 {
            continue;
        }
        let Some(ks) = kits.event_sound(id, game, e, lanes) else {
            continue;
        };
        let Some(s) =
            bank_samples(samples, ks.bank).and_then(|b| b.get(ks.sound as usize).copied())
        else {
            continue;
        };
        let pcm = s.decode(samples);
        let step = s.rate() as f64 / RATE as f64;
        let start = e.time as usize * RATE / TICKS_PER_SECOND as usize;
        let gain = s.volume as f32 / 127.0;
        let (mut t, mut at) = (0f64, start);
        while (t as usize) < pcm.len() && at < mix.len() {
            mix[at] += pcm[t as usize] as f32 * gain;
            t += step;
            at += 1;
        }
    }
    mix.chunks(RATE / STEPS)
        .map(|c| c.iter().map(|v| v.abs() as f64).sum::<f64>() / c.len() as f64)
        .collect()
}

/// A stereo 48 kHz stream as a 1 ms envelope, from its first frame.
fn stream_envelope(stream: &[u8]) -> Vec<f64> {
    let pcm = Decoder::decode_all(MOVIE_AUDIO_BLOCK, stream);
    let per = MUSIC_RATE as usize / STEPS * 2;
    pcm.chunks(per)
        .map(|c| c.iter().map(|&v| (v as f64).abs()).sum::<f64>() / c.len() as f64)
        .collect()
}

/// Rises of the log envelope, smoothed over 5 ms.
fn onsets(env: &[f64]) -> Vec<f64> {
    let log: Vec<f64> = env.iter().map(|v| (v + 1.0).ln()).collect();
    let mut rise = vec![0.0; log.len()];
    for i in 5..log.len() {
        rise[i] = (log[i] - log[i - 5]).max(0.0);
    }
    let mean = rise.iter().sum::<f64>() / rise.len().max(1) as f64;
    rise.iter().map(|v| v - mean).collect()
}

/// (lag in ms where `music` best matches `chart`, normalized peak).
fn best_lag(chart: &[f64], music: &[f64]) -> (i64, f64) {
    let n = chart.len().min(music.len());
    let r = REACH_MS as usize;
    if n < 4 * r {
        return (0, 0.0);
    }
    let energy = |v: &[f64]| v.iter().map(|x| x * x).sum::<f64>().sqrt();
    let a = &chart[r..n - r];
    let mut best = (0, f64::MIN);
    for lag in -REACH_MS..=REACH_MS {
        let b = &music[(r as i64 + lag) as usize..(n as i64 - r as i64 + lag) as usize];
        let c = a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>() / (energy(a) * energy(b));
        if c > best.1 {
            best = (lag, c);
        }
    }
    best
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut d = cs_disc::open(&args[0]).unwrap();
    let volume = Volume::open(&mut d).unwrap();
    let disc = Disc::open(&mut d, &volume)
        .unwrap()
        .expect("no PACKINFO.BIN");
    let exe = volume.read_file(&mut d, EXE_PATH).unwrap().expect("no exe");
    let kits = Kits::parse(&exe).expect("kit tables");
    let mut ids: Vec<u32> = args[1..].iter().map(|a| a.parse().unwrap()).collect();
    if ids.is_empty() {
        ids = kits.ids().collect();
    }
    let read = |d: &mut _, id, f| disc.read_path(d, &song_path(id, f)).unwrap();
    println!("song  stream3~gf (ms, corr)  stream5~dm (ms, corr)");
    for id in ids {
        let movie = read(&mut d, id, SongFile::Movie).unwrap_or_default();
        let streams = movie_audio(&movie);
        let lanes = lane_notes(&read(&mut d, id, SongFile::Fre).unwrap_or_default());
        let mut line = format!("{id:4}");
        for (stream, game, chart, bank) in [
            (
                3,
                Game::GuitarFreaks,
                SongFile::GuitarChart,
                SongFile::GuitarSamples,
            ),
            (
                5,
                Game::DrumMania,
                SongFile::DrumChart,
                SongFile::DrumSamples,
            ),
        ] {
            let Some((_, s)) = streams.iter().find(|(n, _)| *n == stream) else {
                line += "  (no stream)          ";
                continue;
            };
            let (Some(c), Some(b)) = (read(&mut d, id, chart), read(&mut d, id, bank)) else {
                line += "  (no chart)           ";
                continue;
            };
            let rendered = onsets(&rendered_envelope(&kits, id, game, &c, &b, &lanes));
            let music = onsets(&stream_envelope(s));
            let (lag, corr) = best_lag(&rendered, &music);
            line += &format!("  {lag:+5} ms {corr:5.3}        ");
        }
        println!("{line}");
    }
}

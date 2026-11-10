//! A GuitarFreaks & DrumMania song as BMS text, so the player can list and
//! play it like any chart (`gfdm`).
//!
//! - DrumMania plays as 6K (`#6K`): lanes HH, SD, BD, HT, LT, CY (the note
//!   value 0–5) on channels 11, 12, 13, 15, 18, 19.
//! - GuitarFreaks (guitar or bass) plays as 4K (`#4K`): the frets R, G, B
//!   (value bits 1, 2, 4) on channels 11, 12, 14. A chord is one note per
//!   fret; only the first carries the sound. The disc has no open picks, so
//!   4K's last lane stays empty. Wailing and the pick itself are not played.
//! - GuitarFreaks open (track part 3): the guitar chart with some notes
//!   marked open (value bit `0x10`); those go on 4K's last lane (15) alone.
//! - The other parts at the same difficulty play themselves (BGM channel),
//!   as in the game, and so do a track's own sounds without a lane (event
//!   kind 1). The song's music stream starts at 0.
//!
//! Times on the disc are absolute (1/300 s). Bars are the meta track's bar
//! lines; each bar's length comes from the tempo between them, so the BMS
//! timing lands on the disc's times.

use crate::gfdm::{Event, Game, Kits, MusicInfo, Seq, Track, TICKS_PER_SECOND};

/// Bar line, tempo (`a` = microseconds a beat) and note kinds in a track.
const EVENT_BAR: u8 = 0x50;
const EVENT_TEMPO: u8 = 0x10;
const EVENT_NOTE: u8 = 0x00;
const EVENT_SOUND: u8 = 0x01;
/// GF note value bit of an open pick (only in part 3 tracks). Its fret bits
/// are left from the guitar chart and do not count.
const OPEN: u8 = 0x10;
/// Places in a bar a time is rounded to (under 1 ms at 150 BPM).
const GRID: u64 = 960;

/// Which part a chart plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    Drums,
    Guitar,
    /// Guitar with open picks (track part 3).
    Open,
    Bass,
}

impl Part {
    /// The movie music stream to play under this part: the streams are the
    /// song in other mixes, by stream number. Drum parts show up in streams
    /// 4 and 5 and a guitar part in 3, and the long songs keep only 2, 3
    /// and 5, so 3 is the mix for drums (guitar in it, no drums), 5 the mix
    /// for guitar and bass (drums in it) and 2 the session mix
    /// (`docs/specs/gfdm_ps2_disc.md` 4.2). Not yet checked by ear.
    pub fn music_stream(self) -> u16 {
        match self {
            Part::Drums => 3,
            Part::Guitar | Part::Open | Part::Bass => 5,
        }
    }
}

/// One playable chart of a song.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChartRef {
    pub part: Part,
    /// 1 BASIC, 2 ADVANCED, 3 EXTREME.
    pub difficulty: u8,
}

impl ChartRef {
    /// `gfdm/{id}/dm{d}`, `gf{d}` or `bass{d}`.
    pub fn key(&self, id: u32) -> String {
        let part = match self.part {
            Part::Drums => "dm",
            Part::Guitar => "gf",
            Part::Open => "open",
            Part::Bass => "bass",
        };
        format!("gfdm/{id}/{part}{}", self.difficulty)
    }

    /// The song id and chart of a `key`.
    pub fn parse(key: &str) -> Option<(u32, Self)> {
        let rest = key.strip_prefix("gfdm/")?;
        let (id, chart) = rest.split_once('/')?;
        let (part, d) = if let Some(d) = chart.strip_prefix("open") {
            (Part::Open, d)
        } else if let Some(d) = chart.strip_prefix("bass") {
            (Part::Bass, d)
        } else if let Some(d) = chart.strip_prefix("dm") {
            (Part::Drums, d)
        } else {
            (Part::Guitar, chart.strip_prefix("gf")?)
        };
        let difficulty: u8 = d.parse().ok()?;
        (1..=3).contains(&difficulty).then_some(())?;
        Some((id.parse().ok()?, Self { part, difficulty }))
    }

    fn game(&self) -> Game {
        match self.part {
            Part::Drums => Game::DrumMania,
            _ => Game::GuitarFreaks,
        }
    }
}

/// The track of `part` at `difficulty`: DM tracks by difficulty, GF by
/// difficulty and part (1 guitar, 2 bass, 3 open).
fn track<'a>(drums: &'a Seq, guitar: &'a Seq, part: Part, difficulty: u8) -> Option<&'a Track> {
    let (seq, want) = match part {
        Part::Drums => (drums, 0),
        Part::Guitar => (guitar, 1),
        Part::Bass => (guitar, 2),
        Part::Open => (guitar, 3),
    };
    seq.tracks.iter().find(|t| {
        !t.is_meta
            && t.difficulty == difficulty
            && (part == Part::Drums || t.part == want)
            && t.events.iter().any(|e| e.kind == EVENT_NOTE)
    })
}

/// Every chart the song has.
pub fn charts(drums: &Seq, guitar: &Seq) -> Vec<ChartRef> {
    let mut out = Vec::new();
    for part in [Part::Drums, Part::Guitar, Part::Open, Part::Bass] {
        for difficulty in 1..=3 {
            if track(drums, guitar, part, difficulty).is_some() {
                out.push(ChartRef { part, difficulty });
            }
        }
    }
    out
}

/// A sound the BMS names (`#WAVxx`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sound {
    /// Sound `n` of the drum sample file's one bank.
    Drum(u16),
    /// Sound `sound` of the guitar sample file's bank at `bank`, held for
    /// `ticks` (the note's length, `Event::a >> 16`; 0 the whole sound).
    /// Guitar samples ring for a second or more, so a fast run played whole
    /// piles up into noise; the game stops each at its note's length.
    Guitar { bank: usize, sound: u16, ticks: u16 },
    /// The music stream (`gfdm::movie_audio`).
    Music,
    /// No sound: the other frets of a chord.
    Silent,
}

impl Sound {
    pub fn name(&self) -> String {
        match self {
            Sound::Drum(n) => format!("d_{n}.wav"),
            Sound::Guitar { bank, sound, ticks } => format!("g_{bank:x}_{sound}_{ticks}.wav"),
            Sound::Music => "music.wav".to_string(),
            Sound::Silent => "silent.wav".to_string(),
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        let stem = name.strip_suffix(".wav")?;
        match stem {
            "music" => return Some(Sound::Music),
            "silent" => return Some(Sound::Silent),
            _ => {}
        }
        if let Some(n) = stem.strip_prefix("d_") {
            return Some(Sound::Drum(n.parse().ok()?));
        }
        let mut parts = stem.strip_prefix("g_")?.split('_');
        let bank = usize::from_str_radix(parts.next()?, 16).ok()?;
        let sound = parts.next()?.parse().ok()?;
        let ticks = parts.next()?.parse().ok()?;
        parts
            .next()
            .is_none()
            .then_some(Sound::Guitar { bank, sound, ticks })
    }
}

/// What one song needs to become BMS.
pub struct Song<'a> {
    pub info: &'a MusicInfo,
    pub drums: &'a Seq,
    pub guitar: &'a Seq,
    pub kits: &'a Kits,
    /// The drum lanes' default notes (`gfdm::lane_notes`).
    pub lanes: &'a [u16],
}

/// Ticks to beats, by the meta track's tempo events.
struct Tempo {
    /// (tick, beats at that tick, microseconds a beat from there).
    segments: Vec<(u32, f64, u32)>,
}

impl Tempo {
    fn new(meta: &Track) -> Self {
        let mut segments: Vec<(u32, f64, u32)> = Vec::new();
        let mut tempos: Vec<(u32, u32)> = meta
            .events
            .iter()
            .filter(|e| e.kind == EVENT_TEMPO && e.a > 0)
            .map(|e| (e.time, e.a))
            .collect();
        tempos.sort_by_key(|t| t.0);
        if tempos.first().is_none_or(|t| t.0 > 0) {
            tempos.insert(0, (0, tempos.first().map_or(500_000, |t| t.1)));
        }
        for (time, us) in tempos {
            let beats = segments.last().map_or(0.0, |&(t, b, u)| {
                b + (time - t) as f64 * 1e6 / TICKS_PER_SECOND as f64 / u as f64
            });
            if segments.last().is_some_and(|s| s.0 == time) {
                segments.pop();
            }
            segments.push((time, beats, us));
        }
        Self { segments }
    }

    fn beats(&self, time: u32) -> f64 {
        let i = self.segments.partition_point(|s| s.0 <= time).max(1) - 1;
        let (t, b, us) = self.segments[i];
        b + (time as f64 - t as f64) * 1e6 / TICKS_PER_SECOND as f64 / us as f64
    }

    fn bpm(us: u32) -> f64 {
        60e6 / us as f64
    }
}

fn base36(n: usize) -> String {
    const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    format!("{}{}", DIGITS[n / 36 % 36] as char, DIGITS[n % 36] as char)
}

/// The chart as BMS text, `None` when the song has no such chart.
pub fn to_bms(song: &Song, chart: ChartRef) -> Option<String> {
    let id = song.info.id;
    let played = track(song.drums, song.guitar, chart.part, chart.difficulty)?;
    let own = match chart.part {
        Part::Drums => song.drums,
        _ => song.guitar,
    };
    let meta = own.tracks.iter().find(|t| t.is_meta)?;
    let tempo = Tempo::new(meta);

    // Bar starts in beats: the bar lines, then one past the last event.
    let mut bars: Vec<f64> = meta
        .events
        .iter()
        .filter(|e| e.kind == EVENT_BAR)
        .map(|e| tempo.beats(e.time))
        .collect();
    bars.sort_by(f64::total_cmp);
    bars.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    if bars.first().is_none_or(|&b| b > 1e-6) {
        bars.insert(0, 0.0);
    }
    let last_event = [song.drums, song.guitar]
        .iter()
        .flat_map(|s| s.tracks.iter())
        .flat_map(|t| t.events.iter())
        .map(|e| e.time)
        .max()
        .unwrap_or(0);
    let end = tempo.beats(last_event);
    while *bars.last()? <= end {
        let step = match bars.len() {
            n if n >= 2 => bars[n - 1] - bars[n - 2],
            _ => 4.0,
        };
        bars.push(bars.last()? + step.max(1.0));
    }
    let place = |time: u32| -> (usize, u64) {
        let beats = tempo.beats(time);
        let bar = bars.partition_point(|&b| b <= beats + 1e-9).max(1) - 1;
        let bar = bar.min(bars.len() - 2);
        let frac = (beats - bars[bar]) / (bars[bar + 1] - bars[bar]);
        let slot = ((frac * GRID as f64).round() as u64).min(GRID - 1);
        (bar, slot)
    };

    let mut wavs: Vec<Sound> = Vec::new();
    let mut wav_id = |s: Sound| -> String {
        let n = match wavs.iter().position(|&w| w == s) {
            Some(n) => n,
            None => {
                wavs.push(s);
                wavs.len() - 1
            }
        };
        base36(n + 1)
    };
    let sound_of = |game: Game, e: &Event| -> Option<Sound> {
        let ks = song.kits.event_sound(id, game, e, song.lanes)?;
        Some(match game {
            Game::DrumMania => Sound::Drum(ks.sound),
            Game::GuitarFreaks => Sound::Guitar {
                bank: ks.bank,
                sound: ks.sound,
                ticks: (e.a >> 16) as u16,
            },
        })
    };

    // (bar, slot, channel, value).
    let mut objects: Vec<(usize, u64, &'static str, String)> = Vec::new();
    objects.push((0, 0, "01", wav_id(Sound::Music)));
    let game = chart.game();
    for e in &played.events {
        let (bar, slot) = place(e.time);
        match e.kind {
            EVENT_NOTE => {
                let lanes: Vec<&'static str> = match game {
                    Game::DrumMania => {
                        let ch = ["11", "12", "13", "15", "18", "19"];
                        ch.get(e.value as usize).into_iter().copied().collect()
                    }
                    Game::GuitarFreaks if e.value & OPEN != 0 => vec!["15"],
                    Game::GuitarFreaks => [(1, "11"), (2, "12"), (4, "14")]
                        .iter()
                        .filter(|(bit, _)| e.value & bit != 0)
                        .map(|&(_, ch)| ch)
                        .collect(),
                };
                let sound = sound_of(game, e);
                for (k, ch) in lanes.into_iter().enumerate() {
                    let s = if k == 0 { sound } else { Some(Sound::Silent) };
                    let v = wav_id(s.unwrap_or(Sound::Silent));
                    objects.push((bar, slot, ch, v));
                }
            }
            EVENT_SOUND => {
                if let Some(s) = sound_of(game, e) {
                    objects.push((bar, slot, "01", wav_id(s)));
                }
            }
            _ => {}
        }
    }
    let others: &[Part] = match chart.part {
        Part::Drums => &[Part::Guitar],
        Part::Guitar | Part::Open => &[Part::Drums],
        Part::Bass => &[Part::Drums, Part::Guitar],
    };
    for &part in others {
        let other = ChartRef {
            part,
            difficulty: chart.difficulty,
        };
        let Some(t) = track(song.drums, song.guitar, part, chart.difficulty) else {
            continue;
        };
        for e in t
            .events
            .iter()
            .filter(|e| e.kind == EVENT_NOTE || e.kind == EVENT_SOUND)
        {
            if let Some(s) = sound_of(other.game(), e) {
                let (bar, slot) = place(e.time);
                objects.push((bar, slot, "01", wav_id(s)));
            }
        }
    }

    // Tempo changes after the first.
    let mut bpms: Vec<f64> = Vec::new();
    for &(time, _, us) in tempo.segments.iter().skip(1) {
        let bpm = Tempo::bpm(us);
        let n = match bpms.iter().position(|&b| (b - bpm).abs() < 1e-9) {
            Some(n) => n,
            None => {
                bpms.push(bpm);
                bpms.len() - 1
            }
        };
        let (bar, slot) = place(time);
        objects.push((bar, slot, "08", base36(n + 1)));
    }

    let level = match chart.part {
        Part::Drums => song.info.dm_levels,
        Part::Guitar => song.info.gf_levels,
        Part::Open => song.info.open_levels,
        Part::Bass => song.info.bass_levels,
    }[chart.difficulty as usize - 1];
    let (genre, mode) = match chart.part {
        Part::Drums => ("DrumMania", "#6K"),
        Part::Guitar => ("GuitarFreaks", "#4K"),
        Part::Open => ("GuitarFreaks (open)", "#4K"),
        Part::Bass => ("GuitarFreaks (bass)", "#4K"),
    };
    let mut out = String::new();
    out += "#PLAYER 1\n";
    out += &format!("#GENRE {genre}\n#TITLE {}\n", song.info.name);
    out += &format!("#BPM {:.6}\n", Tempo::bpm(tempo.segments[0].2));
    out += &format!(
        "#PLAYLEVEL {level}\n#DIFFICULTY {}\n{mode}\n",
        chart.difficulty
    );
    for (n, s) in wavs.iter().enumerate() {
        out += &format!("#WAV{} {}\n", base36(n + 1), s.name());
    }
    for (n, b) in bpms.iter().enumerate() {
        out += &format!("#BPM{} {b:.6}\n", base36(n + 1));
    }

    objects.sort_by(|a, b| (a.0, a.2, a.1).cmp(&(b.0, b.2, b.1)));
    for bar in 0..bars.len() - 1 {
        let length = (bars[bar + 1] - bars[bar]) / 4.0;
        if (length - 1.0).abs() > 1e-9 {
            out += &format!("#{bar:03}02:{length:.9}\n");
        }
    }
    let mut i = 0;
    while i < objects.len() {
        let (bar, _, ch, _) = objects[i];
        let mut j = i;
        while j < objects.len() && objects[j].0 == bar && objects[j].2 == ch {
            j += 1;
        }
        // Lines of this bar and channel; a taken place starts another line
        // (BGM sounds), or on a lane the later object is dropped.
        let mut lines: Vec<Vec<Option<&str>>> = Vec::new();
        for o in &objects[i..j] {
            let slot = o.1 as usize;
            let first = lines.is_empty();
            match lines.iter_mut().find(|l| l[slot].is_none()) {
                Some(line) => line[slot] = Some(o.3.as_str()),
                None if ch == "01" || first => {
                    let mut line = vec![None; GRID as usize];
                    line[slot] = Some(o.3.as_str());
                    lines.push(line);
                }
                None => {}
            }
        }
        for line in lines {
            let step = line
                .iter()
                .enumerate()
                .filter(|(_, v)| v.is_some())
                .fold(GRID as usize, |g, (k, _)| gcd(g, k));
            let body: String = line
                .iter()
                .step_by(step.max(1))
                .map(|v| v.unwrap_or("00"))
                .collect();
            out += &format!("#{bar:03}{ch}:{body}\n");
        }
        i = j;
    }
    Some(out)
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chart_keys_round_trip() {
        for part in [Part::Drums, Part::Guitar, Part::Open, Part::Bass] {
            let c = ChartRef {
                part,
                difficulty: 2,
            };
            assert_eq!(ChartRef::parse(&c.key(404)), Some((404, c)));
        }
        assert_eq!(ChartRef::parse("gfdm/404/dm4"), None);
        assert_eq!(ChartRef::parse("017/sp_normal"), None);
    }

    #[test]
    fn sound_names_round_trip() {
        for s in [
            Sound::Drum(12),
            Sound::Guitar {
                bank: 0x1c9e0,
                sound: 3,
                ticks: 49,
            },
            Sound::Music,
            Sound::Silent,
        ] {
            assert_eq!(Sound::parse(&s.name()), Some(s));
        }
    }

    fn event(time: u32, kind: u8, value: u8, a: u32) -> Event {
        Event {
            time,
            value,
            kind,
            a,
            b: 0x7f,
        }
    }

    #[test]
    fn a_drum_chart_places_notes_on_bars_by_tempo() {
        // 120 BPM: a beat is 150 ticks, a bar 600.
        let meta = Track {
            game: Game::DrumMania,
            is_meta: true,
            difficulty: 0,
            part: 0,
            events: vec![
                event(0, EVENT_TEMPO, 0, 500_000),
                event(0, EVENT_BAR, 0, 0),
                event(600, EVENT_BAR, 0, 0),
            ],
        };
        let play = Track {
            game: Game::DrumMania,
            is_meta: false,
            difficulty: 1,
            part: 0,
            events: vec![event(300, EVENT_NOTE, 2, 36), event(750, EVENT_NOTE, 5, 49)],
        };
        let drums = Seq {
            tracks: vec![meta, play],
        };
        let guitar = Seq { tracks: vec![] };
        let info = MusicInfo {
            id: 1,
            name: "TEST".into(),
            bpm: 120,
            gf_levels: [1, 2, 3],
            dm_levels: [4, 5, 6],
            bass_levels: [7, 8, 9],
            open_levels: [10, 11, 12],
            length: 900,
        };
        let kits = Kits::default();
        let song = Song {
            info: &info,
            drums: &drums,
            guitar: &guitar,
            kits: &kits,
            lanes: &[],
        };
        assert_eq!(charts(&drums, &guitar).len(), 1);
        let bms = to_bms(
            &song,
            ChartRef {
                part: Part::Drums,
                difficulty: 1,
            },
        )
        .unwrap();
        assert!(bms.contains("#6K"));
        assert!(bms.contains("#BPM 120.000000"));
        assert!(bms.contains("#PLAYLEVEL 4"));
        // Half way through bar 0 on BD, a quarter into bar 1 on CY.
        assert!(bms.contains("#00013:0002"), "{bms}");
        assert!(bms.contains("#00119:00020000"), "{bms}");
    }

    #[test]
    fn an_open_pick_plays_on_the_last_lane_alone() {
        let meta = Track {
            game: Game::GuitarFreaks,
            is_meta: true,
            difficulty: 0,
            part: 0,
            events: vec![
                event(0, EVENT_TEMPO, 0, 500_000),
                event(0, EVENT_BAR, 0, 0),
                event(600, EVENT_BAR, 0, 0),
            ],
        };
        // Bar 0: R+G at the start, then an open pick that keeps an R bit.
        let open = Track {
            game: Game::GuitarFreaks,
            is_meta: false,
            difficulty: 2,
            part: 3,
            events: vec![event(0, EVENT_NOTE, 3, 0), event(300, EVENT_NOTE, 0x11, 0)],
        };
        let guitar = Seq {
            tracks: vec![meta, open],
        };
        let drums = Seq { tracks: vec![] };
        let info = MusicInfo {
            id: 1,
            name: "TEST".into(),
            bpm: 120,
            gf_levels: [1, 2, 3],
            dm_levels: [4, 5, 6],
            bass_levels: [7, 8, 9],
            open_levels: [10, 11, 12],
            length: 900,
        };
        let kits = Kits::default();
        let song = Song {
            info: &info,
            drums: &drums,
            guitar: &guitar,
            kits: &kits,
            lanes: &[],
        };
        let chart = ChartRef {
            part: Part::Open,
            difficulty: 2,
        };
        assert_eq!(charts(&drums, &guitar), vec![chart]);
        assert_eq!(ChartRef::parse(&chart.key(1)), Some((1, chart)));
        let bms = to_bms(&song, chart).unwrap();
        assert!(bms.contains("#PLAYLEVEL 11"), "{bms}");
        // The chord on R and G at the start; the open pick half way, on
        // the last lane and not on R.
        assert!(
            bms.contains(
                "#00011:02
"
            ),
            "{bms}"
        );
        assert!(
            bms.contains(
                "#00012:02
"
            ),
            "{bms}"
        );
        assert!(
            bms.contains(
                "#00015:0002
"
            ),
            "{bms}"
        );
    }
}

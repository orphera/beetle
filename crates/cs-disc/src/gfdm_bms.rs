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
//! - An arranged chart (`gfdm/<id>/<part><d>@<N>k+...`, `gfdm_arrange`): the
//!   drum, guitar or bass notes on N lanes chosen again for a keyboard
//!   (`#4K`, `#6K` or `#8K`), at the same times with the same sounds. A note
//!   left over when more play at once than there are lanes plays as a sound.
//!   An open chart's open picks are notes like the others (placed by their
//!   pitch); they have no frets, so they never play as chords.
//!
//! Times on the disc are absolute (1/300 s). Bars are the meta track's bar
//! lines; each bar's length comes from the tempo between them, so the BMS
//! timing lands on the disc's times.

use crate::gfdm::{Event, Game, Kits, MusicInfo, Seq, Track, TICKS_PER_SECOND};
use crate::gfdm_arrange::{self, Arrange, PartKind};

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

    /// What kind of part an arrangement treats this as.
    pub fn arrange_kind(self) -> PartKind {
        match self {
            Part::Drums => PartKind::Drums,
            Part::Guitar | Part::Open | Part::Bass => PartKind::Strings,
        }
    }
}

/// Note channels of each lane, left to right, by lane count
/// (`docs/plans/2026-10-09-4k-6k-8k-modes-design.md`; 8K's first lane is
/// the scratch channel, played as a plain key).
fn lane_channels(lanes: u8) -> Option<&'static [&'static str]> {
    match lanes {
        4 => Some(&["11", "12", "14", "15"]),
        6 => Some(&["11", "12", "13", "15", "18", "19"]),
        8 => Some(&["16", "11", "12", "13", "14", "15", "18", "19"]),
        _ => None,
    }
}

/// One playable chart of a song.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChartRef {
    pub part: Part,
    /// 1 BASIC, 2 ADVANCED, 3 EXTREME.
    pub difficulty: u8,
    /// The keyboard arrangement (`gfdm_arrange`); `None` the original lanes.
    /// Kept as `Arrange::for_part` leaves it, so a chart has one key.
    pub arrange: Option<Arrange>,
}

impl ChartRef {
    /// An original chart.
    pub fn original(part: Part, difficulty: u8) -> Self {
        Self {
            part,
            difficulty,
            arrange: None,
        }
    }

    /// This chart arranged by `arrange`.
    pub fn arranged(self, arrange: Arrange) -> Self {
        Self {
            arrange: Some(arrange.for_part(self.part.arrange_kind())),
            ..self
        }
    }

    /// `gfdm/{id}/dm{d}`, `gf{d}`, `open{d}` or `bass{d}`, then `@` and the
    /// arrangement when there is one.
    pub fn key(&self, id: u32) -> String {
        let part = match self.part {
            Part::Drums => "dm",
            Part::Guitar => "gf",
            Part::Open => "open",
            Part::Bass => "bass",
        };
        let mut key = format!("gfdm/{id}/{part}{}", self.difficulty);
        if let Some(a) = self.arrange {
            key += "@";
            key += &a.key();
        }
        key
    }

    /// The song id and chart of a `key`.
    pub fn parse(key: &str) -> Option<(u32, Self)> {
        let rest = key.strip_prefix("gfdm/")?;
        let (id, chart) = rest.split_once('/')?;
        let (chart, arrange) = match chart.split_once('@') {
            Some((chart, a)) => (chart, Some(Arrange::parse(a)?)),
            None => (chart, None),
        };
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
        let chart = Self::original(part, difficulty);
        let chart = match arrange {
            Some(a) => chart.arranged(a),
            None => chart,
        };
        Some((id.parse().ok()?, chart))
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
                out.push(ChartRef::original(part, difficulty));
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
/// The lanes a note event takes in its own chart: the drum, or the frets
/// (an open pick takes lane 3 alone; the fret bits it keeps do not count).
fn origins(game: Game, e: &Event) -> Vec<u8> {
    match game {
        Game::DrumMania => (e.value < 6).then_some(e.value).into_iter().collect(),
        Game::GuitarFreaks if e.value & OPEN != 0 => vec![3],
        Game::GuitarFreaks => (0..3).filter(|b| e.value & (1 << b) != 0).collect(),
    }
}

/// A drum hit this close to a guitar or bass note is under it (`acc`).
const ACCENT_MS: i64 = 30;

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
    let arranged = match chart.arrange {
        Some(a) => Some((a, chart.part.arrange_kind(), lane_channels(a.lanes)?)),
        None => None,
    };
    // Arranged notes by place: (bar, slot, ms, notes with their sounds).
    let mut groups: Vec<(usize, u64, i64, Vec<(gfdm_arrange::Note, Sound)>)> = Vec::new();
    // The original lanes, as the original chart keeps them: a lane taken at
    // a place keeps its first note. `from` is another part's kind (`mix`).
    let mut add_note = |e: &Event, game: Game, from: Option<PartKind>, sound: Option<Sound>| {
        let (bar, slot) = place(e.time);
        let at = groups.iter().rposition(|g| {
            g.0 == bar && g.1 == slot && g.3.first().is_none_or(|(n, _)| n.from == from)
        });
        let at = match at {
            Some(at) => at,
            None => {
                let ms = e.time as i64 * 1000 / TICKS_PER_SECOND as i64;
                groups.push((bar, slot, ms, Vec::new()));
                groups.len() - 1
            }
        };
        for (k, origin) in origins(game, e).into_iter().enumerate() {
            let notes = &mut groups[at].3;
            if notes.iter().any(|(n, _)| n.origin == origin) {
                continue;
            }
            let s = if k == 0 { sound } else { Some(Sound::Silent) };
            let note = gfdm_arrange::Note {
                origin,
                key: e.a as u16,
                ticks: (e.a >> 16) as u16,
                from,
            };
            notes.push((note, s.unwrap_or(Sound::Silent)));
        }
    };
    for e in &played.events {
        let (bar, slot) = place(e.time);
        match e.kind {
            EVENT_NOTE if arranged.is_some() => add_note(e, game, None, sound_of(game, e)),
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
    // Other parts' notes that may come onto the lanes (`mix`): those with a
    // sound and a lane of their own; they play from there, not below.
    let borrows = |e: &Event, other: ChartRef| {
        e.kind == EVENT_NOTE
            && arranged.is_some_and(|(a, _, _)| a.mix > 0)
            && !origins(other.game(), e).is_empty()
            && sound_of(other.game(), e).is_some()
    };
    for &part in others {
        let other = ChartRef::original(part, chart.difficulty);
        let Some(t) = track(song.drums, song.guitar, part, chart.difficulty) else {
            continue;
        };
        for e in t.events.iter().filter(|e| borrows(e, other)) {
            add_note(
                e,
                other.game(),
                Some(part.arrange_kind()),
                sound_of(other.game(), e),
            );
        }
    }
    if let Some((a, kind, channels)) = arranged {
        groups.sort_by_key(|g| (g.0, g.1, g.3.first().map(|(n, _)| n.from.is_some())));
        // How hard the drums hit, by time, for a guitar or bass chart
        // (`acc`, `trig`): 2 a snare or cymbal, 1 a kick.
        let mut hits: Vec<(i64, u8)> = Vec::new();
        if kind == PartKind::Strings {
            if let Some(t) = track(song.drums, song.guitar, Part::Drums, chart.difficulty) {
                for e in t.events.iter().filter(|e| e.kind == EVENT_NOTE) {
                    let ms = e.time as i64 * 1000 / TICKS_PER_SECOND as i64;
                    for origin in origins(Game::DrumMania, e) {
                        match origin {
                            1 | 5 => hits.push((ms, 2)),
                            2 => hits.push((ms, 1)),
                            _ => {}
                        }
                    }
                }
                hits.sort();
            }
        }
        let accent_at = |ms: i64| -> u8 {
            let from = hits.partition_point(|h| h.0 < ms - ACCENT_MS);
            hits[from..]
                .iter()
                .take_while(|h| h.0 <= ms + ACCENT_MS)
                .map(|h| h.1)
                .max()
                .unwrap_or(0)
        };
        let input = |groups: &[(usize, u64, i64, Vec<(gfdm_arrange::Note, Sound)>)]| {
            groups
                .iter()
                .map(|g| gfdm_arrange::Group {
                    ms: g.2,
                    bar: g.0 as u32,
                    slot: g.1 as u16,
                    accent: accent_at(g.2),
                    notes: g.3.iter().map(|(n, _)| *n).collect(),
                })
                .collect::<Vec<gfdm_arrange::Group>>()
        };
        // The features for this chart (`auto` looks at its notes), then the
        // groups as `acc` makes them: a second note without a sound, or only
        // the first of several (the others had none).
        let first = input(&groups);
        let a = gfdm_arrange::resolve(&a, kind, &first);
        for (g, accent) in groups
            .iter_mut()
            .zip(gfdm_arrange::accents(&a, kind, &first))
        {
            match accent {
                gfdm_arrange::Accent::Keep => {}
                gfdm_arrange::Accent::Trim(keep) => {
                    // A sound among the dropped notes still plays below.
                    for (_, sound) in g.3.drain(keep.max(1)..) {
                        if sound != Sound::Silent {
                            objects.push((g.0, g.1, "01", wav_id(sound)));
                        }
                    }
                }
                gfdm_arrange::Accent::Double(note) => g.3.push((note, Sound::Silent)),
            }
        }
        let input = input(&groups);
        let lanes = gfdm_arrange::arrange(&a, kind, chart.difficulty, &input);
        for (g, lanes) in groups.iter().zip(lanes) {
            for ((_, sound), lane) in g.3.iter().zip(lanes) {
                let ch = match lane {
                    Some(l) => channels[l as usize],
                    // Thinned, not brought in or left over: its sound plays
                    // by itself.
                    None if *sound == Sound::Silent => continue,
                    None => "01",
                };
                objects.push((g.0, g.1, ch, wav_id(*sound)));
            }
        }
    }
    for &part in others {
        let other = ChartRef::original(part, chart.difficulty);
        let Some(t) = track(song.drums, song.guitar, part, chart.difficulty) else {
            continue;
        };
        for e in t
            .events
            .iter()
            .filter(|e| e.kind == EVENT_NOTE || e.kind == EVENT_SOUND)
            .filter(|e| !borrows(e, other))
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
    let mode = match chart.arrange.map(|a| a.lanes) {
        Some(4) => "#4K",
        Some(6) => "#6K",
        Some(8) => "#8K",
        _ => mode,
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
            let c = ChartRef::original(part, 2);
            assert_eq!(ChartRef::parse(&c.key(404)), Some((404, c)));
        }
        assert_eq!(ChartRef::parse("gfdm/404/dm4"), None);
        // Arranged: one spelling, the other part's features dropped.
        let (id, c) = ChartRef::parse("gfdm/404/gf3@6k+trill+home+pitch+r2+s7").unwrap();
        assert_eq!(c.key(id), "gfdm/404/gf3@6k+pitch+trill+r2+s7");
        assert_eq!(ChartRef::parse(&c.key(id)), Some((id, c)));
        assert_eq!(
            ChartRef::parse("gfdm/404/dm1@4k+pitch").map(|(i, c)| c.key(i)),
            Some("gfdm/404/dm1@4k".to_string())
        );
        assert_eq!(
            ChartRef::parse("gfdm/404/open2@6k+pitch+home").map(|(i, c)| c.key(i)),
            Some("gfdm/404/open2@6k+pitch".to_string())
        );
        assert_eq!(ChartRef::parse("gfdm/404/gf2@5k"), None);
        assert_eq!(ChartRef::parse("gfdm/404/gf2@"), None);
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
        let bms = to_bms(&song, ChartRef::original(Part::Drums, 1)).unwrap();
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
        let chart = ChartRef::original(Part::Open, 2);
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
        // Arranged: the two notes of the chord and the open pick as one
        // note, on six lanes.
        let arranged = chart.arranged(Arrange::parse("6k+pitch").unwrap());
        assert_eq!(arranged.key(1), "gfdm/1/open2@6k+pitch");
        let bms = to_bms(&song, arranged).unwrap();
        assert!(bms.contains("#6K\n"), "{bms}");
        assert_eq!(census(&bms).0, 3, "{bms}");
    }

    /// Notes on lanes, and every sound (by name, silent ones left out) on
    /// any channel, of BMS text.
    fn census(bms: &str) -> (usize, Vec<String>) {
        let mut names = std::collections::HashMap::new();
        let mut notes = 0;
        let mut sounds = Vec::new();
        for line in bms.lines() {
            if let Some(rest) = line.strip_prefix("#WAV") {
                let (id, name) = rest.split_once(' ').unwrap();
                names.insert(id.to_string(), name.to_string());
                continue;
            }
            let Some((head, body)) = line.split_once(':') else {
                continue;
            };
            let ch = &head[4..];
            if ch == "02" || ch == "08" {
                continue;
            }
            for k in (0..body.len()).step_by(2) {
                let v = &body[k..k + 2];
                if v == "00" {
                    continue;
                }
                if ch != "01" {
                    notes += 1;
                }
                if names[v] != "silent.wav" {
                    sounds.push(names[v].clone());
                }
            }
        }
        sounds.sort();
        (notes, sounds)
    }

    #[test]
    fn an_arranged_chart_keeps_its_notes_and_sounds() {
        let meta = Track {
            game: Game::GuitarFreaks,
            is_meta: true,
            difficulty: 0,
            part: 0,
            events: vec![
                event(0, EVENT_TEMPO, 0, 500_000),
                event(0, EVENT_BAR, 0, 0),
                event(600, EVENT_BAR, 0, 0),
                event(1200, EVENT_BAR, 0, 0),
            ],
        };
        // Sixteenths (37.5 ticks) on R with rising pitches, a G+B chord
        // every beat, an auto sound.
        let mut events: Vec<Event> = (0..32)
            .map(|i| event(i * 75 / 2, EVENT_NOTE, 1, 0x30 + (i % 7)))
            .collect();
        events.extend((0..8).map(|i| event(i * 150 + 10, EVENT_NOTE, 6, 0x40)));
        events.push(event(20, EVENT_SOUND, 0, 0x41));
        events.sort_by_key(|e| e.time);
        let play = Track {
            game: Game::GuitarFreaks,
            is_meta: false,
            difficulty: 3,
            part: 1,
            events,
        };
        let guitar = Seq {
            tracks: vec![meta, play],
        };
        // Drums: eighths (75 ticks) of hi-hat for four bars, a kick on each
        // beat; the guitar rests in the last two bars.
        let mut drum_events: Vec<Event> = (0..32)
            .map(|i| event(i * 75, EVENT_NOTE, 0, 0x50))
            .collect();
        drum_events.extend((0..16).map(|i| event(i * 150, EVENT_NOTE, 2, 0x51)));
        drum_events.sort_by_key(|e| e.time);
        let drums = Seq {
            tracks: vec![Track {
                game: Game::DrumMania,
                is_meta: false,
                difficulty: 3,
                part: 0,
                events: drum_events,
            }],
        };
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
        // Every key one sound, so the census tells the notes apart.
        let mut kits = Kits::default();
        kits.add_guitar_song(1, (0x30..0x48).map(|k| (k, k - 0x30)).collect());
        kits.add_drum_notes(1, vec![(0x50, 100), (0x51, 101)]);
        let song = Song {
            info: &info,
            drums: &drums,
            guitar: &guitar,
            kits: &kits,
            lanes: &[],
        };
        let original = ChartRef::original(Part::Guitar, 3);
        let plain = to_bms(&song, original).unwrap();
        let arranged = |key: &str| {
            let chart = original.arranged(Arrange::parse(key).unwrap());
            to_bms(&song, chart).unwrap()
        };
        for key in ["4k", "6k+pitch+trill+r3+s1", "8k+trill+bal"] {
            let bms = arranged(key);
            let mode = format!("#{}K\n", &key[..1]);
            assert!(bms.contains(&mode), "{key}: {bms}");
            assert_eq!(census(&bms), census(&plain), "{key}");
            assert_eq!(bms, arranged(key), "{key}");
        }
        assert_ne!(arranged("6k+r5+s1"), arranged("6k+r5+s2"));
        // Thinning, other parts brought in and outer lanes move notes between
        // the lanes and the auto sounds; every sound still plays once.
        let (notes, sounds) = census(&plain);
        for key in [
            "6k+d150",
            "6k+mix5",
            "8k+pitch+hand+vary+edge+mix9+s2",
            "4k+trill+d200+mix3",
        ] {
            let bms = arranged(key);
            let (n, snd) = census(&bms);
            assert_eq!(snd, sounds, "{key}");
            assert_eq!(bms, arranged(key), "{key}");
            if key.contains("+d") {
                assert!(n < notes, "{key}");
            } else {
                assert!(n > notes, "{key}: drums in the rest");
            }
        }
        // mix0 is no mix.
        assert_eq!(arranged("6k+mix0"), arranged("6k"));
        // Not on 4K's lanes when the arrangement is 6K.
        assert!(!arranged("6k").contains("#00014:"));
    }

    /// With `CS_DISC_GFDM` set to the GuitarFreaks & DrumMania Masterpiece
    /// Silver image: every drum, guitar, open and bass chart arranges to 4K, 6K
    /// and 8K, reads back as that mode, and keeps its notes and sounds
    /// (4K drums may leave notes over when five or six play at once); and
    /// with thinning, other parts and outer lanes on, keeps its sounds.
    #[test]
    fn every_disc_chart_arranges() {
        let Ok(image) = std::env::var("CS_DISC_GFDM") else {
            return;
        };
        let mut d = crate::open(&image).unwrap();
        let volume = crate::Volume::open(&mut d).unwrap();
        let disc = crate::gfdm::Disc::open(&mut d, &volume).unwrap().unwrap();
        let kits = crate::gfdm::read_kits(&mut d, &volume).unwrap().unwrap();
        let features = Arrange::parse("4k+pitch+home+stick+trill+bal+r2+s3").unwrap();
        let stage2 = Arrange::parse(
            "8k+pitch+home+stick+bal+hand+vary+split+jj+fig+motif+acc+trig+d100+run3+mix5+s7",
        )
        .unwrap();
        let (mut arranged, mut left_over) = (0, 0);
        for song in disc.songs(&mut d).unwrap() {
            let c = disc.song_charts(&mut d, &song.info).unwrap();
            let s = Song {
                info: &c.info,
                drums: &c.drums,
                guitar: &c.guitar,
                kits: &kits,
                lanes: &c.lanes,
            };
            for chart in charts(&c.drums, &c.guitar) {
                let plain = to_bms(&s, chart).unwrap();
                let (notes, sounds) = census(&plain);
                for lanes in crate::gfdm_arrange::LANE_COUNTS {
                    let a = chart.arranged(Arrange { lanes, ..features });
                    let key = a.key(c.info.id);
                    let bms = to_bms(&s, a).unwrap_or_else(|| panic!("{key}"));
                    let parsed = beetle_core::parse_bms(&bms).unwrap();
                    let mode = parsed.detect_play_mode();
                    let playable = mode.restricted_lanes().map(<[_]>::len);
                    assert_eq!(playable, Some(lanes as usize), "{key}");
                    let (n, snd) = census(&bms);
                    assert_eq!(parsed.notes.len(), n, "{key}: every note on a lane");
                    assert_eq!(snd, sounds, "{key}");
                    if lanes == 4 && chart.part == Part::Drums {
                        assert!(n <= notes, "{key}");
                        left_over += notes - n;
                    } else {
                        assert_eq!(n, notes, "{key}");
                    }
                    arranged += 1;
                }
                // Thinned, mixed, with outer lanes: the same sounds.
                {
                    let a = chart.arranged(stage2);
                    let key = a.key(c.info.id);
                    let bms = to_bms(&s, a).unwrap_or_else(|| panic!("{key}"));
                    let parsed = beetle_core::parse_bms(&bms).unwrap();
                    let (n, snd) = census(&bms);
                    assert_eq!(parsed.notes.len(), n, "{key}");
                    assert_eq!(snd, sounds, "{key}");
                    arranged += 1;
                }
                // The features chosen for the chart keep every sound.
                let auto = Arrange::parse("6k+auto").unwrap();
                {
                    let a = chart.arranged(auto);
                    let key = a.key(c.info.id);
                    assert!(key.ends_with("@6k+auto"), "{key}");
                    let bms = to_bms(&s, a).unwrap_or_else(|| panic!("{key}"));
                    let (n, snd) = census(&bms);
                    // `acc` adds and drops notes without a sound of their own.
                    let parsed = beetle_core::parse_bms(&bms).unwrap();
                    assert_eq!(parsed.notes.len(), n, "{key}");
                    assert_eq!(snd, sounds, "{key}");
                    arranged += 1;
                }
            }
        }
        println!("{arranged} arranged charts, {left_over} 4K drum notes left over");
        assert!(arranged > 3600);
    }
}

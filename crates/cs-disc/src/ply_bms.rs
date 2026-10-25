//! A `.ply` chart written out as BMS text, so the player reads a disc chart
//! with its own BMS parser.
//!
//! The output is a pure function of the chart and the header given: the same
//! disc always gives the same bytes, so the chart's identity (a hash of the
//! text) is stable.
//!
//! Timing. Event times are whole frames (1/59.94 s), and a bar line event
//! marks every bar. The stored tempo is a whole number for display, so the
//! frames, not the tempo, decide when things happen:
//! - Each run of bars at one stored tempo gets the tempo and start of a
//!   least-squares line through its bar lines, which removes the drift a
//!   rounded tempo would build up without trusting any one rounded bar line. A bar with a tempo change inside it keeps
//!   the stored tempos' ratio, scaled to the bar's own length.
//! - An event's beat inside its bar is snapped to the coarsest musical grid
//!   (1/1 … 1/48 beat) that is within half a frame of it, which undoes the
//!   frame rounding.
//!
//! Notes at time 0 are not played: they are the chart's note count
//! (`ply::Command::Note`), not notes.

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::ply::{Command, Ply, Side};

/// Half of one frame at 59.94 Hz, in milliseconds: the rounding of a time
/// (the game rounds to the nearest frame; measured on both discs).
const HALF_FRAME_MS: f64 = 1000.0 / 59.94 / 2.0;
/// Grids tried when snapping, as divisions of a beat, coarsest first.
const BEAT_GRIDS: [u64; 10] = [1, 2, 3, 4, 6, 8, 12, 16, 24, 48];
/// The same for a chart timed in milliseconds (pop'n), whose notes may sit
/// between the frame chart grids.
const FINE_GRIDS: [u64; 14] = [1, 2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64, 96, 192];
/// How far a millisecond chart's times may be from the grid: the rounding,
/// with room for a tempo that is itself rounded.
const HALF_MS: f64 = 1.0;

/// How far an event of `ply` may be from where it belongs: half a frame for
/// the frame-timed IIDX charts, about a millisecond for pop'n.
fn rounding(ply: &Ply) -> f64 {
    if ply.tick_ms < 2.0 {
        HALF_MS
    } else {
        HALF_FRAME_MS
    }
}

/// What the BMS header says beyond the chart itself.
#[derive(Debug, Clone)]
pub struct Header {
    pub title: String,
    /// Written only when not empty.
    pub subtitle: String,
    pub artist: String,
    pub genre: String,
    pub level: u8,
    /// `#DIFFICULTY`: 1 BEGINNER … 5 INSANE.
    pub difficulty: u8,
    /// Double play: both sides' notes, `#PLAYER 3`.
    pub double: bool,
    /// pop'n music's nine buttons (P1 lanes 0–8) on the PMS channels.
    pub nine_buttons: bool,
    /// The file name `#WAVxx` gives sound `n` (1-based as the chart numbers them).
    pub sound_name: fn(u16) -> String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BmsError {
    NoBarLines,
    TooManyBars,
    TooManySounds,
    TooManyTempos,
}

impl std::fmt::Display for BmsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            BmsError::NoBarLines => "chart has no bar lines",
            BmsError::TooManyBars => "chart has more than 1000 bars",
            BmsError::TooManySounds => "chart uses a sound past ZZ",
            BmsError::TooManyTempos => "chart has more than 1295 tempos",
        })
    }
}

/// One stretch of a bar at one tempo.
#[derive(Debug, Clone, Copy)]
struct Segment {
    start_ms: f64,
    bpm: f64,
}

#[derive(Debug, Clone)]
struct Bar {
    start_ms: f64,
    /// Beats in the bar (`4 * numerator / denominator`) as a fraction.
    beats_num: u64,
    beats_den: u64,
    /// Tempo stretches, the first starting at the bar.
    segments: Vec<Segment>,
    /// How far an event may be from the grid (`rounding`).
    rounding: f64,
}

impl Bar {
    fn beats(&self) -> f64 {
        self.beats_num as f64 / self.beats_den as f64
    }

    /// Beats from the bar's start to `t`, and the tempo at `t`.
    fn beat_at(&self, t: f64) -> (f64, f64) {
        let mut beats = 0.0;
        for (i, seg) in self.segments.iter().enumerate() {
            let end = self
                .segments
                .get(i + 1)
                .map_or(f64::INFINITY, |s| s.start_ms);
            if t < end {
                return (beats + (t - seg.start_ms) * seg.bpm / 60000.0, seg.bpm);
            }
            beats += (end - seg.start_ms) * seg.bpm / 60000.0;
        }
        unreachable!("the last segment has no end")
    }
}

/// A position in the chart: bar, and the beat inside it as `n / d`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Pos {
    bar: usize,
    n: u64,
    d: u64,
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

fn lcm(a: u64, b: u64) -> u64 {
    a / gcd(a, b) * b
}

/// BMS base-36 for 1..=1295.
fn base36(n: usize) -> Option<String> {
    const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    (1..36 * 36)
        .contains(&n)
        .then(|| format!("{}{}", DIGITS[n / 36] as char, DIGITS[n % 36] as char))
}

/// IIDX lanes as BMS channels: keys 1–5 → x1–x5, 6–7 → x8–x9, scratch → x6.
/// pop'n buttons as PMS channels: 1–5 → 11–15, 6–9 → 22–25.
fn channel(side: Side, lane: u8, nine_buttons: bool) -> Option<u32> {
    if nine_buttons {
        return match (side, lane) {
            (Side::P1, 0..=4) => Some(11 + lane as u32),
            (Side::P1, 5..=8) => Some(17 + lane as u32),
            _ => None,
        };
    }
    let base = match side {
        Side::P1 => 10,
        Side::P2 => 20,
    };
    Some(
        base + match lane {
            0..=4 => lane as u32 + 1,
            5 => 8,
            6 => 9,
            7 => 6,
            _ => return None,
        },
    )
}

/// The bars, from the bar line events and the tempo and time signatures.
fn bars(ply: &Ply) -> Result<Vec<Bar>, BmsError> {
    let mut lines = Vec::new();
    let mut tempos: Vec<(f64, f64)> = Vec::new();
    let mut signature = (4u64, 4u64);
    let mut signatures = Vec::new();
    for e in &ply.events {
        let t = ply.millis(e.time);
        match e.command {
            // Double play charts have a bar line for each side.
            Command::Measure if lines.last() == Some(&t) => {}
            Command::Measure => {
                lines.push(t);
                signatures.push(signature);
            }
            Command::TimeSignature {
                numerator,
                denominator,
            } if numerator > 0 && denominator > 0 => {
                signature = (numerator as u64, denominator as u64);
                // A signature at a bar line's time belongs to that bar.
                if lines.last() == Some(&t) {
                    *signatures.last_mut().unwrap() = signature;
                }
            }
            Command::Bpm { bpm } if bpm > 0.0 => tempos.push((t, bpm)),
            _ => {}
        }
    }
    if lines.is_empty() {
        return Err(BmsError::NoBarLines);
    }
    if lines.len() > 1000 {
        return Err(BmsError::TooManyBars);
    }
    // PS2 charts commonly repeat the current BPM at every bar boundary.
    // Those commands are not tempo changes. Keeping them as separate tempo
    // cuts makes the region fitter derive a slightly different BPM from each
    // rounded 59.94 Hz bar interval, producing false 1–2 BPM ranges.
    let mut compact_tempos = Vec::with_capacity(tempos.len());
    for (at, bpm) in tempos {
        if compact_tempos
            .last()
            .is_some_and(|&(_, previous): &(f64, f64)| (previous - bpm).abs() < 1e-9)
        {
            continue;
        }
        compact_tempos.push((at, bpm));
    }
    // A change rounded to a millisecond before or after its bar line (pop'n
    // ゴエモン: 190 BPM at 68786, the line at 68787) starts that bar.
    let tempos: Vec<(f64, f64)> = compact_tempos
        .into_iter()
        .map(|(t, bpm)| {
            let line = lines.iter().find(|&&l| (l - t).abs() < rounding(ply));
            (line.copied().unwrap_or(t), bpm)
        })
        .collect();
    let stored_at = |t: f64| {
        tempos
            .iter()
            .rev()
            .find(|&&(at, _)| at <= t)
            .or(tempos.first())
            .map_or(120.0, |&(_, b)| b)
    };

    let mut bars: Vec<Bar> = lines
        .iter()
        .zip(&signatures)
        .map(|(&start_ms, &(num, den))| {
            let g = gcd(4 * num, den);
            Bar {
                start_ms,
                beats_num: 4 * num / g,
                beats_den: den / g,
                segments: vec![Segment {
                    start_ms,
                    bpm: stored_at(start_ms),
                }],
                rounding: rounding(ply),
            }
        })
        .collect();
    // Some 11 and 12 charts change meter by bar lines alone, under one 4/4
    // signature (GOLDEN HORN's 7/4, 100% minimoo-G's 3 to 9 beats). A bar
    // whose lines are far from its signature at the stored tempo takes the
    // length they measure, in quarter beats.
    for (i, bar) in bars.iter_mut().enumerate() {
        let Some(&end) = lines.get(i + 1) else {
            continue;
        };
        if tempos.iter().any(|&(t, _)| t > lines[i] && t < end) {
            continue;
        }
        let measured = (end - lines[i]) * bar.segments[0].bpm / 60000.0;
        let quarters = (measured * 4.0).round() as u64;
        if (measured - bar.beats()).abs() > 0.2 + 0.03 * bar.beats() && quarters > 0 {
            let g = gcd(quarters, 4);
            bar.beats_num = quarters / g;
            bar.beats_den = 4 / g;
        }
    }

    // Regions: runs of whole bars at one stored tempo, or one bar with a
    // tempo change inside it.
    let changes_inside = |i: usize| match lines.get(i + 1) {
        Some(&end) => tempos.iter().any(|&(t, _)| t > lines[i] && t < end),
        None => false,
    };
    let mut regions: Vec<(usize, usize, bool)> = Vec::new();
    let mut i = 0;
    while i < bars.len() {
        if changes_inside(i) {
            regions.push((i, i, true));
            i += 1;
            continue;
        }
        let stored = bars[i].segments[0].bpm;
        let mut j = i;
        while j + 1 < bars.len()
            && !changes_inside(j + 1)
            && bars[j + 1].segments[0].bpm == stored
            && !tempos.iter().any(|&(t, _)| t == lines[j + 1])
        {
            j += 1;
        }
        regions.push((i, j, false));
        i = j + 1;
    }

    // Where each region starts, and its milliseconds per beat when it could be
    // measured: a least-squares line through its bar lines (and the line that
    // closes it) against beats, so no single rounded bar line decides.
    let fits: Vec<(f64, Option<f64>)> = regions
        .iter()
        .map(|&(first, last, inside)| {
            if inside {
                return (lines[first], None);
            }
            let mut points = Vec::new();
            let mut beats = 0.0;
            for k in first..=last {
                points.push((beats, lines[k]));
                beats += bars[k].beats();
            }
            if let Some(&end) = lines.get(last + 1) {
                points.push((beats, end));
            }
            if points.len() < 2 {
                return (lines[first], None);
            }
            let n = points.len() as f64;
            let mx = points.iter().map(|p| p.0).sum::<f64>() / n;
            let my = points.iter().map(|p| p.1).sum::<f64>() / n;
            let sxx: f64 = points.iter().map(|p| (p.0 - mx).powi(2)).sum();
            let sxy: f64 = points.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
            let slope = sxy / sxx;
            (my - slope * mx, Some(slope))
        })
        .collect();

    for (r, &(first, last, inside)) in regions.iter().enumerate() {
        let start = fits[r].0;
        let next_start = fits.get(r + 1).map(|f| f.0);
        let beats: f64 = bars[first..=last].iter().map(Bar::beats).sum();
        if inside {
            // Stored tempos' ratio, scaled to the bar's length.
            let end = next_start.expect("a bar with a change inside has an end");
            let mut segs = vec![Segment {
                start_ms: start,
                bpm: bars[first].segments[0].bpm,
            }];
            for &(t, b) in tempos.iter().filter(|&&(t, _)| t > lines[first] && t < end) {
                segs.push(Segment {
                    start_ms: t,
                    bpm: b,
                });
            }
            let stored_beats: f64 = segs
                .iter()
                .enumerate()
                .map(|(k, s)| {
                    let seg_end = segs.get(k + 1).map_or(end, |n| n.start_ms);
                    (seg_end - s.start_ms) * s.bpm / 60000.0
                })
                .sum();
            // Where each change falls in the bar by the stored tempos' ratio,
            // snapped to the beat grid; each stretch then gets the tempo that
            // fills its own frames, so a snapped change moves no time.
            let scale = beats / stored_beats;
            let mut at_beat = 0.0;
            let mut cuts: Vec<(f64, f64)> = vec![(0.0, start)];
            for k in 1..segs.len() {
                at_beat +=
                    (segs[k].start_ms - segs[k - 1].start_ms) * segs[k - 1].bpm * scale / 60000.0;
                let (n, d) = snap_beat(at_beat, segs[k].bpm * scale, rounding(ply));
                let beat = n as f64 / d as f64;
                if beat > cuts.last().unwrap().0 && beat < beats {
                    cuts.push((beat, segs[k].start_ms));
                }
            }
            cuts.push((beats, end));
            bars[first].start_ms = start;
            bars[first].segments = cuts
                .windows(2)
                .map(|w| Segment {
                    start_ms: w[0].1,
                    bpm: (w[1].0 - w[0].0) * 60000.0 / (w[1].1 - w[0].1),
                })
                .collect();
            continue;
        }
        let bpm = match (next_start, fits[r].1) {
            (Some(next), _) if next > start => beats * 60000.0 / (next - start),
            (_, Some(ms_per_beat)) if ms_per_beat > 0.0 => 60000.0 / ms_per_beat,
            _ => bars[first].segments[0].bpm,
        };
        let mut at = start;
        for bar in &mut bars[first..=last] {
            bar.start_ms = at;
            bar.segments = vec![Segment { start_ms: at, bpm }];
            at += bar.beats() * 60000.0 / bpm;
        }
    }
    // Events after the last bar line's bar get bars of its length and tempo.
    let last_event = ply.events.last().map_or(0.0, |e| ply.millis(e.time));
    loop {
        let last = bars.last().unwrap();
        let seg = last.segments.last().unwrap();
        let (beat, bpm) = last.beat_at(last_event.max(last.start_ms));
        if beat + last.rounding * bpm / 60000.0 < last.beats() {
            break;
        }
        let end = seg.start_ms + (last.beats() - last.beat_at(seg.start_ms).0) * 60000.0 / seg.bpm;
        let next = Bar {
            start_ms: end,
            beats_num: last.beats_num,
            beats_den: last.beats_den,
            segments: vec![Segment {
                start_ms: end,
                bpm: seg.bpm,
            }],
            rounding: last.rounding,
        };
        bars.push(next);
        if bars.len() > 1000 {
            return Err(BmsError::TooManyBars);
        }
    }
    Ok(bars)
}

/// `beat` as `n / d` on the coarsest grid within `rounding` ms of it at `bpm`.
fn snap_beat(beat: f64, bpm: f64, rounding: f64) -> (u64, u64) {
    let tolerance = rounding * bpm / 60000.0 + 1e-9;
    let grids: &[u64] = if rounding < HALF_FRAME_MS {
        &FINE_GRIDS
    } else {
        &BEAT_GRIDS
    };
    let finest = grids[grids.len() - 1];
    grids
        .iter()
        .map(|&d| ((beat * d as f64).round().max(0.0) as u64, d))
        .find(|&(n, d)| (n as f64 / d as f64 - beat).abs() <= tolerance)
        .unwrap_or_else(|| ((beat * finest as f64).round().max(0.0) as u64, finest))
}

/// Where time `t` falls, snapped.
fn position(bars: &[Bar], t: f64) -> Pos {
    let i = bars
        .iter()
        .rposition(|b| b.start_ms <= t + b.rounding)
        .unwrap_or(0);
    let bar = &bars[i];
    let (beat, bpm) = bar.beat_at(t.max(bar.start_ms));
    let (n, d) = snap_beat(beat, bpm, bar.rounding);
    // Snapped onto the bar's end: the next bar's start.
    if n * bar.beats_den >= bar.beats_num * d && i + 1 < bars.len() {
        return Pos {
            bar: i + 1,
            n: 0,
            d: 1,
        };
    }
    let g = gcd(n, d).max(1);
    Pos {
        bar: i,
        n: n / g,
        d: d / g,
    }
}

/// Each bar's first tempo when it changes, and changes inside bars, as
/// positions; the chart's starting tempo is `bars[0]`'s first and not listed.
fn tempo_changes(bars: &[Bar]) -> Vec<(Pos, f64)> {
    let mut changes = Vec::new();
    let mut current = String::new();
    for (i, bar) in bars.iter().enumerate() {
        for seg in &bar.segments {
            let text = tempo_text(seg.bpm);
            if text == current {
                continue;
            }
            current = text;
            if i == 0 && seg.start_ms == bar.start_ms {
                continue;
            }
            let pos = if seg.start_ms == bar.start_ms {
                Pos { bar: i, n: 0, d: 1 }
            } else {
                position(bars, seg.start_ms)
            };
            changes.push((pos, seg.bpm));
        }
    }
    changes
}

/// The bars and tempos `to_bms` writes, for a player that builds its chart
/// from the `.ply` directly instead of parsing the BMS text.
pub struct Layout {
    bars: Vec<Bar>,
}

impl Layout {
    pub fn of(ply: &Ply) -> Result<Self, BmsError> {
        Ok(Self { bars: bars(ply)? })
    }

    /// Bar lengths in beats, in order.
    pub fn bar_beats(&self) -> impl Iterator<Item = f64> + '_ {
        self.bars.iter().map(Bar::beats)
    }

    pub fn initial_bpm(&self) -> f64 {
        self.bars[0].segments[0].bpm
    }

    /// Tempo changes after the start: (bar, fraction of the bar, bpm).
    pub fn tempo_changes(&self) -> Vec<(usize, f64, f64)> {
        tempo_changes(&self.bars)
            .into_iter()
            .map(|(pos, bpm)| (pos.bar, self.fraction(pos), bpm))
            .collect()
    }

    /// Where time `ms` falls, snapped as in the BMS text: (bar, fraction of the bar).
    pub fn position(&self, ms: f64) -> (usize, f64) {
        let pos = position(&self.bars, ms);
        (pos.bar, self.fraction(pos))
    }

    fn fraction(&self, pos: Pos) -> f64 {
        let bar = &self.bars[pos.bar];
        (pos.n * bar.beats_den) as f64 / (pos.d * bar.beats_num) as f64
    }
}

/// A tempo as written in the BMS text.
fn tempo_text(bpm: f64) -> String {
    format!("{bpm:.6}")
}

/// Writes `ply` as BMS text.
pub fn to_bms(ply: &Ply, header: &Header) -> Result<String, BmsError> {
    let bars = bars(ply)?;

    // Objects by (bar, channel); several lines for channel 01.
    let mut objects: BTreeMap<(usize, u32), Vec<(Pos, String)>> = BTreeMap::new();
    let mut sounds_used: BTreeMap<u16, u16> = BTreeMap::new();
    let mut lane_sound: BTreeMap<(Side, u8), u16> = BTreeMap::new();
    let compact_sounds = ply
        .events
        .iter()
        .filter_map(|e| match e.command {
            Command::Sample { sound, .. } | Command::Auto { sound, .. } => Some(sound),
            Command::Note { .. } => None,
            _ => None,
        })
        .map(|sound| (sound, ()))
        .collect::<BTreeMap<_, _>>()
        .len()
        > 1295;
    let mut sound_id = |s: u16| -> Result<String, BmsError> {
        let id = if compact_sounds {
            if let Some(&id) = sounds_used.get(&s) {
                return base36(id as usize).ok_or(BmsError::TooManySounds);
            }
            let next = sounds_used.len() as u16 + 1;
            if next as usize > 1295 {
                return Ok("ZZ".to_string());
            }
            *sounds_used.entry(s).or_insert(next)
        } else {
            sounds_used.entry(s).or_insert(s);
            s
        };
        base36(id as usize).ok_or(BmsError::TooManySounds)
    };

    for e in &ply.events {
        let t = ply.millis(e.time);
        match e.command {
            Command::Sample { side, lane, sound } => {
                lane_sound.insert((side, lane), sound);
            }
            Command::Note { side, lane, .. } => {
                if e.time == 0 || (side == Side::P2 && !header.double) {
                    continue;
                }
                let Some(ch) = channel(side, lane, header.nine_buttons) else {
                    continue;
                };
                let sound = lane_sound.get(&(side, lane)).copied().unwrap_or(0);
                let id = if sound == 0 {
                    "ZZ".to_string()
                } else {
                    sound_id(sound)?
                };
                let pos = position(&bars, t);
                objects.entry((pos.bar, ch)).or_default().push((pos, id));
            }
            Command::Auto { sound, .. } if sound > 0 => {
                let pos = position(&bars, t);
                objects
                    .entry((pos.bar, 1))
                    .or_default()
                    .push((pos, sound_id(sound)?));
            }
            _ => {}
        }
    }

    // Tempo objects: each bar's first tempo when it changes, and changes inside bars.
    let mut tempo_ids: Vec<String> = Vec::new();
    for (pos, bpm) in tempo_changes(&bars) {
        let text = tempo_text(bpm);
        let index = match tempo_ids.iter().position(|t| *t == text) {
            Some(k) => k,
            None => {
                tempo_ids.push(text);
                tempo_ids.len() - 1
            }
        };
        let id = base36(index + 1).ok_or(BmsError::TooManyTempos)?;
        objects.entry((pos.bar, 8)).or_default().push((pos, id));
    }

    let mut out = String::new();
    let _ = writeln!(out, "#PLAYER {}", if header.double { 3 } else { 1 });
    let _ = writeln!(out, "#TITLE {}", header.title);
    if !header.subtitle.is_empty() {
        let _ = writeln!(out, "#SUBTITLE {}", header.subtitle);
    }
    let _ = writeln!(out, "#ARTIST {}", header.artist);
    let _ = writeln!(out, "#GENRE {}", header.genre);
    let _ = writeln!(out, "#PLAYLEVEL {}", header.level);
    let _ = writeln!(out, "#DIFFICULTY {}", header.difficulty);
    let _ = writeln!(out, "#BPM {}", tempo_text(bars[0].segments[0].bpm));
    let _ = writeln!(out, "#RANK 2");
    for (k, text) in tempo_ids.iter().enumerate() {
        let _ = writeln!(out, "#BPM{} {}", base36(k + 1).unwrap(), text);
    }
    for (&sound, &id) in &sounds_used {
        let _ = writeln!(
            out,
            "#WAV{} {}",
            base36(id as usize).ok_or(BmsError::TooManySounds)?,
            (header.sound_name)(sound)
        );
    }
    for (i, bar) in bars.iter().enumerate() {
        if (bar.beats_num, bar.beats_den) != (4, 1) {
            let _ = writeln!(out, "#{i:03}02:{}", bar.beats() / 4.0);
        }
    }
    for ((bar, ch), objs) in &objects {
        let beats = &bars[*bar];
        // One line per channel, more where objects share a position (background sounds).
        let mut lines: Vec<Vec<&(Pos, String)>> = Vec::new();
        for obj in objs {
            match lines.iter_mut().find(|l| l.iter().all(|o| o.0 != obj.0)) {
                Some(line) => line.push(obj),
                None => lines.push(vec![obj]),
            }
        }
        for line in lines {
            // Slot count: positions are n/d beats of a bar of beats_num/beats_den.
            // Fraction of bar = n * beats_den / (d * beats_num).
            let slots = line.iter().fold(1u64, |acc, (p, _)| {
                let num = p.n * beats.beats_den;
                let den = p.d * beats.beats_num;
                lcm(acc, den / gcd(num, den).max(1))
            });
            let mut cells = vec!["00"; slots as usize];
            for (p, id) in &line {
                let k = p.n * beats.beats_den * slots / (p.d * beats.beats_num);
                cells[k as usize] = id;
            }
            let _ = writeln!(out, "#{bar:03}{ch:02}:{}", cells.concat());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ply::Event;

    fn header(double: bool) -> Header {
        Header {
            title: "T".into(),
            subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            level: 5,
            difficulty: 2,
            double,
            nine_buttons: false,
            sound_name: |n| format!("{n:04}.wav"),
        }
    }

    /// A chart in milliseconds (multiplier 1000), 120 BPM, bars every 2000 ms.
    fn chart(extra: &[(u32, Command)]) -> Ply {
        let mut events = vec![
            Event {
                time: 0,
                command: Command::Bpm { bpm: 120.0 },
            },
            Event {
                time: 0,
                command: Command::TimeSignature {
                    numerator: 4,
                    denominator: 4,
                },
            },
        ];
        for t in [0, 2000, 4000, 6000] {
            events.push(Event {
                time: t,
                command: Command::Measure,
            });
        }
        events.extend(extra.iter().map(|&(time, command)| Event { time, command }));
        events.sort_by_key(|e| e.time);
        Ply {
            tick_ms: 1.0,
            events,
        }
    }

    fn note(side: Side, lane: u8) -> Command {
        Command::Note {
            side,
            lane,
            length: 0,
        }
    }

    #[test]
    fn notes_take_their_lanes_sound_and_snap_to_the_beat() {
        let ply = chart(&[
            (0, note(Side::P1, 0)), // placeholder, not played
            (
                1000,
                Command::Sample {
                    side: Side::P1,
                    lane: 7,
                    sound: 37,
                },
            ),
            // Beat 1 of bar 1, a millisecond off the grid.
            (2501, note(Side::P1, 7)),
            (4000, Command::Auto { slot: 8, sound: 2 }),
        ]);
        let bms = to_bms(&ply, &header(false)).unwrap();
        assert!(bms.contains("#BPM 120.000000\n"), "{bms}");
        assert!(bms.contains("#WAV11 0037.wav\n"), "{bms}");
        assert!(bms.contains("#WAV02 0002.wav\n"), "{bms}");
        // One beat into a four-beat bar.
        assert!(bms.contains("#00116:00110000\n"), "{bms}");
        assert!(bms.contains("#00201:02\n"), "{bms}");
        assert!(!bms.contains("#00011"), "the placeholder is dropped: {bms}");
    }

    #[test]
    fn second_side_notes_play_only_in_double() {
        let ply = chart(&[(2000, note(Side::P2, 5))]);
        assert!(!to_bms(&ply, &header(false)).unwrap().contains("#00128"));
        let dp = to_bms(&ply, &header(true)).unwrap();
        assert!(
            dp.contains("#PLAYER 3\n") && dp.contains("#00128:ZZ\n"),
            "{dp}"
        );
    }

    #[test]
    fn a_double_bar_line_is_one_bar() {
        let mut ply = chart(&[]);
        ply.events.push(Event {
            time: 2000,
            command: Command::Measure,
        });
        ply.events.sort_by_key(|e| e.time);
        ply.events.push(Event {
            time: 6000,
            command: note(Side::P1, 0),
        });
        // Still bar 3 at 6000 ms, not bar 4.
        assert!(to_bms(&ply, &header(false))
            .unwrap()
            .contains("#00311:ZZ\n"));
    }

    #[test]
    fn bar_lines_set_the_tempo_when_the_stored_one_is_rounded() {
        // Stored 120, but the bars are 1980 ms apart: about 121.2 BPM.
        let mut ply = chart(&[]);
        for e in &mut ply.events {
            if e.command == Command::Measure {
                e.time = e.time / 2000 * 1980;
            }
        }
        let bms = to_bms(&ply, &header(false)).unwrap();
        assert!(bms.contains("#BPM 121.212121\n"), "{bms}");
    }

    #[test]
    fn bar_lines_set_the_meter_a_signature_leaves_out() {
        // Under 4/4 at 120 BPM, a 3500 ms bar is seven beats (GOLDEN HORN).
        let mut ply = chart(&[]);
        for e in &mut ply.events {
            if e.command == Command::Measure && e.time > 0 {
                e.time += 1500;
            }
        }
        let bms = to_bms(&ply, &header(false)).unwrap();
        assert!(bms.contains("#00002:1.75\n"), "{bms}");
        assert!(!bms.contains("#00102:"), "{bms}");
    }

    #[test]
    fn a_millisecond_chart_keeps_a_note_off_the_beat() {
        // 4 ms past beat 1 at 120 BPM: a frame chart would snap it, a pop'n
        // chart (times in ms) puts it on the 1/192 grid instead.
        let ply = chart(&[(2504, note(Side::P1, 0))]);
        let bms = to_bms(&ply, &header(false)).unwrap();
        let line = bms.lines().find(|l| l.starts_with("#00111:")).unwrap();
        let cells = (line.len() - 7) / 2;
        let at = line[7..].find("ZZ").unwrap() / 2;
        let beat = at as f64 * 4.0 / cells as f64;
        assert!((beat - 1.008).abs() < 0.003, "{line}");
    }

    #[test]
    fn the_same_chart_gives_the_same_text() {
        let ply = chart(&[
            (2504, note(Side::P1, 3)),
            (3000, Command::Auto { slot: 8, sound: 9 }),
        ]);
        assert_eq!(
            to_bms(&ply, &header(false)).unwrap(),
            to_bms(&ply, &header(false)).unwrap()
        );
    }
}

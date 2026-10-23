//! A chart on a game disc (`disc:<image or drive>::<chart key>`), read with
//! `cs_disc`: gameplay uses the native `.ply` chart conversion, while the
//! synthesized BMS form remains available only to legacy metadata callers.

use std::collections::HashMap;
use std::fs::File;

use beetle_audio::{PcmBuffer, SampleBank};
use beetle_core::bms::{TimingEvent, TimingEventKind};
use beetle_core::{BmpId, BmsChart, BmsHeader, Lane, NoteEvent, NoteType, PlayMode, WavId};
use beetle_render::{ColorRgba, ImageBuffer};
use cs_disc::iidx16::{sound_number, Disc};
use cs_disc::meta::SongMeta;
use cs_disc::ply::{Command, Ply, Side};
use cs_disc::wvb::{decode_bgm, Bank, Pcm};
use cs_disc::{Game, SectorReader, Volume};

use super::{progress, ChartLoadError, SongSource, StageArt, VideoSource};

/// The sound number a chart's BGM cue plays: the chart's BGM stream.
const BGM_CUE: u16 = 1;

pub(super) struct DiscSource {
    disc: SectorReader<File>,
    songs: OpenDisc,
    song: usize,
    chart: usize,
    /// The song's hand-checked details (`disc_meta`), if any.
    details: Option<SongMeta>,
}

enum OpenDisc {
    Iidx15 { disc: cs_disc::iidx15::Disc },
    Iidx16 { disc: Disc },
}

impl DiscSource {
    pub(super) fn open(image: &str, key: &str) -> Result<Self, ChartLoadError> {
        let mut disc = cs_disc::open(image).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ChartLoadError::NotFound,
            _ => ChartLoadError::Unreadable,
        })?;
        // The drive may hold another disc by now.
        let Ok(Some(game)) = cs_disc::identify(&mut disc) else {
            return Err(ChartLoadError::NotFound);
        };
        let volume = Volume::open(&mut disc).map_err(|_| ChartLoadError::Unreadable)?;
        let (songs, song, chart, slug, song_id) = match game {
            Game::Iidx15(edition) => {
                let songs = cs_disc::iidx15::Disc::open(&mut disc, &volume, edition)
                    .map_err(|_| ChartLoadError::Unreadable)?;
                let (song, chart) = songs
                    .songs
                    .iter()
                    .enumerate()
                    .find_map(|(i, s)| {
                        s.charts
                            .iter()
                            .position(|c| c.key(s) == key)
                            .map(|c| (i, c))
                    })
                    .ok_or(ChartLoadError::NotFound)?;
                let id = songs.songs[song].id;
                (
                    OpenDisc::Iidx15 { disc: songs },
                    song,
                    chart,
                    edition.slug(),
                    id,
                )
            }
            Game::Iidx16(edition) => {
                let songs = Disc::open(&mut disc, &volume, edition)
                    .map_err(|_| ChartLoadError::Unreadable)?;
                let (song, chart) = songs
                    .songs
                    .iter()
                    .enumerate()
                    .find_map(|(i, s)| {
                        s.charts
                            .iter()
                            .position(|c| c.key(s) == key)
                            .map(|c| (i, c))
                    })
                    .ok_or(ChartLoadError::NotFound)?;
                let id = songs.songs[song].id;
                (
                    OpenDisc::Iidx16 { disc: songs },
                    song,
                    chart,
                    edition.slug(),
                    id,
                )
            }
        };
        let details = crate::disc_meta::load(slug).remove(&song_id);
        Ok(Self {
            disc,
            songs,
            song,
            chart,
            details,
        })
    }
}

/// 16-bit mono or stereo PCM as the mixer's interleaved stereo floats.
fn to_buffer(pcm: &Pcm) -> PcmBuffer {
    let left = &pcm.channels[0];
    let right = pcm.channels.get(1).unwrap_or(left);
    let frames = left.len().min(right.len());
    let mut samples = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        samples.push(left[i] as f32 / 32768.0);
        samples.push(right[i] as f32 / 32768.0);
    }
    PcmBuffer::new(pcm.rate, samples)
}

impl SongSource for DiscSource {
    fn native_chart(&mut self) -> Result<Option<BmsChart>, ChartLoadError> {
        let (ply, title, level, difficulty, double) = match &mut self.songs {
            OpenDisc::Iidx15 { disc } => {
                let song = &disc.songs[self.song];
                let slot = &song.charts[self.chart];
                (
                    disc.chart_ply(&mut self.disc, slot),
                    song.title.clone(),
                    slot.level,
                    match slot.difficulty {
                        cs_disc::iidx15::Difficulty::Beginner => 1,
                        cs_disc::iidx15::Difficulty::Normal => 2,
                        cs_disc::iidx15::Difficulty::Hyper => 3,
                        cs_disc::iidx15::Difficulty::Another => 4,
                        cs_disc::iidx15::Difficulty::Black => 5,
                    },
                    slot.style == cs_disc::iidx15::Style::Double,
                )
            }
            OpenDisc::Iidx16 { disc } => {
                let song = &disc.songs[self.song];
                let slot = &song.charts[self.chart];
                (
                    disc.chart_ply(&mut self.disc, slot),
                    song.title.clone(),
                    slot.level,
                    match slot.difficulty {
                        cs_disc::iidx16::Difficulty::Beginner => 1,
                        cs_disc::iidx16::Difficulty::Normal => 2,
                        cs_disc::iidx16::Difficulty::Hyper => 3,
                        cs_disc::iidx16::Difficulty::Another => 4,
                        cs_disc::iidx16::Difficulty::Black => 5,
                    },
                    slot.style == cs_disc::iidx16::Style::Double,
                )
            }
        };
        let ply = ply.map_err(|_| ChartLoadError::NotAChart)?;
        let chart = native_ply_chart(&ply, title, level, difficulty, double);
        (!chart.notes.is_empty())
            .then_some(chart)
            .ok_or(ChartLoadError::NotAChart)
            .map(Some)
    }

    fn chart_bytes(&mut self) -> Result<Vec<u8>, ChartLoadError> {
        // Compatibility path for `read_chart_bytes` and metadata tooling. The
        // gameplay loader calls `native_chart` first and never enters here.
        let bytes = match &mut self.songs {
            OpenDisc::Iidx15 { disc, .. } => {
                let song = &disc.songs[self.song];
                let chart = &song.charts[self.chart];
                disc.chart_bms(&mut self.disc, song, chart, self.details.as_ref())
            }
            OpenDisc::Iidx16 { disc, .. } => {
                let song = &disc.songs[self.song];
                let chart = &song.charts[self.chart];
                disc.chart_bms(&mut self.disc, song, chart, self.details.as_ref())
            }
        };
        bytes
            .map(String::into_bytes)
            .map_err(|_| ChartLoadError::NotAChart)
    }

    fn sounds(&mut self, chart: &BmsChart) -> (SampleBank, usize) {
        let mut bank = SampleBank::new();
        let (bgm, wvb_bytes) = match &mut self.songs {
            OpenDisc::Iidx15 { disc, .. } => {
                let song = &disc.songs[self.song];
                let slot = &song.charts[self.chart];
                let (wvb, bgm) = song.sound_files(slot.style, slot.difficulty);
                let bytes = wvb.and_then(|f| disc.read_file(&mut self.disc, f).ok());
                (bgm, bytes)
            }
            OpenDisc::Iidx16 { disc, .. } => {
                let song = &disc.songs[self.song];
                let slot = &song.charts[self.chart];
                let (wvb, bgm) = song.sound_files(slot.style, slot.difficulty);
                let bytes = wvb.and_then(|f| disc.read_file(&mut self.disc, f).ok());
                (bgm, bytes)
            }
        };
        let key_sounds = wvb_bytes.as_deref().and_then(|b| Bank::parse(b).ok());

        // Sound numbers by the names the synthesized chart gave them.
        let mut wanted: Vec<_> = chart.header.wav_table.iter().collect();
        wanted.sort_by_key(|(id, _)| **id);
        let mut loaded = 0;
        for (&wav_id, name) in wanted {
            let Some(n) = sound_number(name) else {
                continue;
            };
            let pcm = if n == BGM_CUE {
                bgm.and_then(|f| match &mut self.songs {
                    OpenDisc::Iidx15 { disc, .. } => disc.read_file(&mut self.disc, f).ok(),
                    OpenDisc::Iidx16 { disc, .. } => disc.read_file(&mut self.disc, f).ok(),
                })
                .and_then(|b| decode_bgm(&b).ok())
            } else {
                key_sounds
                    .as_ref()
                    .and_then(|bank| bank.decode(n))
                    .and_then(Result::ok)
            };
            if let Some(pcm) = pcm {
                bank.insert(wav_id, to_buffer(&pcm));
                loaded += 1;
                progress::set_done(loaded);
            }
        }
        (bank, loaded)
    }

    fn images(&mut self, _: &HashMap<BmpId, String>) -> HashMap<BmpId, ImageBuffer> {
        HashMap::new()
    }

    /// The song's background video. The chart has no BGA events, so the
    /// game plays it from the start of the chart.
    fn videos(&mut self, _: &BmsChart) -> HashMap<BmpId, VideoSource> {
        let Some(bytes) = (match &mut self.songs {
            OpenDisc::Iidx15 { disc, .. } => disc.songs[self.song]
                .video
                .and_then(|file| disc.read_file(&mut self.disc, file).ok()),
            OpenDisc::Iidx16 { disc, .. } => disc.songs[self.song]
                .video
                .and_then(|file| disc.read_file(&mut self.disc, file).ok()),
        }) else {
            return HashMap::new();
        };
        let source = VideoSource::Memory {
            bytes: bytes.into(),
            filename_hint: Some("bga.mpg".to_string()),
        };
        HashMap::from([(BmpId(1), source)])
    }

    /// The song's loading card, on a 4:3 canvas.
    fn stage_image(&mut self) -> Option<StageArt> {
        if let OpenDisc::Iidx15 { disc, .. } = &self.songs {
            let song = &disc.songs[self.song];
            let decoded = disc.stage_image(&mut self.disc, song).ok()??.centered_4_3();
            let mut image =
                ImageBuffer::new(decoded.width, decoded.height, ColorRgba::new(0, 0, 0, 255));
            for (out, [r, g, b, a]) in image.pixels.iter_mut().zip(decoded.pixels) {
                *out = ColorRgba::new(r, g, b, a);
            }
            let bytes = image.encode_bmp_bytes();
            return Some((image, Some(bytes)));
        }
        let song_card = match &self.songs {
            OpenDisc::Iidx16 { disc, .. } => disc.songs[self.song].card,
            OpenDisc::Iidx15 { .. } => unreachable!(),
        };
        let container = self.details.as_ref().and_then(|d| d.card).or(song_card)?;
        let card = match &mut self.songs {
            OpenDisc::Iidx15 { disc, .. } => disc.card_image(&mut self.disc, container).ok()??,
            OpenDisc::Iidx16 { disc, .. } => disc.card_image(&mut self.disc, container).ok()??,
        }
        .centered_4_3();
        let mut image = ImageBuffer::new(card.width, card.height, ColorRgba::new(0, 0, 0, 255));
        for (out, [r, g, b, a]) in image.pixels.iter_mut().zip(card.pixels) {
            *out = ColorRgba::new(r, g, b, a);
        }
        let bytes = image.encode_bmp_bytes();
        Some((image, Some(bytes)))
    }
}

fn native_ply_chart(ply: &Ply, title: String, level: u8, difficulty: u8, double: bool) -> BmsChart {
    let mut chart = BmsChart {
        header: BmsHeader {
            title,
            play_level: level as u32,
            difficulty: Some(difficulty as u32),
            player: if double { 3 } else { 1 },
            declared_mode: Some(if double {
                PlayMode::Keys14
            } else {
                PlayMode::Keys7
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut lines: Vec<f64> = ply
        .events
        .iter()
        .filter_map(|e| matches!(e.command, Command::Measure).then_some(ply.millis(e.time)))
        .collect();
    lines.dedup_by(|a, b| (*a - *b).abs() < 0.001);
    if lines.is_empty() {
        lines.push(0.0);
    }
    // A `.ply` stores the time signature at the bar line, while BMS stores
    // the resulting bar length as a multiplier of four quarter beats.
    let mut signature = (4.0_f64, 4.0_f64);
    for event in &ply.events {
        if let Command::TimeSignature {
            numerator,
            denominator,
        } = event.command
        {
            if numerator > 0 && denominator > 0 {
                signature = (numerator as f64, denominator as f64);
            }
        }
        if let Command::Measure = event.command {
            let measure = lines
                .partition_point(|t| *t <= ply.millis(event.time))
                .saturating_sub(1) as u32;
            chart
                .measure_lengths
                .insert(measure, signature.0 / signature.1);
        }
    }
    // Continue the final measure when notes occur after the last explicit
    // bar line. A fixed fallback interval compresses the chart's tail.
    let last_event_ms = ply
        .events
        .last()
        .map_or(0.0, |event| ply.millis(event.time));
    let last_beats = signature.0 / signature.1 * 4.0;
    let last_bpm = ply
        .events
        .iter()
        .rev()
        .find_map(|event| match event.command {
            Command::Bpm { bpm } if bpm > 0.0 => Some(bpm),
            _ => None,
        })
        .unwrap_or(130.0);
    let tail_ms = last_beats * 60000.0 / last_bpm;
    while last_event_ms > lines.last().copied().unwrap_or(0.0) + tail_ms / 2.0 {
        let next = lines.last().copied().unwrap_or(0.0) + tail_ms;
        lines.push(next);
        let measure = lines.len().saturating_sub(2) as u32;
        chart
            .measure_lengths
            .insert(measure, signature.0 / signature.1);
    }
    let position_lengths = chart.measure_lengths.clone();
    let position_bpm = ply
        .events
        .iter()
        .find_map(|e| match e.command {
            Command::Bpm { bpm } if bpm > 0.0 => Some(bpm),
            _ => None,
        })
        .unwrap_or(130.0);
    // Fit the observed bar lines to their accumulated musical beat positions.
    // This removes the small per-bar drift caused by rounded 59.94 Hz frames
    // for the common (constant-tempo) case.
    let mut observed_beats = Vec::with_capacity(lines.len());
    let mut beat = 0.0;
    for (i, _) in lines.iter().enumerate() {
        observed_beats.push(beat);
        beat += position_lengths.get(&(i as u32)).copied().unwrap_or(1.0) * 4.0;
    }
    let line_bpm = |at: f64| {
        ply.events
            .iter()
            .filter_map(|e| match e.command {
                Command::Bpm { bpm } if bpm > 0.0 && ply.millis(e.time) <= at => Some(bpm),
                _ => None,
            })
            .last()
            .unwrap_or(position_bpm)
    };
    let mut corrected_lines = lines.clone();
    let mut first = 0;
    while first < lines.len() {
        let bpm = line_bpm(lines[first]);
        let mut last = first;
        while last + 1 < lines.len() && (line_bpm(lines[last + 1]) - bpm).abs() < 1e-9 {
            last += 1;
        }
        let end = (last + 1).min(lines.len() - 1);
        let mut xs = observed_beats[first..=end].to_vec();
        let mut ys = lines[first..=end].to_vec();
        if end == last && end + 1 < lines.len() {
            xs.push(observed_beats[end + 1]);
            ys.push(lines[end + 1]);
        }
        let n = xs.len() as f64;
        let mx = xs.iter().sum::<f64>() / n.max(1.0);
        let my = ys.iter().sum::<f64>() / n.max(1.0);
        let sxx = xs.iter().map(|x| (x - mx).powi(2)).sum::<f64>();
        let sxy = xs
            .iter()
            .zip(&ys)
            .map(|(x, y)| (x - mx) * (y - my))
            .sum::<f64>();
        let slope = if sxx > 0.0 { sxy / sxx } else { 60000.0 / bpm };
        let intercept = my - slope * mx;
        for i in first..=last {
            corrected_lines[i] = intercept + slope * observed_beats[i];
        }
        first = last + 1;
    }
    // Keep the raw tempo cuts as well: a bar can contain a speed change, so
    // its note beat is not proportional to wall-clock time across the whole
    // bar. The stored tempo ratios determine the musical position inside it.
    let mut bar_tempos: Vec<Vec<(f64, f64)>> = Vec::with_capacity(lines.len());
    let mut current_bpm = position_bpm;
    for bar in 0..lines.len() {
        let start = lines[bar];
        let end = lines.get(bar + 1).copied().unwrap_or(start + 4000.0);
        for event in &ply.events {
            let at = ply.millis(event.time);
            if at <= start {
                if let Command::Bpm { bpm } = event.command {
                    if bpm > 0.0 {
                        current_bpm = bpm;
                    }
                }
            }
        }
        let mut segments = vec![(start, current_bpm)];
        for event in &ply.events {
            let at = ply.millis(event.time);
            if at > start && at < end {
                if let Command::Bpm { bpm } = event.command {
                    if bpm > 0.0 {
                        segments.push((at, bpm));
                    }
                }
            }
        }
        bar_tempos.push(segments);
    }
    // The generated-BMS path writes the first corrected segment as `#BPM`.
    // Keep the native timing model on that same time scale when the opening
    // bar already contains a speed change.
    let first_start = lines[0];
    let first_end = lines.get(1).copied().unwrap_or(first_start + 4000.0);
    let first_stored_beats = bar_tempos[0]
        .iter()
        .enumerate()
        .map(|(i, &(_, bpm))| {
            let end = bar_tempos[0].get(i + 1).map_or(first_end, |next| next.0);
            (end - bar_tempos[0][i].0).max(0.0) * bpm / 60000.0
        })
        .sum::<f64>();
    let first_target_beats = position_lengths.get(&0).copied().unwrap_or(1.0) * 4.0;
    if first_stored_beats > 0.0 {
        chart.header.bpm = position_bpm * first_target_beats / first_stored_beats;
    }
    let corrected_time = |ms: f64| {
        let i = lines.partition_point(|t| *t <= ms).saturating_sub(1);
        let next = (i + 1).min(lines.len() - 1);
        if next == i || lines[next] <= lines[i] {
            corrected_lines[i]
        } else {
            corrected_lines[i]
                + (ms - lines[i]) / (lines[next] - lines[i])
                    * (corrected_lines[next] - corrected_lines[i])
        }
    };
    let pos = |ms: f64| -> (u32, f64) {
        let corrected_ms = corrected_time(ms);
        let bar = corrected_lines
            .partition_point(|t| *t <= corrected_ms)
            .saturating_sub(1);
        let end = corrected_lines
            .get(bar + 1)
            .copied()
            .unwrap_or(corrected_lines[bar] + 4000.0);
        let beats = position_lengths.get(&(bar as u32)).copied().unwrap_or(1.0) * 4.0;
        // `.ply` times are frame-rounded. Snap to the coarsest musical grid
        // that is still within half a 59.94 Hz frame, matching ply_bms.
        let raw_start = lines[bar];
        let raw_end = lines.get(bar + 1).copied().unwrap_or(raw_start + 4000.0);
        let cuts = &bar_tempos[bar];
        let mut stored_beats = 0.0;
        let mut raw_beat = 0.0;
        for (i, &(at, bpm)) in cuts.iter().enumerate() {
            let segment_end = cuts.get(i + 1).map_or(raw_end, |next| next.0);
            let duration = (segment_end - at).max(0.0);
            stored_beats += duration * bpm / 60000.0;
            if ms <= segment_end {
                raw_beat += (ms - at).max(0.0) * bpm / 60000.0;
                break;
            }
            raw_beat += duration * bpm / 60000.0;
        }
        let raw_beat = if stored_beats > 0.0 {
            raw_beat * beats / stored_beats
        } else {
            ((corrected_ms - corrected_lines[bar]) / (end - corrected_lines[bar]).max(1.0)) * beats
        };
        let bpm = position_bpm.max(1.0);
        let tolerance = (1000.0 / 59.94 / 2.0) * bpm / 60000.0 + 1e-9;
        let mut snapped = raw_beat;
        for denominator in [1_u64, 2, 3, 4, 6, 8, 12, 16, 24, 48] {
            let numerator = (raw_beat * denominator as f64).round().max(0.0) as u64;
            let candidate = numerator as f64 / denominator as f64;
            if (candidate - raw_beat).abs() <= tolerance {
                snapped = candidate;
                break;
            }
        }
        (bar as u32, (snapped / beats.max(1.0)).clamp(0.0, 1.0))
    };
    let scaled_tempo = |ms: f64, bpm: f64| {
        let bar = lines.partition_point(|t| *t <= ms).saturating_sub(1);
        let raw_end = lines.get(bar + 1).copied().unwrap_or(lines[bar] + 4000.0);
        let stored_beats = bar_tempos[bar]
            .iter()
            .enumerate()
            .map(|(i, &(_, segment_bpm))| {
                let end = bar_tempos[bar].get(i + 1).map_or(raw_end, |next| next.0);
                (end - bar_tempos[bar][i].0).max(0.0) * segment_bpm / 60000.0
            })
            .sum::<f64>();
        let target_beats = position_lengths.get(&(bar as u32)).copied().unwrap_or(1.0) * 4.0;
        if stored_beats > 0.0 {
            bpm * target_beats / stored_beats
        } else {
            bpm
        }
    };
    let mut samples = std::collections::HashMap::<(bool, u8), WavId>::new();
    let mut have_initial_bpm = false;
    for event in &ply.events {
        let ms = ply.millis(event.time);
        let (measure, fraction) = pos(ms);
        match event.command {
            Command::Sample { side, lane, sound } => {
                samples.insert((side == Side::P2, lane), WavId(sound));
                chart
                    .header
                    .wav_table
                    .insert(WavId(sound), format!("{sound:04}.wav"));
            }
            Command::Note { side, lane, length } => {
                // IIDX `.ply` files begin with zero-time placeholder objects
                // used by the original game. They are not playable notes;
                // keeping them causes autoplay to start with a false combo.
                if event.time == 0 {
                    continue;
                }
                let source_lane = lane;
                let lane = match (side, lane) {
                    (Side::P1, 7) => Lane::Scratch,
                    (Side::P2, 7) => Lane::P2Scratch,
                    (Side::P1, n @ 0..=6) => [
                        Lane::Key1,
                        Lane::Key2,
                        Lane::Key3,
                        Lane::Key4,
                        Lane::Key5,
                        Lane::Key6,
                        Lane::Key7,
                    ][n as usize],
                    (Side::P2, n @ 0..=6) => [
                        Lane::P2Key1,
                        Lane::P2Key2,
                        Lane::P2Key3,
                        Lane::P2Key4,
                        Lane::P2Key5,
                        Lane::P2Key6,
                        Lane::P2Key7,
                    ][n as usize],
                    _ => continue,
                };
                let wav = samples.get(&(side == Side::P2, source_lane)).copied();
                let kind = if length == 0 {
                    NoteType::Tap
                } else {
                    NoteType::LongNoteStart
                };
                chart.notes.push(NoteEvent {
                    measure,
                    fraction,
                    lane,
                    wav_id: wav,
                    note_type: kind,
                });
                if length != 0 {
                    let (m, f) = pos(ply.millis(event.time + length as u32));
                    chart.notes.push(NoteEvent {
                        measure: m,
                        fraction: f,
                        lane,
                        wav_id: wav,
                        note_type: NoteType::LongNoteEnd,
                    });
                }
            }
            Command::Auto { sound, .. } => {
                chart.bgm_notes.push((measure, fraction, WavId(sound)));
                chart
                    .header
                    .wav_table
                    .insert(WavId(sound), format!("{sound:04}.wav"));
            }
            Command::Bpm { bpm } if bpm > 0.0 && !have_initial_bpm => {
                chart.header.bpm = bpm;
                have_initial_bpm = true;
            }
            Command::Bpm { bpm } if bpm > 0.0 => {
                chart.timing_events.push(TimingEvent {
                    measure,
                    fraction,
                    kind: TimingEventKind::BpmChange(scaled_tempo(ms, bpm)),
                });
            }
            _ => {}
        }
    }
    chart.total_notes_count = chart.notes.len();
    chart.max_measure = lines.len().saturating_sub(1) as u32;
    chart
}

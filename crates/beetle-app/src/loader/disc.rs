//! A chart on a game disc (`disc:<image or drive>::<chart key>`), read with
//! `cs_disc`: gameplay uses the native `.ply` chart conversion, while the
//! synthesized BMS form remains available only to legacy metadata callers.

use std::collections::HashMap;
use std::fs::File;

use beetle_audio::{PcmBuffer, SampleBank};
use beetle_core::bms::{TimingEvent, TimingEventKind};
use beetle_core::{BmpId, BmsChart, BmsHeader, Lane, NoteEvent, NoteType, PlayMode, WavId};
use beetle_render::{ColorRgba, ImageBuffer};
use cs_disc::iidx::{sound_number, Difficulty, Disc, Style};
use cs_disc::meta::SongMeta;
use cs_disc::ply::{Command, Ply, Side};
use cs_disc::wvb::{decode_bgm, Bank, Pcm};
use cs_disc::{SectorReader, Volume};

use super::{progress, ChartLoadError, SongSource, StageArt, VideoSource};

/// The sound number a chart's BGM cue plays: the chart's BGM stream.
const BGM_CUE: u16 = 1;

pub(super) struct DiscSource {
    disc: SectorReader<File>,
    songs: Disc,
    song: usize,
    chart: usize,
    /// The song's hand-checked details (`disc_meta`), if any.
    details: Option<SongMeta>,
}

impl DiscSource {
    pub(super) fn open(image: &str, key: &str) -> Result<Self, ChartLoadError> {
        let mut disc = cs_disc::open(image).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ChartLoadError::NotFound,
            _ => ChartLoadError::Unreadable,
        })?;
        // The drive may hold another disc by now.
        let Ok(Some(edition)) = cs_disc::identify(&mut disc) else {
            return Err(ChartLoadError::NotFound);
        };
        let volume = Volume::open(&mut disc).map_err(|_| ChartLoadError::Unreadable)?;
        let songs =
            Disc::open(&mut disc, &volume, edition).map_err(|_| ChartLoadError::Unreadable)?;
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
        let details = crate::disc_meta::load(edition.slug()).remove(&songs.songs[song].id);
        Ok(Self {
            disc,
            songs,
            song,
            chart,
            details,
        })
    }

    fn song(&self) -> &cs_disc::iidx::Song {
        &self.songs.songs[self.song]
    }

    fn slot(&self) -> &cs_disc::iidx::Chart {
        &self.song().charts[self.chart]
    }
}

/// 16-bit mono or stereo PCM as the mixer's interleaved stereo floats, at `gain`.
fn to_buffer(pcm: &Pcm, gain: f32) -> PcmBuffer {
    let left = &pcm.channels[0];
    let right = pcm.channels.get(1).unwrap_or(left);
    let frames = left.len().min(right.len());
    let scale = gain / 32768.0;
    let mut samples = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        samples.push(left[i] as f32 * scale);
        samples.push(right[i] as f32 * scale);
    }
    PcmBuffer::new(pcm.rate, samples)
}

impl SongSource for DiscSource {
    fn native_chart(&mut self) -> Result<Option<BmsChart>, ChartLoadError> {
        let slot = &self.songs.songs[self.song].charts[self.chart];
        let ply = self
            .songs
            .chart_ply(&mut self.disc, slot)
            .map_err(|_| ChartLoadError::NotAChart)?;
        let slot = self.slot();
        native_ply_chart(
            &ply,
            self.song().title.clone(),
            slot.level,
            slot.difficulty.bms_number(),
            slot.style == Style::Double,
        )
        .filter(|chart| !chart.notes.is_empty())
        .ok_or(ChartLoadError::NotAChart)
        .map(Some)
    }

    fn chart_bytes(&mut self) -> Result<Vec<u8>, ChartLoadError> {
        // Compatibility path for `read_chart_bytes` and metadata tooling. The
        // gameplay loader calls `native_chart` first and never enters here.
        let song = &self.songs.songs[self.song];
        self.songs
            .chart_bms(
                &mut self.disc,
                song,
                &song.charts[self.chart],
                self.details.as_ref(),
            )
            .map(String::into_bytes)
            .map_err(|_| ChartLoadError::NotAChart)
    }

    fn sounds(&mut self, chart: &BmsChart) -> (SampleBank, usize) {
        let mut bank = SampleBank::new();
        let slot = self.slot();
        let (wvb, bgm) = self.song().sound_files(slot.style, slot.difficulty);
        let wvb_bytes = wvb.and_then(|f| self.songs.read_file(&mut self.disc, f).ok());
        let key_sounds = wvb_bytes.as_deref().and_then(|b| Bank::parse(b).ok());

        // Sound numbers by the names the synthesized chart gave them.
        let mut wanted: Vec<_> = chart.header.wav_table.iter().collect();
        wanted.sort_by_key(|(id, _)| **id);
        let mut decoded = HashMap::new();
        for (&wav_id, name) in wanted {
            let Some(n) = sound_number(name) else {
                continue;
            };
            let pcm = if n == BGM_CUE {
                bgm.and_then(|f| self.songs.read_file(&mut self.disc, f).ok())
                    .and_then(|b| decode_bgm(&b).ok())
            } else {
                key_sounds
                    .as_ref()
                    .and_then(|bank| bank.decode(n))
                    .and_then(Result::ok)
            };
            if let Some(pcm) = pcm {
                decoded.insert(wav_id, pcm);
                progress::set_done(decoded.len());
            }
        }
        let gain = song_gain(chart, &decoded);
        let loaded = decoded.len();
        for (wav_id, pcm) in decoded {
            bank.insert(wav_id, to_buffer(&pcm, gain));
        }
        (bank, loaded)
    }

    fn images(&mut self, _: &HashMap<BmpId, String>) -> HashMap<BmpId, ImageBuffer> {
        HashMap::new()
    }

    /// The song's background video. The chart has no BGA events, so the
    /// game plays it from the start of the chart.
    fn videos(&mut self, _: &BmsChart) -> HashMap<BmpId, VideoSource> {
        let Some(bytes) = self
            .song()
            .video
            .and_then(|file| self.songs.read_file(&mut self.disc, file).ok())
        else {
            return HashMap::new();
        };
        let source = VideoSource::Memory {
            bytes: bytes.into(),
            filename_hint: Some("bga.mpg".to_string()),
        };
        HashMap::from([(BmpId(1), source)])
    }

    /// The song's stage art (loading card), on a 4:3 canvas.
    fn stage_image(&mut self) -> Option<StageArt> {
        let song = &self.songs.songs[self.song];
        let another = song.charts[self.chart].difficulty == Difficulty::Another;
        let card = self.details.as_ref().and_then(|d| d.card);
        let decoded = self
            .songs
            .stage_image(&mut self.disc, song, another, card)
            .ok()??
            .centered_4_3();
        let mut image =
            ImageBuffer::new(decoded.width, decoded.height, ColorRgba::new(0, 0, 0, 255));
        for (out, [r, g, b, a]) in image.pixels.iter_mut().zip(decoded.pixels) {
            *out = ColorRgba::new(r, g, b, a);
        }
        let bytes = image.encode_bmp_bytes();
        Some((image, Some(bytes)))
    }
}

/// The level a disc song plays at. The discs master the BGM and the key
/// sounds each to full scale and leave the headroom to the console's mixer,
/// so summed they pass full scale in many songs (EMPRESS's HOLIC 21% of the
/// time, MEI 14%) and the records hold no volume for it. The song's autoplay
/// mix is rendered once and brought down so that its loud stretches (the
/// 99th percentile of 10 ms peaks) sit at the mixer limiter's threshold; the
/// limiter takes the rest, and a song that never gets that loud plays as
/// mastered.
fn song_gain(chart: &BmsChart, sounds: &HashMap<WavId, Pcm>) -> f32 {
    const RATE: usize = 44100;
    const BLOCK: usize = RATE / 100;
    /// The limiter's threshold.
    const TARGET: f32 = 0.9;
    let timing = beetle_core::TimingModel::from_chart(chart);
    let starts: Vec<(f64, WavId)> = chart
        .notes
        .iter()
        .filter(|n| n.note_type != NoteType::LongNoteEnd)
        .filter_map(|n| {
            Some((
                timing.beat_to_time_seconds(n.measure, n.fraction),
                n.wav_id?,
            ))
        })
        .chain(
            chart
                .bgm_notes
                .iter()
                .map(|&(m, f, id)| (timing.beat_to_time_seconds(m, f), id)),
        )
        .collect();
    let end = starts
        .iter()
        .filter_map(|&(at, id)| {
            let pcm = sounds.get(&id)?;
            Some(at + pcm.channels[0].len() as f64 / pcm.rate.max(1) as f64)
        })
        .fold(0.0, f64::max);
    let frames = (end * RATE as f64) as usize + 1;
    let mut mix = vec![[0f32; 2]; frames];
    for (at, id) in starts {
        let Some(pcm) = sounds.get(&id) else {
            continue;
        };
        let left = &pcm.channels[0];
        let right = pcm.channels.get(1).unwrap_or(left);
        let first = ((at.max(0.0) * RATE as f64) as usize).min(frames);
        let out = &mut mix[first..];
        if pcm.rate as usize == RATE {
            for ((frame, &l), &r) in out.iter_mut().zip(left).zip(right) {
                frame[0] += l as f32;
                frame[1] += r as f32;
            }
            continue;
        }
        // Nearest earlier sample, in 32.32 fixed point.
        let step = ((pcm.rate as u64) << 32) / RATE as u64;
        let len = left.len().min(right.len()) as u64;
        let count = (((len << 32) / step.max(1)) as usize).min(out.len());
        let mut pos = 0u64;
        for frame in &mut out[..count] {
            let k = (pos >> 32) as usize;
            frame[0] += left[k] as f32;
            frame[1] += right[k] as f32;
            pos += step;
        }
    }
    let mut peaks: Vec<f32> = mix
        .chunks(BLOCK)
        .map(|b| {
            b.iter()
                .map(|f| f[0].abs().max(f[1].abs()))
                .fold(0.0, f32::max)
                / 32768.0
        })
        .collect();
    if peaks.is_empty() {
        return 1.0;
    }
    let at = (peaks.len() * 99 / 100).min(peaks.len() - 1);
    let loud = *peaks.select_nth_unstable_by(at, f32::total_cmp).1;
    if loud > TARGET {
        TARGET / loud
    } else {
        1.0
    }
}

fn native_ply_chart(
    ply: &Ply,
    title: String,
    level: u8,
    difficulty: u8,
    double: bool,
) -> Option<BmsChart> {
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
    // The bars, tempos and snapped positions the BMS conversion writes
    // (`ply_bms`): bar lines fitted against frame rounding, and the stored
    // tempo ratios inside a bar whose tempo changes.
    let layout = cs_disc::ply_bms::Layout::of(ply).ok()?;
    let bar_beats: Vec<f64> = layout.bar_beats().collect();
    for (bar, &beats) in bar_beats.iter().enumerate() {
        if beats != 4.0 {
            chart.measure_lengths.insert(bar as u32, beats / 4.0);
        }
    }
    chart.header.bpm = layout.initial_bpm();
    for (bar, fraction, bpm) in layout.tempo_changes() {
        chart.timing_events.push(TimingEvent {
            measure: bar as u32,
            fraction,
            kind: TimingEventKind::BpmChange(bpm),
        });
    }
    let pos = |ms: f64| {
        let (bar, fraction) = layout.position(ms);
        (bar as u32, fraction)
    };
    let mut samples = std::collections::HashMap::<(bool, u8), WavId>::new();
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
                // Notes at time 0 are not notes: lane 0 there carries the
                // side's note count in steps of up to 250 (`ply::Command::Note`).
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
            _ => {}
        }
    }
    chart.total_notes_count = chart.notes.len();
    chart.max_measure = bar_beats.len().saturating_sub(1) as u32;
    Some(chart)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(level: i16) -> Pcm {
        Pcm {
            rate: 44100,
            channels: vec![vec![level; 44100]],
        }
    }

    fn chart_playing(ids: &[u16]) -> BmsChart {
        let mut chart = BmsChart::default();
        chart.header.bpm = 120.0;
        chart.bgm_notes = ids.iter().map(|&id| (0, 0.0, WavId(id))).collect();
        chart
    }

    #[test]
    fn a_song_whose_sounds_sum_past_full_scale_is_brought_to_the_limiter_threshold() {
        let sounds = HashMap::from([(WavId(1), tone(30000)), (WavId(2), tone(30000))]);
        let gain = song_gain(&chart_playing(&[1, 2]), &sounds);
        assert!((gain - 0.9 * 32768.0 / 60000.0).abs() < 1e-4, "{gain}");
    }

    #[test]
    fn a_song_that_never_gets_loud_plays_as_mastered() {
        let sounds = HashMap::from([(WavId(1), tone(10000)), (WavId(2), tone(10000))]);
        assert_eq!(song_gain(&chart_playing(&[1, 2]), &sounds), 1.0);
    }
}

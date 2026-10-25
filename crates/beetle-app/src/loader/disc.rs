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
        let mut loaded = 0;
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
            _ => {}
        }
    }
    chart.total_notes_count = chart.notes.len();
    chart.max_measure = bar_beats.len().saturating_sub(1) as u32;
    Some(chart)
}

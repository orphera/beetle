//! A chart on a game disc (`disc:<image or drive>::<chart key>`), read with
//! `cs_disc`: the chart as the BMS text it synthesizes, and the key sounds
//! decoded straight from the disc into PCM (INV-3: all before play starts).

use std::collections::HashMap;
use std::fs::File;

use beetle_audio::{PcmBuffer, SampleBank};
use beetle_core::{BmpId, BmsChart};
use beetle_render::{ColorRgba, ImageBuffer};
use cs_disc::iidx16::{sound_number, Disc};
use cs_disc::meta::SongMeta;
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
    fn chart_bytes(&mut self) -> Result<Vec<u8>, ChartLoadError> {
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

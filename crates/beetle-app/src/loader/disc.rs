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
        let Ok(Some(Game::Iidx16(edition))) = cs_disc::identify(&mut disc) else {
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
        let details = crate::disc_meta::load(edition).remove(&songs.songs[song].id);
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
        let song = &self.songs.songs[self.song];
        let chart = &song.charts[self.chart];
        self.songs
            .chart_bms(&mut self.disc, song, chart, self.details.as_ref())
            .map(String::into_bytes)
            .map_err(|_| ChartLoadError::NotAChart)
    }

    fn sounds(&mut self, chart: &BmsChart) -> (SampleBank, usize) {
        let mut bank = SampleBank::new();
        let song = &self.songs.songs[self.song];
        let slot = &song.charts[self.chart];
        let (wvb, bgm) = song.sound_files(slot.style, slot.difficulty);
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

    fn videos(&mut self, _: &BmsChart) -> HashMap<BmpId, VideoSource> {
        HashMap::new()
    }

    /// The song's loading card, on a 4:3 canvas.
    fn stage_image(&mut self) -> Option<StageArt> {
        let song = &self.songs.songs[self.song];
        let container = self.details.as_ref().and_then(|d| d.card).or(song.card)?;
        let card = self
            .songs
            .card_image(&mut self.disc, container)
            .ok()??
            .centered_4_3();
        let mut image = ImageBuffer::new(card.width, card.height, ColorRgba::new(0, 0, 0, 255));
        for (out, [r, g, b, a]) in image.pixels.iter_mut().zip(card.pixels) {
            *out = ColorRgba::new(r, g, b, a);
        }
        let bytes = image.encode_bmp_bytes();
        Some((image, Some(bytes)))
    }
}

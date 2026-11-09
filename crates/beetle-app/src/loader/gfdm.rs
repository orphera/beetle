//! A chart on a GuitarFreaks & DrumMania disc
//! (`disc:<image or drive>::gfdm/<id>/<part><difficulty>`): the chart is the
//! BMS `cs_disc::gfdm_bms` writes, its sounds the song's sample banks and
//! the music stream in its movie, which also plays as the background.

use std::collections::HashMap;
use std::fs::File;
use std::sync::{Arc, Mutex, OnceLock};

use beetle_audio::SampleBank;
use beetle_core::{BmpId, BmsChart, WavId};
use beetle_render::ImageBuffer;
use cs_disc::gfdm::{
    bank_samples, movie_audio, read_kits, song_path, Disc, Kits, MusicInfo, SongCharts, SongFile,
    MOVIE_AUDIO_BLOCK, MUSIC_LEAD, MUSIC_RATE,
};
use cs_disc::gfdm_bms::{to_bms, ChartRef, Song, Sound};
use cs_disc::wvb::Pcm;
use cs_disc::{SectorReader, Volume};

use super::disc::{song_gain, to_buffer};
use super::{progress, ChartLoadError, SongSource, StageArt, VideoSource};

/// A disc's index, song table and key sound tables.
pub(crate) struct GfdmDisc {
    pub disc: Disc,
    pub kits: Kits,
    pub songs: Vec<MusicInfo>,
}

impl GfdmDisc {
    /// `None` when the disc is not a GuitarFreaks & DrumMania disc.
    pub(crate) fn read(disc: &mut SectorReader<File>) -> Option<Self> {
        let volume = Volume::open(disc).ok()?;
        let index = Disc::open(disc, &volume).ok()??;
        let kits = read_kits(disc, &volume).ok()??;
        let songs = index
            .songs(disc)
            .ok()?
            .into_iter()
            .map(|s| s.info)
            .collect();
        Some(Self {
            disc: index,
            kits,
            songs,
        })
    }
}

/// The discs read this run, by image path (a drive is read each time).
static READ: OnceLock<Mutex<HashMap<String, Arc<GfdmDisc>>>> = OnceLock::new();

pub(super) struct GfdmSource {
    disc: SectorReader<File>,
    songs: Arc<GfdmDisc>,
    pub(super) charts: SongCharts,
    chart: ChartRef,
}

impl GfdmSource {
    /// Whether `key` names a chart of this kind.
    pub(super) fn is_key(key: &str) -> bool {
        ChartRef::parse(key).is_some()
    }

    pub(super) fn open(image: &str, key: &str) -> Result<Self, ChartLoadError> {
        let (id, chart) = ChartRef::parse(key).ok_or(ChartLoadError::NotFound)?;
        let mut disc = cs_disc::open(image).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ChartLoadError::NotFound,
            _ => ChartLoadError::Unreadable,
        })?;
        let is_file = std::path::Path::new(image).is_file();
        let read = READ.get_or_init(Default::default);
        let kept = is_file
            .then(|| read.lock().ok()?.get(image).cloned())
            .flatten();
        let songs = match kept {
            Some(s) => s,
            None => {
                let s = Arc::new(GfdmDisc::read(&mut disc).ok_or(ChartLoadError::NotFound)?);
                if is_file {
                    if let Ok(mut map) = read.lock() {
                        map.insert(image.to_string(), Arc::clone(&s));
                    }
                }
                s
            }
        };
        let info = songs
            .songs
            .iter()
            .find(|s| s.id == id)
            .ok_or(ChartLoadError::NotFound)?
            .clone();
        let charts = songs
            .disc
            .song_charts(&mut disc, &info)
            .map_err(|_| ChartLoadError::NotAChart)?;
        Ok(Self {
            disc,
            songs,
            charts,
            chart,
        })
    }

    fn file(&mut self, file: SongFile) -> Option<Vec<u8>> {
        let path = song_path(self.charts.info.id, file);
        self.songs.disc.read_path(&mut self.disc, &path).ok()?
    }

    /// Every sound the chart names, decoded at its bank volume.
    fn decode_sounds(&mut self, chart: &BmsChart) -> HashMap<WavId, Pcm> {
        let wanted: Vec<(WavId, Sound)> = chart
            .header
            .wav_table
            .iter()
            .filter_map(|(&id, name)| Some((id, Sound::parse(name)?)))
            .collect();
        let needs = |f: fn(&Sound) -> bool| wanted.iter().any(|(_, s)| f(s));
        let drums = needs(|s| matches!(s, Sound::Drum(_)))
            .then(|| self.file(SongFile::DrumSamples))
            .flatten()
            .unwrap_or_default();
        let guitar = needs(|s| matches!(s, Sound::Guitar { .. }))
            .then(|| self.file(SongFile::GuitarSamples))
            .flatten()
            .unwrap_or_default();
        let mut banks: HashMap<(bool, usize), Vec<cs_disc::gfdm::Sample>> = HashMap::new();
        let mut out = HashMap::new();
        for (id, sound) in wanted {
            let pcm = match sound {
                Sound::Music => self.music(),
                Sound::Silent => None,
                Sound::Drum(n) => key_sound(&drums, &mut banks, false, 0, n, 0),
                Sound::Guitar { bank, sound, ticks } => {
                    key_sound(&guitar, &mut banks, true, bank, sound, ticks)
                }
            };
            if let Some(pcm) = pcm {
                out.insert(id, pcm);
                progress::set_done(out.len());
            }
        }
        out
    }

    /// The movie's first music stream as 48000 Hz stereo, from chart time 0.
    fn music(&mut self) -> Option<Pcm> {
        let movie = self.file(SongFile::Movie)?;
        let (_, stream) = movie_audio(&movie).into_iter().next()?;
        let pcm = cs_disc::atrac3::Decoder::decode_all(MOVIE_AUDIO_BLOCK, &stream);
        let (left, right) = pcm
            .chunks_exact(2)
            .skip(MUSIC_LEAD)
            .map(|f| (f[0], f[1]))
            .unzip();
        Some(Pcm {
            rate: MUSIC_RATE,
            channels: vec![left, right],
        })
    }
}

/// How long a held guitar note takes to fade out after its length: the
/// tones' release (ADSR2 `0x9fc5`, linear rate 5) takes the SPU from full
/// volume to 0 in 64 samples at 48 kHz (`docs/specs/gfdm_ps2_disc.md` 4.1).
const RELEASE_SECONDS: f64 = 64.0 / 48000.0;

/// Sound `n` of the bank at `at` in sample file `file`, at its volume,
/// held for `ticks` (1/300 s) and then faded out; 0 plays it whole.
fn key_sound(
    file: &[u8],
    banks: &mut HashMap<(bool, usize), Vec<cs_disc::gfdm::Sample>>,
    guitar: bool,
    at: usize,
    n: u16,
    ticks: u16,
) -> Option<Pcm> {
    let bank = match banks.entry((guitar, at)) {
        std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
        std::collections::hash_map::Entry::Vacant(e) => e.insert(bank_samples(file, at)?),
    };
    let sample = bank.get(n as usize)?;
    let gain = sample.volume as i32;
    let rate = sample.rate();
    let mut mono: Vec<i16> = sample
        .decode(file)
        .into_iter()
        .map(|s| (s as i32 * gain / 127) as i16)
        .collect();
    if ticks > 0 {
        let hold = ticks as usize * rate as usize / cs_disc::gfdm::TICKS_PER_SECOND as usize;
        let release = ((RELEASE_SECONDS * rate as f64) as usize).max(1);
        mono.truncate(hold + release);
        for (k, s) in mono.iter_mut().skip(hold).enumerate() {
            *s = (*s as i64 * (release - k) as i64 / release as i64) as i16;
        }
    }
    Some(Pcm {
        rate,
        channels: vec![mono],
    })
}

impl SongSource for GfdmSource {
    fn chart_bytes(&mut self) -> Result<Vec<u8>, ChartLoadError> {
        let c = &self.charts;
        let song = Song {
            info: &c.info,
            drums: &c.drums,
            guitar: &c.guitar,
            kits: &self.songs.kits,
            lanes: &c.lanes,
        };
        to_bms(&song, self.chart)
            .map(String::into_bytes)
            .ok_or(ChartLoadError::NotAChart)
    }

    fn sounds(&mut self, chart: &BmsChart) -> (SampleBank, usize) {
        let decoded = self.decode_sounds(chart);
        let gain = song_gain(chart, &decoded);
        let mut bank = SampleBank::new();
        let loaded = decoded.len();
        for (id, pcm) in decoded {
            bank.insert(id, to_buffer(&pcm, gain));
        }
        (bank, loaded)
    }

    fn images(&mut self, _: &HashMap<BmpId, String>) -> HashMap<BmpId, ImageBuffer> {
        HashMap::new()
    }

    /// The song's movie (MPEG-2 program stream), from the start of the chart.
    fn videos(&mut self, _: &BmsChart) -> HashMap<BmpId, VideoSource> {
        let Some(bytes) = self.file(SongFile::Movie) else {
            return HashMap::new();
        };
        let source = VideoSource::Memory {
            bytes: bytes.into(),
            filename_hint: Some("bga.mpg".to_string()),
            display_aspect: None,
        };
        HashMap::from([(BmpId(1), source)])
    }

    fn stage_image(&mut self) -> Option<StageArt> {
        None
    }
}

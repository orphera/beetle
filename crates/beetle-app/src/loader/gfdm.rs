//! A chart on a GuitarFreaks & DrumMania disc
//! (`disc:<image or drive>::gfdm/<id>/<part><difficulty>`, or arranged for a
//! keyboard with `@<N>k+...`, `chart_arrange`): the chart is the
//! BMS `cs_disc::gfdm_bms` writes, its sounds the song's sample banks and
//! the music stream in its movie, which also plays as the background.

use std::collections::HashMap;
use std::fs::File;
use std::sync::{Arc, Mutex, OnceLock};

use beetle_audio::SampleBank;
use beetle_core::{BmpId, BmsChart, WavId};
use beetle_render::{ColorRgba, ImageBuffer};
use cs_disc::gfdm::{
    bank_samples, movie_audio, read_kits, song_path, Disc, Kits, MusicInfo, SongCharts, SongFile,
    MOVIE_AUDIO_BLOCK, MUSIC_LEAD, MUSIC_RATE,
};
use cs_disc::gfdm_bms::{to_bms, ChartRef, Song, Sound};
use cs_disc::ps2tex::Rgba;
use cs_disc::texp::{banner_name, Texp};
use cs_disc::wvb::Pcm;
use cs_disc::{SectorReader, Volume};

use super::disc::{song_gain, to_buffer};
use super::{progress, ChartLoadError, SongSource, StageArt, VideoSource};

/// A disc's index, song table and key sound tables.
pub(crate) struct GfdmDisc {
    pub disc: Disc,
    pub kits: Kits,
    pub songs: Vec<MusicInfo>,
    /// Which texture file holds each song banner (`IMG_CO{id}`), found the
    /// first time a banner is wanted.
    banners: OnceLock<HashMap<String, usize>>,
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
            banners: OnceLock::new(),
        })
    }

    /// Song `id`'s banner, `None` when the disc has none.
    fn banner(&self, disc: &mut SectorReader<File>, id: u32) -> Option<Rgba> {
        let files = self.banners.get_or_init(|| {
            let mut map = HashMap::new();
            for n in self.disc.texture_files(disc).unwrap_or_default() {
                let Ok(bytes) = self.disc.read(disc, n) else {
                    continue;
                };
                for r in Texp::parse(&bytes).map(|t| t.rects).unwrap_or_default() {
                    map.entry(r.name).or_insert(n);
                }
            }
            map
        });
        let name = banner_name(id);
        let bytes = self.disc.read(disc, *files.get(&name)?).ok()?;
        Texp::parse(&bytes)?.image(&name)
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

    /// The movie's music stream for the chart's part (`Part::music_stream`,
    /// else the first) as 48000 Hz stereo, from chart time 0.
    fn music(&mut self) -> Option<Pcm> {
        let movie = self.file(SongFile::Movie)?;
        let mut streams = movie_audio(&movie);
        let wanted = self.chart.part.music_stream();
        let at = streams.iter().position(|(n, _)| *n == wanted).unwrap_or(0);
        if streams.is_empty() {
            return None;
        }
        let (_, stream) = streams.swap_remove(at);
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
    let channels = match balance(sample.pan) {
        None => vec![mono],
        Some((l, r)) => {
            let side = |g: f64| mono.iter().map(|&s| (s as f64 * g) as i16).collect();
            vec![side(l), side(r)]
        }
    };
    Some(Pcm { rate, channels })
}

/// Left and right gains for a tone's pan in degrees (0 centre, 90 right,
/// 270 left, as SCREAM places sounds around the listener), or `None` for
/// the centre. A balance, not equal power: the near side stays at full
/// volume so a centred sound is as loud as before.
fn balance(pan: u16) -> Option<(f64, f64)> {
    let x = (pan as f64).to_radians().sin();
    if x.abs() < 1e-3 {
        return None;
    }
    Some(((1.0 - x).min(1.0), (1.0 + x).min(1.0)))
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

    /// Every chart of a song shows its banner.
    fn stage_key(&mut self) -> Option<String> {
        Some(format!("gfdm-banner:{}", self.charts.info.id))
    }

    /// The song's banner (200x80) on a black 4:3 canvas.
    fn stage_image(&mut self) -> Option<StageArt> {
        let mut banner = self.songs.banner(&mut self.disc, self.charts.info.id)?;
        // The GS's alpha: 0x80 is opaque.
        for p in &mut banner.pixels {
            p[3] = (p[3] as u16 * 2).min(255) as u8;
        }
        let picture = banner.letterboxed_4_3();
        let mut image =
            ImageBuffer::new(picture.width, picture.height, ColorRgba::new(0, 0, 0, 255));
        for (out, [r, g, b, a]) in image.pixels.iter_mut().zip(picture.pixels) {
            *out = ColorRgba::new(r, g, b, a);
        }
        Some(image)
    }
}

#[cfg(test)]
mod tests {
    use super::balance;

    #[test]
    fn pan_moves_a_drum_to_its_side() {
        assert_eq!(balance(0), None);
        assert_eq!(balance(180), None);
        let (l, r) = balance(90).unwrap();
        assert!(l.abs() < 1e-9 && r == 1.0);
        let (l, r) = balance(312).unwrap();
        assert!(l == 1.0 && r < 0.3);
    }
}

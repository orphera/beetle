use std::collections::HashMap;
#[cfg(test)]
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use beetle_audio::SampleBank;
use beetle_core::{
    parse_bms_with_seed, BmpId, BmsChart, ChartId, SongAddress, SongMetadata, TimingModel,
};
use beetle_render::{strings, ImageBuffer};

mod disc;
pub(crate) mod gfdm;
pub(crate) use disc::write_mix;
mod folder;
mod package;

/// Unified video data source: either a filesystem path or in-memory byte buffer.
#[derive(Debug, Clone)]
pub enum VideoSource {
    File(PathBuf),
    Memory {
        bytes: Arc<[u8]>,
        filename_hint: Option<String>,
        /// Width over height to show it at, where its frames' own is wrong.
        display_aspect: Option<f32>,
    },
}

/// Fully decoded song package data ready for gameplay.
pub type LoadedSongData = (
    BmsChart,
    TimingModel,
    SampleBank,
    HashMap<BmpId, ImageBuffer>,
    HashMap<BmpId, VideoSource>,
);

/// Why a chart could not be loaded. Such a song is never played.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartLoadError {
    /// The chart file, or its entry in a package, does not exist.
    NotFound,
    /// The file exists but could not be read.
    Unreadable,
    /// The file was read but is not a playable BMS chart.
    NotAChart,
}

pub type SongLoadResult = Result<LoadedSongData, ChartLoadError>;
pub type SongLoadReceiver = Receiver<SongLoadResult>;

const ARTWORK_CANDIDATE_FILENAMES: &[&str] = &[
    "stagefile.bmp",
    "stage.bmp",
    "banner.bmp",
    "title.bmp",
    "stagefile.png",
    "stage.png",
    "banner.png",
    "title.png",
    "stagefile.jpg",
    "stage.jpg",
    "banner.jpg",
    "title.jpg",
    "STAGEFILE.BMP",
    "STAGE.BMP",
    "BANNER.BMP",
    "TITLE.BMP",
];

/// What the song loader is doing, for the loading screen. One song loads at
/// a time, so the loader thread writes these and the render thread reads them.
pub mod progress {
    use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Stage {
        Chart,
        Sounds,
        Images,
        Videos,
    }

    static STAGE: AtomicU8 = AtomicU8::new(0);
    static DONE: AtomicUsize = AtomicUsize::new(0);
    static TOTAL: AtomicUsize = AtomicUsize::new(0);

    /// Starts a stage of `total` items (0 when it has no count).
    pub fn begin(stage: Stage, total: usize) {
        DONE.store(0, Ordering::Relaxed);
        TOTAL.store(total, Ordering::Relaxed);
        STAGE.store(stage as u8, Ordering::Relaxed);
    }

    pub fn set_done(done: usize) {
        DONE.store(done, Ordering::Relaxed);
    }

    /// The stage, items done and items in it.
    pub fn read() -> (Stage, usize, usize) {
        let stage = match STAGE.load(Ordering::Relaxed) {
            1 => Stage::Sounds,
            2 => Stage::Images,
            3 => Stage::Videos,
            _ => Stage::Chart,
        };
        (
            stage,
            DONE.load(Ordering::Relaxed),
            TOTAL.load(Ordering::Relaxed),
        )
    }
}

/// Movie files a chart's folder may hold without naming them, played on the
/// base BGA layer.
const FALLBACK_VIDEO_NAMES: &[&str] = &[
    "bga.mp4",
    "movie.mp4",
    "video.mp4",
    "bg.mp4",
    "pv.mp4",
    "bga.mpg",
    "movie.mpg",
    "video.mpg",
    "bg.mpg",
    "bga.wmv",
    "movie.wmv",
    "video.wmv",
    "bg.wmv",
    "bga.avi",
    "movie.avi",
    "video.avi",
    "bg.avi",
    "bga.webm",
    "movie.webm",
    "video.webm",
    "bg.webm",
    "bga.mkv",
    "movie.mkv",
    "video.mkv",
    "bg.mkv",
];

/// The BMP ids a fallback video plays as: the chart's base-layer ids, or `01`
/// when the chart has no base layer.
fn base_video_ids(chart: &BmsChart) -> Vec<BmpId> {
    let ids: Vec<BmpId> = chart
        .bga_events
        .iter()
        .filter(|ev| ev.channel == beetle_core::BgaChannel::Base)
        .map(|ev| ev.bmp_id)
        .collect();
    if ids.is_empty() {
        vec![BmpId(1)]
    } else {
        ids
    }
}

/// Stage artwork, and the encoded bytes to keep in the artwork cache (`None`
/// when it is not worth caching).
type StageArt = ImageBuffer;

/// Where a song's chart and files come from. The loader parses the chart and
/// times it the same way for every source; a source only reads.
trait SongSource {
    /// The chart file exactly as stored.
    fn chart_bytes(&mut self) -> Result<Vec<u8>, ChartLoadError>;
    /// Native chart, when the source has a format that does not need BMS text.
    fn native_chart(&mut self) -> Result<Option<BmsChart>, ChartLoadError> {
        Ok(None)
    }
    /// The chart's key sounds, counted in `progress` as they load, and how many loaded.
    fn sounds(&mut self, chart: &BmsChart) -> (SampleBank, usize);
    /// The images of `bmp_table`, counted in `progress` as they load.
    fn images(&mut self, bmp_table: &HashMap<BmpId, String>) -> HashMap<BmpId, ImageBuffer>;
    /// The chart's BGA videos.
    fn videos(&mut self, chart: &BmsChart) -> HashMap<BmpId, VideoSource>;
    /// The song's stage artwork.
    fn stage_image(&mut self) -> Option<StageArt>;
    /// Names where the stage art is read from: charts with the same key show
    /// the same art, decoded once (`load_stage`). `None`: read for each chart.
    fn stage_key(&mut self) -> Option<String> {
        None
    }
}

/// Opens the source a song's `file_path` names (`SongAddress`).
fn open_source(file_path: &str) -> Result<Box<dyn SongSource>, ChartLoadError> {
    match SongAddress::parse(file_path) {
        SongAddress::Folder { file } => Ok(Box::new(folder::FolderSource::new(file))),
        SongAddress::Package { bmsp, entry } => {
            Ok(Box::new(package::PackageSource::open(bmsp, entry)?))
        }
        SongAddress::Disc { image, chart } if gfdm::GfdmSource::is_key(chart) => {
            Ok(Box::new(gfdm::GfdmSource::open(image, chart)?))
        }
        SongAddress::Disc { image, chart } => Ok(Box::new(disc::DiscSource::open(image, chart)?)),
    }
}

/// Loads a song's stage artwork (the caller keeps it in memory).
/// The stage image the song select screen shows (its detail panel and jacket).
/// IIDX discs have none there: their loading art is slow to read from the disc,
/// so the select screen shows the text instead. Pop'n art is in the atlas and
/// stays. Gameplay still loads the full stage image (`load_stage_image`).
pub fn select_stage_image(song: &SongMetadata) -> StagePicture {
    if let SongAddress::Disc { image, chart } = SongAddress::parse(&song.file_path) {
        let popn = disc::DiscSource::open(image, chart)
            .map(|source| source.is_popn())
            .unwrap_or(false);
        if !popn {
            return StagePicture::none(song.id);
        }
    }
    load_stage(song)
}

/// A decoded stage picture, shared by every chart that shows it.
pub type Picture = Arc<ImageBuffer>;

/// A song's stage art: its identity (charts that show the same art share it,
/// and the GPU and backdrop caches key on it), the picture (`None`: the song
/// has none) and the blurred backdrop made from it.
#[derive(Clone)]
pub struct StagePicture {
    pub id: ChartId,
    pub image: Option<Picture>,
    pub backdrop: Option<Arc<Vec<u8>>>,
}

impl StagePicture {
    fn none(id: ChartId) -> Self {
        StagePicture {
            id,
            image: None,
            backdrop: None,
        }
    }
}

/// How many stage pictures `STAGE_PICTURES` keeps; the oldest go first.
/// A picture with its backdrop is about 1 MB.
const STAGE_PICTURES_KEPT: usize = 128;

/// Stage pictures decoded recently, by identity (`SongSource::stage_key`):
/// the art of a song's charts is decoded once, however many charts show it.
#[derive(Default)]
struct StagePictures {
    by_id: HashMap<ChartId, StagePicture>,
    order: std::collections::VecDeque<ChartId>,
}

impl StagePictures {
    fn get(&self, id: &ChartId) -> Option<StagePicture> {
        self.by_id.get(id).cloned()
    }

    fn insert(&mut self, pic: StagePicture) {
        if self.by_id.insert(pic.id, pic.clone()).is_none() {
            self.order.push_back(pic.id);
        }
        while self.order.len() > STAGE_PICTURES_KEPT {
            if let Some(old) = self.order.pop_front() {
                self.by_id.remove(&old);
            }
        }
    }
}

static STAGE_PICTURES: OnceLock<Mutex<StagePictures>> = OnceLock::new();

fn stage_pictures() -> std::sync::MutexGuard<'static, StagePictures> {
    STAGE_PICTURES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Drops the decoded stage pictures (a rescan may have changed the files).
pub fn forget_stage_pictures() {
    *stage_pictures() = StagePictures::default();
}

/// Loads a song's stage art, reusing the decode of any chart that shares it.
pub fn load_stage(song: &SongMetadata) -> StagePicture {
    let Ok(mut source) = open_source(&song.file_path) else {
        return StagePicture::none(song.id);
    };
    let shared = source
        .stage_key()
        .map(|key| ChartId::of_bytes(format!("stage:{key}").as_bytes()));
    if let Some(hit) = shared.and_then(|id| stage_pictures().get(&id)) {
        return hit;
    }
    let image = source.stage_image().map(Arc::new);
    let backdrop = image
        .as_deref()
        .and_then(beetle_render::backdrop::blur_backdrop)
        .map(Arc::new);
    let pic = StagePicture {
        id: shared.unwrap_or(song.id),
        image,
        backdrop,
    };
    if shared.is_some() {
        stage_pictures().insert(pic.clone());
    }
    pic
}

/// The song's stage picture, or `None` when it has none.
#[cfg(test)]
pub fn load_stage_image(song: &SongMetadata) -> Option<Picture> {
    load_stage(song).image
}

/// The chart bytes at a song address (a chart file, package entry or disc
/// chart), as the loader reads them.
pub fn read_chart_bytes(file_path: &str) -> Result<Vec<u8>, ChartLoadError> {
    open_source(file_path)?.chart_bytes()
}

/// `load_chart_and_audio_with_seed` with the library's fixed `#RANDOM` seed.
#[cfg(test)]
pub fn load_chart_and_audio(song: &SongMetadata) -> LoadedSongData {
    load_chart_and_audio_with_seed(
        song,
        beetle_core::bms::DEFAULT_RANDOM_SEED,
        true,
        &SoundPlaces::default(),
    )
    .expect("test chart should load")
}

/// Loads and parses the BMS chart file and pre-decodes the entire keysound samplebank and BGA images into memory.
/// `seed` decides the rolls of any `#RANDOM` sections in the chart.
/// A chart that cannot be read or parsed is an error: nothing is played for it.
pub fn load_chart_and_audio_with_seed(
    song: &SongMetadata,
    seed: u64,
    load_bga: bool,
    places: &SoundPlaces,
) -> Result<LoadedSongData, ChartLoadError> {
    load_chart_and_audio_arranged(song, seed, load_bga, places, None)
}

/// An arrangement to load a chart with (`arrange_chart`), and where the
/// arranged chart's own metadata goes once it is made.
pub struct ArrangeJob {
    pub arrange: chart_arrange::Arrange,
    pub meta: Sender<Option<SongMetadata>>,
}

/// `load_chart_and_audio_with_seed`, the chart laid out again by `arrange`
/// when there is one. A chart that cannot be arranged (double play) does
/// not load.
pub fn load_chart_and_audio_arranged(
    song: &SongMetadata,
    seed: u64,
    load_bga: bool,
    places: &SoundPlaces,
    arrange: Option<ArrangeJob>,
) -> Result<LoadedSongData, ChartLoadError> {
    progress::begin(progress::Stage::Chart, 0);
    let mut source = open_source(&song.file_path)?;
    let mut chart = match source.native_chart()? {
        Some(chart) => chart,
        None => {
            let bytes = source.chart_bytes()?;
            let content = beetle_core::decode_bms_text(&bytes);
            parse_playable_chart(&content, seed)?
        }
    };
    // The play and result screens name the difficulty as the game does.
    chart.header.difficulty_names = song.difficulty_names();
    let timing = TimingModel::from_chart(&chart);
    if let Some(job) = arrange {
        let is_pms = song.file_path.to_lowercase().ends_with(".pms");
        let mode = chart.detect_play_mode_with_hint(is_pms);
        let arranged = crate::arrange_chart::arrange_chart(
            &chart,
            &timing,
            mode,
            song.difficulty,
            &job.arrange,
        );
        let Some(arranged) = arranged else {
            let _ = job.meta.send(None);
            return Err(ChartLoadError::Unreadable);
        };
        chart = arranged;
        let _ = job
            .meta
            .send(Some(crate::arrange_chart::arranged_metadata(song, &chart)));
    }

    let started = std::time::Instant::now();
    progress::begin(progress::Stage::Sounds, chart.header.wav_table.len());
    let (soundbank, loaded) = load_sounds(source.as_mut(), &song.file_path, &chart, places);
    let sounds_ms = started.elapsed().as_millis();

    // BGA OFF: nothing to decode, so no image or video work.
    let no_bmps = HashMap::new();
    let bmp_table = if load_bga {
        &chart.header.bmp_table
    } else {
        &no_bmps
    };
    let bga_bank = source.images(bmp_table);

    progress::begin(progress::Stage::Videos, 0);
    let video_sources = if load_bga {
        source.videos(&chart)
    } else {
        HashMap::new()
    };
    for vs in video_sources.values() {
        if let VideoSource::File(p) = vs {
            println!("Detected BGA Video: '{}'", p.display());
        }
    }

    println!(
        "Loaded BMS: '{}' ({} / {} keysounds, {} BGA frames loaded, {} BGA videos) in {} ms (key sounds {} ms)",
        chart.header.title,
        loaded,
        chart.header.wav_table.len(),
        bga_bank.len(),
        video_sources.len(),
        started.elapsed().as_millis(),
        sounds_ms
    );
    Ok((chart, timing, soundbank, bga_bank, video_sources))
}

/// The song's other places to take key sounds from (B17, P2), as the
/// address of a chart in each.
#[derive(Debug, Clone, Default)]
pub struct SoundPlaces {
    /// The key sound set the player chose, when it is not the chart's own place.
    pub chosen: Option<String>,
    /// The rest, in the order to look in.
    pub others: Vec<String>,
    /// The player chose a set (maybe the own place). When not, a set clearly
    /// better than the own place's plays (`sound_sets::better_set`).
    pub decided: bool,
}

/// Whether `loaded` of `total` declared sounds is a whole set (90%, P2).
fn whole_set(loaded: usize, total: usize) -> bool {
    total > 0 && loaded * 10 >= total * 9
}

/// The chart's key sounds. When the chart's own place has a whole set and the
/// chosen set has one too, the chosen set plays and the own place fills its
/// gaps. Otherwise (a difference chart, or a set the chart cannot use) the
/// own place comes first. Sounds still missing are looked for in the chosen
/// set, then the other places.
fn load_sounds(
    source: &mut dyn SongSource,
    address: &str,
    chart: &BmsChart,
    places: &SoundPlaces,
) -> (SampleBank, usize) {
    let total = chart.header.wav_table.len();
    let (mut bank, _) = source.sounds(chart);
    let better = match (&places.chosen, places.decided, places.others.is_empty()) {
        (None, false, false) => {
            let names: Vec<String> = chart.header.wav_table.values().cloned().collect();
            crate::sound_sets::better_set(address, &places.others, &names)
        }
        _ => None,
    };
    let mut chosen = places.chosen.as_deref().or(better.as_deref());
    if let Some(address) = chosen.filter(|_| whole_set(bank.len(), total)) {
        chosen = None;
        if let Ok(mut set) = open_source(address) {
            progress::begin(progress::Stage::Sounds, total);
            let (mut swapped, _) = set.sounds(chart);
            if whole_set(swapped.len(), total) {
                swapped.fill_from(bank);
                bank = swapped;
            }
        }
    }
    for address in chosen
        .into_iter()
        .chain(places.others.iter().map(String::as_str))
    {
        let missing: HashMap<_, _> = chart
            .header
            .wav_table
            .iter()
            .filter(|(id, _)| !bank.contains_key(**id))
            .map(|(&id, name)| (id, name.clone()))
            .collect();
        if missing.is_empty() {
            break;
        }
        let Ok(mut other) = open_source(address) else {
            continue;
        };
        let mut part = BmsChart::default();
        part.header.wav_table = missing;
        bank.fill_from(other.sounds(&part).0);
    }
    let loaded = bank.len();
    (bank, loaded)
}

/// Parses chart text. Text that is not a BMS chart, and a chart with no notes at
/// all, have nothing to play, so both count as not a chart.
fn parse_playable_chart(content: &str, seed: u64) -> Result<BmsChart, ChartLoadError> {
    match parse_bms_with_seed(content, seed) {
        Ok(chart) if !chart.notes.is_empty() => Ok(chart),
        _ => Err(ChartLoadError::NotAChart),
    }
}

/// The name a song is shown by in messages: the chart file name, or the entry
/// name for a song inside a `.bmsp` package.
pub fn song_file_label(song: &SongMetadata) -> String {
    let entry = SongAddress::parse(&song.file_path).inner();
    Path::new(entry)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| entry.to_string())
}

/// The Error toast shown when a song's chart fails to load.
pub fn chart_load_failure_message(song: &SongMetadata) -> String {
    strings::fill(
        strings::TOAST_CHART_LOAD_FAILED(),
        &[&song_file_label(song)],
    )
}

/// Spawns a background thread to load and decode a song's chart, audio soundbank, BGA frames, and video sources.
pub fn spawn_background_song_loader(
    song: &SongMetadata,
    seed: u64,
    load_bga: bool,
    places: SoundPlaces,
    arrange: Option<ArrangeJob>,
) -> SongLoadReceiver {
    let song_clone = song.clone();
    let (tx, rx): (Sender<SongLoadResult>, SongLoadReceiver) = channel();

    thread::spawn(move || {
        let _ = tx.send(load_chart_and_audio_arranged(
            &song_clone,
            seed,
            load_bga,
            &places,
            arrange,
        ));
    });

    rx
}

/// Decoding threads a load in the background of a play may use.
const BACKGROUND_WORKERS: usize = 2;

/// Like `spawn_background_song_loader`, for a load that runs while a song
/// plays (a course's next song): it decodes on fewer threads, so the play
/// keeps its cores.
pub fn spawn_quiet_song_loader(
    song: &SongMetadata,
    seed: u64,
    load_bga: bool,
    places: SoundPlaces,
) -> SongLoadReceiver {
    let song_clone = song.clone();
    let (tx, rx): (Sender<SongLoadResult>, SongLoadReceiver) = channel();
    thread::spawn(move || {
        let result = beetle_audio::with_decode_workers(BACKGROUND_WORKERS, || {
            load_chart_and_audio_with_seed(&song_clone, seed, load_bga, &places)
        });
        let _ = tx.send(result);
    });
    rx
}

/// What the stage image loader hands back: the chart and its stage picture
/// (`load_stage`), with the backdrop already made.
pub type StageLoad = (ChartId, StagePicture);

/// Spawns a background thread that loads the stage images of `songs`, in
/// order, sending each as it is ready; the channel closes after the last.
/// The charts of one song mostly share one picture (`load_stage`), so the
/// rest cost little after the first.
pub fn spawn_background_stage_image_loader(songs: Vec<SongMetadata>) -> Receiver<StageLoad> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        for song in songs {
            if tx
                .send((song.picture_key(), select_stage_image(&song)))
                .is_err()
            {
                return;
            }
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_pictures_keep_the_newest_and_drop_the_oldest() {
        let mut kept = StagePictures::default();
        for n in 0..STAGE_PICTURES_KEPT as u64 + 2 {
            kept.insert(StagePicture::none(ChartId::synthetic(n)));
        }
        assert_eq!(kept.by_id.len(), STAGE_PICTURES_KEPT);
        assert!(kept.get(&ChartId::synthetic(0)).is_none());
        assert!(kept.get(&ChartId::synthetic(1)).is_none());
        assert!(kept.get(&ChartId::synthetic(2)).is_some());
        // Putting one in again does not count it twice.
        kept.insert(StagePicture::none(ChartId::synthetic(5)));
        assert_eq!(kept.order.len(), STAGE_PICTURES_KEPT);
    }

    #[test]
    fn the_charts_of_a_folder_share_the_art_their_headers_name() {
        let dir = std::env::temp_dir().join("beetle_stage_key_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let chart = |name: &str, stage: &str| {
            let path = dir.join(name);
            fs::write(
                &path,
                format!(
                    "#TITLE t
#STAGEFILE {stage}
#00111:01
"
                ),
            )
            .unwrap();
            let mut source = folder::FolderSource::new(path.to_str().unwrap());
            source.stage_key()
        };
        let normal = chart("n.bms", "stage.png");
        assert!(normal.is_some());
        assert_eq!(chart("h.bms", "stage.png"), normal);
        assert_ne!(chart("a.bms", "another.png"), normal);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_bmsp_in_memory_video_loading() {
        let pkg_path = "../../songs/bms.bmsp";
        if !std::path::Path::new(pkg_path).exists() {
            return;
        }

        let meta = SongMetadata {
            id: beetle_core::ChartId::synthetic(12345),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: 12345,
            file_path: format!("{}::roop_dotm_ogg/01_roop_dotm7SPN.bms", pkg_path),
            title: "roop_dotm".to_string(),
            subtitle: "".to_string(),
            artist: "roop".to_string(),
            genre: "".to_string(),
            bpm: 150.0,
            bpm_min: 150.0,
            bpm_max: 150.0,
            play_level: 7,
            notes_count: 100,
            play_mode: beetle_core::PlayMode::Keys7,
            difficulty: None,
        };

        let (_chart, _timing, _soundbank, _bga_bank, video_sources) = load_chart_and_audio(&meta);
        assert!(
            !video_sources.is_empty(),
            "Video sources should not be empty for roop_dotm BMSP"
        );

        for (bmp_id, source) in video_sources {
            eprintln!("[TEST BMSP] Found video source for BMP ID: {:?}", bmp_id);
            match source {
                VideoSource::Memory {
                    bytes,
                    filename_hint,
                    ..
                } => {
                    eprintln!(
                        "[TEST BMSP] Memory video size: {} bytes, hint: {:?}",
                        bytes.len(),
                        filename_hint
                    );
                    assert!(!bytes.is_empty());
                    let player = beetle_render::BgaVideoPlayer::open_from_memory(
                        &bytes,
                        filename_hint.as_deref(),
                    );
                    assert!(
                        player.is_some(),
                        "In-memory video player should open successfully"
                    );
                    let pl = player.unwrap();
                    assert!(
                        pl.current_frame().is_some(),
                        "Initial video frame should be decoded"
                    );
                    eprintln!(
                        "[TEST BMSP] Video dimension: {}x{}",
                        pl.width(),
                        pl.height()
                    );
                }
                VideoSource::File(p) => {
                    panic!(
                        "BMSP package video should be in-memory, but got file: {}",
                        p.display()
                    );
                }
            }
        }
    }

    #[test]
    fn test_turbo_bmsp_loading() {
        use beetle_render::skin::ColorRgba;

        let temp_dir = std::env::temp_dir().join(format!(
            "beetle_loader_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        // 1. Create a minimal BMS chart file
        let bms_content = "#TITLE Turbo Test\n#ARTIST Tester\n#BPM 130\n#STAGEFILE stage.bmp\n#WAV01 kick.wav\n#BMP01 bg.bmp\n#00111:01\n#00104:01\n";
        fs::write(temp_dir.join("test.bms"), bms_content).unwrap();

        // 2. Create minimal WAV file
        let wav_bytes = {
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 44100,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut cur = std::io::Cursor::new(Vec::new());
            {
                let mut w = hound::WavWriter::new(&mut cur, spec).unwrap();
                for &s in &[1000i16, 2000, 3000, -1000, -2000, 0] {
                    w.write_sample(s).unwrap();
                }
                w.finalize().unwrap();
            }
            cur.into_inner()
        };
        fs::write(temp_dir.join("kick.wav"), wav_bytes).unwrap();

        // 3. Create minimal BMP images
        let img1 = ImageBuffer::new(32, 32, ColorRgba::new(255, 0, 0, 255));
        let img2 = ImageBuffer::new(64, 48, ColorRgba::new(0, 255, 0, 255));
        fs::write(temp_dir.join("bg.bmp"), img1.encode_bmp_bytes()).unwrap();
        fs::write(temp_dir.join("stage.bmp"), img2.encode_bmp_bytes()).unwrap();

        // 4. Pack folder with Turbo profile
        let turbo_pkg_bytes = bms_package_manager::pack_bms_folder_profile(
            &temp_dir,
            None,
            bms_package_manager::PackProfile::Turbo,
        )
        .expect("turbo pack failed");
        let pkg_path = temp_dir.join("turbo.bmsp");
        fs::write(&pkg_path, turbo_pkg_bytes).unwrap();

        // 5. Test Turbo loading via load_chart_and_audio
        let meta = SongMetadata {
            id: beetle_core::ChartId::synthetic(99999),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: 99999,
            file_path: format!(
                "{}::test.bms",
                pkg_path.to_string_lossy().replace('\\', "/")
            ),
            title: "Turbo Test".to_string(),
            subtitle: "".to_string(),
            artist: "Tester".to_string(),
            genre: "".to_string(),
            bpm: 130.0,
            bpm_min: 130.0,
            bpm_max: 130.0,
            play_level: 5,
            notes_count: 1,
            play_mode: beetle_core::PlayMode::Keys7,
            difficulty: None,
        };

        let (chart, _timing, soundbank, bga_bank, video_sources) = load_chart_and_audio(&meta);
        assert_eq!(chart.header.title, "Turbo Test");
        assert_eq!(
            soundbank.len(),
            1,
            "Soundbank should load 1 keysound from Sound Atlas"
        );
        assert!(
            soundbank.get(beetle_core::WavId(1)).is_some(),
            "WAV01 should be loaded"
        );
        assert_eq!(
            bga_bank.len(),
            1,
            "BGA bank should load 1 frame from BGA Atlas"
        );
        assert!(
            bga_bank.contains_key(&beetle_core::BmpId(1)),
            "BMP01 should be loaded"
        );
        assert!(video_sources.is_empty());

        // 6. Test Stage image loading from Turbo BGA Atlas
        let stage_img = load_stage_image(&meta);
        assert!(
            stage_img.is_some(),
            "Stage image should load from BGA Atlas"
        );
        let stage = stage_img.unwrap();
        assert_eq!(stage.width, 64);
        assert_eq!(stage.height, 48);

        // 7. Test Classic fallback loading
        let classic_pkg_bytes = bms_package_manager::pack_bms_folder_profile(
            &temp_dir,
            None,
            bms_package_manager::PackProfile::Classic,
        )
        .expect("classic pack failed");
        let classic_pkg_path = temp_dir.join("classic.bmsp");
        fs::write(&classic_pkg_path, classic_pkg_bytes).unwrap();

        let classic_meta = SongMetadata {
            id: beetle_core::ChartId::synthetic(88888),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: 88888,
            file_path: format!(
                "{}::test.bms",
                classic_pkg_path.to_string_lossy().replace('\\', "/")
            ),
            title: "Turbo Test".to_string(),
            subtitle: "".to_string(),
            artist: "Tester".to_string(),
            genre: "".to_string(),
            bpm: 130.0,
            bpm_min: 130.0,
            bpm_max: 130.0,
            play_level: 5,
            notes_count: 1,
            play_mode: beetle_core::PlayMode::Keys7,
            difficulty: None,
        };

        let (c_chart, _c_timing, c_soundbank, c_bga_bank, _c_videos) =
            load_chart_and_audio(&classic_meta);
        assert_eq!(c_chart.header.title, "Turbo Test");
        assert_eq!(
            c_soundbank.len(),
            1,
            "Soundbank should load 1 keysound from Classic package"
        );
        assert_eq!(
            c_bga_bank.len(),
            1,
            "BGA bank should load 1 frame from Classic package"
        );

        let c_stage = load_stage_image(&classic_meta);
        assert!(
            c_stage.is_some(),
            "Stage image should load from Classic package"
        );

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_turbo_bmsp_bga_delta_loading() {
        use beetle_render::skin::ColorRgba;

        let temp_dir = std::env::temp_dir().join(format!(
            "beetle_loader_delta_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        // 1. Create a minimal BMS chart file with animation loop frames
        let bms_content = "#TITLE Delta Loader Test\n#ARTIST Tester\n#BPM 140\n#STAGEFILE stage.bmp\n#WAV01 kick.wav\n#BMP01 loop_01.bmp\n#BMP02 loop_02.bmp\n#00111:01\n#00104:01\n";
        fs::write(temp_dir.join("test.bms"), bms_content).unwrap();

        // 2. Create minimal WAV file
        let wav_bytes = {
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 44100,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut cur = std::io::Cursor::new(Vec::new());
            {
                let mut w = hound::WavWriter::new(&mut cur, spec).unwrap();
                for &s in &[1000i16, 2000, 3000, -1000, -2000, 0] {
                    w.write_sample(s).unwrap();
                }
                w.finalize().unwrap();
            }
            cur.into_inner()
        };
        fs::write(temp_dir.join("kick.wav"), wav_bytes).unwrap();

        // 3. Create minimal animation loop BMP images
        let img1 = ImageBuffer::new(32, 32, ColorRgba::new(255, 0, 0, 255));
        let mut img2 = img1.clone();
        img2.pixels[10 * 32 + 10] = ColorRgba::new(200, 200, 0, 255);
        let img_stage = ImageBuffer::new(64, 48, ColorRgba::new(0, 0, 255, 255));

        fs::write(temp_dir.join("loop_01.bmp"), img1.encode_bmp_bytes()).unwrap();
        fs::write(temp_dir.join("loop_02.bmp"), img2.encode_bmp_bytes()).unwrap();
        fs::write(temp_dir.join("stage.bmp"), img_stage.encode_bmp_bytes()).unwrap();

        // 4. Pack folder with Turbo profile (should auto-detect loop prefix and use BGA Delta)
        let turbo_pkg_bytes = bms_package_manager::pack_bms_folder_profile(
            &temp_dir,
            None,
            bms_package_manager::PackProfile::Turbo,
        )
        .expect("turbo pack failed");
        let pkg_path = temp_dir.join("delta_loader.bmsp");
        fs::write(&pkg_path, turbo_pkg_bytes).unwrap();

        // 5. Test Turbo loading via load_chart_and_audio
        let meta = SongMetadata {
            id: beetle_core::ChartId::synthetic(88888),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: 88888,
            file_path: format!(
                "{}::test.bms",
                pkg_path.to_string_lossy().replace('\\', "/")
            ),
            title: "Delta Loader Test".to_string(),
            subtitle: "".to_string(),
            artist: "Tester".to_string(),
            genre: "".to_string(),
            bpm: 140.0,
            bpm_min: 140.0,
            bpm_max: 140.0,
            play_level: 5,
            notes_count: 1,
            play_mode: beetle_core::PlayMode::Keys7,
            difficulty: None,
        };

        let (chart, _timing, soundbank, bga_bank, video_sources) = load_chart_and_audio(&meta);
        assert_eq!(chart.header.title, "Delta Loader Test");
        assert_eq!(soundbank.len(), 1, "Soundbank should load 1 keysound");
        assert_eq!(
            bga_bank.len(),
            2,
            "BGA bank should load 2 frames from BGA Delta bundle"
        );
        assert!(bga_bank.contains_key(&beetle_core::BmpId(1)));
        assert!(bga_bank.contains_key(&beetle_core::BmpId(2)));
        assert!(video_sources.is_empty());

        // 6. Test Stage image loading from Turbo BGA Delta bundle
        let stage_img = load_stage_image(&meta);
        assert!(
            stage_img.is_some(),
            "Stage image should load from BGA Delta bundle"
        );
        let stage = stage_img.unwrap();
        assert_eq!(stage.width, 64);
        assert_eq!(stage.height, 48);

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_large_song_dual_atlas_benchmark() {
        use beetle_render::skin::ColorRgba;
        use std::time::Instant;

        let temp_dir = std::env::temp_dir().join(format!(
            "beetle_benchmark_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        // 1. Generate 250 keysounds (full 2-digit hex space) and 50 BGA frames
        let sound_count = 250;
        let bga_count = 50;

        let mut bms_lines = vec![
            "#TITLE Benchmark Song".to_string(),
            "#ARTIST Large Ensemble".to_string(),
            "#BPM 175".to_string(),
            "#STAGEFILE stage.bmp".to_string(),
        ];

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let wav_bytes = {
            let mut cur = std::io::Cursor::new(Vec::new());
            {
                let mut w = hound::WavWriter::new(&mut cur, spec).unwrap();
                for s in 0..50 {
                    w.write_sample(((s % 20) * 1000) as i16).unwrap();
                }
                w.finalize().unwrap();
            }
            cur.into_inner()
        };

        for i in 1..=sound_count {
            let wav_name = format!("snd_{:03}.wav", i);
            fs::write(temp_dir.join(&wav_name), &wav_bytes).unwrap();
            let key = beetle_core::encode_base36(beetle_core::WavId(i as u16));
            bms_lines.push(format!("#WAV{} {}", key, wav_name));
        }

        let dummy_bmp =
            ImageBuffer::new(32, 32, ColorRgba::new(100, 150, 200, 255)).encode_bmp_bytes();
        for i in 1..=bga_count {
            let bmp_name = format!("bga_{:02}.bmp", i);
            fs::write(temp_dir.join(&bmp_name), &dummy_bmp).unwrap();
            let key = beetle_core::encode_base36(beetle_core::WavId(i as u16));
            bms_lines.push(format!("#BMP{} {}", key, bmp_name));
        }
        fs::write(temp_dir.join("stage.bmp"), &dummy_bmp).unwrap();

        // Add note events
        bms_lines.push("#00111:01".to_string());
        bms_lines.push("#00104:01".to_string());
        fs::write(temp_dir.join("bench.bms"), bms_lines.join("\n")).unwrap();

        // 2. Pack Classic vs Turbo
        let t0 = Instant::now();
        let classic_bytes = bms_package_manager::pack_bms_folder_profile(
            &temp_dir,
            None,
            bms_package_manager::PackProfile::Classic,
        )
        .expect("classic pack failed");
        let classic_pack_time = t0.elapsed();

        let t1 = Instant::now();
        let turbo_bytes = bms_package_manager::pack_bms_folder_profile(
            &temp_dir,
            None,
            bms_package_manager::PackProfile::Turbo,
        )
        .expect("turbo pack failed");
        let turbo_pack_time = t1.elapsed();

        let classic_path = temp_dir.join("classic.bmsp");
        let turbo_path = temp_dir.join("turbo.bmsp");
        fs::write(&classic_path, &classic_bytes).unwrap();
        fs::write(&turbo_path, &turbo_bytes).unwrap();

        println!("[BENCHMARK] Packages created:");
        println!(
            "  - Classic: {} bytes (pack time: {:?})",
            classic_bytes.len(),
            classic_pack_time
        );
        println!(
            "  - Turbo:   {} bytes (pack time: {:?})",
            turbo_bytes.len(),
            turbo_pack_time
        );

        // 3. Measure Loading Time: Classic vs Turbo
        let classic_meta = SongMetadata {
            id: beetle_core::ChartId::synthetic(10001),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: 10001,
            file_path: format!(
                "{}::bench.bms",
                classic_path.to_string_lossy().replace('\\', "/")
            ),
            title: "Benchmark Song".to_string(),
            subtitle: "".to_string(),
            artist: "Large Ensemble".to_string(),
            genre: "".to_string(),
            bpm: 175.0,
            bpm_min: 175.0,
            bpm_max: 175.0,
            play_level: 10,
            notes_count: 500,
            play_mode: beetle_core::PlayMode::Keys7,
            difficulty: None,
        };

        let turbo_meta = SongMetadata {
            id: beetle_core::ChartId::synthetic(10002),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: 10002,
            file_path: format!(
                "{}::bench.bms",
                turbo_path.to_string_lossy().replace('\\', "/")
            ),
            title: "Benchmark Song".to_string(),
            subtitle: "".to_string(),
            artist: "Large Ensemble".to_string(),
            genre: "".to_string(),
            bpm: 175.0,
            bpm_min: 175.0,
            bpm_max: 175.0,
            play_level: 10,
            notes_count: 250,
            play_mode: beetle_core::PlayMode::Keys7,
            difficulty: None,
        };

        // Classic load
        let start_classic = Instant::now();
        let (_c_chart, _c_timing, c_bank, c_bga, _) = load_chart_and_audio(&classic_meta);
        let classic_load_time = start_classic.elapsed();

        // Turbo load
        let start_turbo = Instant::now();
        let (_t_chart, _t_timing, t_bank, t_bga, _) = load_chart_and_audio(&turbo_meta);
        let turbo_load_time = start_turbo.elapsed();

        println!("[BENCHMARK] 250 Keysounds & 50 BGAs In-Game Load Time:");
        println!("  - Classic: {:?}", classic_load_time);
        println!(
            "  - Turbo:   {:?} (Speedup: {:.1}x)",
            turbo_load_time,
            classic_load_time.as_secs_f64() / turbo_load_time.as_secs_f64().max(0.0001)
        );

        assert_eq!(c_bank.len(), sound_count, "Classic loaded all keysounds");
        assert_eq!(t_bank.len(), sound_count, "Turbo loaded all keysounds");
        assert_eq!(c_bga.len(), bga_count, "Classic loaded all BGA frames");
        assert_eq!(t_bga.len(), bga_count, "Turbo loaded all BGA frames");

        assert!(
            turbo_load_time.as_millis() < 80,
            "Turbo loading should be under 80ms even in debug profile"
        );
        assert!(
            turbo_load_time < classic_load_time,
            "Turbo loading should be faster than Classic loading"
        );

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_decoupled_bga_companion() {
        use bms_package_manager::{
            pack_bms_folder_advanced_with_progress, BgaPackMode, PackOptions,
        };

        let temp_dir = std::env::temp_dir().join(format!(
            "beetle_bga_companion_load_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let src_dir = temp_dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();

        let bms_content = r#"
#TITLE Companion Test Song
#ARTIST Beetle Dev
#WAV01 01.wav
#BMP01 movie.mp4
#00111:01
#00104:01
"#;
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut cur = std::io::Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(&mut cur, spec).unwrap();
            w.write_sample(1000i16).unwrap();
            w.finalize().unwrap();
        }
        let valid_wav = cur.into_inner();

        fs::write(src_dir.join("main.bms"), bms_content).unwrap();
        fs::write(src_dir.join("01.wav"), &valid_wav).unwrap();
        let video_data = vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE];
        fs::write(src_dir.join("movie.mp4"), &video_data).unwrap();

        // 1. Pack with Split BGA mode (Turbo)
        let split_opts = PackOptions::turbo(BgaPackMode::Split);
        let out = pack_bms_folder_advanced_with_progress(
            &src_dir,
            None,
            split_opts,
            None,
            |_, _, _, _| {},
        )
        .expect("pack failed");

        let target_dir = temp_dir.join("installed");
        fs::create_dir_all(&target_dir).unwrap();
        let base_bmsp = target_dir.join("test_song.bmsp");
        let bga_bmsp = target_dir.join("test_song.bga.bmsp");
        fs::write(&base_bmsp, &out.base_package).unwrap();
        fs::write(&bga_bmsp, out.bga_package.unwrap()).unwrap();

        let song_meta = SongMetadata {
            id: beetle_core::ChartId::synthetic(77777),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: 77777,
            file_path: format!(
                "{}::main.bms",
                base_bmsp.to_string_lossy().replace('\\', "/")
            ),
            title: "Companion Test Song".to_string(),
            subtitle: "".to_string(),
            artist: "Beetle Dev".to_string(),
            genre: "".to_string(),
            bpm: 150.0,
            bpm_min: 150.0,
            bpm_max: 150.0,
            play_level: 5,
            notes_count: 1,
            play_mode: beetle_core::PlayMode::Keys7,
            difficulty: None,
        };

        // 2. Load with BGA companion present
        let (chart, _timing, soundbank, _bga_bank, video_sources) =
            load_chart_and_audio(&song_meta);
        assert_eq!(chart.header.title, "Companion Test Song");
        assert_eq!(soundbank.len(), 1);
        assert_eq!(
            video_sources.len(),
            1,
            "Should load video from companion package"
        );

        let vs = video_sources.values().next().unwrap();
        if let VideoSource::Memory {
            bytes,
            filename_hint,
            ..
        } = vs
        {
            assert_eq!(bytes.as_ref(), &video_data);
            assert_eq!(filename_hint.as_deref(), Some("movie.mp4"));
        } else {
            panic!("Expected VideoSource::Memory");
        }

        // 3. Delete companion package -> Verify Graceful Fallback
        fs::remove_file(&bga_bmsp).unwrap();
        let (_chart2, _timing2, soundbank2, _bga_bank2, video_sources2) =
            load_chart_and_audio(&song_meta);
        assert_eq!(soundbank2.len(), 1, "Audio still loads perfectly");
        assert!(
            video_sources2.is_empty(),
            "Video is gracefully omitted when companion is missing"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_turbo_package_with_duplicate_wav_ids() {
        use bms_package_manager::{
            pack_bms_folder_advanced_with_progress, BgaPackMode, PackOptions,
        };

        let temp_dir = std::env::temp_dir().join(format!(
            "beetle_test_dup_wav_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let song_dir = temp_dir.join("src_song");
        fs::create_dir_all(&song_dir).unwrap();

        // 1. Write BMS with multiple #WAV ids pointing to the same file
        let bms_content = "\
#TITLE Duplicate WAV Test
#ARTIST Beetle Dev
#BPM 130
#PLAYER 1
#WAV01 kick.wav
#WAV02 kick.wav
#WAV03 snare.wav
#00111:01020300
";
        fs::write(song_dir.join("main.bms"), bms_content).unwrap();

        // Create synthetic kick and snare WAV files
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut cur1 = std::io::Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(&mut cur1, spec).unwrap();
            w.write_sample(2000i16).unwrap();
            w.finalize().unwrap();
        }
        let kick_wav = cur1.into_inner();

        let mut cur2 = std::io::Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(&mut cur2, spec).unwrap();
            w.write_sample(1500i16).unwrap();
            w.finalize().unwrap();
        }
        let snare_wav = cur2.into_inner();

        fs::write(song_dir.join("kick.wav"), kick_wav).unwrap();
        fs::write(song_dir.join("snare.wav"), snare_wav).unwrap();

        // Pack as Turbo package
        let turbo_opts = PackOptions::turbo(BgaPackMode::Embed);
        let out = pack_bms_folder_advanced_with_progress(
            &song_dir,
            None,
            turbo_opts,
            None,
            |_, _, _, _| {},
        )
        .unwrap();

        let out_bmsp = temp_dir.join("dup_test.bmsp");
        fs::write(&out_bmsp, &out.base_package).unwrap();

        let song_meta = SongMetadata {
            id: beetle_core::ChartId::synthetic(88888),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: 88888,
            file_path: format!(
                "{}::main.bms",
                out_bmsp.to_string_lossy().replace('\\', "/")
            ),
            title: "Duplicate WAV Test".to_string(),
            subtitle: "".to_string(),
            artist: "Beetle Dev".to_string(),
            genre: "".to_string(),
            bpm: 130.0,
            bpm_min: 130.0,
            bpm_max: 130.0,
            play_level: 5,
            notes_count: 3,
            play_mode: beetle_core::PlayMode::Keys7,
            difficulty: None,
        };

        // Load chart and audio from the packed Turbo package
        let (_chart, _timing, soundbank, _bga_bank, _videos) = load_chart_and_audio(&song_meta);

        assert_eq!(
            soundbank.len(),
            3,
            "All 3 #WAV entries must be populated in soundbank"
        );
        assert!(
            soundbank.contains_key(beetle_core::WavId(1)),
            "WavId 1 (kick.wav) must exist"
        );
        assert!(
            soundbank.contains_key(beetle_core::WavId(2)),
            "WavId 2 (kick.wav duplicate) must exist"
        );
        assert!(
            soundbank.contains_key(beetle_core::WavId(3)),
            "WavId 3 (snare.wav) must exist"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_real_aliceblue_package() {
        println!("Current dir: {:?}", std::env::current_dir());
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
        let pkg_path = Path::new(&manifest_dir).join("../../songs/aliceblue.bmsp");
        println!(
            "Checking pkg_path: {:?} (exists: {})",
            pkg_path,
            pkg_path.exists()
        );
        if !pkg_path.exists() {
            return;
        }

        let song_meta = SongMetadata {
            id: beetle_core::ChartId::synthetic(11111),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: 11111,
            file_path: format!(
                "{}::alice7-1.bme",
                pkg_path.to_string_lossy().replace('\\', "/")
            ),
            title: "aliceblue (Radio Edit)".to_string(),
            subtitle: "".to_string(),
            artist: "nekodex".to_string(),
            genre: "".to_string(),
            bpm: 175.0,
            bpm_min: 175.0,
            bpm_max: 175.0,
            play_level: 10,
            notes_count: 1000,
            play_mode: beetle_core::PlayMode::Keys7,
            difficulty: None,
        };

        let (chart, _timing, soundbank, _bga_bank, _videos) = load_chart_and_audio(&song_meta);
        println!(
            "[VERIFICATION] alice7-1.bme wav_table count: {}, loaded in soundbank: {}",
            chart.header.wav_table.len(),
            soundbank.len()
        );
        assert!(
            soundbank.len() >= 193,
            "Should load at least 193 keysounds, got {}",
            soundbank.len()
        );
    }

    #[test]
    fn test_load_real_andromeda_package() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
        let pkg_path = Path::new(&manifest_dir).join("../../songs/星の器.bmsp");
        if !pkg_path.exists() {
            return;
        }

        let song_meta = SongMetadata {
            id: beetle_core::ChartId::synthetic(22222),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: 22222,
            file_path: format!(
                "{}::marisa(NORMAL7).bme",
                pkg_path.to_string_lossy().replace('\\', "/")
            ),
            title: "STAR OF ANDROMEDA".to_string(),
            subtitle: "".to_string(),
            artist: "D.Watt".to_string(),
            genre: "".to_string(),
            bpm: 170.0,
            bpm_min: 170.0,
            bpm_max: 170.0,
            play_level: 6,
            notes_count: 500,
            play_mode: beetle_core::PlayMode::Keys7,
            difficulty: None,
        };

        let (chart, _timing, soundbank, _bga_bank, _videos) = load_chart_and_audio(&song_meta);
        println!(
            "[VERIFICATION] marisa(NORMAL7).bme wav_table count: {}, loaded in soundbank: {}",
            chart.header.wav_table.len(),
            soundbank.len()
        );
        assert!(
            soundbank.len() >= chart.header.wav_table.len(),
            "All keysounds in wav_table should be loaded"
        );
        assert!(
            soundbank.contains_key(beetle_core::WavId(2)),
            "WavId 02 (bd.wav) must be loaded"
        );
        assert!(
            soundbank.contains_key(beetle_core::decode_base36(b'0', b'W').unwrap()),
            "WavId 0W (bass-01.wav) must be loaded"
        );
    }

    fn valid_chart_text() -> &'static [u8] {
        b"#TITLE t
#BPM 120
#WAV01 a.wav
#00111:01
"
    }

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("beetle-loader-{}-{}", tag, std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn meta_at(file_path: &str) -> SongMetadata {
        let mut meta =
            SongMetadata::from_bytes(file_path, valid_chart_text()).expect("valid chart");
        meta.file_path = file_path.to_string();
        meta
    }

    #[test]
    fn missing_chart_is_an_error_not_an_empty_play() {
        let path = scratch_dir("missing").join("gone.bms");
        let meta = meta_at(&path.to_string_lossy());
        let result = load_chart_and_audio_with_seed(&meta, 1, false, &SoundPlaces::default());
        assert_eq!(result.err(), Some(ChartLoadError::NotFound));
    }

    #[test]
    fn text_that_is_not_a_chart_is_an_error() {
        let path = scratch_dir("garbage").join("junk.bms");
        fs::write(
            &path,
            b"hello, this is not a chart
",
        )
        .unwrap();
        let meta = meta_at(&path.to_string_lossy());
        let result = load_chart_and_audio_with_seed(&meta, 1, false, &SoundPlaces::default());
        assert_eq!(result.err(), Some(ChartLoadError::NotAChart));
    }

    #[test]
    fn truncated_empty_chart_is_an_error() {
        let path = scratch_dir("empty").join("empty.bms");
        fs::write(&path, b"").unwrap();
        let meta = meta_at(&path.to_string_lossy());
        let result = load_chart_and_audio_with_seed(&meta, 1, false, &SoundPlaces::default());
        assert_eq!(result.err(), Some(ChartLoadError::NotAChart));
    }

    #[test]
    fn chart_without_notes_is_an_error() {
        let path = scratch_dir("nonotes").join("nonotes.bms");
        fs::write(
            &path,
            b"#TITLE t
#BPM 120
",
        )
        .unwrap();
        let meta = meta_at(&path.to_string_lossy());
        let result = load_chart_and_audio_with_seed(&meta, 1, false, &SoundPlaces::default());
        assert_eq!(result.err(), Some(ChartLoadError::NotAChart));
    }

    #[test]
    fn readable_chart_still_loads() {
        let path = scratch_dir("ok").join("ok.bms");
        fs::write(&path, valid_chart_text()).unwrap();
        let meta = meta_at(&path.to_string_lossy());
        let loaded = load_chart_and_audio_with_seed(&meta, 1, false, &SoundPlaces::default());
        assert!(loaded.is_ok());
    }

    #[test]
    fn failure_message_names_the_chart_file() {
        let meta = meta_at("songs/a/broken.bms");
        assert_eq!(
            chart_load_failure_message(&meta),
            "채보를 불러오지 못했어요: broken.bms"
        );
    }

    #[test]
    fn package_song_is_labelled_by_its_entry_name() {
        let meta = meta_at("pack.bmsp::dir/01.bms");
        assert_eq!(song_file_label(&meta), "01.bms");
    }

    #[test]
    fn a_disc_that_is_not_there_is_not_found() {
        let meta = meta_at("disc:no-such-disc.iso::000/sp_normal");
        assert_eq!(
            load_chart_and_audio_with_seed(&meta, 0, true, &SoundPlaces::default()).err(),
            Some(ChartLoadError::NotFound)
        );
    }

    /// With `CS_DISC_IIDX16_DISC2` set to the PREMIUM BEST image: ANISAKIS
    /// loads from the disc with every key sound and its BGM, as the scanner
    /// lists it.
    #[test]
    fn a_disc_chart_loads_with_its_sounds() {
        let Ok(image) = std::env::var("CS_DISC_IIDX16_DISC2") else {
            return;
        };
        let address = beetle_core::disc_address(&image, "000/sp_another");
        let mut disc = cs_disc::open(&image).unwrap();
        let volume = cs_disc::Volume::open(&mut disc).unwrap();
        let songs =
            cs_disc::iidx::Disc::open(&mut disc, &volume, cs_disc::Edition::PremiumBest).unwrap();
        let (song, chart) = songs.find("000/sp_another").unwrap();
        let bms = songs.chart_bms(&mut disc, song, chart).unwrap();
        let meta = SongMetadata::from_bytes(&address, bms.as_bytes()).unwrap();
        assert_eq!(meta.title, "ANISAKIS");

        let (chart, _, bank, bga, videos) =
            load_chart_and_audio_with_seed(&meta, 0, true, &SoundPlaces::default()).unwrap();
        assert_eq!(bank.len(), chart.header.wav_table.len());
        let bgm = chart
            .header
            .wav_table
            .iter()
            .find(|(_, n)| *n == "0001.wav")
            .unwrap()
            .0;
        let bgm = bank.get(*bgm).expect("the BGM cue has the BGM stream");
        assert!(bgm.duration_seconds() > 100.0, "{}", bgm.duration_seconds());
        assert!(bga.is_empty());
        // The song's background video, an MPEG program stream, as BMP 01.
        match videos.get(&BmpId(1)) {
            Some(VideoSource::Memory { bytes, .. }) => assert_eq!(&bytes[..4], &[0, 0, 1, 0xba]),
            other => panic!("no disc video: {other:?}"),
        }
        assert_eq!(song_file_label(&meta), "sp_another");
    }

    /// With `CS_DISC_IIDX14` set to the GOLD image or drive: Blind Justice
    /// loads from the disc with every key sound, its BGM and its video.
    #[test]
    fn a_gold_disc_chart_loads_with_its_sounds() {
        let Ok(image) = std::env::var("CS_DISC_IIDX14") else {
            return;
        };
        let address = beetle_core::disc_address(&image, "003/sp_another");
        let mut disc = cs_disc::open(&image).unwrap();
        let volume = cs_disc::Volume::open(&mut disc).unwrap();
        let songs = cs_disc::iidx::Disc::open(&mut disc, &volume, cs_disc::Edition::Gold).unwrap();
        let (song, chart) = songs.find("003/sp_another").unwrap();
        assert_eq!(chart.level, 11);
        let bms = songs.chart_bms(&mut disc, song, chart).unwrap();
        let meta = SongMetadata::from_bytes(&address, bms.as_bytes()).unwrap();
        assert_eq!(meta.title, "BLI");

        let (chart, _, bank, _, videos) =
            load_chart_and_audio_with_seed(&meta, 0, true, &SoundPlaces::default()).unwrap();
        assert_eq!(bank.len(), chart.header.wav_table.len());
        let bgm = chart
            .header
            .wav_table
            .iter()
            .find(|(_, n)| *n == "0001.wav")
            .unwrap()
            .0;
        let bgm = bank.get(*bgm).expect("the BGM cue has the BGM stream");
        assert!(bgm.duration_seconds() > 60.0, "{}", bgm.duration_seconds());
        match videos.get(&BmpId(1)) {
            Some(VideoSource::Memory { bytes, .. }) => assert_eq!(&bytes[..4], &[0, 0, 1, 0xba]),
            other => panic!("no disc video: {other:?}"),
        }
    }

    /// With `CS_DISC_POPN13` set to the pop'n music 13 image: ハードＰｆ's HYPER
    /// loads as a nine-button chart with its key sounds and its BGM.
    #[test]
    fn a_popn_disc_chart_loads_as_nine_buttons() {
        let Ok(image) = std::env::var("CS_DISC_POPN13") else {
            return;
        };
        let address = beetle_core::disc_address(&image, "000/sp_hyper");
        let mut disc = cs_disc::open(&image).unwrap();
        let volume = cs_disc::Volume::open(&mut disc).unwrap();
        let songs =
            cs_disc::iidx::Disc::open(&mut disc, &volume, cs_disc::Edition::Popn13).unwrap();
        let (song, chart) = songs.find("000/sp_hyper").unwrap();
        assert_eq!(chart.level, 34);
        let bms = songs.chart_bms(&mut disc, song, chart).unwrap();
        let meta = SongMetadata::from_bytes(&address, bms.as_bytes()).unwrap();
        assert_eq!(meta.title, "ハードＰｆ");

        let (chart, _, bank, _, _) =
            load_chart_and_audio_with_seed(&meta, 0, true, &SoundPlaces::default()).unwrap();
        use beetle_core::{Lane, PlayMode};
        assert_eq!(chart.header.declared_mode, Some(PlayMode::Keys9));
        for lane in [Lane::Key1, Lane::Key5, Lane::Key6, Lane::Key9] {
            assert!(chart.notes.iter().any(|n| n.lane == lane), "{lane:?}");
        }
        let bgm = chart
            .header
            .wav_table
            .iter()
            .find(|(_, n)| *n == "1294.wav")
            .unwrap()
            .0;
        let bgm = bank.get(*bgm).expect("the BGM cue has the BGM stream");
        assert!(bgm.duration_seconds() > 60.0, "{}", bgm.duration_seconds());
        // Every sound the chart names but 0 (silence) loads.
        assert_eq!(bank.len(), chart.header.wav_table.len() - 1);
    }

    /// With `CS_DISC_GFDM` set to the GuitarFreaks & DrumMania Masterpiece
    /// Silver image: song 404's drum chart loads as 6K and its guitar as 4K,
    /// each with every sound and the music, and notes land on the disc's times.
    /// So do the guitar arranged to 6K and the drums to 4K.
    #[test]
    fn a_gfdm_disc_chart_loads_with_its_sounds() {
        let Ok(image) = std::env::var("CS_DISC_GFDM") else {
            return;
        };
        use beetle_core::{Lane, PlayMode, TimingModel};
        for (key, mode, lanes) in [
            (
                "gfdm/404/dm3",
                PlayMode::Keys6,
                &[Lane::Key1, Lane::Key2, Lane::Key3, Lane::Key6, Lane::Key7][..],
            ),
            (
                "gfdm/404/gf3",
                PlayMode::Keys4,
                &[Lane::Key1, Lane::Key2, Lane::Key4][..],
            ),
            // Arranged (B29): the same notes on every lane of the mode.
            (
                "gfdm/404/gf3@6k+pitch+trill+bal",
                PlayMode::Keys6,
                &[
                    Lane::Key1,
                    Lane::Key2,
                    Lane::Key3,
                    Lane::Key5,
                    Lane::Key6,
                    Lane::Key7,
                ][..],
            ),
            (
                "gfdm/404/dm3@4k+home+stick+bal",
                PlayMode::Keys4,
                &[Lane::Key1, Lane::Key2, Lane::Key4, Lane::Key5][..],
            ),
        ] {
            let address = beetle_core::disc_address(&image, key);
            let mut source = gfdm::GfdmSource::open(&image, key).unwrap();
            let bms = source.chart_bytes().unwrap();
            let meta = SongMetadata::from_bytes(&address, &bms).unwrap();
            assert_eq!(meta.title, "CLASSIC PARTY 3");
            let (chart, _, bank, _, videos) =
                load_chart_and_audio_with_seed(&meta, 0, true, &SoundPlaces::default()).unwrap();
            assert_eq!(chart.header.declared_mode, Some(mode));
            for &lane in lanes {
                assert!(chart.notes.iter().any(|n| n.lane == lane), "{key} {lane:?}");
            }
            let music = chart
                .header
                .wav_table
                .iter()
                .find(|(_, n)| *n == "music.wav")
                .unwrap()
                .0;
            let music = bank.get(*music).expect("the music stream loads");
            assert!(
                music.duration_seconds() > 100.0,
                "{}",
                music.duration_seconds()
            );
            // Every sound but the silent chord frets loads.
            let silent = chart.header.wav_table.values().any(|n| n == "silent.wav");
            assert_eq!(bank.len(), chart.header.wav_table.len() - silent as usize);
            assert!(matches!(
                videos.get(&BmpId(1)),
                Some(VideoSource::Memory { .. })
            ));
            // The song's 200x80 banner on a 4:3 canvas, opaque.
            let stage = load_stage(&meta).image.expect("the banner loads");
            assert_eq!((stage.width, stage.height), (200, 150));
            assert!(stage.pixels.iter().all(|p| p.a == 255));

            // Note times against the disc's (1/300 s).
            let c = &source.charts;
            let track = if key.contains("/dm3") {
                c.drums
                    .tracks
                    .iter()
                    .find(|t| !t.is_meta && t.difficulty == 3)
            } else {
                c.guitar
                    .tracks
                    .iter()
                    .find(|t| !t.is_meta && t.difficulty == 3 && t.part == 1)
            }
            .unwrap();
            let mut disc_times: Vec<f64> = track
                .events
                .iter()
                .filter(|e| e.kind == 0)
                .map(|e| e.time as f64 / 300.0)
                .collect();
            disc_times.dedup();
            let timing = TimingModel::from_chart(&chart);
            let mut times: Vec<f64> = chart
                .notes
                .iter()
                .map(|n| timing.beat_to_time_seconds(n.measure, n.fraction))
                .collect();
            times.sort_by(f64::total_cmp);
            times.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
            assert_eq!(times.len(), disc_times.len(), "{key}");
            for (a, b) in times.iter().zip(&disc_times) {
                assert!((a - b).abs() < 0.003, "{key}: {a} vs {b}");
            }
        }
    }

    /// With `CS_DISC_IIDX14` set: THE LEAST 100sec slows down inside its
    /// bars at 20-21.5 s, and every note and background sound still plays at
    /// its frame time (it was up to 0.5 s late).
    #[test]
    fn a_gold_chart_slowing_inside_bars_keeps_its_frame_times() {
        let Ok(image) = std::env::var("CS_DISC_IIDX14") else {
            return;
        };
        let key = "083/sp_another";
        let mut disc = cs_disc::open(&image).unwrap();
        let volume = cs_disc::Volume::open(&mut disc).unwrap();
        let songs = cs_disc::iidx::Disc::open(&mut disc, &volume, cs_disc::Edition::Gold).unwrap();
        let (song, chart) = songs.find(key).unwrap();
        let ply = songs.chart_ply(&mut disc, chart).unwrap();
        let bms = songs.chart_bms(&mut disc, song, chart).unwrap();
        let meta =
            SongMetadata::from_bytes(&beetle_core::disc_address(&image, key), bms.as_bytes())
                .unwrap();
        let (chart, ..) =
            load_chart_and_audio_with_seed(&meta, 0, true, &SoundPlaces::default()).unwrap();
        let timing = beetle_core::timing::TimingModel::from_chart(&chart);
        let ms = |m: u32, f: f64| timing.beat_to_time_seconds(m, f) * 1000.0;
        let mut want: Vec<f64> = ply
            .events
            .iter()
            .filter(|e| {
                e.time > 0
                    && matches!(
                        e.command,
                        cs_disc::ply::Command::Note { .. } | cs_disc::ply::Command::Auto { .. }
                    )
            })
            .map(|e| ply.millis(e.time))
            .collect();
        let mut got: Vec<f64> = chart
            .notes
            .iter()
            .filter(|n| n.note_type != beetle_core::NoteType::LongNoteEnd)
            .map(|n| ms(n.measure, n.fraction))
            .chain(chart.bgm_notes.iter().map(|(m, f, _)| ms(*m, *f)))
            .filter(|t| *t > 1.0)
            .collect();
        want.sort_by(f64::total_cmp);
        got.sort_by(f64::total_cmp);
        assert_eq!(want.len(), got.len());
        for (w, g) in want.iter().zip(&got) {
            assert!((w - g).abs() < 15.0, "{w} ms plays at {g} ms");
        }
    }

    /// A chart whose place has a whole set plays the chosen set; a difference
    /// chart keeps its own sounds and takes the rest from the chosen set.
    #[test]
    fn the_chosen_key_sound_set_plays() {
        let root = std::env::temp_dir().join(format!("beetle-swap-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let wav = |rate: u32| {
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: rate,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut cur = std::io::Cursor::new(Vec::new());
            {
                let mut w = hound::WavWriter::new(&mut cur, spec).unwrap();
                for s in [1000i16, 2000, 3000] {
                    w.write_sample(s).unwrap();
                }
                w.finalize().unwrap();
            }
            cur.into_inner()
        };
        let place = |dir: &str, chart: &str, files: &[(&str, u32)]| {
            let d = root.join(dir);
            fs::create_dir_all(&d).unwrap();
            for (name, rate) in files {
                fs::write(d.join(name), wav(*rate)).unwrap();
            }
            let file = d.join("x.bms");
            fs::write(&file, chart).unwrap();
            SongMetadata::from_bytes(&file.to_string_lossy(), chart.as_bytes()).unwrap()
        };
        let both = "#TITLE t
#WAV01 a.wav
#WAV02 b.wav
#00111:0102
";
        let own = place("own", both, &[("a.wav", 44100), ("b.wav", 44100)]);
        let chosen = place("chosen", both, &[("a.wav", 22050), ("b.wav", 22050)]);
        let diff = place(
            "diff",
            "#TITLE t
#WAV01 a.wav
#WAV02 d.wav
#00111:0102
",
            &[("d.wav", 44100)],
        );
        let places = SoundPlaces {
            chosen: Some(chosen.file_path.clone()),
            others: vec![own.file_path.clone()],
            decided: true,
        };
        let rate = |song: &SongMetadata, id: u16| {
            let (_, _, bank, _, _) =
                load_chart_and_audio_with_seed(song, 0, false, &places).unwrap();
            bank.get(beetle_core::WavId(id)).unwrap().sample_rate
        };
        assert_eq!(rate(&own, 1), 22050);
        assert_eq!(rate(&own, 2), 22050);
        assert_eq!(rate(&diff, 1), 22050);
        assert_eq!(rate(&diff, 2), 44100);
        let _ = fs::remove_dir_all(&root);
    }
}

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use beetle_audio::SampleBank;
use beetle_core::{
    parse_bms_with_seed, BmpId, BmsChart, ChartId, SongAddress, SongMetadata, TimingModel,
};
use beetle_render::{strings, ImageBuffer};

mod disc;
mod folder;
mod package;

pub const ARTWORKS_CACHE_DIR: &str = ".cache/artworks";

/// Unified video data source: either a filesystem path or in-memory byte buffer.
#[derive(Debug, Clone)]
pub enum VideoSource {
    File(PathBuf),
    Memory {
        bytes: Arc<[u8]>,
        filename_hint: Option<String>,
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
type StageArt = (ImageBuffer, Option<Vec<u8>>);

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
}

/// Opens the source a song's `file_path` names (`SongAddress`).
fn open_source(file_path: &str) -> Result<Box<dyn SongSource>, ChartLoadError> {
    match SongAddress::parse(file_path) {
        SongAddress::Folder { file } => Ok(Box::new(folder::FolderSource::new(file))),
        SongAddress::Package { bmsp, entry } => {
            Ok(Box::new(package::PackageSource::open(bmsp, entry)?))
        }
        SongAddress::Disc { image, chart } => Ok(Box::new(disc::DiscSource::open(image, chart)?)),
    }
}

/// Writes an artwork cache file by renaming a temp file over it, so no reader
/// (or a second decoder of the same image) sees a half-written file. The temp
/// name is unique to each write, so two writers never share one. Failures are
/// ignored: the cache is only a speed-up.
fn write_cache_file(path: &Path, bytes: &[u8]) {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.{seq}.tmp", std::process::id()));
    let tmp = PathBuf::from(tmp);
    if fs::write(&tmp, bytes).is_err() || fs::rename(&tmp, path).is_err() {
        let _ = fs::remove_file(&tmp);
    }
}

/// Loads stage artwork image for a song if available on disk, cache, or inside a .bmsp package.
pub fn load_stage_image(song: &SongMetadata) -> Option<ImageBuffer> {
    // 1. Check persistent on-disk artwork cache first (fastest)
    let cache_dir = Path::new(ARTWORKS_CACHE_DIR);
    let cache_file = cache_dir.join(format!("{}.bmp", song.id.short()));
    if cache_file.exists() {
        if let Some(img) = ImageBuffer::load_from_file(&cache_file) {
            return Some(img);
        }
    }

    let (img, cache_bytes) = open_source(&song.file_path).ok()?.stage_image()?;
    if let Some(bytes) = cache_bytes {
        let _ = fs::create_dir_all(cache_dir);
        write_cache_file(&cache_file, &bytes);
    }
    Some(img)
}

/// The chart bytes at a song address (a chart file, package entry or disc
/// chart), as the loader reads them.
pub fn read_chart_bytes(file_path: &str) -> Result<Vec<u8>, ChartLoadError> {
    open_source(file_path)?.chart_bytes()
}

/// `load_chart_and_audio_with_seed` with the library's fixed `#RANDOM` seed.
#[cfg(test)]
pub fn load_chart_and_audio(song: &SongMetadata) -> LoadedSongData {
    load_chart_and_audio_with_seed(song, beetle_core::bms::DEFAULT_RANDOM_SEED, true)
        .expect("test chart should load")
}

/// Loads and parses the BMS chart file and pre-decodes the entire keysound samplebank and BGA images into memory.
/// `seed` decides the rolls of any `#RANDOM` sections in the chart.
/// A chart that cannot be read or parsed is an error: nothing is played for it.
pub fn load_chart_and_audio_with_seed(
    song: &SongMetadata,
    seed: u64,
    load_bga: bool,
) -> Result<LoadedSongData, ChartLoadError> {
    progress::begin(progress::Stage::Chart, 0);
    let mut source = open_source(&song.file_path)?;
    let chart = match source.native_chart()? {
        Some(chart) => chart,
        None => {
            let bytes = source.chart_bytes()?;
            let content = beetle_core::decode_bms_text(&bytes);
            parse_playable_chart(&content, seed)?
        }
    };
    let timing = TimingModel::from_chart(&chart);

    let started = std::time::Instant::now();
    progress::begin(progress::Stage::Sounds, chart.header.wav_table.len());
    let (soundbank, loaded) = source.sounds(&chart);
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
) -> SongLoadReceiver {
    let song_clone = song.clone();
    let (tx, rx): (Sender<SongLoadResult>, SongLoadReceiver) = channel();

    thread::spawn(move || {
        let _ = tx.send(load_chart_and_audio_with_seed(&song_clone, seed, load_bga));
    });

    rx
}

/// What the stage image loader hands back: the song, its artwork (`None`
/// when it has none), and the blurred backdrop picture of that artwork
/// (`beetle_render::backdrop`), computed here so the render thread only copies it.
pub type StageLoad = (ChartId, Option<ImageBuffer>, Option<Vec<u8>>);

/// Spawns a background thread to load a song's stage image without blocking the UI thread.
pub fn spawn_background_stage_image_loader(song: &SongMetadata) -> Receiver<StageLoad> {
    let id = song.id;
    let song_clone = song.clone();
    let (tx, rx) = channel();

    thread::spawn(move || {
        let img = load_stage_image(&song_clone);
        let backdrop = img
            .as_ref()
            .and_then(beetle_render::backdrop::blur_backdrop);
        let _ = tx.send((id, img, backdrop));
    });

    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_artwork_cache_write_replaces_the_file_whole_and_leaves_no_temp() {
        let dir = std::env::temp_dir().join(format!("beetle-art-cache-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.bmp");
        write_cache_file(&path, b"a longer first version");
        write_cache_file(&path, b"second");
        assert_eq!(fs::read(&path).unwrap(), b"second");
        let temps = fs::read_dir(&dir)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".tmp")
            })
            .count();
        assert_eq!(temps, 0);
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
        let result = load_chart_and_audio_with_seed(&meta, 1, false);
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
        let result = load_chart_and_audio_with_seed(&meta, 1, false);
        assert_eq!(result.err(), Some(ChartLoadError::NotAChart));
    }

    #[test]
    fn truncated_empty_chart_is_an_error() {
        let path = scratch_dir("empty").join("empty.bms");
        fs::write(&path, b"").unwrap();
        let meta = meta_at(&path.to_string_lossy());
        let result = load_chart_and_audio_with_seed(&meta, 1, false);
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
        let result = load_chart_and_audio_with_seed(&meta, 1, false);
        assert_eq!(result.err(), Some(ChartLoadError::NotAChart));
    }

    #[test]
    fn readable_chart_still_loads() {
        let path = scratch_dir("ok").join("ok.bms");
        fs::write(&path, valid_chart_text()).unwrap();
        let meta = meta_at(&path.to_string_lossy());
        let loaded = load_chart_and_audio_with_seed(&meta, 1, false);
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
            load_chart_and_audio_with_seed(&meta, 0, true).err(),
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
            cs_disc::iidx16::Disc::open(&mut disc, &volume, cs_disc::iidx16::Edition::PremiumBest)
                .unwrap();
        let (song, chart) = songs.find("000/sp_another").unwrap();
        let bms = songs.chart_bms(&mut disc, song, chart, None).unwrap();
        let meta = SongMetadata::from_bytes(&address, bms.as_bytes()).unwrap();
        assert_eq!(meta.title, "ANISAKIS");

        let (chart, _, bank, bga, videos) = load_chart_and_audio_with_seed(&meta, 0, true).unwrap();
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
}

//! Song-select audio preview.
//!
//! Plays the chart's `#PREVIEW` file (or a `preview*` audio file next to the
//! chart, as other players do) in a loop while a song is highlighted. The file
//! is decoded on a worker thread (INV-5) and played on its own short-lived
//! audio stream, so the gameplay engine and its clock are never involved.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use beetle_audio::{AudioCommand, AudioEngine, PcmBuffer, SampleBank};
use beetle_core::{decode_bms_text, parse_bms, SongMetadata, WavId};

const AUDIO_EXTS: [&str; 3] = ["ogg", "wav", "flac"];
/// How often the event loop checks on a loading or looping preview.
const POLL: Duration = Duration::from_millis(16);
const PREVIEW_VOLUME: f32 = 0.8;
const SAMPLE: WavId = WavId(1);

/// Decodes the preview audio of `song`, if it has one.
pub fn load_preview_pcm(song: &SongMetadata) -> Option<PcmBuffer> {
    if song.file_path == ":demo:" {
        return None;
    }
    let pcm = match song.file_path.split_once("::") {
        Some((pkg, entry)) => load_from_package(pkg, entry)?,
        None => load_from_dir(Path::new(&song.file_path))?,
    };
    (pcm.duration_seconds() >= 0.1).then_some(pcm)
}

fn load_from_dir(chart: &Path) -> Option<PcmBuffer> {
    let dir = chart.parent()?;
    let declared = fs::read(chart)
        .ok()
        .and_then(|bytes| parse_bms(&decode_bms_text(&bytes)).ok())
        .map(|c| c.header.preview)
        .filter(|name| !name.is_empty());
    let path = declared
        .and_then(|name| resolve_in_dir(dir, &name))
        .or_else(|| find_preview_file(dir))?;
    SampleBank::load_audio_file(path).ok()
}

/// `name` as written in the chart, tolerating case and a different audio extension.
fn resolve_in_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    let name = name.replace('\\', "/");
    let exact = dir.join(&name);
    if exact.is_file() {
        return Some(exact);
    }
    let wanted = Path::new(&name).file_name()?.to_string_lossy().to_lowercase();
    let wanted_stem = Path::new(&wanted).file_stem()?.to_string_lossy().into_owned();
    let mut by_stem = None;
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let file = entry.file_name().to_string_lossy().to_lowercase();
        if file == wanted {
            return Some(entry.path());
        }
        if by_stem.is_none() && is_audio(&file) && stem_of(&file) == wanted_stem {
            by_stem = Some(entry.path());
        }
    }
    by_stem
}

/// Fallback when the chart declares no `#PREVIEW`: the first `preview*` audio file.
fn find_preview_file(dir: &Path) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| {
            let file = e.file_name().to_string_lossy().to_lowercase();
            file.starts_with("preview") && is_audio(&file)
        })
        .map(|e| e.path())
        .collect();
    found.sort();
    found.into_iter().next()
}

fn load_from_package(pkg_path: &str, entry: &str) -> Option<PcmBuffer> {
    let mut pkg = bms_package::PackageReader::open_file(pkg_path).ok()?;
    let base = Path::new(entry).parent().unwrap_or_else(|| Path::new("")).to_string_lossy().into_owned();
    let declared = pkg
        .read_entry(entry)
        .ok()
        .and_then(|bytes| parse_bms(&decode_bms_text(&bytes)).ok())
        .map(|c| c.header.preview)
        .filter(|name| !name.is_empty());
    let path = declared.and_then(|name| pkg.find_entry_path(&base, &name)).or_else(|| {
        let prefix = if base.is_empty() { String::new() } else { format!("{}/", base.to_lowercase()) };
        pkg.entries()
            .iter()
            .map(|e| e.path.clone())
            .filter(|p| {
                let lower = p.to_lowercase();
                let file = lower.rsplit('/').next().unwrap_or(&lower);
                lower.starts_with(&prefix) && lower[prefix.len()..] == *file && file.starts_with("preview") && is_audio(file)
            })
            .min()
    })?;
    SampleBank::load_audio_from_bytes(&pkg.read_entry(&path).ok()?).ok()
}

fn is_audio(file: &str) -> bool {
    Path::new(file)
        .extension()
        .is_some_and(|e| AUDIO_EXTS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

fn stem_of(file: &str) -> String {
    Path::new(file).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// One preview sample looping on its own audio stream.
struct PreviewPlayer {
    engine: AudioEngine,
    hash: u64,
    length: Duration,
    started: Instant,
}

impl PreviewPlayer {
    fn start(hash: u64, pcm: PcmBuffer, volume: f32) -> Option<Self> {
        let length = Duration::from_secs_f64(pcm.duration_seconds());
        let mut bank = SampleBank::new();
        bank.insert(SAMPLE, pcm);
        let mut engine = AudioEngine::new(bank).ok()?;
        let _ = engine.set_master_volume(volume);
        let mut player = Self { engine, hash, length, started: Instant::now() };
        player.restart();
        Some(player)
    }

    fn restart(&mut self) {
        let _ = self.engine.send_command(AudioCommand::PlaySample {
            sample_id: SAMPLE,
            volume: PREVIEW_VOLUME,
            pan: 0.0,
        });
        self.started = Instant::now();
    }

    /// Loops the sample; returns how long until the next restart is due.
    fn tick(&mut self) -> Duration {
        let elapsed = self.started.elapsed();
        if elapsed >= self.length {
            self.restart();
            return self.length;
        }
        self.length - elapsed
    }
}

/// Preview state owned by the app: what is loading, what is playing.
#[derive(Default)]
pub struct Preview {
    player: Option<PreviewPlayer>,
    receiver: Option<Receiver<(u64, Option<PcmBuffer>)>>,
    /// Song whose preview was last requested (so it is not requested twice).
    requested: Option<u64>,
}

impl Preview {
    /// Drives the preview for the highlighted song. `settled` is true once the
    /// cursor has rested. Returns how soon the event loop should wake again.
    pub fn update(&mut self, selected: Option<&SongMetadata>, settled: bool, volume: f32) -> Option<Duration> {
        let want = selected.map(|s| s.hash);

        // Moving the cursor cuts the old preview immediately.
        if self.player.as_ref().is_some_and(|p| Some(p.hash) != want) {
            self.player = None;
        }
        if self.requested != want {
            self.requested = None;
            self.receiver = None;
        }

        if let Some(rx) = &self.receiver {
            match rx.try_recv() {
                Ok((hash, pcm)) => {
                    self.receiver = None;
                    if Some(hash) == want {
                        self.player = pcm.and_then(|pcm| PreviewPlayer::start(hash, pcm, volume));
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return Some(POLL),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.receiver = None,
            }
        }

        if settled && self.requested.is_none() {
            if let Some(song) = selected {
                self.requested = Some(song.hash);
                let (hash, song) = (song.hash, song.clone());
                let (tx, rx) = channel();
                thread::spawn(move || {
                    let _ = tx.send((hash, load_preview_pcm(&song)));
                });
                self.receiver = Some(rx);
                return Some(POLL);
            }
        }

        self.player.as_mut().map(PreviewPlayer::tick)
    }

    /// Stops playback and forgets the request (leaving song select).
    pub fn stop(&mut self) {
        self.player = None;
        self.receiver = None;
        self.requested = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("beetle_preview_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_wav(path: &Path, seconds: f32) {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for _ in 0..(44100.0 * seconds) as usize * 2 {
            w.write_sample(1000i16).unwrap();
        }
        w.finalize().unwrap();
    }

    fn song(chart: &Path) -> SongMetadata {
        SongMetadata {
            hash: 1,
            file_path: chart.to_string_lossy().into_owned(),
            title: String::new(),
            subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            bpm: 120.0,
            bpm_min: 120.0,
            bpm_max: 120.0,
            play_level: 1,
            notes_count: 0,
            play_mode: beetle_core::PlayMode::Keys7,
        }
    }

    #[test]
    fn declared_preview_resolves_case_and_extension_insensitively() {
        let dir = temp_dir("declared");
        fs::write(dir.join("a.bms"), "#TITLE T\n#PREVIEW Pre_View.OGG\n#00111:01\n").unwrap();
        write_wav(&dir.join("pre_view.wav"), 0.5);
        let pcm = load_preview_pcm(&song(&dir.join("a.bms"))).expect("preview found");
        assert!((pcm.duration_seconds() - 0.5).abs() < 0.01);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn falls_back_to_preview_file_next_to_the_chart() {
        let dir = temp_dir("fallback");
        fs::write(dir.join("a.bms"), "#TITLE T\n#00111:01\n").unwrap();
        write_wav(&dir.join("preview_auto.wav"), 0.5);
        write_wav(&dir.join("other.wav"), 1.0);
        let pcm = load_preview_pcm(&song(&dir.join("a.bms"))).expect("preview found");
        assert!((pcm.duration_seconds() - 0.5).abs() < 0.01);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn no_preview_means_none() {
        let dir = temp_dir("none");
        fs::write(dir.join("a.bms"), "#TITLE T\n#00111:01\n").unwrap();
        write_wav(&dir.join("other.wav"), 1.0);
        assert!(load_preview_pcm(&song(&dir.join("a.bms"))).is_none());
        let _ = fs::remove_dir_all(dir);
    }
}

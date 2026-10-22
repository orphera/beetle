//! Song-select audio preview.
//!
//! Song-select audio preview.
//!
//! Plays the chart's `#PREVIEW` file (or a `preview*` audio file next to the
//! chart, as other players do) in a loop while a song is highlighted. The file
//! is decoded on a worker thread (INV-5) and played on its own short-lived
//! audio stream, so the gameplay engine and its clock are never involved.
//!
//! The preview starts only once the highlighted song has stayed put for
//! `PREVIEW_DELAY`, so scrolling past songs decodes nothing. It fades in, fades
//! out when the selection moves, and ducks out quickly when a song starts. The
//! fades run on the app thread by sending `SetMasterVolume` through the engine's
//! lock-free queue; the audio callback itself does no fading work.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use beetle_audio::{AudioCommand, AudioEngine, PcmBuffer, SampleBank};
use beetle_core::{decode_bms_text, parse_bms, ChartId, SongMetadata, WavId};

const AUDIO_EXTS: [&str; 3] = ["ogg", "wav", "flac"];
/// How often the event loop checks on a loading, looping or fading preview.
const POLL: Duration = Duration::from_millis(16);
/// Redraw interval for the animated "PREVIEW" badge (about 30 fps).
const BADGE_FRAME: Duration = Duration::from_millis(33);
/// How long the highlighted song must stay put before its preview starts.
const PREVIEW_DELAY: Duration = Duration::from_millis(250);
const FADE_IN: Duration = Duration::from_millis(300);
/// Fade when the selection moves away from a playing preview.
const FADE_OUT: Duration = Duration::from_millis(150);
/// Quick fade when a song starts (and when leaving song select).
const DUCK: Duration = Duration::from_millis(120);
const PREVIEW_VOLUME: f32 = 0.8;
const SAMPLE: WavId = WavId(1);

/// A straight-line gain ramp from `from` to `to` over `over`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fade {
    from: f32,
    to: f32,
    over: Duration,
}

impl Fade {
    pub fn new(from: f32, to: f32, over: Duration) -> Self {
        Self { from, to, over }
    }

    /// The gain `elapsed` into the ramp; holds `to` once the ramp is over.
    pub fn gain_at(&self, elapsed: Duration) -> f32 {
        if elapsed >= self.over {
            return self.to;
        }
        let t = elapsed.as_secs_f32() / self.over.as_secs_f32();
        self.from + (self.to - self.from) * t
    }

    pub fn finished(&self, elapsed: Duration) -> bool {
        elapsed >= self.over
    }
}

/// Tracks how long the highlighted song has stayed the same, on a clock the
/// caller supplies (so tests need no sleeping).
#[derive(Default)]
struct Settle {
    id: Option<ChartId>,
    since: Option<Instant>,
}

impl Settle {
    /// Records what is highlighted at `now`; returns how long it has been unchanged.
    fn stable_for(&mut self, id: Option<ChartId>, now: Instant) -> Duration {
        let since = match self.since {
            Some(since) if self.id == id => since,
            _ => {
                self.id = id;
                self.since = Some(now);
                now
            }
        };
        now.saturating_duration_since(since)
    }
}

/// The sooner of two optional wake-up times.
fn soonest(a: Option<Duration>, b: Option<Duration>) -> Option<Duration> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// Decodes the preview audio of `song`, if it has one.
pub fn load_preview_pcm(song: &SongMetadata) -> Option<PcmBuffer> {
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
    let wanted = Path::new(&name)
        .file_name()?
        .to_string_lossy()
        .to_lowercase();
    let wanted_stem = Path::new(&wanted)
        .file_stem()?
        .to_string_lossy()
        .into_owned();
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
    let base = Path::new(entry)
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .to_string_lossy()
        .into_owned();
    let declared = pkg
        .read_entry(entry)
        .ok()
        .and_then(|bytes| parse_bms(&decode_bms_text(&bytes)).ok())
        .map(|c| c.header.preview)
        .filter(|name| !name.is_empty());
    let path = declared
        .and_then(|name| pkg.find_entry_path(&base, &name))
        .or_else(|| {
            let prefix = if base.is_empty() {
                String::new()
            } else {
                format!("{}/", base.to_lowercase())
            };
            pkg.entries()
                .iter()
                .map(|e| e.path.clone())
                .filter(|p| {
                    let lower = p.to_lowercase();
                    let file = lower.rsplit('/').next().unwrap_or(&lower);
                    lower.starts_with(&prefix)
                        && lower[prefix.len()..] == *file
                        && file.starts_with("preview")
                        && is_audio(file)
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
    Path::new(file)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// One preview sample looping on its own audio stream, with its fade.
struct PreviewPlayer {
    engine: AudioEngine,
    id: ChartId,
    length: Duration,
    started: Instant,
    volume: f32,
    fade: Fade,
    fade_started: Instant,
    /// Master level last sent to the engine (volume times fade gain).
    sent_level: Option<f32>,
}

impl PreviewPlayer {
    /// Opens the stream silent and starts the loop; the fade-in begins at `now`.
    fn start(id: ChartId, pcm: PcmBuffer, volume: f32, now: Instant) -> Option<Self> {
        let length = Duration::from_secs_f64(pcm.duration_seconds());
        let mut bank = SampleBank::new();
        bank.insert(SAMPLE, pcm);
        let engine = AudioEngine::new(bank).ok()?;
        let mut player = Self {
            engine,
            id,
            length,
            started: now,
            volume,
            fade: Fade::new(0.0, 1.0, FADE_IN),
            fade_started: now,
            sent_level: None,
        };
        player.apply_level(now);
        player.restart(now);
        Some(player)
    }

    fn restart(&mut self, now: Instant) {
        let _ = self.engine.send_command(AudioCommand::PlaySample {
            sample_id: SAMPLE,
            volume: PREVIEW_VOLUME,
            pan: 0.0,
        });
        self.started = now;
    }

    /// Loops the sample; returns how long until the next restart is due.
    fn tick(&mut self, now: Instant) -> Duration {
        let elapsed = now.saturating_duration_since(self.started);
        if elapsed >= self.length {
            self.restart(now);
            return self.length;
        }
        self.length - elapsed
    }

    fn fade_elapsed(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.fade_started)
    }

    /// Starts a ramp from the current gain to `to`.
    fn fade_to(&mut self, to: f32, over: Duration, now: Instant) {
        let from = self.fade.gain_at(self.fade_elapsed(now));
        self.fade = Fade::new(from, to, over);
        self.fade_started = now;
    }

    fn fading(&self, now: Instant) -> bool {
        !self.fade.finished(self.fade_elapsed(now))
    }

    /// Sends the engine master level if it changed (a lock-free command).
    fn apply_level(&mut self, now: Instant) {
        let level = self.volume * self.fade.gain_at(self.fade_elapsed(now));
        if self.sent_level != Some(level) {
            let _ = self.engine.set_master_volume(level);
            self.sent_level = Some(level);
        }
    }
}

/// Preview state owned by the app: what is loading, what is playing.
#[derive(Default)]
pub struct Preview {
    settle: Settle,
    player: Option<PreviewPlayer>,
    /// Players fading out (after a selection change, a duck, or leaving song
    /// select). Each is dropped, and its stream closed, once its ramp ends.
    fading_out: Vec<PreviewPlayer>,
    receiver: Option<Receiver<(ChartId, Option<PcmBuffer>)>>,
    /// Song whose preview was last requested (so it is not requested twice).
    requested: Option<ChartId>,
}

impl Preview {
    /// Drives the preview for the highlighted song at `now`. Returns how soon
    /// the event loop should wake again.
    pub fn update(
        &mut self,
        selected: Option<&SongMetadata>,
        now: Instant,
        volume: f32,
    ) -> Option<Duration> {
        let want = selected.map(|s| s.id);
        let stable = self.settle.stable_for(want, now);

        // Moving the cursor fades the old preview out and forgets its request.
        if self.player.as_ref().is_some_and(|p| Some(p.id) != want) {
            self.retire(FADE_OUT, now);
        }
        if self.requested != want {
            self.requested = None;
            self.receiver = None;
        }
        if let Some(p) = &mut self.player {
            p.volume = volume;
        }

        let mut wake = None;
        if let Some(rx) = &self.receiver {
            match rx.try_recv() {
                Ok((id, pcm)) => {
                    self.receiver = None;
                    if Some(id) == want {
                        self.player =
                            pcm.and_then(|pcm| PreviewPlayer::start(id, pcm, volume, now));
                    }
                }
                Err(TryRecvError::Empty) => wake = Some(POLL),
                Err(TryRecvError::Disconnected) => self.receiver = None,
            }
        }

        // Decode only once the song has rested, so a scroll spawns nothing.
        if let Some(song) = selected.filter(|_| self.requested.is_none()) {
            if stable >= PREVIEW_DELAY {
                self.requested = Some(song.id);
                let (id, song) = (song.id, song.clone());
                let (tx, rx) = channel();
                thread::spawn(move || {
                    let _ = tx.send((id, load_preview_pcm(&song)));
                });
                self.receiver = Some(rx);
                wake = soonest(wake, Some(POLL));
            } else {
                wake = soonest(wake, Some(PREVIEW_DELAY - stable));
            }
        }

        soonest(wake, self.drive(now))
    }

    /// Moves the playing preview (if any) into a fade-out and forgets the request.
    fn retire(&mut self, over: Duration, now: Instant) {
        if let Some(mut player) = self.player.take() {
            player.fade_to(0.0, over, now);
            self.fading_out.push(player);
        }
        self.requested = None;
        self.receiver = None;
    }

    /// Advances the loop and every fade; drops the players whose fade-out ended.
    fn drive(&mut self, now: Instant) -> Option<Duration> {
        self.fading_out.retain_mut(|p| {
            p.apply_level(now);
            p.fading(now)
        });
        let mut wake = (!self.fading_out.is_empty()).then_some(POLL);
        if let Some(p) = &mut self.player {
            let next_loop = p.tick(now);
            p.apply_level(now);
            // Fades need a fine step; otherwise wake for the badge.
            let frame = if p.fading(now) { POLL } else { BADGE_FRAME };
            wake = soonest(wake, Some(next_loop.min(frame)));
        }
        wake
    }

    /// Ducks the preview out quickly; called when a song starts, so it does not
    /// cut hard as the game enters Loading.
    pub fn duck(&mut self) {
        self.retire(DUCK, Instant::now());
    }

    /// Leaving song select: ducks the preview and keeps the fade running on
    /// later frames until its stream is closed. The fade is ~120 ms, so the
    /// device is free well within the Loading screen's first moments.
    pub fn release(&mut self, now: Instant) {
        if self.player.is_some() || self.requested.is_some() {
            self.retire(DUCK, now);
        }
        self.drive(now);
    }

    /// True while a faded-out preview is still ramping (the loop must keep ticking).
    pub fn is_fading(&self) -> bool {
        !self.fading_out.is_empty()
    }

    /// Seconds the highlighted song's preview has been playing, if it is.
    pub fn playing_for(&self) -> Option<f32> {
        self.player
            .as_ref()
            .map(|p| p.started.elapsed().as_secs_f32())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("beetle_preview_{name}_{}", std::process::id()));
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
            id: beetle_core::ChartId::synthetic(1),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: 1,
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
            difficulty: None,
        }
    }

    #[test]
    fn declared_preview_resolves_case_and_extension_insensitively() {
        let dir = temp_dir("declared");
        fs::write(
            dir.join("a.bms"),
            "#TITLE T\n#PREVIEW Pre_View.OGG\n#00111:01\n",
        )
        .unwrap();
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

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-5,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn fade_in_runs_from_silent_to_full() {
        let fade = Fade::new(0.0, 1.0, ms(300));
        assert_near(fade.gain_at(ms(0)), 0.0);
        assert_near(fade.gain_at(ms(150)), 0.5);
        assert_near(fade.gain_at(ms(300)), 1.0);
        assert_near(fade.gain_at(ms(900)), 1.0);
        assert!(!fade.finished(ms(299)));
        assert!(fade.finished(ms(300)));
    }

    #[test]
    fn fade_out_runs_from_current_gain_to_silent() {
        // A fade-out begun at gain 0.8 (mid-fade-in) ramps down to 0.
        let fade = Fade::new(0.8, 0.0, ms(150));
        assert_near(fade.gain_at(ms(0)), 0.8);
        assert_near(fade.gain_at(ms(75)), 0.4);
        assert_near(fade.gain_at(ms(150)), 0.0);
        assert_near(fade.gain_at(ms(400)), 0.0);
    }

    #[test]
    fn a_zero_length_fade_is_already_at_its_target() {
        let fade = Fade::new(0.0, 1.0, Duration::ZERO);
        assert_near(fade.gain_at(Duration::ZERO), 1.0);
        assert!(fade.finished(Duration::ZERO));
    }

    #[test]
    fn a_song_starts_only_after_it_has_rested_for_the_delay() {
        let t0 = Instant::now();
        let song = Some(beetle_core::ChartId::synthetic(1));
        let mut settle = Settle::default();
        assert_eq!(settle.stable_for(song, t0), Duration::ZERO);
        // Just short of the delay: still waiting.
        assert!(settle.stable_for(song, t0 + ms(249)) < PREVIEW_DELAY);
        // At the delay: the decode may begin.
        assert!(settle.stable_for(song, t0 + PREVIEW_DELAY) >= PREVIEW_DELAY);
    }

    #[test]
    fn scrolling_past_songs_never_settles() {
        let t0 = Instant::now();
        let mut settle = Settle::default();
        // A new song every 100 ms, as during a fast scroll.
        for step in 0..10u64 {
            let id = Some(beetle_core::ChartId::synthetic(step));
            let stable = settle.stable_for(id, t0 + ms(step * 100));
            assert!(
                stable < PREVIEW_DELAY,
                "step {step} settled after {stable:?}"
            );
        }
    }

    #[test]
    fn changing_the_song_restarts_the_wait() {
        let t0 = Instant::now();
        let a = Some(beetle_core::ChartId::synthetic(1));
        let b = Some(beetle_core::ChartId::synthetic(2));
        let mut settle = Settle::default();
        assert_eq!(settle.stable_for(a, t0), Duration::ZERO);
        assert!(settle.stable_for(a, t0 + ms(600)) >= PREVIEW_DELAY);
        // Moving to B resets the clock, even though A had long settled.
        assert_eq!(settle.stable_for(b, t0 + ms(700)), Duration::ZERO);
        assert!(settle.stable_for(b, t0 + ms(900)) < PREVIEW_DELAY);
        assert!(settle.stable_for(b, t0 + ms(700) + PREVIEW_DELAY) >= PREVIEW_DELAY);
    }

    #[test]
    fn soonest_picks_the_earlier_wake_up() {
        assert_eq!(soonest(None, None), None);
        assert_eq!(soonest(Some(ms(30)), None), Some(ms(30)));
        assert_eq!(soonest(None, Some(ms(30))), Some(ms(30)));
        assert_eq!(soonest(Some(ms(30)), Some(ms(16))), Some(ms(16)));
    }
}

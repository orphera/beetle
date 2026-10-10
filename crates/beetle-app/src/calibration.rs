//! Judge offset calibration (Settings > 판정 > 판정 오프셋 측정).
//!
//! A metronome clicks at a fixed tempo on its own audio engine. The player
//! presses a key on each click, and the press is timed on the audio clock
//! (INV-1). The suggestion is the negated mean error: a player who presses
//! 20 ms late gets -20 ms, which moves the judged press back onto the click.
//!
//! Click times are nominal, `start + k * BEAT_SECONDS`, on the audio clock.
//! Each click is handed to the mixer ahead and starts on its exact frame,
//! the way gameplay starts its BGM notes, so the offset this measures is the
//! one that lines up presses with the music the player hears. Presses are
//! timed when the key arrived (`raw_input`), as in gameplay.
//!
//! The pure parts (schedule, matching, outlier rejection, statistics, the
//! suggestion) take times as plain numbers and do not need audio. `Session`
//! is the thin part that owns the engine.

use std::time::Instant;

use beetle_audio::{AudioEngine, PcmBuffer, SampleBank};
use beetle_core::WavId;

use crate::options_table::{JUDGE_OFFSET_MAX_MS, JUDGE_OFFSET_STEP_MS};
use crate::state::SCHEDULE_AHEAD_SECONDS;

/// Tempo of the metronome.
pub const TEMPO_BPM: f64 = 120.0;
pub const BEAT_SECONDS: f64 = 60.0 / TEMPO_BPM;
/// Clicks before the first counted press (the count-in, shown as 준비).
pub const COUNT_IN_CLICKS: u64 = 4;
/// Counted presses that end the test.
pub const REQUIRED_PRESSES: usize = 16;
/// A press further than this from every click is ignored.
pub const MATCH_WINDOW_MS: f64 = 150.0;
/// Seconds between starting a test and its first click.
pub const LEAD_IN_SECONDS: f64 = 1.0;
/// How long the beat indicator stays lit after its click.
const PULSE_SECONDS: f64 = 0.2;
/// Presses this far from the median, in robust sigmas, are left out.
const OUTLIER_SIGMAS: f64 = 3.0;
/// Fewer presses than this are never trimmed.
const MIN_FOR_OUTLIERS: usize = 4;
/// Floor on the robust spread. The audio clock moves in device-buffer steps
/// (about 10 to 20 ms), so presses can land in two neighbouring steps; the
/// floor keeps the minority step from being trimmed as an outlier.
const MIN_SPREAD_MS: f64 = 10.0;

const CLICK_SAMPLE: WavId = WavId(1);
const CLICK_RATE: u32 = 44_100;
const CLICK_SECONDS: f64 = 0.03;
const CLICK_HZ: f64 = 1_200.0;
const CLICK_DECAY_SECONDS: f64 = 0.006;
const CLICK_LEVEL: f64 = 0.8;

/// The time the judge compares with a note: the audio clock moved by the
/// judge offset. Gameplay uses this for every key press (see
/// `handlers/gameplay.rs` and `gameplay.rs`), and the calibration test below
/// checks the suggestion through the real judge.
pub fn judged_time(audio_seconds: f64, judge_offset_ms: f64) -> f64 {
    audio_seconds + judge_offset_ms / 1000.0
}

/// Nominal audio time of click `k`.
pub fn click_time(start: f64, k: u64) -> f64 {
    start + k as f64 * BEAT_SECONDS
}

/// The judge offset that cancels a mean press error: a press `mean_error_ms`
/// late is judged on time with `-mean_error_ms`. Rounded to the Settings
/// step and kept inside the Settings range; `+ 0.0` turns -0.0 into 0.0.
pub fn suggested_offset(mean_error_ms: f64) -> f64 {
    let stepped = (-mean_error_ms / JUDGE_OFFSET_STEP_MS).round() * JUDGE_OFFSET_STEP_MS;
    (stepped.clamp(-JUDGE_OFFSET_MAX_MS, JUDGE_OFFSET_MAX_MS)) + 0.0
}

/// Robust statistics of the presses (ms, late positive).
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    /// Mean of the presses that count.
    pub mean_ms: f64,
    /// Population standard deviation of the presses that count.
    pub std_ms: f64,
    /// Per press, in order: whether it counts in the mean.
    pub kept: Vec<bool>,
}

fn median(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// Mean and spread of `samples` after trimming outliers. A press is an
/// outlier when it is more than `OUTLIER_SIGMAS` robust sigmas (1.4826 x the
/// median absolute deviation) from the median. Trimming needs at least
/// `MIN_FOR_OUTLIERS` presses. `None` for no presses.
pub fn summarize(samples: &[f64]) -> Option<Summary> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let med = median(&sorted);
    let mut devs: Vec<f64> = sorted.iter().map(|x| (x - med).abs()).collect();
    devs.sort_by(f64::total_cmp);
    let sigma = (1.4826 * median(&devs)).max(MIN_SPREAD_MS);
    let trim = samples.len() >= MIN_FOR_OUTLIERS;

    let kept: Vec<bool> = samples
        .iter()
        .map(|x| !trim || (x - med).abs() <= OUTLIER_SIGMAS * sigma)
        .collect();
    let counted: Vec<f64> = samples
        .iter()
        .zip(&kept)
        .filter(|(_, &k)| k)
        .map(|(x, _)| *x)
        .collect();
    let n = counted.len() as f64;
    let mean = counted.iter().sum::<f64>() / n;
    let var = counted.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n;
    Some(Summary {
        mean_ms: mean,
        std_ms: var.sqrt(),
        kept,
    })
}

/// The schedule and the presses of one test. Times are audio seconds.
#[derive(Debug, Clone)]
pub struct Calibration {
    /// Audio time of click 0.
    start: f64,
    /// The next click that has not sounded yet.
    next_click: u64,
    /// The click the last counted press went with: one press per click.
    last_hit_click: Option<u64>,
    /// Counted presses in order, error in ms (late positive).
    presses: Vec<f64>,
}

impl Calibration {
    pub fn new(start: f64) -> Self {
        Self {
            start,
            next_click: 0,
            last_hit_click: None,
            presses: Vec::new(),
        }
    }

    pub fn is_done(&self) -> bool {
        self.presses.len() >= REQUIRED_PRESSES
    }

    /// The next click that is due at `now`, if any. Call in a loop until it
    /// returns `None`; each click is returned once. Nothing is due once the
    /// test is done.
    pub fn next_click(&mut self, now: f64) -> Option<u64> {
        if self.is_done() || click_time(self.start, self.next_click) > now {
            return None;
        }
        let k = self.next_click;
        self.next_click += 1;
        Some(k)
    }

    /// A key press at audio time `now`. Returns its error in ms when it
    /// counts. A press counts when the nearest click is one after the
    /// count-in, it is within `MATCH_WINDOW_MS` of that click, and that click
    /// has no counted press yet.
    pub fn press(&mut self, now: f64) -> Option<f64> {
        if self.is_done() {
            return None;
        }
        let nearest = ((now - self.start) / BEAT_SECONDS).round();
        if nearest < COUNT_IN_CLICKS as f64 {
            return None;
        }
        let k = nearest as u64;
        let error_ms = (now - click_time(self.start, k)) * 1000.0;
        if error_ms.abs() > MATCH_WINDOW_MS || self.last_hit_click == Some(k) {
            return None;
        }
        self.last_hit_click = Some(k);
        self.presses.push(error_ms);
        Some(error_ms)
    }

    pub fn presses(&self) -> &[f64] {
        &self.presses
    }

    /// Index of the beat `now` is in (negative before the first click).
    pub fn beat_index(&self, now: f64) -> i64 {
        ((now - self.start) / BEAT_SECONDS).floor() as i64
    }

    /// Whether `now` is still in the count-in.
    pub fn counting_in(&self, now: f64) -> bool {
        self.beat_index(now) < COUNT_IN_CLICKS as i64
    }

    /// Beat indicator strength at `now`: 1 on a click, fading to 0 over
    /// `PULSE_SECONDS`. Zero before the first click.
    pub fn pulse(&self, now: f64) -> f32 {
        let since = now - self.start;
        if since < 0.0 {
            return 0.0;
        }
        let elapsed = since - self.beat_index(now) as f64 * BEAT_SECONDS;
        (1.0 - elapsed / PULSE_SECONDS).clamp(0.0, 1.0) as f32
    }

    pub fn summary(&self) -> Option<Summary> {
        summarize(&self.presses)
    }

    /// The suggested judge offset, once there are presses to average.
    pub fn suggestion(&self) -> Option<f64> {
        self.summary().map(|s| suggested_offset(s.mean_ms))
    }
}

/// A short decaying tone, generated in code (INV-3: decoded and registered
/// before the test starts, never during it).
pub fn click_pcm() -> PcmBuffer {
    let frames = (CLICK_RATE as f64 * CLICK_SECONDS) as usize;
    let mut out = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        let t = i as f64 / CLICK_RATE as f64;
        let envelope = (-t / CLICK_DECAY_SECONDS).exp();
        let v = ((std::f64::consts::TAU * CLICK_HZ * t).sin() * envelope * CLICK_LEVEL) as f32;
        out.push(v);
        out.push(v);
    }
    PcmBuffer::new(CLICK_RATE, out)
}

/// A running or finished test, with the engine that plays its clicks. The
/// engine is dropped with the session, which stops the stream.
pub struct Session {
    test: Calibration,
    /// None when no output device could be opened.
    engine: Option<AudioEngine>,
}

impl Session {
    /// Opens an engine holding the click and starts a test. Without an output
    /// device the session has no clicks and reports `has_audio() == false`.
    pub fn open(master_volume: f32) -> Self {
        let mut bank = SampleBank::new();
        bank.insert(CLICK_SAMPLE, click_pcm());
        let mut engine = AudioEngine::new(bank).ok();
        if let Some(audio) = &mut engine {
            let _ = audio.set_master_volume(master_volume);
        }
        let start = clock_of(&engine) + LEAD_IN_SECONDS;
        Self {
            test: Calibration::new(start),
            engine,
        }
    }

    pub fn has_audio(&self) -> bool {
        self.engine.is_some()
    }

    /// The test is running: audio is there and the metronome is still going.
    pub fn is_running(&self) -> bool {
        self.has_audio() && !self.test.is_done()
    }

    /// The test has its presses and the suggestion can be applied.
    pub fn is_done(&self) -> bool {
        self.has_audio() && self.test.is_done()
    }

    pub fn test(&self) -> &Calibration {
        &self.test
    }

    /// Audio clock now, in seconds.
    pub fn now(&self) -> f64 {
        clock_of(&self.engine)
    }

    /// Starts the test again from the top. The engine keeps running.
    pub fn restart(&mut self) {
        self.stop_voices();
        self.test = Calibration::new(self.now() + LEAD_IN_SECONDS);
    }

    /// Hands the mixer every click due within `SCHEDULE_AHEAD_SECONDS`, to
    /// start on its exact frame. Called once per frame while the test runs,
    /// the same way gameplay schedules its BGM notes.
    pub fn tick(&mut self) {
        let until = self.now() + SCHEDULE_AHEAD_SECONDS;
        while let Some(k) = self.test.next_click(until) {
            if let Some(audio) = &mut self.engine {
                let _ = audio.play_at(CLICK_SAMPLE, click_time(self.test.start, k));
            }
        }
    }

    /// A key press, timed on the audio clock now. Stops the clicks when the
    /// test completes.
    pub fn press(&mut self) -> Option<f64> {
        let now = self.now();
        self.press_on_clock(now)
    }

    /// A key press that arrived at `at` (see `raw_input`).
    pub fn press_at(&mut self, at: Instant) -> Option<f64> {
        let t = self
            .engine
            .as_ref()
            .map(|a| a.clock().time_at(at))
            .unwrap_or(0.0);
        self.press_on_clock(t)
    }

    fn press_on_clock(&mut self, now: f64) -> Option<f64> {
        let counted = self.test.press(now);
        if self.test.is_done() {
            self.stop_voices();
        }
        counted
    }

    fn stop_voices(&mut self) {
        if let Some(audio) = &mut self.engine {
            let _ = audio.stop_all();
        }
    }
}

fn clock_of(engine: &Option<AudioEngine>) -> f64 {
    engine
        .as_ref()
        .map(|a| a.clock().current_time_seconds())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use beetle_core::{parse_bms, GaugeType, JudgeGrade, Lane, Ruleset, TimingModel};

    /// Presses `error_ms` late (or early when negative) on each counted click.
    fn run_test(start: f64, errors_ms: &[f64]) -> Calibration {
        let mut cal = Calibration::new(start);
        let mut i = 0;
        let mut k = COUNT_IN_CLICKS;
        while !cal.is_done() {
            let press = click_time(start, k) + errors_ms[i % errors_ms.len()] / 1000.0;
            cal.press(press);
            i += 1;
            k += 1;
        }
        cal
    }

    #[test]
    fn clicks_come_out_once_each_when_due() {
        let mut cal = Calibration::new(10.0);
        assert_eq!(cal.next_click(9.9), None);
        assert_eq!(cal.next_click(10.0), Some(0));
        assert_eq!(cal.next_click(10.0), None);
        // A late frame gets every click that came due in the meantime.
        let due: Vec<u64> = std::iter::from_fn(|| cal.next_click(11.2)).collect();
        assert_eq!(due, vec![1, 2]);
    }

    #[test]
    fn count_in_presses_are_ignored() {
        let mut cal = Calibration::new(0.0);
        for k in 0..COUNT_IN_CLICKS {
            assert_eq!(cal.press(click_time(0.0, k)), None, "click {k}");
        }
        assert!(cal.presses().is_empty());
    }

    #[test]
    fn a_press_matches_the_nearest_click_within_150_ms() {
        let mut cal = Calibration::new(0.0);
        let k = COUNT_IN_CLICKS + 2;
        let on_time = click_time(0.0, k);
        assert!((cal.press(on_time + 0.020).unwrap() - 20.0).abs() < 1e-9);
        // Halfway to the next click is outside the window of both clicks.
        assert_eq!(cal.press(click_time(0.0, k + 1) - BEAT_SECONDS / 2.0), None);
        // 140 ms after the click is inside the window; 160 ms is not.
        assert!(cal.press(click_time(0.0, k + 3) + 0.140).is_some());
        assert_eq!(cal.press(click_time(0.0, k + 5) + 0.160), None);
    }

    #[test]
    fn a_second_press_on_the_same_click_is_ignored() {
        let mut cal = Calibration::new(0.0);
        let t = click_time(0.0, COUNT_IN_CLICKS);
        assert!(cal.press(t + 0.010).is_some());
        assert_eq!(cal.press(t + 0.030), None);
        assert_eq!(cal.presses().len(), 1);
    }

    #[test]
    fn the_test_ends_after_16_counted_presses() {
        let cal = run_test(0.0, &[5.0]);
        assert!(cal.is_done());
        assert_eq!(cal.presses().len(), REQUIRED_PRESSES);
        let mut cal = cal;
        assert_eq!(cal.press(click_time(0.0, 40)), None);
        assert_eq!(cal.next_click(1000.0), None);
    }

    #[test]
    fn summary_gives_mean_and_population_spread() {
        let s = summarize(&[10.0, 20.0, 30.0]).unwrap();
        assert!((s.mean_ms - 20.0).abs() < 1e-9);
        assert!((s.std_ms - (200.0f64 / 3.0).sqrt()).abs() < 1e-9);
        assert_eq!(s.kept, vec![true, true, true]);
        assert_eq!(summarize(&[]), None);
    }

    #[test]
    fn a_gross_outlier_is_left_out_of_the_mean() {
        let mut samples = vec![18.0, 20.0, 22.0, 19.0, 21.0, 20.0, 20.0, 19.0];
        samples.push(140.0);
        let s = summarize(&samples).unwrap();
        assert_eq!(s.kept.last(), Some(&false));
        assert!(s.kept[..8].iter().all(|&k| k));
        assert!((s.mean_ms - 19.875).abs() < 1e-9);
    }

    #[test]
    fn neighbouring_buffer_steps_are_both_kept() {
        // 4 presses in one clock step and 12 in the next, 20 ms apart.
        let mut samples = vec![62.0; 4];
        samples.extend(vec![42.0; 12]);
        let s = summarize(&samples).unwrap();
        assert!(s.kept.iter().all(|&k| k));
        assert!((s.mean_ms - 47.0).abs() < 1e-9);
    }

    #[test]
    fn a_few_presses_are_never_trimmed() {
        let s = summarize(&[0.0, 100.0, 0.0]).unwrap();
        assert_eq!(s.kept, vec![true, true, true]);
    }

    #[test]
    fn suggestion_rounds_to_the_step_and_stays_in_range() {
        assert_eq!(suggested_offset(20.0), -20.0);
        assert_eq!(suggested_offset(-7.4), 7.0);
        assert_eq!(suggested_offset(7.6), -8.0);
        assert_eq!(suggested_offset(250.0), -JUDGE_OFFSET_MAX_MS);
        assert_eq!(suggested_offset(-250.0), JUDGE_OFFSET_MAX_MS);
        // Zero is +0, not -0.
        assert!(suggested_offset(0.2).is_sign_positive());
    }

    #[test]
    fn beat_pulse_is_full_on_a_click_and_fades() {
        let cal = Calibration::new(5.0);
        assert_eq!(cal.pulse(4.0), 0.0);
        assert!((cal.pulse(5.0) - 1.0).abs() < 1e-6);
        assert!((cal.pulse(5.0 + PULSE_SECONDS / 2.0) - 0.5).abs() < 1e-6);
        assert_eq!(cal.pulse(5.0 + BEAT_SECONDS - 0.001), 0.0);
        assert!((cal.pulse(5.0 + BEAT_SECONDS) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn count_in_covers_the_first_four_beats() {
        let cal = Calibration::new(0.0);
        assert!(cal.counting_in(-0.5));
        assert!(cal.counting_in(click_time(0.0, 3) + 0.01));
        assert!(!cal.counting_in(click_time(0.0, 4) + 0.01));
    }

    /// Sign convention, checked through the real judge. A player who presses
    /// 20 ms late gets the suggestion -20 ms, and with it the press is judged
    /// on time. Without the offset it is judged 20 ms late.
    #[test]
    fn suggestion_cancels_a_late_press_in_the_real_judge() {
        // One key note on measure 1: at 120 BPM that is 2.0 s into the chart.
        let chart = parse_bms("#BPM 120\n#00111:01\n").expect("chart");
        let timing = TimingModel::from_chart(&chart);
        let judge_for =
            || beetle_core::JudgeEngine::new(&chart, &timing, GaugeType::Groove, Ruleset::CN);
        let note_time = judge_for().notes()[0].target_time_seconds;
        assert!(
            (note_time - 2.0).abs() < 1e-9,
            "fixture note at {note_time}"
        );

        // Calibration side: 16 presses, each 20 ms late after its click.
        let offset = run_test(0.0, &[20.0]).suggestion().expect("suggestion");
        assert_eq!(offset, -20.0);

        // Judge side: the same late press, judged without and with the offset.
        let press = note_time + 0.020;
        let (raw, _) = judge_for()
            .handle_key_down(Lane::Key1, judged_time(press, 0.0))
            .expect("judged without offset");
        assert!((raw.delta_ms - 20.0).abs() < 1e-6, "raw {}", raw.delta_ms);

        let (fixed, _) = judge_for()
            .handle_key_down(Lane::Key1, judged_time(press, offset))
            .expect("judged with offset");
        assert_eq!(fixed.grade, JudgeGrade::PerfectGreat);
        assert!(fixed.delta_ms.abs() < 1e-6, "delta {}", fixed.delta_ms);
    }
}

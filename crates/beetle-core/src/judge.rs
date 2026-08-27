use crate::bms::{BmsChart, Lane, NoteEvent, NoteType, WavId};
use crate::timing::TimingModel;

/// Judgment ratings for note hits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum JudgeGrade {
    PerfectGreat,
    Great,
    Good,
    Bad,
    Poor,
    Miss,
}

impl JudgeGrade {
    pub fn is_combo_breaker(self) -> bool {
        matches!(self, Self::Bad | Self::Poor | Self::Miss)
    }

    pub fn ex_score_points(self) -> u32 {
        match self {
            Self::PerfectGreat => 2,
            Self::Great => 1,
            _ => 0,
        }
    }
}

/// Timing windows in milliseconds for each grade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JudgeWindow {
    pub pgreat_ms: f64,
    pub great_ms: f64,
    pub good_ms: f64,
    pub bad_ms: f64,
    pub poor_ms: f64,
}

impl Default for JudgeWindow {
    fn default() -> Self {
        Self::from_rank(2) // NORMAL
    }
}

impl JudgeWindow {
    /// Creates judge window preset based on BMS #RANK (0=VERY HARD, 1=HARD, 2=NORMAL, 3=EASY).
    pub fn from_rank(rank: u32) -> Self {
        match rank {
            0 => Self {
                // VERY HARD
                pgreat_ms: 8.0,
                great_ms: 24.0,
                good_ms: 40.0,
                bad_ms: 200.0,
                poor_ms: 300.0,
            },
            1 => Self {
                // HARD
                pgreat_ms: 15.0,
                great_ms: 30.0,
                good_ms: 60.0,
                bad_ms: 200.0,
                poor_ms: 300.0,
            },
            3 => Self {
                // EASY
                pgreat_ms: 21.0,
                great_ms: 60.0,
                good_ms: 120.0,
                bad_ms: 200.0,
                poor_ms: 300.0,
            },
            _ => Self {
                // NORMAL
                pgreat_ms: 18.0,
                great_ms: 40.0,
                good_ms: 100.0,
                bad_ms: 200.0,
                poor_ms: 300.0,
            },
        }
    }

    /// Evaluates a timing difference delta in milliseconds against windows.
    pub fn evaluate(&self, delta_ms: f64) -> Option<JudgeGrade> {
        let abs_delta = delta_ms.abs();
        if abs_delta <= self.pgreat_ms {
            Some(JudgeGrade::PerfectGreat)
        } else if abs_delta <= self.great_ms {
            Some(JudgeGrade::Great)
        } else if abs_delta <= self.good_ms {
            Some(JudgeGrade::Good)
        } else if abs_delta <= self.bad_ms {
            Some(JudgeGrade::Bad)
        } else if abs_delta <= self.poor_ms {
            Some(JudgeGrade::Poor)
        } else {
            None
        }
    }
}

/// Result of a single note judgment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JudgeResult {
    pub grade: JudgeGrade,
    pub delta_ms: f64,
}

/// Gauge mode (Easy, Groove normal, Hard, or Hazard).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GaugeType {
    Easy,
    Groove,
    Hard,
    Hazard,
}

impl GaugeType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Easy => "EASY",
            Self::Groove => "GROOVE",
            Self::Hard => "HARD",
            Self::Hazard => "HAZARD",
        }
    }
}

/// Tracks realtime score, combo, and EX score during gameplay.
#[derive(Debug, Clone)]
pub struct ScoreTracker {
    pub pgreat_count: u32,
    pub great_count: u32,
    pub good_count: u32,
    pub bad_count: u32,
    pub poor_count: u32,
    pub miss_count: u32,
    pub current_combo: u32,
    pub max_combo: u32,
    pub ex_score: u32,
    pub total_notes: u32,
    pub gauge: f64,
    pub gauge_type: GaugeType,
    pub is_failed: bool,
    gauge_gain_pgreat: f64,
    gauge_gain_great: f64,
    gauge_gain_good: f64,
}

impl Default for ScoreTracker {
    fn default() -> Self {
        Self::new(0, 200.0, GaugeType::Groove)
    }
}

impl ScoreTracker {
    pub fn new(total_notes: u32, total: f64, gauge_type: GaugeType) -> Self {
        let n = total_notes.max(1) as f64;
        let effective_total = if total <= 0.0 { 200.0 } else { total };

        // BMS standard gauge gain formula (Easy gauge gets 1.2x boost)
        let multiplier = if gauge_type == GaugeType::Easy { 1.2 } else { 1.0 };
        let gain_pgreat = (effective_total / n) * multiplier;
        let gain_great = gain_pgreat * 0.8;
        let gain_good = gain_pgreat * 0.5;

        let initial_gauge = match gauge_type {
            GaugeType::Easy | GaugeType::Groove => 20.0,
            GaugeType::Hard | GaugeType::Hazard => 100.0,
        };

        Self {
            pgreat_count: 0,
            great_count: 0,
            good_count: 0,
            bad_count: 0,
            poor_count: 0,
            miss_count: 0,
            current_combo: 0,
            max_combo: 0,
            ex_score: 0,
            total_notes,
            gauge: initial_gauge,
            gauge_type,
            is_failed: false,
            gauge_gain_pgreat: gain_pgreat,
            gauge_gain_great: gain_great,
            gauge_gain_good: gain_good,
        }
    }

    /// Records a judge result and updates score and gauge.
    pub fn record_hit(&mut self, grade: JudgeGrade) {
        if self.is_failed && matches!(self.gauge_type, GaugeType::Hard | GaugeType::Hazard) {
            return;
        }

        self.ex_score += grade.ex_score_points();

        match grade {
            JudgeGrade::PerfectGreat => {
                self.pgreat_count += 1;
                self.current_combo += 1;
                self.apply_gauge_delta(self.gauge_gain_pgreat);
            }
            JudgeGrade::Great => {
                self.great_count += 1;
                self.current_combo += 1;
                self.apply_gauge_delta(self.gauge_gain_great);
            }
            JudgeGrade::Good => {
                self.good_count += 1;
                self.current_combo += 1;
                self.apply_gauge_delta(self.gauge_gain_good);
            }
            JudgeGrade::Bad => {
                self.bad_count += 1;
                self.current_combo = 0;
                let penalty = match self.gauge_type {
                    GaugeType::Easy => -1.6,
                    GaugeType::Groove => -2.0,
                    GaugeType::Hard => -5.0,
                    GaugeType::Hazard => -100.0,
                };
                self.apply_gauge_delta(penalty);
            }
            JudgeGrade::Poor => {
                self.poor_count += 1;
                self.current_combo = 0;
                let penalty = match self.gauge_type {
                    GaugeType::Easy => -2.4,
                    GaugeType::Groove => -3.0,
                    GaugeType::Hard => -9.0,
                    GaugeType::Hazard => -100.0,
                };
                self.apply_gauge_delta(penalty);
            }
            JudgeGrade::Miss => {
                self.miss_count += 1;
                self.current_combo = 0;
                let penalty = match self.gauge_type {
                    GaugeType::Easy => -4.0,
                    GaugeType::Groove => -5.0,
                    GaugeType::Hard => -10.0,
                    GaugeType::Hazard => -100.0,
                };
                self.apply_gauge_delta(penalty);
            }
        }

        if self.current_combo > self.max_combo {
            self.max_combo = self.current_combo;
        }
    }

    fn apply_gauge_delta(&mut self, delta: f64) {
        self.gauge += delta;
        if self.gauge > 100.0 {
            self.gauge = 100.0;
        }
        match self.gauge_type {
            GaugeType::Easy | GaugeType::Groove => {
                if self.gauge < 2.0 {
                    self.gauge = 2.0;
                }
            }
            GaugeType::Hard | GaugeType::Hazard => {
                if self.gauge <= 0.0 {
                    self.gauge = 0.0;
                    self.is_failed = true;
                }
            }
        }
    }

    /// Maximum possible EX-Score for this chart (notes * 2).
    pub fn max_ex_score(&self) -> u32 {
        self.total_notes * 2
    }

    /// Current accuracy rate percentage (0.0% ~ 100.0%).
    pub fn accuracy_rate(&self) -> f64 {
        let max_score = self.max_ex_score();
        if max_score == 0 {
            0.0
        } else {
            (self.ex_score as f64 / max_score as f64) * 100.0
        }
    }

    /// Returns true if cleared at the end of the song.
    pub fn is_cleared(&self) -> bool {
        match self.gauge_type {
            GaugeType::Easy | GaugeType::Groove => self.gauge >= 80.0,
            GaugeType::Hard | GaugeType::Hazard => !self.is_failed,
        }
    }
}

/// A playable note with precalculated target audio time.
#[derive(Debug, Clone)]
pub struct PlayNote {
    pub note_event: NoteEvent,
    pub target_time_seconds: f64,
    pub is_judged: bool,
    pub is_holding: bool,
}

/// The runtime judgment engine managing live notes, hit detection, and misses.
pub struct JudgeEngine {
    notes: Vec<PlayNote>,
    window: JudgeWindow,
    score: ScoreTracker,
}

impl JudgeEngine {
    /// Initializes the judgment engine with chart and precalculated timing model.
    pub fn new(chart: &BmsChart, timing: &TimingModel, gauge_type: GaugeType) -> Self {
        let mut play_notes = Vec::with_capacity(chart.notes.len());
        for note in &chart.notes {
            let target_time = timing.beat_to_time_seconds(note.measure, note.fraction);
            play_notes.push(PlayNote {
                note_event: note.clone(),
                target_time_seconds: target_time,
                is_judged: false,
                is_holding: false,
            });
        }

        // Sort chronologically
        play_notes.sort_by(|a, b| {
            a.target_time_seconds
                .partial_cmp(&b.target_time_seconds)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let total_notes = play_notes
            .iter()
            .filter(|n| n.note_event.note_type != NoteType::LongNoteEnd)
            .count() as u32;

        let window = JudgeWindow::from_rank(chart.header.rank);
        let score = ScoreTracker::new(total_notes, chart.header.total, gauge_type);

        Self {
            notes: play_notes,
            window,
            score,
        }
    }

    /// Handles a key press on a specific lane.
    /// Returns the judgment result and keysound `WavId` (if any).
    pub fn handle_key_down(
        &mut self,
        lane: Lane,
        current_time_seconds: f64,
    ) -> Option<(JudgeResult, Option<WavId>)> {
        // Find earliest unjudged note in this lane within poor window
        for note in self.notes.iter_mut() {
            if note.is_judged || note.note_event.lane != lane {
                continue;
            }

            let delta_seconds = current_time_seconds - note.target_time_seconds;
            let delta_ms = delta_seconds * 1000.0;

            if let Some(grade) = self.window.evaluate(delta_ms) {
                note.is_judged = true;
                if note.note_event.note_type == NoteType::LongNoteStart {
                    note.is_holding = true;
                }

                let result = JudgeResult { grade, delta_ms };
                self.score.record_hit(grade);
                return Some((result, note.note_event.wav_id));
            }
        }

        None
    }

    /// Handles key release on a specific lane (for long note releases).
    pub fn handle_key_up(
        &mut self,
        lane: Lane,
        current_time_seconds: f64,
    ) -> Option<JudgeResult> {
        for note in self.notes.iter_mut() {
            if note.is_judged
                || note.note_event.lane != lane
                || note.note_event.note_type != NoteType::LongNoteEnd
            {
                continue;
            }

            let delta_seconds = current_time_seconds - note.target_time_seconds;
            let delta_ms = delta_seconds * 1000.0;

            if let Some(grade) = self.window.evaluate(delta_ms) {
                note.is_judged = true;
                let result = JudgeResult { grade, delta_ms };
                self.score.record_hit(grade);
                return Some(result);
            }
        }
        None
    }

    /// Updates missed notes that passed beyond the POOR timing window.
    pub fn update_misses(&mut self, current_time_seconds: f64) -> Vec<(Lane, JudgeResult)> {
        let mut misses = Vec::new();

        for note in self.notes.iter_mut() {
            if note.is_judged {
                continue;
            }

            let delta_seconds = current_time_seconds - note.target_time_seconds;
            let delta_ms = delta_seconds * 1000.0;

            // Past poor window (note passed judgment line)
            if delta_ms > self.window.poor_ms {
                note.is_judged = true;
                let result = JudgeResult {
                    grade: JudgeGrade::Miss,
                    delta_ms,
                };
                self.score.record_hit(JudgeGrade::Miss);
                misses.push((note.note_event.lane, result));
            }
        }

        misses
    }

    /// Automatically judges all notes that reach the judgment line at the current audio time with PerfectGreat.
    pub fn auto_play_update(
        &mut self,
        current_time_seconds: f64,
    ) -> Vec<(Lane, JudgeResult, Option<WavId>)> {
        let mut hits = Vec::new();
        for note in self.notes.iter_mut() {
            if note.is_judged {
                continue;
            }
            if current_time_seconds >= note.target_time_seconds {
                note.is_judged = true;
                let result = JudgeResult {
                    grade: JudgeGrade::PerfectGreat,
                    delta_ms: 0.0,
                };
                self.score.record_hit(JudgeGrade::PerfectGreat);
                hits.push((note.note_event.lane, result, note.note_event.wav_id));
            }
        }
        hits
    }

    /// Fast-forwards note states when jumping to a practice measure.
    pub fn advance_to_time(&mut self, start_time_seconds: f64) {
        for note in self.notes.iter_mut() {
            if note.target_time_seconds < start_time_seconds {
                note.is_judged = true;
            }
        }
    }

    /// Access the live score tracker.
    pub fn score(&self) -> &ScoreTracker {
        &self.score
    }

    /// Access all playable notes (for renderer).
    pub fn notes(&self) -> &[PlayNote] {
        &self.notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bms::*;
    use crate::timing::TimingModel;

    #[test]
    fn test_judge_window_evaluation() {
        let window = JudgeWindow::from_rank(2); // Normal
        assert_eq!(window.evaluate(0.0), Some(JudgeGrade::PerfectGreat));
        assert_eq!(window.evaluate(10.0), Some(JudgeGrade::PerfectGreat));
        assert_eq!(window.evaluate(-15.0), Some(JudgeGrade::PerfectGreat));
        assert_eq!(window.evaluate(25.0), Some(JudgeGrade::Great));
        assert_eq!(window.evaluate(60.0), Some(JudgeGrade::Good));
        assert_eq!(window.evaluate(150.0), Some(JudgeGrade::Bad));
        assert_eq!(window.evaluate(250.0), Some(JudgeGrade::Poor));
        assert_eq!(window.evaluate(350.0), None);
    }

    #[test]
    fn test_score_tracker_combo_and_ex() {
        let mut tracker = ScoreTracker::new(10, 200.0, GaugeType::Groove);
        tracker.record_hit(JudgeGrade::PerfectGreat);
        assert_eq!(tracker.ex_score, 2);
        assert_eq!(tracker.current_combo, 1);

        tracker.record_hit(JudgeGrade::Great);
        assert_eq!(tracker.ex_score, 3);
        assert_eq!(tracker.current_combo, 2);

        tracker.record_hit(JudgeGrade::Bad);
        assert_eq!(tracker.ex_score, 3);
        assert_eq!(tracker.current_combo, 0);
        assert_eq!(tracker.max_combo, 2);
    }

    #[test]
    fn test_judge_engine_hit_and_miss() {
        let chart = BmsChart {
            header: BmsHeader {
                bpm: 120.0,
                ..Default::default()
            },
            notes: vec![
                NoteEvent {
                    measure: 1,
                    fraction: 0.0,
                    lane: Lane::Key1,
                    wav_id: Some(WavId(1)),
                    note_type: NoteType::Tap,
                },
                NoteEvent {
                    measure: 2,
                    fraction: 0.0,
                    lane: Lane::Key2,
                    wav_id: Some(WavId(2)),
                    note_type: NoteType::Tap,
                },
            ],
            ..Default::default()
        };
        let timing = TimingModel::from_chart(&chart);
        let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Groove);

        // Note 1 target time: 2.0s
        // Hit at 2.005s (PGREAT)
        let hit = engine.handle_key_down(Lane::Key1, 2.005);
        assert!(hit.is_some());
        let (res, wav) = hit.unwrap();
        assert_eq!(res.grade, JudgeGrade::PerfectGreat);
        assert_eq!(wav, Some(WavId(1)));

        // Note 2 target time: 4.0s
        // Time reaches 4.4s without key down -> Miss
        let misses = engine.update_misses(4.4);
        assert_eq!(misses.len(), 1);
        assert_eq!(misses[0].0, Lane::Key2);
        assert_eq!(misses[0].1.grade, JudgeGrade::Miss);
        assert_eq!(engine.score().miss_count, 1);
    }

    #[test]
    fn test_auto_play_update() {
        let chart = BmsChart {
            header: BmsHeader {
                bpm: 120.0,
                ..Default::default()
            },
            notes: vec![
                NoteEvent {
                    measure: 1,
                    fraction: 0.0,
                    lane: Lane::Key1,
                    wav_id: Some(WavId(1)),
                    note_type: NoteType::Tap,
                },
                NoteEvent {
                    measure: 2,
                    fraction: 0.0,
                    lane: Lane::Key2,
                    wav_id: Some(WavId(2)),
                    note_type: NoteType::Tap,
                },
            ],
            ..Default::default()
        };
        let timing = TimingModel::from_chart(&chart);
        let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Groove);

        // At t=1.0s, no notes reached
        let hits = engine.auto_play_update(1.0);
        assert!(hits.is_empty());

        // At t=2.0s, Note 1 hits
        let hits = engine.auto_play_update(2.0);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, Lane::Key1);
        assert_eq!(hits[0].1.grade, JudgeGrade::PerfectGreat);
        assert_eq!(engine.score().pgreat_count, 1);

        // At t=4.0s, Note 2 hits
        let hits = engine.auto_play_update(4.0);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, Lane::Key2);
        assert_eq!(engine.score().pgreat_count, 2);
    }
}

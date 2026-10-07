pub mod score_tracker;

use crate::bms::{BmsChart, Lane, NoteEvent, NoteType, WavId};
use crate::timing::TimingModel;
use std::collections::HashSet;

pub use score_tracker::{GaugeType, ScoreTracker};

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

/// A playable note with precalculated target audio time.
#[derive(Debug, Clone)]
pub struct PlayNote {
    pub note_event: NoteEvent,
    pub target_time_seconds: f64,
    pub end_target_time_seconds: f64,
    pub is_judged: bool,
    pub is_holding: bool,
}

/// The runtime judgment engine managing live notes, hit detection, and misses.
pub struct JudgeEngine {
    notes: Vec<PlayNote>,
    window: JudgeWindow,
    score: ScoreTracker,
    /// Lanes whose key is currently down (a held key sets off a mine too).
    lanes_down: HashSet<Lane>,
}

impl JudgeEngine {
    /// Initializes the judgment engine with chart and precalculated timing model.
    pub fn new(chart: &BmsChart, timing: &TimingModel, gauge_type: GaugeType) -> Self {
        let mut play_notes = Vec::with_capacity(chart.notes.len());
        for (i, note) in chart.notes.iter().enumerate() {
            let target_time = timing.beat_to_time_seconds(note.measure, note.fraction);
            let mut end_time = target_time;
            if note.note_type == NoteType::LongNoteStart {
                for end_note in &chart.notes[i + 1..] {
                    if end_note.lane == note.lane && end_note.note_type == NoteType::LongNoteEnd {
                        end_time = timing.beat_to_time_seconds(end_note.measure, end_note.fraction);
                        break;
                    }
                }
            }
            play_notes.push(PlayNote {
                note_event: note.clone(),
                target_time_seconds: target_time,
                end_target_time_seconds: end_time,
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

        // Every note is judged, and a long note's head and tail are separate
        // judgments, so both count (this also matches the song-select NOTES).
        let total_notes = play_notes
            .iter()
            .filter(|n| n.note_event.note_type != NoteType::Landmine)
            .count() as u32;

        let window = JudgeWindow::from_rank(chart.header.rank);
        let score = ScoreTracker::new(total_notes, chart.header.total, gauge_type);

        Self {
            notes: play_notes,
            window,
            score,
            lanes_down: HashSet::new(),
        }
    }

    /// Handles a key press on a specific lane.
    /// Returns the judgment result and keysound `WavId` (if any).
    pub fn handle_key_down(
        &mut self,
        lane: Lane,
        current_time_seconds: f64,
    ) -> Option<(JudgeResult, Option<WavId>)> {
        self.lanes_down.insert(lane);
        self.trigger_mines(lane, current_time_seconds);

        // Find earliest unjudged note in this lane within poor window
        for note in self.notes.iter_mut() {
            if note.is_judged
                || note.note_event.lane != lane
                || note.note_event.note_type == NoteType::Landmine
            {
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
                self.score.record_hit_with_delta(grade, delta_ms);
                return Some((result, note.note_event.wav_id));
            }
        }

        None
    }

    /// Sets off every unjudged mine on `lane` that is within the GREAT window
    /// of `time_seconds`. Each one drains the gauge by half its value in percent.
    fn trigger_mines(&mut self, lane: Lane, time_seconds: f64) {
        for note in self.notes.iter_mut() {
            if note.is_judged
                || note.note_event.lane != lane
                || note.note_event.note_type != NoteType::Landmine
            {
                continue;
            }
            let delta_ms = (time_seconds - note.target_time_seconds) * 1000.0;
            if delta_ms.abs() <= self.window.great_ms {
                note.is_judged = true;
                let value = note.note_event.wav_id.map_or(0, |id| id.0);
                self.score.record_mine_hit(f64::from(value) / 2.0);
            }
        }
    }

    /// Handles key release on a specific lane (for long note releases).
    pub fn handle_key_up(&mut self, lane: Lane, current_time_seconds: f64) -> Option<JudgeResult> {
        self.lanes_down.remove(&lane);
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
                self.score.record_hit_with_delta(grade, delta_ms);
                return Some(result);
            }
        }
        None
    }

    /// Updates missed notes that passed beyond the POOR timing window.
    pub fn update_misses(&mut self, current_time_seconds: f64) -> Vec<(Lane, JudgeResult)> {
        let mut misses = Vec::new();

        for lane in self.lanes_down.clone() {
            self.trigger_mines(lane, current_time_seconds);
        }

        for note in self.notes.iter_mut() {
            if note.is_judged {
                continue;
            }

            let delta_seconds = current_time_seconds - note.target_time_seconds;
            let delta_ms = delta_seconds * 1000.0;

            // A mine nobody stepped on just goes by.
            if note.note_event.note_type == NoteType::Landmine {
                if delta_ms > self.window.great_ms {
                    note.is_judged = true;
                }
                continue;
            }

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
                if note.note_event.note_type == NoteType::Landmine {
                    continue; // auto play never steps on a mine
                }
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
    fn test_judge_engine_handles_dp_14k_lanes_with_correct_max_score() {
        // Milestone 10 Phase 2: confirm the judge/score engine needs no
        // lane-count-specific changes - it is already lane-agnostic, so a
        // real 14K (Double Play) chart parsed end-to-end must produce a
        // max EX score based on ALL real playable notes (1P + 2P), and
        // P2* lanes must be judgeable exactly like 1P lanes.
        let bms = r#"
#PLAYER 3
#00111:01
#00121:01
#00128:01
"#;
        let chart = parse_bms(bms).expect("Failed to parse 14K-ish DP chart");
        assert_eq!(chart.detect_play_mode(), PlayMode::Keys14);
        assert_eq!(chart.notes.len(), 3); // 1P Key1 + 2P P2Key1 + 2P P2Key6, all judgeable
        assert_eq!(chart.bgm_notes.len(), 0);

        let timing = TimingModel::from_chart(&chart);
        let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Groove);

        // max_ex_score must reflect all 3 real notes (2 points each), not
        // be inflated/deflated by any BGM-passthrough fallback.
        assert_eq!(engine.score().max_ex_score(), 6);

        // A 2P lane must be judgeable exactly like a 1P lane.
        let target = engine
            .notes()
            .iter()
            .find(|n| n.note_event.lane == Lane::P2Key1)
            .expect("P2Key1 note missing from judge engine")
            .target_time_seconds;
        let hit = engine.handle_key_down(Lane::P2Key1, target + 0.005);
        assert!(hit.is_some());
        assert_eq!(hit.unwrap().0.grade, JudgeGrade::PerfectGreat);
    }

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

    #[test]
    fn long_note_head_and_tail_are_both_counted_in_the_totals() {
        // One tap and one long note (LNTYPE 1 pair). Both ends are judged, so a
        // perfect run must land exactly on max EX / max combo, never above.
        let chart = parse_bms(
            "#BPM 120
#00111:01
#00251:0101
",
        )
        .unwrap();
        let timing = TimingModel::from_chart(&chart);
        let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Groove);
        engine.auto_play_update(10.0);

        let score = engine.score();
        assert_eq!(score.pgreat_count, 3);
        assert_eq!(score.total_notes, 3);
        assert_eq!(score.ex_score, score.max_ex_score());
        assert_eq!(score.max_combo, score.total_notes);
        assert_eq!(score.accuracy_rate(), 100.0);
    }

    /// One tap on lane 2 at 4.0s and a mine (value 0A = 5% gauge) on lane 1 at 2.0s.
    fn mine_chart() -> BmsChart {
        parse_bms("#BPM 120
#001D1:0A
#00212:01
").unwrap()
    }

    #[test]
    fn stepping_on_a_mine_drains_the_gauge_but_not_combo_or_ex() {
        let chart = mine_chart();
        let timing = TimingModel::from_chart(&chart);
        let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Hard);
        assert_eq!(engine.score().total_notes, 1);

        // Hit the tap first so there is a combo to protect.
        engine.handle_key_down(Lane::Key2, 4.0);
        let gauge = engine.score().gauge;
        assert!(engine.handle_key_down(Lane::Key1, 2.01).is_none());

        let score = engine.score();
        assert_eq!(score.mine_hit_count, 1);
        assert_eq!(score.gauge, gauge - 5.0);
        assert_eq!(score.current_combo, 1);
        assert_eq!(score.ex_score, 2);

        // A mine only goes off once.
        engine.handle_key_down(Lane::Key1, 2.02);
        assert_eq!(engine.score().mine_hit_count, 1);
    }

    #[test]
    fn untouched_mine_costs_nothing_and_is_not_a_miss() {
        let chart = mine_chart();
        let timing = TimingModel::from_chart(&chart);
        let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Hard);
        let misses = engine.update_misses(2.5);
        assert!(misses.is_empty());
        assert_eq!(engine.score().miss_count, 0);
        assert_eq!(engine.score().mine_hit_count, 0);
        assert_eq!(engine.score().gauge, 100.0);
    }

    #[test]
    fn mine_outside_the_great_window_is_safe_to_press_near() {
        let chart = mine_chart();
        let timing = TimingModel::from_chart(&chart);
        let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Hard);
        engine.handle_key_down(Lane::Key1, 2.3);
        assert_eq!(engine.score().mine_hit_count, 0);
    }

    #[test]
    fn a_key_held_through_a_mine_sets_it_off() {
        let chart = mine_chart();
        let timing = TimingModel::from_chart(&chart);
        let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Hard);
        engine.handle_key_down(Lane::Key1, 1.0); // pressed long before the mine
        engine.update_misses(1.5);
        assert_eq!(engine.score().mine_hit_count, 0);
        engine.update_misses(1.99); // mine now within the window, key still down
        assert_eq!(engine.score().mine_hit_count, 1);
    }

    #[test]
    fn released_key_does_not_set_off_a_mine() {
        let chart = mine_chart();
        let timing = TimingModel::from_chart(&chart);
        let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Hard);
        engine.handle_key_down(Lane::Key1, 1.0);
        engine.handle_key_up(Lane::Key1, 1.2);
        engine.update_misses(2.0);
        assert_eq!(engine.score().mine_hit_count, 0);
    }

    #[test]
    fn auto_play_skips_mines_and_still_scores_perfectly() {
        let chart = mine_chart();
        let timing = TimingModel::from_chart(&chart);
        let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Hard);
        engine.auto_play_update(10.0);
        let score = engine.score();
        assert_eq!(score.mine_hit_count, 0);
        assert_eq!(score.pgreat_count, 1);
        assert_eq!(score.accuracy_rate(), 100.0);
    }

    #[test]
    fn test_score_tracker_rank_and_timing_histogram() {
        let mut score = ScoreTracker::new(90, 200.0, GaugeType::Groove);
        // 90 notes -> max EX = 180
        // Perfect score -> 180 EX -> MAX
        for _ in 0..90 {
            score.record_hit_with_delta(JudgeGrade::PerfectGreat, 0.0);
        }
        assert_eq!(score.rank(), "MAX");
        assert_eq!(score.timing_histogram[8], 90);
        assert_eq!(score.fast_count, 0);
        assert_eq!(score.slow_count, 0);

        let mut score2 = ScoreTracker::new(90, 200.0, GaugeType::Groove);
        // 80 PGREAT (160) + 10 GREAT (10) = 170 EX -> 170/180 = 94.4% -> AAA
        for _ in 0..80 {
            score2.record_hit_with_delta(JudgeGrade::PerfectGreat, -10.0); // Fast
        }
        for _ in 0..10 {
            score2.record_hit_with_delta(JudgeGrade::Great, 15.0); // Slow
        }
        assert_eq!(score2.rank(), "AAA");
        assert_eq!(score2.fast_count, 80);
        assert_eq!(score2.slow_count, 10);
    }
}

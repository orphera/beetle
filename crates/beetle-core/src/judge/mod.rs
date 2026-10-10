pub mod gauge_trend;
pub mod score_tracker;

use crate::bms::{BmsChart, Lane, NoteEvent, NoteType, WavId};
use crate::rules::{LnRule, Ruleset};
use crate::timing::TimingModel;
use std::collections::{HashMap, HashSet};

pub use gauge_trend::{GaugePoint, GaugeTrend, GAUGE_TREND_MAX_POINTS};
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
    /// For a long note head: index (in the engine's sorted note list) of its tail.
    pub tail_index: Option<usize>,
    /// For a long note tail: index of its head. A tail only counts while its head is held.
    pub head_index: Option<usize>,
}

/// The runtime judgment engine managing live notes, hit detection, and misses.
pub struct JudgeEngine {
    notes: Vec<PlayNote>,
    window: JudgeWindow,
    score: ScoreTracker,
    /// Lanes whose key is currently down (a held key sets off a mine too).
    lanes_down: HashSet<Lane>,
    ruleset: Ruleset,
}

/// Under the LN rule, a long note is broken by letting go earlier than this
/// before its tail: the GOOD window.
fn ln_release_slack_ms(window: &JudgeWindow) -> f64 {
    window.good_ms
}

/// What a broken long note counts as under the LN rule.
const LN_BREAK_GRADE: JudgeGrade = JudgeGrade::Poor;

impl JudgeEngine {
    /// Initializes the judgment engine with chart and precalculated timing model.
    pub fn new(
        chart: &BmsChart,
        timing: &TimingModel,
        gauge_type: GaugeType,
        ruleset: Ruleset,
    ) -> Self {
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
                tail_index: None,
                head_index: None,
            });
        }

        // Sort chronologically
        play_notes.sort_by(|a, b| {
            a.target_time_seconds
                .partial_cmp(&b.target_time_seconds)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Pair each long note head with its tail (same lane, in time order).
        let mut open_heads: HashMap<Lane, usize> = HashMap::new();
        for i in 0..play_notes.len() {
            let lane = play_notes[i].note_event.lane;
            match play_notes[i].note_event.note_type {
                NoteType::LongNoteStart => {
                    open_heads.insert(lane, i);
                }
                NoteType::LongNoteEnd => {
                    if let Some(head) = open_heads.remove(&lane) {
                        play_notes[head].tail_index = Some(i);
                        play_notes[i].head_index = Some(head);
                    }
                }
                _ => {}
            }
        }

        // Every note is judged, and a long note's head and tail are separate
        // judgments, so both count (this also matches the song-select NOTES).
        // Under the LN rule a tail is not a judgment, so it is not a note either.
        let total_notes = play_notes
            .iter()
            .filter(|n| match n.note_event.note_type {
                NoteType::Landmine => false,
                NoteType::LongNoteEnd => ruleset.ln == LnRule::Cn,
                _ => true,
            })
            .count() as u32;

        let window = JudgeWindow::from_rank(chart.header.rank);
        let score = ScoreTracker::new(total_notes, chart.header.total, gauge_type);

        Self {
            notes: play_notes,
            window,
            score,
            lanes_down: HashSet::new(),
            ruleset,
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
            // Tails are judged on release, mines by `trigger_mines`; a press is never either.
            if note.is_judged
                || note.note_event.lane != lane
                || matches!(
                    note.note_event.note_type,
                    NoteType::Landmine | NoteType::LongNoteEnd
                )
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
    ///
    /// Releasing a held long note judges its tail by how close the release is
    /// to the tail's time. Letting go earlier than the POOR window allows is
    /// a MISS right away (the hold is over); a release past the tail's window
    /// is left to `update_misses`. A tail whose head is not being held is
    /// never judged by a release.
    pub fn handle_key_up(&mut self, lane: Lane, current_time_seconds: f64) -> Option<JudgeResult> {
        self.lanes_down.remove(&lane);
        for i in 0..self.notes.len() {
            let note = &self.notes[i];
            if note.is_judged
                || note.note_event.lane != lane
                || note.note_event.note_type != NoteType::LongNoteEnd
            {
                continue;
            }
            let Some(head) = note.head_index.filter(|&h| self.notes[h].is_holding) else {
                continue;
            };

            let delta_ms = (current_time_seconds - note.target_time_seconds) * 1000.0;

            if self.ruleset.ln == LnRule::Ln {
                // The tail is not judged: letting go near it (or after it) just
                // completes the note; letting go early breaks it.
                self.notes[i].is_judged = true;
                self.notes[head].is_holding = false;
                if delta_ms >= -ln_release_slack_ms(&self.window) {
                    return None;
                }
                self.score.record_hit(LN_BREAK_GRADE);
                return Some(JudgeResult {
                    grade: LN_BREAK_GRADE,
                    delta_ms,
                });
            }

            let grade = match self.window.evaluate(delta_ms) {
                Some(grade) => grade,
                None if delta_ms < 0.0 => JudgeGrade::Miss,
                None => continue,
            };

            self.notes[i].is_judged = true;
            self.notes[head].is_holding = false;
            self.score.record_hit_with_delta(grade, delta_ms);
            return Some(JudgeResult { grade, delta_ms });
        }
        None
    }

    /// Updates missed notes that passed beyond the POOR timing window.
    pub fn update_misses(&mut self, current_time_seconds: f64) -> Vec<(Lane, JudgeResult)> {
        let mut misses = Vec::new();
        let mut tail_misses: Vec<(usize, Lane, f64)> = Vec::new();
        // Long notes that ended without a judgment (LN rule), by head index and tail index.
        let mut ended_heads: Vec<usize> = Vec::new();
        let mut silent_tails: Vec<usize> = Vec::new();
        let ln_rule = self.ruleset.ln == LnRule::Ln;

        for lane in self.lanes_down.clone() {
            self.trigger_mines(lane, current_time_seconds);
        }

        for note in self.notes.iter_mut() {
            if note.is_judged {
                continue;
            }

            let delta_seconds = current_time_seconds - note.target_time_seconds;
            let delta_ms = delta_seconds * 1000.0;

            // Under the LN rule a tail is never judged: once its time has come the
            // note is complete, and a note whose head was missed has already ended.
            if ln_rule && note.note_event.note_type == NoteType::LongNoteEnd {
                if current_time_seconds >= note.target_time_seconds {
                    note.is_judged = true;
                    ended_heads.extend(note.head_index);
                }
                continue;
            }

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
                note.is_holding = false;
                let lane = note.note_event.lane;
                let result = JudgeResult {
                    grade: JudgeGrade::Miss,
                    delta_ms,
                };
                self.score.record_hit(JudgeGrade::Miss);
                misses.push((lane, result));
                // A missed head takes its tail with it: the player cannot hold
                // what was never grabbed, so the tail is a MISS right now too.
                if let Some(tail) = note.tail_index {
                    if ln_rule {
                        silent_tails.push(tail);
                    } else {
                        tail_misses.push((tail, lane, delta_ms));
                    }
                }
            }
        }

        for head in ended_heads {
            self.notes[head].is_holding = false;
        }
        for tail in silent_tails {
            self.notes[tail].is_judged = true;
        }

        for (tail, lane, delta_ms) in tail_misses {
            if !self.notes[tail].is_judged {
                self.notes[tail].is_judged = true;
                let result = JudgeResult {
                    grade: JudgeGrade::Miss,
                    delta_ms,
                };
                self.score.record_hit(JudgeGrade::Miss);
                misses.push((lane, result));
            }
        }

        misses
    }

    /// Automatically judges all notes that reach the judgment line at the current audio time with PerfectGreat.
    pub fn auto_play_update(
        &mut self,
        current_time_seconds: f64,
    ) -> Vec<(Lane, JudgeResult, Option<WavId>)> {
        let ln_rule = self.ruleset.ln == LnRule::Ln;
        let mut hits = Vec::new();
        for i in 0..self.notes.len() {
            let note = &mut self.notes[i];
            if note.is_judged {
                continue;
            }
            if current_time_seconds >= note.target_time_seconds {
                note.is_judged = true;
                match note.note_event.note_type {
                    NoteType::Landmine => continue, // auto play never steps on a mine
                    // The auto player holds a long note from head to tail.
                    NoteType::LongNoteStart => note.is_holding = true,
                    NoteType::LongNoteEnd => {
                        if let Some(head) = note.head_index {
                            self.notes[head].is_holding = false;
                        }
                        if ln_rule {
                            continue; // a tail is not a judgment under the LN rule
                        }
                    }
                    _ => {}
                }
                let note = &self.notes[i];
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

    /// The key sounds [`Self::auto_play_update`] will hit for notes due in
    /// `(after, until]`, as `(time, sample)`, so they can be started ahead
    /// on the audio clock. Notes already judged are left out.
    pub fn auto_play_sounds(&self, after: f64, until: f64) -> Vec<(f64, WavId)> {
        let ln_rule = self.ruleset.ln == LnRule::Ln;
        self.notes
            .iter()
            .filter(|n| !n.is_judged && n.target_time_seconds > after)
            .filter(|n| n.target_time_seconds <= until)
            .filter(|n| match n.note_event.note_type {
                NoteType::Landmine => false,
                NoteType::LongNoteEnd => !ln_rule,
                _ => true,
            })
            .filter_map(|n| n.note_event.wav_id.map(|w| (n.target_time_seconds, w)))
            .collect()
    }

    /// Fast-forwards note states when jumping to a practice measure.
    pub fn advance_to_time(&mut self, start_time_seconds: f64) {
        for i in 0..self.notes.len() {
            if self.notes[i].target_time_seconds < start_time_seconds {
                self.notes[i].is_judged = true;
                // A long note whose head is skipped is skipped whole.
                if let Some(tail) = self.notes[i].tail_index {
                    self.notes[tail].is_judged = true;
                }
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
        let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Groove, Ruleset::CN);

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
        let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Groove, Ruleset::CN);

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
        let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Groove, Ruleset::CN);

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
    fn auto_play_sounds_list_the_notes_due_in_the_window() {
        let tap = |measure, lane, wav| NoteEvent {
            measure,
            fraction: 0.0,
            lane,
            wav_id: Some(WavId(wav)),
            note_type: NoteType::Tap,
        };
        let chart = BmsChart {
            header: BmsHeader {
                bpm: 120.0,
                ..Default::default()
            },
            // 2 s a measure: due at 2 s, 4 s and 6 s, and a mine at 4 s.
            notes: vec![
                tap(1, Lane::Key1, 1),
                tap(2, Lane::Key2, 2),
                NoteEvent {
                    note_type: NoteType::Landmine,
                    ..tap(2, Lane::Key3, 9)
                },
                tap(3, Lane::Key1, 3),
            ],
            ..Default::default()
        };
        let timing = TimingModel::from_chart(&chart);
        let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Groove, Ruleset::CN);

        let sounds = |e: &JudgeEngine, a, b| -> Vec<WavId> {
            e.auto_play_sounds(a, b)
                .into_iter()
                .map(|(_, w)| w)
                .collect()
        };
        assert_eq!(sounds(&engine, f64::NEG_INFINITY, 1.9), []);
        // The window is open at its start and closed at its end.
        assert_eq!(sounds(&engine, 1.9, 4.0), [WavId(1), WavId(2)]);
        assert_eq!(sounds(&engine, 4.0, 6.0), [WavId(3)]);
        // Each sound comes with its note's time.
        assert_eq!(engine.auto_play_sounds(5.0, 7.0), [(6.0, WavId(3))]);

        // Hit notes are not listed again.
        engine.auto_play_update(4.0);
        assert_eq!(sounds(&engine, f64::NEG_INFINITY, 10.0), [WavId(3)]);
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
        let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Groove, Ruleset::CN);
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
        parse_bms(
            "#BPM 120
#001D1:0A
#00212:01
",
        )
        .unwrap()
    }

    #[test]
    fn stepping_on_a_mine_drains_the_gauge_but_not_combo_or_ex() {
        let chart = mine_chart();
        let timing = TimingModel::from_chart(&chart);
        let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Hard, Ruleset::CN);
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
        let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Hard, Ruleset::CN);
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
        let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Hard, Ruleset::CN);
        engine.handle_key_down(Lane::Key1, 2.3);
        assert_eq!(engine.score().mine_hit_count, 0);
    }

    #[test]
    fn a_key_held_through_a_mine_sets_it_off() {
        let chart = mine_chart();
        let timing = TimingModel::from_chart(&chart);
        let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Hard, Ruleset::CN);
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
        let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Hard, Ruleset::CN);
        engine.handle_key_down(Lane::Key1, 1.0);
        engine.handle_key_up(Lane::Key1, 1.2);
        engine.update_misses(2.0);
        assert_eq!(engine.score().mine_hit_count, 0);
    }

    #[test]
    fn auto_play_skips_mines_and_still_scores_perfectly() {
        let chart = mine_chart();
        let timing = TimingModel::from_chart(&chart);
        let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Hard, Ruleset::CN);
        engine.auto_play_update(10.0);
        let score = engine.score();
        assert_eq!(score.mine_hit_count, 0);
        assert_eq!(score.pgreat_count, 1);
        assert_eq!(score.accuracy_rate(), 100.0);
    }

    /// One long note on lane 1: head at 2.0s, tail at 3.0s (120 BPM).
    fn ln_chart() -> BmsChart {
        parse_bms(
            "#BPM 120
#00151:01000100
",
        )
        .unwrap()
    }

    fn ln_engine() -> JudgeEngine {
        let chart = ln_chart();
        let timing = TimingModel::from_chart(&chart);
        JudgeEngine::new(&chart, &timing, GaugeType::Hard, Ruleset::CN)
    }

    #[test]
    fn releasing_a_held_long_note_on_time_judges_the_tail() {
        let mut engine = ln_engine();
        engine.handle_key_down(Lane::Key1, 2.0);
        let result = engine.handle_key_up(Lane::Key1, 3.01).expect("tail judged");
        assert_eq!(result.grade, JudgeGrade::PerfectGreat);
        assert_eq!(engine.score().pgreat_count, 2);
        assert_eq!(engine.score().max_combo, 2);
        // Nothing left over to miss afterwards.
        assert!(engine.update_misses(10.0).is_empty());
        assert_eq!(engine.score().miss_count, 0);
    }

    #[test]
    fn letting_go_far_too_early_is_a_miss_immediately() {
        let mut engine = ln_engine();
        engine.handle_key_down(Lane::Key1, 2.0);
        let result = engine
            .handle_key_up(Lane::Key1, 2.3)
            .expect("tail judged now");
        assert_eq!(result.grade, JudgeGrade::Miss);
        assert_eq!(engine.score().miss_count, 1);
        assert_eq!(engine.score().current_combo, 0);
        // It does not miss a second time when the tail's time comes around.
        assert!(engine.update_misses(10.0).is_empty());
        assert_eq!(engine.score().miss_count, 1);
    }

    #[test]
    fn pressing_again_near_the_tail_does_not_hit_it() {
        let mut engine = ln_engine();
        engine.handle_key_down(Lane::Key1, 2.0);
        engine.handle_key_up(Lane::Key1, 2.3); // early release: tail missed
        let before = engine.score().pgreat_count;
        assert!(engine.handle_key_down(Lane::Key1, 3.0).is_none());
        assert_eq!(engine.score().pgreat_count, before);
    }

    #[test]
    fn a_missed_head_misses_the_tail_with_it() {
        let mut engine = ln_engine();
        let misses = engine.update_misses(2.5); // head's POOR window is over
        assert_eq!(misses.len(), 2);
        assert_eq!(engine.score().miss_count, 2);
        // Releasing a key near the tail must not give a free judgment.
        assert!(engine.handle_key_up(Lane::Key1, 3.0).is_none());
        assert!(engine.update_misses(10.0).is_empty());
        assert_eq!(engine.score().miss_count, 2);
    }

    #[test]
    fn release_without_holding_the_head_judges_nothing() {
        let mut engine = ln_engine();
        // Key goes down long before the head's window, so the head is not hit.
        assert!(engine.handle_key_down(Lane::Key1, 0.5).is_none());
        assert!(engine.handle_key_up(Lane::Key1, 3.0).is_none());
        assert_eq!(engine.score().pgreat_count, 0);
    }

    #[test]
    fn holding_well_past_the_tail_still_misses_it() {
        let mut engine = ln_engine();
        engine.handle_key_down(Lane::Key1, 2.0);
        assert!(engine.update_misses(3.2).is_empty());
        let misses = engine.update_misses(3.5);
        assert_eq!(misses.len(), 1);
        assert_eq!(misses[0].1.grade, JudgeGrade::Miss);
        // The late release afterwards is not judged again.
        assert!(engine.handle_key_up(Lane::Key1, 3.6).is_none());
        assert_eq!(engine.score().miss_count, 1);
    }

    #[test]
    fn practice_jump_past_a_head_skips_the_whole_long_note() {
        let mut engine = ln_engine();
        engine.advance_to_time(2.5);
        assert!(engine.update_misses(10.0).is_empty());
        assert_eq!(engine.score().miss_count, 0);
    }

    #[test]
    fn clear_lamp_follows_the_gauge_that_was_cleared() {
        use crate::score::ClearType;
        // Enough perfect hits to clear any gauge, then one miss to rule out full combo.
        let lamp = |gauge| {
            let mut score = ScoreTracker::new(100, 300.0, gauge);
            for _ in 0..99 {
                score.record_hit(JudgeGrade::PerfectGreat);
            }
            score.record_hit(JudgeGrade::Miss);
            score.clear_type()
        };
        assert_eq!(lamp(GaugeType::Easy), ClearType::Easy);
        assert_eq!(lamp(GaugeType::Groove), ClearType::Clear);
        assert_eq!(lamp(GaugeType::Hard), ClearType::Hard);
        // Hazard fails on the first miss, so only a clean run clears it.
        assert_eq!(lamp(GaugeType::Hazard), ClearType::Failed);

        // A full combo is a full combo on any gauge.
        let mut easy = ScoreTracker::new(10, 300.0, GaugeType::Easy);
        for _ in 0..10 {
            easy.record_hit(JudgeGrade::Great);
        }
        assert_eq!(easy.clear_type(), ClearType::FullCombo);
    }

    // ---- the LN rule: one judgment per long note, none for the tail ----

    fn ln_rule_engine() -> JudgeEngine {
        let chart = ln_chart();
        let timing = TimingModel::from_chart(&chart);
        JudgeEngine::new(&chart, &timing, GaugeType::Hard, Ruleset::LN)
    }

    #[test]
    fn the_ln_rule_counts_a_long_note_once_and_cn_counts_it_twice() {
        assert_eq!(ln_rule_engine().score().total_notes, 1);
        assert_eq!(ln_engine().score().total_notes, 2);
        // A tap and a long note: 2 under LN, 3 under CN.
        let chart = parse_bms("#BPM 120\n#00111:01\n#00251:01000100\n").unwrap();
        let timing = TimingModel::from_chart(&chart);
        let total = |rule| {
            JudgeEngine::new(&chart, &timing, GaugeType::Groove, rule)
                .score()
                .total_notes
        };
        assert_eq!((total(Ruleset::LN), total(Ruleset::CN)), (2, 3));
    }

    #[test]
    fn every_screen_count_matches_the_judge_under_each_ln_option() {
        use crate::library::SongMetadata;
        use crate::rules::LnOption;
        // Plain LN, LNMODE 2 (CN), LNMODE 3 (HCN, played as CN), and an LNOBJ
        // chart whose lone `02` on lane 2 is a tail with no head.
        let charts: [(&str, &str); 4] = [
            (
                "#BPM 120
#00111:01
#00251:01000100
",
                "tap + long note",
            ),
            (
                "#BPM 120
#LNMODE 2
#00111:01
#00251:01000100
",
                "LNMODE 2",
            ),
            (
                "#BPM 120
#LNMODE 3
#00111:01
#00251:01000100
",
                "LNMODE 3",
            ),
            (
                "#BPM 120
#LNOBJ 02
#00111:0102
#00112:02
",
                "LNOBJ with orphan tail",
            ),
        ];
        for (text, name) in charts {
            let song = SongMetadata::from_bytes("t.bms", text.as_bytes()).unwrap();
            let chart = parse_bms(text).unwrap();
            let timing = TimingModel::from_chart(&chart);
            for option in [LnOption::Auto, LnOption::Ln, LnOption::Cn] {
                // The play that would start now: its rule, judged by the judge.
                let ruleset = Ruleset::resolve(song.ln_mode, option);
                let judged = JudgeEngine::new(&chart, &timing, GaugeType::Groove, ruleset)
                    .score()
                    .total_notes as usize;
                // Select detail (`notes_for`) and loading card (`notes_count_for` of the play's rule).
                assert_eq!(song.notes_for(option), judged, "{name}, {option:?}: detail");
                assert_eq!(
                    song.notes_count_for(ruleset.ln),
                    judged,
                    "{name}, {option:?}: loading"
                );
            }
        }
        // Spot values: AUTO on a chart with no #LNMODE is LN, which drops the tail.
        let plain = SongMetadata::from_bytes("t.bms", charts[0].0.as_bytes()).unwrap();
        assert_eq!(
            (
                plain.notes_for(LnOption::Auto),
                plain.notes_for(LnOption::Cn)
            ),
            (2, 3)
        );
    }

    #[test]
    fn auto_play_holds_a_long_note_from_head_to_tail() {
        let mut engine = ln_engine();
        engine.auto_play_update(2.0);
        let head = engine
            .notes()
            .iter()
            .find(|n| n.tail_index.is_some())
            .unwrap();
        assert!(
            head.is_judged && head.is_holding,
            "the renderer draws a held note, not a broken one"
        );
        engine.auto_play_update(3.5);
        assert!(engine.notes().iter().all(|n| !n.is_holding));
    }

    #[test]
    fn holding_through_the_tail_completes_the_note_for_free() {
        let mut engine = ln_rule_engine();
        engine.handle_key_down(Lane::Key1, 2.0);
        // Held on past the tail, and well past its POOR window: no penalty.
        assert!(engine.update_misses(3.5).is_empty());
        assert!(engine.update_misses(10.0).is_empty());

        let score = engine.score();
        assert_eq!(
            (score.pgreat_count, score.miss_count, score.poor_count),
            (1, 0, 0)
        );
        assert_eq!(score.ex_score, score.max_ex_score());
        assert_eq!(score.max_combo, 1);
        assert_eq!(score.accuracy_rate(), 100.0);
        assert!(
            engine.notes().iter().all(|n| !n.is_holding),
            "the hold ended with the note"
        );
        // Letting go after the tail changes nothing either.
        assert!(engine.handle_key_up(Lane::Key1, 10.5).is_none());
        assert_eq!(engine.score().poor_count, 0);
    }

    #[test]
    fn letting_go_within_the_good_window_of_the_tail_completes_the_note() {
        let slack = JudgeWindow::from_rank(2).good_ms / 1000.0;
        for release in [3.0 - slack + 0.005, 3.0, 3.2] {
            let mut engine = ln_rule_engine();
            engine.handle_key_down(Lane::Key1, 2.0);
            assert!(
                engine.handle_key_up(Lane::Key1, release).is_none(),
                "release at {release}"
            );
            let score = engine.score();
            assert_eq!(
                (score.pgreat_count, score.poor_count, score.miss_count),
                (1, 0, 0)
            );
            assert_eq!(score.current_combo, 1);
        }
    }

    #[test]
    fn letting_go_earlier_breaks_the_note_as_one_poor() {
        let slack = JudgeWindow::from_rank(2).good_ms / 1000.0;
        let mut engine = ln_rule_engine();
        engine.handle_key_down(Lane::Key1, 2.0);
        let result = engine
            .handle_key_up(Lane::Key1, 3.0 - slack - 0.01)
            .expect("a break is reported");
        assert_eq!(result.grade, JudgeGrade::Poor);

        let score = engine.score();
        assert_eq!(score.poor_count, 1);
        assert_eq!(score.current_combo, 0, "the combo breaks");
        assert_eq!(score.max_combo, 1);
        assert_eq!(score.ex_score, 2, "the head's EX stays; a break adds none");
        assert_eq!(score.pgreat_count, 1);

        // Nothing more happens to it: not at the tail's time, not on a re-press.
        assert!(engine.update_misses(10.0).is_empty());
        assert!(engine.handle_key_down(Lane::Key1, 2.9).is_none());
        assert!(engine.handle_key_up(Lane::Key1, 3.0).is_none());
        assert_eq!(engine.score().poor_count, 1);
        assert_eq!(engine.score().miss_count, 0);
    }

    #[test]
    fn a_missed_head_under_the_ln_rule_is_a_single_miss() {
        let mut engine = ln_rule_engine();
        let misses = engine.update_misses(2.5);
        assert_eq!(misses.len(), 1, "the head only");
        assert_eq!(engine.score().miss_count, 1);
        // The tail neither misses later nor can a release near it score.
        assert!(engine.update_misses(10.0).is_empty());
        assert!(engine.handle_key_up(Lane::Key1, 3.0).is_none());
        assert_eq!(engine.score().miss_count, 1);
        assert_eq!(engine.score().poor_count, 0);
    }

    #[test]
    fn auto_play_under_the_ln_rule_is_a_perfect_run() {
        let mut engine = ln_rule_engine();
        let hits = engine.auto_play_update(10.0);
        assert_eq!(hits.len(), 1, "only the head is a judgment");
        let score = engine.score();
        assert_eq!(score.pgreat_count, 1);
        assert_eq!(score.ex_score, score.max_ex_score());
        assert_eq!(score.accuracy_rate(), 100.0);
    }

    #[test]
    fn the_ln_rule_does_not_change_taps_or_mines() {
        let chart = mine_chart();
        let timing = TimingModel::from_chart(&chart);
        for rule in [Ruleset::LN, Ruleset::CN] {
            let mut engine = JudgeEngine::new(&chart, &timing, GaugeType::Hard, rule);
            engine.handle_key_down(Lane::Key2, 4.0);
            engine.handle_key_down(Lane::Key1, 2.01);
            let score = engine.score();
            assert_eq!(
                (score.pgreat_count, score.mine_hit_count, score.total_notes),
                (1, 1, 1),
                "{rule:?}"
            );
        }
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

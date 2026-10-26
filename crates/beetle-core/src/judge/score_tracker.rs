use super::JudgeGrade;
use crate::score::ClearType;

/// Gauge difficulty / health drain type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GaugeType {
    Easy,
    #[default]
    Groove,
    Hard,
    Hazard,
    /// Dan course gauges (beatoraja CLASS / EXCLASS / EXHARDCLASS): start
    /// full, fixed gains, fail at 0, carried from song to song.
    Class,
    ExClass,
    ExHardClass,
}

/// Which numbers a dan gauge uses: beatoraja's own, or its LR2 set
/// (the `gauge_lr2` course rule).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClassGaugeSet {
    #[default]
    Beatoraja,
    Lr2,
}

/// A dan gauge's numbers, as beatoraja's `GaugeProperty` gives them.
#[derive(Debug, Clone)]
struct ClassGaugeTable {
    /// Change per judgement, in beatoraja's order: PG, GR, GD, BD, PR (a
    /// note that passed: `Miss` here), MS (an empty poor: `Poor` here).
    values: [f64; 6],
    /// Damage relief: below the first `gauge` it is under, damage is
    /// multiplied by `factor`.
    guts: &'static [(f64, f64)],
}

impl ClassGaugeTable {
    fn of(gauge: GaugeType, set: ClassGaugeSet) -> Option<Self> {
        use ClassGaugeSet::*;
        let (values, guts): ([f64; 6], &'static [(f64, f64)]) = match (gauge, set) {
            (GaugeType::Class, Beatoraja) => (
                [0.15, 0.12, 0.06, -1.5, -3.0, -1.5],
                &[
                    (5.0, 0.4),
                    (10.0, 0.5),
                    (15.0, 0.6),
                    (20.0, 0.7),
                    (25.0, 0.8),
                ],
            ),
            (GaugeType::ExClass, Beatoraja) => ([0.15, 0.12, 0.03, -3.0, -6.0, -3.0], &[]),
            (GaugeType::ExHardClass, Beatoraja) => ([0.15, 0.06, 0.0, -5.0, -10.0, -5.0], &[]),
            (GaugeType::Class, Lr2) => ([0.10, 0.10, 0.05, -2.0, -3.0, -2.0], &[(30.0, 0.6)]),
            (GaugeType::ExClass, Lr2) => ([0.10, 0.10, 0.05, -6.0, -10.0, -2.0], &[(30.0, 0.6)]),
            (GaugeType::ExHardClass, Lr2) => ([0.10, 0.10, 0.05, -12.0, -20.0, -2.0], &[]),
            _ => return None,
        };
        // f32 in beatoraja; rounded the same way so long runs add up alike.
        Some(Self {
            values: values.map(|v| v as f32 as f64),
            guts,
        })
    }

    fn change(&self, grade: JudgeGrade, gauge: f64) -> f64 {
        let value = self.values[match grade {
            JudgeGrade::PerfectGreat => 0,
            JudgeGrade::Great => 1,
            JudgeGrade::Good => 2,
            JudgeGrade::Bad => 3,
            JudgeGrade::Miss => 4,
            JudgeGrade::Poor => 5,
        }];
        if value >= 0.0 {
            return value;
        }
        let factor = self
            .guts
            .iter()
            .find(|(below, _)| gauge < *below)
            .map_or(1.0, |(_, f)| *f);
        value * factor
    }
}

impl GaugeType {
    /// The gauge whose `as_str` name this is.
    pub fn from_name(name: &str) -> Option<Self> {
        [
            Self::Easy,
            Self::Groove,
            Self::Hard,
            Self::Hazard,
            Self::Class,
            Self::ExClass,
            Self::ExHardClass,
        ]
        .into_iter()
        .find(|g| g.as_str() == name)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Easy => "EASY",
            Self::Groove => "GROOVE",
            Self::Hard => "HARD",
            Self::Hazard => "HAZARD",
            Self::Class => "CLASS",
            Self::ExClass => "EXCLASS",
            Self::ExHardClass => "EXHARDCLASS",
        }
    }

    /// Fails the moment it empties, instead of being judged at the end.
    pub fn is_survival(self) -> bool {
        !matches!(self, Self::Easy | Self::Groove)
    }

    pub fn is_class(self) -> bool {
        matches!(self, Self::Class | Self::ExClass | Self::ExHardClass)
    }

    /// The dan gauge a course plays with for the gauge option the player
    /// chose, as beatoraja maps it.
    pub fn for_course(self) -> Self {
        match self {
            Self::Easy | Self::Groove | Self::Class => Self::Class,
            Self::Hard | Self::ExClass => Self::ExClass,
            Self::Hazard | Self::ExHardClass => Self::ExHardClass,
        }
    }
}

/// Tracks score, combo, EX-score, accuracy, health gauge, and timing distribution stats.
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
    pub gauge: f64, // 0.0 ~ 100.0%
    pub gauge_type: GaugeType,
    pub is_failed: bool,
    pub fast_count: u32,
    pub slow_count: u32,
    /// Landmines stepped on (each one already cost gauge; they never touch combo or EX).
    pub mine_hit_count: u32,
    pub timing_histogram: [u32; 17], // -40ms ~ +40ms binned in 5ms buckets
    gauge_gain_pgreat: f64,
    gauge_gain_great: f64,
    gauge_gain_good: f64,
    class_table: Option<ClassGaugeTable>,
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
        let multiplier = if gauge_type == GaugeType::Easy {
            1.2
        } else {
            1.0
        };
        let gain_pgreat = (effective_total / n) * multiplier;
        let gain_great = gain_pgreat * 0.8;
        let gain_good = gain_pgreat * 0.5;

        let initial_gauge = if gauge_type.is_survival() {
            100.0
        } else {
            20.0
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
            fast_count: 0,
            slow_count: 0,
            mine_hit_count: 0,
            timing_histogram: [0; 17],
            gauge_gain_pgreat: gain_pgreat,
            gauge_gain_great: gain_great,
            gauge_gain_good: gain_good,
            class_table: ClassGaugeTable::of(gauge_type, ClassGaugeSet::Beatoraja),
        }
    }

    /// A dan gauge tracker for one song of a course, starting from the gauge
    /// the previous song left (100 for the first). A `gauge_type` that is
    /// not a dan gauge is taken as the dan gauge it maps to.
    pub fn for_course(
        total_notes: u32,
        total: f64,
        gauge_type: GaugeType,
        set: ClassGaugeSet,
        carried_gauge: f64,
    ) -> Self {
        let gauge_type = gauge_type.for_course();
        let mut tracker = Self::new(total_notes, total, gauge_type);
        tracker.class_table = ClassGaugeTable::of(gauge_type, set);
        tracker.gauge = carried_gauge.clamp(0.0, 100.0);
        tracker.is_failed = tracker.gauge <= 0.0;
        tracker
    }

    /// Records a judge result without explicit timing delta.
    pub fn record_hit(&mut self, grade: JudgeGrade) {
        self.record_hit_with_delta(grade, 0.0);
    }

    /// Records a judge result and updates score, gauge, and timing distribution stats.
    pub fn record_hit_with_delta(&mut self, grade: JudgeGrade, delta_ms: f64) {
        if self.is_failed && self.gauge_type.is_survival() {
            return;
        }

        self.ex_score += grade.ex_score_points();

        if grade != JudgeGrade::Miss && grade != JudgeGrade::Poor {
            if delta_ms < -4.0 {
                self.fast_count += 1;
            } else if delta_ms > 4.0 {
                self.slow_count += 1;
            }

            let bin = ((delta_ms + 42.5) / 5.0).floor().clamp(0.0, 16.0) as usize;
            self.timing_histogram[bin] += 1;
        }

        if let Some(table) = &self.class_table {
            let change = table.change(grade, self.gauge);
            self.count(grade);
            self.apply_gauge_delta(change);
            return;
        }

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
                    // Dan gauges take the table path above.
                    GaugeType::Class | GaugeType::ExClass | GaugeType::ExHardClass => 0.0,
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
                    // Dan gauges take the table path above.
                    GaugeType::Class | GaugeType::ExClass | GaugeType::ExHardClass => 0.0,
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
                    // Dan gauges take the table path above.
                    GaugeType::Class | GaugeType::ExClass | GaugeType::ExHardClass => 0.0,
                };
                self.apply_gauge_delta(penalty);
            }
        }

        if self.current_combo > self.max_combo {
            self.max_combo = self.current_combo;
        }
    }

    /// Counts and combo for a judgement, without the gauge.
    fn count(&mut self, grade: JudgeGrade) {
        match grade {
            JudgeGrade::PerfectGreat => self.pgreat_count += 1,
            JudgeGrade::Great => self.great_count += 1,
            JudgeGrade::Good => self.good_count += 1,
            JudgeGrade::Bad => self.bad_count += 1,
            JudgeGrade::Poor => self.poor_count += 1,
            JudgeGrade::Miss => self.miss_count += 1,
        }
        if grade.is_combo_breaker() {
            self.current_combo = 0;
        } else {
            self.current_combo += 1;
            self.max_combo = self.max_combo.max(self.current_combo);
        }
    }

    /// A landmine was stepped on: the gauge drops by `damage_percent`, and the
    /// combo and EX score are left alone.
    pub fn record_mine_hit(&mut self, damage_percent: f64) {
        if self.is_failed && self.gauge_type.is_survival() {
            return;
        }
        self.mine_hit_count += 1;
        self.apply_gauge_delta(-damage_percent);
    }

    fn apply_gauge_delta(&mut self, delta: f64) {
        self.gauge += delta;
        if self.gauge > 100.0 {
            self.gauge = 100.0;
        }
        if !self.gauge_type.is_survival() {
            if self.gauge < 2.0 {
                self.gauge = 2.0;
            }
        } else if self.gauge <= 0.0 {
            self.gauge = 0.0;
            self.is_failed = true;
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
        if self.gauge_type.is_survival() {
            !self.is_failed
        } else {
            self.gauge >= 80.0
        }
    }

    /// Clear lamp earned by this play (call at the end of the song).
    pub fn clear_type(&self) -> ClearType {
        if !self.is_cleared() {
            ClearType::Failed
        } else if self.bad_count + self.poor_count + self.miss_count > 0 {
            match self.gauge_type {
                GaugeType::Easy => ClearType::Easy,
                GaugeType::Groove => ClearType::Clear,
                // Unreachable for Hazard (it already failed on that miss); Hard is the nearest rung.
                GaugeType::Hard | GaugeType::Hazard => ClearType::Hard,
                // A course's lamp is the course's; one song of it is just cleared.
                GaugeType::Class | GaugeType::ExClass | GaugeType::ExHardClass => ClearType::Clear,
            }
        } else if self.great_count + self.good_count > 0 {
            ClearType::FullCombo
        } else {
            ClearType::Perfect
        }
    }

    /// Evaluates current performance rank string (MAX, AAA, AA, A, B, C, D, F).
    pub fn rank(&self) -> &'static str {
        let max_possible = self.max_ex_score();
        if max_possible == 0 {
            return "F";
        }
        let ratio = self.ex_score as f64 / max_possible as f64;
        if ratio >= 1.0 {
            "MAX"
        } else if ratio >= 8.0 / 9.0 {
            "AAA"
        } else if ratio >= 7.0 / 9.0 {
            "AA"
        } else if ratio >= 6.0 / 9.0 {
            "A"
        } else if ratio >= 5.0 / 9.0 {
            "B"
        } else if ratio >= 4.0 / 9.0 {
            "C"
        } else if ratio >= 3.0 / 9.0 {
            "D"
        } else {
            "F"
        }
    }
}

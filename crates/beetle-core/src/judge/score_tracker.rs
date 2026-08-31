use super::JudgeGrade;

/// Gauge difficulty / health drain type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GaugeType {
    Easy,
    #[default]
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
    pub timing_histogram: [u32; 17], // -40ms ~ +40ms binned in 5ms buckets
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
            fast_count: 0,
            slow_count: 0,
            timing_histogram: [0; 17],
            gauge_gain_pgreat: gain_pgreat,
            gauge_gain_great: gain_great,
            gauge_gain_good: gain_good,
        }
    }

    /// Records a judge result without explicit timing delta.
    pub fn record_hit(&mut self, grade: JudgeGrade) {
        self.record_hit_with_delta(grade, 0.0);
    }

    /// Records a judge result and updates score, gauge, and timing distribution stats.
    pub fn record_hit_with_delta(&mut self, grade: JudgeGrade, delta_ms: f64) {
        if self.is_failed && matches!(self.gauge_type, GaugeType::Hard | GaugeType::Hazard) {
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

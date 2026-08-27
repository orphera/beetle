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
    /// Default EASY/NORMAL BMS judgment window preset.
    fn default() -> Self {
        Self {
            pgreat_ms: 18.0,
            great_ms: 40.0,
            good_ms: 100.0,
            bad_ms: 200.0,
            poor_ms: 300.0,
        }
    }
}

/// Result of a single note judgment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JudgeResult {
    pub grade: JudgeGrade,
    pub delta_ms: f64,
}

/// Tracks realtime score, combo, and EX score during gameplay.
#[derive(Debug, Clone, Default)]
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
    pub gauge: f64,
}

impl ScoreTracker {
    pub fn new() -> Self {
        Self {
            gauge: 100.0,
            ..Default::default()
        }
    }

    /// Records a judge result and updates combo and EX score.
    pub fn record_hit(&mut self, grade: JudgeGrade) {
        match grade {
            JudgeGrade::PerfectGreat => {
                self.pgreat_count += 1;
                self.current_combo += 1;
                self.ex_score += 2;
            }
            JudgeGrade::Great => {
                self.great_count += 1;
                self.current_combo += 1;
                self.ex_score += 1;
            }
            JudgeGrade::Good => {
                self.good_count += 1;
                self.current_combo += 1;
            }
            JudgeGrade::Bad | JudgeGrade::Poor | JudgeGrade::Miss => {
                if grade == JudgeGrade::Bad {
                    self.bad_count += 1;
                } else if grade == JudgeGrade::Poor {
                    self.poor_count += 1;
                } else {
                    self.miss_count += 1;
                }
                self.current_combo = 0;
            }
        }
        if self.current_combo > self.max_combo {
            self.max_combo = self.current_combo;
        }
    }
}

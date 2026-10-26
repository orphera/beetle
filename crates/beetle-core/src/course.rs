//! Courses: charts played back to back as one run, and dan certification
//! (段位認定) courses that add rules such as a carried-over gauge and locked
//! options.
//!
//! The shape follows the `course` field of beatoraja difficulty-table
//! headers, which is where most courses come from. A course looks the same
//! whatever it came from (`CourseSource`), so disc and local courses can be
//! added without touching the rest.

use crate::identity::ChartId;
use crate::judge::{ClassGaugeSet, GaugeType, JudgeGrade, ScoreTracker};
use bms_hash::Sha256Hasher;

/// A rule a course imposes, by its beatoraja `constraint` name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Constraint {
    /// Dan certification, no lane options.
    Grade,
    /// Dan certification, mirror allowed.
    GradeMirror,
    /// Dan certification, random allowed.
    GradeRandom,
    /// Hi-speed cannot be changed during play.
    NoSpeed,
    /// GOOD and below count as a miss.
    NoGood,
    /// GREAT and below count as a miss.
    NoGreat,
    /// Which gauge numbers to use.
    Gauge(GaugeRules),
    /// Long notes are played as this rule.
    LongNote(LongNoteRule),
    /// A name this version does not know, kept so it can be shown and saved again.
    Unknown(String),
}

/// The gauge number sets beatoraja selects with `gauge_*` constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GaugeRules {
    Lr2,
    Keys5,
    Keys7,
    Keys9,
    Keys24,
}

/// `ln` / `cn` / `hcn`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LongNoteRule {
    Ln,
    Cn,
    Hcn,
}

const NAMES: [(&str, Constraint); 14] = [
    ("grade", Constraint::Grade),
    ("grade_mirror", Constraint::GradeMirror),
    ("grade_random", Constraint::GradeRandom),
    ("no_speed", Constraint::NoSpeed),
    ("no_good", Constraint::NoGood),
    ("no_great", Constraint::NoGreat),
    ("gauge_lr2", Constraint::Gauge(GaugeRules::Lr2)),
    ("gauge_5k", Constraint::Gauge(GaugeRules::Keys5)),
    ("gauge_7k", Constraint::Gauge(GaugeRules::Keys7)),
    ("gauge_9k", Constraint::Gauge(GaugeRules::Keys9)),
    ("gauge_24k", Constraint::Gauge(GaugeRules::Keys24)),
    ("ln", Constraint::LongNote(LongNoteRule::Ln)),
    ("cn", Constraint::LongNote(LongNoteRule::Cn)),
    ("hcn", Constraint::LongNote(LongNoteRule::Hcn)),
];

impl Constraint {
    /// Never fails: a name it does not know is `Unknown`.
    pub fn from_name(name: &str) -> Self {
        let name = name.trim();
        NAMES
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map_or_else(|| Self::Unknown(name.to_string()), |(_, c)| c.clone())
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Unknown(name) => name,
            known => NAMES
                .iter()
                .find(|(_, c)| c == known)
                .map_or("", |(n, _)| n),
        }
    }

    /// True for the rules that make a course a dan certification.
    pub fn is_grade(&self) -> bool {
        matches!(self, Self::Grade | Self::GradeMirror | Self::GradeRandom)
    }

    /// True for rules the player does not apply yet. A course with one can
    /// still be listed, but is marked.
    pub fn is_supported(&self) -> bool {
        !matches!(self, Self::NoGood | Self::NoGreat | Self::Unknown(_))
    }
}

/// A medal for passing a course well enough: both limits must be met.
#[derive(Debug, Clone, PartialEq)]
pub struct Trophy {
    /// `bronzemedal`, `silvermedal`, `goldmedal`, or whatever the table names.
    pub name: String,
    /// Most misses allowed, as a percentage of the notes.
    pub max_miss_rate: f32,
    /// Least EX score needed, as a percentage of the maximum.
    pub min_score_rate: f32,
}

/// One chart of a course, as the source names it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CourseChart {
    pub md5: Option<[u8; 16]>,
    pub sha256: Option<ChartId>,
    pub title: String,
}

/// A course, wherever it came from.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CourseDef {
    pub name: String,
    pub charts: Vec<CourseChart>,
    pub constraints: Vec<Constraint>,
    pub trophies: Vec<Trophy>,
}

impl CourseDef {
    pub fn is_grade(&self) -> bool {
        self.constraints.iter().any(Constraint::is_grade)
    }

    /// Rules in this course the player does not apply yet.
    pub fn unsupported(&self) -> impl Iterator<Item = &Constraint> {
        self.constraints.iter().filter(|c| !c.is_supported())
    }
}

/// Where a course came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CourseSource {
    /// A difficulty table's header, by index into the installed tables.
    Table(usize),
    /// A console game disc's own dan courses.
    Disc,
    /// A course file the player made.
    Local,
}

/// Identity of a course for its records: the SHA-256 of its charts' ids in
/// order. The same charts in the same order are the same course, whatever
/// the course is called or where it came from.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CourseId(pub [u8; 32]);

impl CourseId {
    pub fn of_charts(charts: &[ChartId]) -> Self {
        let mut hasher = Sha256Hasher::new();
        for chart in charts {
            hasher.update(chart.as_bytes());
        }
        Self(hasher.finalize())
    }

    pub fn to_hex(&self) -> String {
        bms_hash::to_hex(&self.0)
    }
}

/// What one song of a course left behind.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StageResult {
    pub total_notes: u32,
    pub ex_score: u32,
    pub pgreat: u32,
    pub great: u32,
    pub good: u32,
    pub bad: u32,
    pub poor: u32,
    pub miss: u32,
    pub max_combo: u32,
    /// Gauge when the song ended (or emptied).
    pub gauge: f64,
}

impl StageResult {
    pub fn of(score: &ScoreTracker) -> Self {
        Self {
            total_notes: score.total_notes,
            ex_score: score.ex_score,
            pgreat: score.pgreat_count,
            great: score.great_count,
            good: score.good_count,
            bad: score.bad_count,
            poor: score.poor_count,
            miss: score.miss_count,
            max_combo: score.max_combo,
            gauge: score.gauge,
        }
    }

    /// BAD, POOR and MISS together, as the miss count beatoraja shows.
    pub fn misses(&self) -> u32 {
        self.bad + self.poor + self.miss
    }
}

/// One play of a course: songs in order, the dan gauge carried between
/// them, and what each song left. The app makes each song's `ScoreTracker`
/// with `tracker_for_next` and hands it back with `finish_stage`; the
/// timing of each song is its own (its own audio clock), only the gauge
/// and the results carry over.
#[derive(Debug, Clone)]
pub struct CourseRun {
    stage_count: usize,
    constraints: Vec<Constraint>,
    gauge_type: GaugeType,
    gauge_set: ClassGaugeSet,
    gauge: f64,
    failed: bool,
    stages: Vec<StageResult>,
}

impl CourseRun {
    /// `gauge_option` is the player's gauge choice; the course plays the dan
    /// gauge it maps to.
    pub fn new(course: &CourseDef, gauge_option: GaugeType) -> Self {
        let gauge_set = if course
            .constraints
            .contains(&Constraint::Gauge(GaugeRules::Lr2))
        {
            ClassGaugeSet::Lr2
        } else {
            ClassGaugeSet::Beatoraja
        };
        Self {
            stage_count: course.charts.len(),
            constraints: course.constraints.clone(),
            gauge_type: gauge_option.for_course(),
            gauge_set,
            gauge: 100.0,
            failed: false,
            stages: Vec::new(),
        }
    }

    pub fn gauge_type(&self) -> GaugeType {
        self.gauge_type
    }

    pub fn constraints(&self) -> &[Constraint] {
        &self.constraints
    }

    /// Index of the song to play next (equals the song count once finished).
    pub fn stage(&self) -> usize {
        self.stages.len()
    }

    pub fn stage_count(&self) -> usize {
        self.stage_count
    }

    pub fn stages(&self) -> &[StageResult] {
        &self.stages
    }

    /// Gauge carried into the next song.
    pub fn gauge(&self) -> f64 {
        self.gauge
    }

    /// No more songs to play: all were played, or the gauge emptied.
    pub fn is_finished(&self) -> bool {
        self.failed || self.stages.len() >= self.stage_count
    }

    /// Every song played and the gauge never emptied.
    pub fn is_passed(&self) -> bool {
        !self.failed && self.stages.len() >= self.stage_count
    }

    /// The tracker for the next song, starting from the carried gauge.
    pub fn tracker_for_next(&self, total_notes: u32, total: f64) -> ScoreTracker {
        ScoreTracker::for_course(
            total_notes,
            total,
            self.gauge_type,
            self.gauge_set,
            self.gauge,
        )
    }

    /// Where every judgement of a course song passes before the tracker
    /// counts it. Course rules that change judgements (`no_good`,
    /// `no_great`) belong here; none is applied yet, so this is the grade
    /// as judged.
    pub fn judge(&self, grade: JudgeGrade) -> JudgeGrade {
        grade
    }

    /// Records the song just played (finished, or ended by an empty gauge).
    /// Ignored once the run is finished.
    pub fn finish_stage(&mut self, score: &ScoreTracker) {
        if self.is_finished() {
            return;
        }
        self.gauge = score.gauge;
        self.failed = score.is_failed;
        self.stages.push(StageResult::of(score));
    }

    /// The course stopped early by the player: it counts as failed.
    pub fn abandon(&mut self) {
        self.failed = true;
    }

    /// The songs played so far, added together. `gauge` is the last song's.
    pub fn total(&self) -> StageResult {
        let mut total = StageResult {
            gauge: self.gauge,
            ..StageResult::default()
        };
        for s in &self.stages {
            total.total_notes += s.total_notes;
            total.ex_score += s.ex_score;
            total.pgreat += s.pgreat;
            total.great += s.great;
            total.good += s.good;
            total.bad += s.bad;
            total.poor += s.poor;
            total.miss += s.miss;
            total.max_combo = total.max_combo.max(s.max_combo);
        }
        total
    }

    /// The best trophy earned, judged over the whole course (only a passed
    /// course earns one). Trophies are tried from the last one listed, which
    /// tables list from easiest to hardest.
    pub fn trophy<'a>(&self, trophies: &'a [Trophy]) -> Option<&'a Trophy> {
        if !self.is_passed() {
            return None;
        }
        let total = self.total();
        let notes = total.total_notes.max(1) as f32;
        let miss_rate = total.misses() as f32 * 100.0 / notes;
        let score_rate = total.ex_score as f32 * 100.0 / (notes * 2.0);
        trophies
            .iter()
            .rev()
            .find(|t| miss_rate <= t.max_miss_rate && score_rate >= t.min_score_rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dan(songs: usize, constraints: Vec<Constraint>) -> CourseDef {
        CourseDef {
            name: "dan".into(),
            charts: (0..songs)
                .map(|i| CourseChart {
                    sha256: Some(ChartId::synthetic(i as u64)),
                    ..CourseChart::default()
                })
                .collect(),
            constraints,
            trophies: Vec::new(),
        }
    }

    fn play(run: &mut CourseRun, notes: u32, grades: &[JudgeGrade]) {
        let mut score = run.tracker_for_next(notes, 300.0);
        for &g in grades {
            score.record_hit(run.judge(g));
        }
        run.finish_stage(&score);
    }

    #[test]
    fn the_gauge_carries_over_and_all_songs_pass() {
        let mut run = CourseRun::new(&dan(2, vec![Constraint::GradeMirror]), GaugeType::Groove);
        assert_eq!(run.gauge_type(), GaugeType::Class);
        assert_eq!(run.gauge(), 100.0);

        // Ten POORs (passed notes, -3 each above 25%) take 30.
        play(&mut run, 10, &[JudgeGrade::Miss; 10]);
        assert!((run.gauge() - 70.0).abs() < 1e-4, "{}", run.gauge());
        assert!(!run.is_finished());

        // The next song starts where the last one ended.
        let next = run.tracker_for_next(5, 300.0);
        assert!((next.gauge - 70.0).abs() < 1e-4);
        play(&mut run, 5, &[JudgeGrade::PerfectGreat; 5]);
        assert!(run.is_passed());
        assert_eq!(run.stages().len(), 2);
        let total = run.total();
        assert_eq!(
            (total.total_notes, total.ex_score, total.miss),
            (15, 10, 10)
        );
    }

    #[test]
    fn an_empty_gauge_ends_the_course_at_once() {
        let mut run = CourseRun::new(&dan(4, Vec::new()), GaugeType::Hazard);
        assert_eq!(run.gauge_type(), GaugeType::ExHardClass);
        // EXHARDCLASS: -10 per passed note, no relief.
        play(&mut run, 20, &[JudgeGrade::Miss; 20]);
        assert!(run.is_finished());
        assert!(!run.is_passed());
        assert_eq!(run.gauge(), 0.0);
        play(&mut run, 1, &[JudgeGrade::PerfectGreat]);
        assert_eq!(run.stages().len(), 1, "nothing is played after failing");
    }

    #[test]
    fn low_gauge_damage_is_relieved_on_class_and_lr2_numbers_apply() {
        let mut run = CourseRun::new(&dan(1, Vec::new()), GaugeType::Easy);
        let mut score = run.tracker_for_next(100, 300.0);
        score.gauge = 4.0;
        score.record_hit(JudgeGrade::Miss);
        // Under 5%: -3 x 0.4.
        assert!((score.gauge - 2.8).abs() < 1e-4, "{}", score.gauge);
        run.abandon();
        assert!(run.is_finished() && !run.is_passed());

        let lr2 = CourseRun::new(
            &dan(1, vec![Constraint::Gauge(GaugeRules::Lr2)]),
            GaugeType::Hard,
        );
        let mut score = lr2.tracker_for_next(100, 300.0);
        score.record_hit(JudgeGrade::Miss);
        assert!(
            (score.gauge - 90.0).abs() < 1e-4,
            "EXCLASS_LR2 passed note is -10"
        );
        score.record_hit(JudgeGrade::PerfectGreat);
        assert!((score.gauge - 90.1).abs() < 1e-4);
    }

    #[test]
    fn the_best_trophy_met_is_given_only_for_a_pass() {
        let trophies = [
            Trophy {
                name: "bronzemedal".into(),
                max_miss_rate: 50.0,
                min_score_rate: 0.0,
            },
            Trophy {
                name: "goldmedal".into(),
                max_miss_rate: 0.0,
                min_score_rate: 100.0,
            },
        ];
        let mut run = CourseRun::new(&dan(1, Vec::new()), GaugeType::Groove);
        assert!(run.trophy(&trophies).is_none(), "not played yet");
        play(
            &mut run,
            4,
            &[
                JudgeGrade::PerfectGreat,
                JudgeGrade::Great,
                JudgeGrade::Bad,
                JudgeGrade::PerfectGreat,
            ],
        );
        assert_eq!(
            run.trophy(&trophies).map(|t| t.name.as_str()),
            Some("bronzemedal")
        );

        let mut run = CourseRun::new(&dan(1, Vec::new()), GaugeType::Groove);
        play(&mut run, 2, &[JudgeGrade::PerfectGreat; 2]);
        assert_eq!(
            run.trophy(&trophies).map(|t| t.name.as_str()),
            Some("goldmedal")
        );
    }

    #[test]
    fn constraint_names_round_trip_and_unknown_ones_are_kept() {
        for (name, constraint) in NAMES.iter() {
            assert_eq!(&Constraint::from_name(name), constraint);
            assert_eq!(constraint.name(), *name);
        }
        assert_eq!(
            Constraint::from_name(" GRADE_Mirror "),
            Constraint::GradeMirror
        );
        let unknown = Constraint::from_name("future_rule");
        assert_eq!(unknown, Constraint::Unknown("future_rule".into()));
        assert_eq!(unknown.name(), "future_rule");
        assert!(!unknown.is_supported());
        assert!(!Constraint::NoGood.is_supported());
        assert!(Constraint::NoSpeed.is_supported());
    }

    #[test]
    fn a_course_is_grade_when_any_rule_says_so() {
        let mut course = CourseDef {
            constraints: vec![Constraint::Gauge(GaugeRules::Lr2)],
            ..CourseDef::default()
        };
        assert!(!course.is_grade());
        course.constraints.push(Constraint::GradeMirror);
        assert!(course.is_grade());
        assert_eq!(course.unsupported().count(), 0);
    }

    #[test]
    fn course_id_depends_on_charts_and_their_order() {
        let (a, b) = (ChartId::synthetic(1), ChartId::synthetic(2));
        assert_eq!(CourseId::of_charts(&[a, b]), CourseId::of_charts(&[a, b]));
        assert_ne!(CourseId::of_charts(&[a, b]), CourseId::of_charts(&[b, a]));
        assert_ne!(CourseId::of_charts(&[a]), CourseId::of_charts(&[a, b]));
    }
}

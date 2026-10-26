//! Courses: charts played back to back as one run, and dan certification
//! (段位認定) courses that add rules such as a carried-over gauge and locked
//! options.
//!
//! The shape follows the `course` field of beatoraja difficulty-table
//! headers, which is where most courses come from. A course looks the same
//! whatever it came from (`CourseSource`), so disc and local courses can be
//! added without touching the rest.

use crate::identity::ChartId;
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

#[cfg(test)]
mod tests {
    use super::*;

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

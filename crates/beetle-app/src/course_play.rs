//! Playing a course: its songs one after another with the dan gauge carried
//! over (`beetle_core::CourseRun`), no result between songs, and one course
//! result at the end (drawn on the result screen while a course is active).
//!
//! The songs play through the normal load and play path; this module only
//! decides what plays next and what the play starts from.

use beetle_core::{
    Constraint, CourseDef, CourseId, CourseRecords, CourseRun, LaneModifier, LongNoteRule, Ruleset,
    ScoreTracker, SongMetadata, TableIndex,
};
use beetle_render::{strings, CourseView, ToastKind};
use std::collections::HashMap;

use crate::folders::ListEntry;
use crate::gameplay::queue_start_gameplay;
use crate::options_table::OptionId;
use crate::state::{AppScreen, AppState};

/// The course being played.
pub struct ActiveCourse {
    pub def: CourseDef,
    /// Library index of each song, in course order.
    pub songs: Vec<usize>,
    pub run: CourseRun,
    /// The next song, loading while the current one plays: its stage and
    /// the load. Dropped (the load finishes into nothing) if the course ends.
    preload: Option<(usize, crate::loader::SongLoadReceiver)>,
}

impl ActiveCourse {
    /// The song to load next, while the course goes on.
    pub fn next_song<'a>(&self, library: &'a [SongMetadata]) -> Option<&'a SongMetadata> {
        if self.run.is_finished() {
            return None;
        }
        library.get(*self.songs.get(self.run.stage())?)
    }
}

/// The library index of each chart of the course, in order; `None` for a
/// chart the library does not have. A chart is found by its SHA-256 when it
/// has one, otherwise by its MD5 (as tables are matched).
pub fn resolve(def: &CourseDef, songs: &[SongMetadata]) -> Vec<Option<usize>> {
    SongLookup::new(songs).resolve(def)
}

/// The library songs by SHA-256 and by MD5, built once for many courses.
pub struct SongLookup {
    by_id: HashMap<beetle_core::ChartId, usize>,
    by_md5: HashMap<[u8; 16], usize>,
}

impl SongLookup {
    pub fn new(songs: &[SongMetadata]) -> Self {
        let mut by_id = HashMap::new();
        let mut by_md5 = HashMap::new();
        for (i, s) in songs.iter().enumerate() {
            by_id.entry(s.id).or_insert(i);
            by_md5.entry(s.md5).or_insert(i);
        }
        Self { by_id, by_md5 }
    }

    pub fn resolve(&self, def: &CourseDef) -> Vec<Option<usize>> {
        def.charts
            .iter()
            .map(|c| match (c.sha256, c.md5) {
                (Some(id), _) => self.by_id.get(&id).copied(),
                (None, Some(md5)) => self.by_md5.get(&md5).copied(),
                (None, None) => None,
            })
            .collect()
    }
}

/// The lane option a course song plays with: a dan course allows only what
/// its rule allows (none, mirror, or any), and falls back to none.
pub fn lane_modifier_for(constraints: &[Constraint], chosen: LaneModifier) -> LaneModifier {
    let allowed = if constraints.contains(&Constraint::GradeRandom) {
        true
    } else if constraints.contains(&Constraint::GradeMirror) {
        matches!(chosen, LaneModifier::Regular | LaneModifier::Mirror)
    } else if constraints.contains(&Constraint::Grade) {
        chosen == LaneModifier::Regular
    } else {
        true
    };
    if allowed {
        chosen
    } else {
        LaneModifier::Regular
    }
}

/// The long note rule a course plays every song with, if it sets one.
/// HCN is played as CN (and said so), as charts asking for it are.
pub fn ruleset_for(constraints: &[Constraint]) -> Option<Ruleset> {
    constraints.iter().find_map(|c| match c {
        Constraint::LongNote(LongNoteRule::Ln) => Some(Ruleset::LN),
        Constraint::LongNote(LongNoteRule::Cn) => Some(Ruleset::CN),
        Constraint::LongNote(LongNoteRule::Hcn) => Some(Ruleset {
            hcn_requested: true,
            ..Ruleset::CN
        }),
        _ => None,
    })
}

/// The rules of the course being played, or else of the highlighted course row.
fn rules_in_view(state: &AppState) -> Option<&[Constraint]> {
    if let Some(course) = &state.course {
        return Some(course.run.constraints());
    }
    match *state.entries.get(state.selected_entry)? {
        ListEntry::Course { table, course } => {
            Some(&course_of(&state.tables, table, course)?.constraints)
        }
        _ => None,
    }
}

/// The long note rule of the course being played.
pub fn ruleset(state: &AppState) -> Option<Ruleset> {
    ruleset_for(state.course.as_ref()?.run.constraints())
}

/// The course being played does not let the speed change.
pub fn speed_locked(state: &AppState) -> bool {
    state
        .course
        .as_ref()
        .is_some_and(|c| c.run.constraints().contains(&Constraint::NoSpeed))
}

/// What a course plays a play option row with, when a course decides it
/// (a course is played or highlighted): its dan gauge, its allowed lane
/// option, its long note rule.
pub fn locked_value(state: &AppState, id: OptionId) -> Option<String> {
    let rules = rules_in_view(state)?;
    let o = &state.play_options;
    match id {
        OptionId::Gauge => Some(o.gauge_type.for_course().as_str().to_string()),
        OptionId::Modifier => {
            let played = lane_modifier_for(rules, o.lane_modifier);
            (played != o.lane_modifier || rules.iter().any(Constraint::is_grade))
                .then(|| played.as_str().to_string())
        }
        OptionId::LnMode => ruleset_for(rules).map(|r| r.label().to_string()),
        _ => None,
    }
}

const RECORDS_FILE: &str = "courses.dat";

pub fn load_records() -> CourseRecords {
    std::fs::read_to_string(RECORDS_FILE)
        .map(|text| CourseRecords::parse(&text))
        .unwrap_or_default()
}

/// Through a temporary file, as `scores.dat` is saved.
fn save_records(records: &CourseRecords) {
    let temp = format!("{RECORDS_FILE}.tmp");
    if std::fs::write(&temp, records.serialize()).is_ok() {
        let _ = std::fs::rename(&temp, RECORDS_FILE);
    }
}

/// The id a course's records are kept under.
fn course_id(course: &ActiveCourse, songs: &[SongMetadata]) -> CourseId {
    let ids: Vec<_> = course.songs.iter().map(|&i| songs[i].id).collect();
    CourseId::of_charts(&ids)
}

/// Why the course just played is not saved, if it is not.
pub fn unsaved_reason(state: &AppState) -> Option<&'static str> {
    let course = state.course.as_ref()?;
    if state.is_auto_play {
        Some(strings::AUTO_PLAY())
    } else if course.def.unsupported().next().is_some() {
        Some(strings::COURSE_NOT_SAVED_RULES())
    } else {
        None
    }
}

/// The course is over: saves the run (unless it is not to be saved).
fn conclude(state: &mut AppState) {
    if unsaved_reason(state).is_some() {
        return;
    }
    let Some(course) = &state.course else {
        return;
    };
    let id = course_id(course, &state.songs);
    state
        .course_records
        .record(id, &course.run, &course.def.trophies);
    save_records(&state.course_records);
    state.refresh_course_views();
}

/// What the song list shows of a course.
/// The row of each course among `entries` (`None` for other rows). Built when
/// the list or the records change, not every frame: a disc's course table
/// lists hundreds of courses.
pub fn views(
    entries: &[ListEntry],
    tables: &TableIndex,
    songs: &[SongMetadata],
    records: &CourseRecords,
) -> Vec<Option<CourseView>> {
    let lookup = SongLookup::new(songs);
    entries
        .iter()
        .map(|e| match *e {
            ListEntry::Course { table, course } => {
                course_of(tables, table, course).map(|def| view(def, &lookup, songs, records))
            }
            _ => None,
        })
        .collect()
}

fn view(
    def: &CourseDef,
    lookup: &SongLookup,
    songs: &[SongMetadata],
    records: &CourseRecords,
) -> CourseView {
    let found = lookup.resolve(def);
    let ids: Option<Vec<_>> = found.iter().map(|f| f.map(|i| songs[i].id)).collect();
    let record = ids.and_then(|ids| records.get(&CourseId::of_charts(&ids)));
    CourseView {
        passed: record.is_some_and(|r| r.passed),
        best: record.map(|r| {
            let verdict = match (&r.trophy, r.passed) {
                (Some(trophy), _) => trophy.clone(),
                (None, true) => strings::COURSE_PASSED().to_string(),
                (None, false) => strings::COURSE_FAILED().to_string(),
            };
            strings::fill(
                strings::COURSE_BEST(),
                &[&verdict, &format!("{} / {}", r.ex_score, r.max_ex_score)],
            )
        }),
        name: def.name.clone(),
        grade: def.is_grade(),
        charts: def
            .charts
            .iter()
            .zip(&found)
            .map(|(chart, found)| match found {
                Some(i) => (songs[*i].title.clone(), true),
                None => (chart.title.clone(), false),
            })
            .collect(),
        unsupported: def
            .unsupported()
            .map(Constraint::name)
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// The course of a list row.
pub fn course_of(tables: &TableIndex, table: usize, course: usize) -> Option<&CourseDef> {
    tables.tables().get(table)?.courses.get(course)
}

/// Starts a course from its first song. A course with a song the library
/// does not have does not start (a toast says why).
pub fn start(state: &mut AppState, def: CourseDef) {
    let Some(songs) = resolve(&def, &state.songs)
        .into_iter()
        .collect::<Option<Vec<usize>>>()
    else {
        crate::transition::show_toast(state, ToastKind::Error, strings::COURSE_CANNOT_START());
        return;
    };
    if songs.is_empty() {
        return;
    }
    let run = CourseRun::new(&def, state.play_options.gauge_type);
    state.course = Some(ActiveCourse {
        def,
        songs,
        run,
        preload: None,
    });
    state.is_replay_playback = false;
    play_next(state);
}

/// From the first song again (restart from the pause menu).
pub fn restart(state: &mut AppState) {
    if let Some(course) = state.course.take() {
        start(state, course.def);
    }
}

/// Loads the course's next song. `false` when the course has none left.
pub fn play_next(state: &mut AppState) -> bool {
    let Some(song) = state
        .course
        .as_ref()
        .and_then(|c| c.next_song(&state.songs))
        .cloned()
    else {
        return false;
    };
    if state.capture.is_some() {
        if let Some(c) = &state.course {
            crate::devtools::log(&format!(
                "course stage {}/{}: {}",
                c.run.stage() + 1,
                c.run.stage_count(),
                song.title
            ));
        }
    }
    queue_start_gameplay(state, &song);
    true
}

/// The tracker a course song starts from (with the carried gauge), when a
/// course is being played.
pub fn starting_score(state: &AppState, total_notes: u32, total: f64) -> Option<ScoreTracker> {
    let course = state.course.as_ref()?;
    Some(course.run.tracker_for_next(total_notes, total))
}

/// A course song has started playing: the song after it starts loading
/// now, on few threads, so the next stage needs little or no loading.
pub fn preload_next(state: &mut AppState) {
    let load_bga = state.bga_visible();
    let Some(course) = &mut state.course else {
        return;
    };
    let next = course.run.stage() + 1;
    let Some(song) = course.songs.get(next).and_then(|&i| state.songs.get(i)) else {
        return;
    };
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(42, |d| d.as_nanos() as u64);
    course.preload = Some((
        next,
        crate::loader::spawn_quiet_song_loader(song, seed, load_bga),
    ));
}

/// The load already started for the song about to load, if it is the
/// course's next one.
pub fn take_preload(state: &mut AppState) -> Option<crate::loader::SongLoadReceiver> {
    let course = state.course.as_mut()?;
    match course.preload.take() {
        Some((stage, rx)) if stage == course.run.stage() => Some(rx),
        _ => None,
    }
}

/// A course song has ended (played out, or the gauge emptied).
pub fn finish_stage(state: &mut AppState) {
    let (Some(course), Some(judge)) = (&mut state.course, &state.active_judge) else {
        return;
    };
    course.run.finish_stage(judge.score());
}

/// After a song's end banner: the next song, or the course result once
/// the course is over. `false` when no course is being played.
pub fn after_song(state: &mut AppState) -> bool {
    if state.course.is_none() {
        return false;
    }
    if !play_next(state) {
        conclude(state);
        state.screen = AppScreen::Result;
    }
    true
}

/// The player quit the course mid-song: it counts as failed, and the
/// course result shows.
pub fn quit(state: &mut AppState) {
    if let Some(course) = &mut state.course {
        course.run.abandon();
        conclude(state);
        state.screen = AppScreen::Result;
    }
}

/// Back to the song list: the course is over.
pub fn leave(state: &mut AppState) {
    state.course = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use beetle_core::{ChartId, CourseChart, GaugeType};

    fn song(n: u64) -> SongMetadata {
        SongMetadata {
            id: ChartId::synthetic(n),
            md5: [n as u8; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: n,
            file_path: format!("{n}.bms"),
            title: format!("Song {n}"),
            subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            bpm: 120.0,
            bpm_min: 120.0,
            bpm_max: 120.0,
            play_level: 1,
            notes_count: 100,
            play_mode: beetle_core::PlayMode::Keys7,
            difficulty: None,
        }
    }

    #[test]
    fn charts_are_found_by_sha256_then_md5() {
        let songs = vec![song(1), song(2)];
        let def = CourseDef {
            name: "c".into(),
            charts: vec![
                CourseChart {
                    sha256: Some(ChartId::synthetic(2)),
                    md5: Some([1; 16]),
                    ..CourseChart::default()
                },
                CourseChart {
                    md5: Some([1; 16]),
                    ..CourseChart::default()
                },
                CourseChart {
                    sha256: Some(ChartId::synthetic(9)),
                    title: "Gone".into(),
                    ..CourseChart::default()
                },
            ],
            ..CourseDef::default()
        };
        assert_eq!(resolve(&def, &songs), [Some(1), Some(0), None]);
        let v = view(
            &def,
            &SongLookup::new(&songs),
            &songs,
            &CourseRecords::default(),
        );
        assert!(!v.playable());
        assert_eq!(v.charts[0], ("Song 2".to_string(), true));
        assert_eq!(v.charts[2], ("Gone".to_string(), false));
    }

    #[test]
    fn a_course_long_note_rule_wins() {
        assert_eq!(ruleset_for(&[Constraint::GradeMirror]), None);
        let ln = [Constraint::LongNote(LongNoteRule::Ln)];
        assert_eq!(ruleset_for(&ln), Some(Ruleset::LN));
        let hcn = ruleset_for(&[Constraint::LongNote(LongNoteRule::Hcn)]).unwrap();
        assert!(hcn.hcn_requested);
        assert_eq!(GaugeType::Hard.for_course(), GaugeType::ExClass);
    }

    #[test]
    fn dan_rules_limit_the_lane_option() {
        use LaneModifier::*;
        let grade = [Constraint::Grade];
        let mirror = [Constraint::GradeMirror];
        let random = [Constraint::GradeRandom];
        assert_eq!(lane_modifier_for(&grade, Mirror), Regular);
        assert_eq!(lane_modifier_for(&mirror, Mirror), Mirror);
        assert_eq!(lane_modifier_for(&mirror, Random), Regular);
        assert_eq!(lane_modifier_for(&random, SRandom), SRandom);
        assert_eq!(lane_modifier_for(&[], Random), Random);
    }
}

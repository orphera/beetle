//! Difficulty tables for the song list: reading `tables/` (put there by
//! `bpm table`) and matching them to the songs.

use std::fs;
use std::path::PathBuf;

use beetle_core::{
    disc_address, Constraint, CourseChart, CourseDef, DifficultyTable, SongAddress, SongMetadata,
    TableIndex,
};
use beetle_render::strings;
use cs_disc::iidx::{Difficulty, Style};

/// Where `bpm table` keeps the tables; the same folder it writes to.
fn tables_dir() -> PathBuf {
    std::env::var("BEETLE_TABLES_DIR").map_or_else(|_| PathBuf::from("tables"), PathBuf::from)
}

/// Every table in `tables/`, ordered by file name (the order they are
/// prioritized in). Files that are not tables are skipped.
pub fn load_tables() -> Vec<DifficultyTable> {
    read_tables(&tables_dir())
}

fn read_tables(dir: &std::path::Path) -> Vec<DifficultyTable> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "tbl"))
        .collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|path| DifficultyTable::parse(&fs::read_to_string(path).ok()?))
        .collect()
}

/// The installed tables, matched to `songs`.
pub fn build_index(songs: &[SongMetadata]) -> TableIndex {
    let mut tables = load_tables();
    tables.extend(disc_dan_tables(songs));
    let mut index = TableIndex::new(tables);
    index.match_songs(songs.iter().map(|s| (s.id, s.md5)));
    index
}

/// The courses of the game discs in the library, one course-only table per
/// disc (after the installed tables): the dan courses, then each EXPERT
/// course at every style and difficulty its songs all have (the game lets
/// the player pick; here each pick is its own course). A course names its
/// charts by the ids of the library songs read from that disc; a course with
/// a chart the library does not list from that disc is left out.
fn disc_dan_tables(songs: &[SongMetadata]) -> Vec<DifficultyTable> {
    let mut images: Vec<&str> = Vec::new();
    for song in songs {
        for path in std::iter::once(&song.file_path).chain(&song.also_on) {
            if let SongAddress::Disc { image, .. } = SongAddress::parse(path) {
                if !images.contains(&image) {
                    images.push(image);
                }
            }
        }
    }
    images
        .into_iter()
        .filter_map(|image| disc_dan_table(image, songs))
        .collect()
}

fn disc_dan_table(image: &str, songs: &[SongMetadata]) -> Option<DifficultyTable> {
    let mut disc = cs_disc::open(image).ok()?;
    let edition = cs_disc::identify(&mut disc).ok()??;
    let volume = cs_disc::Volume::open(&mut disc).ok()?;
    let read = cs_disc::iidx::Disc::open(&mut disc, &volume, edition).ok()?;
    let by_address = |address: &str| {
        songs
            .iter()
            .find(|s| s.file_path == address || s.also_on.iter().any(|p| p == address))
    };
    let chart_of = |id: u32, style: Style, difficulty: Difficulty| {
        let song = read.songs.iter().find(|s| s.id == id)?;
        let chart = song
            .charts
            .iter()
            .find(|c| c.style == style && c.difficulty == difficulty)?;
        let found = by_address(&disc_address(image, &chart.key(song)))?;
        Some(CourseChart {
            md5: Some(found.md5),
            sha256: Some(found.id),
            title: found.title.clone(),
        })
    };
    let mut courses: Vec<CourseDef> = read
        .dan_courses
        .iter()
        .filter_map(|dan| {
            let charts = dan
                .charts
                .iter()
                .map(|&(id, difficulty)| chart_of(id, dan.style, difficulty))
                .collect::<Option<Vec<_>>>()?;
            Some(CourseDef {
                name: format!("{} {}", style_name(dan.style), dan.rank),
                charts,
                // The disc does not say which lane options its dan mode
                // allows; mirror is assumed, as in most dan tables.
                constraints: vec![Constraint::GradeMirror],
                trophies: Vec::new(),
            })
        })
        .collect();
    for expert in &read.expert_courses {
        for style in [Style::Single, Style::Double] {
            for difficulty in [Difficulty::Normal, Difficulty::Hyper, Difficulty::Another] {
                let Some(charts) = expert
                    .songs
                    .iter()
                    .map(|&id| chart_of(id, style, difficulty))
                    .collect::<Option<Vec<_>>>()
                else {
                    continue;
                };
                courses.push(CourseDef {
                    name: format!(
                        "{} ({} {})",
                        expert.name,
                        style_name(style),
                        difficulty_name(difficulty)
                    ),
                    charts,
                    constraints: Vec::new(),
                    trophies: Vec::new(),
                });
            }
        }
    }
    if courses.is_empty() {
        return None;
    }
    Some(DifficultyTable {
        name: strings::fill(strings::DISC_COURSE_TABLE(), &[edition.name()]),
        source: disc_address(image, ""),
        courses,
        ..DifficultyTable::default()
    })
}

fn style_name(style: Style) -> &'static str {
    match style {
        Style::Single => "SP",
        Style::Double => "DP",
    }
}

fn difficulty_name(difficulty: Difficulty) -> &'static str {
    match difficulty {
        Difficulty::Beginner => "BEGINNER",
        Difficulty::Normal => "NORMAL",
        Difficulty::Hyper => "HYPER",
        Difficulty::Another => "ANOTHER",
        Difficulty::Black => "BLACK",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beetle_core::{ChartId, TableEntry};

    fn table(name: &str) -> DifficultyTable {
        DifficultyTable {
            name: name.into(),
            symbol: "t".into(),
            entries: vec![TableEntry {
                level: "1".into(),
                sha256: Some(ChartId::synthetic(1)),
                ..TableEntry::default()
            }],
            ..DifficultyTable::default()
        }
    }

    #[test]
    fn tables_are_read_in_file_name_order_and_junk_is_skipped() {
        let dir = std::env::temp_dir().join(format!("beetle_tables_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("b.tbl"), table("Second").serialize()).unwrap();
        fs::write(dir.join("a.tbl"), table("First").serialize()).unwrap();
        fs::write(dir.join("c.tbl"), "not a table").unwrap();
        fs::write(dir.join("readme.txt"), "ignored").unwrap();

        let names: Vec<String> = read_tables(&dir).into_iter().map(|t| t.name).collect();
        assert_eq!(names, ["First", "Second"]);
        assert!(read_tables(&dir.join("missing")).is_empty());
        let _ = fs::remove_dir_all(dir);
    }
}

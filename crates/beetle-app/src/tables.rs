//! Difficulty tables for the song list: reading `tables/` (put there by
//! `bpm table`) and matching them to the songs.

use std::fs;
use std::path::PathBuf;

use beetle_core::{DifficultyTable, SongMetadata, TableIndex};

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
    let mut index = TableIndex::new(load_tables());
    index.match_songs(songs.iter().map(|s| (s.id, s.md5)));
    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use beetle_core::{ChartId, TableEntry};

    fn table(name: &str) -> DifficultyTable {
        DifficultyTable {
            name: name.into(),
            symbol: "t".into(),
            entries: vec![TableEntry { level: "1".into(), sha256: Some(ChartId::synthetic(1)), ..TableEntry::default() }],
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

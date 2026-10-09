//! BMS difficulty tables: the local cache format, level ordering, and
//! matching a table's charts to the charts in the song list.
//!
//! This is the pure part. Fetching a table and turning its JSON into this
//! format is `bpm table`'s job; the player only reads the cache.
//!
//! A table names its charts by the SHA-256 and/or MD5 of the chart file's
//! bytes, which is exactly what `ChartId` and `SongMetadata::md5` are.

use crate::escape::{escape_field, unescape_field};
use crate::identity::{md5_from_hex, md5_to_hex, ChartId};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

/// First line of a table cache file.
const FILE_HEADER: &str = "#BEETLE_TABLE_V1";
/// Line that ends the header fields and starts the entries.
const ENTRIES_MARK: &str = "#ENTRIES";

/// One chart of a table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableEntry {
    /// The table's name for the level (`3`, `★12`, `?`).
    pub level: String,
    /// MD5 of the chart file, as many tables give it.
    pub md5: Option<[u8; 16]>,
    /// SHA-256 of the chart file (`ChartId`), when the table gives it.
    pub sha256: Option<ChartId>,
    pub title: String,
    pub artist: String,
    /// Where the chart can be downloaded.
    pub url: String,
    /// Where the differential (差分) of the chart can be downloaded.
    pub url_diff: String,
}

/// A difficulty table as stored in the local cache.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DifficultyTable {
    pub name: String,
    /// Prefix for the level chip (`sl` in `sl3`).
    pub symbol: String,
    /// Address `bpm table update` fetches it from again.
    pub source: String,
    /// When it was fetched, in seconds since the Unix epoch.
    pub fetched: u64,
    /// The order the table gives its levels in, if it gives one.
    pub level_order: Vec<String>,
    pub entries: Vec<TableEntry>,
}

impl DifficultyTable {
    /// Serializes to the cache format. Entries keep the order they came in.
    pub fn serialize(&self) -> String {
        let mut out = String::with_capacity(256 + self.entries.len() * 200);
        out.push_str(FILE_HEADER);
        out.push('\n');
        let _ = writeln!(out, "name={}", escape_field(&self.name));
        let _ = writeln!(out, "symbol={}", escape_field(&self.symbol));
        let _ = writeln!(out, "source={}", escape_field(&self.source));
        let _ = writeln!(out, "fetched={}", self.fetched);
        for level in &self.level_order {
            let _ = writeln!(out, "level={}", escape_field(level));
        }
        out.push_str(ENTRIES_MARK);
        out.push('\n');
        for e in &self.entries {
            let _ = writeln!(
                out,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                escape_field(&e.level),
                e.md5.as_ref().map(md5_to_hex).unwrap_or_default(),
                e.sha256.map(|id| id.to_hex()).unwrap_or_default(),
                escape_field(&e.title),
                escape_field(&e.artist),
                escape_field(&e.url),
                escape_field(&e.url_diff),
            );
        }
        out
    }

    /// Reads a cache file. `None` when it is not one, or has no name. Header
    /// fields it does not know are ignored; an entry with neither hash is
    /// skipped (nothing could match it).
    pub fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        if lines.next()?.trim() != FILE_HEADER {
            return None;
        }

        let mut table = Self::default();
        let mut in_entries = false;
        for line in lines {
            if !in_entries {
                if line.trim() == ENTRIES_MARK {
                    in_entries = true;
                } else if let Some((key, value)) = line.split_once('=') {
                    match key {
                        "name" => table.name = unescape_field(value),
                        "symbol" => table.symbol = unescape_field(value),
                        "source" => table.source = unescape_field(value),
                        "fetched" => table.fetched = value.parse().unwrap_or(0),
                        "level" => table.level_order.push(unescape_field(value)),
                        _ => {}
                    }
                }
                continue;
            }
            if let Some(entry) = parse_entry(line) {
                table.entries.push(entry);
            }
        }

        (!table.name.is_empty()).then_some(table)
    }

    /// Orders two level names the way this table orders them: its own
    /// `level_order` first, then (for levels it does not list, or when it has
    /// no order) natural order.
    pub fn compare_levels(&self, a: &str, b: &str) -> Ordering {
        let rank = |level: &str| self.level_order.iter().position(|l| l == level);
        match (rank(a), rank(b)) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => natural_cmp(a, b),
        }
    }

    /// The distinct levels of the table, in table order.
    pub fn levels(&self) -> Vec<&str> {
        let mut levels: Vec<&str> = Vec::new();
        for entry in &self.entries {
            if !levels.contains(&entry.level.as_str()) {
                levels.push(&entry.level);
            }
        }
        levels.sort_by(|a, b| self.compare_levels(a, b));
        levels
    }
}

fn parse_entry(line: &str) -> Option<TableEntry> {
    let mut fields = line.split('\t');
    let level = unescape_field(fields.next()?);
    let md5 = fields.next().and_then(md5_from_hex);
    let sha256 = fields.next().and_then(ChartId::from_hex);
    if md5.is_none() && sha256.is_none() {
        return None;
    }
    let mut text = || fields.next().map(unescape_field).unwrap_or_default();
    Some(TableEntry {
        level,
        md5,
        sha256,
        title: text(),
        artist: text(),
        url: text(),
        url_diff: text(),
    })
}

/// Natural order: runs of digits compare as numbers, anything else as text,
/// and a number sorts before text (`2` < `10` < `10a` < `?`, `★3` < `★12`).
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a, b);
    loop {
        match (a.is_empty(), b.is_empty()) {
            (true, true) => return Ordering::Equal,
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }
        let (ta, ra) = next_token(a);
        let (tb, rb) = next_token(b);
        let ordering = match (ta.starts_with(is_digit), tb.starts_with(is_digit)) {
            (true, true) => compare_numbers(ta, tb),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => ta.cmp(tb),
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
        (a, b) = (ra, rb);
    }
}

fn is_digit(c: char) -> bool {
    c.is_ascii_digit()
}

/// A leading run of digits, or of non-digits, and the rest.
fn next_token(s: &str) -> (&str, &str) {
    let digits = s.starts_with(is_digit);
    let end = s.find(|c: char| is_digit(c) != digits).unwrap_or(s.len());
    s.split_at(end)
}

/// Numbers of any length, without overflowing: fewer digits (after leading
/// zeros) is smaller, then compare digit by digit.
fn compare_numbers(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.trim_start_matches('0'), b.trim_start_matches('0'));
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

/// Where a chart sits in a table: indices into `TableIndex::tables()` and
/// that table's `entries`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableMatch {
    pub table: usize,
    pub entry: usize,
}

/// The installed tables, matched against the charts in the song list.
#[derive(Debug, Default)]
pub struct TableIndex {
    tables: Vec<DifficultyTable>,
    /// Per chart, its place in each table that has it, in table order.
    matches: HashMap<ChartId, Vec<TableMatch>>,
    /// Per table, how many of its entries are in the song list.
    owned: Vec<usize>,
}

impl TableIndex {
    /// Tables in the order they should be shown and prioritized.
    pub fn new(tables: Vec<DifficultyTable>) -> Self {
        let owned = vec![0; tables.len()];
        Self {
            tables,
            matches: HashMap::new(),
            owned,
        }
    }

    pub fn tables(&self) -> &[DifficultyTable] {
        &self.tables
    }

    /// Matches every table against the charts of the song list, given as
    /// `(id, md5)` pairs (the same chart listed twice, say loose and in a
    /// package, is fine). Replaces any earlier matching.
    ///
    /// An entry matches by its SHA-256 when it has one, and only then; the
    /// MD5 is the fallback for entries without a SHA-256, since MD5 does not
    /// resist collisions. When the same chart is in a table twice, the first
    /// entry is the one the chart is labeled with; both count as owned.
    pub fn match_songs(&mut self, songs: impl IntoIterator<Item = (ChartId, [u8; 16])>) {
        let mut ids: HashSet<ChartId> = HashSet::new();
        let mut by_md5: HashMap<[u8; 16], Vec<ChartId>> = HashMap::new();
        for (id, md5) in songs {
            if ids.insert(id) {
                by_md5.entry(md5).or_default().push(id);
            }
        }

        self.matches.clear();
        self.owned = vec![0; self.tables.len()];
        for (table_index, table) in self.tables.iter().enumerate() {
            let mut labeled: HashSet<ChartId> = HashSet::new();
            for (entry_index, entry) in table.entries.iter().enumerate() {
                let found: Vec<ChartId> = match (entry.sha256, entry.md5) {
                    (Some(sha), _) => ids.get(&sha).copied().into_iter().collect(),
                    (None, Some(md5)) => by_md5.get(&md5).cloned().unwrap_or_default(),
                    (None, None) => Vec::new(),
                };
                if found.is_empty() {
                    continue;
                }
                self.owned[table_index] += 1;
                for id in found {
                    if labeled.insert(id) {
                        self.matches.entry(id).or_default().push(TableMatch {
                            table: table_index,
                            entry: entry_index,
                        });
                    }
                }
            }
        }
    }

    /// Where a chart is in the tables, in table order; empty when it is in none.
    pub fn matches_for(&self, id: ChartId) -> &[TableMatch] {
        self.matches.get(&id).map_or(&[], Vec::as_slice)
    }

    /// The chart's entry in one table.
    pub fn entry_for(&self, table: usize, id: ChartId) -> Option<&TableEntry> {
        self.matches_for(id)
            .iter()
            .find(|m| m.table == table)
            .map(|m| &self.tables[m.table].entries[m.entry])
    }

    /// The level chip of a chart: its level in the first table that has it,
    /// behind the table's symbol (`sl3`).
    pub fn chip(&self, id: ChartId) -> Option<String> {
        let m = self.matches_for(id).first()?;
        let table = &self.tables[m.table];
        Some(format!("{}{}", table.symbol, table.entries[m.entry].level))
    }

    /// How many of a table's entries are in the song list.
    pub fn owned_count(&self, table: usize) -> usize {
        self.owned.get(table).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::md5_of_bytes;

    fn chart(n: u64) -> (ChartId, [u8; 16]) {
        let bytes = n.to_le_bytes();
        (ChartId::of_bytes(&bytes), md5_of_bytes(&bytes))
    }

    fn entry(level: &str, n: u64) -> TableEntry {
        let (id, md5) = chart(n);
        TableEntry {
            level: level.into(),
            md5: Some(md5),
            sha256: Some(id),
            title: format!("Chart {n}"),
            ..TableEntry::default()
        }
    }

    fn table(name: &str, symbol: &str, entries: Vec<TableEntry>) -> DifficultyTable {
        DifficultyTable {
            name: name.into(),
            symbol: symbol.into(),
            source: "https://example.invalid/header.json".into(),
            fetched: 1_791_500_000,
            entries,
            ..DifficultyTable::default()
        }
    }

    #[test]
    fn cache_round_trips_with_awkward_text_and_missing_hashes() {
        let (id, md5) = chart(1);
        let t = DifficultyTable {
            name: "Tab\tbed \\n table".into(),
            symbol: "★".into(),
            level_order: vec!["?".into(), "1".into(), "10".into()],
            entries: vec![
                TableEntry {
                    level: "★3".into(),
                    md5: Some(md5),
                    sha256: Some(id),
                    title: "日本語 \"title\"\twith tab".into(),
                    artist: "A\nB".into(),
                    url: "https://example.invalid/a?x=1&y=2".into(),
                    url_diff: "https://example.invalid/d".into(),
                },
                TableEntry {
                    level: "2".into(),
                    md5: Some(md5),
                    ..TableEntry::default()
                },
                TableEntry {
                    level: "2".into(),
                    sha256: Some(id),
                    ..TableEntry::default()
                },
            ],
            ..table("", "", vec![])
        };
        assert_eq!(DifficultyTable::parse(&t.serialize()), Some(t));
    }

    #[test]
    fn same_table_same_text() {
        let t = table("T", "t", vec![entry("1", 1), entry("2", 2)]);
        assert_eq!(t.serialize(), t.serialize());
        assert!(t.serialize().starts_with("#BEETLE_TABLE_V1\n"));
    }

    #[test]
    fn parse_rejects_other_files_and_skips_entries_nothing_could_match() {
        assert_eq!(DifficultyTable::parse(""), None);
        assert_eq!(DifficultyTable::parse("#BEETLE_SONGS_V2\nname=x\n"), None);
        assert_eq!(
            DifficultyTable::parse("#BEETLE_TABLE_V1\nsymbol=t\n#ENTRIES\n"),
            None,
            "no name"
        );

        let (id, _) = chart(3);
        let text = format!(
            "#BEETLE_TABLE_V1\nname=T\nfuture=1\n#ENTRIES\n1\t\t\tno hashes\n2\tnot-hex\t{}\tkept\n3\n",
            id.to_hex()
        );
        let t = DifficultyTable::parse(&text).unwrap();
        assert_eq!(
            t.entries.len(),
            1,
            "only the entry with a usable hash survives"
        );
        assert_eq!(t.entries[0].title, "kept");
        assert_eq!(t.entries[0].md5, None, "a malformed md5 is just missing");
    }

    #[test]
    fn natural_order_compares_numbers_as_numbers() {
        let mut levels = vec!["10", "2", "?", "1", "10a", "9", "003"];
        levels.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(levels, ["1", "2", "003", "9", "10", "10a", "?"]);
        // Leading zeros do not change the number.
        assert_eq!(natural_cmp("02", "2"), Ordering::Equal);

        let mut starred = vec!["★12", "★3", "★2"];
        starred.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(starred, ["★2", "★3", "★12"]);

        // Numbers too long for any integer type still compare.
        assert_eq!(
            natural_cmp(
                "100000000000000000000000000000",
                "99999999999999999999999999999"
            ),
            Ordering::Greater
        );
    }

    #[test]
    fn a_tables_own_level_order_wins_over_natural_order() {
        let mut t = table(
            "T",
            "t",
            vec![entry("1", 1), entry("?", 2), entry("10", 3), entry("2", 4)],
        );
        assert_eq!(t.levels(), ["1", "2", "10", "?"]);

        t.level_order = vec!["?".into(), "10".into(), "1".into()];
        // Listed levels first in the given order, then the rest naturally.
        assert_eq!(t.levels(), ["?", "10", "1", "2"]);
    }

    #[test]
    fn songs_match_by_sha256() {
        let mut index =
            TableIndex::new(vec![table("Sat", "sl", vec![entry("3", 1), entry("4", 2)])]);
        index.match_songs([chart(1), chart(99)]);

        let (owned, _) = chart(1);
        assert_eq!(index.chip(owned), Some("sl3".into()));
        assert_eq!(index.entry_for(0, owned).unwrap().title, "Chart 1");
        assert_eq!(index.matches_for(chart(99).0), &[]);
        assert_eq!(index.chip(chart(99).0), None);
        assert_eq!(index.owned_count(0), 1);
    }

    #[test]
    fn md5_is_only_the_fallback_for_entries_without_a_sha256() {
        let (id, md5) = chart(5);
        let md5_only = TableEntry {
            level: "7".into(),
            md5: Some(md5),
            ..TableEntry::default()
        };
        // Same md5 but a sha256 that is some other chart: the sha256 decides.
        let sha_disagrees = TableEntry {
            level: "8".into(),
            md5: Some(md5),
            sha256: Some(chart(6).0),
            ..TableEntry::default()
        };

        let mut index = TableIndex::new(vec![
            table("A", "a", vec![md5_only]),
            table("B", "b", vec![sha_disagrees]),
        ]);
        index.match_songs([(id, md5)]);

        assert_eq!(
            index.chip(id),
            Some("a7".into()),
            "matched by md5 in A only"
        );
        assert!(
            index.entry_for(1, id).is_none(),
            "B's sha256 names another chart"
        );
        assert_eq!((index.owned_count(0), index.owned_count(1)), (1, 0));
    }

    #[test]
    fn a_chart_in_several_tables_lists_them_in_table_order() {
        let mut index = TableIndex::new(vec![
            table("First", "f", vec![entry("1", 7)]),
            table("Second", "s", vec![entry("9", 7)]),
        ]);
        index.match_songs([chart(7)]);
        let id = chart(7).0;

        let found: Vec<(usize, usize)> = index
            .matches_for(id)
            .iter()
            .map(|m| (m.table, m.entry))
            .collect();
        assert_eq!(found, [(0, 0), (1, 0)]);
        assert_eq!(
            index.chip(id),
            Some("f1".into()),
            "the chip comes from the first table"
        );
        assert_eq!(index.entry_for(1, id).unwrap().level, "9");
    }

    #[test]
    fn a_chart_twice_in_one_table_takes_its_label_from_the_first_entry() {
        let mut index = TableIndex::new(vec![table(
            "T",
            "t",
            vec![entry("2", 8), entry("5", 8), entry("3", 9)],
        )]);
        index.match_songs([chart(8)]);
        assert_eq!(index.chip(chart(8).0), Some("t2".into()));
        assert_eq!(index.matches_for(chart(8).0).len(), 1);
        assert_eq!(index.owned_count(0), 2, "both entries are in the song list");
    }

    #[test]
    fn the_same_chart_listed_twice_in_the_song_list_matches_once() {
        let mut index = TableIndex::new(vec![table("T", "t", vec![entry("1", 4)])]);
        index.match_songs([chart(4), chart(4)]);
        assert_eq!(index.matches_for(chart(4).0).len(), 1);
        assert_eq!(index.owned_count(0), 1);
    }

    #[test]
    fn matching_again_replaces_the_earlier_result() {
        let mut index = TableIndex::new(vec![table("T", "t", vec![entry("1", 1), entry("2", 2)])]);
        index.match_songs([chart(1)]);
        assert_eq!(index.owned_count(0), 1);
        index.match_songs([chart(2)]);
        assert_eq!(index.owned_count(0), 1);
        assert_eq!(index.chip(chart(1).0), None);
        assert_eq!(index.chip(chart(2).0), Some("t2".into()));
        index.match_songs([]);
        assert_eq!(index.owned_count(0), 0);
    }

    #[test]
    fn no_tables_is_fine() {
        let mut index = TableIndex::default();
        index.match_songs([chart(1)]);
        assert_eq!(index.chip(chart(1).0), None);
        assert_eq!(index.owned_count(3), 0);
    }
}

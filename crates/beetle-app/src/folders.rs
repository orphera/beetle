//! The song select folder tree: a pure model built from the library, the
//! score records and the installed difficulty tables.
//!
//! A folder either holds child folders (a branch) or a list of songs (a leaf).
//! The list the screen shows is a `ListEntry` per row. A leaf lists songs;
//! a branch lists its child folders with their song counts. U3b adds a group
//! row (one row for the charts of one song, with difficulty tabs) as another
//! `ListEntry` variant; the rows are built here and drawn by `screens/select.rs`.

use std::collections::{BTreeMap, HashMap, HashSet};

use beetle_core::{ClearType, LnOption, PlayMode, ScoreStore, SongMetadata, TableIndex};
use beetle_render::{strings, theme};

/// Where the player is in the tree: the folder ids from the top level down.
/// The empty path is the root (the top-level folders are listed there).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FolderPath {
    segments: Vec<String>,
}

impl FolderPath {
    /// The path to a single top-level folder such as `all`.
    pub fn top(id: &str) -> Self {
        Self {
            segments: vec![id.to_string()],
        }
    }

    /// Reads the config form (`mode/7k/...`). Empty pieces are skipped.
    pub fn parse(text: &str) -> Self {
        Self {
            segments: text
                .split('/')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
        }
    }

    /// The config form: the ids joined with `/`.
    pub fn to_config_string(&self) -> String {
        self.segments.join("/")
    }

    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    pub fn depth(&self) -> usize {
        self.segments.len()
    }

    pub fn is_root(&self) -> bool {
        self.segments.is_empty()
    }

    /// The path one level up (the root's parent is the root).
    pub fn parent(&self) -> Self {
        self.truncated(self.segments.len().saturating_sub(1))
    }

    /// The path to a child of this folder.
    pub fn child(&self, id: &str) -> Self {
        let mut segments = self.segments.clone();
        segments.push(id.to_string());
        Self { segments }
    }

    /// The first `depth` segments (the breadcrumb at that depth).
    pub fn truncated(&self, depth: usize) -> Self {
        Self {
            segments: self.segments[..depth.min(self.segments.len())].to_vec(),
        }
    }

    /// The id of the last folder on the path, if any.
    pub fn last(&self) -> Option<&str> {
        self.segments.last().map(String::as_str)
    }
}

/// What a folder holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    /// Child folders, in display order. Never empty: empty folders are left out.
    Branch(Vec<Folder>),
    /// Song indices into the library, in library (sort) order.
    Leaf(Vec<usize>),
}

/// One folder of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    /// Stable id, used in the config file and in paths (never translated).
    pub id: String,
    /// What the screen shows: a name, a mode tag, a level, a table name.
    pub label: String,
    /// Songs under this folder.
    pub count: usize,
    pub body: Body,
}

/// One row of the visible list. Song rows hold an index into the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListEntry {
    /// A child folder of the current folder.
    Folder {
        id: String,
        label: String,
        count: usize,
    },
    Song(usize),
}

/// Key modes in the order the mode folder lists them.
const MODE_ORDER: [PlayMode; 8] = [
    PlayMode::Keys4,
    PlayMode::Keys5,
    PlayMode::Keys6,
    PlayMode::Keys7,
    PlayMode::Keys8,
    PlayMode::Keys9,
    PlayMode::Keys10,
    PlayMode::Keys14,
];

/// Clear lamps in the order the lamp folder lists them; `None` is 기록 없음.
const LAMP_ORDER: [Option<ClearType>; 7] = [
    Some(ClearType::Perfect),
    Some(ClearType::FullCombo),
    Some(ClearType::Hard),
    Some(ClearType::Clear),
    Some(ClearType::Easy),
    Some(ClearType::Failed),
    None,
];

/// The stable id of a key mode folder.
pub fn mode_id(mode: PlayMode) -> &'static str {
    match mode {
        PlayMode::Keys4 => "4k",
        PlayMode::Keys5 => "5k",
        PlayMode::Keys6 => "6k",
        PlayMode::Keys7 => "7k",
        PlayMode::Keys8 => "8k",
        PlayMode::Keys9 => "9k",
        PlayMode::Keys10 => "10k",
        PlayMode::Keys14 => "14k",
    }
}

/// The stable id of a clear lamp folder.
pub fn lamp_id(lamp: Option<ClearType>) -> &'static str {
    match lamp {
        Some(ClearType::Perfect) => "perfect",
        Some(ClearType::FullCombo) => "fullcombo",
        Some(ClearType::Hard) => "hard",
        Some(ClearType::Clear) => "clear",
        Some(ClearType::Easy) => "easy",
        Some(ClearType::Failed) => "failed",
        None => "none",
    }
}

/// A folder id has no `/` (the config form separates ids with it).
fn id_piece(text: &str) -> String {
    text.replace('/', "|")
}

fn leaf(id: String, label: String, songs: Vec<usize>) -> Option<Folder> {
    (!songs.is_empty()).then(|| Folder {
        id,
        label,
        count: songs.len(),
        body: Body::Leaf(songs),
    })
}

/// A folder of child folders. Its count is the number of distinct songs under
/// it: a song can sit in two children (the demo track is in 5K and in 7K).
fn branch(id: &str, label: &str, children: Vec<Folder>) -> Option<Folder> {
    if children.is_empty() {
        return None;
    }
    let mut songs = HashSet::new();
    for child in &children {
        collect_songs(child, &mut songs);
    }
    Some(Folder {
        id: id.to_string(),
        label: label.to_string(),
        count: songs.len(),
        body: Body::Branch(children),
    })
}

fn collect_songs(folder: &Folder, out: &mut HashSet<usize>) {
    match &folder.body {
        Body::Leaf(songs) => out.extend(songs.iter().copied()),
        Body::Branch(children) => {
            for child in children {
                collect_songs(child, out);
            }
        }
    }
}

/// The whole tree for the current library. Top level: 전체 곡, 키 모드, 레벨,
/// 클리어 램프, 난이도표. Folders with no songs are left out; 난이도표 is left
/// out when no table has a song in the library.
pub fn build_tree(
    songs: &[SongMetadata],
    score_store: &ScoreStore,
    tables: &TableIndex,
    ln_option: LnOption,
) -> Vec<Folder> {
    let mut root = Vec::new();
    root.extend(leaf(
        "all".into(),
        strings::FOLDER_ALL.into(),
        (0..songs.len()).collect(),
    ));

    // Key modes. The demo track is listed under 5K and 7K as it always was.
    let modes: Vec<Folder> = MODE_ORDER
        .iter()
        .filter_map(|&mode| {
            let idx = songs
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    s.play_mode == mode
                        || (is_demo(s) && matches!(mode, PlayMode::Keys5 | PlayMode::Keys7))
                })
                .map(|(i, _)| i)
                .collect();
            leaf(mode_id(mode).into(), theme::mode_label(mode).into(), idx)
        })
        .collect();
    root.extend(branch("mode", strings::FOLDER_MODE, modes));

    // Levels, ascending.
    let mut by_level: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, s) in songs.iter().enumerate() {
        by_level.entry(s.play_level).or_default().push(i);
    }
    let levels: Vec<Folder> = by_level
        .into_iter()
        .filter_map(|(level, idx)| leaf(level.to_string(), level.to_string(), idx))
        .collect();
    root.extend(branch("level", strings::FOLDER_LEVEL, levels));

    // Clear lamps: the best record under the player's long note rule.
    let lamps: Vec<Folder> = LAMP_ORDER
        .iter()
        .filter_map(|&lamp| {
            let idx = songs
                .iter()
                .enumerate()
                .filter(|(_, s)| score_store.best(s, ln_option).map(|r| r.clear_type) == lamp)
                .map(|(i, _)| i)
                .collect();
            let label = match lamp {
                Some(clear) => clear.as_str(),
                None => strings::NO_PLAY,
            };
            leaf(lamp_id(lamp).into(), label.into(), idx)
        })
        .collect();
    root.extend(branch("lamp", strings::FOLDER_LAMP, lamps));

    // Difficulty tables: a table holds its levels, in the table's own order.
    let mut table_folders = Vec::new();
    let mut used_ids: HashMap<String, usize> = HashMap::new();
    for (ti, table) in tables.tables().iter().enumerate() {
        let mut by_level: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, s) in songs.iter().enumerate() {
            if let Some(entry) = tables.entry_for(ti, s.id) {
                by_level.entry(entry.level.as_str()).or_default().push(i);
            }
        }
        let levels: Vec<Folder> = table
            .levels()
            .into_iter()
            .filter_map(|level| {
                let idx = by_level.remove(level)?;
                leaf(id_piece(level), format!("{}{}", table.symbol, level), idx)
            })
            .collect();
        // Two tables with the same name get distinct ids (the second one by its position).
        let mut id = id_piece(&table.name);
        if used_ids.contains_key(&id) {
            id = format!("{id}~{ti}");
        }
        used_ids.insert(id.clone(), ti);
        table_folders.extend(branch(&id, &table.name, levels));
    }
    root.extend(branch("table", strings::FOLDER_TABLE, table_folders));

    root
}

fn is_demo(song: &SongMetadata) -> bool {
    song.file_path == ":demo:"
}

/// The folder at `path`. `None` for the root and for a path that does not exist.
pub fn node<'a>(tree: &'a [Folder], path: &FolderPath) -> Option<&'a Folder> {
    let mut level = tree;
    let mut found = None;
    let count = path.depth();
    for (i, seg) in path.segments().iter().enumerate() {
        let folder = level.iter().find(|f| f.id == *seg)?;
        found = Some(folder);
        match &folder.body {
            Body::Branch(children) => level = children,
            // A leaf has no children: it can only be the last segment.
            Body::Leaf(_) if i + 1 < count => return None,
            Body::Leaf(_) => {}
        }
    }
    found
}

/// The folders listed at `path` (the root's top-level folders). `None` for a leaf.
pub fn children<'a>(tree: &'a [Folder], path: &FolderPath) -> Option<&'a [Folder]> {
    if path.is_root() {
        return Some(tree);
    }
    match &node(tree, path)?.body {
        Body::Branch(children) => Some(children),
        Body::Leaf(_) => None,
    }
}

/// The longest prefix of `path` that exists in the tree. A saved folder that
/// is gone (a table removed, a level with no songs now) falls back to its
/// nearest existing ancestor. An empty tree is the library not read yet (the
/// first layout runs before the worker finishes), so the path is kept as it is.
pub fn normalize(tree: &[Folder], path: &FolderPath) -> FolderPath {
    if tree.is_empty() {
        return path.clone();
    }
    let mut level = tree;
    let mut keep = 0;
    for (i, seg) in path.segments().iter().enumerate() {
        let Some(folder) = level.iter().find(|f| f.id == *seg) else {
            break;
        };
        keep = i + 1;
        match &folder.body {
            Body::Branch(children) => level = children,
            Body::Leaf(_) => break,
        }
    }
    path.truncated(keep)
}

/// The folder next to `path` at the same depth, wrapping at both ends.
/// From the root, forward is the first top-level folder and back is the last.
pub fn sibling(tree: &[Folder], path: &FolderPath, forward: bool) -> Option<FolderPath> {
    let parent = path.parent();
    let siblings = children(tree, &parent)?;
    let n = siblings.len();
    if n == 0 {
        return None;
    }
    let current = if path.is_root() {
        None
    } else {
        siblings
            .iter()
            .position(|f| Some(f.id.as_str()) == path.last())
    };
    let next = match current {
        Some(i) if forward => (i + 1) % n,
        Some(i) => (i + n - 1) % n,
        None if forward => 0,
        None => n - 1,
    };
    Some(parent.child(&siblings[next].id))
}

/// The breadcrumb labels for `path`: the root label, then each folder's label.
/// Index `d` is the path truncated to depth `d`.
pub fn crumbs(tree: &[Folder], path: &FolderPath) -> Vec<String> {
    let mut out = vec![strings::FOLDER_ROOT.to_string()];
    let mut level = tree;
    for seg in path.segments() {
        let Some(folder) = level.iter().find(|f| f.id == *seg) else {
            break;
        };
        out.push(folder.label.clone());
        match &folder.body {
            Body::Branch(children) => level = children,
            Body::Leaf(_) => break,
        }
    }
    out
}

/// Whether a song matches a search (title, artist or genre contains the query).
/// `query` must already be lower case and trimmed.
pub fn matches_query(song: &SongMetadata, query: &str) -> bool {
    song.title.to_lowercase().contains(query)
        || song.artist.to_lowercase().contains(query)
        || song.genre.to_lowercase().contains(query)
}

/// The rows to show for `path`. A branch lists its child folders with their
/// counts; a leaf lists its songs in library order.
///
/// A non-empty search replaces the list with matching songs: from the current
/// leaf when the folder is one, otherwise from every song in the library (a
/// branch has no songs of its own to search in).
pub fn entries_for(
    tree: &[Folder],
    path: &FolderPath,
    songs: &[SongMetadata],
    query: &str,
) -> Vec<ListEntry> {
    let q = query.trim().to_lowercase();
    if !q.is_empty() {
        let pool: Vec<usize> = match node(tree, path).map(|f| &f.body) {
            Some(Body::Leaf(idx)) => idx.clone(),
            _ => (0..songs.len()).collect(),
        };
        return pool
            .into_iter()
            .filter(|&i| matches_query(&songs[i], &q))
            .map(ListEntry::Song)
            .collect();
    }
    match children(tree, path) {
        Some(folders) => folders.iter().map(folder_entry).collect(),
        None => match node(tree, path).map(|f| &f.body) {
            Some(Body::Leaf(idx)) => idx.iter().copied().map(ListEntry::Song).collect(),
            _ => Vec::new(),
        },
    }
}

fn folder_entry(folder: &Folder) -> ListEntry {
    ListEntry::Folder {
        id: folder.id.clone(),
        label: folder.label.clone(),
        count: folder.count,
    }
}

/// Where the cursor should land after going to `path` from a folder: on the
/// child folder the player came from, when it is listed.
pub fn focus_index(entries: &[ListEntry], came_from: Option<&str>) -> usize {
    came_from
        .and_then(|id| {
            entries
                .iter()
                .position(|e| matches!(e, ListEntry::Folder { id: i, .. } if i == id))
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use beetle_core::{ChartId, DifficultyTable, TableEntry};

    fn song(n: u64, title: &str, level: u32, mode: PlayMode) -> SongMetadata {
        SongMetadata {
            id: ChartId::synthetic(n),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: n,
            file_path: format!("{title}.bms"),
            title: title.into(),
            subtitle: String::new(),
            artist: format!("artist {n}"),
            genre: String::new(),
            bpm: 120.0,
            bpm_min: 120.0,
            bpm_max: 120.0,
            play_level: level,
            notes_count: 100,
            play_mode: mode,
        }
    }

    fn table(
        name: &str,
        symbol: &str,
        levels: &[&str],
        entries: &[(u64, &str)],
    ) -> DifficultyTable {
        DifficultyTable {
            name: name.into(),
            symbol: symbol.into(),
            level_order: levels.iter().map(|l| (*l).into()).collect(),
            entries: entries
                .iter()
                .map(|(n, level)| TableEntry {
                    level: (*level).into(),
                    sha256: Some(ChartId::synthetic(*n)),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    /// Apple 7K 12, Banana 7K 13, Cherry 14K 12, Date 5K 5, Elder 9K 9.
    fn library() -> Vec<SongMetadata> {
        vec![
            song(1, "Apple", 12, PlayMode::Keys7),
            song(2, "Banana", 13, PlayMode::Keys7),
            song(3, "Cherry", 12, PlayMode::Keys14),
            song(4, "Date", 5, PlayMode::Keys5),
            song(5, "Elder", 9, PlayMode::Keys9),
        ]
    }

    fn tree_of(songs: &[SongMetadata], tables: &TableIndex) -> Vec<Folder> {
        build_tree(songs, &ScoreStore::new(), tables, LnOption::Cn)
    }

    fn ids(tree: &[Folder]) -> Vec<&str> {
        tree.iter().map(|f| f.id.as_str()).collect()
    }

    fn songs_of(entries: &[ListEntry]) -> Vec<usize> {
        entries
            .iter()
            .filter_map(|e| match e {
                ListEntry::Song(i) => Some(*i),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn top_level_lists_only_folders_that_have_songs() {
        let songs = library();
        let tree = tree_of(&songs, &TableIndex::default());
        // No tables installed, so 난이도표 is left out. Lamps: all "기록 없음".
        assert_eq!(ids(&tree), ["all", "mode", "level", "lamp"]);
        assert_eq!(tree[0].count, 5);
    }

    #[test]
    fn mode_folders_include_4k_6k_8k_and_skip_empty_ones() {
        let mut songs = library();
        songs.push(song(6, "Fig", 3, PlayMode::Keys4));
        songs.push(song(7, "Grape", 3, PlayMode::Keys8));
        let tree = tree_of(&songs, &TableIndex::default());
        let mode = tree.iter().find(|f| f.id == "mode").unwrap();
        let Body::Branch(modes) = &mode.body else {
            panic!("mode is a branch");
        };
        let labels: Vec<&str> = modes.iter().map(|f| f.label.as_str()).collect();
        assert_eq!(labels, ["4K", "5K", "7K", "8K", "9K", "14K"]);
        assert_eq!(mode.count, 7);
    }

    #[test]
    fn level_folders_ascend_and_count_their_songs() {
        let songs = library();
        let tree = tree_of(&songs, &TableIndex::default());
        let level = tree.iter().find(|f| f.id == "level").unwrap();
        let Body::Branch(levels) = &level.body else {
            panic!("level is a branch");
        };
        let shape: Vec<(&str, usize)> = levels.iter().map(|f| (f.id.as_str(), f.count)).collect();
        assert_eq!(shape, [("5", 1), ("9", 1), ("12", 2), ("13", 1)]);
    }

    #[test]
    fn table_levels_follow_the_table_order_and_empty_levels_are_omitted() {
        let songs = library();
        // Level order from the table: "★12" before "★5" (the table's own order, not numeric).
        let t = table(
            "Sat",
            "sl",
            &["★12", "★9", "★5", "★1"],
            &[(1, "★12"), (3, "★12"), (4, "★5"), (2, "★1")],
        );
        let mut tables = TableIndex::new(vec![t]);
        tables.match_songs(songs.iter().map(|s| (s.id, s.md5)));
        let tree = tree_of(&songs, &tables);
        let table_folder = tree.iter().find(|f| f.id == "table").unwrap();
        let Body::Branch(tables_list) = &table_folder.body else {
            panic!("table is a branch");
        };
        assert_eq!(tables_list.len(), 1);
        let sat = &tables_list[0];
        assert_eq!(
            (sat.id.as_str(), sat.label.as_str(), sat.count),
            ("Sat", "Sat", 4)
        );
        let Body::Branch(levels) = &sat.body else {
            panic!("a table is a branch of levels");
        };
        let labels: Vec<&str> = levels.iter().map(|f| f.label.as_str()).collect();
        // The table's order, with its symbol in front. Levels with no charts are left out.
        assert_eq!(labels, ["sl★12", "sl★5", "sl★1"]);
        assert_eq!(levels[0].count, 2);
    }

    #[test]
    fn a_table_with_no_owned_chart_is_omitted() {
        let songs = library();
        let t = table("Empty", "x", &["1"], &[(900, "1")]);
        let mut tables = TableIndex::new(vec![t]);
        tables.match_songs(songs.iter().map(|s| (s.id, s.md5)));
        let tree = tree_of(&songs, &tables);
        assert!(!ids(&tree).contains(&"table"));
    }

    #[test]
    fn same_named_tables_get_distinct_ids() {
        let songs = library();
        let a = table("Dup", "a", &["1"], &[(1, "1")]);
        let b = table("Dup", "b", &["1"], &[(2, "1")]);
        let mut tables = TableIndex::new(vec![a, b]);
        tables.match_songs(songs.iter().map(|s| (s.id, s.md5)));
        let tree = tree_of(&songs, &tables);
        let table_folder = tree.iter().find(|f| f.id == "table").unwrap();
        let Body::Branch(list) = &table_folder.body else {
            panic!();
        };
        let ids: Vec<&str> = list.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, ["Dup", "Dup~1"]);
    }

    #[test]
    fn a_leaf_lists_its_songs_and_a_branch_lists_folders() {
        let songs = library();
        let tree = tree_of(&songs, &TableIndex::default());
        let root = entries_for(&tree, &FolderPath::default(), &songs, "");
        assert_eq!(
            root,
            [
                ListEntry::Folder {
                    id: "all".into(),
                    label: strings::FOLDER_ALL.into(),
                    count: 5
                },
                ListEntry::Folder {
                    id: "mode".into(),
                    label: strings::FOLDER_MODE.into(),
                    count: 5
                },
                ListEntry::Folder {
                    id: "level".into(),
                    label: strings::FOLDER_LEVEL.into(),
                    count: 5
                },
                ListEntry::Folder {
                    id: "lamp".into(),
                    label: strings::FOLDER_LAMP.into(),
                    count: 5
                },
            ]
        );

        let level12 = FolderPath::top("level").child("12");
        let entries = entries_for(&tree, &level12, &songs, "");
        assert_eq!(songs_of(&entries), [0, 2]);
        assert_eq!(entries.len(), 2, "a leaf has no folder rows");

        let modes = FolderPath::top("mode");
        assert_eq!(entries_for(&tree, &modes, &songs, "").len(), 4);
    }

    #[test]
    fn search_in_a_leaf_stays_in_the_leaf() {
        let songs = library();
        let tree = tree_of(&songs, &TableIndex::default());
        let level12 = FolderPath::top("level").child("12");
        // "artist 3" matches Cherry (in level 12), but "banana" is in level 13.
        assert_eq!(
            songs_of(&entries_for(&tree, &level12, &songs, "artist 3")),
            [2]
        );
        assert!(entries_for(&tree, &level12, &songs, "banana").is_empty());
    }

    #[test]
    fn search_in_a_branch_searches_every_song() {
        let songs = library();
        let tree = tree_of(&songs, &TableIndex::default());
        // The level folder is a branch: its search covers all songs, not one level.
        let level = FolderPath::top("level");
        let found = entries_for(&tree, &level, &songs, "BANANA");
        assert_eq!(found, [ListEntry::Song(1)]);
        // Apple, Cherry, Date and Elder contain "e"; the artists ("artist N") do not.
        assert_eq!(
            songs_of(&entries_for(&tree, &FolderPath::default(), &songs, "e")).len(),
            4
        );
    }

    #[test]
    fn normalize_falls_back_to_the_nearest_existing_ancestor() {
        let songs = library();
        let tree = tree_of(&songs, &TableIndex::default());
        // Level 7 does not exist: back to the level branch.
        let gone = FolderPath::top("level").child("7");
        assert_eq!(normalize(&tree, &gone), FolderPath::top("level"));
        // A table that is not installed: back to the root.
        assert_eq!(
            normalize(&tree, &FolderPath::top("table").child("Sat")),
            FolderPath::default()
        );
        // A path that runs past a leaf stops at the leaf.
        let past_leaf = FolderPath::top("all").child("x");
        assert_eq!(normalize(&tree, &past_leaf), FolderPath::top("all"));
        let ok = FolderPath::top("mode").child("7k");
        assert_eq!(normalize(&tree, &ok), ok);
    }

    #[test]
    fn a_saved_folder_survives_an_empty_library_until_it_is_read() {
        let saved = FolderPath::parse("level/12");
        assert_eq!(normalize(&[], &saved), saved);
        assert_eq!(
            normalize(&[], &FolderPath::default()),
            FolderPath::default()
        );
    }

    #[test]
    fn sibling_moves_at_the_same_depth_and_wraps() {
        let songs = library();
        let tree = tree_of(&songs, &TableIndex::default());
        let l12 = FolderPath::top("level").child("12");
        assert_eq!(
            sibling(&tree, &l12, true),
            Some(FolderPath::top("level").child("13"))
        );
        assert_eq!(
            sibling(&tree, &FolderPath::top("level").child("13"), true),
            Some(FolderPath::top("level").child("5"))
        );
        assert_eq!(
            sibling(&tree, &FolderPath::top("level").child("5"), false),
            Some(FolderPath::top("level").child("13"))
        );
        // Top-level folders wrap too; the root goes to the first or last one.
        assert_eq!(
            sibling(&tree, &FolderPath::top("lamp"), true),
            Some(FolderPath::top("all"))
        );
        assert_eq!(
            sibling(&tree, &FolderPath::default(), true),
            Some(FolderPath::top("all"))
        );
        assert_eq!(
            sibling(&tree, &FolderPath::default(), false),
            Some(FolderPath::top("lamp"))
        );
    }

    #[test]
    fn crumbs_name_each_depth() {
        let songs = library();
        let tree = tree_of(&songs, &TableIndex::default());
        let path = FolderPath::top("level").child("12");
        assert_eq!(
            crumbs(&tree, &path),
            [strings::FOLDER_ROOT, strings::FOLDER_LEVEL, "12"]
        );
        assert_eq!(
            crumbs(&tree, &FolderPath::default()),
            [strings::FOLDER_ROOT]
        );
        assert_eq!(path.truncated(1), FolderPath::top("level"));
    }

    #[test]
    fn the_config_form_round_trips() {
        let path = FolderPath::top("mode").child("7k");
        assert_eq!(path.to_config_string(), "mode/7k");
        assert_eq!(FolderPath::parse("mode/7k"), path);
        assert_eq!(FolderPath::parse(" mode//7k/ "), path);
        assert!(FolderPath::parse("").is_root());
    }

    #[test]
    fn the_cursor_lands_on_the_folder_the_player_came_from() {
        let songs = library();
        let tree = tree_of(&songs, &TableIndex::default());
        let entries = entries_for(&tree, &FolderPath::default(), &songs, "");
        assert_eq!(focus_index(&entries, Some("level")), 2);
        assert_eq!(focus_index(&entries, Some("nope")), 0);
        assert_eq!(focus_index(&entries, None), 0);
    }

    #[test]
    fn slash_in_a_table_name_cannot_split_a_path() {
        let songs = library();
        let t = table("A/B", "x", &["1"], &[(1, "1")]);
        let mut tables = TableIndex::new(vec![t]);
        tables.match_songs(songs.iter().map(|s| (s.id, s.md5)));
        let tree = tree_of(&songs, &tables);
        let id = tree
            .iter()
            .find(|f| f.id == "table")
            .and_then(|f| match &f.body {
                Body::Branch(list) => list.first().map(|f| f.id.clone()),
                Body::Leaf(_) => None,
            })
            .unwrap();
        assert_eq!(id, "A|B");
    }
}

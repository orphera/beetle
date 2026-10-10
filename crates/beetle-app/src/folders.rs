//! The song select folder tree: a pure model built from the library, the
//! score records and the installed difficulty tables.
//!
//! A folder either holds child folders (a branch) or a list of songs (a leaf).
//! The list the screen shows is a `ListEntry` per row. A leaf lists songs;
//! a branch lists its child folders with their song counts. U3b adds a group
//! row (one row for the charts of one song, with difficulty tabs) as another
//! `ListEntry` variant; the rows are built here and drawn by `screens/select.rs`.

use std::collections::{BTreeMap, HashMap, HashSet};

use beetle_core::{ChartId, ClearType, LnOption, PlayMode, ScoreStore, SongMetadata, TableIndex};
use beetle_render::{strings, theme};

/// The chart each group shows when it is not the first one: group key → chart.
/// Only choices away from the default (the lowest level) are kept.
pub type ChartChoices = HashMap<u64, ChartId>;

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
    /// Songs under this folder per clear lamp, in `LAMP_ORDER` order (filled by `build_tree`).
    pub lamps: [usize; LAMP_COUNT],
}

/// One row of the visible list. Song rows hold an index into the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListEntry {
    /// A child folder of the current folder.
    Folder {
        id: String,
        label: String,
        count: usize,
        /// The folder's songs per clear lamp (see `Folder::lamps`).
        lamps: [usize; LAMP_COUNT],
    },
    /// The charts of one song (same folder and base title), shown as one row.
    Group {
        /// The group key (see `group_key`); it names the remembered chart.
        key: u64,
        /// The base title (the title without its trailing `[...]`).
        title: String,
        /// Song indices, by level then title. Never fewer than two.
        charts: Vec<usize>,
        /// The position in `charts` the row shows.
        selected: usize,
        /// The chip label of each chart, in `charts` order (see `chip_labels`).
        labels: Vec<String>,
    },
    Song(usize),
}

impl ListEntry {
    /// The library index of the song the row plays: a group's selected chart.
    pub fn song(&self) -> Option<usize> {
        match self {
            ListEntry::Song(i) => Some(*i),
            ListEntry::Group {
                charts, selected, ..
            } => charts.get(*selected).copied(),
            ListEntry::Folder { .. } => None,
        }
    }
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
/// The breakdown rows of the folder detail panel use the same order
/// (`beetle_render::LAMP_COUNT` rows, see `screens/select.rs`).
const LAMP_ORDER: [Option<ClearType>; LAMP_COUNT] = [
    Some(ClearType::Perfect),
    Some(ClearType::FullCombo),
    Some(ClearType::Hard),
    Some(ClearType::Clear),
    Some(ClearType::Easy),
    Some(ClearType::Failed),
    None,
];

/// How many clear lamps a folder counts: one per `LAMP_ORDER` entry.
pub const LAMP_COUNT: usize = beetle_render::LAMP_COUNT;

/// The id of the 전곡 folder at the top of 난이도표 (no table may take it).
const TABLE_ALL_ID: &str = "all";

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

/// The key mode with this folder id (the inverse of `mode_id`).
pub fn mode_from_id(id: &str) -> Option<PlayMode> {
    MODE_ORDER.iter().copied().find(|&m| mode_id(m) == id)
}

/// The key modes the library has songs in, in the folder order.
pub fn present_modes(songs: &[SongMetadata]) -> Vec<PlayMode> {
    MODE_ORDER
        .iter()
        .copied()
        .filter(|&m| songs.iter().any(|s| s.play_mode == m))
        .collect()
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
    (!songs.is_empty()).then_some(Folder {
        id,
        label,
        count: songs.len(),
        body: Body::Leaf(songs),
        lamps: [0; LAMP_COUNT],
    })
}

/// A folder of child folders. Its count is the number of distinct songs under
/// it: a song can sit in two children (a song in two tables).
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
        lamps: [0; LAMP_COUNT],
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
///
/// `pass[i]` says whether song `i` passes the filter (see `filters.rs`). Only
/// passing songs are listed, so the counts are the counts of the filtered list.
pub fn build_tree(
    songs: &[SongMetadata],
    score_store: &ScoreStore,
    tables: &TableIndex,
    ln_option: LnOption,
    pass: &[bool],
) -> Vec<Folder> {
    let mut root = Vec::new();
    root.extend(leaf(
        "all".into(),
        strings::FOLDER_ALL.into(),
        (0..songs.len()).filter(|&i| pass[i]).collect(),
    ));

    // Key modes.
    let modes: Vec<Folder> = MODE_ORDER
        .iter()
        .filter_map(|&mode| {
            let idx = songs
                .iter()
                .enumerate()
                .filter(|(i, s)| pass[*i] && s.play_mode == mode)
                .map(|(i, _)| i)
                .collect();
            leaf(mode_id(mode).into(), theme::mode_label(mode).into(), idx)
        })
        .collect();
    root.extend(branch("mode", strings::FOLDER_MODE, modes));

    // Levels, ascending.
    let mut by_level: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, s) in songs.iter().enumerate().filter(|(i, _)| pass[*i]) {
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
                .filter(|(i, s)| {
                    pass[*i] && score_store.best(s, ln_option).map(|r| r.clear_type) == lamp
                })
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
    // Each table starts with 전곡: every passing song of that table, in library order.
    let mut table_folders = Vec::new();
    let mut used_ids: HashMap<String, usize> = HashMap::new();
    for (ti, table) in tables.tables().iter().enumerate() {
        let mut by_level: HashMap<&str, Vec<usize>> = HashMap::new();
        let order = table.levels();
        let mut all_of_table = Vec::new();
        for (i, s) in songs.iter().enumerate().filter(|(i, _)| pass[*i]) {
            if let Some(entry) = tables.entry_for(ti, s.id) {
                by_level.entry(entry.level.as_str()).or_default().push(i);
                all_of_table.push(i);
            }
        }
        // 전곡 runs in the table's level order; within one level the library order stays.
        all_of_table.sort_by_key(|&i| {
            let level = tables
                .entry_for(ti, songs[i].id)
                .map_or("", |e| e.level.as_str());
            order.iter().position(|&l| l == level)
        });
        let mut levels: Vec<Folder> = leaf(
            TABLE_ALL_ID.into(),
            strings::FOLDER_TABLE_ALL.into(),
            all_of_table,
        )
        .into_iter()
        .collect();
        levels.extend(order.iter().copied().filter_map(|level| {
            let idx = by_level.remove(level)?;
            leaf(id_piece(level), format!("{}{}", table.symbol, level), idx)
        }));
        // Two tables with the same name get distinct ids (the second one by its position).
        let mut id = id_piece(&table.name);
        if used_ids.contains_key(&id) {
            id = format!("{id}~{ti}");
        }
        used_ids.insert(id.clone(), ti);
        table_folders.extend(branch(&id, &table.name, levels));
    }
    root.extend(branch("table", strings::FOLDER_TABLE, table_folders));

    // Clear lamp breakdown per folder, from the same passing songs and records.
    let lamp_of = |i: usize| score_store.best(&songs[i], ln_option).map(|r| r.clear_type);
    for folder in root.iter_mut() {
        fill_lamps(folder, &lamp_of);
    }

    root
}

/// Songs per clear lamp (`LAMP_ORDER`) among the songs `indices` of the library.
fn lamp_counts(
    indices: &[usize],
    lamp_of: &impl Fn(usize) -> Option<ClearType>,
) -> [usize; LAMP_COUNT] {
    let mut counts = [0; LAMP_COUNT];
    for &i in indices {
        let lamp = lamp_of(i);
        if let Some(k) = LAMP_ORDER.iter().position(|&l| l == lamp) {
            counts[k] += 1;
        }
    }
    counts
}

/// Sets `lamps` of `folder` and of every folder under it. A branch counts its
/// distinct songs, as `count` does: a song in two child folders (a song in two
/// tables) counts once.
fn fill_lamps(folder: &mut Folder, lamp_of: &impl Fn(usize) -> Option<ClearType>) {
    folder.lamps = match &mut folder.body {
        Body::Leaf(songs) => lamp_counts(songs, lamp_of),
        Body::Branch(children) => {
            for child in children.iter_mut() {
                fill_lamps(child, lamp_of);
            }
            let mut distinct = HashSet::new();
            for child in children.iter() {
                collect_songs(child, &mut distinct);
            }
            let songs: Vec<usize> = distinct.into_iter().collect();
            lamp_counts(&songs, lamp_of)
        }
    };
}

/// Whether the library is empty, which is when the song list shows the
/// first-run guide instead of songs.
pub fn is_first_run(songs: &[SongMetadata]) -> bool {
    songs.is_empty()
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

/// The form a search compares in: lower case with every whitespace character
/// removed, so a title spelled `A D D i c` still finds `addic`.
pub fn search_key(text: &str) -> String {
    text.split_whitespace().collect::<String>().to_lowercase()
}

/// Whether a song matches a search (title, artist or genre contains the query).
/// `query` must already be a `search_key`.
pub fn matches_query(song: &SongMetadata, query: &str) -> bool {
    search_key(&song.title).contains(query)
        || search_key(&song.artist).contains(query)
        || search_key(&song.genre).contains(query)
}

/// The folder a chart sits in: its directory, with `\` read as `/`. A chart
/// inside a package (`pkg.bmsp::sub/chart.bms`) is in the package path plus its
/// directory inside the package. `None` for a chart with no directory.
pub fn song_folder(file_path: &str) -> Option<String> {
    let path = file_path.replace('\\', "/");
    let dir = |text: &str| text.rsplit_once('/').map_or("", |(d, _)| d).to_string();
    match path.split_once("::") {
        Some((package, inner)) => Some(format!("{package}::{}", dir(inner))),
        None => Some(dir(&path)).filter(|d| !d.is_empty()),
    }
}

/// The title split at its trailing `[...]` or `(...)` part:
/// `AIRSHAVER [7key, Another]` → (`AIRSHAVER`, `7key, Another`). `None` when
/// there is no such part or stripping would leave nothing.
fn split_trailing_bracket(title: &str) -> Option<(&str, &str)> {
    let t = title.trim();
    for (open, close) in [('[', ']'), ('(', ')')] {
        if let Some(stripped) = t.strip_suffix(close) {
            if let Some(pos) = stripped.rfind(open) {
                let base = stripped[..pos].trim_end();
                if !base.is_empty() {
                    return Some((base, stripped[pos + open.len_utf8()..].trim()));
                }
            }
        }
    }
    None
}

/// The title without its trailing `[...]` or `(...)` part:
/// `AIRSHAVER [7key, Another]` → `AIRSHAVER`. The whole title when stripping
/// would leave nothing.
pub fn base_title(title: &str) -> &str {
    split_trailing_bracket(title).map_or(title.trim(), |(base, _)| base)
}

/// Most characters of a chart's tag in its chip label.
const CHIP_TAG_MAX: usize = 8;

/// A tag of at most `CHIP_TAG_MAX` characters from `text`: the text when it
/// fits, else its last word that fits and has a letter or digit, else its first
/// characters. `None` for blank text.
fn short_tag(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let fits = |w: &str| w.chars().count() <= CHIP_TAG_MAX;
    if fits(text) {
        return Some(text.to_string());
    }
    let word = text
        .split_whitespace()
        .rev()
        .find(|w| fits(w) && w.chars().any(char::is_alphanumeric));
    Some(match word {
        Some(w) => w.to_string(),
        None => text
            .chars()
            .take(CHIP_TAG_MAX)
            .collect::<String>()
            .trim_end()
            .to_string(),
    })
}

/// `base` with each tag appended (`6K 21 UwU`), when the labels come out all
/// different. A chart without a tag keeps `base`.
fn labels_with_tags(base: &str, tags: &[Option<String>]) -> Option<Vec<String>> {
    let labels: Vec<String> = tags
        .iter()
        .map(|tag| match tag {
            Some(tag) => format!("{base} {tag}"),
            None => base.to_string(),
        })
        .collect();
    let distinct: HashSet<&String> = labels.iter().collect();
    (distinct.len() == labels.len()).then_some(labels)
}

/// The chip label of each chart of a group, in `charts` order: `{mode} {level}`
/// (`7K 12`). Charts that would show the same label are all told apart at once,
/// by the trailing bracket of their title (`6K 21 UwU`), else by their subtitle,
/// else by an ordinal (`6K 21 #2`). Charts with a unique label keep it.
pub fn chip_labels(songs: &[SongMetadata], charts: &[usize]) -> Vec<String> {
    let base: Vec<String> = charts
        .iter()
        .map(|&i| {
            format!(
                "{} {}",
                theme::mode_label(songs[i].play_mode),
                songs[i].play_level
            )
        })
        .collect();
    let mut labels = base.clone();
    let mut done: HashSet<&str> = HashSet::new();
    for text in &base {
        if !done.insert(text.as_str()) {
            continue;
        }
        let run: Vec<usize> = (0..base.len()).filter(|&q| base[q] == *text).collect();
        if run.len() < 2 {
            continue;
        }
        let members: Vec<&SongMetadata> = run.iter().map(|&q| &songs[charts[q]]).collect();
        let bracket: Vec<Option<String>> = members
            .iter()
            .map(|m| split_trailing_bracket(&m.title).and_then(|(_, inner)| short_tag(inner)))
            .collect();
        let subtitle: Vec<Option<String>> =
            members.iter().map(|m| short_tag(&m.subtitle)).collect();
        let picked = labels_with_tags(text, &bracket)
            .or_else(|| labels_with_tags(text, &subtitle))
            .unwrap_or_else(|| (1..=run.len()).map(|k| format!("{text} #{k}")).collect());
        for (&q, label) in run.iter().zip(picked) {
            labels[q] = label;
        }
    }
    labels
}

/// The key of a song's group: its folder and base title, FNV-1a 64. `None`
/// when the song has no folder (a loose chart file), so it always gets its own row.
pub fn group_key(song: &SongMetadata) -> Option<u64> {
    let folder = song_folder(&song.file_path)?;
    let text = format!("{folder}\n{}", base_title(&song.title));
    Some(text.bytes().fold(0xcbf2_9ce4_8422_2325, |h: u64, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    }))
}

/// The rows for `indices` (song indices in list order). Charts with the same
/// group key share one row, which takes the place of its first chart. A
/// group of one chart is a plain song row. The charts run by level, then title,
/// and the row shows the remembered chart (or the first one).
pub fn group_entries(
    songs: &[SongMetadata],
    indices: &[usize],
    choices: &ChartChoices,
) -> Vec<ListEntry> {
    enum Slot {
        Song(usize),
        Group(u64),
    }
    let mut members: HashMap<u64, Vec<usize>> = HashMap::new();
    let mut order = Vec::new();
    for &i in indices {
        match group_key(&songs[i]) {
            Some(key) => {
                let bucket = members.entry(key).or_default();
                if bucket.is_empty() {
                    order.push(Slot::Group(key));
                }
                bucket.push(i);
            }
            None => order.push(Slot::Song(i)),
        }
    }
    order
        .into_iter()
        .map(|slot| match slot {
            Slot::Song(i) => ListEntry::Song(i),
            Slot::Group(key) => {
                let mut charts = members.remove(&key).unwrap_or_default();
                if charts.len() == 1 {
                    return ListEntry::Song(charts[0]);
                }
                charts.sort_by(|&a, &b| {
                    (songs[a].play_level, &songs[a].title)
                        .cmp(&(songs[b].play_level, &songs[b].title))
                });
                let selected = choices
                    .get(&key)
                    .and_then(|id| charts.iter().position(|&c| songs[c].id == *id))
                    .unwrap_or(0);
                let labels = chip_labels(songs, &charts);
                ListEntry::Group {
                    key,
                    title: base_title(&songs[charts[0]].title).to_string(),
                    charts,
                    selected,
                    labels,
                }
            }
        })
        .collect()
}

/// The row that shows the chart `id`: its song row, or the group holding it.
pub fn row_showing(entries: &[ListEntry], songs: &[SongMetadata], id: ChartId) -> Option<usize> {
    entries.iter().position(|e| match e {
        ListEntry::Song(i) => songs.get(*i).is_some_and(|s| s.id == id),
        ListEntry::Group { charts, .. } => charts
            .iter()
            .any(|&c| songs.get(c).is_some_and(|s| s.id == id)),
        ListEntry::Folder { .. } => false,
    })
}

/// The songs a search at `path` looks in: the current leaf's songs, or every
/// passing song when the folder is a branch (or the root).
fn search_pool(
    tree: &[Folder],
    path: &FolderPath,
    songs: &[SongMetadata],
    pass: &[bool],
) -> Vec<usize> {
    match node(tree, path).map(|f| &f.body) {
        Some(Body::Leaf(idx)) => idx.clone(),
        _ => (0..songs.len()).filter(|&i| pass[i]).collect(),
    }
}

/// How many songs the list at `path` shows as a result: the matches of a
/// non-empty search in its pool, else the songs of the folder (the whole
/// passing library at the root). Used for the "N곡 찾음" count.
pub fn result_count(
    tree: &[Folder],
    path: &FolderPath,
    songs: &[SongMetadata],
    query: &str,
    pass: &[bool],
) -> usize {
    let q = search_key(query);
    if q.is_empty() {
        return match node(tree, path) {
            Some(folder) => folder.count,
            None => pass.iter().filter(|&&p| p).count(),
        };
    }
    search_pool(tree, path, songs, pass)
        .into_iter()
        .filter(|&i| matches_query(&songs[i], &q))
        .count()
}

/// The rows to show for `path`. A branch lists its child folders with their
/// counts; a leaf lists its songs, with the charts of one song grouped.
///
/// A non-empty search replaces the list with matching songs: from the current
/// leaf when the folder is one, otherwise from every passing song in the
/// library (a branch has no songs of its own to search in). Matches are
/// grouped too. `pass` is the filter mask; a leaf already holds only passing songs.
pub fn entries_for(
    tree: &[Folder],
    path: &FolderPath,
    songs: &[SongMetadata],
    query: &str,
    choices: &ChartChoices,
    pass: &[bool],
) -> Vec<ListEntry> {
    let q = search_key(query);
    let flat = is_table_all_songs(path);
    let rows = |indices: &[usize]| {
        if flat {
            indices.iter().map(|&i| ListEntry::Song(i)).collect()
        } else {
            group_entries(songs, indices, choices)
        }
    };
    if !q.is_empty() {
        let matched: Vec<usize> = search_pool(tree, path, songs, pass)
            .into_iter()
            .filter(|&i| matches_query(&songs[i], &q))
            .collect();
        return rows(&matched);
    }
    match children(tree, path) {
        Some(folders) => folders.iter().map(folder_entry).collect(),
        None => match node(tree, path).map(|f| &f.body) {
            Some(Body::Leaf(idx)) => rows(idx),
            _ => Vec::new(),
        },
    }
}

/// Whether `path` is the 전곡 folder of a table: its charts are listed one row each,
/// never grouped by song (the other folders group the charts of one song).
fn is_table_all_songs(path: &FolderPath) -> bool {
    matches!(path.segments(), [table, _, all] if table == "table" && all == TABLE_ALL_ID)
}

fn folder_entry(folder: &Folder) -> ListEntry {
    ListEntry::Folder {
        id: folder.id.clone(),
        label: folder.label.clone(),
        count: folder.count,
        lamps: folder.lamps,
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
        build_tree(
            songs,
            &ScoreStore::new(),
            tables,
            LnOption::Cn,
            &pass_all(songs),
        )
    }

    /// Every song passes: the filter is off.
    fn pass_all(songs: &[SongMetadata]) -> Vec<bool> {
        vec![true; songs.len()]
    }

    fn ids(tree: &[Folder]) -> Vec<&str> {
        tree.iter().map(|f| f.id.as_str()).collect()
    }

    #[test]
    fn lamp_breakdown_counts_each_lamp_and_sums_branches() {
        // Songs 0..=4: 0 and 2 PERFECT, 1 no record, 3 FAILED, 4 CLEAR.
        let lamp = |i: usize| match i {
            0 | 2 => Some(ClearType::Perfect),
            1 => None,
            3 => Some(ClearType::Failed),
            _ => Some(ClearType::Clear),
        };
        // LAMP_ORDER: PERFECT, FULL COMBO, HARD, CLEAR, EASY, FAILED, no record.
        assert_eq!(lamp_counts(&[0, 1, 2, 3, 4], &lamp), [2, 0, 0, 1, 0, 1, 1]);
        assert_eq!(lamp_counts(&[], &lamp), [0; LAMP_COUNT]);

        let leaf = |id: &str, songs: Vec<usize>| Folder {
            id: id.into(),
            label: id.into(),
            count: songs.len(),
            body: Body::Leaf(songs),
            lamps: [0; LAMP_COUNT],
        };
        let mut tree = Folder {
            id: "root".into(),
            label: "root".into(),
            count: 4,
            body: Body::Branch(vec![leaf("a", vec![0, 1]), leaf("b", vec![2, 3])]),
            lamps: [0; LAMP_COUNT],
        };
        fill_lamps(&mut tree, &lamp);
        assert_eq!(tree.lamps, [2, 0, 0, 0, 0, 1, 1]);
        let Body::Branch(children) = &tree.body else {
            panic!("a branch")
        };
        assert_eq!(children[0].lamps, [1, 0, 0, 0, 0, 0, 1]);
        assert_eq!(children[1].lamps, [1, 0, 0, 0, 0, 1, 0]);

        // A song in two children counts once in the branch, as its `count` does.
        let mut shared = Folder {
            id: "root".into(),
            label: "root".into(),
            count: 3,
            body: Body::Branch(vec![leaf("a", vec![0, 1]), leaf("b", vec![1, 2])]),
            lamps: [0; LAMP_COUNT],
        };
        fill_lamps(&mut shared, &lamp);
        assert_eq!(shared.lamps, [2, 0, 0, 0, 0, 0, 1]);
        assert_eq!(shared.lamps.iter().sum::<usize>(), 3);
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
        // 전곡 first, then the table's order, with its symbol in front. Levels with no charts are left out.
        assert_eq!(labels, ["전곡", "sl★12", "sl★5", "sl★1"]);
        assert_eq!(levels[0].count, 4);
        assert_eq!(levels[1].count, 2);
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
    fn each_table_starts_with_an_all_songs_folder_in_library_order() {
        let songs = library();
        // A holds Apple and Cherry; B holds Cherry and Banana.
        let a = table("A", "a", &["1"], &[(1, "1"), (3, "1")]);
        let b = table("B", "b", &["1"], &[(3, "1"), (2, "1")]);
        let mut tables = TableIndex::new(vec![a, b]);
        tables.match_songs(songs.iter().map(|s| (s.id, s.md5)));
        let tree = tree_of(&songs, &tables);
        let table_folder = tree.iter().find(|f| f.id == "table").unwrap();
        let Body::Branch(list) = &table_folder.body else {
            panic!("table is a branch");
        };
        let ids: Vec<&str> = list.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, ["A", "B"]);

        for (table_id, expected) in [("A", vec![0, 2]), ("B", vec![1, 2])] {
            let path = FolderPath::top("table").child(table_id).child(TABLE_ALL_ID);
            let rows = entries_for(
                &tree,
                &path,
                &songs,
                "",
                &ChartChoices::new(),
                &pass_all(&songs),
            );
            assert_eq!(songs_of(&rows), expected, "table {table_id}");
        }
        let Body::Branch(levels) = &list[0].body else {
            panic!("a table is a branch");
        };
        assert_eq!(levels[0].id, TABLE_ALL_ID);
        assert_eq!(levels[0].label, strings::FOLDER_TABLE_ALL);
        assert_eq!(levels[0].count, 2);
    }

    #[test]
    fn the_all_songs_folder_follows_the_table_level_order() {
        let songs = library();
        // The table lists level "b" before "a". Apple (index 0) is "a", Banana (index 1) is "b".
        let t = table("T", "t", &["b", "a"], &[(1, "a"), (2, "b")]);
        let mut tables = TableIndex::new(vec![t]);
        tables.match_songs(songs.iter().map(|s| (s.id, s.md5)));
        let tree = tree_of(&songs, &tables);
        let all = FolderPath::top("table").child("T").child(TABLE_ALL_ID);
        let rows = entries_for(
            &tree,
            &all,
            &songs,
            "",
            &ChartChoices::new(),
            &pass_all(&songs),
        );
        assert_eq!(rows, [ListEntry::Song(1), ListEntry::Song(0)]);
    }

    #[test]
    fn the_table_all_songs_folder_lists_each_chart_of_a_song_on_its_own_row() {
        let songs = vec![
            chart(1, "x/AIR/a.bms", "AIR [7key]", 11),
            chart(2, "x/AIR/b.bms", "AIR [14key]", 12),
        ];
        let t = table("T", "t", &["11", "12"], &[(1, "11"), (2, "12")]);
        let mut tables = TableIndex::new(vec![t]);
        tables.match_songs(songs.iter().map(|s| (s.id, s.md5)));
        let tree = build_tree(
            &songs,
            &ScoreStore::new(),
            &tables,
            LnOption::Cn,
            &pass_all(&songs),
        );
        let pass = pass_all(&songs);
        let all = FolderPath::top("table").child("T").child(TABLE_ALL_ID);
        let rows = entries_for(&tree, &all, &songs, "", &ChartChoices::new(), &pass);
        assert_eq!(rows, [ListEntry::Song(0), ListEntry::Song(1)]);
        // The same charts in the top-level 전곡 folder stay one group row.
        let top = entries_for(
            &tree,
            &FolderPath::top("all"),
            &songs,
            "",
            &ChartChoices::new(),
            &pass,
        );
        assert!(matches!(top[0], ListEntry::Group { .. }));
        // Search inside the table's 전곡 folder is flat too.
        let found = entries_for(&tree, &all, &songs, "air", &ChartChoices::new(), &pass);
        assert_eq!(found, [ListEntry::Song(0), ListEntry::Song(1)]);
    }

    #[test]
    fn the_all_songs_folder_follows_the_filter_and_a_table_named_all_is_unchanged() {
        let songs = library();
        let t = table("all", "x", &["1"], &[(1, "1"), (2, "1")]);
        let mut tables = TableIndex::new(vec![t]);
        tables.match_songs(songs.iter().map(|s| (s.id, s.md5)));
        // Only the level-12 songs pass: Apple (in the table) passes, Banana does not.
        let pass: Vec<bool> = songs.iter().map(|s| s.play_level == 12).collect();
        let tree = build_tree(&songs, &ScoreStore::new(), &tables, LnOption::Cn, &pass);
        let table_folder = tree.iter().find(|f| f.id == "table").unwrap();
        let Body::Branch(list) = &table_folder.body else {
            panic!("table is a branch");
        };
        assert_eq!(list[0].id, "all");
        let Body::Branch(levels) = &list[0].body else {
            panic!("a table is a branch");
        };
        assert_eq!(levels[0].id, TABLE_ALL_ID);
        assert_eq!(levels[0].count, 1);
    }

    #[test]
    fn a_leaf_lists_its_songs_and_a_branch_lists_folders() {
        let songs = library();
        let tree = tree_of(&songs, &TableIndex::default());
        let root = entries_for(
            &tree,
            &FolderPath::default(),
            &songs,
            "",
            &ChartChoices::new(),
            &pass_all(&songs),
        );
        assert_eq!(
            root,
            [
                ListEntry::Folder {
                    id: "all".into(),
                    label: strings::FOLDER_ALL.into(),
                    count: 5,
                    lamps: [0, 0, 0, 0, 0, 0, 5],
                },
                ListEntry::Folder {
                    id: "mode".into(),
                    label: strings::FOLDER_MODE.into(),
                    count: 5,
                    lamps: [0, 0, 0, 0, 0, 0, 5],
                },
                ListEntry::Folder {
                    id: "level".into(),
                    label: strings::FOLDER_LEVEL.into(),
                    count: 5,
                    lamps: [0, 0, 0, 0, 0, 0, 5],
                },
                ListEntry::Folder {
                    id: "lamp".into(),
                    label: strings::FOLDER_LAMP.into(),
                    count: 5,
                    lamps: [0, 0, 0, 0, 0, 0, 5],
                },
            ]
        );

        let level12 = FolderPath::top("level").child("12");
        let entries = entries_for(
            &tree,
            &level12,
            &songs,
            "",
            &ChartChoices::new(),
            &pass_all(&songs),
        );
        assert_eq!(songs_of(&entries), [0, 2]);
        assert_eq!(entries.len(), 2, "a leaf has no folder rows");

        let modes = FolderPath::top("mode");
        assert_eq!(
            entries_for(
                &tree,
                &modes,
                &songs,
                "",
                &ChartChoices::new(),
                &pass_all(&songs)
            )
            .len(),
            4
        );
    }

    #[test]
    fn a_search_ignores_spaces_in_titles_and_queries() {
        let spaced = song(1, "A D D i c T i O N 4 5 0 0 0 0 0", 1, PlayMode::Keys5);
        let plain = song(2, "ADDicTiON", 1, PlayMode::Keys5);
        // Spaced title, unspaced query.
        assert!(matches_query(&spaced, &search_key("addiction")));
        // Spaced title, query with its own spaces.
        assert!(matches_query(&spaced, &search_key("add ic")));
        // Unspaced title, spaced query.
        assert!(matches_query(&plain, &search_key("add ic")));
        // Case is still ignored.
        assert!(matches_query(&plain, &search_key("ADDICTION")));
        assert!(!matches_query(&plain, &search_key("banana")));
        // A blank query becomes empty, which callers treat as "no search".
        assert!(search_key("   ").is_empty());
    }

    #[test]
    fn a_search_in_korean_or_japanese_matches_with_or_without_spaces() {
        let korean = song(1, "사랑 노래", 1, PlayMode::Keys5);
        let japanese = song(2, "ハロー ワールド", 1, PlayMode::Keys5);
        assert!(matches_query(&korean, &search_key("사랑 노래")));
        assert!(matches_query(&korean, &search_key("사랑노래")));
        assert!(!matches_query(&korean, &search_key("노래 사랑")));
        assert!(matches_query(&japanese, &search_key("ハロー ワールド")));
        assert!(matches_query(&japanese, &search_key("ハローワールド")));
        assert!(!matches_query(&japanese, &search_key("ワールド ハロー")));
    }

    #[test]
    fn search_in_a_leaf_stays_in_the_leaf() {
        let songs = library();
        let tree = tree_of(&songs, &TableIndex::default());
        let level12 = FolderPath::top("level").child("12");
        // "artist 3" matches Cherry (in level 12), but "banana" is in level 13.
        assert_eq!(
            songs_of(&entries_for(
                &tree,
                &level12,
                &songs,
                "artist 3",
                &ChartChoices::new(),
                &pass_all(&songs)
            )),
            [2]
        );
        assert!(entries_for(
            &tree,
            &level12,
            &songs,
            "banana",
            &ChartChoices::new(),
            &pass_all(&songs)
        )
        .is_empty());
    }

    #[test]
    fn search_in_a_branch_searches_every_song() {
        let songs = library();
        let tree = tree_of(&songs, &TableIndex::default());
        // The level folder is a branch: its search covers all songs, not one level.
        let level = FolderPath::top("level");
        let found = entries_for(
            &tree,
            &level,
            &songs,
            "BANANA",
            &ChartChoices::new(),
            &pass_all(&songs),
        );
        assert_eq!(found, [ListEntry::Song(1)]);
        // Apple, Cherry, Date and Elder contain "e"; the artists ("artist N") do not.
        assert_eq!(
            songs_of(&entries_for(
                &tree,
                &FolderPath::default(),
                &songs,
                "e",
                &ChartChoices::new(),
                &pass_all(&songs)
            ))
            .len(),
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
    fn an_empty_library_builds_an_empty_tree_and_no_rows() {
        let songs: Vec<SongMetadata> = Vec::new();
        let tables = TableIndex::default();
        let tree = build_tree(&songs, &ScoreStore::new(), &tables, LnOption::Cn, &[]);
        assert!(tree.is_empty());
        let root = FolderPath::default();
        let choices = ChartChoices::new();
        assert!(entries_for(&tree, &root, &songs, "", &choices, &[]).is_empty());
        assert!(entries_for(&tree, &root, &songs, "air", &choices, &[]).is_empty());
        assert!(node(&tree, &FolderPath::parse("level/12")).is_none());
        assert_eq!(result_count(&tree, &root, &songs, "", &[]), 0);
        assert_eq!(result_count(&tree, &root, &songs, "air", &[]), 0);
        assert!(present_modes(&songs).is_empty());
        assert_eq!(focus_index(&[], None), 0);
    }

    #[test]
    fn the_first_run_guide_is_for_a_library_with_no_songs() {
        assert!(is_first_run(&[]));
        assert!(!is_first_run(&[song(1, "AIR", 5, PlayMode::Keys7)]));
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
        let entries = entries_for(
            &tree,
            &FolderPath::default(),
            &songs,
            "",
            &ChartChoices::new(),
            &pass_all(&songs),
        );
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

    /// A chart in `path` (a song folder) with the given title and level.
    fn chart(n: u64, path: &str, title: &str, level: u32) -> SongMetadata {
        SongMetadata {
            file_path: path.into(),
            ..song(n, title, level, PlayMode::Keys7)
        }
    }

    #[test]
    fn base_title_strips_the_trailing_bracket_only() {
        assert_eq!(base_title("AIRSHAVER [7key, Another]"), "AIRSHAVER");
        assert_eq!(base_title("AIRSHAVER [14key, Another]"), "AIRSHAVER");
        assert_eq!(base_title("Song (Remix) [SP]"), "Song (Remix)");
        assert_eq!(base_title("Plain"), "Plain");
        // Nothing left after stripping: keep the title.
        assert_eq!(base_title("[only]"), "[only]");
    }

    #[test]
    fn song_folder_is_the_directory_or_the_package_path() {
        assert_eq!(
            song_folder("songs/AIR/7k.bms").as_deref(),
            Some("songs/AIR")
        );
        assert_eq!(
            song_folder("D:\\songs\\AIR\\7k.bms").as_deref(),
            Some("D:/songs/AIR")
        );
        assert_eq!(
            song_folder("packages/p.bmsp::sub/chart.bms").as_deref(),
            Some("packages/p.bmsp::sub")
        );
        assert_eq!(
            song_folder("packages/p.bmsp::chart.bms").as_deref(),
            Some("packages/p.bmsp::")
        );
        assert_eq!(song_folder("loose.bms"), None);
    }

    #[test]
    fn charts_of_one_song_in_one_folder_share_a_group_key() {
        let a = chart(1, "songs/AIR/a.bms", "AIRSHAVER [7key, Another]", 12);
        let b = chart(2, "songs/AIR/b.bms", "AIRSHAVER [14key, Another]", 12);
        let other_folder = chart(3, "songs/OTHER/c.bms", "AIRSHAVER [7key, Another]", 12);
        let other_song = chart(4, "songs/AIR/d.bms", "AIRSHAVER 2", 12);
        assert_eq!(group_key(&a), group_key(&b));
        assert_ne!(group_key(&a), group_key(&other_folder));
        assert_ne!(group_key(&a), group_key(&other_song));
        assert_eq!(group_key(&chart(5, "loose.bms", "Loose", 1)), None);
    }

    #[test]
    fn a_group_takes_the_place_of_its_first_chart_and_sorts_by_level() {
        let songs = vec![
            chart(1, "x/AIR/a.bms", "AIR [14key]", 12),
            chart(2, "x/Other/b.bms", "Other", 3),
            chart(3, "x/AIR/c.bms", "AIR [7key]", 11),
            chart(4, "x/AIR/d.bms", "AIR [5key]", 11),
        ];
        let entries = group_entries(&songs, &[0, 1, 2, 3], &ChartChoices::new());
        // The group sits where its first chart (index 0) was; its charts run by level, then title.
        assert_eq!(
            entries,
            [
                ListEntry::Group {
                    key: group_key(&songs[0]).unwrap(),
                    title: "AIR".into(),
                    charts: vec![3, 2, 0],
                    selected: 0,
                    labels: vec!["7K 11 5key".into(), "7K 11 7key".into(), "7K 12".into()],
                },
                ListEntry::Song(1),
            ]
        );
    }

    #[test]
    fn a_remembered_chart_is_the_selected_one_and_a_loose_chart_stays_alone() {
        let songs = vec![
            chart(1, "x/AIR/a.bms", "AIR [7key]", 11),
            chart(2, "x/AIR/b.bms", "AIR [14key]", 12),
            chart(9, "loose.bms", "Loose", 5),
        ];
        let key = group_key(&songs[0]).unwrap();
        let mut choices = ChartChoices::new();
        choices.insert(key, songs[1].id);
        let entries = group_entries(&songs, &[0, 1, 2], &choices);
        assert_eq!(entries.len(), 2);
        match &entries[0] {
            ListEntry::Group {
                charts, selected, ..
            } => {
                assert_eq!(charts, &[0, 1]);
                assert_eq!(*selected, 1);
            }
            other => panic!("expected a group, got {other:?}"),
        }
        assert_eq!(entries[0].song(), Some(1));
        assert_eq!(entries[1], ListEntry::Song(2));
        // A group with one chart in this list is a plain song row.
        assert_eq!(group_entries(&songs, &[1], &choices), [ListEntry::Song(1)]);
    }

    #[test]
    fn the_leaf_and_search_rows_are_grouped_and_a_row_finds_its_chart() {
        let songs = vec![
            chart(1, "x/AIR/a.bms", "AIR [7key]", 11),
            chart(2, "x/AIR/b.bms", "AIR [14key]", 12),
            chart(3, "x/Lone/c.bms", "Lone", 4),
        ];
        let tree = build_tree(
            &songs,
            &ScoreStore::new(),
            &TableIndex::default(),
            LnOption::Cn,
            &pass_all(&songs),
        );
        let all = FolderPath::top("all");
        let rows = entries_for(
            &tree,
            &all,
            &songs,
            "",
            &ChartChoices::new(),
            &pass_all(&songs),
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(row_showing(&rows, &songs, songs[1].id), Some(0));
        assert_eq!(row_showing(&rows, &songs, songs[2].id), Some(1));
        // A search over the whole library groups the matches too.
        let found = entries_for(
            &tree,
            &FolderPath::default(),
            &songs,
            "air",
            &ChartChoices::new(),
            &pass_all(&songs),
        );
        assert_eq!(found.len(), 1);
        assert!(matches!(found[0], ListEntry::Group { .. }));
    }

    #[test]
    fn the_filter_mask_shapes_the_folders_the_leaves_and_the_search() {
        let songs = library(); // Apple 7K 12, Banana 7K 13, Cherry 14K 12, Date 5K 5, Elder 9K 9
        let pass: Vec<bool> = songs.iter().map(|s| s.play_level == 12).collect();
        let tree = build_tree(
            &songs,
            &ScoreStore::new(),
            &TableIndex::default(),
            LnOption::Cn,
            &pass,
        );

        // The 전체 곡 count and the level branch count only the passing songs.
        assert_eq!(tree[0].count, 2);
        let level = tree.iter().find(|f| f.id == "level").unwrap();
        let Body::Branch(levels) = &level.body else {
            panic!("level is a branch");
        };
        assert_eq!(
            levels.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(),
            ["12"]
        );
        assert_eq!(levels[0].count, 2);

        // The mode branch lists only the modes that still have a passing song.
        let mode = tree.iter().find(|f| f.id == "mode").unwrap();
        let Body::Branch(modes) = &mode.body else {
            panic!("mode is a branch");
        };
        assert_eq!(
            modes.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(),
            ["7k", "14k"]
        );

        // A search from the root looks only at the passing songs.
        let root = FolderPath::default();
        let found = entries_for(&tree, &root, &songs, "apple", &ChartChoices::new(), &pass);
        assert_eq!(songs_of(&found), [0]);
        assert_eq!(result_count(&tree, &root, &songs, "apple", &pass), 1);
        assert_eq!(result_count(&tree, &root, &songs, "", &pass), 2);
        let hidden = entries_for(&tree, &root, &songs, "date", &ChartChoices::new(), &pass);
        assert!(hidden.is_empty(), "a song the filter hides is not searched");
    }

    #[test]
    fn the_present_modes_follow_the_folder_order_and_the_ids_read_back() {
        let songs = library();
        assert_eq!(
            present_modes(&songs),
            [
                PlayMode::Keys5,
                PlayMode::Keys7,
                PlayMode::Keys9,
                PlayMode::Keys14
            ]
        );
        assert_eq!(mode_from_id("14k"), Some(PlayMode::Keys14));
        assert_eq!(mode_from_id("zz"), None);
    }

    /// A chart of `mode` and `level` in the song folder `x/Song`, with a subtitle.
    fn tagged(n: u64, title: &str, subtitle: &str, mode: PlayMode, level: u32) -> SongMetadata {
        SongMetadata {
            file_path: "x/Song/c.bms".into(),
            subtitle: subtitle.into(),
            ..song(n, title, level, mode)
        }
    }

    #[test]
    fn chip_labels_keep_labels_that_are_already_unique() {
        let songs = vec![
            tagged(1, "S", "", PlayMode::Keys7, 5),
            tagged(2, "S [A]", "", PlayMode::Keys7, 6),
        ];
        assert_eq!(chip_labels(&songs, &[0, 1]), ["7K 5", "7K 6"]);
    }

    #[test]
    fn a_tie_is_told_apart_by_the_bracket_of_the_charts_that_have_one() {
        let songs = vec![
            tagged(1, "ADDicTiON 4500000", "", PlayMode::Keys6, 21),
            tagged(2, "ADDicTiON 4500000 [6K UE UwU]", "", PlayMode::Keys6, 21),
        ];
        assert_eq!(chip_labels(&songs, &[0, 1]), ["6K 21", "6K 21 UwU"]);
    }

    #[test]
    fn two_brackets_that_differ_both_show_their_tag() {
        let songs = vec![
            tagged(1, "X [6K UE UwU]", "", PlayMode::Keys6, 21),
            tagged(2, "X [6K UE ABC]", "", PlayMode::Keys6, 21),
        ];
        assert_eq!(chip_labels(&songs, &[0, 1]), ["6K 21 UwU", "6K 21 ABC"]);
    }

    #[test]
    fn repeated_brackets_fall_back_to_the_subtitles() {
        let songs = vec![
            tagged(1, "X [A]", "Hard", PlayMode::Keys6, 21),
            tagged(2, "X [A]", "Ex", PlayMode::Keys6, 21),
        ];
        assert_eq!(chip_labels(&songs, &[0, 1]), ["6K 21 Hard", "6K 21 Ex"]);
    }

    #[test]
    fn a_three_way_tie_with_no_tags_gets_ordinals() {
        let songs = vec![
            tagged(1, "X", "", PlayMode::Keys6, 21),
            tagged(2, "X", "", PlayMode::Keys6, 21),
            tagged(3, "X", "", PlayMode::Keys6, 21),
        ];
        assert_eq!(
            chip_labels(&songs, &[0, 1, 2]),
            ["6K 21 #1", "6K 21 #2", "6K 21 #3"]
        );
    }

    #[test]
    fn an_empty_bracket_is_no_tag() {
        let songs = vec![
            tagged(1, "X []", "", PlayMode::Keys6, 21),
            tagged(2, "X", "", PlayMode::Keys6, 21),
        ];
        assert_eq!(chip_labels(&songs, &[0, 1]), ["6K 21 #1", "6K 21 #2"]);
    }

    #[test]
    fn a_tie_only_renames_its_own_charts() {
        let songs = vec![
            tagged(1, "X [A]", "", PlayMode::Keys6, 21),
            tagged(2, "X [B]", "", PlayMode::Keys6, 21),
            tagged(3, "X", "", PlayMode::Keys6, 9),
        ];
        assert_eq!(
            chip_labels(&songs, &[0, 1, 2]),
            ["6K 21 A", "6K 21 B", "6K 9"]
        );
    }

    #[test]
    fn short_tags_keep_a_short_word_of_a_long_text() {
        assert_eq!(short_tag("6K UE UwU").as_deref(), Some("UwU"));
        assert_eq!(short_tag("7key, Another").as_deref(), Some("Another"));
        assert_eq!(short_tag("Title").as_deref(), Some("Title"));
        assert_eq!(short_tag("- Some Title -").as_deref(), Some("Title"));
        assert_eq!(short_tag("Extraordinary").as_deref(), Some("Extraord"));
        assert_eq!(short_tag("   "), None);
    }
}

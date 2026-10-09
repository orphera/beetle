//! Songs: the charts that belong to one song, grouped from the collection index
//! (`bpm songs`, `bpm song`, `bpm charts`).
//!
//! Grouping is computed from the index when it is needed, so the index stays a
//! plain record of copies. Rules, applied in order (see the design notes in
//! `docs/plans/2026-10-09-bpm-collection-manager-design.md`, §3 and §3.1):
//!
//! 0. Links the user made with `bpm song link` always join their charts.
//! 1. Charts in one place (folder or package directory): when a title covers at least
//!    half of them, that title is the anchor and the others join only on key-sound
//!    overlap with it. Without such a title, only charts with the same title join.
//! 2. Charts in different places join when title and artist match and their
//!    key sounds overlap (`sounds_match`). Title and artist alone only make a
//!    candidate, which is shown but not merged.

use crate::collection::{Index, Location, LocationKind};
use beetle_core::ChartId;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::PathBuf;

/// The user's hand-made decisions about songs, kept apart from the index so a
/// rescan never loses them.
///
/// - `links`: groups `bpm song link` joined. Each group is one song.
/// - `splits`: pairs `bpm song unlink` separated. The two charts never join
///   again, whatever the automatic rules say.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Manual {
    pub links: Vec<Vec<ChartId>>,
    pub splits: Vec<(ChartId, ChartId)>,
}

const LINKS_HEADER: &str =
    "# bpm song link groups: one group per line, chart ids separated by spaces\n";
const SPLITS_HEADER: &str =
    "# bpm song unlink pairs: two chart ids per line, never grouped together\n";

impl Manual {
    /// Reads `collection-links.txt` and `collection-splits.txt` (or the
    /// `$BEETLE_COLLECTION_LINKS` / `$BEETLE_COLLECTION_SPLITS` overrides).
    /// A missing file means no decisions yet. Any other read error, or a line
    /// that is not a valid decision, is an error: saving after a silent skip
    /// would erase the decisions in that file.
    pub fn load() -> Result<Self, String> {
        let links = read_decision_file(&links_file())?
            .into_iter()
            .map(|line| parse_ids(&line, &links_file()).and_then(non_empty_group))
            .collect::<Result<Vec<_>, String>>()?;
        let splits = read_decision_file(&splits_file())?
            .into_iter()
            .map(|line| {
                let ids = parse_ids(&line, &splits_file())?;
                match ids.as_slice() {
                    [a, b] => Ok((*a, *b)),
                    _ => Err(format!(
                        "{}: a split line needs exactly two chart ids: '{line}'",
                        splits_file().display()
                    )),
                }
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self { links, splits })
    }

    pub fn save(&self) -> std::io::Result<()> {
        let mut links = String::from(LINKS_HEADER);
        for group in self.links.iter().filter(|g| g.len() > 1) {
            let ids: Vec<String> = group.iter().map(ChartId::to_hex).collect();
            links.push_str(&ids.join(" "));
            links.push('\n');
        }
        let mut splits = String::from(SPLITS_HEADER);
        for (a, b) in &self.splits {
            splits.push_str(&format!("{} {}\n", a.to_hex(), b.to_hex()));
        }
        // Both files are replaced atomically; each one is either the old or the new text.
        write_atomic(&splits_file(), &splits)?;
        write_atomic(&links_file(), &links)
    }
}

/// Non-empty lines of a decision file. A missing file is empty; any other error is not.
fn read_decision_file(path: &std::path::Path) -> Result<Vec<String>, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect())
}

fn parse_ids(line: &str, path: &std::path::Path) -> Result<Vec<ChartId>, String> {
    line.split_whitespace()
        .map(|token| {
            ChartId::from_hex(token)
                .ok_or_else(|| format!("{}: '{token}' is not a chart id", path.display()))
        })
        .collect()
}

fn non_empty_group(ids: Vec<ChartId>) -> Result<Vec<ChartId>, String> {
    if ids.len() > 1 {
        Ok(ids)
    } else {
        Err("a link line needs at least two chart ids".to_string())
    }
}

/// Writes through a temporary file and a rename, so a failed write never leaves a half-written file.
fn write_atomic(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    let temp = path.with_extension("tmp");
    fs::write(&temp, text)?;
    fs::rename(&temp, path)
}

/// `collection-links.txt`, or `$BEETLE_COLLECTION_LINKS`.
pub fn links_file() -> PathBuf {
    std::env::var("BEETLE_COLLECTION_LINKS")
        .map_or_else(|_| PathBuf::from("collection-links.txt"), PathBuf::from)
}

/// `collection-splits.txt`, or `$BEETLE_COLLECTION_SPLITS`.
pub fn splits_file() -> PathBuf {
    std::env::var("BEETLE_COLLECTION_SPLITS")
        .map_or_else(|_| PathBuf::from("collection-splits.txt"), PathBuf::from)
}

/// One chart as the song view sees it: its copies plus the values the grouping uses.
#[derive(Debug)]
pub struct ChartRecord<'a> {
    pub chart: ChartId,
    /// Title with chart-variant brackets removed, as shown for the song.
    pub title: String,
    pub artist: String,
    pub mode: String,
    pub play_level: u32,
    pub copies: Vec<&'a Location>,
    /// Lowercase `title`, the grouping key.
    title_key: String,
    /// Artist before any `obj` credit, lowercase; the grouping key.
    artist_key: String,
    stems: Vec<String>,
}

impl ChartRecord<'_> {
    pub fn is_packaged(&self) -> bool {
        self.copies.iter().any(|c| c.kind == LocationKind::Package)
    }
}

/// One song: the charts grouped under it.
#[derive(Debug)]
pub struct Song {
    /// Short id: the first hex characters of the smallest chart id in the song. The
    /// shortest length from 8 that keeps every song id unique is used for all songs.
    /// It changes when the song's members change.
    pub id: String,
    pub title: String,
    pub artist: String,
    /// Indexes into `Collection::records`, in display order.
    pub charts: Vec<usize>,
    /// Indexes into `Collection::songs` that have the same title and artist but
    /// do not match on key sounds. Shown for the user to confirm; never merged.
    pub candidates: Vec<usize>,
}

#[derive(Debug)]
pub struct Collection<'a> {
    pub records: Vec<ChartRecord<'a>>,
    pub songs: Vec<Song>,
}

/// Groups the index into songs using the rules above and the user's `manual` decisions.
/// A split pair is never joined, even by a manual link: `bpm song link` clears the
/// splits it contradicts before it saves.
pub fn build<'a>(index: &'a Index, manual: &Manual) -> Collection<'a> {
    let mut by_chart: BTreeMap<ChartId, Vec<&'a Location>> = BTreeMap::new();
    for location in &index.locations {
        by_chart.entry(location.chart).or_default().push(location);
    }
    let records: Vec<ChartRecord<'a>> = by_chart
        .into_iter()
        .map(|(chart, copies)| {
            let first = copies[0];
            let title = title_base(&first.title);
            ChartRecord {
                chart,
                title_key: title.to_ascii_lowercase(),
                artist: first.artist.clone(),
                artist_key: artist_key(&first.artist),
                title,
                mode: first.mode.clone(),
                play_level: first.play_level,
                stems: first.key_stems.clone(),
                copies,
            }
        })
        .collect();
    let position: BTreeMap<ChartId, usize> = records
        .iter()
        .enumerate()
        .map(|(i, r)| (r.chart, i))
        .collect();

    let mut forbidden = HashSet::new();
    for (a, b) in &manual.splits {
        if let (Some(&x), Some(&y)) = (position.get(a), position.get(b)) {
            forbidden.insert((x, y));
            forbidden.insert((y, x));
        }
    }
    let mut sets = DisjointSets::new(records.len());

    // Rule 0: manual links.
    for group in &manual.links {
        let members: Vec<usize> = group
            .iter()
            .filter_map(|id| position.get(id).copied())
            .collect();
        for pair in members.windows(2) {
            sets.try_union(pair[0], pair[1], &forbidden);
        }
    }

    // Rule 1: charts in one place (folder or package directory).
    //
    // A place with a majority title (the title covers at least half of its
    // charts) is one song: the anchor. Other charts there join only when their
    // key sounds overlap the anchor's. Without a majority, charts join only with
    // the same title.
    let mut unmatched_pairs = Vec::new();
    let mut by_place: BTreeMap<(LocationKind, &str, &str), BTreeSet<usize>> = BTreeMap::new();
    for (i, record) in records.iter().enumerate() {
        for copy in &record.copies {
            let dir = copy.path.rsplit_once('/').map_or("", |(dir, _)| dir);
            by_place
                .entry((copy.kind, copy.source.as_str(), dir))
                .or_default()
                .insert(i);
        }
    }
    for charts in by_place.values() {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for &i in charts {
            if !records[i].title_key.is_empty() {
                *counts.entry(records[i].title_key.as_str()).or_insert(0) += 1;
            }
        }
        // Most common title; ties go to the smaller title.
        let anchor = counts
            .iter()
            .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
            .map(|(key, count)| (*key, *count));
        match anchor {
            Some((key, count)) if count * 2 >= charts.len() => {
                let members: Vec<usize> = charts
                    .iter()
                    .copied()
                    .filter(|&i| records[i].title_key == key)
                    .collect();
                join_in_order(&mut sets, &members, &forbidden);
                let anchor_stems: BTreeSet<&str> = members
                    .iter()
                    .flat_map(|&i| records[i].stems.iter().map(String::as_str))
                    .collect();
                for &i in charts {
                    if records[i].title_key != key {
                        if folder_sounds_match(&records[i].stems, &anchor_stems) {
                            sets.try_union(i, members[0], &forbidden);
                        } else {
                            unmatched_pairs.push((i, members[0]));
                        }
                    }
                }
            }
            _ => {
                // No majority: a folder of several songs. Join the same title only.
                let mut by_title: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
                for &i in charts {
                    if !records[i].title_key.is_empty() {
                        by_title
                            .entry(records[i].title_key.as_str())
                            .or_default()
                            .push(i);
                    }
                }
                for members in by_title.values() {
                    join_in_order(&mut sets, members, &forbidden);
                }
            }
        }
    }
    // Rule 2: same title and artist in different places, joined only by key sounds.
    let mut by_name: BTreeMap<(&str, &str), Vec<usize>> = BTreeMap::new();
    for (i, record) in records.iter().enumerate() {
        if !record.title_key.is_empty() {
            by_name
                .entry((record.title_key.as_str(), record.artist_key.as_str()))
                .or_default()
                .push(i);
        }
    }
    for members in by_name.values() {
        for (k, &a) in members.iter().enumerate() {
            for &b in &members[k + 1..] {
                if sets.find(a) == sets.find(b) {
                    continue;
                }
                if sounds_match(&records[a].stems, &records[b].stems) {
                    sets.try_union(a, b, &forbidden);
                } else {
                    unmatched_pairs.push((a, b));
                }
            }
        }
    }

    let songs = assemble_songs(&records, &mut sets, &unmatched_pairs);
    Collection { records, songs }
}

fn assemble_songs(
    records: &[ChartRecord<'_>],
    sets: &mut DisjointSets,
    unmatched_pairs: &[(usize, usize)],
) -> Vec<Song> {
    let mut members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..records.len() {
        members.entry(sets.find(i)).or_default().push(i);
    }
    let mut songs: Vec<(usize, Song)> = members
        .into_iter()
        .map(|(root, mut charts)| {
            charts.sort_by(|&a, &b| {
                let (x, y) = (&records[a], &records[b]);
                (x.mode.as_str(), x.play_level, x.chart).cmp(&(
                    y.mode.as_str(),
                    y.play_level,
                    y.chart,
                ))
            });
            let smallest = charts
                .iter()
                .map(|&i| records[i].chart)
                .min()
                .map_or_else(String::new, |chart| chart.short());
            let title = most_common(charts.iter().map(|&i| records[i].title.as_str()));
            let artist = most_common(charts.iter().map(|&i| records[i].artist.as_str()));
            (
                root,
                Song {
                    id: smallest,
                    title,
                    artist,
                    charts,
                    candidates: Vec::new(),
                },
            )
        })
        .collect();
    songs.sort_by(|(_, a), (_, b)| {
        (
            a.title.to_ascii_lowercase(),
            a.artist.as_str(),
            a.id.as_str(),
        )
            .cmp(&(
                b.title.to_ascii_lowercase(),
                b.artist.as_str(),
                b.id.as_str(),
            ))
    });

    let song_of_root: BTreeMap<usize, usize> = songs
        .iter()
        .enumerate()
        .map(|(song_index, (root, _))| (*root, song_index))
        .collect();
    let mut result: Vec<Song> = songs.into_iter().map(|(_, song)| song).collect();
    let width = (8..=16)
        .find(|&width| {
            let mut seen = HashSet::new();
            result
                .iter()
                .all(|song| seen.insert(song.id.chars().take(width).collect::<String>()))
        })
        .unwrap_or(16);
    for song in &mut result {
        song.id = song.id.chars().take(width).collect();
    }
    for &(a, b) in unmatched_pairs {
        let (sa, sb) = (song_of_root[&sets.find(a)], song_of_root[&sets.find(b)]);
        if sa != sb {
            result[sa].candidates.push(sb);
            result[sb].candidates.push(sa);
        }
    }
    for song in &mut result {
        song.candidates.sort_unstable();
        song.candidates.dedup();
    }
    result
}

/// Whether two charts' key-sound sets say they are the same song's charts.
///
/// Jaccard of at least 0.5 with an intersection of at least 8, or containment of
/// at least 0.9 in the smaller set with at least 16 in it. A set where most names
/// are serial numbers (`01`, `001`, `0a`) cannot decide, so it never matches.
pub fn sounds_match(a: &[String], b: &[String]) -> bool {
    if a.is_empty() || b.is_empty() || mostly_serial(a) || mostly_serial(b) {
        return false;
    }
    let a: BTreeSet<&str> = a.iter().map(String::as_str).collect();
    let b: BTreeSet<&str> = b.iter().map(String::as_str).collect();
    let inter = a.intersection(&b).count();
    let union = a.len() + b.len() - inter;
    let small = a.len().min(b.len());
    (inter * 2 >= union && inter >= 8) || (inter * 10 >= small * 9 && small >= 16)
}

/// Joins each chart to the previous one in `members`, so a refused join never strands the rest.
fn join_in_order(sets: &mut DisjointSets, members: &[usize], forbidden: &HashSet<(usize, usize)>) {
    for pair in members.windows(2) {
        sets.try_union(pair[0], pair[1], forbidden);
    }
}

/// Whether a chart in a folder belongs to the folder's anchor song: at least 4 key
/// sounds in common, covering at least 80% of the chart's own key sounds. Serial
/// names count here: charts in one folder share the same files.
fn folder_sounds_match(outlier: &[String], anchor: &BTreeSet<&str>) -> bool {
    let own: BTreeSet<&str> = outlier.iter().map(String::as_str).collect();
    let common = own.iter().filter(|name| anchor.contains(*name)).count();
    common >= 4 && common * 10 >= own.len() * 8
}

fn mostly_serial(names: &[String]) -> bool {
    let serial = names.iter().filter(|n| is_serial_name(n)).count();
    serial * 2 > names.len()
}

/// A name whose last segment is all digits, or exactly two letters or digits.
fn is_serial_name(name: &str) -> bool {
    let last = name.rsplit('/').next().unwrap_or(name);
    (!last.is_empty() && last.bytes().all(|b| b.is_ascii_digit()))
        || (last.len() == 2 && last.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// Most common value; ties go to the smallest value, so the result does not depend on order.
fn most_common<'a>(values: impl Iterator<Item = &'a str>) -> String {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for value in values {
        *counts.entry(value).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(value, _)| value.to_string())
        .unwrap_or_default()
}

/// Full-width ASCII and a few punctuation look-alikes folded to their half-width forms.
pub fn fold_width(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\u{3000}' => ' ',
            '\u{ff01}'..='\u{ff5e}' => char::from_u32(c as u32 - 0xfee0).unwrap_or(c),
            '【' | '〔' => '[',
            '】' | '〕' => ']',
            '〜' => '~',
            '‐' | '–' | '—' | '−' => '-',
            other => other,
        })
        .collect()
}

/// The song title with chart-variant brackets and collapsed spaces; case kept.
pub fn title_base(raw: &str) -> String {
    let folded = fold_width(raw);
    let collapsed = folded.split_whitespace().collect::<Vec<_>>().join(" ");
    crate::pack::canonicalize_title(&collapsed)
}

/// The artist without `obj` credits, lowercase, folded.
pub fn artist_key(raw: &str) -> String {
    let folded = fold_width(raw).to_ascii_lowercase();
    let cut = [" obj", "(obj", "/obj", " / "]
        .iter()
        .filter_map(|marker| folded.find(marker))
        .min()
        .unwrap_or(folded.len());
    folded[..cut]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Union-find over chart records that also remembers each set's members, so a
/// join can be refused when it would put a forbidden pair in one song.
struct DisjointSets {
    parent: Vec<usize>,
    /// Members of each root's set; empty for non-roots.
    members: Vec<Vec<usize>>,
}

impl DisjointSets {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            members: (0..n).map(|i| vec![i]).collect(),
        }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    /// Joins the sets of `a` and `b` unless a forbidden pair would end up together.
    /// Returns whether they are in one set afterwards.
    fn try_union(&mut self, a: usize, b: usize, forbidden: &HashSet<(usize, usize)>) -> bool {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return true;
        }
        let clash = self.members[ra].iter().any(|&x| {
            self.members[rb]
                .iter()
                .any(|&y| forbidden.contains(&(x, y)))
        });
        if clash {
            return false;
        }
        // Keep the smaller index as root so results do not depend on call order.
        let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
        let moved = std::mem::take(&mut self.members[hi]);
        self.members[lo].extend(moved);
        self.parent[hi] = lo;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn artist_drops_obj_credits() {
        assert_eq!(artist_key("LeaF obj:X"), "leaf");
        assert_eq!(artist_key("ＬｅａＦ"), "leaf");
        assert_eq!(artist_key("A / B"), "a");
    }

    #[test]
    fn title_drops_variant_brackets() {
        assert_eq!(title_base("Aleph-0 [ANOTHER]"), "Aleph-0");
        assert_eq!(title_base("Snow (Remix)"), "Snow (Remix)");
    }

    #[test]
    fn sounds_match_needs_enough_shared_names() {
        let a = names(&[
            "kick", "snare", "hat", "bass1", "bass2", "bass3", "bass4", "bass5", "bass6", "bass7",
        ]);
        assert!(sounds_match(&a, &a));
        let few = names(&["kick", "snare"]);
        assert!(!sounds_match(&few, &few));
        let other = names(&["x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "y0"]);
        assert!(!sounds_match(&a, &other));
    }

    #[test]
    fn serial_names_never_decide_a_match() {
        let serial: Vec<String> = (0..40).map(|i| format!("{i:02}")).collect();
        assert!(!sounds_match(&serial, &serial));
    }

    #[test]
    fn disjoint_sets_join_and_find() {
        let none = HashSet::new();
        let mut sets = DisjointSets::new(4);
        sets.try_union(3, 1, &none);
        sets.try_union(1, 2, &none);
        assert_eq!(sets.find(3), sets.find(2));
        assert_ne!(sets.find(0), sets.find(3));
    }

    #[test]
    fn a_join_that_would_pair_split_charts_is_refused() {
        // 0 and 2 must stay apart. Joining 0-1 and then 1-2 would put them together.
        let forbidden: HashSet<(usize, usize)> = [(0, 2), (2, 0)].into_iter().collect();
        let mut sets = DisjointSets::new(3);
        assert!(sets.try_union(0, 1, &forbidden));
        assert!(!sets.try_union(1, 2, &forbidden));
        assert_ne!(sets.find(0), sets.find(2));
    }
}

#[cfg(test)]
mod place_rule_tests {
    use super::*;
    use crate::collection::Index;

    fn copy(n: u8, title: &str, path: &str, stems: &[&str]) -> Location {
        Location {
            chart: ChartId::of_bytes(&[n]),
            md5: [0; 16],
            kind: LocationKind::Folder,
            source: r"D:\bms".to_string(),
            path: path.to_string(),
            title: title.to_string(),
            artist: "A".to_string(),
            play_level: 1,
            mode: "7K".to_string(),
            missing_keys: 0,
            key_stems: stems.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn sound_names(count: usize) -> Vec<String> {
        (0..count).map(|i| format!("snd{i:02}")).collect()
    }

    fn with_stems(list: &[String]) -> Vec<&str> {
        list.iter().map(String::as_str).collect()
    }

    fn song_count(locations: Vec<Location>) -> usize {
        let index = Index {
            locations,
            ..Index::default()
        };
        build(&index, &Manual::default()).songs.len()
    }

    #[test]
    fn a_majority_title_anchors_the_folder_and_matching_variants_join() {
        let shared = sound_names(12);
        let stems = with_stems(&shared);
        let mut locations: Vec<Location> = (0..4)
            .map(|n| copy(n, "Foo", &format!("f/{n}.bme"), &stems))
            .collect();
        locations.push(copy(9, "Foo (sabun by X)", "f/v.bme", &stems));
        assert_eq!(song_count(locations), 1);
    }

    #[test]
    fn a_variant_without_shared_sounds_stays_a_separate_song() {
        let shared = sound_names(12);
        let own = ["zz1", "zz2", "zz3", "zz4", "zz5", "zz6"];
        let mut locations: Vec<Location> = (0..4)
            .map(|n| copy(n, "Foo", &format!("f/{n}.bme"), &with_stems(&shared)))
            .collect();
        locations.push(copy(9, "Foo (sabun by X)", "f/v.bme", &own));
        assert_eq!(song_count(locations), 2);
    }

    #[test]
    fn a_folder_without_a_majority_joins_only_the_same_title() {
        let shared = sound_names(12);
        let stems = with_stems(&shared);
        let locations = vec![
            copy(1, "Aa", "p/1.bme", &stems),
            copy(2, "Bb", "p/2.bme", &stems),
            copy(3, "Cc", "p/3.bme", &stems),
        ];
        assert_eq!(song_count(locations), 3);
    }

    #[test]
    fn serial_sound_names_do_not_block_a_folder_match() {
        let serial: Vec<String> = (1..=20).map(|i| format!("{i:02}")).collect();
        let stems = with_stems(&serial);
        let mut locations: Vec<Location> = (0..3)
            .map(|n| copy(n, "Foo", &format!("f/{n}.bme"), &stems))
            .collect();
        locations.push(copy(9, "Foo (sabun)", "f/v.bme", &stems));
        assert_eq!(song_count(locations), 1);
    }
}

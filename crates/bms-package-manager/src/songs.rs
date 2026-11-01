//! Songs: the charts that belong to one song, grouped from the collection index
//! (`bpm songs`, `bpm song`, `bpm charts`).
//!
//! Grouping is computed from the index when it is needed, so the index stays a
//! plain record of copies. The rule is the one the game uses,
//! `grouping::group` (docs/plans/2026-11-01-song-grouping-plan.md): every
//! chart in one place (folder or package directory) is one song, and places
//! join on the user's links, a shared chart, borrowed key sounds, or the same
//! title and artist with overlapping key sounds. Title and artist alone only
//! make a candidate, which is shown but not merged.

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

/// A copy's place: the same text for every copy in one directory of one
/// source, as the game keys places.
pub fn place_key(copy: &Location) -> String {
    let dir = copy.path.rsplit_once('/').map_or("", |(dir, _)| dir);
    format!("{:?}	{}	{dir}", copy.kind, copy.source)
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
                artist: first.artist.clone(),
                title,
                mode: first.mode.clone(),
                play_level: first.play_level,
                copies,
            }
        })
        .collect();
    // The shared rule (`grouping::group`): every place is one song, and places
    // join on links, shared charts, borrowed key sounds, or title and artist
    // with overlapping key sounds.
    let keys: Vec<Vec<String>> = records
        .iter()
        .map(|r| r.copies.iter().map(|c| place_key(c)).collect())
        .collect();
    let copies: Vec<crate::grouping::Copy> = records
        .iter()
        .zip(&keys)
        .flat_map(|(r, keys)| {
            r.copies
                .iter()
                .zip(keys)
                .map(move |(c, place)| crate::grouping::Copy {
                    place,
                    chart: r.chart,
                    title: &c.title,
                    artist: &c.artist,
                    key_stems: &c.key_stems,
                    intact: c.is_intact(),
                })
        })
        .collect();
    let grouped = crate::grouping::group(&copies, &manual.links, &manual.splits);
    // A chart is in one song, whichever of its places is asked.
    let song_of = |i: usize| grouped.song_of(&keys[i][0]);
    let mut sets = DisjointSets::new(records.len());
    let mut first_of_song: BTreeMap<usize, usize> = BTreeMap::new();
    for i in 0..records.len() {
        if let Some(song) = song_of(i) {
            let first = *first_of_song.entry(song).or_insert(i);
            sets.union(first, i);
        }
    }
    let unmatched_pairs: Vec<(usize, usize)> = grouped
        .candidates
        .iter()
        .filter_map(|(a, b)| Some((*first_of_song.get(a)?, *first_of_song.get(b)?)))
        .collect();

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

/// The charts `bpm song unlink <chart>` separates `chart` from: the others in
/// its song that share no place with it. Charts in one place are always one
/// song, so when every other chart shares a place with it, unlink is refused.
pub fn unlink_mates(collection: &Collection, chart: ChartId) -> Result<Vec<ChartId>, String> {
    let Some(record) = collection.records.iter().find(|r| r.chart == chart) else {
        return Ok(Vec::new());
    };
    let own: BTreeSet<String> = record.copies.iter().map(|c| place_key(c)).collect();
    let Some(song) = collection.songs.iter().find(|s| {
        s.charts
            .iter()
            .any(|&i| collection.records[i].chart == chart)
    }) else {
        return Ok(Vec::new());
    };
    let others: Vec<&ChartRecord> = song
        .charts
        .iter()
        .map(|&i| &collection.records[i])
        .filter(|r| r.chart != chart)
        .collect();
    let apart: Vec<ChartId> = others
        .iter()
        .filter(|r| !r.copies.iter().any(|c| own.contains(&place_key(c))))
        .map(|r| r.chart)
        .collect();
    if apart.is_empty() && !others.is_empty() {
        return Err(format!(
            "{} shares its folder with the rest of its song; charts in one folder are always one song",
            chart.short()
        ));
    }
    Ok(apart)
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

pub(crate) fn mostly_serial(names: &[String]) -> bool {
    mostly_serial_names(names.iter().map(String::as_str))
}

/// `mostly_serial` over any sequence of names, without collecting them first.
pub fn mostly_serial_names<'a>(names: impl Iterator<Item = &'a str>) -> bool {
    let (mut total, mut serial) = (0usize, 0usize);
    for name in names {
        total += 1;
        if is_serial_name(name) {
            serial += 1;
        }
    }
    serial * 2 > total
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

/// Union-find over chart records.
struct DisjointSets {
    parent: Vec<usize>,
}

impl DisjointSets {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    /// Joins the sets of `a` and `b`, keeping the smaller index as root.
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
        self.parent[hi] = lo;
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
    fn a_folder_is_one_song_whatever_its_titles_and_sounds() {
        let shared = sound_names(12);
        let own = ["zz1", "zz2", "zz3", "zz4", "zz5", "zz6"];
        let mut locations: Vec<Location> = (0..4)
            .map(|n| copy(n, "Foo", &format!("f/{n}.bme"), &with_stems(&shared)))
            .collect();
        locations.push(copy(9, "Foo (sabun by X)", "f/v.bme", &own));
        locations.push(copy(10, "Bar", "f/b.bme", &own));
        assert_eq!(song_count(locations), 1);
    }

    #[test]
    fn folders_join_on_title_artist_and_sounds_only() {
        let shared = sound_names(12);
        let stems = with_stems(&shared);
        let joined = vec![
            copy(1, "Foo", "a/1.bme", &stems),
            copy(2, "Foo", "b/2.bme", &stems),
        ];
        assert_eq!(song_count(joined), 1);
        let apart = vec![
            copy(1, "Foo", "a/1.bme", &stems),
            copy(2, "Foo", "b/2.bme", &["zz1", "zz2"]),
        ];
        assert_eq!(song_count(apart), 2);
    }

    #[test]
    fn unlink_separates_other_places_and_refuses_inside_one() {
        let shared = sound_names(12);
        let stems = with_stems(&shared);
        let index = Index {
            locations: vec![
                copy(1, "Foo", "a/1.bme", &stems),
                copy(2, "Foo", "a/2.bme", &stems),
                copy(3, "Foo", "b/3.bme", &stems),
            ],
            ..Index::default()
        };
        let collection = build(&index, &Manual::default());
        let id = |n: u8| ChartId::of_bytes(&[n]);
        assert_eq!(unlink_mates(&collection, id(1)), Ok(vec![id(3)]));
        let one_folder = Index {
            locations: index.locations[..2].to_vec(),
            ..Index::default()
        };
        assert!(unlink_mates(&build(&one_folder, &Manual::default()), id(1)).is_err());

        let manual = Manual {
            splits: vec![(id(1), id(3))],
            ..Manual::default()
        };
        assert_eq!(build(&index, &manual).songs.len(), 2);
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

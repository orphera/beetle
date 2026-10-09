//! Songs: the charts that belong to one song, grouped from the collection index
//! (`bpm songs`, `bpm song`, `bpm charts`).
//!
//! Grouping is computed from the index when it is needed, so the index stays a
//! plain record of copies. Rules, applied in order (see the design notes in
//! `docs/plans/2026-10-09-bpm-collection-manager-design.md`, §3 and §3.1):
//!
//! 0. Links the user made with `bpm song link` always join their charts.
//! 1. Charts in the same folder (or the same package directory) with the same title join.
//! 2. Charts in different places join when title and artist match and their
//!    key sounds overlap (`sounds_match`). Title and artist alone only make a
//!    candidate, which is shown but not merged.

use crate::collection::{Index, Location, LocationKind};
use beetle_core::ChartId;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

/// Where `bpm song link` keeps its manual groups: `collection-links.txt`, or
/// `$BEETLE_COLLECTION_LINKS`. One group per line, chart ids separated by spaces.
pub fn links_file() -> PathBuf {
    std::env::var("BEETLE_COLLECTION_LINKS")
        .map_or_else(|_| PathBuf::from("collection-links.txt"), PathBuf::from)
}

pub fn load_links() -> Vec<Vec<ChartId>> {
    let text = fs::read_to_string(links_file()).unwrap_or_default();
    text.lines()
        .map(|line| {
            line.split_whitespace()
                .filter_map(ChartId::from_hex)
                .collect::<Vec<_>>()
        })
        .filter(|group| group.len() > 1)
        .collect()
}

pub fn save_links(groups: &[Vec<ChartId>]) -> std::io::Result<()> {
    let mut out = String::from("# manual song groups (bpm song link/unlink); one group per line\n");
    for group in groups.iter().filter(|g| g.len() > 1) {
        let ids: Vec<String> = group.iter().map(ChartId::to_hex).collect();
        out.push_str(&ids.join(" "));
        out.push('\n');
    }
    fs::write(links_file(), out)
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
    /// Short id: the first 8 hex characters of the smallest chart id in the song.
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

/// Groups the index into songs using the rules above, with `links` as manual groups.
pub fn build<'a>(index: &'a Index, links: &[Vec<ChartId>]) -> Collection<'a> {
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

    let mut sets = DisjointSets::new(records.len());

    // Rule 0: manual links.
    for group in links {
        let members: Vec<usize> = group
            .iter()
            .filter_map(|id| position.get(id).copied())
            .collect();
        for pair in members.windows(2) {
            sets.union(pair[0], pair[1]);
        }
    }

    // Rule 1: same place (folder or package directory) and same title.
    let mut by_place: BTreeMap<(LocationKind, &str, &str), BTreeMap<&str, usize>> = BTreeMap::new();
    for (i, record) in records.iter().enumerate() {
        if record.title_key.is_empty() {
            continue;
        }
        for copy in &record.copies {
            let dir = copy.path.rsplit_once('/').map_or("", |(dir, _)| dir);
            let first_of_title = by_place
                .entry((copy.kind, copy.source.as_str(), dir))
                .or_default()
                .entry(record.title_key.as_str())
                .or_insert(i);
            sets.union(*first_of_title, i);
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
    let mut unmatched_pairs = Vec::new();
    for members in by_name.values() {
        for (k, &a) in members.iter().enumerate() {
            for &b in &members[k + 1..] {
                if sets.find(a) == sets.find(b) {
                    continue;
                }
                if sounds_match(&records[a].stems, &records[b].stems) {
                    sets.union(a, b);
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
                    id: smallest.chars().take(8).collect(),
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

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            // Keep the smaller index as root so results do not depend on call order.
            let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
            self.parent[hi] = lo;
        }
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
    fn disjoint_sets_union_and_find() {
        let mut sets = DisjointSets::new(4);
        sets.union(3, 1);
        sets.union(1, 2);
        assert_eq!(sets.find(3), sets.find(2));
        assert_ne!(sets.find(0), sets.find(3));
    }
}

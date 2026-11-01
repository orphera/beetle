//! Which charts are the same song: the one rule the game and `bpm` share
//! (docs/plans/2026-11-01-song-grouping-plan.md).
//!
//! A place is the directory a chart file sits in: a folder, or a directory
//! inside a package. Every chart in one place is the same song, whatever its
//! title. Places join in this order, each step refused when it would put an
//! unlinked pair together:
//!
//! 0. Groups the user linked (`bpm song link`).
//! 1. A chart (same bytes) in both places.
//! 2. A place whose charts miss key sounds that another place has: a
//!    difficulty chart added next to the song it was made for.
//! 3. The same title and artist with overlapping key sounds (`sounds_match`).
//!    The same title and artist alone only make a candidate, never a join.

use crate::songs::{artist_key, mostly_serial_names, sounds_match, title_base};
use beetle_core::ChartId;
use std::collections::{BTreeMap, BTreeSet};

/// One copy of a chart, as the grouping sees it.
#[derive(Debug, Clone, Copy)]
pub struct Copy<'a> {
    /// The place's key: equal for every copy in the same directory, and
    /// different for different directories (callers use kind, source and dir).
    pub place: &'a str,
    pub chart: ChartId,
    pub title: &'a str,
    pub artist: &'a str,
    /// Normalized key-sound names, as `collection::Location::key_stems`.
    pub key_stems: &'a [String],
    /// Every declared key sound was found next to this copy.
    pub intact: bool,
}

/// Why two places were joined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reason {
    Linked,
    SameChart,
    UsesSounds,
    TitleAndSounds,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Join {
    pub a: usize,
    pub b: usize,
    pub reason: Reason,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Grouping {
    /// Every place key, sorted.
    pub places: Vec<String>,
    /// The song of each place (index into `songs`).
    pub song_of_place: Vec<usize>,
    /// Each song's places, ascending. Songs are ordered by their first place.
    pub songs: Vec<Vec<usize>>,
    /// Song pairs with the same title and artist whose key sounds do not match.
    pub candidates: Vec<(usize, usize)>,
    /// The joins that took effect, in the order they were made.
    pub joins: Vec<Join>,
}

impl Grouping {
    pub fn place_index(&self, place: &str) -> Option<usize> {
        self.places.binary_search_by(|p| p.as_str().cmp(place)).ok()
    }

    pub fn song_of(&self, place: &str) -> Option<usize> {
        self.place_index(place).map(|p| self.song_of_place[p])
    }
}

/// Least share of a difficulty chart's borrowed key sounds a place must have
/// to be the song it was made for (rule 2), and the least borrowed sounds
/// for the rule to decide at all.
const USES_SOUNDS_COVERAGE: f64 = 0.9;
const USES_SOUNDS_MIN: usize = 8;

/// Groups `copies` into songs. `links` and `splits` are the user's decisions
/// by chart id; a split between two charts of one place has no effect.
pub fn group(copies: &[Copy], links: &[Vec<ChartId>], splits: &[(ChartId, ChartId)]) -> Grouping {
    let places: Vec<String> = copies
        .iter()
        .map(|c| c.place)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(str::to_string)
        .collect();
    let place_of = |c: &Copy| {
        places
            .binary_search_by(|p| p.as_str().cmp(c.place))
            .unwrap()
    };
    let mut by_chart: BTreeMap<ChartId, BTreeSet<usize>> = BTreeMap::new();
    for copy in copies {
        by_chart
            .entry(copy.chart)
            .or_default()
            .insert(place_of(copy));
    }

    let mut sets = PlaceSets::new(places.len());
    for (a, b) in splits {
        for &x in by_chart.get(a).into_iter().flatten() {
            for &y in by_chart.get(b).into_iter().flatten() {
                if x != y {
                    sets.forbid(x, y);
                }
            }
        }
    }
    let mut joins = Vec::new();
    let mut join = |sets: &mut PlaceSets, a: usize, b: usize, reason: Reason| {
        if sets.try_union(a, b) {
            joins.push(Join { a, b, reason });
        }
    };

    // 0. The user's links: every place holding one of the charts.
    for group in links {
        let members: Vec<usize> = group
            .iter()
            .filter_map(|id| by_chart.get(id))
            .flatten()
            .copied()
            .collect();
        for pair in members.windows(2) {
            join(&mut sets, pair[0], pair[1], Reason::Linked);
        }
    }

    // 1. A chart in two places.
    for members in by_chart.values() {
        let members: Vec<usize> = members.iter().copied().collect();
        for pair in members.windows(2) {
            join(&mut sets, pair[0], pair[1], Reason::SameChart);
        }
    }

    // 2. Borrowed key sounds: what a place's charts declare but the place lacks.
    let mut found: Vec<BTreeSet<&str>> = vec![BTreeSet::new(); places.len()];
    let mut lacking: Vec<BTreeSet<&str>> = vec![BTreeSet::new(); places.len()];
    for copy in copies {
        let stems = copy.key_stems.iter().map(String::as_str);
        if copy.intact {
            found[place_of(copy)].extend(stems);
        } else {
            lacking[place_of(copy)].extend(stems);
        }
    }
    for b in 0..places.len() {
        // A place with no intact copy lacks every name; one with some lacks
        // the names none of its intact copies has.
        let borrowed: BTreeSet<&str> = lacking[b].difference(&found[b]).copied().collect();
        let borrowed = if borrowed.is_empty() {
            &lacking[b]
        } else {
            &borrowed
        };
        if borrowed.len() < USES_SOUNDS_MIN || mostly_serial_names(borrowed.iter().copied()) {
            continue;
        }
        let best = (0..places.len())
            .filter(|&a| a != b && !found[a].is_empty())
            .filter_map(|a| {
                let have = borrowed.iter().filter(|s| found[a].contains(*s)).count();
                let coverage = have as f64 / borrowed.len() as f64;
                let precision = have as f64 / found[a].len() as f64;
                (coverage >= USES_SOUNDS_COVERAGE).then_some((coverage, precision, a))
            })
            // Highest coverage, then precision; the first place on a tie.
            .max_by(|x, y| {
                x.0.total_cmp(&y.0)
                    .then(x.1.total_cmp(&y.1))
                    .then(y.2.cmp(&x.2))
            });
        if let Some((_, _, a)) = best {
            join(&mut sets, a, b, Reason::UsesSounds);
        }
    }

    // 3. Same title and artist, joined only when key sounds match.
    let mut by_name: BTreeMap<(String, String), Vec<&Copy>> = BTreeMap::new();
    for copy in copies {
        let title = title_base(copy.title).to_lowercase();
        if !title.is_empty() {
            by_name
                .entry((title, artist_key(copy.artist)))
                .or_default()
                .push(copy);
        }
    }
    let mut unmatched = Vec::new();
    for members in by_name.values() {
        for (k, x) in members.iter().enumerate() {
            for y in &members[k + 1..] {
                let (px, py) = (place_of(x), place_of(y));
                if sets.find(px) == sets.find(py) {
                    continue;
                }
                if sounds_match(x.key_stems, y.key_stems) {
                    join(&mut sets, px, py, Reason::TitleAndSounds);
                } else {
                    unmatched.push((px, py));
                }
            }
        }
    }

    let mut song_of_root: BTreeMap<usize, usize> = BTreeMap::new();
    let mut songs: Vec<Vec<usize>> = Vec::new();
    let mut song_of_place = Vec::with_capacity(places.len());
    for p in 0..places.len() {
        let root = sets.find(p);
        let song = *song_of_root.entry(root).or_insert_with(|| {
            songs.push(Vec::new());
            songs.len() - 1
        });
        songs[song].push(p);
        song_of_place.push(song);
    }
    let candidates: BTreeSet<(usize, usize)> = unmatched
        .into_iter()
        .map(|(x, y)| (song_of_place[x], song_of_place[y]))
        .filter(|(a, b)| a != b)
        .map(|(a, b)| (a.min(b), a.max(b)))
        .collect();
    Grouping {
        places,
        song_of_place,
        songs,
        candidates: candidates.into_iter().collect(),
        joins,
    }
}

/// Union-find over places that refuses a join putting a forbidden pair in one set.
struct PlaceSets {
    parent: Vec<usize>,
    members: Vec<Vec<usize>>,
    forbidden: BTreeSet<(usize, usize)>,
}

impl PlaceSets {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            members: (0..n).map(|i| vec![i]).collect(),
            forbidden: BTreeSet::new(),
        }
    }

    fn forbid(&mut self, a: usize, b: usize) {
        self.forbidden.insert((a.min(b), a.max(b)));
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    /// Whether the join happened (false when already together or refused).
    fn try_union(&mut self, a: usize, b: usize) -> bool {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return false;
        }
        let clash = self.members[ra].iter().any(|&x| {
            self.members[rb]
                .iter()
                .any(|&y| self.forbidden.contains(&(x.min(y), x.max(y))))
        });
        if clash {
            return false;
        }
        // The smaller index stays root, so the result does not depend on call order.
        let (lo, hi) = (ra.min(rb), ra.max(rb));
        let moved = std::mem::take(&mut self.members[hi]);
        self.members[lo].extend(moved);
        self.parent[hi] = lo;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u64) -> ChartId {
        ChartId::synthetic(n)
    }

    /// Key sounds `prefix_0` .. `prefix_{n-1}`: names that are not serial.
    fn sounds(prefix: &str, n: usize) -> Vec<String> {
        (0..n).map(|i| format!("{prefix}_{i}")).collect()
    }

    fn copy<'a>(place: &'a str, chart: u64, title: &'a str, stems: &'a [String]) -> Copy<'a> {
        Copy {
            place,
            chart: id(chart),
            title,
            artist: "artist",
            key_stems: stems,
            intact: true,
        }
    }

    fn same_song(g: &Grouping, a: &str, b: &str) -> bool {
        g.song_of(a) == g.song_of(b)
    }

    #[test]
    fn one_place_is_one_song_whatever_the_titles() {
        let (s1, s2) = (sounds("a", 20), sounds("b", 20));
        let copies = [
            copy("F/song", 1, "Song [HYPER]", &s1),
            copy("F/song", 2, "Song (ANOTHER14)", &s1),
            copy("F/song", 3, "Something else entirely", &s2),
        ];
        let g = group(&copies, &[], &[]);
        assert_eq!(g.songs, vec![vec![0]]);
        assert!(g.joins.is_empty());
    }

    #[test]
    fn a_chart_in_two_places_joins_them() {
        let s = sounds("a", 20);
        let copies = [
            copy("F/song", 1, "Song", &s),
            copy("F/song", 2, "Song [A]", &s),
            copy("P/pkg", 1, "Song", &s),
        ];
        let g = group(&copies, &[], &[]);
        assert!(same_song(&g, "F/song", "P/pkg"));
        assert_eq!(g.joins[0].reason, Reason::SameChart);
    }

    #[test]
    fn a_difficulty_chart_joins_the_place_whose_sounds_it_borrows() {
        let base = sounds("base", 40);
        let borrowed: Vec<String> = base[..30].to_vec();
        let mut diff = copy("F/diff", 9, "Song [Insane] obj: me", &borrowed);
        diff.intact = false;
        let copies = [copy("F/base", 1, "Song", &base), diff];
        let g = group(&copies, &[], &[]);
        assert!(same_song(&g, "F/base", "F/diff"));
        assert_eq!(g.joins[0].reason, Reason::UsesSounds);
    }

    #[test]
    fn borrowing_needs_enough_names_that_are_not_serial() {
        let base = sounds("base", 40);
        let few: Vec<String> = base[..5].to_vec();
        let serial: Vec<String> = (0..30).map(|i| format!("{i:02}")).collect();
        let mut serial_base = serial.clone();
        serial_base.push("extra".into());
        for (stems, base_stems) in [(&few, &base), (&serial, &serial_base)] {
            let mut diff = copy("F/diff", 9, "Other", stems);
            diff.intact = false;
            let copies = [copy("F/base", 1, "Song", base_stems), diff];
            let g = group(&copies, &[], &[]);
            assert!(!same_song(&g, "F/base", "F/diff"));
        }
    }

    #[test]
    fn borrowing_needs_most_of_the_sounds() {
        let base = sounds("base", 40);
        let mut half: Vec<String> = base[..10].to_vec();
        half.extend(sounds("elsewhere", 10));
        let mut diff = copy("F/diff", 9, "Other", &half);
        diff.intact = false;
        let g = group(&[copy("F/base", 1, "Song", &base), diff], &[], &[]);
        assert!(!same_song(&g, "F/base", "F/diff"));
    }

    #[test]
    fn same_title_and_artist_join_on_sounds_else_stay_candidates() {
        let (s, other) = (sounds("a", 20), sounds("z", 20));
        let copies = [
            copy("F/one", 1, "Song", &s),
            copy("F/two", 2, "Song [ANOTHER]", &s),
            copy("F/three", 3, "Song", &other),
        ];
        let g = group(&copies, &[], &[]);
        assert!(same_song(&g, "F/one", "F/two"));
        assert!(!same_song(&g, "F/one", "F/three"));
        let (a, b) = (g.song_of("F/one").unwrap(), g.song_of("F/three").unwrap());
        assert_eq!(g.candidates, vec![(a.min(b), a.max(b))]);
    }

    #[test]
    fn links_join_and_splits_refuse_joins_between_places() {
        let (s, t) = (sounds("a", 20), sounds("z", 20));
        let copies = [
            copy("F/one", 1, "Song", &s),
            copy("F/two", 1, "Song", &s),
            copy("F/two", 2, "Song", &s),
            copy("F/far", 3, "Unrelated", &t),
        ];
        let g = group(&copies, &[vec![id(2), id(3)]], &[]);
        assert!(same_song(&g, "F/one", "F/far"));
        assert_eq!(g.joins[0].reason, Reason::Linked);

        // An unlink between the two places wins over the shared chart.
        let g = group(&copies, &[], &[(id(1), id(3))]);
        assert!(same_song(&g, "F/one", "F/two"));
        let g = group(&copies[..3], &[], &[(id(2), id(1))]);
        assert!(
            !same_song(&g, "F/one", "F/two"),
            "chart 1 is in both places"
        );
    }

    #[test]
    fn an_unlink_inside_one_place_changes_nothing() {
        let s = sounds("a", 20);
        let copies = [copy("F/one", 1, "A", &s), copy("F/one", 2, "B", &s)];
        let g = group(&copies, &[], &[(id(1), id(2))]);
        assert_eq!(g.songs.len(), 1);
    }

    #[test]
    fn the_result_does_not_depend_on_input_order() {
        let (s, t) = (sounds("a", 20), sounds("b", 20));
        let mut copies = vec![
            copy("F/c", 3, "Song", &s),
            copy("F/a", 1, "Song", &s),
            copy("P/b", 1, "Song", &s),
            copy("F/d", 4, "Other", &t),
        ];
        let g = group(&copies, &[], &[]);
        copies.reverse();
        let mut h = group(&copies, &[], &[]);
        h.joins.clear();
        let mut g2 = g.clone();
        g2.joins.clear();
        assert_eq!(g2, h);
        assert_eq!(g.songs, vec![vec![0, 1, 3], vec![2]]);
    }
}

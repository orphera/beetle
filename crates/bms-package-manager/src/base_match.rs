//! Estimating which owned song folder is the original (本体) of a difference
//! chart (差分) whose hashes cannot match it: the difference is an edited copy,
//! and difficulty tables name no original. See
//! docs/plans/2026-10-09-diff-base-matching-design.md.
//!
//! Two stages. Metadata alone (before the difference is downloaded) gives weak
//! candidates only. Key sounds (once the difference is on disk) give the
//! candidates that count. Nothing here links charts; the caller decides.
//!
//! The functions take plain sets so they stay free of file system and index
//! access. [`places_from_index`] is the one adapter from the collection index.

use crate::collection::{normalize_stem, Index, Location};
use crate::songs::{artist_key, mostly_serial_names, title_base};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

/// Share of the difference's required sounds a folder must have, in percent.
pub const COVERAGE_MIN_PERCENT: usize = 80;
/// Fewest shared sounds that count as key evidence at all.
pub const SHARED_MIN: usize = 2;
/// Fewest shared sounds for a confident candidate (as `folder_sounds_match`).
pub const CONFIDENT_SHARED_MIN: usize = 4;
/// Lowest precision (share of the folder's sounds the difference uses), in percent, for a confident candidate.
pub const CONFIDENT_PRECISION_MIN_PERCENT: usize = 50;
/// Title similarity (bigram Dice) at which a title counts as strongly similar.
pub const TITLE_STRONG: f64 = 0.88;
/// Title similarity at which a title counts as weakly similar.
pub const TITLE_WEAK: f64 = 0.72;
/// How many candidates a verdict lists.
pub const TOP_LIMIT: usize = 3;

/// How well a folder's metadata agrees with the difference's. Never a link by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Evidence {
    None,
    Weak,
    Strong,
}

/// A local song folder: its key sounds and the title and artist of its charts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Place {
    /// Opaque identifier shown to the user, e.g. `folder:<source>:<dir>`.
    pub id: String,
    /// Key sounds (normalized stems) of the intact charts in this place.
    pub keys: BTreeSet<String>,
    /// Comparison form (see `title_key`, `artist_key`) of the (title, artist) of every chart in this place, intact or not.
    charts: Vec<(String, String)>,
}

/// A place that shares key sounds with the difference.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub place: String,
    /// Sounds the difference requires that the place has.
    pub shared: usize,
    /// `shared` over the difference's required sounds.
    pub coverage: f64,
    /// `shared` over the place's sounds.
    pub precision: f64,
    pub jaccard: f64,
    pub evidence: Evidence,
}

/// A place judged on metadata alone. Its evidence is at most `Weak`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataCandidate {
    pub place: String,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// One place, with enough shared sounds, precision, and agreeing metadata, clearly ahead.
    Confident(Candidate),
    /// Key candidates exist, but none is confident. Best first, at most [`TOP_LIMIT`].
    Ranked(Vec<Candidate>),
    /// The difference was checked by key sounds and no place passed. The list holds
    /// the places that were skipped for serial names but agree on metadata.
    NoKeyMatch(Vec<MetadataCandidate>),
    /// Key sounds cannot decide (none required, or mostly serial names). Metadata only, possibly empty.
    MetadataOnly(Vec<MetadataCandidate>),
}

/// The difference's own title and artist, from the difficulty table entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryMeta<'a> {
    pub title: &'a str,
    pub artist: &'a str,
}

/// The sounds the original must supply: the difference's declared sounds, normalized,
/// minus the ones its own pack brings (`bundled`, normalized stems of its files).
pub fn required_stems(declared: &[String], bundled: &BTreeSet<String>) -> BTreeSet<String> {
    declared
        .iter()
        .map(|name| normalize_stem(name))
        .filter(|stem| !stem.is_empty() && !bundled.contains(stem))
        .collect()
}

/// Bigram Dice coefficient of two strings, 0.0 to 1.0. An empty string, or a
/// string shorter than two characters, is similar to nothing (not even itself).
pub fn title_similarity(left: &str, right: &str) -> f64 {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    if left == right {
        return 1.0;
    }
    if left.len() < 2 || right.len() < 2 {
        return 0.0;
    }
    let left_bigrams = bigram_counts(&left);
    let right_bigrams = bigram_counts(&right);
    let overlap: usize = left_bigrams
        .iter()
        .map(|(pair, &count)| count.min(right_bigrams.get(pair).copied().unwrap_or(0)))
        .sum();
    let total = left.len() - 1 + right.len() - 1;
    (2 * overlap) as f64 / total as f64
}

fn bigram_counts(chars: &[char]) -> BTreeMap<(char, char), usize> {
    let mut counts = BTreeMap::new();
    for pair in chars.windows(2) {
        *counts.entry((pair[0], pair[1])).or_insert(0) += 1;
    }
    counts
}

/// Metadata agreement between the difference and one chart.
///
/// Strong: artist matches and the title matches exactly or strongly.
/// Weak: the title matches exactly, or the artist does, or the title is similar.
pub fn evidence(entry: EntryMeta, chart_title: &str, chart_artist: &str) -> Evidence {
    evidence_keys(
        &normalized_entry(entry),
        &title_key(chart_title),
        &artist_key(chart_artist),
    )
}

/// The difference's title and artist in comparison form, computed once per estimate.
struct NormalizedEntry {
    title: String,
    artist: String,
}

fn normalized_entry(entry: EntryMeta) -> NormalizedEntry {
    NormalizedEntry {
        title: title_key(entry.title),
        artist: artist_key(entry.artist),
    }
}

/// `evidence` with both sides already in comparison form.
fn evidence_keys(entry: &NormalizedEntry, chart_title: &str, chart_artist: &str) -> Evidence {
    let title_exact = !entry.title.is_empty() && entry.title == chart_title;
    let artist_exact = !entry.artist.is_empty() && entry.artist == chart_artist;
    let similarity = title_similarity(&entry.title, chart_title);

    if artist_exact && (title_exact || similarity >= TITLE_STRONG) {
        Evidence::Strong
    } else if title_exact || artist_exact || similarity >= TITLE_WEAK {
        Evidence::Weak
    } else {
        Evidence::None
    }
}

/// The comparison form of a title: the song title without variant brackets, lowercased.
fn title_key(title: &str) -> String {
    title_base(title).to_ascii_lowercase()
}

/// Best evidence any chart of a place gives. The place's charts are already in comparison form.
fn place_evidence(entry: &NormalizedEntry, place: &Place) -> Evidence {
    place
        .charts
        .iter()
        .map(|(title, artist)| evidence_keys(entry, title, artist))
        .max()
        .unwrap_or(Evidence::None)
}

/// Whether a place's key names are mostly serial numbers, so they cannot decide.
fn is_serial_place(place: &Place) -> bool {
    mostly_serial_names(place.keys.iter().map(String::as_str))
}

/// Metadata candidates from places whose evidence is not `None`, best first, at
/// most [`TOP_LIMIT`]. Sorted by the real evidence and cut before the cap, so a
/// strong match is never pushed out by weaker ones. Only the output is capped to
/// `Weak`: metadata alone is never strong.
fn metadata_list<'a>(
    entry: &NormalizedEntry,
    places: impl Iterator<Item = &'a Place>,
) -> Vec<MetadataCandidate> {
    let mut found: Vec<(Evidence, &str)> = places
        .filter_map(|place| {
            let evidence = place_evidence(entry, place);
            (evidence != Evidence::None).then_some((evidence, place.id.as_str()))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    found.truncate(TOP_LIMIT);
    found
        .into_iter()
        .map(|(evidence, place)| MetadataCandidate {
            place: place.to_string(),
            evidence: evidence.min(Evidence::Weak),
        })
        .collect()
}

/// The shared sound count when a place passes the cheap key filters (enough shared
/// sounds, enough coverage), before the serial check. `None` otherwise.
fn shared_if_passing(required: &BTreeSet<String>, place: &Place) -> Option<usize> {
    if place.keys.is_empty() {
        return None;
    }
    let shared = required.intersection(&place.keys).count();
    let passes = shared >= SHARED_MIN && shared * 100 >= required.len() * COVERAGE_MIN_PERCENT;
    passes.then_some(shared)
}

/// Stage A: places whose charts agree with the difference's title and artist.
pub fn metadata_candidates(entry: EntryMeta, places: &[Place]) -> Vec<MetadataCandidate> {
    metadata_list(&normalized_entry(entry), places.iter())
}

/// Stage B: places that pass the key-sound filters, best first.
///
/// `required` is the output of [`required_stems`]. A place with mostly serial
/// names gives no key evidence and is skipped.
pub fn key_candidates(
    entry: EntryMeta,
    required: &BTreeSet<String>,
    places: &[Place],
) -> Vec<Candidate> {
    key_candidates_in(
        &normalized_entry(entry),
        required,
        &places.iter().collect::<Vec<_>>(),
    )
}

/// `key_candidates` over places given by reference, so callers can leave some out
/// without copying their keys.
fn key_candidates_in(
    entry: &NormalizedEntry,
    required: &BTreeSet<String>,
    places: &[&Place],
) -> Vec<Candidate> {
    let mut out = Vec::new();
    if required.is_empty() {
        return out;
    }
    for &place in places {
        // Cheap filters first; the serial check runs only on places that survive them.
        let Some(shared) = shared_if_passing(required, place) else {
            continue;
        };
        if is_serial_place(place) {
            continue;
        }
        let union = required.len() + place.keys.len() - shared;
        out.push(Candidate {
            place: place.id.clone(),
            shared,
            coverage: shared as f64 / required.len() as f64,
            precision: shared as f64 / place.keys.len() as f64,
            jaccard: shared as f64 / union as f64,
            evidence: place_evidence(entry, place),
        });
    }
    // Ordered by coverage, then precision. Confidence is decided only for the top
    // place (see `estimate`), so the order must not depend on it.
    out.sort_by(|a, b| {
        descending(a.coverage, b.coverage)
            .then_with(|| descending(a.precision, b.precision))
            .then_with(|| b.evidence.cmp(&a.evidence))
            .then_with(|| descending(a.jaccard, b.jaccard))
            .then_with(|| a.place.cmp(&b.place))
    });
    out
}

/// Descending order for ratios. Ratios here are never NaN, so ties are the only equal case.
fn descending(a: f64, b: f64) -> Ordering {
    b.partial_cmp(&a).unwrap_or(Ordering::Equal)
}

/// Whether a candidate passes the bar for a confident verdict, before uniqueness is checked.
fn meets_confident_bar(candidate: &Candidate) -> bool {
    candidate.shared >= CONFIDENT_SHARED_MIN
        && candidate.precision >= CONFIDENT_PRECISION_MIN_PERCENT as f64 / 100.0
        && candidate.evidence != Evidence::None
}

/// The whole estimate: key sounds when they can decide, metadata otherwise.
///
/// Callers pass an empty `required` set when the difference's sounds could not be
/// read (parse failure), so this falls back to metadata.
pub fn estimate(entry: EntryMeta, required: &BTreeSet<String>, places: &[Place]) -> Verdict {
    estimate_excluding(entry, required, places, &BTreeSet::new())
}

/// `estimate`, leaving out the places whose ids are in `excluded` (for example the
/// places holding the difference itself).
pub fn estimate_excluding(
    entry: EntryMeta,
    required: &BTreeSet<String>,
    places: &[Place],
    excluded: &BTreeSet<String>,
) -> Verdict {
    let active: Vec<&Place> = places
        .iter()
        .filter(|place| !excluded.contains(&place.id))
        .collect();
    estimate_in(entry, required, &active)
}

fn estimate_in(entry: EntryMeta, required: &BTreeSet<String>, places: &[&Place]) -> Verdict {
    let entry = &normalized_entry(entry);
    if required.is_empty() || mostly_serial_names(required.iter().map(String::as_str)) {
        return Verdict::MetadataOnly(metadata_list(entry, places.iter().copied()));
    }
    let candidates = key_candidates_in(entry, required, places);
    let Some(top) = candidates.first() else {
        // Places that passed the key filters but were skipped for serial names: the
        // only ones that would have been key candidates, so the only ones to show by metadata.
        let skipped_serial = places
            .iter()
            .copied()
            .filter(|place| shared_if_passing(required, place).is_some() && is_serial_place(place));
        return Verdict::NoKeyMatch(metadata_list(entry, skipped_serial));
    };
    // The top place must be at least as good as every other candidate on both
    // coverage and precision, and strictly better than the runner-up on one of them.
    // Otherwise the choice is a tie or a trade-off, and no confident verdict is given.
    let unique = candidates
        .get(1)
        .is_none_or(|second| top.coverage > second.coverage || top.precision > second.precision)
        && candidates
            .iter()
            .all(|other| top.coverage >= other.coverage && top.precision >= other.precision);
    if unique && meets_confident_bar(top) {
        Verdict::Confident(top.clone())
    } else {
        Verdict::Ranked(candidates.into_iter().take(TOP_LIMIT).collect())
    }
}

/// The place a copy belongs to: its kind, source, and the directory part of its path.
pub fn place_id(location: &Location) -> String {
    let dir = location.path.rsplit_once('/').map_or("", |(dir, _)| dir);
    format!("{}:{}:{}", location.kind.as_str(), location.source, dir)
}

/// Builds the places from a collection index. A place is one directory inside a
/// folder or a package, as `songs.rs` groups them (the part of the path before the
/// last `/`). Its keys come from intact copies only; its titles come from all copies.
pub fn places_from_index(index: &Index) -> Vec<Place> {
    let mut by_id: BTreeMap<String, Place> = BTreeMap::new();
    for location in &index.locations {
        let id = place_id(location);
        let place = by_id.entry(id.clone()).or_insert_with(|| Place {
            id,
            ..Place::default()
        });
        place
            .charts
            .push((title_key(&location.title), artist_key(&location.artist)));
        if location.is_intact() {
            place.keys.extend(location.key_stems.iter().cloned());
        }
    }
    by_id.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{Location, LocationKind};
    use beetle_core::ChartId;

    fn set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    fn place(id: &str, keys: &[&str], charts: &[(&str, &str)]) -> Place {
        Place {
            id: id.to_string(),
            keys: set(keys),
            charts: charts
                .iter()
                .map(|(t, a)| (title_key(t), artist_key(a)))
                .collect(),
        }
    }

    const ENTRY: EntryMeta = EntryMeta {
        title: "Song A",
        artist: "Artist",
    };

    fn meta(place: &str, evidence: Evidence) -> MetadataCandidate {
        MetadataCandidate {
            place: place.to_string(),
            evidence,
        }
    }

    #[test]
    fn required_stems_normalize_and_drop_bundled() {
        let declared = vec![
            "Sub\\Kick.WAV".to_string(),
            "snare.ogg".to_string(),
            "bundled.wav".to_string(),
        ];
        let bundled = set(&["bundled"]);
        assert_eq!(
            required_stems(&declared, &bundled),
            set(&["sub/kick", "snare"])
        );
    }

    #[test]
    fn similarity_bounds() {
        assert_eq!(title_similarity("abc", "abc"), 1.0);
        assert_eq!(title_similarity("a", "ab"), 0.0);
        assert_eq!(title_similarity("", "ab"), 0.0);
        // An empty title is not similar to an empty title either.
        assert_eq!(title_similarity("", ""), 0.0);
    }

    #[test]
    fn similarity_of_near_titles_is_between_the_bounds() {
        // Bigrams: 9 and 9, 7 shared, so 2*7/18.
        let s = title_similarity("abcdefghij", "abcdefghxy");
        assert!((s - 14.0 / 18.0).abs() < 1e-12, "{s}");
        assert!(s > TITLE_WEAK && s < TITLE_STRONG, "{s}");
    }

    #[test]
    fn evidence_grades() {
        assert_eq!(evidence(ENTRY, "Song A", "Artist"), Evidence::Strong);
        assert_eq!(evidence(ENTRY, "Song A", "Someone"), Evidence::Weak);
        assert_eq!(evidence(ENTRY, "Other", "Artist"), Evidence::Weak);
        assert_eq!(evidence(ENTRY, "Other", "Someone"), Evidence::None);
    }

    #[test]
    fn strongly_similar_title_with_another_artist_is_weak_not_none() {
        // Similarity above the strong bar, but the artist differs: still worth showing.
        let entry = EntryMeta {
            title: "starlight",
            artist: "x",
        };
        assert_eq!(evidence(entry, "starlight!", "x feat. y"), Evidence::Weak);
    }

    #[test]
    fn fuzzy_title_with_the_same_artist_is_strong() {
        let entry = EntryMeta {
            title: "starlight",
            artist: "x",
        };
        assert_eq!(evidence(entry, "starlight!", "x"), Evidence::Strong);
    }

    #[test]
    fn blank_titles_do_not_match_each_other() {
        let blank = EntryMeta {
            title: "",
            artist: "Artist",
        };
        // Same artist, no title on either side: weak at most, never strong.
        assert_eq!(evidence(blank, "", "Artist"), Evidence::Weak);
    }

    #[test]
    fn metadata_only_is_capped_at_weak() {
        let places = vec![place("a", &["kick"], &[("Song A", "Artist")])];
        assert_eq!(
            estimate(ENTRY, &BTreeSet::new(), &places),
            Verdict::MetadataOnly(vec![meta("a", Evidence::Weak)])
        );
    }

    #[test]
    fn mostly_serial_required_sounds_fall_back_to_metadata() {
        let required = set(&["01", "02", "03", "kick"]);
        let places = vec![place(
            "a",
            &["01", "02", "03", "kick"],
            &[("Song A", "Artist")],
        )];
        assert_eq!(
            estimate(ENTRY, &required, &places),
            Verdict::MetadataOnly(vec![meta("a", Evidence::Weak)])
        );
    }

    #[test]
    fn mostly_serial_place_gives_no_key_evidence_and_is_listed_by_metadata() {
        let required = set(&["kick", "snare", "hat", "bass"]);
        let places = vec![place(
            "serial",
            &["01", "02", "03", "04", "05", "kick", "snare", "hat", "bass"],
            &[("Song A", "Artist")],
        )];
        assert_eq!(key_candidates(ENTRY, &required, &places), Vec::new());
        assert_eq!(
            estimate(ENTRY, &required, &places),
            Verdict::NoKeyMatch(vec![meta("serial", Evidence::Weak)])
        );
    }

    #[test]
    fn a_strong_original_is_not_pushed_out_by_weak_places_with_smaller_ids() {
        // Three artist-only matches sort before "z-orig" by id, but z-orig is strong.
        let places = vec![
            place("a1", &["x"], &[("Other", "Artist")]),
            place("a2", &["x"], &[("Other", "Artist")]),
            place("a3", &["x"], &[("Other", "Artist")]),
            place("z-orig", &["x"], &[("Song A", "Artist")]),
        ];
        let list = metadata_candidates(ENTRY, &places);
        assert_eq!(list.len(), TOP_LIMIT);
        assert_eq!(list[0], meta("z-orig", Evidence::Weak));
        assert!(list.iter().all(|c| c.evidence == Evidence::Weak));
    }

    #[test]
    fn serial_places_without_key_overlap_are_not_listed_as_skipped() {
        let required = set(&["kick", "snare", "hat", "bass"]);
        let places = vec![place(
            "serial-unrelated",
            &["01", "02", "03", "04", "05"],
            &[("Song A", "Artist")],
        )];
        assert_eq!(
            estimate(ENTRY, &required, &places),
            Verdict::NoKeyMatch(vec![])
        );
    }

    #[test]
    fn confident_needs_the_best_precision_among_all_candidates_not_just_the_runner_up() {
        let required = set(&["kick", "snare", "hat", "bass"]);
        // Top covers all four but its folder is half other sounds (precision 0.5).
        let top = place(
            "top",
            &["kick", "snare", "hat", "bass", "x1", "x2", "x3", "x4"],
            &[("Song A", "Artist")],
        );
        // Runner-up covers four of four as well, with less folder noise, so it is not beaten by top on coverage.
        let tight = place(
            "tight",
            &["kick", "snare", "hat", "bass"],
            &[("Song A", "Artist")],
        );
        // A third candidate with the best precision (1.0) but less coverage: top must not stay confident.
        let third = place(
            "third",
            &["kick", "snare", "hat", "bass"],
            &[("Song A", "Artist")],
        );
        let third = Place {
            id: "zz-third".into(),
            ..third
        };
        let trio = [top, tight, third];
        let verdict = estimate(ENTRY, &required, &trio);
        assert!(!matches!(verdict, Verdict::Confident(_)), "{verdict:?}");
    }

    #[test]
    fn intact_original_is_confident() {
        let required = set(&["kick", "snare", "hat", "bass", "lead"]);
        let places = vec![
            place(
                "original",
                &["kick", "snare", "hat", "bass", "lead", "pad"],
                &[("Song A", "Artist")],
            ),
            place("unrelated", &["piano", "drum"], &[("Other", "Someone")]),
        ];
        match estimate(ENTRY, &required, &places) {
            Verdict::Confident(candidate) => {
                assert_eq!(candidate.place, "original");
                assert_eq!(candidate.shared, 5);
                assert_eq!(candidate.coverage, 1.0);
                assert_eq!(candidate.precision, 5.0 / 6.0);
                assert_eq!(candidate.jaccard, 5.0 / 6.0);
            }
            other => panic!("expected confident, got {other:?}"),
        }
    }

    #[test]
    fn jaccard_counts_the_union() {
        let required = set(&["a", "b", "c", "d"]);
        let places = vec![place(
            "p",
            &["a", "b", "c", "d", "e", "f"],
            &[("Other", "Someone")],
        )];
        let candidates = key_candidates(ENTRY, &required, &places);
        assert_eq!(candidates[0].shared, 4);
        assert_eq!(candidates[0].jaccard, 4.0 / 6.0);
    }

    #[test]
    fn too_few_shared_sounds_are_rejected() {
        let required = set(&["kick"]);
        let places = vec![place("a", &["kick", "snare"], &[("Song A", "Artist")])];
        assert_eq!(
            estimate(ENTRY, &required, &places),
            Verdict::NoKeyMatch(vec![])
        );
    }

    #[test]
    fn low_coverage_is_rejected() {
        let required = set(&["a", "b", "c", "d", "e"]);
        let places = vec![place("partial", &["a", "b", "c"], &[("Song A", "Artist")])];
        assert_eq!(
            estimate(ENTRY, &required, &places),
            Verdict::NoKeyMatch(vec![])
        );
    }

    #[test]
    fn sub_folder_key_is_not_the_same_as_the_bare_name() {
        let required = set(&["sub/kick"]);
        let places = vec![place("a", &["kick", "snare"], &[("Song A", "Artist")])];
        assert!(key_candidates(ENTRY, &required, &places).is_empty());
    }

    #[test]
    fn large_mixed_folder_is_ranked_not_confident() {
        let required = set(&["kick", "snare", "hat", "bass"]);
        let mut keys: Vec<String> = (0..40).map(|i| format!("other{i}")).collect();
        keys.extend(required.iter().cloned());
        let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
        let places = vec![place("mixed", &key_refs, &[("Song A", "Artist")])];
        match estimate(ENTRY, &required, &places) {
            Verdict::Ranked(list) => {
                assert_eq!(list.len(), 1);
                assert_eq!(list[0].place, "mixed");
                assert_eq!(list[0].coverage, 1.0);
                assert!(list[0].precision < 0.5);
            }
            other => panic!("expected ranked, got {other:?}"),
        }
    }

    #[test]
    fn a_tie_is_ranked_in_place_order() {
        let required = set(&["kick", "snare", "hat", "bass"]);
        let keys = &["kick", "snare", "hat", "bass"];
        let places = vec![
            place("copy-b", keys, &[("Song A", "Artist")]),
            place("copy-a", keys, &[("Song A", "Artist")]),
        ];
        match estimate(ENTRY, &required, &places) {
            Verdict::Ranked(list) => {
                let ids: Vec<&str> = list.iter().map(|c| c.place.as_str()).collect();
                assert_eq!(ids, vec!["copy-a", "copy-b"]);
            }
            other => panic!("expected ranked, got {other:?}"),
        }
    }

    #[test]
    fn metadata_mismatch_is_not_confident() {
        let required = set(&["kick", "snare", "hat", "bass"]);
        let places = vec![place(
            "a",
            &["kick", "snare", "hat", "bass"],
            &[("Totally Other", "Nobody")],
        )];
        match estimate(ENTRY, &required, &places) {
            Verdict::Ranked(list) => assert_eq!(list[0].evidence, Evidence::None),
            other => panic!("expected ranked, got {other:?}"),
        }
    }

    #[test]
    fn a_strong_but_less_covering_place_does_not_beat_a_better_covering_one() {
        let required = set(&["kick", "snare", "hat", "bass", "lead"]);
        let places = vec![
            place(
                "covers-more",
                &["kick", "snare", "hat", "bass", "lead"],
                &[("Other", "Someone")],
            ),
            place(
                "strong-meta",
                &["kick", "snare", "hat", "bass"],
                &[("Song A", "Artist")],
            ),
        ];
        match estimate(ENTRY, &required, &places) {
            Verdict::Ranked(list) => {
                assert_eq!(list[0].place, "covers-more");
                assert_eq!(list[1].place, "strong-meta");
            }
            other => panic!("expected ranked, got {other:?}"),
        }
    }

    #[test]
    fn a_clear_best_place_is_confident_even_with_ties_below_it() {
        // "good" covers all five sounds with agreeing metadata; the pair below it ties.
        // The order of the input must not change the verdict.
        let required = set(&["kick", "snare", "hat", "bass", "lead"]);
        let good = place(
            "good",
            &["kick", "snare", "hat", "bass", "lead"],
            &[("Song A", "Artist")],
        );
        let tie_a = place(
            "tie-a",
            &["kick", "snare", "hat", "bass"],
            &[("Other", "Someone")],
        );
        let tie_b = place(
            "tie-b",
            &["kick", "snare", "hat", "bass"],
            &[("Other", "Someone")],
        );
        let forward = estimate(
            ENTRY,
            &required,
            &[good.clone(), tie_a.clone(), tie_b.clone()],
        );
        let reversed = estimate(ENTRY, &required, &[tie_b, tie_a, good]);
        assert_eq!(forward, reversed);
        match forward {
            Verdict::Confident(candidate) => assert_eq!(candidate.place, "good"),
            other => panic!("expected confident, got {other:?}"),
        }
    }

    #[test]
    fn ranked_list_is_exact_and_ordered_regardless_of_input_order() {
        // Two places tie at the top (full coverage, same precision); a third covers 4 of 5.
        let required = set(&["kick", "snare", "hat", "bass", "lead"]);
        let top_b = place(
            "top-b",
            &["kick", "snare", "hat", "bass", "lead", "x"],
            &[("Other", "Someone")],
        );
        let top_a = place(
            "top-a",
            &["kick", "snare", "hat", "bass", "lead", "x"],
            &[("Other", "Someone")],
        );
        let low = place(
            "low",
            &["kick", "snare", "hat", "bass", "y"],
            &[("Other", "Someone")],
        );
        let forward = estimate(
            ENTRY,
            &required,
            &[top_b.clone(), low.clone(), top_a.clone()],
        );
        let reversed = estimate(ENTRY, &required, &[top_a, low, top_b]);
        assert_eq!(forward, reversed);
        match forward {
            Verdict::Ranked(list) => {
                let ids: Vec<&str> = list.iter().map(|c| c.place.as_str()).collect();
                assert_eq!(ids, vec!["top-a", "top-b", "low"]);
            }
            other => panic!("expected ranked, got {other:?}"),
        }
    }

    fn location(
        kind: LocationKind,
        source: &str,
        path: &str,
        title: &str,
        missing: u32,
        keys: &[&str],
    ) -> Location {
        Location {
            chart: ChartId::of_bytes(format!("{source}{path}").as_bytes()),
            md5: [0; 16],
            kind,
            source: source.to_string(),
            path: path.to_string(),
            title: title.to_string(),
            artist: "Artist".to_string(),
            play_level: 1,
            mode: "7k".to_string(),
            missing_keys: missing,
            key_stems: keys.iter().map(|k| k.to_string()).collect(),
        }
    }

    #[test]
    fn places_from_index_split_by_directory_and_use_intact_keys() {
        let index = Index {
            scanned_at: 0,
            skipped: 0,
            locations: vec![
                location(
                    LocationKind::Folder,
                    "D:\\songs",
                    "a/x.bme",
                    "Song A",
                    0,
                    &["kick"],
                ),
                location(
                    LocationKind::Folder,
                    "D:\\songs",
                    "a/y.bme",
                    "Song A",
                    1,
                    &["snare"],
                ),
                location(
                    LocationKind::Folder,
                    "D:\\songs",
                    "b/z.bme",
                    "Song B",
                    0,
                    &["hat"],
                ),
                location(
                    LocationKind::Package,
                    "pkg@1",
                    "sub/w.bme",
                    "Song C",
                    0,
                    &["bass"],
                ),
                location(
                    LocationKind::Package,
                    "pkg@1",
                    "root.bme",
                    "Song D",
                    0,
                    &["lead"],
                ),
            ],
        };
        let places = places_from_index(&index);
        let ids: Vec<&str> = places.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "folder:D:\\songs:a",
                "folder:D:\\songs:b",
                "package:pkg@1:",
                "package:pkg@1:sub",
            ]
        );
        assert_eq!(places[0].keys, set(&["kick"]));
        assert_eq!(places[0].charts.len(), 2);
        assert_eq!(places[1].keys, set(&["hat"]));
    }
}

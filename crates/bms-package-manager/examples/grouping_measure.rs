//! B17 measurement: runs the old game rule, the old bpm rule and the planned
//! rule (docs/plans/2026-11-01-song-grouping-plan.md) over `collection.idx`
//! and prints where they differ. Run from the working directory:
//! `cargo run -p bms-package-manager --example grouping_measure`.

use beetle_core::ChartId;
use bms_package_manager::collection::{index_file, Index, Location, LocationKind};
use bms_package_manager::grouping;
use bms_package_manager::songs::{self, artist_key, sounds_match, title_base, Manual};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

type Place = (LocationKind, String, String);

fn place_of(loc: &Location) -> Place {
    let dir = loc.path.rsplit_once('/').map_or("", |(d, _)| d);
    (loc.kind, loc.source.clone(), dir.to_string())
}

fn place_name(p: &Place) -> String {
    match p.0 {
        LocationKind::Folder => format!("{}/{}", p.1, p.2),
        LocationKind::Package => format!("[pkg] {}:{}", p.1.split('@').next().unwrap_or(""), p.2),
    }
}

struct Dsu(Vec<usize>);
impl Dsu {
    fn find(&mut self, x: usize) -> usize {
        let mut r = x;
        while self.0[r] != r {
            r = self.0[r];
        }
        let mut x = x;
        while self.0[x] != r {
            let n = self.0[x];
            self.0[x] = r;
            x = n;
        }
        r
    }
    fn union(&mut self, a: usize, b: usize) -> bool {
        let (a, b) = (self.find(a), self.find(b));
        if a == b {
            return false;
        }
        self.0[a.max(b)] = a.min(b);
        true
    }
}

fn mostly_serial(names: &BTreeSet<&str>) -> bool {
    let serial = names
        .iter()
        .filter(|n| {
            let last = n.rsplit('/').next().unwrap_or(n);
            (!last.is_empty() && last.bytes().all(|b| b.is_ascii_digit()))
                || (last.len() == 2 && last.bytes().all(|b| b.is_ascii_alphanumeric()))
        })
        .count();
    serial * 2 > names.len()
}

fn title_stem(title: &str) -> String {
    title_base(title)
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .take(4)
        .collect()
}

#[derive(Default)]
struct PlaceInfo {
    locs: Vec<usize>,
    charts: BTreeSet<ChartId>,
    /// Union of key stems of the place's intact copies.
    intact_stems: BTreeSet<String>,
    /// Union of key stems of copies that miss key sounds.
    missing_stems: BTreeSet<String>,
}

fn main() {
    let index = Index::parse(&fs::read_to_string(index_file()).expect("collection.idx"))
        .expect("index parses");
    let manual = Manual::load().unwrap_or_default();
    let locs = &index.locations;

    let mut place_ids: BTreeMap<Place, usize> = BTreeMap::new();
    let mut places: Vec<(Place, PlaceInfo)> = Vec::new();
    for (i, loc) in locs.iter().enumerate() {
        let p = place_of(loc);
        let id = *place_ids.entry(p.clone()).or_insert_with(|| {
            places.push((p, PlaceInfo::default()));
            places.len() - 1
        });
        let info = &mut places[id].1;
        info.locs.push(i);
        info.charts.insert(loc.chart);
        let stems = if loc.is_intact() {
            &mut info.intact_stems
        } else {
            &mut info.missing_stems
        };
        stems.extend(loc.key_stems.iter().cloned());
    }
    let charts: BTreeSet<ChartId> = locs.iter().map(|l| l.chart).collect();
    println!(
        "index: {} copies, {} charts, {} places ({} folder, {} package)",
        locs.len(),
        charts.len(),
        places.len(),
        places
            .iter()
            .filter(|p| p.0 .0 == LocationKind::Folder)
            .count(),
        places
            .iter()
            .filter(|p| p.0 .0 == LocationKind::Package)
            .count(),
    );

    // ---- old game rule: the load copy's folder, split when "mixed".
    let mut copies_of: BTreeMap<ChartId, Vec<&Location>> = BTreeMap::new();
    for loc in locs {
        copies_of.entry(loc.chart).or_default().push(loc);
    }
    let load_place: BTreeMap<ChartId, Place> = copies_of
        .iter()
        .map(|(id, copies)| {
            let mut sorted = copies.clone();
            sorted.sort_by(|a, b| a.placement_key().cmp(&b.placement_key()));
            let i = Index::load_index(&sorted).unwrap_or(0);
            (*id, place_of(sorted[i]))
        })
        .collect();
    let mut stems_per_place: BTreeMap<&Place, (BTreeSet<String>, usize)> = BTreeMap::new();
    let title_of: BTreeMap<ChartId, &Location> =
        copies_of.iter().map(|(k, v)| (*k, v[0])).collect();
    for (id, p) in &load_place {
        let e = stems_per_place.entry(p).or_default();
        e.0.insert(title_stem(&title_of[id].title));
        e.1 += 1;
    }
    let mut game_groups: BTreeMap<String, Vec<ChartId>> = BTreeMap::new();
    let mut mixed_places = BTreeSet::new();
    for (id, p) in &load_place {
        let (distinct, n) = &stems_per_place[p];
        let mixed = distinct.len() > 2 && distinct.len() * 2 > *n;
        let key = if mixed {
            mixed_places.insert(p.clone());
            format!("{}\n{}", place_name(p), title_base(&title_of[id].title))
        } else {
            place_name(p)
        };
        game_groups.entry(key).or_default().push(*id);
    }

    // ---- old bpm rule.
    let collection = songs::build(&index, &manual);
    let bpm_song_of: BTreeMap<ChartId, usize> = collection
        .songs
        .iter()
        .enumerate()
        .flat_map(|(s, song)| song.charts.iter().map(move |&r| (s, r)))
        .map(|(s, r)| (collection.records[r].chart, s))
        .collect();

    // ---- new rule over places.
    let mut dsu = Dsu((0..places.len()).collect());
    let mut reasons: Vec<(usize, usize, &str)> = Vec::new();
    // R1: a chart in two places.
    for copies in copies_of.values() {
        let ps: Vec<usize> = copies.iter().map(|l| place_ids[&place_of(l)]).collect();
        for w in ps.windows(2) {
            if dsu.union(w[0], w[1]) {
                reasons.push((w[0], w[1], "same chart"));
            }
        }
    }
    // R2: a place whose missing key sounds another place has.
    let mut diff_links = Vec::new();
    for (b, (_, info)) in places.iter().enumerate() {
        if info.missing_stems.is_empty() {
            continue;
        }
        let needed: BTreeSet<&str> = info
            .missing_stems
            .iter()
            .filter(|s| !info.intact_stems.contains(*s))
            .map(String::as_str)
            .collect();
        let needed: BTreeSet<&str> = if needed.is_empty() {
            info.missing_stems.iter().map(String::as_str).collect()
        } else {
            needed
        };
        if needed.len() < 8 || mostly_serial(&needed) {
            println!(
                "  (diff check skipped) {}: needed {} serial {}",
                place_name(&places[b].0),
                needed.len(),
                mostly_serial(&needed)
            );
            continue;
        }
        let mut best: Option<(usize, f64, f64)> = None;
        for (a, (_, other)) in places.iter().enumerate() {
            if a == b || other.intact_stems.is_empty() {
                continue;
            }
            let inter = needed
                .iter()
                .filter(|s| other.intact_stems.contains(**s))
                .count();
            let c = inter as f64 / needed.len() as f64;
            let p = inter as f64 / other.intact_stems.len() as f64;
            if c >= 0.9 && best.is_none_or(|(_, bc, bp)| (c, p) > (bc, bp)) {
                best = Some((a, c, p));
            }
        }
        if best.is_none() {
            let top = places
                .iter()
                .enumerate()
                .filter(|(a, o)| *a != b && !o.1.intact_stems.is_empty())
                .map(|(a, o)| {
                    (
                        needed
                            .iter()
                            .filter(|s| o.1.intact_stems.contains(**s))
                            .count(),
                        a,
                    )
                })
                .max()
                .unwrap();
            println!(
                "  (no base) {}: needed {}, best {} in {}",
                place_name(&places[b].0),
                needed.len(),
                top.0,
                place_name(&places[top.1].0)
            );
        }
        if let Some((a, c, p)) = best {
            diff_links.push((b, a, c, p));
            if dsu.union(a, b) {
                reasons.push((a, b, "diff uses sounds"));
            }
        }
    }
    // R3: same title and artist with overlapping key sounds, across places.
    let mut by_name: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (i, loc) in locs.iter().enumerate() {
        let t = title_base(&loc.title).to_lowercase();
        if !t.is_empty() {
            by_name
                .entry((t, artist_key(&loc.artist)))
                .or_default()
                .push(i);
        }
    }
    let mut candidates = 0;
    let mut cand_pairs = BTreeSet::new();
    for members in by_name.values() {
        for (k, &x) in members.iter().enumerate() {
            for &y in &members[k + 1..] {
                let (px, py) = (
                    place_ids[&place_of(&locs[x])],
                    place_ids[&place_of(&locs[y])],
                );
                if dsu.find(px) == dsu.find(py) {
                    continue;
                }
                if sounds_match(&locs[x].key_stems, &locs[y].key_stems) {
                    dsu.union(px, py);
                    reasons.push((px, py, "title+artist+sounds"));
                } else {
                    candidates += 1;
                    if cand_pairs.insert((px.min(py), px.max(py))) {
                        let a: BTreeSet<&str> =
                            locs[x].key_stems.iter().map(String::as_str).collect();
                        let inter = locs[y]
                            .key_stems
                            .iter()
                            .filter(|s| a.contains(s.as_str()))
                            .count();
                        println!(
                            "  (candidate) {} | {}  <->  {}  stems {}/{} common {inter}",
                            locs[x].title,
                            place_name(&places[px].0),
                            place_name(&places[py].0),
                            locs[x].key_stems.len(),
                            locs[y].key_stems.len()
                        );
                    }
                }
            }
        }
    }
    // Manual links.
    let mut manual_joins = 0;
    let place_of_chart: BTreeMap<ChartId, usize> = locs
        .iter()
        .map(|l| (l.chart, place_ids[&place_of(l)]))
        .collect();
    for group in &manual.links {
        let ps: Vec<usize> = group
            .iter()
            .filter_map(|c| place_of_chart.get(c).copied())
            .collect();
        for w in ps.windows(2) {
            if dsu.union(w[0], w[1]) {
                manual_joins += 1;
                reasons.push((w[0], w[1], "manual link"));
            }
        }
    }

    // Cross-check: the shared rule function must give the same songs.
    let keys: Vec<String> = locs.iter().map(|l| place_name(&place_of(l))).collect();
    let copies: Vec<grouping::Copy> = locs
        .iter()
        .zip(&keys)
        .map(|(l, k)| grouping::Copy {
            place: k,
            chart: l.chart,
            title: &l.title,
            artist: &l.artist,
            key_stems: &l.key_stems,
            intact: l.is_intact(),
        })
        .collect();
    let shared = grouping::group(&copies, &manual.links, &manual.splits);
    let mut agree = true;
    for (i, a) in locs.iter().enumerate() {
        for (j, b) in locs.iter().enumerate().skip(i + 1).step_by(7) {
            let proto = dsu.find(place_ids[&place_of(a)]) == dsu.find(place_ids[&place_of(b)]);
            let real = shared.song_of(&keys[i]) == shared.song_of(&keys[j]);
            if proto != real {
                agree = false;
                println!("  MISMATCH {} | {}", keys[i], keys[j]);
            }
        }
    }
    println!(
        "shared rule: {} songs, {} candidates, agrees with prototype: {agree}",
        shared.songs.len(),
        shared.candidates.len()
    );

    let mut new_songs: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for p in 0..places.len() {
        new_songs.entry(dsu.find(p)).or_default().push(p);
    }
    let new_song_of_chart: BTreeMap<ChartId, usize> = place_of_chart
        .iter()
        .map(|(c, &p)| (*c, dsu.find(p)))
        .collect();

    println!();
    println!(
        "songs: old game {}, old bpm {}, new {}",
        game_groups.len(),
        collection.songs.len(),
        new_songs.len()
    );
    let mut by_reason: BTreeMap<&str, usize> = BTreeMap::new();
    for r in &reasons {
        *by_reason.entry(r.2).or_default() += 1;
    }
    println!("new-rule joins between places: {by_reason:?}");
    println!("title+artist-only candidates (not joined): {candidates}");
    println!(
        "manual links applied: {manual_joins}, manual splits on record: {}",
        manual.splits.len()
    );
    println!(
        "places the old game split as mixed (P1 now keeps whole): {}",
        mixed_places.len()
    );
    for p in &mixed_places {
        let titles: BTreeSet<String> = places[place_ids[p]]
            .1
            .locs
            .iter()
            .map(|&i| locs[i].title.clone())
            .collect();
        println!(
            "  {}  ({} titles: {})",
            place_name(p),
            titles.len(),
            titles.into_iter().take(6).collect::<Vec<_>>().join(" | ")
        );
    }

    // Old bpm songs that P1 merges (a folder bpm split in two or more).
    let mut bpm_split_places = 0;
    for (p, info) in &places {
        let songs: BTreeSet<usize> = info
            .charts
            .iter()
            .filter_map(|c| bpm_song_of.get(c))
            .copied()
            .collect();
        if songs.len() > 1 {
            bpm_split_places += 1;
            if bpm_split_places <= 15 {
                let titles: BTreeSet<&str> =
                    info.locs.iter().map(|&i| locs[i].title.as_str()).collect();
                println!(
                    "  bpm split: {} -> {} songs ({})",
                    place_name(p),
                    songs.len(),
                    titles.into_iter().take(5).collect::<Vec<_>>().join(" | ")
                );
            }
        }
    }
    println!("places old bpm split into several songs: {bpm_split_places}");

    // Old bpm songs the new rule cuts apart (should be none: new only joins more).
    let mut cut = 0;
    for song in &collection.songs {
        let ns: BTreeSet<usize> = song
            .charts
            .iter()
            .map(|&r| new_song_of_chart[&collection.records[r].chart])
            .collect();
        if ns.len() > 1 {
            cut += 1;
        }
    }
    println!("old bpm songs the new rule splits: {cut}");

    println!();
    println!("new songs over several places:");
    let mut multi = 0;
    for (root, ps) in &new_songs {
        if ps.len() < 2 {
            continue;
        }
        multi += 1;
        let why: BTreeSet<&str> = reasons
            .iter()
            .filter(|r| dsu.find(r.0) == *root)
            .map(|r| r.2)
            .collect();
        let title = locs[places[ps[0]].1.locs[0]].title.clone();
        println!(
            "- {title}  [{}]",
            why.into_iter().collect::<Vec<_>>().join(", ")
        );
        for &p in ps {
            let info = &places[p].1;
            println!(
                "    {}  charts {}  intact {}  missing {}",
                place_name(&places[p].0),
                info.charts.len(),
                info.locs.iter().filter(|&&i| locs[i].is_intact()).count(),
                info.locs.iter().filter(|&&i| !locs[i].is_intact()).count(),
            );
        }
    }
    println!("total: {multi}");
    println!();
    println!("diff -> base links (coverage, precision):");
    for (b, a, c, p) in &diff_links {
        println!(
            "  {}  ->  {}  ({c:.2}, {p:.2})",
            place_name(&places[*b].0),
            place_name(&places[*a].0)
        );
    }

    // ---- key-sound sets per multi-place song: audio files and sizes on disk.
    println!();
    println!("key-sound sets in songs over several places:");
    let pkg_root = bms_package::installed::packages_root();
    let states: BTreeMap<String, PathBuf> = bms_package::installed::read_active_states(&pkg_root)
        .unwrap_or_default()
        .into_iter()
        .map(|s| (format!("{}@{}", s.id, s.state_hash), s.bmsp_path))
        .collect();
    let (mut multi_sets, mut identical, mut differ) = (0, 0, 0);
    for ps in new_songs.values().filter(|ps| ps.len() > 1) {
        let mut sets: Vec<(String, BTreeMap<String, (u64, Vec<u8>)>)> = Vec::new();
        for &p in ps {
            let files = audio_files(&places[p].0, &states);
            if files.is_empty() {
                continue;
            }
            sets.push((place_name(&places[p].0), files));
        }
        if sets.len() < 2 {
            continue;
        }
        multi_sets += 1;
        let song_stems: BTreeSet<&str> = ps
            .iter()
            .flat_map(|&p| places[p].1.locs.iter())
            .flat_map(|&i| locs[i].key_stems.iter())
            .map(|s| s.rsplit('/').next().unwrap_or(s))
            .collect();
        let title = locs[places[ps[0]].1.locs[0]].title.clone();
        println!("- {title}");
        for (name, files) in &sets {
            let total: u64 = files.values().map(|f| f.0).sum();
            let formats: BTreeMap<String, usize> =
                files.values().fold(BTreeMap::new(), |mut m, f| {
                    *m.entry(describe(&f.1)).or_default() += 1;
                    m
                });
            println!(
                "    {name}: {} files, {:.1} MB, {formats:?}",
                files.len(),
                total as f64 / 1e6
            );
        }
        for (i, (na, a)) in sets.iter().enumerate() {
            for (nb, b) in &sets[i + 1..] {
                // Only files the song's charts declare count; previews and extras do not.
                let declared = |k: &String| song_stems.contains(k.as_str());
                let common: Vec<&String> = a
                    .keys()
                    .filter(|k| b.contains_key(*k) && declared(k))
                    .collect();
                let size_diff = common.iter().filter(|k| a[**k].0 != b[**k].0).count();
                let only_a = a
                    .keys()
                    .filter(|k| declared(k) && !b.contains_key(*k))
                    .count();
                let only_b = b
                    .keys()
                    .filter(|k| declared(k) && !a.contains_key(*k))
                    .count();
                let same = size_diff == 0 && only_a == 0 && only_b == 0;
                if same {
                    identical += 1
                } else {
                    differ += 1
                }
                println!("    {} vs {}: {} (declared common {}, size differs {size_diff}, only-left {only_a}, only-right {only_b})", short(na), short(nb), if same { "SAME" } else { "DIFFER" }, common.len());
            }
        }
    }
    println!("songs with 2+ key-sound sets: {multi_sets}; set pairs identical {identical}, differ {differ}");
}

fn short(name: &str) -> &str {
    let n = name.len();
    &name[name
        .char_indices()
        .map(|(i, _)| i)
        .find(|&i| n - i <= 60)
        .unwrap_or(0)..]
}

/// Audio files directly in a place, by lowercase stem: size and the first 4 KB.
fn audio_files(p: &Place, states: &BTreeMap<String, PathBuf>) -> BTreeMap<String, (u64, Vec<u8>)> {
    let is_audio = |name: &str| {
        let ext = name.rsplit_once('.').map_or("", |x| x.1).to_lowercase();
        ["wav", "ogg", "flac", "mp3"].contains(&ext.as_str())
    };
    let stem = |name: &str| name.rsplit_once('.').map_or(name, |x| x.0).to_lowercase();
    let mut out = BTreeMap::new();
    match p.0 {
        LocationKind::Folder => {
            let Ok(entries) = fs::read_dir(Path::new(&p.1).join(&p.2)) else {
                return out;
            };
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if !is_audio(&name) {
                    continue;
                }
                let head = fs::File::open(e.path())
                    .and_then(|f| {
                        let mut v = Vec::new();
                        std::io::Read::take(f, 4096).read_to_end(&mut v).map(|_| v)
                    })
                    .unwrap_or_default();
                out.insert(
                    stem(&name),
                    (e.metadata().map(|m| m.len()).unwrap_or(0), head),
                );
            }
        }
        LocationKind::Package => {
            let Some(path) = states.get(&p.1) else {
                return out;
            };
            let Ok(mut reader) = bms_package::PackageReader::open_file(path) else {
                return out;
            };
            let prefix = if p.2.is_empty() {
                String::new()
            } else {
                format!("{}/", p.2)
            };
            let entries: Vec<(String, u64)> = reader
                .entries()
                .iter()
                .filter(|e| {
                    e.path.starts_with(&prefix)
                        && !e.path[prefix.len()..].contains('/')
                        && is_audio(&e.path)
                })
                .map(|e| (e.path.clone(), e.size))
                .collect();
            for (path, size) in entries {
                let mut head = reader.read_entry(&path).unwrap_or_default();
                head.truncate(4096);
                out.insert(stem(&path[prefix.len()..]), (size, head));
            }
        }
    }
    out
}

/// Format summary from the file header: `WAV 44100/16/2`, `OGG 44100/2 ~128k`.
fn describe(bytes: &[u8]) -> String {
    if bytes.starts_with(b"RIFF") && bytes.len() >= 36 {
        // Find the fmt chunk.
        let mut i = 12;
        while i + 8 <= bytes.len() {
            let size = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().unwrap()) as usize;
            if &bytes[i..i + 4] == b"fmt " && i + 24 <= bytes.len() {
                let fmt = u16::from_le_bytes([bytes[i + 8], bytes[i + 9]]);
                let ch = u16::from_le_bytes([bytes[i + 10], bytes[i + 11]]);
                let sr = u32::from_le_bytes(bytes[i + 12..i + 16].try_into().unwrap());
                let bits = u16::from_le_bytes([bytes[i + 22], bytes[i + 23]]);
                let codec = if fmt == 1 || fmt == 0xfffe {
                    "WAV"
                } else {
                    "WAV-ADPCM"
                };
                return format!("{codec} {sr}/{bits}/{ch}");
            }
            i += 8 + size + (size & 1);
        }
        return "WAV ?".into();
    }
    if bytes.starts_with(b"OggS") {
        if let Some(pos) = bytes.windows(7).position(|w| w == b"\x01vorbis") {
            let h = &bytes[pos + 7..];
            if h.len() >= 16 {
                let ch = h[4];
                let sr = u32::from_le_bytes(h[5..9].try_into().unwrap());
                let nominal =
                    i32::from_le_bytes(h[13..17.min(h.len())].try_into().unwrap_or([0; 4]));
                return format!("OGG {sr}/{ch} ~{}k", nominal / 1000);
            }
        }
        return "OGG ?".into();
    }
    if bytes.starts_with(b"fLaC") {
        return "FLAC".into();
    }
    "MP3/?".into()
}

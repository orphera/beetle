//! B18 measurement (docs/plans/2026-11-01-package-by-song-plan.md, step 1):
//! over `collection.idx`, how many key sound bytes are stored twice (the same
//! set at two places: package and package, folder and package, folder and
//! folder), and what the add-time verdict would have been for each place had
//! it been added last. Also the B20 measurement
//! (docs/plans/2026-11-02-chart-only-packages-plan.md, step 1): package
//! places holding the same set as a library folder, split by whether every
//! chart there is found elsewhere (the place could go) or some chart is not
//! (it could keep only its charts), counted per package too. Run from the
//! working directory:
//! `cargo run --release -p bms-package-manager --example package_measure`.

use beetle_core::ChartId;
use bms_package::installed::{packages_root, PACKAGE_FILENAME};
use bms_package_manager::collection::{index_file, Index, Location, LocationKind};
use bms_package_manager::key_sets::{self, Files, PlaceAt};
use bms_package_manager::manager::PackageManager;
use bms_package_manager::songs::{self, Manual};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_048_576.0)
}

fn main() {
    let text = fs::read_to_string(index_file()).expect("run `bpm scan` first");
    let index = Index::parse(&text).expect("a usable index");
    let collection = songs::build(&index, &Manual::load().unwrap_or_default());
    let manager = PackageManager::new(packages_root()).ok();
    let package_file = |source: &str| -> Option<PathBuf> {
        let (id, hash) = source.split_once('@')?;
        let mgr = manager.as_ref()?;
        let record = mgr
            .registry()
            .list_packages()
            .into_iter()
            .find(|r| r.id == id)?;
        let state = &record.state_hashes.get(hash)?.path;
        Some(mgr.root_dir().join(state).join(PACKAGE_FILENAME))
    };
    let place_at = |copy: &Location| -> Option<PlaceAt> {
        let dir = copy.path.rsplit_once('/').map_or("", |(dir, _)| dir);
        match copy.kind {
            LocationKind::Folder => Some(PlaceAt::Folder(Path::new(&copy.source).join(dir))),
            LocationKind::Package => Some(PlaceAt::Package {
                bmsp: package_file(&copy.source)?,
                dir: dir.to_string(),
            }),
        }
    };

    let mut total = BTreeMap::<&str, u64>::new();
    let mut twice = BTreeMap::<&str, (u64, usize)>::new();
    let mut verdicts = BTreeMap::<&str, usize>::new();
    let mut examples = BTreeMap::<&str, Vec<String>>::new();
    let mut unresolved = 0;
    // B20: package source -> its places that hold a folder's set, with the
    // bytes both have and whether the place has charts found nowhere else.
    let mut dup = BTreeMap::<String, Vec<(String, u64, bool)>>::new();
    for song in &collection.songs {
        let mut places: BTreeMap<String, (&Location, BTreeSet<ChartId>)> = BTreeMap::new();
        for &i in &song.charts {
            let record = &collection.records[i];
            for copy in &record.copies {
                places
                    .entry(songs::place_key(copy))
                    .or_insert_with(|| (copy, BTreeSet::new()))
                    .1
                    .insert(record.chart);
            }
        }
        let Some(names) = song
            .charts
            .iter()
            .flat_map(|&i| &collection.records[i].copies)
            .map(|c| &c.key_stems)
            .max_by_key(|stems| stems.len())
            .map(|stems| stems.iter().map(|s| format!("{s}.wav")).collect::<Vec<_>>())
        else {
            continue;
        };
        // Each place's files, and which places hold the same set.
        let mut found: Vec<(&String, LocationKind, Files)> = Vec::new();
        for (key, (copy, _)) in &places {
            let Some(at) = place_at(copy) else {
                unresolved += 1;
                continue;
            };
            let files = key_sets::files_at(&at, &names);
            let kind = if copy.kind == LocationKind::Folder {
                "folder"
            } else {
                "package"
            };
            *total.entry(kind).or_default() += files.total_size();
            found.push((key, copy.kind, files));
        }
        let mut seen: Vec<usize> = Vec::new();
        for (i, (key, kind, files)) in found.iter().enumerate() {
            let whole = files.whole(&names);
            let same = seen
                .iter()
                .copied()
                .find(|&j| whole && found[j].2.whole(&names) && found[j].2.same(files));
            if let Some(j) = same {
                let pair = match (found[j].1, *kind) {
                    (LocationKind::Package, LocationKind::Package) => "package + package",
                    (LocationKind::Folder, LocationKind::Folder) => "folder + folder",
                    _ => "folder + package",
                };
                let e = twice.entry(pair).or_default();
                e.0 += files.total_size().min(found[j].2.total_size());
                e.1 += 1;
            }
            seen.push(i);

            // B20: a package place with a library folder's set.
            let folder_twin = found.iter().find(|(k, kind, f)| {
                k != key && *kind == LocationKind::Folder && f.whole(&names) && f.same(files)
            });
            if let (LocationKind::Package, true, Some((_, _, twin))) = (*kind, whole, folder_twin) {
                let shared: Vec<String> = {
                    let lacking = twin.missing(&names);
                    names
                        .iter()
                        .filter(|n| !lacking.contains(n))
                        .cloned()
                        .collect()
                };
                let at = place_at(places[*key].0).expect("resolved above");
                let bytes = key_sets::files_at(&at, &shared).total_size();
                let unique = places[*key].1.iter().any(|c| {
                    places
                        .iter()
                        .all(|(k, (_, other))| k == *key || !other.contains(c))
                });
                dup.entry(places[*key].0.source.clone()).or_default().push((
                    (*key).clone(),
                    bytes,
                    unique,
                ));
            }

            // The verdict had this place come last.
            let others: BTreeSet<ChartId> = places
                .iter()
                .filter(|(k, _)| k != key)
                .flat_map(|(_, (_, c))| c.iter().copied())
                .collect();
            let charts = &places[*key].1;
            let new_charts = charts.iter().filter(|c| !others.contains(c)).count();
            let same_elsewhere = whole
                && found
                    .iter()
                    .any(|(k, _, f)| k != key && f.whole(&names) && f.same(files));
            let other_sets = found.iter().any(|(k, _, f)| k != key && f.whole(&names));
            let verdict = if places.len() == 1 {
                "new song"
            } else if new_charts == 0 && (same_elsewhere || !whole) {
                "already there"
            } else if !whole || same_elsewhere {
                "new charts"
            } else if other_sets {
                "other key sound set"
            } else {
                "new charts"
            };
            *verdicts.entry(verdict).or_default() += 1;
            let list = examples.entry(verdict).or_default();
            if list.len() < 5 && verdict != "new song" {
                list.push(format!(
                    "{} ({new_charts} new chart(s)) at {key}",
                    song.title
                ));
            }
        }
    }

    println!("Key sound bytes found at places (declared names only):");
    for (kind, bytes) in &total {
        println!("  {kind:<8} {}", mb(*bytes));
    }
    println!("Stored twice (the same set at another place, smaller copy counted):");
    for (pair, (bytes, n)) in &twice {
        println!("  {pair:<18} {n:>4} place(s)  {}", mb(*bytes));
    }
    println!("Add-time verdict had each place come last:");
    for (verdict, n) in &verdicts {
        println!("  {verdict:<20} {n:>4}");
        for e in examples.get(verdict).into_iter().flatten() {
            println!("      {e}");
        }
    }

    // B20: per package, whether all of its places hold a folder's set.
    let mut places_of = BTreeMap::<&str, BTreeSet<String>>::new();
    for l in index
        .locations
        .iter()
        .filter(|l| l.kind == LocationKind::Package)
    {
        places_of
            .entry(l.source.as_str())
            .or_default()
            .insert(songs::place_key(l));
    }
    let (mut go, mut keep) = ((0, 0u64), (0, 0u64));
    let mut by_package = BTreeMap::<&str, (usize, u64)>::new();
    let mut keep_examples = Vec::new();
    for (source, list) in &dup {
        for (key, bytes, unique) in list {
            let e = if *unique { &mut keep } else { &mut go };
            e.0 += 1;
            e.1 += bytes;
            if *unique && keep_examples.len() < 8 {
                keep_examples.push(format!("{} at {key}", mb(*bytes)));
            }
        }
        let all = places_of.get(source.as_str()).map_or(0, BTreeSet::len);
        let bytes: u64 = list.iter().map(|(_, b, _)| b).sum();
        let class = if list.len() < all {
            "some places only"
        } else if list.iter().all(|(_, _, u)| !u) {
            "whole package, all charts elsewhere"
        } else {
            "whole package, some charts only here"
        };
        let e = by_package.entry(class).or_default();
        e.0 += 1;
        e.1 += bytes;
    }
    println!("B20: package places with a library folder's set (bytes both have):");
    println!(
        "  every chart found elsewhere  {:>4} place(s)  {}",
        go.0,
        mb(go.1)
    );
    println!(
        "  some chart only here         {:>4} place(s)  {}",
        keep.0,
        mb(keep.1)
    );
    for e in &keep_examples {
        println!("      {e}");
    }
    println!("B20: by package:");
    for (class, (n, bytes)) in &by_package {
        println!("  {class:<38} {n:>4} package(s)  {}", mb(*bytes));
    }
    if unresolved > 0 {
        println!("Places whose package file was not found: {unresolved}");
    }
}

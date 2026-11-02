//! Which song a chart-only package plays with (B20,
//! docs/plans/2026-11-02-chart-only-packages-plan.md 3.1 and 3.2).
//!
//! A package whose charts borrow their key sounds from a song elsewhere says
//! so in its manifest: `"base": {"charts": [<chart ids>], "package": <id>}`.
//! Chart ids are fixed by the chart bytes, so the record means the same on
//! any machine. The scan turns each record into a group for the grouping's
//! rule 0 (the package's charts with the base charts), so the package joins
//! that song without the borrowed key sound guess (`grouping` rule 2).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use beetle_core::ChartId;
use bms_package::installed::read_active_states;
use bms_package::{Manifest, MANIFEST_FILENAME};

/// The manifest field.
pub const FIELD: &str = "base";

/// A chart-only package's base.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Base {
    /// Charts of the song it plays with: it joins the song holding any.
    pub charts: Vec<ChartId>,
    /// The package holding that song's key sounds, when one does (for notices).
    pub package: Option<String>,
}

impl Base {
    /// The base a manifest records. `None` without one, or with no chart
    /// that can be read.
    pub fn of(manifest: &Manifest) -> Option<Self> {
        let value = manifest.extra.get(FIELD)?;
        let charts: Vec<ChartId> = value
            .get("charts")?
            .as_array()?
            .iter()
            .filter_map(|c| ChartId::from_hex(c.as_str()?))
            .collect();
        if charts.is_empty() {
            return None;
        }
        let package = value
            .get("package")
            .and_then(|p| p.as_str())
            .map(str::to_string);
        Some(Self { charts, package })
    }

    /// Records the base in `manifest`, charts sorted and unique so the same
    /// base always writes the same bytes (INV-6).
    pub fn write_to(&self, manifest: &mut Manifest) {
        let charts: BTreeSet<String> = self.charts.iter().map(ChartId::to_hex).collect();
        let mut value = serde_json::json!({ "charts": charts.into_iter().collect::<Vec<_>>() });
        if let Some(package) = &self.package {
            value["package"] = serde_json::Value::String(package.clone());
        }
        manifest.extra.insert(FIELD.to_string(), value);
    }
}

/// The base charts of each active package state under the packages folder
/// `root`, by source (`<id>@<state hash>`), from the `manifest.json` each
/// state keeps beside its package.
pub fn of_active(root: &Path) -> BTreeMap<String, Vec<ChartId>> {
    let Ok(states) = read_active_states(root) else {
        return BTreeMap::new();
    };
    states
        .into_iter()
        .filter_map(|state| {
            let text =
                fs::read_to_string(state.bmsp_path.with_file_name(MANIFEST_FILENAME)).ok()?;
            let base = Base::of(&Manifest::from_json_str(&text).ok()?)?;
            Some((format!("{}@{}", state.id, state.state_hash), base.charts))
        })
        .collect()
}

/// Groups for the grouping's rule 0: each based package's charts (`copies`
/// are a source and a chart there) with its base charts.
pub fn links<'a>(
    bases: &BTreeMap<String, Vec<ChartId>>,
    copies: impl IntoIterator<Item = (&'a str, ChartId)>,
) -> Vec<Vec<ChartId>> {
    let mut own: BTreeMap<&str, BTreeSet<ChartId>> = BTreeMap::new();
    for (source, chart) in copies {
        if bases.contains_key(source) {
            own.entry(source).or_default().insert(chart);
        }
    }
    own.into_iter()
        .map(|(source, charts)| {
            let mut group: Vec<ChartId> = charts.into_iter().collect();
            group.extend(bases[source].iter().copied());
            group
        })
        .collect()
}

/// The package id of a source (`<id>@<state hash>`).
fn id_of(source: &str) -> &str {
    source.split_once('@').map_or(source, |(id, _)| id)
}

/// The chart-only packages (ids, sorted) that would lose their key sounds
/// if the packages `going` went: every copy of their base charts is in
/// those packages (B20 plan 3.6). `copies_of` lists where a chart is: the
/// package id of a package copy, `None` for a folder copy. A package in
/// `going` is not listed, nor one whose base charts are found nowhere
/// already (`orphans`).
pub fn leaning_on(
    bases: &BTreeMap<String, Vec<ChartId>>,
    going: &[&str],
    copies_of: impl Fn(ChartId) -> Vec<Option<String>>,
) -> Vec<String> {
    let mut out: Vec<String> = bases
        .iter()
        .filter(|(source, _)| !going.contains(&id_of(source)))
        .filter(|(_, charts)| {
            let copies: Vec<Option<String>> = charts.iter().flat_map(|c| copies_of(*c)).collect();
            !copies.is_empty()
                && copies
                    .iter()
                    .all(|c| c.as_deref().is_some_and(|id| going.contains(&id)))
        })
        .map(|(source, _)| id_of(source).to_string())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The chart-only packages (ids, sorted) whose base charts are found nowhere:
/// their charts have no key sounds to play with.
pub fn orphans(
    bases: &BTreeMap<String, Vec<ChartId>>,
    copies_of: impl Fn(ChartId) -> Vec<Option<String>>,
) -> Vec<String> {
    let mut out: Vec<String> = bases
        .iter()
        .filter(|(_, charts)| charts.iter().all(|c| copies_of(*c).is_empty()))
        .map(|(source, _)| id_of(source).to_string())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// `copies_of` for [`leaning_on`] and [`orphans`] from bpm's index.
pub fn copies_in(index: &crate::collection::Index) -> impl Fn(ChartId) -> Vec<Option<String>> + '_ {
    let mut by_chart: BTreeMap<ChartId, Vec<Option<String>>> = BTreeMap::new();
    for l in &index.locations {
        let id = match l.kind {
            crate::collection::LocationKind::Package => Some(id_of(&l.source).to_string()),
            crate::collection::LocationKind::Folder => None,
        };
        by_chart.entry(l.chart).or_default().push(id);
    }
    move |chart| by_chart.get(&chart).cloned().unwrap_or_default()
}

/// The key sounds that chart-only package `bmsp` (source `<id>@<state
/// hash>` in `index`) lacks and its base set has (B20 plan 3.7): entry paths
/// beside its charts, and bytes. The set is the place of a base chart with
/// the most of the files its charts declare, as the loader would play.
/// Empty without a base. `package_file` finds a package source's file.
pub fn base_sounds(
    bmsp: &Path,
    source: &str,
    index: &crate::collection::Index,
    package_file: impl Fn(&str) -> Option<PathBuf>,
) -> Result<Vec<(String, Vec<u8>)>, crate::error::PackageManagerError> {
    use crate::collection::LocationKind;
    use crate::key_sets::{files_at, PlaceAt};

    let Some(base) = Base::of(bms_package::PackageReader::open_file(bmsp)?.manifest()) else {
        return Ok(Vec::new());
    };
    let dir_of = |path: &str| path.rsplit_once('/').map_or("", |(dir, _)| dir).to_string();
    // The package's chart directories, with the key sounds declared there.
    let mut dirs: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for l in index.locations.iter().filter(|l| l.source == source) {
        let names = l.key_stems.iter().map(|s| format!("{s}.wav"));
        dirs.entry(dir_of(&l.path)).or_default().extend(names);
    }
    let all: Vec<String> = dirs
        .values()
        .flatten()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut places: Vec<PlaceAt> = Vec::new();
    for l in &index.locations {
        if l.source == source || !base.charts.contains(&l.chart) {
            continue;
        }
        let dir = dir_of(&l.path);
        let at = match l.kind {
            LocationKind::Folder => Some(PlaceAt::Folder(Path::new(&l.source).join(&dir))),
            LocationKind::Package => {
                package_file(&l.source).map(|bmsp| PlaceAt::Package { bmsp, dir })
            }
        };
        if let Some(at) = at.filter(|at| !places.contains(at)) {
            places.push(at);
        }
    }
    let from = places
        .into_iter()
        .map(|at| {
            let found = files_at(&at, &all).len();
            (at, found)
        })
        .filter(|(_, found)| *found > 0)
        .max_by_key(|(_, found)| *found)
        .map(|(at, _)| at)
        .ok_or_else(|| {
            crate::error::PackageManagerError::NotInstalled(format!(
                "the key sounds {source} plays with"
            ))
        })?;
    let mut from_package = match &from {
        PlaceAt::Package { bmsp, .. } => Some(bms_package::PackageReader::open_file(bmsp)?),
        PlaceAt::Folder(_) => None,
    };
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (dir, names) in &dirs {
        let names: Vec<String> = names.iter().cloned().collect();
        let own = PlaceAt::Package {
            bmsp: bmsp.to_path_buf(),
            dir: dir.clone(),
        };
        let missing = files_at(&own, &names).missing(&names);
        for path in files_at(&from, &missing).paths() {
            let (rel, bytes) = match (&from, &mut from_package) {
                (PlaceAt::Folder(root), _) => {
                    let rel = Path::new(&path)
                        .strip_prefix(root)
                        .map_or(path.clone(), |p| p.to_string_lossy().replace('\\', "/"));
                    (rel, fs::read(&path)?)
                }
                (PlaceAt::Package { dir: from_dir, .. }, Some(pkg)) => {
                    let rel = match from_dir.as_str() {
                        "" => path.clone(),
                        d => path
                            .strip_prefix(&format!("{d}/"))
                            .unwrap_or(&path)
                            .to_string(),
                    };
                    (rel, pkg.read_entry(&path)?)
                }
                _ => continue,
            };
            let entry = if dir.is_empty() {
                rel
            } else {
                format!("{dir}/{rel}")
            };
            out.entry(entry).or_insert(bytes);
        }
    }
    Ok(out.into_iter().collect())
}

/// Chart-only package `bmsp` with `sounds` (`base_sounds`) as one whole
/// package: the manifest drops its base, and an id ending in
/// `add::CHARTS_ONLY_SUFFIX` loses it. Built as every package is, so the
/// same input gives the same bytes (INV-6).
pub fn merged(
    bmsp: &Path,
    sounds: Vec<(String, Vec<u8>)>,
) -> Result<Vec<u8>, crate::error::PackageManagerError> {
    let mut reader = bms_package::PackageReader::open_file(bmsp)?;
    let mut manifest = reader.manifest().clone();
    manifest.extra.remove(FIELD);
    if let Some(id) = manifest.id.strip_suffix(crate::add::CHARTS_ONLY_SUFFIX) {
        manifest.id = id.to_string();
    }
    let mut builder = bms_package::PackageBuilder::new(manifest);
    let paths: Vec<String> = reader
        .entries()
        .iter()
        .map(|e| e.path.clone())
        .filter(|p| p != MANIFEST_FILENAME)
        .collect();
    for path in &paths {
        let bytes = reader.read_entry(path)?;
        builder.add_file(path.clone(), bytes)?;
    }
    for (path, bytes) in sounds {
        if !paths.contains(&path) {
            builder.add_file(path, bytes)?;
        }
    }
    Ok(builder.build_to_bytes()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_package_leans_on_another_only_when_its_base_is_nowhere_else() {
        let id = |n: u8| ChartId::of_bytes(&[n]);
        let bases = BTreeMap::from([
            ("a.charts@1".to_string(), vec![id(1)]),
            ("b.charts@2".to_string(), vec![id(2)]),
            ("c.charts@3".to_string(), vec![id(3)]),
        ]);
        // Chart 1 is only in package `a`; chart 2 in `b` and a folder;
        // chart 3 is gone.
        let copies = |c: ChartId| match c {
            c if c == id(1) => vec![Some("a".to_string())],
            c if c == id(2) => vec![Some("b".to_string()), None],
            _ => Vec::new(),
        };
        assert_eq!(leaning_on(&bases, &["a"], copies), ["a.charts"]);
        assert!(leaning_on(&bases, &["b"], copies).is_empty());
        // Removing the chart-only package itself warns of nothing.
        assert!(leaning_on(&bases, &["a", "a.charts"], copies).is_empty());
        assert_eq!(orphans(&bases, copies), ["c.charts"]);
    }

    #[test]
    fn an_export_takes_back_the_key_sounds_the_base_folder_has() {
        let root = std::env::temp_dir().join(format!("bpm-merge-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let lib = root.join("lib");
        let song = lib.join("song");
        fs::create_dir_all(&song).unwrap();
        fs::write(
            song.join("x.bms"),
            "#TITLE X
#WAV01 a.wav
#WAV02 b.wav
#00111:0102
",
        )
        .unwrap();
        fs::write(song.join("a.wav"), b"sound a").unwrap();
        fs::write(song.join("B.WAV"), b"sound b").unwrap();
        let folders = vec![lib.to_string_lossy().into_owned()];
        let (index, _) = crate::collection::build_index(&folders, &[]);
        let base = Base {
            charts: index.locations.iter().map(|l| l.chart).collect(),
            package: None,
        };
        assert_eq!(base.charts.len(), 1);

        // The package's own chart declares one more sound, and holds a
        // picture with a key sound's stem.
        let mut manifest = Manifest::new("t.charts", "T");
        base.write_to(&mut manifest);
        let mut builder = bms_package::PackageBuilder::new(manifest);
        builder
            .add_file(
                "song/y.bms",
                "#TITLE X [Y]
#WAV01 a.wav
#WAV02 b.wav
#WAV03 c.wav
#00111:010203
",
            )
            .unwrap();
        builder.add_file("song/c.wav", b"sound c".to_vec()).unwrap();
        builder.add_file("song/b.bmp", b"picture".to_vec()).unwrap();
        let mut mgr = crate::manager::PackageManager::new(root.join("store")).unwrap();
        let installed = mgr
            .install_from_bytes(builder.build_to_bytes().unwrap())
            .unwrap();
        let bmsp = installed.location.join("package.bmsp");
        let source = format!("t.charts@{}", installed.state_hash);
        let (index, _) = crate::collection::build_index(&folders, &[installed]);

        let sounds = base_sounds(&bmsp, &source, &index, |_| None).unwrap();
        let paths: Vec<&str> = sounds.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(paths, ["song/B.WAV", "song/a.wav"]);

        let bytes = merged(&bmsp, sounds.clone()).unwrap();
        assert_eq!(bytes, merged(&bmsp, sounds).unwrap());
        let whole = bms_package::Package::from_bytes(bytes).unwrap();
        assert_eq!(whole.manifest().id, "t");
        assert_eq!(Base::of(whole.manifest()), None);
        let entries: Vec<&str> = whole.entries().iter().map(|e| e.path.as_str()).collect();
        for path in [
            "song/B.WAV",
            "song/a.wav",
            "song/b.bmp",
            "song/c.wav",
            "song/y.bms",
        ] {
            assert!(entries.contains(&path), "{entries:?}");
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_base_survives_the_manifest_and_makes_a_link() {
        let id = |n: u8| ChartId::of_bytes(&[n]);
        let mut manifest = Manifest::new("t.charts", "T");
        assert_eq!(Base::of(&manifest), None);
        let base = Base {
            charts: vec![id(2), id(1), id(2)],
            package: Some("t.song".into()),
        };
        base.write_to(&mut manifest);
        let back = Manifest::from_json_str(&manifest.to_json_string().unwrap()).unwrap();
        let read = Base::of(&back).unwrap();
        let mut sorted = vec![id(1), id(2)];
        sorted.sort_by_key(ChartId::to_hex);
        assert_eq!(read.charts, sorted);
        assert_eq!(read.package.as_deref(), Some("t.song"));

        let bases = BTreeMap::from([("t.charts@h".to_string(), vec![id(1)])]);
        let copies = [
            ("t.charts@h", id(7)),
            ("t.charts@h", id(8)),
            ("other@h", id(9)),
        ];
        let groups = links(&bases, copies);
        assert_eq!(groups.len(), 1);
        let got: BTreeSet<ChartId> = groups[0].iter().copied().collect();
        assert_eq!(got, BTreeSet::from([id(7), id(8), id(1)]));
    }
}

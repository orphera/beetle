//! Key sound sets of a song's places, compared from file headers (B17, plan P2
//! and P2.1 in docs/plans/2026-11-01-song-grouping-plan.md). The game's song
//! menu and `bpm song clean` both use it.
//!
//! A place is a folder, or a directory inside a package. It is a set for a
//! chart when it has 90% of the key sounds the chart declares. Places whose
//! shared files all have the same sizes hold the same set. A set is clearly
//! worse than another when, on 95% of the files both have, the other is no
//! worse in anything and better in something (`audio_info::better`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use beetle_core::audio_info::{self, AudioInfo};
use beetle_core::key_sounds::{folder_sound_candidates, SOUND_EXTENSIONS};
use beetle_core::ChartId;

/// Where a place's files are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaceAt {
    Folder(PathBuf),
    /// A directory (`/` separated, empty for the root) inside a package file.
    Package {
        bmsp: PathBuf,
        dir: String,
    },
}

/// One declared key sound found at a place.
#[derive(Debug, Clone)]
struct FileAt {
    size: u64,
    ext: String,
    /// Folder: the file's path. Package: the entry path.
    path: String,
}

/// A place's files for some declared names, by lowercased name.
#[derive(Debug, Clone, Default)]
pub struct Files(HashMap<String, FileAt>);

impl Files {
    /// Whether the place has 90% of the names (P2).
    pub fn whole(&self, names: &[String]) -> bool {
        !names.is_empty() && self.0.len() * 10 >= names.len() * 9
    }

    /// How many of the names were found.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The names not found at the place.
    pub fn missing(&self, names: &[String]) -> Vec<String> {
        names
            .iter()
            .filter(|n| !self.0.contains_key(&n.to_lowercase()))
            .cloned()
            .collect()
    }

    /// The found files' bytes in all.
    pub fn total_size(&self) -> u64 {
        self.0.values().map(|f| f.size).sum()
    }

    /// Whether two places hold the same files: every name both have is the
    /// same size, and they have at least one name in common.
    /// The paths (`files_at`'s: a folder file's path, a package entry) of
    /// this place's files that `other` also has, with the same size.
    pub fn shared_with(&self, other: &Files) -> Vec<String> {
        let mut out: Vec<String> = self
            .0
            .iter()
            .filter(|(name, file)| other.0.get(*name).is_some_and(|o| o.size == file.size))
            .map(|(_, file)| file.path.clone())
            .collect();
        out.sort();
        out
    }

    pub fn same(&self, other: &Files) -> bool {
        let mut shared = 0;
        for (name, file) in &self.0 {
            if let Some(theirs) = other.0.get(name) {
                if file.size != theirs.size {
                    return false;
                }
                shared += 1;
            }
        }
        shared > 0
    }
}

fn extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
}

/// The files at a place for `names`, found as the loader finds them.
pub fn files_at(at: &PlaceAt, names: &[String]) -> Files {
    let mut out = HashMap::new();
    match at {
        PlaceAt::Folder(dir) => {
            let listing: HashMap<String, (u64, PathBuf)> = fs::read_dir(dir)
                .map(|entries| {
                    entries
                        .flatten()
                        .filter_map(|e| {
                            let meta = e.metadata().ok().filter(|m| m.is_file())?;
                            let name = e.file_name().to_string_lossy().to_lowercase();
                            Some((name, (meta.len(), e.path())))
                        })
                        .collect()
                })
                .unwrap_or_default();
            for name in names {
                let hit = folder_sound_candidates(name).into_iter().find_map(|c| {
                    let (size, path) = if c.contains('/') {
                        let path = dir.join(&c);
                        (fs::metadata(&path).ok()?.len(), path)
                    } else {
                        listing.get(&c.to_lowercase()).cloned()?
                    };
                    Some(FileAt {
                        size,
                        ext: extension(&c),
                        path: path.to_string_lossy().into_owned(),
                    })
                });
                if let Some(hit) = hit {
                    out.insert(name.to_lowercase(), hit);
                }
            }
        }
        PlaceAt::Package { bmsp, dir } => {
            let Ok(pkg) = bms_package::PackageReader::open_file(bmsp) else {
                return Files::default();
            };
            let sizes: HashMap<&str, u64> = pkg
                .entries()
                .iter()
                .map(|e| (e.path.as_str(), e.size))
                .collect();
            for name in names {
                if let Some(path) = pkg.find_entry_path(dir, name) {
                    if let Some(&size) = sizes.get(path.as_str()) {
                        let ext = extension(&path);
                        out.insert(name.to_lowercase(), FileAt { size, ext, path });
                    }
                }
            }
        }
    }
    Files(out)
}

/// The headers of some files at a place, by name.
fn infos(at: &PlaceAt, files: &Files, names: &[&String]) -> HashMap<String, AudioInfo> {
    let mut out = HashMap::new();
    let mut pkg = match at {
        PlaceAt::Package { bmsp, .. } => bms_package::PackageReader::open_file(bmsp).ok(),
        PlaceAt::Folder(_) => None,
    };
    for name in names {
        let Some(file) = files.0.get(*name) else {
            continue;
        };
        let head = match &mut pkg {
            Some(pkg) => pkg.read_entry(&file.path).ok(),
            None => fs::File::open(&file.path).ok().and_then(|f| {
                let mut head = Vec::new();
                f.take(audio_info::PROBE_BYTES as u64)
                    .read_to_end(&mut head)
                    .ok()?;
                Some(head)
            }),
        };
        if let Some(info) = head.as_deref().and_then(audio_info::probe) {
            out.insert((*name).clone(), info);
        }
    }
    out
}

/// What a set's files are, from the header of the first one
/// (`AudioInfo::describe`), or their extension when it cannot be read.
pub fn describe(at: &PlaceAt, files: &Files) -> String {
    let Some(first) = files.0.keys().min() else {
        return String::new();
    };
    match infos(at, files, &[first]).get(first) {
        Some(info) => info.describe(),
        None => files.0[first].ext.to_uppercase(),
    }
}

/// How many shared files a comparison reads at most, spread over the names.
const SAMPLE: usize = 32;

/// Whether set `a` is clearly better than set `b`, from a spread sample of
/// the files both have.
pub fn clearly_better(a: (&PlaceAt, &Files), b: (&PlaceAt, &Files)) -> bool {
    let mut shared: Vec<&String> = a.1 .0.keys().filter(|n| b.1 .0.contains_key(*n)).collect();
    shared.sort();
    let step = shared.len().div_ceil(SAMPLE).max(1);
    let sample: Vec<&String> = shared.into_iter().step_by(step).collect();
    let infos_a = infos(a.0, a.1, &sample);
    let infos_b = infos(b.0, b.1, &sample);
    let mut compared = 0;
    let mut wins = 0;
    for name in &sample {
        if let (Some(x), Some(y)) = (infos_a.get(*name), infos_b.get(*name)) {
            compared += 1;
            wins += usize::from(audio_info::better(x, y));
        }
    }
    compared > 0 && wins * 100 >= compared * 95
}

/// One set among a song's places.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Set {
    /// The first place that has it.
    pub place: String,
    /// What its files are (`describe`).
    pub description: String,
    /// Places with the same files (`place` first).
    pub same: Vec<String>,
    /// Another set is clearly better.
    pub worse: bool,
}

/// The sets a chart declaring `names` can play with among `places` (key and
/// where), in the given order, and the places that are not one of them
/// (difference folders, or sets with other names: B18 plan 4.4).
pub fn compare(places: &[(String, PlaceAt)], names: &[String]) -> (Vec<Set>, Vec<String>) {
    let mut found: Vec<(Set, &PlaceAt, Files)> = Vec::new();
    let mut partial = Vec::new();
    for (key, at) in places {
        let files = files_at(at, names);
        if !files.whole(names) {
            partial.push(key.clone());
            continue;
        }
        if let Some((set, _, _)) = found.iter_mut().find(|(_, _, f)| f.same(&files)) {
            set.same.push(key.clone());
            continue;
        }
        let set = Set {
            place: key.clone(),
            description: describe(at, &files),
            same: vec![key.clone()],
            worse: false,
        };
        found.push((set, at, files));
    }
    let worse: Vec<bool> = (0..found.len())
        .map(|i| {
            (0..found.len()).any(|j| {
                let (a, b) = (&found[j], &found[i]);
                j != i && clearly_better((a.1, &a.2), (b.1, &b.2))
            })
        })
        .collect();
    for ((set, _, _), worse) in found.iter_mut().zip(worse) {
        set.worse = worse;
    }
    let sets = found.into_iter().map(|(set, _, _)| set).collect();
    (sets, partial)
}

/// The places a cleanup may remove: those of the worse sets whose every chart
/// has a copy at a place that stays. `charts` lists each place's charts.
/// The places of `partial` (`compare`) borrow key sounds: nothing goes unless
/// each still finds 90% of what it lacks at one whole place that stays
/// (`places` says where each place is, `names` what the chart declares).
pub fn going<'a>(
    sets: &'a [Set],
    partial: &[String],
    places: &[(String, PlaceAt)],
    names: &[String],
    charts: &BTreeMap<String, Vec<ChartId>>,
) -> Vec<&'a str> {
    let out = going_whole(sets, charts);
    if out.is_empty() || partial.is_empty() {
        return out;
    }
    let at = |key: &str| places.iter().find(|(k, _)| k == key).map(|(_, a)| a);
    let kept: Vec<&PlaceAt> = sets
        .iter()
        .flat_map(|s| &s.same)
        .filter(|k| !out.contains(&k.as_str()))
        .filter_map(|k| at(k))
        .collect();
    let covered = partial.iter().all(|p| {
        let Some(own) = at(p) else {
            return false;
        };
        let missing = files_at(own, names).missing(names);
        missing.is_empty() || kept.iter().any(|k| files_at(k, &missing).whole(&missing))
    });
    if covered {
        out
    } else {
        Vec::new()
    }
}

/// A place of a worse set that keeps its charts and loses the key sounds a
/// better set that stays has (B18 plan 4.5). Its charts then play with that
/// set's files, as a difference folder's do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Strip {
    pub place: String,
    /// The files that go: paths in a folder, entry paths in a package.
    pub files: Vec<String>,
    /// Their bytes in all.
    pub bytes: u64,
    /// The charts at the better set's places: the place's base once it
    /// keeps only charts (`chart_base`, B20).
    pub base: Vec<ChartId>,
}

/// The places of the worse sets that cannot go (`going`) because some chart
/// there is found nowhere else: each loses the files the first better set
/// also has. As with `going`, nothing is stripped unless every borrowing
/// place still finds what it lacks at a better set.
pub fn stripping(
    sets: &[Set],
    partial: &[String],
    places: &[(String, PlaceAt)],
    names: &[String],
    charts: &BTreeMap<String, Vec<ChartId>>,
) -> Vec<Strip> {
    let at = |key: &str| places.iter().find(|(k, _)| k == key).map(|(_, a)| a);
    let Some(better) = sets.iter().find(|s| !s.worse).and_then(|s| at(&s.place)) else {
        return Vec::new();
    };
    let going = going(sets, partial, places, names, charts);
    let stuck: Vec<&str> = sets
        .iter()
        .filter(|s| s.worse)
        .flat_map(|s| s.same.iter().map(String::as_str))
        .filter(|k| !going.contains(k) && charts.contains_key(*k))
        .collect();
    if stuck.is_empty() {
        return Vec::new();
    }
    let kept: Vec<&PlaceAt> = sets
        .iter()
        .filter(|s| !s.worse)
        .flat_map(|s| &s.same)
        .filter_map(|k| at(k))
        .collect();
    let covered = partial.iter().all(|p| {
        let Some(own) = at(p) else {
            return false;
        };
        let missing = files_at(own, names).missing(names);
        missing.is_empty() || kept.iter().any(|k| files_at(k, &missing).whole(&missing))
    });
    if !covered {
        return Vec::new();
    }
    let theirs = files_at(better, names);
    let base: Vec<ChartId> = sets
        .iter()
        .filter(|s| !s.worse)
        .take(1)
        .flat_map(|s| &s.same)
        .filter_map(|k| charts.get(k))
        .flatten()
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut out = Vec::new();
    for key in stuck {
        let Some(own) = at(key) else {
            continue;
        };
        let mut files: Vec<(&String, &FileAt)> = Vec::new();
        let mine = files_at(own, names);
        for (name, file) in &mine.0 {
            if theirs.0.contains_key(name) {
                files.push((name, file));
            }
        }
        if files.is_empty() {
            continue;
        }
        files.sort_by(|a, b| a.0.cmp(b.0));
        out.push(Strip {
            place: key.to_string(),
            bytes: files.iter().map(|(_, f)| f.size).sum(),
            files: files.into_iter().map(|(_, f)| f.path.clone()).collect(),
            base: base.clone(),
        });
    }
    out
}

/// Moves some files of `dir` into `.bpm-trash/<dir name>` beside `dir`
/// (`trash_folder`'s trash), keeping their place under `dir`. A name taken
/// there gets a free one.
pub fn trash_files(dir: &Path, files: &[String]) -> std::io::Result<PathBuf> {
    let parent = dir.parent().unwrap_or_else(|| Path::new("."));
    let name = dir.file_name().unwrap_or_default().to_string_lossy();
    let trash = parent.join(crate::collection::TRASH_DIR).join(&*name);
    for file in files {
        let from = Path::new(file);
        let inner = from.strip_prefix(dir).unwrap_or(from);
        let mut target = trash.join(inner);
        if let Some(up) = target.parent() {
            fs::create_dir_all(up)?;
        }
        let mut n = 1;
        while target.exists() {
            n += 1;
            let stem = inner.file_stem().unwrap_or_default().to_string_lossy();
            let ext = inner.extension().map(|e| e.to_string_lossy());
            let free = match ext {
                Some(ext) => format!("{stem} ({n}).{ext}"),
                None => format!("{stem} ({n})"),
            };
            target.set_file_name(free);
        }
        fs::rename(from, &target)?;
    }
    Ok(trash)
}

/// The package places of a set that a library folder also holds, whose
/// charts are all at that set's folders (B21): copies a cleanup may remove,
/// as the folder stays. A place with files the folder lacks besides charts
/// and key sounds (pictures, video, a BGA companion) does not go: it comes
/// back second with what the folder lacks.
pub fn copies<'a>(
    sets: &'a [Set],
    places: &[(String, PlaceAt)],
    charts: &BTreeMap<String, Vec<ChartId>>,
) -> (Vec<&'a str>, Vec<(&'a str, Vec<String>)>) {
    let at = |key: &str| places.iter().find(|(k, _)| k == key).map(|(_, a)| a);
    let (mut out, mut kept) = (Vec::new(), Vec::new());
    for set in sets {
        let folders: Vec<(&str, &Path)> = set
            .same
            .iter()
            .filter_map(|k| match at(k)? {
                PlaceAt::Folder(dir) => Some((k.as_str(), dir.as_path())),
                PlaceAt::Package { .. } => None,
            })
            .collect();
        let Some(&(_, folder)) = folders.first() else {
            continue;
        };
        let in_folders: HashSet<ChartId> = folders
            .iter()
            .filter_map(|(k, _)| charts.get(*k))
            .flatten()
            .copied()
            .collect();
        for key in &set.same {
            let Some(own @ PlaceAt::Package { .. }) = at(key) else {
                continue;
            };
            let all_there = charts
                .get(key)
                .is_some_and(|c| c.iter().all(|c| in_folders.contains(c)));
            if !all_there {
                continue;
            }
            let lacking = lacking_extras(own, folder);
            if lacking.is_empty() {
                out.push(key.as_str());
            } else {
                kept.push((key.as_str(), lacking));
            }
        }
    }
    out.sort();
    (out, kept)
}

/// The files at a place besides charts and key sounds: path relative to the
/// place (`/` separated) and size. A folder's are found under it, a
/// package place's under its directory.
fn extras(at: &PlaceAt) -> BTreeMap<String, u64> {
    let keep = |rel: &str| {
        rel != bms_package::MANIFEST_FILENAME
            && !crate::collection::is_chart_path(rel)
            && !SOUND_EXTENSIONS.contains(&extension(rel).as_str())
    };
    let mut out = BTreeMap::new();
    match at {
        PlaceAt::Folder(dir) => {
            let mut stack = vec![dir.clone()];
            while let Some(d) = stack.pop() {
                for entry in fs::read_dir(&d).into_iter().flatten().flatten() {
                    let path = entry.path();
                    let Ok(meta) = entry.metadata() else {
                        continue;
                    };
                    if meta.is_dir() {
                        stack.push(path);
                        continue;
                    }
                    let Ok(rel) = path.strip_prefix(dir) else {
                        continue;
                    };
                    let rel = rel.to_string_lossy().replace('\\', "/");
                    if keep(&rel) {
                        out.insert(rel, meta.len());
                    }
                }
            }
        }
        PlaceAt::Package { bmsp, dir } => {
            let Ok(pkg) = bms_package::PackageReader::open_file(bmsp) else {
                return out;
            };
            let prefix = if dir.is_empty() {
                String::new()
            } else {
                format!("{dir}/")
            };
            for e in pkg.entries() {
                if let Some(rel) = e.path.strip_prefix(&prefix) {
                    if keep(rel) {
                        out.insert(rel.to_string(), e.size);
                    }
                }
            }
        }
    }
    out
}

/// The files of `own` besides charts and key sounds that `other` has too,
/// at the same path with the same size: paths relative to `own`.
pub fn shared_extras(own: &PlaceAt, other: &PlaceAt) -> Vec<String> {
    let theirs = extras(other);
    extras(own)
        .into_iter()
        .filter(|(rel, size)| theirs.get(rel) == Some(size))
        .map(|(rel, _)| rel)
        .collect()
}

/// The files of package place `own`, besides charts and key sounds, that
/// `folder` does not have at the same path; a BGA companion beside the
/// package counts as one.
fn lacking_extras(own: &PlaceAt, folder: &Path) -> Vec<String> {
    let PlaceAt::Package { bmsp, dir } = own else {
        return Vec::new();
    };
    let Ok(pkg) = bms_package::PackageReader::open_file(bmsp) else {
        return vec![bmsp.to_string_lossy().into_owned()];
    };
    let prefix = if dir.is_empty() {
        String::new()
    } else {
        format!("{dir}/")
    };
    let mut out: Vec<String> = pkg
        .entries()
        .iter()
        .filter_map(|e| e.path.strip_prefix(&prefix))
        .filter(|rel| *rel != bms_package::MANIFEST_FILENAME)
        .filter(|rel| !crate::collection::is_chart_path(rel))
        .filter(|rel| !SOUND_EXTENSIONS.contains(&extension(rel).as_str()))
        .filter(|rel| !folder.join(rel).is_file())
        .map(str::to_string)
        .collect();
    let companion = bmsp
        .parent()
        .and_then(|d| fs::read_dir(d).ok())
        .and_then(|mut entries| {
            entries.find_map(|e| {
                let name = e.ok()?.file_name().to_string_lossy().into_owned();
                name.ends_with(".bga.bmsp").then_some(name)
            })
        });
    out.extend(companion);
    out
}

/// [`going`] without the borrowing places.
fn going_whole<'a>(sets: &'a [Set], charts: &BTreeMap<String, Vec<ChartId>>) -> Vec<&'a str> {
    let worse: HashSet<&str> = sets
        .iter()
        .filter(|s| s.worse)
        .flat_map(|s| s.same.iter().map(String::as_str))
        .collect();
    let kept: HashSet<ChartId> = charts
        .iter()
        .filter(|(k, _)| !worse.contains(k.as_str()))
        .flat_map(|(_, c)| c.iter().copied())
        .collect();
    let mut out: Vec<&str> = worse
        .into_iter()
        .filter(|k| {
            charts
                .get(*k)
                .is_some_and(|c| c.iter().all(|c| kept.contains(c)))
        })
        .collect();
    out.sort();
    out
}

/// Moves a folder into `.bpm-trash` in its parent folder, under a free name.
pub fn trash_folder(dir: &Path) -> std::io::Result<PathBuf> {
    let parent = dir.parent().unwrap_or_else(|| Path::new("."));
    let trash = parent.join(crate::collection::TRASH_DIR);
    fs::create_dir_all(&trash)?;
    let name = dir.file_name().unwrap_or_default().to_string_lossy();
    let mut target = trash.join(&*name);
    let mut n = 1;
    while target.exists() {
        n += 1;
        target = trash.join(format!("{name} ({n})"));
    }
    fs::rename(dir, &target)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav_bytes(rate: u32) -> Vec<u8> {
        let mut b = b"RIFF\0\0\0\0WAVEfmt \x10\0\0\0".to_vec();
        b.extend(1u16.to_le_bytes());
        b.extend(2u16.to_le_bytes());
        b.extend(rate.to_le_bytes());
        b.extend((rate * 4).to_le_bytes());
        b.extend(4u16.to_le_bytes());
        b.extend(16u16.to_le_bytes());
        b
    }

    fn ogg_bytes(rate: u32) -> Vec<u8> {
        let mut b = b"OggS".to_vec();
        b.extend([0u8; 22]);
        b.extend([1, 30]);
        b.extend(b"\x01vorbis\0\0\0\0\x02");
        b.extend(rate.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        b.extend(192_000u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        b
    }

    #[test]
    fn worse_sets_are_marked_and_go_only_when_their_charts_stay() {
        let root = std::env::temp_dir().join(format!("bpm-key-sets-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mk = |dir: &str, files: &[(&str, Vec<u8>)]| {
            let d = root.join(dir);
            fs::create_dir_all(&d).unwrap();
            for (name, bytes) in files {
                fs::write(d.join(name), bytes).unwrap();
            }
            (dir.to_string(), PlaceAt::Folder(d))
        };
        let places = vec![
            mk(
                "copy",
                &[("a.wav", wav_bytes(44100)), ("b.wav", wav_bytes(44100))],
            ),
            mk(
                "ogg",
                &[("a.ogg", ogg_bytes(44100)), ("b.ogg", ogg_bytes(44100))],
            ),
            mk(
                "wav",
                &[("a.wav", wav_bytes(44100)), ("b.wav", wav_bytes(44100))],
            ),
        ];
        let names = vec!["a.wav".to_string(), "b.wav".to_string()];
        let (sets, partial) = compare(&places, &names);
        let shown: Vec<(&str, &str, bool)> = sets
            .iter()
            .map(|s| (s.place.as_str(), s.description.as_str(), s.worse))
            .collect();
        assert_eq!(
            shown,
            [
                ("copy", "WAV 44.1kHz/16bit", false),
                ("ogg", "OGG 44.1kHz 192kbps", true)
            ]
        );
        assert_eq!(sets[0].same, ["copy", "wav"]);
        assert!(partial.is_empty());

        let id = |n: u8| ChartId::of_bytes(&[n]);
        let mut charts = BTreeMap::from([
            ("copy".to_string(), vec![id(1)]),
            ("ogg".to_string(), vec![id(2)]),
            ("wav".to_string(), vec![id(1)]),
        ]);
        let go = |partial: &[String], places: &[(String, PlaceAt)], charts: &BTreeMap<_, _>| {
            going(&sets, partial, places, &names, charts)
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        assert!(go(&[], &places, &charts).is_empty());
        // Its chart 2 is found nowhere else: it keeps that and loses the
        // key sounds the WAV set has.
        let strips = stripping(&sets, &[], &places, &names, &charts);
        assert_eq!(strips.len(), 1);
        assert_eq!(strips[0].place, "ogg");
        let PlaceAt::Folder(ogg_dir) = &places[1].1 else {
            unreachable!()
        };
        let in_ogg = |n: &str| ogg_dir.join(n).to_string_lossy().into_owned();
        assert_eq!(strips[0].files, [in_ogg("a.ogg"), in_ogg("b.ogg")]);
        assert_eq!(strips[0].bytes, 2 * ogg_bytes(44100).len() as u64);
        // A borrowing place the WAV sets do not cover stops it.
        let mut odd = places.clone();
        odd.push(mk("odd", &[("a.wav", wav_bytes(44100))]));
        let only_ogg = vec!["a.wav".to_string(), "c.wav".to_string()];
        fs::write(ogg_dir.join("c.ogg"), ogg_bytes(44100)).unwrap();
        assert!(stripping(&sets, &["odd".into()], &odd, &only_ogg, &charts).is_empty());
        fs::remove_file(ogg_dir.join("c.ogg")).unwrap();
        fs::remove_dir_all(root.join("odd")).unwrap();
        charts.insert("ogg".into(), vec![id(1)]);
        // Once the place may go, it is not stripped.
        assert!(stripping(&sets, &[], &places, &names, &charts).is_empty());
        assert_eq!(go(&[], &places, &charts), ["ogg"]);

        // A difference folder with only `a.wav` borrows `b.wav`: the WAV
        // sets that stay still have it, so the OGG set may go.
        let mut with_diff = places.clone();
        with_diff.push(mk("diff", &[("a.wav", wav_bytes(44100))]));
        let (_, partial) = compare(&with_diff, &names);
        assert_eq!(partial, ["diff"]);
        assert_eq!(go(&partial, &with_diff, &charts), ["ogg"]);
        // Where the borrowing place is not known, nothing goes.
        assert!(go(&partial, &places, &charts).is_empty());

        let PlaceAt::Folder(dir) = &places[1].1 else {
            unreachable!()
        };
        let moved = trash_folder(dir).unwrap();
        assert!(!dir.exists() && moved.join("a.ogg").is_file());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_package_copy_of_a_folder_goes_unless_the_folder_lacks_something() {
        let root = std::env::temp_dir().join(format!("bpm-copies-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let folder = root.join("folder");
        fs::create_dir_all(&folder).unwrap();
        for f in ["a.wav", "b.wav"] {
            fs::write(folder.join(f), wav_bytes(44100)).unwrap();
        }
        fs::write(folder.join("x.bms"), "#TITLE X").unwrap();
        let package = |name: &str, extra: &[(&str, &[u8])]| {
            let mut b = bms_package::PackageBuilder::new(bms_package::Manifest::new(name, name));
            b.add_file("song/x.bms", "#TITLE X").unwrap();
            b.add_file("song/a.wav", wav_bytes(44100)).unwrap();
            b.add_file("song/b.wav", wav_bytes(44100)).unwrap();
            for (path, bytes) in extra {
                b.add_file(format!("song/{path}"), bytes.to_vec()).unwrap();
            }
            let dir = root.join(name);
            fs::create_dir_all(&dir).unwrap();
            let bmsp = dir.join("package.bmsp");
            b.build_to_file(&bmsp).unwrap();
            (
                format!("{name}::song"),
                PlaceAt::Package {
                    bmsp,
                    dir: "song".into(),
                },
            )
        };
        let places = vec![
            ("folder".to_string(), PlaceAt::Folder(folder.clone())),
            package("plain", &[]),
            package("picture", &[("back.png", b"png")]),
            package("more", &[]),
        ];
        let names = vec!["a.wav".to_string(), "b.wav".to_string()];
        let (sets, _) = compare(&places, &names);
        assert_eq!(sets.len(), 1);
        let id = |n: u8| ChartId::of_bytes(&[n]);
        let charts = BTreeMap::from([
            ("folder".to_string(), vec![id(1)]),
            ("plain::song".to_string(), vec![id(1)]),
            ("picture::song".to_string(), vec![id(1)]),
            ("more::song".to_string(), vec![id(1), id(2)]),
        ]);
        let (going, kept) = copies(&sets, &places, &charts);
        assert_eq!(going, ["plain::song"]);
        assert_eq!(kept, [("picture::song", vec!["back.png".to_string()])]);
        // Once the folder has the picture too, that copy goes as well.
        fs::write(folder.join("back.png"), "png").unwrap();
        let (going, kept) = copies(&sets, &places, &charts);
        assert_eq!(going, ["picture::song", "plain::song"]);
        assert!(kept.is_empty());
        // A BGA companion beside the package keeps it.
        fs::write(root.join("plain").join("plain.bga.bmsp"), "bga").unwrap();
        let (going, kept) = copies(&sets, &places, &charts);
        assert_eq!(going, ["picture::song"]);
        assert_eq!(kept, [("plain::song", vec!["plain.bga.bmsp".to_string()])]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stripped_files_move_into_the_trash_under_their_folder() {
        let root = std::env::temp_dir().join(format!("bpm-strip-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let dir = root.join("song");
        fs::create_dir_all(dir.join("snd")).unwrap();
        for f in ["a.ogg", "snd/b.ogg", "chart.bms"] {
            fs::write(dir.join(f), f).unwrap();
        }
        let files = |names: &[&str]| -> Vec<String> {
            names
                .iter()
                .map(|n| dir.join(n).to_string_lossy().into_owned())
                .collect()
        };
        let trash = trash_files(&dir, &files(&["a.ogg", "snd/b.ogg"])).unwrap();
        assert!(dir.join("chart.bms").is_file());
        assert!(!dir.join("a.ogg").exists() && !dir.join("snd/b.ogg").exists());
        assert!(trash.join("a.ogg").is_file() && trash.join("snd/b.ogg").is_file());
        // A second strip of the same name keeps the first one.
        fs::write(dir.join("a.ogg"), "again").unwrap();
        trash_files(&dir, &files(&["a.ogg"])).unwrap();
        assert_eq!(
            fs::read_to_string(trash.join("a (2).ogg")).unwrap(),
            "again"
        );
        let _ = fs::remove_dir_all(&root);
    }
}

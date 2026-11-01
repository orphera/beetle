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
use beetle_core::key_sounds::folder_sound_candidates;
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

    /// The found files' bytes in all.
    pub fn total_size(&self) -> u64 {
        self.0.values().map(|f| f.size).sum()
    }

    /// Whether two places hold the same files: every name both have is the
    /// same size, and they have at least one name in common.
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
/// where), in the given order, and whether every place is one of them.
pub fn compare(places: &[(String, PlaceAt)], names: &[String]) -> (Vec<Set>, bool) {
    let mut found: Vec<(Set, &PlaceAt, Files)> = Vec::new();
    let mut whole_places = 0;
    for (key, at) in places {
        let files = files_at(at, names);
        if !files.whole(names) {
            continue;
        }
        whole_places += 1;
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
    (sets, whole_places == places.len())
}

/// The places a cleanup may remove: those of the worse sets whose every chart
/// has a copy at a place that stays. Nothing when some place is not a whole
/// set (`all_whole` false), since a difference folder may draw on any of them.
/// `charts` lists each place's charts.
pub fn going<'a>(
    sets: &'a [Set],
    all_whole: bool,
    charts: &BTreeMap<String, Vec<ChartId>>,
) -> Vec<&'a str> {
    if !all_whole {
        return Vec::new();
    }
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
        let (sets, all_whole) = compare(&places, &names);
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
        assert!(all_whole);

        let id = |n: u8| ChartId::of_bytes(&[n]);
        let mut charts = BTreeMap::from([
            ("copy".to_string(), vec![id(1)]),
            ("ogg".to_string(), vec![id(2)]),
            ("wav".to_string(), vec![id(1)]),
        ]);
        assert!(going(&sets, all_whole, &charts).is_empty());
        charts.insert("ogg".into(), vec![id(1)]);
        assert_eq!(going(&sets, all_whole, &charts), ["ogg"]);
        assert!(going(&sets, false, &charts).is_empty());

        let PlaceAt::Folder(dir) = &places[1].1 else {
            unreachable!()
        };
        let moved = trash_folder(dir).unwrap();
        assert!(!dir.exists() && moved.join("a.ogg").is_file());
        let _ = fs::remove_dir_all(&root);
    }
}

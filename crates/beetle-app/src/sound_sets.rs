//! Key sound sets of a song (B17, plan P2 in
//! docs/plans/2026-11-01-song-grouping-plan.md).
//!
//! A song's places are the folders (or folders inside packages) of its
//! charts. The loader takes key sounds from the chart's own place, the set
//! the player chose, and then the other places (`loader::SoundPlaces`). The
//! song menu lists the sets a chart can play with: places that have nearly all
//! the names the chart declares, one per set of different files. The choice is
//! kept in `sound_sets.dat`, under the song's first place.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use beetle_core::key_sounds::folder_sound_candidates;
use beetle_core::msgpack::{self, Value, Writer};
use beetle_core::{SongAddress, SongGroups, SongMetadata};

use crate::loader::SoundPlaces;

/// A place's key: the same text for every chart there. `None` for a disc song.
pub fn place_of(file_path: &str) -> Option<String> {
    match SongAddress::parse(file_path) {
        SongAddress::Folder { file } => Some(parent(file).to_string()),
        SongAddress::Package { bmsp, entry } => Some(format!("{bmsp}::{}", parent(entry))),
        SongAddress::Disc { .. } => None,
    }
}

fn parent(path: &str) -> &str {
    path.rfind(['/', '\\']).map_or("", |i| &path[..i])
}

/// The places of `song`'s song by key, each with the address of a chart
/// there. The song's own place is one of them.
pub fn places(
    songs: &[SongMetadata],
    groups: &SongGroups,
    song: &SongMetadata,
) -> BTreeMap<String, String> {
    let group = groups.get(&song.id);
    let mut out = BTreeMap::new();
    for s in songs
        .iter()
        .filter(|s| s.id == song.id || (group.is_some() && groups.get(&s.id) == group))
    {
        if let Some(place) = place_of(&s.file_path) {
            out.entry(place).or_insert_with(|| s.file_path.clone());
        }
    }
    out
}

/// Where the loader takes `song`'s key sounds from besides the chart's own place.
pub fn plan(
    songs: &[SongMetadata],
    groups: &SongGroups,
    choices: &Choices,
    song: &SongMetadata,
) -> SoundPlaces {
    let Some(own) = place_of(&song.file_path) else {
        return SoundPlaces::default();
    };
    let places = places(songs, groups, song);
    let chosen = places
        .keys()
        .next()
        .and_then(|first| choices.0.get(first))
        .filter(|c| **c != own && places.contains_key(*c));
    SoundPlaces {
        chosen: chosen.map(|c| places[c].clone()),
        others: places
            .iter()
            .filter(|(k, _)| **k != own && Some(*k) != chosen)
            .map(|(_, address)| address.clone())
            .collect(),
    }
}

/// A set the menu offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundSet {
    pub place: String,
    /// The place's folder name and the files' format, e.g. `xi · WAV`.
    pub label: String,
}

/// A place's files for some declared names, by lowercased name: size and extension.
type Files = HashMap<String, (u64, String)>;

/// The sets `song`'s chart can play with, in place order. A place is a set
/// for the chart when it has 90% of the names the chart declares. Places
/// whose shared files all have the same sizes are one set (the first is kept).
pub fn sets(songs: &[SongMetadata], groups: &SongGroups, song: &SongMetadata) -> Vec<SoundSet> {
    let Ok(bytes) = crate::loader::read_chart_bytes(&song.file_path) else {
        return Vec::new();
    };
    let Some((_, mut names)) = SongMetadata::from_bytes_with_sounds(&song.file_path, &bytes) else {
        return Vec::new();
    };
    names.sort();
    names.dedup();
    let mut found: Vec<(SoundSet, Files)> = Vec::new();
    for (place, address) in places(songs, groups, song) {
        let files = files_at(&address, &names);
        if names.is_empty() || files.len() * 10 < names.len() * 9 {
            continue;
        }
        if found.iter().any(|(_, f)| same_files(f, &files)) {
            continue;
        }
        let label = format!("{} · {}", place_name(&place), format_of(&files));
        found.push((SoundSet { place, label }, files));
    }
    found.into_iter().map(|(set, _)| set).collect()
}

/// Whether two places hold the same files: every name both have is the same
/// size, and they have at least one name in common.
fn same_files(a: &Files, b: &Files) -> bool {
    let mut shared = 0;
    for (name, (size, _)) in a {
        if let Some((other, _)) = b.get(name) {
            if size != other {
                return false;
            }
            shared += 1;
        }
    }
    shared > 0
}

/// The last folder name of a place (for a package's root, the package file's name).
fn place_name(place: &str) -> &str {
    let (pkg, inner) = place.split_once("::").unwrap_or(("", place));
    let inner = inner.rsplit(['/', '\\']).next().unwrap_or("");
    if !inner.is_empty() {
        return inner;
    }
    Path::new(pkg)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(pkg)
}

/// The most common extension of the files, upper case.
fn format_of(files: &Files) -> String {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, ext) in files.values() {
        *counts.entry(ext).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|&(_, n)| n)
        .map_or(String::new(), |(ext, _)| ext.to_uppercase())
}

fn extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
}

/// The files at a chart's address for `names`, found as the loader finds them.
fn files_at(address: &str, names: &[String]) -> Files {
    let mut out = Files::new();
    match SongAddress::parse(address) {
        SongAddress::Folder { file } => {
            let dir = Path::new(file).parent().unwrap_or_else(|| Path::new("."));
            let listing: HashMap<String, u64> = fs::read_dir(dir)
                .map(|entries| {
                    entries
                        .flatten()
                        .filter_map(|e| {
                            let meta = e.metadata().ok().filter(|m| m.is_file())?;
                            Some((e.file_name().to_string_lossy().to_lowercase(), meta.len()))
                        })
                        .collect()
                })
                .unwrap_or_default();
            for name in names {
                let hit = folder_sound_candidates(name).into_iter().find_map(|c| {
                    let size = if c.contains('/') {
                        fs::metadata(dir.join(&c)).ok().map(|m| m.len())
                    } else {
                        listing.get(&c.to_lowercase()).copied()
                    }?;
                    Some((size, extension(&c)))
                });
                if let Some(hit) = hit {
                    out.insert(name.to_lowercase(), hit);
                }
            }
        }
        SongAddress::Package { bmsp, entry } => {
            let Ok(pkg) = bms_package::PackageReader::open_file(bmsp) else {
                return out;
            };
            let sizes: HashMap<&str, u64> = pkg
                .entries()
                .iter()
                .map(|e| (e.path.as_str(), e.size))
                .collect();
            for name in names {
                if let Some(path) = pkg.find_entry_path(parent(entry), name) {
                    if let Some(&size) = sizes.get(path.as_str()) {
                        out.insert(name.to_lowercase(), (size, extension(&path)));
                    }
                }
            }
        }
        SongAddress::Disc { .. } => {}
    }
    out
}

// ---------------------------------------------------------------------------
// sound_sets.dat
// ---------------------------------------------------------------------------

/// The chosen set of each song: the song's first place to the chosen place.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Choices(pub HashMap<String, String>);

const FORMAT: &str = "beetle-sound-sets";
const VERSION: u64 = 1;

fn choices_file() -> PathBuf {
    std::env::var("BEETLE_SOUND_SETS_FILE")
        .map_or_else(|_| PathBuf::from("sound_sets.dat"), PathBuf::from)
}

impl Choices {
    pub fn parse(bytes: &[u8]) -> Self {
        let Some(file) = msgpack::decode_file(bytes, FORMAT, VERSION) else {
            return Self::default();
        };
        let pairs = file
            .get("choices")
            .and_then(Value::as_array)
            .unwrap_or_default();
        Choices(
            pairs
                .iter()
                .filter_map(|pair| match pair.as_array()? {
                    [song, chosen] => {
                        Some((song.as_str()?.to_string(), chosen.as_str()?.to_string()))
                    }
                    _ => None,
                })
                .collect(),
        )
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut pairs: Vec<_> = self.0.iter().collect();
        pairs.sort();
        let mut w = Writer::new();
        w.file(FORMAT, VERSION, 1);
        w.str("choices");
        w.array(pairs.len());
        for (song, chosen) in pairs {
            w.array(2);
            w.str(song);
            w.str(chosen);
        }
        w.into_bytes()
    }

    pub fn load() -> Self {
        Self::parse(&fs::read(choices_file()).unwrap_or_default())
    }

    pub fn save(&self) -> std::io::Result<()> {
        beetle_core::write_atomic(choices_file(), self.serialize())
    }

    /// The chosen place for `song`, or the first set (the song's own order).
    pub fn chosen<'a>(&'a self, places: &'a BTreeMap<String, String>) -> Option<&'a str> {
        let first = places.keys().next()?;
        self.0
            .get(first)
            .filter(|c| places.contains_key(*c))
            .map(String::as_str)
    }

    /// Chooses `place` for the song whose places are `places`.
    pub fn choose(&mut self, places: &BTreeMap<String, String>, place: &str) {
        if let Some(first) = places.keys().next() {
            self.0.insert(first.clone(), place.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beetle_core::ChartId;

    fn song(path: &str, id: u8) -> SongMetadata {
        SongMetadata {
            id: ChartId::synthetic(id as u64),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: id as u64,
            file_path: path.to_string(),
            title: String::new(),
            subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            bpm: 120.0,
            bpm_min: 120.0,
            bpm_max: 120.0,
            play_level: 1,
            notes_count: 1,
            play_mode: beetle_core::PlayMode::Keys7,
            difficulty: None,
        }
    }

    #[test]
    fn a_place_is_the_chart_folder_or_the_folder_in_the_package() {
        assert_eq!(place_of("D:\\bms\\a\\x.bms").as_deref(), Some("D:\\bms\\a"));
        let pkg = beetle_core::package_address("D:\\p\\s.bmsp", "song/x.bms");
        assert_eq!(place_of(&pkg).as_deref(), Some("D:\\p\\s.bmsp::song"));
    }

    #[test]
    fn the_plan_puts_the_chosen_set_first_and_skips_the_own_place() {
        let songs = vec![
            song("D:\\a\\1.bms", 1),
            song("D:\\b\\2.bms", 2),
            song("D:\\c\\3.bms", 3),
        ];
        let groups: SongGroups = songs.iter().map(|s| (s.id, 7)).collect();
        let mut choices = Choices::default();
        let plan_for = |c: &Choices| plan(&songs, &groups, c, &songs[0]);
        assert_eq!(plan_for(&choices).chosen, None);
        assert_eq!(plan_for(&choices).others, ["D:\\b\\2.bms", "D:\\c\\3.bms"]);

        choices.choose(&places(&songs, &groups, &songs[0]), "D:\\c");
        let p = plan_for(&choices);
        assert_eq!(p.chosen.as_deref(), Some("D:\\c\\3.bms"));
        assert_eq!(p.others, ["D:\\b\\2.bms"]);
        // From the chosen place itself nothing is swapped.
        assert_eq!(plan(&songs, &groups, &choices, &songs[2]).chosen, None);
    }

    #[test]
    fn choices_round_trip() {
        let mut c = Choices::default();
        c.0.insert("D:\\a".into(), "D:\\b".into());
        assert_eq!(Choices::parse(&c.serialize()), c);
        assert_eq!(Choices::parse(b"junk"), Choices::default());
    }

    #[test]
    fn sets_with_the_same_sizes_are_one() {
        let f = |pairs: &[(&str, u64)]| -> Files {
            pairs
                .iter()
                .map(|&(n, s)| (n.to_string(), (s, "wav".to_string())))
                .collect()
        };
        assert!(same_files(&f(&[("a", 1), ("b", 2)]), &f(&[("a", 1)])));
        assert!(!same_files(&f(&[("a", 1)]), &f(&[("a", 3)])));
        assert!(!same_files(&f(&[("a", 1)]), &f(&[("b", 1)])));
    }

    #[test]
    fn the_menu_offers_sets_that_differ_and_have_the_names() {
        let root = std::env::temp_dir().join(format!("beetle-sets-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let chart = "#WAV01 a.wav\n#WAV02 b.wav\n#00111:0102\n";
        let mk = |dir: &str, files: &[(&str, usize)]| {
            let d = root.join(dir);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("x.bms"), chart).unwrap();
            for (name, len) in files {
                fs::write(d.join(name), vec![0u8; *len]).unwrap();
            }
            d.join("x.bms").to_string_lossy().into_owned()
        };
        let wav = mk("wav", &[("a.wav", 10), ("b.wav", 20)]);
        let copy = mk("copy", &[("a.wav", 10), ("b.wav", 20)]);
        let ogg = mk("ogg", &[("a.ogg", 3), ("b.ogg", 4)]);
        let diff = mk("diff", &[]);
        let songs: Vec<SongMetadata> = [&copy, &diff, &ogg, &wav]
            .iter()
            .enumerate()
            .map(|(i, p)| song(p, i as u8))
            .collect();
        let groups: SongGroups = songs.iter().map(|s| (s.id, 1)).collect();
        let labels: Vec<String> = sets(&songs, &groups, &songs[1])
            .into_iter()
            .map(|s| s.label)
            .collect();
        // `copy` and `wav` are the same files; `diff` has none.
        assert_eq!(labels, ["copy · WAV", "ogg · OGG"]);
        let _ = fs::remove_dir_all(&root);
    }
}

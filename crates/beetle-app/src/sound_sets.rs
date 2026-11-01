//! Key sound sets of a song (B17, plan P2 and P2.1 in
//! docs/plans/2026-11-01-song-grouping-plan.md).
//!
//! A song's places are the folders (or folders inside packages) of its
//! charts, as the scan lists them (`SongPlaces`, every copy counted). The
//! loader takes key sounds from the chart's own place, the set the player
//! chose (else one clearly better than the own place's), and then the other
//! places (`loader::SoundPlaces`). The song menu lists the sets a chart can
//! play with, marks the clearly worse ones, and moves them to the trash on
//! request. The choice is kept in `sound_sets.dat`, under the song's first place.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use beetle_core::audio_info::{self, AudioInfo};
use beetle_core::key_sounds::folder_sound_candidates;
use beetle_core::msgpack::{self, Value, Writer};
use beetle_core::{SongAddress, SongGroups, SongMetadata, SongPlace, SongPlaces};

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

/// The places of `song`'s song by key. The song's own place is one of them.
pub fn places(
    groups: &SongGroups,
    song_places: &SongPlaces,
    song: &SongMetadata,
) -> BTreeMap<String, SongPlace> {
    let mut out = BTreeMap::new();
    let listed = groups.get(&song.id).and_then(|g| song_places.get(g));
    for place in listed.into_iter().flatten() {
        if let Some(key) = place_of(&place.address) {
            out.entry(key).or_insert_with(|| place.clone());
        }
    }
    if let Some(own) = place_of(&song.file_path) {
        out.entry(own).or_insert_with(|| SongPlace {
            address: song.file_path.clone(),
            charts: vec![song.id],
        });
    }
    out
}

/// Where the loader takes `song`'s key sounds from besides the chart's own place.
pub fn plan(
    groups: &SongGroups,
    song_places: &SongPlaces,
    choices: &Choices,
    song: &SongMetadata,
) -> SoundPlaces {
    let Some(own) = place_of(&song.file_path) else {
        return SoundPlaces::default();
    };
    let places = places(groups, song_places, song);
    let choice = choices.chosen(&places);
    let chosen = choice.filter(|c| *c != own);
    SoundPlaces {
        chosen: chosen.map(|c| places[c].address.clone()),
        others: places
            .iter()
            .filter(|(k, _)| **k != own && Some(k.as_str()) != chosen)
            .map(|(_, p)| p.address.clone())
            .collect(),
        decided: choice.is_some(),
    }
}

/// A set the menu offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundSet {
    pub place: String,
    /// The place's folder name and the files' format, e.g. `xi · WAV`.
    pub label: String,
    /// Places with the same files (`place` first).
    pub same: Vec<String>,
    /// Another set is clearly better (P2.1).
    pub worse: bool,
}

/// The sets of a chart, and whether every place of the song is one of them.
#[derive(Debug, Clone, Default)]
pub struct Sets {
    pub sets: Vec<SoundSet>,
    /// Every place of the song has a whole set for the chart: none of them is
    /// a difference folder (or a set with other names) that draws on the rest.
    pub all_whole: bool,
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
type Files = HashMap<String, FileAt>;

/// Whether a place has 90% of the names (P2).
fn whole(files: &Files, names: &[String]) -> bool {
    !names.is_empty() && files.len() * 10 >= names.len() * 9
}

/// The key sound names a chart declares, sorted.
fn declared(address: &str) -> Vec<String> {
    let Ok(bytes) = crate::loader::read_chart_bytes(address) else {
        return Vec::new();
    };
    let mut names = SongMetadata::from_bytes_with_sounds(address, &bytes)
        .map(|(_, names)| names)
        .unwrap_or_default();
    names.sort();
    names.dedup();
    names
}

/// The sets `song`'s chart can play with, in place order. A place is a set
/// for the chart when it has 90% of the names the chart declares. Places
/// whose shared files all have the same sizes are one set.
pub fn sets(groups: &SongGroups, song_places: &SongPlaces, song: &SongMetadata) -> Sets {
    let names = declared(&song.file_path);
    let places = places(groups, song_places, song);
    let mut found: Vec<(SoundSet, String, Files)> = Vec::new();
    let mut whole_places = 0;
    for (place, at) in &places {
        let files = files_at(&at.address, &names);
        if !whole(&files, &names) {
            continue;
        }
        whole_places += 1;
        if let Some((set, _, _)) = found.iter_mut().find(|(_, _, f)| same_files(f, &files)) {
            set.same.push(place.clone());
            continue;
        }
        let label = format!("{} · {}", place_name(place), describe(&at.address, &files));
        let set = SoundSet {
            place: place.clone(),
            label,
            same: vec![place.clone()],
            worse: false,
        };
        found.push((set, at.address.clone(), files));
    }
    let worse: Vec<bool> = (0..found.len())
        .map(|i| {
            (0..found.len()).any(|j| {
                let (a, b) = (&found[j], &found[i]);
                j != i && clearly_better((&a.1, &a.2), (&b.1, &b.2))
            })
        })
        .collect();
    for ((set, _, _), worse) in found.iter_mut().zip(worse) {
        set.worse = worse;
    }
    Sets {
        sets: found.into_iter().map(|(set, _, _)| set).collect(),
        all_whole: whole_places == places.len(),
    }
}

/// A set clearly better than the chart's own place, for a song whose player
/// has not chosen one: the first of `others` that is (P2.1). Reads only headers.
pub fn better_set(own: &str, others: &[String], names: &[String]) -> Option<String> {
    let mine = files_at(own, names);
    if !whole(&mine, names) {
        return None;
    }
    others
        .iter()
        .find(|other| {
            let theirs = files_at(other, names);
            whole(&theirs, names)
                && !same_files(&theirs, &mine)
                && clearly_better((other.as_str(), &theirs), (own, &mine))
        })
        .cloned()
}

/// How many shared files a comparison reads at most, spread over the names.
const SAMPLE: usize = 32;

/// Whether set `a` is clearly better than set `b` (P2.1): on 95% of the
/// files both have (a spread sample of them), `a` is no worse in anything
/// and better in something (`audio_info::better`).
fn clearly_better(a: (&str, &Files), b: (&str, &Files)) -> bool {
    let mut shared: Vec<&String> = a.1.keys().filter(|n| b.1.contains_key(*n)).collect();
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

/// The headers of some files at a place, by name.
fn infos(address: &str, files: &Files, names: &[&String]) -> HashMap<String, AudioInfo> {
    let mut out = HashMap::new();
    let mut pkg = match SongAddress::parse(address) {
        SongAddress::Package { bmsp, .. } => bms_package::PackageReader::open_file(bmsp).ok(),
        _ => None,
    };
    for name in names {
        let Some(file) = files.get(*name) else {
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

/// Whether two places hold the same files: every name both have is the same
/// size, and they have at least one name in common.
fn same_files(a: &Files, b: &Files) -> bool {
    let mut shared = 0;
    for (name, file) in a {
        if let Some(other) = b.get(name) {
            if file.size != other.size {
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
    // An installed package is `<id>/<state hash>/package.bmsp`: name it by its id.
    let file = Path::new(pkg);
    let id = file
        .parent()
        .and_then(Path::parent)
        .and_then(Path::file_name);
    match (file.file_stem().and_then(|s| s.to_str()), id) {
        (Some("package"), Some(id)) => id.to_str().unwrap_or(pkg),
        (Some(stem), _) => stem,
        _ => pkg,
    }
}

/// What a set's files are, from the header of the first one
/// (`AudioInfo::describe`), or their extension when it cannot be read.
fn describe(address: &str, files: &Files) -> String {
    let Some(first) = files.keys().min() else {
        return String::new();
    };
    match infos(address, files, &[first]).get(first) {
        Some(info) => info.describe(),
        None => files[first].ext.to_uppercase(),
    }
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
                        let ext = extension(&path);
                        out.insert(name.to_lowercase(), FileAt { size, ext, path });
                    }
                }
            }
        }
        SongAddress::Disc { .. } => {}
    }
    out
}

// ---------------------------------------------------------------------------
// Cleanup (P2.1)
// ---------------------------------------------------------------------------

/// What a cleanup removes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Trash {
    /// A library folder, moved into `.bpm-trash` beside it.
    Folder(PathBuf),
    /// An installed package file, uninstalled.
    Package(PathBuf),
}

/// What cleaning up the worse sets of a chart removes. Nothing when a place
/// of the song is not a whole set (a difference folder may draw on any of
/// them). A worse place goes only when every chart there has a copy at a
/// place that stays, and a package only when all of its places go.
pub fn cleanup(
    sets: &Sets,
    places: &BTreeMap<String, SongPlace>,
    song_places: &SongPlaces,
) -> Vec<Trash> {
    if !sets.all_whole {
        return Vec::new();
    }
    let worse: HashSet<&str> = sets
        .sets
        .iter()
        .filter(|s| s.worse)
        .flat_map(|s| s.same.iter().map(String::as_str))
        .collect();
    let kept_charts: HashSet<_> = places
        .iter()
        .filter(|(k, _)| !worse.contains(k.as_str()))
        .flat_map(|(_, p)| p.charts.iter().copied())
        .collect();
    let going: HashSet<&str> = worse
        .iter()
        .copied()
        .filter(|k| {
            places
                .get(*k)
                .is_some_and(|p| p.charts.iter().all(|c| kept_charts.contains(c)))
        })
        .collect();
    let mut out = Vec::new();
    for key in &going {
        let trash = match key.split_once("::") {
            None => Trash::Folder(PathBuf::from(key)),
            Some((bmsp, _)) => {
                let all_go = song_places
                    .values()
                    .flatten()
                    .filter_map(|p| place_of(&p.address))
                    .filter(|k| k.split_once("::").is_some_and(|(b, _)| b == bmsp))
                    .all(|k| going.contains(k.as_str()));
                if !all_go {
                    continue;
                }
                Trash::Package(PathBuf::from(bmsp))
            }
        };
        out.push(trash);
    }
    out.sort();
    out.dedup();
    out
}

/// Moves a folder into `.bpm-trash` in its parent folder, under a free name.
pub fn trash_folder(dir: &Path) -> std::io::Result<PathBuf> {
    let parent = dir.parent().unwrap_or_else(|| Path::new("."));
    let trash = parent.join(bms_package_manager::collection::TRASH_DIR);
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
    pub fn chosen<'a, V>(&'a self, places: &'a BTreeMap<String, V>) -> Option<&'a str> {
        let first = places.keys().next()?;
        self.0
            .get(first)
            .filter(|c| places.contains_key(*c))
            .map(String::as_str)
    }

    /// Chooses `place` for the song whose places are `places`.
    pub fn choose<V>(&mut self, places: &BTreeMap<String, V>, place: &str) {
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

    /// One song (number 1) with a place for each chart, holding that chart.
    fn library(songs: &[SongMetadata]) -> (SongGroups, SongPlaces) {
        let groups = songs.iter().map(|s| (s.id, 1)).collect();
        let places = songs
            .iter()
            .map(|s| SongPlace {
                address: s.file_path.clone(),
                charts: vec![s.id],
            })
            .collect();
        (groups, SongPlaces::from([(1, places)]))
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
        let (groups, places_of) = library(&songs);
        let mut choices = Choices::default();
        let plan_for = |c: &Choices| plan(&groups, &places_of, c, &songs[0]);
        assert_eq!(plan_for(&choices).chosen, None);
        assert!(!plan_for(&choices).decided);
        assert_eq!(plan_for(&choices).others, ["D:\\b\\2.bms", "D:\\c\\3.bms"]);

        choices.choose(&places(&groups, &places_of, &songs[0]), "D:\\c");
        let p = plan_for(&choices);
        assert_eq!(p.chosen.as_deref(), Some("D:\\c\\3.bms"));
        assert_eq!(p.others, ["D:\\b\\2.bms"]);
        // From the chosen place itself nothing is swapped.
        let own = plan(&groups, &places_of, &choices, &songs[2]);
        assert_eq!((own.chosen, own.decided), (None, true));
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
                .map(|&(n, size)| {
                    let file = FileAt {
                        size,
                        ext: "wav".into(),
                        path: n.into(),
                    };
                    (n.to_string(), file)
                })
                .collect()
        };
        assert!(same_files(&f(&[("a", 1), ("b", 2)]), &f(&[("a", 1)])));
        assert!(!same_files(&f(&[("a", 1)]), &f(&[("a", 3)])));
        assert!(!same_files(&f(&[("a", 1)]), &f(&[("b", 1)])));
    }

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
    fn the_menu_offers_sets_that_differ_and_marks_the_worse_one() {
        let root = std::env::temp_dir().join(format!("beetle-sets-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let chart = "#WAV01 a.wav\n#WAV02 b.wav\n#00111:0102\n";
        let mk = |dir: &str, files: &[(&str, Vec<u8>)]| {
            let d = root.join(dir);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("x.bms"), chart).unwrap();
            for (name, bytes) in files {
                fs::write(d.join(name), bytes).unwrap();
            }
            d.join("x.bms").to_string_lossy().into_owned()
        };
        let wav = mk(
            "wav",
            &[("a.wav", wav_bytes(44100)), ("b.wav", wav_bytes(44100))],
        );
        let copy = mk(
            "copy",
            &[("a.wav", wav_bytes(44100)), ("b.wav", wav_bytes(44100))],
        );
        let ogg = mk(
            "ogg",
            &[("a.ogg", ogg_bytes(44100)), ("b.ogg", ogg_bytes(44100))],
        );
        let diff = mk("diff", &[]);
        let songs: Vec<SongMetadata> = [&copy, &diff, &ogg, &wav]
            .iter()
            .enumerate()
            .map(|(i, p)| song(p, i as u8))
            .collect();
        let (groups, mut places_of) = library(&songs);
        let found = sets(&groups, &places_of, &songs[1]);
        let shown: Vec<(&str, bool)> = found
            .sets
            .iter()
            .map(|s| (s.label.as_str(), s.worse))
            .collect();
        // `copy` and `wav` are the same files; `diff` has none.
        assert_eq!(
            shown,
            [
                ("copy · WAV 44.1kHz/16bit", false),
                ("ogg · OGG 44.1kHz 192kbps", true)
            ]
        );
        assert_eq!(found.sets[0].same.len(), 2);
        assert!(!found.all_whole);
        let names = vec!["a.wav".to_string(), "b.wav".to_string()];
        assert_eq!(
            better_set(&ogg, &[copy.clone()], &names),
            Some(copy.clone())
        );
        assert_eq!(better_set(&copy, &[ogg.clone()], &names), None);

        // A difference folder may draw on any set: nothing is cleaned up.
        let at = |p: &SongPlaces, s: &SongMetadata| places(&groups, p, s);
        assert!(cleanup(&found, &at(&places_of, &songs[0]), &places_of).is_empty());

        // Without it, the worse place goes only when its charts stay elsewhere.
        places_of.get_mut(&1).unwrap().retain(|p| p.address != diff);
        let found = sets(&groups, &places_of, &songs[0]);
        assert!(found.all_whole);
        assert!(cleanup(&found, &at(&places_of, &songs[0]), &places_of).is_empty());
        for p in places_of.get_mut(&1).unwrap() {
            if p.address == ogg {
                p.charts = vec![songs[3].id];
            }
        }
        let ogg_dir = PathBuf::from(place_of(&ogg).unwrap());
        assert_eq!(
            cleanup(&found, &at(&places_of, &songs[0]), &places_of),
            [Trash::Folder(ogg_dir.clone())]
        );
        let moved = trash_folder(&ogg_dir).unwrap();
        assert!(!ogg_dir.exists() && moved.join("x.bms").is_file());
        let _ = fs::remove_dir_all(&root);
    }
}

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
use std::path::{Path, PathBuf};

use beetle_core::msgpack::{self, Value, Writer};
use beetle_core::{ChartId, SongAddress, SongGroups, SongMetadata, SongPlace, SongPlaces};
use bms_package_manager::key_sets::{self, PlaceAt};

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
    /// The place's folder name and what its files are, e.g. `xi · WAV 44.1kHz/16bit`.
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

/// The library's places for judging songs being added (B18,
/// `same_song::judge`), from the scan's places and the song list. Disc
/// songs have no place and are left out.
pub fn known(
    songs: &[SongMetadata],
    song_places: &SongPlaces,
) -> Vec<bms_package_manager::same_song::KnownPlace> {
    let by_id: HashMap<ChartId, &SongMetadata> = songs.iter().map(|s| (s.id, s)).collect();
    let mut known = Vec::new();
    for (&song, places) in song_places {
        for place in places {
            let (Some(at), Some(meta)) = (
                place_at(&place.address),
                place.charts.iter().find_map(|c| by_id.get(c)),
            ) else {
                continue;
            };
            let (chart_file, label, package) = match SongAddress::parse(&place.address) {
                SongAddress::Folder { file } => {
                    let name = file.rsplit(['/', '\\']).next().unwrap_or(file);
                    (name.to_string(), parent(file).to_string(), None)
                }
                SongAddress::Package { bmsp, entry } => {
                    // packages/<id>/<state>/package.bmsp
                    let id = Path::new(bmsp)
                        .parent()
                        .and_then(Path::parent)
                        .and_then(Path::file_name)
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let name = entry.rsplit('/').next().unwrap_or(entry);
                    (name.to_string(), id.clone(), Some(id))
                }
                SongAddress::Disc { .. } => continue,
            };
            known.push(bms_package_manager::same_song::KnownPlace {
                song: song as usize,
                at,
                chart_file,
                charts: place.charts.clone(),
                title: meta.title.clone(),
                artist: meta.artist.clone(),
                label,
                package,
            });
        }
    }
    known
}

/// Where a chart address's files are. `None` for a disc song.
fn place_at(address: &str) -> Option<PlaceAt> {
    match SongAddress::parse(address) {
        SongAddress::Folder { file } => Some(PlaceAt::Folder(
            Path::new(file)
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .into(),
        )),
        SongAddress::Package { bmsp, entry } => Some(PlaceAt::Package {
            bmsp: bmsp.into(),
            dir: parent(entry).to_string(),
        }),
        SongAddress::Disc { .. } => None,
    }
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

/// The sets `song`'s chart can play with, in place order (`key_sets::compare`).
pub fn sets(groups: &SongGroups, song_places: &SongPlaces, song: &SongMetadata) -> Sets {
    let names = declared(&song.file_path);
    let places: Vec<(String, PlaceAt)> = places(groups, song_places, song)
        .into_iter()
        .filter_map(|(key, p)| Some((key, place_at(&p.address)?)))
        .collect();
    let (found, all_whole) = key_sets::compare(&places, &names);
    Sets {
        sets: found
            .into_iter()
            .map(|set| SoundSet {
                label: format!("{} · {}", place_name(&set.place), set.description),
                place: set.place,
                same: set.same,
                worse: set.worse,
            })
            .collect(),
        all_whole,
    }
}

/// A set clearly better than the chart's own place, for a song whose player
/// has not chosen one: the first of `others` that is (P2.1). Reads only headers.
pub fn better_set(own: &str, others: &[String], names: &[String]) -> Option<String> {
    let own_at = place_at(own)?;
    let mine = key_sets::files_at(&own_at, names);
    if !mine.whole(names) {
        return None;
    }
    others
        .iter()
        .find(|other| {
            let Some(at) = place_at(other) else {
                return false;
            };
            let theirs = key_sets::files_at(&at, names);
            theirs.whole(names)
                && !theirs.same(&mine)
                && key_sets::clearly_better((&at, &theirs), (&own_at, &mine))
        })
        .cloned()
}

/// The last folder name of a place (for a package's root, the package's id).
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

/// What cleaning up the worse sets of a chart removes (`key_sets::going`):
/// a folder, or a package when all of its places go.
pub fn cleanup(
    sets: &Sets,
    places: &BTreeMap<String, SongPlace>,
    song_places: &SongPlaces,
) -> Vec<Trash> {
    let found: Vec<key_sets::Set> = sets
        .sets
        .iter()
        .map(|s| key_sets::Set {
            place: s.place.clone(),
            description: String::new(),
            same: s.same.clone(),
            worse: s.worse,
        })
        .collect();
    let charts: BTreeMap<String, Vec<ChartId>> = places
        .iter()
        .map(|(k, p)| (k.clone(), p.charts.clone()))
        .collect();
    let going: HashSet<&str> = key_sets::going(&found, sets.all_whole, &charts)
        .into_iter()
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

pub use key_sets::trash_folder;

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

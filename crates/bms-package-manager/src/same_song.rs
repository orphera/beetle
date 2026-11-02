//! Whether something about to be added is a song already in the library
//! (B18, docs/plans/2026-11-01-package-by-song-plan.md §4.1).
//!
//! The judgement uses the B17 rules: a place joins a known song when it has a
//! chart (same bytes) of it, or the same title and artist with overlapping key
//! sounds (`songs::sounds_match`). The same title and artist alone only make a
//! candidate. Key sound sets are compared with `key_sets`.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::Path;

use beetle_core::{ChartId, SongMetadata};

use crate::collection::normalize_stem;
use crate::key_sets::{self, PlaceAt};
use crate::songs::{artist_key, sounds_match, title_base};

/// A place already in the library.
#[derive(Debug, Clone)]
pub struct KnownPlace {
    /// The song it belongs to: places with the same number are one song.
    pub song: usize,
    pub at: PlaceAt,
    /// A chart file there, relative to the place (read for its key sounds).
    pub chart_file: String,
    pub charts: Vec<ChartId>,
    pub title: String,
    pub artist: String,
    /// Shown to the user: a folder path or a package name.
    pub label: String,
    /// The installed package this place is in, when it is one.
    pub package: Option<String>,
}

/// One chart of what is being added.
#[derive(Debug, Clone)]
pub struct NewChart {
    pub id: ChartId,
    pub title: String,
    pub artist: String,
    /// Declared key sounds, as written in the chart.
    pub names: Vec<String>,
}

/// What is being added, read before it goes in: one song folder or package.
#[derive(Debug, Clone)]
pub struct Incoming {
    pub at: PlaceAt,
    pub charts: Vec<NewChart>,
}

const CHART_EXTENSIONS: &[&str] = &["bms", "bme", "bml", "pms"];

fn is_chart(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    CHART_EXTENSIONS
        .iter()
        .any(|ext| lower.ends_with(&format!(".{ext}")))
}

fn read_chart(path: &str, bytes: &[u8]) -> Option<NewChart> {
    let (meta, names) = SongMetadata::from_bytes_with_sounds(path, bytes)?;
    Some(NewChart {
        id: meta.id,
        title: meta.title,
        artist: meta.artist,
        names,
    })
}

impl Incoming {
    /// The charts directly in a song folder.
    pub fn folder(dir: &Path) -> Self {
        let mut charts = Vec::new();
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_file() && is_chart(&name) {
                if let Some(chart) = fs::read(&path)
                    .ok()
                    .and_then(|b| read_chart(&path.to_string_lossy(), &b))
                {
                    charts.push(chart);
                }
            }
        }
        Incoming {
            at: PlaceAt::Folder(dir.to_path_buf()),
            charts,
        }
    }

    /// The charts of a package file, in the directory that holds the first of
    /// them (a song package has one).
    pub fn package(bmsp: &Path) -> Option<Self> {
        let mut pkg = bms_package::PackageReader::open_file(bmsp).ok()?;
        let paths: Vec<String> = pkg
            .entries()
            .iter()
            .map(|e| e.path.clone())
            .filter(|p| is_chart(p))
            .collect();
        let dir = paths
            .first()
            .map(|p| p.rsplit_once('/').map_or("", |(d, _)| d).to_string())?;
        let charts = paths
            .iter()
            .filter(|p| p.rsplit_once('/').map_or("", |(d, _)| d) == dir)
            .filter_map(|p| read_chart(p, &pkg.read_entry(p).ok()?))
            .collect();
        Some(Incoming {
            at: PlaceAt::Package {
                bmsp: bmsp.to_path_buf(),
                dir,
            },
            charts,
        })
    }

    /// Every key sound the charts declare, sorted.
    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.charts.iter().flat_map(|c| c.names.clone()).collect();
        names.sort();
        names.dedup();
        names
    }
}

/// What a key sound set is, for the user to compare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetInfo {
    /// Where it is.
    pub label: String,
    /// Format and quality, e.g. `WAV 44.1kHz/16bit` (`key_sets::describe`).
    pub description: String,
    pub files: usize,
    pub bytes: u64,
    /// Clearly better than the other side.
    pub better: bool,
}

/// The judgement on something being added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// No known song is the same.
    NewSong,
    /// A known song has the same title and artist, but nothing else ties them.
    Candidate { song: String },
    /// Every chart is already there, with the same key sounds (or none of its own).
    AlreadyThere { song: String },
    /// Joins a known song and brings `count` charts it does not have.
    NewCharts { song: String, count: usize },
    /// Joins a known song with a key sound set of its own.
    OtherSet {
        song: String,
        /// The new charts it brings.
        count: usize,
        existing: Vec<SetInfo>,
        new: SetInfo,
        /// Installed packages holding the existing set whose charts all come
        /// with the new one: what "replace" removes.
        replaceable: Vec<String>,
    },
}

/// The key sounds of a known place's chart, as stems.
fn known_stems(place: &KnownPlace) -> Vec<String> {
    let bytes = match &place.at {
        PlaceAt::Folder(dir) => fs::read(dir.join(&place.chart_file)).ok(),
        PlaceAt::Package { bmsp, dir } => {
            let entry = if dir.is_empty() {
                place.chart_file.clone()
            } else {
                format!("{dir}/{}", place.chart_file)
            };
            bms_package::PackageReader::open_file(bmsp)
                .ok()
                .and_then(|mut p| p.read_entry(&entry).ok())
        }
    };
    let names = bytes
        .and_then(|b| SongMetadata::from_bytes_with_sounds(&place.chart_file, &b))
        .map(|(_, names)| names)
        .unwrap_or_default();
    stems(&names)
}

fn stems(names: &[String]) -> Vec<String> {
    let mut stems: Vec<String> = names
        .iter()
        .map(|n| normalize_stem(n))
        .filter(|s| !s.is_empty())
        .collect();
    stems.sort();
    stems.dedup();
    stems
}

/// The library's places from bpm's collection (`collection.idx` grouped by
/// `songs::build`). Package places need the installed package file, so those
/// whose package is gone are left out.
pub fn known_from_collection(
    collection: &crate::songs::Collection<'_>,
    mgr: &crate::manager::PackageManager,
) -> Vec<KnownPlace> {
    use crate::collection::LocationKind;
    let packages = mgr.registry().list_packages();
    let package_file = |source: &str| {
        let (id, hash) = source.split_once('@')?;
        let record = packages.iter().find(|r| r.id == id)?;
        let state = &record.state_hashes.get(hash)?.path;
        Some((
            id.to_string(),
            mgr.root_dir()
                .join(state)
                .join(bms_package::installed::PACKAGE_FILENAME),
        ))
    };
    let mut known: Vec<KnownPlace> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for (song_index, song) in collection.songs.iter().enumerate() {
        for &i in &song.charts {
            let record = &collection.records[i];
            for copy in &record.copies {
                let key = crate::songs::place_key(copy);
                if let Some(&k) = seen.get(&key) {
                    known[k].charts.push(record.chart);
                    continue;
                }
                let (dir, file) = copy.path.rsplit_once('/').unwrap_or(("", &copy.path));
                let (at, label, package) = match copy.kind {
                    LocationKind::Folder => {
                        let path = Path::new(&copy.source).join(dir);
                        let label = path.display().to_string();
                        (PlaceAt::Folder(path), label, None)
                    }
                    LocationKind::Package => {
                        let Some((id, bmsp)) = package_file(&copy.source) else {
                            continue;
                        };
                        let at = PlaceAt::Package {
                            bmsp,
                            dir: dir.to_string(),
                        };
                        (at, id.clone(), Some(id))
                    }
                };
                seen.insert(key, known.len());
                known.push(KnownPlace {
                    song: song_index,
                    at,
                    chart_file: file.to_string(),
                    charts: vec![record.chart],
                    title: song.title.clone(),
                    artist: song.artist.clone(),
                    label,
                    package,
                });
            }
        }
    }
    known
}

/// The known song `incoming` joins: one sharing a chart, else the same
/// title and artist with overlapping key sounds. `Err` with the first place
/// of the same title and artist (a candidate), when there is one.
fn song_of<'a>(
    incoming: &Incoming,
    known: &'a [KnownPlace],
) -> Result<usize, Option<&'a KnownPlace>> {
    let ids: BTreeSet<ChartId> = incoming.charts.iter().map(|c| c.id).collect();
    let first = incoming.charts.first().ok_or(None)?;
    let title = title_base(&first.title).to_ascii_lowercase();
    let artist = artist_key(&first.artist);
    let new_stems = stems(&incoming.names());
    let same_name: Vec<&KnownPlace> = known
        .iter()
        .filter(|p| {
            title_base(&p.title).to_ascii_lowercase() == title && artist_key(&p.artist) == artist
        })
        .collect();
    known
        .iter()
        .find(|p| p.charts.iter().any(|c| ids.contains(c)))
        .or_else(|| {
            same_name
                .iter()
                .copied()
                .find(|p| sounds_match(&known_stems(p), &new_stems))
        })
        .map(|p| p.song)
        .ok_or(same_name.first().copied())
}

/// Judges `incoming` against the library's places.
pub fn judge(incoming: &Incoming, known: &[KnownPlace]) -> Verdict {
    let ids: BTreeSet<ChartId> = incoming.charts.iter().map(|c| c.id).collect();
    let song = match song_of(incoming, known) {
        Ok(song) => song,
        Err(Some(p)) => {
            return Verdict::Candidate {
                song: p.title.clone(),
            }
        }
        Err(None) => return Verdict::NewSong,
    };
    let places: Vec<&KnownPlace> = known.iter().filter(|p| p.song == song).collect();
    let song_title = places[0].title.clone();
    let known_charts: BTreeSet<ChartId> = places
        .iter()
        .flat_map(|p| p.charts.iter().copied())
        .collect();
    let count = ids.iter().filter(|c| !known_charts.contains(c)).count();

    let names = incoming.names();
    let mine = key_sets::files_at(&incoming.at, &names);
    let sets: Vec<(&KnownPlace, key_sets::Files)> = places
        .iter()
        .map(|p| (*p, key_sets::files_at(&p.at, &names)))
        .filter(|(_, f)| f.whole(&names))
        .collect();
    if !mine.whole(&names) || sets.is_empty() || sets.iter().any(|(_, f)| f.same(&mine)) {
        return if count == 0 {
            Verdict::AlreadyThere { song: song_title }
        } else {
            Verdict::NewCharts {
                song: song_title,
                count,
            }
        };
    }

    // A set of its own: what each side is, and which is clearly better.
    let new_better = sets
        .iter()
        .all(|(p, f)| key_sets::clearly_better((&incoming.at, &mine), (&p.at, f)));
    let mut existing: Vec<SetInfo> = Vec::new();
    let mut seen: Vec<&key_sets::Files> = Vec::new();
    for (p, f) in &sets {
        if seen.iter().any(|s| s.same(f)) {
            continue;
        }
        seen.push(f);
        existing.push(SetInfo {
            label: p.label.clone(),
            description: key_sets::describe(&p.at, f),
            files: f.len(),
            bytes: f.total_size(),
            better: key_sets::clearly_better((&p.at, f), (&incoming.at, &mine)),
        });
    }
    // A package goes on replace only when the new charts cover all of its
    // charts in this song, and every place of it belongs to this song.
    let mut by_package: HashMap<&str, (bool, bool)> = HashMap::new();
    for p in known {
        let Some(id) = p.package.as_deref() else {
            continue;
        };
        let entry = by_package.entry(id).or_insert((true, false));
        let covered = p.song == song && p.charts.iter().all(|c| ids.contains(c));
        entry.0 &= covered;
        entry.1 |= p.song == song;
    }
    let mut replaceable: Vec<String> = by_package
        .into_iter()
        .filter(|(_, (covered, here))| *covered && *here)
        .map(|(id, _)| id.to_string())
        .collect();
    replaceable.sort();
    Verdict::OtherSet {
        song: song_title,
        count,
        existing,
        new: SetInfo {
            label: String::new(),
            description: key_sets::describe(&incoming.at, &mine),
            files: mine.len(),
            bytes: mine.total_size(),
            better: new_better,
        },
        replaceable,
    }
}

/// How something joining a known song goes in with only what the song does
/// not have yet (B20, docs/plans/2026-11-02-chart-only-packages-plan.md 3.3):
/// its charts play with a set already in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartOnly {
    /// The song's charts at the places of that set (`chart_base::Base::charts`).
    pub base: Vec<ChartId>,
    /// The installed package holding the set, when one does.
    pub package: Option<String>,
    /// Key sounds of the incoming place that the set has already, with the
    /// same size, relative to the place with `/` separators. These are left
    /// out. Pictures and video stay: the game takes only key sounds from
    /// another place.
    pub leave_out: Vec<String>,
}

/// What `incoming`, judged `Verdict::NewCharts`, leaves out when it goes in
/// with only its charts, and the set they play with. That set is a whole
/// one of the song with the same files as the incoming place; for a place
/// that is not a whole set itself (a difference archive), one sharing its
/// files, else the first. `None` when the song has no whole set.
pub fn chart_only(incoming: &Incoming, known: &[KnownPlace]) -> Option<ChartOnly> {
    let song = song_of(incoming, known).ok()?;
    let names = incoming.names();
    let mine = key_sets::files_at(&incoming.at, &names);
    let sets: Vec<(&KnownPlace, key_sets::Files)> = known
        .iter()
        .filter(|p| p.song == song)
        .map(|p| (p, key_sets::files_at(&p.at, &names)))
        .filter(|(_, f)| f.whole(&names))
        .collect();
    let (twin, files) = sets
        .iter()
        .find(|(_, f)| f.same(&mine))
        .or_else(|| sets.first().filter(|_| !mine.whole(&names)))?;
    let base: BTreeSet<ChartId> = sets
        .iter()
        .filter(|(_, f)| f.same(files))
        .flat_map(|(p, _)| p.charts.iter().copied())
        .collect();
    let relative = |path: String| match &incoming.at {
        PlaceAt::Folder(dir) => Path::new(&path)
            .strip_prefix(dir)
            .map_or(path.clone(), |p| p.to_string_lossy().replace('\\', "/")),
        PlaceAt::Package { dir, .. } if !dir.is_empty() => path
            .strip_prefix(&format!("{dir}/"))
            .map_or(path.clone(), str::to_string),
        PlaceAt::Package { .. } => path,
    };
    let leave_out: Vec<String> = mine.shared_with(files).into_iter().map(relative).collect();
    Some(ChartOnly {
        base: base.into_iter().collect(),
        package: twin.package.clone(),
        leave_out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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

    fn ogg_bytes() -> Vec<u8> {
        let mut b = b"OggS".to_vec();
        b.extend([0u8; 22]);
        b.extend([1, 30]);
        b.extend(b"\x01vorbis\0\0\0\0\x02");
        b.extend(44100u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        b.extend(192_000u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        b
    }

    /// A folder with charts (`#TITLE`, level) and key sound files.
    fn place(
        root: &Path,
        dir: &str,
        charts: &[(&str, u32)],
        sounds: &[(String, Vec<u8>)],
    ) -> PathBuf {
        let d = root.join(dir);
        fs::create_dir_all(&d).unwrap();
        let wavs: String = (1..=10).map(|i| format!("#WAV{i:02} s{i}.wav\n")).collect();
        for (i, (title, level)) in charts.iter().enumerate() {
            let text =
                format!("#TITLE {title}\n#ARTIST A\n#PLAYLEVEL {level}\n{wavs}#00111:0102\n");
            fs::write(d.join(format!("{i}.bms")), text).unwrap();
        }
        for (name, bytes) in sounds {
            fs::write(d.join(name), bytes).unwrap();
        }
        d
    }

    fn known(dir: &Path, song: usize, package: Option<&str>) -> KnownPlace {
        let incoming = Incoming::folder(dir);
        KnownPlace {
            song,
            at: PlaceAt::Folder(dir.to_path_buf()),
            chart_file: "0.bms".into(),
            charts: incoming.charts.iter().map(|c| c.id).collect(),
            title: incoming.charts[0].title.clone(),
            artist: incoming.charts[0].artist.clone(),
            label: dir.file_name().unwrap().to_string_lossy().into_owned(),
            package: package.map(str::to_string),
        }
    }

    #[test]
    fn verdicts() {
        let root = std::env::temp_dir().join(format!("bpm-same-song-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let wavs: Vec<(String, Vec<u8>)> = (1..=10)
            .map(|i| (format!("s{i}.wav"), wav_bytes(44100)))
            .collect();
        let oggs: Vec<(String, Vec<u8>)> = (1..=10)
            .map(|i| (format!("s{i}.ogg"), ogg_bytes()))
            .collect();

        let have = place(&root, "have", &[("Foo", 1), ("Foo", 2)], &wavs);
        let library = vec![known(&have, 0, Some("pkg.foo"))];

        let copy = place(&root, "copy", &[("Foo", 1), ("Foo", 2)], &wavs);
        assert_eq!(
            judge(&Incoming::folder(&copy), &library),
            Verdict::AlreadyThere { song: "Foo".into() }
        );
        let more = place(&root, "more", &[("Foo", 1), ("Foo", 3)], &wavs);
        assert_eq!(
            judge(&Incoming::folder(&more), &library),
            Verdict::NewCharts {
                song: "Foo".into(),
                count: 1
            }
        );
        let other = place(&root, "other", &[("Foo", 1), ("Foo", 2)], &oggs);
        match judge(&Incoming::folder(&other), &library) {
            Verdict::OtherSet {
                existing,
                new,
                replaceable,
                count,
                ..
            } => {
                assert_eq!(count, 0);
                assert_eq!(existing.len(), 1);
                assert_eq!(existing[0].description, "WAV 44.1kHz/16bit");
                assert!(existing[0].better && !new.better);
                assert_eq!(
                    (new.description.as_str(), new.files),
                    ("OGG 44.1kHz 192kbps", 10)
                );
                assert_eq!(replaceable, ["pkg.foo"]);
            }
            v => panic!("{v:?}"),
        }
        let fresh = place(&root, "fresh", &[("Bar", 1)], &wavs);
        assert_eq!(judge(&Incoming::folder(&fresh), &library), Verdict::NewSong);
        let _ = fs::remove_dir_all(&root);
    }
}

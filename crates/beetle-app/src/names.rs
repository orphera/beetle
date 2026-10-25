//! Word pictures of game disc songs: each song's title and artist as the
//! disc draws them (`cs_disc::popn::Mask`), kept in `.cache/names/<edition>.names`
//! The file is made from the disc once, when the
//! disc is scanned, and reused while it is newer than the disc image, the same
//! rule `songs.cache` follows. Without it a song shows its text title.
//!
//! The list draws charts, so the file also maps each chart (its `ChartId`) to
//! its disc song.
//!
//! Format (little endian): the magic `BNM6` (the small title and the genre too, IIDX; ink cut from a faint floor); `u32` song count, then per song
//! `u32` song id and the title and the artist, each `u8` present (0 or 1) and,
//! when present, `u32` width, `u32` height and width × height coverage bytes;
//! then `u32` chart count, then per chart its 32-byte `ChartId` and `u32` song id.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use beetle_core::ChartId;
use cs_disc::iidx::Disc;
use cs_disc::popn::{Mask, SongNames};
use cs_disc::SectorReader;

const MAGIC: &[u8; 4] = b"BNM7";

/// The names of a disc: its songs by id, and which song each chart is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Names {
    pub songs: BTreeMap<u32, SongNames>,
    pub charts: Vec<(ChartId, u32)>,
}

/// The folder of the names files: derived data, made again from the discs.
pub const DIR: &str = ".cache/names";

/// The names file of `slug`.
pub fn path(slug: &str) -> PathBuf {
    PathBuf::from(DIR).join(format!("{slug}.names"))
}

pub fn encode(names: &Names) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    out.extend((names.songs.len() as u32).to_le_bytes());
    for (id, song) in &names.songs {
        out.extend(id.to_le_bytes());
        put_mask(&mut out, song.title.as_ref());
        put_mask(&mut out, song.small_title.as_ref());
        put_mask(&mut out, song.artist.as_ref());
        put_mask(&mut out, song.genre.as_ref());
        out.push(song.popn as u8);
    }
    out.extend((names.charts.len() as u32).to_le_bytes());
    for (chart, song) in &names.charts {
        out.extend(chart.as_bytes());
        out.extend(song.to_le_bytes());
    }
    out
}

fn put_mask(out: &mut Vec<u8>, mask: Option<&Mask>) {
    let Some(mask) = mask else {
        out.push(0);
        return;
    };
    out.push(1);
    out.extend(mask.width.to_le_bytes());
    out.extend(mask.height.to_le_bytes());
    out.extend(&mask.coverage);
}

/// The names `encode` wrote; `None` for a file that is not one (or is cut short).
pub fn decode(bytes: &[u8]) -> Option<Names> {
    let mut at = bytes.strip_prefix(MAGIC)?;
    let mut names = Names::default();
    for _ in 0..take_u32(&mut at)? {
        let id = take_u32(&mut at)?;
        let title = take_mask(&mut at)?;
        let small_title = take_mask(&mut at)?;
        let artist = take_mask(&mut at)?;
        let genre = take_mask(&mut at)?;
        let popn = take(&mut at, 1)?[0] != 0;
        names.songs.insert(
            id,
            SongNames {
                title,
                small_title,
                artist,
                genre,
                popn,
            },
        );
    }
    for _ in 0..take_u32(&mut at)? {
        let chart: [u8; 32] = take(&mut at, 32)?.try_into().ok()?;
        let song = take_u32(&mut at)?;
        names.charts.push((ChartId::from_digest(chart), song));
    }
    at.is_empty().then_some(names)
}

fn take<'a>(at: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
    let head = at.get(..n)?;
    *at = &at[n..];
    Some(head)
}

fn take_u32(at: &mut &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(take(at, 4)?.try_into().ok()?))
}

fn take_mask(at: &mut &[u8]) -> Option<Option<Mask>> {
    match take(at, 1)?[0] {
        0 => Some(None),
        1 => {
            let width = take_u32(at)?;
            let height = take_u32(at)?;
            let coverage = take(at, (width as usize).checked_mul(height as usize)?)?.to_vec();
            Some(Some(Mask {
                width,
                height,
                coverage,
            }))
        }
        _ => None,
    }
}

/// Every chart's song names, from every names file in the library folder.
pub fn load_all() -> HashMap<ChartId, Arc<SongNames>> {
    let mut by_chart = HashMap::new();
    let files = fs::read_dir(DIR).into_iter().flatten().flatten();
    for file in files {
        let p = file.path();
        if p.extension().is_none_or(|e| e != "names") {
            continue;
        }
        let Some(names) = fs::read(&p).ok().and_then(|b| decode(&b)) else {
            continue;
        };
        let songs: HashMap<u32, Arc<SongNames>> = names
            .songs
            .into_iter()
            .map(|(id, song)| (id, Arc::new(song)))
            .collect();
        for (chart, song) in names.charts {
            if let Some(song_names) = songs.get(&song) {
                by_chart.insert(chart, Arc::clone(song_names));
            }
        }
    }
    by_chart
}

/// Whether the names file of `slug` is current for the disc image `source`:
/// it exists and is newer than the image. A drive (no file to compare) is
/// never current.
fn is_current(slug: &str, source: &Path) -> bool {
    let modified = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).ok();
    let readable = fs::read(path(slug)).ok().and_then(|b| decode(&b)).is_some();
    match (modified(&path(slug)), modified(source)) {
        (Some(cached), Some(image)) => readable && cached >= image,
        _ => false,
    }
}

/// Whether any names file in the library folder is in a format this build
/// cannot read (made by an older build): its disc then needs a scan to make
/// it again.
pub fn any_unreadable() -> bool {
    fs::read_dir(DIR)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "names"))
        .any(|p| fs::read(&p).ok().and_then(|b| decode(&b)).is_none())
}

/// Makes the names file of `slug` from `disc` when it is missing or old.
/// `source` is the disc image's path (or the drive); `charts` lists each chart
/// the scan made with its song id. Errors are ignored: the songs then show
/// their text titles.
pub fn ensure<R: io::Read + io::Seek>(
    disc: &mut SectorReader<R>,
    songs: &Disc,
    slug: &str,
    source: &str,
    charts: &[(ChartId, u32)],
) {
    if is_current(slug, Path::new(source)) {
        return;
    }
    let mut names = Names {
        charts: charts.to_vec(),
        ..Names::default()
    };
    for song in &songs.songs {
        if let Ok(Some(song_names)) = songs.song_names(disc, song) {
            names.songs.insert(song.id, song_names);
        }
    }
    if names.songs.is_empty() {
        return;
    }
    let _ = fs::create_dir_all(DIR);
    let _ = fs::write(path(slug), encode(&names));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask(width: u32, height: u32, fill: u8) -> Mask {
        Mask {
            width,
            height,
            coverage: vec![fill; (width * height) as usize],
        }
    }

    #[test]
    fn names_round_trip() {
        let mut names = Names::default();
        names.songs.insert(
            3,
            SongNames {
                title: Some(mask(2, 1, 255)),
                small_title: None,
                artist: None,
                genre: None,
                popn: true,
            },
        );
        names.songs.insert(
            7,
            SongNames {
                title: None,
                small_title: Some(mask(3, 1, 7)),
                artist: Some(mask(1, 2, 9)),
                genre: None,
                popn: true,
            },
        );
        names.charts.push((ChartId::of_bytes(b"chart"), 3));
        assert_eq!(decode(&encode(&names)), Some(names));
    }

    #[test]
    fn a_cut_short_file_is_rejected() {
        let mut names = Names::default();
        names.songs.insert(
            1,
            SongNames {
                title: Some(mask(4, 4, 1)),
                small_title: None,
                artist: None,
                genre: None,
                popn: true,
            },
        );
        let bytes = encode(&names);
        assert_eq!(decode(&bytes[..bytes.len() - 1]), None);
        assert_eq!(decode(b"NOPE"), None);
        assert_eq!(decode(&[]), None);
        assert_eq!(decode(MAGIC), None);
    }
}

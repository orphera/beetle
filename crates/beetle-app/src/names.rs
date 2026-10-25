//! Word pictures of game disc songs: each song's title and artist as the
//! disc draws them (`cs_disc::popn::Mask`), kept in `disc_meta/<edition>.names`
//! next to the details table. The file is made from the disc once, when the
//! disc is scanned, and reused while it is newer than the disc image, the same
//! rule `songs.cache` follows. Without it a song shows its text title.
//!
//! The list draws charts, so the file also maps each chart (its `ChartId`) to
//! its disc song.
//!
//! Format (little endian): the magic `BNM2`; `u32` song count, then per song
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

use crate::disc_meta;

const MAGIC: &[u8; 4] = b"BNM2";

/// The names of a disc: its songs by id, and which song each chart is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Names {
    pub songs: BTreeMap<u32, SongNames>,
    pub charts: Vec<(ChartId, u32)>,
}

/// The names file of `slug`, next to its details table.
pub fn path(slug: &str) -> PathBuf {
    PathBuf::from(disc_meta::DIR).join(format!("{slug}.names"))
}

pub fn encode(names: &Names) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    out.extend((names.songs.len() as u32).to_le_bytes());
    for (id, song) in &names.songs {
        out.extend(id.to_le_bytes());
        put_mask(&mut out, song.title.as_ref());
        put_mask(&mut out, song.artist.as_ref());
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
        let artist = take_mask(&mut at)?;
        names.songs.insert(id, SongNames { title, artist });
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
    let files = fs::read_dir(disc_meta::DIR).into_iter().flatten().flatten();
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
    match (modified(&path(slug)), modified(source)) {
        (Some(cached), Some(image)) => cached >= image,
        _ => false,
    }
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
    let _ = fs::create_dir_all(disc_meta::DIR);
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
                artist: None,
            },
        );
        names.songs.insert(
            7,
            SongNames {
                title: None,
                artist: Some(mask(1, 2, 9)),
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
                artist: None,
            },
        );
        let bytes = encode(&names);
        assert_eq!(decode(&bytes[..bytes.len() - 1]), None);
        assert_eq!(decode(b"NOPE"), None);
        assert_eq!(decode(&[]), None);
        assert_eq!(decode(MAGIC), None);
    }
}

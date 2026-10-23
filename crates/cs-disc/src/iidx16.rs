//! beatmania IIDX 16 EMPRESS + PREMIUM BEST (PS2, two discs): the song list
//! and where each song's files are.
//!
//! The boot executable holds both. Its song table has one 0x134-byte record per
//! song slot (title, levels, file numbers of the charts and sounds); its file
//! tables give each file number a span inside `BM2DX16A/B/C.DAT`. Offsets come
//! from iidx-ps2tools (`plugins/iidx16thempresscs.py`) and were checked against
//! both discs in the iidx16-bms work.
//!
//! The files are returned as stored: charts are still encrypted and
//! compressed, sounds still ADPCM.

use std::io::{self, Read, Seek};

use crate::iso9660::{FileExtent, Volume};
use crate::sector::SectorReader;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edition {
    /// Disc 1, `SLPM_552.21`.
    Empress,
    /// Disc 2, `SLPM_552.22`.
    PremiumBest,
}

/// Where one edition keeps its tables inside the boot executable (file offsets).
struct Layout {
    executable: &'static str,
    /// The file tables of `BM2DX16A/B/C.DAT`, in that order: (offset, bytes).
    file_tables: [(usize, usize); 3],
    /// The song table: (offset, bytes).
    songs: (usize, usize),
}

const EMPRESS: Layout = Layout {
    executable: "SLPM_552.21",
    file_tables: [(0x13c370, 0x2a0), (0x13c610, 0x28c8), (0x13eed8, 0x660)],
    songs: (0x178bf0, 0x7e54),
};

const PREMIUM_BEST: Layout = Layout {
    executable: "SLPM_552.22",
    file_tables: [(0x140e90, 0x2a0), (0x141130, 0x28f8), (0x143a28, 0x660)],
    songs: (0x17e0f0, 0x7850),
};

const DAT_FILES: [&str; 3] = ["BM2DX16A.DAT", "BM2DX16B.DAT", "BM2DX16C.DAT"];

impl Edition {
    pub fn from_executable(name: &str) -> Option<Self> {
        [Edition::Empress, Edition::PremiumBest]
            .into_iter()
            .find(|e| e.layout().executable.eq_ignore_ascii_case(name))
    }

    pub fn name(self) -> &'static str {
        match self {
            Edition::Empress => "beatmania IIDX 16 EMPRESS",
            Edition::PremiumBest => "beatmania IIDX 16 PREMIUM BEST",
        }
    }

    fn layout(self) -> &'static Layout {
        match self {
            Edition::Empress => &EMPRESS,
            Edition::PremiumBest => &PREMIUM_BEST,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Style {
    Single,
    Double,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Difficulty {
    Beginner,
    Normal,
    Hyper,
    Another,
    Black,
}

/// The chart slots of a song record, in the order its chart file numbers are stored.
const CHART_SLOTS: [(Style, Difficulty); 10] = [
    (Style::Single, Difficulty::Hyper),
    (Style::Double, Difficulty::Hyper),
    (Style::Single, Difficulty::Another),
    (Style::Double, Difficulty::Another),
    (Style::Single, Difficulty::Normal),
    (Style::Double, Difficulty::Normal),
    (Style::Single, Difficulty::Beginner),
    (Style::Double, Difficulty::Beginner),
    (Style::Single, Difficulty::Black),
    (Style::Double, Difficulty::Black),
];

/// Where a slot's level is in the record's level bytes (SP N/H/A/B/Black, then DP).
fn level_index(style: Style, difficulty: Difficulty) -> usize {
    let base = match style {
        Style::Single => 0,
        Style::Double => 5,
    };
    base + match difficulty {
        Difficulty::Normal => 0,
        Difficulty::Hyper => 1,
        Difficulty::Another => 2,
        Difficulty::Beginner => 3,
        Difficulty::Black => 4,
    }
}

const RECORD_SIZE: usize = 0x134;
const TITLE_LEN: usize = 0x40;
const LEVELS_AT: usize = 0x4a;
const CHARTS_AT: usize = 0xc0;
const SOUNDS_AT: usize = 0xe8;

/// One chart of a song.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chart {
    pub style: Style,
    pub difficulty: Difficulty,
    /// The level the game shows (0 when the table has none).
    pub level: u8,
    /// The chart's file number.
    pub file: u32,
}

/// One song: a record of the song table that has at least one chart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Song {
    /// The record's index in the song table.
    pub id: u32,
    /// The title as the disc stores it (ASCII; other bytes become `?`).
    pub title: String,
    /// In `Style`, then `Difficulty` order.
    pub charts: Vec<Chart>,
    /// The sound file numbers as stored (key sounds and BGM per chart pair; `0xffff` is none).
    pub sounds: [u16; 16],
}

/// A file's span inside one of the DAT files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileEntry {
    /// 0, 1, 2 for `BM2DX16A/B/C.DAT`.
    pub dat: u8,
    pub offset: u64,
    pub size: u32,
}

/// The song list of one IIDX 16 disc and where its files are.
#[derive(Debug, Clone)]
pub struct Disc {
    pub edition: Edition,
    pub songs: Vec<Song>,
    files: Vec<FileEntry>,
    dats: [FileExtent; 3],
}

fn missing(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{what} is not on the disc"),
    )
}

impl Disc {
    /// Reads the song and file tables from the disc's boot executable.
    pub fn open<R: Read + Seek>(
        disc: &mut SectorReader<R>,
        volume: &Volume,
        edition: Edition,
    ) -> io::Result<Self> {
        let layout = edition.layout();
        let exe = volume
            .read_file(disc, layout.executable)?
            .ok_or_else(|| missing(layout.executable))?;
        let mut dats = [FileExtent { lba: 0, size: 0 }; 3];
        for (slot, name) in dats.iter_mut().zip(DAT_FILES) {
            *slot = volume.find(disc, name)?.ok_or_else(|| missing(name))?;
        }
        let files = read_file_tables(&exe, &layout.file_tables)?;
        let songs = read_songs(&exe, layout.songs)?;
        Ok(Self {
            edition,
            songs,
            files,
            dats,
        })
    }

    /// Where file number `file` is, or `None` for a number the tables lack.
    pub fn file(&self, file: u32) -> Option<FileEntry> {
        self.files.get(file as usize).copied()
    }

    /// File number `file` as stored on the disc.
    pub fn read_file<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        file: u32,
    ) -> io::Result<Vec<u8>> {
        let entry = self
            .file(file)
            .ok_or_else(|| missing(&format!("file {file}")))?;
        let dat = self.dats[entry.dat as usize];
        if entry.offset + entry.size as u64 > dat.size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "file {file} runs past the end of {}",
                    DAT_FILES[entry.dat as usize]
                ),
            ));
        }
        disc.read_at(dat.offset() + entry.offset, entry.size as usize)
    }
}

fn table(exe: &[u8], (at, len): (usize, usize)) -> io::Result<&[u8]> {
    exe.get(at..at + len).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "the executable is shorter than its tables",
        )
    })
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// The file tables, numbered across all three DAT files in order. Each record
/// is 12 bytes: sector inside the DAT, an unused word, size in bytes.
fn read_file_tables(exe: &[u8], tables: &[(usize, usize); 3]) -> io::Result<Vec<FileEntry>> {
    let mut files = Vec::new();
    for (dat, &span) in tables.iter().enumerate() {
        for record in table(exe, span)?.chunks_exact(12) {
            files.push(FileEntry {
                dat: dat as u8,
                offset: u32_at(record, 0) as u64 * crate::SECTOR_SIZE,
                size: u32_at(record, 8),
            });
        }
    }
    Ok(files)
}

fn read_songs(exe: &[u8], span: (usize, usize)) -> io::Result<Vec<Song>> {
    let mut songs = Vec::new();
    for (id, record) in table(exe, span)?.chunks_exact(RECORD_SIZE).enumerate() {
        let levels = &record[LEVELS_AT..LEVELS_AT + 10];
        let mut charts: Vec<Chart> = CHART_SLOTS
            .iter()
            .enumerate()
            .filter_map(|(i, &(style, difficulty))| {
                let file = u32_at(record, CHARTS_AT + i * 4);
                (file != 0 && file != u32::MAX).then(|| Chart {
                    style,
                    difficulty,
                    level: levels[level_index(style, difficulty)],
                    file,
                })
            })
            .collect();
        if charts.is_empty() {
            continue;
        }
        charts.sort_by_key(|c| (c.style, c.difficulty));
        let mut sounds = [0u16; 16];
        for (i, sound) in sounds.iter_mut().enumerate() {
            let at = SOUNDS_AT + i * 2;
            *sound = u16::from_le_bytes([record[at], record[at + 1]]);
        }
        let title = record[..TITLE_LEN]
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| if b.is_ascii() { b as char } else { '?' })
            .collect::<String>()
            .trim()
            .to_string();
        songs.push(Song {
            id: id as u32,
            title,
            charts,
            sounds,
        });
    }
    Ok(songs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(title: &str, levels: [u8; 10], charts: [u32; 10]) -> Vec<u8> {
        let mut r = vec![0u8; RECORD_SIZE];
        r[..title.len()].copy_from_slice(title.as_bytes());
        r[LEVELS_AT..LEVELS_AT + 10].copy_from_slice(&levels);
        for (i, c) in charts.iter().enumerate() {
            r[CHARTS_AT + i * 4..][..4].copy_from_slice(&c.to_le_bytes());
        }
        r
    }

    #[test]
    fn a_record_lists_its_charts_with_their_levels() {
        // SP N 4, SP H 7, SP A 10, DP H 8; charts stored H, DPH, A, DPA, N, …
        let levels = [4, 7, 10, 0, 0, 0, 8, 0, 0, 0];
        let charts = [11, 12, 13, 0, 14, u32::MAX, 0, 0, 0, 0];
        let table = record("ANISAKIS", levels, charts);
        let songs = read_songs(&table, (0, RECORD_SIZE)).unwrap();
        assert_eq!(songs.len(), 1);
        assert_eq!(songs[0].title, "ANISAKIS");
        let got: Vec<(Style, Difficulty, u8, u32)> = songs[0]
            .charts
            .iter()
            .map(|c| (c.style, c.difficulty, c.level, c.file))
            .collect();
        assert_eq!(
            got,
            [
                (Style::Single, Difficulty::Normal, 4, 14),
                (Style::Single, Difficulty::Hyper, 7, 11),
                (Style::Single, Difficulty::Another, 10, 13),
                (Style::Double, Difficulty::Hyper, 8, 12),
            ]
        );
    }

    #[test]
    fn a_record_without_charts_is_not_a_song() {
        let mut table = record("", [0; 10], [0; 10]);
        table.extend(record("B", [1; 10], [5; 10]));
        let songs = read_songs(&table, (0, RECORD_SIZE * 2)).unwrap();
        assert_eq!(songs.len(), 1);
        assert_eq!(songs[0].id, 1);
    }

    #[test]
    fn file_numbers_run_across_the_three_dats() {
        let mut exe = Vec::new();
        for (sector, size) in [(1u32, 10u32), (2, 20), (3, 30)] {
            exe.extend(sector.to_le_bytes());
            exe.extend(0u32.to_le_bytes());
            exe.extend(size.to_le_bytes());
        }
        let files = read_file_tables(&exe, &[(0, 12), (12, 24), (36, 0)]).unwrap();
        assert_eq!(
            files,
            [
                FileEntry {
                    dat: 0,
                    offset: 2048,
                    size: 10
                },
                FileEntry {
                    dat: 1,
                    offset: 4096,
                    size: 20
                },
                FileEntry {
                    dat: 1,
                    offset: 6144,
                    size: 30
                },
            ]
        );
    }

    #[test]
    fn editions_are_told_by_their_executable() {
        assert_eq!(
            Edition::from_executable("SLPM_552.21"),
            Some(Edition::Empress)
        );
        assert_eq!(
            Edition::from_executable("slpm_552.22"),
            Some(Edition::PremiumBest)
        );
        assert_eq!(Edition::from_executable("SLPM_000.00"), None);
    }
}

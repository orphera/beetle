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
use crate::meta::SongMeta;
use crate::ply::Ply;
use crate::ply_bms::{to_bms, Header};
use crate::ps2tex::{decode_lzss, decode_psmt8, texture_size, Rgba};
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
    /// The `DATA1.DAT` container table: (offset, bytes).
    data1: (usize, usize),
}

const EMPRESS: Layout = Layout {
    executable: "SLPM_552.21",
    file_tables: [(0x13c370, 0x2a0), (0x13c610, 0x28c8), (0x13eed8, 0x660)],
    songs: (0x178bf0, 0x7e54),
    data1: (0x139d00, 0x1cf0),
};

const PREMIUM_BEST: Layout = Layout {
    executable: "SLPM_552.22",
    file_tables: [(0x140e90, 0x2a0), (0x141130, 0x28f8), (0x143a28, 0x660)],
    songs: (0x17e0f0, 0x7850),
    data1: (0x139980, 0x5bd0),
};

/// The Blowfish key of the chart files. iidx-ps2tools builds it from letters of
/// option names in the game (`generate_encryption_key`); this is its result.
const CHART_KEY: &[u8] = b"uYlFgVwOpaQmXdIR";

const DAT_FILES: [&str; 3] = ["BM2DX16A.DAT", "BM2DX16B.DAT", "BM2DX16C.DAT"];

impl Edition {
    pub fn from_executable(name: &str) -> Option<Self> {
        [Edition::Empress, Edition::PremiumBest]
            .into_iter()
            .find(|e| e.layout().executable.eq_ignore_ascii_case(name))
    }

    /// The disc's entry in the title list: its name and chart layout.
    pub fn title(self) -> &'static crate::titles::Title {
        crate::titles::find(self.layout().executable).expect("IIDX 16 is listed")
    }

    /// A short file-name-safe name (`iidx16-empress`).
    pub fn slug(self) -> &'static str {
        match self {
            Edition::Empress => "iidx16-empress",
            Edition::PremiumBest => "iidx16-premium-best",
        }
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
const VIDEO_AT: usize = 0x58;

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

impl Chart {
    /// The chart's name on its disc, `<song id>/<style>_<difficulty>`
    /// (`017/sp_another`): what a player stores to find it again.
    pub fn key(&self, song: &Song) -> String {
        let style = match self.style {
            Style::Single => "sp",
            Style::Double => "dp",
        };
        let difficulty = match self.difficulty {
            Difficulty::Beginner => "beginner",
            Difficulty::Normal => "normal",
            Difficulty::Hyper => "hyper",
            Difficulty::Another => "another",
            Difficulty::Black => "black",
        };
        format!("{:03}/{style}_{difficulty}", song.id)
    }
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
    /// The `DATA1.DAT` container of the song's loading card, by the rule
    /// found on PREMIUM BEST (the last `(0, 0x5xx..0x6xx)` word pair that is
    /// not a shared UI container). EMPRESS records do not follow it, so its
    /// cards come from a hand-made table (`meta`).
    pub card: Option<u32>,
    /// The file number of the song's background video (MPEG-2, stored as
    /// is), the first of the record's two video words as iidx-ps2tools reads them.
    pub video: Option<u32>,
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
    data1: FileExtent,
    /// `DATA1.DAT` containers: (byte offset, bytes).
    containers: Vec<(u64, u64)>,
}

fn missing(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{what} is not on the disc"),
    )
}

impl Disc {
    /// Read and parse a chart without serializing it to BMS text.
    pub fn chart_ply<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        chart: &Chart,
    ) -> io::Result<Ply> {
        let bytes = self.read_chart(disc, chart.file)?;
        Ply::parse_as(&bytes, self.edition.title().charts)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }

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
        let data1 = volume
            .find(disc, "DATA1.DAT")?
            .ok_or_else(|| missing("DATA1.DAT"))?;
        let files = read_file_tables(&exe, &layout.file_tables)?;
        let songs = read_songs(&exe, layout.songs)?;
        // EMPRESS cards come from DATA1 containers (verified against empress_cards.py);
        // do not force None -- card_container determines it from the song record.
        // Previous unconditional None was the root cause of stagefile read failures.
        // Container records: id, sector, sectors, 0.
        let containers = table(&exe, layout.data1)?
            .chunks_exact(16)
            .map(|r| {
                (
                    u32_at(r, 4) as u64 * crate::SECTOR_SIZE,
                    u32_at(r, 8) as u64 * crate::SECTOR_SIZE,
                )
            })
            .collect();
        Ok(Self {
            edition,
            songs,
            files,
            dats,
            data1,
            containers,
        })
    }

    /// The first picture of `DATA1.DAT` container `container` (its file 0 is
    /// not a picture): a song's loading card. `None` when it has none.
    pub fn card_image<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        container: u32,
    ) -> io::Result<Option<Rgba>> {
        let Some(&(offset, size)) = self.containers.get(container as usize) else {
            return Ok(None);
        };
        if size == 0 || offset + size > self.data1.size {
            return Ok(None);
        }
        let bytes = disc.read_at(self.data1.offset() + offset, size as usize)?;
        Ok(first_picture(&bytes))
    }

    /// The song and chart a `Chart::key` names.
    pub fn find(&self, key: &str) -> Option<(&Song, &Chart)> {
        let (id, _) = key.split_once('/')?;
        let id: u32 = id.parse().ok()?;
        let song = self.songs.iter().find(|s| s.id == id)?;
        let chart = song.charts.iter().find(|c| c.key(song) == key)?;
        Some((song, chart))
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

impl Disc {
    /// Chart file number `file`, decrypted and decompressed: the `.ply` bytes.
    pub fn read_chart<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        file: u32,
    ) -> io::Result<Vec<u8>> {
        let stored = self.read_file(disc, file)?;
        let plain = crate::blowfish::decrypt_file(CHART_KEY, &stored);
        crate::lz::decode(&plain).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("chart file {file} does not decompress"),
            )
        })
    }
}

/// Where a record keeps the (key sound bank, BGM) file numbers of the chart
/// slots, in `CHART_SLOTS` order; the BLACK slots have none.
const SOUND_PAIRS: [(usize, usize); 8] = [
    (0, 2),
    (1, 3),
    (4, 6),
    (5, 7),
    (8, 10),
    (9, 11),
    (12, 14),
    (13, 15),
];

/// No file.
const NO_SOUND: u16 = 0xffff;

impl Song {
    /// The (key sound bank, BGM stream) file numbers a chart plays. A chart
    /// slot without its own takes DP HYPER's (double) or SP HYPER's, field by
    /// field, as iidx-ps2tools resolves them.
    pub fn sound_files(&self, style: Style, difficulty: Difficulty) -> (Option<u32>, Option<u32>) {
        let own = |slot: usize, field: usize| -> Option<u32> {
            let pair = SOUND_PAIRS.get(slot)?;
            let index = if field == 0 { pair.0 } else { pair.1 };
            let file = self.sounds[index];
            (file != NO_SOUND).then_some(file as u32)
        };
        let slot = CHART_SLOTS
            .iter()
            .position(|&s| s == (style, difficulty))
            .unwrap();
        let pick = |field: usize| {
            own(slot, field)
                .or_else(|| (style == Style::Double).then(|| own(1, field)).flatten())
                .or_else(|| own(0, field))
        };
        (pick(0), pick(1))
    }
}

/// The file name a disc chart's BMS gives sound `n`: its number in the song's
/// sound bank.
pub fn sound_name(n: u16) -> String {
    format!("{n:04}.wav")
}

/// The sound number `sound_name` gave `name`.
pub fn sound_number(name: &str) -> Option<u16> {
    let digits = name.strip_suffix(".wav")?;
    (digits.len() == 4).then(|| digits.parse().ok()).flatten()
}

impl Disc {
    /// The chart as BMS text (`ply_bms`), titled with `meta` where it says
    /// more than the disc does. Without `meta` the text depends on the disc
    /// alone, so it can identify the chart.
    pub fn chart_bms<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        song: &Song,
        chart: &Chart,
        meta: Option<&SongMeta>,
    ) -> io::Result<String> {
        let known =
            |pick: fn(&SongMeta) -> &String| meta.map(pick).filter(|v| !v.is_empty()).cloned();
        let bytes = self.read_chart(disc, chart.file)?;
        let invalid = |e: String| io::Error::new(io::ErrorKind::InvalidData, e);
        let ply = Ply::parse_as(&bytes, self.edition.title().charts)
            .map_err(|e| invalid(e.to_string()))?;
        let header = Header {
            title: known(|m| &m.title).unwrap_or_else(|| song.title.clone()),
            subtitle: known(|m| &m.subtitle).unwrap_or_default(),
            artist: known(|m| &m.artist).unwrap_or_default(),
            genre: known(|m| &m.genre).unwrap_or_default(),
            level: chart.level,
            difficulty: match chart.difficulty {
                Difficulty::Beginner => 1,
                Difficulty::Normal => 2,
                Difficulty::Hyper => 3,
                Difficulty::Another => 4,
                Difficulty::Black => 5,
            },
            double: chart.style == Style::Double,
            sound_name,
        };
        to_bms(&ply, &header).map_err(|e| invalid(e.to_string()))
    }
}

/// A container: `u16 data offset, u16 count`, (decompressed, compressed)
/// `u32` size pairs for `count + 1` files ending at the data offset, then the
/// files back to back.
fn first_picture(container: &[u8]) -> Option<Rgba> {
    let data = u16::from_le_bytes([*container.first()?, *container.get(1)?]) as usize;
    let count = u16::from_le_bytes([*container.get(2)?, *container.get(3)?]) as usize;
    let sizes = data.checked_sub((count + 1) * 8)?;
    let mut at = data;
    for k in 0..=count {
        let unpacked = u32_at(container.get(sizes + k * 8..sizes + k * 8 + 4)?, 0) as usize;
        let packed = u32_at(container.get(sizes + k * 8 + 4..sizes + k * 8 + 8)?, 0) as usize;
        if k > 0 {
            if let Some((w, h)) = texture_size(unpacked) {
                let raw = decode_lzss(container.get(at..)?, unpacked)?;
                return decode_psmt8(&raw, w, h);
            }
        }
        at += packed;
    }
    None
}

/// Containers every song record names that are shared UI, not a song's card.
const SHARED_CONTAINERS: [u32; 6] = [0x520, 0x521, 0x522, 0x523, 0x6a6, 0x5a8];

fn card_container(record: &[u8]) -> Option<u32> {
    let words: Vec<u32> = record
        .chunks_exact(2)
        .map(|w| u16::from_le_bytes([w[0], w[1]]) as u32)
        .collect();
    words
        .windows(2)
        .filter(|w| {
            w[0] == 0 && (0x500..0x700).contains(&w[1]) && !SHARED_CONTAINERS.contains(&w[1])
        })
        .map(|w| w[1])
        .next_back()
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
            card: card_container(record),
            video: {
                let file = u32_at(record, VIDEO_AT);
                (file != 0 && file & 0xffff != 0xffff).then_some(file)
            },
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
    fn a_chart_is_found_again_by_its_key() {
        let levels = [4, 7, 10, 0, 0, 0, 8, 0, 0, 0];
        let charts = [11, 12, 13, 0, 14, 0, 0, 0, 0, 0];
        let mut table = record("A", [0; 10], [0; 10]);
        table.extend(record("B", levels, charts));
        let disc = Disc {
            edition: Edition::Empress,
            songs: read_songs(&table, (0, RECORD_SIZE * 2)).unwrap(),
            files: Vec::new(),
            dats: [FileExtent { lba: 0, size: 0 }; 3],
            data1: FileExtent { lba: 0, size: 0 },
            containers: Vec::new(),
        };
        let song = &disc.songs[0];
        let key = song.charts[1].key(song);
        assert_eq!(key, "001/sp_hyper");
        let (found_song, found) = disc.find(&key).unwrap();
        assert_eq!((found_song.id, found.file), (1, 11));
        assert!(disc.find("001/dp_black").is_none());
    }

    #[test]
    fn sound_names_read_back() {
        assert_eq!(sound_number(&sound_name(42)), Some(42));
        assert_eq!(sound_number("0042.ogg"), None);
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

//! beatmania IIDX 15 DJ Troopers (PS2): the song list
//! and where each song's files are.
//!
//! The boot executable holds both. Its song table has one 0x134-byte record per
//! song slot (title, levels, file numbers of the charts and sounds); its file
//! tables give each file number a span inside `BM2DX15A/B/C.DAT`. Offsets come
//! from iidx-ps2tools (`plugins/iidx15thcs.py`) and were checked against
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
    /// DJ Troopers
    DJTroopers,
}

/// Where one edition keeps its tables inside the boot executable (file offsets).
struct Layout {
    executable: &'static str,
    /// The file tables of `BM2DX15A/B/C.DAT`, in that order: (offset, bytes).
    file_tables: [(usize, usize); 3],
    /// The song table: (offset, bytes).
    songs: (usize, usize),
    /// The `DATA1.DAT` container table: (offset, bytes).
    data1: (usize, usize),
}

const DJTROOPERS: Layout = Layout {
    executable: "SLPM_551.17",
    file_tables: [(0x134020, 0x240), (0x134260, 0x27c0), (0x136a20, 0x630)],
    songs: (0x16fe60, 0x80bc),
    data1: (0x131680, 0x1f80),
};

/// The Blowfish key of the chart files. iidx-ps2tools builds it from letters of
/// option names in the game (`generate_encryption_key`); this is its result.
const CHART_KEY: &[u8] = b"eqkBipx2wgExUAIG";

const DAT_FILES: [&str; 3] = ["BM2DX15A.DAT", "BM2DX15B.DAT", "BM2DX15C.DAT"];

impl Edition {
    pub fn from_executable(name: &str) -> Option<Self> {
        [Edition::DJTroopers]
            .into_iter()
            .find(|e| e.layout().executable.eq_ignore_ascii_case(name))
    }

    /// The disc's entry in the title list: its name and chart layout.
    pub fn title(self) -> &'static crate::titles::Title {
        crate::titles::find(self.layout().executable).expect("IIDX 15 is listed")
    }

    /// A short file-name-safe name (`iidx15-dj-troopers`).
    pub fn slug(self) -> &'static str {
        match self {
            Edition::DJTroopers => "iidx15-dj-troopers",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Edition::DJTroopers => "beatmania IIDX 15 DJ TROOPERS",
        }
    }

    fn layout(self) -> &'static Layout {
        match self {
            Edition::DJTroopers => &DJTROOPERS,
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
/// The DATA1 animation file used for the song's loading/stage overlay.
/// This is a file-table index, not a DATA1 container number.
const OVERLAY_AT: usize = 0x94;

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
    /// The DATA1 animation file-table entry for the song's stage overlay.
    pub overlay: Option<u32>,
    /// The file number of the song's background video (MPEG-2, stored as
    /// is), the first of the record's two video words as iidx-ps2tools reads them.
    pub video: Option<u32>,
}

/// A file's span inside one of the DAT files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileEntry {
    /// 0, 1, 2 for `BM2DX15A/B/C.DAT`.
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
    data1_files: Vec<FileEntry>,
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
        let data1 = volume
            .find(disc, "DATA1.DAT")?
            .ok_or_else(|| missing("DATA1.DAT"))?;
        let files = read_file_tables(&exe, &layout.file_tables)?;
        let songs = read_songs(&exe, layout.songs)?;
        // EMPRESS cards come from DATA1 containers (verified against empress_cards.py);
        // do not force None -- card_container determines it from the song record.
        // Previous unconditional None was the root cause of stagefile read failures.
        // Container records: id, sector, sectors, 0.
        let data1_files = table(&exe, layout.data1)?
            .chunks_exact(16)
            .map(|r| FileEntry {
                dat: 0,
                offset: u32_at(r, 4) as u64 * crate::SECTOR_SIZE,
                size: u32_at(r, 8).saturating_mul(crate::SECTOR_SIZE as u32),
            })
            .collect();
        Ok(Self {
            edition,
            songs,
            files,
            dats,
            data1,
            data1_files,
        })
    }

    /// Reads a song's DATA1 file. The stage image is the first texture in the
    /// file's internal archive (`e01` in the extraction tools).
    pub fn read_overlay<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        file: u32,
    ) -> io::Result<Vec<u8>> {
        let entry = self
            .data1_files
            .get(file as usize)
            .ok_or_else(|| missing(&format!("DATA1 file {file}")))?;
        if entry.offset + entry.size as u64 > self.data1.size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "overlay runs past DATA1",
            ));
        }
        disc.read_at(self.data1.offset() + entry.offset, entry.size as usize)
    }

    /// Decodes the static stage image used by DJ TROOPERS.
    pub fn stage_image<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        song: &Song,
    ) -> io::Result<Option<Rgba>> {
        let Some(file) = song.overlay else {
            return Ok(None);
        };
        let archive = self.read_overlay(disc, file)?;
        let Some(texture) = archive_texture(&archive, 1) else {
            return Ok(None);
        };
        let Some((width, height)) = texture_size(texture.0) else {
            return Ok(None);
        };
        Ok(decode_psmt8(&texture.1, width, height))
    }

    /// Legacy card accessor retained for the old diagnostic path. IIDX 15
    /// stage art is an animation, not a single DATA1 picture.
    pub fn card_image<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        container: u32,
    ) -> io::Result<Option<Rgba>> {
        let Some(entry) = self.data1_files.get(container as usize) else {
            return Ok(None);
        };
        let offset = entry.offset;
        let size = entry.size as u64;
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

/// Returns `(decompressed_size, decompressed_texture)` for an internal DATA1
/// file. Entry zero is metadata; entry one is the static stage texture.
fn archive_texture(archive: &[u8], wanted: usize) -> Option<(usize, Vec<u8>)> {
    let data = u16::from_le_bytes([*archive.first()?, *archive.get(1)?]) as usize;
    let count = u16::from_le_bytes([*archive.get(2)?, *archive.get(3)?]) as usize;
    let table_size = (count + 1).checked_mul(8)?;
    let table = data.checked_sub(table_size)?;
    let mut at = data;
    for index in 0..=count {
        let record = archive.get(table + index * 8..table + index * 8 + 8)?;
        let unpacked = u32_at(record, 0) as usize;
        let packed = u32_at(record, 4) as usize;
        if index == wanted {
            let bytes = archive.get(at..at + packed)?;
            return Some((unpacked, crate::ps2tex::decode_lzss(bytes, unpacked)?));
        }
        at = at.checked_add(packed)?;
    }
    None
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
            overlay: {
                let file = u32_at(record, OVERLAY_AT);
                (file != 0 && file != u32::MAX).then_some(file)
            },
            video: {
                let file = u32_at(record, VIDEO_AT);
                (file != 0 && file & 0xffff != 0xffff).then_some(file)
            },
        });
    }
    Ok(songs)
}

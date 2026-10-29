//! The PS2 pop'n music discs this crate reads (12 いろは, 13 カーニバル,
//! 14 FEVER!): where each song's files are and how its charts read.
//!
//! Found by reading the discs; no tool or document described them.
//!
//! - `SD.DAT` holds every song file back to back. The boot executable lists
//!   where each starts, as a table of increasing sector numbers whose last
//!   entry is the file's end. A song has three: the key sound bank (the IIDX
//!   `.wvb` layout, `"ef"` on 12 and `"wp"` on 13 and 14), the chart file and
//!   the BGM stream (the IIDX `.pcm` layout), in that order: the record names
//!   the chart. Some songs have two sets, `bank, bank, chart, BGM, BGM`, the
//!   first bank with the first BGM; each chart plays one of them
//!   (`pick_sound_sets`).
//! - The song table: 0x58-byte records, `u32` pointers to the genre and the
//!   comment (Shift-JIS) at 0 and 4, the chart file at 0x4c (`u32::MAX` for an
//!   empty slot), level bytes at 0x51: NORMAL, HYPER, 5-button (EASY), EX, then
//!   two battle levels. A level of 0 means no chart.
//! - A chart file: eight `u32` offsets of event lists, in the order NORMAL,
//!   HYPER, 5-button, EX, battle, then two short demo lists and an empty one.
//!   An absent chart is a 30-note placeholder. Events are 8 bytes,
//!   `u32 time (ms), u8 button, u8 command, u16 value`.
//!
//! Titles are pictures, not text, so a song is named by its genre, as the game
//! lists them, and its banner (genre and title, from `IMAGE.DAT`) is its
//! loading picture (`banner`). Format notes: `docs/specs/popn_ps2_disc.md`.

use crate::iidx::{Chart, Difficulty, FileEntry, Song, Style};
use crate::ply::{Command, Event, Ply, Side};
use crate::ps2tex::{decode_psmt8, Rgba};

/// Where one disc keeps its tables inside the boot executable (file offsets).
pub(crate) struct Layout {
    pub executable: &'static str,
    pub slug: &'static str,
    /// The `SD.DAT` sector table: (offset, entries).
    pub files: (usize, usize),
    /// The song table: (offset, records).
    pub songs: (usize, usize),
    /// The `IMAGE.DAT` sector table: (offset, entries).
    pub images: (usize, usize),
    /// The `IMAGE.DAT` file of the song banners (`banner`).
    pub banners: u32,
    /// The `IMAGE.DAT` files of the title and artist word pictures, when the
    /// disc's records name them (`name_refs`). `None` where not yet read.
    pub names: Option<NameFiles>,
    /// The data file of the key sounds, charts and BGMs.
    pub dat: &'static str,
    pub record: Record,
}

/// How a disc's song records read.
#[derive(Clone, Copy)]
pub(crate) struct Record {
    pub size: usize,
    /// The chart file number: `u32` on 12 to 14, `u16` on 10.
    pub chart_at: usize,
    pub chart_u16: bool,
    pub levels_at: usize,
    /// The banner's rectangle (`u16`), where the record names one.
    pub banner_at: Option<usize>,
    /// The genre name pointer (Shift-JIS), where the record has one. 10's
    /// records point only at the comment; its songs are named by number.
    pub genre_at: Option<usize>,
    /// Whether a song has a BGM stream after its chart (12 to 14). 10's
    /// songs are key sounds alone, and its BGM start names a key sound.
    pub bgm: bool,
}

/// 12 to 14: 0x58 bytes.
const RECORD_12: Record = Record {
    size: 0x58,
    chart_at: 0x4c,
    chart_u16: false,
    levels_at: 0x51,
    banner_at: Some(0x40),
    genre_at: Some(0),
    bgm: true,
};

/// pop'n music 10: the songs in `SDS.DAT` (a key sound bank, then the chart),
/// 0x38-byte records. `SDB.DAT`'s streams are not the songs' BGMs (no record
/// field matches them), and the charts play over a thousand background
/// sounds each: the music is key sounds.
pub(crate) const POPN10: Layout = Layout {
    executable: "SLPM_657.70",
    slug: "popn10",
    files: (0xbe1a0, 233),
    songs: (0xccae0, 106),
    images: (0xbffd0, 683),
    banners: u32::MAX,
    names: None,
    dat: "SDS.DAT",
    record: Record {
        size: 0x38,
        chart_at: 0x2c,
        chart_u16: true,
        levels_at: 0x30,
        banner_at: None,
        genre_at: None,
        bgm: false,
    },
};

/// The `IMAGE.DAT` files holding the title and artist word pictures. Each
/// file's part 0 lists the words as rectangles (`rect_picture`).
#[derive(Clone, Copy)]
pub(crate) struct NameFiles {
    pub title: u32,
    pub artist: u32,
}

pub(crate) const POPN12: Layout = Layout {
    executable: "SLPM_663.14",
    slug: "popn12-iroha",
    files: (0xddb00, 337),
    songs: (0xf6d30, 106),
    images: (0xde120, 600),
    banners: 3,
    names: Some(NameFiles {
        title: 54,
        artist: 56,
    }),
    dat: DAT_FILE,
    record: RECORD_12,
};

pub(crate) const POPN13: Layout = Layout {
    executable: "SLPM_665.33",
    slug: "popn13-carnival",
    files: (0x10c420, 372),
    songs: (0x1339a0, 129),
    images: (0x10a420, 654),
    banners: 347,
    names: Some(NameFiles {
        title: 349,
        artist: 160,
    }),
    dat: DAT_FILE,
    record: RECORD_12,
};

pub(crate) const POPN14: Layout = Layout {
    executable: "SLPM_667.42",
    slug: "popn14-fever",
    files: (0x119f00, 349),
    songs: (0x13eb50, 127),
    images: (0x11ab20, 674),
    banners: 3,
    names: Some(NameFiles {
        title: 68,
        artist: 71,
    }),
    dat: DAT_FILE,
    record: RECORD_12,
};

/// The data file every pop'n disc keeps its songs in.
pub(crate) const DAT_FILE: &str = "SD.DAT";
/// The data file of its pictures.
pub(crate) const IMAGE_FILE: &str = "IMAGE.DAT";

/// The sound number a pop'n chart's BGM start plays: BMS `ZY`, the last id
/// before the `ZZ` silent notes take. Key sounds run from 1 to under 700.
pub const BGM_SOUND: u16 = 1294;

/// The record's title and artist word rectangles (`u16` each), in the title
/// and artist files of `NameFiles`. Found on 12, 13 and 14 (checked by eye).
const TITLE_AT: usize = 0x10;
const ARTIST_AT: usize = 0x18;
/// The executable's load address, less its file offset.
const LOAD: u32 = 0x100000 - 0x80;

/// The charts a record lists, by its level bytes: (difficulty, level byte).
/// EASY (the 5-button chart) is listed as BEGINNER and EX as ANOTHER.
const LEVELS: [(Difficulty, usize); 4] = [
    (Difficulty::Beginner, 2),
    (Difficulty::Normal, 0),
    (Difficulty::Hyper, 1),
    (Difficulty::Another, 3),
];

/// Which event list of a chart file a difficulty is.
fn list_of(difficulty: Difficulty) -> usize {
    match difficulty {
        Difficulty::Normal => 0,
        Difficulty::Hyper => 1,
        Difficulty::Beginner => 2,
        Difficulty::Another | Difficulty::Black => 3,
    }
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// The `SD.DAT` files: each runs from its sector to the next one's.
pub(crate) fn read_files(exe: &[u8], layout: &Layout) -> Option<Vec<FileEntry>> {
    let (at, count) = layout.files;
    let sectors: Vec<u32> = (0..count)
        .map(|i| u32_at(exe, at + i * 4))
        .collect::<Option<_>>()?;
    Some(
        sectors
            .windows(2)
            .map(|w| FileEntry {
                dat: 0,
                offset: w[0] as u64 * crate::SECTOR_SIZE,
                size: w[1].saturating_sub(w[0]) * crate::SECTOR_SIZE as u32,
            })
            .collect(),
    )
}

/// The songs: the records with a chart file and at least one level.
pub(crate) fn read_songs(exe: &[u8], layout: &Layout, files: usize) -> Option<Vec<Song>> {
    let (at, count) = layout.songs;
    let form = layout.record;
    let mut songs = Vec::new();
    for id in 0..count {
        let record = exe.get(at + id * form.size..at + (id + 1) * form.size)?;
        let file = if form.chart_u16 {
            u16::from_le_bytes([record[form.chart_at], record[form.chart_at + 1]]) as u32
        } else {
            u32_at(record, form.chart_at)?
        };
        if file == 0 || file as usize + 1 >= files {
            continue;
        }
        let levels = &record[form.levels_at..];
        let charts: Vec<Chart> = LEVELS
            .iter()
            .filter(|&&(_, at)| levels[at] != 0)
            .map(|&(difficulty, at)| Chart {
                style: Style::Single,
                difficulty,
                level: levels[at],
                file,
            })
            .collect();
        if charts.is_empty() {
            continue;
        }
        // The SP HYPER pair, which every chart falls back to.
        let mut sounds = [0xffff; 16];
        sounds[0] = (file - 1) as u16;
        if form.bgm {
            sounds[2] = (file + 1) as u16;
        }
        let title = match form.genre_at {
            Some(genre_at) => u32_at(record, genre_at)?
                .checked_sub(LOAD)
                .and_then(|o| exe.get(o as usize..))
                .map(|s| shift_jis(&s[..s.iter().position(|&b| b == 0).unwrap_or(0)]))
                .unwrap_or_default(),
            None => format!("No.{:03}", id + 1),
        };
        songs.push(Song {
            id: id as u32,
            title,
            charts,
            sounds,
            stage: form
                .banner_at
                .map(|b| u16::from_le_bytes([record[b], record[b + 1]]) as u32),
            stage_picks: None,
            video: None,
        });
    }
    Some(songs)
}

/// The `IMAGE.DAT` files: (byte offset, bytes), each running from its sector
/// to the next one's.
pub(crate) fn read_images(exe: &[u8], layout: &Layout) -> Option<Vec<(u64, u64)>> {
    let (at, count) = layout.images;
    let sectors: Vec<u32> = (0..count)
        .map(|i| u32_at(exe, at + i * 4))
        .collect::<Option<_>>()?;
    Some(
        sectors
            .windows(2)
            .map(|w| {
                (
                    w[0] as u64 * crate::SECTOR_SIZE,
                    w[1].saturating_sub(w[0]) as u64 * crate::SECTOR_SIZE,
                )
            })
            .collect(),
    )
}

/// Decompresses an `IMAGE.DAT` part, as the boot executable's routine at
/// 0x125ed8 on 12 does. There are no control bytes; each code's first byte
/// `a` (and the next, `b`) says what it is:
/// - under 0x80: copy `(a >> 2) + 2` bytes from `(((a & 3) << 8 | b) ^ 0x3ff) + 1`
///   back (2 bytes);
/// - 0x80–0x9f: `(a & 0x1f) + 1` literal bytes follow;
/// - 0xa0–0xbf: as many bytes follow, each written after a zero;
/// - 0xc0–0xdf: `b`, `(a & 0x1f) + 2` times;
/// - 0xe0–0xfe: `(a & 0x1f) + 1` zeros (1 byte);
/// - 0xff: `b + 0x20` zeros;
/// - `7f ff`: the end.
///
/// Stops at the end code or once `size` bytes are out (the size the part
/// states), giving exactly `size` bytes.
pub fn unpack(input: &[u8], size: usize) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = Vec::with_capacity(size);
    let mut i = 0;
    while out.len() < size {
        let a = *input.get(i)?;
        let b = input.get(i + 1).copied().unwrap_or(0);
        match a {
            0x7f if b == 0xff => break,
            0x00..=0x7f => {
                let back = ((((a as usize & 3) << 8) | b as usize) ^ 0x3ff) + 1;
                let start = out.len().checked_sub(back)?;
                for k in 0..(a as usize >> 2) + 2 {
                    out.push(out[start + k]);
                }
                i += 2;
            }
            0x80..=0x9f => {
                let n = (a as usize & 0x1f) + 1;
                out.extend_from_slice(input.get(i + 1..i + 1 + n)?);
                i += n + 1;
            }
            0xa0..=0xbf => {
                let n = (a as usize & 0x1f) + 1;
                for &byte in input.get(i + 1..i + 1 + n)? {
                    out.extend([0, byte]);
                }
                i += n + 1;
            }
            0xc0..=0xdf => {
                out.extend(std::iter::repeat_n(b, (a as usize & 0x1f) + 2));
                i += 2;
            }
            0xe0..=0xfe => {
                out.extend(std::iter::repeat_n(0, (a as usize & 0x1f) + 1));
                i += 1;
            }
            0xff => {
                out.extend(std::iter::repeat_n(0, b as usize + 0x20));
                i += 2;
            }
        }
    }
    out.truncate(size);
    Some(out)
}

/// The parts of an `IMAGE.DAT` file, unpacked: a `u32` offset table (its
/// first entry is its own length; 0 ends it), each part `u32 size` then
/// packed bytes.
fn parts(file: &[u8]) -> Option<Vec<Vec<u8>>> {
    let table = u32_at(file, 0)? as usize;
    (0..table / 4)
        .map_while(|k| u32_at(file, k * 4).filter(|&o| o != 0))
        .map(|at| {
            let at = at as usize;
            let size = u32_at(file, at)? as usize;
            unpack(file.get(at + 4..)?, size)
        })
        .collect()
}

/// Picture `n` of an `IMAGE.DAT` file that lists rectangles: a banner (the
/// song's genre and title, 244x58) or a title or artist word. Part 0 lists
/// rectangles `i16 x, y, w, h` from 0x10, `y` running down the 512-high pages
/// that follow; a page is a 16-byte header (`u16 width, height`, 8 bits a
/// pixel at 8) then a PSMT8 texture.
pub(crate) fn rect_picture(file: &[u8], n: u32) -> Option<Rgba> {
    let parts = parts(file)?;
    let at = 0x10 + n as usize * 8;
    let rect = parts.first()?.get(at..at + 8)?;
    let field = |k: usize| i16::from_le_bytes([rect[k * 2], rect[k * 2 + 1]]);
    let (x, y, w, h) = (field(0), field(1), field(2), field(3));
    if x < 0 || y < 0 || w <= 0 || h <= 0 {
        return None;
    }
    let (x, y, w, h) = (x as u32, y as u32, w as u32, h as u32);
    let page = parts.get(1 + (y / 512) as usize)?;
    let width = u16::from_le_bytes([*page.first()?, *page.get(1)?]) as u32;
    let height = u16::from_le_bytes([*page.get(2)?, *page.get(3)?]) as u32;
    let top = y % 512;
    if *page.get(8)? != 8 || x + w > width || top + h > height {
        return None;
    }
    let image = decode_psmt8(page.get(0x10..)?, width, height)?;
    let pixels = (top..top + h)
        .flat_map(|row| {
            let at = (row * width + x) as usize;
            image.pixels[at..at + w as usize].iter().copied()
        })
        .collect();
    Some(Rgba {
        width: w,
        height: h,
        pixels,
    })
}

/// The coverage from which a column of a word picture counts as ink.
const INK_FLOOR: u8 = 64;

/// A picture as coverage: one byte a pixel, 255 where the ink is. The word
/// pictures are white ink on black with an opaque alpha, so the coverage is
/// the pixel's mean brightness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mask {
    pub width: u32,
    pub height: u32,
    pub coverage: Vec<u8>,
}

impl Mask {
    /// The picture's coverage, cut to the columns that hold ink on either side
    /// (the picture's own margins are not drawn). The height is kept, so every
    /// word of a row keeps its baseline.
    pub(crate) fn from_rgba(picture: &Rgba) -> Mask {
        let (w, h) = (picture.width as usize, picture.height as usize);
        let coverage: Vec<u8> = picture
            .pixels
            .iter()
            .map(|&[r, g, b, _]| ((r as u32 + g as u32 + b as u32) / 3) as u8)
            .collect();
        // The picture's background is not quite black: faint ink-free values
        // would keep a blank margin, so a column counts as ink from INK_FLOOR.
        let inked = |x: usize| (0..h).any(|y| coverage[y * w + x] >= INK_FLOOR);
        let Some(left) = (0..w).find(|&x| inked(x)) else {
            return Mask {
                width: 0,
                height: picture.height,
                coverage: Vec::new(),
            };
        };
        let right = (left..w).rev().find(|&x| inked(x)).unwrap_or(left);
        let cut = right - left + 1;
        let trimmed = (0..h)
            .flat_map(|y| coverage[y * w + left..y * w + left + cut].iter().copied())
            .collect();
        Mask {
            width: cut as u32,
            height: picture.height,
            coverage: trimmed,
        }
    }
}

/// A song's word pictures: its title and its artist. `None` where the
/// record or the disc gives none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SongNames {
    /// The title as the song list's big picture (the detail panel's).
    pub title: Option<Mask>,
    /// The small title picture the list rows use, where the disc has one
    /// (IIDX). `None` means the rows use `title`.
    pub small_title: Option<Mask>,
    pub artist: Option<Mask>,
    /// The genre picture (IIDX), where the disc has one.
    pub genre: Option<Mask>,
    /// Whether the disc is pop'n music (its picture is a banner bar).
    pub popn: bool,
}

/// Each record's title and artist rectangle numbers, in record order. Records
/// past the table's end, and an empty table on a disc without `NameFiles`,
/// read as none.
pub(crate) fn read_name_refs(exe: &[u8], layout: &Layout) -> Vec<(u16, u16)> {
    if layout.names.is_none() {
        return Vec::new();
    }
    let (at, count) = layout.songs;
    (0..count)
        .map(|id| {
            let record = at + id * layout.record.size;
            let field = |k: usize| {
                exe.get(record + k..record + k + 2)
                    .map(|b| u16::from_le_bytes([b[0], b[1]]))
                    .unwrap_or(0)
            };
            (field(TITLE_AT), field(ARTIST_AT))
        })
        .collect()
}

/// The first bytes of a bank and of a BGM stream.
const BANK_MAGICS: [&[u8]; 2] = [b"ef\x01\x00", b"wp\x02\x00"];
const BGM_MAGIC: &[u8] = b"\x01\x00\x64\x08";
/// Where a bank's key sound entries end (`wvb`).
const BANK_TABLE_END: usize = 0x8000;

/// For a song with two sound sets, which one each chart plays. Nothing in the
/// record says (none of its fields differ by difficulty), so it is the
/// sounds: a chart plays the first set when every sound it names is in the
/// first bank, which is the smaller one (13's song 74: NORMAL names up to 139
/// of 139 and ends with the first BGM at 101 s, HYPER up to 507 and ends with
/// the second at 116 s). `peek(file, bytes)` reads the start of an `SD.DAT`
/// file, the whole of it for `usize::MAX`.
pub(crate) fn pick_sound_sets(
    song: &mut Song,
    bgm: bool,
    peek: &mut dyn FnMut(u32, usize) -> Option<Vec<u8>>,
) {
    let Some(file) = song.charts.first().map(|c| c.file) else {
        return;
    };
    if !bgm {
        return pick_banks(song, peek, file);
    }
    let is = |bytes: Option<Vec<u8>>, magics: &[&[u8]]| {
        bytes.is_some_and(|b| magics.iter().any(|m| b.starts_with(m)))
    };
    if !is(peek(file - 2, 4), &BANK_MAGICS) || !is(peek(file + 2, 4), &[BGM_MAGIC]) {
        return;
    }
    // The last key sound the first bank has: entries from 0x10, 16 bytes,
    // numbered from 1, `kind` at 2 is 0 when empty.
    let Some(first) = peek(file - 2, BANK_TABLE_END) else {
        return;
    };
    let in_first = first
        .get(0x10..)
        .unwrap_or_default()
        .chunks_exact(16)
        .rposition(|e| e[2] != 0 || e[3] != 0)
        .map_or(0, |i| i as u16 + 1);
    let Some(bytes) = peek(file, usize::MAX) else {
        return;
    };
    let difficulties: Vec<Difficulty> = song.charts.iter().map(|c| c.difficulty).collect();
    for difficulty in difficulties {
        let most = chart(&bytes, difficulty, true)
            .into_iter()
            .flat_map(|ply| ply.events)
            .filter_map(|e| match e.command {
                Command::Sample { sound, .. } | Command::Auto { sound, .. }
                    if sound != BGM_SOUND =>
                {
                    Some(sound)
                }
                _ => None,
            })
            .max()
            .unwrap_or(0);
        let (bank, bgm) = if most <= in_first {
            (file - 2, file + 1)
        } else {
            (file - 1, file + 2)
        };
        song.set_sound_files(Style::Single, difficulty, bank, bgm);
    }
}

/// The head of a pop'n 8 to 10 key sound bank (`wvb`).
const SPLIT_BANK_HEAD: [u8; 8] = [0x10, 0, 0, 0, 0x10, 0x40, 0, 0];

/// 10: a song with two banks before its chart (`bank, bank, chart`, one
/// song: 33), each chart playing the one that holds every sound it names.
/// Neither bank is a subset of the other's numbers, so the larger names win:
/// a chart takes the second (the one before the chart) when its sounds fit
/// it, the first otherwise.
fn pick_banks(song: &mut Song, peek: &mut dyn FnMut(u32, usize) -> Option<Vec<u8>>, file: u32) {
    let is_bank = |b: &Option<Vec<u8>>| b.as_ref().is_some_and(|b| b.starts_with(&SPLIT_BANK_HEAD));
    if file < 2 || !is_bank(&peek(file - 2, 8)) {
        return;
    }
    // The last sound the bank before the chart has: entries from 0x20 (past
    // the 16-byte head), 16 bytes, `kind` at 0 is 0 when empty.
    let Some(second) = peek(file - 1, 0x4010) else {
        return;
    };
    let in_second = second
        .get(0x20..0x4010)
        .unwrap_or_default()
        .chunks_exact(16)
        .rposition(|e| e[0] != 0 || e[1] != 0)
        .map_or(0, |i| i as u16 + 1);
    let Some(bytes) = peek(file, usize::MAX) else {
        return;
    };
    let difficulties: Vec<Difficulty> = song.charts.iter().map(|c| c.difficulty).collect();
    for difficulty in difficulties {
        let most = chart(&bytes, difficulty, false)
            .into_iter()
            .flat_map(|ply| ply.events)
            .filter_map(|e| match e.command {
                Command::Sample { sound, .. } | Command::Auto { sound, .. } => Some(sound),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        let bank = if most <= in_second {
            file - 1
        } else {
            file - 2
        };
        song.set_sound_files(Style::Single, difficulty, bank, u16::MAX as u32);
    }
}

/// The chart of `difficulty` in a chart file, as `Ply` events in milliseconds.
/// Buttons 0–8 are P1 lanes 0–8.
/// `bgm`: whether the disc has BGM streams; without them (10) the BGM start
/// plays the key sound its value names.
pub(crate) fn chart(file: &[u8], difficulty: Difficulty, bgm: bool) -> Option<Ply> {
    let list = list_of(difficulty);
    let start = u32_at(file, list * 4)? as usize;
    let end = match list {
        7 => file.len(),
        _ => (u32_at(file, (list + 1) * 4)? as usize).min(file.len()),
    };
    let mut events: Vec<Event> = file
        .get(start..end)?
        .chunks_exact(8)
        .filter_map(|e| {
            let time = u32_at(e, 0)?;
            let (button, value) = (e[4], u16::from_le_bytes([e[6], e[7]]));
            let command = match e[5] {
                1 => Command::Note {
                    side: Side::P1,
                    lane: button,
                    length: value,
                },
                2 => Command::Sample {
                    side: Side::P1,
                    lane: button,
                    sound: value,
                },
                3 if bgm => Command::Auto {
                    slot: 8,
                    sound: BGM_SOUND,
                },
                3 if value == 0 => return None,
                3 => Command::Auto {
                    slot: 8,
                    sound: value,
                },
                4 => Command::Bpm { bpm: value as f64 },
                5 => Command::TimeSignature {
                    numerator: (value >> 8) as u8,
                    denominator: value & 0xff,
                },
                6 => Command::End,
                // The button is the pan, as on IIDX discs.
                7 => Command::Auto {
                    slot: button,
                    sound: value,
                },
                10 => Command::Measure,
                _ => return None,
            };
            Some(Event { time, command })
        })
        .collect();
    events.sort_by_key(|e| e.time);
    Some(Ply {
        tick_ms: 1.0,
        events,
    })
}

/// JIS X 0208 row 1 (symbols), as Shift-JIS lays it out.
const ROW1: &str = "　、。，．・：；？！゛゜´｀¨＾￣＿ヽヾゝゞ〃仝々〆〇ー―‐／＼～∥｜…‥‘’“”（）〔〕［］｛｝〈〉《》「」『』【】＋－±×÷＝≠＜＞≦≧∞∴♂♀°′″℃￥＄￠￡％＃＆＊＠§☆★○●◎◇";

/// Shift-JIS text as far as genre names need it: ASCII, symbols, full-width
/// letters and digits, hiragana and katakana. Anything else becomes `?`.
pub fn shift_jis(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < 0x80 {
            out.push(b as char);
            i += 1;
            continue;
        }
        let Some(&second) = bytes.get(i + 1) else {
            break;
        };
        i += 2;
        let (row, col) = match (b, second) {
            (0x81..=0x9f, 0x40..=0xfc) | (0xe0..=0xef, 0x40..=0xfc) => {
                let pair = if b < 0xa0 { b - 0x81 } else { b - 0xc1 } as u32;
                if second >= 0x9f {
                    (pair * 2 + 2, second as u32 - 0x9e)
                } else {
                    (
                        pair * 2 + 1,
                        second as u32 - 0x40 + 1 - (second > 0x7f) as u32,
                    )
                }
            }
            _ => (0, 0),
        };
        let c = match (row, col) {
            (1, 1..=94) => ROW1.chars().nth(col as usize - 1),
            (3, 16..=25) => char::from_u32(0xff10 + col - 16),
            (3, 33..=58) => char::from_u32(0xff21 + col - 33),
            (3, 65..=90) => char::from_u32(0xff41 + col - 65),
            (4, 1..=83) => char::from_u32(0x3041 + col - 1),
            (5, 1..=86) => char::from_u32(0x30a1 + col - 1),
            _ => None,
        };
        out.push(c.unwrap_or('?'));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpack_reads_every_code() {
        let input = [
            0x81, b'a', b'b', // two literals
            0x03, 0xfe, // copy 2 from 2 back: "ab"
            0xa0, 7, // a zero, then 7
            0xc1, b'x', // "xxx"
            0xe1, // two zeros
            0xff, 0x00, // 32 zeros
            0x7f, 0xff, // end
        ];
        let mut want = b"abab".to_vec();
        want.extend([0, 7, b'x', b'x', b'x', 0, 0]);
        want.extend([0; 32]);
        assert_eq!(unpack(&input, 100).unwrap(), want);
        assert_eq!(unpack(&input, 3).unwrap(), b"aba");
        assert_eq!(unpack(&[0x81, b'a'], 2), None);
        assert_eq!(unpack(&[], 0).unwrap(), b"");
    }

    #[test]
    fn word_mask_is_mean_brightness_cut_to_its_ink() {
        // Columns 0 and 3 are blank: the mask keeps columns 1 and 2.
        let dark = [0, 0, 0, 255];
        let white = [255, 255, 255, 255];
        let picture = Rgba {
            width: 4,
            height: 2,
            pixels: vec![dark, white, white, dark, dark, dark, dark, dark],
        };
        let mask = Mask::from_rgba(&picture);
        assert_eq!((mask.width, mask.height), (2, 2));
        assert_eq!(mask.coverage, vec![255, 255, 0, 0]);
    }

    #[test]
    fn name_refs_read_the_title_and_artist_words() {
        // Two records at the table's start on a made-up layout, the rest of
        // the executable zero.
        let mut exe = vec![0u8; 0x200];
        let record = |exe: &mut Vec<u8>, at: usize, title: u16, artist: u16| {
            exe[at + TITLE_AT..at + TITLE_AT + 2].copy_from_slice(&title.to_le_bytes());
            exe[at + ARTIST_AT..at + ARTIST_AT + 2].copy_from_slice(&artist.to_le_bytes());
        };
        record(&mut exe, 0x40, 50, 55);
        record(&mut exe, 0x40 + RECORD_12.size, 51, 56);
        let layout = Layout {
            songs: (0x40, 2),
            names: Some(NameFiles {
                title: 68,
                artist: 71,
            }),
            ..POPN14
        };
        assert_eq!(read_name_refs(&exe, &layout), vec![(50, 55), (51, 56)]);
        let none = Layout {
            names: None,
            ..layout
        };
        assert!(read_name_refs(&exe, &none).is_empty());
    }

    #[test]
    fn genre_names_decode() {
        // ハイパージャパネスク, Ｊ－ロック０, ひ
        let bytes = [
            0x83, 0x6e, 0x83, 0x43, 0x83, 0x70, 0x81, 0x5b, 0x83, 0x57, 0x83, 0x83, 0x83, 0x70,
            0x83, 0x6c, 0x83, 0x58, 0x83, 0x4e,
        ];
        assert_eq!(shift_jis(&bytes), "ハイパージャパネスク");
        let bytes = [
            0x82, 0x69, 0x81, 0x7c, 0x83, 0x8d, 0x83, 0x62, 0x83, 0x4e, 0x82, 0x4f,
        ];
        assert_eq!(shift_jis(&bytes), "Ｊ－ロック０");
        assert_eq!(shift_jis(&[0x82, 0xd0, b'A']), "ひA");
        assert_eq!(shift_jis(&[0x8e, 0x71]), "?");
    }

    fn event(time: u32, button: u8, command: u8, value: u16) -> [u8; 8] {
        let mut e = [0; 8];
        e[..4].copy_from_slice(&time.to_le_bytes());
        e[4] = button;
        e[5] = command;
        e[6..].copy_from_slice(&value.to_le_bytes());
        e
    }

    #[test]
    fn a_chart_list_reads_as_ply_events() {
        let lists = [
            vec![
                event(0, 15, 4, 140),
                event(0, 15, 5, 0x0708),
                event(0, 15, 10, 0),
                event(418, 15, 3, 10),
                event(1500, 3, 2, 4),
                event(1714, 3, 1, 0),
                event(1714, 8, 7, 1),
                event(1714, 15, 9, 1),
            ],
            vec![event(0, 15, 6, 0)],
        ];
        let mut file = Vec::new();
        let mut at = 32u32;
        for k in 0..8 {
            file.extend(at.to_le_bytes());
            if k < lists.len() {
                at += lists[k].len() as u32 * 8;
            }
        }
        for list in &lists {
            for e in list {
                file.extend(e);
            }
        }
        let ply = chart(&file, Difficulty::Normal, true).unwrap();
        let commands: Vec<(u32, Command)> =
            ply.events.iter().map(|e| (e.time, e.command)).collect();
        assert_eq!(
            commands,
            [
                (0, Command::Bpm { bpm: 140.0 }),
                (
                    0,
                    Command::TimeSignature {
                        numerator: 7,
                        denominator: 8
                    }
                ),
                (0, Command::Measure),
                (
                    418,
                    Command::Auto {
                        slot: 8,
                        sound: BGM_SOUND
                    }
                ),
                (
                    1500,
                    Command::Sample {
                        side: Side::P1,
                        lane: 3,
                        sound: 4
                    }
                ),
                (
                    1714,
                    Command::Note {
                        side: Side::P1,
                        lane: 3,
                        length: 0
                    }
                ),
                (1714, Command::Auto { slot: 8, sound: 1 }),
            ]
        );
        assert_eq!(ply.millis(1714), 1714.0);
        let hyper = chart(&file, Difficulty::Hyper, true).unwrap();
        assert_eq!(hyper.events[0].command, Command::End);
    }
}

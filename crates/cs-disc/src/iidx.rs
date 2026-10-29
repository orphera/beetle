//! The PS2 beatmania IIDX discs this crate reads (10th style, 11 IIDX RED,
//! 12 HAPPY SKY, 13 DistorteD, 14 GOLD, 15 DJ TROOPERS, 16 EMPRESS and its
//! PREMIUM BEST disc): the song list and where each song's
//! files are.
//!
//! The boot executable holds both. Its song table has one record per song slot
//! (title, levels, file numbers of the charts and sounds); its file tables give
//! each file number a span inside the `BM2DX<nn>A/B/C.DAT` files (`DATA2.DAT`
//! on 10 and 11). The
//! editions differ only in where those tables are, the record layout, the chart
//! key and where a song's stage art is (`Layout`). Offsets come from
//! iidx-ps2tools (`plugins/iidx10thcs.py` to `iidx13thcs.py`, `iidx14thcs.py`, `iidx15thcs.py`,
//! `iidx16thempresscs.py`, `iidx16thpremiumbestcs.py`).
//!
//! The files are returned as stored: charts are still encrypted (from 14 on)
//! and compressed, sounds still ADPCM.

use std::io::{self, Read, Seek};

use crate::iso9660::{FileExtent, Volume};
use crate::ply::Ply;
use crate::ply_bms::{to_bms, Header};
use crate::ps2tex::{decode_lzss, decode_psmt8, texture_size, Rgba};
use crate::sector::SectorReader;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edition {
    /// 3rd style, `SLPM_650.06`.
    ThirdStyle,
    /// 4th style, `SLPM_650.26`.
    FourthStyle,
    /// 5th style, `SLPM_650.49`.
    FifthStyle,
    /// 6th style, `SLPM_651.56`.
    SixthStyle,
    /// 7th style, `SLPM_655.93`.
    SeventhStyle,
    /// 8th style, `SLPM_657.68`.
    EighthStyle,
    /// 9th style, `SLPM_659.46`.
    NinthStyle,
    /// 10th style, `SLPM_661.80`.
    TenthStyle,
    /// 11 IIDX RED, `SLPM_664.26`.
    Red,
    /// 12 HAPPY SKY, `SLPM_666.21`.
    HappySky,
    /// 13 DistorteD, `SLPM_668.28`.
    Distorted,
    /// 14 GOLD, `SLPM_669.95`.
    Gold,
    /// 15 DJ TROOPERS, `SLPM_551.17`.
    DjTroopers,
    /// 16 EMPRESS, disc 1, `SLPM_552.21`.
    Empress,
    /// 16 EMPRESS, disc 2, `SLPM_552.22`.
    PremiumBest,
    /// pop'n music 8, `SLPM_653.16`.
    Popn8,
    /// pop'n music 9, `SLPM_655.08`.
    Popn9,
    /// pop'n music 10, `SLPM_657.70`.
    Popn10,
    /// pop'n music 12 いろは, `SLPM_663.14`.
    Popn12,
    /// pop'n music 13 カーニバル, `SLPM_665.33`.
    Popn13,
    /// pop'n music 14 FEVER!, `SLPM_667.42`.
    Popn14,
}

const EDITIONS: [Edition; 21] = [
    Edition::ThirdStyle,
    Edition::FourthStyle,
    Edition::FifthStyle,
    Edition::SixthStyle,
    Edition::SeventhStyle,
    Edition::EighthStyle,
    Edition::NinthStyle,
    Edition::TenthStyle,
    Edition::Red,
    Edition::HappySky,
    Edition::Distorted,
    Edition::Gold,
    Edition::DjTroopers,
    Edition::Empress,
    Edition::PremiumBest,
    Edition::Popn8,
    Edition::Popn9,
    Edition::Popn10,
    Edition::Popn12,
    Edition::Popn13,
    Edition::Popn14,
];

/// Where one edition keeps its tables inside the boot executable (file offsets).
struct Layout {
    executable: &'static str,
    slug: &'static str,
    /// The file tables of the DAT files, in order: (offset, bytes).
    file_tables: &'static [(usize, usize)],
    file_records: FileRecords,
    /// `DATA2.DAT` (10, 11) or the three `BM2DX<nn>A/B/C.DAT` files.
    dat_files: &'static [&'static str],
    /// The song table: (offset, bytes).
    songs: (usize, usize),
    /// The `DATA1.DAT` file table: (offset, bytes), `(0, 0)` without one.
    data1: (usize, usize),
    /// The Blowfish key of the chart files. iidx-ps2tools builds it from
    /// letters of song names in the game (`generate_encryption_key`); this is
    /// its result.
    /// `None` where charts are not encrypted (10 to 13).
    chart_key: Option<&'static [u8]>,
    /// A sound number charts start with the BGM and no bank holds (8th: 2,
    /// beside the BGM's 1; the stream is already stereo).
    bgm_pair: Option<u16>,
    /// Titles of records whose title is blank (`Song::id`, title): 8th's
    /// unlockable songs, named only by pictures (RemyWiki, told apart by levels).
    titles: &'static [(u32, &'static str)],
    record: Record,
    /// The dan course table, where the edition has one
    /// (9th on; docs/specs/iidx_ps2_disc.md, 7).
    dan: Option<DanTable>,
    /// The EXPERT course table, where the edition has one
    /// (9th on; docs/specs/iidx_ps2_disc.md, 8).
    expert: Option<ExpertTable>,
    /// The song select's word pictures on 8th, which keeps them in the
    /// executable (`OldWords`).
    old_words: Option<OldWords>,
}

/// 8th's word pictures: LZ-packed pictures in the executable
/// (`ps2tex::decode_old_picture`) cut by rectangle tables beside them
/// (`u16 x, y, w, h`). The record numbers each word over its kind's sheets
/// in order: the artist at `artist_at`, the genre at `genre_at`, the title
/// at `title_at` (`u16` each). Found by the sheets' rectangles running out
/// exactly at the records' largest numbers (62, 57, 170) and checked by eye.
#[derive(Clone, Copy)]
struct OldWords {
    artist_at: usize,
    genre_at: usize,
    title_at: usize,
    artist: &'static [Sheet],
    genre: &'static [Sheet],
    title: &'static [Sheet],
}

/// One word picture of `OldWords`: (picture's file offset in the
/// executable, its rectangle table's, its rectangles).
type Sheet = (usize, usize, usize);

/// How an edition stores its dan courses.
#[derive(Clone, Copy)]
enum DanTable {
    /// 14 on: `DAN_PAIRS` (song, chart) word pairs at this offset.
    Pairs(usize),
    /// 7th to 13: per course, one entry per song ending in an end mark,
    /// in runs of consecutive ranks (`DanRun`). `charts`: the difficulty of
    /// each chart value.
    Listed {
        form: ListForm,
        runs: &'static [DanRun],
        charts: &'static [Option<Difficulty>],
    },
}

/// How a `DanTable::Listed` table stores a course.
#[derive(Clone, Copy)]
enum ListForm {
    /// 9th to 13: (song, chart, slot bit) word triples ending in -1, each
    /// course starting on a 16-byte boundary.
    Triples,
    /// 7th: 68-byte records of five (`u16` song, `u16` chart, `u16`)
    /// entries, the first four (SP) or three (DP) used; song 0 is a song.
    /// Ranks run down from `DanRun::first`.
    Fixed,
    /// 8th: 16-byte entries of `u16 song, u16 chart`, eight unknown
    /// bytes and four slot flags, ending in an entry of song 0xffff.
    Records,
}

/// Dan chart values (`DanTable::Listed::charts`).
const DAN_CHARTS: &[Option<Difficulty>] = &[
    Some(Difficulty::Normal),
    Some(Difficulty::Hyper),
    Some(Difficulty::Another),
    Some(Difficulty::Beginner),
    Some(Difficulty::Black),
];
/// 9th to 11: HYPER 0, NORMAL 1.
const DAN_CHARTS_HYPER_FIRST: &[Option<Difficulty>] = &[
    Some(Difficulty::Hyper),
    Some(Difficulty::Normal),
    Some(Difficulty::Another),
    Some(Difficulty::Beginner),
];
/// 8th: HYPER 0, NORMAL 1, ANOTHER 3, BEGINNER 4.
const DAN_CHARTS_EIGHTH: &[Option<Difficulty>] = &[
    Some(Difficulty::Hyper),
    Some(Difficulty::Normal),
    None,
    Some(Difficulty::Another),
    Some(Difficulty::Beginner),
];

/// Consecutive courses of one style in a `DanTable::Listed` table.
#[derive(Clone, Copy)]
struct DanRun {
    style: Style,
    at: usize,
    /// The first course's rank (`DAN_RANKS` index).
    first: usize,
    count: usize,
}

const fn run(style: Style, at: usize, first: usize, count: usize) -> DanRun {
    DanRun {
        style,
        at,
        first,
        count,
    }
}

/// How an edition stores its EXPERT courses.
#[derive(Clone, Copy)]
enum ExpertTable {
    /// 14 on: 64-byte records of a pointer to the name and five
    /// (song, 1, 1) word triples.
    Pointers { at: usize, count: usize },
    /// 9th to 13: 124-byte records of the name (ASCII, inline) and, from 0x24,
    /// five (song, 1, 1, 1) word quadruples.
    Inline { at: usize, count: usize },
    /// 6th to 8th: records (96, 68 and 72 bytes) of five (`u16` song, two
    /// `u16` fixing the chart) entries; the names are pictures, so they come
    /// from RemyWiki (`CS_6th_style` …), one per record, matched by songs.
    /// An empty name (a course the wiki leaves out) becomes `#<number>`.
    Unnamed {
        at: usize,
        size: usize,
        /// Bytes from one song to the next: 6, or 4 in 5th (`u16` song,
        /// `u16` chart).
        step: usize,
        names: &'static [&'static str],
    },
}

/// How a DAT file table entry gives a file's span.
#[derive(Clone, Copy)]
enum FileRecords {
    /// 8 bytes: sector, sectors (10 to 13).
    Sectors,
    /// 12 bytes: sector, an unused word, bytes (14 on).
    Bytes,
    /// 16 bytes: sector, an unused word, bytes, a hash (5th to 8th).
    Hashed,
    /// 12 bytes: sector, bytes, a hash (3rd, 4th).
    ShortHashed,
}

/// Where a song record keeps its fields.
struct Record {
    size: usize,
    title: TitleField,
    numbers: FileNumbers,
    /// The level bytes and which of them each chart slot (`CHART_SLOTS` order) shows.
    levels_at: usize,
    level_of: [usize; 10],
    video_at: usize,
    charts_at: usize,
    /// How many chart file numbers the record has (`CHART_SLOTS` prefix):
    /// 10 to 13 have no BLACK slots.
    chart_count: usize,
    sounds_at: usize,
    /// How many sound file numbers it has (16; 4th 12, 3rd 4: SP and DP
    /// HYPER).
    sound_count: usize,
    stage: StageArt,
}

/// Where a record keeps the song title.
#[derive(Clone, Copy)]
enum TitleField {
    /// ASCII, `TITLE_LEN` bytes from 0x00 (9th on).
    Inline,
    /// The executable address of the ASCII title at this field: the file
    /// offset is the address less `base` (8th and older).
    Pointer { at: usize, base: u32 },
}

/// How a record's file numbers count across the DAT files.
#[derive(Clone, Copy)]
enum FileNumbers {
    /// One number from the first file of the first DAT on (9th on).
    Running,
    /// 8th: the first DAT has `first` files. A chart number is
    /// `0xf0000000 | n` for file `n` of the second DAT; a sound or video
    /// number below `first` is a file of the first DAT, and `n` from `first`
    /// on file `n` of the second.
    Split { first: u32 },
    /// 6th: a chart number is the executable address of the LZ-packed
    /// chart (`Disc::open` gives it a file of the executable); a sound or
    /// video number counts in the DAT the record's `u16` at `at` names (1 or
    /// 0 the first, 2 the second, which follows the first's `first` files).
    Archives { at: usize, first: u32 },
    /// 5th: charts as 6th's, sounds and videos `u16` numbers of the one DAT.
    Executable,
}

impl FileNumbers {
    fn chart(self, raw: u32) -> u32 {
        match self {
            FileNumbers::Running => raw,
            FileNumbers::Split { first } => (raw & 0x0fff_ffff) + first,
            FileNumbers::Archives { .. } | FileNumbers::Executable => raw,
        }
    }

    /// A sound or video number of `record`; 6th to 8th's are `u16`, 8th's
    /// video one of a pair.
    fn other(self, raw: u32, record: &[u8]) -> u32 {
        match self {
            FileNumbers::Running => raw,
            FileNumbers::Split { first } => {
                let raw = raw & 0xffff;
                if raw >= first {
                    raw + first
                } else {
                    raw
                }
            }
            FileNumbers::Executable => raw & 0xffff,
            FileNumbers::Archives { at, first } => {
                let raw = raw & 0xffff;
                if u16_le(record, at) == Some(2) {
                    raw + first
                } else {
                    raw
                }
            }
        }
    }
}

/// Where a song's stage art (the picture shown while it loads) is.
#[derive(Clone, Copy)]
enum StageArt {
    /// The DATA1 file a record field names: an animation archive whose first
    /// texture is the stage image (10 to 15).
    Overlay {
        at: usize,
        /// Which picture of a wide texture each use shows: where the
        /// record's `(picture, file)` word pairs start (the first is the usual
        /// card) and which pair is ANOTHER's. Picture 0 is the left one.
        /// `None` where records name no picture (10 to 12): a wide texture
        /// there holds the usual card and ANOTHER's.
        picks: Option<Picks>,
    },
    /// The DATA1 file a record field names, holding the loading card (16).
    Card { at: usize },
    /// None read (8th and older: no DATA1).
    None,
}

/// Where a record's stage picture choices are (`StageArt::Overlay`).
#[derive(Clone, Copy)]
struct Picks {
    at: usize,
    count: usize,
    another: usize,
}

/// 15 and 16 records. Levels: SP N/H/A/B/Black, then DP N/H/A/B/Black
/// (DJ TROOPERS: DAZZLIN' DARLIN' DP BEGINNER 1, RISTACCIA DP BLACK 11).
const RECORD_15_16: Record = Record {
    size: 0x134,
    title: TitleField::Inline,
    numbers: FileNumbers::Running,
    levels_at: 0x4a,
    level_of: [1, 6, 2, 7, 0, 5, 3, 8, 4, 9],
    video_at: 0x58,
    charts_at: 0xc0,
    chart_count: 10,
    sounds_at: 0xe8,
    sound_count: 16,
    stage: StageArt::Overlay {
        at: 0x94,
        picks: Some(Picks {
            at: 0x98,
            count: 4,
            another: 2,
        }),
    },
};

/// 3rd style: the 4th style form, 0x7c-byte records with the HYPER key
/// sounds and BGMs alone.
const THIRD_STYLE: Layout = Layout {
    executable: "SLPM_650.06",
    slug: "iidx3",
    file_tables: &[(0x145cd0, 0x1050)],
    dat_files: &["DX2_3/BM2DX3.BIN"],
    // 81 songs; then the ending roll.
    songs: (0x77fc8, 81 * 0x7c),
    record: Record {
        size: 0x7c,
        levels_at: 0x0c,
        video_at: 0x0a,
        charts_at: 0x54,
        sounds_at: 0x6c,
        sound_count: 4,
        ..FOURTH_STYLE.record
    },
    // The wiki gives the second course no name.
    expert: Some(ExpertTable::Unnamed {
        at: 0x7a780,
        size: 20,
        step: 4,
        names: &[
            "SOUL",
            "",
            "DANCE",
            "TECHNO",
            "CRY-MAX",
            "PIANO",
            "NAOKI",
            "SPECIAL",
            "U",
            "CLASSIC",
            "TAKA",
            "UPPER",
            "TaQ",
            "2nd STYLE",
            "GOOD-COOL",
            "HELL",
        ],
    }),
    ..FOURTH_STYLE
};

/// 4th style: the 5th style form, 0x90-byte records of six charts (no
/// BEGINNER) and 12-byte file records.
const FOURTH_STYLE: Layout = Layout {
    executable: "SLPM_650.26",
    slug: "iidx4",
    file_tables: &[(0x137450, 0x9d8)],
    file_records: FileRecords::ShortHashed,
    dat_files: &["DX2_4/BM2DX4.BIN"],
    // 53 songs; then the ending roll and three survival records.
    songs: (0x8bc98, 53 * 0x90),
    // Levels: HYPER and ANOTHER (both styles), then NORMAL.
    record: Record {
        size: 0x90,
        levels_at: 0x09,
        level_of: [0, 0, 0, 0, 1, 1, 1, 1, 0, 0],
        video_at: 0x0e,
        charts_at: 0x58,
        chart_count: 6,
        sounds_at: 0x70,
        sound_count: 12,
        ..FIFTH_STYLE.record
    },
    expert: Some(ExpertTable::Unnamed {
        at: 0x13f5c8,
        size: 20,
        step: 4,
        names: &[
            "VOCAL",
            "HAPPY",
            "TRANCE",
            "TRIP-TRAP",
            "CLUB",
            "TECHNO",
            "7STARS",
            "EURO",
            "FLOWER",
            "TURBO",
            "HYPER",
            "dj TAKA",
            "MILLENNIUM",
            "BONUS",
            "L.E.D.",
        ],
    }),
    ..FIFTH_STYLE
};

/// 5th style: one `DX2_5` BIN, charts inside the executable.
const FIFTH_STYLE: Layout = Layout {
    executable: "SLPM_650.49",
    slug: "iidx5",
    file_tables: &[(0x1837d8, 0x1230)],
    dat_files: &["DX2_5/BM2DX5.BIN"],
    // 64 songs; then the ending roll, three survival records and d01 to
    // d73, which are not songs.
    songs: (0xae520, 64 * 0xa4),
    record: Record {
        size: 0xa4,
        numbers: FileNumbers::Executable,
        levels_at: 0x08,
        video_at: 0x12,
        charts_at: 0x5c,
        sounds_at: 0x7c,
        sound_count: 16,
        ..SIXTH_STYLE.record
    },
    expert: Some(ExpertTable::Unnamed {
        at: 0x18ebb8,
        size: 20,
        step: 4,
        names: &[
            "HEAVEN",
            "FLY HIGH",
            "REGULAR",
            "NEW-WAVE",
            "UG",
            "NAOKI",
            "TRANCE",
            "OutPhase",
            "Osamu Kubota",
            "good-cool",
            "dj TAKA",
            "TaQ",
            "RAPID",
            "HIDDEN",
            "AKIRA YAMAOKA",
            "HELL2",
            "BONUS",
            "L.E.D.",
        ],
    }),
    ..SIXTH_STYLE
};

/// 6th style: two `DX2_6` BINs, charts inside the executable.
const SIXTH_STYLE: Layout = Layout {
    executable: "SLPM_651.56",
    slug: "iidx6",
    file_tables: &[(0x180058, 0x1590), (0x1815e8, 0x5b0)],
    dat_files: &["DX2_6/BM2DX6A.BIN", "DX2_6/BM2DX6B.BIN"],
    // 69 songs; then 15 records named D, one END and d001 to d130, all
    // level 1 placeholders.
    songs: (0x1885b8, 69 * 0xa0),
    titles: &[],
    record: Record {
        size: 0xa0,
        title: TitleField::Pointer {
            at: 0,
            base: 0xff000,
        },
        numbers: FileNumbers::Archives {
            at: 0x16,
            first: 0x1590 / 16,
        },
        levels_at: 0x10,
        video_at: 0x28,
        charts_at: 0x58,
        sounds_at: 0x78,
        sound_count: 16,
        ..EIGHTH_STYLE.record
    },
    dan: None,
    expert: Some(ExpertTable::Unnamed {
        at: 0x191da4,
        size: 96,
        step: 6,
        names: &[
            "LOUNGE",
            "VIVID",
            "RAVE",
            "TRANCE",
            "HEAVY",
            "ALL-MIX",
            "HOUSE",
            "RESIDENT",
            "SPECIAL",
            "",
            "",
            "",
            "NAOKI",
            "EURO",
            "VARIOUS",
            "sampling masters",
            "TAKA",
            "HELL3",
            "BONUS",
            "L.E.D.-G",
            "SPEEDY",
            "FAREWELL",
            "PARTY",
            "HISTORY",
            "月 VJ SHIRO",
            "火 3rd style",
            "水 VJ YUZ",
            "木 4th style",
            "金 HELL",
            "土 VJ GYO",
            "日 5th style",
        ],
    }),
    old_words: None,
    ..EIGHTH_STYLE
};

/// 7th style: the 8th style form, 0xa0-byte records.
const SEVENTH_STYLE: Layout = Layout {
    executable: "SLPM_655.93",
    slug: "iidx7",
    file_tables: &[(0x1b7460, 0x8a0), (0x1b9a30, 0x2bc0), (0x1b6a50, 0xa10)],
    dat_files: &[
        "DX2_7/BM2DX7B.BIN",
        "DX2_7/BM2DX7C.BIN",
        "DX2_7/BM2DX7A.BIN",
    ],
    songs: (0x1c1af0, 0x3840),
    titles: &[],
    // SP 十段 down to 7級, DP 十段 down to 5級, then SP 裏八段 and 裏九段.
    dan: Some(DanTable::Listed {
        form: ListForm::Fixed,
        runs: &[
            run(Style::Single, 0x1c5ee8, 16, 17),
            run(Style::Double, 0x1c636c, 16, 15),
            run(Style::Single, 0x1c6768, 18, 1),
            run(Style::Single, 0x1c67ac, 19, 1),
        ],
        charts: DAN_CHARTS_EIGHTH,
    }),
    record: Record {
        size: 0xa0,
        numbers: FileNumbers::Split { first: 138 },
        levels_at: 0x10,
        video_at: 0x1a,
        charts_at: 0x58,
        sounds_at: 0x78,
        sound_count: 16,
        ..EIGHTH_STYLE.record
    },
    expert: Some(ExpertTable::Unnamed {
        at: 0x1c59dc,
        size: 68,
        step: 6,
        names: &[
            "MORNING",
            "EVENING",
            "EURO",
            "TRANCE",
            "JEWEL",
            "STOIC",
            "HOUSE",
            "APPEARANCE",
            "SELECTION",
        ],
    }),
    old_words: None,
    ..EIGHTH_STYLE
};

/// 8th style: files in `DX2_8`, titles by pointer, no DATA1.
const EIGHTH_STYLE: Layout = Layout {
    executable: "SLPM_657.68",
    slug: "iidx8",
    file_tables: &[(0x19a940, 0x740), (0x19b080, 0x2db0), (0x19a180, 0x790)],
    file_records: FileRecords::Hashed,
    dat_files: &[
        "DX2_8/BM2DX8B.BIN",
        "DX2_8/BM2DX8C.BIN",
        "DX2_8/BM2DX8A.BIN",
    ],
    songs: (0x1a4060, 0x36e0),
    data1: (0, 0),
    chart_key: None,
    bgm_pair: Some(2),
    titles: &[
        (87, "TAIYO"),
        (88, "DON'T BE AFRAID MYSELF"),
        (89, "SPIRAL GALAXY"),
    ],
    // Levels: SP H/A, DP H/A, SP N, DP N, SP B, DP B.
    record: Record {
        size: 0x9c,
        title: TitleField::Pointer {
            at: 0,
            base: 0xfff80,
        },
        numbers: FileNumbers::Split { first: 116 },
        levels_at: 0x08,
        level_of: [0, 1, 0, 1, 2, 3, 4, 5, 0, 1],
        // Two `u16` MPEG-2 files, much alike; 0x24 names one on 10 songs.
        video_at: 0x14,
        charts_at: 0x54,
        chart_count: 8,
        sounds_at: 0x74,
        sound_count: 16,
        stage: StageArt::None,
    },
    // SP 九段 draws four of twelve (0x199c00), so SP runs 7級 to 八段 and
    // 十段 apart. A course after DP 十段 (0x19a0e0) is not in the game.
    dan: Some(DanTable::Listed {
        form: ListForm::Records,
        runs: &[
            run(Style::Single, 0x199750, 0, 15),
            run(Style::Single, 0x199cd0, 16, 1),
            run(Style::Double, 0x199d20, 2, 15),
        ],
        charts: DAN_CHARTS_EIGHTH,
    }),
    expert: Some(ExpertTable::Unnamed {
        at: 0x1a7e18,
        size: 72,
        step: 6,
        names: &[
            "FEEL",
            "EUROBEAT",
            "VOCAL",
            "TRANCE",
            "NO.5",
            "UG2",
            "NIGHT",
            "PARTY",
            "V.A.",
            "NEW COMER",
            "RAPID2",
            "TaQ",
            "HIRATA",
            "L.E.D.",
            "HELL",
            "ASSAULT",
            "7-SP10D",
        ],
    }),
    old_words: Some(OldWords {
        artist_at: 0x20,
        genre_at: 0x18,
        title_at: 0x1c,
        artist: &[(0x6e5720, 0x1969b0, 63)],
        genre: &[(0x6ed7a0, 0x196bb0, 56), (0x6f3141, 0x196bb0 + 56 * 8, 2)],
        title: &[
            (0x6f36c0, 0x196bb0 + 58 * 8, 23),
            (0x6fa9b0, 0x196bb0 + 81 * 8, 21),
            (0x7016f5, 0x196bb0 + 102 * 8, 16),
            (0x7077de, 0x196bb0 + 118 * 8, 18),
            (0x70f6cb, 0x196bb0 + 136 * 8, 18),
            (0x716c38, 0x196bb0 + 154 * 8, 22),
            (0x71d74d, 0x196bb0 + 176 * 8, 24),
            (0x723fad, 0x196bb0 + 200 * 8, 24),
            (0x72ae8d, 0x196bb0 + 224 * 8, 6),
        ],
    }),
};

/// 9th style: the 10th style record, its tables elsewhere.
const NINTH_STYLE: Layout = Layout {
    executable: "SLPM_659.46",
    slug: "iidx9",
    file_tables: &[(0xbd230, 0x1928)],
    data1: (0xb7c28, 0x1180),
    songs: (0xc1500, 0x7bb4),
    // SP 九段 draws four of a pool (0xb58f0, slot bits not 1/2/4/8), so
    // SP runs 7級 to 八段 and 十段 apart.
    dan: Some(DanTable::Listed {
        runs: &[
            run(Style::Single, 0xb5540, 0, 15),
            run(Style::Single, 0xb59b0, 16, 1),
            run(Style::Double, 0xb59f0, 2, 15),
        ],
        form: ListForm::Triples,
        charts: DAN_CHARTS_HYPER_FIRST,
    }),
    // STYLISH … ASSAULT; the three words after each song are 0 or 1.
    expert: Some(ExpertTable::Inline {
        at: 0xb6e08,
        count: 17,
    }),
    ..TENTH_STYLE
};

const TENTH_STYLE: Layout = Layout {
    executable: "SLPM_661.80",
    slug: "iidx10",
    file_tables: &[(0xcdc90, 0x1a38)],
    file_records: FileRecords::Sectors,
    dat_files: &["DATA2.DAT"],
    songs: (0x10bae0, 0x7d20),
    data1: (0xc8e08, 0x11b0),
    chart_key: None,
    bgm_pair: None,
    titles: &[],
    // Levels: beginner (both styles), eight unused bytes, then SP H/A, SP N,
    // DP H/A, DP N: HYPER and ANOTHER share one, as RemyWiki lists the CS.
    record: Record {
        size: 0x16c,
        title: TitleField::Inline,
        numbers: FileNumbers::Running,
        levels_at: 0x41,
        level_of: [9, 11, 9, 11, 10, 12, 0, 0, 9, 11],
        video_at: 0x58,
        charts_at: 0x12c,
        chart_count: 8,
        sounds_at: 0x14c,
        sound_count: 16,
        stage: StageArt::Overlay {
            at: 0xd4,
            // One 384x384 card; the words after it are not picture picks.
            picks: None,
        },
    },
    dan: Some(DanTable::Listed {
        runs: &[
            run(Style::Single, 0xc6790, 0, 17),
            run(Style::Double, 0xc6bd0, 2, 15),
        ],
        form: ListForm::Triples,
        charts: DAN_CHARTS_HYPER_FIRST,
    }),
    expert: Some(ExpertTable::Inline {
        at: 0xc7fb8,
        count: 17,
    }),
    old_words: None,
};

const RED: Layout = Layout {
    executable: "SLPM_664.26",
    slug: "iidx11-red",
    file_tables: &[(0xee440, 0x1b40)],
    file_records: FileRecords::Sectors,
    dat_files: &["DATA2.DAT"],
    songs: (0x1c21f0, 0x6f40),
    data1: (0xe83e8, 0x19c0),
    chart_key: None,
    bgm_pair: None,
    titles: &[],
    // Levels: beginner (both styles), ten unused bytes, SP H/N/A, DP H/N/A.
    record: Record {
        size: 0x140,
        title: TitleField::Inline,
        numbers: FileNumbers::Running,
        levels_at: 0x41,
        level_of: [11, 14, 13, 16, 12, 15, 0, 0, 13, 16],
        video_at: 0x5c,
        charts_at: 0x100,
        chart_count: 8,
        sounds_at: 0x120,
        sound_count: 16,
        stage: StageArt::Overlay {
            at: 0xd8,
            // One 384x384 card; the words after it are not picture picks.
            picks: None,
        },
    },
    dan: Some(DanTable::Listed {
        runs: &[
            run(Style::Single, 0xe5dc0, 0, 17),
            run(Style::Double, 0xe6200, 2, 15),
        ],
        form: ListForm::Triples,
        charts: DAN_CHARTS_HYPER_FIRST,
    }),
    expert: Some(ExpertTable::Inline {
        at: 0xe7328,
        count: 17,
    }),
    old_words: None,
};

const HAPPY_SKY: Layout = Layout {
    executable: "SLPM_666.21",
    slug: "iidx12-happy-sky",
    file_tables: &[(0x105260, 0xe0), (0x105340, 0x1700), (0x106a40, 0x458)],
    file_records: FileRecords::Sectors,
    dat_files: &["BM2DX12A.DAT", "BM2DX12B.DAT", "BM2DX12C.DAT"],
    songs: (0x115f10, 0x7470),
    data1: (0x100108, 0x1be0),
    chart_key: None,
    bgm_pair: None,
    titles: &[],
    // Levels: beginner (both styles), eight unused bytes, SP N/H/A, DP N/H/A.
    record: Record {
        size: 0x144,
        title: TitleField::Inline,
        numbers: FileNumbers::Running,
        levels_at: 0x41,
        level_of: [10, 13, 11, 14, 9, 12, 0, 0, 11, 14],
        video_at: 0x54,
        charts_at: 0xec,
        chart_count: 8,
        sounds_at: 0x10c,
        sound_count: 16,
        stage: StageArt::Overlay {
            at: 0xcc,
            // One 384x384 card; the words after it are not picture picks.
            picks: None,
        },
    },
    dan: Some(DanTable::Listed {
        runs: &[
            run(Style::Single, 0xfd9c0, 0, 17),
            run(Style::Double, 0xfde30, 2, 15),
        ],
        form: ListForm::Triples,
        charts: DAN_CHARTS,
    }),
    expert: Some(ExpertTable::Inline {
        at: 0xfefd8,
        count: 17,
    }),
    old_words: None,
};

const DISTORTED: Layout = Layout {
    executable: "SLPM_668.28",
    slug: "iidx13-distorted",
    file_tables: &[(0x112a00, 0xc0), (0x112ac0, 0x1900), (0x1143c0, 0x4a0)],
    file_records: FileRecords::Sectors,
    dat_files: &["BM2DX13A.DAT", "BM2DX13B.DAT", "BM2DX13C.DAT"],
    songs: (0x1353e0, 0x71c0),
    data1: (0x10d7f8, 0x1bf0),
    chart_key: None,
    bgm_pair: None,
    titles: &[],
    // Levels as GOLD; eight overlay slots instead of six and no BLACK charts.
    // The stage art is not the overlay file (0x60, BGA overlays on 27 songs)
    // but a DATA1 file every record names.
    record: Record {
        size: 0x118,
        title: TitleField::Inline,
        numbers: FileNumbers::Running,
        levels_at: 0x42,
        level_of: [11, 14, 12, 15, 10, 13, 0, 0, 12, 15],
        video_at: 0x54,
        charts_at: 0xb8,
        chart_count: 8,
        sounds_at: 0xd8,
        sound_count: 16,
        stage: StageArt::Overlay {
            at: 0x88,
            // The third pair is CARDINAL GATE's card.
            picks: Some(Picks {
                at: 0x8c,
                count: 6,
                another: 3,
            }),
        },
    },
    dan: Some(DanTable::Listed {
        runs: &[
            run(Style::Single, 0x10af90, 0, 18),
            run(Style::Double, 0x10b410, 2, 16),
        ],
        form: ListForm::Triples,
        charts: DAN_CHARTS,
    }),
    expert: Some(ExpertTable::Inline {
        at: 0x10c558,
        count: 17,
    }),
    old_words: None,
};

const GOLD: Layout = Layout {
    executable: "SLPM_669.95",
    slug: "iidx14-gold",
    file_tables: &[(0x11ad60, 0x248), (0x11afa0, 0x26ac), (0x11d64c, 0x72c)],
    dat_files: &["BM2DX14A.DAT", "BM2DX14B.DAT", "BM2DX14C.DAT"],
    songs: (0x156300, 0x76b4),
    data1: (0x118500, 0x1e80),
    file_records: FileRecords::Bytes,
    chart_key: Some(b"sEk2I1Xq7bz09Mtv"),
    bgm_pair: None,
    titles: &[],
    // Levels: beginner (both styles), 9 unused bytes, SP N/H/A, DP N/H/A
    // (BLACK shares ANOTHER's).
    record: Record {
        size: 0x11c,
        title: TitleField::Inline,
        numbers: FileNumbers::Running,
        levels_at: 0x42,
        level_of: [11, 14, 12, 15, 10, 13, 0, 0, 12, 15],
        video_at: 0x54,
        charts_at: 0xb4,
        chart_count: 10,
        sounds_at: 0xdc,
        sound_count: 16,
        stage: StageArt::Overlay {
            at: 0x84,
            picks: Some(Picks {
                at: 0x88,
                count: 6,
                another: 3,
            }),
        },
    },
    dan: Some(DanTable::Pairs(0x145250)),
    expert: Some(ExpertTable::Pointers {
        at: 0x144cc0,
        count: 16,
    }),
    old_words: None,
};

const DJ_TROOPERS: Layout = Layout {
    executable: "SLPM_551.17",
    slug: "iidx15-dj-troopers",
    file_tables: &[(0x134020, 0x240), (0x134260, 0x27c0), (0x136a20, 0x630)],
    dat_files: &["BM2DX15A.DAT", "BM2DX15B.DAT", "BM2DX15C.DAT"],
    songs: (0x16fe60, 0x80bc),
    data1: (0x131680, 0x1f80),
    file_records: FileRecords::Bytes,
    chart_key: Some(b"eqkBipx2wgExUAIG"),
    bgm_pair: None,
    titles: &[],
    record: RECORD_15_16,
    dan: Some(DanTable::Pairs(0x15e200)),
    expert: Some(ExpertTable::Pointers {
        at: 0x15dc70,
        count: 16,
    }),
    old_words: None,
};

const EMPRESS: Layout = Layout {
    executable: "SLPM_552.21",
    slug: "iidx16-empress",
    file_tables: &[(0x13c370, 0x2a0), (0x13c610, 0x28c8), (0x13eed8, 0x660)],
    dat_files: &["BM2DX16A.DAT", "BM2DX16B.DAT", "BM2DX16C.DAT"],
    songs: (0x178bf0, 0x7e54),
    data1: (0x139d00, 0x1cf0),
    file_records: FileRecords::Bytes,
    chart_key: Some(b"uYlFgVwOpaQmXdIR"),
    bgm_pair: None,
    titles: &[],
    record: Record {
        stage: StageArt::Card { at: 0x94 },
        ..RECORD_15_16
    },
    dan: Some(DanTable::Pairs(0x167fe0)),
    expert: Some(ExpertTable::Pointers {
        at: 0x167b10,
        count: 10,
    }),
    old_words: None,
};

const PREMIUM_BEST: Layout = Layout {
    executable: "SLPM_552.22",
    slug: "iidx16-premium-best",
    file_tables: &[(0x140e90, 0x2a0), (0x141130, 0x28f8), (0x143a28, 0x660)],
    songs: (0x17e0f0, 0x7850),
    data1: (0x139980, 0x5bd0),
    // The disc has the tables of EMPRESS's modes, filled with its own songs,
    // but no STANDARD, EXPERT or CLASS mode to play them in (RemyWiki).
    dan: None,
    expert: None,
    ..EMPRESS
};

impl Edition {
    /// The edition a PS2 boot executable name belongs to (`SLPM_552.21`), in any case.
    pub fn from_executable(name: &str) -> Option<Self> {
        EDITIONS
            .into_iter()
            .find(|e| e.executable().eq_ignore_ascii_case(name))
    }

    /// The disc's entry in the title list: its name and chart layout.
    pub fn title(self) -> &'static crate::titles::Title {
        crate::titles::find(self.executable()).expect("every edition is listed")
    }

    /// A short file-name-safe name (`iidx16-empress`).
    pub fn slug(self) -> &'static str {
        match self.tables() {
            Tables::Iidx(layout) => layout.slug,
            Tables::Popn(layout) => layout.slug,
        }
    }

    /// Whether this is a pop'n music disc: nine buttons, one chart file per
    /// song (`popn`).
    pub fn is_popn(self) -> bool {
        matches!(self.tables(), Tables::Popn(_))
    }

    /// Width over height to show the songs' background videos at, where it
    /// is not their frames' own: 3rd to 8th store 4:3 videos squeezed to 192
    /// (320 on 6th) pixels wide, 224 or 256 high, marked as square pixels.
    pub fn video_aspect(self) -> Option<f32> {
        matches!(
            self,
            Edition::ThirdStyle
                | Edition::FourthStyle
                | Edition::FifthStyle
                | Edition::SixthStyle
                | Edition::SeventhStyle
                | Edition::EighthStyle
        )
        .then_some(4.0 / 3.0)
    }

    /// The sound number a chart's BGM start plays: the song's BGM stream.
    pub fn bgm_sound(self) -> u16 {
        match self.tables() {
            Tables::Iidx(_) => 1,
            Tables::Popn(_) => crate::popn::BGM_SOUND,
        }
    }

    /// A sound number that starts with the BGM and plays nothing of its own
    /// (`Layout::bgm_pair`).
    pub fn bgm_pair_sound(self) -> Option<u16> {
        match self.tables() {
            Tables::Iidx(layout) => layout.bgm_pair,
            Tables::Popn(_) => None,
        }
    }

    fn executable(self) -> &'static str {
        match self.tables() {
            Tables::Iidx(layout) => layout.executable,
            Tables::Popn(layout) => layout.executable,
        }
    }

    pub fn name(self) -> &'static str {
        self.title().name
    }

    fn tables(self) -> Tables {
        Tables::Iidx(match self {
            Edition::ThirdStyle => &THIRD_STYLE,
            Edition::FourthStyle => &FOURTH_STYLE,
            Edition::FifthStyle => &FIFTH_STYLE,
            Edition::SixthStyle => &SIXTH_STYLE,
            Edition::SeventhStyle => &SEVENTH_STYLE,
            Edition::EighthStyle => &EIGHTH_STYLE,
            Edition::NinthStyle => &NINTH_STYLE,
            Edition::TenthStyle => &TENTH_STYLE,
            Edition::Red => &RED,
            Edition::HappySky => &HAPPY_SKY,
            Edition::Distorted => &DISTORTED,
            Edition::Gold => &GOLD,
            Edition::DjTroopers => &DJ_TROOPERS,
            Edition::Empress => &EMPRESS,
            Edition::PremiumBest => &PREMIUM_BEST,
            Edition::Popn8 => return Tables::Popn(&crate::popn::POPN8),
            Edition::Popn9 => return Tables::Popn(&crate::popn::POPN9),
            Edition::Popn10 => return Tables::Popn(&crate::popn::POPN10),
            Edition::Popn12 => return Tables::Popn(&crate::popn::POPN12),
            Edition::Popn13 => return Tables::Popn(&crate::popn::POPN13),
            Edition::Popn14 => return Tables::Popn(&crate::popn::POPN14),
        })
    }
}

/// Where an edition's tables are: an IIDX layout or a pop'n one.
enum Tables {
    Iidx(&'static Layout),
    Popn(&'static crate::popn::Layout),
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

impl Difficulty {
    /// BMS `#DIFFICULTY`: 1 BEGINNER … 5 INSANE.
    pub fn bms_number(self) -> u8 {
        match self {
            Difficulty::Beginner => 1,
            Difficulty::Normal => 2,
            Difficulty::Hyper => 3,
            Difficulty::Another => 4,
            Difficulty::Black => 5,
        }
    }
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

const TITLE_LEN: usize = 0x40;

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
    /// The `DATA1.DAT` file of the song's stage art (`Disc::stage_image`).
    /// On 16 it is the loading card by the rule found on PREMIUM BEST; EMPRESS
    /// records do not follow it, so its cards come from a hand-made table (`meta`).
    pub stage: Option<u32>,
    /// Which picture of a stage texture holding two the song shows (0 the
    /// left), usually and on ANOTHER (13, 14, 15); `None` for one picture.
    pub stage_picks: Option<(u16, Option<u16>)>,
    /// The file number of the song's background video (MPEG-2, stored as
    /// is), the first of the record's two video words as iidx-ps2tools reads them.
    pub video: Option<u32>,
}

/// A file's span inside one of the DAT files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileEntry {
    /// The DAT file: 0, 1, 2 for the edition's A/B/C files (0 for `DATA2.DAT`).
    pub dat: u8,
    pub offset: u64,
    pub size: u32,
}

/// The song list of one disc and where its files are.
#[derive(Debug, Clone)]
pub struct Disc {
    pub edition: Edition,
    pub songs: Vec<Song>,
    files: Vec<FileEntry>,
    dats: Vec<FileExtent>,
    data1: FileExtent,
    /// `DATA1.DAT` files: (byte offset, bytes).
    data1_files: Vec<(u64, u64)>,
    /// pop'n: each record's title and artist word rectangles (`song_names`).
    name_refs: Vec<(u16, u16)>,
    /// Each record's word sheet references (`word_ref_offsets`); on 8th
    /// (`OldWords`) the genre, title and artist numbers, as `(n, 0)`.
    word_refs: Vec<[(u16, u16); 4]>,
    /// Where the boot executable is (8th's word pictures are in it).
    executable: FileExtent,
    /// The game's own dan courses (empty where none was found).
    pub dan_courses: Vec<DanCourse>,
    /// The game's EXPERT courses (empty where none was found).
    pub expert_courses: Vec<ExpertCourse>,
}

/// One EXPERT course of the game: songs in order; the player picks the
/// style and difficulty they are played at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpertCourse {
    pub name: String,
    /// Song ids (`Song::id`).
    pub songs: Vec<u32>,
}

/// Reads the EXPERT course table (`ExpertTable`). The words after each song
/// id (all 1) are not known. A record that does not read as one gives no
/// courses rather than wrong ones.
fn read_expert_courses(exe: &[u8], table: ExpertTable, songs: &[Song]) -> Vec<ExpertCourse> {
    let (at, count, size, first, step) = match table {
        ExpertTable::Pointers { at, count } => (at, count, 64, 4, 12),
        ExpertTable::Inline { at, count } => (at, count, 124, 0x24, 16),
        ExpertTable::Unnamed {
            at,
            size,
            step,
            names,
        } => (at, names.len(), size, 0, step),
    };
    let mut courses = Vec::new();
    for c in 0..count {
        let Some(record) = exe.get(at + c * size..at + c * size + size) else {
            return Vec::new();
        };
        let name = match table {
            ExpertTable::Pointers { .. } => elf_string(exe, u32_at(record, 0)),
            ExpertTable::Inline { .. } => ascii(&record[..first]),
            ExpertTable::Unnamed { names, .. } => Some(match names[c] {
                "" => format!("#{}", c + 1),
                name => name.to_string(),
            }),
        };
        let Some(name) = name else {
            return Vec::new();
        };
        let ids: Vec<u32> = (0..5)
            .map(|i| match table {
                ExpertTable::Unnamed { .. } => u16_le(record, first + step * i).unwrap() as u32,
                _ => u32_at(record, first + step * i),
            })
            .collect();
        if !ids.iter().all(|id| songs.iter().any(|s| s.id == *id)) {
            return Vec::new();
        }
        courses.push(ExpertCourse { name, songs: ids });
    }
    courses
}

/// The ASCII string at a virtual address of the executable's first loaded
/// segment, or `None`.
fn elf_string(exe: &[u8], address: u32) -> Option<String> {
    let phoff = u32_at(exe.get(0x1c..0x20)?, 0) as usize;
    let header = exe.get(phoff..phoff + 32)?;
    let (offset, vaddr) = (u32_at(header, 4), u32_at(header, 8));
    let at = address.checked_sub(vaddr)?.checked_add(offset)? as usize;
    ascii(exe.get(at..at.checked_add(64)?.min(exe.len()))?)
}

/// The NUL-terminated printable ASCII text at the start of `bytes`.
fn ascii(bytes: &[u8]) -> Option<String> {
    let end = bytes.iter().position(|&b| b == 0)?;
    let text = &bytes[..end];
    (end > 0 && text.iter().all(|b| (0x20..0x7f).contains(b)))
        .then(|| String::from_utf8_lossy(text).into_owned())
}

/// One dan course of the game (段位認定, CLASS mode).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DanCourse {
    pub style: Style,
    /// The rank, as the game names it: `7級` … `1級`, `初段` … `十段`, `皆伝`.
    pub rank: &'static str,
    /// Song ids (`Song::id`) and chart difficulties, in order.
    pub charts: Vec<(u32, Difficulty)>,
}

/// Song and chart pairs in the dan table: 18 ranks of 4 per style, SP first.
const DAN_PAIRS: usize = 2 * 18 * 4;
const DAN_RANKS: [&str; 20] = [
    "7級",
    "6級",
    "5級",
    "4級",
    "3級",
    "2級",
    "1級",
    "初段",
    "二段",
    "三段",
    "四段",
    "五段",
    "六段",
    "七段",
    "八段",
    "九段",
    "十段",
    "皆伝",
    // 7th's SP-only extra courses.
    "裏八段",
    "裏九段",
];

/// Reads the dan table: (song id, chart) pairs of two words each, 4 per rank
/// from 7級 to 皆伝, SP then DP. `(0, 0)` is an empty slot (DP has no 7級
/// and 6級, and 3 songs a rank). Chart: 0 NORMAL, 1 HYPER (swapped in 9th to 11), 2 ANOTHER,
/// 3 BEGINNER, 4 BLACK. A pair naming no song, or the table not fitting,
/// gives no courses rather than wrong ones.
fn read_dan_courses(exe: &[u8], table: DanTable, songs: &[Song]) -> Vec<DanCourse> {
    match table {
        DanTable::Pairs(at) => read_dan_pairs(exe, at, songs),
        DanTable::Listed { form, runs, charts } => {
            let mut courses = Vec::new();
            for run in runs {
                let found = read_dan_lists(exe, form, run, charts, songs);
                if found.is_empty() {
                    return Vec::new();
                }
                courses.extend(found);
            }
            courses.sort_by_key(|c| (c.style, DAN_RANKS.iter().position(|&r| r == c.rank)));
            courses
        }
    }
}

/// A dan table chart number as a difficulty.
fn dan_chart(chart: u32, charts: &[Option<Difficulty>]) -> Option<Difficulty> {
    charts.get(chart as usize).copied().flatten()
}

/// One run of a `DanTable::Listed` table.
fn read_dan_lists(
    exe: &[u8],
    form: ListForm,
    run: &DanRun,
    table: &[Option<Difficulty>],
    songs: &[Song],
) -> Vec<DanCourse> {
    let DanRun {
        style,
        mut at,
        first,
        count,
    } = *run;
    let descending = matches!(form, ListForm::Fixed);
    let fits = if descending {
        first < DAN_RANKS.len() && count <= first + 1
    } else {
        first + count <= DAN_RANKS.len()
    };
    if !fits {
        return Vec::new();
    }
    let mut courses = Vec::new();
    for i in 0..count {
        let mut charts = Vec::new();
        let rank = if descending { first - i } else { first + i };
        if descending {
            let used = match style {
                Style::Single => 4,
                Style::Double => 3,
            };
            let Some(record) = exe.get(at..at + 68) else {
                return Vec::new();
            };
            for k in 0..used {
                let song = u16_le(record, k * 6).unwrap() as u32;
                let chart = u16_le(record, k * 6 + 2).unwrap() as u32;
                let Some(difficulty) = dan_chart(chart, table) else {
                    return Vec::new();
                };
                if !songs.iter().any(|s| s.id == song) {
                    return Vec::new();
                }
                charts.push((song, difficulty));
            }
            at += 68;
            courses.push(DanCourse {
                style,
                rank: DAN_RANKS[rank],
                charts,
            });
            continue;
        }
        loop {
            let (size, song, chart) = match form {
                ListForm::Triples => {
                    let Some(word) = exe.get(at..at + 4) else {
                        return Vec::new();
                    };
                    if u32_at(word, 0) == u32::MAX {
                        at += 4;
                        break;
                    }
                    let Some(triple) = exe.get(at..at + 12) else {
                        return Vec::new();
                    };
                    (12, u32_at(triple, 0), u32_at(triple, 4))
                }
                ListForm::Records => {
                    let Some(entry) = exe.get(at..at + 16) else {
                        return Vec::new();
                    };
                    let song = u16_le(entry, 0).unwrap();
                    if song == 0xffff {
                        at += 16;
                        break;
                    }
                    (16, song as u32, u16_le(entry, 2).unwrap() as u32)
                }
                ListForm::Fixed => unreachable!("read above"),
            };
            let Some(difficulty) = dan_chart(chart, table) else {
                return Vec::new();
            };
            if charts.len() == 4 || !songs.iter().any(|s| s.id == song) {
                return Vec::new();
            }
            charts.push((song, difficulty));
            at += size;
        }
        at = at.next_multiple_of(16);
        courses.push(DanCourse {
            style,
            rank: DAN_RANKS[rank],
            charts,
        });
    }
    courses
}

/// `DanTable::Pairs`.
fn read_dan_pairs(exe: &[u8], at: usize, songs: &[Song]) -> Vec<DanCourse> {
    let Some(bytes) = exe.get(at..at + DAN_PAIRS * 8) else {
        return Vec::new();
    };
    let mut courses = Vec::new();
    for (i, rank) in bytes.chunks_exact(32).enumerate() {
        let mut charts = Vec::new();
        for pair in rank.chunks_exact(8) {
            let (song, chart) = (u32_at(pair, 0), u32_at(pair, 4));
            if (song, chart) == (0, 0) {
                continue;
            }
            let Some(difficulty) = dan_chart(chart, DAN_CHARTS) else {
                return Vec::new();
            };
            if !songs.iter().any(|s| s.id == song) {
                return Vec::new();
            }
            charts.push((song, difficulty));
        }
        if charts.is_empty() {
            continue;
        }
        courses.push(DanCourse {
            style: if i < 18 { Style::Single } else { Style::Double },
            rank: DAN_RANKS[i % 18],
            charts,
        });
    }
    courses
}

fn missing(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{what} is not on the disc"),
    )
}

fn invalid(what: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

impl Disc {
    /// Reads the song and file tables from the disc's boot executable.
    pub fn open<R: Read + Seek>(
        disc: &mut SectorReader<R>,
        volume: &Volume,
        edition: Edition,
    ) -> io::Result<Self> {
        let layout = match edition.tables() {
            Tables::Iidx(layout) => layout,
            Tables::Popn(layout) => return Self::open_popn(disc, volume, edition, layout),
        };
        let exe = volume
            .read_file(disc, layout.executable)?
            .ok_or_else(|| missing(layout.executable))?;
        let dats = layout
            .dat_files
            .iter()
            .map(|name| volume.find(disc, name)?.ok_or_else(|| missing(name)))
            .collect::<io::Result<_>>()?;
        // 8th and older have no DATA1 (`data1` empty).
        let data1 = if layout.data1.1 == 0 {
            FileExtent { lba: 0, size: 0 }
        } else {
            volume
                .find(disc, "DATA1.DAT")?
                .ok_or_else(|| missing("DATA1.DAT"))?
        };
        let mut files = read_file_tables(&exe, layout.file_tables, layout.file_records)?;
        let mut dats: Vec<FileExtent> = dats;
        let mut songs = read_songs(&exe, layout.songs, &layout.record)?;
        for song in &mut songs {
            if let Some(&(_, title)) = layout.titles.iter().find(|&&(id, _)| id == song.id) {
                if song.title.is_empty() {
                    song.title = title.to_string();
                }
            }
        }
        if let TitleField::Pointer { base, .. } = layout.record.title {
            if matches!(
                layout.record.numbers,
                FileNumbers::Archives { .. } | FileNumbers::Executable
            ) {
                exe_charts(
                    &mut songs, &mut files, &mut dats, base, &exe, volume, disc, layout,
                )?;
            }
        }
        let dan_courses = layout
            .dan
            .map(|table| read_dan_courses(&exe, table, &songs))
            .unwrap_or_default();
        let expert_courses = layout
            .expert
            .map(|table| read_expert_courses(&exe, table, &songs))
            .unwrap_or_default();
        // DATA1 records: id, sector, sectors, 0.
        let data1_files = table(&exe, layout.data1)?
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
            data1_files,
            name_refs: Vec::new(),
            word_refs: match (word_ref_offsets(edition), layout.old_words) {
                (Some(offsets), _) => {
                    read_word_refs(&exe, layout.songs, layout.record.size, offsets)
                }
                (None, Some(words)) => (0..layout.songs.1 / layout.record.size)
                    .map(|id| {
                        let at = layout.songs.0 + id * layout.record.size;
                        let n = |k: usize| (u16_le(&exe, at + k).unwrap_or(u16::MAX), 0);
                        [
                            n(words.genre_at),
                            n(words.title_at),
                            n(words.artist_at),
                            (u16::MAX, 0),
                        ]
                    })
                    .collect(),
                (None, None) => Vec::new(),
            },
            executable: volume
                .find(disc, layout.executable)?
                .unwrap_or(FileExtent { lba: 0, size: 0 }),
            dan_courses,
            expert_courses,
        })
    }

    fn open_popn<R: Read + Seek>(
        disc: &mut SectorReader<R>,
        volume: &Volume,
        edition: Edition,
        layout: &crate::popn::Layout,
    ) -> io::Result<Self> {
        let exe = volume
            .read_file(disc, layout.executable)?
            .ok_or_else(|| missing(layout.executable))?;
        let name = layout.dat;
        let dat = volume.find(disc, name)?.ok_or_else(|| missing(name))?;
        let short = || invalid("the executable is shorter than its tables".to_string());
        let files = crate::popn::read_files(&exe, layout).ok_or_else(short)?;
        let mut songs = crate::popn::read_songs(&exe, layout, files.len(), &mut |file| {
            files.get(file as usize).is_some_and(|entry| {
                disc.read_at(dat.offset() + entry.offset, 4)
                    .is_ok_and(|b| b.first() == Some(&0x20))
            })
        })
        .ok_or_else(short)?;
        if layout.record.levels_at.is_none() {
            for song in &mut songs {
                crate::popn::real_charts(song, &mut |file| {
                    let entry = files.get(file as usize)?;
                    disc.read_at(dat.offset() + entry.offset, entry.size as usize)
                        .ok()
                });
            }
            songs.retain(|s| !s.charts.is_empty());
        }
        for song in &mut songs {
            crate::popn::pick_sound_sets(song, layout.record.bgm, &mut |file, bytes| {
                let entry = files.get(file as usize)?;
                let len = (entry.size as usize).min(bytes);
                disc.read_at(dat.offset() + entry.offset, len).ok()
            });
        }
        let mut files = files;
        let mut dats = vec![dat];
        if let Some(streams) = layout.streams {
            let stream_dat = volume
                .find(disc, streams.dat)?
                .ok_or_else(|| missing(streams.dat))?;
            let stream_files = crate::popn::read_files(
                &exe,
                &crate::popn::Layout {
                    files: streams.files,
                    ..*layout
                },
            )
            .ok_or_else(short)?;
            let base = files.len() as u32;
            files.extend(stream_files.into_iter().map(|f| FileEntry { dat: 1, ..f }));
            dats.push(stream_dat);
            crate::popn::assign_streams(&mut songs, &exe, layout, base);
        }
        Ok(Self {
            edition,
            songs,
            files,
            dats,
            // `IMAGE.DAT` stands in for DATA1: the song banners are there.
            data1: volume
                .find(disc, crate::popn::IMAGE_FILE)?
                .unwrap_or(FileExtent { lba: 0, size: 0 }),
            data1_files: crate::popn::read_images(&exe, layout).unwrap_or_default(),
            name_refs: crate::popn::read_name_refs(&exe, layout),
            word_refs: Vec::new(),
            executable: FileExtent { lba: 0, size: 0 },
            dan_courses: Vec::new(),
            expert_courses: Vec::new(),
        })
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
            return Err(invalid(format!(
                "file {file} runs past the end of {}",
                match self.edition.tables() {
                    Tables::Iidx(layout) => layout.dat_files[entry.dat as usize],
                    Tables::Popn(layout) => layout.dat,
                }
            )));
        }
        disc.read_at(dat.offset() + entry.offset, entry.size as usize)
    }

    /// Chart file number `file`, decrypted and decompressed: the `.ply` bytes.
    pub fn read_chart<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        file: u32,
    ) -> io::Result<Vec<u8>> {
        let stored = self.read_file(disc, file)?;
        let layout = match self.edition.tables() {
            Tables::Iidx(layout) => layout,
            // Stored plain: eight event lists (`popn`).
            Tables::Popn(_) => return Ok(stored),
        };
        let plain = match layout.chart_key {
            Some(key) => crate::blowfish::decrypt_file(key, &stored),
            None => stored,
        };
        crate::lz::decode(&plain)
            .ok_or_else(|| invalid(format!("chart file {file} does not decompress")))
    }

    /// A chart parsed, without serializing it to BMS text.
    pub fn chart_ply<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        chart: &Chart,
    ) -> io::Result<Ply> {
        let bytes = self.read_chart(disc, chart.file)?;
        if self.edition.is_popn() {
            use crate::popn::BgmStart;
            let bgm = match self.edition.tables() {
                Tables::Popn(layout) if layout.streams.is_some() => BgmStart::StreamAtZero,
                Tables::Popn(layout) if layout.record.bgm => BgmStart::Stream,
                _ => BgmStart::KeySound,
            };
            return crate::popn::chart(&bytes, chart.difficulty, bgm)
                .ok_or_else(|| invalid(format!("chart file {} is cut short", chart.file)));
        }
        Ply::parse_as(&bytes, self.edition.title().charts).map_err(|e| invalid(e.to_string()))
    }

    /// The chart as BMS text (`ply_bms`), titled with the disc's own title.
    /// The text depends on the disc alone, so it can identify the chart.
    pub fn chart_bms<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        song: &Song,
        chart: &Chart,
    ) -> io::Result<String> {
        let ply = self.chart_ply(disc, chart)?;
        let header = Header {
            title: song.title.clone(),
            subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            level: chart.level,
            difficulty: chart.difficulty.bms_number(),
            double: chart.style == Style::Double,
            nine_buttons: self.edition.is_popn(),
            sound_name,
        };
        to_bms(&ply, &header).map_err(|e| invalid(e.to_string()))
    }

    /// How many files the `DATA1.DAT` table lists.
    pub fn data1_count(&self) -> usize {
        self.data1_files.len()
    }

    /// `DATA1.DAT` file `file` as stored, or `None` for a number the table lacks.
    pub fn read_data1<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        file: u32,
    ) -> io::Result<Option<Vec<u8>>> {
        let Some(&(offset, size)) = self.data1_files.get(file as usize) else {
            return Ok(None);
        };
        if size == 0 || offset + size > self.data1.size {
            return Ok(None);
        }
        disc.read_at(self.data1.offset() + offset, size as usize)
            .map(Some)
    }

    /// The first picture of `DATA1.DAT` file `file` (its file 0 is not a
    /// picture): a 16 song's loading card. `None` when it has none.
    pub fn card_image<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        file: u32,
    ) -> io::Result<Option<Rgba>> {
        Ok(self
            .read_data1(disc, file)?
            .and_then(|bytes| first_picture(&bytes)))
    }

    /// The word pictures of a `song`: its title and its artist (and on
    /// PREMIUM BEST its small title), as coverage masks. `None` where the disc
    /// has no word pictures for it.
    pub fn song_names<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        song: &Song,
    ) -> io::Result<Option<crate::popn::SongNames>> {
        let layout = match self.edition.tables() {
            Tables::Popn(layout) => layout,
            Tables::Iidx(_) => return self.iidx_song_names(disc, song),
        };
        let Some(files) = layout.names else {
            return Ok(None);
        };
        let Some(&(title, artist)) = self.name_refs.get(song.id as usize) else {
            return Ok(None);
        };
        // Titles are white or coloured on black; anything else is a stand-in
        // (`popn::on_black`).
        let mut word = |file: u32, n: u16, title: bool| -> io::Result<Option<crate::popn::Mask>> {
            Ok(self
                .read_data1(disc, file)?
                .and_then(|bytes| crate::popn::rect_picture(&bytes, n as u32))
                .filter(|picture| !title || crate::popn::on_black(picture))
                .map(|picture| crate::popn::Mask::from_rgba(&picture)))
        };
        Ok(Some(crate::popn::SongNames {
            title: match files.title {
                Some(file) => word(file, title, true)?,
                None => None,
            },
            small_title: None,
            artist: word(files.artist, artist, false)?,
            genre: match files.genre {
                Some(file) => word(file, title, false)?,
                None => None,
            },
            popn: true,
        }))
    }

    /// The word pictures of an IIDX `song` (see `word_ref_offsets`).
    fn iidx_song_names<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        song: &Song,
    ) -> io::Result<Option<crate::popn::SongNames>> {
        let Some(refs) = self.word_refs.get(song.id as usize) else {
            return Ok(None);
        };
        if let Tables::Iidx(Layout {
            old_words: Some(words),
            ..
        }) = self.edition.tables()
        {
            let [(genre, _), (title, _), (artist, _), _] = *refs;
            let mut word = |sheets: &[Sheet], n: u16| -> io::Result<Option<crate::popn::Mask>> {
                Ok(self
                    .old_word(disc, sheets, n)?
                    .map(|picture| crate::popn::Mask::from_rgba(&picture)))
            };
            return Ok(Some(crate::popn::SongNames {
                title: word(words.title, title)?,
                small_title: None,
                artist: word(words.artist, artist)?,
                genre: word(words.genre, genre)?,
                popn: false,
            }));
        }
        let [genre, big, artist, small] = *refs;
        let rect_base = if matches!(
            self.edition,
            Edition::Red | Edition::TenthStyle | Edition::NinthStyle
        ) {
            0x90
        } else {
            0xd0
        };
        let mask = |disc: &mut SectorReader<R>, (index, container): (u16, u16)| {
            Ok::<_, io::Error>(
                self.word_sprite(disc, container, index, rect_base)?
                    .map(|picture| crate::popn::Mask::from_rgba(&picture)),
            )
        };
        Ok(Some(crate::popn::SongNames {
            title: mask(disc, big)?,
            small_title: mask(disc, small)?,
            artist: mask(disc, artist)?,
            genre: mask(disc, genre)?,
            popn: false,
        }))
    }

    /// Word `n` of 8th's `sheets` (`OldWords`): the sheet that holds it
    /// unpacked from the executable, cut to its rectangle.
    fn old_word<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        sheets: &[Sheet],
        n: u16,
    ) -> io::Result<Option<Rgba>> {
        let mut n = n as usize;
        let Some(&(picture_at, rects_at, _)) = sheets.iter().find(|&&(_, _, count)| {
            let here = n < count;
            if !here {
                n -= count;
            }
            here
        }) else {
            return Ok(None);
        };
        // A sheet packs to well under this; the LZ stops at its end code.
        const PACKED: u64 = 0x40000;
        let start = self.executable.offset() + picture_at as u64;
        let len = PACKED.min(self.executable.size.saturating_sub(picture_at as u64));
        let packed = disc.read_at(start, len as usize)?;
        let rect = disc.read_at(self.executable.offset() + (rects_at + n * 8) as u64, 8)?;
        let Some(sheet) = crate::lz::decode(&packed)
            .as_deref()
            .and_then(crate::ps2tex::decode_old_picture)
        else {
            return Ok(None);
        };
        let field = |k: usize| u16_le(&rect, k * 2).unwrap_or(0) as u32;
        let (x, y, w, h) = (field(0), field(1), field(2), field(3));
        if w == 0 || h == 0 || x + w > sheet.width || y + h > sheet.height {
            return Ok(None);
        }
        let pixels = (y..y + h)
            .flat_map(|row| {
                let at = (row * sheet.width + x) as usize;
                sheet.pixels[at..at + w as usize].iter().copied()
            })
            .collect();
        Ok(Some(Rgba {
            width: w,
            height: h,
            pixels,
        }))
    }

    /// Rectangle `index` of the sprite sheet `container`: its picture as
    /// `Rgba`. The sheet's first file holds the rectangles (`x, y, w, h`,
    /// 8 bytes each from 0xd0; the count is the u16 at 6). A sheet of more than
    /// one texture stacks them 1024 texels apart in `y`.
    fn word_sprite<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        container: u16,
        index: u16,
        rect_base: usize,
    ) -> io::Result<Option<Rgba>> {
        let Some(archive) = self.read_data1(disc, container as u32)? else {
            return Ok(None);
        };
        let Some(mut files) = archive_files(&archive) else {
            return Ok(None);
        };
        let Some((unpacked, Some(bytes))) = files.next() else {
            return Ok(None);
        };
        let Some(table) = decode_lzss(bytes, unpacked) else {
            return Ok(None);
        };
        let count = u16_le(&table, 6).unwrap_or(0);
        // The rectangles start at `rect_base` (0xd0, or 0x90 where the
        // header has room for eight more); the count covers the rest.
        let rects = (table.len().saturating_sub(rect_base)) / 8;
        if index as usize >= rects.min(count as usize + (0xd0 - rect_base) / 8) {
            return Ok(None);
        }
        let at = rect_base + index as usize * 8;
        let Some(rect) = table.get(at..at + 8) else {
            return Ok(None);
        };
        let field = |k: usize| u16_le(rect, k * 2).unwrap_or(0) as i16 as i32;
        let (x, y, w, h) = (field(0), field(1), field(2) as u32, field(3) as u32);
        // The sheet's file 0 lists each texture's size as (width, height) u16
        // pairs from 0x10, in file order (file k is pair k - 1).
        let listed = |k: usize| {
            let at = 0x10 + (k - 1) * 4;
            Some((u16_le(&table, at)? as u32, u16_le(&table, at + 2)? as u32))
        };
        let textures: Vec<Rgba> = files
            .enumerate()
            .filter_map(|(i, (unpacked, bytes))| {
                let (w, h) = listed(i + 1)
                    .filter(|&(w, h)| (w * h) as usize + 0x400 == unpacked)
                    .or_else(|| texture_size(unpacked))?;
                decode_psmt8(&decode_lzss(bytes?, unpacked)?, w, h)
            })
            .collect();
        let stack = y.max(0) as usize / 1024;
        let Some(texture) = textures.get(stack) else {
            return Ok(None);
        };
        let top = (y - stack as i32 * 1024).max(0) as u32;
        if x < 0 || top + h > texture.height || x as u32 + w > texture.width {
            return Ok(None);
        }
        let pixels = (top..top + h)
            .flat_map(|row| {
                let at = (row * texture.width + x as u32) as usize;
                texture.pixels[at..at + w as usize].iter().copied()
            })
            .collect();
        Ok(Some(Rgba {
            width: w,
            height: h,
            pixels,
        }))
    }

    /// The picture shown while `song` loads: 13, 14 and 15's stage image, of the
    /// ANOTHER variant when `another` and the texture holds two (10 to 15); 16's loading
    /// card.
    /// Where a song's stage art is on the disc: the absolute byte offset and
    /// size of its DATA1 file (pop'n: its banner sheet), and the picture's
    /// index in that file (pop'n's banner; 0 otherwise). The art is the same
    /// for every chart of the song, except for the Another variant.
    pub fn stage_extent(&self, song: &Song) -> Option<(u64, u64, u32)> {
        let (file, index) = match self.edition.tables() {
            Tables::Popn(layout) => (layout.banners, song.stage?),
            Tables::Iidx(_) => (song.stage?, 0),
        };
        let &(offset, size) = self.data1_files.get(file as usize)?;
        (size != 0 && offset + size <= self.data1.size)
            .then(|| (self.data1.offset() + offset, size, index))
    }

    pub fn stage_image<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        song: &Song,
        another: bool,
    ) -> io::Result<Option<Rgba>> {
        let layout = match self.edition.tables() {
            Tables::Iidx(layout) => layout,
            // The song's banner (genre and title).
            Tables::Popn(layout) => {
                let Some(banner) = song.stage else {
                    return Ok(None);
                };
                return Ok(self
                    .read_data1(disc, layout.banners)?
                    .and_then(|file| crate::popn::rect_picture(&file, banner)));
            }
        };
        match layout.record.stage {
            StageArt::None => Ok(None),
            StageArt::Card { .. } => match song.stage {
                Some(file) => self.card_image(disc, file),
                None => Ok(None),
            },
            StageArt::Overlay { picks, .. } => {
                let Some(file) = song.stage else {
                    return Ok(None);
                };
                let Some(archive) = self.read_data1(disc, file)? else {
                    return Ok(None);
                };
                let image = archive_texture(&archive, 1).and_then(|(size, raw)| {
                    let (width, height) = texture_size(size)?;
                    decode_psmt8(&raw, width, height)
                });
                let picture = match song.stage_picks {
                    Some((usual, on_another)) if another => on_another.unwrap_or(usual),
                    Some((usual, _)) => usual,
                    // 10 to 12 records name no picture: a wide texture (RED's
                    // A and AA) holds the usual card and ANOTHER's.
                    None if picks.is_none() => {
                        return Ok(image.map(|image| select_packed_variant(image, another)))
                    }
                    None => return Ok(image),
                };
                Ok(image.map(|image| select_packed_variant(image, picture == 1)))
            }
        }
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
    /// Gives a chart slot its own (key sound bank, BGM stream).
    pub(crate) fn set_sound_files(
        &mut self,
        style: Style,
        difficulty: Difficulty,
        bank: u32,
        bgm: u32,
    ) {
        let slot = CHART_SLOTS
            .iter()
            .position(|&s| s == (style, difficulty))
            .unwrap();
        if let Some(&(key, stream)) = SOUND_PAIRS.get(slot) {
            self.sounds[key] = bank as u16;
            self.sounds[stream] = bgm as u16;
        }
    }

    /// Every key sound bank of the song, for a sound missing from the one a
    /// chart plays (3rd's NORMAL charts take SP HYPER's bank but play a
    /// few of DP HYPER's).
    pub fn key_banks(&self) -> Vec<u32> {
        let mut banks: Vec<u32> = SOUND_PAIRS
            .iter()
            .map(|&(key, _)| self.sounds[key])
            .filter(|&f| f != NO_SOUND)
            .map(u32::from)
            .collect();
        banks.dedup();
        banks
    }

    /// The (key sound bank, BGM stream) file numbers a chart plays. A chart
    /// slot without its own takes the other style's of its difficulty, then
    /// DP HYPER's (double) or SP HYPER's, field by field, as
    /// iidx-ps2tools resolves them.
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
                // 9th GENOM SCREAMS DP ANOTHER plays SP ANOTHER's, 8th
                // ABSTRACT DP NORMAL SP NORMAL's.
                .or_else(|| own(slot ^ 1, field))
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

/// A packed texture holds two 298-pixel-wide stage variants edge-to-edge at
/// the start of its 640-pixel width. The second variant is selected for
/// ANOTHER; the remaining 44 pixels are padding. A narrower texture holds one.
fn select_packed_variant(image: Rgba, another: bool) -> Rgba {
    const VARIANT_WIDTH: u32 = 298;

    if image.width < VARIANT_WIDTH * 2 {
        return image;
    }
    let start = if another { VARIANT_WIDTH } else { 0 };
    let mut pixels = Vec::with_capacity((VARIANT_WIDTH * image.height) as usize);
    for y in 0..image.height {
        let row = (y * image.width + start) as usize;
        pixels.extend_from_slice(&image.pixels[row..row + VARIANT_WIDTH as usize]);
    }
    Rgba {
        width: VARIANT_WIDTH,
        height: image.height,
        pixels,
    }
}

/// A DATA1 archive: `u16 data offset, u16 count`, (decompressed, compressed)
/// `u32` size pairs for `count + 1` files ending at the data offset, then the
/// files back to back. Returns file `wanted` decompressed, with its size.
fn archive_texture(archive: &[u8], wanted: usize) -> Option<(usize, Vec<u8>)> {
    archive_files(archive)?
        .nth(wanted)
        .and_then(|(unpacked, bytes)| Some((unpacked, decode_lzss(bytes?, unpacked)?)))
}

/// The files of a DATA1 archive (`archive_texture`): (decompressed size,
/// stored bytes, `None` past the end).
fn archive_files(archive: &[u8]) -> Option<impl Iterator<Item = (usize, Option<&[u8]>)>> {
    let data = u16::from_le_bytes([*archive.first()?, *archive.get(1)?]) as usize;
    let count = u16::from_le_bytes([*archive.get(2)?, *archive.get(3)?]) as usize;
    let sizes = data.checked_sub((count + 1) * 8)?;
    let mut at = data;
    Some((0..=count).map_while(move |k| {
        let record = archive.get(sizes + k * 8..sizes + k * 8 + 8)?;
        let unpacked = u32_at(record, 0) as usize;
        let packed = u32_at(record, 4) as usize;
        let bytes = archive.get(at..at.checked_add(packed)?);
        at += packed;
        Some((unpacked, bytes))
    }))
}

/// The record offsets of the word sheet references (see `word_ref_offsets`): each an
/// `(index, container)` pair of u16s, for genre, big title, artist and small
/// title (`iidx_ps2_disc.md`, found by checking the sheets by eye).
/// Where each edition's record keeps the word references, as offsets of the
/// index word (the container is two bytes on): genre, big title, artist,
/// small title. The sheets of one song are consecutive containers: small
/// title, big title, genre, artist (EMPRESS 298 to 301, DJ TROOPERS 330 to 333,
/// GOLD 313 to 316, DistorteD 284 to 287), so the offsets follow the layout.
fn word_ref_offsets(edition: Edition) -> Option<[usize; 4]> {
    match edition {
        Edition::Empress | Edition::PremiumBest | Edition::DjTroopers => {
            Some([0xac, 0xb0, 0xb4, 0xb8])
        }
        Edition::Gold => Some([0xa0, 0xa4, 0xa8, 0xac]),
        Edition::Red => Some([0xec, 0xf0, 0xf4, 0xf8]),
        Edition::TenthStyle | Edition::NinthStyle => Some([0xdc, 0xe0, 0xe4, 0xec]),
        Edition::Distorted => Some([0xa4, 0xa8, 0xac, 0xb0]),
        Edition::HappySky => Some([0xd8, 0xdc, 0xe0, 0xe4]),
        // RED (0xec..0xf8, sheets 281-284) and 10th style (0xdc..0xec, sheets
        // 162-165) are found but not verified: 34 and 48 pictures are empty.
        // GOLD (0xa0..0xac) and DistorteD (0xa4..0xb0) have the sheets at these
        // offsets, but their genre and big title pictures are not verified yet.
        _ => None,
    }
}

/// Each song record's word references (`offsets`), in record order.
fn read_word_refs(
    exe: &[u8],
    (at, bytes): (usize, usize),
    record: usize,
    offsets: [usize; 4],
) -> Vec<[(u16, u16); 4]> {
    let at_record = |id: usize| at + id * record;
    (0..bytes / record)
        .map(|id| {
            offsets.map(|off| {
                let r = at_record(id) + off;
                let index = u16_le(exe, r).unwrap_or(0);
                (index, u16_le(exe, r + 2).unwrap_or(0))
            })
        })
        .collect()
}

fn u16_le(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

/// The first picture after file 0 of a DATA1 archive.
fn first_picture(archive: &[u8]) -> Option<Rgba> {
    archive_files(archive)?
        .skip(1)
        .find_map(|(unpacked, bytes)| {
            let (w, h) = texture_size(unpacked)?;
            decode_psmt8(&decode_lzss(bytes?, unpacked)?, w, h)
        })
}

fn table(exe: &[u8], (at, len): (usize, usize)) -> io::Result<&[u8]> {
    exe.get(at..at + len)
        .ok_or_else(|| invalid("the executable is shorter than its tables".to_string()))
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// The file tables, numbered across the DAT files in order. Each record
/// is 12 bytes: sector inside the DAT, an unused word, size in bytes.
/// 6th: gives each chart (an executable address) a file of the executable,
/// added as one more DAT, running to the next chart.
#[allow(clippy::too_many_arguments)]
fn exe_charts<R: Read + Seek>(
    songs: &mut [Song],
    files: &mut Vec<FileEntry>,
    dats: &mut Vec<FileExtent>,
    base: u32,
    exe: &[u8],
    volume: &Volume,
    disc: &mut SectorReader<R>,
    layout: &Layout,
) -> io::Result<()> {
    let extent = volume
        .find(disc, layout.executable)?
        .ok_or_else(|| missing(layout.executable))?;
    let dat = dats.len() as u8;
    dats.push(extent);
    let mut starts: Vec<u32> = songs
        .iter()
        .flat_map(|s| s.charts.iter().map(|c| c.file.wrapping_sub(base)))
        .filter(|&at| (at as usize) < exe.len())
        .collect();
    starts.sort_unstable();
    starts.dedup();
    let first = files.len() as u32;
    for (i, &at) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(exe.len() as u32);
        files.push(FileEntry {
            dat,
            offset: at as u64,
            size: end - at,
        });
    }
    for song in songs {
        song.charts.retain_mut(|chart| {
            let at = chart.file.wrapping_sub(base);
            match starts.binary_search(&at) {
                Ok(i) => {
                    chart.file = first + i as u32;
                    true
                }
                Err(_) => false,
            }
        });
    }
    Ok(())
}

fn read_file_tables(
    exe: &[u8],
    tables: &[(usize, usize)],
    records: FileRecords,
) -> io::Result<Vec<FileEntry>> {
    let mut files = Vec::new();
    let len = match records {
        FileRecords::Sectors => 8,
        FileRecords::Bytes => 12,
        FileRecords::Hashed => 16,
        FileRecords::ShortHashed => 12,
    };
    for (dat, &span) in tables.iter().enumerate() {
        for record in table(exe, span)?.chunks_exact(len) {
            let size = match records {
                FileRecords::Sectors => u32_at(record, 4) * crate::SECTOR_SIZE as u32,
                FileRecords::Bytes | FileRecords::Hashed => u32_at(record, 8),
                FileRecords::ShortHashed => u32_at(record, 4),
            };
            files.push(FileEntry {
                dat: dat as u8,
                offset: u32_at(record, 0) as u64 * crate::SECTOR_SIZE,
                size,
            });
        }
    }
    Ok(files)
}

/// A tutorial or practice record, not a song: one SP BEGINNER chart under
/// `PRACTICE` (13), `TUTORIAL` (14), `TUTORIAL01`–`04` (15) or no title (16).
fn is_tutorial(title: &str, charts: &[Chart]) -> bool {
    let named = title.is_empty() || title == "PRACTICE" || title.starts_with("TUTORIAL");
    let beginner_only = matches!(
        charts,
        [Chart {
            style: Style::Single,
            difficulty: Difficulty::Beginner,
            ..
        }]
    );
    named && beginner_only
}

fn read_songs(exe: &[u8], span: (usize, usize), layout: &Record) -> io::Result<Vec<Song>> {
    let mut songs = Vec::new();
    let mut two_pictures = Vec::new();
    for (id, record) in table(exe, span)?.chunks_exact(layout.size).enumerate() {
        let levels = &record[layout.levels_at..];
        let mut charts: Vec<Chart> = CHART_SLOTS[..layout.chart_count]
            .iter()
            .enumerate()
            .filter_map(|(i, &(style, difficulty))| {
                let file = u32_at(record, layout.charts_at + i * 4);
                (file != 0 && file != u32::MAX).then(|| Chart {
                    style,
                    difficulty,
                    level: levels[layout.level_of[i]],
                    file: layout.numbers.chart(file),
                })
            })
            .collect();
        if charts.is_empty() {
            continue;
        }
        charts.sort_by_key(|c| (c.style, c.difficulty));
        let mut sounds = [NO_SOUND; 16];
        for (i, sound) in sounds.iter_mut().enumerate().take(layout.sound_count) {
            let at = layout.sounds_at + i * 2;
            let raw = u16::from_le_bytes([record[at], record[at + 1]]);
            *sound = if raw == NO_SOUND {
                raw
            } else {
                layout.numbers.other(raw as u32, record) as u16
            };
        }
        let title = match layout.title {
            TitleField::Inline => &record[..TITLE_LEN],
            TitleField::Pointer { at, base } => {
                let start = u32_at(record, at).wrapping_sub(base) as usize;
                exe.get(start..(start + TITLE_LEN).min(exe.len()))
                    .unwrap_or(&[])
            }
        };
        let title = title
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| if b.is_ascii() { b as char } else { '?' })
            .collect::<String>()
            .trim()
            .to_string();
        if is_tutorial(&title, &charts) {
            continue;
        }
        let stage_picks = match layout.stage {
            StageArt::Overlay { picks: None, .. } => None,
            StageArt::Overlay {
                picks: Some(picks), ..
            } => {
                let pick = |slot: usize| {
                    let at = picks.at + slot * 4;
                    let picture = u16::from_le_bytes([record[at], record[at + 1]]);
                    let file = u16::from_le_bytes([record[at + 2], record[at + 3]]);
                    (file != 0).then_some(picture)
                };
                let second = (0..picks.count).any(|slot| pick(slot).is_some_and(|p| p != 0));
                pick(0).map(|usual| (usual, pick(picks.another), second))
            }
            StageArt::Card { .. } | StageArt::None => None,
        };
        let stage = match layout.stage {
            StageArt::Overlay { at, .. } | StageArt::Card { at } => {
                let file = u32_at(record, at);
                (file != 0 && file != u32::MAX).then_some(file)
            }
            StageArt::None => None,
        };
        songs.push(Song {
            id: id as u32,
            title,
            charts,
            sounds,
            stage,
            stage_picks: stage_picks.map(|(usual, another, _)| (usual, another)),
            video: {
                let file = u32_at(record, layout.video_at);
                (file != 0 && file & 0xffff != 0xffff).then(|| layout.numbers.other(file, record))
            },
        });
        if stage_picks.is_some_and(|(_, _, second)| second) {
            two_pictures.extend(stage);
        }
    }
    // A wide texture holds one picture unless some record (not always the
    // song's own: GOLD's 5KEYS DENIM) names the second.
    for song in &mut songs {
        if !song.stage.is_some_and(|f| two_pictures.contains(&f)) {
            song.stage_picks = None;
        }
    }
    Ok(songs)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECORD: &Record = &EMPRESS.record;

    fn record(title: &str, levels: [u8; 10], charts: [u32; 10]) -> Vec<u8> {
        let mut r = vec![0u8; RECORD.size];
        r[..title.len()].copy_from_slice(title.as_bytes());
        r[RECORD.levels_at..RECORD.levels_at + 10].copy_from_slice(&levels);
        for (i, c) in charts.iter().enumerate() {
            r[RECORD.charts_at + i * 4..][..4].copy_from_slice(&c.to_le_bytes());
        }
        r
    }

    #[test]
    fn a_record_lists_its_charts_with_their_levels() {
        // SP N 4, SP H 7, SP A 10, DP H 8; charts stored H, DPH, A, DPA, N, …
        let levels = [4, 7, 10, 0, 0, 0, 8, 0, 0, 0];
        let charts = [11, 12, 13, 0, 14, u32::MAX, 0, 0, 0, 0];
        let table = record("ANISAKIS", levels, charts);
        let songs = read_songs(&table, (0, RECORD.size), RECORD).unwrap();
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
    fn a_gold_record_shares_beginner_and_another_levels() {
        let g = &GOLD.record;
        let mut r = vec![0u8; g.size];
        // Beginner 2, nine unused bytes, SP N/H/A 4/7/9, DP N/H/A 4/9/0.
        r[g.levels_at..g.levels_at + 16]
            .copy_from_slice(&[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 7, 9, 4, 9, 0]);
        // SP H, DP H, SP A, -, SP N, DP N, SP B.
        for (i, file) in [51u32, 52, 53, 0, 49, 50, 54].iter().enumerate() {
            r[g.charts_at + i * 4..][..4].copy_from_slice(&file.to_le_bytes());
        }
        let songs = read_songs(&r, (0, g.size), g).unwrap();
        let got: Vec<(Style, Difficulty, u8)> = songs[0]
            .charts
            .iter()
            .map(|c| (c.style, c.difficulty, c.level))
            .collect();
        assert_eq!(
            got,
            [
                (Style::Single, Difficulty::Beginner, 2),
                (Style::Single, Difficulty::Normal, 4),
                (Style::Single, Difficulty::Hyper, 7),
                (Style::Single, Difficulty::Another, 9),
                (Style::Double, Difficulty::Normal, 4),
                (Style::Double, Difficulty::Hyper, 9),
            ]
        );
    }

    #[test]
    fn tutorials_are_not_songs() {
        let beginner = |file| Chart {
            style: Style::Single,
            difficulty: Difficulty::Beginner,
            level: 1,
            file,
        };
        assert!(is_tutorial("TUTORIAL02", &[beginner(1)]));
        assert!(is_tutorial("PRACTICE", &[beginner(1)]));
        assert!(is_tutorial("", &[beginner(1)]));
        // A song with a BEGINNER chart, or a tutorial-like title on a real song.
        assert!(!is_tutorial("GAMBOL", &[beginner(1)]));
        let normal = Chart {
            difficulty: Difficulty::Normal,
            ..beginner(2)
        };
        assert!(!is_tutorial("TUTORIAL", &[beginner(1), normal]));
    }

    #[test]
    fn a_record_without_charts_is_not_a_song() {
        let mut table = record("", [0; 10], [0; 10]);
        table.extend(record("B", [1; 10], [5; 10]));
        let songs = read_songs(&table, (0, RECORD.size * 2), RECORD).unwrap();
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
        let files =
            read_file_tables(&exe, &[(0, 12), (12, 24), (36, 0)], FileRecords::Bytes).unwrap();
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
    fn dan_courses_skip_empty_slots_and_refuse_a_bad_table() {
        let charts = [11, 12, 13, 0, 14, 0, 0, 0, 0, 0];
        let mut table = record("A", [1; 10], charts);
        table.extend(record("B", [1; 10], charts));
        let songs = read_songs(&table, (0, RECORD.size * 2), RECORD).unwrap();
        assert_eq!(songs.len(), 2);
        let mut exe = vec![0u8; DAN_PAIRS * 8];
        let put = |exe: &mut Vec<u8>, pair: usize, song: u32, chart: u32| {
            exe[pair * 8..pair * 8 + 4].copy_from_slice(&song.to_le_bytes());
            exe[pair * 8 + 4..pair * 8 + 8].copy_from_slice(&chart.to_le_bytes());
        };
        put(&mut exe, 0, 1, 3);
        put(&mut exe, 1, 0, 2);
        put(&mut exe, 17 * 4, 1, 4);
        put(&mut exe, 20 * 4 + 2, 1, 0);
        let courses = read_dan_courses(&exe, DanTable::Pairs(0), &songs);
        let ranks: Vec<_> = courses.iter().map(|c| (c.style, c.rank)).collect();
        assert_eq!(
            ranks,
            [
                (Style::Single, "7級"),
                (Style::Single, "皆伝"),
                (Style::Double, "5級")
            ]
        );
        assert_eq!(
            courses[0].charts,
            [(1, Difficulty::Beginner), (0, Difficulty::Another)]
        );
        assert_eq!(courses[2].charts, [(1, Difficulty::Normal)]);

        put(&mut exe, 5, 9, 0);
        assert!(
            read_dan_courses(&exe, DanTable::Pairs(0), &songs).is_empty(),
            "no song 9"
        );
        assert!(read_dan_courses(&exe[..100], DanTable::Pairs(0), &songs).is_empty());
    }

    #[test]
    fn a_chart_is_found_again_by_its_key() {
        let levels = [4, 7, 10, 0, 0, 0, 8, 0, 0, 0];
        let charts = [11, 12, 13, 0, 14, 0, 0, 0, 0, 0];
        let mut table = record("A", [0; 10], [0; 10]);
        table.extend(record("B", levels, charts));
        let disc = Disc {
            edition: Edition::Empress,
            songs: read_songs(&table, (0, RECORD.size * 2), RECORD).unwrap(),
            files: Vec::new(),
            dats: Vec::new(),
            data1: FileExtent { lba: 0, size: 0 },
            data1_files: Vec::new(),
            name_refs: Vec::new(),
            word_refs: Vec::new(),
            executable: FileExtent { lba: 0, size: 0 },
            dan_courses: Vec::new(),
            expert_courses: Vec::new(),
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
        assert_eq!(Edition::from_executable("SLPM_669.95"), Some(Edition::Gold));
        assert_eq!(
            Edition::from_executable("SLPM_551.17"),
            Some(Edition::DjTroopers)
        );
        assert_eq!(
            Edition::from_executable("SLPM_552.21"),
            Some(Edition::Empress)
        );
        assert_eq!(
            Edition::from_executable("slpm_552.22"),
            Some(Edition::PremiumBest)
        );
        assert_eq!(Edition::from_executable("SLPM_000.00"), None);
        assert_eq!(Edition::Gold.name(), "beatmania IIDX 14 GOLD");
    }

    #[test]
    fn a_packed_texture_gives_the_variant_asked_for() {
        let image = Rgba {
            width: 640,
            height: 1,
            pixels: (0..640).map(|x| [(x / 298) as u8, 0, 0, 255]).collect(),
        };
        let normal = select_packed_variant(image.clone(), false);
        let another = select_packed_variant(image, true);
        assert_eq!(normal.width, 298);
        assert!(normal.pixels.iter().all(|p| p[0] == 0));
        assert!(another.pixels.iter().all(|p| p[0] == 1));
    }
}

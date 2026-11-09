//! GuitarFreaks & DrumMania PS2 discs (Masterpiece Silver, `SLPM_664.84`).
//!
//! Every file is in `DATA/PACK/Dnnn/PACKnnnn.PAK`, found through the index
//! `DATA/PACK/PACKINFO.BIN`. Nothing is encrypted or compressed. One song is
//! one pack: two charts (`SEQP`, one DrumMania, one GuitarFreaks), two sample
//! files (`SBlk` banks, PS-ADPCM), a movie with the song's music (ATRAC3,
//! `movie_audio`) and the drum lanes' default notes (`lane_notes`). Which
//! sound a note plays is in the executable (`Kits`). The index
//! keys files by the CRC-32 of their path (`song_path`); the song table is
//! `music_info.bin`, and titles are banner pictures (`texp`).
//! Format notes: `docs/specs/gfdm_ps2_disc.md`.
//!
//! Only reading is here so far; nothing turns a song into BMS yet.

use std::io::{self, Read, Seek};

use crate::iso9660::{FileExtent, Volume};
use crate::sector::SectorReader;

/// The index's path on the disc.
pub const INDEX_PATH: &str = "DATA/PACK/PACKINFO.BIN";

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_string())
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// One file of the index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// MD5 of the file's bytes.
    pub md5: [u8; 16],
    /// Sort key of the index; most likely a hash of the file's name.
    pub key: u32,
    /// Which `PACKnnnn.PAK` holds it.
    pub pack: u16,
    /// The low half of the pack word; meaning unknown.
    pub tag: u16,
    /// Byte offset inside the pack.
    pub offset: u32,
    pub size: u32,
}

/// What a file is, by its first bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A chart file (`SEQP`).
    Seq,
    /// A sample bank (`03 00 00 00`, `SBlk` at 0x18).
    SoundBank,
    /// An MPEG program stream: a song's movie, its music in stream `0xBD`.
    Movie,
    /// The 14-byte `fre` file (seven `u16`, meaning not yet known).
    SongInfo,
    Other,
}

/// Tells a file's kind from its bytes (the whole file or at least its head).
pub fn kind(bytes: &[u8], size: u32) -> Kind {
    if bytes.starts_with(b"SEQP") {
        Kind::Seq
    } else if bytes.len() >= 0x1c
        && bytes.starts_with(&[3, 0, 0, 0])
        && &bytes[0x18..0x1c] == b"SBlk"
    {
        Kind::SoundBank
    } else if bytes.starts_with(&[0, 0, 1, 0xba]) {
        Kind::Movie
    } else if size == 14 {
        Kind::SongInfo
    } else {
        Kind::Other
    }
}

/// The parsed index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Index {
    pub entries: Vec<Entry>,
    /// One 16-byte value per pack after the entries, probably each pack's MD5.
    pub pack_hashes: Vec<[u8; 16]>,
}

impl Index {
    /// Reads `PACKINFO.BIN`: a 16-byte head (`u32` at 8 is where the
    /// entries end), 32-byte entries, then 16 bytes per pack.
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < 16 {
            return Err(bad("PACKINFO too short"));
        }
        let end = u32_at(bytes, 8) as usize;
        if end < 16
            || end > bytes.len()
            || !(end - 16).is_multiple_of(32)
            || !(bytes.len() - end).is_multiple_of(16)
        {
            return Err(bad("PACKINFO sizes do not add up"));
        }
        let entries = bytes[16..end]
            .chunks_exact(32)
            .map(|e| {
                let word = u32_at(e, 20);
                Entry {
                    md5: e[..16].try_into().unwrap(),
                    key: u32_at(e, 16),
                    pack: (word >> 16) as u16,
                    tag: word as u16,
                    offset: u32_at(e, 24),
                    size: u32_at(e, 28),
                }
            })
            .collect();
        let pack_hashes = bytes[end..]
            .chunks_exact(16)
            .map(|h| h.try_into().unwrap())
            .collect();
        Ok(Self {
            entries,
            pack_hashes,
        })
    }
}

/// A mounted disc: the index and where each pack is.
#[derive(Debug, Clone)]
pub struct Disc {
    pub index: Index,
    packs: Vec<Option<FileExtent>>,
}

/// CRC-32 (IEEE, as zlib): the index keys every file by this of its path.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in bytes {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// The song table's path.
pub const MUSIC_INFO_PATH: &str = "/data/product/music/music_info.bin";

/// One file of a song, by the name the game builds for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongFile {
    /// DrumMania chart (`SEQP`).
    DrumChart,
    /// GuitarFreaks chart (`SEQP`).
    GuitarChart,
    /// The movie, with the music in it.
    Movie,
    DrumSamples,
    GuitarSamples,
    /// The 14-byte file (`fre`), meaning not known.
    Fre,
    /// Preview sounds (ATRAC3), packs 74 and 75.
    GuitarPreview,
    DrumPreview,
}

/// The path the game opens for song `id`'s `file`.
pub fn song_path(id: u32, file: SongFile) -> String {
    let name = match file {
        SongFile::DrumChart => format!("d{id:04}.sq2"),
        SongFile::GuitarChart => format!("g{id:04}.sq2"),
        SongFile::Movie => format!("m{id:04}.pss"),
        SongFile::DrumSamples => format!("spu{id:04}d.va2"),
        SongFile::GuitarSamples => format!("spu{id:04}g.va2"),
        SongFile::Fre => format!("fre{id:04}.bin"),
        SongFile::GuitarPreview => format!("i{id:04}gf.at3"),
        SongFile::DrumPreview => format!("i{id:04}dm.at3"),
    };
    format!("/data/product/music/m{id:04}/{name}")
}

impl Index {
    /// The entry of the file at `path` (as the game spells it).
    pub fn find(&self, path: &str) -> Option<usize> {
        let key = crc32(path.as_bytes());
        self.entries.iter().position(|e| e.key == key)
    }
}

/// One record of `music_info.bin` (0x5c bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicInfo {
    /// The song number: its folder `m{id:04}` and banner `IMG_CO{id:04}`.
    pub id: u32,
    /// Upper case or romanised, cut to 15 characters (`FIRE IN THE DAR`).
    /// The real title is the banner picture.
    pub name: String,
    /// Shown BPM (`u16` at 6); some songs list half the chart's tempo.
    pub bpm: u16,
    /// GuitarFreaks guitar levels BASIC, ADVANCED, EXTREME (bytes 9–0xb;
    /// byte 8 is difficulty 0, which ten songs have). 0: no such chart.
    pub gf_levels: [u8; 3],
    /// DrumMania levels (bytes 0x31–0x33, 0x30 difficulty 0).
    pub dm_levels: [u8; 3],
    /// GuitarFreaks bass levels (bytes 0x39–0x3b).
    pub bass_levels: [u8; 3],
    /// GuitarFreaks open levels, the part 3 tracks (bytes 0x35–0x37).
    pub open_levels: [u8; 3],
    /// Song length in 1/300 s (`u32` at 0x2c), close to the charts' end.
    pub length: u32,
}

impl MusicInfo {
    pub const SIZE: usize = 0x5c;

    /// Reads the whole table.
    pub fn parse_all(bytes: &[u8]) -> Vec<Self> {
        bytes
            .chunks_exact(Self::SIZE)
            .map(|r| {
                let name = &r[0x4c..];
                let name = &name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())];
                Self {
                    id: u32_at(r, 0),
                    name: name.iter().map(|&b| b as char).collect(),
                    bpm: u16::from_le_bytes([r[6], r[7]]),
                    gf_levels: [r[9], r[0xa], r[0xb]],
                    dm_levels: [r[0x31], r[0x32], r[0x33]],
                    bass_levels: [r[0x39], r[0x3a], r[0x3b]],
                    open_levels: [r[0x35], r[0x36], r[0x37]],
                    length: u32_at(r, 0x2c),
                }
            })
            .collect()
    }
}

/// One song: its table record and where its files are in the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Song {
    pub info: MusicInfo,
    pub drum_chart: Option<usize>,
    pub guitar_chart: Option<usize>,
    pub movie: Option<usize>,
    pub drum_samples: Option<usize>,
    pub guitar_samples: Option<usize>,
}

impl Disc {
    /// Reads the index and finds every pack it names. `None` when the disc
    /// has no `PACKINFO.BIN`.
    pub fn open<R: Read + Seek>(
        disc: &mut SectorReader<R>,
        volume: &Volume,
    ) -> io::Result<Option<Self>> {
        let Some(bytes) = volume.read_file(disc, INDEX_PATH)? else {
            return Ok(None);
        };
        let index = Index::parse(&bytes)?;
        let count = index
            .entries
            .iter()
            .map(|e| e.pack as usize + 1)
            .max()
            .unwrap_or(0);
        let mut packs = vec![None; count];
        for (n, slot) in packs.iter_mut().enumerate() {
            let path = format!("DATA/PACK/D{:03}/PACK{n:04}.PAK", n / 30);
            *slot = volume.find(disc, &path)?;
        }
        Ok(Some(Self { index, packs }))
    }

    /// Where a pack is on the disc.
    pub fn pack(&self, n: u16) -> Option<FileExtent> {
        self.packs.get(n as usize).copied().flatten()
    }

    /// The bytes of index entry `n`.
    pub fn read<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        n: usize,
    ) -> io::Result<Vec<u8>> {
        let e = self
            .index
            .entries
            .get(n)
            .ok_or_else(|| bad("no such entry"))?;
        let pack = self.pack(e.pack).ok_or_else(|| bad("pack not on disc"))?;
        if e.offset as u64 + e.size as u64 > pack.size {
            return Err(bad("entry runs past its pack"));
        }
        disc.read_at(pack.offset() + e.offset as u64, e.size as usize)
    }

    /// The bytes of the file at `path`, or `None` when the index has none.
    pub fn read_path<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        path: &str,
    ) -> io::Result<Option<Vec<u8>>> {
        match self.index.find(path) {
            Some(n) => self.read(disc, n).map(Some),
            None => Ok(None),
        }
    }

    /// The songs of `music_info.bin`, in its order, with their files.
    pub fn songs<R: Read + Seek>(&self, disc: &mut SectorReader<R>) -> io::Result<Vec<Song>> {
        let table = self
            .read_path(disc, MUSIC_INFO_PATH)?
            .ok_or_else(|| bad("no music_info.bin"))?;
        let find = |id, file| self.index.find(&song_path(id, file));
        Ok(MusicInfo::parse_all(&table)
            .into_iter()
            .map(|info| Song {
                drum_chart: find(info.id, SongFile::DrumChart),
                guitar_chart: find(info.id, SongFile::GuitarChart),
                movie: find(info.id, SongFile::Movie),
                drum_samples: find(info.id, SongFile::DrumSamples),
                guitar_samples: find(info.id, SongFile::GuitarSamples),
                info,
            })
            .collect())
    }

    /// Every `TEXP` texture file of the disc (reads each file's head).
    pub fn texture_files<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
    ) -> io::Result<Vec<usize>> {
        let mut out = Vec::new();
        for (n, e) in self.index.entries.iter().enumerate() {
            let Some(pack) = self.pack(e.pack) else {
                continue;
            };
            if e.size >= 4 && disc.read_at(pack.offset() + e.offset as u64, 4)? == b"PXET" {
                out.push(n);
            }
        }
        Ok(out)
    }
}

/// What a song's charts need: its record, both charts and the drum lanes'
/// default notes (`lane_notes`).
#[derive(Debug, Clone)]
pub struct SongCharts {
    pub info: MusicInfo,
    pub drums: Seq,
    pub guitar: Seq,
    pub lanes: Vec<u16>,
}

impl Disc {
    /// Reads song `info`'s charts and lane notes.
    pub fn song_charts<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        info: &MusicInfo,
    ) -> io::Result<SongCharts> {
        let mut seq = |file| -> io::Result<Seq> {
            let bytes = self
                .read_path(disc, &song_path(info.id, file))?
                .ok_or_else(|| bad("chart missing"))?;
            Seq::parse(&bytes)
        };
        let drums = seq(SongFile::DrumChart)?;
        let guitar = seq(SongFile::GuitarChart)?;
        let fre = self
            .read_path(disc, &song_path(info.id, SongFile::Fre))?
            .unwrap_or_default();
        Ok(SongCharts {
            info: info.clone(),
            drums,
            guitar,
            lanes: lane_notes(&fre),
        })
    }
}

/// The key sound tables from the disc's executable (`Kits`), `None` when
/// the disc has another.
pub fn read_kits<R: Read + Seek>(
    disc: &mut SectorReader<R>,
    volume: &Volume,
) -> io::Result<Option<Kits>> {
    Ok(volume
        .read_file(disc, EXE_PATH)?
        .and_then(|exe| Kits::parse(&exe)))
}

/// Which game a chart track is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Game {
    DrumMania,
    GuitarFreaks,
}

/// One event of a track (16 bytes on disc).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    /// Absolute time in [`TICKS_PER_SECOND`].
    pub time: u32,
    /// For notes the lane (DM 0–5) or the fret mask (GF: 1 R, 2 G, 4 B,
    /// 0x20 probably wailing).
    pub value: u8,
    /// 0 note, 0x10 tempo, 0x20 meter, 0x50 bar line, 0x60 beat line,
    /// 0x70/0x80 play start and end, 0xE0/0xF0 track start and end.
    pub kind: u8,
    /// For notes the sample number in the bank, for tempo µs per beat.
    pub a: u32,
    /// For notes the volume (0x7F), for meter `0x204` (4/4).
    pub b: u32,
}

/// Event times count this many ticks a second.
pub const TICKS_PER_SECOND: u32 = 300;

pub const EVENT_NOTE: u8 = 0x00;
pub const EVENT_TEMPO: u8 = 0x10;
pub const EVENT_END: u8 = 0xf0;

/// One `SEQT` track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub game: Game,
    /// The tempo and bar track, not a playable chart.
    pub is_meta: bool,
    /// 1 BASIC, 2 ADVANCED, 3 EXTREME (so far as seen).
    pub difficulty: u8,
    /// GF: 1 guitar, 2 bass, 3 open (guitar with open picks). DM: 0.
    pub part: u8,
    pub events: Vec<Event>,
}

/// A chart file: a meta track then one track per difficulty and part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seq {
    pub tracks: Vec<Track>,
}

impl Seq {
    /// Reads a `SEQP` file. Head 0x30 bytes (`u32` at 0x18 is the track
    /// count), then each `SEQT`: 0x20 head (`u32` at 0x10 the event count,
    /// bytes 0x14–0x17 game, meta, difficulty, part), its events, and (but
    /// for the last track) 16 bytes not yet known.
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < 0x30 || !bytes.starts_with(b"SEQP") {
            return Err(bad("not a SEQP file"));
        }
        let count = u32_at(bytes, 0x18) as usize;
        let mut at = 0x30;
        let mut tracks = Vec::with_capacity(count);
        for _ in 0..count {
            if bytes.len() < at + 0x20 || &bytes[at..at + 4] != b"SEQT" {
                return Err(bad("SEQT track missing"));
            }
            let events = u32_at(bytes, at + 0x10) as usize;
            let h = &bytes[at + 0x14..at + 0x18];
            let body = at + 0x20;
            let end = body + events * 16;
            // Every track but the last is followed by 16 more bytes.
            if end > bytes.len() {
                return Err(bad("SEQT track runs past the file"));
            }
            let events = bytes[body..end]
                .chunks_exact(16)
                .map(|e| Event {
                    time: u32_at(e, 0),
                    value: e[4],
                    kind: e[5],
                    a: u32_at(e, 8),
                    b: u32_at(e, 12),
                })
                .collect();
            tracks.push(Track {
                game: if h[0] == 1 {
                    Game::GuitarFreaks
                } else {
                    Game::DrumMania
                },
                is_meta: h[1] == 1,
                difficulty: h[2],
                part: h[3],
                events,
            });
            at = end + 16;
        }
        Ok(Self { tracks })
    }
}

/// The music streams of a movie (`m{id}.pss`, an MPEG program stream):
/// private stream 1 (`0xBD`) packets whose payload starts `ff 90`, a
/// big-endian stream number, then ATRAC3 frames of `MOVIE_AUDIO_BLOCK`
/// bytes (`atrac3`). Songs have several streams of the same music in other
/// mixes (`docs/specs/gfdm_ps2_disc.md`). By stream number, in order.
pub fn movie_audio(pss: &[u8]) -> Vec<(u16, Vec<u8>)> {
    let mut streams: Vec<(u16, Vec<u8>)> = Vec::new();
    let mut at = 0;
    while at + 6 <= pss.len() {
        if pss[at..at + 3] != [0, 0, 1] {
            // Not on a start code: find the next one.
            match pss[at + 1..].windows(3).position(|w| w == [0, 0, 1]) {
                Some(p) => at += 1 + p,
                None => break,
            }
            continue;
        }
        let id = pss[at + 3];
        match id {
            0xb9 => break,
            0xba => {
                // MPEG-2 pack head: 14 bytes and its stuffing; MPEG-1: 12.
                at += if pss[at + 4] >> 6 == 1 {
                    14 + (pss.get(at + 13).copied().unwrap_or(0) & 7) as usize
                } else {
                    12
                };
                continue;
            }
            _ => {}
        }
        let len = u16::from_be_bytes([pss[at + 4], pss[at + 5]]) as usize;
        let body = &pss[at + 6..(at + 6 + len).min(pss.len())];
        if id == 0xbd && body.len() >= 3 {
            let payload = &body[(3 + body[2] as usize).min(body.len())..];
            if payload.len() >= 4 && payload[..2] == [0xff, 0x90] {
                let n = u16::from_be_bytes([payload[2], payload[3]]);
                let slot = match streams.iter().position(|s| s.0 == n) {
                    Some(i) => i,
                    None => {
                        streams.push((n, Vec::new()));
                        streams.len() - 1
                    }
                };
                streams[slot].1.extend_from_slice(&payload[4..]);
            }
        }
        at += 6 + len;
    }
    streams.sort_by_key(|s| s.0);
    streams
}

/// Bytes of one ATRAC3 frame (two channels of 192) in the movies and previews.
pub const MOVIE_AUDIO_BLOCK: usize = 384;
pub const AUDIO_RATE: u32 = 44100;
/// Rate of the movie music and previews (ATRAC3). Played at 44100 Hz they
/// run 48000/44100 slower than the charts (measured tempo, 2026-11-10).
pub const MUSIC_RATE: u32 = 48000;
/// Frames of decoded music before chart time 0: the parts in the music land
/// this late against the charts rendered with their key sounds (about 33 ms,
/// steady through each song; songs 404, 734, 920, 1040, 2026-11-10).
pub const MUSIC_LEAD: usize = 1600;

/// One sound of a sample bank (`SBlk`, Sony's SCREAM sound bank).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    /// 0..=127.
    pub volume: u8,
    /// Degrees, 0 centre (as stored; 0..360).
    pub pan: u16,
    /// The note the sound plays at its own rate (`0xc2` 44100 Hz, each
    /// step a semitone).
    pub note: u8,
    /// Where its PS-ADPCM data starts in the file.
    pub offset: usize,
}

impl Sample {
    /// The rate the sound plays at, in Hz.
    pub fn rate(&self) -> u32 {
        (AUDIO_RATE as f64 * 2f64.powf((self.note as f64 - 0xc2 as f64) / 12.0)).round() as u32
    }

    /// The sound as mono PCM at `rate()`.
    pub fn decode(&self, file: &[u8]) -> Vec<i16> {
        file.get(self.offset..)
            .map(|d| crate::adpcm::decode(d, None, true))
            .unwrap_or_default()
    }
}

/// Where each bank of a sample file (`spu{id}d.va2`, `spu{id}g.va2`)
/// starts: a 24-byte head `(3, 2, 0x18, SBlk size, data offset, data size)`,
/// the `SBlk`, the sound data; the next bank at the next 16 bytes.
pub fn sample_banks(file: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 24 <= file.len() && u32_at(file, at) == 3 && u32_at(file, at + 4) == 2 {
        out.push(at);
        let end = at as u64 + u32_at(file, at + 16) as u64 + u32_at(file, at + 20) as u64;
        at = ((end + 15) & !15) as usize;
    }
    out
}

/// The sounds of the bank at `at`, by index. `SBlk` head: `u16` sound
/// count at 0x16, then at 0x1c the sound table (12 bytes each, `u32` at 8
/// the program byte offset), at 0x20 the programs (8 bytes, `u16` tone
/// offset first), at 0x34 the tones (24 bytes: volume 1, note 2, pan `u16`
/// 4, sound data offset `u32` 0x10).
pub fn bank_samples(file: &[u8], at: usize) -> Option<Vec<Sample>> {
    let sblk = at + u32_at(file, at + 8) as usize;
    let data = at + u32_at(file, at + 16) as usize;
    if file.get(sblk..sblk + 4)? != b"SBlk" {
        return None;
    }
    let u16_at = |p: usize| Some(u16::from_le_bytes(file.get(p..p + 2)?.try_into().ok()?));
    let u32_le = |p: usize| Some(u32::from_le_bytes(file.get(p..p + 4)?.try_into().ok()?));
    let count = u16_at(sblk + 0x16)? as usize;
    let sounds = sblk + u32_le(sblk + 0x1c)? as usize;
    let programs = sblk + u32_le(sblk + 0x20)? as usize;
    let tones = sblk + u32_le(sblk + 0x34)? as usize;
    (0..count)
        .map(|k| {
            let program = programs + u32_le(sounds + k * 12 + 8)? as usize;
            let tone = tones + u16_at(program)? as usize;
            let t = file.get(tone..tone + 24)?;
            Some(Sample {
                volume: t[1],
                pan: u16::from_le_bytes([t[4], t[5]]),
                note: t[2],
                offset: data + u32_at(t, 0x10) as usize,
            })
        })
        .collect()
}

/// The default note of each DrumMania lane (`fre{id}.bin`: `u16` per lane,
/// by `Event::value`). A drum note whose own note is not in the song's kit
/// (small values 0..14 in six songs, 860 notes) plays its lane's default;
/// 64% of all drum notes are their lane's default anyway.
pub fn lane_notes(fre: &[u8]) -> Vec<u16> {
    fre.chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect()
}

/// The boot executable, which holds the key sound tables (`Kits`).
pub const EXE_PATH: &str = "SLPM_664.84";

/// Which sound a chart note plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeySound {
    /// The bank's offset in the song's sample file (`sample_banks`).
    pub bank: usize,
    /// The sound in that bank (`bank_samples`).
    pub sound: u16,
}

/// The key sound tables of the executable: for each song, which bank and
/// sound each chart note (`Event::a & 0xffff`) plays.
#[derive(Debug, Clone, Default)]
pub struct Kits {
    songs: Vec<SongKits>,
}

/// One song's notes and their sounds.
#[derive(Debug, Clone, Default)]
struct SongKits {
    id: u32,
    /// DrumMania notes (all in the one bank).
    drum: Vec<(u16, KeySound)>,
    guitar: Vec<(u16, KeySound)>,
}

/// Executable layout (Masterpiece Silver): the sorted song ids, the
/// DrumMania and GuitarFreaks kit pointers in that order, and the
/// pointers to each song's GuitarFreaks bank table.
const EXE_SONG_IDS: usize = 0x193c70;
const EXE_DRUM_KITS: usize = 0x1940b0;
const EXE_GUITAR_KITS: usize = 0x1941e0;
const EXE_GUITAR_BANKS: u32 = 0x28bde0;
const EXE_SONGS: usize = 74;
/// A GuitarFreaks kit is pieces of this many bytes, one per bank.
const GUITAR_PIECE: usize = 0xa0;
/// Ends a kit (piece); a piece starting with `KIT_LAST` ends the GF kit.
const KIT_END: u16 = 0xff00;
const KIT_LAST: u16 = 0xffff;

impl Kits {
    /// Reads the tables, `None` when `exe` is not the executable they are in.
    ///
    /// A DrumMania kit is `(u16 note, u16 sound)` pairs to `KIT_END`, all in
    /// the one bank. A GuitarFreaks kit is pieces of `GUITAR_PIECE` bytes,
    /// each the same pairs for the notes `hi << 8 | n` of one high byte
    /// `hi`. The bank of `hi` is entry `hi` of the song's bank table (8
    /// bytes: offset in the sample file, size). The banks are not in `hi`
    /// order in eleven songs, so the table is needed.
    pub fn parse(exe: &[u8]) -> Option<Self> {
        if exe.get(..4)? != b"\x7fELF" {
            return None;
        }
        // The first program header: file offset and address of the image.
        let ph = u32_at(exe, 0x1c) as usize;
        let (file_at, vaddr) = (u32_at(exe, ph + 4), u32_at(exe, ph + 8));
        let at = |v: u32| Some(v.checked_sub(vaddr)? as usize + file_at as usize);
        let u16_at = |p: usize| Some(u16::from_le_bytes(exe.get(p..p + 2)?.try_into().ok()?));
        let pairs = |p: usize, max: usize| -> Option<Vec<(u16, u16)>> {
            let mut out = Vec::new();
            for k in 0..max {
                let note = u16_at(p + k * 4)?;
                if note == KIT_END {
                    return Some(out);
                }
                out.push((note, u16_at(p + k * 4 + 2)?));
            }
            Some(out)
        };
        let mut songs = Vec::with_capacity(EXE_SONGS);
        let bank_tables: Vec<u32> = (0..=EXE_SONGS)
            .map(|k| u32_at(exe, at(EXE_GUITAR_BANKS).unwrap_or(0) + k * 4))
            .collect();
        for k in 0..EXE_SONGS {
            let id = u32_at(exe, EXE_SONG_IDS + k * 4);
            let drum = pairs(at(u32_at(exe, EXE_DRUM_KITS + k * 4))?, 256)?
                .into_iter()
                .map(|(n, s)| (n, KeySound { bank: 0, sound: s }))
                .collect();
            let banks = at(bank_tables[k])?;
            // Entries past the next song's table are not this song's.
            let entries = if bank_tables[k + 1] > bank_tables[k] {
                ((bank_tables[k + 1] - bank_tables[k]) / 8).min(256) as u16
            } else {
                256
            };
            let mut guitar = Vec::new();
            let mut piece = at(u32_at(exe, EXE_GUITAR_KITS + k * 4))?;
            loop {
                if u16_at(piece)? == KIT_LAST {
                    break;
                }
                let notes = pairs(piece, GUITAR_PIECE / 4)?;
                if notes.is_empty() {
                    break;
                }
                for (n, s) in notes {
                    if n >> 8 < entries {
                        let bank = u32_at(exe, banks + (n >> 8) as usize * 8) as usize;
                        guitar.push((n, KeySound { bank, sound: s }));
                    }
                }
                piece += GUITAR_PIECE;
            }
            songs.push(SongKits { id, drum, guitar });
        }
        Some(Self { songs })
    }

    fn song(&self, id: u32) -> Option<&SongKits> {
        self.songs.iter().find(|s| s.id == id)
    }

    /// The sound of note `key` in song `id`'s chart for `game`.
    pub fn key_sound(&self, id: u32, game: Game, key: u16) -> Option<KeySound> {
        let s = self.song(id)?;
        let notes = match game {
            Game::DrumMania => &s.drum,
            Game::GuitarFreaks => &s.guitar,
        };
        notes.iter().find(|n| n.0 == key).map(|n| n.1)
    }

    /// The sound of chart event `e`: its own note, or for a drum note not
    /// in the kit, its lane's default (`lane_notes`).
    pub fn event_sound(&self, id: u32, game: Game, e: &Event, lanes: &[u16]) -> Option<KeySound> {
        self.key_sound(id, game, e.a as u16).or_else(|| match game {
            Game::DrumMania => self.key_sound(id, game, *lanes.get(e.value as usize)?),
            Game::GuitarFreaks => None,
        })
    }

    /// The song ids the tables cover.
    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.songs.iter().map(|s| s.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movie_audio_joins_each_stream() {
        let mut pss = vec![0, 0, 1, 0xba, 0x44, 0, 4, 0, 4, 1, 0, 0x44, 0x23, 0xf8];
        for (n, data) in [(1u8, [1u8, 2]), (2, [3, 4]), (1, [5, 6])] {
            let payload = [0xff, 0x90, 0, n, data[0], data[1]];
            let body = [&[0x81, 0x80, 0][..], &payload].concat();
            pss.extend_from_slice(&[0, 0, 1, 0xbd, 0, body.len() as u8]);
            pss.extend_from_slice(&body);
        }
        pss.extend_from_slice(&[0, 0, 1, 0xb9]);
        assert_eq!(
            movie_audio(&pss),
            vec![(1, vec![1, 2, 5, 6]), (2, vec![3, 4])]
        );
    }

    #[test]
    fn a_sample_plays_at_44100_hz_at_its_note() {
        let s = |note| Sample {
            volume: 127,
            pan: 0,
            note,
            offset: 0,
        };
        assert_eq!(s(0xc2).rate(), 44100);
        assert_eq!(s(0xb6).rate(), 22050);
    }

    fn event(out: &mut Vec<u8>, time: u32, value: u8, kind: u8, a: u32, b: u32) {
        out.extend(time.to_le_bytes());
        out.extend([value, kind, 0, 0]);
        out.extend(a.to_le_bytes());
        out.extend(b.to_le_bytes());
    }

    fn track(out: &mut Vec<u8>, head: [u8; 4], events: &[(u32, u8, u8, u32, u32)]) {
        out.extend(b"SEQT\0\0\x02\0\0\0\x01\0\x20\0\0\0");
        out.extend((events.len() as u32).to_le_bytes());
        out.extend(head);
        out.extend([0; 8]);
        for &(t, v, k, a, b) in events {
            event(out, t, v, k, a, b);
        }
        out.extend([0; 16]);
    }

    #[test]
    fn parses_a_meta_and_a_drum_track() {
        let mut b = b"SEQP\0\0\x02\0\0\0\x01\0".to_vec();
        b.extend([0; 12]);
        b.extend(2u32.to_le_bytes());
        b.resize(0x30, 0);
        track(
            &mut b,
            [0, 1, 1, 0],
            &[
                (0, 0, 0xe0, 0, 0),
                (0, 0, 0x10, 0x605f0, 0),
                (90, 0, 0xf0, 0, 0),
            ],
        );
        track(
            &mut b,
            [0, 0, 3, 0],
            &[(474, 2, 0, 0x18, 0x7f), (90, 0, 0xf0, 0, 0)],
        );
        let seq = Seq::parse(&b).unwrap();
        assert_eq!(seq.tracks.len(), 2);
        assert!(seq.tracks[0].is_meta);
        assert_eq!(seq.tracks[0].events[1].a, 0x605f0);
        let dm = &seq.tracks[1];
        assert_eq!(
            (dm.game, dm.difficulty, dm.is_meta),
            (Game::DrumMania, 3, false)
        );
        assert_eq!(
            dm.events[0],
            Event {
                time: 474,
                value: 2,
                kind: 0,
                a: 0x18,
                b: 0x7f
            }
        );
    }

    #[test]
    fn crc32_matches_the_index_key_of_a_chart() {
        assert_eq!(
            crc32(song_path(404, SongFile::DrumChart).as_bytes()),
            0xd1f8_d571
        );
    }

    #[test]
    fn index_splits_the_pack_word() {
        let mut b = vec![0u8; 16];
        b[8..12].copy_from_slice(&48u32.to_le_bytes());
        b.extend([7; 16]);
        for v in [1u32, (35 << 16) | 0xbf27, 0x11a00, 14] {
            b.extend(v.to_le_bytes());
        }
        b.extend([9; 32]);
        let index = Index::parse(&b).unwrap();
        let e = index.entries[0];
        assert_eq!((e.pack, e.tag, e.offset, e.size), (35, 0xbf27, 0x11a00, 14));
        assert_eq!(index.pack_hashes.len(), 2);
    }
}

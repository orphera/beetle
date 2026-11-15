//! GuitarFreaks 4thMIX & DrumMania 3rdMIX (`SLPM_650.52`).
//!
//! Three files in the root:
//! - `GF4DM3.DAT`: the game data, 44 files one after another, each a 0x30-byte
//!   head XORed with a key in the executable (`DAT_KEY`), then the file
//!   (16-byte aligned). The songs' files are `music_info/music_info5.bin`
//!   (the song table), `seq/seqcode.dat` (a hash of each chart's name) and
//!   `seq/seqdata.dat` (the charts, packed: `unpack`).
//! - `GF4DM3.SPK`: the sounds, a `PAK` of `.sdd` files. Only its first
//!   0x4000 bytes (head and file table) are scrambled (`unscramble`).
//!   Each song has six: `g1`/`g2` (guitar and bass mode: the guitar sound
//!   bank and the music without the played part), `d1` (drum mode: the drum
//!   bank and the music without the drums), `d2`/`d3` (sessions) and `pr`
//!   (preview). An `.sdd` is `SDDT`: a stream head (`VSHD`), sound bank heads
//!   (`VPHD`, Sony VAB-like), the banks' PS-ADPCM bodies and the music (stereo
//!   PS-ADPCM, 0x1000 bytes a channel in turn, 44100 Hz).
//! - `GF4DM3.BG`: backgrounds (not read).
//!
//! Which `.sdd` a song plays is song `k` of the executable's table
//! (`SONG_IDS`) and file `k * 6 + part` of the sound driver's name table
//! (`DRIVER/SDDRV.IRX`); songs without music on this disc point at another
//! song's files and are left out.
//!
//! Charts are 8-byte events at 1/75 s. Drums: `u32` time, `u8` lane (0–5
//! HH SD BD HT LT CY, 6 a sound without a lane, 7 bar line, 8 beat line),
//! `u8` volume, `u16` key (the bank's tone for that key). Guitar: `u16` time,
//! `u16` tone, `u16` length (1/75 s), `u16` frets (1 R, 2 G, 4 B, 0x20
//! probably wailing; `0 0x10` bar line). They come out as `gfdm::Seq`
//! (1/300 s, a meta track of bar lines and tempos), so `gfdm_bms` makes their
//! BMS. Format notes: `docs/specs/gf4dm3_ps2_disc.md`.

use std::io::{self, Read, Seek};

use crate::gfdm::{Event, Game, MusicInfo, Seq, SongCharts, Track};
use crate::iso9660::{FileExtent, Volume};
use crate::sector::SectorReader;

/// The boot executable.
pub const EXE_PATH: &str = "SLPM_650.52";
const DAT_PATH: &str = "GF4DM3.DAT";
const SPK_PATH: &str = "GF4DM3.SPK";
const DRIVER_PATH: &str = "DRIVER/SDDRV.IRX";

/// Executable addresses: the DAT head key (48 bytes) and the song ids
/// (`SONGS` `u32`), whose position is a song's sound number / 6.
const DAT_KEY: u32 = 0x1fd450;
const SONG_IDS: u32 = 0x1fd480;
const SONGS: usize = 49;
/// The sound driver's table of (`.sdd` name pointer, `0x1880`), 8 bytes
/// each, at this offset in its loaded segment.
const DRIVER_TABLE: usize = 64960;
const DRIVER_TAG: u32 = 0x1880;

/// Chart times count this many ticks a second; `gfdm` counts 300.
pub const TICKS_PER_SECOND: u32 = 75;
const TO_GFDM: u32 = crate::gfdm::TICKS_PER_SECOND / TICKS_PER_SECOND;

/// The music's sample rate.
pub const MUSIC_RATE: u32 = 44100;
/// Frames of music dropped before chart time 0: the key sounds sound early
/// by about 40 ms against it (by ear, 2026-11-15; the drums measured against
/// the music agree to a few ms, so it is likely the game's output delay).
pub const MUSIC_LEAD: usize = 1764;
/// Bytes of one channel's turn in the music.
const MUSIC_INTERLEAVE: usize = 0x1000;

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_string())
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Undoes the scramble of the SPK's first bytes: each byte is XORed with
/// the byte before it (as stored) plus its offset; the first is plain.
pub fn unscramble(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    for (n, &c) in raw.iter().enumerate() {
        out.push(match n {
            0 => c,
            _ => c ^ raw[n - 1].wrapping_add(n as u8),
        });
    }
    out
}

/// One file of the SPK: name, offset in the SPK, size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpkFile {
    pub name: String,
    pub offset: u64,
    pub size: u64,
}

/// The SPK's file table, from its unscrambled first 0x4000 bytes:
/// `u32` at 8 where the files start, `u32` at 0xc how many, then 16-byte
/// entries from 0x40 (`u32` size, `u8` name length, the name). The files
/// follow one another.
pub fn spk_files(head: &[u8]) -> Option<Vec<SpkFile>> {
    let mut at = u32_at(head, 8)? as u64;
    let count = u32_at(head, 0xc)? as usize;
    let mut out = Vec::with_capacity(count);
    for k in 0..count {
        let e = 0x40 + k * 16;
        let size = u32_at(head, e)? as u64;
        let len = *head.get(e + 4)? as usize;
        let name = head.get(e + 5..e + 5 + len)?;
        out.push(SpkFile {
            name: name.iter().map(|&b| b as char).collect(),
            offset: at,
            size,
        });
        at += size;
    }
    Some(out)
}

/// The hash the game finds a chart by (`seq/seqcode.dat`): 28 bits, each
/// byte added after turning the hash 5 bits left; never 0.
pub fn name_hash(name: &str) -> u32 {
    let mut h = 0u32;
    for b in name.bytes() {
        h = (((h << 5) | (h >> 23)).wrapping_add(b as u32)) & 0x0fff_ffff;
    }
    h.max(1)
}

/// Unpacks a packed chart: a canonical Huffman code over bytes (`huffman`),
/// whose output is Konami's LZ (`lz`).
pub fn unpack(packed: &[u8]) -> Option<Vec<u8>> {
    crate::lz::decode(&huffman(packed)?)
}

/// MSB-first bits.
struct Bits<'a> {
    bytes: &'a [u8],
    at: usize,
    left: u32,
    byte: u8,
}

impl Bits<'_> {
    fn bit(&mut self) -> Option<bool> {
        if self.left == 0 {
            self.byte = *self.bytes.get(self.at)?;
            self.at += 1;
            self.left = 8;
        }
        self.left -= 1;
        Some(self.byte >> self.left & 1 != 0)
    }

    /// Elias gamma: zeros, a one, then as many bits more.
    fn gamma(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while !self.bit()? {
            zeros += 1;
            if zeros > 16 {
                return None;
            }
        }
        let mut v = 1u32;
        for _ in 0..zeros {
            v = v << 1 | self.bit()? as u32;
        }
        Some(v)
    }
}

/// The first stage of a packed chart: `u32` output length, then bits. The
/// code lengths of the 256 bytes come first, each a flag (1: changed, then
/// gamma XOR the last length; 0: the same as the last); codes are canonical
/// (shorter first, then by byte), at most 16 bits.
fn huffman(packed: &[u8]) -> Option<Vec<u8>> {
    let count = u32_at(packed, 0)? as usize;
    let mut bits = Bits {
        bytes: packed.get(4..)?,
        at: 0,
        left: 0,
        byte: 0,
    };
    let mut lens = [0u32; 256];
    let mut last = 0;
    for len in lens.iter_mut() {
        if bits.bit()? {
            last ^= bits.gamma()?;
        }
        if last > 16 {
            return None;
        }
        *len = last;
    }
    let mut counts = [0u32; 18];
    for &l in &lens {
        counts[l as usize] += 1;
    }
    let mut next = [0u32; 18];
    for l in 1..17 {
        next[l + 1] = (next[l] + counts[l]) << 1;
    }
    // A tree: node n's children at `tree[n]`; a value under 256 is a byte.
    const ROOT: usize = 256;
    let mut tree: Vec<[u32; 2]> = vec![[u32::MAX; 2]; ROOT + 1];
    for len in 1..=16u32 {
        for (byte, &l) in lens.iter().enumerate() {
            if l != len {
                continue;
            }
            let code = next[len as usize];
            next[len as usize] += 1;
            let mut node = ROOT;
            for i in (0..len).rev() {
                let b = (code >> i & 1) as usize;
                if i == 0 {
                    tree[node][b] = byte as u32;
                } else {
                    if tree[node][b] == u32::MAX {
                        tree.push([u32::MAX; 2]);
                        tree[node][b] = (tree.len() - 1) as u32;
                    }
                    node = tree[node][b] as usize;
                    if node < ROOT {
                        return None;
                    }
                }
            }
        }
    }
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let mut node = ROOT;
        loop {
            let next = tree[node][bits.bit()? as usize];
            if next == u32::MAX {
                return None;
            }
            node = next as usize;
            if node < ROOT {
                break;
            }
        }
        out.push(node as u8);
    }
    Some(out)
}

/// One song of the disc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Song {
    pub info: MusicInfo,
    /// The three letters of its `.sdd` files (`g1_<code>.sdd`).
    pub code: String,
}

/// Which `.sdd` of a song.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundFile {
    /// Guitar mode: guitar bank, music without the guitar.
    Guitar,
    /// Bass mode: guitar bank, music without the bass.
    Bass,
    /// Drum mode: drum bank, music without the drums.
    Drums,
}

impl SoundFile {
    fn prefix(self) -> &'static str {
        match self {
            SoundFile::Guitar => "g1",
            SoundFile::Bass => "g2",
            SoundFile::Drums => "d1",
        }
    }
}

/// A mounted disc.
#[derive(Debug, Clone)]
pub struct Disc {
    pub songs: Vec<Song>,
    spk: FileExtent,
    spk_files: Vec<SpkFile>,
    seq_codes: Vec<u32>,
    seq_data: Vec<u8>,
}

/// Reads executable address `addr` through the ELF's first program header.
fn exe_at(exe: &[u8], addr: u32, len: usize) -> Option<&[u8]> {
    if exe.get(..4)? != b"\x7fELF" {
        return None;
    }
    let ph = u32_at(exe, 0x1c)? as usize;
    let (file_at, vaddr) = (u32_at(exe, ph + 4)?, u32_at(exe, ph + 8)?);
    let at = (addr.checked_sub(vaddr)? + file_at) as usize;
    exe.get(at..at + len)
}

/// The DAT's files: (name, offset, size).
fn dat_files<R: Read + Seek>(
    disc: &mut SectorReader<R>,
    dat: FileExtent,
    key: &[u8],
) -> io::Result<Vec<(String, u64, usize)>> {
    let head = disc.read_at(dat.offset(), 16)?;
    let count = u32_at(&head, 0).unwrap_or(0) as usize;
    let mut at = 16u64;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if at + 0x30 > dat.size {
            return Err(bad("DAT runs short"));
        }
        let raw = disc.read_at(dat.offset() + at, 0x30)?;
        let h: Vec<u8> = raw.iter().zip(key).map(|(a, b)| a ^ b).collect();
        let name_end = h[..0x2b].iter().position(|&b| b == 0).unwrap_or(0x2b);
        let name: String = h[..name_end].iter().map(|&b| b as char).collect();
        let size = u32_at(&h, 0x2c).unwrap_or(0) as usize;
        out.push((name, at + 0x30, size));
        at += 0x30 + size.next_multiple_of(16) as u64;
    }
    Ok(out)
}

/// The sound driver's `.sdd` name for each sound number.
fn driver_names(irx: &[u8]) -> Option<Vec<String>> {
    // The second program header loads the image.
    let ph = u32_at(irx, 0x1c)? as usize;
    let seg = u32_at(irx, ph + 32 + 4)? as usize;
    let image = irx.get(seg..)?;
    (0..SONGS * 6)
        .map(|k| {
            let e = DRIVER_TABLE + k * 8;
            if u32_at(image, e + 4)? != DRIVER_TAG {
                return None;
            }
            let at = u32_at(image, e)? as usize;
            let name = image.get(at..at + 10)?;
            name.ends_with(b".sdd")
                .then(|| name.iter().map(|&b| b as char).collect())
        })
        .collect()
}

/// `music_info5.bin`: `u16` record count, `u16`, then 0x50-byte records
/// (`u16` song id at 0xc, the 8-byte name at 0x10).
fn music_info(bytes: &[u8]) -> Vec<(u32, String)> {
    let count = u16_at(bytes, 0).unwrap_or(0) as usize;
    (0..count)
        .filter_map(|k| {
            let r = bytes.get(4 + k * 0x50..4 + (k + 1) * 0x50)?;
            let name = &r[0x10..0x18];
            let end = name.iter().position(|&b| b == 0).unwrap_or(8);
            Some((
                u16_at(r, 0xc)? as u32,
                name[..end].iter().map(|&b| b as char).collect(),
            ))
        })
        .collect()
}

impl Disc {
    /// Reads the tables, `None` when the disc is not this game.
    pub fn open<R: Read + Seek>(
        disc: &mut SectorReader<R>,
        volume: &Volume,
    ) -> io::Result<Option<Self>> {
        let Some(exe) = volume.read_file(disc, EXE_PATH)? else {
            return Ok(None);
        };
        let (Some(dat), Some(spk), Some(irx)) = (
            volume.find(disc, DAT_PATH)?,
            volume.find(disc, SPK_PATH)?,
            volume.read_file(disc, DRIVER_PATH)?,
        ) else {
            return Ok(None);
        };
        let key = exe_at(&exe, DAT_KEY, 0x30).ok_or_else(|| bad("no DAT key"))?;
        let ids: Vec<u32> = exe_at(&exe, SONG_IDS, SONGS * 4)
            .ok_or_else(|| bad("no song table"))?
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        let names = driver_names(&irx).ok_or_else(|| bad("no sound names"))?;
        let files = dat_files(disc, dat, key)?;
        let mut read = |name: &str| -> io::Result<Vec<u8>> {
            let (_, at, size) = files
                .iter()
                .find(|f| f.0 == name)
                .ok_or_else(|| bad("DAT file missing"))?;
            disc.read_at(dat.offset() + at, *size)
        };
        let info = music_info(&read("music_info/music_info5.bin")?);
        let seq_codes = read("seq/seqcode.dat")?
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        let seq_data = read("seq/seqdata.dat")?;
        let head = unscramble(&disc.read_at(spk.offset(), 0x4000)?);
        let spk_files = spk_files(&head).ok_or_else(|| bad("bad SPK table"))?;

        let mut songs: Vec<Song> = Vec::new();
        for (k, &id) in ids.iter().enumerate() {
            let code = names[k * 6].get(3..6).unwrap_or_default().to_string();
            // A song without music here borrows another's files.
            if songs.iter().any(|s| s.code == code) {
                continue;
            }
            let Some((_, name)) = info.iter().find(|(i, _)| *i == id) else {
                continue;
            };
            songs.push(Song {
                info: MusicInfo {
                    id,
                    name: name.to_uppercase(),
                    bpm: 0,
                    gf_levels: [0; 3],
                    dm_levels: [0; 3],
                    bass_levels: [0; 3],
                    open_levels: [0; 3],
                    length: 0,
                },
                code,
            });
        }
        Ok(Some(Self {
            songs,
            spk,
            spk_files,
            seq_codes,
            seq_data,
        }))
    }

    /// Chart `name` (`seq120_1p1nor`), unpacked: the first entry with its
    /// hash, as the game finds it.
    pub fn chart(&self, name: &str) -> Option<Vec<u8>> {
        let h = name_hash(name);
        let n = self.seq_codes.iter().position(|&c| c == h)?;
        let count = u32_at(&self.seq_data, 0)? as usize;
        if n >= count {
            return None;
        }
        let entry = |k: usize| u32_at(&self.seq_data, 0x10 + k * 4);
        let start = entry(n)?;
        let end = match n + 1 < count {
            true => entry(n + 1)? & 0x7fff_ffff,
            false => self.seq_data.len() as u32,
        } as usize;
        let bytes = self.seq_data.get((start & 0x7fff_ffff) as usize..end)?;
        match start >> 31 {
            1 => unpack(bytes),
            _ => Some(bytes.to_vec()),
        }
    }

    /// A song's charts as `gfdm` sequences.
    pub fn song_charts(&self, song: &Song) -> SongCharts {
        let id = song.info.id;
        let name = |s: &str| format!("seq{id:03}{s}");
        let pick =
            |names: &[&str]| -> Option<Vec<u8>> { names.iter().find_map(|s| self.chart(&name(s))) };
        let mut guitar = Seq { tracks: Vec::new() };
        // (part, difficulty, names to try): 1 guitar, 2 bass.
        let mut used: Vec<String> = Vec::new();
        let mut guitar_bars = Vec::new();
        for (part, side) in [(1u8, "_1p1"), (2, "_2p2")] {
            for (difficulty, names) in [
                (1u8, &["nor"][..]),
                (2, &["exp", "ex1"][..]),
                (3, &["ex2", "ex1"][..]),
            ] {
                let names: Vec<String> = names.iter().map(|d| format!("{side}{d}")).collect();
                let Some(n) = names
                    .iter()
                    .find(|n| !used.contains(n) && self.chart(&name(n)).is_some())
                else {
                    continue;
                };
                used.push(n.clone());
                let bytes = pick(&[n]).unwrap_or_default();
                let bars = guitar_track(&mut guitar, &bytes, difficulty, part);
                if guitar_bars.is_empty() {
                    guitar_bars = bars;
                }
            }
        }
        let mut drums = Seq { tracks: Vec::new() };
        let mut drum_lines = (Vec::new(), Vec::new());
        for (difficulty, names) in [
            (1u8, &["norm", "easy"][..]),
            (2, &["real"][..]),
            (3, &["expr"][..]),
        ] {
            if let Some(bytes) = pick(names) {
                let lines = drum_track(&mut drums, &bytes, difficulty);
                if drum_lines.0.is_empty() {
                    drum_lines = lines;
                }
            }
        }
        // The tempo comes from the drums' beat lines, which both parts
        // share; guitar charts have bar lines only, and some bars are two
        // beats. Each part keeps its own bar lines.
        let (mut drum_bars, beats) = drum_lines;
        drum_bars.sort_unstable();
        drum_bars.dedup();
        guitar_bars.sort_unstable();
        guitar_bars.dedup();
        let grid = match drum_bars.is_empty() {
            true => beat_grid(&guitar_bars, &[]),
            false => beat_grid(&drum_bars, &beats),
        };
        if !drums.tracks.is_empty() {
            let meta = meta_track(Game::DrumMania, &mut drum_bars, &grid);
            drums.tracks.insert(0, meta);
        }
        if !guitar.tracks.is_empty() {
            let meta = meta_track(Game::GuitarFreaks, &mut guitar_bars, &grid);
            guitar.tracks.insert(0, meta);
        }
        SongCharts {
            info: song.info.clone(),
            drums,
            guitar,
            lanes: Vec::new(),
        }
    }

    /// File `name` of the SPK.
    pub fn read_spk<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        name: &str,
    ) -> io::Result<Option<Vec<u8>>> {
        let Some(f) = self.spk_files.iter().find(|f| f.name == name) else {
            return Ok(None);
        };
        if f.offset + f.size > self.spk.size {
            return Err(bad("SPK file runs past the SPK"));
        }
        disc.read_at(self.spk.offset() + f.offset, f.size as usize)
            .map(Some)
    }

    /// A song's `.sdd` (`SoundFile`).
    pub fn sound_file<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        song: &Song,
        file: SoundFile,
    ) -> io::Result<Option<Vec<u8>>> {
        self.read_spk(disc, &format!("{}_{}.sdd", file.prefix(), song.code))
    }
}

/// Raw 8-byte events, up to the all-zero one that ends a chart.
fn events(bytes: &[u8]) -> impl Iterator<Item = (u32, u16, u16)> + '_ {
    bytes
        .chunks_exact(8)
        .map(|e| {
            (
                u32::from_le_bytes(e[..4].try_into().unwrap()),
                u16::from_le_bytes([e[4], e[5]]),
                u16::from_le_bytes([e[6], e[7]]),
            )
        })
        .take_while(|&e| e != (0, 0, 0))
}

/// Steady stretches of bars (times in 1/75 s): `(first bar, ticks a
/// beat)`. A bar's beats are the beat lines inside it plus one, or four
/// when there are no beat lines (guitar charts). The disc rounds each line
/// to a tick, so bars of one tempo come out 138 and 139 ticks long; fitting
/// a stretch gets the tempo back (130.00). Bar lines, not beat lines, set
/// the stretches: in DAY DREAM the beat lines run on a grid of their own.
fn beat_grid(bars: &[u32], beat_lines: &[u32]) -> Vec<(u32, f64)> {
    // A bar off the stretch's tempo by more than this starts a new tempo.
    const TOLERANCE: f64 = 1.5;
    // Without beat lines a bar is four beats, or the count (a two-beat bar)
    // that keeps the last bar's beat when one fits.
    let mut last_beat: Option<f64> = None;
    let beats: Vec<u32> = bars
        .windows(2)
        .map(|w| {
            let len = (w[1] - w[0]) as f64;
            let n = match (beat_lines.is_empty(), last_beat) {
                (false, _) => {
                    1 + beat_lines.iter().filter(|&&t| t > w[0] && t < w[1]).count() as u32
                }
                (true, Some(beat)) => (1..=8)
                    .find(|&n| (len - n as f64 * beat).abs() <= TOLERANCE)
                    .unwrap_or(4),
                (true, None) => 4,
            };
            last_beat = Some(len / n as f64);
            n
        })
        .collect();
    let mut grid = Vec::new();
    let mut i = 0;
    while i + 1 < bars.len() {
        let mut j = i + 1;
        while j + 1 < bars.len() {
            let per = (bars[j] - bars[i]) as f64 / beats[i..j].iter().sum::<u32>() as f64;
            let next = (bars[j + 1] - bars[j]) as f64;
            if (next - per * beats[j] as f64).abs() > TOLERANCE {
                break;
            }
            j += 1;
        }
        let n = beats[i..j].iter().sum::<u32>() as f64;
        let mut bpm = 60.0 * TICKS_PER_SECOND as f64 * n / (bars[j] - bars[i]) as f64;
        // 140.01 from the rounding is 140.
        bpm = if (bpm - bpm.round()).abs() < 0.03 {
            bpm.round()
        } else {
            (bpm * 100.0).round() / 100.0
        };
        grid.push((bars[i], 60.0 * TICKS_PER_SECOND as f64 / bpm));
        i = j;
    }
    grid
}

/// A meta track: a tempo at the start of each stretch of `grid`, and the
/// bar lines (1/75 s), each moved onto the grid's nearest beat when it is
/// that close.
fn meta_track(game: Game, bars: &mut Vec<u32>, grid: &[(u32, f64)]) -> Track {
    bars.sort_unstable();
    bars.dedup();
    let mut events = Vec::new();
    for &(start, ticks) in grid {
        let us = (ticks * 1_000_000.0 / TICKS_PER_SECOND as f64).round() as u32;
        events.push(event(start * TO_GFDM, 0, 0x10, us, 0));
    }
    if grid.is_empty() {
        events.push(event(0, 0, 0x10, 500_000, 0));
    }
    for &t in bars.iter() {
        let mut time = (t * TO_GFDM) as f64;
        if let Some(&(start, ticks)) = grid.iter().rev().find(|g| g.0 <= t).or(grid.first()) {
            let k = (t as f64 - start as f64) / ticks;
            if (k - k.round()).abs() < 0.1 {
                time = (start as f64 + k.round() * ticks) * TO_GFDM as f64;
            }
        }
        events.push(event(time.round() as u32, 0, 0x50, 0, 0));
    }
    Track {
        game,
        is_meta: true,
        difficulty: 0,
        part: 0,
        events,
    }
}

fn event(time: u32, value: u8, kind: u8, a: u32, b: u32) -> Event {
    Event {
        time,
        value,
        kind,
        a,
        b,
    }
}

/// Adds a guitar chart: its notes (`a` the tone, high half the length in
/// 1/300 s), and the meta track from its bar lines if there is none yet.
fn guitar_track(seq: &mut Seq, bytes: &[u8], difficulty: u8, part: u8) -> Vec<u32> {
    let mut bars = Vec::new();
    let mut notes = Vec::new();
    for (time, length, frets) in events(bytes) {
        let (t, tone) = (time & 0xffff, time >> 16);
        if length == 0 && frets == 0x10 {
            // A last bar line at 0xffff marks the end.
            if t != 0xffff {
                bars.push(t);
            }
        } else if frets & 7 != 0 {
            let ticks = (length as u32 * TO_GFDM).min(0xffff);
            notes.push(event(
                t * TO_GFDM,
                (frets & 0x27) as u8,
                0,
                tone | ticks << 16,
                0x7f,
            ));
        }
    }
    if notes.is_empty() {
        return Vec::new();
    }
    seq.tracks.push(Track {
        game: Game::GuitarFreaks,
        is_meta: false,
        difficulty,
        part,
        events: notes,
    });
    bars
}

/// Adds a drum chart: lanes 0–5 as notes, lane 6 as sounds (`a` the key).
fn drum_track(seq: &mut Seq, bytes: &[u8], difficulty: u8) -> (Vec<u32>, Vec<u32>) {
    let mut bars = Vec::new();
    let mut beats = Vec::new();
    let mut notes = Vec::new();
    for (time, kind, key) in events(bytes) {
        let (lane, volume) = (kind as u8, (kind >> 8) as u32);
        match lane {
            0..=5 => notes.push(event(time * TO_GFDM, lane, 0, key as u32, volume)),
            6 => notes.push(event(time * TO_GFDM, 0xff, 1, key as u32, volume)),
            7 => bars.push(time),
            8 => beats.push(time),
            _ => {}
        }
    }
    if !notes.iter().any(|e| e.kind == 0) {
        return (Vec::new(), Vec::new());
    }
    seq.tracks.push(Track {
        game: Game::DrumMania,
        is_meta: false,
        difficulty,
        part: 0,
        events: notes,
    });
    (bars, beats)
}

/// One tone of a sound bank (`VPHD`, 0x20 bytes from 0x240).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tone {
    /// 0..=127.
    pub volume: u8,
    /// 0..=127, 64 the centre.
    pub pan: u8,
    /// The key the sample sounds at its own rate.
    pub root: u8,
    /// The keys it plays.
    pub low: u8,
    pub high: u8,
    /// Byte range of its PS-ADPCM in the bank body.
    pub start: usize,
    pub end: usize,
}

impl Tone {
    /// The sample rate to play it at for `key`.
    pub fn rate(&self, key: u8) -> u32 {
        let semis = key as f64 - self.root as f64;
        (MUSIC_RATE as f64 * (semis / 12.0).exp2()).round() as u32
    }
}

/// A sound bank: its tones and its body.
#[derive(Debug, Clone)]
pub struct Bank<'a> {
    pub tones: Vec<Tone>,
    pub body: &'a [u8],
}

impl Bank<'_> {
    /// The tone for `key`.
    pub fn tone_for(&self, key: u8) -> Option<&Tone> {
        self.tones.iter().find(|t| t.low <= key && key <= t.high)
    }

    /// A tone's samples (mono, at `Tone::rate`).
    pub fn decode(&self, tone: &Tone) -> Vec<i16> {
        let data = self.body.get(tone.start..tone.end).unwrap_or_default();
        crate::adpcm::decode(data, None, true)
    }
}

/// The sections of an `.sdd` (`SDDT`): `u32` count at 0xc, (offset, size)
/// pairs from 0x40.
fn sections(sdd: &[u8]) -> Option<Vec<&[u8]>> {
    if sdd.get(..4)? != b"SDDT" {
        return None;
    }
    let count = u32_at(sdd, 0xc)? as usize;
    (0..count)
        .map(|k| {
            let at = u32_at(sdd, 0x40 + k * 8)? as usize;
            let size = u32_at(sdd, 0x44 + k * 8)? as usize;
            sdd.get(at..at + size)
        })
        .collect()
}

/// The sound banks of an `.sdd`, in order: each `VPHD` head goes with the
/// body at the same place after the heads.
pub fn banks(sdd: &[u8]) -> Vec<Bank<'_>> {
    let Some(s) = sections(sdd) else {
        return Vec::new();
    };
    let heads: Vec<&[u8]> = s
        .iter()
        .copied()
        .filter(|x| x.starts_with(b"VPHD"))
        .collect();
    let first = s.iter().position(|x| x.starts_with(b"VPHD")).unwrap_or(0);
    heads
        .iter()
        .enumerate()
        .filter_map(|(k, h)| {
            let body = *s.get(first + heads.len() + k)?;
            let count = u16_at(h, 0xe)? as usize;
            let tones = (0..count)
                .map(|n| {
                    let t = h.get(0x240 + n * 0x20..0x260 + n * 0x20)?;
                    let start = u32_at(t, 0xc)? as usize * 16;
                    Some(Tone {
                        volume: t[2],
                        pan: t[3],
                        root: t[4],
                        low: t[6],
                        high: t[7],
                        start,
                        end: start + u32_at(t, 0x10)? as usize * 16,
                    })
                })
                .collect::<Option<Vec<Tone>>>()?;
            Some(Bank { tones, body })
        })
        .collect()
}

/// The music of an `.sdd` (its last section), stereo at `MUSIC_RATE`.
pub fn music(sdd: &[u8]) -> Option<[Vec<i16>; 2]> {
    let stream = *sections(sdd)?.last()?;
    let mut sides = [Vec::new(), Vec::new()];
    for (k, block) in stream.chunks(MUSIC_INTERLEAVE).enumerate() {
        sides[k % 2].extend_from_slice(block);
    }
    let [l, r] = sides;
    Some([
        crate::adpcm::decode(&l, None, false),
        crate::adpcm::decode(&r, None, false),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spk_scramble_hides_zeros_as_a_rising_run() {
        // Zeros scrambled: each byte is the last plus its offset.
        let mut raw = vec![0x50u8];
        for n in 1..64 {
            let last: u8 = raw[n - 1];
            raw.push(last.wrapping_add(n as u8));
        }
        let plain = unscramble(&raw);
        assert_eq!(plain[0], 0x50);
        assert!(plain[1..].iter().all(|&b| b == 0));
    }

    #[test]
    fn the_spk_table_lists_files_after_the_head() {
        let mut head = vec![0u8; 0x60];
        head[8..12].copy_from_slice(&0x4000u32.to_le_bytes());
        head[0xc..0x10].copy_from_slice(&2u32.to_le_bytes());
        for (k, (size, name)) in [(0x800u32, "d1_abc.sdd"), (0x1000, "pr_abc.sdd")]
            .iter()
            .enumerate()
        {
            let e = 0x40 + k * 16;
            head[e..e + 4].copy_from_slice(&size.to_le_bytes());
            head[e + 4] = name.len() as u8;
            head[e + 5..e + 5 + name.len()].copy_from_slice(name.as_bytes());
        }
        let files = spk_files(&head).unwrap();
        assert_eq!(files[1].name, "pr_abc.sdd");
        assert_eq!(files[1].offset, 0x4800);
    }

    #[test]
    fn chart_names_hash_like_the_game() {
        // Values from the disc's seqcode.dat.
        assert_eq!(name_hash("seq120_1p1ex1"), name_hash("seq172_1p21p1"));
        assert_ne!(name_hash("seq120_1p1nor"), name_hash("seq120_1p1ex1"));
        assert!(name_hash("") >= 1);
    }

    /// Packs `data` with every byte code 8 bits long (all lengths equal)
    /// and LZ literals only, as a test of the two stages.
    #[test]
    fn a_flat_code_unpacks_to_its_bytes() {
        // LZ: control 0x00 (8 literals), then the end code under a 1 flag.
        let lz: Vec<u8> = [&[0u8][..], b"ABCDEFGH", &[0x01, 0xff]].concat();
        let mut bits: Vec<bool> = Vec::new();
        // Byte 0: flag 1, gamma(8) = 0001000; the rest: flag 0.
        bits.extend([true, false, false, false, true, false, false, false]);
        bits.extend(std::iter::repeat_n(false, 255));
        for &b in &lz {
            bits.extend((0..8).rev().map(|i| b >> i & 1 != 0));
        }
        let mut packed = (lz.len() as u32).to_le_bytes().to_vec();
        for chunk in bits.chunks(8) {
            let mut byte = 0u8;
            for (i, &b) in chunk.iter().enumerate() {
                byte |= (b as u8) << (7 - i);
            }
            packed.push(byte);
        }
        assert_eq!(unpack(&packed).unwrap(), b"ABCDEFGH");
    }

    #[test]
    fn a_guitar_note_keeps_its_tone_and_length() {
        let mut bytes = Vec::new();
        for (time, a, b) in [
            (0u32, 0u16, 0x10u16),
            (100, 0, 0x10),
            (40 | 70 << 16, 17, 0x21),
        ] {
            bytes.extend_from_slice(&time.to_le_bytes());
            bytes.extend_from_slice(&a.to_le_bytes());
            bytes.extend_from_slice(&b.to_le_bytes());
        }
        let mut seq = Seq { tracks: Vec::new() };
        let mut bars = guitar_track(&mut seq, &bytes, 1, 1);
        let note = seq.tracks[0].events[0];
        assert_eq!(note.time, 160);
        assert_eq!(note.value, 0x21);
        assert_eq!(note.a, 70 | 68 << 16);
        // 100 ticks a bar at 75/s, four beats: 1/3 s a beat (180 BPM).
        let grid = beat_grid(&bars, &[]);
        let tempo = meta_track(Game::GuitarFreaks, &mut bars, &grid).events[0];
        assert_eq!((tempo.kind, tempo.a), (0x10, 333_333));
    }

    #[test]
    fn bars_rounded_to_ticks_keep_one_tempo() {
        // 130 BPM: 138.46 ticks a bar, each rounded to a tick on the disc.
        let mut bars: Vec<u32> = (0..20)
            .map(|k| (k as f64 * 138.4615).round() as u32)
            .collect();
        let grid = beat_grid(&bars, &[]);
        let meta = meta_track(Game::DrumMania, &mut bars, &grid);
        let tempos: Vec<_> = meta.events.iter().filter(|e| e.kind == 0x10).collect();
        assert_eq!(tempos.len(), 1);
        assert_eq!(tempos[0].a, 461_538);
        // Bar 3 sits on the grid (3 × 138.46 ticks × 4), not the rounded 415.
        let bar3 = meta
            .events
            .iter()
            .filter(|e| e.kind == 0x50)
            .nth(3)
            .unwrap();
        assert_eq!(bar3.time, 1662);
    }

    #[test]
    fn a_tone_plays_higher_above_its_root() {
        let t = Tone {
            volume: 127,
            pan: 64,
            root: 43,
            low: 40,
            high: 44,
            start: 0,
            end: 0,
        };
        assert_eq!(t.rate(43), 44100);
        assert_eq!(t.rate(55), 88200);
    }
}

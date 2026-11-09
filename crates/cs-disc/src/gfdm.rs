//! GuitarFreaks & DrumMania PS2 discs (Masterpiece Silver, `SLPM_664.84`).
//!
//! Every file is in `DATA/PACK/Dnnn/PACKnnnn.PAK`, found through the index
//! `DATA/PACK/PACKINFO.BIN`. Nothing is encrypted or compressed. One song is
//! one pack: two charts (`SEQP`, one DrumMania, one GuitarFreaks), two sample
//! banks, a movie with the song's music and a small info file.
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
    /// The 14-byte song info file (seven `u16`, meaning not yet known).
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
        if end < 16 || end > bytes.len() || (end - 16) % 32 != 0 || (bytes.len() - end) % 16 != 0 {
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

/// One song: the files of a pack that holds charts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Song {
    pub pack: u16,
    /// Index entries, in index order.
    pub files: Vec<(usize, Kind)>,
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

    /// The songs: packs with at least one chart, files sorted by kind.
    /// Reads each file's first sector to tell kinds apart.
    pub fn songs<R: Read + Seek>(&self, disc: &mut SectorReader<R>) -> io::Result<Vec<Song>> {
        let mut by_pack: Vec<Vec<(usize, Kind)>> = vec![Vec::new(); self.packs.len()];
        for (n, e) in self.index.entries.iter().enumerate() {
            let Some(pack) = self.pack(e.pack) else {
                continue;
            };
            let head =
                disc.read_at(pack.offset() + e.offset as u64, (e.size as usize).min(0x20))?;
            by_pack[e.pack as usize].push((n, kind(&head, e.size)));
        }
        Ok(by_pack
            .into_iter()
            .enumerate()
            .filter(|(_, files)| files.iter().any(|f| f.1 == Kind::Seq))
            .map(|(pack, files)| Song {
                pack: pack as u16,
                files,
            })
            .collect())
    }
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
    /// GF: 1 guitar, 2 bass, 3 seen on a few songs (unknown). DM: 0.
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

#[cfg(test)]
mod tests {
    use super::*;

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

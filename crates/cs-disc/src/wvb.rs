//! IIDX 14–16 sound files: key sound banks (`.wvb`) and BGM streams (`.pcm`).
//!
//! Layouts from iidx-ps2tools (`parse_wvb.parse_wvb_new`) and the iidx16-bms
//! work (`tools/ps2adpcm.py`, `tools/extract_sounds.py`).
//!
//! Key sound bank:
//! - `0x10..0x8000`: one 16-byte entry per key sound, numbered from 1 as the
//!   charts number them: `u16 channel` (0xff00 mono, 0x0000 left, 0x0100
//!   right: a left entry followed by a right one is a stereo sound, and the
//!   right entry is still a sound of its own, which charts play too — DXY!'s
//!   42, GEKKO's 94), `u16 kind` (0 unused), `u32 duration ms`, `u8 pan`,
//!   `u8 pan` again, `u16 file`, `u8 volume`, 3 bytes. Kind 1 sounds are
//!   mono placed by the pan (64 center); kind 2 store 1 and 127 there and
//!   play as stored. The volume (0 to 127, linear) is every sound's.
//! - `0x8000`: `u32 file count, u32, u32 scrambled` (with RED's key on 11,
//!   `adpcm::KEY_KEYSOUND_RED`, told by its end frames), then one 16-byte record
//!   per file: `u32 offset, u32 size, u16, u16 rate code (high byte), u32`.
//!   File data follows the records.
//!
//! Older key sound bank (8th and before, `parse_wvb.parse_wvb_old`), told by
//! its first word being the file's length: `u32 size, u32, u32 from end`
//! (the third section starts that many bytes before the end), then from
//! 0x20 to 0x4000 one 32-byte entry per sound: `u16 number, u16, u8,
//! u8 volume, u8 pan, u8 kind (low nibble), u32 rate, u32, u32 start, u32
//! end, u32 size, u32`. Kinds 2 and 4 start at `start - 0xf020` (4 ending at
//! `end - 0xf020`, 2 at its end frame), kind 3 at `start` into the third
//! section. Every sound is mono and not scrambled.
//!
//! pop'n music 8 to 10 bank (`"6png"`/`"rpng"`): a 16-byte head `0x10,
//! 0x4010, size, 0`, then the bank. In it, from 0x10 to 0x4000, one 16-byte
//! entry per sound numbered from 1: `u16 kind` (0 unused, 0x100 mono, 0x200
//! stereo of files `file` and `file + 1`), `u16` (0xff00 mostly), `u16 file`,
//! `u16` (0 or 1),
//! `u8 volume`, `u8`, `u8 pan`, `u8 pan`, `u32`. At 0x4000: `u32, u32 file
//! count, u32, u32 data size`, then one 16-byte record per file: `u32 offset,
//! u16, u16 rate code (high byte), u32 size, u32`. File data follows the
//! records. Not scrambled.
//!
//! pop'n music 8 bank: a 16-byte head `0x10, file table offset, size, 0`,
//! then from 0x20 to 0x2010 one 16-byte entry per sound numbered from 1: `u8,
//! u8 kind` (0 unused, 1 mono, 2 stereo), `u8, u8 0xff`, then one `u32` per
//! channel: the offset from 0x2010 of its parameter list. A list is `u32`
//! tokens, the tag in the high byte and the value below: 0xd2 the file, 0xdd
//! the pan, 0xf8 the volume (bits 16 to 23), 0xff the end. At the file table:
//! `u32, u32 count, u32, u32 data size`, then 16-byte records whose first
//! `u32` is the offset; a file runs to the next one. No rate is stored that
//! reads as one; 44100 Hz is assumed (not checked by ear).
//!
//! BGM stream: a 0x800-byte header (`u32` fields: data start at 8, size at
//! 12, rate at 24, channels at 28, scrambled at 32, interleave at 36, volume
//! at 40, which is not applied to the stream (see `decode_bgm`)), then
//! ADPCM frames interleaved per channel every 16 bytes. 8th and before:
//! big-endian `u32 size, u16, u16 rate, u8 channels`, told by the size
//! filling the file after the 0x800 header; not scrambled, and the channels
//! take turns every 0x800 bytes (read every 16, the whole song was noise;
//! 0x800 matches vgmstream sample for sample on 8th's GIUDECCA). pop'n 9
//! and 10 (`"GbTs"`): the same fields at 8, 12, 24 and 28, but 1 at 32 and
//! 32 at 36 are not a scramble and an interleave: the stream is plain and
//! takes turns every 16 bytes (the only interleave whose two channels come
//! out alike, as a song's left and right do).

use std::fmt;

use crate::adpcm::{self, Key, KEY_BGM, KEY_KEYSOUND, KEY_KEYSOUND_RED};

/// Decoded audio: one 16-bit sample vector per channel (1 or 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pcm {
    pub rate: u32,
    pub channels: Vec<Vec<i16>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SoundError {
    TooShort,
    UnknownRate(u8),
    BadInterleave(u32),
}

impl fmt::Display for SoundError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SoundError::TooShort => write!(f, "sound data is too short"),
            SoundError::UnknownRate(code) => write!(f, "unknown sample rate code {code:#04x}"),
            SoundError::BadInterleave(n) => write!(f, "unsupported interleave {n}"),
        }
    }
}

fn u16_at(b: &[u8], at: usize) -> Result<u16, SoundError> {
    Ok(u16::from_le_bytes(
        b.get(at..at + 2)
            .ok_or(SoundError::TooShort)?
            .try_into()
            .unwrap(),
    ))
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, SoundError> {
    Ok(u32::from_le_bytes(
        b.get(at..at + 4)
            .ok_or(SoundError::TooShort)?
            .try_into()
            .unwrap(),
    ))
}

/// The rate a sound's stored length implies: its samples over its duration.
/// The code table is off for some codes (pop'n 13's 34000 plays at 33000,
/// IIDX 12's 36000 at 35000), and the same code differs between discs, while
/// the durations agree with the table where it is right. Every sound with a
/// duration plays at its measured rate; the table is only a fallback for a
/// zero duration or a rate outside the range discs use.
fn measured_rate(samples: usize, duration_ms: u32) -> Option<u32> {
    if duration_ms == 0 {
        return None;
    }
    let rate = (samples as u64 * 1000 + duration_ms as u64 / 2) / duration_ms as u64;
    (8000..=96000).contains(&rate).then_some(rate as u32)
}

/// Sample rates by the code in a bank's file record ("Thanks Konami").
fn rate(code: u8) -> Result<u32, SoundError> {
    Ok(match code {
        // pop'n 14 (モッズ): its entries' lengths in ms give 48016.
        0x3c => 48000,
        0x3e => 44100,
        0x3f => 42000,
        0x40 => 40000,
        0x41 => 37000,
        0x42 => 36000,
        0x43 => 34000,
        0x44 => 32000,
        0x45 => 30000,
        0x46 => 28000,
        0x47 => 26000,
        0x48 => 24000,
        0x4a => 22050,
        0x4c => 20000,
        0x4d => 18000,
        0x50 => 16000,
        0x54 => 12000,
        // The codes are semitones down from 44100 (0x4a is 22050); one the
        // table lacks (pop'n 9's 0x68) takes that.
        0x30..=0x7f => (44100.0 * 2f64.powf(-(code as f64 - 62.0) / 12.0)).round() as u32,
        _ => return Err(SoundError::UnknownRate(code)),
    })
}

const TABLE_START: usize = 0x10;
const TABLE_END: usize = 0x8000;
const ENTRY: usize = 16;

/// How a key sound is placed in the stereo mix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// A mono sound, panned by its `pan`.
    Mono,
    /// A stereo sound: this entry's file is the left channel and `right` the right.
    Stereo { right: u16 },
}

/// One key sound of a bank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeySound {
    pub file: u16,
    pub placement: Placement,
    /// Where a mono sound sits, 1 left, 64 center, 127 right: byte 8 of a
    /// kind 1 entry (pop'n places most sounds off center), 64 for kind 2.
    pub pan: u8,
    /// Byte 12, 0 to 127. Masters differ in loudness and the volumes even
    /// them out: GOLD's THE LEAST 100sec plays its sliced music at 34 and
    /// 60 against 86 to 127 for its other sounds.
    pub volume: u8,
    /// Bytes 4 to 7: the sound's length in milliseconds.
    pub duration_ms: u32,
}

/// A key sound bank, borrowed from its file's bytes.
pub struct Bank<'a> {
    data: &'a [u8],
    key: Option<Key>,
    files: Vec<(usize, usize, u32)>,
    /// By key sound number - 1.
    sounds: Vec<Option<KeySound>>,
}

impl<'a> Bank<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, SoundError> {
        if u32_at(data, 0)? as usize == data.len() {
            return Self::parse_old(data);
        }
        if u32_at(data, 0)? == 0x10 && u32_at(data, 4)? == 0x4010 {
            return Self::parse_popn(&data[0x10..]);
        }
        if u32_at(data, 0)? == 0x10 && u32_at(data, 0x14)? == 7 {
            return Self::parse_popn8(data);
        }
        let count = u32_at(data, TABLE_END)? as usize;
        let scrambled = u32_at(data, TABLE_END + 8)? != 0;
        let records = TABLE_END + 0x10;
        // Not a bank (a wrong file number): its count would not fit.
        if count > (data.len().saturating_sub(records)) / ENTRY {
            return Err(SoundError::TooShort);
        }
        let base = records + count * ENTRY;
        let mut files = Vec::with_capacity(count);
        for i in 0..count {
            let at = records + i * ENTRY;
            let offset = base + u32_at(data, at)? as usize;
            let size = u32_at(data, at + 4)? as usize;
            let code = (u16_at(data, at + 10)? >> 8) as u8;
            files.push((offset, size, rate(code)?));
        }
        let entries: Vec<[u8; ENTRY]> = (TABLE_START..TABLE_END)
            .step_by(ENTRY)
            .map(|at| data[at..at + ENTRY].try_into().unwrap())
            .collect();
        let mut sounds = Vec::with_capacity(entries.len());
        for (i, e) in entries.iter().enumerate() {
            let channel = u16::from_le_bytes([e[0], e[1]]);
            let kind = u16::from_le_bytes([e[2], e[3]]);
            let file = u16::from_le_bytes([e[10], e[11]]);
            let sound = (kind != 0 && (file as usize) < files.len()).then(|| {
                let right = entries.get(i + 1).and_then(|n| {
                    let n_channel = u16::from_le_bytes([n[0], n[1]]);
                    let n_file = u16::from_le_bytes([n[10], n[11]]);
                    (channel == 0x0000 && n_channel == 0x0100 && (n_file as usize) < files.len())
                        .then_some(n_file)
                });
                KeySound {
                    file,
                    placement: match right {
                        Some(right) => Placement::Stereo { right },
                        None => Placement::Mono,
                    },
                    pan: if kind >> 8 == 1 { e[8] } else { CENTER },
                    volume: e[12],
                    duration_ms: u32::from_le_bytes([e[4], e[5], e[6], e[7]]),
                }
            });
            sounds.push(sound);
        }
        Ok(Bank {
            data,
            key: scrambled.then(|| scramble_key(data, &files)),
            files,
            sounds,
        })
    }

    /// The pop'n 8 to 10 bank form (module docs), past its 16-byte head.
    fn parse_popn(data: &'a [u8]) -> Result<Self, SoundError> {
        const END: usize = 0x4000;
        let count = u32_at(data, END + 4)? as usize;
        let records = END + 0x10;
        if count > data.len().saturating_sub(records) / ENTRY {
            return Err(SoundError::TooShort);
        }
        let base = records + count * ENTRY;
        let mut files = Vec::with_capacity(count);
        for i in 0..count {
            let at = records + i * ENTRY;
            let offset = base + u32_at(data, at)? as usize;
            let code = (u16_at(data, at + 6)? >> 8) as u8;
            let size = u32_at(data, at + 8)? as usize;
            files.push((offset, size, rate(code)?));
        }
        let mut sounds = Vec::new();
        for at in (TABLE_START..END).step_by(ENTRY) {
            let kind = u16_at(data, at)?;
            let file = u16_at(data, at + 4)? as usize;
            let stereo = kind == 0x200 && file + 1 < files.len();
            sounds.push((kind != 0 && file < files.len()).then(|| KeySound {
                file: file as u16,
                placement: if stereo {
                    Placement::Stereo {
                        right: file as u16 + 1,
                    }
                } else {
                    Placement::Mono
                },
                pan: if stereo { CENTER } else { data[at + 10] },
                volume: data[at + 8],
                duration_ms: 0,
            }));
        }
        while sounds.last() == Some(&None) {
            sounds.pop();
        }
        Ok(Bank {
            data,
            key: None,
            files,
            sounds,
        })
    }

    /// The pop'n 8 bank form (module docs).
    fn parse_popn8(data: &'a [u8]) -> Result<Self, SoundError> {
        const PARAMS: usize = 0x2010;
        let table = u32_at(data, 4)? as usize;
        let count = u32_at(data, table + 4)? as usize;
        let data_size = u32_at(data, table + 12)? as usize;
        let records = table + 0x10;
        if count > data.len().saturating_sub(records) / ENTRY {
            return Err(SoundError::TooShort);
        }
        let base = records + count * ENTRY;
        let offsets: Vec<usize> = (0..count)
            .map(|i| u32_at(data, records + i * ENTRY).map(|o| o as usize))
            .collect::<Result<_, _>>()?;
        let files = (0..count)
            .map(|i| {
                let end = offsets.get(i + 1).copied().unwrap_or(data_size);
                (base + offsets[i], end.saturating_sub(offsets[i]), 44100)
            })
            .collect::<Vec<_>>();
        // (file, pan, volume) of a parameter list.
        let params = |offset: u32| -> Option<(usize, u8, u8)> {
            let mut at = PARAMS + offset as usize;
            let (mut file, mut pan, mut volume) = (None, CENTER, 127);
            loop {
                let token = u32_at(data, at).ok()?;
                let value = token & 0xff_ffff;
                match token >> 24 {
                    0xd2 => file = Some(value as usize),
                    0xdd => pan = value as u8,
                    0xf8 => volume = (value >> 16) as u8,
                    0xff => break,
                    _ => {}
                }
                at += 4;
            }
            Some((file?, pan, volume))
        };
        let mut sounds = Vec::new();
        for at in (0x20..PARAMS).step_by(ENTRY) {
            let kind = data[at + 1];
            let left = (kind == 1 || kind == 2)
                .then(|| params(u32_at(data, at + 4).ok()?))
                .flatten()
                .filter(|&(file, _, _)| file < files.len());
            let right = (kind == 2)
                .then(|| params(u32_at(data, at + 8).ok()?))
                .flatten()
                .filter(|&(file, _, _)| file < files.len());
            sounds.push(left.map(|(file, pan, volume)| KeySound {
                file: file as u16,
                placement: match right {
                    Some((right, _, _)) => Placement::Stereo {
                        right: right as u16,
                    },
                    None => Placement::Mono,
                },
                pan: if right.is_some() { CENTER } else { pan },
                volume,
                duration_ms: 0,
            }));
        }
        while sounds.last() == Some(&None) {
            sounds.pop();
        }
        Ok(Bank {
            data,
            key: None,
            files,
            sounds,
        })
    }

    /// The older bank form (module docs): one file per sound.
    fn parse_old(data: &'a [u8]) -> Result<Self, SoundError> {
        const OLD_START: usize = 0x20;
        const OLD_END: usize = 0x4000;
        const OLD_ENTRY: usize = 0x20;
        const OLD_BASE: usize = 0xf020;
        let third = data
            .len()
            .checked_sub(u32_at(data, 8)? as usize)
            .ok_or(SoundError::TooShort)?;
        let mut files = Vec::new();
        let mut sounds = Vec::new();
        for at in (OLD_START..OLD_END).step_by(OLD_ENTRY) {
            let e = data.get(at..at + OLD_ENTRY).ok_or(SoundError::TooShort)?;
            let word = |i: usize| u32::from_le_bytes(e[i..i + 4].try_into().unwrap()) as usize;
            let number = u16::from_le_bytes([e[0], e[1]]) as usize;
            let (start, end) = match e[7] & 0x0f {
                2 => (word(16).wrapping_sub(OLD_BASE), data.len()),
                4 => (
                    word(16).wrapping_sub(OLD_BASE),
                    word(20).wrapping_sub(OLD_BASE),
                ),
                3 => (third + word(16), third + word(16) + word(24)),
                _ => continue,
            };
            if number == 0 || start >= end || end > data.len() {
                continue;
            }
            if sounds.len() < number {
                sounds.resize(number, None);
            }
            sounds[number - 1] = Some(KeySound {
                file: files.len() as u16,
                placement: Placement::Mono,
                pan: e[6],
                volume: e[5],
                duration_ms: 0,
            });
            files.push((start, end - start, word(8) as u32));
        }
        Ok(Bank {
            data,
            key: None,
            files,
            sounds,
        })
    }

    /// Key sound `n` (1-based), or `None` for an empty slot.
    pub fn sound(&self, n: u16) -> Option<KeySound> {
        self.sounds
            .get((n as usize).checked_sub(1)?)
            .copied()
            .flatten()
    }

    /// The numbers of the key sounds the bank has.
    pub fn numbers(&self) -> impl Iterator<Item = u16> + '_ {
        (1..=self.sounds.len() as u16).filter(|&n| self.sound(n).is_some())
    }

    /// One file's stored bytes, and whether they are scrambled.
    pub fn raw_file(&self, file: u16) -> Option<(&'a [u8], bool)> {
        let &(offset, size, _) = self.files.get(file as usize)?;
        Some((self.data.get(offset..offset + size)?, self.key.is_some()))
    }

    /// One file decoded: its rate and samples.
    pub fn decode_file(&self, file: u16) -> Result<(u32, Vec<i16>), SoundError> {
        let &(offset, size, rate) = self.files.get(file as usize).ok_or(SoundError::TooShort)?;
        let bytes = self
            .data
            .get(offset..offset + size)
            .ok_or(SoundError::TooShort)?;
        let key = self.key;
        Ok((rate, adpcm::decode(bytes, key, true)))
    }

    /// Key sound `n` decoded at its volume, both channels for a stereo sound
    /// or a mono one off center. `None` for an empty slot.
    pub fn decode(&self, n: u16) -> Option<Result<Pcm, SoundError>> {
        let sound = self.sound(n)?;
        Some((|| {
            let (table_rate, left) = self.decode_file(sound.file)?;
            let rate = measured_rate(left.len(), sound.duration_ms).unwrap_or(table_rate);
            let mut channels = vec![left];
            if let Placement::Stereo { right } = sound.placement {
                // pop'n 9 has two right halves stored with a size of -48
                // (ラテンポップ's 120, ホネホネ's 215): the left plays both.
                let right = match self.decode_file(right) {
                    Ok((_, right)) => right,
                    Err(_) => channels[0].clone(),
                };
                channels.push(right);
            }
            if channels.len() == 1 && sound.pan != CENTER {
                let (left, right) = pan_gains(sound.pan);
                let mono = channels.pop().unwrap();
                channels = [left, right]
                    .map(|gain| mono.iter().map(|&s| (s as f32 * gain) as i16).collect())
                    .into();
            }
            for channel in &mut channels {
                apply_volume(channel, sound.volume);
            }
            Ok(Pcm { rate, channels })
        })())
    }
}

/// Which key a scrambled bank uses: RED's when a file ends in RED's
/// scrambled end frame.
fn scramble_key(data: &[u8], files: &[(usize, usize, u32)]) -> Key {
    const RED_END: [u8; 4] = [0xff, 0x05, 0x66, 0xae];
    let red = files.iter().any(|&(offset, size, _)| {
        size >= 16 && data.get(offset + size - 16..offset + size - 12) == Some(&RED_END[..])
    });
    if red {
        KEY_KEYSOUND_RED
    } else {
        KEY_KEYSOUND
    }
}

/// The pan of a sound in the middle.
const CENTER: u8 = 64;

/// (left, right) gains of a mono sound at `pan`: the far side fades out
/// and the near side stays as stored, as a centered sound plays in both.
fn pan_gains(pan: u8) -> (f32, f32) {
    let pan = pan.min(127) as f32;
    (
        ((127.0 - pan) / 63.0).min(1.0),
        (pan / CENTER as f32).min(1.0),
    )
}

/// Scales samples by a bank or BGM volume, 127 being as stored.
fn apply_volume(samples: &mut [i16], volume: u8) {
    if volume >= 127 {
        return;
    }
    for s in samples {
        *s = (*s as i32 * volume as i32 / 127) as i16;
    }
}

/// A BGM stream decoded at full level. Its header volume (40) is not applied:
/// XENON's 48 would play it at 38%, too quiet under the key sounds.
pub fn decode_bgm(data: &[u8]) -> Result<Pcm, SoundError> {
    let head = data.get(..9).ok_or(SoundError::TooShort)?;
    let old_size = u32::from_be_bytes(head[..4].try_into().unwrap()) as usize;
    let (start, size, rate, count, scrambled, interleave) = if old_size + 0x800 == data.len() {
        let rate = u16::from_be_bytes([head[6], head[7]]) as u32;
        (
            0x800,
            old_size,
            rate,
            (head[8] as usize).max(1),
            false,
            0x800,
        )
    } else if head.starts_with(b"GbTs") {
        (
            u32_at(data, 8)? as usize,
            u32_at(data, 12)? as usize,
            u32_at(data, 24)?,
            (u32_at(data, 28)? as usize).max(1),
            false,
            16,
        )
    } else {
        (
            u32_at(data, 8)? as usize,
            u32_at(data, 12)? as usize,
            u32_at(data, 24)?,
            (u32_at(data, 28)? as usize).max(1),
            u32_at(data, 32)? != 0,
            u32_at(data, 36)?,
        )
    };
    if interleave != 16 && interleave != 0x800 {
        return Err(SoundError::BadInterleave(interleave));
    }
    let body = data
        .get(start..(start + size).min(data.len()))
        .ok_or(SoundError::TooShort)?;
    let key = scrambled.then_some(KEY_BGM);
    let channels = (0..count)
        .map(|c| {
            let frames: Vec<u8> = body
                .chunks_exact(interleave as usize)
                .skip(c)
                .step_by(count)
                .flatten()
                .copied()
                .collect();
            let samples = adpcm::decode(&frames, key, false);
            samples
        })
        .collect();
    Ok(Pcm { rate, channels })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bank of one scrambled file of one silent frame, and `entries`
    /// (channel, kind, pan, volume, file) from sound 1 on.
    fn bank(entries: &[(u16, u16, u8, u8, u16)]) -> Vec<u8> {
        let mut data = vec![0u8; TABLE_END];
        for (i, &(channel, kind, pan, volume, file)) in entries.iter().enumerate() {
            let at = TABLE_START + i * ENTRY;
            data[at..at + 2].copy_from_slice(&channel.to_le_bytes());
            data[at + 2..at + 4].copy_from_slice(&kind.to_le_bytes());
            data[at + 8] = pan;
            data[at + 9] = pan;
            data[at + 10..at + 12].copy_from_slice(&file.to_le_bytes());
            data[at + 12] = volume;
        }
        data.extend(2u32.to_le_bytes()); // two files
        data.extend(0u32.to_le_bytes());
        data.extend(1u32.to_le_bytes()); // scrambled
        data.extend(0u32.to_le_bytes());
        for offset in [0u32, 16] {
            data.extend(offset.to_le_bytes());
            data.extend(16u32.to_le_bytes());
            data.extend(0u16.to_le_bytes());
            data.extend(0x4800u16.to_le_bytes()); // 24000 Hz
            data.extend(0u32.to_le_bytes());
        }
        for _ in 0..2 {
            let mut frame = [0u8; 16];
            frame[0] = 0x0c ^ 0xff;
            frame[2] = KEY_KEYSOUND[1];
            data.extend(frame);
        }
        data
    }

    #[test]
    fn a_left_entry_with_a_right_entry_after_it_is_one_stereo_sound() {
        let data = bank(&[
            (0xff00, 0x0200, 64, 64, 0),
            (0x0000, 0x0100, 0, 127, 0),
            (0x0100, 0x0100, 0, 127, 1),
        ]);
        let bank = Bank::parse(&data).unwrap();
        assert_eq!(bank.sound(1).unwrap().placement, Placement::Mono);
        assert_eq!(
            bank.sound(2).unwrap().placement,
            Placement::Stereo { right: 1 }
        );
        // The right half is a sound too: DXY! plays its 42 alone.
        assert_eq!(bank.sound(3).unwrap().placement, Placement::Mono);
        assert_eq!(bank.numbers().collect::<Vec<_>>(), [1, 2, 3]);
        let pcm = bank.decode(2).unwrap().unwrap();
        assert_eq!(pcm.rate, 24000);
        assert_eq!(pcm.channels, vec![vec![0i16; 28]; 2]);
    }

    #[test]
    fn a_kind_1_sound_is_placed_by_its_pan_and_kind_2_plays_as_stored() {
        let data = bank(&[
            (0xff00, 0x0100, 1, 127, 0),
            (0xff00, 0x0100, 64, 127, 0),
            (0xff00, 0x0200, 1, 127, 0),
        ]);
        let bank = Bank::parse(&data).unwrap();
        assert_eq!(bank.sound(1).unwrap().pan, 1);
        assert_eq!(bank.sound(2).unwrap().pan, CENTER);
        assert_eq!(bank.sound(3).unwrap().pan, CENTER);
        assert_eq!(bank.decode(1).unwrap().unwrap().channels.len(), 2);
        assert_eq!(bank.decode(2).unwrap().unwrap().channels.len(), 1);
        assert_eq!(pan_gains(1), (1.0, 1.0 / 64.0));
        assert_eq!(pan_gains(CENTER), (1.0, 1.0));
        assert_eq!(pan_gains(127), (0.0, 1.0));
    }

    #[test]
    fn a_sound_plays_at_its_stored_volume() {
        let mut samples = [12700i16, -12700, 0];
        apply_volume(&mut samples, 72);
        assert_eq!(samples, [7200, -7200, 0]);
        let mut full = [32767i16, -32768];
        apply_volume(&mut full, 127);
        assert_eq!(full, [32767, -32768]);
    }

    #[test]
    fn a_long_sound_plays_at_the_rate_its_length_gives() {
        // 35000 samples in 1000 ms is 35000 Hz, whatever the code says.
        assert_eq!(measured_rate(35000, 1000), Some(35000));
        // Rounding the duration to the millisecond moves the rate by under 0.1%.
        assert_eq!(measured_rate(44100, 1000), Some(44100));
        assert_eq!(measured_rate(2205, 99), Some(22273));
        assert_eq!(measured_rate(11250, 255), Some(44118));
        assert_eq!(measured_rate(0, 1000), None);
        assert_eq!(measured_rate(1000, 0), None);
    }

    #[test]
    fn an_unused_slot_is_no_sound() {
        let data = bank(&[(0xff00, 0, 1, 127, 0), (0xff00, 0x0200, 1, 127, 1)]);
        let bank = Bank::parse(&data).unwrap();
        assert_eq!(bank.numbers().collect::<Vec<_>>(), [2]);
        assert!(bank.decode(1).is_none());
    }

    #[test]
    fn an_old_bgm_takes_turns_per_channel_every_0x800_bytes() {
        // Left block: frames of +7 nibbles; right block: frames of -8.
        let frame = |nibble: u8| {
            let mut f = [nibble * 0x11; 16];
            (f[0], f[1]) = (0x0c, 0x02);
            f
        };
        let mut body = Vec::new();
        for nibble in [0x7, 0x8] {
            for _ in 0..0x800 / 16 {
                body.extend(frame(nibble));
            }
        }
        let mut data = vec![0u8; 0x800];
        data[..4].copy_from_slice(&(body.len() as u32).to_be_bytes());
        data[6..8].copy_from_slice(&48000u16.to_be_bytes());
        data[8] = 2;
        data.extend(body);
        let pcm = decode_bgm(&data).unwrap();
        assert_eq!(pcm.rate, 48000);
        assert_eq!(pcm.channels[0].len(), 0x800 / 16 * 28);
        assert!(pcm.channels[0].iter().all(|&s| s > 0));
        assert!(pcm.channels[1].iter().all(|&s| s < 0));
    }
}

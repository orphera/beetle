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
//! - `0x8000`: `u32 file count, u32, u32 scrambled`, then one 16-byte record
//!   per file: `u32 offset, u32 size, u16, u16 rate code (high byte), u32`.
//!   File data follows the records.
//!
//! BGM stream: a 0x800-byte header (`u32` fields: data start at 8, size at
//! 12, rate at 24, channels at 28, scrambled at 32, interleave at 36, volume
//! at 40 on the key sounds' scale), then
//! ADPCM frames interleaved per channel every 16 bytes.

use std::fmt;

use crate::adpcm::{self, KEY_BGM, KEY_KEYSOUND};

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
}

/// A key sound bank, borrowed from its file's bytes.
pub struct Bank<'a> {
    data: &'a [u8],
    scrambled: bool,
    files: Vec<(usize, usize, u32)>,
    /// By key sound number - 1.
    sounds: Vec<Option<KeySound>>,
}

impl<'a> Bank<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, SoundError> {
        let count = u32_at(data, TABLE_END)? as usize;
        let scrambled = u32_at(data, TABLE_END + 8)? != 0;
        let records = TABLE_END + 0x10;
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
                }
            });
            sounds.push(sound);
        }
        Ok(Bank {
            data,
            scrambled,
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
        Some((self.data.get(offset..offset + size)?, self.scrambled))
    }

    /// One file decoded: its rate and samples.
    pub fn decode_file(&self, file: u16) -> Result<(u32, Vec<i16>), SoundError> {
        let &(offset, size, rate) = self.files.get(file as usize).ok_or(SoundError::TooShort)?;
        let bytes = self
            .data
            .get(offset..offset + size)
            .ok_or(SoundError::TooShort)?;
        let key = self.scrambled.then_some(KEY_KEYSOUND);
        Ok((rate, adpcm::decode(bytes, key, true)))
    }

    /// Key sound `n` decoded at its volume, both channels for a stereo sound
    /// or a mono one off center. `None` for an empty slot.
    pub fn decode(&self, n: u16) -> Option<Result<Pcm, SoundError>> {
        let sound = self.sound(n)?;
        Some((|| {
            let (rate, left) = self.decode_file(sound.file)?;
            let mut channels = vec![left];
            if let Placement::Stereo { right } = sound.placement {
                channels.push(self.decode_file(right)?.1);
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

/// A BGM stream decoded.
pub fn decode_bgm(data: &[u8]) -> Result<Pcm, SoundError> {
    let start = u32_at(data, 8)? as usize;
    let size = u32_at(data, 12)? as usize;
    let rate = u32_at(data, 24)?;
    let count = (u32_at(data, 28)? as usize).max(1);
    let scrambled = u32_at(data, 32)? != 0;
    let interleave = u32_at(data, 36)?;
    if interleave != 16 {
        return Err(SoundError::BadInterleave(interleave));
    }
    let volume = u32_at(data, 40)?.min(127) as u8;
    let body = data
        .get(start..(start + size).min(data.len()))
        .ok_or(SoundError::TooShort)?;
    let key = scrambled.then_some(KEY_BGM);
    let channels = (0..count)
        .map(|c| {
            let frames: Vec<u8> = body
                .chunks_exact(16)
                .skip(c)
                .step_by(count)
                .flatten()
                .copied()
                .collect();
            let mut samples = adpcm::decode(&frames, key, false);
            apply_volume(&mut samples, volume);
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
            frame[2] = KEY_KEYSOUND;
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
    fn an_unused_slot_is_no_sound() {
        let data = bank(&[(0xff00, 0, 1, 127, 0), (0xff00, 0x0200, 1, 127, 1)]);
        let bank = Bank::parse(&data).unwrap();
        assert_eq!(bank.numbers().collect::<Vec<_>>(), [2]);
        assert!(bank.decode(1).is_none());
    }
}

//! The `.ply` chart format of the PS2 IIDX games from 7th style on
//! (`charttool/plugins/iidx7thcs.py` in iidx-ps2tools).
//!
//! Header: `u32 first_event_offset`, then (unless that offset is 4) `u32
//! timestamp_multiplier`. Events are 8 bytes: `u32 time, u8 c1, u8 c2, u16
//! param`; the low nibble of `c1` is the command, the high nibble its slot.
//! An event's time in milliseconds is `time * multiplier / 1000`.

use std::fmt;

/// The multiplier when the header has none.
const DEFAULT_MULTIPLIER: u32 = 0x414c;
/// A time that ends the event list.
const END_MARK: u32 = 0x7fff;

/// Which side of the cabinet an event belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Side {
    P1,
    P2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// A note on `lane` (0..=6 keys, 7 scratch). `length` is `param`
    /// (non-zero for a long/backspin note, in the same time unit).
    Note {
        side: Side,
        lane: u8,
        length: u16,
    },
    /// From now on, `lane` sounds `sound` when hit.
    Sample {
        side: Side,
        lane: u8,
        sound: u16,
    },
    /// Tempo in beats per minute: the slot nibble is its high byte and `c2`
    /// its low byte (SAKURA's 300 is slot 1, `c2` 44).
    Bpm {
        bpm: u16,
    },
    TimeSignature {
        numerator: u8,
        denominator: u16,
    },
    End,
    /// Sound `sound` plays by itself (background).
    Auto {
        slot: u8,
        sound: u16,
    },
    /// Judgement timing: `value` is signed.
    Timing {
        slot: u8,
        value: i8,
    },
    /// A bar line.
    Measure,
    /// Commands the game knows but this reader does not use (0x0b, 0x10).
    Other {
        c1: u8,
        c2: u8,
        param: u16,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    /// The raw time field.
    pub time: u32,
    pub command: Command,
}

/// A parsed chart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ply {
    pub multiplier: u32,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlyError {
    TooShort,
    UnknownCommand { at: usize, c1: u8 },
}

impl fmt::Display for PlyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlyError::TooShort => write!(f, "chart is too short"),
            PlyError::UnknownCommand { at, c1 } => {
                write!(f, "unknown chart command {c1:#04x} at byte {at:#x}")
            }
        }
    }
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

impl Ply {
    pub fn parse(bytes: &[u8]) -> Result<Self, PlyError> {
        let first = u32_at(bytes, 0).ok_or(PlyError::TooShort)? as usize;
        let multiplier = if first == 4 {
            DEFAULT_MULTIPLIER
        } else {
            u32_at(bytes, 4).ok_or(PlyError::TooShort)?
        };
        let mut events = Vec::new();
        let mut at = first;
        // The list ends at the end mark, at an `End` event, or when the bytes
        // run out (charts have trailing bytes after `End`).
        while let Some(time) = u32_at(bytes, at) {
            if time == END_MARK {
                break;
            }
            let Some(&[c1, c2, p0, p1]) = bytes.get(at + 4..at + 8) else {
                break;
            };
            let param = u16::from_le_bytes([p0, p1]);
            let slot = c1 >> 4;
            let side = |odd: bool| if odd { Side::P2 } else { Side::P1 };
            let command = match c1 & 0x0f {
                c @ (0x0 | 0x1) => Command::Note {
                    side: side(c == 1),
                    lane: slot,
                    length: param,
                },
                c @ (0x2 | 0x3) => Command::Sample {
                    side: side(c == 3),
                    lane: slot,
                    sound: param,
                },
                0x4 => Command::Bpm {
                    bpm: (slot as u16) << 8 | c2 as u16,
                },
                0x5 => Command::TimeSignature {
                    numerator: c2,
                    denominator: param,
                },
                0x6 => Command::End,
                0x7 => Command::Auto { slot, sound: param },
                0x8 => Command::Timing {
                    slot,
                    value: c2 as i8,
                },
                0xc => Command::Measure,
                0xb => Command::Other { c1, c2, param },
                _ => return Err(PlyError::UnknownCommand { at, c1 }),
            };
            events.push(Event { time, command });
            at += 8;
            if command == Command::End {
                break;
            }
        }
        Ok(Ply { multiplier, events })
    }

    /// An event time in milliseconds.
    pub fn millis(&self, time: u32) -> f64 {
        time as f64 * self.multiplier as f64 / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(time: u32, c1: u8, c2: u8, param: u16) -> Vec<u8> {
        let mut e = time.to_le_bytes().to_vec();
        e.extend([c1, c2]);
        e.extend(param.to_le_bytes());
        e
    }

    #[test]
    fn events_are_read_until_end() {
        let mut bytes = vec![8, 0, 0, 0, 0x2b, 0x41, 0, 0];
        bytes.extend(event(0, 0x32, 0, 17)); // 1P lane 3 sounds 17
        bytes.extend(event(100, 0x30, 0, 0)); // 1P note lane 3
        bytes.extend(event(100, 0x71, 0, 25)); // 2P note lane 7, length 25
        bytes.extend(event(100, 0x14, 44, 0)); // tempo 300
        bytes.extend(event(200, 0x0c, 0, 0));
        bytes.extend(event(300, 0x06, 0, 0));
        bytes.extend([0, 0, 0, 0]);
        let ply = Ply::parse(&bytes).unwrap();
        assert_eq!(ply.multiplier, 0x412b);
        let commands: Vec<Command> = ply.events.iter().map(|e| e.command).collect();
        assert_eq!(
            commands,
            [
                Command::Sample {
                    side: Side::P1,
                    lane: 3,
                    sound: 17
                },
                Command::Note {
                    side: Side::P1,
                    lane: 3,
                    length: 0
                },
                Command::Note {
                    side: Side::P2,
                    lane: 7,
                    length: 25
                },
                Command::Bpm { bpm: 300 },
                Command::Measure,
                Command::End,
            ]
        );
        assert!((ply.millis(1000) - 16683.0).abs() < 1e-9);
    }

    #[test]
    fn an_unknown_command_is_an_error() {
        let mut bytes = vec![8, 0, 0, 0, 0x2b, 0x41, 0, 0];
        bytes.extend(event(0, 0x0d, 0, 0));
        assert_eq!(
            Ply::parse(&bytes),
            Err(PlyError::UnknownCommand { at: 8, c1: 0x0d })
        );
    }
}

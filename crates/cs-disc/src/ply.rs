//! IIDX chart event lists, in the layouts the versions use (`ChartFormat`).
//! All share one set of commands (notes, sample changes, tempo, meter, end,
//! background sounds, judgement timing, bar lines); they differ in the event
//! size, how the command and its slot are packed, the tempo encoding and the
//! time unit.
//!
//! - PS2 (iidx-ps2tools `charttool/plugins/iidx3rdcs.py`, `iidx5thcs.py`,
//!   `iidx7thcs.py`): the low nibble of `c1` is the command, the high nibble
//!   its slot; times are in units of `multiplier / 1000` ms (0x412b, about one
//!   59.94 Hz frame, on the 14 GOLD to 16th style discs; 0x414c when the chart
//!   has no header). The list ends at time 0x7fff. Unlike arcade GOLD's
//!   60.046 Hz ticks, every GOLD disc chart stores 0x412b, and its bar lines
//!   against its tempo measure 59.94 frames a second (median of 89 songs).
//!   - 3rd, 4th style: no header, 4-byte events `u16 time, u8 c1, u8 value`.
//!   - 5th, 6th style: `u32 first event, u32 multiplier` (only the first when
//!     it is 4), then 4-byte events.
//!   - 7th to 16th style: that header, then 8-byte events
//!     `u32 time, u8 c1, u8 c2, u16 value` (`.ply`).
//! - Arcade `.1` (SaxxonPike, rhythm-game-formats `iidx/1.md`): one chart of
//!   the file, 8-byte events `i32 tick, u8 type, u8 parameter, i16 value`,
//!   ticks per second by version (59.94 before GOLD, 60.046 on GOLD, 1000
//!   after), tempo `value / parameter`. The list ends at tick 0x7fffffff.
//!
//! Only the 7th–16th layout is checked against real discs (IIDX 16); the
//! others follow those sources and their unit tests.

use std::fmt;

/// The multiplier when the header has none.
const DEFAULT_MULTIPLIER: u32 = 0x414c;
/// A PS2 time that ends the event list.
const END_MARK: u32 = 0x7fff;
/// An arcade tick that ends the event list.
const ARCADE_END_MARK: i32 = 0x7fff_ffff;

/// How a version lays out its charts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChartFormat {
    /// PS2 3rd and 4th style.
    Ps2NoHeader,
    /// PS2 5th and 6th style.
    Ps2Short,
    /// PS2 7th to 16th style (`.ply`).
    Ps2Long,
    /// One chart of an arcade `.1` file.
    Arcade { ticks_per_second: f64 },
    /// A pop'n music PS2 chart file, read by `popn` rather than `Ply::parse_as`.
    Popn,
}

impl ChartFormat {
    /// The `.1` time unit of arcade version `version` (9 = 9th style, 14 = GOLD).
    pub fn arcade(version: u32) -> Self {
        let ticks_per_second = match version {
            ..=13 => 59.94,
            14 => 60.046,
            _ => 1000.0,
        };
        ChartFormat::Arcade { ticks_per_second }
    }
}

/// Which side of the cabinet an event belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Side {
    P1,
    P2,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    /// A note on `lane` (0..=6 keys, 7 scratch). `length` is `param`
    /// (non-zero for a long/backspin note, in the same time unit).
    ///
    /// At time 0 on a PS2 disc it is not a note: lane 0 notes there carry
    /// the side's note count in `c2`, 250 at a time (2HO SP HYPER: 250, 250,
    /// 112 for 612 notes), like the arcade's note count event (0x10). True
    /// of every chart on GOLD, DJ TROOPERS and both EMPRESS discs.
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
    /// Tempo in beats per minute. PS2: the slot nibble is its high byte and
    /// `c2` its low byte (SAKURA's 300 is slot 1, `c2` 44). Arcade: value /
    /// parameter (100 for fractions).
    Bpm {
        bpm: f64,
    },
    TimeSignature {
        numerator: u8,
        denominator: u16,
    },
    End,
    /// Sound `sound` plays by itself (background). `slot` is its pan, as the
    /// arcade's parameter: 1 left … 8 center … 15 right. Nearly all are 8;
    /// some songs use 0 throughout (meaning unknown, likely center).
    Auto {
        slot: u8,
        sound: u16,
    },
    /// Judgement window bounds at time 0, slots 0-5, in frames (default
    /// -16, -6, -1, 3, 8, 18; many charts differ). See
    /// `docs/specs/iidx_ps2_disc.md`.
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Event {
    /// The raw time field.
    pub time: u32,
    pub command: Command,
}

/// A parsed chart.
#[derive(Debug, Clone, PartialEq)]
pub struct Ply {
    /// Milliseconds per time unit.
    pub tick_ms: f64,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlyError {
    TooShort,
    UnknownCommand {
        at: usize,
        c1: u8,
    },
    /// A format `Ply::parse_as` does not read (`ChartFormat::Popn`).
    OtherFormat,
}

impl fmt::Display for PlyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlyError::TooShort => write!(f, "chart is too short"),
            PlyError::OtherFormat => write!(f, "chart is not a .ply or .1 chart"),
            PlyError::UnknownCommand { at, c1 } => {
                write!(f, "unknown chart command {c1:#04x} at byte {at:#x}")
            }
        }
    }
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// The command of a PS2 event: `c1` packs it (low nibble) with its slot.
fn ps2_command(c1: u8, c2: u8, param: u16, at: usize) -> Result<Command, PlyError> {
    let slot = c1 >> 4;
    let side = |odd: bool| if odd { Side::P2 } else { Side::P1 };
    Ok(match c1 & 0x0f {
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
            bpm: ((slot as u16) << 8 | c2 as u16) as f64,
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
    })
}

/// The command of an arcade event: a whole type byte, its parameter, a value.
fn arcade_command(kind: u8, p: u8, value: i16, at: usize) -> Result<Command, PlyError> {
    let side = |odd: bool| if odd { Side::P2 } else { Side::P1 };
    Ok(match kind {
        0x00 | 0x01 => Command::Note {
            side: side(kind == 1),
            lane: p,
            length: value as u16,
        },
        0x02 | 0x03 => Command::Sample {
            side: side(kind == 3),
            lane: p,
            sound: value as u16,
        },
        // A parameter of 0 is invalid; read it as 1.
        0x04 => Command::Bpm {
            bpm: value as f64 / p.max(1) as f64,
        },
        0x05 => Command::TimeSignature {
            numerator: value as u8,
            denominator: p as u16,
        },
        0x06 => Command::End,
        0x07 => Command::Auto {
            slot: p,
            sound: value as u16,
        },
        0x08 => Command::Timing {
            slot: p,
            value: value as i8,
        },
        0x0c => Command::Measure,
        0x0b | 0x10 => Command::Other {
            c1: kind,
            c2: p,
            param: value as u16,
        },
        _ => return Err(PlyError::UnknownCommand { at, c1: kind }),
    })
}

impl Ply {
    /// A PS2 7th–16th style chart (`.ply`).
    pub fn parse(bytes: &[u8]) -> Result<Self, PlyError> {
        Self::parse_as(bytes, ChartFormat::Ps2Long)
    }

    pub fn parse_as(bytes: &[u8], format: ChartFormat) -> Result<Self, PlyError> {
        let (first, multiplier) = match format {
            ChartFormat::Popn => return Err(PlyError::OtherFormat),
            ChartFormat::Ps2NoHeader => (0, DEFAULT_MULTIPLIER),
            ChartFormat::Ps2Short | ChartFormat::Ps2Long => {
                let first = u32_at(bytes, 0).ok_or(PlyError::TooShort)? as usize;
                let multiplier = if first == 4 {
                    DEFAULT_MULTIPLIER
                } else {
                    u32_at(bytes, 4).ok_or(PlyError::TooShort)?
                };
                (first, multiplier)
            }
            ChartFormat::Arcade { .. } => (0, 0),
        };
        let tick_ms = match format {
            ChartFormat::Arcade { ticks_per_second } => 1000.0 / ticks_per_second,
            _ => multiplier as f64 / 1000.0,
        };
        let size = match format {
            ChartFormat::Ps2NoHeader | ChartFormat::Ps2Short => 4,
            _ => 8,
        };
        let mut events = Vec::new();
        let mut at = first;
        // The list ends at the end mark, at an `End` event, or when the bytes
        // run out (charts have trailing bytes after `End`).
        while let Some(e) = bytes.get(at..at + size) {
            let (time, command) = match format {
                ChartFormat::Ps2NoHeader | ChartFormat::Ps2Short => {
                    let time = u16::from_le_bytes([e[0], e[1]]) as u32;
                    if time == END_MARK {
                        break;
                    }
                    // One value byte serves as both `c2` and the value.
                    (time, ps2_command(e[2], e[3], e[3] as u16, at)?)
                }
                ChartFormat::Ps2Long => {
                    let time = u32_at(e, 0).unwrap();
                    if time == END_MARK {
                        break;
                    }
                    let param = u16::from_le_bytes([e[6], e[7]]);
                    (time, ps2_command(e[4], e[5], param, at)?)
                }
                ChartFormat::Popn => unreachable!("refused above"),
                ChartFormat::Arcade { .. } => {
                    let tick = u32_at(e, 0).unwrap() as i32;
                    if tick == ARCADE_END_MARK {
                        break;
                    }
                    let value = i16::from_le_bytes([e[6], e[7]]);
                    (tick.max(0) as u32, arcade_command(e[4], e[5], value, at)?)
                }
            };
            events.push(Event { time, command });
            at += size;
            if command == Command::End {
                break;
            }
        }
        Ok(Ply { tick_ms, events })
    }

    /// An event time in milliseconds.
    pub fn millis(&self, time: u32) -> f64 {
        time as f64 * self.tick_ms
    }
}

/// The charts of an arcade `.1` file: (directory index, bytes) for each
/// directory entry that has one. The directory is 12 `(offset, length)`
/// pairs; which index is which difficulty depends on the version.
pub fn arcade_charts(file: &[u8]) -> Vec<(usize, &[u8])> {
    (0..12)
        .filter_map(|i| {
            let offset = u32_at(file, i * 8)? as usize;
            let length = u32_at(file, i * 8 + 4)? as usize;
            (offset != 0 && length != 0)
                .then(|| file.get(offset..offset + length))
                .flatten()
                .map(|chart| (i, chart))
        })
        .collect()
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
        assert!((ply.tick_ms - 16.683).abs() < 1e-9);
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
                Command::Bpm { bpm: 300.0 },
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

    #[test]
    fn third_style_charts_have_no_header_and_short_events() {
        // u16 time, c1, value: tempo 150 at 0, 1P lane 2 note at 10, end.
        let bytes = [0, 0, 0x04, 150, 10, 0, 0x20, 0, 0xff, 0x7f, 0, 0];
        let ply = Ply::parse_as(&bytes, ChartFormat::Ps2NoHeader).unwrap();
        assert!((ply.tick_ms - 0x414c as f64 / 1000.0).abs() < 1e-9);
        assert_eq!(ply.events.len(), 2);
        assert_eq!(ply.events[0].command, Command::Bpm { bpm: 150.0 });
        assert_eq!(ply.events[1].time, 10);
        assert_eq!(
            ply.events[1].command,
            Command::Note {
                side: Side::P1,
                lane: 2,
                length: 0
            }
        );
    }

    #[test]
    fn fifth_style_charts_have_the_header_and_short_events() {
        let mut bytes = vec![8, 0, 0, 0, 0x2b, 0x41, 0, 0];
        bytes.extend([5, 0, 0x0c, 0]); // bar line at 5
        bytes.extend([0xff, 0x7f, 0, 0]);
        let ply = Ply::parse_as(&bytes, ChartFormat::Ps2Short).unwrap();
        assert!((ply.tick_ms - 16.683).abs() < 1e-9);
        assert_eq!(
            ply.events,
            [Event {
                time: 5,
                command: Command::Measure
            }]
        );
    }

    #[test]
    fn arcade_charts_split_type_and_parameter() {
        let mut chart = Vec::new();
        for (tick, kind, p, value) in [
            (0i32, 0x04u8, 100u8, 15050i16), // 150.5 BPM
            (0, 0x02, 7, 33),                // scratch sounds 33
            (500, 0x00, 7, 0),               // scratch note
            (600, 0x01, 3, 120),             // 2P long note on key 4
            (0x7fff_ffff, 0, 0, 0),
        ] {
            chart.extend(tick.to_le_bytes());
            chart.extend([kind, p]);
            chart.extend(value.to_le_bytes());
        }
        let mut file = vec![0u8; 96];
        file[8..12].copy_from_slice(&96u32.to_le_bytes()); // entry 1
        file[12..16].copy_from_slice(&(chart.len() as u32).to_le_bytes());
        file.extend(&chart);
        let charts = arcade_charts(&file);
        assert_eq!(charts.len(), 1);
        assert_eq!(charts[0].0, 1);
        let ply = Ply::parse_as(charts[0].1, ChartFormat::arcade(15)).unwrap();
        assert_eq!(ply.tick_ms, 1.0);
        assert_eq!(ply.events[0].command, Command::Bpm { bpm: 150.5 });
        assert_eq!(
            ply.events[3].command,
            Command::Note {
                side: Side::P2,
                lane: 3,
                length: 120
            }
        );
        assert_eq!(ply.events.len(), 4);
    }

    #[test]
    fn arcade_tick_rates_follow_the_version() {
        let rate = |v| match ChartFormat::arcade(v) {
            ChartFormat::Arcade { ticks_per_second } => ticks_per_second,
            _ => unreachable!(),
        };
        assert_eq!(rate(9), 59.94);
        assert_eq!(rate(14), 60.046);
        assert_eq!(rate(17), 1000.0);
    }
}

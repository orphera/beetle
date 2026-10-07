use crate::bms::Lane;
use crate::score::ScoreRecord;
use std::fmt::Write;

/// A single timestamped key input event in a replay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReplayEvent {
    pub time_seconds: f64,
    pub lane: Lane,
    pub is_down: bool,
}

/// Recorded replay data for a chart playthrough.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayData {
    pub chart_hash: u64,
    pub ex_score: u32,
    pub max_combo: u32,
    /// Seed the chart's `#RANDOM` sections were rolled with (`None` for charts without any).
    pub random_seed: Option<u64>,
    pub events: Vec<ReplayEvent>,
}

impl ReplayData {
    pub fn new(chart_hash: u64) -> Self {
        Self {
            chart_hash,
            ex_score: 0,
            max_combo: 0,
            random_seed: None,
            events: Vec::new(),
        }
    }

    pub fn record(&mut self, time_seconds: f64, lane: Lane, is_down: bool) {
        self.events.push(ReplayEvent {
            time_seconds,
            lane,
            is_down,
        });
    }

    pub fn set_score(&mut self, record: &ScoreRecord) {
        self.ex_score = record.ex_score;
        self.max_combo = record.max_combo;
    }

    /// Serializes replay into compact flat string format.
    pub fn serialize_to_string(&self) -> String {
        let mut buf = String::with_capacity(64 + self.events.len() * 24);
        let _ = writeln!(buf, "#BEETLE_REPLAY_V1");
        let _ = writeln!(buf, "hash={:016x}", self.chart_hash);
        let _ = writeln!(buf, "ex_score={}", self.ex_score);
        let _ = writeln!(buf, "max_combo={}", self.max_combo);
        if let Some(seed) = self.random_seed {
            let _ = writeln!(buf, "seed={seed:016x}");
        }
        let _ = writeln!(buf, "#EVENTS");

        for ev in &self.events {
            let lane_idx = match ev.lane {
                Lane::Scratch => 0,
                Lane::Key1 => 1,
                Lane::Key2 => 2,
                Lane::Key3 => 3,
                Lane::Key4 => 4,
                Lane::Key5 => 5,
                Lane::Key6 => 6,
                Lane::Key7 => 7,
                // Appended after the original 8 values (INVARIANT in
                // bms.rs::Lane) so existing 5K/7K replays stay byte-stable.
                Lane::Key8 => 8,
                Lane::Key9 => 9,
                Lane::P2Scratch => 10,
                Lane::P2Key1 => 11,
                Lane::P2Key2 => 12,
                Lane::P2Key3 => 13,
                Lane::P2Key4 => 14,
                Lane::P2Key5 => 15,
                Lane::P2Key6 => 16,
                Lane::P2Key7 => 17,
            };
            let action = if ev.is_down { 'D' } else { 'U' };
            let _ = writeln!(buf, "{:.4}\t{}\t{}", ev.time_seconds, lane_idx, action);
        }

        buf
    }

    /// Parses replay from serialized string.
    pub fn parse_from_str(data: &str) -> Option<Self> {
        let mut lines = data.lines();
        let first = lines.next()?.trim();
        if first != "#BEETLE_REPLAY_V1" {
            return None;
        }

        let mut chart_hash = 0;
        let mut ex_score = 0;
        let mut max_combo = 0;
        let mut random_seed = None;
        let mut in_events = false;
        let mut events = Vec::new();

        for line in lines {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if line == "#EVENTS" {
                in_events = true;
                continue;
            }

            if !in_events {
                let parts: Vec<&str> = line.splitn(2, '=').collect();
                if parts.len() == 2 {
                    let key = parts[0].trim();
                    let val = parts[1].trim();
                    match key {
                        "hash" => chart_hash = u64::from_str_radix(val, 16).unwrap_or(0),
                        "ex_score" => ex_score = val.parse::<u32>().unwrap_or(0),
                        "max_combo" => max_combo = val.parse::<u32>().unwrap_or(0),
                        "seed" => random_seed = u64::from_str_radix(val, 16).ok(),
                        _ => (),
                    }
                }
            } else {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() == 3 {
                    let time = parts[0].parse::<f64>().unwrap_or(0.0);
                    let lane_idx = parts[1].parse::<u8>().unwrap_or(0);
                    let is_down = parts[2] == "D";

                    let lane = match lane_idx {
                        0 => Lane::Scratch,
                        1 => Lane::Key1,
                        2 => Lane::Key2,
                        3 => Lane::Key3,
                        4 => Lane::Key4,
                        5 => Lane::Key5,
                        6 => Lane::Key6,
                        7 => Lane::Key7,
                        8 => Lane::Key8,
                        9 => Lane::Key9,
                        10 => Lane::P2Scratch,
                        11 => Lane::P2Key1,
                        12 => Lane::P2Key2,
                        13 => Lane::P2Key3,
                        14 => Lane::P2Key4,
                        15 => Lane::P2Key5,
                        16 => Lane::P2Key6,
                        _ => Lane::P2Key7,
                    };

                    events.push(ReplayEvent {
                        time_seconds: time,
                        lane,
                        is_down,
                    });
                }
            }
        }

        Some(Self {
            chart_hash,
            ex_score,
            max_combo,
            random_seed,
            events,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replay_serialization_roundtrip() {
        let mut replay = ReplayData::new(0x123456789ABCDEF0);
        replay.ex_score = 1520;
        replay.max_combo = 850;
        replay.random_seed = Some(0xfeed_beef_1234);
        replay.record(1.2345, Lane::Key1, true);
        replay.record(1.3456, Lane::Key1, false);
        replay.record(2.0000, Lane::Scratch, true);
        replay.record(2.1000, Lane::Scratch, false);

        let serialized = replay.serialize_to_string();
        let parsed = ReplayData::parse_from_str(&serialized).expect("Failed to parse replay");

        assert_eq!(replay.chart_hash, parsed.chart_hash);
        assert_eq!(replay.ex_score, parsed.ex_score);
        assert_eq!(replay.max_combo, parsed.max_combo);
        assert_eq!(replay.random_seed, parsed.random_seed);
        assert_eq!(replay.events.len(), parsed.events.len());
        assert_eq!(replay.events[0].lane, parsed.events[0].lane);
        assert_eq!(replay.events[0].is_down, parsed.events[0].is_down);
        assert!((replay.events[0].time_seconds - parsed.events[0].time_seconds).abs() < 0.001);
    }
}

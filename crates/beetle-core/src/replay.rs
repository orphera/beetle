use crate::bms::Lane;
use crate::identity::{ChartId, ChartKey};
use crate::judge::GaugeType;
use crate::modifier::LaneModifier;
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
    /// The chart it was played on: its id, or the old key in a replay from before chart ids.
    pub chart: ChartKey,
    pub ex_score: u32,
    pub max_combo: u32,
    /// Seed the chart's `#RANDOM` sections were rolled with (`None` for charts without any).
    pub random_seed: Option<u64>,
    /// Lane modifier and gauge the play was made with (`None` in older files).
    pub modifier: Option<LaneModifier>,
    pub gauge: Option<GaugeType>,
    pub events: Vec<ReplayEvent>,
}

impl ReplayData {
    pub fn new(chart: ChartId) -> Self {
        Self {
            chart: ChartKey::Id(chart),
            ex_score: 0,
            max_combo: 0,
            random_seed: None,
            modifier: None,
            gauge: None,
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

    pub fn set_score(&mut self, ex_score: u32, max_combo: u32) {
        self.ex_score = ex_score;
        self.max_combo = max_combo;
    }

    /// Serializes replay into compact flat string format.
    pub fn serialize_to_string(&self) -> String {
        let mut buf = String::with_capacity(64 + self.events.len() * 24);
        let _ = writeln!(buf, "#BEETLE_REPLAY_V1");
        let _ = writeln!(buf, "chart={}", self.chart);
        let _ = writeln!(buf, "ex_score={}", self.ex_score);
        let _ = writeln!(buf, "max_combo={}", self.max_combo);
        if let Some(seed) = self.random_seed {
            let _ = writeln!(buf, "seed={seed:016x}");
        }
        if let Some(modifier) = self.modifier {
            let _ = writeln!(buf, "modifier={}", modifier.as_str());
        }
        if let Some(gauge) = self.gauge {
            let _ = writeln!(buf, "gauge={}", gauge.as_str());
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

        let mut chart = ChartKey::default();
        let mut ex_score = 0;
        let mut max_combo = 0;
        let mut random_seed = None;
        let mut modifier = None;
        let mut gauge = None;
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
                        // `hash=` is the header replays had before chart ids.
                        "hash" => {
                            chart = u64::from_str_radix(val, 16).map_or(chart, ChartKey::Legacy)
                        }
                        "chart" => chart = ChartKey::parse(val).unwrap_or(chart),
                        "ex_score" => ex_score = val.parse::<u32>().unwrap_or(0),
                        "max_combo" => max_combo = val.parse::<u32>().unwrap_or(0),
                        "seed" => random_seed = u64::from_str_radix(val, 16).ok(),
                        "modifier" => modifier = LaneModifier::from_name(val),
                        "gauge" => gauge = GaugeType::from_name(val),
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
            chart,
            ex_score,
            max_combo,
            random_seed,
            modifier,
            gauge,
            events,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replays_from_before_chart_ids_still_read() {
        let old = "#BEETLE_REPLAY_V1
hash=00000000000000ab
ex_score=10
max_combo=5
#EVENTS
1.0000	1	D
";
        let replay = ReplayData::parse_from_str(old).expect("old replay parses");
        assert_eq!(replay.chart, ChartKey::Legacy(0xab));
        assert_eq!(replay.events.len(), 1);
    }

    #[test]
    fn test_replay_serialization_roundtrip() {
        let mut replay = ReplayData::new(ChartId::synthetic(0x1234));
        replay.ex_score = 1520;
        replay.max_combo = 850;
        replay.random_seed = Some(0xfeed_beef_1234);
        replay.modifier = Some(LaneModifier::Mirror);
        replay.gauge = Some(GaugeType::Hard);
        replay.record(1.2345, Lane::Key1, true);
        replay.record(1.3456, Lane::Key1, false);
        replay.record(2.0000, Lane::Scratch, true);
        replay.record(2.1000, Lane::Scratch, false);

        let serialized = replay.serialize_to_string();
        let parsed = ReplayData::parse_from_str(&serialized).expect("Failed to parse replay");

        assert_eq!(replay.chart, parsed.chart);
        assert_eq!(replay.ex_score, parsed.ex_score);
        assert_eq!(replay.max_combo, parsed.max_combo);
        assert_eq!(replay.random_seed, parsed.random_seed);
        assert_eq!(replay.modifier, parsed.modifier);
        assert_eq!(replay.gauge, parsed.gauge);
        assert_eq!(replay.events.len(), parsed.events.len());
        assert_eq!(replay.events[0].lane, parsed.events[0].lane);
        assert_eq!(replay.events[0].is_down, parsed.events[0].is_down);
        assert!((replay.events[0].time_seconds - parsed.events[0].time_seconds).abs() < 0.001);
    }
}

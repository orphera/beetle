use crate::bms::Lane;
use crate::identity::{ChartId, ChartKey};
use crate::judge::GaugeType;
use crate::modifier::LaneModifier;
use crate::msgpack::{self, Value, Writer};
use crate::rules::LnRule;

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
    /// The long note rule it was played under (`None` for a chart without long notes, or an older file).
    pub ln: Option<LnRule>,
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
            ln: None,
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

    /// Writes the replay file (see `msgpack`). The events are one binary
    /// field of 5-byte records: the time in microseconds as a little-endian
    /// `i32`, then the lane index with the top bit set for a press.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.file(REPLAY_FORMAT, REPLAY_VERSION, 8);
        w.str("chart");
        self.chart.write_msgpack(&mut w);
        w.str("ex");
        w.uint(u64::from(self.ex_score));
        w.str("combo");
        w.uint(u64::from(self.max_combo));
        w.str("seed");
        w.opt_uint(self.random_seed);
        w.str("mod");
        w.opt_str(self.modifier.map(LaneModifier::as_str));
        w.str("gauge");
        w.opt_str(self.gauge.map(GaugeType::as_str));
        w.str("ln");
        w.opt_str(self.ln.map(LnRule::as_str));
        let mut events = Vec::with_capacity(self.events.len() * 5);
        for ev in &self.events {
            let micros = (ev.time_seconds * 1_000_000.0).round() as i32;
            events.extend_from_slice(&micros.to_le_bytes());
            events.push(lane_index(ev.lane) | if ev.is_down { 0x80 } else { 0 });
        }
        w.str("events");
        w.bin(&events);
        w.into_bytes()
    }

    /// Reads a replay file: the current format, or the text one replays
    /// were saved in before (they are rewritten when the replay is replaced).
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        match msgpack::decode_file(bytes, REPLAY_FORMAT, REPLAY_VERSION) {
            Some(file) => Self::from_file(&file),
            None => Self::parse_text(std::str::from_utf8(bytes).ok()?),
        }
    }

    fn from_file(file: &Value) -> Option<Self> {
        let name = |key| file.get(key).and_then(Value::as_str);
        let events = file
            .get("events")?
            .as_bin()?
            .chunks_exact(5)
            .map(|rec| ReplayEvent {
                time_seconds: f64::from(i32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]))
                    / 1_000_000.0,
                lane: lane_from_index(rec[4] & 0x7f),
                is_down: rec[4] & 0x80 != 0,
            })
            .collect();
        Some(Self {
            chart: ChartKey::read_msgpack(file.get("chart")?)?,
            ex_score: file.get("ex").and_then(Value::as_u32).unwrap_or(0),
            max_combo: file.get("combo").and_then(Value::as_u32).unwrap_or(0),
            random_seed: file.get("seed").and_then(Value::as_u64),
            modifier: name("mod").and_then(LaneModifier::from_name),
            gauge: name("gauge").and_then(GaugeType::from_name),
            ln: name("ln").and_then(LnRule::from_name),
            events,
        })
    }

    /// Reads a replay in the text format of earlier builds.
    fn parse_text(data: &str) -> Option<Self> {
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
        let mut ln = None;
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
                        "ln" => ln = LnRule::from_name(val),
                        _ => (),
                    }
                }
            } else {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() == 3 {
                    let time = parts[0].parse::<f64>().unwrap_or(0.0);
                    let lane_idx = parts[1].parse::<u8>().unwrap_or(0);
                    let is_down = parts[2] == "D";

                    let lane = lane_from_index(lane_idx);

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
            ln,
            events,
        })
    }
}

const REPLAY_FORMAT: &str = "beetle-replay";
const REPLAY_VERSION: u64 = 2;

/// The lane's number in a replay. Lanes past Key7 were appended after the
/// original 8 (INVARIANT in bms.rs::Lane), so the first numbers never change.
fn lane_index(lane: Lane) -> u8 {
    match lane {
        Lane::Scratch => 0,
        Lane::Key1 => 1,
        Lane::Key2 => 2,
        Lane::Key3 => 3,
        Lane::Key4 => 4,
        Lane::Key5 => 5,
        Lane::Key6 => 6,
        Lane::Key7 => 7,
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
    }
}

fn lane_from_index(index: u8) -> Lane {
    match index {
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
        let replay = ReplayData::from_bytes(old.as_bytes()).expect("old replay parses");
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
        replay.ln = Some(LnRule::Cn);
        replay.record(1.2345, Lane::Key1, true);
        replay.record(1.3456, Lane::Key1, false);
        replay.record(2.0000, Lane::Scratch, true);
        replay.record(2.1000, Lane::Scratch, false);

        let bytes = replay.to_bytes();
        let parsed = ReplayData::from_bytes(&bytes).expect("Failed to read replay");

        assert_eq!(replay.chart, parsed.chart);
        assert_eq!(replay.ex_score, parsed.ex_score);
        assert_eq!(replay.max_combo, parsed.max_combo);
        assert_eq!(replay.random_seed, parsed.random_seed);
        assert_eq!(replay.modifier, parsed.modifier);
        assert_eq!(replay.gauge, parsed.gauge);
        assert_eq!(replay.ln, parsed.ln);
        assert_eq!(replay.events.len(), parsed.events.len());
        assert_eq!(replay.events[0].lane, parsed.events[0].lane);
        assert_eq!(replay.events[0].is_down, parsed.events[0].is_down);
        assert!((replay.events[0].time_seconds - parsed.events[0].time_seconds).abs() < 1e-6);
        assert_eq!(parsed.events[2].lane, Lane::Scratch);
        assert!(!parsed.events[3].is_down);
    }

    #[test]
    fn old_keys_and_lead_in_presses_round_trip() {
        let mut replay = ReplayData::new(ChartId::synthetic(1));
        replay.chart = ChartKey::Legacy(0xdead_beef);
        replay.record(-0.25, Lane::P2Key7, true);
        let parsed = ReplayData::from_bytes(&replay.to_bytes()).unwrap();
        assert_eq!(parsed, replay);
    }
}

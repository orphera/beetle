use std::collections::HashMap;

/// BMS `#WAVxx` sound identifier (Base36).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WavId(pub u16);

impl WavId {
    pub const fn new(id: u16) -> Self {
        Self(id)
    }

    pub const fn as_u16(self) -> u16 {
        self.0
    }
}

/// BMS `#BMPxx` picture/bga identifier (Base36).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BmpId(pub u16);

/// Key lanes supported by Beetle (7 Keys + 1 Scratch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Lane {
    Scratch,
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
}

/// Note type (tap, long note endpoints, landmine).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteType {
    Tap,
    LongNoteStart,
    LongNoteEnd,
    Landmine,
}

/// A parsed note event placed on the timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct NoteEvent {
    pub measure: u32,
    pub fraction: f64,
    pub lane: Lane,
    pub wav_id: Option<WavId>,
    pub note_type: NoteType,
}

/// Header metadata extracted from BMS command lines.
#[derive(Debug, Clone)]
pub struct BmsHeader {
    pub player: u32,
    pub genre: String,
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub subartist: String,
    pub bpm: f64,
    pub play_level: u32,
    pub rank: u32,
    pub total: f64,
    pub vol_wav: f64,
    pub stage_file: String,
    pub banner: String,
    pub ln_obj: Option<WavId>,
    pub wav_table: HashMap<WavId, String>,
    pub bmp_table: HashMap<BmpId, String>,
    pub bpm_table: HashMap<WavId, f64>,
    pub stop_table: HashMap<WavId, f64>,
}

impl Default for BmsHeader {
    fn default() -> Self {
        Self {
            player: 1,
            genre: String::new(),
            title: String::new(),
            subtitle: String::new(),
            artist: String::new(),
            subartist: String::new(),
            bpm: 130.0,
            play_level: 1,
            rank: 2,
            total: 200.0,
            vol_wav: 1.0,
            stage_file: String::new(),
            banner: String::new(),
            ln_obj: None,
            wav_table: HashMap::new(),
            bmp_table: HashMap::new(),
            bpm_table: HashMap::new(),
            stop_table: HashMap::new(),
        }
    }
}

/// Timing event kind occurring at a specific measure and fraction.
#[derive(Debug, Clone, PartialEq)]
pub enum TimingEventKind {
    /// BPM change to absolute BPM value.
    BpmChange(f64),
    /// Stop event with duration in 4-beat measures (e.g. 1.0 = 4 beats = 1 measure).
    StopMeasures(f64),
}

/// Timed event on the measure timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct TimingEvent {
    pub measure: u32,
    pub fraction: f64,
    pub kind: TimingEventKind,
}

/// A fully parsed BMS chart structure.
#[derive(Debug, Clone, Default)]
pub struct BmsChart {
    pub header: BmsHeader,
    pub notes: Vec<NoteEvent>,
    pub bgm_notes: Vec<(u32, f64, WavId)>,
    pub timing_events: Vec<TimingEvent>,
    pub measure_lengths: HashMap<u32, f64>,
}

/// Errors that can occur when parsing BMS text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BmsParseError {
    EmptyChart,
    InvalidHeader(String),
    InvalidMeasure(String),
    InvalidChannel(String),
}

impl std::fmt::Display for BmsParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyChart => write!(f, "BMS chart content is empty"),
            Self::InvalidHeader(msg) => write!(f, "Invalid header command: {msg}"),
            Self::InvalidMeasure(msg) => write!(f, "Invalid measure format: {msg}"),
            Self::InvalidChannel(msg) => write!(f, "Invalid channel format: {msg}"),
        }
    }
}

impl std::error::Error for BmsParseError {}

/// Decode a 2-character Base36 string (`00`..`ZZ`) to `WavId`.
/// Returns `None` for `00` or invalid characters.
pub fn decode_base36(c1: u8, c2: u8) -> Option<WavId> {
    let d1 = match c1 {
        b'0'..=b'9' => (c1 - b'0') as u16,
        b'A'..=b'Z' => (c1 - b'A' + 10) as u16,
        b'a'..=b'z' => (c1 - b'a' + 10) as u16,
        _ => return None,
    };
    let d2 = match c2 {
        b'0'..=b'9' => (c2 - b'0') as u16,
        b'A'..=b'Z' => (c2 - b'A' + 10) as u16,
        b'a'..=b'z' => (c2 - b'a' + 10) as u16,
        _ => return None,
    };
    let val = d1 * 36 + d2;
    if val == 0 {
        None
    } else {
        Some(WavId(val))
    }
}

/// Decode a 2-character hexadecimal string (`00`..`FF`) to `u8`.
pub fn decode_hex(c1: u8, c2: u8) -> Option<u8> {
    let d1 = match c1 {
        b'0'..=b'9' => c1 - b'0',
        b'A'..=b'F' => c1 - b'A' + 10,
        b'a'..=b'f' => c1 - b'a' + 10,
        _ => return None,
    };
    let d2 = match c2 {
        b'0'..=b'9' => c2 - b'0',
        b'A'..=b'F' => c2 - b'A' + 10,
        b'a'..=b'f' => c2 - b'a' + 10,
        _ => return None,
    };
    Some(d1 * 16 + d2)
}

/// Parses raw BMS/BME/BML chart text into a `BmsChart`.
pub fn parse_bms(input: &str) -> Result<BmsChart, BmsParseError> {
    let mut chart = BmsChart::default();
    let mut raw_ln_events: Vec<(u32, f64, Lane, WavId)> = Vec::new();

    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(BmsParseError::EmptyChart);
    }

    for line in trimmed.lines() {
        let line = line.trim();
        if !line.starts_with('#') {
            continue;
        }

        let content = &line[1..].trim_start();
        if content.is_empty() {
            continue;
        }

        // Check if this line is a measure data command: #XXXYY:ZZ...
        // Form: 3 digits measure + 2 chars channel + ':'
        let bytes = content.as_bytes();
        if bytes.len() >= 6
            && bytes[0..3].iter().all(|b| b.is_ascii_digit())
            && (bytes[5] == b':' || bytes[5] == b' ')
        {
            parse_measure_line(content, &mut chart, &mut raw_ln_events)?;
        } else {
            parse_header_line(content, &mut chart.header);
        }
    }

    // Process LNTYPE 1 long notes (pairs of channel 5x events)
    process_lntype1_notes(&raw_ln_events, &mut chart.notes);

    // Process #LNOBJ long notes
    if let Some(ln_obj_id) = chart.header.ln_obj {
        process_lnobj_notes(ln_obj_id, &mut chart.notes);
    }

    // Sort notes and timing events chronologically
    chart.notes.sort_by(|a, b| {
        a.measure
            .cmp(&b.measure)
            .then_with(|| a.fraction.partial_cmp(&b.fraction).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.lane.cmp(&b.lane))
    });

    chart.bgm_notes.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    });

    chart.timing_events.sort_by(|a, b| {
        a.measure
            .cmp(&b.measure)
            .then_with(|| a.fraction.partial_cmp(&b.fraction).unwrap_or(std::cmp::Ordering::Equal))
    });

    Ok(chart)
}

fn parse_header_line(content: &str, header: &mut BmsHeader) {
    let mut parts = content.splitn(2, |c: char| c.is_whitespace() || c == ':');
    let key = parts.next().unwrap_or("").trim();
    let val = parts.next().unwrap_or("").trim();

    if key.eq_ignore_ascii_case("TITLE") {
        header.title = val.to_string();
    } else if key.eq_ignore_ascii_case("SUBTITLE") {
        header.subtitle = val.to_string();
    } else if key.eq_ignore_ascii_case("ARTIST") {
        header.artist = val.to_string();
    } else if key.eq_ignore_ascii_case("SUBARTIST") {
        header.subartist = val.to_string();
    } else if key.eq_ignore_ascii_case("GENRE") {
        header.genre = val.to_string();
    } else if key.eq_ignore_ascii_case("BPM") {
        if let Ok(bpm) = val.parse::<f64>() {
            header.bpm = bpm;
        }
    } else if key.eq_ignore_ascii_case("PLAYLEVEL") {
        if let Ok(lvl) = val.parse::<u32>() {
            header.play_level = lvl;
        }
    } else if key.eq_ignore_ascii_case("RANK") {
        if let Ok(rank) = val.parse::<u32>() {
            header.rank = rank;
        }
    } else if key.eq_ignore_ascii_case("TOTAL") {
        if let Ok(total) = val.parse::<f64>() {
            header.total = total;
        }
    } else if key.eq_ignore_ascii_case("VOLWAV") {
        if let Ok(vol) = val.parse::<f64>() {
            header.vol_wav = vol;
        }
    } else if key.eq_ignore_ascii_case("STAGEFILE") {
        header.stage_file = val.to_string();
    } else if key.eq_ignore_ascii_case("BANNER") {
        header.banner = val.to_string();
    } else if key.eq_ignore_ascii_case("LNOBJ") {
        let val_bytes = val.as_bytes();
        if val_bytes.len() >= 2 {
            header.ln_obj = decode_base36(val_bytes[0], val_bytes[1]);
        }
    } else if key.len() >= 4 && key[..3].eq_ignore_ascii_case("WAV") {
        let id_str = &key[3..];
        if id_str.len() == 2 {
            let b = id_str.as_bytes();
            if let Some(id) = decode_base36(b[0], b[1]) {
                header.wav_table.insert(id, val.to_string());
            }
        }
    } else if key.len() >= 4 && key[..3].eq_ignore_ascii_case("BMP") {
        let id_str = &key[3..];
        if id_str.len() == 2 {
            let b = id_str.as_bytes();
            if let Some(id) = decode_base36(b[0], b[1]) {
                header.bmp_table.insert(BmpId(id.0), val.to_string());
            }
        }
    } else if key.len() >= 4 && key[..3].eq_ignore_ascii_case("BPM") {
        let id_str = &key[3..];
        if id_str.len() == 2 {
            let b = id_str.as_bytes();
            if let Some(id) = decode_base36(b[0], b[1]) {
                if let Ok(bpm_val) = val.parse::<f64>() {
                    header.bpm_table.insert(id, bpm_val);
                }
            }
        }
    } else if key.len() >= 5 && key[..4].eq_ignore_ascii_case("STOP") {
        let id_str = &key[4..];
        if id_str.len() == 2 {
            let b = id_str.as_bytes();
            if let Some(id) = decode_base36(b[0], b[1]) {
                if let Ok(stop_val) = val.parse::<f64>() {
                    header.stop_table.insert(id, stop_val);
                }
            }
        }
    }
}

fn parse_measure_line(
    content: &str,
    chart: &mut BmsChart,
    raw_ln_events: &mut Vec<(u32, f64, Lane, WavId)>,
) -> Result<(), BmsParseError> {
    let mut parts = content.splitn(2, |c: char| c == ':' || c == ' ');
    let tag = parts.next().unwrap_or("").trim();
    let data = parts.next().unwrap_or("").trim();

    if tag.len() < 5 {
        return Err(BmsParseError::InvalidMeasure(tag.to_string()));
    }

    let measure: u32 = tag[0..3]
        .parse()
        .map_err(|_| BmsParseError::InvalidMeasure(tag.to_string()))?;
    let channel = &tag[3..5];

    // Channel 02: Measure length ratio (e.g. #00102:0.75)
    if channel == "02" {
        if let Ok(len) = data.parse::<f64>() {
            chart.measure_lengths.insert(measure, len);
        }
        return Ok(());
    }

    // Note / Event channels: 2 characters per slot
    let data_bytes = data.as_bytes();
    if data_bytes.len() % 2 != 0 {
        return Ok(()); // Ignore malformed slot lengths gracefully
    }

    let slot_count = data_bytes.len() / 2;
    if slot_count == 0 {
        return Ok(());
    }

    for i in 0..slot_count {
        let c1 = data_bytes[i * 2];
        let c2 = data_bytes[i * 2 + 1];
        let fraction = i as f64 / slot_count as f64;

        match channel {
            // 01: BGM Channel
            "01" => {
                if let Some(wav_id) = decode_base36(c1, c2) {
                    chart.bgm_notes.push((measure, fraction, wav_id));
                }
            }
            // 03: Direct Hex BPM change
            "03" => {
                if let Some(bpm_hex) = decode_hex(c1, c2) {
                    if bpm_hex > 0 {
                        chart.timing_events.push(TimingEvent {
                            measure,
                            fraction,
                            kind: TimingEventKind::BpmChange(bpm_hex as f64),
                        });
                    }
                }
            }
            // 08: Extended BPM change via #BPMxx
            "08" => {
                if let Some(id) = decode_base36(c1, c2) {
                    if let Some(&bpm_val) = chart.header.bpm_table.get(&id) {
                        chart.timing_events.push(TimingEvent {
                            measure,
                            fraction,
                            kind: TimingEventKind::BpmChange(bpm_val),
                        });
                    }
                }
            }
            // 09: STOP event via #STOPxx
            "09" => {
                if let Some(id) = decode_base36(c1, c2) {
                    if let Some(&stop_units) = chart.header.stop_table.get(&id) {
                        // Standard BMS: 192 units = 1 measure (4 beats)
                        let stop_measures = stop_units / 192.0;
                        chart.timing_events.push(TimingEvent {
                            measure,
                            fraction,
                            kind: TimingEventKind::StopMeasures(stop_measures),
                        });
                    }
                }
            }
            // 11..19: 1P Tap Notes
            "11" | "12" | "13" | "14" | "15" | "16" | "18" | "19" => {
                if let Some(wav_id) = decode_base36(c1, c2) {
                    if let Some(lane) = channel_to_lane(channel) {
                        chart.notes.push(NoteEvent {
                            measure,
                            fraction,
                            lane,
                            wav_id: Some(wav_id),
                            note_type: NoteType::Tap,
                        });
                    }
                }
            }
            // 51..59: 1P Long Notes (LNTYPE 1)
            "51" | "52" | "53" | "54" | "55" | "56" | "58" | "59" => {
                if let Some(wav_id) = decode_base36(c1, c2) {
                    if let Some(lane) = channel_to_lane(channel) {
                        raw_ln_events.push((measure, fraction, lane, wav_id));
                    }
                }
            }
            _ => (),
        }
    }

    Ok(())
}

fn channel_to_lane(ch: &str) -> Option<Lane> {
    match ch {
        "11" | "51" => Some(Lane::Key1),
        "12" | "52" => Some(Lane::Key2),
        "13" | "53" => Some(Lane::Key3),
        "14" | "54" => Some(Lane::Key4),
        "15" | "55" => Some(Lane::Key5),
        "16" | "56" => Some(Lane::Scratch),
        "18" | "58" => Some(Lane::Key6),
        "19" | "59" => Some(Lane::Key7),
        _ => None,
    }
}

fn process_lntype1_notes(
    raw_lns: &[(u32, f64, Lane, WavId)],
    notes: &mut Vec<NoteEvent>,
) {
    let mut lane_pending: HashMap<Lane, (u32, f64, WavId)> = HashMap::new();

    let mut sorted_lns = raw_lns.to_vec();
    sorted_lns.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    });

    for (measure, fraction, lane, wav_id) in sorted_lns {
        if let Some((start_m, start_f, start_wav)) = lane_pending.remove(&lane) {
            // End of LN
            notes.push(NoteEvent {
                measure: start_m,
                fraction: start_f,
                lane,
                wav_id: Some(start_wav),
                note_type: NoteType::LongNoteStart,
            });
            notes.push(NoteEvent {
                measure,
                fraction,
                lane,
                wav_id: Some(wav_id),
                note_type: NoteType::LongNoteEnd,
            });
        } else {
            // Start of LN
            lane_pending.insert(lane, (measure, fraction, wav_id));
        }
    }
}

fn process_lnobj_notes(ln_obj: WavId, notes: &mut [NoteEvent]) {
    let mut last_note_per_lane: HashMap<Lane, usize> = HashMap::new();

    for i in 0..notes.len() {
        let lane = notes[i].lane;
        if notes[i].wav_id == Some(ln_obj) {
            notes[i].note_type = NoteType::LongNoteEnd;
            if let Some(&prev_idx) = last_note_per_lane.get(&lane) {
                notes[prev_idx].note_type = NoteType::LongNoteStart;
            }
        }
        last_note_per_lane.insert(lane, i);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_base36() {
        assert_eq!(decode_base36(b'0', b'0'), None);
        assert_eq!(decode_base36(b'0', b'1'), Some(WavId(1)));
        assert_eq!(decode_base36(b'0', b'9'), Some(WavId(9)));
        assert_eq!(decode_base36(b'0', b'A'), Some(WavId(10)));
        assert_eq!(decode_base36(b'0', b'a'), Some(WavId(10)));
        assert_eq!(decode_base36(b'0', b'Z'), Some(WavId(35)));
        assert_eq!(decode_base36(b'1', b'0'), Some(WavId(36)));
        assert_eq!(decode_base36(b'Z', b'Z'), Some(WavId(35 * 36 + 35)));
        assert_eq!(decode_base36(b'!', b'A'), None);
    }

    #[test]
    fn test_decode_hex() {
        assert_eq!(decode_hex(b'0', b'0'), Some(0));
        assert_eq!(decode_hex(b'9', b'6'), Some(150));
        assert_eq!(decode_hex(b'F', b'F'), Some(255));
        assert_eq!(decode_hex(b'g', b'0'), None);
    }

    #[test]
    fn test_parse_bms_header() {
        let bms = r#"
#PLAYER 1
#GENRE Hardcore
#TITLE Spica
#SUBTITLE (Original Mix)
#ARTIST void
#BPM 175.5
#PLAYLEVEL 10
#RANK 1
#TOTAL 300
#STAGEFILE bg.png
#BANNER banner.png
#LNOBJ 0Z
#WAV01 kick.wav
#WAV02 snare.wav
#BPM01 190.0
#STOP01 192
"#;
        let chart = parse_bms(bms).expect("Failed to parse header");
        assert_eq!(chart.header.title, "Spica");
        assert_eq!(chart.header.subtitle, "(Original Mix)");
        assert_eq!(chart.header.artist, "void");
        assert_eq!(chart.header.bpm, 175.5);
        assert_eq!(chart.header.play_level, 10);
        assert_eq!(chart.header.total, 300.0);
        assert_eq!(chart.header.ln_obj, Some(WavId(35)));
        assert_eq!(chart.header.wav_table.get(&WavId(1)), Some(&"kick.wav".to_string()));
        assert_eq!(chart.header.bpm_table.get(&WavId(1)), Some(&190.0));
        assert_eq!(chart.header.stop_table.get(&WavId(1)), Some(&192.0));
    }

    #[test]
    fn test_parse_bms_notes_and_timing() {
        let bms = r#"
#BPM 150
#BPM01 200
#STOP01 96
#00102:0.75
#00101:01000200
#00103:96
#00108:0001
#00109:00000100
#00111:0100
#00116:0002
"#;
        let chart = parse_bms(bms).expect("Failed to parse notes");
        assert_eq!(chart.measure_lengths.get(&1), Some(&0.75));

        // BGM check
        assert_eq!(chart.bgm_notes.len(), 2);
        assert_eq!(chart.bgm_notes[0], (1, 0.0, WavId(1)));
        assert_eq!(chart.bgm_notes[1], (1, 0.5, WavId(2)));

        // Timing checks (BPM hex, BPM extended, STOP)
        assert_eq!(chart.timing_events.len(), 3);
        assert_eq!(chart.timing_events[0], TimingEvent {
            measure: 1,
            fraction: 0.0,
            kind: TimingEventKind::BpmChange(150.0),
        });
        assert_eq!(chart.timing_events[1], TimingEvent {
            measure: 1,
            fraction: 0.5,
            kind: TimingEventKind::BpmChange(200.0),
        });
        assert_eq!(chart.timing_events[2], TimingEvent {
            measure: 1,
            fraction: 0.5,
            kind: TimingEventKind::StopMeasures(0.5),
        });

        // 1P notes check
        assert_eq!(chart.notes.len(), 2);
        assert_eq!(chart.notes[0], NoteEvent {
            measure: 1,
            fraction: 0.0,
            lane: Lane::Key1,
            wav_id: Some(WavId(1)),
            note_type: NoteType::Tap,
        });
        assert_eq!(chart.notes[1], NoteEvent {
            measure: 1,
            fraction: 0.5,
            lane: Lane::Scratch,
            wav_id: Some(WavId(2)),
            note_type: NoteType::Tap,
        });
    }

    #[test]
    fn test_parse_long_notes_lntype1() {
        let bms = r#"
#00151:01000000
#00251:01000000
"#;
        let chart = parse_bms(bms).expect("Failed to parse LN");
        assert_eq!(chart.notes.len(), 2);
        assert_eq!(chart.notes[0].note_type, NoteType::LongNoteStart);
        assert_eq!(chart.notes[0].measure, 1);
        assert_eq!(chart.notes[1].note_type, NoteType::LongNoteEnd);
        assert_eq!(chart.notes[1].measure, 2);
    }

    #[test]
    fn test_parse_long_notes_lnobj() {
        let bms = r#"
#LNOBJ ZZ
#00111:01000000
#00211:ZZ000000
"#;
        let chart = parse_bms(bms).expect("Failed to parse LNOBJ");
        assert_eq!(chart.notes.len(), 2);
        assert_eq!(chart.notes[0].note_type, NoteType::LongNoteStart);
        assert_eq!(chart.notes[1].note_type, NoteType::LongNoteEnd);
    }
}

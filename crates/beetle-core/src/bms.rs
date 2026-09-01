use std::collections::HashMap;

/// BMS `#WAVxx` sound identifier (Base36).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct BmpId(pub u16);

/// BMS play mode category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PlayMode {
    #[default]
    Keys7,
    Keys5,
    Keys9,
    Keys10,
    Keys14,
}

impl PlayMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Keys7 => "7KEYS",
            Self::Keys5 => "5KEYS",
            Self::Keys9 => "9KEYS",
            Self::Keys10 => "10KEYS",
            Self::Keys14 => "14KEYS",
        }
    }
}

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

/// BGA event channel kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BgaChannel {
    /// 04: Base background animation frame.
    Base,
    /// 06: Poor animation overlay frame shown on miss/poor.
    Poor,
    /// 07: Layer animation overlay frame.
    Layer,
}

/// A parsed BGA event placed on the timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct BgaEvent {
    pub measure: u32,
    pub fraction: f64,
    pub channel: BgaChannel,
    pub bmp_id: BmpId,
}

/// BGA slice and coordinate definition from `#BGAxx`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BgaDefinition {
    pub bmp_id: BmpId,
    pub sx: i32,
    pub sy: i32,
    pub w: u32,
    pub h: u32,
    pub dx: i32,
    pub dy: i32,
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
    pub difficulty: Option<u32>,
    pub lntype: u32,
    pub wav_table: HashMap<WavId, String>,
    pub bmp_table: HashMap<BmpId, String>,
    pub bga_table: HashMap<BmpId, BgaDefinition>,
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
            difficulty: None,
            lntype: 1,
            wav_table: HashMap::new(),
            bmp_table: HashMap::new(),
            bga_table: HashMap::new(),
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
    pub bga_events: Vec<BgaEvent>,
    pub timing_events: Vec<TimingEvent>,
    pub measure_lengths: HashMap<u32, f64>,
    pub total_notes_count: usize,
    pub has_2p_dp: bool,
    pub has_pms_ch: bool,
    pub has_k67: bool,
}

impl BmsChart {
    /// Detects the play mode based on header commands and note lanes present.
    pub fn detect_play_mode(&self) -> PlayMode {
        self.detect_play_mode_with_hint(false)
    }

    /// Detects the play mode with an optional file extension hint (e.g. true if `.pms`).
    pub fn detect_play_mode_with_hint(&self, is_pms_ext: bool) -> PlayMode {
        if is_pms_ext {
            PlayMode::Keys9
        } else if self.header.player == 2 || self.header.player == 3 || self.has_2p_dp {
            // #PLAYER 3 (Double Play) or #PLAYER 2 (Couple Play) or 2P note channels present
            if self.has_k67 {
                PlayMode::Keys14
            } else {
                PlayMode::Keys10
            }
        } else if self.has_pms_ch && !self.has_k67 {
            PlayMode::Keys9
        } else if self.has_k67 {
            PlayMode::Keys7
        } else {
            PlayMode::Keys5
        }
    }
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

/// Helper to identify if a trimmed BMS line is a measure data command `#XXXYY:...`
#[inline(always)]
pub fn is_measure_line(content: &str) -> bool {
    let bytes = content.as_bytes();
    bytes.len() >= 6
        && bytes[0..3].iter().all(|b| b.is_ascii_digit())
        && (bytes[5] == b':' || bytes[5] == b' ')
}

/// Parses raw BMS/BME/BML chart text into a `BmsChart`.
pub fn parse_bms(input: &str) -> Result<BmsChart, BmsParseError> {
    let mut chart = BmsChart::default();
    let mut raw_ln_events: Vec<(u32, f64, Lane, WavId)> = Vec::new();

    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(BmsParseError::EmptyChart);
    }

    // PASS 1: Parse all header commands and definition tables (#WAV, #BMP, #BGA, #BPMxx, #STOPxx, #LNOBJ, etc.)
    // This ensures that all definitions placed at the bottom of the file are available before measure lines are evaluated.
    for line in trimmed.lines() {
        let line = line.trim();
        if !line.starts_with('#') {
            continue;
        }
        let content = &line[1..].trim_start();
        if content.is_empty() || is_measure_line(content) {
            continue;
        }
        parse_header_line(content, &mut chart.header);
    }

    // PASS 2: Parse all measure channels using the fully populated header definition tables
    for line in trimmed.lines() {
        let line = line.trim();
        if !line.starts_with('#') {
            continue;
        }
        let content = &line[1..].trim_start();
        if content.is_empty() || !is_measure_line(content) {
            continue;
        }
        parse_measure_line(content, &mut chart, &mut raw_ln_events)?;
    }

    // Process LNTYPE 1 long notes (pairs of channel 5x events)
    process_lntype1_notes(&raw_ln_events, &mut chart.notes, &mut chart.total_notes_count);

    // CRITICAL: Sort notes chronologically BEFORE LNOBJ processing
    // In real BMS files, measure lines may appear in arbitrary order. Notes must be strictly sorted by time
    // so that the preceding note on the lane is correctly matched as LongNoteStart.
    chart.notes.sort_by(|a, b| {
        a.measure
            .cmp(&b.measure)
            .then_with(|| a.fraction.partial_cmp(&b.fraction).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.lane.cmp(&b.lane))
    });

    // Process #LNOBJ long notes on chronologically sorted notes
    if let Some(ln_obj_id) = chart.header.ln_obj {
        process_lnobj_notes(ln_obj_id, &mut chart.notes);
    }

    chart.bgm_notes.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    });

    chart.bga_events.sort_by(|a, b| {
        a.measure
            .cmp(&b.measure)
            .then_with(|| a.fraction.partial_cmp(&b.fraction).unwrap_or(std::cmp::Ordering::Equal))
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

    if key.eq_ignore_ascii_case("PLAYER") {
        if let Ok(p) = val.parse::<u32>() {
            header.player = p;
        }
    } else if key.eq_ignore_ascii_case("DIFFICULTY") {
        if let Ok(diff) = val.parse::<u32>() {
            header.difficulty = Some(diff);
        }
    } else if key.eq_ignore_ascii_case("LNTYPE") {
        if let Ok(t) = val.parse::<u32>() {
            header.lntype = t;
        }
    } else if key.eq_ignore_ascii_case("TITLE") {
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
    } else if key.len() >= 4 && key[..3].eq_ignore_ascii_case("BGA") {
        let id_str = &key[3..];
        if id_str.len() == 2 {
            let b = id_str.as_bytes();
            if let Some(id) = decode_base36(b[0], b[1]) {
                // Form: #BGAxx bmp_id sx sy w h dx dy
                let tokens: Vec<&str> = val.split_whitespace().collect();
                if tokens.len() >= 7 {
                    let bmp_id_bytes = tokens[0].as_bytes();
                    let bmp_id = if bmp_id_bytes.len() >= 2 {
                        decode_base36(bmp_id_bytes[0], bmp_id_bytes[1]).map(|w| BmpId(w.0)).unwrap_or(BmpId(id.0))
                    } else {
                        BmpId(id.0)
                    };
                    let sx = tokens[1].parse::<i32>().unwrap_or(0);
                    let sy = tokens[2].parse::<i32>().unwrap_or(0);
                    let w = tokens[3].parse::<u32>().unwrap_or(0);
                    let h = tokens[4].parse::<u32>().unwrap_or(0);
                    let dx = tokens[5].parse::<i32>().unwrap_or(0);
                    let dy = tokens[6].parse::<i32>().unwrap_or(0);
                    header.bga_table.insert(
                        BmpId(id.0),
                        BgaDefinition {
                            bmp_id,
                            sx,
                            sy,
                            w,
                            h,
                            dx,
                            dy,
                        },
                    );
                }
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

    // Record channel usage for accurate play mode detection (5K, 7K, 9K, 10K, 14K)
    match channel {
        "18" | "19" | "58" | "59" => {
            chart.has_k67 = true;
        }
        "28" | "29" | "68" | "69" => {
            chart.has_k67 = true;
            chart.has_2p_dp = true;
        }
        "21" | "26" | "61" | "66" => {
            chart.has_2p_dp = true;
        }
        "22" | "23" | "24" | "25" | "62" | "63" | "64" | "65" => {
            chart.has_pms_ch = true;
        }
        _ => {}
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
            // 04: BGA Base Channel
            "04" => {
                if let Some(wav_id) = decode_base36(c1, c2) {
                    chart.bga_events.push(BgaEvent {
                        measure,
                        fraction,
                        channel: BgaChannel::Base,
                        bmp_id: BmpId(wav_id.0),
                    });
                }
            }
            // 06: BGA Poor Channel
            "06" => {
                if let Some(wav_id) = decode_base36(c1, c2) {
                    chart.bga_events.push(BgaEvent {
                        measure,
                        fraction,
                        channel: BgaChannel::Poor,
                        bmp_id: BmpId(wav_id.0),
                    });
                }
            }
            // 07: BGA Layer Channel
            "07" => {
                if let Some(wav_id) = decode_base36(c1, c2) {
                    chart.bga_events.push(BgaEvent {
                        measure,
                        fraction,
                        channel: BgaChannel::Layer,
                        bmp_id: BmpId(wav_id.0),
                    });
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
                    chart.total_notes_count += 1;
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
            // 21..29: 2P Tap Notes (Counted in chart notes total, routed to BGM keysounds in 1P mode)
            "21" | "22" | "23" | "24" | "25" | "26" | "28" | "29" => {
                if let Some(wav_id) = decode_base36(c1, c2) {
                    chart.total_notes_count += 1;
                    chart.bgm_notes.push((measure, fraction, wav_id));
                }
            }
            // 31..39, 41..49: Invisible/Freezone Notes (Plays keysound on beat without visual lane note)
            "31" | "32" | "33" | "34" | "35" | "36" | "38" | "39"
            | "41" | "42" | "43" | "44" | "45" | "46" | "48" | "49" => {
                if let Some(wav_id) = decode_base36(c1, c2) {
                    chart.bgm_notes.push((measure, fraction, wav_id));
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
            // 61..69: 2P Long Notes (LNTYPE 1)
            "61" | "62" | "63" | "64" | "65" | "66" | "68" | "69" => {
                if let Some(wav_id) = decode_base36(c1, c2) {
                    chart.total_notes_count += 1;
                    chart.bgm_notes.push((measure, fraction, wav_id));
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
    total_notes_count: &mut usize,
) {
    let mut lane_pending: HashMap<Lane, (u32, f64, WavId)> = HashMap::new();

    let mut sorted_lns = raw_lns.to_vec();
    sorted_lns.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    });

    for (measure, fraction, lane, wav_id) in sorted_lns {
        if let Some((start_m, start_f, start_wav)) = lane_pending.remove(&lane) {
            *total_notes_count += 1;
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
                wav_id: None,
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
            notes[i].wav_id = None; // Release of LN does not re-trigger sound
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
        assert_eq!(chart.notes[1].wav_id, None);
    }

    #[test]
    fn test_two_pass_parsing_and_bottom_definitions() {
        let bms = r#"
#PLAYER 1
#00211:01000000
#00111:02000000
#00108:01
#00209:02
#WAV01 kick.wav
#WAV02 ln_start.wav
#WAVFF ln_end.wav
#BPM01 175.5
#STOP02 96
#LNOBJ FF
#DIFFICULTY 4
#LNTYPE 2
"#;
        let chart = parse_bms(bms).expect("Failed to parse two-pass BMS");
        assert_eq!(chart.header.difficulty, Some(4));
        assert_eq!(chart.header.lntype, 2);
        assert_eq!(chart.header.ln_obj, Some(WavId(15 * 36 + 15))); // FF in base36: 15*36+15 = 555
        assert_eq!(chart.timing_events.len(), 2);
        assert_eq!(chart.timing_events[0].measure, 1);
        assert_eq!(chart.timing_events[0].kind, TimingEventKind::BpmChange(175.5));
        assert_eq!(chart.timing_events[1].measure, 2);
        assert_eq!(chart.timing_events[1].kind, TimingEventKind::StopMeasures(96.0 / 192.0));
    }

    #[test]
    fn test_detect_play_mode() {
        let bms_5k = r#"
#PLAYER 1
#00111:01000000
#00116:02000000
"#;
        let chart_5k = parse_bms(bms_5k).unwrap();
        assert_eq!(chart_5k.detect_play_mode(), PlayMode::Keys5);

        let bms_7k = r#"
#PLAYER 1
#00118:01000000
"#;
        let chart_7k = parse_bms(bms_7k).unwrap();
        assert_eq!(chart_7k.detect_play_mode(), PlayMode::Keys7);

        let bms_10k = r#"
#PLAYER 2
#00111:01000000
"#;
        let chart_10k = parse_bms(bms_10k).unwrap();
        assert_eq!(chart_10k.detect_play_mode(), PlayMode::Keys10);

        let bms_14k = r#"
#PLAYER 2
#00118:01000000
"#;
        let chart_14k = parse_bms(bms_14k).unwrap();
        assert_eq!(chart_14k.detect_play_mode(), PlayMode::Keys14);

        let bms_dp3 = r#"
#PLAYER 3
#00118:01000000
"#;
        let chart_dp3 = parse_bms(bms_dp3).unwrap();
        assert_eq!(chart_dp3.detect_play_mode(), PlayMode::Keys14);

        let bms_pms = r#"
#PLAYER 1
#00111:01000000
#00122:02000000
"#;
        let chart_pms = parse_bms(bms_pms).unwrap();
        assert_eq!(chart_pms.detect_play_mode(), PlayMode::Keys9);

        let chart_pms_hint = parse_bms(bms_7k).unwrap();
        assert_eq!(chart_pms_hint.detect_play_mode_with_hint(true), PlayMode::Keys9);
    }

    #[test]
    fn test_2p_and_invisible_notes_handling() {
        let bms = r#"
#PLAYER 3
#00111:01000000
#00121:02000000
#00131:03000000
#00161:04000000
#00161:00000000
"#;
        let chart = parse_bms(bms).expect("Failed to parse DP chart with invisible notes");
        assert_eq!(chart.notes.len(), 1); // 1P note on key 1
        assert_eq!(chart.bgm_notes.len(), 3); // 2P note, invisible note, and 2P LN routed to bgm_notes
        assert_eq!(chart.total_notes_count, 3); // 1 1P note + 1 2P note + 1 2P LN
        assert_eq!(chart.detect_play_mode(), PlayMode::Keys10);
    }

    #[test]
    fn test_parse_bga_events_and_definitions() {
        let bms = r#"
#BMP01 bg.bmp
#BMP02 miss.bmp
#BMP03 overlay.bmp
#BGA04 01 0 0 256 256 0 0
#00104:01000000
#00106:02000000
#00107:03000000
"#;
        let chart = parse_bms(bms).expect("Failed to parse BGA");
        assert_eq!(chart.header.bmp_table.len(), 3);
        assert_eq!(chart.header.bmp_table.get(&BmpId(1)), Some(&"bg.bmp".to_string()));
        assert_eq!(chart.header.bga_table.get(&BmpId(4)), Some(&BgaDefinition {
            bmp_id: BmpId(1),
            sx: 0,
            sy: 0,
            w: 256,
            h: 256,
            dx: 0,
            dy: 0,
        }));
        assert_eq!(chart.bga_events.len(), 3);
        assert_eq!(chart.bga_events[0], BgaEvent {
            measure: 1,
            fraction: 0.0,
            channel: BgaChannel::Base,
            bmp_id: BmpId(1),
        });
        assert_eq!(chart.bga_events[1], BgaEvent {
            measure: 1,
            fraction: 0.0,
            channel: BgaChannel::Poor,
            bmp_id: BmpId(2),
        });
        assert_eq!(chart.bga_events[2], BgaEvent {
            measure: 1,
            fraction: 0.0,
            channel: BgaChannel::Layer,
            bmp_id: BmpId(3),
        });
    }
}

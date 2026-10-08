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
    /// UE-pack 4K: Key1, Key2, Key4, Key5. Declared by `#4K`.
    Keys4,
    /// UE-pack 6K: Key1..Key3, Key5..Key7. Declared by `#6K`.
    Keys6,
    /// UE-pack 8K: Scratch + Key1..Key7. Declared by `#8K`.
    Keys8,
}

impl PlayMode {
    /// Lanes a declared 4K/6K/8K chart can play; other modes have no
    /// restriction beyond what their channels map to (`None`).
    pub fn restricted_lanes(&self) -> Option<&'static [Lane]> {
        match self {
            Self::Keys4 => Some(&[Lane::Key1, Lane::Key2, Lane::Key4, Lane::Key5]),
            Self::Keys6 => Some(&[
                Lane::Key1,
                Lane::Key2,
                Lane::Key3,
                Lane::Key5,
                Lane::Key6,
                Lane::Key7,
            ]),
            Self::Keys8 => Some(&[
                Lane::Scratch,
                Lane::Key1,
                Lane::Key2,
                Lane::Key3,
                Lane::Key4,
                Lane::Key5,
                Lane::Key6,
                Lane::Key7,
            ]),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Keys4 => "4KEYS",
            Self::Keys6 => "6KEYS",
            Self::Keys8 => "8KEYS",
            Self::Keys7 => "7KEYS",
            Self::Keys5 => "5KEYS",
            Self::Keys9 => "9KEYS",
            Self::Keys10 => "10KEYS",
            Self::Keys14 => "14KEYS",
        }
    }
}

/// Key lanes supported by Beetle: 1P (Scratch + 7 keys + 2 PMS-only extra
/// buttons) and 2P / Double-Play lanes used by 10K/14K charts.
///
/// INVARIANT: the first 8 variants (`Scratch`..`Key7`) must keep their
/// current order. `.rep` replay files encode `Lane` as a `u8` using that
/// exact ordering (see `replay.rs`), so changing it would silently corrupt
/// previously recorded 5K/7K replays. New variants are appended only.
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
    /// PMS (9-Key / pop'n style) extra buttons; unused outside `PlayMode::Keys9`.
    Key8,
    Key9,
    /// Double Play (10K/14K) 2P-side lanes; unused outside `PlayMode::Keys10`/`Keys14`.
    P2Scratch,
    P2Key1,
    P2Key2,
    P2Key3,
    P2Key4,
    P2Key5,
    P2Key6,
    P2Key7,
}

impl Lane {
    /// Every `Lane` variant, in declaration order. Used wherever all lanes
    /// need enumerating (e.g. serializing custom key bindings) instead of
    /// each call site maintaining its own duplicate list.
    pub const ALL: [Lane; 18] = [
        Lane::Scratch,
        Lane::Key1,
        Lane::Key2,
        Lane::Key3,
        Lane::Key4,
        Lane::Key5,
        Lane::Key6,
        Lane::Key7,
        Lane::Key8,
        Lane::Key9,
        Lane::P2Scratch,
        Lane::P2Key1,
        Lane::P2Key2,
        Lane::P2Key3,
        Lane::P2Key4,
        Lane::P2Key5,
        Lane::P2Key6,
        Lane::P2Key7,
    ];
}

/// Note type (tap, long note endpoints, landmine).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteType {
    Tap,
    LongNoteStart,
    LongNoteEnd,
    /// Channels D1..D9 / E1..E9. `wav_id` carries the object value: the
    /// damage, in half-percent units of the gauge. Never a playable note.
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
    /// Mode named by a `#4K` / `#6K` / `#8K` line; `None` when absent.
    pub declared_mode: Option<PlayMode>,
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
    /// `#PREVIEW` audio file played on song select (empty when absent).
    pub preview: String,
    pub ln_obj: Option<WavId>,
    pub difficulty: Option<u32>,
    pub lntype: u32,
    /// `#LNMODE` (1 LN, 2 CN, 3 HCN); `None` when absent or not one of those.
    pub ln_mode: Option<u32>,
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
            declared_mode: None,
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
            preview: String::new(),
            ln_obj: None,
            difficulty: None,
            lntype: 1,
            ln_mode: None,
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
    /// Freezone / transparent-note samples (channels 31..39, 41..49)
    pub freezone_notes: Vec<(u32, f64, WavId)>,
    pub bga_events: Vec<BgaEvent>,
    pub timing_events: Vec<TimingEvent>,
    pub measure_lengths: HashMap<u32, f64>,
    pub total_notes_count: usize,
    pub max_measure: u32,
    pub has_scratch: bool,
    pub has_2p_dp: bool,
    pub has_2p_key1: bool,
    pub has_pms_ch: bool,
    pub has_k67: bool,
    /// Seed the `#RANDOM` rolls were made with; `None` when the chart has no random sections.
    pub random_seed: Option<u64>,
}

impl BmsChart {
    /// Notes the player has to hit; landmines are obstacles, not notes.
    pub fn playable_notes_len(&self) -> usize {
        self.notes
            .iter()
            .filter(|n| n.note_type != NoteType::Landmine)
            .count()
    }

    /// Lowest and highest BPM the chart passes through (initial BPM included).
    pub fn bpm_range(&self) -> (f64, f64) {
        let (mut min, mut max) = (self.header.bpm, self.header.bpm);
        for event in &self.timing_events {
            if let TimingEventKind::BpmChange(bpm) = event.kind {
                if bpm > 0.0 {
                    min = min.min(bpm);
                    max = max.max(bpm);
                }
            }
        }
        (min, max)
    }

    /// Detects the play mode based on header commands and note lanes present.
    pub fn detect_play_mode(&self) -> PlayMode {
        self.detect_play_mode_with_hint(false)
    }

    /// Detects the play mode with an optional file extension hint (e.g. true if `.pms`).
    pub fn detect_play_mode_with_hint(&self, is_pms_ext: bool) -> PlayMode {
        if is_pms_ext {
            PlayMode::Keys9
        } else if let Some(mode) = self.header.declared_mode {
            // Wins over a stray `#PLAYER 2/3`: 4K/6K/8K are single play.
            mode
        } else if self.header.player == 2 || self.header.player == 3 {
            // #PLAYER 3 (Double Play) or #PLAYER 2 (Couple Play)
            if self.has_k67 {
                PlayMode::Keys14
            } else {
                PlayMode::Keys10
            }
        } else if self.has_2p_dp {
            // 2P note channels are present.
            // Pop'n Music (9KEYS) strictly uses only 1P keys 1..5 (11..15) and 2P channels 22..25,
            // with NO scratch lanes (16/26), NO key 6/7 lanes (18/19/28/29),
            // NO 2P key 1 (21/61), and player is 1 or unset.
            let is_pms_layout = self.has_pms_ch
                && !self.has_scratch
                && !self.has_k67
                && !self.has_2p_key1
                && (self.header.player <= 1);

            if is_pms_layout {
                PlayMode::Keys9
            } else if self.has_k67 {
                PlayMode::Keys14
            } else {
                PlayMode::Keys10
            }
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

/// Encode a `WavId` (1..=1295) to a 2-character Base36 string (`01`..`ZZ`).
pub fn encode_base36(id: WavId) -> String {
    let val = id.0;
    let d1 = ((val / 36) % 36) as u8;
    let d2 = (val % 36) as u8;
    let c1 = if d1 < 10 { b'0' + d1 } else { b'A' + (d1 - 10) };
    let c2 = if d2 < 10 { b'0' + d2 } else { b'A' + (d2 - 10) };
    let mut s = String::with_capacity(2);
    s.push(c1 as char);
    s.push(c2 as char);
    s
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

/// Safely decodes BMS chart text bytes into a UTF-8 Rust `String`.
/// Attempts:
/// 1. Strict UTF-8 first (if valid UTF-8, returns immediately)
/// 2. On Windows: Native Win32 `MultiByteToWideChar` with CP932 (Shift-JIS) then CP949 (Korean)
/// 3. Fallback: `String::from_utf8_lossy(bytes)`
#[cfg(target_os = "windows")]
pub fn decode_bms_text(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    // 1. Strict UTF-8
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }

    // 2. Shift-JIS (CP932)
    if let Some(s) = win32_decode_cp(bytes, 932) {
        return s;
    }

    // 3. Korean CP949 (EUC-KR)
    if let Some(s) = win32_decode_cp(bytes, 949) {
        return s;
    }

    // 4. Current ANSI codepage (CP_ACP = 0)
    if let Some(s) = win32_decode_cp(bytes, 0) {
        return s;
    }

    // 5. Lossy UTF-8 fallback
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(not(target_os = "windows"))]
pub fn decode_bms_text(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        s.to_string()
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

#[cfg(target_os = "windows")]
fn win32_decode_cp(bytes: &[u8], code_page: u32) -> Option<String> {
    extern "system" {
        fn MultiByteToWideChar(
            code_page: u32,
            dw_flags: u32,
            lp_multi_byte_str: *const u8,
            cb_multi_byte: i32,
            lp_wide_char_str: *mut u16,
            cch_wide_char: i32,
        ) -> i32;
    }

    if bytes.is_empty() {
        return Some(String::new());
    }

    const MB_ERR_INVALID_CHARS: u32 = 0x00000008;

    unsafe {
        // Query length with strict character validation
        let len = MultiByteToWideChar(
            code_page,
            MB_ERR_INVALID_CHARS,
            bytes.as_ptr(),
            bytes.len() as i32,
            std::ptr::null_mut(),
            0,
        );

        if len <= 0 {
            // For CP_ACP (0), try non-strict validation
            if code_page == 0 {
                let len_lenient = MultiByteToWideChar(
                    code_page,
                    0,
                    bytes.as_ptr(),
                    bytes.len() as i32,
                    std::ptr::null_mut(),
                    0,
                );
                if len_lenient > 0 {
                    let mut buf = vec![0u16; len_lenient as usize];
                    let written = MultiByteToWideChar(
                        code_page,
                        0,
                        bytes.as_ptr(),
                        bytes.len() as i32,
                        buf.as_mut_ptr(),
                        len_lenient,
                    );
                    if written > 0 {
                        return Some(String::from_utf16_lossy(&buf[..written as usize]));
                    }
                }
            }
            return None;
        }

        let mut buf = vec![0u16; len as usize];
        let written = MultiByteToWideChar(
            code_page,
            MB_ERR_INVALID_CHARS,
            bytes.as_ptr(),
            bytes.len() as i32,
            buf.as_mut_ptr(),
            len,
        );

        if written > 0 {
            Some(String::from_utf16_lossy(&buf[..written as usize]))
        } else {
            None
        }
    }
}

/// Helper to identify if a trimmed BMS line is a measure data command `#XXXYY:...`
#[inline(always)]
pub fn is_measure_line(content: &str) -> bool {
    let bytes = content.as_bytes();
    bytes.len() >= 6
        && bytes[0..3].iter().all(|b| b.is_ascii_digit())
        && (bytes[5] == b':' || bytes[5] == b' ')
}

/// Seed `parse_bms` rolls `#RANDOM` with: library metadata stays stable from run to run.
pub const DEFAULT_RANDOM_SEED: u64 = 1;

/// Parses raw BMS/BME/BML chart text into a `BmsChart`, rolling any `#RANDOM`
/// with `DEFAULT_RANDOM_SEED`.
pub fn parse_bms(input: &str) -> Result<BmsChart, BmsParseError> {
    parse_bms_with_seed(input, DEFAULT_RANDOM_SEED)
}

/// Parses a chart, resolving `#RANDOM` / `#IF` sections with the rolls `seed`
/// produces. The same text and seed always give the same chart; the seed used
/// is kept in `BmsChart::random_seed` (so a replay can ask for the same one).
pub fn parse_bms_with_seed(input: &str, seed: u64) -> Result<BmsChart, BmsParseError> {
    let resolved = crate::resolver::resolve_random(input, seed);
    let mut chart = parse_resolved(resolved.as_deref().unwrap_or(input))?;
    chart.random_seed = resolved.is_some().then_some(seed);
    Ok(chart)
}

fn parse_resolved(input: &str) -> Result<BmsChart, BmsParseError> {
    let mut chart = BmsChart::default();
    let mut raw_ln_events: Vec<(u32, f64, Lane, WavId)> = Vec::new();
    let mut skipped_ln_events: Vec<(u32, f64, String, WavId)> = Vec::new();

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

    // PASS 2a: Scan every measure channel once to resolve play-mode flags
    // (has_scratch/has_k67/has_2p_dp/has_pms_ch/has_2p_key1) across the WHOLE
    // file before any note is generated. This matters because channels
    // 21..29 mean different lanes depending on the final PlayMode (PMS 9K
    // extra buttons vs. Double Play 2P keys), which can only be known once
    // every measure has been scanned.
    for line in trimmed.lines() {
        let line = line.trim();
        if !line.starts_with('#') {
            continue;
        }
        let content = &line[1..].trim_start();
        if content.is_empty() || !is_measure_line(content) {
            continue;
        }
        scan_measure_flags(content, &mut chart);
    }
    let play_mode = chart.detect_play_mode();

    // PASS 2b: Parse all measure channels using the fully populated header
    // definition tables and the now-resolved PlayMode.
    for line in trimmed.lines() {
        let line = line.trim();
        if !line.starts_with('#') {
            continue;
        }
        let content = &line[1..].trim_start();
        if content.is_empty() || !is_measure_line(content) {
            continue;
        }
        parse_measure_line(
            content,
            &mut chart,
            &mut raw_ln_events,
            &mut skipped_ln_events,
            play_mode,
        )?;
    }

    // Long notes on a lane the declared mode doesn't have play as sound only:
    // the sound of each head, none for the tail.
    skipped_ln_events.sort_by(|a, b| {
        a.2.cmp(&b.2)
            .then(a.0.cmp(&b.0))
            .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    });
    let mut open_channel: Option<&str> = None;
    for (measure, fraction, channel, wav_id) in &skipped_ln_events {
        if open_channel == Some(channel.as_str()) {
            open_channel = None;
        } else {
            open_channel = Some(channel.as_str());
            chart.bgm_notes.push((*measure, *fraction, *wav_id));
        }
    }

    // Process LNTYPE 1 long notes (pairs of channel 5x events)
    process_lntype1_notes(
        &raw_ln_events,
        &mut chart.notes,
        &mut chart.total_notes_count,
    );

    // CRITICAL: Sort notes chronologically BEFORE LNOBJ processing
    // In real BMS files, measure lines may appear in arbitrary order. Notes must be strictly sorted by time
    // so that the preceding note on the lane is correctly matched as LongNoteStart.
    chart.notes.sort_by(|a, b| {
        a.measure
            .cmp(&b.measure)
            .then_with(|| {
                a.fraction
                    .partial_cmp(&b.fraction)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
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
        a.measure.cmp(&b.measure).then_with(|| {
            a.fraction
                .partial_cmp(&b.fraction)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });

    // At the same position a BPM change applies before a STOP, whatever order
    // the file lists their channels in (a stop lasts as long as the BPM then in effect).
    let kind_rank = |e: &TimingEvent| match e.kind {
        TimingEventKind::BpmChange(_) => 0,
        TimingEventKind::StopMeasures(_) => 1,
    };
    chart.timing_events.sort_by(|a, b| {
        a.measure
            .cmp(&b.measure)
            .then_with(|| {
                a.fraction
                    .partial_cmp(&b.fraction)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| kind_rank(a).cmp(&kind_rank(b)))
    });

    Ok(chart)
}

fn parse_header_line(content: &str, header: &mut BmsHeader) {
    let mut parts = content.splitn(2, |c: char| c.is_whitespace() || c == ':');
    let key = parts.next().unwrap_or("").trim();
    let val = parts.next().unwrap_or("").trim();

    if val.is_empty() && header.declared_mode.is_none() {
        let declared = match key.to_ascii_uppercase().as_str() {
            "4K" => Some(PlayMode::Keys4),
            "6K" => Some(PlayMode::Keys6),
            "8K" => Some(PlayMode::Keys8),
            _ => None,
        };
        if declared.is_some() {
            header.declared_mode = declared;
            return;
        }
    }

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
    } else if key.eq_ignore_ascii_case("LNMODE") {
        header.ln_mode = val.parse::<u32>().ok().filter(|m| (1..=3).contains(m));
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
    } else if key.eq_ignore_ascii_case("PREVIEW") {
        header.preview = val.to_string();
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
                        decode_base36(bmp_id_bytes[0], bmp_id_bytes[1])
                            .map(|w| BmpId(w.0))
                            .unwrap_or(BmpId(id.0))
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

/// PASS 2a helper: inspects a single measure line and updates only the
/// play-mode-determining flags on `chart` (does not generate any notes).
/// Mirrors the channel set in `parse_measure_line`'s note-generation pass.
fn scan_measure_flags(content: &str, chart: &mut BmsChart) {
    let mut parts = content.splitn(2, [':', ' ']);
    let tag = parts.next().unwrap_or("").trim();
    let data = parts.next().unwrap_or("").trim();

    if tag.len() < 5 {
        return;
    }
    let channel = &tag[3..5];
    if channel == "02" {
        return;
    }

    let data_bytes = data.as_bytes();
    if !data_bytes.len().is_multiple_of(2) {
        return;
    }
    let slot_count = data_bytes.len() / 2;
    if slot_count == 0 {
        return;
    }

    let has_non_zero_slot = (0..slot_count).any(|i| {
        let c1 = data_bytes[i * 2];
        let c2 = data_bytes[i * 2 + 1];
        c1 != b'0' || c2 != b'0'
    });
    if !has_non_zero_slot {
        return;
    }

    match channel {
        "16" | "56" => {
            chart.has_scratch = true;
        }
        "18" | "19" | "58" | "59" => {
            chart.has_k67 = true;
        }
        "26" | "66" => {
            chart.has_scratch = true;
            chart.has_2p_dp = true;
        }
        "28" | "29" | "68" | "69" => {
            chart.has_k67 = true;
            chart.has_2p_dp = true;
        }
        "21" | "61" => {
            chart.has_2p_dp = true;
            chart.has_2p_key1 = true;
        }
        "22" | "23" | "24" | "25" | "62" | "63" | "64" | "65" => {
            chart.has_2p_dp = true;
            chart.has_pms_ch = true;
        }
        _ => {}
    }
}

fn parse_measure_line(
    content: &str,
    chart: &mut BmsChart,
    raw_ln_events: &mut Vec<(u32, f64, Lane, WavId)>,
    skipped_ln_events: &mut Vec<(u32, f64, String, WavId)>,
    mode: PlayMode,
) -> Result<(), BmsParseError> {
    let mut parts = content.splitn(2, [':', ' ']);
    let tag = parts.next().unwrap_or("").trim();
    let data = parts.next().unwrap_or("").trim();

    if tag.len() < 5 {
        return Err(BmsParseError::InvalidMeasure(tag.to_string()));
    }

    let measure: u32 = tag[0..3]
        .parse()
        .map_err(|_| BmsParseError::InvalidMeasure(tag.to_string()))?;
    let channel = &tag[3..5];
    chart.max_measure = chart.max_measure.max(measure);

    // Channel 02: Measure length ratio (e.g. #00102:0.75)
    if channel == "02" {
        if let Ok(len) = data.parse::<f64>() {
            chart.measure_lengths.insert(measure, len);
        }
        return Ok(());
    }

    // Note / Event channels: 2 characters per slot
    let data_bytes = data.as_bytes();
    if !data_bytes.len().is_multiple_of(2) {
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
                    if let Some(lane) = channel_to_lane(channel, mode) {
                        chart.total_notes_count += 1;
                        chart.notes.push(NoteEvent {
                            measure,
                            fraction,
                            lane,
                            wav_id: Some(wav_id),
                            note_type: NoteType::Tap,
                        });
                    } else if Some(wav_id) != chart.header.ln_obj {
                        // A lane the declared 4K/6K mode doesn't have: sound only.
                        chart.bgm_notes.push((measure, fraction, wav_id));
                    }
                }
            }
            // 21..29: PMS (9K) extra-button or Double Play 2P Tap Notes,
            // resolved by `mode`. Falls back to BGM passthrough (old
            // behavior) for charts that aren't actually DP/PMS.
            "21" | "22" | "23" | "24" | "25" | "26" | "28" | "29" => {
                if let Some(wav_id) = decode_base36(c1, c2) {
                    chart.total_notes_count += 1;
                    if let Some(lane) = channel_to_lane(channel, mode) {
                        chart.notes.push(NoteEvent {
                            measure,
                            fraction,
                            lane,
                            wav_id: Some(wav_id),
                            note_type: NoteType::Tap,
                        });
                    } else {
                        chart.bgm_notes.push((measure, fraction, wav_id));
                    }
                }
            }
            // 31..39, 41..49: Invisible/Freezone Notes (Transparent notes)
            "31" | "32" | "33" | "34" | "35" | "36" | "38" | "39" | "41" | "42" | "43" | "44"
            | "45" | "46" | "48" | "49" => {
                if let Some(wav_id) = decode_base36(c1, c2) {
                    chart.freezone_notes.push((measure, fraction, wav_id));
                }
            }
            // 51..59: 1P Long Notes (LNTYPE 1)
            "51" | "52" | "53" | "54" | "55" | "56" | "58" | "59" => {
                if let Some(wav_id) = decode_base36(c1, c2) {
                    if let Some(lane) = channel_to_lane(channel, mode) {
                        raw_ln_events.push((measure, fraction, lane, wav_id));
                    } else {
                        skipped_ln_events.push((measure, fraction, channel.to_string(), wav_id));
                    }
                }
            }
            // 61..69: PMS (9K) extra-button or Double Play 2P Long Notes
            // (LNTYPE 1), resolved by `mode`; same BGM fallback as 21..29.
            "61" | "62" | "63" | "64" | "65" | "66" | "68" | "69" => {
                if let Some(wav_id) = decode_base36(c1, c2) {
                    if let Some(lane) = channel_to_lane(channel, mode) {
                        raw_ln_events.push((measure, fraction, lane, wav_id));
                    } else {
                        chart.total_notes_count += 1;
                        chart.bgm_notes.push((measure, fraction, wav_id));
                    }
                }
            }
            // D1..D9 / E1..E9: landmines on the 1P / 2P lanes that 11..19 / 21..29
            // use. They are not notes: no keysound, not counted in the totals.
            _ if is_mine_channel(channel) => {
                if let (Some(value), Some(lane)) =
                    (decode_base36(c1, c2), mine_channel_to_lane(channel, mode))
                {
                    chart.notes.push(NoteEvent {
                        measure,
                        fraction,
                        lane,
                        wav_id: Some(value),
                        note_type: NoteType::Landmine,
                    });
                }
            }
            _ => (),
        }
    }

    Ok(())
}

fn is_mine_channel(ch: &str) -> bool {
    let b = ch.as_bytes();
    b.len() == 2
        && matches!(b[0].to_ascii_uppercase(), b'D' | b'E')
        && (b'1'..=b'9').contains(&b[1])
}

fn mine_channel_to_lane(ch: &str, mode: PlayMode) -> Option<Lane> {
    let b = ch.as_bytes();
    let side = if b[0].eq_ignore_ascii_case(&b'D') { '1' } else { '2' };
    channel_to_lane(&format!("{side}{}", b[1] as char), mode)
}

fn channel_to_lane(ch: &str, mode: PlayMode) -> Option<Lane> {
    let lane = channel_to_lane_unrestricted(ch, mode)?;
    match mode.restricted_lanes() {
        Some(lanes) if !lanes.contains(&lane) => None,
        _ => Some(lane),
    }
}

fn channel_to_lane_unrestricted(ch: &str, mode: PlayMode) -> Option<Lane> {
    match ch {
        "11" | "51" => Some(Lane::Key1),
        "12" | "52" => Some(Lane::Key2),
        "13" | "53" => Some(Lane::Key3),
        "14" | "54" => Some(Lane::Key4),
        "15" | "55" => Some(Lane::Key5),
        "16" | "56" => Some(Lane::Scratch),
        "18" | "58" => Some(Lane::Key6),
        "19" | "59" => Some(Lane::Key7),
        // 21..29 / 61..69: the same BMS channel numbers are reused for two
        // unrelated conventions, so their lane depends on the resolved
        // PlayMode: PMS's extra buttons (9K, no real 2nd player) vs. Double
        // Play's 2P side (10K/14K). Any mode that isn't DP/PMS falls through
        // to `None`, letting the caller keep the old BGM-passthrough behavior.
        "21" | "61" if matches!(mode, PlayMode::Keys10 | PlayMode::Keys14) => Some(Lane::P2Key1),
        "22" | "62" => match mode {
            PlayMode::Keys9 => Some(Lane::Key6),
            PlayMode::Keys10 | PlayMode::Keys14 => Some(Lane::P2Key2),
            _ => None,
        },
        "23" | "63" => match mode {
            PlayMode::Keys9 => Some(Lane::Key7),
            PlayMode::Keys10 | PlayMode::Keys14 => Some(Lane::P2Key3),
            _ => None,
        },
        "24" | "64" => match mode {
            PlayMode::Keys9 => Some(Lane::Key8),
            PlayMode::Keys10 | PlayMode::Keys14 => Some(Lane::P2Key4),
            _ => None,
        },
        "25" | "65" => match mode {
            PlayMode::Keys9 => Some(Lane::Key9),
            PlayMode::Keys10 | PlayMode::Keys14 => Some(Lane::P2Key5),
            _ => None,
        },
        "26" | "66" if matches!(mode, PlayMode::Keys10 | PlayMode::Keys14) => Some(Lane::P2Scratch),
        "28" | "68" if mode == PlayMode::Keys14 => Some(Lane::P2Key6),
        "29" | "69" if mode == PlayMode::Keys14 => Some(Lane::P2Key7),
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
        // A mine's value is a damage amount, not an object id, and must not
        // become (or come between) the head and tail of a long note.
        if notes[i].note_type == NoteType::Landmine {
            continue;
        }
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
    fn test_encode_base36() {
        assert_eq!(encode_base36(WavId(1)), "01");
        assert_eq!(encode_base36(WavId(9)), "09");
        assert_eq!(encode_base36(WavId(10)), "0A");
        assert_eq!(encode_base36(WavId(35)), "0Z");
        assert_eq!(encode_base36(WavId(36)), "10");
        assert_eq!(encode_base36(WavId(1295)), "ZZ");

        // Round-trip test for all 1..=1295 IDs
        for id in 1..=1295 {
            let encoded = encode_base36(WavId(id));
            let b = encoded.as_bytes();
            let decoded = decode_base36(b[0], b[1]).unwrap();
            assert_eq!(decoded.0, id);
        }
    }

    #[test]
    fn test_decode_hex() {
        assert_eq!(decode_hex(b'0', b'0'), Some(0));
        assert_eq!(decode_hex(b'9', b'6'), Some(150));
        assert_eq!(decode_hex(b'F', b'F'), Some(255));
        assert_eq!(decode_hex(b'g', b'0'), None);
    }

    #[test]
    fn bpm_change_sorts_before_stop_at_the_same_position() {
        // Stop channel (09) listed before the BPM channel (08) in the file.
        let chart = parse_bms(
            "#BPM 120
#BPM01 240
#STOP01 192
#00109:01
#00108:01
#00111:01
",
        )
        .unwrap();
        assert!(matches!(chart.timing_events[0].kind, TimingEventKind::BpmChange(_)));
        assert!(matches!(chart.timing_events[1].kind, TimingEventKind::StopMeasures(_)));
    }

    #[test]
    fn landmine_channels_become_non_playable_mines() {
        let chart = parse_bms(
            "#BPM 120
#LNOBJ 0A
#00111:01
#001d1:0A
#00111:00000A00
#001E3:01
#00212:01
",
        )
        .unwrap();
        let mines: Vec<_> = chart
            .notes
            .iter()
            .filter(|n| n.note_type == NoteType::Landmine)
            .collect();
        // Case-insensitive D1 on Key1; the E3 mine is dropped (no 2P lanes in a 7K chart).
        assert_eq!(mines.len(), 1);
        assert_eq!(mines[0].lane, Lane::Key1);
        assert_eq!(mines[0].wav_id, Some(WavId(10)));
        // Mines are not notes: not counted, and the LNOBJ pairing ignores them
        // even though the mine's value equals the LNOBJ id.
        assert_eq!(chart.playable_notes_len(), chart.notes.len() - 1);
        assert_eq!(chart.total_notes_count, 3);
        assert!(chart
            .notes
            .iter()
            .any(|n| n.lane == Lane::Key1 && n.note_type == NoteType::LongNoteEnd));
    }

    #[test]
    fn random_sections_are_resolved_before_parsing() {
        let src = "#BPM 120\n#RANDOM 1\n#IF 1\n#00111:01\n#ENDIF\n#IF 2\n#00112:01\n#00113:01\n#ENDIF\n";
        let chart = parse_bms_with_seed(src, 5).unwrap();
        // Only the taken branch's note is in the chart, and the seed is remembered.
        assert_eq!(chart.notes.len(), 1);
        assert_eq!(chart.notes[0].lane, Lane::Key1);
        assert_eq!(chart.random_seed, Some(5));
        // A chart without random sections has no seed to remember.
        assert_eq!(parse_bms("#BPM 120\n#00111:01\n").unwrap().random_seed, None);
    }

    #[test]
    fn same_seed_same_chart_different_seed_can_differ() {
        let src = "#RANDOM 2\n#IF 1\n#00111:01\n#ENDIF\n#IF 2\n#00112:01\n#ENDIF\n";
        let lane_of = |seed| parse_bms_with_seed(src, seed).unwrap().notes[0].lane;
        assert_eq!(lane_of(9), lane_of(9));
        assert!((0..64).any(|s| lane_of(s) != lane_of(0)));
    }

    #[test]
    fn lnmode_is_read_and_only_1_to_3_count() {
        let mode = |text: &str| parse_bms(&format!("#BPM 120\n{text}\n#00111:01\n")).unwrap().header.ln_mode;
        assert_eq!(mode("#LNMODE 1"), Some(1));
        assert_eq!(mode("#lnmode 2"), Some(2));
        assert_eq!(mode("#LNMODE 3"), Some(3));
        assert_eq!(mode("#LNMODE 0"), None);
        assert_eq!(mode("#LNMODE 4"), None);
        assert_eq!(mode("#LNMODE x"), None);
        assert_eq!(mode(""), None);
    }

    #[test]
    fn parses_preview_header() {
        let chart = parse_bms("#TITLE T
#PREVIEW preview.ogg
#00111:01
").unwrap();
        assert_eq!(chart.header.preview, "preview.ogg");
        assert!(parse_bms("#TITLE T
#00111:01
").unwrap().header.preview.is_empty());
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
        assert_eq!(
            chart.header.wav_table.get(&WavId(1)),
            Some(&"kick.wav".to_string())
        );
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
        assert_eq!(
            chart.timing_events[0],
            TimingEvent {
                measure: 1,
                fraction: 0.0,
                kind: TimingEventKind::BpmChange(150.0),
            }
        );
        assert_eq!(
            chart.timing_events[1],
            TimingEvent {
                measure: 1,
                fraction: 0.5,
                kind: TimingEventKind::BpmChange(200.0),
            }
        );
        assert_eq!(
            chart.timing_events[2],
            TimingEvent {
                measure: 1,
                fraction: 0.5,
                kind: TimingEventKind::StopMeasures(0.5),
            }
        );

        // 1P notes check
        assert_eq!(chart.notes.len(), 2);
        assert_eq!(
            chart.notes[0],
            NoteEvent {
                measure: 1,
                fraction: 0.0,
                lane: Lane::Key1,
                wav_id: Some(WavId(1)),
                note_type: NoteType::Tap,
            }
        );
        assert_eq!(
            chart.notes[1],
            NoteEvent {
                measure: 1,
                fraction: 0.5,
                lane: Lane::Scratch,
                wav_id: Some(WavId(2)),
                note_type: NoteType::Tap,
            }
        );
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
        assert_eq!(
            chart.timing_events[0].kind,
            TimingEventKind::BpmChange(175.5)
        );
        assert_eq!(chart.timing_events[1].measure, 2);
        assert_eq!(
            chart.timing_events[1].kind,
            TimingEventKind::StopMeasures(96.0 / 192.0)
        );
    }

    #[test]
    fn declared_4k_6k_8k_modes() {
        let mode = |src: &str| parse_bms(src).unwrap().detect_play_mode();
        assert_eq!(mode("#4K
#00111:01
"), PlayMode::Keys4);
        assert_eq!(mode("#4k
#00111:01
"), PlayMode::Keys4);
        assert_eq!(mode("#6K
#00118:01
"), PlayMode::Keys6);
        assert_eq!(mode("#8K
#00116:01
#00118:01
"), PlayMode::Keys8);
        // Without a declaration the old channel-based detection applies.
        assert_eq!(mode("#00111:01
#00118:01
"), PlayMode::Keys7);
        // 4K/6K/8K are single play, so a stray #PLAYER 3 doesn't turn them into DP.
        assert_eq!(mode("#6K
#PLAYER 3
#00111:01
"), PlayMode::Keys6);
    }

    #[test]
    fn declared_mode_turns_foreign_lanes_into_sound_only() {
        let chart = parse_bms(
            "#4K
#WAV01 a.wav
#WAV02 b.wav
#00111:01
#00113:0200
#00119:01
",
        )
        .unwrap();
        assert_eq!(chart.detect_play_mode(), PlayMode::Keys4);
        // Only the Key1 note is playable; Key3 (13) and Key7 (19) are not 4K lanes.
        assert_eq!(chart.notes.len(), 1);
        assert_eq!(chart.notes[0].lane, Lane::Key1);
        assert_eq!(chart.total_notes_count, 1);
        assert_eq!(chart.bgm_notes.len(), 2);
    }

    #[test]
    fn declared_mode_long_note_on_foreign_lane_is_one_sound() {
        let chart = parse_bms(
            "#6K
#LNTYPE 1
#WAV01 a.wav
#00114:01
#00154:01
#00254:01
#00112:01
",
        )
        .unwrap();
        assert_eq!(chart.detect_play_mode(), PlayMode::Keys6);
        // Key4 isn't a 6K lane: its tap and LN (head + tail) are not notes;
        // sound plays for the tap and the LN head only.
        assert_eq!(chart.notes.len(), 1);
        assert_eq!(chart.notes[0].lane, Lane::Key2);
        assert_eq!(chart.bgm_notes.len(), 2);
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
        assert_eq!(
            chart_pms_hint.detect_play_mode_with_hint(true),
            PlayMode::Keys9
        );

        // 10K with #PLAYER 1 but scratch lane 16 + 2P channel 22 (PMS cannot have scratch)
        let bms_10k_scratch = r#"
#PLAYER 1
#00116:01000000
#00122:02000000
"#;
        let chart_10k_scratch = parse_bms(bms_10k_scratch).unwrap();
        assert_eq!(chart_10k_scratch.detect_play_mode(), PlayMode::Keys10);

        // 10K with #PLAYER 1 and 2P Key 1 (channel 21)
        let bms_10k_key1 = r#"
#PLAYER 1
#00111:01000000
#00121:02000000
"#;
        let chart_10k_key1 = parse_bms(bms_10k_key1).unwrap();
        assert_eq!(chart_10k_key1.detect_play_mode(), PlayMode::Keys10);

        // 14K with #PLAYER 1, 1P Key 6 (18) and 2P channel 22
        let bms_14k_p1 = r#"
#PLAYER 1
#00118:01000000
#00122:02000000
"#;
        let chart_14k_p1 = parse_bms(bms_14k_p1).unwrap();
        assert_eq!(chart_14k_p1.detect_play_mode(), PlayMode::Keys14);
    }

    #[test]
    fn test_2p_dp_notes_are_now_judgeable_not_bgm() {
        // Milestone 10 Phase 1: 2P/DP channels must become real judgeable
        // notes on P2* lanes instead of silently degrading to bgm_notes.
        let bms = r#"
#PLAYER 3
#00111:01000000
#00121:02000000
#00131:03000000
#00161:04000000
"#;
        let chart = parse_bms(bms).expect("Failed to parse DP chart with invisible notes");
        assert_eq!(chart.detect_play_mode(), PlayMode::Keys10);
        // 1P Key1 tap + 2P P2Key1 tap are both real, judgeable notes now.
        assert_eq!(chart.notes.len(), 2);
        assert!(chart.notes.iter().any(|n| n.lane == Lane::Key1));
        assert!(chart.notes.iter().any(|n| n.lane == Lane::P2Key1));
        // The lone, unpaired channel-61 LN-start event has no matching end
        // in this fixture, so it is correctly dropped rather than becoming
        // a phantom note (matches pre-existing LNTYPE1 pairing behavior).
        assert_eq!(chart.bgm_notes.len(), 0);
        assert_eq!(chart.freezone_notes.len(), 1); // invisible note routed to freezone_notes
        assert_eq!(chart.total_notes_count, 2); // 1P tap + 2P tap (unpaired LN uncounted)
    }

    #[test]
    fn test_dp_14k_all_2p_lanes_and_long_notes_judgeable() {
        // Full 14K (7+7 Double Play) chart: every 2P channel (21..29, LN
        // pairs on 61..69) must map to a distinct P2* lane.
        let bms = r#"
#PLAYER 3
#00111:01
#00112:01
#00113:01
#00114:01
#00115:01
#00118:01
#00119:01
#00121:01
#00122:01
#00123:01
#00124:01
#00125:01
#00126:01
#00128:01
#00129:01
#00161:02
#00161:03
"#;
        let chart = parse_bms(bms).expect("Failed to parse 14K DP chart");
        assert_eq!(chart.detect_play_mode(), PlayMode::Keys14);
        assert_eq!(
            chart.bgm_notes.len(),
            0,
            "no 2P channel should fall back to BGM in true DP mode"
        );

        let p2_lanes = [
            Lane::P2Key1,
            Lane::P2Key2,
            Lane::P2Key3,
            Lane::P2Key4,
            Lane::P2Key5,
            Lane::P2Scratch,
            Lane::P2Key6,
            Lane::P2Key7,
        ];
        for lane in p2_lanes {
            assert!(
                chart.notes.iter().any(|n| n.lane == lane),
                "missing judgeable note on {lane:?}"
            );
        }
        // Channel 61 appears twice -> one completed LongNoteStart/End pair on P2Key1.
        assert!(chart
            .notes
            .iter()
            .any(|n| n.lane == Lane::P2Key1 && n.note_type == NoteType::LongNoteEnd));
    }

    #[test]
    fn test_pms_9k_extra_buttons_are_judgeable_key6_to_9() {
        // PMS (9K): 1P channels 11..15 = buttons 1..5, 2P-numbered channels
        // 22..25 are REUSED as buttons 6..9 (no real second player).
        let bms = r#"
#00111:01
#00112:01
#00113:01
#00114:01
#00115:01
#00122:01
#00123:01
#00124:01
#00125:01
"#;
        let chart = parse_bms(bms).expect("Failed to parse PMS 9K chart");
        assert_eq!(chart.detect_play_mode(), PlayMode::Keys9);
        assert_eq!(chart.bgm_notes.len(), 0);

        for lane in [
            Lane::Key1,
            Lane::Key2,
            Lane::Key3,
            Lane::Key4,
            Lane::Key5,
            Lane::Key6,
            Lane::Key7,
            Lane::Key8,
            Lane::Key9,
        ] {
            assert!(
                chart.notes.iter().any(|n| n.lane == lane),
                "missing judgeable note on {lane:?}"
            );
        }
        // Must not leak into Double Play lanes.
        assert!(chart.notes.iter().all(|n| {
            !matches!(
                n.lane,
                Lane::P2Scratch
                    | Lane::P2Key1
                    | Lane::P2Key2
                    | Lane::P2Key3
                    | Lane::P2Key4
                    | Lane::P2Key5
                    | Lane::P2Key6
                    | Lane::P2Key7
            )
        }));
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
        assert_eq!(
            chart.header.bmp_table.get(&BmpId(1)),
            Some(&"bg.bmp".to_string())
        );
        assert_eq!(
            chart.header.bga_table.get(&BmpId(4)),
            Some(&BgaDefinition {
                bmp_id: BmpId(1),
                sx: 0,
                sy: 0,
                w: 256,
                h: 256,
                dx: 0,
                dy: 0,
            })
        );
        assert_eq!(chart.bga_events.len(), 3);
        assert_eq!(
            chart.bga_events[0],
            BgaEvent {
                measure: 1,
                fraction: 0.0,
                channel: BgaChannel::Base,
                bmp_id: BmpId(1),
            }
        );
        assert_eq!(
            chart.bga_events[1],
            BgaEvent {
                measure: 1,
                fraction: 0.0,
                channel: BgaChannel::Poor,
                bmp_id: BmpId(2),
            }
        );
        assert_eq!(
            chart.bga_events[2],
            BgaEvent {
                measure: 1,
                fraction: 0.0,
                channel: BgaChannel::Layer,
                bmp_id: BmpId(3),
            }
        );
    }

    #[test]
    fn test_decode_bms_text_utf8_and_sjis() {
        // UTF-8 test
        let utf8_bytes = "Hello World, 音ゲー #TITLE".as_bytes();
        assert_eq!(decode_bms_text(utf8_bytes), "Hello World, 音ゲー #TITLE");

        // Shift-JIS test (0x8C 0xDC = 五, 0x82 0xC2 = つ)
        let sjis_bytes: &[u8] = &[0x8C, 0xDC, 0x82, 0xC2];
        let decoded = decode_bms_text(sjis_bytes);
        #[cfg(target_os = "windows")]
        assert_eq!(decoded, "五つ");
        #[cfg(not(target_os = "windows"))]
        assert!(!decoded.is_empty());
    }
}

use std::collections::HashMap;

/// BMS `#WAVxx` sound identifier (Base36 / Hex).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WavId(pub u16);

/// BMS `#BMPxx` picture/bga identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BmpId(pub u16);

/// Key lanes supported by Beetle (focusing on 7K + 1S first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, Default)]
pub struct BmsHeader {
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub genre: String,
    pub bpm: f64,
    pub play_level: u32,
    pub total: f64,
    pub wav_table: HashMap<WavId, String>,
}

/// Timing event occurred at a specific measure and fraction.
#[derive(Debug, Clone, PartialEq)]
pub enum TimingEventKind {
    BpmChange(f64),
    StopBeats(f64),
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
}

/// Errors that can occur when parsing BMS text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BmsParseError {
    EmptyChart,
    InvalidHeader(String),
    InvalidMeasure(String),
    InvalidChannel(String),
    UnknownEncoding,
}

impl std::fmt::Display for BmsParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyChart => write!(f, "BMS chart content is empty"),
            Self::InvalidHeader(msg) => write!(f, "Invalid header command: {msg}"),
            Self::InvalidMeasure(msg) => write!(f, "Invalid measure format: {msg}"),
            Self::InvalidChannel(msg) => write!(f, "Invalid channel format: {msg}"),
            Self::UnknownEncoding => write!(f, "Unable to decode text encoding"),
        }
    }
}

impl std::error::Error for BmsParseError {}

/// Skeleton BMS text parser. Full parser logic will be implemented in Phase 1.
pub fn parse_bms(_input: &str) -> Result<BmsChart, BmsParseError> {
    // TODO: Implement BMS parser in Phase 1
    Ok(BmsChart::default())
}

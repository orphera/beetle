//! # beetle-core
//!
//! Pure BMS parser, timing calculations, rhythm game judgment logic,
//! note lane modifiers, song library indexing, and flat-file score management.
//! Designed with zero GUI/Audio dependencies for maximum portability and fast testing.

pub mod bms;
mod escape;
pub mod identity;
pub mod judge;
pub mod library;
pub mod modifier;
pub mod replay;
pub mod resolver;
pub mod rules;
pub mod score;
pub mod table;
pub mod timing;

pub use bms::{
    decode_base36, decode_bms_text, encode_base36, parse_bms, parse_bms_with_seed, BgaChannel, BgaDefinition, BgaEvent,
    BmpId, BmsChart, BmsHeader, BmsParseError, Lane, NoteEvent, NoteType, PlayMode, WavId,
};
pub use identity::{
    hash_chart_bytes, md5_from_hex, md5_of_bytes, md5_to_hex, ChartId, ChartKey,
};
pub use judge::{
    GaugeType, JudgeEngine, JudgeGrade, JudgeResult, JudgeWindow, PlayNote, ScoreTracker,
};
pub use library::{
    compute_chart_hash, deserialize_song_cache, serialize_song_cache, sort_songs, SongMetadata,
    SortMode,
};
pub use modifier::{apply_lane_modifier, LaneModifier, PlayOptions};
pub use replay::{ReplayData, ReplayEvent};
pub use rules::{LnOption, LnRule, Ruleset};
pub use score::{ClearType, PlayResult, ScoreRecord, ScoreStore, ScoreUpdate, ENGINE_VERSION};
pub use table::{DifficultyTable, TableEntry, TableIndex, TableMatch};
pub use timing::{TimingModel, TimingSegment};

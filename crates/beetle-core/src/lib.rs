//! # beetle-core
//!
//! Pure BMS parser, timing calculations, rhythm game judgment logic,
//! song library indexing, and flat-file score management.
//! Designed with zero GUI/Audio dependencies for maximum portability and fast testing.

pub mod bms;
pub mod judge;
pub mod library;
pub mod score;
pub mod timing;

pub use bms::{parse_bms, BmsChart, BmsHeader, BmsParseError, Lane, NoteEvent, NoteType, WavId};
pub use judge::{GaugeType, JudgeEngine, JudgeGrade, JudgeResult, JudgeWindow, PlayNote, ScoreTracker};
pub use library::{
    compute_chart_hash, deserialize_song_cache, serialize_song_cache, SongMetadata,
};
pub use score::{ClearType, ScoreRecord, ScoreStore};
pub use timing::{TimingModel, TimingSegment};

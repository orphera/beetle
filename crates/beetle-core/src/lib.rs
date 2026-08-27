//! # beetle-core
//!
//! Pure BMS parser, timing calculations, and rhythm game judgment logic.
//! Designed with zero GUI/Audio dependencies for maximum portability and fast testing.

pub mod bms;
pub mod judge;
pub mod timing;

pub use bms::{parse_bms, BmsChart, BmsHeader, BmsParseError, Lane, NoteEvent, NoteType, WavId};
pub use judge::{GaugeType, JudgeEngine, JudgeGrade, JudgeResult, JudgeWindow, PlayNote, ScoreTracker};
pub use timing::{TimingModel, TimingSegment};

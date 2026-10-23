//! # mpeg-video
//!
//! MPEG-1 and MPEG-2 video decoding with no dependencies, for BGA movies:
//! `.mpg` program streams of BMS songs and the PS2 IIDX discs' backgrounds.
//! Windows' own MPEG-2 decoder is an optional Store extension, so the player
//! cannot count on it.
//!
//! ```no_run
//! let file = std::fs::read("bga.mpg").unwrap();
//! let mut decoder = mpeg_video::Decoder::new(mpeg_video::ps::video_stream(&file));
//! while let Some(Ok(frame)) = decoder.next_frame() {
//!     let _rgba = frame.to_rgba();
//! }
//! ```

mod bits;
mod decoder;
mod idct;
pub mod ps;
mod vlc;

pub use decoder::{Decoder, Error, Frame};

//! # cs-disc
//!
//! Reads songs straight off console (CS) rhythm game discs, so the player can
//! list and play them like installed BMS without extracting anything first.
//!
//! Layers, bottom up:
//! - `sector`: whole 2048-byte sectors from a disc image file or a physical
//!   optical drive (`\\.\E:`), behind one reader.
//! - `iso9660`: the disc's file system.
//! - `ps2`: which game a PS2 disc is (its boot executable from `SYSTEM.CNF`).
//! - one module per game (`iidx16`), knowing where that game keeps its songs.
//!
//! The crate has no dependencies and no player code: it hands out bytes and
//! plain song records; turning them into charts and sounds is the caller's job.
//! Plan: `docs/plans/2026-10-23-disc-direct-playback.md`.

pub mod adpcm;
mod blowfish;
mod blowfish_pi;
pub mod drive;
pub mod iidx16;
pub mod iso9660;
pub mod lz;
pub mod ply;
pub mod ply_bms;
pub mod ps2;
pub mod sector;
pub mod wvb;

pub use iso9660::{FileExtent, Volume};
pub use sector::{SectorReader, SECTOR_SIZE};

use std::io::{self, Read, Seek};

/// A disc whose game this crate knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Game {
    Iidx16(iidx16::Edition),
}

impl Game {
    /// The game a PS2 boot executable name belongs to (`SLPM_552.21`).
    pub fn from_ps2_executable(name: &str) -> Option<Self> {
        iidx16::Edition::from_executable(name).map(Game::Iidx16)
    }

    pub fn name(self) -> &'static str {
        match self {
            Game::Iidx16(edition) => edition.name(),
        }
    }
}

/// Which known game is on the disc, or `None` for any other disc (not PS2, or
/// a game this crate does not read).
pub fn identify<R: Read + Seek>(disc: &mut SectorReader<R>) -> io::Result<Option<Game>> {
    let volume = Volume::open(disc)?;
    let Some(exe) = ps2::boot_executable(disc, &volume)? else {
        return Ok(None);
    };
    Ok(Game::from_ps2_executable(&exe))
}

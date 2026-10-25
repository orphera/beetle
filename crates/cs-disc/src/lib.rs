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
//! - one module per game (`iidx15`, which also reads 14 GOLD, and `iidx16`),
//!   knowing where that game keeps its songs.
//!
//! The crate has no dependencies and no player code: it hands out bytes and
//! plain song records; turning them into charts and sounds is the caller's job.
//! Plan: `docs/plans/2026-10-23-disc-direct-playback.md`.

pub mod adpcm;
mod blowfish;
mod blowfish_pi;
pub mod drive;
pub mod iidx15;
pub mod iidx16;
pub mod iso9660;
pub mod lz;
pub mod meta;
pub mod ply;
pub mod ply_bms;
pub mod ps2;
pub mod ps2tex;
pub mod sector;
pub mod titles;
pub mod wvb;

pub use iso9660::{FileExtent, Volume};
pub use sector::{SectorReader, SECTOR_SIZE};

use std::io::{self, Read, Seek};

/// A disc whose game this crate knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Game {
    Iidx15(iidx15::Edition),
    Iidx16(iidx16::Edition),
}

impl Game {
    /// The game a PS2 boot executable name belongs to (`SLPM_552.21`).
    pub fn from_ps2_executable(name: &str) -> Option<Self> {
        iidx15::Edition::from_executable(name)
            .map(Game::Iidx15)
            .or_else(|| iidx16::Edition::from_executable(name).map(Game::Iidx16))
    }

    pub fn name(self) -> &'static str {
        match self {
            Game::Iidx15(edition) => edition.name(),
            Game::Iidx16(edition) => edition.name(),
        }
    }
}

/// Whether `target` names a disc: a disc image file (`.iso`) or a drive (`E:`).
pub fn is_disc_target(target: &str) -> bool {
    drive::as_drive_letter(target).is_some()
        || std::path::Path::new(target)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("iso"))
}

/// Opens a disc image file, or a drive's disc when `target` is a drive (`E:`).
pub fn open(target: &str) -> io::Result<SectorReader<std::fs::File>> {
    match drive::as_drive_letter(target) {
        Some(letter) => drive::open(letter),
        None => SectorReader::open_image(target),
    }
}

/// The listed title of a PS2 disc (`titles`), playable or not: what to call
/// a disc `identify` does not read. `None` for any other disc.
pub fn identify_title<R: Read + Seek>(
    disc: &mut SectorReader<R>,
) -> io::Result<Option<&'static titles::Title>> {
    let volume = Volume::open(disc)?;
    Ok(ps2::boot_executable(disc, &volume)?.and_then(|exe| titles::find(&exe)))
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

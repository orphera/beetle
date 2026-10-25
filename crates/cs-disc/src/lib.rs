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
//! - `iidx`: where each IIDX edition (10th style to 16 EMPRESS) keeps its songs,
//!   and `popn` the same for pop'n music 12 to 14 through the same `iidx::Disc`.
//!
//! The crate has no dependencies and no player code: it hands out bytes and
//! plain song records; turning them into charts and sounds is the caller's job.
//! Plan: `docs/plans/2026-10-23-disc-direct-playback.md`.

pub mod adpcm;
mod blowfish;
mod blowfish_pi;
pub mod drive;
pub mod iidx;
pub mod iso9660;
pub mod lz;
pub mod ply;
pub mod ply_bms;
pub mod popn;
pub mod ps2;
pub mod ps2tex;
pub mod sector;
pub mod titles;
pub mod wvb;

pub use iidx::Edition;
pub use iso9660::{FileExtent, Volume};
pub use sector::{SectorReader, SECTOR_SIZE};

use std::io::{self, Read, Seek};

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

/// Which edition this crate reads is on the disc, or `None` for any other
/// disc (not PS2, or a game this crate does not read).
pub fn identify<R: Read + Seek>(disc: &mut SectorReader<R>) -> io::Result<Option<Edition>> {
    let volume = Volume::open(disc)?;
    let Some(exe) = ps2::boot_executable(disc, &volume)? else {
        return Ok(None);
    };
    Ok(Edition::from_executable(&exe))
}

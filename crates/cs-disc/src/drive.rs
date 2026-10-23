//! Physical optical drives (Windows): which drive letters are CD/DVD drives,
//! and opening one as a raw volume to read its sectors.
//!
//! Reading a raw CD-ROM volume does not need administrator rights.

use std::fs::File;
use std::io;

use crate::sector::SectorReader;

/// Opens drive `letter` (`'E'`) as a raw volume.
pub fn open(letter: char) -> io::Result<SectorReader<File>> {
    let letter = letter.to_ascii_uppercase();
    if !letter.is_ascii_uppercase() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a drive letter",
        ));
    }
    Ok(SectorReader::new(File::open(format!(r"\\.\{letter}:"))?))
}

/// The drive letter a path names when it is a bare drive (`E:`, `E:\`, `E:/`).
pub fn as_drive_letter(path: &str) -> Option<char> {
    let mut chars = path.chars();
    let letter = chars.next().filter(char::is_ascii_alphabetic)?;
    (chars.next() == Some(':') && matches!(chars.as_str(), "" | "\\" | "/"))
        .then(|| letter.to_ascii_uppercase())
}

/// The letters of the CD/DVD drives on this machine, in order.
#[cfg(windows)]
pub fn optical_drives() -> Vec<char> {
    const DRIVE_CDROM: u32 = 5;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLogicalDrives() -> u32;
        fn GetDriveTypeW(root: *const u16) -> u32;
    }
    // SAFETY: GetLogicalDrives takes no arguments.
    let mask = unsafe { GetLogicalDrives() };
    (0..26u8)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| (b'A' + i) as char)
        .filter(|&letter| {
            let root: Vec<u16> = format!("{letter}:\\").encode_utf16().chain([0]).collect();
            // SAFETY: `root` is a NUL-terminated UTF-16 string that outlives the call.
            unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_CDROM }
        })
        .collect()
}

#[cfg(not(windows))]
pub fn optical_drives() -> Vec<char> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_bare_drive_is_a_drive_letter() {
        assert_eq!(as_drive_letter("e:"), Some('E'));
        assert_eq!(as_drive_letter("E:\\"), Some('E'));
        assert_eq!(as_drive_letter("E:/"), Some('E'));
        assert_eq!(as_drive_letter("E:/x.iso"), None);
        assert_eq!(as_drive_letter("x.iso"), None);
    }
}

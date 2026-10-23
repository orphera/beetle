//! PlayStation 2 discs: the boot executable named in `SYSTEM.CNF`, which is
//! the disc's product code (`SLPM_552.21`) and tells the games apart.

use std::io::{self, Read, Seek};

use crate::iso9660::Volume;
use crate::sector::SectorReader;

/// The boot executable's file name, or `None` when the disc has no PS2
/// `SYSTEM.CNF` (a PS1 disc names it with `BOOT`, not `BOOT2`).
pub fn boot_executable<R: Read + Seek>(
    disc: &mut SectorReader<R>,
    volume: &Volume,
) -> io::Result<Option<String>> {
    let Some(cnf) = volume.read_file(disc, "SYSTEM.CNF")? else {
        return Ok(None);
    };
    Ok(parse_boot2(&String::from_utf8_lossy(&cnf)))
}

/// `BOOT2 = cdrom0:\SLPM_552.21;1` → `SLPM_552.21`.
fn parse_boot2(cnf: &str) -> Option<String> {
    cnf.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        if !key.trim().eq_ignore_ascii_case("BOOT2") {
            return None;
        }
        let path = value.trim();
        let name = path.rsplit(['\\', '/', ':']).next()?;
        let name = name.split(';').next()?.trim();
        (!name.is_empty()).then(|| name.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iso9660::test_image;
    use std::io::Cursor;

    #[test]
    fn boot2_names_the_executable() {
        assert_eq!(
            parse_boot2("BOOT2 = cdrom0:\\SLPM_552.21;1\r\nVER = 1.00\r\nVMODE = NTSC\r\n"),
            Some("SLPM_552.21".to_string())
        );
        assert_eq!(parse_boot2("BOOT = cdrom:\\SLPS_000.01;1"), None);
    }

    #[test]
    fn the_executable_is_read_from_the_disc() {
        let mut disc = SectorReader::new(Cursor::new(test_image::build(
            &[("SYSTEM.CNF", b"BOOT2 = cdrom0:\\SLPM_552.22;1\n")],
            &[],
        )));
        let volume = Volume::open(&mut disc).unwrap();
        assert_eq!(
            boot_executable(&mut disc, &volume).unwrap().as_deref(),
            Some("SLPM_552.22")
        );
    }
}

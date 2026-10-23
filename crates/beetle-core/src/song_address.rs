//! Where a song's chart is, as written in `SongMetadata::file_path`.
//!
//! - A chart file in a folder: its path (`D:/BMS/song/x.bms`).
//! - An entry of a `.bmsp` package: `<package>::<entry>`.
//! - A chart read off a game disc: `disc:<image>::<chart>`, where `<image>` is
//!   a disc image file or a drive (`E:`), and
//!   `<chart>` is the disc reader's own name for the chart, written like a path
//!   (`<song>/<chart>`) so the song's folder is the part before the last `/`.
//!
//! Every reader of `file_path` goes through `SongAddress::parse`, so a new
//! kind of source is one more variant here.

const DISC_PREFIX: &str = "disc:";
const SEPARATOR: &str = "::";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongAddress<'a> {
    /// A chart file in a folder.
    Folder { file: &'a str },
    /// An entry of a `.bmsp` package.
    Package { bmsp: &'a str, entry: &'a str },
    /// A chart on a game disc image.
    Disc { image: &'a str, chart: &'a str },
}

impl<'a> SongAddress<'a> {
    pub fn parse(file_path: &'a str) -> Self {
        if let Some((image, chart)) = file_path
            .strip_prefix(DISC_PREFIX)
            // The last separator: a drive's image is `E:`, so `E:::017/…`.
            .and_then(|rest| rest.rsplit_once(SEPARATOR))
        {
            return SongAddress::Disc { image, chart };
        }
        match file_path.split_once(SEPARATOR) {
            Some((bmsp, entry)) => SongAddress::Package { bmsp, entry },
            None => SongAddress::Folder { file: file_path },
        }
    }

    /// The path inside the container: the entry, the disc chart, or the file itself.
    pub fn inner(&self) -> &'a str {
        match *self {
            SongAddress::Folder { file } => file,
            SongAddress::Package { entry, .. } => entry,
            SongAddress::Disc { chart, .. } => chart,
        }
    }

    /// The file on disk that holds the chart: the chart file, the package or the disc image.
    pub fn container(&self) -> &'a str {
        match *self {
            SongAddress::Folder { file } => file,
            SongAddress::Package { bmsp, .. } => bmsp,
            SongAddress::Disc { image, .. } => image,
        }
    }
}

/// The address of `entry` in the package at `bmsp`.
pub fn package_address(bmsp: &str, entry: &str) -> String {
    format!("{bmsp}{SEPARATOR}{entry}")
}

/// The address of `chart` on the disc image at `image`.
pub fn disc_address(image: &str, chart: &str) -> String {
    format!("{DISC_PREFIX}{image}{SEPARATOR}{chart}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_path_is_a_folder_chart() {
        assert_eq!(
            SongAddress::parse("D:/BMS/song/x.bms"),
            SongAddress::Folder {
                file: "D:/BMS/song/x.bms"
            }
        );
    }

    #[test]
    fn a_package_entry_splits_at_the_separator() {
        let address = package_address("D:/p/package.bmsp", "a/x.bms");
        assert_eq!(
            SongAddress::parse(&address),
            SongAddress::Package {
                bmsp: "D:/p/package.bmsp",
                entry: "a/x.bms"
            }
        );
    }

    #[test]
    fn a_disc_chart_keeps_the_drive_letter_of_its_image() {
        let address = disc_address("D:/iso/iidx16.iso", "017/sp_another");
        assert_eq!(address, "disc:D:/iso/iidx16.iso::017/sp_another");
        let parsed = SongAddress::parse(&address);
        assert_eq!(
            parsed,
            SongAddress::Disc {
                image: "D:/iso/iidx16.iso",
                chart: "017/sp_another"
            }
        );
        assert_eq!(parsed.container(), "D:/iso/iidx16.iso");
        assert_eq!(parsed.inner(), "017/sp_another");
    }

    #[test]
    fn a_disc_in_a_drive_is_its_drive_letter() {
        let address = disc_address("E:", "017/sp_another");
        assert_eq!(
            SongAddress::parse(&address),
            SongAddress::Disc {
                image: "E:",
                chart: "017/sp_another"
            }
        );
    }

    #[test]
    fn the_disc_prefix_without_a_chart_is_not_a_disc() {
        assert_eq!(
            SongAddress::parse("disc:x.iso"),
            SongAddress::Folder { file: "disc:x.iso" }
        );
    }
}

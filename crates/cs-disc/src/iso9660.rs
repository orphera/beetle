//! The ISO 9660 file system: finding a file's extent by path.
//!
//! Only what game discs need: the primary volume descriptor, directories and
//! single-extent files. Names compare without case and without the `;1`
//! version suffix.

use std::io::{self, Read, Seek};

use crate::sector::{SectorReader, SECTOR_SIZE};

/// Where a file's bytes are on the disc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileExtent {
    pub lba: u64,
    pub size: u64,
}

impl FileExtent {
    /// Byte offset of the file's start on the disc.
    pub fn offset(&self) -> u64 {
        self.lba * SECTOR_SIZE
    }
}

/// One entry of a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// The name without its `;1` version suffix.
    pub name: String,
    pub extent: FileExtent,
    pub is_dir: bool,
}

/// A mounted ISO 9660 volume: its root directory.
#[derive(Debug, Clone, Copy)]
pub struct Volume {
    root: FileExtent,
}

const PVD_LBA: u64 = 16;

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_string())
}

fn u32_le(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

impl Volume {
    /// Reads the primary volume descriptor.
    pub fn open<R: Read + Seek>(disc: &mut SectorReader<R>) -> io::Result<Self> {
        let pvd = disc.read_sectors(PVD_LBA, 1)?;
        if pvd[0] != 1 || &pvd[1..6] != b"CD001" {
            return Err(bad("not an ISO 9660 volume"));
        }
        let root = &pvd[156..190];
        Ok(Self {
            root: FileExtent {
                lba: u32_le(root, 2) as u64,
                size: u32_le(root, 10) as u64,
            },
        })
    }

    /// The entries of the directory at `extent`, without `.` and `..`.
    pub fn list<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        extent: FileExtent,
    ) -> io::Result<Vec<DirEntry>> {
        let data = disc.read_at(extent.offset(), extent.size as usize)?;
        let mut entries = Vec::new();
        let mut i = 0;
        while i < data.len() {
            let len = data[i] as usize;
            if len == 0 {
                // Records never cross a sector: the rest of this one is padding.
                i = (i / SECTOR_SIZE as usize + 1) * SECTOR_SIZE as usize;
                continue;
            }
            let Some(record) = data.get(i..i + len) else {
                break;
            };
            i += len;
            if record.len() < 34 {
                continue;
            }
            let name_len = record[32] as usize;
            let Some(raw) = record.get(33..33 + name_len) else {
                continue;
            };
            if raw == [0] || raw == [1] {
                continue;
            }
            let name = String::from_utf8_lossy(raw);
            let name = name.split(';').next().unwrap_or("").to_string();
            entries.push(DirEntry {
                name,
                extent: FileExtent {
                    lba: u32_le(record, 2) as u64,
                    size: u32_le(record, 10) as u64,
                },
                is_dir: record[25] & 2 != 0,
            });
        }
        Ok(entries)
    }

    /// The entries of the root directory.
    pub fn root<R: Read + Seek>(&self, disc: &mut SectorReader<R>) -> io::Result<Vec<DirEntry>> {
        self.list(disc, self.root)
    }

    /// The file at `path` (`/`- or `\`-separated, any case), or `None`.
    pub fn find<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        path: &str,
    ) -> io::Result<Option<FileExtent>> {
        let mut dir = self.root;
        let mut parts = path.split(['/', '\\']).filter(|p| !p.is_empty()).peekable();
        while let Some(part) = parts.next() {
            let last = parts.peek().is_none();
            let found = self
                .list(disc, dir)?
                .into_iter()
                .find(|e| e.name.eq_ignore_ascii_case(part) && e.is_dir != last);
            match found {
                Some(entry) if last => return Ok(Some(entry.extent)),
                Some(entry) => dir = entry.extent,
                None => return Ok(None),
            }
        }
        Ok(None)
    }

    /// The directory at `path` (`/`- or `\`-separated, any case), or `None`.
    pub fn find_dir<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        path: &str,
    ) -> io::Result<Option<FileExtent>> {
        let mut dir = self.root;
        for part in path.split(['/', '\\']).filter(|p| !p.is_empty()) {
            let found = self
                .list(disc, dir)?
                .into_iter()
                .find(|e| e.is_dir && e.name.eq_ignore_ascii_case(part));
            match found {
                Some(entry) => dir = entry.extent,
                None => return Ok(None),
            }
        }
        Ok(Some(dir))
    }

    /// The whole file at `path`, or `None`.
    pub fn read_file<R: Read + Seek>(
        &self,
        disc: &mut SectorReader<R>,
        path: &str,
    ) -> io::Result<Option<Vec<u8>>> {
        match self.find(disc, path)? {
            Some(extent) => disc
                .read_at(extent.offset(), extent.size as usize)
                .map(Some),
            None => Ok(None),
        }
    }
}

/// Builds small ISO 9660 images for tests.
#[cfg(test)]
pub(crate) mod test_image {
    use super::*;

    /// A file in a test image: (name, bytes).
    pub type File<'a> = (&'a str, &'a [u8]);

    /// An image with `files` (name, bytes) in the root and `dirs` (name, files)
    /// one level down.
    pub fn build(files: &[File], dirs: &[(&str, &[File])]) -> Vec<u8> {
        let sector = SECTOR_SIZE as usize;
        let mut image = vec![0u8; sector * 18];
        let mut next_lba = 18u32;
        let mut put = |image: &mut Vec<u8>, bytes: &[u8]| -> FileExtent {
            let lba = next_lba;
            let sectors = bytes.len().div_ceil(sector).max(1);
            image.resize((lba as usize + sectors) * sector, 0);
            image[lba as usize * sector..][..bytes.len()].copy_from_slice(bytes);
            next_lba += sectors as u32;
            FileExtent {
                lba: lba as u64,
                size: bytes.len() as u64,
            }
        };
        let record = |name: &[u8], extent: FileExtent, dir: bool| -> Vec<u8> {
            let mut r = vec![0u8; 33 + name.len() + (1 - name.len() % 2)];
            r[0] = r.len() as u8;
            r[2..6].copy_from_slice(&(extent.lba as u32).to_le_bytes());
            r[10..14].copy_from_slice(&(extent.size as u32).to_le_bytes());
            r[25] = if dir { 2 } else { 0 };
            r[32] = name.len() as u8;
            r[33..33 + name.len()].copy_from_slice(name);
            r
        };
        let dir_bytes = |entries: Vec<(String, FileExtent, bool)>| -> Vec<u8> {
            let mut out = record(&[0], FileExtent { lba: 0, size: 0 }, true);
            out.extend(record(&[1], FileExtent { lba: 0, size: 0 }, true));
            for (name, extent, dir) in entries {
                out.extend(record(name.as_bytes(), extent, dir));
            }
            out
        };

        let mut root_entries = Vec::new();
        for (name, bytes) in files {
            root_entries.push((format!("{name};1"), put(&mut image, bytes), false));
        }
        for (dir, inner) in dirs {
            let mut entries = Vec::new();
            for (name, bytes) in *inner {
                entries.push((format!("{name};1"), put(&mut image, bytes), false));
            }
            let listing = dir_bytes(entries);
            root_entries.push((dir.to_string(), put(&mut image, &listing), true));
        }
        let root_listing = dir_bytes(root_entries);
        let root = put(&mut image, &root_listing);

        let pvd = &mut image[16 * sector..17 * sector];
        pvd[0] = 1;
        pvd[1..6].copy_from_slice(b"CD001");
        pvd[156..190].copy_from_slice(&record(&[0], root, true)[..34]);
        image
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn disc() -> SectorReader<Cursor<Vec<u8>>> {
        let big = vec![7u8; 5000];
        SectorReader::new(Cursor::new(test_image::build(
            &[("SYSTEM.CNF", b"BOOT2 = cdrom0:\\SLPM_552.21;1\r\n")],
            &[("DATA", &[("BIG.DAT", &big), ("SMALL.DAT", b"abc")])],
        )))
    }

    #[test]
    fn a_file_is_found_by_path_in_any_case() {
        let mut disc = disc();
        let volume = Volume::open(&mut disc).unwrap();
        assert_eq!(
            volume.read_file(&mut disc, "data/small.dat").unwrap(),
            Some(b"abc".to_vec())
        );
        assert_eq!(
            volume
                .read_file(&mut disc, "\\DATA\\BIG.DAT")
                .unwrap()
                .unwrap(),
            vec![7u8; 5000]
        );
    }

    #[test]
    fn a_directory_is_not_a_file_and_a_missing_name_is_none() {
        let mut disc = disc();
        let volume = Volume::open(&mut disc).unwrap();
        assert_eq!(volume.find(&mut disc, "DATA").unwrap(), None);
        assert_eq!(volume.find(&mut disc, "DATA/NONE.DAT").unwrap(), None);
    }

    #[test]
    fn the_root_lists_names_without_their_version() {
        let mut disc = disc();
        let volume = Volume::open(&mut disc).unwrap();
        let names: Vec<String> = volume
            .root(&mut disc)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, ["SYSTEM.CNF", "DATA"]);
    }

    #[test]
    fn a_disc_without_a_volume_descriptor_is_refused() {
        let mut disc = SectorReader::new(Cursor::new(vec![0u8; 2048 * 17]));
        assert!(Volume::open(&mut disc).is_err());
    }
}

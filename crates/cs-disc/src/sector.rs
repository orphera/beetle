//! Whole sectors from a disc.
//!
//! A raw optical drive (`\\.\E:` on Windows) only accepts reads that start and
//! end on a sector boundary, so every read here is widened to whole sectors
//! and cut back afterwards. A disc image file is read the same way, which keeps
//! one code path for both.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// Bytes in one data sector (ISO 9660 / Mode 1 / DVD).
pub const SECTOR_SIZE: u64 = 2048;

/// Reads sector-aligned spans of a disc.
pub struct SectorReader<R> {
    inner: R,
}

impl SectorReader<File> {
    /// A disc image file (`.iso`).
    pub fn open_image(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self::new(File::open(path)?))
    }
}

impl<R: Read + Seek> SectorReader<R> {
    pub fn new(inner: R) -> Self {
        Self { inner }
    }

    /// `count` sectors from `lba`.
    pub fn read_sectors(&mut self, lba: u64, count: u64) -> io::Result<Vec<u8>> {
        let mut buf = vec![0; (count * SECTOR_SIZE) as usize];
        self.inner.seek(SeekFrom::Start(lba * SECTOR_SIZE))?;
        self.inner.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// `len` bytes from byte `offset` of the disc, read as whole sectors.
    pub fn read_at(&mut self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        if len == 0 {
            return Ok(Vec::new());
        }
        let first = offset / SECTOR_SIZE;
        let end = offset + len as u64;
        let last = end.div_ceil(SECTOR_SIZE);
        let mut buf = self.read_sectors(first, last - first)?;
        let skip = (offset - first * SECTOR_SIZE) as usize;
        buf.drain(..skip);
        buf.truncate(len);
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A stream that, like a raw drive, refuses reads not on sector boundaries.
    struct AlignedOnly(Cursor<Vec<u8>>);

    impl Read for AlignedOnly {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let at = self.0.position();
            if !at.is_multiple_of(SECTOR_SIZE) || !(buf.len() as u64).is_multiple_of(SECTOR_SIZE) {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "unaligned"));
            }
            self.0.read(buf)
        }
    }

    impl Seek for AlignedOnly {
        fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
            self.0.seek(pos)
        }
    }

    fn disc() -> Vec<u8> {
        (0..SECTOR_SIZE * 4).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn an_unaligned_span_is_read_as_whole_sectors() {
        let bytes = disc();
        let mut reader = SectorReader::new(AlignedOnly(Cursor::new(bytes.clone())));
        let got = reader.read_at(2000, 3000).unwrap();
        assert_eq!(got, bytes[2000..5000]);
    }

    #[test]
    fn a_span_ending_on_a_boundary_reads_no_extra_sector() {
        let bytes = disc();
        let mut reader = SectorReader::new(AlignedOnly(Cursor::new(bytes.clone())));
        let got = reader
            .read_at(SECTOR_SIZE * 3, SECTOR_SIZE as usize)
            .unwrap();
        assert_eq!(got, bytes[(SECTOR_SIZE * 3) as usize..]);
    }
}

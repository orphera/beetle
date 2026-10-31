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
    trace: Option<SectorReadStats>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SectorReadStats {
    pub seeks: u64,
    pub sectors: u64,
    /// The first 128 sector ranges, as `(starting LBA, sector count)`.
    pub locations: Vec<(u64, u64)>,
}

impl SectorReader<File> {
    /// A disc image file (`.iso`).
    pub fn open_image(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self::new(File::open(path)?))
    }
}

impl<R: Read + Seek> SectorReader<R> {
    pub fn new(inner: R) -> Self {
        Self { inner, trace: None }
    }

    /// Records sector reads and their locations for diagnostics.
    pub fn enable_trace(&mut self) {
        self.trace.get_or_insert_with(SectorReadStats::default);
    }

    pub fn read_stats(&self) -> Option<&SectorReadStats> {
        self.trace.as_ref()
    }

    /// `count` sectors from `lba`.
    pub fn read_sectors(&mut self, lba: u64, count: u64) -> io::Result<Vec<u8>> {
        let mut buf = vec![0; (count * SECTOR_SIZE) as usize];
        self.inner.seek(SeekFrom::Start(lba * SECTOR_SIZE))?;
        self.inner.read_exact(&mut buf)?;
        if let Some(stats) = &mut self.trace {
            stats.seeks += 1;
            stats.sectors += count;
            if stats.locations.len() < 128 {
                stats.locations.push((lba, count));
            }
        }
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

    /// Reads several byte ranges while combining ranges that touch the same
    /// sectors. Results are returned in the same order as `ranges`.
    pub fn read_at_many(&mut self, ranges: &[(u64, usize)]) -> io::Result<Vec<Vec<u8>>> {
        let mut output: Vec<Option<Vec<u8>>> = vec![None; ranges.len()];
        let mut spans = Vec::new();
        for (index, &(offset, len)) in ranges.iter().enumerate() {
            if len == 0 {
                output[index] = Some(Vec::new());
                continue;
            }
            let end = offset.checked_add(len as u64).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "disc read range overflows")
            })?;
            let first = offset / SECTOR_SIZE;
            let last = end.div_ceil(SECTOR_SIZE);
            spans.push((first, last, index, offset, len));
        }
        spans.sort_by_key(|span| span.0);

        let mut start = 0;
        while start < spans.len() {
            let first = spans[start].0;
            let mut last = spans[start].1;
            let mut end = start + 1;
            while end < spans.len() && spans[end].0 <= last {
                last = last.max(spans[end].1);
                end += 1;
            }

            let sectors = self.read_sectors(first, last - first)?;
            for &(_, _, index, offset, len) in &spans[start..end] {
                let begin = (offset - first * SECTOR_SIZE) as usize;
                output[index] = Some(sectors[begin..begin + len].to_vec());
            }
            start = end;
        }

        Ok(output
            .into_iter()
            .map(|range| range.expect("every requested range is populated"))
            .collect())
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

    struct CountReads {
        inner: Cursor<Vec<u8>>,
        seeks: usize,
        reads: usize,
    }

    impl Read for CountReads {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.reads += 1;
            self.inner.read(buf)
        }
    }

    impl Seek for CountReads {
        fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
            self.seeks += 1;
            self.inner.seek(pos)
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

    #[test]
    fn adjacent_ranges_share_one_sector_read() {
        let bytes: Vec<u8> = (0..SECTOR_SIZE * 6).map(|i| (i % 251) as u8).collect();
        let mut reader = SectorReader::new(CountReads {
            inner: Cursor::new(bytes.clone()),
            seeks: 0,
            reads: 0,
        });
        reader.enable_trace();
        let ranges = [(100, 8), (2040, 20), (4090, 8), (SECTOR_SIZE * 4, 5)];
        let got = reader.read_at_many(&ranges).unwrap();
        assert_eq!(
            got,
            ranges
                .iter()
                .map(|&(offset, len)| bytes[offset as usize..offset as usize + len].to_vec())
                .collect::<Vec<_>>()
        );
        assert_eq!(reader.inner.seeks, 2);
        assert_eq!(reader.inner.reads, 2);
        assert_eq!(
            reader.read_stats(),
            Some(&SectorReadStats {
                seeks: 2,
                sectors: 4,
                locations: vec![(0, 3), (4, 1)],
            })
        );
    }
}

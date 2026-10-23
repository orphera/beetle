//! The variable-length codes of ISO/IEC 11172-2 and 13818-2 (Annex B), as
//! 16-bit lookup tables: index by the next 16 bits, get (value, length).

use crate::bits::Bits;

const LOOKUP_BITS: u32 = 16;

/// A decoding table; an entry with length 0 is an invalid code.
pub struct Vlc<T: Copy> {
    table: Vec<(T, u8)>,
}

impl<T: Copy> Vlc<T> {
    /// From (code, length, value) triples.
    fn new(codes: &[(u32, u8, T)], invalid: T) -> Self {
        let mut table = vec![(invalid, 0u8); 1 << LOOKUP_BITS];
        for &(code, len, value) in codes {
            let shift = LOOKUP_BITS - len as u32;
            let start = (code << shift) as usize;
            for entry in &mut table[start..start + (1 << shift)] {
                *entry = (value, len);
            }
        }
        Self { table }
    }

    /// The next value, or `None` for an invalid code.
    pub fn read(&self, bits: &mut Bits) -> Option<T> {
        let (value, len) = self.table[bits.peek(LOOKUP_BITS) as usize];
        if len == 0 {
            return None;
        }
        bits.skip(len as u32);
        Some(value)
    }
}

/// `macroblock_address_increment` (B.1): 1..=33, 34 for the escape (+33),
/// 0 for MPEG-1 stuffing. Codes are grouped as the standard prints them.
#[allow(clippy::unusual_byte_groupings)]
pub fn address_increment() -> Vlc<u8> {
    let mut codes: Vec<(u32, u8, u8)> = vec![
        (0b1, 1, 1),
        (0b011, 3, 2),
        (0b010, 3, 3),
        (0b0011, 4, 4),
        (0b0010, 4, 5),
        (0b00011, 5, 6),
        (0b00010, 5, 7),
        (0b0000111, 7, 8),
        (0b0000110, 7, 9),
        (0b00001011, 8, 10),
        (0b00001010, 8, 11),
        (0b00001001, 8, 12),
        (0b00001000, 8, 13),
        (0b00000111, 8, 14),
        (0b00000110, 8, 15),
    ];
    // 16..=21: 0000 0101 11 down to 0000 0100 10 (10 bits).
    for (i, v) in (16..=21).enumerate() {
        codes.push((0b0000_0101_11 - i as u32, 10, v));
    }
    // 22..=33: 0000 0100 011 down to 0000 0011 000 (11 bits).
    for (i, v) in (22..=33).enumerate() {
        codes.push((0b0000_0100_011 - i as u32, 11, v));
    }
    codes.push((0b0000_0001_111, 11, 0)); // stuffing (MPEG-1)
    codes.push((0b0000_0001_000, 11, 34)); // escape
    Vlc::new(&codes, 255)
}

/// Macroblock type flags.
pub const QUANT: u8 = 1;
pub const FORWARD: u8 = 2;
pub const BACKWARD: u8 = 4;
pub const PATTERN: u8 = 8;
pub const INTRA: u8 = 16;

/// `macroblock_type` for I (B.2), P (B.3) and B (B.4) pictures.
pub fn macroblock_type(picture: u8) -> Vlc<u8> {
    let codes: Vec<(u32, u8, u8)> = match picture {
        1 => vec![(0b1, 1, INTRA), (0b01, 2, INTRA | QUANT)],
        2 => vec![
            (0b1, 1, FORWARD | PATTERN),
            (0b01, 2, PATTERN),
            (0b001, 3, FORWARD),
            (0b00011, 5, INTRA),
            (0b00010, 5, QUANT | FORWARD | PATTERN),
            (0b00001, 5, QUANT | PATTERN),
            (0b000001, 6, QUANT | INTRA),
        ],
        _ => vec![
            (0b10, 2, FORWARD | BACKWARD),
            (0b11, 2, FORWARD | BACKWARD | PATTERN),
            (0b010, 3, BACKWARD),
            (0b011, 3, BACKWARD | PATTERN),
            (0b0010, 4, FORWARD),
            (0b0011, 4, FORWARD | PATTERN),
            (0b00011, 5, INTRA),
            (0b00010, 5, QUANT | FORWARD | BACKWARD | PATTERN),
            (0b000011, 6, QUANT | FORWARD | PATTERN),
            (0b000010, 6, QUANT | BACKWARD | PATTERN),
            (0b000001, 6, QUANT | INTRA),
        ],
    };
    Vlc::new(&codes, 255)
}

/// `coded_block_pattern` (B.9), by pattern value: (code, length).
const CBP: [(u32, u8); 64] = [
    (0x01, 9),
    (0x0b, 5),
    (0x09, 5),
    (0x0d, 6),
    (0x0d, 4),
    (0x17, 7),
    (0x13, 7),
    (0x1f, 8),
    (0x0c, 4),
    (0x16, 7),
    (0x12, 7),
    (0x1e, 8),
    (0x13, 5),
    (0x1b, 8),
    (0x17, 8),
    (0x13, 8),
    (0x0b, 4),
    (0x15, 7),
    (0x11, 7),
    (0x1d, 8),
    (0x11, 5),
    (0x19, 8),
    (0x15, 8),
    (0x11, 8),
    (0x0f, 6),
    (0x0f, 8),
    (0x0d, 8),
    (0x03, 9),
    (0x0f, 5),
    (0x0b, 8),
    (0x07, 8),
    (0x07, 9),
    (0x0a, 4),
    (0x14, 7),
    (0x10, 7),
    (0x1c, 8),
    (0x0e, 6),
    (0x0e, 8),
    (0x0c, 8),
    (0x02, 9),
    (0x10, 5),
    (0x18, 8),
    (0x14, 8),
    (0x10, 8),
    (0x0e, 5),
    (0x0a, 8),
    (0x06, 8),
    (0x06, 9),
    (0x12, 5),
    (0x1a, 8),
    (0x16, 8),
    (0x12, 8),
    (0x0d, 5),
    (0x09, 8),
    (0x05, 8),
    (0x05, 9),
    (0x0c, 5),
    (0x08, 8),
    (0x04, 8),
    (0x04, 9),
    (0x07, 3),
    (0x0a, 5),
    (0x08, 5),
    (0x0c, 6),
];

pub fn coded_block_pattern() -> Vlc<u8> {
    let codes: Vec<(u32, u8, u8)> = CBP
        .iter()
        .enumerate()
        .map(|(v, &(c, l))| (c, l, v as u8))
        .collect();
    Vlc::new(&codes, 255)
}

/// `motion_code` (B.10): -16..=16. The listed codes are the positive ones;
/// the negative code has the last bit set.
pub fn motion_code() -> Vlc<i8> {
    const POSITIVE: [(u32, u8); 17] = [
        (0x1, 1),
        (0x2, 3),
        (0x2, 4),
        (0x2, 5),
        (0x6, 7),
        (0xa, 8),
        (0x8, 8),
        (0x6, 8),
        (0x16, 10),
        (0x14, 10),
        (0x12, 10),
        (0x22, 11),
        (0x20, 11),
        (0x1e, 11),
        (0x1c, 11),
        (0x1a, 11),
        (0x18, 11),
    ];
    let mut codes = vec![(0x1, 1, 0i8)];
    for (n, &(c, l)) in POSITIVE.iter().enumerate().skip(1) {
        codes.push((c, l, n as i8));
        codes.push((c | 1, l, -(n as i8)));
    }
    Vlc::new(&codes, i8::MIN)
}

/// `dct_dc_size_luminance` (B.12) and `dct_dc_size_chrominance` (B.13).
pub fn dc_size(chroma: bool) -> Vlc<u8> {
    let (codes, lens): ([u32; 12], [u8; 12]) = if chroma {
        (
            [
                0x0, 0x1, 0x2, 0x6, 0xe, 0x1e, 0x3e, 0x7e, 0xfe, 0x1fe, 0x3fe, 0x3ff,
            ],
            [2, 2, 2, 3, 4, 5, 6, 7, 8, 9, 10, 10],
        )
    } else {
        (
            [
                0x4, 0x0, 0x1, 0x5, 0x6, 0xe, 0x1e, 0x3e, 0x7e, 0xfe, 0x1fe, 0x1ff,
            ],
            [3, 2, 2, 3, 3, 4, 5, 6, 7, 8, 9, 9],
        )
    };
    let triples: Vec<(u32, u8, u8)> = (0..12).map(|i| (codes[i], lens[i], i as u8)).collect();
    Vlc::new(&triples, 255)
}

/// A DCT coefficient code.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Coef {
    /// Run of zeros, then this level; its sign bit follows the code.
    RunLevel(u8, u8),
    Escape,
    EndOfBlock,
}

/// Runs and levels of the 111 run/level codes, in table order.
const RUNS: [u8; 111] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 3,
    3, 3, 3, 4, 4, 4, 5, 5, 5, 6, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13, 14, 14,
    15, 15, 16, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
];
const LEVELS: [u8; 111] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26,
    27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
    13, 14, 15, 16, 17, 18, 1, 2, 3, 4, 5, 1, 2, 3, 4, 1, 2, 3, 1, 2, 3, 1, 2, 3, 1, 2, 1, 2, 1, 2,
    1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
];

/// Table B.14 codes (without the sign bit), in `RUNS`/`LEVELS` order, then
/// escape and end of block.
const TABLE_ZERO: [(u32, u8); 113] = [
    (0x3, 2),
    (0x4, 4),
    (0x5, 5),
    (0x6, 7),
    (0x26, 8),
    (0x21, 8),
    (0xa, 10),
    (0x1d, 12),
    (0x18, 12),
    (0x13, 12),
    (0x10, 12),
    (0x1a, 13),
    (0x19, 13),
    (0x18, 13),
    (0x17, 13),
    (0x1f, 14),
    (0x1e, 14),
    (0x1d, 14),
    (0x1c, 14),
    (0x1b, 14),
    (0x1a, 14),
    (0x19, 14),
    (0x18, 14),
    (0x17, 14),
    (0x16, 14),
    (0x15, 14),
    (0x14, 14),
    (0x13, 14),
    (0x12, 14),
    (0x11, 14),
    (0x10, 14),
    (0x18, 15),
    (0x17, 15),
    (0x16, 15),
    (0x15, 15),
    (0x14, 15),
    (0x13, 15),
    (0x12, 15),
    (0x11, 15),
    (0x10, 15),
    (0x3, 3),
    (0x6, 6),
    (0x25, 8),
    (0xc, 10),
    (0x1b, 12),
    (0x16, 13),
    (0x15, 13),
    (0x1f, 15),
    (0x1e, 15),
    (0x1d, 15),
    (0x1c, 15),
    (0x1b, 15),
    (0x1a, 15),
    (0x19, 15),
    (0x13, 16),
    (0x12, 16),
    (0x11, 16),
    (0x10, 16),
    (0x5, 4),
    (0x4, 7),
    (0xb, 10),
    (0x14, 12),
    (0x14, 13),
    (0x7, 5),
    (0x24, 8),
    (0x1c, 12),
    (0x13, 13),
    (0x6, 5),
    (0xf, 10),
    (0x12, 12),
    (0x7, 6),
    (0x9, 10),
    (0x12, 13),
    (0x5, 6),
    (0x1e, 12),
    (0x14, 16),
    (0x4, 6),
    (0x15, 12),
    (0x7, 7),
    (0x11, 12),
    (0x5, 7),
    (0x11, 13),
    (0x27, 8),
    (0x10, 13),
    (0x23, 8),
    (0x1a, 16),
    (0x22, 8),
    (0x19, 16),
    (0x20, 8),
    (0x18, 16),
    (0xe, 10),
    (0x17, 16),
    (0xd, 10),
    (0x16, 16),
    (0x8, 10),
    (0x15, 16),
    (0x1f, 12),
    (0x1a, 12),
    (0x19, 12),
    (0x17, 12),
    (0x16, 12),
    (0x1f, 13),
    (0x1e, 13),
    (0x1d, 13),
    (0x1c, 13),
    (0x1b, 13),
    (0x1f, 16),
    (0x1e, 16),
    (0x1d, 16),
    (0x1c, 16),
    (0x1b, 16),
    (0x1, 6),
    (0x2, 2),
];

/// Table B.15 (`intra_vlc_format` 1), same order.
const TABLE_ONE: [(u32, u8); 113] = [
    (0x02, 2),
    (0x06, 3),
    (0x07, 4),
    (0x1c, 5),
    (0x1d, 5),
    (0x05, 6),
    (0x04, 6),
    (0x7b, 7),
    (0x7c, 7),
    (0x23, 8),
    (0x22, 8),
    (0xfa, 8),
    (0xfb, 8),
    (0xfe, 8),
    (0xff, 8),
    (0x1f, 14),
    (0x1e, 14),
    (0x1d, 14),
    (0x1c, 14),
    (0x1b, 14),
    (0x1a, 14),
    (0x19, 14),
    (0x18, 14),
    (0x17, 14),
    (0x16, 14),
    (0x15, 14),
    (0x14, 14),
    (0x13, 14),
    (0x12, 14),
    (0x11, 14),
    (0x10, 14),
    (0x18, 15),
    (0x17, 15),
    (0x16, 15),
    (0x15, 15),
    (0x14, 15),
    (0x13, 15),
    (0x12, 15),
    (0x11, 15),
    (0x10, 15),
    (0x02, 3),
    (0x06, 5),
    (0x79, 7),
    (0x27, 8),
    (0x20, 8),
    (0x16, 13),
    (0x15, 13),
    (0x1f, 15),
    (0x1e, 15),
    (0x1d, 15),
    (0x1c, 15),
    (0x1b, 15),
    (0x1a, 15),
    (0x19, 15),
    (0x13, 16),
    (0x12, 16),
    (0x11, 16),
    (0x10, 16),
    (0x05, 5),
    (0x07, 7),
    (0xfc, 8),
    (0x0c, 10),
    (0x14, 13),
    (0x07, 5),
    (0x26, 8),
    (0x1c, 12),
    (0x13, 13),
    (0x06, 6),
    (0xfd, 8),
    (0x12, 12),
    (0x07, 6),
    (0x04, 9),
    (0x12, 13),
    (0x06, 7),
    (0x1e, 12),
    (0x14, 16),
    (0x04, 7),
    (0x15, 12),
    (0x05, 7),
    (0x11, 12),
    (0x78, 7),
    (0x11, 13),
    (0x7a, 7),
    (0x10, 13),
    (0x21, 8),
    (0x1a, 16),
    (0x25, 8),
    (0x19, 16),
    (0x24, 8),
    (0x18, 16),
    (0x05, 9),
    (0x17, 16),
    (0x07, 9),
    (0x16, 16),
    (0x0d, 10),
    (0x15, 16),
    (0x1f, 12),
    (0x1a, 12),
    (0x19, 12),
    (0x17, 12),
    (0x16, 12),
    (0x1f, 13),
    (0x1e, 13),
    (0x1d, 13),
    (0x1c, 13),
    (0x1b, 13),
    (0x1f, 16),
    (0x1e, 16),
    (0x1d, 16),
    (0x1c, 16),
    (0x1b, 16),
    (0x01, 6),
    (0x06, 4),
];

/// The DCT coefficient table: B.14 (`one` false) or B.15.
pub fn dct_coefficients(one: bool) -> Vlc<Coef> {
    let table = if one { &TABLE_ONE } else { &TABLE_ZERO };
    let mut codes: Vec<(u32, u8, Coef)> = (0..111)
        .map(|i| (table[i].0, table[i].1, Coef::RunLevel(RUNS[i], LEVELS[i])))
        .collect();
    codes.push((table[111].0, table[111].1, Coef::Escape));
    codes.push((table[112].0, table[112].1, Coef::EndOfBlock));
    Vlc::new(&codes, Coef::EndOfBlock)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read<T: Copy>(vlc: &Vlc<T>, bits: &[u8]) -> (Option<T>, usize) {
        let mut b = Bits::new(bits);
        let v = vlc.read(&mut b);
        (v, b.byte_pos())
    }

    #[test]
    fn address_increments() {
        let vlc = address_increment();
        assert_eq!(read(&vlc, &[0b1000_0000]).0, Some(1));
        assert_eq!(read(&vlc, &[0b0001_0000]).0, Some(7));
        assert_eq!(read(&vlc, &[0b0000_0101, 0b1100_0000]).0, Some(16));
        assert_eq!(read(&vlc, &[0b0000_0011, 0b0000_0000]).0, Some(33));
        assert_eq!(read(&vlc, &[0b0000_0001, 0b0000_0000]).0, Some(34));
        assert_eq!(read(&vlc, &[0b0000_0001, 0b1110_0000]).0, Some(0));
    }

    #[test]
    fn coded_block_patterns() {
        let vlc = coded_block_pattern();
        assert_eq!(read(&vlc, &[0b1110_0000]).0, Some(60));
        assert_eq!(read(&vlc, &[0b0101_1000]).0, Some(1));
        assert_eq!(read(&vlc, &[0b0011_0000]).0, Some(63));
    }

    #[test]
    fn motion_codes_carry_their_sign() {
        let vlc = motion_code();
        assert_eq!(read(&vlc, &[0b1000_0000]).0, Some(0));
        assert_eq!(read(&vlc, &[0b0100_0000]).0, Some(1));
        assert_eq!(read(&vlc, &[0b0110_0000]).0, Some(-1));
        assert_eq!(read(&vlc, &[0b0000_0110, 0b0000_0000]).0, Some(7));
        assert_eq!(read(&vlc, &[0b0000_0011, 0b0000_0000]).0, Some(16));
    }

    #[test]
    fn every_code_of_a_table_is_its_own() {
        for one in [false, true] {
            let vlc = dct_coefficients(one);
            let table = if one { &TABLE_ONE } else { &TABLE_ZERO };
            for (i, &(code, len)) in table.iter().enumerate() {
                let word = (code << (32 - len as u32)).to_be_bytes();
                let mut b = Bits::new(&word);
                let got = vlc.read(&mut b).unwrap();
                let want = match i {
                    111 => Coef::Escape,
                    112 => Coef::EndOfBlock,
                    _ => Coef::RunLevel(RUNS[i], LEVELS[i]),
                };
                assert_eq!(got, want, "table {one} entry {i}");
                assert!(b.byte_pos() * 8 >= len as usize);
            }
        }
    }
}

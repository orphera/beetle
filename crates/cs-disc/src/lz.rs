//! The LZ compression of Konami's PS2 IIDX files (`common.decode_lz` in
//! iidx-ps2tools, "based on decompression code from IIDX GOLD CS").
//!
//! A control byte gives 8 flags, low bit first. Flag 0: one literal byte.
//! Flag 1: a code byte:
//! - `0xxxxxxx` + 1 byte: copy `(x >> 2) + 3` bytes from 10-bit distance.
//! - `10xxxxxx`: copy `(x >> 4) - 6` bytes (2..5) from distance `(x & 15) + 1`.
//! - `0xff`: end.
//! - `11xxxxxx`: `x - 0xb8` literal bytes follow.

/// Decompresses `input`. Stops at the end code, or when the input runs out
/// (`None`, since the data is then cut short).
pub fn decode(input: &[u8]) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = Vec::with_capacity(input.len() * 4);
    let mut i = 0;
    let mut control: u32 = 0;
    let next = |i: &mut usize| -> Option<u8> {
        let b = *input.get(*i)?;
        *i += 1;
        Some(b)
    };
    loop {
        control >>= 1;
        if control & 0x100 == 0 {
            control = next(&mut i)? as u32 | 0xff00;
        }
        let data = next(&mut i)?;
        if control & 1 == 0 {
            out.push(data);
            continue;
        }
        let copy = if data & 0x80 == 0 {
            let distance = ((data as usize & 0x03) << 8) | next(&mut i)? as usize;
            Some((distance, (data as usize >> 2) + 3))
        } else if data & 0x40 == 0 {
            Some(((data as usize & 0x0f) + 1, (data as usize >> 4) - 6))
        } else {
            None
        };
        if let Some((distance, len)) = copy {
            // A distance of 0 points past the output: not LZ data.
            if distance == 0 {
                return None;
            }
            let start = out.len().checked_sub(distance)?;
            // Byte by byte: the copy may overlap what it writes.
            for k in 0..len {
                out.push(out[start + k]);
            }
            continue;
        }
        if data == 0xff {
            return Some(out);
        }
        for _ in 0..(data as usize - 0xb8) {
            out.push(next(&mut i)?);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_runs_and_copies() {
        // control 0b0000_1110 (low bit first): literal 'a', then code 0x80
        // (short copy: len 2, distance 1) → "aa", then code 0xc1 → 9 literal
        // bytes, then the end code 0xff.
        let mut input = vec![0b0000_1110, b'a', 0x80, 0xc1];
        input.extend(b"bcdefghij");
        input.push(0xff);
        assert_eq!(decode(&input).unwrap(), b"aaabcdefghij");
    }

    #[test]
    fn a_long_copy_overlaps_its_output() {
        // literal 'x', then 0b0000_0100 << 0: code byte 0x04 + 0x01 → len 4, distance 1.
        let input = [0b0000_0110, b'x', 0x04, 0x01, 0xff];
        assert_eq!(decode(&input).unwrap(), b"xxxxx");
    }

    #[test]
    fn a_copy_from_distance_0_is_none() {
        assert_eq!(decode(&[0b0000_0001, 0x00, 0x00]), None);
    }

    #[test]
    fn cut_short_input_is_none() {
        assert_eq!(decode(&[0b0000_0000, b'a']), None);
    }
}

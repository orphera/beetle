//! PS2 SPU ADPCM (the "VAG" format), as the IIDX 16 discs store sounds.
//!
//! A frame is 16 bytes for 28 samples: byte 0 holds the predictor (high
//! nibble) and shift (low nibble), byte 1 the flags, then 14 bytes of 4-bit
//! samples, low nibble first. Konami's discs scramble the first bytes of
//! every frame: byte 0 is XORed with 0xff and a `Key` is added to bytes 1 to 3.

/// What a disc adds to bytes 1, 2 and 3 of every frame.
pub type Key = [u8; 3];

/// The scramble key of the BGM streams.
pub const KEY_BGM: Key = [0, 0xfe, 0];
/// The scramble key of the key sound banks (14 to 16).
pub const KEY_KEYSOUND: Key = [0, 0xdc, 0];
/// The key sound banks of 11 IIDX RED: its end frames read `ff 05 66 ae`
/// for `00 07 77 77`.
pub const KEY_KEYSOUND_RED: Key = [0xfe, 0xef, 0x37];

const FRAME: usize = 16;
const COEFFICIENTS: [(i32, i32); 5] = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)];
/// Flag value that ends a sound.
const FLAG_END: u8 = 7;

/// Decodes one channel. `key` unscrambles the frames first; `stop_at_end`
/// ends at an end or loop-end flag, as key sounds do (BGM streams play every frame).
pub fn decode(data: &[u8], key: Option<Key>, stop_at_end: bool) -> Vec<i16> {
    let mut out = Vec::with_capacity(data.len() / FRAME * 28);
    let (mut hist1, mut hist2) = (0i32, 0i32);
    for (index, raw) in data.chunks_exact(FRAME).enumerate() {
        let mut frame = [0u8; FRAME];
        frame.copy_from_slice(raw);
        if let Some(key) = key {
            frame[0] ^= 0xff;
            for (byte, k) in frame[1..4].iter_mut().zip(key) {
                *byte = byte.wrapping_sub(k);
            }
        }
        let flags = frame[1];
        if stop_at_end && flags == FLAG_END {
            break;
        }
        let predictor = ((frame[0] >> 4) as usize).min(4);
        let mut shift = frame[0] & 0x0f;
        // A shift past 12 is read as 9, as vgmstream does. (Treating such
        // frames as silence was one cause of the harsh sound in the first
        // Python extraction.)
        if shift > 12 {
            shift = 9;
        }
        let (c1, c2) = COEFFICIENTS[predictor];
        for &byte in &frame[2..] {
            for nibble in [byte & 0x0f, byte >> 4] {
                let sample = ((nibble as i8) << 4 >> 4) as i32;
                let scaled = (sample << 12) >> shift;
                // The sum is truncated toward zero, as vgmstream's float
                // decoder does; a floored prediction is off by up to 1 per
                // sample and drifts (checked sample-exact against vgmstream).
                let value = ((scaled * 64 + hist1 * c1 + hist2 * c2) / 64).clamp(-32768, 32767);
                out.push(value as i16);
                hist2 = hist1;
                hist1 = value;
            }
        }
        if stop_at_end && (flags == 1 || flags == 3) && index > 0 {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_silent_frame_decodes_to_zeros_and_the_end_flag_stops() {
        let mut data = vec![0u8; 32];
        data[16 + 1] = FLAG_END;
        assert_eq!(decode(&data, None, true), vec![0; 28]);
        assert_eq!(decode(&data, None, false).len(), 56);
    }

    #[test]
    fn nibbles_are_signed_and_low_first() {
        // predictor 0, shift 12: a sample is its nibble's value.
        let mut frame = [0u8; 16];
        frame[0] = 0x0c;
        frame[2] = 0xf1; // 1, then -1
        let out = decode(&frame, None, false);
        assert_eq!(&out[..2], &[1, -1]);
    }

    #[test]
    fn the_scramble_is_undone() {
        let mut frame = [0u8; 16];
        frame[0] = 0x0c ^ 0xff;
        frame[2] = 0xf1u8.wrapping_add(KEY_KEYSOUND[1]);
        assert_eq!(&decode(&frame, Some(KEY_KEYSOUND), false)[..2], &[1, -1]);
    }

    #[test]
    fn a_red_end_frame_unscrambles_to_the_end_flag() {
        let mut data = vec![0u8; 16];
        data[0] = 0xff;
        data[1..4].copy_from_slice(&KEY_KEYSOUND_RED);
        let mut end = [0x77u8; 16];
        end[..4].copy_from_slice(&[0xff, 0x05, 0x66, 0xae]);
        data.extend(end);
        // The first frame is silence, the second the end.
        assert_eq!(decode(&data, Some(KEY_KEYSOUND_RED), true), vec![0; 28]);
    }
}

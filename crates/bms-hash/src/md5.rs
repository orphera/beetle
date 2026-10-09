//! MD5 (RFC 1321).
//!
//! Only for matching the MD5 values that BMS difficulty tables and LR2 score
//! databases use to name charts. MD5 is not collision resistant, so it is
//! never used to decide that two charts are the same; SHA-256 does that.

/// Per-round left-rotation amounts.
const S: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
    21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

/// `floor(2^32 * abs(sin(i + 1)))`.
const K: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
    0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
    0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
    0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
    0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
    0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
    0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
    0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
];

/// Incremental MD5 hasher.
#[derive(Clone, Debug)]
pub struct Md5Hasher {
    state: [u32; 4],
    buffer: [u8; 64],
    buf_len: usize,
    total_bytes: u64,
}

impl Default for Md5Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Md5Hasher {
    pub fn new() -> Self {
        Self {
            state: [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476],
            buffer: [0; 64],
            buf_len: 0,
            total_bytes: 0,
        }
    }

    fn process_block(state: &mut [u32; 4], block: &[u8; 64]) {
        let mut m = [0u32; 16];
        for (i, word) in m.iter_mut().enumerate() {
            *word = u32::from_le_bytes([
                block[4 * i],
                block[4 * i + 1],
                block[4 * i + 2],
                block[4 * i + 3],
            ]);
        }

        let [mut a, mut b, mut c, mut d] = *state;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let rotated = a
                .wrapping_add(f)
                .wrapping_add(K[i])
                .wrapping_add(m[g])
                .rotate_left(S[i]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(rotated);
        }

        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
    }

    /// Appends a chunk of bytes.
    pub fn update(&mut self, mut data: &[u8]) {
        self.total_bytes += data.len() as u64;

        if self.buf_len > 0 {
            let to_fill = 64 - self.buf_len;
            if data.len() < to_fill {
                self.buffer[self.buf_len..self.buf_len + data.len()].copy_from_slice(data);
                self.buf_len += data.len();
                return;
            }
            self.buffer[self.buf_len..].copy_from_slice(&data[..to_fill]);
            Self::process_block(&mut self.state, &self.buffer);
            data = &data[to_fill..];
            self.buf_len = 0;
        }

        while data.len() >= 64 {
            let (chunk, rest) = data.split_at(64);
            let block: &[u8; 64] = chunk.try_into().expect("chunk is 64 bytes");
            Self::process_block(&mut self.state, block);
            data = rest;
        }

        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
            self.buf_len = data.len();
        }
    }

    /// Finishes and returns the 16-byte digest.
    pub fn finalize(mut self) -> [u8; 16] {
        let bit_len = self.total_bytes.wrapping_mul(8);

        self.buffer[self.buf_len] = 0x80;
        self.buf_len += 1;
        if self.buf_len > 56 {
            self.buffer[self.buf_len..].fill(0);
            Self::process_block(&mut self.state, &self.buffer);
            self.buffer = [0; 64];
        } else {
            self.buffer[self.buf_len..56].fill(0);
        }
        self.buffer[56..].copy_from_slice(&bit_len.to_le_bytes());
        Self::process_block(&mut self.state, &self.buffer);

        let mut out = [0u8; 16];
        for (i, word) in self.state.iter().enumerate() {
            out[4 * i..4 * i + 4].copy_from_slice(&word.to_le_bytes());
        }
        out
    }
}

/// MD5 digest of `data`.
pub fn md5_digest(data: &[u8]) -> [u8; 16] {
    let mut hasher = Md5Hasher::new();
    hasher.update(data);
    hasher.finalize()
}

/// Lowercase hex MD5 of `data` (32 characters).
pub fn md5_hex(data: &[u8]) -> String {
    crate::to_hex(&md5_digest(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc_1321_test_suite() {
        let vectors: [(&[u8], &str); 7] = [
            (b"", "d41d8cd98f00b204e9800998ecf8427e"),
            (b"a", "0cc175b9c0f1b6a831c399e269772661"),
            (b"abc", "900150983cd24fb0d6963f7d28e17f72"),
            (b"message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
            (
                b"abcdefghijklmnopqrstuvwxyz",
                "c3fcd3d76192e4007dfb496cca67e13b",
            ),
            (
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
                "d174ab98d277d9f5a5611c2c9f419d9f",
            ),
            (
                b"12345678901234567890123456789012345678901234567890123456789012345678901234567890",
                "57edf4a22be3c955ac49da2e2107b67a",
            ),
        ];
        for (input, expected) in vectors {
            assert_eq!(
                md5_hex(input),
                expected,
                "{:?}",
                String::from_utf8_lossy(input)
            );
        }
    }

    #[test]
    fn chunked_updates_match_one_shot_at_every_split() {
        let data: Vec<u8> = (0..300u32).map(|i| (i * 7 + 3) as u8).collect();
        let whole = md5_digest(&data);
        for split in 0..data.len() {
            let mut hasher = Md5Hasher::new();
            hasher.update(&data[..split]);
            hasher.update(&data[split..]);
            assert_eq!(hasher.finalize(), whole, "split at {split}");
        }
    }

    #[test]
    fn lengths_around_the_padding_boundaries() {
        // 55, 56 and 64 bytes straddle where the length field no longer fits.
        let known = [
            (55usize, "ef1772b6dff9a122358552954ad0df65"),
            (56, "3b0c8ac703f828b04c6c197006d17218"),
            (64, "014842d480b571495a4a0363793f7367"),
        ];
        for (len, expected) in known {
            assert_eq!(md5_hex(&vec![b'a'; len]), expected, "{len} bytes");
        }
    }
}

//! Blowfish decryption as Konami's PS2 discs use it: 32-bit halves read
//! little-endian, CBC with ciphertext stealing and a zero IV.
//!
//! Follows iidx-ps2tools (`common.decrypt_blowfish`, which calls the Python
//! `blowfish` package's `decrypt_cbc_cts`), quirks included, so its output is
//! byte for byte what that tool extracted.

use crate::blowfish_pi::{P, S};

pub struct Blowfish {
    p: [u32; 18],
    s: [[u32; 256]; 4],
}

impl Blowfish {
    /// The standard key schedule. `key` is 4 to 56 bytes.
    pub fn new(key: &[u8]) -> Self {
        assert!((4..=56).contains(&key.len()), "Blowfish key length");
        let mut bf = Blowfish { p: P, s: S };
        let mut k = key.iter().cycle();
        for p in bf.p.iter_mut() {
            let word = (0..4).fold(0u32, |w, _| (w << 8) | *k.next().unwrap() as u32);
            *p ^= word;
        }
        let (mut l, mut r) = (0u32, 0u32);
        for i in (0..18).step_by(2) {
            (l, r) = bf.encrypt_words(l, r);
            bf.p[i] = l;
            bf.p[i + 1] = r;
        }
        for b in 0..4 {
            for i in (0..256).step_by(2) {
                (l, r) = bf.encrypt_words(l, r);
                bf.s[b][i] = l;
                bf.s[b][i + 1] = r;
            }
        }
        bf
    }

    fn f(&self, x: u32) -> u32 {
        let [a, b, c, d] = x.to_be_bytes();
        (self.s[0][a as usize].wrapping_add(self.s[1][b as usize]) ^ self.s[2][c as usize])
            .wrapping_add(self.s[3][d as usize])
    }

    fn encrypt_words(&self, mut l: u32, mut r: u32) -> (u32, u32) {
        for i in (0..16).step_by(2) {
            l ^= self.p[i];
            r ^= self.f(l);
            r ^= self.p[i + 1];
            l ^= self.f(r);
        }
        (r ^ self.p[17], l ^ self.p[16])
    }

    fn decrypt_words(&self, mut l: u32, mut r: u32) -> (u32, u32) {
        for i in (2..18).step_by(2).rev() {
            l ^= self.p[i + 1];
            r ^= self.f(l);
            r ^= self.p[i];
            l ^= self.f(r);
        }
        (r ^ self.p[0], l ^ self.p[1])
    }

    fn decrypt_block(&self, block: [u8; 8]) -> [u8; 8] {
        let l = u32::from_le_bytes([block[0], block[1], block[2], block[3]]);
        let r = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
        let (l, r) = self.decrypt_words(l, r);
        let mut out = [0u8; 8];
        out[..4].copy_from_slice(&l.to_le_bytes());
        out[4..].copy_from_slice(&r.to_le_bytes());
        out
    }

    /// CBC-CTS decryption with a zero IV. `data` must be longer than 8 bytes.
    pub fn decrypt_cbc_cts(&self, data: &[u8]) -> Vec<u8> {
        assert!(data.len() > 8, "CBC-CTS needs more than one block");
        let extra = data.len() % 8;
        let last_stop = data.len() - extra;
        let last_start = last_stop - 8;
        let mut out = Vec::with_capacity(data.len());
        let mut prev = [0u8; 8];
        for chunk in data[..last_start].chunks_exact(8) {
            let block: [u8; 8] = chunk.try_into().unwrap();
            out.extend(xor(self.decrypt_block(block), prev));
            prev = block;
        }
        // The stolen tail: the last full block decrypts to the tail's
        // plaintext XOR its own bytes, and lends the rest to the tail.
        let d = self.decrypt_block(data[last_start..last_stop].try_into().unwrap());
        let mut tail = [0u8; 8];
        tail[..extra].copy_from_slice(&data[last_stop..]);
        let xn = xor(d, tail);
        let mut e = [0u8; 8];
        e[..extra].copy_from_slice(&data[last_stop..]);
        e[extra..].copy_from_slice(&xn[extra..]);
        out.extend(xor(self.decrypt_block(e), prev));
        out.extend(&xn[..extra]);
        out
    }
}

fn xor(a: [u8; 8], b: [u8; 8]) -> [u8; 8] {
    std::array::from_fn(|i| a[i] ^ b[i])
}

/// `common.decrypt_blowfish`: data that is a whole number of blocks gets a
/// zero block appended first, so the result has 8 trailing junk bytes then.
pub fn decrypt_file(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut data = data.to_vec();
    if data.len().is_multiple_of(8) {
        data.extend([0u8; 8]);
    }
    if data.len() <= 8 {
        return data;
    }
    Blowfish::new(key).decrypt_cbc_cts(&data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_standard_test_vector_decrypts() {
        // Eric Young's vector: key 0000000000000000, cipher 4EF997456198DD78
        // (big-endian halves), plaintext 0000000000000000.
        let bf = Blowfish::new(&[0; 8]);
        assert_eq!(bf.decrypt_words(0x4EF99745, 0x6198DD78), (0, 0));
        assert_eq!(bf.encrypt_words(0, 0), (0x4EF99745, 0x6198DD78));
    }

    #[test]
    fn a_second_vector_round_trips() {
        // key FFFFFFFFFFFFFFFF, plain FFFFFFFFFFFFFFFF, cipher 51866FD5B85ECB8A.
        let bf = Blowfish::new(&[0xff; 8]);
        assert_eq!(
            bf.encrypt_words(0xFFFFFFFF, 0xFFFFFFFF),
            (0x51866FD5, 0xB85ECB8A)
        );
    }
}

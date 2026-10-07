//! # bms-hash
//!
//! SHA-256 and MD5 with no external dependencies, shared by the player core
//! (chart identity) and the package format (entry and package checksums).

pub mod md5;
pub mod sha256;

pub use md5::{md5_digest, md5_hex, Md5Hasher};
pub use sha256::{sha256_digest, sha256_hex, Sha256Hasher};

/// Lowercase hex text of `bytes`.
pub fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(DIGITS[usize::from(b >> 4)] as char);
        out.push(DIGITS[usize::from(b & 15)] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_is_lowercase_and_padded() {
        assert_eq!(to_hex(&[0x00, 0x0f, 0xa0, 0xff]), "000fa0ff");
        assert_eq!(to_hex(&[]), "");
    }
}

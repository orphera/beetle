//! Chart identity: what makes two chart files the same chart.
//!
//! The key is the SHA-256 of the file's bytes exactly as stored: no decoding,
//! no newline or case normalization. The same file is the same chart wherever
//! it sits (loose folder, any package); files that differ by even an encoding
//! or a line ending are different charts, as in other BMS players and in
//! difficulty tables. MD5 of the same bytes is kept only as the alias those
//! tables and LR2 score databases use; it is never a key.

use bms_hash::{md5_digest, sha256_digest, to_hex, Md5Hasher, Sha256Hasher};
use std::fmt;
use std::str::FromStr;

/// Text form prefix of a chart id.
const PREFIX: &str = "sha256:";

/// SHA-256 of a chart file's bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct ChartId([u8; 32]);

impl ChartId {
    /// The id of a chart file with these bytes.
    pub fn of_bytes(data: &[u8]) -> Self {
        Self(sha256_digest(data))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// 64 lowercase hex characters.
    pub fn to_hex(&self) -> String {
        to_hex(&self.0)
    }

    /// First 16 hex characters (64 bits): short enough for file names. Always
    /// compare whole ids, never these.
    pub fn short(&self) -> String {
        to_hex(&self.0[..8])
    }

    /// Reads 64 hex characters, with or without the `sha256:` prefix.
    pub fn from_hex(text: &str) -> Option<Self> {
        let hex = text.strip_prefix(PREFIX).unwrap_or(text);
        if hex.len() != 64 || !hex.is_ascii() {
            return None;
        }
        let mut bytes = [0u8; 32];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()?;
        }
        Some(Self(bytes))
    }
}

impl fmt::Display for ChartId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{PREFIX}{}", self.to_hex())
    }
}

impl fmt::Debug for ChartId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChartId({})", self.short())
    }
}

impl FromStr for ChartId {
    type Err = ();

    fn from_str(text: &str) -> Result<Self, ()> {
        Self::from_hex(text).ok_or(())
    }
}

/// MD5 of a chart file's bytes, as difficulty tables and LR2 name charts.
pub fn md5_of_bytes(data: &[u8]) -> [u8; 16] {
    md5_digest(data)
}

/// Lowercase hex text of an MD5 digest.
pub fn md5_to_hex(md5: &[u8; 16]) -> String {
    to_hex(md5)
}

/// Reads 32 hex characters back into an MD5 digest.
pub fn md5_from_hex(text: &str) -> Option<[u8; 16]> {
    if text.len() != 32 || !text.is_ascii() {
        return None;
    }
    let mut bytes = [0u8; 16];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(bytes)
}

/// Both hashes of a chart file in a single pass over its bytes.
pub fn hash_chart_bytes(data: &[u8]) -> (ChartId, [u8; 16]) {
    let mut sha = Sha256Hasher::new();
    let mut md5 = Md5Hasher::new();
    sha.update(data);
    md5.update(data);
    (ChartId(sha.finalize()), md5.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_is_the_sha256_of_the_bytes() {
        let id = ChartId::of_bytes(b"abc");
        assert_eq!(
            id.to_string(),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(id.short(), "ba7816bf8f01cfea");
    }

    #[test]
    fn same_bytes_same_id_and_any_difference_changes_it() {
        let lf = ChartId::of_bytes(b"#TITLE T\n#BPM 120\n");
        assert_eq!(lf, ChartId::of_bytes(b"#TITLE T\n#BPM 120\n"));
        // A different line ending is a different file, so a different chart.
        assert_ne!(lf, ChartId::of_bytes(b"#TITLE T\r\n#BPM 120\r\n"));
    }

    #[test]
    fn text_form_round_trips() {
        let id = ChartId::of_bytes(b"chart");
        assert_eq!(id.to_string().parse::<ChartId>(), Ok(id));
        assert_eq!(ChartId::from_hex(&id.to_hex()), Some(id));
        assert_eq!(ChartId::from_hex("sha256:1234"), None);
        assert_eq!(ChartId::from_hex(&"g".repeat(64)), None);
        assert_eq!(ChartId::from_hex(""), None);
    }

    #[test]
    fn both_hashes_come_from_one_pass() {
        let data = b"#TITLE test chart\n#BPM 150\n#00111:01\n";
        let (id, md5) = hash_chart_bytes(data);
        assert_eq!(id, ChartId::of_bytes(data));
        assert_eq!(md5, md5_of_bytes(data));
        assert_eq!(md5_from_hex(&md5_to_hex(&md5)), Some(md5));
        assert_eq!(md5_from_hex("xyz"), None);
    }
}

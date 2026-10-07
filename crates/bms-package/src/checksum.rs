//! SHA-256 now lives in `bms-hash` (shared with the player core); re-exported here.

pub use bms_hash::{sha256_digest, sha256_hex, Sha256Hasher};

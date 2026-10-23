//! Hand-checked details of game disc songs: `disc_meta/<edition>.tsv` next
//! to `library.dat` (format: `cs_disc::meta`). The files are the user's own
//! (made with `scripts/disc_meta_from_textage.py` and checked by hand) and are
//! not part of the repository. Without one, a disc song shows what the disc says.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use cs_disc::meta::{parse, SongMeta};

pub const DIR: &str = "disc_meta";

pub fn path(slug: &str) -> PathBuf {
    PathBuf::from(DIR).join(format!("{slug}.tsv"))
}

/// The table for `edition`, by song id; empty when there is none.
pub fn load(slug: &str) -> BTreeMap<u32, SongMeta> {
    fs::read_to_string(path(slug))
        .map(|text| parse(&text))
        .unwrap_or_default()
}

//! What a disc does not say about its songs, kept in a hand-checked table:
//! the real title, subtitle, artist and genre, and the container of the
//! loading card where the disc gives no rule for it.
//!
//! One UTF-8 tab-separated file per disc edition. The first line that is not
//! a comment names the columns; `id` is required, the others are optional and
//! may come in any order:
//!
//! ```text
//! # comments start with '#'
//! id | disc_title | title | subtitle | artist | genre | card | note
//! 0 | ANISAKIS | Anisakis | -somatic mutation type "Forza"- | 朱雀 | HARD RENAISSANCE | | textage anisakis
//! ```
//!
//! (cells separated by tabs, shown here as ` | `)
//!
//! `disc_title` and `note` are for the person editing the file; an empty cell
//! means "unknown" and leaves what the disc says.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SongMeta {
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub genre: String,
    /// The loading card's container, when the table gives one.
    pub card: Option<u32>,
}

/// The rows of a table, by song id. Rows without a numeric id are skipped.
pub fn parse(text: &str) -> BTreeMap<u32, SongMeta> {
    let mut lines = text
        .lines()
        .map(|l| l.trim_start_matches('\u{feff}').trim_end_matches('\r'))
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'));
    let Some(header) = lines.next() else {
        return BTreeMap::new();
    };
    let columns: Vec<&str> = header.split('\t').map(str::trim).collect();
    let at = |name: &str| columns.iter().position(|c| c.eq_ignore_ascii_case(name));
    let (id, title, subtitle, artist, genre, card) = (
        at("id"),
        at("title"),
        at("subtitle"),
        at("artist"),
        at("genre"),
        at("card"),
    );
    let Some(id) = id else {
        return BTreeMap::new();
    };
    let mut rows = BTreeMap::new();
    for line in lines {
        let cells: Vec<&str> = line.split('\t').collect();
        let cell = |i: Option<usize>| {
            i.and_then(|i| cells.get(i))
                .map_or(String::new(), |c| c.trim().to_string())
        };
        let Ok(song) = cell(Some(id)).parse::<u32>() else {
            continue;
        };
        rows.insert(
            song,
            SongMeta {
                title: cell(title),
                subtitle: cell(subtitle),
                artist: cell(artist),
                genre: cell(genre),
                card: cell(card).parse().ok(),
            },
        );
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_are_found_by_name_and_empty_cells_stay_empty() {
        let text = "\u{feff}# made by a script, checked by hand\n\
                    id\tdisc_title\tartist\ttitle\tcard\tnote\n\
                    0\tANISAKIS\t朱雀\tAnisakis\t\tok\n\
                    17\tJACK\tDesQ\tJack\t334\n\
                    x\tnot a row\n";
        let rows = parse(text);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[&0].title, "Anisakis");
        assert_eq!(rows[&0].artist, "朱雀");
        assert_eq!(rows[&0].card, None);
        assert_eq!(rows[&0].genre, "");
        assert_eq!(rows[&17].card, Some(334));
    }

    #[test]
    fn a_table_without_an_id_column_is_empty() {
        assert!(parse("title\tartist\nA\tB\n").is_empty());
        assert!(parse("").is_empty());
    }
}

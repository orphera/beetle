//! Key sounds a chart declares, and which of them are missing next to a copy.
//!
//! "Missing" is the collection's notion of an intact copy: every declared
//! `#WAVxx` file was found, with the usual audio extension fallbacks. The
//! callers supply the file lookup, so this module stays free of file system
//! and package code.

use crate::bms::{decode_bms_text, parse_bms};

/// Audio extensions a chart's sound name may have been re-encoded to.
pub const SOUND_EXTENSIONS: &[&str] = &["wav", "ogg", "flac", "mp3"];

/// The sound file names a chart declares with `#WAVxx`, sorted and without
/// duplicates. Every declared name counts, including ones no note uses.
/// `None` when the chart does not parse.
pub fn declared_key_sounds(bytes: &[u8]) -> Option<Vec<String>> {
    let chart = parse_bms(&decode_bms_text(bytes)).ok()?;
    let mut names: Vec<String> = chart.header.wav_table.values().cloned().collect();
    names.sort();
    names.dedup();
    Some(names)
}

/// How many of `names` `found` cannot find.
pub fn count_missing(names: &[String], mut found: impl FnMut(&str) -> bool) -> u32 {
    names.iter().filter(|name| !found(name)).count() as u32
}

/// Paths to try, in order, for a declared sound next to a chart: the name as
/// written (with `\` as `/` and surrounding spaces trimmed), then the same
/// stem with each audio extension.
pub fn folder_sound_candidates(name: &str) -> Vec<String> {
    let rel = name.trim().replace('\\', "/");
    let stem = without_extension(&rel).to_string();
    let mut candidates = vec![rel];
    candidates.extend(SOUND_EXTENSIONS.iter().map(|ext| format!("{stem}.{ext}")));
    candidates
}

/// Drops the extension of the last path segment, if it has one.
pub fn without_extension(path: &str) -> &str {
    let segment_start = path.rfind('/').map_or(0, |slash| slash + 1);
    match path[segment_start..].rfind('.') {
        Some(dot) if dot > 0 => &path[..segment_start + dot],
        _ => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_sounds_are_sorted_and_unique() {
        let chart = b"#WAV02 b.wav\n#WAV01 a.wav\n#WAV03 a.wav\n#00111:0100\n";
        assert_eq!(
            declared_key_sounds(chart),
            Some(vec!["a.wav".to_string(), "b.wav".to_string()])
        );
    }

    #[test]
    fn missing_count_uses_the_lookup() {
        let names = vec!["a.wav".to_string(), "b.wav".to_string()];
        assert_eq!(count_missing(&names, |name| name == "a.wav"), 1);
    }

    #[test]
    fn folder_candidates_try_exact_then_sound_extensions() {
        let candidates = folder_sound_candidates(" Sub\\kick.WAV ");
        assert_eq!(candidates[0], "Sub/kick.WAV");
        assert_eq!(candidates[1], "Sub/kick.wav");
        assert_eq!(candidates[2], "Sub/kick.ogg");
        assert_eq!(candidates.len(), 1 + SOUND_EXTENSIONS.len());
    }
}

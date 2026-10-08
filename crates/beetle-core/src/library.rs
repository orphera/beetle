use crate::bms::{decode_bms_text, parse_bms, NoteType, PlayMode};
use crate::rules::LnRule;
use crate::escape::{escape_field, unescape_field};
use crate::identity::{hash_chart_bytes, md5_from_hex, md5_to_hex, ChartId};
use crate::score::ScoreStore;

/// High-speed FNV-1a 64-bit hash for chart identification without external cryptographic dependencies.
pub fn compute_chart_hash(data: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Song list sorting criteria.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Title,
    Level,
    ClearLamp,
    ScoreRate,
    Bpm,
}

impl SortMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Title => "TITLE",
            Self::Level => "LEVEL",
            Self::ClearLamp => "CLEAR LAMP",
            Self::ScoreRate => "SCORE RATE",
            Self::Bpm => "BPM",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Title => Self::Level,
            Self::Level => Self::ClearLamp,
            Self::ClearLamp => Self::ScoreRate,
            Self::ScoreRate => Self::Bpm,
            Self::Bpm => Self::Title,
        }
    }
}

/// Metadata summary of a BMS chart stored in memory or cache.
#[derive(Debug, Clone, PartialEq)]
pub struct SongMetadata {
    /// Chart identity: SHA-256 of the file's bytes.
    pub id: ChartId,
    /// MD5 of the same bytes. Difficulty tables and LR2 name charts by it; it is
    /// an alias and never a key.
    pub md5: [u8; 16],
    /// The previous chart key: FNV-1a 64 over the decoded text. Only used to
    /// carry records made under it over to `id`.
    pub legacy_hash: u64,
    pub file_path: String,
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub genre: String,
    pub bpm: f64,
    /// Lowest/highest BPM in the chart (equal to `bpm` when it never changes).
    pub bpm_min: f64,
    pub bpm_max: f64,
    pub play_level: u32,
    /// Notes as the CN rule counts them: a long note's head and tail each count.
    pub notes_count: usize,
    /// How many long notes the chart has.
    pub ln_count: u32,
    /// The chart's own `#LNMODE` (1 LN, 2 CN, 3 HCN), if it has one.
    pub ln_mode: Option<u32>,
    pub play_mode: PlayMode,
}

impl SongMetadata {
    /// Extracts metadata from a chart file's raw bytes. The identity is taken
    /// from the bytes exactly as stored; only parsing sees them decoded.
    pub fn from_bytes(file_path: &str, bytes: &[u8]) -> Option<Self> {
        let content = decode_bms_text(bytes);
        let chart = parse_bms(&content).ok()?;
        let legacy_hash = compute_chart_hash(content.as_bytes());
        let (id, md5) = hash_chart_bytes(bytes);
        let notes_count = chart.total_notes_count.max(chart.playable_notes_len());
        let (bpm_min, bpm_max) = chart.bpm_range();
        let is_pms = file_path.to_lowercase().ends_with(".pms");
        let play_mode = chart.detect_play_mode_with_hint(is_pms);
        let ln_count = chart
            .notes
            .iter()
            .filter(|n| n.note_type == NoteType::LongNoteStart)
            .count() as u32;
        let ln_mode = chart.header.ln_mode;

        let title = if chart.header.title.is_empty() {
            "Unknown Title".to_string()
        } else {
            chart.header.title
        };

        Some(Self {
            id,
            md5,
            legacy_hash,
            file_path: file_path.to_string(),
            title,
            subtitle: chart.header.subtitle,
            artist: chart.header.artist,
            genre: chart.header.genre,
            bpm: chart.header.bpm,
            bpm_min,
            bpm_max,
            play_level: chart.header.play_level,
            notes_count,
            ln_count,
            ln_mode,
            play_mode,
        })
    }

    /// How many notes the chart has under a long note rule: the same as
    /// `notes_count` for CN, and without the tails for LN.
    pub fn notes_count_for(&self, rule: LnRule) -> usize {
        match rule {
            LnRule::Cn => self.notes_count,
            LnRule::Ln => self.notes_count.saturating_sub(self.ln_count as usize),
        }
    }

    /// How many notes the song has under the rule the player's setting gives it
    /// (the maximum combo, and half the maximum EX score).
    pub fn notes_for(&self, option: crate::rules::LnOption) -> usize {
        self.score_rule(option).map_or(self.notes_count, |rule| self.notes_count_for(rule))
    }

    /// The long note rule this song's score record is filed under with the
    /// player's setting: its resolved rule, or `None` when it has no long notes
    /// (the rule changes nothing there, so there is just one record).
    pub fn score_rule(&self, option: crate::rules::LnOption) -> Option<LnRule> {
        (self.ln_count > 0).then(|| crate::rules::Ruleset::resolve(self.ln_mode, option).ln)
    }

    /// Extracts metadata from chart text that has no file bytes of its own
    /// (its UTF-8 bytes are what get hashed).
    pub fn from_content(file_path: &str, content: &str) -> Option<Self> {
        Self::from_bytes(file_path, content.as_bytes())
    }

    /// BPM text for display: `150`, or `120-240` when the chart changes tempo.
    pub fn bpm_label(&self) -> String {
        let (lo, hi) = (self.bpm_min.round() as i64, self.bpm_max.round() as i64);
        if lo == hi {
            lo.to_string()
        } else {
            format!("{lo}-{hi}")
        }
    }

    /// Serializes to one `field=value` line, tab separated.
    pub fn serialize_line(&self) -> String {
        format!(
            "path={}\ttitle={}\tsub={}\tartist={}\tgenre={}\tbpm={}\tbpmmin={}\tbpmmax={}\tlevel={}\tnotes={}\tlns={}\tlnmode={}\tmode={}\tid={}\tmd5={}\tlegacy={:016x}",
            escape_field(&self.file_path),
            escape_field(&self.title),
            escape_field(&self.subtitle),
            escape_field(&self.artist),
            escape_field(&self.genre),
            self.bpm,
            self.bpm_min,
            self.bpm_max,
            self.play_level,
            self.notes_count,
            self.ln_count,
            self.ln_mode.map_or(String::new(), |m| m.to_string()),
            self.play_mode.as_str(),
            self.id,
            md5_to_hex(&self.md5),
            self.legacy_hash,
        )
    }

    /// Parses a line written by `serialize_line`. A line missing its path,
    /// identity or legacy key is rejected, so a cache that predates any of
    /// them is rebuilt rather than half-trusted. Unknown fields are ignored.
    pub fn parse_line(line: &str) -> Option<Self> {
        let mut file_path = None;
        let (mut title, mut subtitle, mut artist, mut genre) =
            (String::new(), String::new(), String::new(), String::new());
        let (mut bpm, mut bpm_min, mut bpm_max) = (130.0, None, None);
        let (mut play_level, mut notes_count) = (1, 0);
        let (mut ln_count, mut ln_mode) = (0, None);
        let mut play_mode = PlayMode::Keys7;
        let (mut id, mut md5, mut hash) = (None, None, None);

        for field in line.split('\t') {
            let Some((key, value)) = field.split_once('=') else {
                continue;
            };
            match key {
                "path" => file_path = Some(unescape_field(value)),
                "title" => title = unescape_field(value),
                "sub" => subtitle = unescape_field(value),
                "artist" => artist = unescape_field(value),
                "genre" => genre = unescape_field(value),
                "bpm" => bpm = value.parse().unwrap_or(130.0),
                "bpmmin" => bpm_min = value.parse().ok(),
                "bpmmax" => bpm_max = value.parse().ok(),
                "level" => play_level = value.parse().unwrap_or(1),
                "notes" => notes_count = value.parse().unwrap_or(0),
                "lns" => ln_count = value.parse().unwrap_or(0),
                "lnmode" => ln_mode = value.parse().ok(),
                "mode" => {
                    play_mode = match value {
                        "4KEYS" => PlayMode::Keys4,
                        "5KEYS" => PlayMode::Keys5,
                        "6KEYS" => PlayMode::Keys6,
                        "8KEYS" => PlayMode::Keys8,
                        "9KEYS" => PlayMode::Keys9,
                        "10KEYS" => PlayMode::Keys10,
                        "14KEYS" => PlayMode::Keys14,
                        _ => PlayMode::Keys7,
                    }
                }
                "id" => id = ChartId::from_hex(value),
                "md5" => md5 = md5_from_hex(value),
                "legacy" => hash = u64::from_str_radix(value, 16).ok(),
                _ => {}
            }
        }

        Some(Self {
            id: id?,
            md5: md5?,
            legacy_hash: hash?,
            file_path: file_path?,
            title,
            subtitle,
            artist,
            genre,
            bpm,
            bpm_min: bpm_min.unwrap_or(bpm),
            bpm_max: bpm_max.unwrap_or(bpm),
            play_level,
            notes_count,
            ln_count,
            ln_mode,
            play_mode,
        })
    }
}

/// Sorts song list in-place according to the chosen sort mode.
pub fn sort_songs(
    songs: &mut [SongMetadata],
    mode: SortMode,
    store: &ScoreStore,
    ln_option: crate::rules::LnOption,
) {
    match mode {
        SortMode::Title => {
            songs.sort_by_key(|a| a.title.to_lowercase());
        }
        SortMode::Level => {
            songs.sort_by(|a, b| {
                a.play_level
                    .cmp(&b.play_level)
                    .then_with(|| a.title.cmp(&b.title))
            });
        }
        SortMode::ClearLamp => {
            songs.sort_by(|a, b| {
                let lamp_a = store.best(a, ln_option).map(|r| r.clear_type);
                let lamp_b = store.best(b, ln_option).map(|r| r.clear_type);
                lamp_b.cmp(&lamp_a).then_with(|| a.title.cmp(&b.title))
            });
        }
        SortMode::ScoreRate => {
            songs.sort_by(|a, b| {
                let acc_a = store.best(a, ln_option).map(|r| r.accuracy_rate()).unwrap_or(0.0);
                let acc_b = store.best(b, ln_option).map(|r| r.accuracy_rate()).unwrap_or(0.0);
                acc_b
                    .partial_cmp(&acc_a)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.title.cmp(&b.title))
            });
        }
        SortMode::Bpm => {
            songs.sort_by(|a, b| {
                a.bpm
                    .partial_cmp(&b.bpm)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.title.cmp(&b.title))
            });
        }
    }
}

/// First line of a song list cache.
const SONG_CACHE_HEADER: &str = "#BEETLE_SONGS_V3";

/// Serializes song list to flat cache text.
pub fn serialize_song_cache(songs: &[SongMetadata]) -> String {
    let mut out = String::with_capacity(32 + songs.len() * 256);
    out.push_str(SONG_CACHE_HEADER);
    out.push('\n');
    for song in songs {
        out.push_str(&song.serialize_line());
        out.push('\n');
    }
    out
}

/// Deserializes a song list from cache text. A cache in an older format (no
/// header) gives an empty list, which makes the app rescan and rewrite it.
pub fn deserialize_song_cache(cache_text: &str) -> Vec<SongMetadata> {
    let mut lines = cache_text.lines();
    if lines.next().map(str::trim) != Some(SONG_CACHE_HEADER) {
        return Vec::new();
    }
    lines.filter_map(SongMetadata::parse_line).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fnv1a_hash() {
        let h1 = compute_chart_hash(b"hello world");
        let h2 = compute_chart_hash(b"hello world");
        let h3 = compute_chart_hash(b"hello beetle");

        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_song_metadata_tsv_serialization() {
        let meta = SongMetadata {
            id: ChartId::synthetic(0x123456789abcdef0),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: 0x123456789abcdef0,
            file_path: "songs/test.bms".to_string(),
            title: "Test Song".to_string(),
            subtitle: "Original".to_string(),
            artist: "Sound Team".to_string(),
            genre: "Hardcore".to_string(),
            bpm: 180.0,
            bpm_min: 90.0,
            bpm_max: 240.0,
            play_level: 10,
            notes_count: 1200,
            play_mode: PlayMode::Keys7,
        };

        let line = meta.serialize_line();
        let decoded = SongMetadata::parse_line(&line).expect("Failed to parse cache line");

        assert_eq!(meta, decoded);
    }

    #[test]
    fn test_sort_songs_by_level() {
        let mut songs = vec![
            SongMetadata {
                id: ChartId::synthetic(1),
                md5: [0; 16],
                ln_count: 0,
                ln_mode: None,
                legacy_hash: 1,
                file_path: "1.bms".into(),
                title: "Song B".into(),
                subtitle: "".into(),
                artist: "A".into(),
                genre: "".into(),
                bpm: 120.0,
                bpm_min: 120.0,
                bpm_max: 120.0,
                play_level: 8,
                notes_count: 100,
                play_mode: PlayMode::Keys7,
            },
            SongMetadata {
                id: ChartId::synthetic(2),
                md5: [0; 16],
                ln_count: 0,
                ln_mode: None,
                legacy_hash: 2,
                file_path: "2.bms".into(),
                title: "Song A".into(),
                subtitle: "".into(),
                artist: "A".into(),
                genre: "".into(),
                bpm: 140.0,
                bpm_min: 140.0,
                bpm_max: 140.0,
                play_level: 4,
                notes_count: 50,
                play_mode: PlayMode::Keys7,
            },
        ];
        let store = ScoreStore::new();
        sort_songs(&mut songs, SortMode::Level, &store, crate::rules::LnOption::Auto);
        assert_eq!(songs[0].play_level, 4);
        assert_eq!(songs[1].play_level, 8);
    }

    #[test]
    fn bpm_label_shows_range_only_when_tempo_changes() {
        let content = "#TITLE T
#BPM 150
#BPM01 300
#BPM02 75
#00108:01
#00208:02
";
        let meta = SongMetadata::from_content("a.bms", content).unwrap();
        assert_eq!((meta.bpm_min, meta.bpm_max), (75.0, 300.0));
        assert_eq!(meta.bpm_label(), "75-300");

        let flat = SongMetadata::from_content("b.bms", "#TITLE T
#BPM 150
").unwrap();
        assert_eq!(flat.bpm_label(), "150");
    }

    const CHART: &[u8] = b"#TITLE T
#ARTIST A
#BPM 150
#00111:01
";

    #[test]
    fn identity_comes_from_the_raw_bytes_not_the_path_or_the_decoded_text() {
        let a = SongMetadata::from_bytes("songs/one/a.bms", CHART).unwrap();
        let b = SongMetadata::from_bytes("packages/two.bmsp::x/a.bms", CHART).unwrap();
        assert_eq!(a.id, b.id, "same bytes, different places: same chart");
        assert_eq!(a.id, ChartId::of_bytes(CHART));
        assert_eq!(a.md5, bms_hash::md5_digest(CHART));
        assert_eq!(a.legacy_hash, compute_chart_hash(CHART), "the legacy key is unchanged");

        let crlf = CHART.iter().fold(Vec::new(), |mut v, &c| {
            if c == b'\n' {
                v.push(b'\r');
            }
            v.push(c);
            v
        });
        let c = SongMetadata::from_bytes("a.bms", &crlf).unwrap();
        assert_ne!(a.id, c.id, "a different line ending is a different file");
    }

    #[test]
    fn song_cache_v3_roundtrips_and_old_caches_are_dropped() {
        let songs = vec![
            SongMetadata::from_bytes("a.bms", CHART).unwrap(),
            SongMetadata::from_bytes("b\ttab.bms", b"#TITLE Two\n#BPM 90\n#LNMODE 2\n#00112:01\n#00151:0101\n").unwrap(),
        ];
        let text = serialize_song_cache(&songs);
        assert!(text.starts_with("#BEETLE_SONGS_V3\n"));
        assert_eq!(deserialize_song_cache(&text), songs);

        // Earlier caches have no long note fields: not trusted, so the app rescans.
        let v2 = text.replacen("#BEETLE_SONGS_V3", "#BEETLE_SONGS_V2", 1);
        assert!(deserialize_song_cache(&v2).is_empty());
        let old = "0000000000000001\ta.bms\tT\t\tA\tG\t140.00\t5\t100\t7KEYS\n";
        assert!(deserialize_song_cache(old).is_empty());
    }

    #[test]
    fn long_notes_and_lnmode_are_read_from_the_chart() {
        let ln = SongMetadata::from_bytes(
            "ln.bms",
            b"#BPM 120\n#LNMODE 2\n#00112:01\n#00251:01000100\n#00252:01000100\n",
        )
        .unwrap();
        assert_eq!((ln.ln_count, ln.ln_mode), (2, Some(2)));
        let plain = SongMetadata::from_bytes("p.bms", CHART).unwrap();
        assert_eq!((plain.ln_count, plain.ln_mode), (0, None));
    }

    #[test]
    fn note_counts_follow_the_long_note_rule() {
        // A tap and two long notes: CN counts both ends of each, LN only the heads.
        let song = SongMetadata::from_bytes(
            "ln.bms",
            b"#BPM 120\n#00112:01\n#00251:01000100\n#00252:01000100\n",
        )
        .unwrap();
        assert_eq!(song.notes_count_for(LnRule::Cn), 5);
        assert_eq!(song.notes_count_for(LnRule::Ln), 3);
        let plain = SongMetadata::from_bytes("p.bms", CHART).unwrap();
        assert_eq!(plain.notes_count_for(LnRule::Ln), plain.notes_count_for(LnRule::Cn));
    }

    #[test]
    fn notes_for_counts_under_the_rule_the_setting_gives() {
        use crate::rules::LnOption;
        let song = SongMetadata::from_bytes(
            "ln.bms",
            b"#BPM 120\n#00112:01\n#00251:01000100\n#00252:01000100\n",
        )
        .unwrap();
        // No #LNMODE: AUTO is LN.
        assert_eq!(song.notes_for(LnOption::Auto), 3);
        assert_eq!(song.notes_for(LnOption::Ln), 3);
        assert_eq!(song.notes_for(LnOption::Cn), 5);
        let plain = SongMetadata::from_bytes("p.bms", CHART).unwrap();
        assert_eq!(plain.notes_for(LnOption::Cn), plain.notes_count);
    }

    #[test]
    fn a_cache_line_without_identity_is_rejected() {
        let line = SongMetadata::from_bytes("a.bms", CHART).unwrap().serialize_line();
        let without_id: Vec<&str> = line.split('\t').filter(|f| !f.starts_with("id=")).collect();
        assert!(SongMetadata::parse_line(&without_id.join("	")).is_none());
        let without_md5: Vec<&str> = line.split('\t').filter(|f| !f.starts_with("md5=")).collect();
        assert!(SongMetadata::parse_line(&without_md5.join("	")).is_none());
    }
}

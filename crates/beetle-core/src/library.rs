use crate::bms::{decode_bms_text, parse_bms, NoteType, PlayMode};
use crate::identity::{hash_chart_bytes, ChartId};
use crate::msgpack::{self, Value, Writer};
use crate::rules::LnRule;
use crate::score::ScoreStore;
use std::collections::HashMap;

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
    /// Every sort mode, in the order the sort menu lists them.
    pub const ALL: [SortMode; 5] = [
        SortMode::Title,
        SortMode::Level,
        SortMode::ClearLamp,
        SortMode::ScoreRate,
        SortMode::Bpm,
    ];

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
    /// Notes as the CN rule judges them: a long note's head and tail each count.
    /// The same count `JudgeEngine` totals under CN.
    pub notes_count: usize,
    /// How many long notes the chart has, counted by their tails (the notes the
    /// LN rule does not judge). Each pair has one of each.
    pub ln_count: u32,
    /// The chart's own `#LNMODE` (1 LN, 2 CN, 3 HCN), if it has one.
    pub ln_mode: Option<u32>,
    pub play_mode: PlayMode,
    /// The chart's `#DIFFICULTY`: 1 BEGINNER, 2 NORMAL, 3 HYPER, 4 ANOTHER,
    /// 5 INSANE. `None` when the chart does not say.
    pub difficulty: Option<u32>,
}

impl SongMetadata {
    /// What the pictures drawn for this entry (words, jacket, stage art) are
    /// kept under. A game disc lists its own copy of a chart that other discs
    /// also have, each with that disc's pictures, so a disc copy's key is its
    /// address; any other copy's is its `id`. Records stay under `id`.
    pub fn picture_key(&self) -> ChartId {
        match crate::SongAddress::parse(&self.file_path) {
            crate::SongAddress::Disc { .. } => {
                ChartId::of_bytes(format!("picture:{}", self.file_path).as_bytes())
            }
            _ => self.id,
        }
    }

    /// Extracts metadata from a chart file's raw bytes. The identity is taken
    /// from the bytes exactly as stored; only parsing sees them decoded.
    pub fn from_bytes(file_path: &str, bytes: &[u8]) -> Option<Self> {
        Self::from_bytes_with_sounds(file_path, bytes).map(|(meta, _)| meta)
    }

    /// `from_bytes`, with the key sounds the chart declares (as
    /// `key_sounds::declared_key_sounds` gives them) from the same parse.
    pub fn from_bytes_with_sounds(file_path: &str, bytes: &[u8]) -> Option<(Self, Vec<String>)> {
        let content = decode_bms_text(bytes);
        let chart = parse_bms(&content).ok()?;
        let mut sounds: Vec<String> = chart.header.wav_table.values().cloned().collect();
        sounds.sort();
        sounds.dedup();
        let legacy_hash = compute_chart_hash(content.as_bytes());
        let (id, md5) = hash_chart_bytes(bytes);
        // Every non-landmine note is judged under CN (the judge's own total).
        // `total_notes_count` is not used: it also counts notes the judge drops
        // (a note on a lane the mode does not have).
        let notes_count = chart.playable_notes_len();
        let (bpm_min, bpm_max) = chart.bpm_range();
        let is_pms = file_path.to_lowercase().ends_with(".pms");
        let play_mode = chart.detect_play_mode_with_hint(is_pms);
        let ln_count = chart
            .notes
            .iter()
            .filter(|n| n.note_type == NoteType::LongNoteEnd)
            .count() as u32;
        let ln_mode = chart.header.ln_mode;
        let difficulty = chart.header.difficulty;

        let title = if chart.header.title.is_empty() {
            "Unknown Title".to_string()
        } else {
            chart.header.title
        };

        let meta = Self {
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
            difficulty,
        };
        Some((meta, sounds))
    }

    /// How many notes the song has under a long note rule: the same as
    /// `notes_count` for CN, and without the tails for LN. This is the total
    /// `JudgeEngine` reports under that rule, so it is the note count every
    /// screen shows for a play judged under it.
    pub fn notes_count_for(&self, rule: LnRule) -> usize {
        match rule {
            LnRule::Cn => self.notes_count,
            LnRule::Ln => self.notes_count.saturating_sub(self.ln_count as usize),
        }
    }

    /// How many notes the song has under the rule the player's setting gives it
    /// (the maximum combo, and half the maximum EX score).
    pub fn notes_for(&self, option: crate::rules::LnOption) -> usize {
        self.notes_count_for(crate::rules::Ruleset::resolve(self.ln_mode, option).ln)
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

    /// Writes the song as a map of its fields (see `msgpack`). Fields that
    /// can be missing are written as nil, so every song has the same keys.
    pub fn write_msgpack(&self, w: &mut Writer) {
        w.map(17);
        w.str("path");
        w.str(&self.file_path);
        w.str("title");
        w.str(&self.title);
        w.str("sub");
        w.str(&self.subtitle);
        w.str("artist");
        w.str(&self.artist);
        w.str("genre");
        w.str(&self.genre);
        w.str("bpm");
        w.f64(self.bpm);
        w.str("bpmmin");
        w.f64(self.bpm_min);
        w.str("bpmmax");
        w.f64(self.bpm_max);
        w.str("level");
        w.uint(u64::from(self.play_level));
        w.str("notes");
        w.uint(self.notes_count as u64);
        w.str("lns");
        w.uint(u64::from(self.ln_count));
        w.str("lnmode");
        w.opt_uint(self.ln_mode.map(u64::from));
        w.str("mode");
        w.str(self.play_mode.as_str());
        w.str("diff");
        w.opt_uint(self.difficulty.map(u64::from));
        w.str("id");
        w.bin(self.id.as_bytes());
        w.str("md5");
        w.bin(&self.md5);
        w.str("legacy");
        w.uint(self.legacy_hash);
    }

    /// Reads a map written by `write_msgpack`. A song missing its path,
    /// identity or legacy key is rejected. Unknown keys are ignored.
    pub fn read_msgpack(v: &Value) -> Option<Self> {
        let text = |key| {
            v.get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let num = |key| v.get(key).and_then(Value::as_u32);
        let opt = |key| v.get(key).and_then(Value::non_nil).and_then(Value::as_u32);
        let bpm = v.get("bpm").and_then(Value::as_f64).unwrap_or(130.0);
        let play_mode = match v.get("mode").and_then(Value::as_str) {
            Some("4KEYS") => PlayMode::Keys4,
            Some("5KEYS") => PlayMode::Keys5,
            Some("6KEYS") => PlayMode::Keys6,
            Some("8KEYS") => PlayMode::Keys8,
            Some("9KEYS") => PlayMode::Keys9,
            Some("10KEYS") => PlayMode::Keys10,
            Some("14KEYS") => PlayMode::Keys14,
            _ => PlayMode::Keys7,
        };
        Some(Self {
            id: ChartId::from_digest(v.get("id")?.as_bin()?.try_into().ok()?),
            md5: v.get("md5")?.as_bin()?.try_into().ok()?,
            legacy_hash: v.get("legacy")?.as_u64()?,
            file_path: v.get("path")?.as_str()?.to_string(),
            title: text("title"),
            subtitle: text("sub"),
            artist: text("artist"),
            genre: text("genre"),
            bpm,
            bpm_min: v.get("bpmmin").and_then(Value::as_f64).unwrap_or(bpm),
            bpm_max: v.get("bpmmax").and_then(Value::as_f64).unwrap_or(bpm),
            play_level: num("level").unwrap_or(1),
            notes_count: num("notes").unwrap_or(0) as usize,
            ln_count: num("lns").unwrap_or(0),
            ln_mode: opt("lnmode"),
            play_mode,
            difficulty: opt("diff"),
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
                let acc_a = store
                    .best(a, ln_option)
                    .map(|r| r.accuracy_rate())
                    .unwrap_or(0.0);
                let acc_b = store
                    .best(b, ln_option)
                    .map(|r| r.accuracy_rate())
                    .unwrap_or(0.0);
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

/// File name of the song list cache, kept in the working directory and in the songs folder.
/// `bpm scan` deletes it so the game rebuilds it from the same copies bpm indexed.
pub const SONGS_CACHE_FILE: &str = "songs.cache";

/// `format` and `version` of a song list cache. A cache of any other
/// version is not trusted: the app rescans and rewrites it.
const SONG_CACHE_FORMAT: &str = "beetle-songs";
const SONG_CACHE_VERSION: u64 = 12;

/// The song each chart belongs to, by chart id: charts with the same number
/// are one song (see `bms_package_manager::grouping`). Numbers mean nothing
/// beyond equality and change from scan to scan. Only folder and package
/// copies are grouped; a game disc's entries are not in it.
pub type SongGroups = HashMap<ChartId, u32>;

/// One place of a song (a folder, or a folder inside a package): the address
/// of a chart copy there, and every chart with a copy there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SongPlace {
    pub address: String,
    pub charts: Vec<ChartId>,
}

/// Each song's places by its number in `SongGroups`, every copy counted (the
/// song list holds only the copy each chart loads from). In place order.
pub type SongPlaces = HashMap<u32, Vec<SongPlace>>;

/// Serializes the song list and its grouping as a cache file.
pub fn serialize_song_cache(
    songs: &[SongMetadata],
    groups: &SongGroups,
    places: &SongPlaces,
) -> Vec<u8> {
    let mut w = Writer::new();
    w.file(SONG_CACHE_FORMAT, SONG_CACHE_VERSION, 3);
    w.str("songs");
    w.array(songs.len());
    for song in songs {
        song.write_msgpack(&mut w);
    }
    // Ordered by chart id, so the same scan always writes the same bytes.
    let mut grouped: Vec<(&ChartId, &u32)> = groups.iter().collect();
    grouped.sort();
    w.str("groups");
    w.map(grouped.len());
    for (id, song) in grouped {
        w.bin(id.as_bytes());
        w.uint(u64::from(*song));
    }
    let mut by_song: Vec<(&u32, &Vec<SongPlace>)> = places.iter().collect();
    by_song.sort_by_key(|(song, _)| **song);
    w.str("places");
    w.map(by_song.len());
    for (song, places) in by_song {
        w.uint(u64::from(*song));
        w.array(places.len());
        for place in places {
            w.array(2);
            w.str(&place.address);
            w.array(place.charts.len());
            for id in &place.charts {
                w.bin(id.as_bytes());
            }
        }
    }
    w.into_bytes()
}

/// Reads a song list cache. A cache in any other format or version gives
/// an empty list, which makes the app rescan and rewrite it.
pub fn deserialize_song_cache(bytes: &[u8]) -> (Vec<SongMetadata>, SongGroups, SongPlaces) {
    msgpack::decode_file(bytes, SONG_CACHE_FORMAT, SONG_CACHE_VERSION)
        .and_then(|file| {
            let songs = file.get("songs")?.as_array()?;
            let songs = songs
                .iter()
                .filter_map(SongMetadata::read_msgpack)
                .collect();
            let Value::Map(entries) = file.get("groups")? else {
                return None;
            };
            let groups = entries
                .iter()
                .filter_map(|(id, song)| {
                    let id = ChartId::from_digest(id.as_bin()?.try_into().ok()?);
                    Some((id, song.as_u32()?))
                })
                .collect();
            let Value::Map(entries) = file.get("places")? else {
                return None;
            };
            let read_id = |v: &Value| Some(ChartId::from_digest(v.as_bin()?.try_into().ok()?));
            let places = entries
                .iter()
                .filter_map(|(song, places)| {
                    let places = places
                        .as_array()?
                        .iter()
                        .filter_map(|place| match place.as_array()? {
                            [address, charts] => Some(SongPlace {
                                address: address.as_str()?.to_string(),
                                charts: charts.as_array()?.iter().filter_map(read_id).collect(),
                            }),
                            _ => None,
                        })
                        .collect();
                    Some((song.as_u32()?, places))
                })
                .collect();
            Some((songs, groups, places))
        })
        .unwrap_or_default()
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
    fn song_metadata_round_trips() {
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
            difficulty: None,
        };

        let mut w = Writer::new();
        meta.write_msgpack(&mut w);
        let bytes = w.into_bytes();
        let decoded = SongMetadata::read_msgpack(&msgpack::decode(&bytes).unwrap())
            .expect("Failed to read cached song");

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
                difficulty: None,
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
                difficulty: None,
            },
        ];
        let store = ScoreStore::new();
        sort_songs(
            &mut songs,
            SortMode::Level,
            &store,
            crate::rules::LnOption::Auto,
        );
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

        let flat = SongMetadata::from_content(
            "b.bms",
            "#TITLE T
#BPM 150
",
        )
        .unwrap();
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
        assert_eq!(
            a.legacy_hash,
            compute_chart_hash(CHART),
            "the legacy key is unchanged"
        );

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
    fn song_cache_round_trips_and_old_caches_are_dropped() {
        let songs = vec![
            SongMetadata::from_bytes("a.bms", CHART).unwrap(),
            SongMetadata::from_bytes(
                "b\ttab.bms",
                b"#TITLE Two\n#BPM 90\n#LNMODE 2\n#00112:01\n#00151:0101\n",
            )
            .unwrap(),
        ];
        let groups = SongGroups::from([(songs[0].id, 3), (songs[1].id, 3)]);
        let places = SongPlaces::from([(
            3,
            vec![SongPlace {
                address: songs[0].file_path.clone(),
                charts: vec![songs[0].id, songs[1].id],
            }],
        )]);
        let bytes = serialize_song_cache(&songs, &groups, &places);
        assert_eq!(
            deserialize_song_cache(&bytes),
            (songs.clone(), groups, places)
        );

        // Text caches from before, and other versions, are not trusted, so the app rescans.
        let old = b"#BEETLE_SONGS_V9\npath=a.bms\ttitle=T\n";
        assert!(deserialize_song_cache(old).0.is_empty());
        let mut w = Writer::new();
        w.file(SONG_CACHE_FORMAT, SONG_CACHE_VERSION - 1, 1);
        w.str("songs");
        w.array(0);
        assert!(deserialize_song_cache(&w.into_bytes()).0.is_empty());
    }

    #[test]
    fn difficulty_is_read_from_the_chart_and_kept_in_the_cache() {
        let hyper =
            SongMetadata::from_bytes("h.bms", b"#DIFFICULTY 3\n#BPM 120\n#00112:01\n").unwrap();
        assert_eq!(hyper.difficulty, Some(3));
        let plain = SongMetadata::from_bytes("p.bms", CHART).unwrap();
        assert_eq!(plain.difficulty, None);
        let bytes = serialize_song_cache(
            &[hyper.clone(), plain.clone()],
            &SongGroups::new(),
            &SongPlaces::new(),
        );
        assert_eq!(deserialize_song_cache(&bytes).0, vec![hyper, plain]);
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
        assert_eq!(
            plain.notes_count_for(LnRule::Ln),
            plain.notes_count_for(LnRule::Cn)
        );
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
    fn a_cached_song_without_identity_is_rejected() {
        let mut w = Writer::new();
        SongMetadata::from_bytes("a.bms", CHART)
            .unwrap()
            .write_msgpack(&mut w);
        let bytes = w.into_bytes();
        let Value::Map(entries) = msgpack::decode(&bytes).unwrap() else {
            panic!("a song is a map");
        };
        for key in ["id", "md5", "legacy", "path"] {
            let without: Vec<_> = entries
                .iter()
                .filter(|(k, _)| k.as_str() != Some(key))
                .cloned()
                .collect();
            assert!(
                SongMetadata::read_msgpack(&Value::Map(without)).is_none(),
                "{key}"
            );
        }
    }
}

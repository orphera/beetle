use crate::bms::parse_bms;
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
    pub hash: u64,
    pub file_path: String,
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub genre: String,
    pub bpm: f64,
    pub play_level: u32,
    pub notes_count: usize,
}

impl SongMetadata {
    /// Extracts metadata from raw chart text.
    pub fn from_content(file_path: &str, content: &str) -> Option<Self> {
        let chart = parse_bms(content).ok()?;
        let hash = compute_chart_hash(content.as_bytes());
        let notes_count = chart.notes.len();

        let title = if chart.header.title.is_empty() {
            "Unknown Title".to_string()
        } else {
            chart.header.title
        };

        Some(Self {
            hash,
            file_path: file_path.to_string(),
            title,
            subtitle: chart.header.subtitle,
            artist: chart.header.artist,
            genre: chart.header.genre,
            bpm: chart.header.bpm,
            play_level: chart.header.play_level,
            notes_count,
        })
    }

    /// Serializes metadata to a simple flat TSV line.
    pub fn serialize_tsv(&self) -> String {
        format!(
            "{:016x}\t{}\t{}\t{}\t{}\t{}\t{:.2}\t{}\t{}",
            self.hash,
            escape_field(&self.file_path),
            escape_field(&self.title),
            escape_field(&self.subtitle),
            escape_field(&self.artist),
            escape_field(&self.genre),
            self.bpm,
            self.play_level,
            self.notes_count,
        )
    }

    /// Deserializes metadata from a TSV line.
    pub fn deserialize_tsv(line: &str) -> Option<Self> {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 9 {
            return None;
        }

        let hash = u64::from_str_radix(parts[0], 16).ok()?;
        let file_path = unescape_field(parts[1]);
        let title = unescape_field(parts[2]);
        let subtitle = unescape_field(parts[3]);
        let artist = unescape_field(parts[4]);
        let genre = unescape_field(parts[5]);
        let bpm = parts[6].parse().unwrap_or(130.0);
        let play_level = parts[7].parse().unwrap_or(1);
        let notes_count = parts[8].parse().unwrap_or(0);

        Some(Self {
            hash,
            file_path,
            title,
            subtitle,
            artist,
            genre,
            bpm,
            play_level,
            notes_count,
        })
    }
}

fn escape_field(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
}

fn unescape_field(s: &str) -> String {
    s.replace("\\n", "\n")
        .replace("\\t", "\t")
        .replace("\\\\", "\\")
}

/// Sorts song list in-place according to the chosen sort mode.
pub fn sort_songs(songs: &mut [SongMetadata], mode: SortMode, store: &ScoreStore) {
    match mode {
        SortMode::Title => {
            songs.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        }
        SortMode::Level => {
            songs.sort_by(|a, b| a.play_level.cmp(&b.play_level).then_with(|| a.title.cmp(&b.title)));
        }
        SortMode::ClearLamp => {
            songs.sort_by(|a, b| {
                let lamp_a = store.get(a.hash).map(|r| r.clear_type);
                let lamp_b = store.get(b.hash).map(|r| r.clear_type);
                lamp_b.cmp(&lamp_a).then_with(|| a.title.cmp(&b.title))
            });
        }
        SortMode::ScoreRate => {
            songs.sort_by(|a, b| {
                let acc_a = store.get(a.hash).map(|r| r.accuracy_rate).unwrap_or(0.0);
                let acc_b = store.get(b.hash).map(|r| r.accuracy_rate).unwrap_or(0.0);
                acc_b.partial_cmp(&acc_a).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.title.cmp(&b.title))
            });
        }
        SortMode::Bpm => {
            songs.sort_by(|a, b| {
                a.bpm.partial_cmp(&b.bpm).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.title.cmp(&b.title))
            });
        }
    }
}

/// Serializes song list to flat cache text.
pub fn serialize_song_cache(songs: &[SongMetadata]) -> String {
    let mut out = String::new();
    for song in songs {
        out.push_str(&song.serialize_tsv());
        out.push('\n');
    }
    out
}

/// Deserializes song list from flat cache text.
pub fn deserialize_song_cache(cache_text: &str) -> Vec<SongMetadata> {
    cache_text
        .lines()
        .filter_map(SongMetadata::deserialize_tsv)
        .collect()
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
            hash: 0x123456789abcdef0,
            file_path: "songs/test.bms".to_string(),
            title: "Test Song".to_string(),
            subtitle: "Original".to_string(),
            artist: "Sound Team".to_string(),
            genre: "Hardcore".to_string(),
            bpm: 180.0,
            play_level: 10,
            notes_count: 1200,
        };

        let tsv = meta.serialize_tsv();
        let decoded = SongMetadata::deserialize_tsv(&tsv).expect("Failed to deserialize TSV");

        assert_eq!(meta, decoded);
    }

    #[test]
    fn test_sort_songs_by_level() {
        let mut songs = vec![
            SongMetadata {
                hash: 1,
                file_path: "1.bms".into(),
                title: "Song B".into(),
                subtitle: "".into(),
                artist: "A".into(),
                genre: "".into(),
                bpm: 120.0,
                play_level: 8,
                notes_count: 100,
            },
            SongMetadata {
                hash: 2,
                file_path: "2.bms".into(),
                title: "Song A".into(),
                subtitle: "".into(),
                artist: "A".into(),
                genre: "".into(),
                bpm: 140.0,
                play_level: 4,
                notes_count: 50,
            },
        ];
        let store = ScoreStore::new();
        sort_songs(&mut songs, SortMode::Level, &store);
        assert_eq!(songs[0].play_level, 4);
        assert_eq!(songs[1].play_level, 8);
    }
}

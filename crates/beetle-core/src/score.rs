use std::collections::HashMap;

/// Clear status lamps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ClearType {
    Failed,
    Clear,
    FullCombo,
    Perfect,
}

impl ClearType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Failed => "FAILED",
            Self::Clear => "CLEARED",
            Self::FullCombo => "FULL COMBO",
            Self::Perfect => "PERFECT",
        }
    }
}

/// A stored high score and judgment record for a chart.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoreRecord {
    pub chart_hash: u64,
    pub ex_score: u32,
    pub max_combo: u32,
    pub accuracy_rate: f64,
    pub clear_type: ClearType,
    pub pgreat_count: u32,
    pub great_count: u32,
    pub good_count: u32,
    pub bad_count: u32,
    pub poor_count: u32,
    pub miss_count: u32,
}

impl ScoreRecord {
    /// Serializes a score record to a flat TSV line.
    pub fn serialize_tsv(&self) -> String {
        let clear_str = match self.clear_type {
            ClearType::Failed => "F",
            ClearType::Clear => "C",
            ClearType::FullCombo => "FC",
            ClearType::Perfect => "P",
        };

        format!(
            "{:016x}\t{}\t{}\t{:.2}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.chart_hash,
            self.ex_score,
            self.max_combo,
            self.accuracy_rate,
            clear_str,
            self.pgreat_count,
            self.great_count,
            self.good_count,
            self.bad_count,
            self.poor_count,
            self.miss_count,
        )
    }

    /// Deserializes a score record from a flat TSV line.
    pub fn deserialize_tsv(line: &str) -> Option<Self> {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 11 {
            return None;
        }

        let chart_hash = u64::from_str_radix(parts[0], 16).ok()?;
        let ex_score = parts[1].parse().ok()?;
        let max_combo = parts[2].parse().ok()?;
        let accuracy_rate = parts[3].parse().unwrap_or(0.0);
        let clear_type = match parts[4] {
            "P" => ClearType::Perfect,
            "FC" => ClearType::FullCombo,
            "C" => ClearType::Clear,
            _ => ClearType::Failed,
        };
        let pgreat_count = parts[5].parse().unwrap_or(0);
        let great_count = parts[6].parse().unwrap_or(0);
        let good_count = parts[7].parse().unwrap_or(0);
        let bad_count = parts[8].parse().unwrap_or(0);
        let poor_count = parts[9].parse().unwrap_or(0);
        let miss_count = parts[10].parse().unwrap_or(0);

        Some(Self {
            chart_hash,
            ex_score,
            max_combo,
            accuracy_rate,
            clear_type,
            pgreat_count,
            great_count,
            good_count,
            bad_count,
            poor_count,
            miss_count,
        })
    }
}

/// Local flat-file score storage manager (no SQLite / embedded DB dependencies).
#[derive(Debug, Default, Clone)]
pub struct ScoreStore {
    records: HashMap<u64, ScoreRecord>,
}

impl ScoreStore {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
        }
    }

    /// Retrieves best record for a chart hash.
    pub fn get(&self, chart_hash: u64) -> Option<&ScoreRecord> {
        self.records.get(&chart_hash)
    }

    /// Inserts or updates personal best record.
    /// Returns true if this is a new personal best EX-Score or higher clear lamp.
    pub fn update(&mut self, new_record: ScoreRecord) -> bool {
        if let Some(existing) = self.records.get_mut(&new_record.chart_hash) {
            let is_new_best = new_record.ex_score > existing.ex_score
                || new_record.clear_type > existing.clear_type;

            if is_new_best {
                *existing = new_record;
                true
            } else {
                false
            }
        } else {
            self.records.insert(new_record.chart_hash, new_record);
            true
        }
    }

    /// Deserializes entire score store from flat text.
    pub fn load_from_str(&mut self, data: &str) {
        for line in data.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                if let Some(rec) = ScoreRecord::deserialize_tsv(trimmed) {
                    self.records.insert(rec.chart_hash, rec);
                }
            }
        }
    }

    /// Serializes entire score store to flat text.
    pub fn save_to_string(&self) -> String {
        let mut out = String::new();
        for rec in self.records.values() {
            out.push_str(&rec.serialize_tsv());
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_record_serialization() {
        let record = ScoreRecord {
            chart_hash: 0xaabbccddeeff0011,
            ex_score: 1540,
            max_combo: 680,
            accuracy_rate: 98.45,
            clear_type: ClearType::FullCombo,
            pgreat_count: 700,
            great_count: 140,
            good_count: 0,
            bad_count: 0,
            poor_count: 0,
            miss_count: 0,
        };

        let tsv = record.serialize_tsv();
        let decoded = ScoreRecord::deserialize_tsv(&tsv).expect("Failed to decode score record");
        assert_eq!(record, decoded);
    }

    #[test]
    fn test_score_store_update() {
        let mut store = ScoreStore::new();
        let r1 = ScoreRecord {
            chart_hash: 1,
            ex_score: 100,
            max_combo: 50,
            accuracy_rate: 80.0,
            clear_type: ClearType::Clear,
            pgreat_count: 50,
            great_count: 0,
            good_count: 0,
            bad_count: 0,
            poor_count: 0,
            miss_count: 0,
        };
        assert!(store.update(r1));

        let r2 = ScoreRecord {
            chart_hash: 1,
            ex_score: 80, // Lower score
            max_combo: 40,
            accuracy_rate: 70.0,
            clear_type: ClearType::Clear,
            pgreat_count: 40,
            great_count: 0,
            good_count: 0,
            bad_count: 0,
            poor_count: 0,
            miss_count: 0,
        };
        assert!(!store.update(r2)); // Should not overwrite with lower score

        let r3 = ScoreRecord {
            chart_hash: 1,
            ex_score: 120, // Higher score
            max_combo: 60,
            accuracy_rate: 90.0,
            clear_type: ClearType::FullCombo,
            pgreat_count: 60,
            great_count: 0,
            good_count: 0,
            bad_count: 0,
            poor_count: 0,
            miss_count: 0,
        };
        assert!(store.update(r3));
        assert_eq!(store.get(1).unwrap().ex_score, 120);
    }
}

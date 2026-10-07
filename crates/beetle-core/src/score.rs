use crate::judge::{GaugeType, ScoreTracker};
use crate::modifier::LaneModifier;
use std::collections::HashMap;
use std::fmt::Write;

/// Version of the play and judgment rules a record was made under. Records
/// from before rules were tracked are 0.
pub const ENGINE_VERSION: u32 = 1;

/// Clear lamps, lowest to highest. The declaration order is the ranking:
/// a clear on a harder gauge outranks one on an easier gauge, and a full
/// combo outranks any plain clear whatever the gauge. (A Hazard gauge fails
/// on the first BAD, POOR or MISS, so a Hazard clear is always a full combo
/// and has no lamp of its own.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ClearType {
    #[default]
    Failed,
    /// Cleared on the Easy gauge.
    Easy,
    /// Cleared on the Groove gauge (what older records called "cleared").
    Clear,
    Hard,
    FullCombo,
    Perfect,
}

impl ClearType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Failed => "FAILED",
            Self::Easy => "EASY CLEAR",
            Self::Clear => "CLEAR",
            Self::Hard => "HARD CLEAR",
            Self::FullCombo => "FULL COMBO",
            Self::Perfect => "PERFECT",
        }
    }

    fn code(self) -> &'static str {
        match self {
            Self::Failed => "F",
            Self::Easy => "E",
            Self::Clear => "C",
            Self::Hard => "H",
            Self::FullCombo => "FC",
            Self::Perfect => "P",
        }
    }

    fn from_code(code: &str) -> Self {
        match code {
            "E" => Self::Easy,
            "C" => Self::Clear,
            "H" => Self::Hard,
            "FC" => Self::FullCombo,
            "P" => Self::Perfect,
            _ => Self::Failed,
        }
    }
}

/// Everything one finished play contributes to the store.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayResult {
    pub chart_hash: u64,
    pub lamp: ClearType,
    pub ex_score: u32,
    pub max_combo: u32,
    pub pgreat_count: u32,
    pub great_count: u32,
    pub good_count: u32,
    pub bad_count: u32,
    pub poor_count: u32,
    pub miss_count: u32,
    /// Notes the chart had in this play (the EX score's maximum is twice this).
    pub total_notes: u32,
    pub modifier: LaneModifier,
    pub gauge: GaugeType,
    /// Seed of the chart's `#RANDOM` rolls, for charts that have any.
    pub random_seed: Option<u64>,
    /// When the play ended, in seconds since the Unix epoch.
    pub played_at: u64,
}

impl PlayResult {
    pub fn from_tracker(
        chart_hash: u64,
        score: &ScoreTracker,
        modifier: LaneModifier,
        random_seed: Option<u64>,
        played_at: u64,
    ) -> Self {
        Self {
            chart_hash,
            lamp: score.clear_type(),
            ex_score: score.ex_score,
            max_combo: score.max_combo,
            pgreat_count: score.pgreat_count,
            great_count: score.great_count,
            good_count: score.good_count,
            bad_count: score.bad_count,
            poor_count: score.poor_count,
            miss_count: score.miss_count,
            total_notes: score.total_notes,
            modifier,
            gauge: score.gauge_type,
            random_seed,
            played_at,
        }
    }

    /// BAD + POOR + MISS.
    pub fn bp(&self) -> u32 {
        self.bad_count + self.poor_count + self.miss_count
    }
}

/// A chart's personal bests.
///
/// Lamp, EX score, max combo and BP are each the best ever, kept
/// independently. The judgment counts, note total, modifier, gauge and seed
/// are a snapshot of the play that set the best EX score.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScoreRecord {
    pub chart_hash: u64,
    pub clear_type: ClearType,
    pub ex_score: u32,
    pub max_combo: u32,
    /// Fewest BAD + POOR + MISS in any play.
    pub min_bp: u32,
    pub pgreat_count: u32,
    pub great_count: u32,
    pub good_count: u32,
    pub bad_count: u32,
    pub poor_count: u32,
    pub miss_count: u32,
    /// Note total of the best-EX play; 0 when it is not known.
    pub total_notes: u32,
    pub modifier: Option<LaneModifier>,
    pub gauge: Option<GaugeType>,
    pub random_seed: Option<u64>,
    pub play_count: u32,
    pub clear_count: u32,
    /// Seconds since the Unix epoch; 0 when not known.
    pub last_played: u64,
    /// Rules version of the best-EX play (see `ENGINE_VERSION`).
    pub engine: u32,
}

impl ScoreRecord {
    /// Percentage of the maximum EX score the best play reached.
    pub fn accuracy_rate(&self) -> f64 {
        let max = f64::from(self.total_notes) * 2.0;
        if max == 0.0 {
            0.0
        } else {
            f64::from(self.ex_score) / max * 100.0
        }
    }

    fn first_play(play: &PlayResult) -> Self {
        let mut record = Self {
            chart_hash: play.chart_hash,
            clear_type: play.lamp,
            max_combo: play.max_combo,
            min_bp: play.bp(),
            ..Self::default()
        };
        record.take_ex_snapshot(play);
        record.count_play(play);
        record
    }

    fn take_ex_snapshot(&mut self, play: &PlayResult) {
        self.ex_score = play.ex_score;
        self.pgreat_count = play.pgreat_count;
        self.great_count = play.great_count;
        self.good_count = play.good_count;
        self.bad_count = play.bad_count;
        self.poor_count = play.poor_count;
        self.miss_count = play.miss_count;
        self.total_notes = play.total_notes;
        self.modifier = Some(play.modifier);
        self.gauge = Some(play.gauge);
        self.random_seed = play.random_seed;
        self.engine = ENGINE_VERSION;
    }

    fn count_play(&mut self, play: &PlayResult) {
        self.play_count += 1;
        if play.lamp > ClearType::Failed {
            self.clear_count += 1;
        }
        self.last_played = play.played_at;
    }

    /// Serializes to one `field=value` line, tab separated. Optional fields
    /// that are not known are left out, so a record that never had them
    /// round-trips unchanged.
    pub fn serialize_line(&self) -> String {
        let mut line = String::with_capacity(160);
        let _ = write!(
            line,
            "chart={:016x}\tlamp={}\tex={}\tcombo={}\tbp={}\tn={}\tpg={}\tgr={}\tgd={}\tbd={}\tpr={}\tms={}",
            self.chart_hash,
            self.clear_type.code(),
            self.ex_score,
            self.max_combo,
            self.min_bp,
            self.total_notes,
            self.pgreat_count,
            self.great_count,
            self.good_count,
            self.bad_count,
            self.poor_count,
            self.miss_count,
        );
        if let Some(modifier) = self.modifier {
            let _ = write!(line, "\tmod={}", modifier.as_str());
        }
        if let Some(gauge) = self.gauge {
            let _ = write!(line, "\tgauge={}", gauge.as_str());
        }
        if let Some(seed) = self.random_seed {
            let _ = write!(line, "\tseed={seed:016x}");
        }
        let _ = write!(
            line,
            "\tplays={}\tclears={}\tlast={}\tengine={}",
            self.play_count, self.clear_count, self.last_played, self.engine
        );
        line
    }

    /// Parses a line written by `serialize_line`. Fields it does not know are
    /// ignored, so files written by a newer version still load.
    pub fn parse_line(line: &str) -> Option<Self> {
        let mut record = Self::default();
        let mut has_chart = false;
        for field in line.split('\t') {
            let Some((key, value)) = field.split_once('=') else {
                continue;
            };
            let number = || value.parse::<u32>().unwrap_or(0);
            match key {
                "chart" => {
                    record.chart_hash = u64::from_str_radix(value, 16).ok()?;
                    has_chart = true;
                }
                "lamp" => record.clear_type = ClearType::from_code(value),
                "ex" => record.ex_score = number(),
                "combo" => record.max_combo = number(),
                "bp" => record.min_bp = number(),
                "n" => record.total_notes = number(),
                "pg" => record.pgreat_count = number(),
                "gr" => record.great_count = number(),
                "gd" => record.good_count = number(),
                "bd" => record.bad_count = number(),
                "pr" => record.poor_count = number(),
                "ms" => record.miss_count = number(),
                "mod" => record.modifier = LaneModifier::from_name(value),
                "gauge" => record.gauge = GaugeType::from_name(value),
                "seed" => record.random_seed = u64::from_str_radix(value, 16).ok(),
                "plays" => record.play_count = number(),
                "clears" => record.clear_count = number(),
                "last" => record.last_played = value.parse().unwrap_or(0),
                "engine" => record.engine = number(),
                _ => {}
            }
        }
        has_chart.then_some(record)
    }

    /// Reads a line of the original format: positional columns, a single
    /// best play, no gauge or modifier. What it did not record is left empty.
    fn parse_legacy_line(line: &str) -> Option<Self> {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 11 {
            return None;
        }
        let chart_hash = u64::from_str_radix(parts[0], 16).ok()?;
        let mut ex_score: u32 = parts[1].parse().ok()?;
        let max_combo = parts[2].parse().ok()?;
        let accuracy: f64 = parts[3].parse().unwrap_or(0.0);
        // Old codes: C was a clear on whichever gauge; it is kept as Groove.
        let clear_type = ClearType::from_code(parts[4]);
        let count = |i: usize| parts[i].parse::<u32>().unwrap_or(0);
        let (bad, poor, miss) = (count(8), count(9), count(10));

        // The note total was not stored; recover it from the accuracy.
        let mut total_notes = if accuracy > 0.0 && ex_score > 0 {
            (f64::from(ex_score) * 50.0 / accuracy).round() as u32
        } else {
            0
        };
        // Over 100% is impossible: that EX score counted long note tails that
        // the total did not, so it cannot be trusted. Let the next play replace it.
        if accuracy > 100.0 {
            ex_score = 0;
            total_notes = 0;
        }

        Some(Self {
            chart_hash,
            clear_type,
            ex_score,
            max_combo,
            min_bp: bad + poor + miss,
            pgreat_count: count(5),
            great_count: count(6),
            good_count: count(7),
            bad_count: bad,
            poor_count: poor,
            miss_count: miss,
            total_notes,
            modifier: None,
            gauge: None,
            random_seed: None,
            play_count: 1,
            clear_count: u32::from(clear_type > ClearType::Failed),
            last_played: 0,
            engine: 0,
        })
    }
}

/// Which of a chart's bests a play beat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScoreUpdate {
    pub lamp: bool,
    pub ex: bool,
    pub combo: bool,
    pub bp: bool,
}

impl ScoreUpdate {
    /// True when the play set any new best.
    pub fn any(self) -> bool {
        self.lamp || self.ex || self.combo || self.bp
    }

    fn everything() -> Self {
        Self { lamp: true, ex: true, combo: true, bp: true }
    }
}

/// First line of a version 2 score file.
const FILE_HEADER: &str = "#BEETLE_SCORES_V2";

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

    /// Retrieves the personal bests for a chart hash.
    pub fn get(&self, chart_hash: u64) -> Option<&ScoreRecord> {
        self.records.get(&chart_hash)
    }

    /// Folds one finished play into the chart's record. Each best is kept on
    /// its own, so a play that only raises the lamp leaves the best EX score
    /// alone. Returns which bests the play beat.
    pub fn update(&mut self, play: PlayResult) -> ScoreUpdate {
        let Some(record) = self.records.get_mut(&play.chart_hash) else {
            self.records
                .insert(play.chart_hash, ScoreRecord::first_play(&play));
            return ScoreUpdate::everything();
        };

        let update = ScoreUpdate {
            lamp: play.lamp > record.clear_type,
            ex: play.ex_score > record.ex_score,
            combo: play.max_combo > record.max_combo,
            bp: play.bp() < record.min_bp,
        };
        if update.lamp {
            record.clear_type = play.lamp;
        }
        if update.ex {
            record.take_ex_snapshot(&play);
        }
        if update.combo {
            record.max_combo = play.max_combo;
        }
        if update.bp {
            record.min_bp = play.bp();
        }
        record.count_play(&play);
        update
    }

    /// True when `data` is a score file in the original (version 1) format.
    pub fn is_legacy_format(data: &str) -> bool {
        data.lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .is_some_and(|first| first != FILE_HEADER)
    }

    /// Loads a score file of either format into the store.
    pub fn load_from_str(&mut self, data: &str) {
        let legacy = Self::is_legacy_format(data);
        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let record = if legacy {
                ScoreRecord::parse_legacy_line(line)
            } else {
                ScoreRecord::parse_line(line)
            };
            if let Some(record) = record {
                self.records.insert(record.chart_hash, record);
            }
        }
    }

    /// Serializes the store as a version 2 file. Records are ordered by chart
    /// so the same contents always give the same text.
    pub fn save_to_string(&self) -> String {
        let mut records: Vec<&ScoreRecord> = self.records.values().collect();
        records.sort_by_key(|r| r.chart_hash);

        let mut out = String::with_capacity(32 + records.len() * 160);
        out.push_str(FILE_HEADER);
        out.push('\n');
        for record in records {
            out.push_str(&record.serialize_line());
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(hash: u64, lamp: ClearType, ex: u32, combo: u32, bp: u32) -> PlayResult {
        PlayResult {
            chart_hash: hash,
            lamp,
            ex_score: ex,
            max_combo: combo,
            pgreat_count: ex / 2,
            great_count: 0,
            good_count: 0,
            bad_count: 0,
            poor_count: 0,
            miss_count: bp,
            total_notes: 100,
            modifier: LaneModifier::Regular,
            gauge: GaugeType::Groove,
            random_seed: None,
            played_at: 1_000,
        }
    }

    #[test]
    fn lamps_rank_by_gauge_then_combo() {
        use ClearType::*;
        let ladder = [Failed, Easy, Clear, Hard, FullCombo, Perfect];
        assert!(ladder.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn each_best_is_kept_on_its_own() {
        let mut store = ScoreStore::new();
        assert!(store.update(play(1, ClearType::Clear, 150, 60, 5)).any());

        // Higher lamp, but a lower EX score, combo and a worse BP.
        let update = store.update(play(1, ClearType::Hard, 120, 40, 9));
        assert_eq!(update, ScoreUpdate { lamp: true, ex: false, combo: false, bp: false });
        let record = store.get(1).unwrap();
        assert_eq!(record.clear_type, ClearType::Hard);
        assert_eq!(record.ex_score, 150, "a lamp-only play must not erase the best EX");
        assert_eq!(record.max_combo, 60);
        assert_eq!(record.min_bp, 5);
        assert_eq!(record.gauge, Some(GaugeType::Groove), "snapshot stays with the best EX play");

        // Lower lamp but a better combo and BP.
        let update = store.update(play(1, ClearType::Easy, 100, 90, 1));
        assert_eq!(update, ScoreUpdate { lamp: false, ex: false, combo: true, bp: true });
        let record = store.get(1).unwrap();
        assert_eq!(record.clear_type, ClearType::Hard);
        assert_eq!((record.max_combo, record.min_bp), (90, 1));
    }

    #[test]
    fn a_new_best_ex_replaces_the_whole_snapshot() {
        let mut store = ScoreStore::new();
        store.update(play(1, ClearType::Clear, 100, 50, 3));
        let mut better = play(1, ClearType::Clear, 180, 50, 3);
        better.modifier = LaneModifier::Mirror;
        better.gauge = GaugeType::Hard;
        better.random_seed = Some(77);
        better.pgreat_count = 90;

        assert!(store.update(better).ex);
        let record = store.get(1).unwrap();
        assert_eq!(record.ex_score, 180);
        assert_eq!(record.pgreat_count, 90);
        assert_eq!(record.modifier, Some(LaneModifier::Mirror));
        assert_eq!(record.gauge, Some(GaugeType::Hard));
        assert_eq!(record.random_seed, Some(77));
        assert_eq!(record.accuracy_rate(), 90.0);
    }

    #[test]
    fn plays_and_clears_are_counted_every_time() {
        let mut store = ScoreStore::new();
        store.update(play(1, ClearType::Failed, 10, 5, 50));
        store.update(play(1, ClearType::Clear, 100, 50, 3));
        let mut last = play(1, ClearType::Failed, 5, 1, 90);
        last.played_at = 5_000;
        store.update(last);

        let record = store.get(1).unwrap();
        assert_eq!((record.play_count, record.clear_count), (3, 1));
        assert_eq!(record.last_played, 5_000);
    }

    #[test]
    fn equal_plays_beat_nothing() {
        let mut store = ScoreStore::new();
        store.update(play(1, ClearType::Clear, 100, 50, 3));
        assert!(!store.update(play(1, ClearType::Clear, 100, 50, 3)).any());
    }

    #[test]
    fn record_line_roundtrip_keeps_every_field() {
        let record = ScoreRecord {
            chart_hash: 0xaabb_ccdd_eeff_0011,
            clear_type: ClearType::Hard,
            ex_score: 1540,
            max_combo: 680,
            min_bp: 2,
            pgreat_count: 700,
            great_count: 140,
            good_count: 3,
            bad_count: 1,
            poor_count: 0,
            miss_count: 1,
            total_notes: 844,
            modifier: Some(LaneModifier::SRandom),
            gauge: Some(GaugeType::Hazard),
            random_seed: Some(0xfeed),
            play_count: 12,
            clear_count: 7,
            last_played: 1_791_500_000,
            engine: ENGINE_VERSION,
        };
        assert_eq!(ScoreRecord::parse_line(&record.serialize_line()), Some(record));
    }

    #[test]
    fn unknown_optional_fields_stay_unknown() {
        let record = ScoreRecord { chart_hash: 5, ..ScoreRecord::default() };
        let line = record.serialize_line();
        assert!(!line.contains("mod=") && !line.contains("gauge=") && !line.contains("seed="));
        assert_eq!(ScoreRecord::parse_line(&line), Some(record));
    }

    #[test]
    fn fields_from_a_newer_version_are_ignored() {
        let line = "chart=0000000000000001\tlamp=C\tex=10\tfuture=whatever\tcombo=4";
        let record = ScoreRecord::parse_line(line).unwrap();
        assert_eq!((record.clear_type, record.ex_score, record.max_combo), (ClearType::Clear, 10, 4));
    }

    #[test]
    fn store_text_is_deterministic_and_roundtrips() {
        let mut store = ScoreStore::new();
        for hash in [9u64, 3, 7, 1] {
            store.update(play(hash, ClearType::Clear, 100 + hash as u32, 50, 2));
        }
        let text = store.save_to_string();
        assert!(text.starts_with("#BEETLE_SCORES_V2\n"));
        let order: Vec<&str> = text.lines().skip(1).map(|l| &l[6..22]).collect();
        assert_eq!(
            order,
            ["0000000000000001", "0000000000000003", "0000000000000007", "0000000000000009"]
        );

        let mut loaded = ScoreStore::new();
        loaded.load_from_str(&text);
        assert_eq!(loaded.save_to_string(), text);
        assert!(!ScoreStore::is_legacy_format(&text));
    }

    #[test]
    fn old_files_load_with_what_they_recorded() {
        // chart, ex, combo, accuracy, clear, pg, gr, gd, bd, pr, ms
        let old = "00000000000000aa\t1500\t700\t93.75\tC\t700\t100\t0\t2\t1\t3\n\
                   00000000000000bb\t80\t30\t80.00\tF\t40\t0\t0\t0\t5\t5\n";
        assert!(ScoreStore::is_legacy_format(old));

        let mut store = ScoreStore::new();
        store.load_from_str(old);
        let a = store.get(0xaa).unwrap();
        assert_eq!(a.clear_type, ClearType::Clear);
        assert_eq!((a.ex_score, a.max_combo, a.min_bp), (1500, 700, 6));
        assert_eq!(a.total_notes, 800, "recovered from EX 1500 at 93.75%");
        assert_eq!(a.accuracy_rate(), 93.75);
        assert_eq!((a.modifier, a.gauge, a.random_seed), (None, None, None));
        assert_eq!((a.play_count, a.clear_count, a.engine), (1, 1, 0));
        assert_eq!(store.get(0xbb).unwrap().clear_type, ClearType::Failed);

        // Saving upgrades the file, and loading it back changes nothing.
        let upgraded = store.save_to_string();
        assert!(!ScoreStore::is_legacy_format(&upgraded));
        let mut again = ScoreStore::new();
        again.load_from_str(&upgraded);
        assert_eq!(again.get(0xaa), store.get(0xaa));
    }

    #[test]
    fn an_old_record_over_100_percent_gives_up_its_ex_score() {
        let old = "00000000000000cc\t1000\t500\t104.17\tFC\t500\t0\t0\t0\t0\t0\n";
        let mut store = ScoreStore::new();
        store.load_from_str(old);
        let record = store.get(0xcc).unwrap();
        assert_eq!((record.ex_score, record.total_notes), (0, 0));
        assert_eq!(record.clear_type, ClearType::FullCombo, "the lamp is still believable");

        // Any real play now beats the discarded EX score.
        assert!(store.update(play(0xcc, ClearType::Clear, 900, 400, 4)).ex);
        assert_eq!(store.get(0xcc).unwrap().ex_score, 900);
    }

    #[test]
    fn an_empty_or_garbage_file_loads_nothing() {
        let mut store = ScoreStore::new();
        store.load_from_str("");
        store.load_from_str("#BEETLE_SCORES_V2\nnot a record\n");
        assert!(store.get(0).is_none());
        assert!(!ScoreStore::is_legacy_format(""));
    }
}

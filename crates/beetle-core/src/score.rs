use crate::identity::{ChartId, ChartKey};
use crate::judge::{GaugeType, ScoreTracker};
use crate::library::SongMetadata;
use crate::modifier::LaneModifier;
use crate::rules::{LnOption, LnRule};
use std::collections::HashMap;
use std::fmt::Write;

/// Version of the play and judgment rules a record was made under. Records
/// from before rules were tracked are 0. Version 2 added the long note rule
/// (records of version 1 and earlier were made under what is now CN).
pub const ENGINE_VERSION: u32 = 2;

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
    pub chart: ChartId,
    /// The long note rule the play was judged under; `None` for a chart with no long notes.
    pub ln: Option<LnRule>,
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
        chart: ChartId,
        score: &ScoreTracker,
        modifier: LaneModifier,
        random_seed: Option<u64>,
        played_at: u64,
        ln: Option<LnRule>,
    ) -> Self {
        Self {
            chart,
            ln,
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
    /// What the record is filed under: the chart's id, or an old key awaiting migration.
    pub chart: ChartKey,
    /// The long note rule the record belongs to; `None` for a chart with no long notes.
    pub ln: Option<LnRule>,
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
            chart: ChartKey::Id(play.chart),
            ln: play.ln,
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

    fn copy_ex_snapshot_from(&mut self, other: &Self) {
        self.ex_score = other.ex_score;
        self.pgreat_count = other.pgreat_count;
        self.great_count = other.great_count;
        self.good_count = other.good_count;
        self.bad_count = other.bad_count;
        self.poor_count = other.poor_count;
        self.miss_count = other.miss_count;
        self.total_notes = other.total_notes;
        self.modifier = other.modifier;
        self.gauge = other.gauge;
        self.random_seed = other.random_seed;
        self.engine = other.engine;
    }

    /// Folds another record of the same chart into this one, each best on its
    /// own (as `ScoreStore::update` does for a play) and the histories added up.
    fn absorb(&mut self, other: &Self) {
        self.clear_type = self.clear_type.max(other.clear_type);
        if other.ex_score > self.ex_score {
            self.copy_ex_snapshot_from(other);
        }
        self.max_combo = self.max_combo.max(other.max_combo);
        self.min_bp = self.min_bp.min(other.min_bp);
        self.play_count += other.play_count;
        self.clear_count += other.clear_count;
        self.last_played = self.last_played.max(other.last_played);
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
        let _ = write!(line, "chart={}", self.chart);
        if let Some(ln) = self.ln {
            let _ = write!(line, "\tln={}", ln.as_str());
        }
        line.push('\t');
        let _ = write!(
            line,
            "lamp={}\tex={}\tcombo={}\tbp={}\tn={}\tpg={}\tgr={}\tgd={}\tbd={}\tpr={}\tms={}",
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
                    record.chart = ChartKey::parse(value)?;
                    has_chart = true;
                }
                "ln" => record.ln = LnRule::from_name(value),
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
        let chart = ChartKey::Legacy(u64::from_str_radix(parts[0], 16).ok()?);
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
            chart,
            ln: None,
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
        Self {
            lamp: true,
            ex: true,
            combo: true,
            bp: true,
        }
    }
}

/// First line of a version 2 score file.
const FILE_HEADER: &str = "#BEETLE_SCORES_V2";

/// Local flat-file score storage manager (no SQLite / embedded DB dependencies).
#[derive(Debug, Default, Clone)]
pub struct ScoreStore {
    /// By chart and long note rule: a chart with long notes has one record per rule.
    records: HashMap<(ChartKey, Option<LnRule>), ScoreRecord>,
}

impl ScoreStore {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
        }
    }

    /// The personal bests of a chart that has no long notes (and so no rule).
    /// For any chart that might have long notes use `best`.
    pub fn get(&self, chart: ChartId) -> Option<&ScoreRecord> {
        self.get_for(chart, None)
    }

    /// The personal bests of a chart under a long note rule (`None`: no long notes).
    pub fn get_for(&self, chart: ChartId, ln: Option<LnRule>) -> Option<&ScoreRecord> {
        self.records.get(&(ChartKey::Id(chart), ln))
    }

    /// The personal bests shown for a song: under the rule the player's setting
    /// gives it, or the one record it has when it has no long notes.
    pub fn best(&self, song: &SongMetadata, option: LnOption) -> Option<&ScoreRecord> {
        self.get_for(song.id, song.score_rule(option))
    }

    /// Retrieves a rule-less record by whatever key it is filed under, old keys included.
    pub fn get_key(&self, key: ChartKey) -> Option<&ScoreRecord> {
        self.records.get(&(key, None))
    }

    /// How many records are still filed under an old key.
    pub fn legacy_count(&self) -> usize {
        self.records
            .keys()
            .filter(|(chart, _)| matches!(chart, ChartKey::Legacy(_)))
            .count()
    }

    /// Re-files records made under an old key under the chart's id, given every
    /// `(old key, id)` pair the song list knows. A record whose old key matches
    /// several ids (the same decoded text in files with different bytes) is
    /// copied to each, since the player played that chart whichever file it was;
    /// one that meets a record already under the id is merged into it. Records
    /// whose chart is not in the list stay as they are until it appears.
    /// Returns how many old-key records were moved.
    pub fn migrate(&mut self, pairs: &[(u64, ChartId)]) -> usize {
        let mut ids_by_legacy: HashMap<u64, Vec<ChartId>> = HashMap::new();
        for &(legacy, id) in pairs {
            let ids = ids_by_legacy.entry(legacy).or_default();
            if !ids.contains(&id) {
                ids.push(id);
            }
        }

        let old_keys: Vec<(ChartKey, Option<LnRule>)> = self
            .records
            .keys()
            .filter(
                |(chart, _)| matches!(chart, ChartKey::Legacy(h) if ids_by_legacy.contains_key(h)),
            )
            .copied()
            .collect();

        let mut moved = 0;
        for key in old_keys {
            let ChartKey::Legacy(legacy) = key.0 else {
                continue;
            };
            let Some(record) = self.records.remove(&key) else {
                continue;
            };
            moved += 1;
            for &id in &ids_by_legacy[&legacy] {
                let mut copy = record.clone();
                copy.chart = ChartKey::Id(id);
                self.file(copy);
            }
        }
        moved
    }

    /// Files a record under its own key, merging into one already there.
    fn file(&mut self, record: ScoreRecord) {
        match self.records.get_mut(&(record.chart, record.ln)) {
            Some(existing) => existing.absorb(&record),
            None => {
                self.records.insert((record.chart, record.ln), record);
            }
        }
    }

    /// Records made before long note rules existed carry no rule, and were all
    /// made under what is now CN. For the charts given (those that have long
    /// notes) such a record moves to the CN rule, merging into a CN record if
    /// there already is one. Charts without long notes keep their rule-less
    /// record. Returns how many records moved.
    pub fn migrate_ln_rules(&mut self, charts_with_long_notes: &[ChartId]) -> usize {
        let mut moved = 0;
        for &id in charts_with_long_notes {
            let Some(mut record) = self.records.remove(&(ChartKey::Id(id), None)) else {
                continue;
            };
            moved += 1;
            record.ln = Some(LnRule::Cn);
            self.file(record);
        }
        moved
    }

    /// Folds one finished play into the chart's record. Each best is kept on
    /// its own, so a play that only raises the lamp leaves the best EX score
    /// alone. Returns which bests the play beat.
    pub fn update(&mut self, play: PlayResult) -> ScoreUpdate {
        let key = (ChartKey::Id(play.chart), play.ln);
        let Some(record) = self.records.get_mut(&key) else {
            self.records.insert(key, ScoreRecord::first_play(&play));
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
                self.records.insert((record.chart, record.ln), record);
            }
        }
    }

    /// Serializes the store as a version 2 file. Records are ordered by chart
    /// so the same contents always give the same text.
    pub fn save_to_string(&self) -> String {
        let mut records: Vec<&ScoreRecord> = self.records.values().collect();
        records.sort_by_key(|r| (r.chart, r.ln));

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

    fn play(n: u64, lamp: ClearType, ex: u32, combo: u32, bp: u32) -> PlayResult {
        PlayResult {
            chart: ChartId::synthetic(n),
            ln: None,
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
        assert_eq!(
            update,
            ScoreUpdate {
                lamp: true,
                ex: false,
                combo: false,
                bp: false
            }
        );
        let record = store.get(ChartId::synthetic(1)).unwrap();
        assert_eq!(record.clear_type, ClearType::Hard);
        assert_eq!(
            record.ex_score, 150,
            "a lamp-only play must not erase the best EX"
        );
        assert_eq!(record.max_combo, 60);
        assert_eq!(record.min_bp, 5);
        assert_eq!(
            record.gauge,
            Some(GaugeType::Groove),
            "snapshot stays with the best EX play"
        );

        // Lower lamp but a better combo and BP.
        let update = store.update(play(1, ClearType::Easy, 100, 90, 1));
        assert_eq!(
            update,
            ScoreUpdate {
                lamp: false,
                ex: false,
                combo: true,
                bp: true
            }
        );
        let record = store.get(ChartId::synthetic(1)).unwrap();
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
        let record = store.get(ChartId::synthetic(1)).unwrap();
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

        let record = store.get(ChartId::synthetic(1)).unwrap();
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
            chart: ChartKey::Id(ChartId::synthetic(0xaabb)),
            ln: Some(LnRule::Cn),
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
        assert_eq!(
            ScoreRecord::parse_line(&record.serialize_line()),
            Some(record)
        );
    }

    #[test]
    fn unknown_optional_fields_stay_unknown() {
        let record = ScoreRecord {
            chart: ChartKey::Legacy(5),
            ..ScoreRecord::default()
        };
        let line = record.serialize_line();
        assert!(!line.contains("mod=") && !line.contains("gauge=") && !line.contains("seed="));
        assert_eq!(ScoreRecord::parse_line(&line), Some(record));
    }

    #[test]
    fn fields_from_a_newer_version_are_ignored() {
        let line = "chart=0000000000000001\tlamp=C\tex=10\tfuture=whatever\tcombo=4";
        let record = ScoreRecord::parse_line(line).unwrap();
        assert_eq!(
            (record.clear_type, record.ex_score, record.max_combo),
            (ClearType::Clear, 10, 4)
        );
    }

    #[test]
    fn store_text_is_deterministic_and_roundtrips() {
        let mut store = ScoreStore::new();
        for hash in [9u64, 3, 7, 1] {
            store.update(play(hash, ClearType::Clear, 100 + hash as u32, 50, 2));
        }
        let text = store.save_to_string();
        assert!(text.starts_with("#BEETLE_SCORES_V2\n"));
        // Lines come out ordered by chart id, whatever order the plays came in.
        let mut ids: Vec<ChartId> = [9u64, 3, 7, 1]
            .into_iter()
            .map(ChartId::synthetic)
            .collect();
        ids.sort();
        let order: Vec<&str> = text
            .lines()
            .skip(1)
            .map(|l| l.split('\t').next().unwrap())
            .collect();
        let expected: Vec<String> = ids.iter().map(|id| format!("chart={id}")).collect();
        assert_eq!(order, expected);

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
        let a = store.get_key(ChartKey::Legacy(0xaa)).unwrap();
        assert_eq!(a.clear_type, ClearType::Clear);
        assert_eq!((a.ex_score, a.max_combo, a.min_bp), (1500, 700, 6));
        assert_eq!(a.total_notes, 800, "recovered from EX 1500 at 93.75%");
        assert_eq!(a.accuracy_rate(), 93.75);
        assert_eq!((a.modifier, a.gauge, a.random_seed), (None, None, None));
        assert_eq!((a.play_count, a.clear_count, a.engine), (1, 1, 0));
        assert_eq!(
            store.get_key(ChartKey::Legacy(0xbb)).unwrap().clear_type,
            ClearType::Failed
        );

        // Saving upgrades the file, and loading it back changes nothing.
        let upgraded = store.save_to_string();
        assert!(!ScoreStore::is_legacy_format(&upgraded));
        let mut again = ScoreStore::new();
        again.load_from_str(&upgraded);
        assert_eq!(
            again.get_key(ChartKey::Legacy(0xaa)),
            store.get_key(ChartKey::Legacy(0xaa))
        );
    }

    #[test]
    fn an_old_record_over_100_percent_gives_up_its_ex_score() {
        let old = "00000000000000cc\t1000\t500\t104.17\tFC\t500\t0\t0\t0\t0\t0\n";
        let mut store = ScoreStore::new();
        store.load_from_str(old);
        let record = store.get_key(ChartKey::Legacy(0xcc)).unwrap();
        assert_eq!((record.ex_score, record.total_notes), (0, 0));
        assert_eq!(
            record.clear_type,
            ClearType::FullCombo,
            "the lamp is still believable"
        );

        // Once the chart is in the song list the record moves under its id, and
        // any real play then beats the discarded EX score.
        store.migrate(&[(0xcc, ChartId::synthetic(0xcc))]);
        assert!(store.update(play(0xcc, ClearType::Clear, 900, 400, 4)).ex);
        let record = store.get(ChartId::synthetic(0xcc)).unwrap();
        assert_eq!(record.ex_score, 900);
        assert_eq!(
            record.clear_type,
            ClearType::FullCombo,
            "the old lamp is kept"
        );
    }

    fn legacy_record(hash: u64, lamp: ClearType, ex: u32, combo: u32, bp: u32) -> ScoreRecord {
        ScoreRecord {
            chart: ChartKey::Legacy(hash),
            clear_type: lamp,
            ex_score: ex,
            max_combo: combo,
            min_bp: bp,
            total_notes: 100,
            play_count: 3,
            clear_count: 2,
            last_played: 50,
            ..ScoreRecord::default()
        }
    }

    fn store_with(records: &[ScoreRecord]) -> ScoreStore {
        let mut store = ScoreStore::new();
        let text: String = records.iter().map(|r| r.serialize_line() + "\n").collect();
        store.load_from_str(&format!("#BEETLE_SCORES_V2\n{text}"));
        store
    }

    #[test]
    fn migrate_moves_an_old_record_under_the_chart_id() {
        let mut store = store_with(&[legacy_record(0xa1, ClearType::Hard, 150, 60, 4)]);
        assert_eq!(store.legacy_count(), 1);

        let id = ChartId::synthetic(1);
        assert_eq!(store.migrate(&[(0xa1, id)]), 1);

        assert_eq!(store.legacy_count(), 0);
        let record = store.get(id).unwrap();
        assert_eq!(
            (record.clear_type, record.ex_score, record.max_combo),
            (ClearType::Hard, 150, 60)
        );
        assert_eq!(record.chart, ChartKey::Id(id));
        // Nothing left to move the second time.
        assert_eq!(store.migrate(&[(0xa1, id)]), 0);
    }

    #[test]
    fn migrate_copies_to_every_id_that_shares_the_old_key() {
        let mut store = store_with(&[legacy_record(0xb2, ClearType::Clear, 120, 50, 7)]);
        let (sjis, utf8) = (ChartId::synthetic(10), ChartId::synthetic(11));
        // Same pair listed twice must not double anything.
        assert_eq!(
            store.migrate(&[(0xb2, sjis), (0xb2, utf8), (0xb2, sjis)]),
            1
        );
        assert_eq!(store.get(sjis).unwrap().ex_score, 120);
        assert_eq!(store.get(utf8).unwrap().ex_score, 120);
        assert_eq!(store.get(sjis).unwrap().play_count, 3);
    }

    #[test]
    fn migrate_merges_into_a_record_that_already_has_the_id() {
        let id = ChartId::synthetic(5);
        let mut store = ScoreStore::new();
        store.update(play(5, ClearType::Easy, 90, 80, 2)); // played since the keys changed
        let old = legacy_record(0xc3, ClearType::Hard, 140, 40, 9);
        store.load_from_str(&format!("#BEETLE_SCORES_V2\n{}\n", old.serialize_line()));

        store.migrate(&[(0xc3, id)]);
        let record = store.get(id).unwrap();
        assert_eq!(record.clear_type, ClearType::Hard, "best lamp of the two");
        assert_eq!(record.ex_score, 140, "best EX of the two");
        assert_eq!(record.max_combo, 80);
        assert_eq!(record.min_bp, 2);
        assert_eq!(record.play_count, 1 + 3, "plays add up");
        assert_eq!(record.clear_count, 1 + 2);
        assert_eq!(record.last_played, 1_000, "the later play time");
    }

    #[test]
    fn records_of_charts_not_in_the_list_wait_for_them() {
        let mut store = store_with(&[
            legacy_record(0xd4, ClearType::Clear, 100, 30, 5),
            legacy_record(0xd5, ClearType::Clear, 110, 31, 6),
        ]);
        assert_eq!(store.migrate(&[(0xd4, ChartId::synthetic(40))]), 1);
        assert_eq!(store.legacy_count(), 1);
        assert!(store.get_key(ChartKey::Legacy(0xd5)).is_some());

        // When the second chart turns up later, it is picked up then.
        assert_eq!(store.migrate(&[(0xd5, ChartId::synthetic(41))]), 1);
        assert_eq!(store.legacy_count(), 0);
        assert!(store.get(ChartId::synthetic(41)).is_some());
    }

    #[test]
    fn migrated_store_text_has_no_old_keys_left() {
        let mut store = store_with(&[legacy_record(0xe5, ClearType::Clear, 100, 30, 5)]);
        store.migrate(&[(0xe5, ChartId::synthetic(50))]);
        let text = store.save_to_string();
        assert!(text.contains("chart=sha256:"));
        assert!(!text.contains("chart=00000000000000e5"));
        let mut reloaded = ScoreStore::new();
        reloaded.load_from_str(&text);
        assert_eq!(
            reloaded.get(ChartId::synthetic(50)),
            store.get(ChartId::synthetic(50))
        );
    }

    // ---- long note rules ----

    fn ln_play(n: u64, rule: Option<LnRule>, lamp: ClearType, ex: u32) -> PlayResult {
        PlayResult {
            ln: rule,
            ..play(n, lamp, ex, 50, 2)
        }
    }

    #[test]
    fn a_chart_has_a_separate_record_for_each_long_note_rule() {
        let mut store = ScoreStore::new();
        let id = ChartId::synthetic(1);
        assert!(store
            .update(ln_play(1, Some(LnRule::Cn), ClearType::Clear, 400))
            .any());
        // Another rule is a first play, not a comparison against the CN record.
        assert_eq!(
            store.update(ln_play(1, Some(LnRule::Ln), ClearType::Easy, 200)),
            ScoreUpdate::everything()
        );

        assert_eq!(store.get_for(id, Some(LnRule::Cn)).unwrap().ex_score, 400);
        assert_eq!(store.get_for(id, Some(LnRule::Ln)).unwrap().ex_score, 200);
        assert!(store.get_for(id, None).is_none());

        // Beating one leaves the other alone.
        assert!(
            store
                .update(ln_play(1, Some(LnRule::Ln), ClearType::Hard, 260))
                .ex
        );
        assert_eq!(store.get_for(id, Some(LnRule::Cn)).unwrap().ex_score, 400);
        assert_eq!(store.get_for(id, Some(LnRule::Ln)).unwrap().ex_score, 260);
    }

    #[test]
    fn a_chart_without_long_notes_has_one_record_whatever_the_setting() {
        let mut store = ScoreStore::new();
        store.update(ln_play(2, None, ClearType::Clear, 300));
        let song = SongMetadata::from_bytes("plain.bms", b"#BPM 120\n#00111:01\n").unwrap();
        // Not the synthetic id used above: file the record under the song's own.
        store.update(PlayResult {
            chart: song.id,
            ..ln_play(2, None, ClearType::Clear, 300)
        });
        for option in [LnOption::Auto, LnOption::Ln, LnOption::Cn] {
            assert_eq!(
                store.best(&song, option).unwrap().ex_score,
                300,
                "{option:?}"
            );
        }
    }

    #[test]
    fn best_looks_a_song_up_under_the_rule_its_setting_gives() {
        let song = SongMetadata::from_bytes(
            "ln.bms",
            b"#BPM 120\n#LNMODE 2\n#00111:01\n#00151:01000100\n",
        )
        .unwrap();
        let mut store = ScoreStore::new();
        store.update(PlayResult {
            chart: song.id,
            ..ln_play(3, Some(LnRule::Cn), ClearType::Clear, 111)
        });
        store.update(PlayResult {
            chart: song.id,
            ..ln_play(3, Some(LnRule::Ln), ClearType::Clear, 222)
        });

        // The chart says CN, so AUTO shows the CN record; forcing LN shows the other.
        assert_eq!(store.best(&song, LnOption::Auto).unwrap().ex_score, 111);
        assert_eq!(store.best(&song, LnOption::Cn).unwrap().ex_score, 111);
        assert_eq!(store.best(&song, LnOption::Ln).unwrap().ex_score, 222);

        let no_mode =
            SongMetadata::from_bytes("ln2.bms", b"#BPM 120\n#00111:01\n#00151:01000100\n").unwrap();
        assert_eq!(
            no_mode.score_rule(LnOption::Auto),
            Some(LnRule::Ln),
            "LN is the default"
        );
    }

    #[test]
    fn old_records_of_charts_with_long_notes_move_to_the_cn_rule() {
        let (with_ln, without_ln) = (ChartId::synthetic(10), ChartId::synthetic(11));
        let mut store = ScoreStore::new();
        // Both made before rules existed: rule-less.
        store.update(play(10, ClearType::Hard, 300, 80, 4));
        store.update(play(11, ClearType::Clear, 200, 60, 6));
        assert_eq!(store.migrate_ln_rules(&[with_ln]), 1);

        assert!(
            store.get(with_ln).is_none(),
            "no rule-less record is left for a chart with long notes"
        );
        let moved = store.get_for(with_ln, Some(LnRule::Cn)).unwrap();
        assert_eq!((moved.clear_type, moved.ex_score), (ClearType::Hard, 300));
        assert_eq!(
            store.get(without_ln).unwrap().ex_score,
            200,
            "a chart without long notes is untouched"
        );
        // Nothing left to move the second time.
        assert_eq!(store.migrate_ln_rules(&[with_ln]), 0);
    }

    #[test]
    fn moving_to_cn_merges_into_a_cn_record_that_already_exists() {
        let id = ChartId::synthetic(12);
        let mut store = ScoreStore::new();
        store.update(play(12, ClearType::Easy, 150, 40, 9)); // old, rule-less
        store.update(ln_play(12, Some(LnRule::Cn), ClearType::Clear, 120)); // since played under CN

        store.migrate_ln_rules(&[id]);
        let record = store.get_for(id, Some(LnRule::Cn)).unwrap();
        assert_eq!(record.ex_score, 150, "best of the two");
        assert_eq!(record.clear_type, ClearType::Clear);
        assert_eq!(record.play_count, 2);
        assert!(store.get(id).is_none());
    }

    #[test]
    fn the_rule_is_saved_and_read_back_and_files_without_it_stay_rule_less() {
        let mut store = ScoreStore::new();
        store.update(ln_play(20, Some(LnRule::Ln), ClearType::Clear, 100));
        store.update(ln_play(20, Some(LnRule::Cn), ClearType::Clear, 190));
        store.update(ln_play(21, None, ClearType::Clear, 90));
        let text = store.save_to_string();
        assert!(text.contains("\tln=LN\t") && text.contains("\tln=CN\t"));
        assert_eq!(
            text.lines().filter(|l| l.contains("\tln=")).count(),
            2,
            "the rule-less record has no ln field"
        );

        let mut loaded = ScoreStore::new();
        loaded.load_from_str(&text);
        assert_eq!(loaded.save_to_string(), text);
        assert_eq!(
            loaded
                .get_for(ChartId::synthetic(20), Some(LnRule::Cn))
                .unwrap()
                .ex_score,
            190
        );

        // A file from before rules existed has no ln fields, and reads as rule-less.
        let line = format!("chart={}\tlamp=C\tex=5\tcombo=1", ChartId::synthetic(30));
        let mut old = ScoreStore::new();
        old.load_from_str(&format!("#BEETLE_SCORES_V2\n{line}\n"));
        assert!(old.get(ChartId::synthetic(30)).is_some());
    }

    #[test]
    fn new_records_carry_the_new_engine_version() {
        let mut store = ScoreStore::new();
        store.update(ln_play(40, Some(LnRule::Ln), ClearType::Clear, 100));
        let record = store
            .get_for(ChartId::synthetic(40), Some(LnRule::Ln))
            .unwrap();
        assert_eq!(record.engine, ENGINE_VERSION);
        assert_eq!(ENGINE_VERSION, 2);
    }

    #[test]
    fn an_empty_or_garbage_file_loads_nothing() {
        let mut store = ScoreStore::new();
        store.load_from_str("");
        store.load_from_str("#BEETLE_SCORES_V2\nnot a record\n");
        assert!(store.get(ChartId::synthetic(0)).is_none());
        assert!(!ScoreStore::is_legacy_format(""));
    }
}

//! Song select filters: the key modes to show, a level range, and the
//! unplayed / uncleared toggles. A pure model. The folder tree and the search
//! read it through `pass_mask`, so a filtered list, its folder counts and its
//! search results all agree.

use beetle_core::{ClearType, LnOption, PlayMode, ScoreStore, SongMetadata};

/// What the song list is filtered to. The default (every field empty or off)
/// shows every song.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Filter {
    /// Modes to show; empty means every mode.
    pub modes: Vec<PlayMode>,
    /// Lowest and highest level to show (inclusive); `None` is no bound.
    pub level_min: Option<u32>,
    pub level_max: Option<u32>,
    /// Only songs with no record under the player's long note rule.
    pub only_unplayed: bool,
    /// Only songs whose best play is not a clear (no record, or a failed one).
    pub only_uncleared: bool,
}

impl Filter {
    /// Whether any part of the filter is set (the reset button shows then).
    pub fn is_active(&self) -> bool {
        !self.modes.is_empty()
            || self.level_min.is_some()
            || self.level_max.is_some()
            || self.only_unplayed
            || self.only_uncleared
    }

    /// Shows every song again.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Adds the mode to the set, or removes it when it is there.
    pub fn toggle_mode(&mut self, mode: PlayMode) {
        match self.modes.iter().position(|&m| m == mode) {
            Some(pos) => {
                self.modes.remove(pos);
            }
            None => self.modes.push(mode),
        }
    }

    /// Whether `song` passes the filter. `scores` and `ln` pick the record
    /// the unplayed / uncleared toggles look at.
    pub fn matches(&self, song: &SongMetadata, scores: &ScoreStore, ln: LnOption) -> bool {
        if !self.modes.is_empty() && !self.modes.contains(&song.play_mode) {
            return false;
        }
        if self.level_min.is_some_and(|min| song.play_level < min) {
            return false;
        }
        if self.level_max.is_some_and(|max| song.play_level > max) {
            return false;
        }
        let best = scores.best(song, ln).map(|r| r.clear_type);
        if self.only_unplayed && best.is_some() {
            return false;
        }
        if self.only_uncleared && best.is_some_and(|clear| clear != ClearType::Failed) {
            return false;
        }
        true
    }

    /// Moves the minimum (`max_side` false) or maximum one step along `levels`
    /// (the levels the library has, ascending). The minimum starts at "no
    /// bound" on the left and the maximum ends at "no bound" on the right; the
    /// ends do not wrap. A step that would cross the other bound does nothing.
    pub fn step_level(&mut self, levels: &[u32], max_side: bool, up: bool) {
        let ring = level_ring(levels, max_side);
        let current = if max_side {
            self.level_max
        } else {
            self.level_min
        };
        let (idx, stale) = match ring.iter().position(|&v| v == current) {
            Some(i) => (i, false),
            // A level the library no longer has: its place is the next level
            // up, and one step up lands on that level.
            None => {
                let c = current.unwrap_or(0);
                let i = ring
                    .iter()
                    .position(|v| v.is_some_and(|l| l >= c))
                    .unwrap_or(ring.len() - 1);
                (i, true)
            }
        };
        let next = match (up, stale) {
            (true, true) => idx,
            (true, false) => (idx + 1).min(ring.len() - 1),
            (false, _) => idx.saturating_sub(1),
        };
        let value = ring[next];
        let crosses = match (max_side, value) {
            (false, Some(v)) => self.level_max.is_some_and(|max| v > max),
            (true, Some(v)) => self.level_min.is_some_and(|min| v < min),
            _ => false,
        };
        if crosses {
            return;
        }
        if max_side {
            self.level_max = value;
        } else {
            self.level_min = value;
        }
    }
}

/// The values a level bound steps through: `None` (no bound) at one end.
fn level_ring(levels: &[u32], max_side: bool) -> Vec<Option<u32>> {
    let mut ring: Vec<Option<u32>> = levels.iter().map(|&l| Some(l)).collect();
    if max_side {
        ring.push(None);
    } else {
        ring.insert(0, None);
    }
    ring
}

/// The distinct levels of the library, ascending.
pub fn level_steps(songs: &[SongMetadata]) -> Vec<u32> {
    let mut levels: Vec<u32> = songs.iter().map(|s| s.play_level).collect();
    levels.sort_unstable();
    levels.dedup();
    levels
}

/// Whether each song passes the filter, by song index.
pub fn pass_mask(
    songs: &[SongMetadata],
    scores: &ScoreStore,
    ln: LnOption,
    filter: &Filter,
) -> Vec<bool> {
    songs
        .iter()
        .map(|s| filter.matches(s, scores, ln))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use beetle_core::{ChartId, ChartKey, ScoreRecord};

    fn song(n: u64, level: u32, mode: PlayMode) -> SongMetadata {
        SongMetadata {
            id: ChartId::synthetic(n),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: n,
            file_path: format!("{n}.bms"),
            title: format!("song {n}"),
            subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            bpm: 120.0,
            bpm_min: 120.0,
            bpm_max: 120.0,
            play_level: level,
            notes_count: 100,
            play_mode: mode,
        }
    }

    /// A store where song `n` has a record with `clear` (no record for the rest).
    fn store_with(records: &[(u64, ClearType)]) -> ScoreStore {
        // An empty store writes the file header; the record lines follow it.
        let mut text = ScoreStore::new().save_to_string();
        for &(n, clear) in records {
            let record = ScoreRecord {
                chart: ChartKey::Id(ChartId::synthetic(n)),
                clear_type: clear,
                play_count: 1,
                ..Default::default()
            };
            text.push_str(&record.serialize_line());
            text.push('\n');
        }
        let mut store = ScoreStore::new();
        store.load_from_str(&text);
        store
    }

    #[test]
    fn the_default_filter_is_off_and_passes_everything() {
        let filter = Filter::default();
        assert!(!filter.is_active());
        let scores = ScoreStore::new();
        assert!(filter.matches(&song(1, 3, PlayMode::Keys7), &scores, LnOption::Cn));
    }

    #[test]
    fn modes_are_a_set_and_toggle_on_and_off() {
        let mut filter = Filter::default();
        filter.toggle_mode(PlayMode::Keys7);
        filter.toggle_mode(PlayMode::Keys14);
        assert!(filter.is_active());
        let scores = ScoreStore::new();
        assert!(filter.matches(&song(1, 3, PlayMode::Keys7), &scores, LnOption::Cn));
        assert!(filter.matches(&song(2, 3, PlayMode::Keys14), &scores, LnOption::Cn));
        assert!(!filter.matches(&song(3, 3, PlayMode::Keys5), &scores, LnOption::Cn));
        filter.toggle_mode(PlayMode::Keys7);
        assert_eq!(filter.modes, [PlayMode::Keys14]);
        filter.toggle_mode(PlayMode::Keys14);
        assert!(!filter.is_active());
    }

    #[test]
    fn the_level_range_is_inclusive_on_both_ends() {
        let filter = Filter {
            level_min: Some(10),
            level_max: Some(12),
            ..Default::default()
        };
        let scores = ScoreStore::new();
        let at = |level| filter.matches(&song(1, level, PlayMode::Keys7), &scores, LnOption::Cn);
        assert!(!at(9));
        assert!(at(10));
        assert!(at(12));
        assert!(!at(13));
    }

    #[test]
    fn unplayed_and_uncleared_read_the_record() {
        // 1 has no record, 2 failed, 3 cleared on Easy.
        let scores = store_with(&[(2, ClearType::Failed), (3, ClearType::Easy)]);
        let songs = [
            song(1, 3, PlayMode::Keys7),
            song(2, 3, PlayMode::Keys7),
            song(3, 3, PlayMode::Keys7),
        ];
        let unplayed = Filter {
            only_unplayed: true,
            ..Default::default()
        };
        let pass: Vec<bool> = songs
            .iter()
            .map(|s| unplayed.matches(s, &scores, LnOption::Cn))
            .collect();
        assert_eq!(pass, [true, false, false]);

        let uncleared = Filter {
            only_uncleared: true,
            ..Default::default()
        };
        let pass: Vec<bool> = songs
            .iter()
            .map(|s| uncleared.matches(s, &scores, LnOption::Cn))
            .collect();
        assert_eq!(pass, [true, true, false]);
    }

    #[test]
    fn the_level_steps_follow_the_levels_the_library_has() {
        let songs = [
            song(1, 12, PlayMode::Keys7),
            song(2, 3, PlayMode::Keys7),
            song(3, 12, PlayMode::Keys7),
            song(4, 7, PlayMode::Keys7),
        ];
        assert_eq!(level_steps(&songs), [3, 7, 12]);
    }

    #[test]
    fn the_minimum_steps_up_from_no_bound_and_back() {
        let levels = [3, 7, 12];
        let mut filter = Filter::default();
        filter.step_level(&levels, false, true);
        assert_eq!(filter.level_min, Some(3));
        filter.step_level(&levels, false, true);
        assert_eq!(filter.level_min, Some(7));
        filter.step_level(&levels, false, false);
        filter.step_level(&levels, false, false);
        assert_eq!(
            filter.level_min, None,
            "down from the first level is no bound"
        );
        filter.step_level(&levels, false, false);
        assert_eq!(filter.level_min, None, "no bound does not go further down");
    }

    #[test]
    fn the_maximum_ends_at_no_bound_and_does_not_wrap() {
        let levels = [3, 7, 12];
        let mut filter = Filter::default();
        filter.step_level(&levels, true, false);
        assert_eq!(
            filter.level_max,
            Some(12),
            "down from no bound is the last level"
        );
        filter.step_level(&levels, true, true);
        assert_eq!(filter.level_max, None);
        filter.step_level(&levels, true, true);
        assert_eq!(filter.level_max, None);
    }

    #[test]
    fn the_bounds_never_cross() {
        let levels = [3, 7, 12];
        let mut filter = Filter {
            level_min: Some(7),
            level_max: Some(7),
            ..Default::default()
        };
        filter.step_level(&levels, false, true);
        assert_eq!(filter.level_min, Some(7), "min cannot pass max");
        filter.step_level(&levels, true, false);
        assert_eq!(filter.level_max, Some(7), "max cannot pass min");
    }

    #[test]
    fn a_stale_level_steps_from_the_next_one_up() {
        // Level 5 is not in the library any more: one step up lands on 7,
        // and one step down on 3.
        let mut filter = Filter {
            level_min: Some(5),
            ..Default::default()
        };
        filter.step_level(&[3, 7, 12], false, true);
        assert_eq!(filter.level_min, Some(7));
        let mut filter = Filter {
            level_min: Some(5),
            ..Default::default()
        };
        filter.step_level(&[3, 7, 12], false, false);
        assert_eq!(filter.level_min, Some(3));
    }
}

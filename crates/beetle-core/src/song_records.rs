//! Songs that last across library reads (B18, plan 4.2 in
//! docs/plans/2026-11-01-package-by-song-plan.md): each song keeps the id it
//! got when it first appeared, the smallest chart id it had then. The
//! grouping of a library read (`SongGroups`) numbers songs afresh each time;
//! [`SongRecords::assign`] maps those songs to their records.
//!
//! A record lists every chart it has been seen with, so a song is found
//! again when the chart its id came from is gone. When songs join, the
//! smallest id stays and takes the other records' charts. When a song
//! splits, the part with the record's own chart keeps the record and the
//! other part gets a record of its own. Records of songs no longer in the
//! library stay, so their choices come back with the song.

use std::collections::{BTreeSet, HashMap};

use crate::msgpack::{self, Value, Writer};
use crate::ChartId;

/// One song's record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SongRecord {
    pub id: ChartId,
    /// Every chart seen in the song, its own id among them.
    pub charts: BTreeSet<ChartId>,
    /// The chosen key sound set: the place key the game's `sound_sets` uses.
    pub sound_set: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SongRecords {
    pub records: Vec<SongRecord>,
}

const FORMAT: &str = "beetle-songs";
const VERSION: u64 = 1;

impl SongRecords {
    pub fn parse(bytes: &[u8]) -> Self {
        let Some(file) = msgpack::decode_file(bytes, FORMAT, VERSION) else {
            return Self::default();
        };
        let id = |v: &Value| Some(ChartId::from_digest(v.as_bin()?.try_into().ok()?));
        let records = file
            .get("songs")
            .and_then(Value::as_array)
            .unwrap_or_default()
            .iter()
            .filter_map(|song| match song.as_array()? {
                [own, charts, sound_set] => {
                    let own = id(own)?;
                    let mut charts: BTreeSet<ChartId> =
                        charts.as_array()?.iter().filter_map(id).collect();
                    charts.insert(own);
                    Some(SongRecord {
                        id: own,
                        charts,
                        sound_set: sound_set
                            .non_nil()
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    })
                }
                _ => None,
            })
            .collect();
        SongRecords { records }
    }

    /// Records sorted by id, so the same records always write the same file.
    pub fn serialize(&self) -> Vec<u8> {
        let mut records: Vec<&SongRecord> = self.records.iter().collect();
        records.sort_by_key(|r| r.id);
        let mut w = Writer::new();
        w.file(FORMAT, VERSION, 1);
        w.str("songs");
        w.array(records.len());
        for r in records {
            w.array(3);
            w.bin(r.id.as_bytes());
            w.array(r.charts.len());
            for c in &r.charts {
                w.bin(c.as_bytes());
            }
            w.opt_str(r.sound_set.as_deref());
        }
        w.into_bytes()
    }

    pub fn get(&self, id: ChartId) -> Option<&SongRecord> {
        self.records.iter().find(|r| r.id == id)
    }

    pub fn get_mut(&mut self, id: ChartId) -> Option<&mut SongRecord> {
        self.records.iter_mut().find(|r| r.id == id)
    }

    /// The id of each song (its charts, in `songs` order), making and joining
    /// records as needed. Also whether any record changed.
    pub fn assign(&mut self, songs: &[Vec<ChartId>]) -> (Vec<ChartId>, bool) {
        let mut changed = false;
        let mut by_id: HashMap<ChartId, usize> = HashMap::new();
        let mut by_chart: HashMap<ChartId, Vec<usize>> = HashMap::new();
        for (i, r) in self.records.iter().enumerate() {
            by_id.insert(r.id, i);
            for c in &r.charts {
                by_chart.entry(*c).or_default().push(i);
            }
        }
        let song_of: HashMap<ChartId, usize> = songs
            .iter()
            .enumerate()
            .flat_map(|(s, charts)| charts.iter().map(move |c| (*c, s)))
            .collect();
        let mut owner: Vec<Option<usize>> = vec![None; self.records.len()];
        let mut ids: Vec<Option<ChartId>> = vec![None; songs.len()];
        // A song holding a record's own chart goes first. A song takes every
        // free record it shares a chart with, unless that record's own chart
        // is in another song (a split: that song keeps it).
        for own_chart_first in [true, false] {
            for (s, charts) in songs.iter().enumerate() {
                if ids[s].is_some() || charts.is_empty() {
                    continue;
                }
                if own_chart_first && !charts.iter().any(|c| by_id.contains_key(c)) {
                    continue;
                }
                let mut found: Vec<usize> = charts
                    .iter()
                    .flat_map(|c| by_chart.get(c).into_iter().flatten().copied())
                    .collect();
                let records = &self.records;
                found.retain(|&r| {
                    owner[r].is_none() && song_of.get(&records[r].id).is_none_or(|&o| o == s)
                });
                found.sort_by_key(|&r| self.records[r].id);
                found.dedup();
                let Some((&keep, others)) = found.split_first() else {
                    continue;
                };
                for &other in others {
                    owner[other] = Some(s);
                    let (charts, sound_set) = {
                        let o = &mut self.records[other];
                        (std::mem::take(&mut o.charts), o.sound_set.take())
                    };
                    let k = &mut self.records[keep];
                    k.charts.extend(charts);
                    if k.sound_set.is_none() {
                        k.sound_set = sound_set;
                    }
                    changed = true;
                }
                owner[keep] = Some(s);
                ids[s] = Some(self.records[keep].id);
            }
        }
        // Joined records are gone: their charts live in the one kept.
        let joined: BTreeSet<ChartId> = owner
            .iter()
            .enumerate()
            .filter(|(r, o)| o.is_some() && !ids.contains(&Some(self.records[*r].id)))
            .map(|(r, _)| self.records[r].id)
            .collect();
        self.records.retain(|r| !joined.contains(&r.id));
        let mut out = Vec::with_capacity(songs.len());
        for (s, charts) in songs.iter().enumerate() {
            let id = match ids[s] {
                Some(id) => id,
                None => {
                    let Some(&id) = charts.iter().min() else {
                        out.push(ChartId::from_digest([0; 32]));
                        continue;
                    };
                    self.records.push(SongRecord {
                        id,
                        charts: BTreeSet::new(),
                        sound_set: None,
                    });
                    id
                }
            };
            let record = self.get_mut(id).expect("assigned above");
            for c in charts {
                changed |= record.charts.insert(*c);
            }
            out.push(id);
        }
        (out, changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(n: u64) -> ChartId {
        ChartId::synthetic(n)
    }

    #[test]
    fn ids_last_through_new_charts_a_lost_chart_joins_and_splits() {
        let mut r = SongRecords::default();
        let first = c(5).min(c(3));
        let (ids, changed) = r.assign(&[vec![c(5), c(3)], vec![c(9)]]);
        assert!(changed);
        assert_eq!(ids, vec![first, c(9)]);
        r.get_mut(first).unwrap().sound_set = Some("a".into());

        // Same songs again: nothing changes.
        assert_eq!(
            r.assign(&[vec![c(3), c(5)], vec![c(9)]]),
            (ids.clone(), false)
        );

        // New charts come and the id's own chart is gone: same id.
        let (again, _) = r.assign(&[vec![c(1), c(3).max(c(5))], vec![c(9)]]);
        assert_eq!(again, ids);

        // The two songs join: the smaller id stays, with the choice.
        let joined = first.min(c(9));
        let (ids, _) = r.assign(&[vec![c(1), c(9)]]);
        assert_eq!(ids, vec![joined]);
        assert_eq!(r.records.len(), 1);
        assert_eq!(r.get(joined).unwrap().sound_set.as_deref(), Some("a"));

        // A split: the part holding the record's own chart keeps it, the
        // other part gets a record of its own.
        let parts = [vec![c(9)], vec![c(3), c(5)]];
        let (ids, _) = r.assign(&parts);
        let keeper = parts.iter().position(|p| p.contains(&joined)).unwrap();
        assert_eq!(ids[keeper], joined);
        let other = 1 - keeper;
        assert_eq!(ids[other], *parts[other].iter().min().unwrap());
        assert_eq!(r.get(ids[other]).unwrap().sound_set, None);

        assert_eq!(SongRecords::parse(&r.serialize()), r);
        assert_eq!(SongRecords::parse(b"junk"), SongRecords::default());
    }
}

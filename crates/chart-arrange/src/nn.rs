//! The neural arrangement (`nn`): a small network trained on hand-made 4K,
//! 6K and 8K charts (docs/plans/2026-11-21-arrange-nn-plan.md) picks each
//! note's lane in time order. For every free lane it reads 36 features of
//! what was placed so far (when the lane was last hit, the last groups'
//! lanes, the lanes one, two, four and eight beats back, the hands' load,
//! where in the beat the note is); one network scores each lane, and the lane
//! is drawn from the scores' softmax with the arrangement's seed, so the same
//! key always gives the same chart.
//!
//! The features and the draw follow the training script exactly
//! (scratch/nn/nnfeat.py, export_nn.py); the tests compare with its output.

use crate::{Group, SplitMix64, BAR_SLOTS};
use std::collections::HashMap;

#[path = "nn_weights.rs"]
mod weights;
use weights::{B1, B2, B3, FEATURES, HIDDEN, W1, W2, W3};

const MAX_LANES: usize = 8;
/// Gaps are read up to this long, ms.
const GAP_CAP: f64 = 4000.0;
/// Sampling temperature: 1 keeps the network's odds (the most human-like
/// patterns in the evaluation; picking the best lane repeats shapes).
const TEMPERATURE: f64 = 1.0;

/// Each group's lanes, as `crate::arrange` gives them: `None` for a note
/// that plays by itself (another part's note, or no lane free).
pub fn arrange(lanes: u8, seed: u64, groups: &[Group]) -> Vec<Vec<Option<u8>>> {
    let n = (lanes as usize).clamp(2, MAX_LANES);
    let mut st = State::new(n);
    let mut rng = SplitMix64::new(seed);
    let mut out = Vec::with_capacity(groups.len());
    for g in groups {
        if g.borrowed() {
            out.push(vec![None; g.notes.len()]);
            continue;
        }
        let t = g.ms as f64;
        let beat = g.bar as f64 * 4.0 + g.slot as f64 * 4.0 / BAR_SLOTS as f64;
        let mut chosen: Vec<usize> = Vec::new();
        let mut row = Vec::with_capacity(g.notes.len());
        let mut placed: Vec<(usize, f64)> = Vec::new();
        for (i, note) in g.notes.iter().enumerate() {
            let hold = note.hold_ms as f64;
            let (x, mask) = st.features(t, beat, g.notes.len(), i, hold, &chosen);
            if !mask.iter().any(|&m| m) {
                row.push(None);
                continue;
            }
            let mut s = [f64::NEG_INFINITY; MAX_LANES];
            for l in 0..MAX_LANES {
                if mask[l] {
                    s[l] = score(&x[l]) as f64;
                }
            }
            let l = draw(&s, &mask, &mut rng);
            chosen.push(l);
            row.push(Some(l as u8));
            placed.push((l, hold));
        }
        st.place(t, beat, &placed);
        out.push(row);
    }
    out
}

/// A lane drawn from the softmax of `s` over the free lanes.
fn draw(s: &[f64; MAX_LANES], mask: &[bool; MAX_LANES], rng: &mut SplitMix64) -> usize {
    let mx = (0..MAX_LANES)
        .filter(|&l| mask[l])
        .map(|l| s[l])
        .fold(f64::NEG_INFINITY, f64::max);
    let mut p = [0.0f64; MAX_LANES];
    let mut total = 0.0;
    for l in 0..MAX_LANES {
        if mask[l] {
            p[l] = ((s[l] - mx) / TEMPERATURE).exp();
            total += p[l];
        }
    }
    let u = rng.next_unit() * total;
    let mut acc = 0.0;
    let mut pick = 0;
    for l in 0..MAX_LANES {
        if mask[l] {
            acc += p[l];
            pick = l;
            if u < acc {
                break;
            }
        }
    }
    pick
}

/// The network's score for one lane's features.
fn score(x: &[f32; FEATURES]) -> f32 {
    let mut h1 = [0.0f32; HIDDEN];
    for (j, h) in h1.iter_mut().enumerate() {
        let mut a = B1[j];
        for (f, &v) in x.iter().enumerate() {
            a += v * W1[f * HIDDEN + j];
        }
        *h = a.max(0.0);
    }
    let mut out = B3;
    for j in 0..HIDDEN {
        let mut a = B2[j];
        for (i, &v) in h1.iter().enumerate() {
            a += v * W2[i * HIDDEN + j];
        }
        out += a.max(0.0) * W3[j];
    }
    out
}

/// What was placed so far.
struct State {
    n: usize,
    last_t: [Option<f64>; MAX_LANES],
    /// When each lane's long note ends (free from then).
    end: [f64; MAX_LANES],
    /// Each lane's hits in the last second.
    hits: [Vec<f64>; MAX_LANES],
    /// The last two groups: (ms, lanes), newest last.
    prev: Vec<(f64, Vec<usize>)>,
    /// Lanes of the group at each beat (in 1/48 beat).
    beats: HashMap<i64, Vec<usize>>,
    /// The last 16 lanes placed.
    recent: Vec<usize>,
    pos: [f64; MAX_LANES],
    side: [f64; MAX_LANES],
}

impl State {
    fn new(n: usize) -> Self {
        let mut pos = [0.0; MAX_LANES];
        let mut side = [0.0; MAX_LANES];
        for l in 0..n {
            pos[l] = (l as f64 / (n - 1) as f64) * 2.0 - 1.0;
            side[l] = if pos[l] < 0.0 { -1.0 } else { 1.0 };
        }
        State {
            n,
            last_t: [None; MAX_LANES],
            end: [f64::NEG_INFINITY; MAX_LANES],
            hits: Default::default(),
            prev: Vec::new(),
            beats: HashMap::new(),
            recent: Vec::new(),
            pos,
            side,
        }
    }

    fn features(
        &self,
        t: f64,
        beat: f64,
        gsize: usize,
        idx: usize,
        hold: f64,
        chosen: &[usize],
    ) -> ([[f32; FEATURES]; MAX_LANES], [bool; MAX_LANES]) {
        let n = self.n;
        let nf = n as f64;
        let mut xs = [[0.0f32; FEATURES]; MAX_LANES];
        let mut mask = [false; MAX_LANES];
        let log_gap = |d: f64| d.min(GAP_CAP).ln_1p() / GAP_CAP.ln_1p();
        let p1 = self.prev.last();
        let p2 = if self.prev.len() > 1 {
            self.prev.get(self.prev.len() - 2)
        } else {
            None
        };
        let dt1 = p1.map_or(1.0, |p| log_gap(t - p.0));
        let dt2 = match (p1, p2) {
            (Some(a), Some(b)) => log_gap(a.0 - b.0),
            _ => 1.0,
        };
        let b48 = (beat * 48.0).round() as i64;
        let empty: Vec<usize> = Vec::new();
        let back: Vec<&Vec<usize>> = [1i64, 2, 4, 8]
            .iter()
            .map(|k| self.beats.get(&(b48 - k * 48)).unwrap_or(&empty))
            .collect();
        let rep4 = if back[2].is_empty() { 0.0 } else { 1.0 };
        let rep8 = if back[3].is_empty() { 0.0 } else { 1.0 };
        let fr = beat.rem_euclid(1.0);
        let q4 = (fr.abs() < 1e-3 || (fr - 1.0).abs() < 1e-3) as u8 as f64;
        let q8 = ((fr - 0.5).abs() < 1e-3) as u8 as f64;
        let q16 = ((fr - 0.25).abs() < 1e-3 || (fr - 0.75).abs() < 1e-3) as u8 as f64;
        let pl: &[usize] = p1.map_or(&[], |p| &p.1);
        let pmean = (!pl.is_empty()).then(|| pl.iter().sum::<usize>() as f64 / pl.len() as f64);
        let pside: f64 = pl.iter().map(|&q| self.side[q]).sum();
        let (mut load, mut tot) = (0.0, 0usize);
        for l in 0..n {
            let c = self.hits[l].iter().filter(|&&h| t - h < 1000.0).count();
            load += c as f64 * self.side[l];
            tot += c;
        }
        let load = load / (tot + 1) as f64;
        let rc = self.recent.len().max(1) as f64;
        for l in 0..n {
            if chosen.contains(&l) || self.end[l] > t {
                continue;
            }
            mask[l] = true;
            let s = self.side[l];
            let mut x = [0.0f64; FEATURES];
            x[0] = self.pos[l];
            x[1] = s;
            x[2] = (l == 0 || l == n - 1) as u8 as f64;
            x[3] = (n == 8 && l == 0) as u8 as f64;
            x[4] = (n == 4) as u8 as f64;
            x[5] = (n == 6) as u8 as f64;
            x[6] = (n == 8) as u8 as f64;
            match self.last_t[l] {
                None => {
                    x[7] = 1.0;
                    x[8] = 1.0;
                }
                Some(lt) => x[7] = log_gap(t - lt),
            }
            let hs = &self.hits[l];
            x[9] = hs.iter().filter(|&&h| t - h < 500.0).count() as f64 / 4.0;
            x[10] = hs.iter().filter(|&&h| t - h < 1000.0).count() as f64 / 8.0;
            x[11] = pl.contains(&l) as u8 as f64;
            x[12] = p2.is_some_and(|p| p.1.contains(&l)) as u8 as f64;
            if let Some(pm) = pmean {
                let d = pl.iter().map(|&q| q.abs_diff(l)).min().unwrap_or(0);
                x[13] = d as f64 / nf;
                x[14] = (l as f64 - pm) / nf;
                x[15] = (s * pside > 0.0) as u8 as f64;
                x[16] = (d == 1) as u8 as f64;
            } else {
                x[13] = 1.0;
            }
            x[17] = dt1;
            x[18] = dt2;
            x[19] = gsize as f64 / 4.0;
            x[20] = idx as f64 / 4.0;
            x[21] = (hold > 0.0) as u8 as f64;
            x[22] = hold.ln_1p() / 10.0;
            for k in 0..4 {
                x[23 + k] = back[k].contains(&l) as u8 as f64;
            }
            x[27] = rep4;
            x[28] = rep8;
            if chosen.is_empty() {
                x[30] = 1.0;
            } else {
                x[29] = chosen.iter().filter(|&&c| self.side[c] == s).count() as f64 / 3.0;
                x[30] = chosen.iter().map(|&c| c.abs_diff(l)).min().unwrap_or(0) as f64 / nf;
            }
            x[31] = q4;
            x[32] = q8;
            x[33] = q16;
            x[34] = load * s;
            x[35] = self.recent.iter().filter(|&&r| r == l).count() as f64 / rc;
            xs[l] = x.map(|v| v as f32);
        }
        (xs, mask)
    }

    fn place(&mut self, t: f64, beat: f64, placed: &[(usize, f64)]) {
        let mut lanes = Vec::with_capacity(placed.len());
        for &(l, hold) in placed {
            self.last_t[l] = Some(t);
            self.end[l] = if hold > 0.0 {
                t + hold + 1.0
            } else {
                f64::NEG_INFINITY
            };
            let hs = &mut self.hits[l];
            hs.push(t);
            hs.retain(|&h| t - h < 1000.0);
            self.recent.push(l);
            lanes.push(l);
        }
        if self.recent.len() > 16 {
            self.recent.drain(..self.recent.len() - 16);
        }
        self.prev.push((t, lanes.clone()));
        if self.prev.len() > 2 {
            self.prev.remove(0);
        }
        self.beats.insert((beat * 48.0).round() as i64, lanes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Note;

    /// export_nn.py's sample chart: 120 BPM sixteenths, a chord every
    /// eighth group, a 600 ms long note, a rest.
    fn sample() -> Vec<Group> {
        let mut gs = Vec::new();
        for i in 0..64u32 {
            if (40..44).contains(&i) {
                continue;
            }
            let (bar, slot) = (i * 60 / 960, (i * 60 % 960) as u16);
            let holds: Vec<u32> = match i {
                20 => vec![600],
                _ if i % 8 == 0 => vec![0, 0],
                _ => vec![0],
            };
            gs.push(Group {
                ms: i as i64 * 125,
                bar,
                slot,
                accent: 0,
                notes: holds
                    .into_iter()
                    .map(|h| Note {
                        origin: 0,
                        key: 0,
                        ticks: 0,
                        hold_ms: h,
                        from: None,
                    })
                    .collect(),
            });
        }
        gs
    }

    fn lanes_text(out: &[Vec<Option<u8>>]) -> String {
        out.iter()
            .flatten()
            .map(|l| l.map_or('-', |l| (b'0' + l) as char))
            .collect()
    }

    /// The same lanes as the training script for the same seed
    /// (scratch/nn/export_nn.py, seed 7).
    #[test]
    fn matches_the_training_script() {
        let gs = sample();
        let want = [
            (
                4,
                "1032102102132321302310123103123021201320231001231231012120321221212",
            ),
            (
                6,
                "2053213212054443412151235402525031420512351012343251023131522341234",
            ),
            (
                8,
                "3064324234065453523562436514626134521633462213454352135242562453425",
            ),
        ];
        for (lanes, want) in want {
            assert_eq!(lanes_text(&arrange(lanes, 7, &gs)), want, "{lanes}K");
        }
    }

    #[test]
    fn a_long_note_holds_its_lane_and_chords_split() {
        let gs = sample();
        for lanes in [4u8, 6, 8] {
            for seed in 0..20 {
                let out = arrange(lanes, seed, &gs);
                for (g, row) in gs.iter().zip(&out) {
                    let mut seen = Vec::new();
                    for l in row.iter().flatten() {
                        assert!(*l < lanes && !seen.contains(l));
                        seen.push(*l);
                    }
                    let _ = g;
                }
                // The long note at group 20 (600 ms) keeps its lane through
                // the next four sixteenths.
                let ln = out[20][0].unwrap();
                for row in &out[21..25] {
                    assert!(!row.contains(&Some(ln)), "{lanes}K seed {seed}");
                }
            }
        }
    }

    #[test]
    fn seeds_change_the_chart_and_repeat_it() {
        let gs = sample();
        assert_eq!(arrange(6, 3, &gs), arrange(6, 3, &gs));
        assert_ne!(arrange(6, 3, &gs), arrange(6, 4, &gs));
    }
}

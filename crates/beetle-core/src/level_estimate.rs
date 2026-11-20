//! A level for a 4K/6K/8K chart, estimated from its notes: how hard an
//! arranged chart is when no table knows it.
//!
//! A small network (16 features, 24 hidden units) trained on the 4UE, 6UE and
//! 8UE difficulty tables (1..30, 19+ as 19.5, Ω as 31). Off by about 0.8 of a
//! level on charts it was not trained on, and orders charts almost as the
//! tables do (docs/plans/2026-11-21-arrange-nn-plan.md). The features are
//! computed exactly as in the training script, so the weights fit them.

use crate::bms::{BmsChart, NoteType, PlayMode};
use crate::timing::TimingModel;

#[path = "level_estimate_weights.rs"]
mod weights;
use weights::{B1, B2, FEATURES, HIDDEN, MEAN, SCALE, W1, W2};

/// Notes closer than this start one group (a chord).
const CHORD_S: f64 = 0.002;
/// Two single notes this close in a row count for `samehand`.
const QUICK_S: f64 = 0.22;

/// The estimated level of a chart of `keys` lanes (4, 6 or 8) from its
/// playable notes: (seconds, lane from the left, held seconds; 0 a tap).
/// `None` for other key counts or fewer than two notes.
pub fn estimate(notes: &[(f64, u8, f64)], keys: u8) -> Option<f32> {
    if !matches!(keys, 4 | 6 | 8) {
        return None;
    }
    let x = features(notes, keys)?;
    let mut out = B2;
    for h in 0..HIDDEN {
        let mut a = B1[h];
        for (f, &v) in x.iter().enumerate() {
            a += (v - MEAN[f]) / SCALE[f] * W1[f * HIDDEN + h];
        }
        out += a.max(0.0) * W2[h];
    }
    Some(out)
}

/// The estimated level of a parsed 4K/6K/8K chart (`estimate`), its notes
/// timed by `timing`. `None` for other key modes.
pub fn estimate_chart(chart: &BmsChart, timing: &TimingModel) -> Option<f32> {
    let mode = chart
        .header
        .declared_mode
        .unwrap_or_else(|| chart.detect_play_mode());
    let keys: u8 = match mode {
        PlayMode::Keys4 => 4,
        PlayMode::Keys6 => 6,
        PlayMode::Keys8 => 8,
        _ => return None,
    };
    let lanes = mode.restricted_lanes()?;
    let at = |n: &crate::bms::NoteEvent| timing.beat_to_time_seconds(n.measure, n.fraction);
    let mut order: Vec<usize> = (0..chart.notes.len()).collect();
    order.sort_by(|&a, &b| at(&chart.notes[a]).total_cmp(&at(&chart.notes[b])));
    let mut notes = Vec::with_capacity(chart.notes.len());
    // Index in `notes` of the long note each lane holds.
    let mut open: [Option<usize>; 8] = [None; 8];
    for i in order {
        let n = &chart.notes[i];
        let Some(lane) = lanes.iter().position(|&l| l == n.lane) else {
            continue;
        };
        let t = at(n);
        match n.note_type {
            NoteType::Tap => notes.push((t, lane as u8, 0.0)),
            NoteType::LongNoteStart => {
                open[lane] = Some(notes.len());
                notes.push((t, lane as u8, 0.0));
            }
            NoteType::LongNoteEnd => {
                if let Some(j) = open[lane].take() {
                    notes[j].2 = t - notes[j].0;
                }
            }
            NoteType::Landmine => {}
        }
    }
    estimate(&notes, keys)
}

fn features(notes: &[(f64, u8, f64)], keys: u8) -> Option<[f32; FEATURES]> {
    let mut notes = notes.to_vec();
    notes.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let n = notes.len();
    if n < 2 {
        return None;
    }
    let nf = n as f64;
    let (t0, t1) = (notes[0].0, notes[n - 1].0);
    let dur = (t1 - t0).max(1.0);

    // Groups: (start time, lanes).
    let mut groups: Vec<(f64, Vec<u8>)> = Vec::new();
    for &(t, l, _) in &notes {
        match groups.last_mut() {
            Some(g) if t - g.0 < CHORD_S => g.1.push(l),
            _ => groups.push((t, vec![l])),
        }
    }
    let ts: Vec<f64> = notes.iter().map(|x| x.0).collect();

    // Notes in 2 s windows, every 0.5 s.
    let mut dens = Vec::new();
    let (mut lo, mut hi) = (0, 0);
    let mut w = t0;
    while w <= t1 {
        while lo < n && ts[lo] < w {
            lo += 1;
        }
        while hi < n && ts[hi] < w + 2.0 {
            hi += 1;
        }
        dens.push((hi - lo) as f64 / 2.0);
        w += 0.5;
    }
    dens.sort_by(f64::total_cmp);
    let pct = |q: f64| dens[((q * (dens.len() - 1) as f64 + 0.5) as usize).min(dens.len() - 1)];

    let mut last = [f64::NAN; 8];
    let (mut j125, mut j190) = (0usize, 0usize);
    for &(t, l, _) in &notes {
        let prev = last[l as usize];
        if !prev.is_nan() {
            j125 += (t - prev < 0.125) as usize;
            j190 += (t - prev < 0.190) as usize;
        }
        last[l as usize] = t;
    }
    let chords = groups.iter().filter(|g| g.1.len() > 1).count();
    let half = keys as f64 / 2.0;
    let (mut same, mut pairs) = (0usize, 0usize);
    for w in groups.windows(2) {
        let ((ta, la), (tb, lb)) = ((w[0].0, &w[0].1), (w[1].0, &w[1].1));
        if la.len() == 1 && lb.len() == 1 && tb - ta < QUICK_S {
            pairs += 1;
            same += (((la[0] as f64) < half) == ((lb[0] as f64) < half)) as usize;
        }
    }
    let mut peak = 0usize;
    let mut j = 0;
    for (i, &t) in ts.iter().enumerate() {
        while t - ts[j] > 4.0 {
            j += 1;
        }
        peak = peak.max(i - j + 1);
    }
    let mean4 = nf / dur * 4.0;
    let gl = groups.len() as f64;
    let held = notes.iter().filter(|x| x.2 > 0.0).count();
    let v = [
        (nf / dur).ln_1p(),
        (gl / dur).ln_1p(),
        pct(0.5).ln_1p(),
        pct(0.8).ln_1p(),
        pct(0.95).ln_1p(),
        pct(0.99).ln_1p(),
        j125 as f64 / nf,
        j190 as f64 / nf,
        chords as f64 / gl,
        nf / gl,
        held as f64 / nf,
        if pairs > 0 {
            same as f64 / pairs as f64
        } else {
            0.0
        },
        if mean4 > 0.0 {
            peak as f64 / mean4
        } else {
            0.0
        },
        (keys == 4) as u8 as f64,
        (keys == 6) as u8 as f64,
        (keys == 8) as u8 as f64,
    ];
    Some(v.map(|x| x as f32))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Steady eighth notes at 150 BPM walking over the lanes.
    fn stream(keys: u8, per_second: f64, seconds: f64) -> Vec<(f64, u8, f64)> {
        let count = (per_second * seconds) as usize;
        (0..count)
            .map(|i| (i as f64 / per_second, (i % keys as usize) as u8, 0.0))
            .collect()
    }

    #[test]
    fn denser_charts_are_harder() {
        for keys in [4, 6, 8] {
            let easy = estimate(&stream(keys, 3.0, 90.0), keys).unwrap();
            let hard = estimate(&stream(keys, 12.0, 90.0), keys).unwrap();
            assert!(hard > easy + 3.0, "{keys}K: {easy} vs {hard}");
        }
    }

    /// The same network as the training script: 4K streams of 3 and 12
    /// notes a second (scratch/nn/level_small.py gives 5.51 and 16.87).
    #[test]
    fn levels_match_the_training_script() {
        let easy = estimate(&stream(4, 3.0, 90.0), 4).unwrap();
        let hard = estimate(&stream(4, 12.0, 90.0), 4).unwrap();
        assert!((easy - 5.51).abs() < 0.01, "{easy}");
        assert!((hard - 16.87).abs() < 0.01, "{hard}");
    }

    #[test]
    fn only_four_six_and_eight_keys() {
        assert!(estimate(&stream(7, 5.0, 60.0), 7).is_none());
        assert!(estimate(&[(0.0, 0, 0.0)], 4).is_none());
    }

    /// The features match the training script's on a small hand-made chart
    /// (values from scratch/nn/level_small.py `feats`).
    #[test]
    fn features_match_the_training_script() {
        let notes = [
            (0.0, 0, 0.0),
            (0.0, 3, 0.0),
            (0.1, 1, 0.0),
            (0.2, 1, 0.5),
            (0.3, 2, 0.0),
            (1.0, 0, 0.0),
            (2.5, 3, 0.0),
            (3.0, 2, 0.0),
        ];
        let f = features(&notes, 4).unwrap();
        let want = [
            1.299283, 1.203973, 0.6931472, 0.6931472, 1.386294, 1.386294, 0.125, 0.125, 0.1428571,
            1.142857, 0.125, 0.5, 0.75, 1.0, 0.0, 0.0,
        ];
        for (i, (a, b)) in f.iter().zip(want).enumerate() {
            assert!((a - b).abs() < 1e-5, "feature {i}: {a} vs {b}");
        }
    }
}

//! Keyboard arrangements of GuitarFreaks & DrumMania charts (B29): the same
//! notes at the same times with the same sounds, on lanes chosen again so a
//! keyboard can play them (`docs/plans/2026-11-10-gfdm-keyboard-arrange-plan.md`).
//!
//! An arrangement is a lane count (4, 6 or 8) and a set of features, each a
//! rule that can be switched on or off, plus a seed. The lanes come from a
//! search over the notes in time order (a beam over the lowest cost
//! placements); the costs are a lane hit again too soon, the part's liked
//! lanes, lanes and hands used unevenly, and seeded noise. The same chart,
//! features and seed always give the same lanes.
//!
//! The arrangement is written into chart keys as `@<N>k[+<feature>...]`:
//!
//! | feature | meaning |
//! |---|---|
//! | `pitch` | guitar and bass: low notes to the left, high to the right, within a phrase. Without it, the frets spread over the lanes |
//! | `home` | drums: each drum has its own lane (a table per lane count). Without it, no drum likes a lane |
//! | `stick` | drums: a drum hit again quickly goes between its lane and the one next to it |
//! | `trill` | a sound or drum hit again quickly goes to a lane next to the last one |
//! | `bal` | lanes and hands used unevenly cost more |
//! | `j<ms>` | the shortest time between two hits on one lane (default by difficulty) |
//! | `r<0-9>` | how much seeded noise goes into the costs; 0 none |
//! | `s<seed>` | the seed (decimal); means nothing when `r` is 0 |

/// Lane counts an arrangement can have.
pub const LANE_COUNTS: [u8; 3] = [4, 6, 8];
/// The most noise an arrangement takes (`r`).
pub const MAX_NOISE: u8 = 9;

/// How a chart is laid out again. Two arrangements that are equal give the
/// same lanes; `key` writes each in one way only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Arrange {
    /// 4, 6 or 8.
    pub lanes: u8,
    pub pitch: bool,
    pub home: bool,
    pub stick: bool,
    pub trill: bool,
    pub bal: bool,
    /// The shortest time between two hits on one lane, ms; `None` the
    /// difficulty's default (`default_jack_ms`).
    pub jack_ms: Option<u16>,
    /// Noise in the costs, 0..=`MAX_NOISE`.
    pub noise: u8,
    pub seed: u64,
}

/// Which kind of part an arrangement is for: features of the other kind
/// mean nothing and are dropped (`Arrange::for_part`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartKind {
    Drums,
    /// Guitar or bass.
    Strings,
}

impl Arrange {
    /// The arrangement with only what changes the lanes of `kind` kept: so
    /// two keys that give the same chart are one key.
    pub fn for_part(mut self, kind: PartKind) -> Self {
        match kind {
            PartKind::Drums => self.pitch = false,
            PartKind::Strings => {
                self.home = false;
                self.stick = false;
            }
        }
        if self.noise == 0 {
            self.seed = 0;
        }
        self
    }

    /// `<N>k[+feature...]`, the features in a fixed order (without the `@`).
    pub fn key(&self) -> String {
        let mut out = format!("{}k", self.lanes);
        for (on, name) in [
            (self.pitch, "pitch"),
            (self.home, "home"),
            (self.stick, "stick"),
            (self.trill, "trill"),
            (self.bal, "bal"),
        ] {
            if on {
                out += "+";
                out += name;
            }
        }
        if let Some(j) = self.jack_ms {
            out += &format!("+j{j}");
        }
        if self.noise > 0 {
            out += &format!("+r{}", self.noise);
            if self.seed != 0 {
                out += &format!("+s{}", self.seed);
            }
        }
        out
    }

    /// The arrangement `key` writes (without the `@`), features in any order.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('+');
        let lanes: u8 = parts.next()?.strip_suffix('k')?.parse().ok()?;
        if !LANE_COUNTS.contains(&lanes) {
            return None;
        }
        let mut a = Arrange {
            lanes,
            ..Default::default()
        };
        for p in parts {
            match p {
                "pitch" => a.pitch = true,
                "home" => a.home = true,
                "stick" => a.stick = true,
                "trill" => a.trill = true,
                "bal" => a.bal = true,
                _ => {
                    let (head, num) = p.split_at(p.char_indices().nth(1)?.0);
                    if num.is_empty() || !num.bytes().all(|b| b.is_ascii_digit()) {
                        return None;
                    }
                    match head {
                        "j" => a.jack_ms = Some(num.parse().ok().filter(|&j| j > 0)?),
                        "r" => a.noise = num.parse().ok().filter(|&r| r <= MAX_NOISE)?,
                        "s" => a.seed = num.parse().ok()?,
                        _ => return None,
                    }
                }
            }
        }
        Some(a)
    }
}

/// The shortest time between two hits on one lane at `difficulty` (1 BASIC,
/// 2 ADVANCED, 3 EXTREME), ms.
pub fn default_jack_ms(difficulty: u8) -> u16 {
    match difficulty {
        1 => 250,
        2 => 180,
        _ => 125,
    }
}

/// SplitMix64: a small seeded generator (Steele, Lea and Flood 2014).
#[derive(Debug, Clone)]
pub struct SplitMix64(u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number in [0, 1).
    pub fn next_unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// One note to place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Note {
    /// Its lane in the original chart: the drum (0 HH, 1 SD, 2 BD, 3 HT,
    /// 4 LT, 5 CY) or the fret (0 R, 1 G, 2 B).
    pub origin: u8,
    /// Its sound key (`Event::a` low 16 bits). For guitar and bass the low
    /// byte is the note's pitch.
    pub key: u16,
}

/// Notes at one place in the chart, played together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// Ms from the start.
    pub ms: i64,
    pub notes: Vec<Note>,
}

/// Each drum's own lane and the lane it shares quick hits with (`stick`),
/// by lane count, indexed by drum (HH, SD, BD, HT, LT, CY).
fn drum_lanes(lanes: u8) -> ([u8; 6], [u8; 6]) {
    match lanes {
        // HH and CY / SD / BD / toms.
        4 => ([0, 1, 2, 3, 3, 0], [1, 0, 3, 2, 2, 1]),
        // The original order.
        6 => ([0, 1, 2, 3, 4, 5], [1, 0, 3, 2, 5, 4]),
        // A spare lane by HH (left) and by SD (right) for quick hits.
        _ => ([1, 2, 4, 5, 6, 7], [0, 3, 5, 6, 5, 6]),
    }
}

// Costs. A hit on a lane sooner than the shortest repeat costs `JACK_BASE`
// and up to `JACK` more the sooner it is.
const JACK_BASE: f64 = 300.0;
const JACK: f64 = 700.0;
/// A lane away from a liked lane.
const PREFER: f64 = 100.0;
/// Each lane further than next to the last lane of a quick repeat (`trill`).
const TRILL: f64 = 150.0;
/// Each recent hit (decayed over `LOAD_HALF_LIFE_MS`) on a lane, and each
/// more on one hand than the other (`bal`).
const LANE_LOAD: f64 = 30.0;
const HAND_LOAD: f64 = 15.0;
const LOAD_HALF_LIFE_MS: f64 = 1000.0;
/// Noise at `r1`.
const NOISE: f64 = 40.0;
/// A rest this long ends a phrase (`pitch`).
const PHRASE_GAP_MS: i64 = 750;
/// Placements kept at each group.
const BEAM: usize = 48;
/// Lane counts go up to this.
const MAX_LANES: usize = 8;
/// Recent sounds or drums kept for repeats.
const RECENT: usize = 8;

#[derive(Clone)]
struct State {
    /// Ms of each lane's last hit (`i64::MIN / 4`: never).
    last: [i64; MAX_LANES],
    /// Hits on each lane, decayed to `load_ms`.
    load: [f64; MAX_LANES],
    load_ms: i64,
    /// Recent notes by identity: (identity, lane, ms), newest first.
    recent: [(u16, u8, i64); RECENT],
    cost: f64,
    /// This placement's node in the history.
    node: u32,
}

/// What a quick repeat is a repeat of: the drum, or the pitch.
fn identity(kind: PartKind, n: &Note) -> u16 {
    match kind {
        PartKind::Drums => n.origin as u16,
        PartKind::Strings => 0x100 | (n.key & 0xff),
    }
}

/// The lane each note of each group goes on, `None` for a note left over
/// when a group has more notes than lanes (it plays as a sound only).
/// `difficulty` sets the default shortest repeat on a lane.
pub fn arrange(
    a: &Arrange,
    kind: PartKind,
    difficulty: u8,
    groups: &[Group],
) -> Vec<Vec<Option<u8>>> {
    let a = a.for_part(kind);
    let n = a.lanes.clamp(1, MAX_LANES as u8) as usize;
    let jack = a.jack_ms.unwrap_or_else(|| default_jack_ms(difficulty)) as f64;
    let repeat_ms = (2.0 * jack).max(250.0) as i64;
    let (home, alt) = drum_lanes(a.lanes);

    // Each note's liked lane (a fraction of a lane allowed), by group.
    let liked = liked_lanes(&a, kind, n, groups, &home);

    // Which notes play, in the order they take lanes (left to right).
    let order: Vec<Vec<usize>> = groups
        .iter()
        .zip(&liked)
        .map(|(g, likes)| {
            let mut idx: Vec<usize> = (0..g.notes.len()).collect();
            // The notes kept when there are too many: by original lane.
            idx.sort_by_key(|&i| (g.notes[i].origin, i));
            idx.truncate(n);
            idx.sort_by(|&x, &y| {
                let (lx, ly) = (likes[x].unwrap_or(0.0), likes[y].unwrap_or(0.0));
                lx.total_cmp(&ly)
                    .then(g.notes[x].origin.cmp(&g.notes[y].origin))
                    .then(x.cmp(&y))
            });
            idx
        })
        .collect();

    // Lane sets for k notes: bit masks, ascending, with their lanes.
    let sets: Vec<Vec<(u16, Vec<u8>)>> = (0..=n)
        .map(|k| {
            (0u16..1 << n)
                .filter(|m| m.count_ones() as usize == k)
                .map(|m| (m, (0..n as u8).filter(|l| m & 1 << l != 0).collect()))
                .collect()
        })
        .collect();

    // History of placements: (parent, lane mask).
    let mut nodes: Vec<(u32, u16)> = vec![(u32::MAX, 0)];
    let never = i64::MIN / 4;
    let mut beam = vec![State {
        last: [never; MAX_LANES],
        load: [0.0; MAX_LANES],
        load_ms: 0,
        recent: [(u16::MAX, 0, never); RECENT],
        cost: 0.0,
        node: 0,
    }];
    let seed = SplitMix64::new(a.seed).next_u64();
    let bucket = (jack / 2.0).max(1.0);
    let half = n / 2;
    // Per group, reused: each state's note-by-lane costs, the candidates.
    let mut costs: Vec<f64> = Vec::new();
    let mut next: Vec<(f64, u32, u16)> = Vec::new();
    let mut seen: std::collections::HashSet<(u16, [u8; MAX_LANES])> =
        std::collections::HashSet::new();

    for (gi, g) in groups.iter().enumerate() {
        let ids = &order[gi];
        let k = ids.len();
        let mut noise = [0.0f64; MAX_LANES];
        if a.noise > 0 {
            for (lane, v) in noise.iter_mut().enumerate().take(n) {
                let mut r = SplitMix64::new(
                    seed ^ (gi as u64).wrapping_mul(0x2545_f491_4f6c_dd1d) ^ lane as u64,
                );
                *v = r.next_unit() * NOISE * a.noise as f64;
            }
        }
        next.clear();
        for (si, s) in beam.iter().enumerate() {
            // Loads decayed to now.
            let decay = 0.5f64.powf((g.ms - s.load_ms) as f64 / LOAD_HALF_LIFE_MS);
            let mut load = [0.0f64; MAX_LANES];
            let mut hands = [0.0f64; 2];
            for l in 0..n {
                load[l] = s.load[l] * decay;
                hands[(l >= half) as usize] += load[l];
            }
            // The cost of each note on each lane.
            costs.clear();
            for &i in ids {
                let note = &g.notes[i];
                let id = identity(kind, note);
                let prev = s
                    .recent
                    .iter()
                    .find(|r| r.0 == id && g.ms - r.2 <= repeat_ms)
                    .map(|r| r.1);
                let mut target = liked[gi][i];
                if kind == PartKind::Drums && a.stick {
                    let d = note.origin.min(5) as usize;
                    if let Some(last_lane) = prev {
                        target = Some(if last_lane == home[d] {
                            alt[d]
                        } else {
                            home[d]
                        } as f64);
                    }
                }
                for l in 0..n {
                    let mut cost = noise[l];
                    let gap = (g.ms - s.last[l]) as f64;
                    if gap < jack {
                        cost += JACK_BASE + JACK * (jack - gap) / jack;
                    }
                    if let Some(t) = target {
                        cost += PREFER * (l as f64 - t).abs();
                    }
                    if let (true, Some(last_lane)) = (a.trill, prev) {
                        let d = (l as i32 - last_lane as i32).unsigned_abs();
                        cost += TRILL * d.saturating_sub(1) as f64;
                    }
                    if a.bal {
                        cost += LANE_LOAD * load[l];
                    }
                    costs.push(cost);
                }
            }
            for (mask, lanes) in &sets[k] {
                let mut cost = s.cost;
                for (j, &lane) in lanes.iter().enumerate() {
                    cost += costs[j * n + lane as usize];
                }
                if a.bal {
                    let mut h = hands;
                    for &lane in lanes {
                        h[(lane as usize >= half) as usize] += 1.0;
                    }
                    cost += HAND_LOAD * (h[0] - h[1]).abs();
                }
                next.push((cost, si as u32, *mask));
            }
        }
        // Lowest cost first; ties by the order they were made in. Of
        // placements alike in the lanes they just hit and how long ago each
        // lane was hit, only the cheapest is kept.
        next.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
        seen.clear();
        let mut kept = Vec::with_capacity(BEAM);
        for &(cost, si, mask) in &next {
            let s = &beam[si as usize];
            let mut sig = [0u8; MAX_LANES];
            for (l, v) in sig.iter_mut().enumerate().take(n) {
                if mask & 1 << l == 0 {
                    *v = ((g.ms - s.last[l]) as f64 / bucket).clamp(0.0, 4.0) as u8;
                }
            }
            if !seen.insert((mask, sig)) {
                continue;
            }
            let mut t = s.clone();
            t.cost = cost;
            let decay = 0.5f64.powf((g.ms - s.load_ms) as f64 / LOAD_HALF_LIFE_MS);
            for v in t.load.iter_mut() {
                *v *= decay;
            }
            t.load_ms = g.ms;
            let lanes = (0..n as u8).filter(|l| mask & 1 << l != 0);
            for (&i, lane) in ids.iter().zip(lanes) {
                t.last[lane as usize] = g.ms;
                t.load[lane as usize] += 1.0;
                t.recent.copy_within(0..RECENT - 1, 1);
                t.recent[0] = (identity(kind, &g.notes[i]), lane, g.ms);
            }
            nodes.push((s.node, mask));
            t.node = (nodes.len() - 1) as u32;
            kept.push(t);
            if kept.len() == BEAM {
                break;
            }
        }
        beam = kept;
    }

    // Back from the best placement.
    let best = beam
        .iter()
        .min_by(|x, y| x.cost.total_cmp(&y.cost))
        .map_or(0, |s| s.node);
    let mut masks = vec![0u16; groups.len()];
    let mut at = best;
    for gi in (0..groups.len()).rev() {
        masks[gi] = nodes[at as usize].1;
        at = nodes[at as usize].0;
    }
    groups
        .iter()
        .enumerate()
        .map(|(gi, g)| {
            let mut out = vec![None; g.notes.len()];
            let lanes = (0..n as u8).filter(|l| masks[gi] & 1 << l != 0);
            for (&i, lane) in order[gi].iter().zip(lanes) {
                out[i] = Some(lane);
            }
            out
        })
        .collect()
}

/// Each note's liked lane (`None`: any), by group.
fn liked_lanes(
    a: &Arrange,
    kind: PartKind,
    n: usize,
    groups: &[Group],
    home: &[u8; 6],
) -> Vec<Vec<Option<f64>>> {
    let top = (n - 1) as f64;
    match kind {
        PartKind::Drums => groups
            .iter()
            .map(|g| {
                g.notes
                    .iter()
                    .map(|note| a.home.then(|| home[note.origin.min(5) as usize] as f64))
                    .collect()
            })
            .collect(),
        PartKind::Strings if !a.pitch => groups
            .iter()
            .map(|g| {
                g.notes
                    .iter()
                    .map(|note| Some(note.origin.min(2) as f64 * top / 2.0))
                    .collect()
            })
            .collect(),
        PartKind::Strings => {
            let mut out: Vec<Vec<Option<f64>>> = Vec::with_capacity(groups.len());
            let mut start = 0;
            while start < groups.len() {
                let mut end = start + 1;
                while end < groups.len() && groups[end].ms - groups[end - 1].ms < PHRASE_GAP_MS {
                    end += 1;
                }
                let pitches = groups[start..end]
                    .iter()
                    .flat_map(|g| g.notes.iter().map(|n| (n.key & 0xff) as f64));
                let (lo, hi) =
                    pitches.fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p), hi.max(p)));
                for g in &groups[start..end] {
                    // A chord's notes keep their fret order around its pitch.
                    let k = g.notes.len() as f64;
                    out.push(
                        g.notes
                            .iter()
                            .enumerate()
                            .map(|(rank, note)| {
                                let p = (note.key & 0xff) as f64;
                                let at = if hi > lo {
                                    (p - lo) / (hi - lo) * top
                                } else {
                                    top / 2.0
                                };
                                let spread = rank as f64 - (k - 1.0) / 2.0;
                                Some((at + spread).clamp(0.0, top))
                            })
                            .collect(),
                    );
                }
                start = end;
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_in_one_spelling() {
        let a = Arrange::parse("6k+trill+pitch+j180+r2+s7").unwrap();
        assert_eq!(a.key(), "6k+pitch+trill+j180+r2+s7");
        assert_eq!(Arrange::parse(&a.key()), Some(a));
        let plain = Arrange::parse("4k").unwrap();
        assert_eq!(plain.key(), "4k");
        for bad in [
            "5k", "4", "4k+foo", "4k+r10", "4k+j", "4k+j0", "4k+jx", "k", "4k+s-1",
        ] {
            assert_eq!(Arrange::parse(bad), None, "{bad}");
        }
        let all = Arrange {
            lanes: 8,
            pitch: true,
            home: true,
            stick: true,
            trill: true,
            bal: true,
            jack_ms: Some(150),
            noise: 9,
            seed: 12345,
        };
        assert_eq!(Arrange::parse(&all.key()), Some(all));
    }

    #[test]
    fn a_part_drops_the_other_parts_features_and_an_unused_seed() {
        let a = Arrange::parse("6k+pitch+home+stick+s5").unwrap();
        assert_eq!(a.for_part(PartKind::Drums).key(), "6k+home+stick");
        assert_eq!(a.for_part(PartKind::Strings).key(), "6k+pitch");
    }

    #[test]
    fn splitmix_matches_the_reference() {
        // The reference generator's first outputs from seed 0.
        let mut r = SplitMix64::new(0);
        assert_eq!(r.next_u64(), 0xe220_a839_7b1d_cdaf);
        assert_eq!(r.next_u64(), 0x6e78_9e6a_a1b9_65f4);
    }

    fn run(kind: PartKind, origins: &[(i64, &[u8])]) -> Vec<Group> {
        origins
            .iter()
            .map(|&(ms, o)| Group {
                ms,
                notes: o
                    .iter()
                    .map(|&origin| Note {
                        origin,
                        key: match kind {
                            PartKind::Drums => 36,
                            PartKind::Strings => 0x2400 | (0x30 + origin as u16),
                        },
                    })
                    .collect(),
            })
            .collect()
    }

    #[test]
    fn a_fast_repeat_moves_off_its_lane() {
        // Sixteenths at 150 BPM (100 ms) on one fret.
        let groups = run(
            PartKind::Strings,
            &(0..16).map(|i| (i * 100, &[1u8][..])).collect::<Vec<_>>(),
        );
        let a = Arrange::parse("6k+trill").unwrap();
        let lanes = arrange(&a, PartKind::Strings, 2, &groups);
        let flat: Vec<u8> = lanes.iter().map(|l| l[0].unwrap()).collect();
        for w in flat.windows(2) {
            assert_ne!(w[0], w[1], "{flat:?}");
            assert_eq!((w[0] as i32 - w[1] as i32).abs(), 1, "{flat:?}");
        }
    }

    #[test]
    fn chords_keep_their_size_up_to_the_lanes() {
        let groups = run(PartKind::Drums, &[(0, &[0, 1, 2, 3, 4, 5]), (500, &[0, 2])]);
        let a = Arrange::parse("4k+home").unwrap();
        let lanes = arrange(&a, PartKind::Drums, 3, &groups);
        assert_eq!(lanes[0].iter().filter(|l| l.is_some()).count(), 4);
        assert_eq!(lanes[1].iter().filter(|l| l.is_some()).count(), 2);
        let mut used: Vec<u8> = lanes[0].iter().flatten().copied().collect();
        used.dedup();
        assert_eq!(used.len(), 4);
    }

    #[test]
    fn the_seed_changes_the_lanes_only_with_noise() {
        let pattern: Vec<(i64, &[u8])> = (0..64)
            .map(|i| (i * 150, [&[0u8][..], &[1], &[2], &[1, 2]][i as usize % 4]))
            .collect();
        let groups = run(PartKind::Drums, &pattern);
        let at = |key: &str| arrange(&Arrange::parse(key).unwrap(), PartKind::Drums, 2, &groups);
        assert_eq!(at("6k+home+r3+s1"), at("6k+home+r3+s1"));
        assert_ne!(at("6k+home+r3+s1"), at("6k+home+r3+s2"));
        assert_eq!(at("6k+home+s1"), at("6k+home+s2"));
    }
}

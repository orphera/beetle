//! Keyboard arrangements of GuitarFreaks & DrumMania charts (B29): the same
//! song with the notes on lanes chosen again so a keyboard can play them
//! (`docs/plans/2026-11-10-gfdm-keyboard-arrange-plan.md`).
//!
//! An arrangement is a lane count (4, 6 or 8) and a set of features, each a
//! rule that can be switched on or off, plus a seed. The lanes come from a
//! search over the notes in time order (a beam over the lowest cost
//! placements); the costs are a lane hit again too soon, the part's liked
//! lanes, lanes and hands used unevenly, and seeded noise. The same chart,
//! features and seed always give the same lanes.
//!
//! No sound is lost: a note taken off the lanes (thinned, or more at once
//! than there are lanes) still plays, by itself. Notes of the other parts
//! (which play by themselves in the original) can be brought onto the lanes
//! (`mix`); their times and sounds are those of the other part's own chart.
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
//! | `hand` | quick notes that stay on one hand cost more the longer they do: fast runs go between the hands |
//! | `vary` | a bar that comes back as it was gets its lanes laid out differently (by the seed) |
//! | `edge` | 8K: the two outer lanes take only accents (cymbals, kicks and long or chord notes on strong beats), seldom |
//! | `j<ms>` | the shortest time between two hits on one lane (default by difficulty) |
//! | `d<ms>` | the shortest time between two played notes: denser notes leave the lanes (and play by themselves), weak beats first |
//! | `mix<1-9>` | brings the other parts' notes onto the lanes where the part rests; more the higher |
//! | `r<0-9>` | how much seeded noise goes into the costs; 0 none |
//! | `s<seed>` | the seed (decimal); means nothing without `r` or `vary` |

/// Lane counts an arrangement can have.
pub const LANE_COUNTS: [u8; 3] = [4, 6, 8];
/// The most noise an arrangement takes (`r`).
pub const MAX_NOISE: u8 = 9;
/// The most of the other parts an arrangement brings in (`mix`).
pub const MAX_MIX: u8 = 9;
/// Places in a bar `Group::slot` counts.
pub const BAR_SLOTS: u16 = 960;

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
    pub hand: bool,
    pub vary: bool,
    pub edge: bool,
    /// The shortest time between two hits on one lane, ms; `None` the
    /// difficulty's default (`default_jack_ms`).
    pub jack_ms: Option<u16>,
    /// The shortest time between two played notes, ms; `None` keeps all.
    pub thin_ms: Option<u16>,
    /// How much of the other parts comes onto the lanes, 0..=`MAX_MIX`.
    pub mix: u8,
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
        if self.lanes != 8 {
            self.edge = false;
        }
        if self.noise == 0 && !self.vary {
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
            (self.hand, "hand"),
            (self.vary, "vary"),
            (self.edge, "edge"),
        ] {
            if on {
                out += "+";
                out += name;
            }
        }
        if let Some(j) = self.jack_ms {
            out += &format!("+j{j}");
        }
        if let Some(d) = self.thin_ms {
            out += &format!("+d{d}");
        }
        if self.mix > 0 {
            out += &format!("+mix{}", self.mix);
        }
        if self.noise > 0 {
            out += &format!("+r{}", self.noise);
        }
        if self.seed != 0 && (self.noise > 0 || self.vary) {
            out += &format!("+s{}", self.seed);
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
        let number = |s: &str| -> Option<u64> {
            (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                .then(|| s.parse().ok())
                .flatten()
        };
        for p in parts {
            match p {
                "pitch" => a.pitch = true,
                "home" => a.home = true,
                "stick" => a.stick = true,
                "trill" => a.trill = true,
                "bal" => a.bal = true,
                "hand" => a.hand = true,
                "vary" => a.vary = true,
                "edge" => a.edge = true,
                _ => {
                    if let Some(m) = p.strip_prefix("mix") {
                        a.mix = number(m).filter(|&m| m <= MAX_MIX as u64)? as u8;
                        continue;
                    }
                    let (head, num) = p.split_at(p.char_indices().nth(1)?.0);
                    let num = number(num)?;
                    let ms = || u16::try_from(num).ok().filter(|&v| v > 0);
                    match head {
                        "j" => a.jack_ms = Some(ms()?),
                        "d" => a.thin_ms = Some(ms()?),
                        "r" => a.noise = (num <= MAX_NOISE as u64).then_some(num as u8)?,
                        "s" => a.seed = num,
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
    /// Its lane in its own chart: the drum (0 HH, 1 SD, 2 BD, 3 HT, 4 LT,
    /// 5 CY) or the fret (0 R, 1 G, 2 B).
    pub origin: u8,
    /// Its sound key (`Event::a` low 16 bits). For guitar and bass the low
    /// byte is the note's pitch; drum keys are about General MIDI drums.
    pub key: u16,
    /// Its length in 1/300 s (guitar and bass; `Event::a >> 16`).
    pub ticks: u16,
    /// `None` for a note of the arranged part; for another part's note
    /// (`mix`), that part's kind.
    pub from: Option<PartKind>,
}

/// Notes at one place in the chart, played together. The notes of a group
/// are all of the arranged part, or all of one other part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// Ms from the start.
    pub ms: i64,
    pub bar: u32,
    /// Where in the bar, in 1/`BAR_SLOTS` of it.
    pub slot: u16,
    pub notes: Vec<Note>,
}

impl Group {
    fn borrowed(&self) -> bool {
        self.notes.first().is_some_and(|n| n.from.is_some())
    }

    /// How strong the group's beat is: 4 the bar's start, 3 its half, 2 a
    /// quarter, 1 an eighth, 0 anything else.
    fn strength(&self) -> u8 {
        let s = self.slot as u32 * 8;
        let q = BAR_SLOTS as u32;
        match () {
            _ if self.slot == 0 => 4,
            _ if s % (q * 4) == 0 => 3,
            _ if s % (q * 2) == 0 => 2,
            _ if s % q == 0 => 1,
            _ => 0,
        }
    }
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
/// `hand`: notes this close are one quick run; each more of a run on one
/// hand costs `HAND_RUN` times its place in the run.
const FAST_MS: i64 = 220;
const HAND_RUN: f64 = 70.0;
/// `vary`: the most a lane's bias in a returning bar costs.
const VARY: f64 = 160.0;
/// `edge`: an outer note that is not an accent, one sooner than
/// `EDGE_GAP_MS` after the last outer note, and the pull an accent has.
const EDGE_PLAIN: f64 = 600.0;
const EDGE_SOON: f64 = 500.0;
const EDGE_GAP_MS: i64 = 700;
const EDGE_ACCENT: f64 = 120.0;
/// `edge`: a guitar or bass note this long (1/300 s) is an accent.
const LONG_TICKS: u16 = 90;
/// `mix`: another part's note comes in when the arranged part has no note
/// within `MIX_GAP_MS - MIX_STEP_MS * mix` of it.
const MIX_GAP_MS: i64 = 600;
const MIX_STEP_MS: i64 = 50;
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
    /// The last group's time and hand (0 left, 1 right, 2 both), and how
    /// many quick groups in a row stayed on that hand (`hand`).
    group_ms: i64,
    side: u8,
    run: u16,
    /// The last outer-lane hit (`edge`).
    edge_ms: i64,
    cost: f64,
    /// This placement's node in the history.
    node: u32,
}

/// What a quick repeat is a repeat of: the drum, or the pitch.
fn identity(kind: PartKind, n: &Note) -> u16 {
    match n.from.unwrap_or(kind) {
        PartKind::Drums => n.origin as u16,
        PartKind::Strings => 0x100 | (n.key & 0xff),
    }
}

/// Whether note `i` of `g` is an accent (`edge`): a cymbal, a kick on a
/// strong beat; a long note, or a chord's lowest note on a beat; anything
/// on a bar's first beat.
fn accent(kind: PartKind, g: &Group, i: usize) -> bool {
    let n = &g.notes[i];
    let strength = g.strength();
    if strength == 4 {
        return true;
    }
    match n.from.unwrap_or(kind) {
        PartKind::Drums => n.origin == 5 || (n.origin == 2 && strength >= 3),
        PartKind::Strings => {
            let lowest = g.notes.iter().all(|m| (m.key & 0xff) >= (n.key & 0xff));
            n.ticks >= LONG_TICKS || (g.notes.len() > 1 && lowest && strength >= 2)
        }
    }
}

/// How much a group is kept when notes are too dense (`d`, `mix`): strong
/// beats, chords, kicks, snares and cymbals, a change of pitch, long notes;
/// another part's notes least.
fn priority(kind: PartKind, g: &Group, last_pitch: Option<u16>) -> i32 {
    let mut p = 2 * g.strength() as i32;
    if g.notes.len() > 1 {
        p += 2;
    }
    for n in &g.notes {
        match n.from.unwrap_or(kind) {
            PartKind::Drums => {
                p += match n.origin {
                    1 | 2 | 5 => 2,
                    3 | 4 => 1,
                    _ => 0,
                }
            }
            PartKind::Strings => {
                if last_pitch.is_some_and(|l| l != n.key & 0xff) {
                    p += 2;
                }
                if n.ticks >= 60 {
                    p += 1;
                }
            }
        }
    }
    if g.borrowed() {
        p -= 3;
    }
    p
}

/// Of `order` (group indices in time order), those kept when no two may be
/// closer than `gap` ms: of two too close, the lower `prio` goes (the later
/// on a tie).
fn thin(order: &[usize], groups: &[Group], prio: &[i32], gap: i64) -> Vec<usize> {
    let mut kept: Vec<usize> = Vec::with_capacity(order.len());
    'next: for &g in order {
        while let Some(&top) = kept.last() {
            if groups[g].ms - groups[top].ms >= gap {
                break;
            }
            if prio[g] > prio[top] {
                kept.pop();
            } else {
                continue 'next;
            }
        }
        kept.push(g);
    }
    kept
}

/// The groups that take lanes (in time order): the arranged part's, other
/// parts' where the part rests (`mix`), then thinned (`d`).
fn played(a: &Arrange, kind: PartKind, groups: &[Group]) -> Vec<usize> {
    let mut last_pitch = None;
    let prio: Vec<i32> = groups
        .iter()
        .map(|g| {
            let p = priority(kind, g, last_pitch);
            if !g.borrowed() && kind == PartKind::Strings {
                last_pitch = g.notes.first().map(|n| n.key & 0xff);
            }
            p
        })
        .collect();
    let main_ms: Vec<i64> = groups
        .iter()
        .filter(|g| !g.borrowed())
        .map(|g| g.ms)
        .collect();
    let mut order: Vec<usize> = (0..groups.len())
        .filter(|&i| !groups[i].borrowed())
        .collect();
    if a.mix > 0 {
        let gap = MIX_GAP_MS - MIX_STEP_MS * a.mix as i64;
        let free: Vec<usize> = (0..groups.len())
            .filter(|&i| groups[i].borrowed())
            .filter(|&i| {
                let ms = groups[i].ms;
                let at = main_ms.partition_point(|&m| m < ms);
                let before = at.checked_sub(1).map(|j| ms - main_ms[j]);
                let after = main_ms.get(at).map(|m| m - ms);
                before.into_iter().chain(after).all(|d| d >= gap)
            })
            .collect();
        order.extend(thin(&free, groups, &prio, gap / 2));
        order.sort_by_key(|&i| (groups[i].ms, groups[i].borrowed(), i));
    }
    match a.thin_ms {
        Some(d) => thin(&order, groups, &prio, d as i64),
        None => order,
    }
}

/// How a returning bar is laid out again (`vary`): its liked lanes turned
/// (1 mirrored, 2 moved one lane right, 3 mirrored and moved left) and a
/// small lane bias.
#[derive(Debug, Clone, Copy)]
struct Turn {
    how: u8,
    bias: [f64; MAX_LANES],
}

impl Turn {
    fn apply(&self, t: f64, top: f64) -> f64 {
        match self.how {
            1 => top - t,
            2 => (t + 1.0).min(top),
            _ => (top - t - 1.0).max(0.0),
        }
    }
}

/// Each returning bar's turn (`vary`), by group: `None` for a bar the
/// first time it comes, and for other parts' notes.
fn bar_turns(a: &Arrange, kind: PartKind, n: usize, groups: &[Group]) -> Vec<Option<Turn>> {
    let mut out = vec![None; groups.len()];
    if !a.vary {
        return out;
    }
    let mut seen: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
    let mut start = 0;
    while start < groups.len() {
        let bar = groups[start].bar;
        let mut end = start;
        while end < groups.len() && groups[end].bar == bar {
            end += 1;
        }
        // The bar as the part plays it: places and sounds (FNV-1a).
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |v: u64| {
            for b in v.to_le_bytes() {
                h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
            }
        };
        for g in groups[start..end].iter().filter(|g| !g.borrowed()) {
            eat(g.slot as u64);
            for note in &g.notes {
                eat(identity(kind, note) as u64);
            }
        }
        let count = seen.entry(h).or_insert(0);
        let round = *count % 4;
        *count += 1;
        if round > 0 {
            // The seed picks which turn comes first; each return takes the next.
            let first = SplitMix64::new(a.seed ^ h).next_u64() % 3;
            let how = ((first + round - 1) % 3 + 1) as u8;
            let mut r = SplitMix64::new(a.seed ^ h ^ round.wrapping_mul(0x9e37_79b9));
            let mut bias = [0.0; MAX_LANES];
            for v in bias.iter_mut().take(n) {
                *v = r.next_unit() * VARY;
            }
            for (i, g) in groups.iter().enumerate().take(end).skip(start) {
                if !g.borrowed() {
                    out[i] = Some(Turn { how, bias });
                }
            }
        }
        start = end;
    }
    out
}

/// The lane each note of each group goes on, `None` for a note that plays
/// by itself: thinned (`d`), another part's note not brought in (`mix`), or
/// left over when a group has more notes than lanes. `difficulty` sets the
/// default shortest repeat on a lane. Groups are in time order.
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
    let edge = a.edge && n == 8;
    let outer = |l: usize| edge && (l == 0 || l == n - 1);

    // Each note's liked lane (a fraction of a lane allowed), by group.
    let mut liked = liked_lanes(&a, kind, n, groups, &home);
    let play = played(&a, kind, groups);
    let turns = bar_turns(&a, kind, n, groups);
    for (likes, turn) in liked.iter_mut().zip(&turns) {
        if let Some(t) = turn {
            for v in likes.iter_mut().flatten() {
                *v = t.apply(*v, (n - 1) as f64);
            }
        }
    }

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
        group_ms: never,
        side: 2,
        run: 0,
        edge_ms: never,
        cost: 0.0,
        node: 0,
    }];
    let seed = SplitMix64::new(a.seed).next_u64();
    let bucket = (jack / 2.0).max(1.0);
    let half = n / 2;
    let side_of = |mask: u16| -> u8 {
        let left = mask & ((1 << half) - 1) != 0;
        let right = mask >> half != 0;
        match (left, right) {
            (true, false) => 0,
            (false, true) => 1,
            _ => 2,
        }
    };
    // Per group, reused: each state's note-by-lane costs, the candidates.
    let mut costs: Vec<f64> = Vec::new();
    let mut next: Vec<(f64, u32, u16)> = Vec::new();
    type Sig = (u16, [u8; MAX_LANES], u8, u8, u8);
    let mut seen: std::collections::HashSet<Sig> = std::collections::HashSet::new();

    for &gi in &play {
        let g = &groups[gi];
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
        if let Some(t) = &turns[gi] {
            for (v, b) in noise.iter_mut().zip(&t.bias) {
                *v += b;
            }
        }
        let accents: Vec<bool> = ids.iter().map(|&i| edge && accent(kind, g, i)).collect();
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
            for (j, &i) in ids.iter().enumerate() {
                let note = &g.notes[i];
                let id = identity(kind, note);
                let prev = s
                    .recent
                    .iter()
                    .find(|r| r.0 == id && g.ms - r.2 <= repeat_ms)
                    .map(|r| r.1);
                let mut target = liked[gi][i];
                if kind == PartKind::Drums && a.stick && note.from.is_none() {
                    let d = note.origin.min(5) as usize;
                    if let Some(last_lane) = prev {
                        target = Some(if last_lane == home[d] {
                            alt[d]
                        } else {
                            home[d]
                        } as f64);
                    }
                }
                // Plain notes like the inner lanes.
                if edge && !accents[j] {
                    target = target.map(|t| t.clamp(1.0, (n - 2) as f64));
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
                    if outer(l) {
                        if accents[j] {
                            cost -= EDGE_ACCENT;
                        } else {
                            cost += EDGE_PLAIN;
                        }
                        if g.ms - s.edge_ms < EDGE_GAP_MS {
                            cost += EDGE_SOON;
                        }
                    }
                    costs.push(cost);
                }
            }
            let quick = g.ms - s.group_ms < FAST_MS;
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
                if a.hand && quick {
                    let side = side_of(*mask);
                    if side != 2 && side == s.side {
                        cost += HAND_RUN * (s.run + 1) as f64;
                    }
                }
                next.push((cost, si as u32, *mask));
            }
        }
        // Lowest cost first; ties by the order they were made in. Of
        // placements alike in the lanes they just hit, how long ago each
        // lane was hit and their hand run, only the cheapest is kept.
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
            let side = side_of(mask);
            let run = if a.hand && g.ms - s.group_ms < FAST_MS && side != 2 && side == s.side {
                s.run + 1
            } else {
                0
            };
            let edge_mask = edge && (mask & (1 | 1 << (n - 1))) != 0;
            let edge_ms = if edge_mask { g.ms } else { s.edge_ms };
            let edge_sig = (edge && g.ms - edge_ms < EDGE_GAP_MS) as u8;
            if !seen.insert((mask, sig, side * a.hand as u8, run.min(4) as u8, edge_sig)) {
                continue;
            }
            let mut t = s.clone();
            t.cost = cost;
            let decay = 0.5f64.powf((g.ms - s.load_ms) as f64 / LOAD_HALF_LIFE_MS);
            for v in t.load.iter_mut() {
                *v *= decay;
            }
            t.load_ms = g.ms;
            t.group_ms = g.ms;
            t.side = side;
            t.run = run;
            t.edge_ms = edge_ms;
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
    for &gi in play.iter().rev() {
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

/// Each note's liked lane (`None`: any), by group. Other parts' notes like
/// none.
fn liked_lanes(
    a: &Arrange,
    kind: PartKind,
    n: usize,
    groups: &[Group],
    home: &[u8; 6],
) -> Vec<Vec<Option<f64>>> {
    let top = (n - 1) as f64;
    let none = |g: &Group| vec![None; g.notes.len()];
    match kind {
        PartKind::Drums => groups
            .iter()
            .map(|g| {
                if g.borrowed() {
                    return none(g);
                }
                g.notes
                    .iter()
                    .map(|note| a.home.then(|| home[note.origin.min(5) as usize] as f64))
                    .collect()
            })
            .collect(),
        PartKind::Strings if !a.pitch => groups
            .iter()
            .map(|g| {
                if g.borrowed() {
                    return none(g);
                }
                g.notes
                    .iter()
                    .map(|note| Some(note.origin.min(2) as f64 * top / 2.0))
                    .collect()
            })
            .collect(),
        PartKind::Strings => {
            let mut out: Vec<Vec<Option<f64>>> = groups.iter().map(none).collect();
            let main: Vec<usize> = (0..groups.len())
                .filter(|&i| !groups[i].borrowed())
                .collect();
            let mut start = 0;
            while start < main.len() {
                let mut end = start + 1;
                while end < main.len()
                    && groups[main[end]].ms - groups[main[end - 1]].ms < PHRASE_GAP_MS
                {
                    end += 1;
                }
                let phrase = &main[start..end];
                let pitches = phrase
                    .iter()
                    .flat_map(|&i| groups[i].notes.iter().map(|n| (n.key & 0xff) as f64));
                let (lo, hi) =
                    pitches.fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p), hi.max(p)));
                for &i in phrase {
                    let g = &groups[i];
                    // A chord's notes keep their fret order around its pitch.
                    let k = g.notes.len() as f64;
                    out[i] = g
                        .notes
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
                        .collect();
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
            "5k", "4", "4k+foo", "4k+r10", "4k+j", "4k+j0", "4k+jx", "k", "4k+s-1", "4k+mix10",
            "4k+mix", "4k+d0", "4k+mixa",
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
            hand: true,
            vary: true,
            edge: true,
            jack_ms: Some(150),
            thin_ms: Some(90),
            mix: 4,
            noise: 9,
            seed: 12345,
        };
        assert_eq!(
            all.key(),
            "8k+pitch+home+stick+trill+bal+hand+vary+edge+j150+d90+mix4+r9+s12345"
        );
        assert_eq!(Arrange::parse(&all.key()), Some(all));
    }

    #[test]
    fn a_part_drops_the_other_parts_features_and_an_unused_seed() {
        let a = Arrange::parse("6k+pitch+home+stick+edge+s5").unwrap();
        assert_eq!(a.for_part(PartKind::Drums).key(), "6k+home+stick");
        assert_eq!(a.for_part(PartKind::Strings).key(), "6k+pitch");
        let a = Arrange::parse("8k+vary+edge+s5").unwrap();
        assert_eq!(a.for_part(PartKind::Drums).key(), "8k+vary+edge+s5");
    }

    #[test]
    fn splitmix_matches_the_reference() {
        // The first outputs of the reference generator from seed 0.
        let mut r = SplitMix64::new(0);
        assert_eq!(r.next_u64(), 0xe220_a839_7b1d_cdaf);
        assert_eq!(r.next_u64(), 0x6e78_9e6a_a1b9_65f4);
    }

    fn note(kind: PartKind, origin: u8) -> Note {
        Note {
            origin,
            key: match kind {
                PartKind::Drums => 36,
                PartKind::Strings => 0x2400 | (0x30 + origin as u16),
            },
            ticks: 0,
            from: None,
        }
    }

    fn group(ms: i64, notes: Vec<Note>) -> Group {
        Group {
            ms,
            bar: (ms / 2000) as u32,
            slot: ((ms % 2000) * BAR_SLOTS as i64 / 2000) as u16,
            notes,
        }
    }

    /// Groups at those ms; a bar is 2 s (120 BPM 4/4).
    fn run(kind: PartKind, origins: &[(i64, &[u8])]) -> Vec<Group> {
        origins
            .iter()
            .map(|&(ms, o)| group(ms, o.iter().map(|&origin| note(kind, origin)).collect()))
            .collect()
    }

    fn lanes_of(out: &[Vec<Option<u8>>]) -> Vec<Option<u8>> {
        out.iter().map(|l| l[0]).collect()
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

    fn longest_one_hand_run(flat: &[u8], n: u8) -> usize {
        let (mut run, mut longest) = (1, 1);
        for w in flat.windows(2) {
            run = if (w[0] < n / 2) == (w[1] < n / 2) {
                run + 1
            } else {
                1
            };
            longest = longest.max(run);
        }
        longest
    }

    #[test]
    fn hand_sends_a_fast_run_between_the_hands() {
        // A fast R-G trill: both frets like the left hand.
        let groups = run(
            PartKind::Strings,
            &(0..32)
                .map(|i| (i * 110, [&[0u8][..], &[1]][i as usize % 2]))
                .collect::<Vec<_>>(),
        );
        let flat = |key: &str| -> Vec<u8> {
            lanes_of(&arrange(
                &Arrange::parse(key).unwrap(),
                PartKind::Strings,
                3,
                &groups,
            ))
            .into_iter()
            .map(Option::unwrap)
            .collect()
        };
        let without = flat("6k");
        let with = flat("6k+hand");
        assert!(longest_one_hand_run(&with, 6) <= 3, "{with:?}");
        assert!(
            longest_one_hand_run(&with, 6) < longest_one_hand_run(&without, 6),
            "{without:?} {with:?}"
        );
    }

    #[test]
    fn thinning_keeps_strong_beats_and_drops_the_rest_to_sounds() {
        // 16ths (125 ms) with d200: every other note at most, the beats kept.
        let groups = run(
            PartKind::Drums,
            &(0..32).map(|i| (i * 125, &[0u8][..])).collect::<Vec<_>>(),
        );
        let a = Arrange::parse("6k+home+d200").unwrap();
        let lanes = lanes_of(&arrange(&a, PartKind::Drums, 3, &groups));
        let kept: Vec<i64> = groups
            .iter()
            .zip(&lanes)
            .filter(|(_, l)| l.is_some())
            .map(|(g, _)| g.ms)
            .collect();
        assert!(kept.windows(2).all(|w| w[1] - w[0] >= 200), "{kept:?}");
        for beat in (0..4000).step_by(500) {
            assert!(kept.contains(&beat), "{beat} {kept:?}");
        }
        assert_eq!(kept.len(), 16);
    }

    #[test]
    fn edge_lanes_take_only_spaced_accents() {
        // 8ths of hi-hat with a crash every bar.
        let pattern: Vec<(i64, &[u8])> = (0..64)
            .map(|i| (i * 250, if i % 8 == 0 { &[5u8][..] } else { &[0u8][..] }))
            .collect();
        let groups = run(PartKind::Drums, &pattern);
        let a = Arrange::parse("8k+home+stick+bal+edge").unwrap();
        let lanes = lanes_of(&arrange(&a, PartKind::Drums, 3, &groups));
        let mut last = i64::MIN / 2;
        let mut outer = 0;
        for (g, l) in groups.iter().zip(&lanes) {
            let l = l.unwrap();
            if l == 0 || l == 7 {
                assert!(accent(PartKind::Drums, g, 0), "{} {l}", g.ms);
                assert!(g.ms - last >= EDGE_GAP_MS, "{}", g.ms);
                last = g.ms;
                outer += 1;
            }
        }
        assert!(outer >= 4, "{lanes:?}");
    }

    #[test]
    fn vary_lays_a_returning_bar_out_differently() {
        // The same bar eight times (2 s each, 8ths over three frets).
        let pattern: Vec<(i64, &[u8])> = (0..64)
            .map(|i| (i * 250, [&[0u8][..], &[1], &[2], &[1]][i as usize % 4]))
            .collect();
        let groups = run(PartKind::Strings, &pattern);
        let bars = |key: &str| -> Vec<Vec<Option<u8>>> {
            let lanes = lanes_of(&arrange(
                &Arrange::parse(key).unwrap(),
                PartKind::Strings,
                3,
                &groups,
            ));
            lanes.chunks(8).map(<[_]>::to_vec).collect()
        };
        let distinct = |b: &[Vec<Option<u8>>]| {
            let mut d = b.to_vec();
            d.sort();
            d.dedup();
            d.len()
        };
        let plain = bars("6k+bal");
        let varied = bars("6k+bal+vary+s3");
        assert!(distinct(&varied) > distinct(&plain), "{plain:?} {varied:?}");
        assert_eq!(varied, bars("6k+bal+vary+s3"));
    }

    #[test]
    fn mix_brings_other_parts_in_only_where_the_part_rests() {
        // Guitar plays the first 2 s, then rests 2 s; drums play 8ths all along.
        let mut groups = run(
            PartKind::Strings,
            &(0..8).map(|i| (i * 250, &[1u8][..])).collect::<Vec<_>>(),
        );
        for i in 0..16 {
            let n = Note {
                from: Some(PartKind::Drums),
                ..note(PartKind::Drums, 2)
            };
            groups.push(group(i * 250, vec![n]));
        }
        groups.sort_by_key(|g| (g.ms, g.borrowed()));
        let at = |key: &str| arrange(&Arrange::parse(key).unwrap(), PartKind::Strings, 3, &groups);
        let taken = |out: &[Vec<Option<u8>>]| -> Vec<i64> {
            groups
                .iter()
                .zip(out)
                .filter(|(g, l)| g.borrowed() && l[0].is_some())
                .map(|(g, _)| g.ms)
                .collect()
        };
        assert!(taken(&at("6k")).is_empty());
        let some = taken(&at("6k+mix5"));
        assert!(!some.is_empty());
        assert!(some.iter().all(|&ms| ms >= 2000), "{some:?}");
        assert!(taken(&at("6k+mix9")).len() >= some.len());
        // Every guitar note still plays on a lane.
        let out = at("6k+mix9");
        assert!(groups
            .iter()
            .zip(&out)
            .all(|(g, l)| g.borrowed() || l[0].is_some()));
    }

    #[test]
    fn chords_keep_their_size_up_to_the_lanes() {
        let groups = run(PartKind::Drums, &[(0, &[0, 1, 2, 3, 4, 5]), (500, &[0, 2])]);
        let a = Arrange::parse("4k+home").unwrap();
        let lanes = arrange(&a, PartKind::Drums, 3, &groups);
        assert_eq!(lanes[0].iter().filter(|l| l.is_some()).count(), 4);
        assert_eq!(lanes[1].iter().filter(|l| l.is_some()).count(), 2);
        let mut used: Vec<u8> = lanes[0].iter().flatten().copied().collect();
        used.sort();
        used.dedup();
        assert_eq!(used.len(), 4);
    }

    #[test]
    fn the_seed_changes_the_lanes_only_with_noise_or_vary() {
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

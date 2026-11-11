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
//! | `auto` | the features are chosen for the chart (`auto`): by its part, lane count and how dense it is. Written alone |
//! | `pitch` | guitar and bass: low notes to the left, high to the right, within a phrase. Without it, the frets spread over the lanes |
//! | `home` | drums: each drum has its own lane (a table per lane count). Without it, no drum likes a lane |
//! | `stick` | drums: a drum hit again quickly goes between its lane and the one next to it |
//! | `trill` | a sound or drum hit again quickly goes to a lane next to the last one |
//! | `bal` | lanes and hands used unevenly cost more |
//! | `hand` | quick notes that stay on one hand cost more the longer they do: fast runs go between the hands |
//! | `vary` | a bar that comes back as it was gets its lanes laid out differently (by the seed) |
//! | `edge` | 8K: the two outer lanes take only accents (cymbals, kicks and long or chord notes on strong beats), seldom |
//! | `split` | two notes together go one to each hand, and (6K, 8K) not on lanes side by side |
//! | `jj` | a sound hit twice quickly stays on its lane (a double); a third hit moves |
//! | `fig` | a sound hit four times or more quickly is laid out as a figure a beat long (stairs, a bent stair, a zigzag, a trill), one figure a bar |
//! | `motif` | a bar that comes back is played with the same hands as the first time; the lanes may differ |
//! | `acc` | guitar and bass: two notes together where the drums hit hard, one where nothing does |
//! | `even` | guitar and bass: notes together are sized so the keys a second stay level: one note where the notes come fastest, never two quick groups of notes together in a row, and two notes on the beats of a slow passage |
//! | `trig` | 8K: the two outer lanes take the accents on the beat (snares and cymbals; long or hard-hit low notes), side to side. Instead of `edge` |
//! | `j<ms>` | the shortest time between two hits on one lane (default by difficulty) |
//! | `run<2-6>` | with `hand`: this many quick notes may stay on one hand before it costs |
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
/// The longest one-hand run `run` lets through.
pub const MAX_RUN: u8 = 6;
/// Places in a bar `Group::slot` counts.
pub const BAR_SLOTS: u16 = 960;

/// How a chart is laid out again. Two arrangements that are equal give the
/// same lanes; `key` writes each in one way only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Arrange {
    /// 4, 6 or 8.
    pub lanes: u8,
    /// The features are chosen for each chart (`auto`); the others here
    /// mean nothing.
    pub auto: bool,
    pub pitch: bool,
    pub home: bool,
    pub stick: bool,
    pub trill: bool,
    pub bal: bool,
    pub hand: bool,
    pub vary: bool,
    pub edge: bool,
    pub split: bool,
    pub jj: bool,
    pub fig: bool,
    pub motif: bool,
    pub acc: bool,
    pub trig: bool,
    pub even: bool,
    /// With `hand`: how many quick groups may stay on one hand at no cost
    /// (2..=`MAX_RUN`); 0 the plain `hand` (one).
    pub run: u8,
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
        if self.auto {
            return Arrange {
                lanes: self.lanes,
                auto: true,
                ..Default::default()
            };
        }
        match kind {
            PartKind::Drums => {
                self.pitch = false;
                self.acc = false;
                self.even = false;
            }
            PartKind::Strings => {
                self.home = false;
                self.stick = false;
            }
        }
        if self.lanes != 8 {
            self.edge = false;
            self.trig = false;
        }
        if self.trig {
            self.edge = false;
        }
        if !self.hand || self.run < 2 {
            self.run = 0;
        }
        if self.noise == 0 && !self.vary && !self.fig {
            self.seed = 0;
        }
        self
    }

    /// `<N>k[+feature...]`, the features in a fixed order (without the `@`).
    pub fn key(&self) -> String {
        let mut out = format!("{}k", self.lanes);
        if self.auto {
            return out + "+auto";
        }
        for (on, name) in [
            (self.pitch, "pitch"),
            (self.home, "home"),
            (self.stick, "stick"),
            (self.trill, "trill"),
            (self.bal, "bal"),
            (self.hand, "hand"),
            (self.vary, "vary"),
            (self.edge, "edge"),
            (self.split, "split"),
            (self.jj, "jj"),
            (self.fig, "fig"),
            (self.motif, "motif"),
            (self.acc, "acc"),
            (self.trig, "trig"),
            (self.even, "even"),
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
        if self.run >= 2 {
            out += &format!("+run{}", self.run);
        }
        if self.mix > 0 {
            out += &format!("+mix{}", self.mix);
        }
        if self.noise > 0 {
            out += &format!("+r{}", self.noise);
        }
        if self.seed != 0 && (self.noise > 0 || self.vary || self.fig) {
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
                "auto" => a.auto = true,
                "pitch" => a.pitch = true,
                "home" => a.home = true,
                "stick" => a.stick = true,
                "trill" => a.trill = true,
                "bal" => a.bal = true,
                "hand" => a.hand = true,
                "vary" => a.vary = true,
                "edge" => a.edge = true,
                "split" => a.split = true,
                "jj" => a.jj = true,
                "fig" => a.fig = true,
                "motif" => a.motif = true,
                "acc" => a.acc = true,
                "trig" => a.trig = true,
                "even" => a.even = true,
                _ => {
                    if let Some(m) = p.strip_prefix("run") {
                        a.run = number(m).filter(|m| (2..=MAX_RUN as u64).contains(m))? as u8;
                        continue;
                    }
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

/// A named set of features.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Preset {
    /// Like a hand-made keyboard chart: a few quick notes may stay on one
    /// hand, two notes together go to both hands, doubles stay on a lane,
    /// quick repeats make figures, returning bars keep their hands, and
    /// guitar chords follow the drums' accents and keep the load level.
    Flow,
    /// Quick notes go strictly between the hands, no lane is hit twice
    /// quickly.
    Alternate,
    /// `Flow` with notes closer than 150 ms thinned to sounds.
    Easy,
    /// `Flow` with the other parts' notes brought in where the part rests.
    Session,
}

impl Preset {
    pub const ALL: [Preset; 4] = [
        Preset::Flow,
        Preset::Alternate,
        Preset::Easy,
        Preset::Session,
    ];

    /// The name in `config.dat`.
    pub fn name(self) -> &'static str {
        match self {
            Preset::Flow => "flow",
            Preset::Alternate => "alt",
            Preset::Easy => "easy",
            Preset::Session => "session",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }

    /// The preset's features at `lanes` (for both kinds of part; a chart
    /// keeps its own, `Arrange::for_part`).
    pub fn arrange(self, lanes: u8) -> Arrange {
        let base = Arrange {
            lanes,
            pitch: true,
            home: true,
            stick: true,
            bal: true,
            hand: true,
            vary: true,
            edge: true,
            ..Default::default()
        };
        // One-hand runs: three on four lanes, two on more (measured against
        // hand-made charts, `docs/plans/2026-11-11-gfdm-arrange-djmax-plan.md`).
        let flow = Arrange {
            split: true,
            jj: true,
            fig: true,
            motif: true,
            acc: true,
            even: true,
            run: if lanes <= 4 { 3 } else { 2 },
            ..base
        };
        match self {
            Preset::Flow => flow,
            Preset::Alternate => base,
            Preset::Easy => Arrange {
                thin_ms: Some(150),
                ..flow
            },
            Preset::Session => Arrange { mix: 5, ..flow },
        }
    }
}

/// Drum charts with more notes a second than this keep no doubles (`auto`):
/// with them, such charts have more doubles than hand-made ones.
const AUTO_DENSE_DRUMS: f64 = 8.0;

/// The features `auto` chooses for a chart of `kind` with these `groups`:
/// `Preset::Flow`, without doubles for a dense drum chart.
pub fn auto(lanes: u8, kind: PartKind, groups: &[Group]) -> Arrange {
    let mut a = Preset::Flow.arrange(lanes);
    if kind == PartKind::Drums && density(groups) >= AUTO_DENSE_DRUMS {
        a.jj = false;
    }
    a
}

/// The part's own notes a second.
fn density(groups: &[Group]) -> f64 {
    let own = || groups.iter().filter(|g| !g.borrowed());
    let notes: usize = own().map(|g| g.notes.len()).sum();
    let span = match (own().next(), own().last()) {
        (Some(first), Some(last)) => (last.ms - first.ms) as f64 / 1000.0,
        _ => 0.0,
    };
    if span > 0.0 {
        notes as f64 / span
    } else {
        0.0
    }
}

/// The features `a` stands for on this chart: those `auto` chooses, or its
/// own; only the ones that mean something for `kind`.
pub fn resolve(a: &Arrange, kind: PartKind, groups: &[Group]) -> Arrange {
    if a.auto {
        auto(a.lanes, kind, groups)
    } else {
        *a
    }
    .for_part(kind)
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
    /// How hard the drums hit here, for a guitar or bass group: 2 a snare
    /// or cymbal, 1 a kick, 0 nothing (or a drum chart's own group).
    pub accent: u8,
    pub notes: Vec<Note>,
}

impl Group {
    fn borrowed(&self) -> bool {
        self.notes.first().is_some_and(|n| n.from.is_some())
    }

    /// How strong the group's beat is: 4 the bar's start, 3 its half, 2 a
    /// quarter, 1 an eighth, 0 anything else.
    pub(crate) fn strength(&self) -> u8 {
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
/// `split`: two notes together on one hand, and (6 lanes or more) on lanes
/// side by side.
const SPLIT_HAND: f64 = 120.0;
const SPLIT_NEXT: f64 = 80.0;
/// `jj`: a double may be this much of the shortest repeat apart.
const DOUBLE: f64 = 0.75;
/// `fig`: a lane away from a figure's lane; the figures (lanes from the
/// figure's left edge, a beat of four) and how often each is picked of 11.
const FIG: f64 = 150.0;
const FIGURES: [([u8; 4], u64); 6] = [
    ([0, 1, 2, 3], 2),
    ([3, 2, 1, 0], 2),
    ([0, 1, 2, 1], 2),
    ([3, 2, 1, 2], 2),
    ([0, 2, 1, 3], 2),
    ([0, 1, 0, 1], 1),
];
/// `motif`: a group of a returning bar on other hands than the first time.
const MOTIF: f64 = 90.0;
/// `motif`: under this many notes a second a returning bar keeps its lanes
/// too, three times of four.
const MOTIF_CALM: f64 = 5.0;
/// `trig`: the pull an accent has to an outer lane, and an outer hit on
/// the side of the last one within `EDGE_GAP_MS`.
const TRIG_ACCENT: f64 = 250.0;
const TRIG_SAME: f64 = 80.0;
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
    /// Recent notes by identity: (identity, lane, ms, on the lane its sound
    /// was last on), newest first.
    recent: [(u16, u8, i64, bool); RECENT],
    /// The last group's time and hand (0 left, 1 right, 2 both), and how
    /// many quick groups in a row stayed on that hand (`hand`).
    group_ms: i64,
    side: u8,
    run: u16,
    /// The last outer-lane hit (`edge`, `trig`) and its side (0 left, 1
    /// right, 2 both).
    edge_ms: i64,
    edge_side: u8,
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
            4 => (t - 1.0).max(0.0),
            _ => (top - t - 1.0).max(0.0),
        }
    }
}

/// A bar as the part plays it: places and sounds (FNV-1a).
fn bar_hash(kind: PartKind, bar: &[Group]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |v: u64| {
        for b in v.to_le_bytes() {
            h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
        }
    };
    for g in bar.iter().filter(|g| !g.borrowed()) {
        eat(g.slot as u64);
        for note in &g.notes {
            eat(identity(kind, note) as u64);
        }
    }
    h
}

/// Each group's bar's `bar_hash`.
fn bar_hashes(kind: PartKind, groups: &[Group]) -> Vec<u64> {
    let mut out = vec![0; groups.len()];
    let mut start = 0;
    while start < groups.len() {
        let bar = groups[start].bar;
        let mut end = start;
        while end < groups.len() && groups[end].bar == bar {
            end += 1;
        }
        let h = bar_hash(kind, &groups[start..end]);
        out[start..end].fill(h);
        start = end;
    }
    out
}

/// `fig`: lays each quick run of one sound (four groups or more, each one
/// note or two of that sound) out as a figure, by changing the liked lane
/// of each group's first note; says which groups are in
/// one. The figure is picked by the bar and the seed, so a bar that comes
/// back gets the same, and it turns with the beat: its first lane falls on
/// the run's first quarter beat.
fn figures(
    a: &Arrange,
    kind: PartKind,
    n: usize,
    groups: &[Group],
    liked: &mut [Vec<Option<f64>>],
) -> Vec<bool> {
    let mut out = vec![false; groups.len()];
    if !a.fig || n < 4 {
        return out;
    }
    let hashes = bar_hashes(kind, groups);
    let own: Vec<usize> = (0..groups.len())
        .filter(|&i| !groups[i].borrowed())
        .collect();
    let id_of = |i: usize| identity(kind, &groups[own[i]].notes[0]);
    // One sound: a single note, or two of one sound (`acc`'s second note).
    let single = |i: usize| {
        let notes = &groups[own[i]].notes;
        notes.len() <= 2 && notes.iter().all(|n| identity(kind, n) == id_of(i))
    };
    let mut s = 0;
    while s < own.len() {
        if !single(s) {
            s += 1;
            continue;
        }
        let id = id_of(s);
        let mut e = s + 1;
        while e < own.len()
            && single(e)
            && id_of(e) == id
            && groups[own[e]].ms - groups[own[e - 1]].ms < FAST_MS
        {
            e += 1;
        }
        if e - s >= 4 {
            let beat = (s..e)
                .position(|i| groups[own[i]].strength() >= 2)
                .unwrap_or(0);
            for j in 0..e - s {
                let gi = own[s + j];
                let mut pick =
                    SplitMix64::new(a.seed ^ hashes[gi] ^ id as u64 ^ 0x66_6967).next_u64() % 11;
                let mut cell = FIGURES[0].0;
                for (lanes, weight) in FIGURES {
                    if pick < weight {
                        cell = lanes;
                        break;
                    }
                    pick -= weight;
                }
                let at = liked[gi][0].unwrap_or((n - 1) as f64 / 2.0);
                let left = (at - 1.5).round().clamp(0.0, (n - 4) as f64);
                let step = (j + 4 - beat % 4) % 4;
                liked[gi][0] = Some(left + cell[step] as f64);
                out[gi] = true;
            }
        }
        s = e;
    }
    out
}

/// `motif`: the hands (0 left, 1 right, 2 both) each group of a returning
/// bar should be on: those of the group at its place the first time the bar
/// came, in the placement `masks`. `None` for the rest.
fn motif_hands(kind: PartKind, n: usize, groups: &[Group], masks: &[u16]) -> Vec<Option<u8>> {
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
    let hashes = bar_hashes(kind, groups);
    // By bar hash: the bar it first came in, and its hands by slot.
    let mut first: std::collections::HashMap<u64, (u32, Vec<(u16, u8)>)> =
        std::collections::HashMap::new();
    let mut out = vec![None; groups.len()];
    for (gi, g) in groups.iter().enumerate() {
        if g.borrowed() || masks[gi] == 0 {
            continue;
        }
        let entry = first.entry(hashes[gi]).or_insert((g.bar, Vec::new()));
        if entry.0 == g.bar {
            entry.1.push((g.slot, side_of(masks[gi])));
        } else {
            out[gi] = entry
                .1
                .iter()
                .find(|(slot, _)| *slot == g.slot)
                .map(|p| p.1);
        }
    }
    out
}

/// What `acc` and `even` do with a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accent {
    Keep,
    /// Only the group's first notes stay, this many.
    Trim(usize),
    /// The group gets this second note (it has no sound of its own).
    Double(Note),
}

/// `even`: the keys a second are counted over this long before and after a
/// group.
const EVEN_WINDOW_MS: i64 = 500;
/// `even`: the most keys a second, as a multiple of the groups a second
/// where the chart is fast (nine tenths of its groups are slower).
const EVEN_CAP: f64 = 1.3;
/// `even`: a slow passage gets second notes up to this much of the most.
const EVEN_FILL: f64 = 0.7;

/// `acc`, `even`: how many notes each guitar and bass group plays.
///
/// `acc` sizes a group by how hard the song hits there. A single note on a
/// quarter beat where the drums play a snare or cymbal (`Group::accent`),
/// or where a bar starts, becomes two; notes together off the quarter beats
/// with no drum under them become one.
///
/// `even` then keeps the load level (a note more in a group is another key
/// at once, and has no sound of its own). Every group starts as one note,
/// and the groups take their notes back, strong beats and drum accents
/// first, while the keys a second around them stay under the most the chart
/// may have, and no two quick groups in a row have more than one (a quick
/// group no more than two). Where that leaves a passage slow, single notes
/// on the beats become two.
///
/// Every group `Keep`s without both.
pub fn accents(a: &Arrange, kind: PartKind, groups: &[Group]) -> Vec<Accent> {
    let mut out = vec![Accent::Keep; groups.len()];
    if kind != PartKind::Strings || !(a.acc || a.even) {
        return out;
    }
    let own: Vec<usize> = (0..groups.len())
        .filter(|&i| !groups[i].borrowed() && !groups[i].notes.is_empty())
        .collect();
    // The notes each group wants.
    let mut want: Vec<usize> = own
        .iter()
        .map(|&i| {
            let g = &groups[i];
            let strength = g.strength();
            match g.notes.len() {
                k if !a.acc => k,
                1 if strength == 4 || (g.accent >= 2 && strength >= 2) => 2,
                k if k >= 2 && g.accent == 0 && strength < 2 => 1,
                k => k,
            }
        })
        .collect();
    if a.even {
        want = even_sizes(groups, &own, &want);
    }
    for (&i, &size) in own.iter().zip(&want) {
        let notes = &groups[i].notes;
        out[i] = match size.cmp(&notes.len()) {
            std::cmp::Ordering::Equal => Accent::Keep,
            std::cmp::Ordering::Less => Accent::Trim(size),
            std::cmp::Ordering::Greater => {
                let n = notes[0];
                Accent::Double(Note {
                    origin: (n.origin + 1) % 3,
                    ticks: 0,
                    ..n
                })
            }
        };
    }
    out
}

/// `even`: the notes each of the part's groups (`own`, in time order) plays,
/// from the notes each wants.
fn even_sizes(groups: &[Group], own: &[usize], want: &[usize]) -> Vec<usize> {
    let ms = |k: usize| groups[own[k]].ms;
    // The groups within the window of each, as a range of `own`.
    let mut windows: Vec<(usize, usize)> = Vec::with_capacity(own.len());
    let (mut lo, mut hi) = (0, 0);
    for k in 0..own.len() {
        while ms(lo) < ms(k) - EVEN_WINDOW_MS {
            lo += 1;
        }
        while hi < own.len() && ms(hi) <= ms(k) + EVEN_WINDOW_MS {
            hi += 1;
        }
        windows.push((lo, hi));
    }
    let mut hits: Vec<usize> = windows.iter().map(|w| w.1 - w.0).collect();
    hits.sort_unstable();
    let fast = hits.get(hits.len() * 9 / 10).copied().unwrap_or(0);
    let most = fast as f64 * EVEN_CAP;
    // Strong beats, drum accents and long notes first; then as they come.
    let prio = |k: usize| {
        let g = &groups[own[k]];
        let long = g.notes.iter().any(|n| n.ticks >= LONG_TICKS);
        2 * g.strength() as i32 + 2 * g.accent as i32 + long as i32
    };
    let mut order: Vec<usize> = (0..own.len()).collect();
    order.sort_by_key(|&k| (-prio(k), k));
    let quick = |x: usize, y: usize| ms(y) - ms(x) < FAST_MS;
    // Whether group `k` may play a note more: with the keys around it no
    // more than `limit`, and no window it is in over the most.
    let fits = |size: &[usize], k: usize, limit: f64| {
        let before = k > 0 && quick(k - 1, k);
        let after = k + 1 < own.len() && quick(k, k + 1);
        if (before && size[k - 1] > 1) || (after && size[k + 1] > 1) {
            return false;
        }
        if (before || after) && size[k] >= 2 {
            return false;
        }
        let keys = |w: (usize, usize)| size[w.0..w.1].iter().sum::<usize>() as f64;
        let (lo, hi) = windows[k];
        keys(windows[k]) + 1.0 <= limit && (lo..hi).all(|j| keys(windows[j]) + 1.0 <= most)
    };
    let mut size = vec![1usize; own.len()];
    for &k in &order {
        while size[k] < want[k] && fits(&size, k, most) {
            size[k] += 1;
        }
    }
    for &k in &order {
        if size[k] == 1 && groups[own[k]].strength() >= 1 && fits(&size, k, most * EVEN_FILL) {
            size[k] = 2;
        }
    }
    size
}

/// `trig`: whether note `i` of `g` is an accent on the beat.
fn trig_accent(kind: PartKind, g: &Group, i: usize) -> bool {
    let n = &g.notes[i];
    let strength = g.strength();
    match n.from.unwrap_or(kind) {
        PartKind::Drums => (n.origin == 1 || n.origin == 5) && strength >= 1,
        PartKind::Strings => {
            let lowest = g.notes.iter().all(|m| (m.key & 0xff) >= (n.key & 0xff));
            let first = g.notes.iter().position(|m| m.key == n.key) == Some(i);
            lowest
                && first
                && (n.ticks >= LONG_TICKS || strength == 4 || (strength >= 2 && g.accent >= 2))
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
    let calm = a.motif && density(groups) < MOTIF_CALM;
    let mut seen: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
    let mut start = 0;
    while start < groups.len() {
        let bar = groups[start].bar;
        let mut end = start;
        while end < groups.len() && groups[end].bar == bar {
            end += 1;
        }
        let h = bar_hash(kind, &groups[start..end]);
        let count = seen.entry(h).or_insert(0);
        let round = *count % 4;
        *count += 1;
        // `motif`: a calm chart keeps a returning bar as it was but once.
        if round > 0 && !(calm && round != 3) {
            // The seed picks which turn comes first; each return takes the next.
            let first = SplitMix64::new(a.seed ^ h).next_u64() % 3;
            let how = if a.motif {
                // Moved over a lane, never mirrored: the hands stay.
                [2, 4][((first + round) % 2) as usize]
            } else {
                ((first + round - 1) % 3 + 1) as u8
            };
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
    let a = resolve(a, kind, groups);
    let (masks, lanes) = search(&a, kind, difficulty, groups, &[]);
    if !a.motif {
        return lanes;
    }
    // Again, now that the hands of each bar's first time are known.
    let n = a.lanes.clamp(1, MAX_LANES as u8) as usize;
    let want = motif_hands(kind, n, groups, &masks);
    search(&a, kind, difficulty, groups, &want).1
}

/// The placement with the lowest cost: each group's lane mask, and each
/// note's lane. `want` is the hands each group should be on (`motif`), empty
/// for none.
fn search(
    a: &Arrange,
    kind: PartKind,
    difficulty: u8,
    groups: &[Group],
    want: &[Option<u8>],
) -> (Vec<u16>, Vec<Vec<Option<u8>>>) {
    let a = *a;
    let n = a.lanes.clamp(1, MAX_LANES as u8) as usize;
    let jack = a.jack_ms.unwrap_or_else(|| default_jack_ms(difficulty)) as f64;
    let repeat_ms = (2.0 * jack).max(250.0) as i64;
    let (home, alt) = drum_lanes(a.lanes);
    let trig = a.trig && n == 8;
    let edge = a.edge && n == 8 && !trig;
    let outer = |l: usize| (edge || trig) && (l == 0 || l == n - 1);

    // Each note's liked lane (a fraction of a lane allowed), by group.
    let mut liked = liked_lanes(&a, kind, n, groups, &home);
    let figs = figures(&a, kind, n, groups, &mut liked);
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
        recent: [(u16::MAX, 0, never, false); RECENT],
        group_ms: never,
        side: 2,
        run: 0,
        edge_ms: never,
        edge_side: 2,
        cost: 0.0,
        node: 0,
    }];
    let seed = SplitMix64::new(a.seed).next_u64();
    let bucket = (jack / 2.0).max(1.0);
    let half = n / 2;
    // Quick groups that may follow the first on one hand at no cost.
    let free_run = a.run.max(1) as u16 - 1;
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
        let accents: Vec<bool> = ids
            .iter()
            .map(|&i| (edge && accent(kind, g, i)) || (trig && trig_accent(kind, g, i)))
            .collect();
        let fig = figs[gi];
        let prefer = if fig { FIG } else { PREFER };
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
                    .map(|r| (r.1, r.2, r.3));
                // `jj`: the second hit of a sound may stay where the first
                // was (if nothing else hit that lane since); a third may not.
                let own = note.from.is_none();
                let double = match prev {
                    Some((lane, ms, false))
                        if a.jj && own && !fig && s.last[lane as usize] == ms =>
                    {
                        Some(lane as usize)
                    }
                    _ => None,
                };
                let tripled = match prev {
                    Some((lane, _, true)) if a.jj => Some(lane as usize),
                    _ => None,
                };
                let prev = prev.map(|p| p.0);
                let mut target = liked[gi][i];
                if kind == PartKind::Drums && a.stick && note.from.is_none() && !fig {
                    let d = note.origin.min(5) as usize;
                    if let Some(lane) = double {
                        target = Some(lane as f64);
                    } else if let Some(last_lane) = prev {
                        target = Some(if last_lane == home[d] {
                            alt[d]
                        } else {
                            home[d]
                        } as f64);
                    }
                }
                // Plain notes like the inner lanes.
                if (edge || trig) && !accents[j] {
                    target = target.map(|t| t.clamp(1.0, (n - 2) as f64));
                }
                // `trig`: an accent likes no lane but the outer ones.
                if trig && accents[j] {
                    target = None;
                }
                for l in 0..n {
                    let mut cost = noise[l];
                    let gap = (g.ms - s.last[l]) as f64;
                    let shortest = if double == Some(l) {
                        jack * DOUBLE
                    } else {
                        jack
                    };
                    if gap < shortest {
                        cost += JACK_BASE + JACK * (jack - gap) / jack;
                    } else if tripled == Some(l) && gap < repeat_ms as f64 {
                        cost += JACK_BASE;
                    }
                    if let Some(t) = target {
                        cost += prefer * (l as f64 - t).abs();
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
                            cost -= if trig { TRIG_ACCENT } else { EDGE_ACCENT };
                        } else {
                            cost += EDGE_PLAIN;
                        }
                        let soon = g.ms - s.edge_ms < EDGE_GAP_MS;
                        if edge && soon {
                            cost += EDGE_SOON;
                        }
                        if trig && soon && s.edge_side == (l != 0) as u8 {
                            cost += TRIG_SAME;
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
                if a.split && k == 2 {
                    let (x, y) = (lanes[0] as usize, lanes[1] as usize);
                    if (x < half) == (y < half) {
                        cost += SPLIT_HAND;
                    }
                    if n >= 6 && y == x + 1 {
                        cost += SPLIT_NEXT;
                    }
                }
                if let Some(Some(hands)) = want.get(gi) {
                    if side_of(*mask) != *hands {
                        cost += MOTIF;
                    }
                }
                if a.hand && quick {
                    let side = side_of(*mask);
                    // `jj`: hitting the last group's lanes again is no run.
                    let again = a.jj && lanes.iter().all(|&l| s.last[l as usize] == s.group_ms);
                    if side != 2 && side == s.side && !again {
                        let over = (s.run + 1).saturating_sub(free_run);
                        cost += HAND_RUN * over as f64;
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
            let outer_hit = mask & (1 | 1 << (n - 1));
            let edge_mask = (edge || trig) && outer_hit != 0;
            let edge_ms = if edge_mask { g.ms } else { s.edge_ms };
            let edge_side = match outer_hit {
                _ if !edge_mask => s.edge_side,
                1 => 0,
                m if m == 1 << (n - 1) => 1,
                _ => 2,
            };
            let recent_edge = (edge || trig) && g.ms - edge_ms < EDGE_GAP_MS;
            let edge_sig = if trig {
                recent_edge as u8 * (1 + edge_side)
            } else {
                recent_edge as u8
            };
            let run_sig = run.min(4 + free_run) as u8;
            if !seen.insert((mask, sig, side * a.hand as u8, run_sig, edge_sig)) {
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
            t.edge_side = edge_side;
            let lanes = (0..n as u8).filter(|l| mask & 1 << l != 0);
            for (&i, lane) in ids.iter().zip(lanes) {
                t.last[lane as usize] = g.ms;
                t.load[lane as usize] += 1.0;
                let id = identity(kind, &g.notes[i]);
                let stayed = s
                    .recent
                    .iter()
                    .find(|r| r.0 == id && g.ms - r.2 <= repeat_ms)
                    .is_some_and(|r| r.1 == lane);
                t.recent.copy_within(0..RECENT - 1, 1);
                t.recent[0] = (id, lane, g.ms, stayed);
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
    let lanes = groups
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
        .collect();
    (masks, lanes)
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
            "4k+mix", "4k+d0", "4k+mixa", "4k+run", "4k+run1", "4k+run7",
        ] {
            assert_eq!(Arrange::parse(bad), None, "{bad}");
        }
        let all = Arrange {
            lanes: 8,
            auto: false,
            pitch: true,
            home: true,
            stick: true,
            trill: true,
            bal: true,
            hand: true,
            vary: true,
            edge: true,
            split: true,
            jj: true,
            fig: true,
            motif: true,
            acc: true,
            trig: false,
            even: true,
            run: 3,
            jack_ms: Some(150),
            thin_ms: Some(90),
            mix: 4,
            noise: 9,
            seed: 12345,
        };
        assert_eq!(
            all.key(),
            "8k+pitch+home+stick+trill+bal+hand+vary+edge+split+jj+fig+motif+acc+even+j150+d90+run3+mix4+r9+s12345"
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
            accent: 0,
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
    fn auto_is_written_alone_and_chooses_by_the_chart() {
        let a = Arrange::parse("6k+auto+pitch+j150").unwrap();
        assert_eq!(a.for_part(PartKind::Strings).key(), "6k+auto");
        assert_eq!(Arrange::parse("6k+auto").unwrap().key(), "6k+auto");
        // A sparse drum chart keeps doubles; a dense one does not.
        let sparse = run(
            PartKind::Drums,
            &(0..40).map(|i| (i * 500, &[1u8][..])).collect::<Vec<_>>(),
        );
        let dense = run(
            PartKind::Drums,
            &(0..400).map(|i| (i * 100, &[1u8][..])).collect::<Vec<_>>(),
        );
        assert!(auto(6, PartKind::Drums, &sparse).jj);
        assert!(!auto(6, PartKind::Drums, &dense).jj);
        assert!(auto(6, PartKind::Strings, &dense).jj);
        assert_eq!(auto(4, PartKind::Strings, &sparse).run, 3);
        assert_eq!(auto(8, PartKind::Strings, &sparse).run, 2);
        // The lanes are those of the features it chose.
        let chosen = auto(6, PartKind::Drums, &sparse);
        assert_eq!(
            arrange(
                &Arrange::parse("6k+auto").unwrap(),
                PartKind::Drums,
                2,
                &sparse
            ),
            arrange(&chosen, PartKind::Drums, 2, &sparse)
        );
    }

    #[test]
    fn presets_have_names_and_differ() {
        for p in Preset::ALL {
            assert_eq!(Preset::parse(p.name()), Some(p));
        }
        assert_eq!(Preset::parse("auto"), None);
        assert_eq!(
            Preset::Alternate.arrange(6).key(),
            "6k+pitch+home+stick+bal+hand+vary+edge"
        );
        assert_eq!(
            Preset::Flow.arrange(4).for_part(PartKind::Strings).key(),
            "4k+pitch+bal+hand+vary+split+jj+fig+motif+acc+even+run3"
        );
        assert_eq!(Preset::Easy.arrange(6).thin_ms, Some(150));
        assert_eq!(Preset::Session.arrange(6).mix, 5);
    }

    #[test]
    fn fig_lays_a_quick_run_out_as_a_figure_a_beat_long() {
        // Two bars of hi-hat sixteenths at 120 BPM (125 ms).
        let groups = run(
            PartKind::Drums,
            &(0..32).map(|i| (i * 125, &[0u8][..])).collect::<Vec<_>>(),
        );
        let flat = |key: &str| -> Vec<u8> {
            lanes_of(&arrange(
                &Arrange::parse(key).unwrap(),
                PartKind::Drums,
                3,
                &groups,
            ))
            .into_iter()
            .map(Option::unwrap)
            .collect()
        };
        let without = flat("6k+home");
        assert!(without.iter().all(|&l| l == without[0]), "{without:?}");
        let with = flat("6k+home+fig");
        for bar in with.chunks(16) {
            // The same four lanes each beat, and they are one of the figures.
            for j in 0..12 {
                assert_eq!(bar[j], bar[j + 4], "{with:?}");
            }
            let left = *bar[..4].iter().min().unwrap();
            let cell: Vec<u8> = bar[..4].iter().map(|l| l - left).collect();
            let known = FIGURES.iter().any(|(lanes, _)| {
                let low = *lanes.iter().min().unwrap();
                lanes.iter().map(|l| l - low).collect::<Vec<_>>() == cell
            });
            assert!(known, "{with:?}");
        }
        // The seed picks the figures.
        assert_eq!(with, flat("6k+home+fig"));
        let seeds: Vec<Vec<u8>> = (1..6).map(|s| flat(&format!("6k+home+fig+s{s}"))).collect();
        assert!(seeds.iter().any(|s| *s != with), "{with:?}");
    }

    #[test]
    fn motif_gives_a_returning_bar_the_hands_of_its_first_time() {
        // Bars X, Y, X: frets R (left) and B (right).
        let x: [(i64, &[u8]); 4] = [(0, &[0]), (500, &[2]), (1000, &[0]), (1500, &[2])];
        let y: [(i64, &[u8]); 4] = [(0, &[2]), (500, &[2]), (1000, &[0]), (1500, &[0])];
        let mut all = Vec::new();
        for (bar, pattern) in [x, y, x].iter().enumerate() {
            for &(ms, frets) in pattern {
                all.push((bar as i64 * 2000 + ms, frets));
            }
        }
        let groups = run(PartKind::Strings, &all);
        // The hands asked of the third bar are those the first was given.
        let masks: Vec<u16> = (0..12)
            .map(|i| if i % 2 == 0 { 0b000001 } else { 0b100000 })
            .collect();
        let want = motif_hands(PartKind::Strings, 6, &groups, &masks);
        assert_eq!(&want[..8], &[None; 8]);
        assert_eq!(&want[8..], &[Some(0), Some(1), Some(0), Some(1)]);
        // A calm chart: the bar comes back on the same lanes, even with `vary`.
        let a = Arrange::parse("6k+bal+hand+vary+motif+s3").unwrap();
        let lanes = lanes_of(&arrange(&a, PartKind::Strings, 2, &groups));
        assert_eq!(lanes[8..], lanes[..4], "{lanes:?}");
    }

    #[test]
    fn acc_sizes_chords_by_how_hard_the_song_hits() {
        let mut groups = run(
            PartKind::Strings,
            &[
                (0, &[0][..]),   // a bar's start
                (125, &[0, 1]),  // off the beat, no drum
                (500, &[0, 1]),  // a quarter beat
                (625, &[1]),     // off the beat, no drum
                (1125, &[0, 1]), // off the beat, a kick under it
                (1500, &[2]),    // a quarter beat, a snare under it
                (1625, &[1]),    // off the beat, a snare under it
            ],
        );
        groups[4].accent = 1;
        groups[5].accent = 2;
        groups[6].accent = 2;
        let a = Arrange::parse("6k+acc").unwrap();
        let got = accents(&a, PartKind::Strings, &groups);
        assert!(matches!(got[0], Accent::Double(_)));
        assert_eq!(got[1], Accent::Trim(1));
        assert_eq!(got[2], Accent::Keep);
        assert_eq!(got[3], Accent::Keep);
        assert_eq!(got[4], Accent::Keep);
        assert!(
            matches!(got[5], Accent::Double(n) if n.origin != 2 && n.key == groups[5].notes[0].key)
        );
        assert_eq!(got[6], Accent::Keep);
        // Not for drums, and not without the feature.
        assert!(
            accents(&a.for_part(PartKind::Drums), PartKind::Drums, &groups)
                .iter()
                .all(|x| *x == Accent::Keep)
        );
        let plain = Arrange::parse("6k").unwrap();
        assert!(accents(&plain, PartKind::Strings, &groups)
            .iter()
            .all(|x| *x == Accent::Keep));
    }

    #[test]
    fn even_levels_quick_chords_and_fills_slow_beats() {
        // Two bars of eighths (250 ms) of single notes, then two bars of
        // sixteenths (125 ms) of R+G chords.
        let mut all: Vec<(i64, &[u8])> = (0..16).map(|i| (i * 250, &[0u8][..])).collect();
        all.extend((0..32).map(|i| (4000 + i * 125, &[0u8, 1][..])));
        let groups = run(PartKind::Strings, &all);
        let sizes = |key: &str| -> Vec<usize> {
            let a = Arrange::parse(key).unwrap();
            accents(&a, PartKind::Strings, &groups)
                .iter()
                .zip(&groups)
                .map(|(x, g)| match x {
                    Accent::Keep => g.notes.len(),
                    Accent::Trim(n) => *n,
                    Accent::Double(_) => g.notes.len() + 1,
                })
                .collect()
        };
        let plain = sizes("6k");
        assert!(plain[..16].iter().all(|&n| n == 1) && plain[16..].iter().all(|&n| n == 2));
        let even = sizes("6k+even");
        // No two quick groups in a row play two notes, and the quick part
        // has fewer keys than it had.
        for w in even[16..].windows(2) {
            assert!(w[0] == 1 || w[1] == 1, "{even:?}");
        }
        let keys = |v: &[usize]| v.iter().sum::<usize>();
        assert!(keys(&even[16..]) < keys(&plain[16..]), "{even:?}");
        // The slow part gets second notes on its beats.
        assert!(keys(&even[..16]) > 16, "{even:?}");
        assert!(even[..16].iter().step_by(2).any(|&n| n == 2), "{even:?}");
        // The keys in any second stay close: the hardest second is no more
        // than twice the easiest (it was over three times).
        let load = |v: &[usize]| -> (usize, usize) {
            let per: Vec<usize> = (0..8)
                .map(|sec| {
                    groups
                        .iter()
                        .zip(v)
                        .filter(|(g, _)| g.ms / 1000 == sec)
                        .map(|(_, n)| *n)
                        .sum()
                })
                .collect();
            (*per.iter().min().unwrap(), *per.iter().max().unwrap())
        };
        let (lo, hi) = load(&plain);
        assert!(hi > 3 * lo, "{plain:?}");
        let (lo, hi) = load(&even);
        assert!(hi <= 2 * lo, "{even:?}");
        // Not for drums.
        assert_eq!(
            Arrange::parse("6k+even")
                .unwrap()
                .for_part(PartKind::Drums)
                .key(),
            "6k"
        );
    }

    #[test]
    fn trig_puts_the_snares_on_the_beat_on_the_outer_lanes() {
        // Hi-hat eighths with a snare on beats 2 and 4, two bars.
        let mut all: Vec<(i64, &[u8])> = Vec::new();
        for i in 0..16 {
            let ms = i * 250;
            let backbeat = ms % 1000 == 500;
            all.push((ms, if backbeat { &[0, 1][..] } else { &[0][..] }));
        }
        let groups = run(PartKind::Drums, &all);
        let a = Arrange::parse("8k+home+trig").unwrap();
        let lanes = arrange(&a, PartKind::Drums, 2, &groups);
        let mut snares = 0;
        for (g, l) in groups.iter().zip(&lanes) {
            let hat = l[0].unwrap();
            assert!(hat != 0 && hat != 7, "{lanes:?}");
            if g.notes.len() == 2 {
                let snare = l[1].unwrap();
                assert!(snare == 0 || snare == 7, "{lanes:?}");
                snares += 1;
            }
        }
        assert_eq!(snares, 4);
        // `trig` takes the place of `edge`, and is for 8K only.
        assert_eq!(
            Arrange::parse("8k+edge+trig")
                .unwrap()
                .for_part(PartKind::Drums)
                .key(),
            "8k+trig"
        );
        assert_eq!(
            Arrange::parse("6k+trig")
                .unwrap()
                .for_part(PartKind::Drums)
                .key(),
            "6k"
        );
    }

    #[test]
    fn run_lets_a_few_quick_notes_stay_on_one_hand() {
        // The same fast R-G trill: `run3` keeps three on the hand they like.
        let groups = run(
            PartKind::Strings,
            &(0..32)
                .map(|i| (i * 110, [&[0u8][..], &[1]][i as usize % 2]))
                .collect::<Vec<_>>(),
        );
        let a = Arrange::parse("6k+hand+run3").unwrap();
        let flat: Vec<u8> = lanes_of(&arrange(&a, PartKind::Strings, 3, &groups))
            .into_iter()
            .map(Option::unwrap)
            .collect();
        let longest = longest_one_hand_run(&flat, 6);
        assert!((3..=4).contains(&longest), "{flat:?}");
        // Without `hand` there is nothing for `run` to change.
        let alone = Arrange::parse("6k+run3").unwrap();
        assert_eq!(alone.for_part(PartKind::Strings).key(), "6k");
    }

    #[test]
    fn split_puts_two_notes_on_two_hands_and_apart() {
        // R+G chords: both frets like the left half.
        let groups = run(
            PartKind::Strings,
            &(0..8).map(|i| (i * 500, &[0u8, 1][..])).collect::<Vec<_>>(),
        );
        let pairs = |key: &str| -> Vec<(u8, u8)> {
            arrange(&Arrange::parse(key).unwrap(), PartKind::Strings, 3, &groups)
                .iter()
                .map(|l| (l[0].unwrap(), l[1].unwrap()))
                .collect()
        };
        let without = pairs("6k");
        assert!(
            without.iter().any(|&(x, y)| y < 3 || x + 1 == y),
            "{without:?}"
        );
        let with = pairs("6k+split");
        for &(x, y) in &with {
            assert!(x < 3 && y >= 3 && y > x + 1, "{with:?}");
        }
    }

    #[test]
    fn jj_keeps_a_double_on_its_lane_and_moves_the_third() {
        // Snare doubles 150 ms apart at ADVANCED (shortest repeat 180 ms),
        // then three in a row.
        let times = [0, 150, 1000, 1150, 2000, 2150, 2300];
        let groups = run(
            PartKind::Drums,
            &times.iter().map(|&ms| (ms, &[1u8][..])).collect::<Vec<_>>(),
        );
        let flat = |key: &str| -> Vec<u8> {
            lanes_of(&arrange(
                &Arrange::parse(key).unwrap(),
                PartKind::Drums,
                2,
                &groups,
            ))
            .into_iter()
            .map(Option::unwrap)
            .collect()
        };
        let without = flat("6k+home+stick");
        assert_ne!(without[0], without[1], "{without:?}");
        let with = flat("6k+home+stick+jj");
        assert_eq!(with[0], with[1], "{with:?}");
        assert_eq!(with[2], with[3], "{with:?}");
        assert_eq!(with[4], with[5], "{with:?}");
        assert_ne!(with[5], with[6], "{with:?}");
        // Closer than three quarters of the shortest repeat is no double.
        let close = run(PartKind::Drums, &[(0, &[1u8][..]), (100, &[1])]);
        let a = Arrange::parse("6k+home+stick+jj").unwrap();
        let lanes = lanes_of(&arrange(&a, PartKind::Drums, 2, &close));
        assert_ne!(lanes[0], lanes[1]);
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

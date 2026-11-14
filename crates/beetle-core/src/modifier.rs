use crate::bms::{Lane, NoteEvent, NoteType};
use crate::judge::GaugeType;
use crate::rules::LnOption;
use std::collections::HashMap;

/// Note lane modifiers for chart variation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneModifier {
    /// Standard chart as authored.
    Regular,
    /// Mirrors keys 1..7 (1<->7, 2<->6, 3<->5). Scratch unchanged.
    Mirror,
    /// Shuffles keys 1..7 with a fixed random permutation for the whole song.
    Random,
    /// Cyclically rotates keys 1..7.
    RRandom,
    /// Randomizes each note individually (maintains Long Note integrity).
    SRandom,
}

impl LaneModifier {
    /// The modifier whose `as_str` name this is.
    pub fn from_name(name: &str) -> Option<Self> {
        [
            Self::Regular,
            Self::Mirror,
            Self::Random,
            Self::RRandom,
            Self::SRandom,
        ]
        .into_iter()
        .find(|m| m.as_str() == name)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Regular => "REGULAR",
            Self::Mirror => "MIRROR",
            Self::Random => "RANDOM",
            Self::RRandom => "R-RANDOM",
            Self::SRandom => "S-RANDOM",
        }
    }
}

/// Comprehensive player options configured in song select.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayOptions {
    /// Green number: how long a note takes to cross the visible lane, in ms
    /// (`beetle_render::green_ms_to_px_per_sec` turns it into scroll speed).
    pub green_ms: u32,
    pub lane_modifier: LaneModifier,
    pub gauge_type: GaugeType,
    pub ln: LnOption,
    pub judge_offset_ms: f64,
}

impl Default for PlayOptions {
    fn default() -> Self {
        Self {
            // 400 px/s with no lane cover (592 units of visible lane / 400 px/s).
            green_ms: 1480,
            lane_modifier: LaneModifier::Regular,
            gauge_type: GaugeType::Groove,
            ln: LnOption::Auto,
            judge_offset_ms: 0.0,
        }
    }
}

const KEY_LANES: [Lane; 7] = [
    Lane::Key1,
    Lane::Key2,
    Lane::Key3,
    Lane::Key4,
    Lane::Key5,
    Lane::Key6,
    Lane::Key7,
];

/// Applies note lane modifiers to a vector of note events.
pub fn apply_lane_modifier(
    notes: &[NoteEvent],
    modifier: LaneModifier,
    seed: u64,
    note_seconds: &dyn Fn(u32, f64) -> f64,
) -> Vec<NoteEvent> {
    if modifier == LaneModifier::Regular {
        return notes.to_vec();
    }

    let mut rng = XorShift64::new(if seed == 0 { 123456789 } else { seed });
    let mut modified = notes.to_vec();

    match modifier {
        LaneModifier::Regular => (),
        LaneModifier::Mirror => {
            for note in &mut modified {
                note.lane = mirror_lane(note.lane);
            }
        }
        LaneModifier::Random => {
            let mapping = generate_random_permutation(&mut rng);
            for note in &mut modified {
                note.lane = apply_lane_mapping(note.lane, &mapping);
            }
        }
        LaneModifier::RRandom => {
            let shift = (rng.next_u64() % 6 + 1) as usize;
            let mapping = generate_rotated_permutation(shift);
            for note in &mut modified {
                note.lane = apply_lane_mapping(note.lane, &mapping);
            }
        }
        LaneModifier::SRandom => s_random(&mut modified, &mut rng, note_seconds),
    }

    modified
}

/// S-RANDOM: every note gets its own lane, but never one that another note
/// at the same moment took or that a held long note still covers. Long note
/// ends follow their start. Only 1P Key1..Key7 are shuffled; Scratch, PMS
/// extra buttons and 2P lanes pass through.
fn s_random(notes: &mut [NoteEvent], rng: &mut XorShift64, note_seconds: &dyn Fn(u32, f64) -> f64) {
    let mut order: Vec<usize> = (0..notes.len())
        .filter(|&i| KEY_LANES.contains(&notes[i].lane))
        .collect();
    // In time order; at the same moment, long note ends free their lane first.
    order.sort_by(|&a, &b| {
        let (na, nb) = (&notes[a], &notes[b]);
        na.measure
            .cmp(&nb.measure)
            .then(na.fraction.total_cmp(&nb.fraction))
            .then(
                (na.note_type != NoteType::LongNoteEnd)
                    .cmp(&(nb.note_type != NoteType::LongNoteEnd)),
            )
    });

    // Original lane of a held long note -> the lane it was moved to.
    let mut held: HashMap<Lane, Lane> = HashMap::new();
    let mut used: Vec<Lane> = Vec::with_capacity(7);
    let mut group_at: Option<(u32, f64)> = None;
    // When each key lane last got a note (landmines don't count).
    let mut last_hit = [f64::NEG_INFINITY; 7];
    let mut now = 0.0;

    for i in order {
        let at = (notes[i].measure, notes[i].fraction);
        if group_at != Some(at) {
            group_at = Some(at);
            used.clear();
            now = note_seconds(at.0, at.1);
        }
        let original = notes[i].lane;
        let new_lane = match notes[i].note_type {
            NoteType::LongNoteEnd => match held.remove(&original) {
                Some(lane) => lane,
                None => pick_free_lane(rng, &held, &used, &last_hit, now),
            },
            NoteType::LongNoteStart => {
                let lane = pick_free_lane(rng, &held, &used, &last_hit, now);
                held.insert(original, lane);
                lane
            }
            NoteType::Tap | NoteType::Landmine => pick_free_lane(rng, &held, &used, &last_hit, now),
        };
        used.push(new_lane);
        if notes[i].note_type != NoteType::Landmine {
            last_hit[lane_slot(new_lane)] = now;
        }
        notes[i].lane = new_lane;
    }
}

/// Shortest gap between two notes on one lane that S-RANDOM makes: a faster
/// jack no one can hit. beatoraja's S-RANDOM uses the same 40 ms.
const S_RANDOM_JACK_SECONDS: f64 = 0.040;

fn lane_slot(lane: Lane) -> usize {
    KEY_LANES.iter().position(|&l| l == lane).unwrap_or(0)
}

/// A random key lane that no held long note covers and no note at this
/// moment took, preferring lanes whose last note is more than
/// [`S_RANDOM_JACK_SECONDS`] ago. If every free lane is that recent, the one
/// that rested longest. If none is free (more than 7 notes at once), any key lane.
fn pick_free_lane(
    rng: &mut XorShift64,
    held: &HashMap<Lane, Lane>,
    used: &[Lane],
    last_hit: &[f64; 7],
    now: f64,
) -> Lane {
    let mut free = [Lane::Key1; 7];
    let mut n = 0;
    let mut rested = [Lane::Key1; 7];
    let mut r = 0;
    for lane in KEY_LANES {
        if !used.contains(&lane) && !held.values().any(|&h| h == lane) {
            free[n] = lane;
            n += 1;
            if now - last_hit[lane_slot(lane)] > S_RANDOM_JACK_SECONDS {
                rested[r] = lane;
                r += 1;
            }
        }
    }
    if r > 0 {
        return rested[(rng.next_u64() % r as u64) as usize];
    }
    if n == 0 {
        return KEY_LANES[(rng.next_u64() % 7) as usize];
    }
    *free[..n]
        .iter()
        .min_by(|a, b| last_hit[lane_slot(**a)].total_cmp(&last_hit[lane_slot(**b)]))
        .unwrap()
}

fn mirror_lane(lane: Lane) -> Lane {
    match lane {
        Lane::Scratch => Lane::Scratch,
        Lane::Key1 => Lane::Key7,
        Lane::Key2 => Lane::Key6,
        Lane::Key3 => Lane::Key5,
        Lane::Key4 => Lane::Key4,
        Lane::Key5 => Lane::Key3,
        Lane::Key6 => Lane::Key2,
        Lane::Key7 => Lane::Key1,
        // PMS extra buttons / Double Play 2P lanes are out of scope for this
        // modifier (designed for 1P 7-key only); pass them through unchanged.
        other => other,
    }
}

fn apply_lane_mapping(lane: Lane, mapping: &[Lane; 7]) -> Lane {
    match lane {
        Lane::Scratch => Lane::Scratch,
        Lane::Key1 => mapping[0],
        Lane::Key2 => mapping[1],
        Lane::Key3 => mapping[2],
        Lane::Key4 => mapping[3],
        Lane::Key5 => mapping[4],
        Lane::Key6 => mapping[5],
        Lane::Key7 => mapping[6],
        // Same identity-passthrough rationale as `mirror_lane` above.
        other => other,
    }
}

fn generate_random_permutation(rng: &mut XorShift64) -> [Lane; 7] {
    let mut lanes = KEY_LANES;
    // Fisher-Yates shuffle
    for i in (1..7).rev() {
        let j = (rng.next_u64() as usize) % (i + 1);
        lanes.swap(i, j);
    }
    lanes
}

fn generate_rotated_permutation(shift: usize) -> [Lane; 7] {
    let mut lanes = KEY_LANES;
    lanes.rotate_left(shift % 7);
    lanes
}

/// Lightweight deterministic 64-bit XorShift PRNG with zero external dependencies.
struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0xdeadbeefcafebabe } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 120 BPM in 4/4: a bar is two seconds.
    fn secs(measure: u32, fraction: f64) -> f64 {
        (measure as f64 + fraction) * 2.0
    }

    #[test]
    fn test_mirror_modifier() {
        let notes = vec![
            NoteEvent {
                measure: 1,
                fraction: 0.0,
                lane: Lane::Key1,
                wav_id: None,
                note_type: NoteType::Tap,
            },
            NoteEvent {
                measure: 1,
                fraction: 0.5,
                lane: Lane::Scratch,
                wav_id: None,
                note_type: NoteType::Tap,
            },
        ];

        let mirrored = apply_lane_modifier(&notes, LaneModifier::Mirror, 0, &secs);
        assert_eq!(mirrored[0].lane, Lane::Key7);
        assert_eq!(mirrored[1].lane, Lane::Scratch); // Scratch unchanged
    }

    #[test]
    fn test_random_modifier_lane_bijectivity() {
        let notes: Vec<NoteEvent> = (0..7)
            .map(|i| NoteEvent {
                measure: 1,
                fraction: i as f64 * 0.1,
                lane: KEY_LANES[i],
                wav_id: None,
                note_type: NoteType::Tap,
            })
            .collect();

        let randomized = apply_lane_modifier(&notes, LaneModifier::Random, 42, &secs);

        // Every key lane 1..7 must appear exactly once
        let mut seen = std::collections::HashSet::new();
        for note in &randomized {
            assert_ne!(note.lane, Lane::Scratch);
            seen.insert(note.lane);
        }
        assert_eq!(seen.len(), 7);
    }
    fn tap(measure: u32, fraction: f64, lane: Lane, note_type: NoteType) -> NoteEvent {
        NoteEvent {
            measure,
            fraction,
            lane,
            wav_id: None,
            note_type,
        }
    }

    #[test]
    fn s_random_never_stacks_chords() {
        // Seven-note chords on every beat: each must land on seven distinct lanes.
        let mut notes = Vec::new();
        for beat in 0..32 {
            for lane in KEY_LANES {
                notes.push(tap(
                    1 + beat / 4,
                    (beat % 4) as f64 * 0.25,
                    lane,
                    NoteType::Tap,
                ));
            }
        }
        for seed in 1..20 {
            let out = apply_lane_modifier(&notes, LaneModifier::SRandom, seed, &secs);
            for chord in out.chunks(7) {
                let lanes: std::collections::HashSet<_> = chord.iter().map(|n| n.lane).collect();
                assert_eq!(lanes.len(), 7, "seed {seed}");
            }
        }
    }

    #[test]
    fn s_random_keeps_held_lanes_clear() {
        // A long note on Key1 over a whole bar, with taps on the other six lanes inside it.
        let mut notes = vec![tap(1, 0.0, Lane::Key1, NoteType::LongNoteStart)];
        for step in 1..8 {
            for &lane in &KEY_LANES[1..] {
                notes.push(tap(1, step as f64 / 8.0, lane, NoteType::Tap));
            }
        }
        notes.push(tap(2, 0.0, Lane::Key1, NoteType::LongNoteEnd));
        for seed in 1..20 {
            let out = apply_lane_modifier(&notes, LaneModifier::SRandom, seed, &secs);
            let ln = out[0].lane;
            assert_eq!(out.last().unwrap().lane, ln, "seed {seed}");
            assert!(
                out[1..out.len() - 1].iter().all(|n| n.lane != ln),
                "seed {seed}"
            );
        }
    }

    #[test]
    fn s_random_avoids_impossible_jacks() {
        // A 32nd-note stream at 300 BPM (25 ms apart), two notes at a time:
        // five lanes are always rested, so no lane may repeat within 40 ms.
        let step = 0.025;
        let mut notes = Vec::new();
        for k in 0..64u32 {
            let t = k as f64 * step;
            let (m, f) = ((t / 2.0) as u32, (t / 2.0).fract());
            notes.push(tap(m, f, Lane::Key1, NoteType::Tap));
            notes.push(tap(m, f, Lane::Key2, NoteType::Tap));
        }
        for seed in 1..20 {
            let out = apply_lane_modifier(&notes, LaneModifier::SRandom, seed, &secs);
            let mut last = [f64::NEG_INFINITY; 7];
            for n in &out {
                let t = secs(n.measure, n.fraction);
                let slot = lane_slot(n.lane);
                assert!(t - last[slot] > S_RANDOM_JACK_SECONDS, "seed {seed}");
                last[slot] = t;
            }
        }
    }
}

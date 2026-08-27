use crate::bms::{Lane, NoteEvent, NoteType};
use crate::judge::GaugeType;
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
    pub hi_speed: f32,
    pub lane_modifier: LaneModifier,
    pub gauge_type: GaugeType,
    pub judge_offset_ms: f64,
}

impl Default for PlayOptions {
    fn default() -> Self {
        Self {
            hi_speed: 400.0, // 400 pixels/sec default
            lane_modifier: LaneModifier::Regular,
            gauge_type: GaugeType::Groove,
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
pub fn apply_lane_modifier(notes: &[NoteEvent], modifier: LaneModifier, seed: u64) -> Vec<NoteEvent> {
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
        LaneModifier::SRandom => {
            // Random per note, but Long Note start & end must share the same lane
            let mut active_ln_lanes: HashMap<Lane, Lane> = HashMap::new();

            for note in &mut modified {
                if note.lane == Lane::Scratch {
                    continue;
                }

                match note.note_type {
                    NoteType::LongNoteStart => {
                        let new_lane = KEY_LANES[(rng.next_u64() % 7) as usize];
                        active_ln_lanes.insert(note.lane, new_lane);
                        note.lane = new_lane;
                    }
                    NoteType::LongNoteEnd => {
                        if let Some(target_lane) = active_ln_lanes.remove(&note.lane) {
                            note.lane = target_lane;
                        } else {
                            note.lane = KEY_LANES[(rng.next_u64() % 7) as usize];
                        }
                    }
                    NoteType::Tap | NoteType::Landmine => {
                        note.lane = KEY_LANES[(rng.next_u64() % 7) as usize];
                    }
                }
            }
        }
    }

    modified
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

        let mirrored = apply_lane_modifier(&notes, LaneModifier::Mirror, 0);
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

        let randomized = apply_lane_modifier(&notes, LaneModifier::Random, 42);

        // Every key lane 1..7 must appear exactly once
        let mut seen = std::collections::HashSet::new();
        for note in &randomized {
            assert_ne!(note.lane, Lane::Scratch);
            seen.insert(note.lane);
        }
        assert_eq!(seen.len(), 7);
    }
}

//! A parsed chart laid out again on 4, 6 or 8 lanes (`chart-arrange`, B29):
//! BMS, IIDX and pop'n charts at another key count. GFDM charts are
//! arranged where their BMS is made (`cs_disc::gfdm_bms`), not here.
//!
//! The notes keep their times and sounds; only their lanes change. A long
//! note keeps one lane from its start to its end. A note that gets no lane
//! (more notes at once than free lanes, or thinned) plays by itself, and
//! mines go. The scratch is a lane like the others.
//!
//! The arranged chart is its own chart for records: its id is made from the
//! original's id and the notes as arranged (`arranged_id`), so a change in
//! the arranger's rules gives a new id, as it does for GFDM charts.

use beetle_core::{
    BmsChart, ChartId, Lane, NoteEvent, NoteType, PlayMode, SongMetadata, TimingModel,
};
use chart_arrange::{Arrange, Group, Note, Source, BAR_SLOTS};

/// The lanes of a chart of `mode`, left to right, as the arranger sees
/// them. `None` for a mode that is not arranged (double play).
fn source_lanes(mode: PlayMode) -> Option<&'static [Lane]> {
    const KEYS: [Lane; 10] = [
        Lane::Scratch,
        Lane::Key1,
        Lane::Key2,
        Lane::Key3,
        Lane::Key4,
        Lane::Key5,
        Lane::Key6,
        Lane::Key7,
        Lane::Key8,
        Lane::Key9,
    ];
    match mode {
        PlayMode::Keys5 => Some(&KEYS[..6]),
        PlayMode::Keys7 => Some(&KEYS[..8]),
        PlayMode::Keys9 => Some(&KEYS[1..]),
        PlayMode::Keys4 | PlayMode::Keys6 | PlayMode::Keys8 => mode.restricted_lanes(),
        PlayMode::Keys10 | PlayMode::Keys14 => None,
    }
}

/// The key mode an arrangement of `lanes` lanes plays in.
pub fn arranged_mode(lanes: u8) -> PlayMode {
    match lanes {
        4 => PlayMode::Keys4,
        8 => PlayMode::Keys8,
        _ => PlayMode::Keys6,
    }
}

/// Whether a chart of `mode` can be arranged here.
pub fn can_arrange(mode: PlayMode) -> bool {
    source_lanes(mode).is_some()
}

/// The shortest time between two hits on one lane, by the chart's
/// `#DIFFICULTY`, when the arrangement has none (`j`), ms.
fn default_jack_ms(difficulty: Option<u32>) -> u16 {
    match difficulty {
        Some(1 | 2) => 250,
        Some(3) => 180,
        _ => 125,
    }
}

/// `chart` (of `mode`) arranged by `a`: the same chart with its notes on the
/// lanes of the arrangement's key mode. `None` when the mode is not
/// arranged.
pub fn arrange_chart(
    chart: &BmsChart,
    timing: &TimingModel,
    mode: PlayMode,
    difficulty: Option<u32>,
    a: &Arrange,
) -> Option<BmsChart> {
    let source = source_lanes(mode)?;
    let target_mode = arranged_mode(a.lanes);
    let target = target_mode.restricted_lanes()?;
    let ms_of =
        |n: &NoteEvent| (timing.beat_to_time_seconds(n.measure, n.fraction) * 1000.0).round();

    // The notes that take part, by index: taps and long note starts on the
    // mode's lanes, each start with its end.
    let mut open: [Option<usize>; Lane::ALL.len()] = [None; Lane::ALL.len()];
    let mut end_of: Vec<Option<usize>> = vec![None; chart.notes.len()];
    let lane_at = |lane: Lane| Lane::ALL.iter().position(|&l| l == lane).unwrap_or(0);
    for (i, n) in chart.notes.iter().enumerate() {
        match n.note_type {
            NoteType::LongNoteStart => open[lane_at(n.lane)] = Some(i),
            NoteType::LongNoteEnd => {
                if let Some(start) = open[lane_at(n.lane)].take() {
                    end_of[start] = Some(i);
                }
            }
            _ => {}
        }
    }
    struct Item {
        index: usize,
        ms: i64,
        note: Note,
        scratch: bool,
    }
    let mut items: Vec<Item> = Vec::new();
    // Notes that play by themselves from the start: on a lane the mode
    // does not have.
    let mut auto: Vec<usize> = Vec::new();
    for (i, n) in chart.notes.iter().enumerate() {
        let start = match n.note_type {
            NoteType::Tap => true,
            // A start without an end is not a long note the judge pairs.
            NoteType::LongNoteStart => end_of[i].is_some(),
            NoteType::LongNoteEnd | NoteType::Landmine => false,
        };
        if !start {
            continue;
        }
        let Some(origin) = source.iter().position(|&l| l == n.lane) else {
            auto.push(i);
            continue;
        };
        let ms = ms_of(n) as i64;
        let hold_ms = end_of[i]
            .map(|e| (ms_of(&chart.notes[e]) as i64 - ms).max(1) as u32)
            .unwrap_or(0);
        items.push(Item {
            index: i,
            ms,
            note: Note {
                origin: origin as u8,
                key: n.wav_id.map_or(0, |w| w.0),
                ticks: 0,
                hold_ms,
                from: None,
            },
            scratch: n.lane == Lane::Scratch,
        });
    }
    items.sort_by_key(|it| (it.ms, it.note.origin, it.index));

    // Notes at one time are one group.
    let mut groups: Vec<Group> = Vec::new();
    let mut members: Vec<Vec<usize>> = Vec::new();
    for it in &items {
        if groups.last().is_none_or(|g| g.ms != it.ms) {
            let n = &chart.notes[it.index];
            groups.push(Group {
                ms: it.ms,
                bar: n.measure,
                slot: ((n.fraction * BAR_SLOTS as f64) as u16).min(BAR_SLOTS - 1),
                accent: 0,
                notes: Vec::new(),
            });
            members.push(Vec::new());
        }
        let (Some(g), Some(m)) = (groups.last_mut(), members.last_mut()) else {
            continue;
        };
        if it.scratch {
            g.accent = 2;
        }
        g.notes.push(it.note);
        m.push(it.index);
    }

    let kind = Source::Keys {
        lanes: source.len() as u8,
    };
    let lanes = chart_arrange::arrange(a, kind, default_jack_ms(difficulty), &groups);

    // Each note's new lane; `None` plays by itself.
    let mut placed: Vec<Option<Lane>> = vec![None; chart.notes.len()];
    for (m, lanes) in members.iter().zip(&lanes) {
        for (&i, lane) in m.iter().zip(lanes) {
            placed[i] = lane.and_then(|l| target.get(l as usize).copied());
        }
    }
    let mut out = chart.clone();
    out.notes.clear();
    for (i, n) in chart.notes.iter().enumerate() {
        match n.note_type {
            NoteType::Landmine => {}
            NoteType::LongNoteEnd => {}
            NoteType::Tap | NoteType::LongNoteStart => match (placed[i], end_of[i]) {
                (Some(lane), end) => {
                    out.notes.push(NoteEvent { lane, ..n.clone() });
                    if let Some(e) = end {
                        out.notes.push(NoteEvent {
                            lane,
                            ..chart.notes[e].clone()
                        });
                    }
                }
                (None, _) => {
                    if let Some(wav) = n.wav_id {
                        out.bgm_notes.push((n.measure, n.fraction, wav));
                    }
                }
            },
        }
    }
    let by_place = |am: u32, af: f64, bm: u32, bf: f64| am.cmp(&bm).then(af.total_cmp(&bf));
    out.notes
        .sort_by(|x, y| by_place(x.measure, x.fraction, y.measure, y.fraction));
    out.bgm_notes.sort_by(|x, y| by_place(x.0, x.1, y.0, y.1));
    out.header.declared_mode = Some(target_mode);
    out.header.player = 1;
    out.total_notes_count = out.notes.len();
    out.has_scratch = out.notes.iter().any(|n| n.lane == Lane::Scratch);
    out.has_k67 = out
        .notes
        .iter()
        .any(|n| matches!(n.lane, Lane::Key6 | Lane::Key7));
    out.has_2p_dp = false;
    out.has_2p_key1 = false;
    out.has_pms_ch = false;
    Some(out)
}

/// The id of `arranged`, made from the original chart's id: records of an
/// arranged chart are its own, and another arrangement's are another's.
pub fn arranged_id(original: ChartId, arranged: &BmsChart) -> ChartId {
    let mut bytes = Vec::with_capacity(48 + arranged.notes.len() * 16);
    bytes.extend_from_slice(b"chart-arrange\0");
    bytes.extend_from_slice(original.as_bytes());
    for n in &arranged.notes {
        bytes.extend_from_slice(&n.measure.to_le_bytes());
        bytes.extend_from_slice(&n.fraction.to_bits().to_le_bytes());
        let lane = Lane::ALL.iter().position(|&l| l == n.lane).unwrap_or(0) as u8;
        let kind = match n.note_type {
            NoteType::Tap => 0u8,
            NoteType::LongNoteStart => 1,
            NoteType::LongNoteEnd => 2,
            NoteType::Landmine => 3,
        };
        bytes.extend_from_slice(&[lane, kind]);
        bytes.extend_from_slice(&n.wav_id.map_or(u16::MAX, |w| w.0).to_le_bytes());
    }
    ChartId::of_bytes(&bytes)
}

/// `song` as its arranged chart: its own id, key mode and note counts.
pub fn arranged_metadata(song: &SongMetadata, arranged: &BmsChart) -> SongMetadata {
    let id = arranged_id(song.id, arranged);
    let digest = id.as_bytes();
    let mut md5 = [0u8; 16];
    md5.copy_from_slice(&digest[16..]);
    let mut legacy = [0u8; 8];
    legacy.copy_from_slice(&digest[..8]);
    SongMetadata {
        id,
        md5,
        legacy_hash: u64::from_le_bytes(legacy),
        notes_count: arranged.playable_notes_len(),
        ln_count: arranged
            .notes
            .iter()
            .filter(|n| n.note_type == NoteType::LongNoteEnd)
            .count() as u32,
        play_mode: arranged.header.declared_mode.unwrap_or(song.play_mode),
        ..song.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beetle_core::parse_bms;
    use std::collections::BTreeMap;

    /// A 7 key chart: scratch and keys, chords, two long notes (one on the
    /// scratch, under a run of quick notes) and a mine.
    const CHART: &str = "\
#PLAYER 1
#BPM 150
#LNTYPE 1
#WAV01 a.wav
#WAV02 b.wav
#WAV03 c.wav
#WAV04 d.wav
#WAV05 e.wav
#00111:01020102
#00112:00010001
#00113:02000200
#00114:03030303
#00115:04040404
#00116:05000000
#00118:01020304
#00119:04030201
#00211:0102030401020304
#00212:0403020104030201
#00213:0101010101010101
#00214:02020202
#00215:03000300
#00218:0405
#00219:0504
#00256:0101000000000000
#002D3:0A00
#00311:01
#00312:02
#00313:03
#00314:04
#00315:05
#00316:01
#00318:02
#00319:03
#00355:0002020000000000
";

    fn sounds(chart: &BmsChart) -> BTreeMap<u16, usize> {
        let mut out = BTreeMap::new();
        let played = chart
            .notes
            .iter()
            .filter(|n| matches!(n.note_type, NoteType::Tap | NoteType::LongNoteStart))
            .filter_map(|n| n.wav_id);
        for w in played.chain(chart.bgm_notes.iter().map(|b| b.2)) {
            *out.entry(w.0).or_insert(0) += 1;
        }
        out
    }

    #[test]
    fn a_7_key_chart_arranges_onto_each_key_count() {
        let chart = parse_bms(CHART).unwrap();
        let timing = TimingModel::from_chart(&chart);
        let mode = chart.detect_play_mode();
        assert_eq!(mode, PlayMode::Keys7);
        let mut ids = Vec::new();
        for key in ["4k+auto", "6k+auto", "8k+auto", "4k+bal+hand+d250"] {
            let a = Arrange::parse(key).unwrap();
            let out = arrange_chart(&chart, &timing, mode, Some(4), &a).unwrap();
            let target = arranged_mode(a.lanes);
            assert_eq!(out.detect_play_mode(), target, "{key}");
            let lanes = target.restricted_lanes().unwrap();
            assert!(out.notes.iter().all(|n| lanes.contains(&n.lane)), "{key}");
            assert!(
                out.notes.iter().all(|n| n.note_type != NoteType::Landmine),
                "{key}"
            );
            // No sound is lost or made.
            assert_eq!(sounds(&out), sounds(&chart), "{key}");
            // A long note's end is on its start's lane, and nothing else is
            // on that lane between them; no two notes share a place.
            let ms = |n: &NoteEvent| {
                (timing.beat_to_time_seconds(n.measure, n.fraction) * 1000.0).round() as i64
            };
            let mut long = 0;
            for &lane in lanes {
                let on: Vec<&NoteEvent> = out.notes.iter().filter(|n| n.lane == lane).collect();
                let mut held = false;
                for pair in on.windows(2) {
                    assert!(ms(pair[0]) < ms(pair[1]), "{key}: two notes at one place");
                }
                for n in on {
                    match n.note_type {
                        NoteType::LongNoteStart => {
                            assert!(!held, "{key}: a start under a long note");
                            held = true;
                            long += 1;
                        }
                        NoteType::LongNoteEnd => {
                            assert!(held, "{key}: an end without a start");
                            held = false;
                        }
                        _ => assert!(!held, "{key}: a note under a long note"),
                    }
                }
                assert!(!held, "{key}: a long note without an end");
            }
            if a.thin_ms.is_none() {
                assert_eq!(long, 2, "{key}: both long notes play");
            }
            // The same chart and arrangement give the same chart and id.
            let again = arrange_chart(&chart, &timing, mode, Some(4), &a).unwrap();
            let id = arranged_id(ChartId::of_bytes(CHART.as_bytes()), &out);
            assert_eq!(
                id,
                arranged_id(ChartId::of_bytes(CHART.as_bytes()), &again),
                "{key}"
            );
            ids.push(id);
        }
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 4, "each arrangement is its own chart");
    }

    #[test]
    fn a_9_button_chart_arranges_and_double_play_does_not() {
        let pms = "\
#BPM 120
#WAV01 a.wav
#00111:01000000
#00112:00010000
#00113:00000100
#00114:00000001
#00115:01000000
#00122:00010000
#00123:00000100
#00124:00000001
#00125:01000000
";
        let chart = parse_bms(pms).unwrap();
        let timing = TimingModel::from_chart(&chart);
        assert_eq!(chart.detect_play_mode(), PlayMode::Keys9);
        let a = Arrange::parse("6k").unwrap();
        let out = arrange_chart(&chart, &timing, PlayMode::Keys9, None, &a).unwrap();
        assert_eq!(out.detect_play_mode(), PlayMode::Keys6);
        assert_eq!(out.notes.len(), 9);
        // The first three notes are buttons 1, 5 and 9 together: the first
        // is on the left edge, the last on the right.
        let first: Vec<Lane> = out.notes[..3].iter().map(|n| n.lane).collect();
        assert!(first.contains(&Lane::Key1) && first.contains(&Lane::Key7));
        assert!(!can_arrange(PlayMode::Keys14));
        assert!(arrange_chart(&chart, &timing, PlayMode::Keys14, None, &a).is_none());
    }

    #[test]
    fn arranged_metadata_is_its_own_chart() {
        let chart = parse_bms(CHART).unwrap();
        let timing = TimingModel::from_chart(&chart);
        let song = SongMetadata::from_bytes("D:/BMS/x.bms", CHART.as_bytes()).unwrap();
        let a = Arrange::parse("4k+auto").unwrap();
        let out = arrange_chart(&chart, &timing, song.play_mode, song.difficulty, &a).unwrap();
        let meta = arranged_metadata(&song, &out);
        assert_ne!(meta.id, song.id);
        assert_eq!(meta.play_mode, PlayMode::Keys4);
        assert_eq!(meta.file_path, song.file_path);
        assert_eq!(meta.ln_count, 2);
        assert_eq!(meta.notes_count, out.notes.len());
    }
}

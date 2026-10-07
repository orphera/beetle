use crate::bms::{BmsChart, TimingEventKind};
use std::collections::HashMap;

/// A segment in the timeline with constant BPM and optional stop duration.
#[derive(Debug, Clone, PartialEq)]
pub struct TimingSegment {
    pub measure: u32,
    pub fraction: f64,
    pub start_beat: f64,
    pub start_time_seconds: f64,
    pub bpm: f64,
    pub stop_duration_seconds: f64,
}

/// Timing model that maps measure/fraction to absolute audio seconds and vice versa.
#[derive(Debug, Clone)]
pub struct TimingModel {
    segments: Vec<TimingSegment>,
    measure_lengths: HashMap<u32, f64>,
    initial_bpm: f64,
}

impl Default for TimingModel {
    fn default() -> Self {
        Self {
            segments: vec![TimingSegment {
                measure: 0,
                fraction: 0.0,
                start_beat: 0.0,
                start_time_seconds: 0.0,
                bpm: 130.0,
                stop_duration_seconds: 0.0,
            }],
            measure_lengths: HashMap::new(),
            initial_bpm: 130.0,
        }
    }
}

impl TimingModel {
    /// Builds a timing model from a parsed BMS chart.
    pub fn from_chart(chart: &BmsChart) -> Self {
        let initial_bpm = if chart.header.bpm > 0.0 {
            chart.header.bpm
        } else {
            130.0
        };

        let measure_lengths = chart.measure_lengths.clone();

        let mut segments: Vec<TimingSegment> = Vec::new();
        let mut current_bpm = initial_bpm;
        let mut last_beat = 0.0;
        let mut last_time = 0.0;

        // Push initial segment at measure 0, fraction 0.0
        segments.push(TimingSegment {
            measure: 0,
            fraction: 0.0,
            start_beat: 0.0,
            start_time_seconds: 0.0,
            bpm: initial_bpm,
            stop_duration_seconds: 0.0,
        });

        for event in &chart.timing_events {
            let event_beat =
                Self::calculate_beat_pos(event.measure, event.fraction, &measure_lengths);
            if event_beat < last_beat {
                continue;
            }

            let delta_beats = event_beat - last_beat;
            let delta_time = if current_bpm > 0.0 {
                (delta_beats * 60.0) / current_bpm
            } else {
                0.0
            };

            let event_time = last_time + delta_time;

            match event.kind {
                TimingEventKind::BpmChange(new_bpm) => {
                    if new_bpm > 0.0 {
                        current_bpm = new_bpm;
                        segments.push(TimingSegment {
                            measure: event.measure,
                            fraction: event.fraction,
                            start_beat: event_beat,
                            start_time_seconds: event_time,
                            bpm: current_bpm,
                            stop_duration_seconds: 0.0,
                        });
                        last_beat = event_beat;
                        last_time = event_time;
                    }
                }
                TimingEventKind::StopMeasures(measures) => {
                    // Stop duration in seconds calculated at current BPM
                    let stop_beats = measures * 4.0;
                    let stop_seconds = if current_bpm > 0.0 {
                        (stop_beats * 60.0) / current_bpm
                    } else {
                        0.0
                    };

                    segments.push(TimingSegment {
                        measure: event.measure,
                        fraction: event.fraction,
                        start_beat: event_beat,
                        start_time_seconds: event_time,
                        bpm: current_bpm,
                        stop_duration_seconds: stop_seconds,
                    });
                    last_beat = event_beat;
                    last_time = event_time + stop_seconds;
                }
            }
        }

        Self {
            segments,
            measure_lengths,
            initial_bpm,
        }
    }

    /// Measure length in 4-beat units (default 1.0 = 4 beats).
    pub fn measure_length(&self, measure: u32) -> f64 {
        self.measure_lengths.get(&measure).copied().unwrap_or(1.0)
    }

    /// Calculate cumulative beats from measure 0 up to (measure, fraction).
    fn calculate_beat_pos(measure: u32, fraction: f64, lengths: &HashMap<u32, f64>) -> f64 {
        let mut total_beats = 0.0;
        for m in 0..measure {
            let len = lengths.get(&m).copied().unwrap_or(1.0);
            total_beats += len * 4.0;
        }
        let curr_len = lengths.get(&measure).copied().unwrap_or(1.0);
        total_beats + (fraction * curr_len * 4.0)
    }

    /// Returns the absolute beat position for a given (measure, fraction).
    pub fn beat_position(&self, measure: u32, fraction: f64) -> f64 {
        Self::calculate_beat_pos(measure, fraction, &self.measure_lengths)
    }

    /// Converts a measure and beat fraction into absolute time in seconds.
    pub fn beat_to_time_seconds(&self, measure: u32, fraction: f64) -> f64 {
        let target_beat = self.beat_position(measure, fraction);

        // Last segment starting at or before target_beat.
        let idx = self
            .segments
            .partition_point(|s| s.start_beat <= target_beat)
            .saturating_sub(1);
        let best_segment = &self.segments[idx];

        // An object exactly on a STOP's beat is hit before the stop begins:
        // use the time of the first segment at that beat, ahead of any stops.
        if target_beat <= best_segment.start_beat {
            let first = self.segments[..=idx]
                .iter()
                .rposition(|s| s.start_beat < best_segment.start_beat)
                .map_or(0, |i| i + 1);
            return self.segments[first].start_time_seconds;
        }

        let delta_beats = target_beat - best_segment.start_beat;
        let delta_time = if best_segment.bpm > 0.0 {
            (delta_beats * 60.0) / best_segment.bpm
        } else {
            0.0
        };

        best_segment.start_time_seconds + best_segment.stop_duration_seconds + delta_time
    }

    /// Converts absolute time in seconds to the corresponding (measure, fraction).
    pub fn time_to_beat(&self, time_seconds: f64) -> (u32, f64) {
        if time_seconds <= 0.0 {
            return (0, 0.0);
        }
        self.beat_to_measure_fraction(self.time_to_beat_position(time_seconds))
    }

    /// Converts absolute time in seconds to a cumulative beat position.
    ///
    /// Frozen during STOP, and extrapolated at the first BPM before time 0 so
    /// lead-in notes keep a consistent scroll distance. Renderers use this to
    /// place notes by beat distance, which is what makes BPM changes and STOPs
    /// visible as scroll-speed changes.
    pub fn time_to_beat_position(&self, time_seconds: f64) -> f64 {
        if time_seconds <= 0.0 {
            return time_seconds * self.segments[0].bpm / 60.0;
        }
        let idx = self
            .segments
            .partition_point(|s| s.start_time_seconds <= time_seconds)
            .saturating_sub(1);
        let seg = &self.segments[idx];
        let moving_from = seg.start_time_seconds + seg.stop_duration_seconds;
        if time_seconds < moving_from {
            // Frozen in STOP
            seg.start_beat
        } else {
            seg.start_beat + (time_seconds - moving_from) * seg.bpm / 60.0
        }
    }

    /// Converts a cumulative beat count to (measure, fraction).
    pub fn beat_to_measure_fraction(&self, mut beat: f64) -> (u32, f64) {
        if beat <= 0.0 {
            return (0, 0.0);
        }

        let mut measure = 0;
        loop {
            let beats_in_measure = self.measure_length(measure) * 4.0;
            if beat < beats_in_measure || beats_in_measure <= 0.0 {
                let fraction = (beat / beats_in_measure).clamp(0.0, 1.0);
                return (measure, fraction);
            }
            beat -= beats_in_measure;
            measure += 1;
        }
    }

    /// Initial BPM of the chart.
    pub fn initial_bpm(&self) -> f64 {
        self.initial_bpm
    }

    /// Calculates total playable duration in seconds of a chart.
    pub fn total_duration_seconds(&self, chart: &BmsChart) -> f64 {
        let mut max_time = 0.0;
        for note in &chart.notes {
            let t = self.beat_to_time_seconds(note.measure, note.fraction);
            if t > max_time {
                max_time = t;
            }
        }
        for &(m, f, _) in &chart.bgm_notes {
            let t = self.beat_to_time_seconds(m, f);
            if t > max_time {
                max_time = t;
            }
        }
        max_time
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bms::*;

    #[test]
    fn test_constant_bpm_timing() {
        let chart = BmsChart {
            header: BmsHeader {
                bpm: 120.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let model = TimingModel::from_chart(&chart);

        // At 120 BPM: 1 beat = 0.5s, 1 measure (4 beats) = 2.0s
        assert_eq!(model.beat_to_time_seconds(0, 0.0), 0.0);
        assert_eq!(model.beat_to_time_seconds(0, 0.5), 1.0);
        assert_eq!(model.beat_to_time_seconds(1, 0.0), 2.0);
        assert_eq!(model.beat_to_time_seconds(2, 0.0), 4.0);

        // Inverse check
        assert_eq!(model.time_to_beat(0.0), (0, 0.0));
        assert_eq!(model.time_to_beat(1.0), (0, 0.5));
        assert_eq!(model.time_to_beat(2.0), (1, 0.0));
        assert_eq!(model.time_to_beat(4.0), (2, 0.0));
    }

    #[test]
    fn test_variable_bpm_timing() {
        let chart = BmsChart {
            header: BmsHeader {
                bpm: 120.0,
                ..Default::default()
            },
            timing_events: vec![TimingEvent {
                measure: 1,
                fraction: 0.0,
                kind: TimingEventKind::BpmChange(240.0),
            }],
            ..Default::default()
        };
        let model = TimingModel::from_chart(&chart);

        // Measure 0 (120 BPM, 4 beats) -> duration 2.0s
        // Measure 1 (240 BPM, 4 beats) -> 1 beat = 0.25s, measure duration = 1.0s
        assert_eq!(model.beat_to_time_seconds(0, 0.0), 0.0);
        assert_eq!(model.beat_to_time_seconds(1, 0.0), 2.0);
        assert_eq!(model.beat_to_time_seconds(1, 0.5), 2.5);
        assert_eq!(model.beat_to_time_seconds(2, 0.0), 3.0);
    }

    #[test]
    fn test_stop_event_timing() {
        let chart = BmsChart {
            header: BmsHeader {
                bpm: 120.0,
                ..Default::default()
            },
            timing_events: vec![TimingEvent {
                measure: 1,
                fraction: 0.0,
                kind: TimingEventKind::StopMeasures(1.0), // Stop 1 measure (2.0s at 120 BPM)
            }],
            ..Default::default()
        };
        let model = TimingModel::from_chart(&chart);

        // Measure 0 -> 2.0s
        // Measure 1 start is the stop's own beat: an object there is hit at 2.0s,
        // before the stop. Anything after it is pushed back by the 2.0s stop.
        assert_eq!(model.beat_to_time_seconds(0, 0.0), 0.0);
        assert_eq!(model.beat_to_time_seconds(1, 0.0), 2.0);
        assert_eq!(model.beat_to_time_seconds(1, 0.25), 4.5);
        assert_eq!(model.beat_to_time_seconds(2, 0.0), 6.0);

        // Time during stop (2.5s) maps to measure 1, fraction 0.0
        assert_eq!(model.time_to_beat(2.5), (1, 0.0));
    }

    #[test]
    fn beat_position_follows_bpm_change_and_freezes_in_stop() {
        let chart = BmsChart {
            header: BmsHeader {
                bpm: 120.0,
                ..Default::default()
            },
            timing_events: vec![
                TimingEvent {
                    measure: 1,
                    fraction: 0.0,
                    kind: TimingEventKind::BpmChange(240.0),
                },
                TimingEvent {
                    measure: 2,
                    fraction: 0.0,
                    kind: TimingEventKind::StopMeasures(1.0),
                },
            ],
            ..Default::default()
        };
        let model = TimingModel::from_chart(&chart);

        // 120 BPM: 2 beats/s. 240 BPM from beat 4: 4 beats/s.
        assert_eq!(model.time_to_beat_position(1.0), 2.0);
        assert_eq!(model.time_to_beat_position(2.0), 4.0);
        assert_eq!(model.time_to_beat_position(2.5), 6.0);
        // Measure 2 starts at beat 8 (t=3.0); the stop lasts 1 measure = 1.0s at 240 BPM.
        assert_eq!(model.time_to_beat_position(3.5), 8.0);
        assert_eq!(model.time_to_beat_position(4.0), 8.0);
        assert_eq!(model.time_to_beat_position(4.5), 10.0);
        // Before the start, extrapolate at the first BPM.
        assert_eq!(model.time_to_beat_position(-1.0), -2.0);
    }

    #[test]
    fn objects_on_a_stacked_stop_beat_are_hit_before_all_of_the_stops() {
        let stop = |measures| TimingEvent {
            measure: 1,
            fraction: 0.0,
            kind: TimingEventKind::StopMeasures(measures),
        };
        let chart = BmsChart {
            header: BmsHeader {
                bpm: 120.0,
                ..Default::default()
            },
            // BPM change first (parser order), then two stops, all at beat 4.
            timing_events: vec![
                TimingEvent {
                    measure: 1,
                    fraction: 0.0,
                    kind: TimingEventKind::BpmChange(240.0),
                },
                stop(1.0),
                stop(0.5),
            ],
            ..Default::default()
        };
        let model = TimingModel::from_chart(&chart);

        // Beat 4 is reached at 2.0s; stops last 1.0s + 0.5s at 240 BPM.
        assert_eq!(model.beat_to_time_seconds(1, 0.0), 2.0);
        assert_eq!(model.beat_to_time_seconds(1, 0.5), 2.0 + 1.5 + 0.5);
        assert_eq!(model.time_to_beat_position(2.0), 4.0);
        assert_eq!(model.time_to_beat_position(3.4), 4.0);
        assert_eq!(model.time_to_beat_position(4.0), 4.0 + 0.5 * 4.0);
    }

    #[test]
    fn test_custom_measure_length() {
        let mut measure_lengths = HashMap::new();
        measure_lengths.insert(0, 0.75); // 3/4 time measure (3 beats)

        let chart = BmsChart {
            header: BmsHeader {
                bpm: 120.0,
                ..Default::default()
            },
            measure_lengths,
            ..Default::default()
        };
        let model = TimingModel::from_chart(&chart);

        // Measure 0 has 3 beats = 1.5s
        assert_eq!(model.beat_to_time_seconds(0, 0.0), 0.0);
        assert_eq!(model.beat_to_time_seconds(1, 0.0), 1.5);
        assert_eq!(model.beat_to_time_seconds(2, 0.0), 3.5); // 1.5s + 2.0s
    }
}

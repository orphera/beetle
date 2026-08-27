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
            let event_beat = Self::calculate_beat_pos(event.measure, event.fraction, &measure_lengths);
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

        // Find the segment applicable to target_beat
        let mut best_segment = &self.segments[0];
        for seg in &self.segments {
            if seg.start_beat <= target_beat {
                best_segment = seg;
            } else {
                break;
            }
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

        // Find the segment applicable to time_seconds
        let mut best_segment = &self.segments[0];
        for seg in &self.segments {
            if seg.start_time_seconds <= time_seconds {
                best_segment = seg;
            } else {
                break;
            }
        }

        let beat = if time_seconds < best_segment.start_time_seconds + best_segment.stop_duration_seconds {
            // Frozen in STOP
            best_segment.start_beat
        } else {
            let delta_time = time_seconds - (best_segment.start_time_seconds + best_segment.stop_duration_seconds);
            let delta_beats = (delta_time * best_segment.bpm) / 60.0;
            best_segment.start_beat + delta_beats
        };

        self.beat_to_measure_fraction(beat)
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
        // Measure 1 start -> time = 2.0s + 2.0s (stop) = 4.0s for playback beyond stop
        assert_eq!(model.beat_to_time_seconds(0, 0.0), 0.0);
        assert_eq!(model.beat_to_time_seconds(1, 0.0), 4.0);
        assert_eq!(model.beat_to_time_seconds(2, 0.0), 6.0);

        // Time during stop (2.5s) maps to measure 1, fraction 0.0
        assert_eq!(model.time_to_beat(2.5), (1, 0.0));
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

use crate::bms::BmsChart;

/// A discrete point on the timing timeline with precalculated audio seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct TimingPoint {
    pub measure: u32,
    pub fraction: f64,
    pub time_seconds: f64,
    pub bpm: f64,
}

/// Timing model that maps measure/fraction to absolute audio seconds and vice versa.
#[derive(Debug, Clone, Default)]
pub struct TimingModel {
    pub points: Vec<TimingPoint>,
    pub initial_bpm: f64,
}

impl TimingModel {
    /// Builds a timing model from a parsed BMS chart.
    pub fn from_chart(chart: &BmsChart) -> Self {
        // Skeleton initialization
        Self {
            points: Vec::new(),
            initial_bpm: if chart.header.bpm > 0.0 { chart.header.bpm } else { 130.0 },
        }
    }

    /// Converts a measure and beat fraction into absolute time in seconds.
    pub fn beat_to_time_seconds(&self, _measure: u32, _fraction: f64) -> f64 {
        // TODO: Full timing calculation in Phase 1
        0.0
    }

    /// Converts absolute time in seconds to the corresponding measure and beat fraction.
    pub fn time_to_beat(&self, _time_seconds: f64) -> (u32, f64) {
        // TODO: Full timing calculation in Phase 1
        (0, 0.0)
    }

    /// Returns the initial BPM of the chart.
    pub fn initial_bpm(&self) -> f64 {
        self.initial_bpm
    }
}

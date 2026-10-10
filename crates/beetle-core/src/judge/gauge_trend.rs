//! The gauge over a play, sampled at a fixed interval of song time for the
//! result screen's trend graph.
//!
//! The buffer is reserved when the song starts (`new`), so sampling never
//! allocates. The interval is set from the song length so that a song of any
//! length fits in `GAUGE_TREND_MAX_POINTS` points. Times are audio clock
//! seconds (INV-1), so the graph lines up with the notes.

/// Most points a trend keeps.
pub const GAUGE_TREND_MAX_POINTS: usize = 512;

/// One sample: the song time and the gauge (0 ~ 100) at that time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GaugePoint {
    pub time: f64,
    pub gauge: f64,
}

#[derive(Debug, Clone)]
pub struct GaugeTrend {
    points: Vec<GaugePoint>,
    /// Length of the song the graph spans (its right edge), in seconds.
    span: f64,
    interval: f64,
    next_at: f64,
    max_points: usize,
    /// Song time at which the stage failed (Hard / Hazard gauge reached 0).
    failed_at: Option<f64>,
}

impl Default for GaugeTrend {
    fn default() -> Self {
        Self::new(0.0)
    }
}

impl GaugeTrend {
    /// An empty trend for a song of `song_seconds`, with room for
    /// `GAUGE_TREND_MAX_POINTS` samples.
    pub fn new(song_seconds: f64) -> Self {
        Self::with_limit(song_seconds, GAUGE_TREND_MAX_POINTS)
    }

    /// As `new`, with another point limit (at least 2).
    pub fn with_limit(song_seconds: f64, max_points: usize) -> Self {
        let max_points = max_points.max(2);
        let span = song_seconds.max(0.0);
        // One slot is left spare for the failure point.
        let slots = (max_points - 1) as f64;
        Self {
            points: Vec::with_capacity(max_points),
            span,
            interval: (span / slots).max(0.05),
            next_at: 0.0,
            max_points,
            failed_at: None,
        }
    }

    /// Records the gauge at song time `time`, when a sample is due. `failed`
    /// is the stage's failed flag; the first time it is set, its moment is
    /// kept whatever the interval says. Times must not go backwards (the audio
    /// clock does not); an earlier time is ignored.
    pub fn sample(&mut self, time: f64, gauge: f64, failed: bool) {
        if self.points.last().is_some_and(|last| time < last.time) {
            return;
        }
        if failed && self.failed_at.is_none() {
            self.failed_at = Some(time);
            if self.points.len() < self.max_points {
                self.points.push(GaugePoint { time, gauge });
            }
            return;
        }
        // The stage is over once it failed: nothing after the failure point.
        if self.failed_at.is_some() {
            return;
        }
        // The last slot is kept for the failure point.
        if time >= self.next_at && self.points.len() + 1 < self.max_points {
            self.points.push(GaugePoint { time, gauge });
            self.next_at = time + self.interval;
        }
    }

    pub fn points(&self) -> &[GaugePoint] {
        &self.points
    }

    /// Song time at which the stage failed, if it did.
    pub fn failed_at(&self) -> Option<f64> {
        self.failed_at
    }

    /// Length of the song the graph spans, in seconds.
    pub fn span(&self) -> f64 {
        self.span
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_stays_within_the_limit_for_any_length() {
        for seconds in [0.0, 1.0, 90.0, 600.0, 7200.0] {
            let mut trend = GaugeTrend::new(seconds);
            let mut t = 0.0;
            // Sample far more often than the interval, past the song end.
            while t < seconds + 30.0 {
                trend.sample(t, 50.0, false);
                t += 0.01;
            }
            assert!(
                trend.points().len() < GAUGE_TREND_MAX_POINTS,
                "{seconds} s gave {} points",
                trend.points().len()
            );
            assert!(trend.points().len() >= 2, "{seconds} s");
        }
    }

    #[test]
    fn buffer_is_reserved_up_front() {
        let trend = GaugeTrend::new(240.0);
        let cap = trend.points.capacity();
        let mut trend = trend;
        for i in 0..10_000 {
            trend.sample(i as f64 * 0.02, 50.0, false);
        }
        assert_eq!(trend.points.capacity(), cap, "sampling must not reallocate");
    }

    #[test]
    fn timestamps_are_monotone_and_spaced_by_the_interval() {
        let mut trend = GaugeTrend::new(120.0);
        let mut t = 0.0;
        while t < 130.0 {
            trend.sample(t, 60.0, false);
            t += 0.016;
        }
        let pts = trend.points();
        assert!(pts.len() > 100);
        assert!(pts.windows(2).all(|w| w[0].time < w[1].time));
        let interval = 120.0 / (GAUGE_TREND_MAX_POINTS - 1) as f64;
        assert!(pts
            .windows(2)
            .all(|w| w[1].time - w[0].time >= interval - 1e-9));
    }

    #[test]
    fn a_backwards_time_is_ignored() {
        let mut trend = GaugeTrend::new(60.0);
        trend.sample(5.0, 40.0, false);
        trend.sample(3.0, 10.0, false);
        trend.sample(7.0, 42.0, false);
        let times: Vec<f64> = trend.points().iter().map(|p| p.time).collect();
        assert_eq!(times, vec![5.0, 7.0]);
    }

    #[test]
    fn failure_is_recorded_at_its_own_time_once() {
        let mut trend = GaugeTrend::new(100.0);
        trend.sample(0.0, 100.0, false);
        trend.sample(10.0, 30.0, false);
        // The stage fails between two samples: its moment is kept, not the
        // next interval.
        trend.sample(10.3, 0.0, true);
        trend.sample(10.4, 0.0, true);
        trend.sample(60.0, 0.0, true);
        assert_eq!(trend.failed_at(), Some(10.3));
        let last = *trend.points().last().unwrap();
        assert_eq!(
            last,
            GaugePoint {
                time: 10.3,
                gauge: 0.0
            }
        );
        assert_eq!(trend.points().len(), 3);
    }

    #[test]
    fn a_failure_at_the_limit_still_fits() {
        let mut trend = GaugeTrend::with_limit(10.0, 4);
        for i in 0..100 {
            trend.sample(i as f64 * 0.1, 50.0, false);
        }
        assert!(trend.points().len() <= 3);
        trend.sample(9.95, 0.0, true);
        assert_eq!(trend.failed_at(), Some(9.95));
        assert!(trend.points().len() <= 4);
        assert_eq!(trend.points().last().unwrap().time, 9.95);
    }
}

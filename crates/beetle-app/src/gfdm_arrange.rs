//! The GFDM arrangement option (B29, `docs/plans/2026-11-10-gfdm-keyboard-arrange-plan.md`
//! 2.6): with it on, playing an original GuitarFreaks & DrumMania chart opens
//! the same chart arranged for a keyboard (`cs_disc::gfdm_arrange`) instead.
//! The song list stays as it is; only the chart that opens changes.
//!
//! The arranged chart's metadata (its identity, key mode and counts) comes
//! from its own bytes, made on a worker thread while the load runs
//! (`spawn_metadata`), so its records and replays are its own.

use std::sync::mpsc::{channel, Receiver};

use beetle_core::{disc_address, PlayMode, SongAddress, SongMetadata};
use cs_disc::gfdm_arrange::{Arrange, LANE_COUNTS, MAX_MIX, MAX_NOISE, MAX_RUN};
use cs_disc::gfdm_bms::ChartRef;

/// The option as chosen: whether it is on, and the arrangement (its lane
/// count is the one to use when on; the features stay when it is off).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArrangeSetting {
    pub on: bool,
    pub arrange: Arrange,
}

impl Default for ArrangeSetting {
    fn default() -> Self {
        Self {
            on: false,
            arrange: Arrange {
                lanes: 6,
                pitch: true,
                home: true,
                stick: true,
                bal: true,
                hand: true,
                vary: true,
                edge: true,
                ..Default::default()
            },
        }
    }
}

/// Shortest same-lane gaps the row cycles through; `None` the difficulty's.
pub const JACK_STEPS: [Option<u16>; 7] = [
    None,
    Some(100),
    Some(125),
    Some(150),
    Some(180),
    Some(220),
    Some(250),
];
/// Thinning gaps the row cycles through; `None` keeps every note.
pub const THIN_STEPS: [Option<u16>; 6] =
    [None, Some(80), Some(100), Some(120), Some(150), Some(200)];
/// The largest seed the row steps to.
pub const MAX_SEED: u64 = 9999;

impl ArrangeSetting {
    /// `config.dat`'s `gfdm_arrange` value: the arrangement (`6k+pitch+...`).
    pub fn config_value(&self) -> String {
        self.arrange.key()
    }

    /// The lane count row: off, 4K, 6K, 8K, wrapping.
    pub fn step_lanes(&mut self, forward: bool) {
        let mut order: Vec<Option<u8>> = vec![None];
        order.extend(LANE_COUNTS.iter().map(|&n| Some(n)));
        let cur = self.on.then_some(self.arrange.lanes);
        let i = order.iter().position(|&v| v == cur).unwrap_or(0);
        let n = order.len();
        let next = order[if forward {
            (i + 1) % n
        } else {
            (i + n - 1) % n
        }];
        match next {
            Some(lanes) => {
                self.on = true;
                self.arrange.lanes = lanes;
            }
            None => self.on = false,
        }
    }

    pub fn step_jack(&mut self, forward: bool) {
        self.arrange.jack_ms =
            crate::options_table::cycle(&JACK_STEPS, self.arrange.jack_ms, forward);
    }

    /// The one-hand run row: 1 (plain `hand`) up to `MAX_RUN`, wrapping.
    pub fn step_run(&mut self, forward: bool) {
        let cur = self.arrange.run.max(1);
        let next = if forward {
            if cur >= MAX_RUN {
                1
            } else {
                cur + 1
            }
        } else if cur <= 1 {
            MAX_RUN
        } else {
            cur - 1
        };
        self.arrange.run = if next < 2 { 0 } else { next };
    }

    pub fn step_thin(&mut self, forward: bool) {
        self.arrange.thin_ms =
            crate::options_table::cycle(&THIN_STEPS, self.arrange.thin_ms, forward);
    }

    pub fn step_mix(&mut self, forward: bool) {
        let m = self.arrange.mix;
        self.arrange.mix = if forward {
            (m + 1).min(MAX_MIX)
        } else {
            m.saturating_sub(1)
        };
    }

    pub fn step_noise(&mut self, forward: bool) {
        let r = self.arrange.noise;
        self.arrange.noise = if forward {
            (r + 1).min(MAX_NOISE)
        } else {
            r.saturating_sub(1)
        };
    }

    pub fn step_seed(&mut self, forward: bool) {
        let s = self.arrange.seed;
        self.arrange.seed = if forward {
            (s + 1).min(MAX_SEED)
        } else {
            s.saturating_sub(1)
        };
    }

    /// Where `song`'s chart opens with the option: the address of the
    /// arranged chart and its key mode, `None` when it opens as it is (the
    /// option is off, or the song is not an original drum, guitar or bass
    /// chart of a GFDM disc).
    pub fn target(&self, song: &SongMetadata) -> Option<(String, PlayMode)> {
        if !self.on {
            return None;
        }
        let SongAddress::Disc { image, chart } = SongAddress::parse(&song.file_path) else {
            return None;
        };
        let (id, chart) = ChartRef::parse(chart)?;
        if chart.arrange.is_some() {
            return None;
        }
        let arranged = chart.arranged(self.arrange)?;
        let mode = match self.arrange.lanes {
            4 => PlayMode::Keys4,
            8 => PlayMode::Keys8,
            _ => PlayMode::Keys6,
        };
        Some((disc_address(image, &arranged.key(id)), mode))
    }
}

/// `song` as the arranged chart at `address` will be, until its own
/// metadata arrives (`spawn_metadata`): the lane shows the right key mode
/// while the load runs.
pub fn provisional(song: &SongMetadata, address: String, mode: PlayMode) -> SongMetadata {
    SongMetadata {
        file_path: address,
        play_mode: mode,
        ..song.clone()
    }
}

/// Reads the chart at `address` on a worker thread and sends its metadata
/// (`None` when it does not read).
pub fn spawn_metadata(address: String) -> Receiver<Option<SongMetadata>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let meta = crate::loader::read_chart_bytes(&address)
            .ok()
            .and_then(|bytes| SongMetadata::from_bytes(&address, &bytes));
        let _ = tx.send(meta);
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(path: &str) -> SongMetadata {
        SongMetadata::from_bytes(path, b"#WAV01 a.wav\n#00111:01\n").unwrap()
    }

    #[test]
    fn only_an_original_gfdm_chart_is_arranged() {
        let mut s = ArrangeSetting::default();
        let gf = song("disc:D:/x.iso::gfdm/404/gf3");
        assert_eq!(s.target(&gf), None);
        s.on = true;
        let (address, mode) = s.target(&gf).unwrap();
        assert_eq!(
            address,
            "disc:D:/x.iso::gfdm/404/gf3@6k+pitch+bal+hand+vary"
        );
        assert_eq!(mode, PlayMode::Keys6);
        s.arrange.lanes = 4;
        let dm = song("disc:E:::gfdm/404/dm1");
        assert_eq!(
            s.target(&dm).unwrap(),
            (
                "disc:E:::gfdm/404/dm1@4k+home+stick+bal+hand+vary".to_string(),
                PlayMode::Keys4
            )
        );
        for other in [
            "disc:D:/x.iso::gfdm/404/open3",
            "disc:D:/x.iso::gfdm/404/gf3@8k",
            "disc:D:/x.iso::017/sp_normal",
            "D:/BMS/x.bms",
        ] {
            assert_eq!(s.target(&song(other)), None, "{other}");
        }
    }

    #[test]
    fn the_lane_row_cycles_through_off() {
        let mut s = ArrangeSetting::default();
        let mut seen = Vec::new();
        for _ in 0..4 {
            s.step_lanes(true);
            seen.push(s.on.then_some(s.arrange.lanes));
        }
        assert_eq!(seen, vec![Some(4), Some(6), Some(8), None]);
        s.step_lanes(false);
        assert_eq!((s.on, s.arrange.lanes), (true, 8));
    }

    #[test]
    fn the_thin_and_mix_rows_step_within_their_range() {
        let mut s = ArrangeSetting::default();
        s.step_thin(true);
        assert_eq!(s.arrange.thin_ms, Some(80));
        s.step_thin(false);
        s.step_thin(false);
        assert_eq!(s.arrange.thin_ms, Some(200));
        s.step_mix(false);
        assert_eq!(s.arrange.mix, 0);
        for _ in 0..20 {
            s.step_mix(true);
        }
        assert_eq!(s.arrange.mix, MAX_MIX);
        assert_eq!(
            s.config_value(),
            "6k+pitch+home+stick+bal+hand+vary+edge+d200+mix9"
        );
    }
}

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
use cs_disc::gfdm_arrange::{Arrange, Preset, LANE_COUNTS, MAX_MIX, MAX_NOISE, MAX_RUN};
use cs_disc::gfdm_bms::ChartRef;

/// Which features an arranged chart gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetChoice {
    /// Chosen for each chart (`cs_disc::gfdm_arrange::auto`).
    Auto,
    Named(Preset),
    /// The features of `ArrangeSetting::arrange`, set row by row.
    Custom,
}

impl PresetChoice {
    pub const ORDER: [PresetChoice; 6] = [
        PresetChoice::Auto,
        PresetChoice::Named(Preset::Flow),
        PresetChoice::Named(Preset::Alternate),
        PresetChoice::Named(Preset::Easy),
        PresetChoice::Named(Preset::Session),
        PresetChoice::Custom,
    ];

    /// `config.dat`'s `gfdm_arrange_preset` value.
    pub fn name(self) -> &'static str {
        match self {
            PresetChoice::Auto => "auto",
            PresetChoice::Named(p) => p.name(),
            PresetChoice::Custom => "custom",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ORDER.into_iter().find(|p| p.name() == name)
    }
}

/// The option as chosen: whether it is on, the preset, and the arrangement
/// (its lane count is the one to use when on; its features are the custom
/// ones, and stay when another preset is chosen or the option is off).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArrangeSetting {
    pub on: bool,
    pub preset: PresetChoice,
    pub arrange: Arrange,
    /// The feature rows show in the play options panel (not saved).
    pub advanced: bool,
}

impl Default for ArrangeSetting {
    fn default() -> Self {
        Self {
            on: false,
            preset: PresetChoice::Auto,
            advanced: false,
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

    /// The arrangement a chart opens with: the preset's at the chosen lane
    /// count, or the custom features.
    pub fn effective(&self) -> Arrange {
        let lanes = self.arrange.lanes;
        match self.preset {
            PresetChoice::Auto => Arrange {
                lanes,
                auto: true,
                ..Default::default()
            },
            PresetChoice::Named(p) => p.arrange(lanes),
            PresetChoice::Custom => self.arrange,
        }
    }

    /// The features the rows show: the preset's (for the automatic one, what
    /// it starts from before it looks at the chart), or the custom ones.
    pub fn shown(&self) -> Arrange {
        match self.preset {
            PresetChoice::Auto => Preset::Flow.arrange(self.arrange.lanes),
            _ => self.effective(),
        }
    }

    /// The custom features, to change one: a preset's features are copied
    /// into them first and the choice becomes `Custom`.
    pub fn edit(&mut self) -> &mut Arrange {
        if self.preset != PresetChoice::Custom {
            self.arrange = self.shown();
            self.preset = PresetChoice::Custom;
        }
        &mut self.arrange
    }

    /// The preset row, wrapping.
    pub fn step_preset(&mut self, forward: bool) {
        self.preset = crate::options_table::cycle(&PresetChoice::ORDER, self.preset, forward);
    }

    pub fn step_jack(&mut self, forward: bool) {
        let a = self.edit();
        a.jack_ms = crate::options_table::cycle(&JACK_STEPS, a.jack_ms, forward);
    }

    /// The one-hand run row: 1 (plain `hand`) up to `MAX_RUN`, wrapping.
    pub fn step_run(&mut self, forward: bool) {
        let cur = self.shown().run.max(1);
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
        self.edit().run = if next < 2 { 0 } else { next };
    }

    pub fn step_thin(&mut self, forward: bool) {
        let a = self.edit();
        a.thin_ms = crate::options_table::cycle(&THIN_STEPS, a.thin_ms, forward);
    }

    pub fn step_mix(&mut self, forward: bool) {
        let a = self.edit();
        let m = a.mix;
        a.mix = if forward {
            (m + 1).min(MAX_MIX)
        } else {
            m.saturating_sub(1)
        };
    }

    pub fn step_noise(&mut self, forward: bool) {
        let a = self.edit();
        let r = a.noise;
        a.noise = if forward {
            (r + 1).min(MAX_NOISE)
        } else {
            r.saturating_sub(1)
        };
    }

    pub fn step_seed(&mut self, forward: bool) {
        let a = self.edit();
        let s = a.seed;
        a.seed = if forward {
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
        let arranged = chart.arranged(self.effective())?;
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
        assert_eq!(address, "disc:D:/x.iso::gfdm/404/gf3@6k+auto");
        s.preset = PresetChoice::Named(Preset::Flow);
        assert_eq!(
            s.target(&gf).unwrap().0,
            "disc:D:/x.iso::gfdm/404/gf3@6k+pitch+bal+hand+vary+split+jj+fig+motif+acc+run2"
        );
        s.preset = PresetChoice::Custom;
        let (address, _) = s.target(&gf).unwrap();
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
    fn changing_a_feature_makes_the_preset_custom() {
        let mut s = ArrangeSetting::default();
        assert_eq!(s.preset, PresetChoice::Auto);
        // The rows show what the automatic preset starts from.
        assert!(s.shown().split && s.shown().jj);
        s.step_preset(true);
        assert_eq!(s.preset, PresetChoice::Named(Preset::Flow));
        s.step_thin(true);
        assert_eq!(s.preset, PresetChoice::Custom);
        // The custom features are the preset's with the one change.
        assert_eq!(
            s.arrange,
            Arrange {
                thin_ms: Some(80),
                ..Preset::Flow.arrange(6)
            }
        );
        // Another preset leaves the custom features as they are.
        s.step_preset(true);
        assert_eq!(s.preset, PresetChoice::Auto);
        assert_eq!(s.arrange.thin_ms, Some(80));
        for p in PresetChoice::ORDER {
            assert_eq!(PresetChoice::parse(p.name()), Some(p));
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
        let mut s = ArrangeSetting {
            preset: PresetChoice::Custom,
            ..Default::default()
        };
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

//! The PS2 beatmania IIDX discs by product code (the boot executable's name),
//! and how each lays out its charts.
//!
//! Codes and chart layouts from iidx-ps2tools (`plugins/*.py` `is_match`,
//! `charttool/plugins/iidx*cs.py`). A disc whose code is listed but not
//! `playable` is recognised (so the player can say what it is) but not read:
//! its song table, file tables and sounds have not been ported yet.

use crate::ply::ChartFormat;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Title {
    /// The boot executable (`SLPM_552.21`).
    pub code: &'static str,
    pub name: &'static str,
    /// The chart layout.
    pub charts: ChartFormat,
    /// Whether this crate reads the disc's songs.
    pub playable: bool,
}

const fn title(code: &'static str, name: &'static str, charts: ChartFormat) -> Title {
    Title {
        code,
        name,
        charts,
        playable: false,
    }
}

/// Every PS2 IIDX disc iidx-ps2tools knows, oldest first, then the pop'n
/// music discs this crate reads.
pub const IIDX_PS2: [Title; 19] = [
    title(
        "SLPM_650.06",
        "beatmania IIDX 3rd style",
        ChartFormat::Ps2NoHeader,
    ),
    title(
        "SLPM_650.26",
        "beatmania IIDX 4th style",
        ChartFormat::Ps2NoHeader,
    ),
    title(
        "SLPM_650.49",
        "beatmania IIDX 5th style",
        ChartFormat::Ps2Short,
    ),
    title(
        "SLPM_651.56",
        "beatmania IIDX 6th style",
        ChartFormat::Ps2Short,
    ),
    title(
        "SLPM_655.93",
        "beatmania IIDX 7th style",
        ChartFormat::Ps2Long,
    ),
    Title {
        playable: true,
        ..title(
            "SLPM_657.68",
            "beatmania IIDX 8th style",
            ChartFormat::Ps2Long,
        )
    },
    Title {
        playable: true,
        ..title(
            "SLPM_659.46",
            "beatmania IIDX 9th style",
            ChartFormat::Ps2Long,
        )
    },
    Title {
        playable: true,
        ..title(
            "SLPM_661.80",
            "beatmania IIDX 10th style",
            ChartFormat::Ps2Long,
        )
    },
    Title {
        playable: true,
        ..title(
            "SLPM_664.26",
            "beatmania IIDX 11 IIDX RED",
            ChartFormat::Ps2Long,
        )
    },
    Title {
        playable: true,
        ..title(
            "SLPM_666.21",
            "beatmania IIDX 12 HAPPY SKY",
            ChartFormat::Ps2Long,
        )
    },
    Title {
        playable: true,
        ..title(
            "SLPM_668.28",
            "beatmania IIDX 13 DistorteD",
            ChartFormat::Ps2Long,
        )
    },
    Title {
        playable: true,
        ..title(
            "SLPM_669.95",
            "beatmania IIDX 14 GOLD",
            ChartFormat::Ps2Long,
        )
    },
    Title {
        playable: true,
        ..title(
            "SLPM_551.17",
            "beatmania IIDX 15 DJ TROOPERS",
            ChartFormat::Ps2Long,
        )
    },
    Title {
        playable: true,
        ..title(
            "SLPM_552.21",
            "beatmania IIDX 16 EMPRESS",
            ChartFormat::Ps2Long,
        )
    },
    Title {
        playable: true,
        ..title(
            "SLPM_552.22",
            "beatmania IIDX 16 PREMIUM BEST",
            ChartFormat::Ps2Long,
        )
    },
    // The 9th style chart code reads it.
    title("SLUS_212.39", "beatmania (US)", ChartFormat::Ps2Long),
    Title {
        playable: true,
        ..title("SLPM_663.14", "pop'n music 12 いろは", ChartFormat::Popn)
    },
    Title {
        playable: true,
        ..title(
            "SLPM_665.33",
            "pop'n music 13 カーニバル",
            ChartFormat::Popn,
        )
    },
    Title {
        playable: true,
        ..title("SLPM_667.42", "pop'n music 14 FEVER!", ChartFormat::Popn)
    },
];

/// The title a boot executable name belongs to, in any case.
pub fn find(code: &str) -> Option<&'static Title> {
    IIDX_PS2.iter().find(|t| t.code.eq_ignore_ascii_case(code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empress_is_found_by_its_code_and_plays() {
        let t = find("slpm_552.21").unwrap();
        assert_eq!(t.name, "beatmania IIDX 16 EMPRESS");
        assert_eq!(t.charts, ChartFormat::Ps2Long);
        assert!(t.playable);
    }

    #[test]
    fn older_discs_are_known_but_not_read() {
        let t = find("SLPM_655.93").unwrap();
        assert!(!t.playable);
        let t = find("SLPM_650.06").unwrap();
        assert_eq!(t.charts, ChartFormat::Ps2NoHeader);
        assert!(!t.playable);
        assert!(find("SLPM_000.00").is_none());
    }

    #[test]
    fn every_playable_title_is_a_game_the_crate_reads() {
        for t in IIDX_PS2.iter().filter(|t| t.playable) {
            assert!(
                crate::Edition::from_executable(t.code).is_some(),
                "{}",
                t.code
            );
        }
    }

    #[test]
    fn codes_are_unique() {
        for (i, a) in IIDX_PS2.iter().enumerate() {
            assert!(
                IIDX_PS2[i + 1..].iter().all(|b| b.code != a.code),
                "{}",
                a.code
            );
        }
    }
}

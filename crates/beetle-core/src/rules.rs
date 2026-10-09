//! Play rules that change what a chart's judgments mean. For now that is the
//! long note rule; the judge window and gauge rules join it here later.

/// How long notes are judged.
///
/// - `Ln`: one judgment per long note, at the head. The tail is not judged;
///   holding through it is free, and letting go well before it breaks the
///   note.
/// - `Cn` (charge note): two judgments, the head's press and the tail's
///   release, each against the usual windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum LnRule {
    #[default]
    Ln,
    Cn,
}

impl LnRule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ln => "LN",
            Self::Cn => "CN",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Ln, Self::Cn]
            .into_iter()
            .find(|r| r.as_str() == name)
    }
}

/// The player's long note setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LnOption {
    /// The chart's own `#LNMODE`, or LN when it has none.
    #[default]
    Auto,
    /// LN whatever the chart says.
    Ln,
    /// CN whatever the chart says.
    Cn,
}

impl LnOption {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "AUTO",
            Self::Ln => "LN",
            Self::Cn => "CN",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Auto, Self::Ln, Self::Cn]
            .into_iter()
            .find(|o| o.as_str() == name)
    }
}

/// The rules a play is judged under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Ruleset {
    pub ln: LnRule,
    /// The chart asked for HCN (`#LNMODE 3`), which is not implemented: it is
    /// played as CN, and this tells the screens to say so.
    pub hcn_requested: bool,
}

impl Ruleset {
    pub const LN: Self = Self {
        ln: LnRule::Ln,
        hcn_requested: false,
    };
    pub const CN: Self = Self {
        ln: LnRule::Cn,
        hcn_requested: false,
    };

    /// The rules for a chart with the given `#LNMODE` (1 LN, 2 CN, 3 HCN) under
    /// the player's setting. A forced LN or CN wins over the chart; `Auto`
    /// follows the chart and falls back to LN.
    pub fn resolve(chart_ln_mode: Option<u32>, option: LnOption) -> Self {
        match option {
            LnOption::Ln => Self::LN,
            LnOption::Cn => Self::CN,
            LnOption::Auto => match chart_ln_mode {
                Some(2) => Self::CN,
                // HCN is not played yet; CN is its nearest neighbor.
                Some(3) => Self {
                    ln: LnRule::Cn,
                    hcn_requested: true,
                },
                _ => Self::LN,
            },
        }
    }

    /// Label for the options display: `LN`, `CN`, or `CN (HCN)`.
    pub fn label(self) -> String {
        if self.hcn_requested {
            format!("{} (HCN)", self.ln.as_str())
        } else {
            self.ln.as_str().to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_follows_the_chart_and_defaults_to_ln() {
        for (mode, expected) in [
            (None, Ruleset::LN),
            (Some(1), Ruleset::LN),
            (Some(2), Ruleset::CN),
            (Some(9), Ruleset::LN),
        ] {
            assert_eq!(Ruleset::resolve(mode, LnOption::Auto), expected, "{mode:?}");
        }
    }

    #[test]
    fn hcn_opens_as_cn_and_says_so() {
        let hcn = Ruleset::resolve(Some(3), LnOption::Auto);
        assert_eq!(hcn.ln, LnRule::Cn);
        assert!(hcn.hcn_requested);
        assert_eq!(hcn.label(), "CN (HCN)");
        assert_eq!(Ruleset::CN.label(), "CN");
    }

    #[test]
    fn a_forced_option_wins_over_the_chart() {
        for mode in [None, Some(1), Some(2), Some(3)] {
            assert_eq!(Ruleset::resolve(mode, LnOption::Ln), Ruleset::LN);
            assert_eq!(
                Ruleset::resolve(mode, LnOption::Cn),
                Ruleset::CN,
                "no HCN note when CN is forced"
            );
        }
    }

    #[test]
    fn names_round_trip() {
        for rule in [LnRule::Ln, LnRule::Cn] {
            assert_eq!(LnRule::from_name(rule.as_str()), Some(rule));
        }
        for option in [LnOption::Auto, LnOption::Ln, LnOption::Cn] {
            assert_eq!(LnOption::from_name(option.as_str()), Some(option));
        }
        assert_eq!(LnRule::from_name("HCN"), None);
        assert_eq!(Ruleset::default(), Ruleset::LN);
    }
}

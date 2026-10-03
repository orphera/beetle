//! PULSE palette and shared text styles for every Canvas-based screen.
//! Screens take colors from here only (no inline `ColorRgba::new`), so the
//! whole UI stays one visual language (docs/plans/2026-10-04-d3d11-ui-rebuild.md P4).
//!
//! Two color vocabularies never mix: UI chrome uses the cyan/magenta duotone;
//! judgement, rank, clear-lamp and difficulty colors are reserved for data.

use crate::skin::ColorRgba;
use crate::text::TextStyle;
use beetle_core::JudgeGrade;

const fn rgb(hex: u32) -> ColorRgba {
    ColorRgba::new((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, 255)
}

// Surfaces (darkest → lightest)
pub const BG: ColorRgba = rgb(0x05060a);
pub const SURF1: ColorRgba = rgb(0x0b0e16);
pub const SURF2: ColorRgba = rgb(0x11151f);
pub const SURF3: ColorRgba = rgb(0x171c29);
pub const LINE: ColorRgba = rgb(0x232a3c);
pub const PLAYFIELD: ColorRgba = rgb(0x020306);

// Chrome accents
pub const CYAN: ColorRgba = rgb(0x00e5ff);
pub const MAGENTA: ColorRgba = rgb(0xff2d6a);

// Text
pub const TEXT: ColorRgba = rgb(0xf2f4fa);
pub const MUTED: ColorRgba = rgb(0x7b8299);
pub const MUTED2: ColorRgba = rgb(0x4c5368);
/// Text drawn on top of a bright CYAN fill.
pub const ON_ACCENT: ColorRgba = BG;

// Data colors
pub const GOLD: ColorRgba = rgb(0xffd400);
pub const ORANGE: ColorRgba = rgb(0xff9a3c);
pub const GREEN: ColorRgba = rgb(0x3ce07a);
pub const BLUE: ColorRgba = rgb(0x4aa8ff);
pub const PURPLE: ColorRgba = rgb(0xc06bff);
pub const RED: ColorRgba = rgb(0xff3b3b);
pub const GRAY: ColorRgba = rgb(0x8a90a3);
pub const FAST: ColorRgba = rgb(0x4ad6ff);
pub const SLOW: ColorRgba = rgb(0xff8a4a);

// Lane / note colors (IIDX layout: white, blue, scratch)
pub const NOTE_WHITE: ColorRgba = rgb(0xf2f4fa);
pub const NOTE_BLUE: ColorRgba = BLUE;
pub const NOTE_SCRATCH: ColorRgba = MAGENTA;

pub const BLACK: ColorRgba = ColorRgba::new(0, 0, 0, 255);
pub const WHITE: ColorRgba = ColorRgba::new(255, 255, 255, 255);

pub fn judge_color(grade: JudgeGrade) -> ColorRgba {
    match grade {
        JudgeGrade::PerfectGreat => GOLD,
        JudgeGrade::Great => ORANGE,
        JudgeGrade::Good => GREEN,
        JudgeGrade::Bad => PURPLE,
        JudgeGrade::Poor => MAGENTA,
        JudgeGrade::Miss => GRAY,
    }
}

pub fn judge_label(grade: JudgeGrade) -> &'static str {
    match grade {
        JudgeGrade::PerfectGreat => "PGREAT",
        JudgeGrade::Great => "GREAT",
        JudgeGrade::Good => "GOOD",
        JudgeGrade::Bad => "BAD",
        JudgeGrade::Poor => "POOR",
        JudgeGrade::Miss => "MISS",
    }
}

/// DJ rank from accuracy percent (EX score / max), IIDX ninths.
pub fn rank(accuracy_percent: f64) -> (&'static str, ColorRgba) {
    let ninths = accuracy_percent / 100.0 * 9.0;
    match ninths {
        n if n >= 8.0 => ("AAA", GOLD),
        n if n >= 7.0 => ("AA", TEXT),
        n if n >= 6.0 => ("A", GREEN),
        n if n >= 5.0 => ("B", BLUE),
        n if n >= 4.0 => ("C", PURPLE),
        n if n >= 3.0 => ("D", ORANGE),
        _ => ("F", RED),
    }
}

/// Difficulty tier name and color for a play level.
pub fn level_tier(level: u32) -> (&'static str, ColorRgba) {
    match level {
        1..=4 => ("NORMAL", GREEN),
        5..=8 => ("HYPER", BLUE),
        9..=10 => ("ANOTHER", ORANGE),
        11..=12 => ("INSANE", MAGENTA),
        _ => ("OVERJOY", PURPLE),
    }
}

/// Small all-caps caption ("EX SCORE", "BPM") above a value.
pub fn caption(size: f32, scale: f32) -> TextStyle {
    TextStyle::new(size * scale)
        .bold()
        .tracking(1.5 * scale)
        .color(MUTED2)
}

/// `1234567` → `"1,234,567"`.
pub fn thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Seconds → `m:ss`.
pub fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(thousands(1234567), "1,234,567");
        assert_eq!(clock(125.9), "2:05");
    }

    #[test]
    fn rank_boundaries() {
        assert_eq!(rank(100.0).0, "AAA");
        assert_eq!(rank(8.0 / 9.0 * 100.0).0, "AAA");
        assert_eq!(rank(88.0).0, "AA");
        assert_eq!(rank(0.0).0, "F");
    }
}

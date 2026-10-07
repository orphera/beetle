//! `#RANDOM` / `#IF` conditional resolver.
//!
//! A BMS file may contain control flow that picks between alternative chart
//! fragments at load time. This turns such a file into the single concrete
//! chart one roll produces, as plain BMS text with no control lines left, so
//! the parser itself never sees a branch.
//!
//! Supported: `#RANDOM n`, `#SETRANDOM n`, `#IF n`, `#ELSEIF n`, `#ELSE`,
//! `#ENDIF`, `#ENDRANDOM`, nested to any depth. Not supported: the
//! `#SWITCH` / `#CASE` / `#SKIP` / `#DEF` / `#ENDSW` family (those lines are
//! left alone).
//!
//! Scoping: a `#RANDOM` written inside an `#IF` block belongs to that block
//! and is dropped at its `#ELSEIF` / `#ELSE` / `#ENDIF`, so a later sibling
//! `#IF` still tests the outer roll.

/// Keywords whose presence means the file needs resolving.
const CONTROL_KEYS: [&str; 7] = [
    "RANDOM",
    "SETRANDOM",
    "IF",
    "ELSEIF",
    "ELSE",
    "ENDIF",
    "ENDRANDOM",
];

#[derive(Clone, Copy, PartialEq)]
enum Control {
    Random(u32),
    SetRandom(u32),
    If(u32),
    ElseIf(u32),
    Else,
    EndIf,
    EndRandom,
}

fn parse_control(line: &str) -> Option<Control> {
    let content = line.trim().strip_prefix('#')?.trim_start();
    let mut parts = content.splitn(2, |c: char| c.is_whitespace() || c == ':');
    let key = parts.next()?;
    if !CONTROL_KEYS.iter().any(|k| key.eq_ignore_ascii_case(k)) {
        return None;
    }
    let arg = parts
        .next()
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|v| v.parse::<u32>().ok());
    let n = arg.unwrap_or(0);
    Some(match key.to_ascii_uppercase().as_str() {
        "RANDOM" => Control::Random(n),
        "SETRANDOM" => Control::SetRandom(n),
        "IF" => Control::If(n),
        "ELSEIF" => Control::ElseIf(n),
        "ELSE" => Control::Else,
        "ENDIF" => Control::EndIf,
        _ => Control::EndRandom,
    })
}

/// Small deterministic generator (SplitMix64); the same seed always rolls the same.
struct Rolls(u64);

impl Rolls {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

/// One open `#IF` group.
struct Frame {
    /// Whether the text around the group is being kept.
    parent_active: bool,
    /// Whether some branch of the group has already been taken.
    taken: bool,
    /// Length of the random stack when the group opened.
    randoms_len: usize,
}

/// True when the text has any `#RANDOM` / `#IF` style control line.
pub fn has_control_flow(input: &str) -> bool {
    input.lines().any(|l| parse_control(l).is_some())
}

/// Resolves every conditional in `input` with the rolls `seed` produces.
/// Returns `None` when there is nothing to resolve.
pub fn resolve_random(input: &str, seed: u64) -> Option<String> {
    if !has_control_flow(input) {
        return None;
    }

    let mut rolls = Rolls(seed);
    let mut out = String::with_capacity(input.len());
    let mut randoms: Vec<u32> = Vec::new();
    let mut frames: Vec<Frame> = Vec::new();
    let mut active = true;

    for line in input.split_inclusive('\n') {
        let Some(control) = parse_control(line) else {
            if active {
                out.push_str(line);
            }
            continue;
        };
        match control {
            // Only a kept `#RANDOM` rolls; a dropped one just keeps the stack balanced.
            Control::Random(n) => {
                let roll = if active {
                    1 + (rolls.next() % u64::from(n.max(1))) as u32
                } else {
                    0
                };
                randoms.push(roll);
            }
            Control::SetRandom(n) => randoms.push(if active { n } else { 0 }),
            Control::EndRandom => {
                randoms.pop();
            }
            Control::If(n) => {
                let matches = randoms.last() == Some(&n);
                frames.push(Frame {
                    parent_active: active,
                    taken: active && matches,
                    randoms_len: randoms.len(),
                });
                active = active && matches;
            }
            Control::ElseIf(n) => {
                if let Some(frame) = frames.last_mut() {
                    randoms.truncate(frame.randoms_len);
                    let matches = randoms.last() == Some(&n);
                    active = frame.parent_active && !frame.taken && matches;
                    frame.taken |= active;
                }
            }
            Control::Else => {
                if let Some(frame) = frames.last_mut() {
                    randoms.truncate(frame.randoms_len);
                    active = frame.parent_active && !frame.taken;
                    frame.taken |= active;
                }
            }
            Control::EndIf => {
                if let Some(frame) = frames.pop() {
                    randoms.truncate(frame.randoms_len);
                    active = frame.parent_active;
                }
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept(input: &str, seed: u64) -> Vec<String> {
        resolve_random(input, seed)
            .unwrap_or_else(|| input.to_string())
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    }

    #[test]
    fn text_without_control_flow_is_left_alone() {
        assert_eq!(resolve_random("#TITLE T\n#00111:01\n", 1), None);
    }

    #[test]
    fn keeps_only_the_branch_that_matches() {
        let src = "A\n#RANDOM 1\n#IF 1\nB\n#ENDIF\n#IF 2\nC\n#ENDIF\nD\n";
        assert_eq!(kept(src, 7), ["A", "B", "D"]);
    }

    #[test]
    fn every_seed_takes_exactly_one_branch_and_both_get_taken() {
        let src = "#RANDOM 2\n#IF 1\nONE\n#ENDIF\n#IF 2\nTWO\n#ENDIF\n";
        let mut seen = std::collections::HashSet::new();
        for seed in 0..64 {
            let lines = kept(src, seed);
            assert_eq!(lines.len(), 1, "seed {seed}: {lines:?}");
            seen.insert(lines[0].clone());
        }
        assert_eq!(seen.len(), 2);
    }

    #[test]
    fn same_seed_gives_the_same_chart() {
        let src = "#RANDOM 9\n#IF 1\nA\n#ELSEIF 2\nB\n#ELSEIF 3\nC\n#ELSE\nD\n#ENDIF\n";
        assert_eq!(resolve_random(src, 42), resolve_random(src, 42));
    }

    #[test]
    fn elseif_and_else_take_at_most_one_branch() {
        let src = "#SETRANDOM 3\n#IF 1\nA\n#ELSEIF 2\nB\n#ELSEIF 3\nC\n#ELSEIF 3\nX\n#ELSE\nD\n#ENDIF\n";
        assert_eq!(kept(src, 1), ["C"]);
        let src = "#SETRANDOM 9\n#IF 1\nA\n#ELSEIF 2\nB\n#ELSE\nD\n#ENDIF\n";
        assert_eq!(kept(src, 1), ["D"]);
    }

    #[test]
    fn nested_blocks_need_every_enclosing_branch_to_match() {
        let src = "#SETRANDOM 2\n#IF 1\nNO\n#SETRANDOM 1\n#IF 1\nNO2\n#ENDIF\n#ENDIF\n#IF 2\nYES\n#SETRANDOM 5\n#IF 5\nINNER\n#ENDIF\n#ENDIF\nEND\n";
        assert_eq!(kept(src, 1), ["YES", "INNER", "END"]);
    }

    #[test]
    fn a_random_inside_a_block_does_not_leak_to_the_next_sibling_if() {
        // The inner roll must not replace the outer value of 2 for `#IF 2`.
        let src = "#SETRANDOM 2\n#IF 1\n#SETRANDOM 1\n#ENDIF\n#IF 2\nOUTER\n#ENDIF\n";
        assert_eq!(kept(src, 1), ["OUTER"]);
    }

    #[test]
    fn endrandom_restores_the_previous_roll() {
        let src = "#SETRANDOM 1\n#SETRANDOM 2\n#IF 2\nA\n#ENDIF\n#ENDRANDOM\n#IF 1\nB\n#ENDIF\n";
        assert_eq!(kept(src, 1), ["A", "B"]);
    }

    #[test]
    fn if_without_a_roll_keeps_nothing_and_stray_endif_is_ignored() {
        assert_eq!(kept("A\n#IF 1\nB\n#ENDIF\n#ENDIF\nC\n", 1), ["A", "C"]);
    }

    #[test]
    fn keywords_are_case_insensitive_and_indented_lines_count() {
        let src = "#random 1\n  #if 1\nA\n  #endif\n";
        assert_eq!(kept(src, 1), ["A"]);
    }

    #[test]
    fn control_lines_never_reach_the_output() {
        let out = resolve_random("#RANDOM 2\n#IF 1\nA\n#ELSE\nB\n#ENDIF\n#ENDRANDOM\n", 3).unwrap();
        assert!(!out.contains('#'), "{out}");
    }

    #[test]
    fn deep_nesting_resolves_without_recursion() {
        let mut src = String::new();
        for _ in 0..5000 {
            src.push_str("#SETRANDOM 1\n#IF 1\n");
        }
        src.push_str("DEEP\n");
        for _ in 0..5000 {
            src.push_str("#ENDIF\n");
        }
        assert_eq!(kept(&src, 1), ["DEEP"]);
    }
}

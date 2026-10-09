//! BMS compatibility corpus survey.
//!
//! Walks a folder of charts, runs the real parser and a raw-text census over
//! each, and reports what the corpus uses that Beetle does not handle yet
//! (header commands, channels, `#RANDOM` blocks, mines, LN flavours, parse
//! failures, unplayable results). Read-only: no file is modified.
//!
//! ```text
//! cargo run --release -p beetle-core --example corpus -- <dir> [--all] [--show TAG] [--tsv out.tsv]
//! ```
//!
//! Tags are `H:<HEADER>` (xx = two base-36 digits), `C:<channel>`, `F:<flag>`.
//! `--show TAG` lists every file carrying that tag.

use beetle_core::{decode_bms_text, parse_bms, TimingModel};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const CHART_EXTS: [&str; 4] = ["bms", "bme", "bml", "pms"];

/// Header commands `parse_bms` understands (snapshot; keep in sync with `parse_header_line`).
const SUPPORTED_HEADERS: [&str; 23] = [
    "PLAYER",
    "DIFFICULTY",
    "LNTYPE",
    "TITLE",
    "SUBTITLE",
    "ARTIST",
    "SUBARTIST",
    "GENRE",
    "BPM",
    "PLAYLEVEL",
    "RANK",
    "TOTAL",
    "VOLWAV",
    "STAGEFILE",
    "BANNER",
    "PREVIEW",
    "LNOBJ",
    "LNMODE",
    "WAVxx",
    "BMPxx",
    "BGAxx",
    "BPMxx",
    "STOPxx",
];

/// Header families that carry a two-digit base-36 id suffix.
const ID_PREFIXES: [&str; 12] = [
    "WAV", "BMP", "BGA", "BPM", "STOP", "EXBPM", "EXWAV", "SCROLL", "TEXT", "ARGB", "SEEK",
    "EXRANK",
];

/// Control-flow commands that make the chart's contents depend on a random roll.
const RANDOM_KEYS: [&str; 10] = [
    "RANDOM",
    "IF",
    "ELSEIF",
    "ELSE",
    "ENDIF",
    "ENDRANDOM",
    "SETRANDOM",
    "SWITCH",
    "CASE",
    "ENDSW",
];

fn is_supported_channel(ch: &str) -> bool {
    is_mine_channel(ch)
        || matches!(
            ch,
            "01" | "02"
                | "03"
                | "04"
                | "06"
                | "07"
                | "08"
                | "09"
                | "11"
                | "12"
                | "13"
                | "14"
                | "15"
                | "16"
                | "18"
                | "19"
                | "21"
                | "22"
                | "23"
                | "24"
                | "25"
                | "26"
                | "28"
                | "29"
                | "31"
                | "32"
                | "33"
                | "34"
                | "35"
                | "36"
                | "38"
                | "39"
                | "41"
                | "42"
                | "43"
                | "44"
                | "45"
                | "46"
                | "48"
                | "49"
                | "51"
                | "52"
                | "53"
                | "54"
                | "55"
                | "56"
                | "58"
                | "59"
                | "61"
                | "62"
                | "63"
                | "64"
                | "65"
                | "66"
                | "68"
                | "69"
        )
}

fn normalize_header_key(key: &str) -> String {
    let up = key.to_ascii_uppercase();
    for prefix in ID_PREFIXES {
        if let Some(rest) = up.strip_prefix(prefix) {
            if rest.len() == 2 && rest.bytes().all(|b| b.is_ascii_alphanumeric()) {
                return format!("{prefix}xx");
            }
        }
    }
    up
}

/// What a single file contributes to the survey.
#[derive(Default)]
struct FileReport {
    tags: BTreeSet<String>,
    parse_error: Option<String>,
}

fn survey_file(path: &Path) -> FileReport {
    let mut report = FileReport::default();
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            report.parse_error = Some(format!("read: {e}"));
            return report;
        }
    };
    let text = decode_bms_text(&bytes);

    let is_pms = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pms"));
    report.tags.insert(format!("F:ext.{}", ext_lower(path)));

    // Raw census, independent of what the parser chooses to look at.
    let mut lntype = 1u32;
    for line in text.lines() {
        let line = line.trim();
        let Some(content) = line.strip_prefix('#') else {
            continue;
        };
        let content = content.trim_start();
        if content.is_empty() {
            continue;
        }
        if beetle_core::bms::is_measure_line(content) {
            let tag = content.split([':', ' ']).next().unwrap_or("");
            let data = content[tag.len()..].trim_start_matches([':', ' ']).trim();
            if data.bytes().any(|b| b != b'0') && tag.len() >= 5 {
                let ch = tag[3..5].to_ascii_uppercase();
                if is_mine_channel(&ch) {
                    report.tags.insert("F:mine".into());
                }
                if is_ln_channel(&ch) {
                    report.tags.insert("F:ln.channel".into());
                }
                report.tags.insert(format!("C:{ch}"));
            }
            continue;
        }
        let mut parts = content.splitn(2, |c: char| c.is_whitespace() || c == ':');
        let key = parts.next().unwrap_or("");
        let val = parts.next().unwrap_or("").trim();
        let norm = normalize_header_key(key);
        if RANDOM_KEYS.contains(&norm.as_str()) {
            report.tags.insert("F:random".into());
        }
        if norm == "LNTYPE" {
            report.tags.insert(format!("F:lntype.{val}"));
            lntype = val.parse().unwrap_or(1);
        }
        report.tags.insert(format!("H:{norm}"));
    }

    // `#LNTYPE` only matters when the chart really uses 5x/6x LN channels; a
    // `#LNTYPE 2` header on a pure `#LNOBJ` chart changes nothing.
    if report.tags.contains("F:ln.channel") && lntype != 1 {
        report.tags.insert(format!("F:ln.channel-lntype{lntype}"));
    }

    // Does the real pipeline get a playable chart out of it?
    match parse_bms(&text) {
        Err(e) => report.parse_error = Some(e.to_string()),
        Ok(chart) => {
            if chart.notes.is_empty() {
                report.tags.insert("F:no-playable-notes".into());
            }
            let (lo, hi) = chart.bpm_range();
            if lo < 10.0 || hi > 1000.0 {
                report.tags.insert("F:extreme-bpm".into());
            }
            let secs = TimingModel::from_chart(&chart).total_duration_seconds(&chart);
            if !secs.is_finite() || secs <= 0.0 {
                report.tags.insert("F:bad-duration".into());
            }
            report.tags.insert(format!(
                "F:mode.{}",
                chart.detect_play_mode_with_hint(is_pms).as_str()
            ));
            if chart.header.ln_obj.is_some() {
                report.tags.insert("F:ln.lnobj".into());
            }
        }
    }
    report
}

fn is_ln_channel(ch: &str) -> bool {
    let b = ch.as_bytes();
    b.len() == 2 && (b[0] == b'5' || b[0] == b'6') && (b'1'..=b'9').contains(&b[1])
}

fn is_mine_channel(ch: &str) -> bool {
    let b = ch.as_bytes();
    b.len() == 2 && (b[0] == b'D' || b[0] == b'E') && (b'1'..=b'9').contains(&b[1])
}

fn ext_lower(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn collect_charts(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_charts(&path, out);
        } else if CHART_EXTS.contains(&ext_lower(&path).as_str()) {
            out.push(path);
        }
    }
}

/// True when `tag` names something Beetle's parser already handles.
fn is_supported(tag: &str) -> bool {
    match tag.split_once(':') {
        Some(("H", key)) => {
            SUPPORTED_HEADERS.contains(&key)
                || beetle_core::resolver::has_control_flow(&format!("#{key} 1"))
        }
        Some(("C", ch)) => is_supported_channel(ch),
        _ => true,
    }
}

/// Flags that mean "the chart is parsed but not played the way it should be".
fn is_problem_flag(tag: &str) -> bool {
    matches!(
        tag,
        "F:no-playable-notes" | "F:extreme-bpm" | "F:bad-duration"
    ) || tag.starts_with("F:ln.channel-lntype")
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut dir = None;
    let (mut show_all, mut show, mut tsv) = (false, None, None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--all" => show_all = true,
            "--show" => show = args.next(),
            "--tsv" => tsv = args.next(),
            _ => dir = Some(PathBuf::from(a)),
        }
    }
    let Some(dir) = dir else {
        eprintln!("usage: corpus <dir> [--all] [--show TAG] [--tsv out.tsv]");
        std::process::exit(2);
    };

    let mut files = Vec::new();
    collect_charts(&dir, &mut files);
    files.sort();

    let mut by_tag: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut errors: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut tsv_rows = String::new();
    for (i, path) in files.iter().enumerate() {
        let r = survey_file(path);
        for tag in &r.tags {
            by_tag.entry(tag.clone()).or_default().push(i);
        }
        if let Some(e) = &r.parse_error {
            errors.entry(e.clone()).or_default().push(i);
        }
        if tsv.is_some() {
            let tags: Vec<&str> = r.tags.iter().map(String::as_str).collect();
            tsv_rows.push_str(&format!(
                "{}\t{}\t{}\n",
                path.display(),
                r.parse_error.as_deref().unwrap_or(""),
                tags.join(",")
            ));
        }
    }

    if let Some(out) = tsv {
        fs::write(&out, tsv_rows).expect("write tsv");
        println!("wrote {out}");
    }
    if let Some(tag) = show {
        for &i in by_tag.get(&tag).map(Vec::as_slice).unwrap_or(&[]) {
            println!("{}", files[i].display());
        }
        return;
    }

    let total = files.len();
    let pct = |n: usize| {
        if total == 0 {
            0.0
        } else {
            100.0 * n as f64 / total as f64
        }
    };
    let example = |idxs: &[usize]| {
        idxs.iter()
            .take(2)
            .map(|&i| {
                files[i]
                    .strip_prefix(&dir)
                    .unwrap_or(&files[i])
                    .display()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("  |  ")
    };

    println!("charts scanned: {total}");
    let failed: usize = errors.values().map(Vec::len).sum();
    println!("parse failures: {failed} ({:.1}%)", pct(failed));
    for (msg, idxs) in &errors {
        println!("  {:>6}  {msg}   e.g. {}", idxs.len(), example(idxs));
    }

    println!("\n== play-mode / extension / LN-style distribution ==");
    for (tag, idxs) in by_tag.iter().filter(|(t, _)| {
        ["F:mode.", "F:ext.", "F:ln.", "F:mine", "F:random"]
            .iter()
            .any(|p| t.starts_with(p))
            && !t.starts_with("F:ln.channel-")
    }) {
        println!("  {:>6} ({:>5.1}%)  {tag}", idxs.len(), pct(idxs.len()));
    }

    println!("\n== parsed, but not played correctly (likely gaps) ==");
    for (tag, idxs) in by_tag.iter().filter(|(t, _)| is_problem_flag(t)) {
        println!(
            "  {:>6} ({:>5.1}%)  {tag}   e.g. {}",
            idxs.len(),
            pct(idxs.len()),
            example(idxs)
        );
    }

    println!("\n== used by the corpus, ignored by the parser ==");
    let mut unsupported: Vec<_> = by_tag
        .iter()
        .filter(|(t, _)| (t.starts_with("H:") || t.starts_with("C:")) && !is_supported(t))
        .collect();
    unsupported.sort_by_key(|(_, idxs)| std::cmp::Reverse(idxs.len()));
    for (tag, idxs) in unsupported
        .iter()
        .take(if show_all { usize::MAX } else { 25 })
    {
        println!(
            "  {:>6} ({:>5.1}%)  {tag}   e.g. {}",
            idxs.len(),
            pct(idxs.len()),
            example(idxs)
        );
    }
    if !show_all && unsupported.len() > 25 {
        println!("  ... {} more (use --all)", unsupported.len() - 25);
    }
}

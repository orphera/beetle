//! The title a folder of charts shows, from the charts' own titles.
//!
//! The charts of one folder are one song, so their titles share a prefix: the
//! song's name, and then whatever tells the charts apart (`- HYPER`, `[ANOTHER]`,
//! `-A-`). The shown title is that prefix, cut back to a word and without the
//! separators and open brackets it leaves behind. Brackets that belong to the
//! name (`(Remix)`) stay when every chart shares them.

/// Characters that separate a name from the chart tag after it.
const SEPARATORS: &[char] = &['-', '_', '/', ':', '~', '(', '[', '{'];

/// The title the charts of one folder show. Titles are compared with their
/// whitespace collapsed. With one distinct title, or when the titles share
/// nothing usable, the most common title (the first on a tie) is shown as it is.
/// An empty result means no title at all.
pub fn shared_title(titles: &[&str]) -> String {
    let collapsed: Vec<String> = titles
        .iter()
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty())
        .collect();
    let fallback = most_common(&collapsed);
    let [first, rest @ ..] = collapsed.as_slice() else {
        return fallback;
    };
    if rest.is_empty() {
        return fallback;
    }

    let first: Vec<char> = first.chars().collect();
    let mut len = first.len();
    for other in rest {
        let same = other
            .chars()
            .zip(&first)
            .take_while(|(a, b)| a == *b)
            .count();
        len = len.min(same);
    }
    // A cut inside a word (`Abc` and `Abd` share `Ab`) gives no name: back off to
    // the last separator before it.
    if len < first.len() && first[len].is_alphanumeric() {
        while len > 0 && first[len - 1].is_alphanumeric() {
            len -= 1;
        }
    }
    let shared: String = first[..len].iter().collect();
    let name = shared.trim_end_matches(|c: char| c.is_whitespace() || SEPARATORS.contains(&c));
    if name.is_empty() {
        fallback
    } else {
        name.to_string()
    }
}

/// Chart names a title may carry in its bracket or after its dash, as BMS
/// charts spell them. Longer names come first, so `BLACK ANOTHER` wins over
/// `ANOTHER`.
const CHART_NAMES: &[&str] = &[
    "BLACK ANOTHER",
    "BEGINNER",
    "LEGGENDARIA",
    "ANOTHER",
    "NORMAL",
    "HYPER",
    "INSANE",
    "MASTER",
    "EASY",
    "EX",
];

/// The chart name a title ends with, from its trailing `(...)` or `[...]` or
/// the part after its last ` -`: `Song [14K Hyper]` → `HYPER`, `Song - another`
/// → `ANOTHER`. `None` when neither part ends with a known chart name.
pub fn chart_name(title: &str) -> Option<&'static str> {
    let title = title.trim();
    let mut parts: Vec<&str> = Vec::new();
    for (open, close) in [('(', ')'), ('[', ']')] {
        if let Some(inner) = title
            .strip_suffix(close)
            .and_then(|t| t.rfind(open).map(|pos| &t[pos + open.len_utf8()..]))
        {
            parts.push(inner);
        }
    }
    if let Some(pos) = title.rfind(" -") {
        parts.push(&title[pos + 2..]);
    }
    parts.iter().find_map(|part| name_ending(part))
}

/// The known chart name `part` ends with, as whole words, ignoring case.
fn name_ending(part: &str) -> Option<&'static str> {
    let words: Vec<String> = part
        .split_whitespace()
        .map(|w| w.to_ascii_uppercase())
        .collect();
    CHART_NAMES.iter().copied().find(|name| {
        let name_words: Vec<&str> = name.split(' ').collect();
        words.len() >= name_words.len()
            && words[words.len() - name_words.len()..]
                .iter()
                .zip(&name_words)
                .all(|(w, n)| w == n)
    })
}

/// The value that occurs most often (the first one on a tie), or `""`.
fn most_common(values: &[String]) -> String {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for value in values {
        match counts.iter_mut().find(|(v, _)| *v == value.as_str()) {
            Some((_, n)) => *n += 1,
            None => counts.push((value.as_str(), 1)),
        }
    }
    let best = counts.iter().map(|(_, n)| *n).max().unwrap_or(0);
    counts
        .iter()
        .find(|(_, n)| *n == best)
        .map_or(String::new(), |(v, _)| v.to_string())
}

#[cfg(test)]
mod tests {
    use super::shared_title;

    #[test]
    fn a_difficulty_after_a_dash_is_cut_off() {
        assert_eq!(shared_title(&["Song - HYPER", "Song - ANOTHER"]), "Song");
        assert_eq!(shared_title(&["Song -A-", "Song -B-"]), "Song");
        assert_eq!(
            shared_title(&["Aleph-0 - HYPER", "Aleph-0 - ANOTHER"]),
            "Aleph-0"
        );
    }

    #[test]
    fn a_bracket_tag_is_cut_off_but_a_shared_one_stays() {
        assert_eq!(
            shared_title(&["Aleph-0 [ANOTHER]", "Aleph-0 [HYPER]"]),
            "Aleph-0"
        );
        assert_eq!(
            shared_title(&["Song (Remix) [SP]", "Song (Remix) [DP]"]),
            "Song (Remix)"
        );
        assert_eq!(shared_title(&["Snow (Remix)", "Snow (Official)"]), "Snow");
    }

    #[test]
    fn a_lone_title_keeps_its_brackets() {
        assert_eq!(
            shared_title(&["Conflict (Official)"]),
            "Conflict (Official)"
        );
        assert_eq!(shared_title(&["Song (14K Normal)"]), "Song (14K Normal)");
    }

    #[test]
    fn a_cut_inside_a_word_gives_the_most_common_title() {
        assert_eq!(shared_title(&["Abc", "Abd", "Abd"]), "Abd");
        assert_eq!(shared_title(&["Abc", "Abd"]), "Abc");
    }

    #[test]
    fn a_chart_name_is_read_from_the_bracket_or_the_dash() {
        use super::chart_name;
        assert_eq!(chart_name("Song [14K Hyper]"), Some("HYPER"));
        assert_eq!(chart_name("Song (SP ANOTHER)"), Some("ANOTHER"));
        assert_eq!(chart_name("Song - Black Another"), Some("BLACK ANOTHER"));
        assert_eq!(chart_name("Aleph-0 - normal"), Some("NORMAL"));
        assert_eq!(chart_name("Song (Remix)"), None);
        assert_eq!(chart_name("Song -A-"), None);
    }

    #[test]
    fn no_titles_give_an_empty_title() {
        assert_eq!(shared_title(&[]), "");
        assert_eq!(shared_title(&["  "]), "");
    }
}

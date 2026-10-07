//! Escaping for text fields in the flat tab-separated cache files, so a tab or
//! newline inside a title can never be mistaken for a field or line break.

/// Backslash, tab, newline and carriage return become `\\`, `\t`, `\n`, `\r`.
pub(crate) fn escape_field(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

/// Inverse of `escape_field`. Read one character at a time so that an escaped
/// backslash followed by `n` stays a backslash and an `n`, not a newline.
pub(crate) fn unescape_field(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            // Not an escape this file writes: keep both characters as they are.
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_awkward_text() {
        for text in [
            "plain",
            "tab\there",
            "line\nbreak\r\n",
            "back\\slash",
            "looks like an escape: \\n and \\t and \\\\",
            "ends with backslash\\",
            "日本語\tタイトル",
            "",
        ] {
            assert_eq!(unescape_field(&escape_field(text)), text, "{text:?}");
        }
    }

    #[test]
    fn escaped_text_has_no_raw_separators() {
        let escaped = escape_field("a\tb\nc\rd");
        assert!(!escaped.contains(['\t', '\n', '\r']));
    }

    #[test]
    fn a_backslash_before_n_is_not_a_newline() {
        // The older replace-based version turned this into a newline.
        assert_eq!(unescape_field(&escape_field("\\n")), "\\n");
    }
}

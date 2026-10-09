//! `bpm table`: fetching BMS difficulty tables and keeping them in the local
//! cache the player reads.
//!
//! A table is an HTML page that names a `header.json`, which names the data
//! JSON. Everything network- and JSON-shaped lives here; the cache format and
//! the matching to the song list are in `beetle_core::table`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use beetle_core::{md5_from_hex, ChartId, DifficultyTable, TableEntry};
use serde_json::Value;

/// Largest page or JSON document taken from a table server.
pub const MAX_TABLE_BYTES: u64 = 32 * 1024 * 1024;
/// Largest `header.json` or HTML page (they are small; this stops a wrong address from filling memory).
pub const MAX_HEADER_BYTES: u64 = 2 * 1024 * 1024;
/// An `update` of a table fetched more recently than this asks for `--force`.
pub const MIN_UPDATE_INTERVAL_SECS: u64 = 15 * 60;

/// Why a table could not be fetched, read or stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableError(pub String);

impl std::fmt::Display for TableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TableError {}

fn err<T>(message: impl Into<String>) -> Result<T, TableError> {
    Err(TableError(message.into()))
}

/// Gets the bytes at an address, giving up past `max_bytes`.
pub type Getter<'a> = &'a dyn Fn(&str, u64) -> Result<Vec<u8>, String>;

// ---------------------------------------------------------------------------
// Addresses
// ---------------------------------------------------------------------------

/// True for `http://` and `https://` addresses, the only kind a table may use.
fn is_web_url(url: &str) -> bool {
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Resolves `reference` against `base` the way a browser would for the
/// addresses tables use: absolute, protocol-relative, root-relative and
/// relative (with `.` and `..`). Anything that does not end up an `http(s)`
/// address is refused.
pub fn resolve_url(base: &str, reference: &str) -> Result<String, TableError> {
    let reference = reference.trim();
    if reference.is_empty() {
        return err("empty address");
    }
    let resolved = if is_web_url(reference) {
        reference.to_string()
    } else if reference.contains("://")
        || reference.starts_with("javascript:")
        || reference.starts_with("data:")
    {
        return err(format!(
            "only http and https addresses are allowed: {reference}"
        ));
    } else {
        let scheme_end = base.find("://").map_or(0, |i| i + 3);
        let scheme = &base[..scheme_end.saturating_sub(3)];
        let after_scheme = &base[scheme_end..];
        let host_end = after_scheme.find('/').unwrap_or(after_scheme.len());
        let origin = &base[..scheme_end + host_end];
        let path = &after_scheme[host_end..];
        // The base's path without its query or fragment.
        let path = path.split(['?', '#']).next().unwrap_or("");

        if let Some(rest) = reference.strip_prefix("//") {
            format!("{scheme}://{rest}")
        } else if reference.starts_with('/') {
            format!("{origin}{}", normalize_path(reference))
        } else {
            let dir = path.rfind('/').map_or("/", |i| &path[..=i]);
            format!("{origin}{}", normalize_path(&format!("{dir}{reference}")))
        }
    };
    if is_web_url(&resolved) {
        Ok(resolved)
    } else {
        err(format!("not an http or https address: {resolved}"))
    }
}

/// Collapses `.` and `..` segments; a query or fragment is left on the end.
fn normalize_path(path: &str) -> String {
    let (path, tail) = match path.find(['?', '#']) {
        Some(i) => path.split_at(i),
        None => (path, ""),
    };
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "." => {}
            ".." => {
                segments.pop();
            }
            s => segments.push(s),
        }
    }
    let joined = segments.join("/");
    let joined = if joined.starts_with('/') {
        joined
    } else {
        format!("/{joined}")
    };
    format!("{joined}{tail}")
}

// ---------------------------------------------------------------------------
// Reading what the server sends
// ---------------------------------------------------------------------------

/// Text of a response body: UTF-8, with a byte order mark removed.
fn body_text(bytes: &[u8]) -> Result<&str, TableError> {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    std::str::from_utf8(bytes).or_else(|_| err("the response is not UTF-8 text"))
}

/// True when the text is JSON rather than an HTML page.
fn looks_like_json(text: &str) -> bool {
    text.trim_start().starts_with(['{', '['])
}

/// The `content` of `<meta name="bmstable" content="…">`, whatever the
/// attribute order, quote style or letter case.
fn find_bmstable_meta(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(start) = lower[from..].find("<meta") {
        let start = from + start;
        let end = lower[start..].find('>').map_or(lower.len(), |e| start + e);
        let tag = &html[start..end];
        let tag_lower = &lower[start..end];
        if attribute(tag, tag_lower, "name").is_some_and(|n| n.eq_ignore_ascii_case("bmstable")) {
            return attribute(tag, tag_lower, "content");
        }
        from = end.max(start + 5);
    }
    None
}

/// Value of an attribute inside one tag (`name="v"`, `name='v'` or `name=v`).
fn attribute(tag: &str, tag_lower: &str, name: &str) -> Option<String> {
    let mut search = 0;
    while let Some(i) = tag_lower[search..].find(name) {
        let at = search + i;
        search = at + name.len();
        let before_ok = at == 0
            || !tag_lower.as_bytes()[at - 1].is_ascii_alphanumeric()
                && tag_lower.as_bytes()[at - 1] != b'-';
        let rest = tag[at + name.len()..].trim_start();
        let Some(value) = rest.strip_prefix('=') else {
            continue;
        };
        if !before_ok {
            continue;
        }
        let value = value.trim_start();
        return Some(match value.chars().next()? {
            q @ ('"' | '\'') => value[1..].split(q).next()?.to_string(),
            _ => value
                .split(|c: char| c.is_whitespace() || c == '>')
                .next()?
                .to_string(),
        });
    }
    None
}

/// Level names come as strings or numbers.
fn level_text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// 64 hex characters (any case) as a chart id.
fn parse_sha256(text: &str) -> Option<ChartId> {
    ChartId::from_hex(&text.trim().to_ascii_lowercase())
}

/// 32 hex characters (any case) as an MD5.
fn parse_md5(text: &str) -> Option<[u8; 16]> {
    md5_from_hex(&text.trim().to_ascii_lowercase())
}

fn str_field(entry: &Value, key: &str) -> String {
    entry
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Reads a table's data JSON (an array of charts). Entries with neither a
/// usable `md5` nor `sha256` are dropped, as nothing could match them.
fn parse_entries(data: &Value) -> Result<Vec<TableEntry>, TableError> {
    let Some(list) = data.as_array() else {
        return err("the table data is not a list of charts");
    };
    Ok(list
        .iter()
        .filter_map(|entry| {
            let md5 = entry.get("md5").and_then(Value::as_str).and_then(parse_md5);
            let sha256 = entry
                .get("sha256")
                .and_then(Value::as_str)
                .and_then(parse_sha256);
            if md5.is_none() && sha256.is_none() {
                return None;
            }
            Some(TableEntry {
                level: entry.get("level").and_then(level_text).unwrap_or_default(),
                md5,
                sha256,
                title: str_field(entry, "title"),
                artist: str_field(entry, "artist"),
                url: str_field(entry, "url"),
                url_diff: str_field(entry, "url_diff"),
            })
        })
        .collect())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Fetches a table: from its page (`table.html`) or straight from its
/// `header.json`, then the data the header points to.
pub fn fetch_table(get: Getter, address: &str) -> Result<DifficultyTable, TableError> {
    let address = address.trim();
    if !is_web_url(address) {
        return err(format!(
            "only http and https addresses are allowed: {address}"
        ));
    }

    let first = get(address, MAX_HEADER_BYTES)
        .or_else(|e| err(format!("could not fetch {address}: {e}")))?;
    let first_text = body_text(&first)?;
    let (header_url, header_text): (String, String) = if looks_like_json(first_text) {
        (address.to_string(), first_text.to_string())
    } else {
        let Some(content) = find_bmstable_meta(first_text) else {
            return err(format!("{address} has no <meta name=\"bmstable\"> tag, so it is not a difficulty table page"));
        };
        let header_url = resolve_url(address, &content)?;
        let header = get(&header_url, MAX_HEADER_BYTES)
            .or_else(|e| err(format!("could not fetch {header_url}: {e}")))?;
        (header_url, body_text(&header)?.to_string())
    };

    let header: Value = serde_json::from_str(&header_text)
        .or_else(|e| err(format!("{header_url} is not valid JSON: {e}")))?;
    let name = header
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if name.is_empty() {
        return err(format!("{header_url} has no table name"));
    }
    let Some(data_url) = header.get("data_url").and_then(Value::as_str) else {
        return err(format!("{header_url} has no data_url"));
    };
    let data_url = resolve_url(&header_url, data_url)?;
    let level_order: Vec<String> = header
        .get("level_order")
        .and_then(Value::as_array)
        .map(|levels| levels.iter().filter_map(level_text).collect())
        .unwrap_or_default();

    let data_bytes = get(&data_url, MAX_TABLE_BYTES)
        .or_else(|e| err(format!("could not fetch {data_url}: {e}")))?;
    let data: Value = serde_json::from_str(body_text(&data_bytes)?)
        .or_else(|e| err(format!("{data_url} is not valid JSON: {e}")))?;
    let entries = parse_entries(&data)?;
    if entries.is_empty() {
        return err(format!("{data_url} has no charts with an md5 or sha256"));
    }

    Ok(DifficultyTable {
        name,
        symbol: header
            .get("symbol")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string(),
        source: header_url,
        fetched: now_secs(),
        level_order,
        entries,
    })
}

// ---------------------------------------------------------------------------
// The local cache folder
// ---------------------------------------------------------------------------

/// The `tables/` folder the player reads.
pub struct TableStore {
    dir: PathBuf,
}

/// What `TableStore::update` did with one table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    Updated {
        entries: usize,
        previous_entries: usize,
    },
    /// Fetched less than `MIN_UPDATE_INTERVAL_SECS` ago and `--force` was not given.
    TooSoon { minutes_ago: u64 },
}

impl TableStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// File-name-safe form of a table name: lowercase letters and digits, other
    /// runs collapsed to `-`.
    pub fn slug(name: &str) -> String {
        let mut slug = String::new();
        for c in name.chars() {
            if c.is_alphanumeric() {
                slug.extend(c.to_lowercase());
            } else if !slug.ends_with('-') && !slug.is_empty() {
                slug.push('-');
            }
        }
        let slug = slug.trim_end_matches('-').to_string();
        if slug.is_empty() {
            "table".to_string()
        } else {
            slug
        }
    }

    fn path_for(&self, slug: &str) -> PathBuf {
        self.dir.join(format!("{slug}.tbl"))
    }

    /// Every installed table with the file it is in, ordered by file name (the
    /// order the player prioritizes them in). Files that are not tables are skipped.
    pub fn list(&self) -> Vec<(PathBuf, DifficultyTable)> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "tbl"))
            .collect();
        files.sort();
        files
            .into_iter()
            .filter_map(|path| {
                let table = DifficultyTable::parse(&fs::read_to_string(&path).ok()?)?;
                Some((path, table))
            })
            .collect()
    }

    /// The installed table with this name or file name (`satellite`).
    pub fn find(&self, name: &str) -> Option<(PathBuf, DifficultyTable)> {
        let wanted = name.trim().to_lowercase();
        self.list().into_iter().find(|(path, table)| {
            table.name.to_lowercase() == wanted
                || path
                    .file_stem()
                    .is_some_and(|s| s.to_string_lossy().to_lowercase() == wanted)
        })
    }

    /// Writes `table` to the cache through a temporary file, so a failed write
    /// leaves what was there. A table already installed from the same address
    /// is replaced; a different table that would take the same file name is an error.
    pub fn install(&self, table: &DifficultyTable) -> Result<PathBuf, TableError> {
        if table.entries.is_empty() {
            return err("refusing to install a table with no charts");
        }
        let path = self.path_for(&Self::slug(&table.name));
        if let Some(existing) = fs::read_to_string(&path)
            .ok()
            .and_then(|t| DifficultyTable::parse(&t))
        {
            if existing.source != table.source {
                return err(format!(
                    "a table named '{}' is already installed from {}; remove it first with `bpm table remove`",
                    existing.name, existing.source
                ));
            }
        }
        self.write_atomic(&path, &table.serialize())?;
        Ok(path)
    }

    fn write_atomic(&self, path: &Path, text: &str) -> Result<(), TableError> {
        fs::create_dir_all(&self.dir)
            .or_else(|e| err(format!("cannot create {}: {e}", self.dir.display())))?;
        let temp = path.with_extension("tbl.tmp");
        fs::write(&temp, text).or_else(|e| err(format!("cannot write {}: {e}", temp.display())))?;
        fs::rename(&temp, path).or_else(|e| {
            let _ = fs::remove_file(&temp);
            err(format!("cannot replace {}: {e}", path.display()))
        })
    }

    /// Deletes an installed table's cache file; its name on success.
    pub fn remove(&self, name: &str) -> Result<String, TableError> {
        let Some((path, table)) = self.find(name) else {
            return err(format!("no installed table named '{name}'"));
        };
        fs::remove_file(&path)
            .or_else(|e| err(format!("cannot delete {}: {e}", path.display())))?;
        Ok(table.name)
    }

    /// Fetches an installed table again from where it came from. A failed
    /// fetch leaves the old copy as it was.
    pub fn update(
        &self,
        get: Getter,
        installed: &(PathBuf, DifficultyTable),
        force: bool,
        now: u64,
    ) -> Result<UpdateOutcome, TableError> {
        let (path, old) = installed;
        let age = now.saturating_sub(old.fetched);
        if !force && old.fetched != 0 && age < MIN_UPDATE_INTERVAL_SECS {
            return Ok(UpdateOutcome::TooSoon {
                minutes_ago: age / 60,
            });
        }
        let fresh = fetch_table(get, &old.source)?;
        self.write_atomic(path, &fresh.serialize())?;
        Ok(UpdateOutcome::Updated {
            entries: fresh.entries.len(),
            previous_entries: old.entries.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bpm_table_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// A fake web: address -> body.
    struct Web(HashMap<String, Vec<u8>>);

    impl Web {
        fn new(pages: &[(&str, &str)]) -> Self {
            Self(
                pages
                    .iter()
                    .map(|(u, b)| (u.to_string(), b.as_bytes().to_vec()))
                    .collect(),
            )
        }

        fn get(&self) -> impl Fn(&str, u64) -> Result<Vec<u8>, String> + '_ {
            |url, max| {
                let body = self.0.get(url).ok_or_else(|| format!("404 {url}"))?;
                if body.len() as u64 > max {
                    return Err(format!("{url} is larger than {max} bytes"));
                }
                Ok(body.clone())
            }
        }
    }

    const SHA_A: &str = "9d91220250962fc64006b2eb5f3bd642361c721e904708701ea43a562f80d89b";
    const MD5_A: &str = "176c2b2db4efd66cf186caae7923d477";
    const SHA_B: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    const MD5_B: &str = "900150983cd24fb0d6963f7d28e17f72";

    fn data_json() -> String {
        format!(
            r#"[
              {{"md5":"{MD5_A}","sha256":"{SHA_A}","level":"3","title":"日本語 \"A\"","artist":"X","url":"https://e.invalid/a","url_diff":""}},
              {{"md5":"{}","level":12,"title":"B (md5 only)"}},
              {{"sha256":"{}","level":"?","title":"C (sha256 only, upper case hex)"}},
              {{"title":"no hashes at all","level":"1"}},
              {{"md5":"not-hex","level":"1","title":"bad md5 and nothing else"}}
            ]"#,
            MD5_B.to_uppercase(),
            SHA_B.to_uppercase(),
        )
    }

    fn page(meta: &str) -> String {
        format!("<html><head>{meta}<title>t</title></head><body>table</body></html>")
    }

    #[test]
    fn urls_resolve_like_a_browser() {
        let base = "https://example.invalid/sl/table.html?x=1";
        assert_eq!(
            resolve_url(base, "header.json").unwrap(),
            "https://example.invalid/sl/header.json"
        );
        assert_eq!(
            resolve_url(base, "./header.json").unwrap(),
            "https://example.invalid/sl/header.json"
        );
        assert_eq!(
            resolve_url(base, "../data/score.json").unwrap(),
            "https://example.invalid/data/score.json"
        );
        assert_eq!(
            resolve_url(base, "/root/score.json").unwrap(),
            "https://example.invalid/root/score.json"
        );
        assert_eq!(
            resolve_url(base, "//cdn.example.invalid/s.json").unwrap(),
            "https://cdn.example.invalid/s.json"
        );
        assert_eq!(
            resolve_url(base, "http://other.invalid/s.json").unwrap(),
            "http://other.invalid/s.json"
        );
        assert_eq!(
            resolve_url(base, "score.json?v=2").unwrap(),
            "https://example.invalid/sl/score.json?v=2"
        );
        assert_eq!(
            resolve_url("https://example.invalid", "a.json").unwrap(),
            "https://example.invalid/a.json"
        );
        // Never above the root, never another scheme.
        assert_eq!(
            resolve_url(base, "../../../x.json").unwrap(),
            "https://example.invalid/x.json"
        );
        for bad in [
            "file:///etc/passwd",
            "ftp://example.invalid/x",
            "javascript:alert(1)",
            "data:text/plain,hi",
            "",
        ] {
            assert!(resolve_url(base, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_meta_tag_is_found_in_any_form() {
        for html in [
            page(r#"<meta name="bmstable" content="header.json" />"#),
            page(r#"<meta content="header.json" name="bmstable">"#),
            page(r#"<META NAME='bmstable' CONTENT='header.json'>"#),
            page(r#"<meta charset="utf-8"><meta name=bmstable content=header.json>"#),
            page("<meta\n name=\"bmstable\"\n content=\"header.json\"\n>"),
        ] {
            assert_eq!(
                find_bmstable_meta(&html).as_deref(),
                Some("header.json"),
                "{html}"
            );
        }
        assert_eq!(
            find_bmstable_meta(&page(
                r#"<meta name="viewport" content="width=device-width">"#
            )),
            None
        );
        assert_eq!(find_bmstable_meta("no html here"), None);
    }

    #[test]
    fn a_table_is_fetched_from_its_page() {
        let web = Web::new(&[
            (
                "https://t.invalid/sl/table.html",
                &page(r#"<meta name="bmstable" content="header.json">"#),
            ),
            (
                "https://t.invalid/sl/header.json",
                r#"{"name":"Satellite","symbol":"sl","data_url":"score.json","level_order":["?","3",12]}"#,
            ),
            ("https://t.invalid/sl/score.json", &data_json()),
        ]);
        let table = fetch_table(&web.get(), "https://t.invalid/sl/table.html").unwrap();

        assert_eq!(
            (table.name.as_str(), table.symbol.as_str()),
            ("Satellite", "sl")
        );
        assert_eq!(
            table.source, "https://t.invalid/sl/header.json",
            "updates re-fetch the header"
        );
        assert_eq!(
            table.level_order,
            ["?", "3", "12"],
            "numbers in level_order become text"
        );
        // Three of the five entries can be matched.
        assert_eq!(table.entries.len(), 3);
        assert_eq!(table.entries[0].title, "日本語 \"A\"");
        assert_eq!(table.entries[0].sha256, ChartId::from_hex(SHA_A));
        assert_eq!(table.entries[0].md5, md5_from_hex(MD5_A));
        assert_eq!(
            (table.entries[1].level.as_str(), table.entries[1].sha256),
            ("12", None),
            "numeric level, md5 only"
        );
        assert_eq!(
            table.entries[1].md5,
            md5_from_hex(MD5_B),
            "upper case hex is normalized"
        );
        assert_eq!(table.entries[2].md5, None);
        assert_eq!(table.entries[2].sha256, ChartId::from_hex(SHA_B));
        assert!(table.fetched > 0);
    }

    #[test]
    fn a_header_json_address_works_directly_and_a_byte_order_mark_is_ignored() {
        let mut header = b"\xef\xbb\xbf".to_vec();
        header.extend_from_slice(
            br#"{"name":"Direct","symbol":"d","data_url":"https://data.invalid/x.json"}"#,
        );
        let mut web = Web::new(&[("https://data.invalid/x.json", &data_json())]);
        web.0.insert("https://t.invalid/header.json".into(), header);

        let table = fetch_table(&web.get(), "https://t.invalid/header.json").unwrap();
        assert_eq!(table.name, "Direct");
        assert_eq!(table.entries.len(), 3);
    }

    #[test]
    fn bad_tables_are_refused_with_a_reason() {
        let web = Web::new(&[
            ("https://t.invalid/plain.html", "<html>no table here</html>"),
            (
                "https://t.invalid/nameless.json",
                r#"{"data_url":"d.json"}"#,
            ),
            ("https://t.invalid/nodata.json", r#"{"name":"N"}"#),
            (
                "https://t.invalid/badscheme.json",
                r#"{"name":"N","data_url":"file:///etc/passwd"}"#,
            ),
            (
                "https://t.invalid/empty.json",
                r#"{"name":"N","data_url":"empty-data.json"}"#,
            ),
            (
                "https://t.invalid/empty-data.json",
                r#"[{"title":"no hashes"}]"#,
            ),
            (
                "https://t.invalid/object.json",
                r#"{"name":"N","data_url":"object-data.json"}"#,
            ),
            ("https://t.invalid/object-data.json", r#"{"not":"a list"}"#),
            ("https://t.invalid/garbled.json", r#"{"name":"N""#),
        ]);
        for (address, expect) in [
            ("https://t.invalid/plain.html", "bmstable"),
            ("https://t.invalid/nameless.json", "name"),
            ("https://t.invalid/nodata.json", "data_url"),
            ("https://t.invalid/badscheme.json", "http"),
            ("https://t.invalid/empty.json", "no charts"),
            ("https://t.invalid/object.json", "list"),
            ("https://t.invalid/garbled.json", "valid JSON"),
            ("https://t.invalid/missing.json", "404"),
            ("ftp://t.invalid/x", "http"),
            ("file:///etc/hosts", "http"),
        ] {
            let error = fetch_table(&web.get(), address).unwrap_err();
            assert!(error.0.contains(expect), "{address}: {error}");
        }
    }

    #[test]
    fn slugs_are_file_name_safe() {
        assert_eq!(TableStore::slug("Satellite"), "satellite");
        assert_eq!(TableStore::slug("Insane BMS (2nd)"), "insane-bms-2nd");
        assert_eq!(TableStore::slug("発光"), "発光");
        assert_eq!(TableStore::slug("../../etc"), "etc");
        assert_eq!(TableStore::slug("???"), "table");
    }

    fn sample_table(name: &str, source: &str, fetched: u64, entries: usize) -> DifficultyTable {
        DifficultyTable {
            name: name.into(),
            symbol: "s".into(),
            source: source.into(),
            fetched,
            entries: (0..entries)
                .map(|i| TableEntry {
                    level: "1".into(),
                    sha256: Some(ChartId::synthetic(i as u64)),
                    ..TableEntry::default()
                })
                .collect(),
            ..DifficultyTable::default()
        }
    }

    #[test]
    fn install_list_find_and_remove() {
        let dir = temp_dir("store");
        let store = TableStore::new(&dir);
        assert!(store.list().is_empty(), "no folder yet is just no tables");

        store
            .install(&sample_table(
                "Satellite",
                "https://a.invalid/h.json",
                10,
                3,
            ))
            .unwrap();
        store
            .install(&sample_table("Stella", "https://b.invalid/h.json", 20, 2))
            .unwrap();

        let names: Vec<String> = store.list().into_iter().map(|(_, t)| t.name).collect();
        assert_eq!(names, ["Satellite", "Stella"], "ordered by file name");
        assert!(store.find("satellite").is_some());
        assert!(store.find("STELLA").is_some());
        assert!(store.find("nope").is_none());

        // Installing the same table again replaces it; no temp file is left.
        store
            .install(&sample_table(
                "Satellite",
                "https://a.invalid/h.json",
                30,
                5,
            ))
            .unwrap();
        assert_eq!(store.find("satellite").unwrap().1.entries.len(), 5);
        assert!(!dir.join("satellite.tbl.tmp").exists());

        // Another table with the same file name is not allowed to overwrite it.
        let clash = store
            .install(&sample_table(
                "SATELLITE",
                "https://other.invalid/h.json",
                1,
                1,
            ))
            .unwrap_err();
        assert!(clash.0.contains("already installed"), "{clash}");
        assert_eq!(
            store.find("satellite").unwrap().1.entries.len(),
            5,
            "left as it was"
        );

        assert_eq!(store.remove("Stella").unwrap(), "Stella");
        assert!(store.remove("Stella").is_err());
        assert_eq!(store.list().len(), 1);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn nothing_without_charts_is_installed() {
        let dir = temp_dir("empty");
        let store = TableStore::new(&dir);
        assert!(store
            .install(&sample_table("Empty", "https://a.invalid/h.json", 1, 0))
            .is_err());
        assert!(store.list().is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn files_that_are_not_tables_are_skipped_in_the_list() {
        let dir = temp_dir("junk");
        let store = TableStore::new(&dir);
        store
            .install(&sample_table("Real", "https://a.invalid/h.json", 1, 1))
            .unwrap();
        fs::write(dir.join("notes.tbl"), "just some notes").unwrap();
        fs::write(dir.join("readme.txt"), "ignored").unwrap();
        assert_eq!(store.list().len(), 1);
        let _ = fs::remove_dir_all(dir);
    }

    fn web_with_table() -> Web {
        Web::new(&[
            (
                "https://t.invalid/header.json",
                r#"{"name":"Sat","symbol":"sl","data_url":"score.json"}"#,
            ),
            ("https://t.invalid/score.json", &data_json()),
        ])
    }

    #[test]
    fn update_fetches_again_but_not_too_soon() {
        let dir = temp_dir("update");
        let store = TableStore::new(&dir);
        let web = web_with_table();
        let table = fetch_table(&web.get(), "https://t.invalid/header.json").unwrap();
        store.install(&table).unwrap();
        let installed = store.find("sat").unwrap();
        let fetched = installed.1.fetched;

        // Right after fetching: asks for --force.
        let soon = store
            .update(&web.get(), &installed, false, fetched + 60)
            .unwrap();
        assert_eq!(soon, UpdateOutcome::TooSoon { minutes_ago: 1 });
        // Forced, or after the interval: fetched again.
        for (force, now) in [
            (true, fetched + 60),
            (false, fetched + MIN_UPDATE_INTERVAL_SECS),
        ] {
            let outcome = store.update(&web.get(), &installed, force, now).unwrap();
            assert_eq!(
                outcome,
                UpdateOutcome::Updated {
                    entries: 3,
                    previous_entries: 3
                }
            );
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_update_leaves_the_old_table_alone() {
        let dir = temp_dir("failed");
        let store = TableStore::new(&dir);
        let web = web_with_table();
        store
            .install(&fetch_table(&web.get(), "https://t.invalid/header.json").unwrap())
            .unwrap();
        let installed = store.find("sat").unwrap();
        let before = fs::read_to_string(&installed.0).unwrap();

        // The server is gone, and then it serves a table with nothing in it.
        let gone = Web::new(&[]);
        assert!(store
            .update(&gone.get(), &installed, true, u64::MAX)
            .is_err());
        let emptied = Web::new(&[
            (
                "https://t.invalid/header.json",
                r#"{"name":"Sat","data_url":"score.json"}"#,
            ),
            ("https://t.invalid/score.json", "[]"),
        ]);
        assert!(store
            .update(&emptied.get(), &installed, true, u64::MAX)
            .is_err());

        assert_eq!(
            fs::read_to_string(&installed.0).unwrap(),
            before,
            "the cache is untouched"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn oversized_responses_are_refused() {
        let big = "x".repeat(10);
        let web = Web::new(&[("https://t.invalid/big", &big)]);
        assert!(web.get()("https://t.invalid/big", 5).is_err());
        assert!(web.get()("https://t.invalid/big", 50).is_ok());
    }
}

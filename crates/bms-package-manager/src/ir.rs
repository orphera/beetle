//! Reading the download links of a Stella IR chart page.
//!
//! The page for a chart is `ir.stellabms.xyz/charts/<md5>`. Its table has a `곡`
//! row (the body, as an archive the IR links to) and a `차분` row (the difference
//! pack). Only the links are read here. Nothing is downloaded by this module, and
//! the caller checks every downloaded pack against the table entry's hash.

use crate::net::HttpClient;
use beetle_core::md5_to_hex;

const CHART_PAGE_PREFIX: &str = "https://ir.stellabms.xyz/charts/";
/// Largest chart page accepted. The real pages are about 100 KB.
const MAX_PAGE_BYTES: u64 = 2 * 1024 * 1024;

/// The links one chart page lists. Each list is in page order and has no duplicates.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct IrLinks {
    /// Links from the `곡` row: the body archive.
    pub body: Vec<String>,
    /// Links from the `차분` row: the difference pack.
    pub diff: Vec<String>,
}

/// The chart page address for a chart's MD5.
pub fn chart_page_url(md5: &[u8; 16]) -> String {
    format!("{CHART_PAGE_PREFIX}{}", md5_to_hex(md5))
}

/// Downloads the chart page for `md5` and reads its links.
pub fn fetch_chart_links(client: &HttpClient, md5: &[u8; 16]) -> Result<IrLinks, String> {
    let url = chart_page_url(md5);
    let bytes = client
        .get_bytes(&url, MAX_PAGE_BYTES)
        .map_err(|e| format!("cannot read the IR page: {e}"))?;
    parse_chart_page(&String::from_utf8_lossy(&bytes))
}

/// Reads the `곡` and `차분` rows of a chart page.
///
/// A page without the chart-page layout is an error: the IR may have changed, or
/// it may be showing a check page. A chart page with no rows is not an error. It
/// lists no links.
pub fn parse_chart_page(html: &str) -> Result<IrLinks, String> {
    if !html.contains("class=\"archive-root\"") {
        return Err("the IR page is not a chart page (its layout is unknown)".into());
    }
    Ok(IrLinks {
        body: row_links(html, "곡"),
        diff: row_links(html, "차분"),
    })
}

/// The web links in the cells of the row whose header is `<th>{label}</th>`.
fn row_links(html: &str, label: &str) -> Vec<String> {
    let header = format!("<th>{label}</th>");
    let mut out: Vec<String> = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find(&header) {
        let row = &rest[at + header.len()..];
        // The row ends at its closing tag, or at the next row or header if the tag is missing.
        let end = ["</tr>", "<tr", "<th"]
            .iter()
            .filter_map(|tag| row.find(tag))
            .min()
            .unwrap_or(row.len());
        for href in hrefs(&row[..end]) {
            if crate::table_fetch::is_web_url(&href) && !out.contains(&href) {
                out.push(href);
            }
        }
        rest = &row[end..];
    }
    out
}

/// The `href` values of the anchors in `fragment`, with `&amp;` decoded.
fn hrefs(fragment: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = fragment;
    while let Some(at) = rest.find("href=\"") {
        let value = &rest[at + "href=\"".len()..];
        let Some(end) = value.find('"') else {
            break;
        };
        out.push(value[..end].replace("&amp;", "&"));
        rest = &value[end..];
    }
    out
}

/// Splits the links of a chart page into the zip links that are fetched
/// automatically (with their row label) and every other link, which is only shown.
pub fn split_zip_links(links: IrLinks) -> (Vec<(&'static str, String)>, Vec<String>) {
    let mut zips = Vec::new();
    let mut others = Vec::new();
    for (label, found) in [("body", links.body), ("diff", links.diff)] {
        for url in found {
            if crate::table_fetch::pack_extension(&url) == Some("zip") {
                zips.push((label, url));
            } else {
                others.push(url);
            }
        }
    }
    (zips, others)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<div class="archive-root"><table>
<tr><th>곡</th><td colspan="7"><a href="https://web.archive.org/web/2016/junk_qualia.rar" target="_blank">https://web.archive.org/web/2016/junk_qualia.rar</a></td></tr>
<tr><th>차분</th><td colspan="7"><a href="https://bms.hexlataia.xyz/mirror/gnqg-upload/02646.zip?a=1&amp;b=2" target="_blank">x</a></td></tr>
<tr><th>Viewer</th><td colspan="7"><a href="https://bms-score-viewer.pages.dev/view?md5=1ab0" target="_blank">BMS Score Viewer</a></td></tr>
</table></div>"#;

    #[test]
    fn body_and_diff_rows_are_read_separately() {
        let links = parse_chart_page(PAGE).unwrap();
        assert_eq!(
            links.body,
            vec!["https://web.archive.org/web/2016/junk_qualia.rar"]
        );
        assert_eq!(
            links.diff,
            vec!["https://bms.hexlataia.xyz/mirror/gnqg-upload/02646.zip?a=1&b=2"]
        );
    }

    #[test]
    fn the_viewer_row_is_not_a_download() {
        let links = parse_chart_page(PAGE).unwrap();
        assert!(!links
            .body
            .iter()
            .chain(&links.diff)
            .any(|l| l.contains("viewer")));
    }

    #[test]
    fn only_web_links_are_kept_and_duplicates_dropped() {
        let html = r#"<div class="archive-root"><tr><th>차분</th><td>
<a href="javascript:alert(1)">a</a>
<a href="file:///C:/x.zip">b</a>
<a href="https://a.test/x.zip">c</a>
<a href="https://a.test/x.zip">d</a>
</td></tr></div>"#;
        assert_eq!(
            parse_chart_page(html).unwrap().diff,
            vec!["https://a.test/x.zip"]
        );
    }

    #[test]
    fn a_row_without_a_closing_tag_ends_at_the_next_row() {
        let html = r#"<div class="archive-root"><tr><th>곡</th><td><a href="https://a.test/body.zip">a</a></td>
<tr><th>차분</th><td><a href="https://a.test/diff.zip">b</a></td></tr></div>"#;
        let links = parse_chart_page(html).unwrap();
        assert_eq!(links.body, vec!["https://a.test/body.zip"]);
        assert_eq!(links.diff, vec!["https://a.test/diff.zip"]);
    }

    #[test]
    fn a_chart_page_without_rows_lists_nothing() {
        let links = parse_chart_page(r#"<div class="archive-root">no rows</div>"#).unwrap();
        assert_eq!(links, IrLinks::default());
    }

    #[test]
    fn only_zip_links_are_split_off_for_fetching() {
        let links = IrLinks {
            body: vec!["https://a.test/body.rar".into()],
            diff: vec![
                "https://a.test/diff.zip".into(),
                "https://a.test/x.7z".into(),
            ],
        };
        let (zips, others) = split_zip_links(links);
        assert_eq!(zips, vec![("diff", "https://a.test/diff.zip".to_string())]);
        assert_eq!(
            others,
            vec!["https://a.test/body.rar", "https://a.test/x.7z"]
        );
    }

    #[test]
    fn a_page_of_another_layout_is_an_error() {
        assert!(parse_chart_page("<html>Just a moment...</html>").is_err());
    }

    #[test]
    fn the_chart_page_address_uses_the_md5() {
        let md5 = beetle_core::md5_from_hex("1ab0ea1a6d3d28f7b4ce2ad366edc38d").unwrap();
        assert_eq!(
            chart_page_url(&md5),
            "https://ir.stellabms.xyz/charts/1ab0ea1a6d3d28f7b4ce2ad366edc38d"
        );
    }
}

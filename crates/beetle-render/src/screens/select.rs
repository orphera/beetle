//! Song select screen on the `Ui` (Canvas + TextEngine + generated Skin),
//! plus the two modals that open on top of it (play options, quit).
//! Replaces `song_select.rs` and the option / exit modals in `modals.rs`.
//!
//! Layout is designed at 1280×720 and multiplied by the viewport scale.
//! Visual reference: the menu composition in `tests/d3d11_skin.rs`.

use super::widgets::{self, hint_row, keycap, keycap_width, wrap2, OptionLine, LEFT_RIGHT};
use crate::art::Skin;
use crate::canvas::{Canvas, Rect};
use crate::hit::{HitId, HitSink};
use crate::screens::play::{cover_uv, SizedTexture};
use crate::skin::ColorRgba;
use crate::strings;
use crate::text::{Align, TextEngine, TextStyle};
use crate::theme::{self, caption, thousands};
use crate::ui::Ui;
use crate::view::Viewport;
use beetle_core::{
    ClearType, LnOption, Ruleset, ScoreRecord, ScoreStore, SongMetadata, TableIndex,
};

/// Everything the song select screen shows for one frame.
/// One row of the list: a song (an index into `SelectFrame::songs`), a folder
/// (its name and how many songs it holds), or a group of charts of one song.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectRow<'a> {
    Song(usize),
    Folder {
        label: &'a str,
        count: usize,
        /// Songs of the folder per clear lamp, in `LAMP_ROWS` order.
        lamps: [usize; LAMP_COUNT],
    },
    /// The charts of one song: `charts` are indices into `SelectFrame::songs`
    /// (by level), `selected` the position in `charts` the row shows.
    Group {
        title: &'a str,
        charts: &'a [usize],
        selected: usize,
    },
}

/// The charts of the highlighted group, shown as tabs above the detail panel.
#[derive(Clone, Copy)]
struct ChartTabs<'a> {
    /// The group's row (its index in the visible rows).
    row: usize,
    charts: &'a [usize],
    selected: usize,
}

/// Clear lamps a folder's breakdown counts: the lamp rows, in the order the
/// folder tree counts them (`folders::LAMP_ORDER`).
pub const LAMP_COUNT: usize = 7;

/// The lamp of each breakdown row. `None` is the chart with no record.
const LAMP_ROWS: [Option<ClearType>; LAMP_COUNT] = [
    Some(ClearType::Perfect),
    Some(ClearType::FullCombo),
    Some(ClearType::Hard),
    Some(ClearType::Clear),
    Some(ClearType::Easy),
    Some(ClearType::Failed),
    None,
];

pub struct SelectFrame<'a> {
    pub viewport: &'a Viewport,
    /// The whole library; `Song` rows index into it.
    pub songs: &'a [SongMetadata],
    /// The rows of the current folder, in order.
    pub rows: &'a [SelectRow<'a>],
    /// Cursor position within `rows`.
    pub selected: usize,
    /// First visible row of the list. Clamped and moved only as far as needed
    /// to show `selected` (see `window_start`), so a click never moves the rows.
    pub scroll: usize,
    /// The library has no songs but the demo: the list shows the first-run guide.
    pub library_empty: bool,
    pub scores: &'a ScoreStore,
    /// The player's long note setting, which decides which record of a chart with long notes is shown.
    pub ln_option: LnOption,
    /// Installed difficulty tables, matched to the song list (level chips).
    pub tables: &'a TableIndex,
    /// The breadcrumb: the root label, then each folder on the way down (index = depth).
    pub crumbs: &'a [String],
    pub sort: &'a str,
    pub search: &'a str,
    pub search_active: bool,
    /// IME composition text shown after the query (underlined); empty when none.
    pub preedit: &'a str,
    /// Stage image of the selected song, once loaded.
    pub jacket: Option<SizedTexture>,
    /// Dominant color of that image (tints the ambient light).
    pub ambient: Option<ColorRgba>,
    /// Current play options shown above the PLAY button ("그린 500", ...).
    pub option_chips: &'a [String],
    pub auto_play: bool,
    pub has_replay: bool,
    /// Seconds the selected song's audio preview has been playing; `None` when it is not.
    pub preview_secs: Option<f32>,
    /// The filter row (mode chips, level bounds, toggles).
    pub filter: FilterBar<'a>,
    /// The "N곡 찾음" count while a search or a filter is on.
    pub result_count: Option<usize>,
    /// The sort menu, while it is open.
    pub sort_menu: Option<SortMenu<'a>>,
}

// Layout grid (1280×720 units).
use super::widgets::{FOOTER_H, PAD, TOPBAR_H};
const LIST_W: f32 = 640.0;
const ROW_H: f32 = 52.0;
const ROW_GAP: f32 = 6.0;

pub fn draw_song_select(ui: &mut Ui, f: &SelectFrame) {
    let sk = ui.skin;
    let lite = ui.lite;
    let mut hs = HitSink::new(&mut ui.hits, ui.pointer);
    let (c, t) = (&mut ui.canvas, &mut ui.text);
    let vp = f.viewport;
    let s = vp.scale;
    let selected = match f.rows.get(f.selected) {
        Some(SelectRow::Song(i)) => f.songs.get(*i),
        Some(SelectRow::Group {
            charts,
            selected: pos,
            ..
        }) => charts.get(*pos).and_then(|&i| f.songs.get(i)),
        _ => None,
    };

    backdrop(c, &sk, f, selected, lite);

    let content = content_rect(vp);
    if f.rows.is_empty() {
        empty_state(c, t, f, content, s, &mut hs);
    } else {
        let list = Rect::new(content.x, content.y, LIST_W * s, content.h);
        if f.library_empty {
            first_run_guide(c, t, &sk, list, s, &mut hs);
        } else {
            song_list(c, t, &sk, f, list, s, &mut hs);
        }
        let detail = Rect::from_ltrb(
            list.right() + PAD * s,
            content.y,
            content.right(),
            content.bottom(),
        );
        let tabs = match f.rows.get(f.selected) {
            Some(&SelectRow::Group {
                charts,
                selected: pos,
                ..
            }) => Some(ChartTabs {
                row: f.selected,
                charts,
                selected: pos,
            }),
            _ => None,
        };
        match (f.rows.get(f.selected), selected) {
            (Some(SelectRow::Song(_) | SelectRow::Group { .. }), Some(song)) => {
                detail_panel(c, t, &sk, f, song, detail, tabs, s, &mut hs)
            }
            (
                Some(&SelectRow::Folder {
                    label,
                    count,
                    lamps,
                }),
                _,
            ) => folder_panel(c, t, &sk, label, count, &lamps, detail, s),
            _ => {}
        }
    }

    let (caret, sort_anchor) = top_bar(c, t, &sk, f, s, &mut hs);
    ui.ime_caret = caret;
    filter_row(c, t, &sk, f, s, &mut hs);
    footer(c, t, &sk, f, s, &mut hs);
    if let Some(menu) = &f.sort_menu {
        sort_menu(c, t, &sk, vp, menu, sort_anchor, s, &mut hs);
    }
}

fn backdrop(c: &mut Canvas, sk: &Skin, f: &SelectFrame, song: Option<&SongMetadata>, lite: bool) {
    let tier = song.map_or(theme::MAGENTA, |s| theme::level_tier(s.play_level).1);
    let ambient = f.ambient.map_or(tier, |a| theme::vivid(a, tier));
    widgets::backdrop(c, sk, f.viewport, ambient, lite);
}

// ---------------------------------------------------------------------------
// Top bar: wordmark, folder / sort selectors, search box
// ---------------------------------------------------------------------------

/// A labeled top bar button with a keycap for its key; records `id`.
#[allow(clippy::too_many_arguments)]
fn bar_button(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    hs: &mut HitSink,
    r: Rect,
    label: &str,
    key: &str,
    id: HitId,
    s: f32,
) {
    let hot = hs.hovered(r);
    hs.add(r, id);
    c.nine(
        &sk.cut_panel,
        r,
        if hot { theme::SURF3 } else { theme::SURF2 },
    );
    c.nine(
        &sk.cut_outline,
        r,
        if hot {
            theme::CYAN.with_alpha(200)
        } else {
            theme::LINE
        },
    );
    let label_st =
        TextStyle::new(12.0 * s)
            .bold()
            .color(if hot { theme::TEXT } else { theme::MUTED });
    let lw = t.measure(c, label, &label_st);
    let kw = keycap_width(c, t, key, s);
    let bx = r.x + (r.w - lw - kw - 10.0 * s) / 2.0;
    t.draw_in(
        c,
        label,
        Rect::new(bx, r.y, lw + 1.0, r.h),
        Align::Left,
        &label_st,
    );
    keycap(
        c,
        t,
        sk,
        key,
        Rect::new(
            bx + lw + 10.0 * s,
            r.y + (r.h - 20.0 * s) / 2.0,
            kw,
            20.0 * s,
        ),
        s,
    );
}

/// Between two breadcrumb segments.
const CRUMB_SEP: &str = " > ";

/// Draws the top bar. Returns the IME caret rect and the sort selector's rect
/// (the sort menu opens under it).
fn top_bar(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    f: &SelectFrame,
    s: f32,
    hs: &mut HitSink,
) -> (Option<Rect>, Rect) {
    let vp = f.viewport;
    let x0 = vp.x + PAD * s;
    let adv = widgets::top_bar(c, t, vp, strings::WORDMARK, s);
    let bar_top = vp.y + 16.0 * s;
    let bar_h = 32.0 * s;

    // Left group: FOLDER ‹ breadcrumb ›, then SORT. The right group (SETTINGS,
    // OPTIONS, search) is laid out below; its left edge bounds this group.
    let right_limit = vp.x + vp.width - (PAD + 112.0) * s - 144.0 * s - 292.0 * s - 16.0 * s;
    let icon = 16.0 * s;
    let iy = vp.y + 32.0 * s - icon / 2.0;
    let base = vp.y + 37.0 * s;
    let cap = caption(12.0, s);
    let sort_value_st = TextStyle::new(13.0 * s).bold().color(theme::TEXT);
    let sort_label_w = t.measure(c, strings::SORT, &cap);
    let sort_value_w = t.measure(c, f.sort, &sort_value_st);
    let sort_x = right_limit - (sort_label_w + 12.0 * s + sort_value_w + 32.0 * s);

    let mut x = x0 + adv + 48.0 * s;
    x += t.draw(c, strings::FOLDER, x, base, &cap) + 12.0 * s;
    let prev = Rect::new(x - 4.0 * s, iy, icon, icon);
    let prev_hit = Rect::new(prev.x - 6.0 * s, bar_top, icon + 12.0 * s, bar_h);
    hs.add(prev_hit, HitId::FolderPrev);
    let prev_col = if hs.hovered(prev_hit) {
        theme::TEXT
    } else {
        theme::MUTED
    };
    c.sprite(sk.icons.chevron_left, prev, prev_col);
    x += icon + 4.0 * s;

    // Breadcrumb: keeps its last segments when the bar is too narrow (the
    // dropped ones become "…"). Each segment is a click target.
    let crumb_avail = (sort_x - 24.0 * s) - x - icon - 8.0 * s;
    let sep_st = TextStyle::new(12.0 * s).color(theme::MUTED2);
    let seg_st = |hot: bool, current: bool| {
        TextStyle::new(13.0 * s).bold().color(if current || hot {
            theme::TEXT
        } else {
            theme::MUTED
        })
    };
    let sep_w = t.measure(c, CRUMB_SEP, &sep_st);
    let mut shown: Vec<(usize, &str)> = f
        .crumbs
        .iter()
        .enumerate()
        .map(|(d, label)| (d, label.as_str()))
        .collect();
    let width = |t: &mut TextEngine, c: &mut Canvas, shown: &[(usize, &str)]| -> f32 {
        let names: f32 = shown
            .iter()
            .map(|(_, label)| t.measure(c, label, &seg_st(false, true)))
            .sum();
        names + sep_w * shown.len().saturating_sub(1) as f32
    };
    let mut dropped = false;
    while shown.len() > 1 && width(t, c, &shown) > crumb_avail {
        shown.remove(0);
        dropped = true;
    }
    let fitted = if shown.len() == 1 && width(t, c, &shown) > crumb_avail {
        Some(
            t.fit(c, shown[0].1, crumb_avail, &seg_st(false, true))
                .into_owned(),
        )
    } else {
        None
    };
    if let Some(text) = &fitted {
        shown[0].1 = text.as_str();
    }
    if dropped {
        x += t.draw(c, "…", x, base, &sep_st) + 4.0 * s;
    }
    let last = shown.len().saturating_sub(1);
    for (i, &(depth, label)) in shown.iter().enumerate() {
        if i > 0 {
            x += t.draw(c, CRUMB_SEP, x, base, &sep_st);
        }
        let w = t.measure(c, label, &seg_st(false, true));
        let hit = Rect::new(x - 3.0 * s, bar_top, w + 6.0 * s, bar_h);
        hs.add(hit, HitId::Crumb(depth));
        let hot = hs.hovered(hit);
        x += t.draw(c, label, x, base, &seg_st(hot, i == last));
    }

    let next = Rect::new(x + 4.0 * s, iy, icon, icon);
    let next_hit = Rect::new(next.x - 6.0 * s, bar_top, icon + 12.0 * s, bar_h);
    hs.add(next_hit, HitId::FolderNext);
    let next_col = if hs.hovered(next_hit) {
        theme::TEXT
    } else {
        theme::MUTED
    };
    c.sprite(sk.icons.chevron_right, next, next_col);

    // Sort: "LABEL  value", a click opens the sort menu.
    let mut sx = sort_x;
    sx += t.draw(c, strings::SORT, sx, base, &cap) + 12.0 * s;
    t.draw(c, f.sort, sx, base, &sort_value_st);
    let sort_hit = Rect::new(
        sort_x - 8.0 * s,
        bar_top,
        sort_label_w + sort_value_w + 28.0 * s + 12.0 * s,
        bar_h,
    );
    hs.add(sort_hit, HitId::Sort);
    if hs.hovered(sort_hit) || f.sort_menu.is_some() {
        c.stroke_rect(sort_hit, s.max(1.0), theme::CYAN.with_alpha(90));
    }

    // Right edge: SETTINGS (F4), then OPTIONS (TAB); the search box sits to their left.
    let settings = Rect::new(
        vp.x + vp.width - (PAD + 112.0) * s,
        bar_top,
        112.0 * s,
        bar_h,
    );
    bar_button(
        c,
        t,
        sk,
        hs,
        settings,
        strings::SETTINGS,
        "F4",
        HitId::OpenSettings,
        s,
    );
    let options = Rect::new(settings.x - (8.0 + 136.0) * s, bar_top, 136.0 * s, bar_h);
    bar_button(
        c,
        t,
        sk,
        hs,
        options,
        strings::OPTIONS,
        "TAB",
        HitId::PlayOptions,
        s,
    );

    // Search box
    let search = Rect::new(options.x - (12.0 + 280.0) * s, bar_top, 280.0 * s, bar_h);
    hs.add(search, HitId::Search);
    c.nine(
        &sk.panel_lg,
        search,
        if f.search_active {
            theme::SURF3
        } else {
            theme::SURF2
        },
    );
    if !f.search_active && hs.hovered(search) {
        c.nine(&sk.panel_lg, search, theme::WHITE.with_alpha(10));
    }
    if f.search_active {
        c.nine(&sk.panel_outline, search, theme::CYAN.with_alpha(200));
    }
    let inner = Rect::new(search.x + 16.0 * s, search.y, search.w - 52.0 * s, search.h);
    let mut caret = None;
    if f.search.is_empty() && f.preedit.is_empty() && !f.search_active {
        t.draw_in(
            c,
            strings::SEARCH_PLACEHOLDER,
            inner,
            Align::Left,
            &TextStyle::new(13.0 * s).color(theme::MUTED2),
        );
    } else {
        let st = TextStyle::new(13.0 * s).color(theme::TEXT);
        // The query and the composing text share one line. Trim from the front
        // (on char boundaries) so the end, where the caret is, stays visible.
        let full = format!("{}{}", f.search, f.preedit);
        let qlen = f.search.len();
        let mut start = 0;
        while start < full.len() && t.measure(c, &full[start..], &st) > inner.w - 8.0 * s {
            start += full[start..].chars().next().map_or(1, char::len_utf8);
        }
        let query = if start < qlen { &full[start..qlen] } else { "" };
        let preedit = &full[qlen.max(start)..];
        let qw = if query.is_empty() {
            0.0
        } else {
            t.draw_in(c, query, inner, Align::Left, &st)
        };
        let mut pw = 0.0;
        if !preedit.is_empty() {
            let px = inner.x + qw;
            let rest = Rect::from_ltrb(px, inner.y, inner.right(), inner.bottom());
            pw = t.draw_in(c, preedit, rest, Align::Left, &st);
            // Underline marks the text as still being composed.
            c.fill_rect(
                Rect::new(px, search.y + 23.0 * s, pw, s.max(1.0)),
                theme::CYAN,
            );
        }
        if f.search_active {
            let caret_rect = Rect::new(
                inner.x + qw + pw + 2.0 * s,
                search.y + 8.0 * s,
                2.0 * s,
                16.0 * s,
            );
            c.fill_rect(caret_rect, theme::CYAN);
            caret = Some(caret_rect);
        }
    }
    let key = Rect::new(
        search.right() - 32.0 * s,
        search.y + 6.0 * s,
        20.0 * s,
        20.0 * s,
    );
    keycap(c, t, sk, "/", key, s);
    (caret, sort_hit)
}

// ---------------------------------------------------------------------------
// Song list
// ---------------------------------------------------------------------------

/// The area under the top bar and the filter row, above the footer.
fn content_rect(vp: &Viewport) -> Rect {
    let s = vp.scale;
    Rect::from_ltrb(
        vp.x + PAD * s,
        vp.y + (TOPBAR_H + 36.0) * s,
        vp.x + vp.width - PAD * s,
        vp.y + vp.height - (FOOTER_H + 24.0) * s,
    )
}

/// How many song rows fit in the list at this viewport (at least one).
pub fn visible_rows(vp: &Viewport) -> usize {
    let s = vp.scale;
    let list_h = content_rect(vp).h;
    let step = (ROW_H + ROW_GAP) * s;
    (((list_h + ROW_GAP * s) / step).floor() as usize).max(1)
}

/// First visible row that puts `selected` in the middle of the window. Keyboard
/// moves use it, so the cursor stays centred as before.
pub fn centred_start(selected: usize, total: usize, rows: usize) -> usize {
    selected
        .saturating_sub(rows / 2)
        .min(total.saturating_sub(rows))
}

/// The first visible row for a list scrolled to `scroll`: the offset is
/// clamped to the list, then moved only as far as needed to show `selected`.
/// A click does not change `scroll`, so the rows stay under the pointer.
pub fn window_start(scroll: usize, selected: usize, total: usize, rows: usize) -> usize {
    let rows = rows.max(1);
    let max_start = total.saturating_sub(rows);
    let mut start = scroll.min(max_start);
    if selected < start {
        start = selected;
    } else if selected >= start + rows {
        start = selected + 1 - rows;
    }
    start.min(max_start)
}

/// The offset after a wheel scroll of `delta` rows (negative = up), clamped to the list.
pub fn scroll_by(scroll: usize, delta: isize, total: usize, rows: usize) -> usize {
    let max_start = total.saturating_sub(rows.max(1));
    let moved = scroll as isize + delta;
    (moved.max(0) as usize).min(max_start)
}

/// `selected` moved into the visible window `start..start + rows`. The wheel
/// moves the cursor only when it would leave the view.
pub fn clamp_into_window(selected: usize, start: usize, total: usize, rows: usize) -> usize {
    if total == 0 {
        return 0;
    }
    let last = (start + rows.max(1)).min(total) - 1;
    selected.clamp(start.min(last), last)
}

fn song_list(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    f: &SelectFrame,
    list: Rect,
    s: f32,
    hs: &mut HitSink,
) {
    let step = (ROW_H + ROW_GAP) * s;
    let rows = visible_rows(f.viewport);
    let total = f.rows.len();
    let start = window_start(f.scroll, f.selected, total, rows);
    let row_w = list.w - 16.0 * s;

    for (slot, entry) in f.rows.iter().enumerate().skip(start).take(rows) {
        let row = Rect::new(
            list.x,
            list.y + (slot - start) as f32 * step,
            row_w,
            ROW_H * s,
        );
        let on = slot == f.selected;
        let hot = !on && hs.hovered(row);
        match *entry {
            SelectRow::Song(idx) => {
                let Some(song) = f.songs.get(idx) else {
                    continue;
                };
                hs.add(row, HitId::ListRow(slot));
                let chip = f.tables.chip(song.id);
                song_row(
                    c,
                    t,
                    sk,
                    song,
                    f.scores.best(song, f.ln_option),
                    chip.as_deref(),
                    row,
                    on,
                    hot,
                    s,
                );
            }
            SelectRow::Folder { label, count, .. } => {
                hs.add(row, HitId::ListRow(slot));
                folder_row(c, t, sk, label, count, row, on, hot, s);
            }
            SelectRow::Group {
                title,
                charts,
                selected,
            } => {
                hs.add(row, HitId::ListRow(slot));
                group_row(
                    c, t, sk, f, title, charts, selected, slot, row, on, hot, s, hs,
                );
            }
        }
    }

    // Scrollbar
    if total > rows {
        let track = Rect::new(
            list.right() - 4.0 * s,
            list.y,
            3.0 * s,
            rows as f32 * step - ROW_GAP * s,
        );
        c.nine(&sk.panel_sm, track, theme::SURF2);
        let thumb_h = (track.h * rows as f32 / total as f32).max(24.0 * s);
        let thumb_y = track.y + (track.h - thumb_h) * start as f32 / (total - rows) as f32;
        c.nine(
            &sk.panel_sm,
            Rect::new(track.x, thumb_y, track.w, thumb_h),
            theme::CYAN.with_alpha(200),
        );
    }
}

/// Small pill with a difficulty table level (`sl3`); returns its width.
fn level_chip(
    c: &mut Canvas,
    t: &mut TextEngine,
    text: &str,
    x: f32,
    baseline: f32,
    s: f32,
) -> f32 {
    let st = TextStyle::new(11.0 * s).color(theme::PURPLE);
    let w = t.measure(c, text, &st) + 12.0 * s;
    let pill = Rect::new(x, baseline - 12.0 * s, w, 16.0 * s);
    c.fill_rect(pill, theme::PURPLE.with_alpha(36));
    c.stroke_rect(pill, s.max(1.0), theme::PURPLE.with_alpha(140));
    t.draw_in(c, text, pill, Align::Center, &st);
    w
}

#[allow(clippy::too_many_arguments)]
fn song_row(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    song: &SongMetadata,
    best: Option<&ScoreRecord>,
    table_chip: Option<&str>,
    row: Rect,
    on: bool,
    hot: bool,
    s: f32,
) {
    row_background(c, sk, row, on, hot, s);
    lamp_strip(c, sk, row, best, s);
    let tx = level_badge(c, t, sk, song.play_level, row, s);
    let right_w = 112.0 * s;
    let text_w = row.right() - right_w - 16.0 * s - tx;
    row_title(c, t, row, &song.title, tx, text_w, on, s);
    let sub_st = TextStyle::new(12.0 * s).color(theme::MUTED);
    let mode = theme::mode_label(song.play_mode);
    let mode_w = t.draw(
        c,
        mode,
        tx,
        row.y + 42.0 * s,
        &caption(12.0, s).color(if on { theme::CYAN } else { theme::MUTED }),
    );
    let mut ax = tx + mode_w + 8.0 * s;
    if let Some(chip) = table_chip {
        ax += level_chip(c, t, chip, ax, row.y + 42.0 * s, s) + 8.0 * s;
    }
    let artist = t
        .fit(c, &song.artist, text_w - (ax - tx), &sub_st)
        .into_owned();
    t.draw(c, &artist, ax, row.y + 42.0 * s, &sub_st);

    row_record(c, t, sk, best, row, right_w, s);
}

/// The frame of a list row: a lit row when it is the highlight, else the
/// background (the hover tint when the pointer is over it).
fn row_background(c: &mut Canvas, sk: &Skin, row: Rect, on: bool, hot: bool, s: f32) {
    if on {
        c.halo(&sk.shadow, row, theme::WHITE.with_alpha(200));
        c.nine(&sk.panel, row, theme::SURF3);
        c.push_clip(row);
        c.fill_rect_hgradient(
            Rect::new(row.x, row.y, row.w * 0.6, row.h),
            theme::CYAN.with_alpha(40),
            theme::CYAN.with_alpha(0),
        );
        c.pop_clip();
        c.nine(&sk.panel_outline, row, theme::CYAN.with_alpha(200));
        c.set_additive(true);
        c.sprite_centered(
            sk.glow,
            row.x,
            row.y + row.h / 2.0,
            80.0 * s,
            120.0 * s,
            theme::CYAN.with_alpha(110),
        );
        c.set_additive(false);
    } else {
        let bg = if hot { theme::SURF2 } else { theme::SURF1 };
        c.nine(&sk.panel, row, bg.with_alpha(220));
    }
}

/// The clear lamp: a strip on the row's left edge (IIDX convention). A row
/// without a record shows a dim strip.
fn lamp_strip(c: &mut Canvas, sk: &Skin, row: Rect, best: Option<&ScoreRecord>, s: f32) {
    // Clear lamp: a strip on the left edge (IIDX convention).
    let (_, lamp) = theme::clear_lamp(best.map(|b| b.clear_type));
    let lamp = if best.is_some() { lamp } else { theme::LINE };
    c.nine(
        &sk.panel_sm,
        Rect::new(row.x + 6.0 * s, row.y + 10.0 * s, 4.0 * s, row.h - 20.0 * s),
        lamp,
    );
}

/// The level badge at the row's left. Returns the x where the title starts.
fn level_badge(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    level: u32,
    row: Rect,
    s: f32,
) -> f32 {
    // Level badge
    let (_, diff) = theme::level_tier(level);
    let badge = Rect::new(row.x + 20.0 * s, row.y + 10.0 * s, 40.0 * s, 32.0 * s);
    c.nine(&sk.panel_sm, badge, diff.with_alpha(40));
    c.nine(
        &sk.panel_sm,
        Rect::new(badge.x, badge.bottom() - 3.0 * s, badge.w, 3.0 * s),
        diff,
    );
    t.draw_in(
        c,
        &level.to_string(),
        badge,
        Align::Center,
        &TextStyle::new(17.0 * s).bold().color(diff),
    );

    badge.right() + 16.0 * s
}

/// The row title on one line, cut to `text_w`.
#[allow(clippy::too_many_arguments)]
fn row_title(
    c: &mut Canvas,
    t: &mut TextEngine,
    row: Rect,
    text: &str,
    tx: f32,
    text_w: f32,
    on: bool,
    s: f32,
) {
    let title_st = TextStyle::new(16.0 * s).bold().color(if on {
        theme::TEXT
    } else {
        theme::TEXT.with_alpha(215)
    });
    let shown = t.fit(c, text, text_w, &title_st).into_owned();
    t.draw(c, &shown, tx, row.y + 24.0 * s, &title_st);
}

/// The right column of a row: the personal best's rank and rate bar. A row
/// without a record draws nothing here.
fn row_record(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    best: Option<&ScoreRecord>,
    row: Rect,
    right_w: f32,
    s: f32,
) {
    // Personal best: rank + score-rate bar, or "NO PLAY".
    let rx = row.right() - right_w - 16.0 * s;
    match best {
        Some(b) => {
            let (rank, rank_col) = theme::rank(b.accuracy_rate());
            t.draw_in(
                c,
                rank,
                Rect::new(rx, row.y + 8.0 * s, right_w, 22.0 * s),
                Align::Right,
                &TextStyle::new(16.0 * s).bold().color(rank_col),
            );
            let bar = Rect::new(rx, row.y + 36.0 * s, right_w, 4.0 * s);
            c.nine(&sk.panel_sm, bar, theme::LINE);
            let rate = (b.accuracy_rate() / 100.0).clamp(0.0, 1.0) as f32;
            c.nine(
                &sk.panel_sm,
                Rect::new(bar.x, bar.y, bar.w * rate, bar.h),
                rank_col,
            );
        }
        // An unplayed row stays quiet: its lamp strip is already dim, and the
        // right side stays empty so the played rows stand out.
        None => {}
    }
}

/// A chart chip on a group row or in the detail tabs' text: `7K 12` in the
/// level's tier colour. The chosen chart is filled. Returns its width.
fn chart_chip(
    c: &mut Canvas,
    t: &mut TextEngine,
    text: &str,
    x: f32,
    baseline: f32,
    col: ColorRgba,
    on: bool,
    s: f32,
) -> f32 {
    let st = TextStyle::new(11.0 * s).color(if on { theme::TEXT } else { col });
    let w = t.measure(c, text, &st) + 12.0 * s;
    let pill = Rect::new(x, baseline - 12.0 * s, w, 16.0 * s);
    c.fill_rect(pill, col.with_alpha(if on { 90 } else { 24 }));
    c.stroke_rect(pill, s.max(1.0), col.with_alpha(if on { 255 } else { 110 }));
    t.draw_in(c, text, pill, Align::Center, &st);
    w
}

/// A group row: the group's title, one chip per chart (the chosen one filled)
/// and the chosen chart's artist, badge, lamp and record. Each chip is a hit
/// region that picks its chart.
#[allow(clippy::too_many_arguments)]
fn group_row(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    f: &SelectFrame,
    title: &str,
    charts: &[usize],
    selected: usize,
    slot: usize,
    row: Rect,
    on: bool,
    hot: bool,
    s: f32,
    hs: &mut HitSink,
) {
    let Some(song) = charts.get(selected).and_then(|&i| f.songs.get(i)) else {
        return;
    };
    let best = f.scores.best(song, f.ln_option);
    row_background(c, sk, row, on, hot, s);
    lamp_strip(c, sk, row, best, s);
    let tx = level_badge(c, t, sk, song.play_level, row, s);
    let right_w = 112.0 * s;
    let text_w = row.right() - right_w - 16.0 * s - tx;
    row_title(c, t, row, title, tx, text_w, on, s);

    // Chips on the second line, then the chosen chart's artist.
    let base = row.y + 42.0 * s;
    let mut ax = tx;
    for (pos, &i) in charts.iter().enumerate() {
        let Some(chart) = f.songs.get(i) else {
            continue;
        };
        let (_, col) = theme::level_tier(chart.play_level);
        let text = format!(
            "{} {}",
            theme::mode_label(chart.play_mode),
            chart.play_level
        );
        let w = chart_chip(c, t, &text, ax, base, col, pos == selected, s);
        hs.add(
            Rect::new(ax, base - 12.0 * s, w, 16.0 * s),
            HitId::ChartTab { row: slot, pos },
        );
        ax += w + 6.0 * s;
    }
    let sub_st = TextStyle::new(12.0 * s).color(theme::MUTED);
    let artist = t
        .fit(
            c,
            &song.artist,
            row.right() - right_w - 16.0 * s - (ax + 8.0 * s),
            &sub_st,
        )
        .into_owned();
    t.draw(c, &artist, ax + 8.0 * s, base, &sub_st);
    row_record(c, t, sk, best, row, right_w, s);
}

/// The difficulty tabs of a group, in a strip at the top of the detail panel.
/// Each tab is a hit region that picks its chart.
fn chart_tabs(
    c: &mut Canvas,
    t: &mut TextEngine,
    f: &SelectFrame,
    tabs: ChartTabs,
    strip: Rect,
    s: f32,
    hs: &mut HitSink,
) {
    let mut x = strip.x;
    for (pos, &i) in tabs.charts.iter().enumerate() {
        let Some(chart) = f.songs.get(i) else {
            continue;
        };
        let (_, col) = theme::level_tier(chart.play_level);
        let text = format!(
            "{} {}",
            theme::mode_label(chart.play_mode),
            chart.play_level
        );
        let on = pos == tabs.selected;
        let st = caption(12.0, s).color(if on { theme::TEXT } else { theme::MUTED });
        let w = t.measure(c, &text, &st) + 24.0 * s;
        let rect = Rect::new(x, strip.y, w, strip.h);
        c.fill_rect(rect, if on { col.with_alpha(70) } else { theme::SURF2 });
        c.stroke_rect(rect, s.max(1.0), if on { col } else { theme::LINE });
        if on {
            c.fill_rect(
                Rect::new(rect.x, rect.bottom() - 3.0 * s, rect.w, 3.0 * s),
                col,
            );
        }
        t.draw_in(c, &text, rect, Align::Center, &st);
        hs.add(rect, HitId::ChartTab { row: tabs.row, pos });
        x += w + 6.0 * s;
    }
}

/// A folder row: the same height as a song row, with an accent strip, the
/// name, the song count and a chevron that says it opens.
#[allow(clippy::too_many_arguments)]
fn folder_row(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    label: &str,
    count: usize,
    row: Rect,
    on: bool,
    hot: bool,
    s: f32,
) {
    if on {
        c.halo(&sk.shadow, row, theme::WHITE.with_alpha(200));
        c.nine(&sk.panel, row, theme::SURF3);
        c.nine(&sk.panel_outline, row, theme::CYAN.with_alpha(200));
    } else {
        let bg = if hot { theme::SURF2 } else { theme::SURF1 };
        c.nine(&sk.panel, row, bg.with_alpha(220));
    }
    // Folder accent: a strip on the left, in the folder color (not the level tier colors).
    c.nine(
        &sk.panel_sm,
        Rect::new(row.x + 6.0 * s, row.y + 10.0 * s, 4.0 * s, row.h - 20.0 * s),
        theme::BLUE,
    );

    let chevron = 14.0 * s;
    let count_text = strings::fill(strings::FOLDER_SONGS, &[&thousands(count as u32)]);
    let count_w = t.measure(c, &count_text, &caption(12.0, s));
    let name_x = row.x + 24.0 * s;
    let name_w = row.w - (name_x - row.x) - count_w - chevron - 32.0 * s;
    let name_st = TextStyle::new(16.0 * s).bold().color(if on {
        theme::TEXT
    } else {
        theme::TEXT.with_alpha(215)
    });
    let name = t.fit(c, label, name_w.max(0.0), &name_st).into_owned();
    t.draw(c, &name, name_x, row.y + row.h / 2.0 + 6.0 * s, &name_st);

    let count_x = row.right() - chevron - 12.0 * s - count_w;
    t.draw(
        c,
        &count_text,
        count_x,
        row.y + row.h / 2.0 + 5.0 * s,
        &caption(12.0, s).color(if on { theme::CYAN } else { theme::MUTED }),
    );
    c.sprite(
        sk.icons.chevron_right,
        Rect::new(
            row.right() - chevron - 8.0 * s,
            row.y + (row.h - chevron) / 2.0,
            chevron,
            chevron,
        ),
        if on { theme::CYAN } else { theme::MUTED },
    );
}

/// The detail panel of a highlighted folder: its name, song count, how to open
/// it, and the clear lamp breakdown of its songs.
#[allow(clippy::too_many_arguments)]
fn folder_panel(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    label: &str,
    count: usize,
    lamps: &[usize; LAMP_COUNT],
    panel: Rect,
    s: f32,
) {
    c.halo(&sk.shadow, panel, theme::WHITE.with_alpha(160));
    c.nine(&sk.panel_lg, panel, theme::SURF1.with_alpha(235));
    let inner = panel.inset(24.0 * s);
    let name_st = TextStyle::new(30.0 * s).bold().color(theme::TEXT);
    let name = t.fit(c, label, inner.w, &name_st).into_owned();
    t.draw(c, &name, inner.x, inner.y + 40.0 * s, &name_st);
    t.draw(
        c,
        &strings::fill(strings::FOLDER_SONGS, &[&thousands(count as u32)]),
        inner.x,
        inner.y + 80.0 * s,
        &TextStyle::new(16.0 * s).bold().color(theme::CYAN),
    );
    t.draw_in(
        c,
        strings::FOLDER_OPEN_HINT,
        Rect::new(inner.x, inner.y + 120.0 * s, inner.w, 20.0 * s),
        Align::Left,
        &TextStyle::new(13.0 * s).color(theme::MUTED),
    );
    lamp_breakdown(
        c,
        t,
        sk,
        lamps,
        Rect::new(inner.x, inner.y + 164.0 * s, inner.w, inner.h - 164.0 * s),
        s,
    );
}

/// The clear lamp breakdown: a stacked bar of the songs per lamp, then one
/// row per lamp with its count. Lamp colours are the theme's lamp colours.
fn lamp_breakdown(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    lamps: &[usize; LAMP_COUNT],
    area: Rect,
    s: f32,
) {
    let total: usize = lamps.iter().sum();
    if total == 0 {
        return;
    }
    t.draw(
        c,
        strings::FOLDER_LAMP,
        area.x,
        area.y + 12.0 * s,
        &caption(12.0, s).color(theme::MUTED),
    );
    let bar = Rect::new(area.x, area.y + 22.0 * s, area.w, 10.0 * s);
    c.nine(&sk.panel_sm, bar, theme::SURF2);
    let mut x = bar.x;
    for (&n, lamp) in lamps.iter().zip(LAMP_ROWS) {
        if n == 0 || total == 0 {
            continue;
        }
        let w = bar.w * n as f32 / total as f32;
        c.fill_rect(Rect::new(x, bar.y, w, bar.h), lamp_colour(lamp));
        x += w;
    }
    for (i, (&n, lamp)) in lamps.iter().zip(LAMP_ROWS).enumerate() {
        let y = bar.bottom() + 14.0 * s + i as f32 * 20.0 * s;
        let col = lamp_colour(lamp);
        c.fill_rect(Rect::new(area.x, y + 3.0 * s, 4.0 * s, 12.0 * s), col);
        let name = match lamp {
            Some(_) => theme::clear_lamp(lamp).0,
            None => strings::NO_PLAY,
        };
        let name_col = if n == 0 {
            theme::MUTED2
        } else {
            theme::TEXT.with_alpha(215)
        };
        t.draw(
            c,
            name,
            area.x + 14.0 * s,
            y + 14.0 * s,
            &TextStyle::new(13.0 * s).color(name_col),
        );
        t.draw_in(
            c,
            &thousands(n as u32),
            Rect::new(area.x, y, area.w, 18.0 * s),
            Align::Right,
            &TextStyle::new(13.0 * s).bold().color(if n == 0 {
                theme::MUTED2
            } else {
                theme::TEXT
            }),
        );
    }
}

/// The colour of a lamp row (the theme's clear lamp colour; no record is dim).
fn lamp_colour(lamp: Option<ClearType>) -> ColorRgba {
    theme::clear_lamp(lamp).1
}

fn empty_state(
    c: &mut Canvas,
    t: &mut TextEngine,
    f: &SelectFrame,
    area: Rect,
    s: f32,
    hs: &mut HitSink,
) {
    let (head, hint) = if !f.search.is_empty() {
        (
            strings::fill(strings::NO_MATCH, &[&f.search]),
            strings::SEARCH_HINT,
        )
    } else if f.filter.active {
        (
            strings::FILTER_NO_MATCH.to_string(),
            strings::FILTER_EMPTY_HINT,
        )
    } else {
        (strings::EMPTY_FOLDER.to_string(), strings::EMPTY_HINT)
    };
    let cy = area.y + area.h * 0.42;
    let head_st = TextStyle::new(22.0 * s).bold().color(theme::TEXT);
    let head = t.fit(c, &head, area.w, &head_st).into_owned();
    t.draw_in(
        c,
        &head,
        Rect::new(area.x, cy - 20.0 * s, area.w, 30.0 * s),
        Align::Center,
        &head_st,
    );
    t.draw_in(
        c,
        hint,
        Rect::new(area.x, cy + 16.0 * s, area.w, 20.0 * s),
        Align::Center,
        &TextStyle::new(13.0 * s).color(theme::MUTED),
    );
    if f.filter.active {
        let w = 120.0 * s;
        let r = Rect::new(area.x + (area.w - w) / 2.0, cy + 46.0 * s, w, 28.0 * s);
        hs.add(r, HitId::FilterReset);
        let hot = hs.hovered(r);
        chip_frame(c, r, false, hot, false, s);
        t.draw_in(
            c,
            strings::FILTER_RESET,
            r,
            Align::Center,
            &TextStyle::new(12.0 * s)
                .bold()
                .color(if hot { theme::TEXT } else { theme::MUTED }),
        );
    }
}

// ---------------------------------------------------------------------------
// Detail panel
// ---------------------------------------------------------------------------

/// The first-run guide, in the list area of an empty library: the three ways to
/// add songs (the second one is a drop, so it has no button), and the rescan.
fn first_run_guide(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    area: Rect,
    s: f32,
    hs: &mut HitSink,
) {
    c.nine(&sk.panel_lg, area, theme::SURF1.with_alpha(235));
    let inner = area.inset(24.0 * s);
    t.draw(
        c,
        strings::GUIDE_TITLE,
        inner.x,
        inner.y + 30.0 * s,
        &TextStyle::new(22.0 * s).bold().color(theme::TEXT),
    );
    t.draw(
        c,
        strings::GUIDE_LEAD,
        inner.x,
        inner.y + 60.0 * s,
        &TextStyle::new(13.0 * s).color(theme::MUTED),
    );
    // (step text, button label and the action it records, keycap of the button)
    let steps: [(&str, Option<(&str, HitId, Option<&str>)>); 4] = [
        (
            strings::GUIDE_MANAGER,
            Some((strings::GUIDE_BTN_MANAGER, HitId::OpenManager, None)),
        ),
        (strings::GUIDE_DROP, None),
        (
            strings::GUIDE_FOLDER,
            Some((strings::GUIDE_BTN_FOLDER, HitId::OpenSongsFolder, None)),
        ),
        (
            strings::GUIDE_RESCAN,
            Some((strings::GUIDE_BTN_RESCAN, HitId::Rescan, Some("F5"))),
        ),
    ];
    let mut y = inner.y + 96.0 * s;
    for (i, (text, button)) in steps.into_iter().enumerate() {
        let badge = Rect::new(inner.x, y, 26.0 * s, 26.0 * s);
        c.nine(&sk.panel_sm, badge, theme::CYAN.with_alpha(40));
        t.draw_in(
            c,
            &(i + 1).to_string(),
            badge,
            Align::Center,
            &TextStyle::new(14.0 * s).bold().color(theme::CYAN),
        );
        t.draw(
            c,
            text,
            inner.x + 38.0 * s,
            y + 18.0 * s,
            &TextStyle::new(14.0 * s).color(theme::TEXT.with_alpha(215)),
        );
        if let Some((label, id, key)) = button {
            guide_button(
                c,
                t,
                sk,
                hs,
                Rect::new(inner.x + 38.0 * s, y + 32.0 * s, 210.0 * s, 32.0 * s),
                label,
                id,
                key,
                s,
            );
        }
        y += 84.0 * s;
    }
}

/// A button of the first-run guide: records `id`, and shows its keycap when it has one.
#[allow(clippy::too_many_arguments)]
fn guide_button(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    hs: &mut HitSink,
    r: Rect,
    label: &str,
    id: HitId,
    key: Option<&str>,
    s: f32,
) {
    hs.add(r, id);
    let hot = hs.hovered(r);
    chip_frame(c, r, false, hot, false, s);
    let st = TextStyle::new(12.0 * s)
        .bold()
        .color(if hot { theme::TEXT } else { theme::MUTED });
    match key {
        None => {
            t.draw_in(c, label, r, Align::Center, &st);
        }
        Some(key) => {
            let lw = t.measure(c, label, &st);
            let kw = keycap_width(c, t, key, s);
            let x = r.x + (r.w - lw - kw - 10.0 * s) / 2.0;
            t.draw_in(c, label, Rect::new(x, r.y, lw + 1.0, r.h), Align::Left, &st);
            keycap(
                c,
                t,
                sk,
                key,
                Rect::new(
                    x + lw + 10.0 * s,
                    r.y + (r.h - 20.0 * s) / 2.0,
                    kw,
                    20.0 * s,
                ),
                s,
            );
        }
    }
}

/// "PREVIEW" pill with a small level meter in the jacket's lower-left corner.
fn preview_badge(c: &mut Canvas, t: &mut TextEngine, jacket: Rect, secs: f32, s: f32) {
    let pill = Rect::new(
        jacket.x + 8.0 * s,
        jacket.bottom() - 28.0 * s,
        86.0 * s,
        20.0 * s,
    );
    c.fill_rect(pill, theme::BG.with_alpha(205));
    c.stroke_rect(pill, s.max(1.0), theme::CYAN.with_alpha(110));
    let (bar_w, gap, floor) = (3.0 * s, 2.0 * s, pill.bottom() - 5.0 * s);
    for i in 0..4 {
        let phase = secs * 7.0 + i as f32 * 1.7;
        let h = (3.0 + 8.0 * (0.5 + 0.5 * phase.sin())) * s;
        c.fill_rect(
            Rect::new(
                pill.x + 7.0 * s + i as f32 * (bar_w + gap),
                floor - h,
                bar_w,
                h,
            ),
            theme::CYAN,
        );
    }
    t.draw(
        c,
        strings::PREVIEW,
        pill.x + 30.0 * s,
        pill.bottom() - 6.0 * s,
        &TextStyle::new(11.0 * s).color(theme::CYAN),
    );
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn detail_panel(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    f: &SelectFrame,
    song: &SongMetadata,
    panel: Rect,
    tabs: Option<ChartTabs>,
    s: f32,
    hs: &mut HitSink,
) {
    c.halo(&sk.shadow, panel, theme::WHITE.with_alpha(160));
    c.nine(&sk.panel_lg, panel, theme::SURF1.with_alpha(235));
    let inner = panel.inset(24.0 * s);
    // A group's difficulty tabs take a strip at the top; the content goes below it.
    let inner = match tabs {
        Some(tabs) => {
            let strip = Rect::new(inner.x, inner.y, inner.w, 22.0 * s);
            chart_tabs(c, t, f, tabs, strip, s, hs);
            Rect::from_ltrb(
                inner.x,
                strip.bottom() + 12.0 * s,
                inner.right(),
                inner.bottom(),
            )
        }
        None => inner,
    };
    let (tier, tier_col) = theme::level_tier(song.play_level);

    // Jacket (stage image, 4:3) or a generated placeholder in the tier color.
    let jacket = Rect::new(inner.x, inner.y, 256.0 * s, 192.0 * s);
    match f.jacket {
        Some(tex) => {
            c.fill_rect(jacket, theme::BG);
            c.image(tex.id, jacket, cover_uv(tex, jacket), theme::WHITE);
        }
        None => {
            c.fill_rect_corners(
                jacket,
                [tier_col, theme::SURF3, theme::BG, tier_col.with_alpha(160)],
            );
            c.set_additive(true);
            c.sprite_centered(
                sk.flare,
                jacket.x + jacket.w * 0.7,
                jacket.y + jacket.h * 0.3,
                160.0 * s,
                160.0 * s,
                theme::WHITE.with_alpha(90),
            );
            c.set_additive(false);
            t.draw_in(
                c,
                theme::mode_label(song.play_mode),
                jacket.inset(12.0 * s),
                Align::Left,
                &TextStyle::new(44.0 * s)
                    .bold()
                    .color(theme::WHITE.with_alpha(40)),
            );
        }
    }
    c.stroke_rect(jacket, s.max(1.0), theme::LINE);
    if let Some(secs) = f.preview_secs {
        preview_badge(c, t, jacket, secs, s);
    }

    // Title block, right of the jacket
    let ix = jacket.right() + 24.0 * s;
    let iw = inner.right() - ix;
    let tier_txt = format!("{tier} {}", song.play_level);
    let tier_w = t.draw(
        c,
        &tier_txt,
        ix,
        inner.y + 14.0 * s,
        &caption(12.0, s).color(tier_col),
    );
    // The tables the chart is in (the first ones; the panel has no room for more).
    let mut chip_x = ix + tier_w + 12.0 * s;
    for m in f.tables.matches_for(song.id).iter().take(3) {
        let table = &f.tables.tables()[m.table];
        let text = format!("{}{}", table.symbol, table.entries[m.entry].level);
        chip_x += level_chip(c, t, &text, chip_x, inner.y + 14.0 * s, s) + 6.0 * s;
    }
    // One line at 26px if it fits, otherwise two lines at 20px.
    let big = TextStyle::new(26.0 * s).bold().color(theme::TEXT);
    let mut y = if t.measure(c, &song.title, &big) <= iw {
        t.draw(c, &song.title, ix, inner.y + 46.0 * s, &big);
        inner.y + 70.0 * s
    } else {
        let st = TextStyle::new(20.0 * s).bold().color(theme::TEXT);
        let (l1, l2) = wrap2(c, t, &song.title, iw, &st);
        t.draw(c, &l1, ix, inner.y + 42.0 * s, &st);
        if let Some(l2) = l2 {
            t.draw(c, &l2, ix, inner.y + 66.0 * s, &st);
        }
        inner.y + 90.0 * s
    };
    if !song.subtitle.is_empty() {
        let st = TextStyle::new(13.0 * s).color(theme::TEXT.with_alpha(180));
        let sub = t.fit(c, &song.subtitle, iw, &st).into_owned();
        t.draw(c, &sub, ix, y, &st);
        y += 20.0 * s;
    }
    let artist_st = TextStyle::new(14.0 * s).color(theme::MUTED);
    let artist = t.fit(c, &song.artist, iw, &artist_st).into_owned();
    t.draw(c, &artist, ix, y, &artist_st);
    if !song.genre.is_empty() {
        let st = TextStyle::new(12.0 * s).color(theme::MUTED);
        let genre = t.fit(c, &song.genre, iw, &st).into_owned();
        t.draw(c, &genre, ix, y + 18.0 * s, &st);
    }

    // Chart stats along the jacket's bottom edge. MODE and NOTES hang off the
    // right edge at their content width; BPM gets the room that is left, so a
    // long tempo range shrinks rather than running into the next value.
    let label_st = caption(12.0, s).color(theme::MUTED);
    let value_st = TextStyle::new(22.0 * s).bold().color(theme::TEXT);
    let gap = 18.0 * s;
    let bpm = song.bpm_label();
    let notes = thousands(song.notes_for(f.ln_option) as u32);
    let mode = theme::mode_label(song.play_mode);
    let mode_col = t
        .measure(c, mode, &value_st)
        .max(t.measure(c, strings::MODE, &label_st));
    let notes_col = t
        .measure(c, &notes, &value_st)
        .max(t.measure(c, strings::NOTES, &label_st));
    let mode_x = inner.right() - mode_col;
    let notes_x = mode_x - gap - notes_col;
    let (bpm_text, bpm_st) = fit_stat(c, t, &bpm, (notes_x - gap - ix).max(0.0), value_st);
    let (label_y, value_y) = (jacket.bottom() - 30.0 * s, jacket.bottom() - 2.0 * s);
    t.draw(c, strings::BPM, ix, label_y, &label_st);
    t.draw(c, &bpm_text, ix, value_y, &bpm_st);
    t.draw(c, strings::NOTES, notes_x, label_y, &label_st);
    t.draw(c, &notes, notes_x, value_y, &value_st);
    t.draw(c, strings::MODE, mode_x, label_y, &label_st);
    t.draw(c, mode, mode_x, value_y, &value_st);

    let rule_y = jacket.bottom() + 24.0 * s;
    c.fill_rect(Rect::new(inner.x, rule_y, inner.w, s.max(1.0)), theme::LINE);
    // Which long note rule the record below is for, when the rule matters.
    let rule = (song.ln_count > 0).then(|| Ruleset::resolve(song.ln_mode, f.ln_option).label());
    personal_best(
        c,
        t,
        sk,
        song,
        f.scores.best(song, f.ln_option),
        f.ln_option,
        rule.as_deref(),
        Rect::new(inner.x, rule_y, inner.w, 150.0 * s),
        s,
    );

    // Play options + CTA at the bottom
    let cta_h = 52.0 * s;
    let mut cta = Rect::new(inner.x, inner.bottom() - cta_h, inner.w, cta_h);
    let chips_y = cta.y - 40.0 * s;
    let mut cx = inner.x;
    for chip in f.option_chips {
        let st = caption(12.0, s).color(theme::MUTED);
        let w = t.measure(c, chip, &st) + 20.0 * s;
        if cx + w > inner.right() {
            break;
        }
        let r = Rect::new(cx, chips_y, w, 24.0 * s);
        c.nine(&sk.panel_lg, r, theme::SURF2);
        t.draw_in(c, chip, r, Align::Center, &st);
        cx += w + 8.0 * s;
    }
    if f.auto_play {
        let st = caption(12.0, s).color(theme::ON_ACCENT);
        let w = t.measure(c, strings::AUTO_PLAY, &st) + 20.0 * s;
        let r = Rect::new(inner.right() - w, chips_y, w, 24.0 * s);
        c.nine(&sk.panel_lg, r, theme::CYAN);
        t.draw_in(c, strings::AUTO_PLAY, r, Align::Center, &st);
    }

    if f.has_replay {
        let rw = 132.0 * s;
        let replay = Rect::new(cta.right() - rw, cta.y, rw, cta.h);
        cta.w -= rw + 12.0 * s;
        let replay_hot = hs.hovered(replay);
        hs.add(replay, HitId::Replay);
        c.nine(
            &sk.cut_panel,
            replay,
            if replay_hot {
                theme::SURF3
            } else {
                theme::SURF2
            },
        );
        c.nine(
            &sk.cut_outline,
            replay,
            if replay_hot {
                theme::CYAN.with_alpha(200)
            } else {
                theme::LINE
            },
        );
        let st = TextStyle::new(13.0 * s).bold().color(theme::TEXT);
        let lw = t.measure(c, strings::REPLAY, &st);
        let kw = 20.0 * s;
        let x = replay.x + (replay.w - lw - kw - 10.0 * s) / 2.0;
        t.draw_in(
            c,
            strings::REPLAY,
            Rect::new(x, replay.y, lw, replay.h),
            Align::Left,
            &st,
        );
        keycap(
            c,
            t,
            sk,
            "R",
            Rect::new(x + lw + 10.0 * s, replay.y + (replay.h - kw) / 2.0, kw, kw),
            s,
        );
    }
    c.set_additive(true);
    c.sprite_centered(
        sk.glow,
        cta.x + cta.w / 2.0,
        cta.y + cta.h / 2.0,
        cta.w * 1.1,
        140.0 * s,
        theme::CYAN.with_alpha(55),
    );
    c.set_additive(false);
    c.nine_hgradient(&sk.cut_panel, cta, theme::CYAN, theme::BLUE);
    hs.add(cta, HitId::Play);
    if hs.hovered(cta) {
        c.fill_rect(cta, theme::WHITE.with_alpha(28));
    }
    c.sprite(
        sk.icons.play,
        Rect::new(cta.x + 22.0 * s, cta.y + 12.0 * s, 28.0 * s, 28.0 * s),
        theme::ON_ACCENT,
    );
    let label = if f.auto_play {
        strings::AUTO_PLAY
    } else {
        strings::PLAY
    };
    t.draw_in(
        c,
        label,
        Rect::new(cta.x + 60.0 * s, cta.y, cta.w - 160.0 * s, cta.h),
        Align::Left,
        &TextStyle::new(20.0 * s).bold().color(theme::ON_ACCENT),
    );
    t.draw_in(
        c,
        "ENTER",
        Rect::new(cta.right() - 120.0 * s, cta.y, 96.0 * s, cta.h),
        Align::Right,
        &TextStyle::new(12.0 * s)
            .bold()
            .color(theme::ON_ACCENT.with_alpha(150)),
    );
}

/// A stat value that must fit `room`: steps the size down a pixel at a time
/// to 60% of its size, and only then is cut short with an ellipsis.
fn fit_stat<'a>(
    c: &mut Canvas,
    t: &mut TextEngine,
    text: &'a str,
    room: f32,
    st: TextStyle,
) -> (std::borrow::Cow<'a, str>, TextStyle) {
    let floor = st.size * 0.6;
    let mut shrunk = st;
    while t.measure(c, text, &shrunk) > room && shrunk.size - 1.0 >= floor {
        shrunk.size -= 1.0;
    }
    (t.fit(c, text, room, &shrunk), shrunk)
}

#[allow(clippy::too_many_arguments)]
fn personal_best(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    song: &SongMetadata,
    best: Option<&ScoreRecord>,
    ln_option: LnOption,
    rule: Option<&str>,
    area: Rect,
    s: f32,
) {
    let y = area.y;
    let header_w = t.draw(
        c,
        strings::PERSONAL_BEST,
        area.x,
        y + 32.0 * s,
        &caption(12.0, s),
    );
    let Some(b) = best else {
        if let Some(rule) = rule {
            t.draw(
                c,
                rule,
                area.x + header_w + 14.0 * s,
                y + 32.0 * s,
                &TextStyle::new(11.0 * s).color(theme::MUTED),
            );
        }
        t.draw(
            c,
            strings::NOT_PLAYED,
            area.x,
            y + 70.0 * s,
            &TextStyle::new(18.0 * s).bold().color(theme::MUTED),
        );
        t.draw(
            c,
            strings::CLEAR_TO_RECORD,
            area.x,
            y + 92.0 * s,
            &TextStyle::new(13.0 * s).color(theme::MUTED2),
        );
        return;
    };

    // How the best score was made, and how often the chart was played.
    let mut notes = Vec::new();
    if let Some(rule) = rule {
        notes.push(rule.to_string());
    }
    if let Some(modifier) = b.modifier {
        notes.push(modifier.as_str().to_string());
    }
    if let Some(gauge) = b.gauge {
        notes.push(strings::fill(strings::GAUGE_NAME, &[gauge.as_str()]));
    }
    if b.play_count > 0 {
        notes.push(strings::fill(
            strings::PLAY_COUNT,
            &[&b.play_count.to_string()],
        ));
    }
    if !notes.is_empty() {
        let st = TextStyle::new(11.0 * s).color(theme::MUTED);
        t.draw(
            c,
            &notes.join("  /  "),
            area.x + header_w + 14.0 * s,
            y + 32.0 * s,
            &st,
        );
    }

    // Clear lamp chip
    let (lamp, lamp_col) = theme::clear_lamp(Some(b.clear_type));
    let st = caption(12.0, s).color(lamp_col);
    let w = t.measure(c, lamp, &st) + 24.0 * s;
    let chip = Rect::new(area.right() - w, y + 16.0 * s, w, 24.0 * s);
    c.nine(&sk.panel_lg, chip, lamp_col.with_alpha(36));
    c.nine(&sk.panel_outline, chip, lamp_col.with_alpha(160));
    t.draw_in(c, lamp, chip, Align::Center, &st);

    // EX score / max, rank
    let ex_w = t.draw(
        c,
        &thousands(b.ex_score),
        area.x,
        y + 80.0 * s,
        &TextStyle::new(40.0 * s).bold().color(theme::TEXT),
    );
    let max = format!("/ {}", thousands(song.notes_for(ln_option) as u32 * 2));
    t.draw(
        c,
        &max,
        area.x + ex_w + 10.0 * s,
        y + 80.0 * s,
        &TextStyle::new(13.0 * s).color(theme::MUTED2),
    );
    let (rank, rank_col) = theme::rank(b.accuracy_rate());
    t.draw_in(
        c,
        rank,
        Rect::new(area.right() - 120.0 * s, y + 46.0 * s, 120.0 * s, 40.0 * s),
        Align::Right,
        &TextStyle::new(36.0 * s).bold().color(rank_col),
    );

    let rate = (b.accuracy_rate() / 100.0).clamp(0.0, 1.0) as f32;
    widgets::rate_bar(
        c,
        t,
        sk,
        Rect::new(area.x, y + 96.0 * s, area.w, 4.0 * s),
        rate,
        rank_col,
        s,
    );

    // Accuracy, combo, miss count
    let stats = [
        (strings::ACCURACY, format!("{:.2}%", b.accuracy_rate())),
        (
            strings::MAX_COMBO,
            format!(
                "{} / {}",
                thousands(b.max_combo),
                thousands(song.notes_for(ln_option) as u32)
            ),
        ),
        (strings::MIN_BP, thousands(b.min_bp)),
    ];
    let col_w = area.w / 3.0;
    for (i, (k, v)) in stats.iter().enumerate() {
        let sx = area.x + i as f32 * col_w;
        t.draw(c, k, sx, y + 134.0 * s, &caption(12.0, s));
        t.draw(
            c,
            v,
            sx,
            y + 156.0 * s,
            &TextStyle::new(16.0 * s).bold().color(theme::TEXT),
        );
    }
}

// ---------------------------------------------------------------------------
// Footer
// ---------------------------------------------------------------------------

fn footer(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    f: &SelectFrame,
    s: f32,
    hs: &mut HitSink,
) {
    let vp = f.viewport;
    let bar = widgets::footer_bar(c, vp, s);
    let base = bar.y + 25.0 * s;
    let count_st = TextStyle::new(13.0 * s).bold().color(theme::TEXT);

    // Count: songs of the list (with the library total), or the folders of a folder list.
    let mut x = vp.x + PAD * s;
    let folders_only =
        !f.rows.is_empty() && f.rows.iter().all(|r| matches!(r, SelectRow::Folder { .. }));
    if folders_only {
        let text = strings::fill(strings::FOLDER_COUNT, &[&thousands(f.rows.len() as u32)]);
        x += t.draw(c, &text, x, base, &count_st);
    } else {
        x += t.draw(c, &thousands(f.rows.len() as u32), x, base, &count_st);
        let total = strings::fill(strings::SONGS_TOTAL, &[&thousands(f.songs.len() as u32)]);
        t.draw(c, &total, x, base, &caption(12.0, s));
    }

    // Only the keys a player needs here; the rest are in the help overlay (?).
    // ENTER opens a folder row and plays a song row; BKSP goes up when not at the root.
    let on_folder = matches!(f.rows.get(f.selected), Some(SelectRow::Folder { .. }));
    let mut hints: Vec<widgets::Hint> = vec![("↑↓", strings::FOOTER_MOVE, None)];
    hints.push((
        "ENTER",
        if on_folder {
            strings::FOOTER_OPEN
        } else {
            strings::FOOTER_PLAY
        },
        Some(HitId::Play),
    ));
    if f.crumbs.len() > 1 {
        hints.push(("BKSP", strings::BACK, Some(HitId::FolderUp)));
    }
    hints.push(("TAB", strings::OPTIONS, Some(HitId::PlayOptions)));
    hints.push(("?", strings::FOOTER_HELP, Some(HitId::Help)));

    // Key hints, right-aligned; drop from the left if they do not fit.
    let right = vp.x + vp.width - PAD * s;
    let left_limit = x + 200.0 * s;
    let mut first = 0;
    while first < hints.len() && right - widgets::hints_width(c, t, &hints[first..], s) < left_limit
    {
        first += 1;
    }
    widgets::footer_buttons(c, t, sk, &hints[first..], bar, s, hs);
}

// ---------------------------------------------------------------------------
// Filter row
// ---------------------------------------------------------------------------

/// The filter row's top edge and height (1280×720 units). It sits under the
/// top bar's divider; the list starts below it (see `draw_song_select`).
const FILTER_Y: f32 = 70.0;
const FILTER_H: f32 = 24.0;

/// One item of the filter row. The row lists them left to right in this order,
/// and the app's keys and clicks index them with `filter_items`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterItem {
    /// A mode chip: the index into `FilterBar::modes`.
    Mode(usize),
    /// The lowest level shown (`‹ value ›`).
    LevelMin,
    /// The highest level shown.
    LevelMax,
    Unplayed,
    Uncleared,
    /// 초기화, listed only while a filter is on.
    Reset,
}

/// The items of the filter row for `modes` mode chips.
pub fn filter_items(modes: usize, active: bool) -> Vec<FilterItem> {
    let mut items: Vec<FilterItem> = (0..modes).map(FilterItem::Mode).collect();
    items.extend([
        FilterItem::LevelMin,
        FilterItem::LevelMax,
        FilterItem::Unplayed,
        FilterItem::Uncleared,
    ]);
    if active {
        items.push(FilterItem::Reset);
    }
    items
}

/// What the filter row shows: the mode chips (label, in the filter), the level
/// bounds, the two toggles, and the keyboard focus.
#[derive(Clone, Copy, Debug, Default)]
pub struct FilterBar<'a> {
    pub modes: &'a [(&'a str, bool)],
    pub level_min: Option<u32>,
    pub level_max: Option<u32>,
    pub unplayed: bool,
    pub uncleared: bool,
    /// Whether any filter is on (the row then lists 초기화).
    pub active: bool,
    /// The focused item, an index into `filter_items`.
    pub focus: Option<usize>,
}

/// The open sort menu: its option names, the option in use, and the highlighted one.
#[derive(Clone, Copy, Debug)]
pub struct SortMenu<'a> {
    pub options: &'a [&'a str],
    pub current: usize,
    pub highlight: usize,
}

/// A filter chip (or button) frame: filled and outlined in the accent when
/// `on`, with a white ring when keyboard focus is on it.
fn chip_frame(c: &mut Canvas, r: Rect, on: bool, hot: bool, focus: bool, s: f32) {
    let fill = if on {
        theme::CYAN.with_alpha(36)
    } else if hot {
        theme::SURF3
    } else {
        theme::SURF2
    };
    c.fill_rect(r, fill);
    c.stroke_rect(
        r,
        s.max(1.0),
        if on {
            theme::CYAN.with_alpha(200)
        } else {
            theme::LINE
        },
    );
    if focus {
        let ring = Rect::new(r.x - 2.0 * s, r.y - 2.0 * s, r.w + 4.0 * s, r.h + 4.0 * s);
        c.stroke_rect(ring, s.max(1.0), theme::WHITE.with_alpha(200));
    }
}

/// A chip's label, centered. Lit text when the chip is on or hovered.
fn chip_label(c: &mut Canvas, t: &mut TextEngine, r: Rect, label: &str, lit: bool, s: f32) {
    let st = TextStyle::new(11.0 * s)
        .bold()
        .color(if lit { theme::TEXT } else { theme::MUTED });
    t.draw_in(c, label, r, Align::Center, &st);
}

/// Width of a text chip: its label plus padding.
fn chip_width(c: &mut Canvas, t: &mut TextEngine, label: &str, s: f32) -> f32 {
    t.measure(c, label, &TextStyle::new(11.0 * s).bold()) + 16.0 * s
}

/// The filter row: `필터 F10`, the mode chips, the level bounds, the toggles,
/// 초기화 while a filter is on, and the result count at the right.
fn filter_row(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    f: &SelectFrame,
    s: f32,
    hs: &mut HitSink,
) {
    let vp = f.viewport;
    let bar = Rect::new(
        vp.x + PAD * s,
        vp.y + FILTER_Y * s,
        vp.width - 2.0 * PAD * s,
        FILTER_H * s,
    );
    let fb = &f.filter;
    let cap = caption(12.0, s);
    let mut x = bar.x;

    let label_w = t.measure(c, strings::FILTER, &cap);
    t.draw_in(
        c,
        strings::FILTER,
        Rect::new(x, bar.y, label_w + 1.0, bar.h),
        Align::Left,
        &cap,
    );
    x += label_w + 6.0 * s;
    let key_w = keycap_width(c, t, "F10", s);
    keycap(
        c,
        t,
        sk,
        "F10",
        Rect::new(x, bar.y + (bar.h - 20.0 * s) / 2.0, key_w, 20.0 * s),
        s,
    );
    x += key_w + 14.0 * s;

    // The item indices follow `filter_items`: modes, the bounds, the toggles, reset.
    let mut item = 0;
    for &(label, on) in fb.modes {
        let r = Rect::new(x, bar.y, chip_width(c, t, label, s), bar.h);
        hs.add(r, HitId::FilterItem(item));
        let hot = hs.hovered(r);
        chip_frame(c, r, on, hot, fb.focus == Some(item), s);
        chip_label(c, t, r, label, on || hot, s);
        x += r.w + 6.0 * s;
        item += 1;
    }

    x += 8.0 * s;
    c.fill_rect(
        Rect::new(x, bar.y + 4.0 * s, s.max(1.0), bar.h - 8.0 * s),
        theme::LINE,
    );
    x += 10.0 * s;

    let level_w = t.measure(c, strings::FILTER_LEVEL, &cap);
    t.draw_in(
        c,
        strings::FILTER_LEVEL,
        Rect::new(x, bar.y, level_w + 1.0, bar.h),
        Align::Left,
        &cap,
    );
    x += level_w + 6.0 * s;
    let min_label = fb
        .level_min
        .map_or(strings::FILTER_LEVEL_MIN_ANY.to_string(), |v| v.to_string());
    let min_on = fb.level_min.is_some();
    x += level_stepper(
        c,
        t,
        sk,
        hs,
        x,
        bar,
        &min_label,
        min_on,
        fb.focus == Some(item),
        item,
        s,
    );
    item += 1;
    let tilde_w = t.measure(c, "~", &cap);
    t.draw_in(
        c,
        "~",
        Rect::new(x, bar.y, tilde_w + 1.0, bar.h),
        Align::Left,
        &cap,
    );
    x += tilde_w + 6.0 * s;
    let max_label = fb
        .level_max
        .map_or(strings::FILTER_LEVEL_MAX_ANY.to_string(), |v| v.to_string());
    let max_on = fb.level_max.is_some();
    x += level_stepper(
        c,
        t,
        sk,
        hs,
        x,
        bar,
        &max_label,
        max_on,
        fb.focus == Some(item),
        item,
        s,
    );
    item += 1;

    x += 12.0 * s;
    for (label, on) in [
        (strings::FILTER_UNPLAYED, fb.unplayed),
        (strings::FILTER_UNCLEARED, fb.uncleared),
    ] {
        let r = Rect::new(x, bar.y, chip_width(c, t, label, s), bar.h);
        hs.add(r, HitId::FilterItem(item));
        let hot = hs.hovered(r);
        chip_frame(c, r, on, hot, fb.focus == Some(item), s);
        chip_label(c, t, r, label, on || hot, s);
        x += r.w + 6.0 * s;
        item += 1;
    }

    if fb.active {
        let r = Rect::new(
            x + 6.0 * s,
            bar.y,
            chip_width(c, t, strings::FILTER_RESET, s),
            bar.h,
        );
        hs.add(r, HitId::FilterItem(item));
        let hot = hs.hovered(r);
        chip_frame(c, r, false, hot, fb.focus == Some(item), s);
        chip_label(c, t, r, strings::FILTER_RESET, hot, s);
    }

    // The result count ends at the search box's right edge.
    if let Some(n) = f.result_count {
        let right = vp.x + vp.width - (PAD + 112.0 + 144.0 + 12.0) * s;
        let text = strings::fill(strings::RESULT_COUNT, &[&thousands(n as u32)]);
        t.draw_in(
            c,
            &text,
            Rect::new(right - 160.0 * s, bar.y, 160.0 * s, bar.h),
            Align::Right,
            &TextStyle::new(12.0 * s).bold().color(theme::TEXT),
        );
    }
}

/// A level bound `‹ value ›`. The chip is item `item` (a click or ENTER steps
/// it up); the two arrows record `FilterStep`. Returns the width used.
#[allow(clippy::too_many_arguments)]
fn level_stepper(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    hs: &mut HitSink,
    x: f32,
    bar: Rect,
    value: &str,
    on: bool,
    focus: bool,
    item: usize,
    s: f32,
) -> f32 {
    let arrow = 12.0 * s;
    let st = TextStyle::new(11.0 * s).bold();
    let value_w = t.measure(c, value, &st);
    let w = arrow + 8.0 * s + value_w + 8.0 * s + arrow + 4.0 * s;
    let r = Rect::new(x, bar.y, w + 8.0 * s, bar.h);
    hs.add(r, HitId::FilterItem(item));
    let hot = hs.hovered(r);
    chip_frame(c, r, on, hot, focus, s);

    let arrow_y = bar.y + (bar.h - arrow) / 2.0;
    let left = Rect::new(r.x + 4.0 * s, arrow_y, arrow, arrow);
    let right = Rect::new(r.right() - 4.0 * s - arrow, arrow_y, arrow, arrow);
    let left_hit = Rect::new(r.x, bar.y, arrow + 8.0 * s, bar.h);
    let right_hit = Rect::new(r.right() - arrow - 8.0 * s, bar.y, arrow + 8.0 * s, bar.h);
    hs.add(left_hit, HitId::FilterStep { item, up: false });
    hs.add(right_hit, HitId::FilterStep { item, up: true });
    let left_col = if hs.hovered(left_hit) {
        theme::TEXT
    } else {
        theme::MUTED
    };
    let right_col = if hs.hovered(right_hit) {
        theme::TEXT
    } else {
        theme::MUTED
    };
    c.sprite(sk.icons.chevron_left, left, left_col);
    c.sprite(sk.icons.chevron_right, right, right_col);
    t.draw_in(
        c,
        value,
        Rect::new(left.right(), bar.y, right.x - left.right(), bar.h),
        Align::Center,
        &st.color(if on || hot { theme::TEXT } else { theme::MUTED }),
    );
    r.w
}

// ---------------------------------------------------------------------------
// Sort menu
// ---------------------------------------------------------------------------

/// The sort menu: a small list under the sort selector. The full-screen
/// blocker is recorded first, so a click outside closes it; the panel and then
/// the options record after it and win inside.
#[allow(clippy::too_many_arguments)]
fn sort_menu(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    vp: &Viewport,
    menu: &SortMenu,
    anchor: Rect,
    s: f32,
    hs: &mut HitSink,
) {
    hs.add(Rect::new(vp.x, vp.y, vp.width, vp.height), HitId::Blocker);
    let item_h = 28.0 * s;
    let pad = 6.0 * s;
    let panel = Rect::new(
        anchor.x,
        anchor.bottom() + 4.0 * s,
        144.0 * s,
        menu.options.len() as f32 * item_h + pad * 2.0,
    );
    hs.add(panel, HitId::ModalPanel);
    c.nine(&sk.panel_lg, panel, theme::SURF1);
    c.stroke_rect(panel, s.max(1.0), theme::CYAN.with_alpha(160));
    for (i, label) in menu.options.iter().enumerate() {
        let r = Rect::new(
            panel.x + 4.0 * s,
            panel.y + pad + i as f32 * item_h,
            panel.w - 8.0 * s,
            item_h,
        );
        hs.add(r, HitId::SortOption(i));
        let hot = hs.hovered(r);
        if i == menu.highlight {
            c.fill_rect(r, theme::SURF3);
        }
        let current = i == menu.current;
        if current {
            c.fill_rect(
                Rect::new(r.x + 4.0 * s, r.y + 7.0 * s, 3.0 * s, r.h - 14.0 * s),
                theme::CYAN,
            );
        }
        let st = TextStyle::new(13.0 * s).bold().color(if current {
            theme::CYAN
        } else if hot || i == menu.highlight {
            theme::TEXT
        } else {
            theme::MUTED
        });
        t.draw_in(
            c,
            label,
            Rect::new(r.x + 14.0 * s, r.y, r.w - 18.0 * s, r.h),
            Align::Left,
            &st,
        );
    }
}

// ---------------------------------------------------------------------------
// Modals
// ---------------------------------------------------------------------------

fn modal_panel(
    c: &mut Canvas,
    sk: &Skin,
    vp: &Viewport,
    hs: &mut HitSink,
    w: f32,
    h: f32,
    s: f32,
) -> Rect {
    let screen = Rect::new(vp.x, vp.y, vp.width, vp.height);
    c.fill_rect(screen, theme::BLACK.with_alpha(180));
    // Clicks outside the panel close the modal; inside it they do nothing
    // unless they land on a control recorded after these two.
    hs.add(screen, HitId::Blocker);
    let panel = Rect::new(
        vp.x + (vp.width - w) / 2.0,
        vp.y + (vp.height - h) / 2.0,
        w,
        h,
    );
    hs.add(panel, HitId::ModalPanel);
    c.halo(&sk.shadow, panel, theme::WHITE);
    c.nine(&sk.panel_lg, panel, theme::SURF1);
    c.fill_rect_hgradient(
        Rect::new(panel.x + 16.0 * s, panel.y, panel.w - 32.0 * s, 2.0 * s),
        theme::CYAN,
        theme::MAGENTA,
    );
    panel
}

/// Play options panel over the song list: one column of per-play values,
/// with the highlighted option's name and help at the bottom. `help` is
/// (name, sentence) of the highlighted row.
pub fn draw_options_modal(
    ui: &mut Ui,
    vp: &Viewport,
    lines: &[OptionLine],
    selected: usize,
    help: (&str, &str),
) {
    draw_options_panel(ui, vp, lines, selected, help, OptionsFooter::Close);
}

/// What the play options panel offers besides the rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionsFooter {
    /// Over the song list: TAB closes it (the values were saved as changed).
    Close,
    /// Over the result screen: a start button plays again with the options as
    /// changed (ENTER, or a click on the button). TAB closes without playing.
    Retry,
}

/// The play options panel with the footer of `footer` (see `OptionsFooter`).
pub fn draw_options_panel(
    ui: &mut Ui,
    vp: &Viewport,
    lines: &[OptionLine],
    selected: usize,
    help: (&str, &str),
    footer: OptionsFooter,
) {
    let sk = ui.skin;
    let mut hs = HitSink::new(&mut ui.hits, ui.pointer);
    let (c, t) = (&mut ui.canvas, &mut ui.text);
    let s = vp.scale;
    let row_h = 34.0 * s;
    let section_h = 30.0 * s;
    let start_h = match footer {
        OptionsFooter::Close => 0.0,
        OptionsFooter::Retry => 56.0 * s,
    };
    // Rows, section headers, then the help card and the hint line below them.
    let body: f32 = lines
        .iter()
        .map(|l| row_h + if l.section.is_some() { section_h } else { 0.0 })
        .sum();
    let h = (body + (widgets::HELP_CARD_H + 136.0) * s + start_h).min(vp.height - 32.0 * s);
    let panel = modal_panel(c, &sk, vp, &mut hs, 500.0 * s, h, s);
    let inner = panel.inset(28.0 * s);

    t.draw(
        c,
        strings::MODAL_PLAY_OPTIONS,
        inner.x,
        inner.y + 22.0 * s,
        &TextStyle::new(22.0 * s).bold().color(theme::TEXT),
    );
    let mut y = inner.y + 40.0 * s;
    for (i, line) in lines.iter().enumerate() {
        if let Some(section) = line.section {
            widgets::section_header(c, t, section, inner.x, y, inner.w, s);
            y += section_h;
        }
        let row = Rect::new(inner.x - 8.0 * s, y, inner.w + 16.0 * s, row_h - 2.0 * s);
        widgets::option_row(
            c,
            t,
            &sk,
            &mut hs,
            row,
            i,
            line,
            i == selected,
            150.0 * s,
            s,
        );
        y += row_h;
    }
    let card = Rect::new(inner.x, y + 12.0 * s, inner.w, widgets::HELP_CARD_H * s);
    widgets::help_card(c, t, &sk, card, help.0, help.1, s);

    let hints: &[(&str, &str)] = match footer {
        OptionsFooter::Close => &[
            ("↑↓", strings::HINT_MOVE),
            (LEFT_RIGHT, strings::HINT_CHANGE),
            ("TAB", strings::HINT_CLOSE),
        ],
        OptionsFooter::Retry => {
            let start = Rect::new(inner.x, card.bottom() + 12.0 * s, inner.w, 36.0 * s);
            hs.add(start, HitId::OptionStart);
            let hovered = hs.hovered(start);
            c.nine(
                &sk.cut_panel,
                start,
                if hovered {
                    theme::CYAN
                } else {
                    theme::CYAN.with_alpha(200)
                },
            );
            t.draw_in(
                c,
                strings::RESULT_OPTIONS_START,
                start,
                Align::Center,
                &TextStyle::new(15.0 * s).bold().color(theme::ON_ACCENT),
            );
            &[
                ("↑↓", strings::HINT_MOVE),
                (LEFT_RIGHT, strings::HINT_CHANGE),
                ("ENTER", strings::HINT_START),
                ("TAB", strings::HINT_CLOSE),
            ]
        }
    };
    let w = hint_row(c, t, &sk, hints, 0.0, 0.0, s, false);
    hint_row(
        c,
        t,
        &sk,
        hints,
        panel.x + (panel.w - w) / 2.0,
        panel.bottom() - 44.0 * s,
        s,
        true,
    );
}

/// "Quit Beetle?" confirmation over the song list.
pub fn draw_exit_modal(ui: &mut Ui, vp: &Viewport) {
    let sk = ui.skin;
    let mut hs = HitSink::new(&mut ui.hits, ui.pointer);
    let (c, t) = (&mut ui.canvas, &mut ui.text);
    let s = vp.scale;
    let panel = modal_panel(c, &sk, vp, &mut hs, 420.0 * s, 212.0 * s, s);
    let inner = panel.inset(28.0 * s);
    t.draw(
        c,
        strings::QUIT_TITLE,
        inner.x,
        inner.y + 26.0 * s,
        &TextStyle::new(24.0 * s).bold().color(theme::TEXT),
    );
    t.draw(
        c,
        strings::QUIT_NOTE,
        inner.x,
        inner.y + 56.0 * s,
        &TextStyle::new(13.0 * s).color(theme::MUTED),
    );

    let bw = (inner.w - 12.0 * s) / 2.0;
    let bh = 48.0 * s;
    let by = inner.bottom() - bh;
    let cancel = Rect::new(inner.x, by, bw, bh);
    let quit = Rect::new(cancel.right() + 12.0 * s, by, bw, bh);
    hs.add(cancel, HitId::ExitCancel);
    hs.add(quit, HitId::ExitQuit);
    let cancel_hot = hs.hovered(cancel);
    let quit_hot = hs.hovered(quit);
    c.nine(
        &sk.cut_panel,
        cancel,
        if cancel_hot {
            theme::SURF3
        } else {
            theme::SURF2
        },
    );
    c.nine(
        &sk.cut_outline,
        cancel,
        if cancel_hot {
            theme::CYAN.with_alpha(200)
        } else {
            theme::LINE
        },
    );
    c.nine(&sk.cut_panel, quit, theme::MAGENTA);
    if quit_hot {
        c.fill_rect(quit, theme::WHITE.with_alpha(28));
    }
    for (r, label, key, col) in [
        (cancel, strings::CANCEL, "ESC", theme::TEXT),
        (quit, strings::QUIT, "ENTER", theme::WHITE),
    ] {
        let st = TextStyle::new(14.0 * s).bold().color(col);
        let lw = t.measure(c, label, &st);
        let kw = keycap_width(c, t, key, s);
        let x = r.x + (r.w - lw - kw - 10.0 * s) / 2.0;
        t.draw_in(c, label, Rect::new(x, r.y, lw + 1.0, r.h), Align::Left, &st);
        keycap(
            c,
            t,
            &sk,
            key,
            Rect::new(
                x + lw + 10.0 * s,
                r.y + (r.h - 20.0 * s) / 2.0,
                kw,
                20.0 * s,
            ),
            s,
        );
    }
}

// ---------------------------------------------------------------------------
// Help overlay (?) and the drag overlay
// ---------------------------------------------------------------------------

/// One row of the help overlay: its keycaps, then what it does. A row without
/// keycaps shows `caption` (a mouse action, or dropping a file) in their place.
pub struct HelpRow {
    pub keys: &'static [&'static str],
    pub caption: &'static str,
    pub text: &'static str,
}

/// One group of the help overlay.
pub struct HelpGroup {
    pub title: &'static str,
    pub rows: &'static [HelpRow],
}

/// Every song select key and mouse action, grouped. The keys are the ones the
/// song select handler reads; the mouse rows are the regions the screen records.
pub const HELP_GROUPS: [HelpGroup; 6] = [
    HelpGroup {
        title: strings::HELP_GROUP_MOVE,
        rows: &[
            HelpRow {
                keys: &["↑↓"],
                caption: "",
                text: strings::HELP_MOVE_ONE,
            },
            HelpRow {
                keys: &["PgUp", "PgDn"],
                caption: "",
                text: strings::HELP_MOVE_PAGE,
            },
            HelpRow {
                keys: &["Home", "End"],
                caption: "",
                text: strings::HELP_MOVE_ENDS,
            },
            HelpRow {
                keys: &[],
                caption: strings::HELP_CAP_MOUSE,
                text: strings::HELP_MOUSE_WHEEL,
            },
            HelpRow {
                keys: &[],
                caption: strings::HELP_CAP_MOUSE,
                text: strings::HELP_MOUSE_ROW,
            },
        ],
    },
    HelpGroup {
        title: strings::HELP_GROUP_FOLDER,
        rows: &[
            HelpRow {
                keys: &["BKSP"],
                caption: "",
                text: strings::HELP_FOLDER_UP,
            },
            HelpRow {
                keys: &["ENTER"],
                caption: "",
                text: strings::HELP_FOLDER_OPEN,
            },
            HelpRow {
                keys: &["F1", "F3"],
                caption: "",
                text: strings::HELP_FOLDER_SIDE,
            },
            HelpRow {
                keys: &[],
                caption: strings::HELP_CAP_MOUSE,
                text: strings::HELP_MOUSE_SIDE,
            },
            HelpRow {
                keys: &[],
                caption: strings::HELP_CAP_MOUSE,
                text: strings::HELP_MOUSE_CRUMB,
            },
        ],
    },
    HelpGroup {
        title: strings::HELP_GROUP_SONG,
        rows: &[
            HelpRow {
                keys: &["ENTER"],
                caption: "",
                text: strings::HELP_SONG_PLAY,
            },
            HelpRow {
                keys: &["A"],
                caption: "",
                text: strings::HELP_SONG_AUTO,
            },
            HelpRow {
                keys: &["R"],
                caption: "",
                text: strings::HELP_SONG_REPLAY,
            },
            HelpRow {
                keys: &["LEFT", "RIGHT"],
                caption: "",
                text: strings::HELP_SONG_CHART,
            },
            HelpRow {
                keys: &[],
                caption: strings::HELP_CAP_MOUSE,
                text: strings::HELP_MOUSE_AGAIN,
            },
        ],
    },
    HelpGroup {
        title: strings::HELP_GROUP_OPTIONS,
        rows: &[
            HelpRow {
                keys: &["TAB", "O"],
                caption: "",
                text: strings::HELP_OPTIONS,
            },
            HelpRow {
                keys: &["F4"],
                caption: "",
                text: strings::HELP_SETTINGS,
            },
            HelpRow {
                keys: &["F12", "C"],
                caption: "",
                text: strings::HELP_KEYS,
            },
            HelpRow {
                keys: &[],
                caption: strings::HELP_CAP_MOUSE,
                text: strings::HELP_MOUSE_BUTTONS,
            },
        ],
    },
    HelpGroup {
        title: strings::HELP_GROUP_FILTER,
        rows: &[
            HelpRow {
                keys: &["/"],
                caption: "",
                text: strings::HELP_SEARCH,
            },
            HelpRow {
                keys: &["F2"],
                caption: "",
                text: strings::HELP_SORT,
            },
            HelpRow {
                keys: &["F10"],
                caption: "",
                text: strings::HELP_FILTER,
            },
            HelpRow {
                keys: &[],
                caption: strings::HELP_CAP_MOUSE,
                text: strings::HELP_MOUSE_FILTER,
            },
        ],
    },
    HelpGroup {
        title: strings::HELP_GROUP_MISC,
        rows: &[
            HelpRow {
                keys: &["ESC"],
                caption: "",
                text: strings::HELP_BACK_QUIT,
            },
            HelpRow {
                keys: &["F5"],
                caption: "",
                text: strings::HELP_RESCAN,
            },
            HelpRow {
                keys: &["?"],
                caption: "",
                text: strings::HELP_HELP,
            },
            HelpRow {
                keys: &[],
                caption: strings::HELP_CAP_DROP,
                text: strings::HELP_DROP,
            },
        ],
    },
];

/// The help overlay over song select: every key and mouse action, in two
/// columns of three groups. A click outside the panel, `?` or ESC closes it.
pub fn draw_help_overlay(ui: &mut Ui, vp: &Viewport) {
    let sk = ui.skin;
    let mut hs = HitSink::new(&mut ui.hits, ui.pointer);
    let (c, t) = (&mut ui.canvas, &mut ui.text);
    let s = vp.scale;
    let panel = modal_panel(c, &sk, vp, &mut hs, 1040.0 * s, 580.0 * s, s);
    let inner = panel.inset(28.0 * s);
    t.draw(
        c,
        strings::HELP_TITLE,
        inner.x,
        inner.y + 24.0 * s,
        &TextStyle::new(22.0 * s).bold().color(theme::TEXT),
    );
    t.draw_in(
        c,
        strings::HELP_CLOSE,
        Rect::new(inner.x, inner.y + 4.0 * s, inner.w, 24.0 * s),
        Align::Right,
        &TextStyle::new(12.0 * s).color(theme::MUTED),
    );

    let col_gap = 32.0 * s;
    let col_w = (inner.w - col_gap) / 2.0;
    let key_w = 150.0 * s;
    for (i, group) in HELP_GROUPS.iter().enumerate() {
        let gx = inner.x + (i % 2) as f32 * (col_w + col_gap);
        let gy = inner.y + 52.0 * s + (i / 2) as f32 * 160.0 * s;
        t.draw(
            c,
            group.title,
            gx,
            gy + 16.0 * s,
            &TextStyle::new(14.0 * s).bold().color(theme::CYAN),
        );
        c.fill_rect(Rect::new(gx, gy + 24.0 * s, col_w, s.max(1.0)), theme::LINE);
        for (j, row) in group.rows.iter().enumerate() {
            let ry = gy + 36.0 * s + j as f32 * 24.0 * s;
            let mut kx = gx;
            if row.keys.is_empty() {
                t.draw(
                    c,
                    row.caption,
                    kx,
                    ry + 16.0 * s,
                    &caption(12.0, s).color(theme::MUTED2),
                );
            }
            for key in row.keys {
                let kw = keycap_width(c, t, key, s);
                keycap(c, t, &sk, key, Rect::new(kx, ry + 2.0 * s, kw, 20.0 * s), s);
                kx += kw + 6.0 * s;
            }
            let text = t
                .fit(
                    c,
                    row.text,
                    col_w - key_w,
                    &TextStyle::new(13.0 * s).color(theme::TEXT.with_alpha(215)),
                )
                .into_owned();
            t.draw(
                c,
                &text,
                gx + key_w,
                ry + 16.0 * s,
                &TextStyle::new(13.0 * s).color(theme::TEXT.with_alpha(215)),
            );
        }
    }
}

/// The overlay over the whole window while a file is dragged over it. It
/// records no click regions: the drop is handled by the window event.
pub fn draw_drop_overlay(ui: &mut Ui, vp: &Viewport) {
    let sk = ui.skin;
    let s = vp.scale;
    let (c, t) = (&mut ui.canvas, &mut ui.text);
    c.fill_rect(
        Rect::new(vp.x, vp.y, vp.width, vp.height),
        theme::BLACK.with_alpha(150),
    );
    let w = 560.0 * s;
    let h = 130.0 * s;
    let panel = Rect::new(
        vp.x + (vp.width - w) / 2.0,
        vp.y + (vp.height - h) / 2.0,
        w,
        h,
    );
    c.halo(&sk.shadow, panel, theme::CYAN.with_alpha(160));
    c.nine(&sk.panel_lg, panel, theme::SURF1);
    c.stroke_rect(panel, s.max(1.0), theme::CYAN.with_alpha(200));
    t.draw_in(
        c,
        strings::DROP_TITLE,
        Rect::new(panel.x, panel.y + 30.0 * s, panel.w, 30.0 * s),
        Align::Center,
        &TextStyle::new(22.0 * s).bold().color(theme::CYAN),
    );
    t.draw_in(
        c,
        strings::DROP_NOTE,
        Rect::new(panel.x, panel.y + 74.0 * s, panel.w, 20.0 * s),
        Align::Center,
        &TextStyle::new(13.0 * s).color(theme::MUTED),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use beetle_core::{ClearType, GaugeType, LaneModifier, PlayMode, PlayResult};

    fn song(i: usize) -> SongMetadata {
        SongMetadata {
            id: beetle_core::ChartId::synthetic(i as u64 + 1),
            md5: [0; 16],
            ln_count: 0,
            ln_mode: None,
            legacy_hash: i as u64 + 1,
            file_path: format!("songs/{i}.bms"),
            title: format!("Song number {i} with a fairly long title (ANOTHER)"),
            subtitle: String::new(),
            artist: "Artist 아티스트 アーティスト".into(),
            genre: "TRANCE".into(),
            bpm: 150.0,
            bpm_min: 150.0,
            bpm_max: 150.0,
            play_level: (i % 13) as u32,
            notes_count: 1000 + i,
            play_mode: if i % 3 == 0 {
                PlayMode::Keys14
            } else {
                PlayMode::Keys7
            },
        }
    }

    #[test]
    fn scroll_keeps_cursor_in_window() {
        assert_eq!(centred_start(0, 100, 9), 0);
        assert_eq!(centred_start(4, 100, 9), 0);
        assert_eq!(centred_start(10, 100, 9), 6);
        assert_eq!(centred_start(99, 100, 9), 91);
        assert_eq!(centred_start(3, 5, 9), 0);
    }

    #[test]
    fn window_only_moves_to_show_the_cursor() {
        // The offset stays put while the cursor is inside the window.
        assert_eq!(window_start(20, 22, 100, 9), 20);
        assert_eq!(window_start(20, 28, 100, 9), 20);
        // Leaving the window at either edge moves it the least needed.
        assert_eq!(window_start(20, 29, 100, 9), 21);
        assert_eq!(window_start(20, 19, 100, 9), 19);
        // A stale offset past the end is clamped.
        assert_eq!(window_start(500, 99, 100, 9), 91);
        assert_eq!(window_start(50, 0, 5, 9), 0);
    }

    #[test]
    fn wheel_scrolls_the_offset_and_clamps_it() {
        assert_eq!(scroll_by(0, -1, 100, 9), 0);
        assert_eq!(scroll_by(0, 3, 100, 9), 3);
        assert_eq!(scroll_by(90, 5, 100, 9), 91);
        assert_eq!(scroll_by(3, -10, 100, 9), 0);
        assert_eq!(
            scroll_by(0, 5, 4, 9),
            0,
            "a list shorter than the window stays put"
        );
    }

    #[test]
    fn wheel_moves_the_cursor_only_when_it_leaves_the_view() {
        // Window rows 20..29: a cursor inside stays where it is.
        assert_eq!(clamp_into_window(24, 20, 100, 9), 24);
        assert_eq!(clamp_into_window(10, 20, 100, 9), 20);
        assert_eq!(clamp_into_window(40, 20, 100, 9), 28);
        assert_eq!(clamp_into_window(0, 0, 0, 9), 0);
    }

    #[test]
    fn clicking_a_row_in_a_long_list_keeps_it_under_the_pointer() {
        use crate::hit::{hit_at, HitId};
        let vp = Viewport::new(1280, 720);
        let songs: Vec<_> = (0..60).map(song).collect();
        let rows: Vec<SelectRow> = (0..60).map(SelectRow::Song).collect();
        let tables = TableIndex::default();
        let scores = ScoreStore::default();
        let chips: Vec<String> = Vec::new();
        let crumbs = vec!["전체".to_string(), "전체 곡".to_string()];
        let visible = visible_rows(&vp);
        assert!(visible < 60, "the list must be longer than the window");
        let frame = |selected: usize, scroll: usize| SelectFrame {
            viewport: &vp,
            songs: &songs,
            tables: &tables,
            ln_option: LnOption::Auto,
            rows: &rows,
            selected,
            scroll,
            library_empty: false,
            scores: &scores,
            crumbs: &crumbs,
            sort: "TITLE",
            search: "",
            search_active: false,
            preedit: "",
            jacket: None,
            ambient: None,
            option_chips: &chips,
            auto_play: false,
            has_replay: false,
            preview_secs: None,
            filter: FilterBar::default(),
            result_count: None,
            sort_menu: None,
        };
        let mut ui = Ui::new(vp.scale);
        // The cursor on row 0, the window at the top. Find a row lower in the window.
        ui.begin(1280, 720, vp.scale);
        draw_song_select(&mut ui, &frame(0, 0));
        let k = visible - 2;
        let row_rect = ui
            .hits
            .iter()
            .find(|h| h.id == HitId::ListRow(k))
            .expect("row k is on screen")
            .rect;
        let point = (row_rect.x + 40.0, row_rect.y + row_rect.h / 2.0);
        assert_eq!(hit_at(&ui.hits, point.0, point.1), Some(HitId::ListRow(k)));

        // The click selects row k and leaves the window alone, so the same
        // point hits the same row again (a second click plays it).
        ui.begin(1280, 720, vp.scale);
        draw_song_select(&mut ui, &frame(k, 0));
        assert_eq!(hit_at(&ui.hits, point.0, point.1), Some(HitId::ListRow(k)));
    }

    #[test]
    fn hit_regions_follow_the_drawn_layout() {
        use crate::hit::{hit_at, HitId};
        let vp = Viewport::new(1280, 720);
        let songs: Vec<_> = (0..40).map(song).collect();
        let rows: Vec<SelectRow> = (0..40).map(SelectRow::Song).collect();
        let tables = TableIndex::default();
        let scores = ScoreStore::default();
        let chips: Vec<String> = Vec::new();
        let mut ui = Ui::new(vp.scale);
        let frame = SelectFrame {
            viewport: &vp,
            songs: &songs,
            tables: &tables,
            ln_option: LnOption::Auto,
            rows: &rows,
            selected: 5,
            scroll: 0,
            library_empty: false,
            scores: &scores,
            crumbs: &["전체".to_string(), "전체 곡".to_string()],
            sort: "TITLE",
            search: "",
            search_active: false,
            preedit: "",
            jacket: None,
            ambient: None,
            option_chips: &chips,
            auto_play: false,
            has_replay: true,
            preview_secs: None,
            filter: FilterBar::default(),
            result_count: None,
            sort_menu: None,
        };

        ui.begin(1280, 720, vp.scale);
        draw_song_select(&mut ui, &frame);
        let rows: Vec<_> = ui
            .hits
            .iter()
            .filter_map(|h| match h.id {
                HitId::ListRow(i) => Some((i, h.rect)),
                _ => None,
            })
            .collect();
        assert!(!rows.is_empty());
        for (i, r) in &rows {
            let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
            assert_eq!(hit_at(&ui.hits, cx, cy), Some(HitId::ListRow(*i)));
        }
        for id in [
            HitId::PlayOptions,
            HitId::OpenSettings,
            HitId::Search,
            HitId::Sort,
            HitId::FolderPrev,
            HitId::FolderNext,
            HitId::Play,
            HitId::Replay,
        ] {
            let (x, y) = hit_center(&ui.hits, id);
            assert_eq!(hit_at(&ui.hits, x, y), Some(id));
        }

        // Footer key hints are buttons inside the footer (the crumbs make BKSP show).
        let footer_top = vp.y + vp.height - FOOTER_H * vp.scale;
        for id in [
            HitId::Play,
            HitId::FolderUp,
            HitId::PlayOptions,
            HitId::Help,
        ] {
            let r = ui.hits.iter().rev().find(|h| h.id == id).expect("hit").rect;
            assert!(r.y >= footer_top, "{id:?} is outside the footer");
            let (x, y) = hit_center(&ui.hits, id);
            assert_eq!(hit_at(&ui.hits, x, y), Some(id));
        }

        // An open options modal sits above the rows: a click on a row's
        // position outside the panel reaches the blocker, not the row.
        ui.begin(1280, 720, vp.scale);
        draw_song_select(&mut ui, &frame);
        draw_options_modal(&mut ui, &vp, &panel_lines(), 1, ("GAUGE", "help"));
        let (rx, ry) = (rows[0].1.x + 10.0, rows[0].1.y + 10.0);
        assert_eq!(hit_at(&ui.hits, rx, ry), Some(HitId::Blocker));
        assert_eq!(hit_at(&ui.hits, 2.0, 2.0), Some(HitId::Blocker));
        // Empty panel space (above the first row) is the panel, not the blocker.
        let panel = ui
            .hits
            .iter()
            .find(|h| h.id == HitId::ModalPanel)
            .expect("panel")
            .rect;
        assert_eq!(
            hit_at(&ui.hits, panel.x + 4.0, panel.y + 4.0),
            Some(HitId::ModalPanel)
        );
        let (ox, oy) = hit_center(&ui.hits, HitId::OptionNext(1));
        assert_eq!(hit_at(&ui.hits, ox, oy), Some(HitId::OptionNext(1)));
        let (ax, ay) = hit_center(&ui.hits, HitId::OptionRow(0));
        assert_eq!(hit_at(&ui.hits, ax, ay), Some(HitId::OptionRow(0)));

        // The exit prompt: its buttons sit above its own blocker and panel.
        ui.begin(1280, 720, vp.scale);
        draw_exit_modal(&mut ui, &vp);
        let (qx, qy) = hit_center(&ui.hits, HitId::ExitQuit);
        assert_eq!(hit_at(&ui.hits, qx, qy), Some(HitId::ExitQuit));
        let (cx, cy) = hit_center(&ui.hits, HitId::ExitCancel);
        assert_eq!(hit_at(&ui.hits, cx, cy), Some(HitId::ExitCancel));
        assert_eq!(hit_at(&ui.hits, 2.0, 2.0), Some(HitId::Blocker));
    }

    /// Two rows of the play options panel, the first under a section header.
    fn panel_lines() -> Vec<OptionLine<'static>> {
        vec![
            OptionLine {
                column: 0,
                section: Some("플레이"),
                label: "그린 넘버",
                value: "500 ms".into(),
            },
            OptionLine {
                column: 0,
                section: None,
                label: "GAUGE",
                value: "GROOVE".into(),
            },
        ]
    }

    /// Center of the first region recorded as `id`.
    fn hit_center(hits: &[crate::hit::Hit], id: crate::hit::HitId) -> (f32, f32) {
        let r = hits.iter().find(|h| h.id == id).expect("recorded").rect;
        (r.x + r.w / 2.0, r.y + r.h / 2.0)
    }

    #[test]
    fn screen_and_modals_are_one_batch() {
        let vp = Viewport::new(1280, 720);
        let songs: Vec<_> = (0..40).map(song).collect();
        let tables = TableIndex::default();
        let mut scores = ScoreStore::default();
        scores.update(PlayResult {
            chart: beetle_core::ChartId::synthetic(6),
            ln: None,
            lamp: ClearType::FullCombo,
            ex_score: 1900,
            max_combo: 800,
            pgreat_count: 900,
            great_count: 100,
            good_count: 5,
            bad_count: 0,
            poor_count: 0,
            miss_count: 0,
            total_notes: 1010,
            modifier: LaneModifier::Regular,
            gauge: GaugeType::Groove,
            random_seed: None,
            played_at: 0,
        });
        let chips = vec!["그린 500".to_string(), "REGULAR".into(), "GROOVE".into()];
        let mut ui = Ui::new(vp.scale);
        for (selected, search) in [(5, ""), (30, "zzz"), (0, "")] {
            let rows: Vec<SelectRow> = if search.is_empty() {
                (0..40).map(SelectRow::Song).collect()
            } else {
                Vec::new()
            };
            ui.begin(1280, 720, vp.scale);
            let frame = SelectFrame {
                viewport: &vp,
                songs: &songs,
                tables: &tables,
                ln_option: LnOption::Auto,
                rows: &rows,
                selected,
                scroll: 0,
                library_empty: false,
                scores: &scores,
                crumbs: &["전체".to_string(), "전체 곡".to_string()],
                sort: "TITLE",
                search,
                search_active: !search.is_empty(),
                preedit: "",
                jacket: None,
                ambient: None,
                option_chips: &chips,
                auto_play: selected == 0,
                has_replay: selected == 5,
                preview_secs: None,
                filter: FilterBar::default(),
                result_count: None,
                sort_menu: None,
            };
            draw_song_select(&mut ui, &frame);
            draw_options_modal(&mut ui, &vp, &panel_lines(), 1, ("GAUGE", "help"));
            draw_exit_modal(&mut ui, &vp);
            assert_eq!(
                ui.canvas.debug_batches().len(),
                1,
                "selected={selected} search={search:?}"
            );
        }
    }

    #[test]
    fn help_guide_and_drop_overlays_are_one_batch() {
        use crate::hit::hit_at;
        let vp = Viewport::new(1280, 720);
        let songs: Vec<_> = (0..4).map(song).collect();
        let tables = TableIndex::default();
        let scores = ScoreStore::default();
        let chips: Vec<String> = Vec::new();
        let crumbs = vec!["전체".to_string(), "전체 곡".to_string()];
        let mut ui = Ui::new(vp.scale);
        // The first-run guide: an empty library (the demo row is the only one listed).
        let rows: Vec<SelectRow> = vec![SelectRow::Song(0)];
        let guide = SelectFrame {
            viewport: &vp,
            songs: &songs,
            tables: &tables,
            ln_option: LnOption::Auto,
            rows: &rows,
            selected: 0,
            scroll: 0,
            library_empty: true,
            scores: &scores,
            crumbs: &crumbs,
            sort: "TITLE",
            search: "",
            search_active: false,
            preedit: "",
            jacket: None,
            ambient: None,
            option_chips: &chips,
            auto_play: false,
            has_replay: false,
            preview_secs: None,
            filter: FilterBar::default(),
            result_count: None,
            sort_menu: None,
        };
        ui.begin(1280, 720, vp.scale);
        draw_song_select(&mut ui, &guide);
        assert_eq!(ui.canvas.debug_batches().len(), 1, "guide");
        let ids: Vec<HitId> = ui.hits.iter().map(|h| h.id).collect();
        assert!(ids.contains(&HitId::OpenManager));
        assert!(ids.contains(&HitId::OpenSongsFolder));
        assert!(ids.contains(&HitId::Rescan));
        assert!(
            !ids.contains(&HitId::ListRow(0)),
            "the guide replaces the list"
        );

        ui.begin(1280, 720, vp.scale);
        draw_song_select(&mut ui, &guide);
        draw_help_overlay(&mut ui, &vp);
        assert_eq!(ui.canvas.debug_batches().len(), 1, "help");
        // The overlay blocks the footer and the list; its panel swallows clicks.
        let blocker = ui.hits.iter().rev().find(|h| h.id == HitId::Blocker);
        assert!(blocker.is_some());
        let footer_help = ui
            .hits
            .iter()
            .find(|h| h.id == HitId::Help)
            .expect("footer ?");
        let (hx, hy) = (
            footer_help.rect.x + footer_help.rect.w / 2.0,
            footer_help.rect.y + footer_help.rect.h / 2.0,
        );
        assert_eq!(hit_at(&ui.hits, hx, hy), Some(HitId::Blocker));
        let panel = ui
            .hits
            .iter()
            .find(|h| h.id == HitId::ModalPanel)
            .expect("panel")
            .rect;
        assert_eq!(
            hit_at(&ui.hits, panel.x + 2.0, panel.y + 2.0),
            Some(HitId::ModalPanel)
        );

        ui.begin(1280, 720, vp.scale);
        draw_drop_overlay(&mut ui, &vp);
        assert_eq!(ui.canvas.debug_batches().len(), 1, "drop");
    }

    #[test]
    fn help_lists_every_row_with_a_key_or_a_caption() {
        assert_eq!(HELP_GROUPS.len(), 6);
        for group in &HELP_GROUPS {
            assert!(!group.rows.is_empty(), "{}", group.title);
            for row in group.rows {
                assert!(
                    !row.keys.is_empty() || !row.caption.is_empty(),
                    "{}: {}",
                    group.title,
                    row.text
                );
            }
        }
    }

    #[test]
    fn ime_caret_sits_in_the_search_box_after_the_preedit() {
        use crate::hit::HitId;
        let vp = Viewport::new(1280, 720);
        let songs: Vec<_> = (0..4).map(song).collect();
        let rows: Vec<SelectRow> = (0..4).map(SelectRow::Song).collect();
        let crumbs = vec!["전체".to_string(), "전체 곡".to_string()];
        let tables = TableIndex::default();
        let scores = ScoreStore::default();
        let chips: Vec<String> = Vec::new();
        let mut ui = Ui::new(vp.scale);
        let frame = |search: &'static str, preedit: &'static str, active: bool| SelectFrame {
            viewport: &vp,
            songs: &songs,
            tables: &tables,
            ln_option: LnOption::Auto,
            rows: &rows,
            selected: 0,
            scroll: 0,
            library_empty: false,
            scores: &scores,
            crumbs: &crumbs,
            sort: "TITLE",
            search,
            search_active: active,
            preedit,
            jacket: None,
            ambient: None,
            option_chips: &chips,
            auto_play: false,
            has_replay: false,
            preview_secs: None,
            filter: FilterBar::default(),
            result_count: None,
            sort_menu: None,
        };

        ui.begin(1280, 720, vp.scale);
        draw_song_select(&mut ui, &frame("", "", false));
        assert_eq!(ui.ime_caret, None, "no caret while the search is closed");

        ui.begin(1280, 720, vp.scale);
        draw_song_select(&mut ui, &frame("가을", "밤", true));
        let caret = ui.ime_caret.expect("caret while searching");
        let boxed = ui
            .hits
            .iter()
            .find(|h| h.id == HitId::Search)
            .expect("search box")
            .rect;
        assert!(
            caret.x > boxed.x && caret.x < boxed.right(),
            "{caret:?} in {boxed:?}"
        );
        assert!(caret.y >= boxed.y && caret.bottom() <= boxed.bottom());

        // The preedit moves the caret right of the committed query alone.
        ui.begin(1280, 720, vp.scale);
        draw_song_select(&mut ui, &frame("가을", "", true));
        let without = ui.ime_caret.expect("caret");
        assert!(caret.x > without.x);
    }
}

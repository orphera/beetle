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
use beetle_core::{LnOption, Ruleset, ScoreRecord, ScoreStore, SongMetadata, TableIndex};

/// Everything the song select screen shows for one frame.
pub struct SelectFrame<'a> {
    pub viewport: &'a Viewport,
    /// The whole library; `visible` indexes into it (filter + sort order).
    pub songs: &'a [SongMetadata],
    pub visible: &'a [usize],
    /// Cursor position within `visible`.
    pub selected: usize,
    pub scores: &'a ScoreStore,
    /// The player's long note setting, which decides which record of a chart with long notes is shown.
    pub ln_option: LnOption,
    /// Installed difficulty tables, matched to the song list (level chips).
    pub tables: &'a TableIndex,
    pub folder: &'a str,
    pub sort: &'a str,
    pub search: &'a str,
    pub search_active: bool,
    /// IME composition text shown after the query (underlined); empty when none.
    pub preedit: &'a str,
    /// Stage image of the selected song, once loaded.
    pub jacket: Option<SizedTexture>,
    /// Dominant color of that image (tints the ambient light).
    pub ambient: Option<ColorRgba>,
    /// Current play options shown above the PLAY button ("HI-SPEED 1100", ...).
    pub option_chips: &'a [String],
    pub auto_play: bool,
    pub has_replay: bool,
    /// Seconds the selected song's audio preview has been playing; `None` when it is not.
    pub preview_secs: Option<f32>,
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
    let selected = f.visible.get(f.selected).and_then(|&i| f.songs.get(i));

    backdrop(c, &sk, f, selected, lite);

    let content = Rect::from_ltrb(
        vp.x + PAD * s,
        vp.y + (TOPBAR_H + 24.0) * s,
        vp.x + vp.width - PAD * s,
        vp.y + vp.height - (FOOTER_H + 24.0) * s,
    );
    match selected {
        Some(song) => {
            let list = Rect::new(content.x, content.y, LIST_W * s, content.h);
            song_list(c, t, &sk, f, list, s, &mut hs);
            let detail = Rect::from_ltrb(
                list.right() + PAD * s,
                content.y,
                content.right(),
                content.bottom(),
            );
            detail_panel(c, t, &sk, f, song, detail, s, &mut hs);
        }
        None => empty_state(c, t, f, content, s),
    }

    ui.ime_caret = top_bar(c, t, &sk, f, s, &mut hs);
    footer(c, t, &sk, f, s, &mut hs);
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

fn top_bar(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    f: &SelectFrame,
    s: f32,
    hs: &mut HitSink,
) -> Option<Rect> {
    let vp = f.viewport;
    let x0 = vp.x + PAD * s;
    let adv = widgets::top_bar(c, t, vp, strings::WORDMARK, s);
    let bar_top = vp.y + 16.0 * s;
    let bar_h = 32.0 * s;

    // Folder and sort read as "LABEL  ‹ value ›" selectors.
    let mut x = x0 + adv + 48.0 * s;
    for (label, value, arrows) in [
        (strings::FOLDER, f.folder, true),
        (strings::SORT, f.sort, false),
    ] {
        let cap = caption(10.0, s);
        let start = x;
        x += t.draw(c, label, x, vp.y + 37.0 * s, &cap) + 12.0 * s;
        let icon = 16.0 * s;
        let iy = vp.y + 32.0 * s - icon / 2.0;
        if arrows {
            let arrow = Rect::new(x - 4.0 * s, iy, icon, icon);
            let hit = Rect::new(arrow.x - 6.0 * s, bar_top, icon + 12.0 * s, bar_h);
            hs.add(hit, HitId::FolderPrev);
            let col = if hs.hovered(hit) {
                theme::TEXT
            } else {
                theme::MUTED
            };
            c.sprite(sk.icons.chevron_left, arrow, col);
            x += icon;
        }
        let st = TextStyle::new(13.0 * s).bold().color(theme::TEXT);
        x += t.draw(c, value, x, vp.y + 37.0 * s, &st);
        if arrows {
            let arrow = Rect::new(x + 4.0 * s, iy, icon, icon);
            let hit = Rect::new(arrow.x - 6.0 * s, bar_top, icon + 12.0 * s, bar_h);
            hs.add(hit, HitId::FolderNext);
            let col = if hs.hovered(hit) {
                theme::TEXT
            } else {
                theme::MUTED
            };
            c.sprite(sk.icons.chevron_right, arrow, col);
            x += icon + 4.0 * s;
        } else {
            let hit = Rect::new(start - 8.0 * s, bar_top, x - start + 16.0 * s, bar_h);
            hs.add(hit, HitId::Sort);
            if hs.hovered(hit) {
                c.stroke_rect(hit, s.max(1.0), theme::CYAN.with_alpha(90));
            }
        }
        x += 32.0 * s;
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
    caret
}

// ---------------------------------------------------------------------------
// Song list
// ---------------------------------------------------------------------------

/// First visible row so the cursor sits near the middle of the window.
fn scroll_start(selected: usize, total: usize, rows: usize) -> usize {
    selected
        .saturating_sub(rows / 2)
        .min(total.saturating_sub(rows))
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
    let rows = (((list.h + ROW_GAP * s) / step).floor() as usize).max(1);
    let total = f.visible.len();
    let start = scroll_start(f.selected, total, rows);
    let row_w = list.w - 16.0 * s;

    for (slot, &idx) in f.visible.iter().enumerate().skip(start).take(rows) {
        let Some(song) = f.songs.get(idx) else {
            continue;
        };
        let row = Rect::new(
            list.x,
            list.y + (slot - start) as f32 * step,
            row_w,
            ROW_H * s,
        );
        hs.add(row, HitId::SongRow(slot));
        let chip = f.tables.chip(song.id);
        song_row(
            c,
            t,
            sk,
            song,
            f.scores.best(song, f.ln_option),
            chip.as_deref(),
            row,
            slot == f.selected,
            slot != f.selected && hs.hovered(row),
            s,
        );
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
    let st = caption(9.0, s).color(theme::PURPLE);
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

    // Clear lamp: a strip on the left edge (IIDX convention).
    let (_, lamp) = theme::clear_lamp(best.map(|b| b.clear_type));
    let lamp = if best.is_some() { lamp } else { theme::LINE };
    c.nine(
        &sk.panel_sm,
        Rect::new(row.x + 6.0 * s, row.y + 10.0 * s, 4.0 * s, row.h - 20.0 * s),
        lamp,
    );

    // Level badge
    let (_, diff) = theme::level_tier(song.play_level);
    let badge = Rect::new(row.x + 20.0 * s, row.y + 10.0 * s, 40.0 * s, 32.0 * s);
    c.nine(&sk.panel_sm, badge, diff.with_alpha(40));
    c.nine(
        &sk.panel_sm,
        Rect::new(badge.x, badge.bottom() - 3.0 * s, badge.w, 3.0 * s),
        diff,
    );
    t.draw_in(
        c,
        &song.play_level.to_string(),
        badge,
        Align::Center,
        &TextStyle::new(17.0 * s).bold().color(diff),
    );

    // Title + artist
    let tx = badge.right() + 16.0 * s;
    let right_w = 112.0 * s;
    let text_w = row.right() - right_w - 16.0 * s - tx;
    let title_st = TextStyle::new(16.0 * s).bold().color(if on {
        theme::TEXT
    } else {
        theme::TEXT.with_alpha(215)
    });
    let title = t.fit(c, &song.title, text_w, &title_st).into_owned();
    t.draw(c, &title, tx, row.y + 24.0 * s, &title_st);
    let sub_st = TextStyle::new(12.0 * s).color(theme::MUTED);
    let mode = theme::mode_label(song.play_mode);
    let mode_w = t.draw(
        c,
        mode,
        tx,
        row.y + 42.0 * s,
        &caption(10.0, s).color(if on { theme::CYAN } else { theme::MUTED }),
    );
    let mut ax = tx + mode_w + 8.0 * s;
    if let Some(chip) = table_chip {
        ax += level_chip(c, t, chip, ax, row.y + 42.0 * s, s) + 8.0 * s;
    }
    let artist = t
        .fit(c, &song.artist, text_w - (ax - tx), &sub_st)
        .into_owned();
    t.draw(c, &artist, ax, row.y + 42.0 * s, &sub_st);

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
        None => {
            t.draw_in(
                c,
                strings::NO_PLAY,
                Rect::new(rx, row.y, right_w, row.h),
                Align::Right,
                &caption(10.0, s).color(theme::MUTED),
            );
        }
    }
}

fn empty_state(c: &mut Canvas, t: &mut TextEngine, f: &SelectFrame, area: Rect, s: f32) {
    let (head, hint) = if f.search.is_empty() {
        (strings::EMPTY_FOLDER.to_string(), strings::EMPTY_HINT)
    } else {
        (
            strings::fill(strings::NO_MATCH, &[&f.search]),
            strings::SEARCH_HINT,
        )
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
}

// ---------------------------------------------------------------------------
// Detail panel
// ---------------------------------------------------------------------------

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
        &caption(9.0, s).color(theme::CYAN),
    );
}

#[allow(clippy::too_many_arguments)]
fn detail_panel(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    f: &SelectFrame,
    song: &SongMetadata,
    panel: Rect,
    s: f32,
    hs: &mut HitSink,
) {
    c.halo(&sk.shadow, panel, theme::WHITE.with_alpha(160));
    c.nine(&sk.panel_lg, panel, theme::SURF1.with_alpha(235));
    let inner = panel.inset(24.0 * s);
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
        &caption(11.0, s).color(tier_col),
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
    let label_st = caption(10.0, s).color(theme::MUTED);
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
        let st = caption(10.0, s).color(theme::MUTED);
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
        let st = caption(10.0, s).color(theme::ON_ACCENT);
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
        &caption(10.0, s),
    );
    let Some(b) = best else {
        if let Some(rule) = rule {
            t.draw(
                c,
                rule,
                area.x + header_w + 14.0 * s,
                y + 32.0 * s,
                &caption(9.0, s).color(theme::MUTED),
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
        let st = caption(9.0, s).color(theme::MUTED);
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
    let st = caption(10.0, s).color(lamp_col);
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
        t.draw(c, k, sx, y + 134.0 * s, &caption(10.0, s));
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

const HINTS: [widgets::Hint; 10] = [
    ("↑↓", strings::FOOTER_MOVE, None),
    ("ENTER", strings::FOOTER_PLAY, Some(HitId::Play)),
    ("/", strings::FOOTER_SEARCH, Some(HitId::Search)),
    ("F1 F3", strings::FOOTER_FOLDER, Some(HitId::FolderNext)),
    ("F2", strings::FOOTER_SORT, Some(HitId::Sort)),
    ("TAB", strings::OPTIONS, Some(HitId::PlayOptions)),
    ("F4", strings::FOOTER_SETTINGS, Some(HitId::OpenSettings)),
    ("A", strings::FOOTER_AUTO, Some(HitId::Auto)),
    ("F12", strings::FOOTER_KEYS, Some(HitId::KeyConfig)),
    ("ESC", strings::FOOTER_QUIT, Some(HitId::Quit)),
];
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

    // Song count (filtered / library)
    let mut x = vp.x + PAD * s;
    x += t.draw(
        c,
        &thousands(f.visible.len() as u32),
        x,
        base,
        &TextStyle::new(13.0 * s).bold().color(theme::TEXT),
    );
    let total = strings::fill(strings::SONGS_TOTAL, &[&thousands(f.songs.len() as u32)]);
    t.draw(c, &total, x, base, &caption(10.0, s));

    // Key hints, right-aligned; drop from the left if they do not fit.
    let right = vp.x + vp.width - PAD * s;
    let left_limit = x + 200.0 * s;
    let mut first = 0;
    while first < HINTS.len() && right - widgets::hints_width(c, t, &HINTS[first..], s) < left_limit
    {
        first += 1;
    }
    widgets::footer_buttons(c, t, sk, &HINTS[first..], bar, s, hs);
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
    let sk = ui.skin;
    let mut hs = HitSink::new(&mut ui.hits, ui.pointer);
    let (c, t) = (&mut ui.canvas, &mut ui.text);
    let s = vp.scale;
    let row_h = 34.0 * s;
    let section_h = 30.0 * s;
    // Rows, section headers, then the help card and the hint line below them.
    let body: f32 = lines
        .iter()
        .map(|l| row_h + if l.section.is_some() { section_h } else { 0.0 })
        .sum();
    let h = (body + (widgets::HELP_CARD_H + 136.0) * s).min(vp.height - 32.0 * s);
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

    let hints = [
        ("↑↓", strings::HINT_MOVE),
        (LEFT_RIGHT, strings::HINT_CHANGE),
        ("TAB", strings::HINT_CLOSE),
    ];
    let w = hint_row(c, t, &sk, &hints, 0.0, 0.0, s, false);
    hint_row(
        c,
        t,
        &sk,
        &hints,
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
        assert_eq!(scroll_start(0, 100, 9), 0);
        assert_eq!(scroll_start(4, 100, 9), 0);
        assert_eq!(scroll_start(10, 100, 9), 6);
        assert_eq!(scroll_start(99, 100, 9), 91);
        assert_eq!(scroll_start(3, 5, 9), 0);
    }

    #[test]
    fn hit_regions_follow_the_drawn_layout() {
        use crate::hit::{hit_at, HitId};
        let vp = Viewport::new(1280, 720);
        let songs: Vec<_> = (0..40).map(song).collect();
        let visible: Vec<_> = (0..40).collect();
        let tables = TableIndex::default();
        let scores = ScoreStore::default();
        let chips: Vec<String> = Vec::new();
        let mut ui = Ui::new(vp.scale);
        let frame = SelectFrame {
            viewport: &vp,
            songs: &songs,
            tables: &tables,
            ln_option: LnOption::Auto,
            visible: &visible,
            selected: 5,
            scores: &scores,
            folder: "ALL SONGS",
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
        };

        ui.begin(1280, 720, vp.scale);
        draw_song_select(&mut ui, &frame);
        let rows: Vec<_> = ui
            .hits
            .iter()
            .filter_map(|h| match h.id {
                HitId::SongRow(i) => Some((i, h.rect)),
                _ => None,
            })
            .collect();
        assert!(!rows.is_empty());
        for (i, r) in &rows {
            let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
            assert_eq!(hit_at(&ui.hits, cx, cy), Some(HitId::SongRow(*i)));
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

        // Footer key hints are buttons inside the footer.
        let footer_top = vp.y + vp.height - FOOTER_H * vp.scale;
        for id in [
            HitId::Play,
            HitId::Search,
            HitId::FolderNext,
            HitId::Sort,
            HitId::PlayOptions,
            HitId::OpenSettings,
            HitId::Auto,
            HitId::KeyConfig,
            HitId::Quit,
        ] {
            let r = ui.hits.iter().rev().find(|h| h.id == id).expect("hit").rect;
            if matches!(id, HitId::Auto | HitId::KeyConfig | HitId::Quit) {
                assert!(r.y >= footer_top, "{id:?} is outside the footer");
                let (x, y) = hit_center(&ui.hits, id);
                assert_eq!(hit_at(&ui.hits, x, y), Some(id));
            }
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
                label: "HI-SPEED",
                value: "1100 px/s".into(),
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
        let visible: Vec<_> = (0..40).collect();
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
        let chips = vec![
            "HI-SPEED 1100".to_string(),
            "REGULAR".into(),
            "GROOVE".into(),
        ];
        let mut ui = Ui::new(vp.scale);
        for (selected, search) in [(5, ""), (30, "zzz"), (0, "")] {
            let visible = if search.is_empty() {
                &visible[..]
            } else {
                &[][..]
            };
            ui.begin(1280, 720, vp.scale);
            let frame = SelectFrame {
                viewport: &vp,
                songs: &songs,
                tables: &tables,
                ln_option: LnOption::Auto,
                visible,
                selected,
                scores: &scores,
                folder: "ALL SONGS",
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
    fn ime_caret_sits_in_the_search_box_after_the_preedit() {
        use crate::hit::HitId;
        let vp = Viewport::new(1280, 720);
        let songs: Vec<_> = (0..4).map(song).collect();
        let visible: Vec<_> = (0..4).collect();
        let tables = TableIndex::default();
        let scores = ScoreStore::default();
        let chips: Vec<String> = Vec::new();
        let mut ui = Ui::new(vp.scale);
        let frame = |search: &'static str, preedit: &'static str, active: bool| SelectFrame {
            viewport: &vp,
            songs: &songs,
            tables: &tables,
            ln_option: LnOption::Auto,
            visible: &visible,
            selected: 0,
            scores: &scores,
            folder: "ALL SONGS",
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

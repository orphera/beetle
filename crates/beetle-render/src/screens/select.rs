//! Song select screen on the `Ui` (Canvas + TextEngine + generated Skin),
//! plus the two modals that open on top of it (play options, quit).
//! Replaces `song_select.rs` and the option / exit modals in `modals.rs`.
//!
//! Layout is designed at 1280×720 and multiplied by the viewport scale.
//! Visual reference: the menu composition in `tests/d3d11_skin.rs`.

use crate::art::Skin;
use crate::canvas::{Canvas, Rect};
use crate::view::Viewport;
use crate::screens::play::{cover_uv, SizedTexture};
use crate::skin::ColorRgba;
use crate::text::{Align, TextEngine, TextStyle};
use crate::theme::{self, caption, thousands};
use crate::ui::Ui;
use super::widgets::{self, hint_row, keycap, keycap_width, wrap2, LEFT_RIGHT};
use beetle_core::{ScoreRecord, ScoreStore, SongMetadata};

/// Everything the song select screen shows for one frame.
pub struct SelectFrame<'a> {
    pub viewport: &'a Viewport,
    /// The whole library; `visible` indexes into it (filter + sort order).
    pub songs: &'a [SongMetadata],
    pub visible: &'a [usize],
    /// Cursor position within `visible`.
    pub selected: usize,
    pub scores: &'a ScoreStore,
    pub folder: &'a str,
    pub sort: &'a str,
    pub search: &'a str,
    pub search_active: bool,
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
            song_list(c, t, &sk, f, list, s);
            let detail = Rect::from_ltrb(list.right() + PAD * s, content.y, content.right(), content.bottom());
            detail_panel(c, t, &sk, f, song, detail, s);
        }
        None => empty_state(c, t, f, content, s),
    }

    top_bar(c, t, &sk, f, s);
    footer(c, t, &sk, f, s);
}

fn backdrop(c: &mut Canvas, sk: &Skin, f: &SelectFrame, song: Option<&SongMetadata>, lite: bool) {
    let tier = song.map_or(theme::MAGENTA, |s| theme::level_tier(s.play_level).1);
    let ambient = f.ambient.map_or(tier, |a| theme::vivid(a, tier));
    widgets::backdrop(c, sk, f.viewport, ambient, lite);
}

// ---------------------------------------------------------------------------
// Top bar: wordmark, folder / sort selectors, search box
// ---------------------------------------------------------------------------

fn top_bar(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, f: &SelectFrame, s: f32) {
    let vp = f.viewport;
    let x0 = vp.x + PAD * s;
    let adv = widgets::top_bar(c, t, vp, "BEETLE", s);

    // Folder and sort read as "LABEL  ‹ value ›" selectors.
    let mut x = x0 + adv + 48.0 * s;
    for (label, value, arrows) in [("FOLDER", f.folder, true), ("SORT", f.sort, false)] {
        let cap = caption(10.0, s);
        x += t.draw(c, label, x, vp.y + 37.0 * s, &cap) + 12.0 * s;
        let icon = 16.0 * s;
        let iy = vp.y + 32.0 * s - icon / 2.0;
        if arrows {
            c.sprite(sk.icons.chevron_left, Rect::new(x - 4.0 * s, iy, icon, icon), theme::MUTED);
            x += icon;
        }
        let st = TextStyle::new(13.0 * s).bold().tracking(1.0 * s).color(theme::TEXT);
        x += t.draw(c, value, x, vp.y + 37.0 * s, &st);
        if arrows {
            c.sprite(sk.icons.chevron_right, Rect::new(x + 4.0 * s, iy, icon, icon), theme::MUTED);
            x += icon + 4.0 * s;
        }
        x += 32.0 * s;
    }

    // Search box
    let search = Rect::new(vp.x + vp.width - (PAD + 280.0) * s, vp.y + 16.0 * s, 280.0 * s, 32.0 * s);
    c.nine(&sk.panel_lg, search, if f.search_active { theme::SURF3 } else { theme::SURF2 });
    if f.search_active {
        c.nine(&sk.panel_outline, search, theme::CYAN.with_alpha(200));
    }
    let inner = Rect::new(search.x + 16.0 * s, search.y, search.w - 52.0 * s, search.h);
    if f.search.is_empty() && !f.search_active {
        t.draw_in(c, "Search title, artist", inner, Align::Left, &TextStyle::new(13.0 * s).color(theme::MUTED2));
    } else {
        let st = TextStyle::new(13.0 * s).color(theme::TEXT);
        // Keep the end of a long query (where the caret is) visible.
        let mut q = f.search;
        while !q.is_empty() && t.measure(c, q, &st) > inner.w - 8.0 * s {
            let mut it = q.chars();
            it.next();
            q = it.as_str();
        }
        let w = t.draw_in(c, q, inner, Align::Left, &st);
        if f.search_active {
            c.fill_rect(Rect::new(inner.x + w + 2.0 * s, search.y + 8.0 * s, 2.0 * s, 16.0 * s), theme::CYAN);
        }
    }
    let key = Rect::new(search.right() - 32.0 * s, search.y + 6.0 * s, 20.0 * s, 20.0 * s);
    keycap(c, t, sk, "/", key, s);
}

// ---------------------------------------------------------------------------
// Song list
// ---------------------------------------------------------------------------

/// First visible row so the cursor sits near the middle of the window.
fn scroll_start(selected: usize, total: usize, rows: usize) -> usize {
    selected.saturating_sub(rows / 2).min(total.saturating_sub(rows))
}

fn song_list(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, f: &SelectFrame, list: Rect, s: f32) {
    let step = (ROW_H + ROW_GAP) * s;
    let rows = (((list.h + ROW_GAP * s) / step).floor() as usize).max(1);
    let total = f.visible.len();
    let start = scroll_start(f.selected, total, rows);
    let row_w = list.w - 16.0 * s;

    for (slot, &idx) in f.visible.iter().enumerate().skip(start).take(rows) {
        let Some(song) = f.songs.get(idx) else { continue };
        let row = Rect::new(list.x, list.y + (slot - start) as f32 * step, row_w, ROW_H * s);
        song_row(c, t, sk, song, f.scores.get(song.hash), row, slot == f.selected, s);
    }

    // Scrollbar
    if total > rows {
        let track = Rect::new(list.right() - 4.0 * s, list.y, 3.0 * s, rows as f32 * step - ROW_GAP * s);
        c.nine(&sk.panel_sm, track, theme::SURF2);
        let thumb_h = (track.h * rows as f32 / total as f32).max(24.0 * s);
        let thumb_y = track.y + (track.h - thumb_h) * start as f32 / (total - rows) as f32;
        c.nine(&sk.panel_sm, Rect::new(track.x, thumb_y, track.w, thumb_h), theme::CYAN.with_alpha(200));
    }
}

#[allow(clippy::too_many_arguments)]
fn song_row(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    song: &SongMetadata,
    best: Option<&ScoreRecord>,
    row: Rect,
    on: bool,
    s: f32,
) {
    if on {
        c.halo(&sk.shadow, row, theme::WHITE.with_alpha(200));
        c.nine(&sk.panel, row, theme::SURF3);
        c.push_clip(row);
        c.fill_rect_hgradient(Rect::new(row.x, row.y, row.w * 0.6, row.h), theme::CYAN.with_alpha(40), theme::CYAN.with_alpha(0));
        c.pop_clip();
        c.nine(&sk.panel_outline, row, theme::CYAN.with_alpha(200));
        c.set_additive(true);
        c.sprite_centered(sk.glow, row.x, row.y + row.h / 2.0, 80.0 * s, 120.0 * s, theme::CYAN.with_alpha(110));
        c.set_additive(false);
    } else {
        c.nine(&sk.panel, row, theme::SURF1.with_alpha(220));
    }

    // Clear lamp: a strip on the left edge (IIDX convention).
    let (_, lamp) = theme::clear_lamp(best.map(|b| b.clear_type));
    let lamp = if best.is_some() { lamp } else { theme::LINE };
    c.nine(&sk.panel_sm, Rect::new(row.x + 6.0 * s, row.y + 10.0 * s, 4.0 * s, row.h - 20.0 * s), lamp);

    // Level badge
    let (_, diff) = theme::level_tier(song.play_level);
    let badge = Rect::new(row.x + 20.0 * s, row.y + 10.0 * s, 40.0 * s, 32.0 * s);
    c.nine(&sk.panel_sm, badge, diff.with_alpha(40));
    c.nine(&sk.panel_sm, Rect::new(badge.x, badge.bottom() - 3.0 * s, badge.w, 3.0 * s), diff);
    t.draw_in(c, &song.play_level.to_string(), badge, Align::Center, &TextStyle::new(17.0 * s).bold().color(diff));

    // Title + artist
    let tx = badge.right() + 16.0 * s;
    let right_w = 112.0 * s;
    let text_w = row.right() - right_w - 16.0 * s - tx;
    let title_st = TextStyle::new(16.0 * s).bold().color(if on { theme::TEXT } else { theme::TEXT.with_alpha(215) });
    let title = t.fit(c, &song.title, text_w, &title_st).into_owned();
    t.draw(c, &title, tx, row.y + 24.0 * s, &title_st);
    let sub_st = TextStyle::new(12.0 * s).color(theme::MUTED);
    let mode = theme::mode_label(song.play_mode);
    let mode_w = t.draw(c, mode, tx, row.y + 42.0 * s, &caption(10.0, s).color(if on { theme::CYAN } else { theme::MUTED2 }));
    let ax = tx + mode_w + 8.0 * s;
    let artist = t.fit(c, &song.artist, text_w - (ax - tx), &sub_st).into_owned();
    t.draw(c, &artist, ax, row.y + 42.0 * s, &sub_st);

    // Personal best: rank + score-rate bar, or "NO PLAY".
    let rx = row.right() - right_w - 16.0 * s;
    match best {
        Some(b) => {
            let (rank, rank_col) = theme::rank(b.accuracy_rate());
            t.draw_in(c, rank, Rect::new(rx, row.y + 8.0 * s, right_w, 22.0 * s), Align::Right, &TextStyle::new(16.0 * s).bold().color(rank_col));
            let bar = Rect::new(rx, row.y + 36.0 * s, right_w, 4.0 * s);
            c.nine(&sk.panel_sm, bar, theme::LINE);
            let rate = (b.accuracy_rate() / 100.0).clamp(0.0, 1.0) as f32;
            c.nine(&sk.panel_sm, Rect::new(bar.x, bar.y, bar.w * rate, bar.h), rank_col);
        }
        None => {
            t.draw_in(c, "NO PLAY", Rect::new(rx, row.y, right_w, row.h), Align::Right, &caption(10.0, s));
        }
    }
}

fn empty_state(c: &mut Canvas, t: &mut TextEngine, f: &SelectFrame, area: Rect, s: f32) {
    let (head, hint) = if f.search.is_empty() {
        ("No songs in this folder".to_string(), "Put .bms / .bme / .bmsp files into the songs folder, then press F5 to rescan.")
    } else {
        (format!("No songs match \"{}\"", f.search), "Press / to edit the search, Esc to clear it.")
    };
    let cy = area.y + area.h * 0.42;
    let head_st = TextStyle::new(22.0 * s).bold().color(theme::TEXT);
    let head = t.fit(c, &head, area.w, &head_st).into_owned();
    t.draw_in(c, &head, Rect::new(area.x, cy - 20.0 * s, area.w, 30.0 * s), Align::Center, &head_st);
    t.draw_in(c, hint, Rect::new(area.x, cy + 16.0 * s, area.w, 20.0 * s), Align::Center, &TextStyle::new(13.0 * s).color(theme::MUTED));
}

// ---------------------------------------------------------------------------
// Detail panel
// ---------------------------------------------------------------------------

/// "PREVIEW" pill with a small level meter in the jacket's lower-left corner.
fn preview_badge(c: &mut Canvas, t: &mut TextEngine, jacket: Rect, secs: f32, s: f32) {
    let pill = Rect::new(jacket.x + 8.0 * s, jacket.bottom() - 28.0 * s, 86.0 * s, 20.0 * s);
    c.fill_rect(pill, theme::BG.with_alpha(205));
    c.stroke_rect(pill, s.max(1.0), theme::CYAN.with_alpha(110));
    let (bar_w, gap, floor) = (3.0 * s, 2.0 * s, pill.bottom() - 5.0 * s);
    for i in 0..4 {
        let phase = secs * 7.0 + i as f32 * 1.7;
        let h = (3.0 + 8.0 * (0.5 + 0.5 * phase.sin())) * s;
        c.fill_rect(Rect::new(pill.x + 7.0 * s + i as f32 * (bar_w + gap), floor - h, bar_w, h), theme::CYAN);
    }
    t.draw(c, "PREVIEW", pill.x + 30.0 * s, pill.bottom() - 6.0 * s, &caption(9.0, s).color(theme::CYAN));
}

fn detail_panel(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, f: &SelectFrame, song: &SongMetadata, panel: Rect, s: f32) {
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
            c.fill_rect_corners(jacket, [tier_col, theme::SURF3, theme::BG, tier_col.with_alpha(160)]);
            c.set_additive(true);
            c.sprite_centered(sk.flare, jacket.x + jacket.w * 0.7, jacket.y + jacket.h * 0.3, 160.0 * s, 160.0 * s, theme::WHITE.with_alpha(90));
            c.set_additive(false);
            t.draw_in(c, theme::mode_label(song.play_mode), jacket.inset(12.0 * s), Align::Left, &TextStyle::new(44.0 * s).bold().color(theme::WHITE.with_alpha(40)));
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
    t.draw(c, &tier_txt, ix, inner.y + 14.0 * s, &caption(11.0, s).color(tier_col));
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
        let st = TextStyle::new(12.0 * s).color(theme::MUTED2);
        let genre = t.fit(c, &song.genre, iw, &st).into_owned();
        t.draw(c, &genre, ix, y + 18.0 * s, &st);
    }

    // Chart stats, aligned to the jacket's bottom edge
    let col_w = iw / 3.0;
    let bpm = song.bpm_label();
    let notes = thousands(song.notes_count as u32);
    for (i, (k, v)) in [("BPM", bpm.as_str()), ("NOTES", notes.as_str()), ("MODE", theme::mode_label(song.play_mode))].iter().enumerate() {
        let sx = ix + i as f32 * col_w;
        t.draw(c, k, sx, jacket.bottom() - 30.0 * s, &caption(10.0, s));
        t.draw(c, v, sx, jacket.bottom() - 2.0 * s, &TextStyle::new(22.0 * s).bold().color(theme::TEXT));
    }

    let rule_y = jacket.bottom() + 24.0 * s;
    c.fill_rect(Rect::new(inner.x, rule_y, inner.w, s.max(1.0)), theme::LINE);
    personal_best(c, t, sk, song, f.scores.get(song.hash), Rect::new(inner.x, rule_y, inner.w, 150.0 * s), s);

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
        let w = t.measure(c, "AUTO PLAY", &st) + 20.0 * s;
        let r = Rect::new(inner.right() - w, chips_y, w, 24.0 * s);
        c.nine(&sk.panel_lg, r, theme::CYAN);
        t.draw_in(c, "AUTO PLAY", r, Align::Center, &st);
    }

    if f.has_replay {
        let rw = 132.0 * s;
        let replay = Rect::new(cta.right() - rw, cta.y, rw, cta.h);
        cta.w -= rw + 12.0 * s;
        c.nine(&sk.cut_panel, replay, theme::SURF2);
        c.nine(&sk.cut_outline, replay, theme::LINE);
        let st = TextStyle::new(13.0 * s).bold().tracking(2.0 * s).color(theme::TEXT);
        let lw = t.measure(c, "REPLAY", &st);
        let kw = 20.0 * s;
        let x = replay.x + (replay.w - lw - kw - 10.0 * s) / 2.0;
        t.draw_in(c, "REPLAY", Rect::new(x, replay.y, lw, replay.h), Align::Left, &st);
        keycap(c, t, sk, "R", Rect::new(x + lw + 10.0 * s, replay.y + (replay.h - kw) / 2.0, kw, kw), s);
    }
    c.set_additive(true);
    c.sprite_centered(sk.glow, cta.x + cta.w / 2.0, cta.y + cta.h / 2.0, cta.w * 1.1, 140.0 * s, theme::CYAN.with_alpha(55));
    c.set_additive(false);
    c.nine_hgradient(&sk.cut_panel, cta, theme::CYAN, theme::BLUE);
    c.sprite(sk.icons.play, Rect::new(cta.x + 22.0 * s, cta.y + 12.0 * s, 28.0 * s, 28.0 * s), theme::ON_ACCENT);
    let label = if f.auto_play { "AUTO PLAY" } else { "PLAY" };
    t.draw_in(c, label, Rect::new(cta.x + 60.0 * s, cta.y, cta.w - 160.0 * s, cta.h), Align::Left, &TextStyle::new(20.0 * s).bold().tracking(4.0 * s).color(theme::ON_ACCENT));
    t.draw_in(c, "ENTER", Rect::new(cta.right() - 120.0 * s, cta.y, 96.0 * s, cta.h), Align::Right, &TextStyle::new(12.0 * s).bold().tracking(2.0 * s).color(theme::ON_ACCENT.with_alpha(150)));
}

fn personal_best(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, song: &SongMetadata, best: Option<&ScoreRecord>, area: Rect, s: f32) {
    let y = area.y;
    let header_w = t.draw(c, "PERSONAL BEST", area.x, y + 32.0 * s, &caption(10.0, s));
    let Some(b) = best else {
        t.draw(c, "Not played yet", area.x, y + 70.0 * s, &TextStyle::new(18.0 * s).bold().color(theme::MUTED));
        t.draw(c, "Clear this chart to record a score.", area.x, y + 92.0 * s, &TextStyle::new(13.0 * s).color(theme::MUTED2));
        return;
    };

    // How the best score was made, and how often the chart was played.
    let mut notes = Vec::new();
    if let Some(modifier) = b.modifier {
        notes.push(modifier.as_str().to_string());
    }
    if let Some(gauge) = b.gauge {
        notes.push(format!("{} GAUGE", gauge.as_str()));
    }
    if b.play_count > 0 {
        notes.push(format!("{} {}", b.play_count, if b.play_count == 1 { "PLAY" } else { "PLAYS" }));
    }
    if !notes.is_empty() {
        let st = caption(9.0, s).color(theme::MUTED2);
        t.draw(c, &notes.join("  ·  "), area.x + header_w + 14.0 * s, y + 32.0 * s, &st);
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
    let ex_w = t.draw(c, &thousands(b.ex_score), area.x, y + 80.0 * s, &TextStyle::new(40.0 * s).bold().color(theme::TEXT));
    let max = format!("/ {}", thousands(song.notes_count as u32 * 2));
    t.draw(c, &max, area.x + ex_w + 10.0 * s, y + 80.0 * s, &TextStyle::new(13.0 * s).color(theme::MUTED2));
    let (rank, rank_col) = theme::rank(b.accuracy_rate());
    t.draw_in(c, rank, Rect::new(area.right() - 120.0 * s, y + 46.0 * s, 120.0 * s, 40.0 * s), Align::Right, &TextStyle::new(36.0 * s).bold().color(rank_col));

    let rate = (b.accuracy_rate() / 100.0).clamp(0.0, 1.0) as f32;
    widgets::rate_bar(c, t, sk, Rect::new(area.x, y + 96.0 * s, area.w, 4.0 * s), rate, rank_col, s);

    // Accuracy, combo, miss count
    let stats = [
        ("ACCURACY", format!("{:.2}%", b.accuracy_rate())),
        ("MAX COMBO", format!("{} / {}", thousands(b.max_combo), thousands(song.notes_count as u32))),
        ("MIN BP", thousands(b.min_bp)),
    ];
    let col_w = area.w / 3.0;
    for (i, (k, v)) in stats.iter().enumerate() {
        let sx = area.x + i as f32 * col_w;
        t.draw(c, k, sx, y + 134.0 * s, &caption(10.0, s));
        t.draw(c, v, sx, y + 156.0 * s, &TextStyle::new(16.0 * s).bold().color(theme::TEXT));
    }
}

// ---------------------------------------------------------------------------
// Footer
// ---------------------------------------------------------------------------

const HINTS: [(&str, &str); 9] = [
    ("↑↓", "SELECT"),
    ("ENTER", "PLAY"),
    ("/", "SEARCH"),
    ("F1 F3", "FOLDER"),
    ("F2", "SORT"),
    ("TAB", "OPTIONS"),
    ("A", "AUTO"),
    ("F12", "KEYS"),
    ("ESC", "QUIT"),
];

fn footer(c: &mut Canvas, t: &mut TextEngine, sk: &Skin, f: &SelectFrame, s: f32) {
    let vp = f.viewport;
    let bar = widgets::footer_bar(c, vp, s);
    let base = bar.y + 25.0 * s;

    // Song count (filtered / library)
    let mut x = vp.x + PAD * s;
    x += t.draw(c, &thousands(f.visible.len() as u32), x, base, &TextStyle::new(13.0 * s).bold().color(theme::TEXT));
    let total = format!(" / {} SONGS", thousands(f.songs.len() as u32));
    t.draw(c, &total, x, base, &caption(10.0, s));

    // Key hints, right-aligned; drop from the left if they do not fit.
    let right = vp.x + vp.width - PAD * s;
    let left_limit = x + 200.0 * s;
    let mut first = 0;
    while first < HINTS.len() && right - hint_row(c, t, sk, &HINTS[first..], 0.0, 0.0, s, false) < left_limit {
        first += 1;
    }
    widgets::footer_hints(c, t, sk, &HINTS[first..], bar, s);
}

// ---------------------------------------------------------------------------
// Modals
// ---------------------------------------------------------------------------

/// Section headers of the play options modal: (first row index, label).
/// Row order is defined by the app's option handler.
pub const OPTION_SECTIONS: [(usize, &str); 4] = [(0, "PLAY"), (4, "AUDIO"), (5, "DISPLAY / SYSTEM"), (9, "INPUT & SESSION")];

fn modal_panel(c: &mut Canvas, sk: &Skin, vp: &Viewport, w: f32, h: f32, s: f32) -> Rect {
    c.fill_rect(Rect::new(vp.x, vp.y, vp.width, vp.height), theme::BLACK.with_alpha(180));
    let panel = Rect::new(vp.x + (vp.width - w) / 2.0, vp.y + (vp.height - h) / 2.0, w, h);
    c.halo(&sk.shadow, panel, theme::WHITE);
    c.nine(&sk.panel_lg, panel, theme::SURF1);
    c.fill_rect_hgradient(Rect::new(panel.x + 16.0 * s, panel.y, panel.w - 32.0 * s, 2.0 * s), theme::CYAN, theme::MAGENTA);
    panel
}

/// Play options modal over the song list. `rows` are (label, value).
pub fn draw_options_modal(ui: &mut Ui, vp: &Viewport, rows: &[(&str, String)], selected: usize) {
    let sk = ui.skin;
    let (c, t) = (&mut ui.canvas, &mut ui.text);
    let s = vp.scale;
    let row_h = 30.0 * s;
    let section_h = 30.0 * s;
    let sections = OPTION_SECTIONS.iter().filter(|(i, _)| *i < rows.len()).count() as f32;
    let h = (84.0 * s + rows.len() as f32 * row_h + sections * section_h + 56.0 * s).min(vp.height - 32.0 * s);
    let panel = modal_panel(c, &sk, vp, 560.0 * s, h, s);
    let inner = panel.inset(28.0 * s);

    t.draw(c, "PLAY OPTIONS", inner.x, inner.y + 22.0 * s, &TextStyle::new(22.0 * s).bold().tracking(3.0 * s).color(theme::TEXT));
    let mut y = inner.y + 40.0 * s;
    for (i, (label, value)) in rows.iter().enumerate() {
        if let Some((_, section)) = OPTION_SECTIONS.iter().find(|(at, _)| *at == i) {
            let cap = caption(10.0, s).color(theme::CYAN.with_alpha(200));
            let w = t.draw(c, section, inner.x, y + 20.0 * s, &cap);
            c.fill_rect(Rect::new(inner.x + w + 12.0 * s, y + 16.0 * s, inner.w - w - 12.0 * s, s.max(1.0)), theme::LINE);
            y += section_h;
        }
        let row = Rect::new(inner.x - 8.0 * s, y, inner.w + 16.0 * s, row_h - 2.0 * s);
        let on = i == selected;
        if on {
            c.nine(&sk.panel, row, theme::CYAN.with_alpha(30));
            c.nine(&sk.panel_outline, row, theme::CYAN.with_alpha(200));
        }
        let label_st = TextStyle::new(13.0 * s).bold().tracking(1.0 * s).color(if on { theme::TEXT } else { theme::MUTED });
        t.draw_in(c, label, Rect::new(row.x + 14.0 * s, row.y, 220.0 * s, row.h), Align::Left, &label_st);
        let value_st = TextStyle::new(13.0 * s).bold().color(if on { theme::TEXT } else { theme::MUTED });
        let icon = 16.0 * s;
        let vr = Rect::new(row.right() - 300.0 * s, row.y, 300.0 * s - 12.0 * s - icon, row.h);
        t.draw_in(c, value, Rect::new(vr.x + icon, vr.y, vr.w - icon - 4.0 * s, vr.h), Align::Center, &value_st);
        if on {
            let iy = row.y + (row.h - icon) / 2.0;
            c.sprite(sk.icons.chevron_left, Rect::new(vr.x, iy, icon, icon), theme::CYAN);
            c.sprite(sk.icons.chevron_right, Rect::new(vr.right(), iy, icon, icon), theme::CYAN);
        }
        y += row_h;
    }
    let hints = [("↑↓", "SELECT"), (LEFT_RIGHT, "CHANGE"), ("TAB", "CLOSE")];
    let w = hint_row(c, t, &sk, &hints, 0.0, 0.0, s, false);
    hint_row(c, t, &sk, &hints, panel.x + (panel.w - w) / 2.0, panel.bottom() - 44.0 * s, s, true);
}

/// "Quit Beetle?" confirmation over the song list.
pub fn draw_exit_modal(ui: &mut Ui, vp: &Viewport) {
    let sk = ui.skin;
    let (c, t) = (&mut ui.canvas, &mut ui.text);
    let s = vp.scale;
    let panel = modal_panel(c, &sk, vp, 420.0 * s, 212.0 * s, s);
    let inner = panel.inset(28.0 * s);
    t.draw(c, "QUIT BEETLE?", inner.x, inner.y + 26.0 * s, &TextStyle::new(24.0 * s).bold().tracking(2.0 * s).color(theme::TEXT));
    t.draw(c, "Scores and settings are already saved.", inner.x, inner.y + 56.0 * s, &TextStyle::new(13.0 * s).color(theme::MUTED));

    let bw = (inner.w - 12.0 * s) / 2.0;
    let bh = 48.0 * s;
    let by = inner.bottom() - bh;
    let cancel = Rect::new(inner.x, by, bw, bh);
    let quit = Rect::new(cancel.right() + 12.0 * s, by, bw, bh);
    c.nine(&sk.cut_panel, cancel, theme::SURF2);
    c.nine(&sk.cut_outline, cancel, theme::LINE);
    c.nine(&sk.cut_panel, quit, theme::MAGENTA);
    for (r, label, key, col) in [(cancel, "CANCEL", "ESC", theme::TEXT), (quit, "QUIT", "ENTER", theme::WHITE)] {
        let st = TextStyle::new(14.0 * s).bold().tracking(2.0 * s).color(col);
        let lw = t.measure(c, label, &st);
        let kw = keycap_width(c, t, key, s);
        let x = r.x + (r.w - lw - kw - 10.0 * s) / 2.0;
        t.draw_in(c, label, Rect::new(x, r.y, lw + 1.0, r.h), Align::Left, &st);
        keycap(c, t, &sk, key, Rect::new(x + lw + 10.0 * s, r.y + (r.h - 20.0 * s) / 2.0, kw, 20.0 * s), s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beetle_core::{ClearType, GaugeType, LaneModifier, PlayMode, PlayResult};

    fn song(i: usize) -> SongMetadata {
        SongMetadata {
            id: Default::default(),
            md5: [0; 16],
            hash: i as u64 + 1,
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
            play_mode: if i % 3 == 0 { PlayMode::Keys14 } else { PlayMode::Keys7 },
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
    fn screen_and_modals_are_one_batch() {
        let vp = Viewport::new(1280, 720);
        let songs: Vec<_> = (0..40).map(song).collect();
        let visible: Vec<_> = (0..40).collect();
        let mut scores = ScoreStore::default();
        scores.update(PlayResult {
            chart_hash: 6,
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
        let chips = vec!["HI-SPEED 1100".to_string(), "REGULAR".into(), "GROOVE".into()];
        let mut ui = Ui::new(vp.scale);
        for (selected, search) in [(5, ""), (30, "zzz"), (0, "")] {
            let visible = if search.is_empty() { &visible[..] } else { &[][..] };
            ui.begin(1280, 720, vp.scale);
            let frame = SelectFrame {
                viewport: &vp,
                songs: &songs,
                visible,
                selected,
                scores: &scores,
                folder: "ALL SONGS",
                sort: "TITLE",
                search,
                search_active: !search.is_empty(),
                jacket: None,
                ambient: None,
                option_chips: &chips,
                auto_play: selected == 0,
                has_replay: selected == 5,
                preview_secs: None,
            };
            draw_song_select(&mut ui, &frame);
            draw_options_modal(&mut ui, &vp, &[("HI-SPEED", "1100".into()), ("GAUGE", "GROOVE".into())], 1);
            draw_exit_modal(&mut ui, &vp);
            assert_eq!(ui.canvas.debug_batches().len(), 1, "selected={selected} search={search:?}");
        }
    }
}

//! Settings screen on the `Ui` (Canvas + TextEngine + generated Skin): the
//! values that are set once, one page per group. The page tabs run down the
//! left; the selected row's page fills the right, with the highlighted row's
//! help in a card at the bottom. Same frame as the Key Configuration screen.

use std::ops::Range;

use super::widgets::{self, Hint, OptionLine, FOOTER_H, HELP_CARD_H, PAD, TOPBAR_H};
use crate::art::Skin;
use crate::canvas::{Canvas, Rect};
use crate::hit::{HitId, HitSink};
use crate::strings;
use crate::text::{Align, TextEngine, TextStyle};
use crate::theme;
use crate::ui::Ui;
use crate::view::Viewport;

/// Row height (1280×720 units).
const ROW_H: f32 = 52.0;
const LABEL_W: f32 = 200.0;
/// The page tabs: rail width, tab height and the gap between tabs.
const RAIL_W: f32 = 200.0;
const TAB_H: f32 = 44.0;
const TAB_GAP: f32 = 6.0;
/// Between the rail and the page.
const RAIL_GAP: f32 = 32.0;
/// The page's rows stop growing at this width.
const PAGE_MAX_W: f32 = 760.0;

/// Everything the Settings screen shows for one frame. `lines` are the whole
/// table in order; a line with a `section` starts a page, named after it.
pub struct SettingsFrame<'a> {
    pub viewport: &'a Viewport,
    pub lines: &'a [OptionLine<'a>],
    pub selected: usize,
    /// Help sentence of the selected line.
    pub help: &'a str,
    /// The page's scroll in device px, from `follow_settings_scroll`.
    pub scroll: f32,
}

fn hints() -> [Hint; 5] {
    [
        ("TAB", strings::HINT_PAGE(), None),
        ("↑↓", strings::FOOTER_MOVE(), None),
        (widgets::LEFT_RIGHT, strings::HINT_CHANGE(), None),
        ("ENTER", strings::HINT_SELECT(), None),
        ("ESC", strings::BACK(), Some(HitId::SettingsBack)),
    ]
}

/// The pages: each starts at a line with a section (and at the first line).
fn pages(lines: &[OptionLine]) -> Vec<Range<usize>> {
    let mut pages: Vec<Range<usize>> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        match pages.last_mut() {
            Some(page) if line.section.is_none() => page.end = i + 1,
            _ => pages.push(i..i + 1),
        }
    }
    pages
}

/// The page that holds line `selected` (the first one when out of range).
fn page_of(pages: &[Range<usize>], selected: usize) -> usize {
    pages
        .iter()
        .position(|page| page.contains(&selected))
        .unwrap_or(0)
}

/// Where one row of the page sits, from the page's top before it scrolls.
#[derive(Debug, Clone, Copy, PartialEq)]
struct RowSlot {
    /// Index into the screen's lines.
    index: usize,
    /// Top of the row.
    top: f32,
}

/// The rows of `page`, stacked from the top in table order.
fn page_slots(page: Range<usize>, s: f32) -> Vec<RowSlot> {
    page.enumerate()
        .map(|(n, index)| RowSlot {
            index,
            top: n as f32 * ROW_H * s,
        })
        .collect()
}

/// The area the rail and the page share, above the help card.
fn body_area(vp: &Viewport) -> Rect {
    let s = vp.scale;
    let content = content_area(vp);
    let card_y = content.bottom() - HELP_CARD_H * s;
    Rect::from_ltrb(content.x, content.y, content.right(), card_y - 16.0 * s)
}

/// Between the top bar and the footer.
fn content_area(vp: &Viewport) -> Rect {
    let s = vp.scale;
    Rect::from_ltrb(
        vp.x + PAD * s,
        vp.y + (TOPBAR_H + 24.0) * s,
        vp.x + vp.width - PAD * s,
        vp.y + vp.height - (FOOTER_H + 24.0) * s,
    )
}

/// The page's rows, right of the rail.
fn page_area(vp: &Viewport) -> Rect {
    let s = vp.scale;
    let body = body_area(vp);
    let x = body.x + (RAIL_W + RAIL_GAP) * s;
    let w = (body.right() - x).min(PAGE_MAX_W * s);
    Rect::new(x, body.y, w, body.h)
}

/// The scroll of the selected row's page for the next frame. The page keeps
/// its scroll while the selected row stays in its window (moving inside the
/// window does not scroll), and scrolls just far enough when the selection
/// leaves it. Clamped to the page, so a page that fits never scrolls.
pub fn follow_settings_scroll(
    vp: &Viewport,
    lines: &[OptionLine],
    selected: usize,
    previous: f32,
) -> f32 {
    let s = vp.scale;
    let pages = pages(lines);
    let Some(page) = pages.get(page_of(&pages, selected)) else {
        return 0.0;
    };
    follow_page(
        &page_slots(page.clone(), s),
        selected,
        page_area(vp).h,
        s,
        previous,
    )
}

/// The page's scroll after following the selected row (see
/// `follow_settings_scroll`).
fn follow_page(slots: &[RowSlot], selected: usize, height: f32, s: f32, current: f32) -> f32 {
    let row_h = (ROW_H - 4.0) * s;
    let content_bottom = slots.last().map_or(0.0, |slot| slot.top + row_h);
    let max = (content_bottom - height).max(0.0);
    let mut scroll = current.clamp(0.0, max);
    if let Some(slot) = slots.iter().find(|slot| slot.index == selected) {
        if slot.top < scroll {
            scroll = slot.top;
        } else if slot.top + row_h > scroll + height {
            scroll = slot.top + row_h - height;
        }
        scroll = scroll.clamp(0.0, max);
    }
    scroll
}

/// The page tabs down the left of `body`, the current one marked. Tabs
/// shrink to fit when the rail is short (a large UI scale).
#[allow(clippy::too_many_arguments)]
fn page_tabs(
    c: &mut Canvas,
    t: &mut TextEngine,
    sk: &Skin,
    hs: &mut HitSink,
    names: &[&str],
    current: usize,
    body: Rect,
    s: f32,
) {
    let gap = TAB_GAP * s;
    let n = names.len().max(1) as f32;
    let tab_h = (TAB_H * s).min((body.h - gap * (n - 1.0)) / n);
    for (i, name) in names.iter().enumerate() {
        let r = Rect::new(body.x, body.y + i as f32 * (tab_h + gap), RAIL_W * s, tab_h);
        hs.add(r, HitId::SettingsPage(i));
        let on = i == current;
        if on {
            c.nine(&sk.panel, r, theme::SURF3);
            c.fill_rect(Rect::new(r.x, r.y, 3.0 * s, r.h), theme::CYAN);
        } else if hs.hovered(r) {
            c.nine(&sk.panel, r, theme::SURF2);
        }
        let fg = if on { theme::TEXT } else { theme::MUTED };
        t.draw_in(
            c,
            name,
            Rect::new(r.x + 18.0 * s, r.y, r.w - 24.0 * s, r.h),
            Align::Left,
            &TextStyle::new(14.0 * s).bold().color(fg),
        );
    }
    // A hairline between the rail and the page.
    c.fill_rect(
        Rect::new(
            body.x + (RAIL_W + RAIL_GAP / 2.0) * s,
            body.y,
            s.max(1.0),
            body.h,
        ),
        theme::LINE,
    );
}

pub fn draw_settings(ui: &mut Ui, f: &SettingsFrame) {
    let sk = ui.skin;
    let lite = ui.lite;
    let mut hs = HitSink::new(&mut ui.hits, ui.pointer);
    let (c, t) = (&mut ui.canvas, &mut ui.text);
    let vp = f.viewport;
    let s = vp.scale;

    widgets::backdrop(c, &sk, vp, theme::BLUE, ui.clock, lite);
    widgets::top_bar(c, t, vp, strings::SETTINGS(), s);

    let pages = pages(f.lines);
    let current = page_of(&pages, f.selected);
    let names: Vec<&str> = pages
        .iter()
        .map(|page| f.lines[page.start].section.unwrap_or(""))
        .collect();
    page_tabs(c, t, &sk, &mut hs, &names, current, body_area(vp), s);

    let area = page_area(vp);
    if let Some(page) = pages.get(current) {
        for slot in page_slots(page.clone(), s) {
            let Some(line) = f.lines.get(slot.index) else {
                continue;
            };
            // A row that is not wholly in the page is not drawn (nor hit).
            let top = slot.top - f.scroll;
            if top < 0.0 || top + ROW_H * s - 4.0 * s > area.h {
                continue;
            }
            let row = Rect::new(area.x, area.y + top, area.w, (ROW_H - 4.0) * s);
            widgets::option_row(
                c,
                t,
                &sk,
                &mut hs,
                row,
                slot.index,
                line,
                slot.index == f.selected,
                LABEL_W * s,
                s,
            );
        }
    }

    let content = content_area(vp);
    let card = Rect::new(
        content.x,
        content.bottom() - HELP_CARD_H * s,
        content.w,
        HELP_CARD_H * s,
    );
    let label = f.lines.get(f.selected).map_or("", |l| l.label);
    widgets::help_card(c, t, &sk, card, label, f.help, s);

    let bar = widgets::footer_bar(c, vp, s);
    widgets::footer_buttons(c, t, &sk, &hints(), bar, s, &mut hs);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hit::hit_at;

    /// Two pages of two rows.
    fn lines() -> Vec<OptionLine<'static>> {
        vec![
            OptionLine {
                section: Some("화면"),
                label: "화면 모드",
                value: "창 모드".into(),
                stepper: true,
            },
            OptionLine {
                section: None,
                label: "해상도",
                value: "1280 x 720".into(),
                stepper: true,
            },
            OptionLine {
                section: Some("플레이 화면"),
                label: "플레이필드",
                value: "가운데".into(),
                stepper: true,
            },
            OptionLine {
                section: None,
                label: "노트 크기",
                value: "100%".into(),
                stepper: true,
            },
        ]
    }

    fn draw(ui: &mut Ui, vp: &Viewport, lines: &[OptionLine], selected: usize, scroll: f32) {
        ui.begin(1280, 720, vp.scale);
        draw_settings(
            ui,
            &SettingsFrame {
                viewport: vp,
                lines,
                selected,
                help: "help",
                scroll,
            },
        );
    }

    fn center(ui: &Ui, id: HitId) -> (f32, f32) {
        let r = ui.hits.iter().find(|h| h.id == id).expect("recorded").rect;
        (r.x + r.w / 2.0, r.y + r.h / 2.0)
    }

    #[test]
    fn lines_with_a_section_start_pages() {
        assert_eq!(pages(&lines()), vec![0..2, 2..4]);
        assert_eq!(page_of(&pages(&lines()), 3), 1);
        assert_eq!(page_of(&pages(&lines()), 99), 0);
    }

    #[test]
    fn only_the_selected_rows_page_is_drawn_and_every_tab_hits() {
        let vp = Viewport::new(1280, 720);
        let mut ui = Ui::new(vp.scale);
        let lines = lines();
        draw(&mut ui, &vp, &lines, 2, 0.0);
        for i in 2..4 {
            let (x, y) = center(&ui, HitId::OptionRow(i));
            assert_eq!(hit_at(&ui.hits, x, y), Some(HitId::OptionRow(i)));
        }
        for i in 0..2 {
            assert!(!ui.hits.iter().any(|h| h.id == HitId::OptionRow(i)));
        }
        for i in 0..2 {
            let (x, y) = center(&ui, HitId::SettingsPage(i));
            assert_eq!(hit_at(&ui.hits, x, y), Some(HitId::SettingsPage(i)));
        }
        let (x, y) = center(&ui, HitId::SettingsBack);
        assert!(y >= vp.y + vp.height - FOOTER_H * vp.scale);
        assert_eq!(hit_at(&ui.hits, x, y), Some(HitId::SettingsBack));
    }

    #[test]
    fn at_150_percent_the_tabs_fit_above_the_help_card() {
        let vp = Viewport::new(1280, 720).with_ui_scale(150);
        let mut ui = Ui::new(vp.scale);
        // Six pages, as the app has.
        let lines: Vec<OptionLine> = (0..12)
            .map(|i| OptionLine {
                section: (i % 2 == 0).then_some("분류"),
                label: "항목",
                value: "값".into(),
                stepper: true,
            })
            .collect();
        draw(&mut ui, &vp, &lines, 11, 0.0);
        assert_eq!(ui.canvas.debug_batches().len(), 1);
        let body = body_area(&vp);
        for i in 0..6 {
            let r = ui
                .hits
                .iter()
                .find(|h| h.id == HitId::SettingsPage(i))
                .expect("tab")
                .rect;
            assert!(r.bottom() <= body.bottom() + 0.5, "tab {i}");
        }
        let (x, y) = center(&ui, HitId::SettingsBack);
        assert!(y <= vp.y + vp.height, "back stays on screen");
        assert_eq!(hit_at(&ui.hits, x, y), Some(HitId::SettingsBack));
    }

    #[test]
    fn screen_is_one_batch() {
        let vp = Viewport::new(1280, 720);
        let mut ui = Ui::new(vp.scale);
        let lines = lines();
        for selected in 0..lines.len() {
            draw(&mut ui, &vp, &lines, selected, 0.0);
            assert_eq!(ui.canvas.debug_batches().len(), 1, "selected {selected}");
        }
    }

    /// One page of `n` rows.
    fn long_page(n: usize) -> Vec<OptionLine<'static>> {
        (0..n)
            .map(|i| OptionLine {
                section: (i == 0).then_some("분류"),
                label: "항목",
                value: "값".into(),
                stepper: true,
            })
            .collect()
    }

    #[test]
    fn a_page_that_fits_never_scrolls() {
        let s = 1.0;
        let short = page_slots(0..4, s);
        assert_eq!(follow_page(&short, 3, 464.0, s, 0.0), 0.0);
        // Even a stale offset (from another page) is pulled back to 0.
        assert_eq!(follow_page(&short, 3, 464.0, s, 80.0), 0.0);
    }

    #[test]
    fn selecting_the_bottom_row_scrolls_it_to_the_page_bottom() {
        let s = 1.0;
        let long = page_slots(0..14, s);
        let bottom = long[13].top + (ROW_H - 4.0) * s;
        let scroll = follow_page(&long, 13, 464.0, s, 0.0);
        assert!(scroll > 0.0);
        assert!(
            (bottom - scroll - 464.0).abs() < 1e-3,
            "bottom row ends at the page bottom"
        );
        // Every row can be brought into view from the top.
        for i in 0..14 {
            let scroll = follow_page(&long, i, 464.0, s, 0.0);
            assert!(long[i].top - scroll >= 0.0, "row {i} above the window");
            assert!(
                long[i].top + (ROW_H - 4.0) * s - scroll <= 464.0 + 1e-3,
                "row {i} below"
            );
        }
    }

    #[test]
    fn moving_inside_the_window_does_not_scroll() {
        let s = 1.0;
        let long = page_slots(0..14, s);
        let row_h = (ROW_H - 4.0) * s;
        let scroll = follow_page(&long, 13, 464.0, s, 0.0);
        let visible: Vec<usize> = (0..14)
            .filter(|&i| long[i].top >= scroll && long[i].top + row_h <= scroll + 464.0)
            .collect();
        assert!(visible.len() > 2, "the window shows several rows");
        for pair in visible.windows(2) {
            assert_eq!(follow_page(&long, pair[0], 464.0, s, scroll), scroll);
            assert_eq!(follow_page(&long, pair[1], 464.0, s, scroll), scroll);
        }
        let mut current = scroll;
        for &i in visible.iter().rev() {
            current = follow_page(&long, i, 464.0, s, current);
        }
        assert_eq!(current, scroll);
    }

    #[test]
    fn leaving_the_window_scrolls_minimally() {
        let s = 1.0;
        let long = page_slots(0..14, s);
        let row_h = (ROW_H - 4.0) * s;
        let scroll = follow_page(&long, 13, 464.0, s, 0.0);
        let first_visible = (0..14).find(|&i| long[i].top >= scroll).unwrap();
        // Up past the top edge: the row's top becomes the top edge.
        let up = follow_page(&long, first_visible - 1, 464.0, s, scroll);
        assert_eq!(up, long[first_visible - 1].top);
        // Down past the bottom edge (from the top): the row's bottom becomes
        // the bottom edge.
        let last_visible = (0..14)
            .rev()
            .find(|&i| long[i].top + row_h <= 464.0)
            .unwrap();
        assert!(last_visible < 13);
        let down = follow_page(&long, last_visible + 1, 464.0, s, 0.0);
        assert!((down - (long[last_visible + 1].top + row_h - 464.0)).abs() < 1e-3);
    }

    #[test]
    fn a_long_page_draws_and_hits_only_rows_inside_the_page() {
        let vp = Viewport::new(1280, 720);
        let lines = long_page(14);
        let mut ui = Ui::new(vp.scale);
        let area = page_area(&vp);
        // Each frame follows the selection from the last one, as the app does.
        let mut scroll = 0.0;
        for selected in 0..lines.len() {
            scroll = follow_settings_scroll(&vp, &lines, selected, scroll);
            draw(&mut ui, &vp, &lines, selected, scroll);
            assert!(
                ui.hits.iter().any(|h| h.id == HitId::OptionRow(selected)),
                "selected {selected} is not drawn"
            );
            for h in ui
                .hits
                .iter()
                .filter(|h| matches!(h.id, HitId::OptionRow(_)))
            {
                assert!(
                    h.rect.y >= area.y - 0.5 && h.rect.bottom() <= area.bottom() + 0.5,
                    "{:?}",
                    h.id
                );
            }
        }
    }
}

//! Settings screen on the `Ui` (Canvas + TextEngine + generated Skin): the
//! values that are set once (screen, sound, judge, layout, input) in two
//! columns, with the highlighted row's help in a card at the bottom. Same
//! frame as the Key Configuration screen.

use super::widgets::{self, Hint, OptionLine, FOOTER_H, HELP_CARD_H, PAD, TOPBAR_H};
use crate::canvas::Rect;
use crate::hit::{HitId, HitSink};
use crate::strings;
use crate::theme;
use crate::ui::Ui;
use crate::view::Viewport;

/// Row height and section header height (1280×720 units).
const ROW_H: f32 = 52.0;
const SECTION_H: f32 = 36.0;
const LABEL_W: f32 = 170.0;
const COLUMN_GAP: f32 = 32.0;

/// Everything the Settings screen shows for one frame. `lines` are in table
/// order; a line's `column` picks the left or right column.
pub struct SettingsFrame<'a> {
    pub viewport: &'a Viewport,
    pub lines: &'a [OptionLine<'a>],
    pub selected: usize,
    /// Help sentence of the selected line.
    pub help: &'a str,
}

const HINTS: [Hint; 4] = [
    ("↑↓", strings::FOOTER_MOVE, None),
    (widgets::LEFT_RIGHT, strings::HINT_CHANGE, None),
    ("ENTER", strings::HINT_SELECT, None),
    ("ESC", strings::BACK, Some(HitId::SettingsBack)),
];

/// Where one row of a Settings column sits, from the column's top before
/// it scrolls.
#[derive(Debug, Clone, Copy, PartialEq)]
struct RowSlot {
    /// Index into the screen's lines.
    index: usize,
    /// Top of the section header above the row, if the row starts a section.
    header: Option<f32>,
    /// Top of the row.
    top: f32,
}

/// The rows of `column`, stacked from the top in table order.
fn column_slots(lines: &[OptionLine], column: usize, s: f32) -> Vec<RowSlot> {
    let mut y = 0.0;
    let mut slots = Vec::new();
    for (index, line) in lines.iter().enumerate().filter(|(_, l)| l.column == column) {
        let header = line.section.map(|_| {
            let top = y;
            y += SECTION_H * s;
            top
        });
        slots.push(RowSlot {
            index,
            header,
            top: y,
        });
        y += ROW_H * s;
    }
    slots
}

/// How far a column scrolls so that the selected row (when it is in this
/// column) is its bottom row in view. Stateless: it follows the cursor, and
/// it is 0 while the whole column fits.
fn scroll_for(slots: &[RowSlot], selected: usize, height: f32, s: f32) -> f32 {
    slots
        .iter()
        .find(|slot| slot.index == selected)
        .map_or(0.0, |slot| (slot.top + (ROW_H - 4.0) * s - height).max(0.0))
}

pub fn draw_settings(ui: &mut Ui, f: &SettingsFrame) {
    let sk = ui.skin;
    let lite = ui.lite;
    let mut hs = HitSink::new(&mut ui.hits, ui.pointer);
    let (c, t) = (&mut ui.canvas, &mut ui.text);
    let vp = f.viewport;
    let s = vp.scale;

    widgets::backdrop(c, &sk, vp, theme::BLUE, ui.clock, lite);
    widgets::top_bar(c, t, vp, strings::SETTINGS, s);

    let content = Rect::from_ltrb(
        vp.x + PAD * s,
        vp.y + (TOPBAR_H + 24.0) * s,
        vp.x + vp.width - PAD * s,
        vp.y + vp.height - (FOOTER_H + 24.0) * s,
    );
    let card = Rect::new(
        content.x,
        content.bottom() - HELP_CARD_H * s,
        content.w,
        HELP_CARD_H * s,
    );
    let area = Rect::from_ltrb(content.x, content.y, content.right(), card.y - 16.0 * s);
    let col_w = (area.w - COLUMN_GAP * s) / 2.0;
    for column in 0..2 {
        let col = Rect::new(
            area.x + column as f32 * (col_w + COLUMN_GAP * s),
            area.y,
            col_w,
            area.h,
        );
        let slots = column_slots(f.lines, column, s);
        let scroll = scroll_for(&slots, f.selected, col.h, s);
        for slot in &slots {
            let Some(line) = f.lines.get(slot.index) else {
                continue;
            };
            // A row that is not wholly in the column is not drawn (nor hit).
            let top = slot.top - scroll;
            if top < 0.0 || top + ROW_H * s - 4.0 * s > col.h {
                continue;
            }
            if let (Some(section), Some(header)) = (line.section, slot.header) {
                let hy = header - scroll;
                if hy >= 0.0 {
                    widgets::section_header(c, t, section, col.x, col.y + hy, col.w, s);
                }
            }
            let row = Rect::new(col.x, col.y + top, col.w, (ROW_H - 4.0) * s);
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

    let label = f.lines.get(f.selected).map_or("", |l| l.label);
    widgets::help_card(c, t, &sk, card, label, f.help, s);

    let bar = widgets::footer_bar(c, vp, s);
    widgets::footer_buttons(c, t, &sk, &HINTS, bar, s, &mut hs);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hit::hit_at;

    fn lines() -> Vec<OptionLine<'static>> {
        vec![
            OptionLine {
                column: 0,
                section: Some("화면"),
                label: "화면 모드",
                value: "창 모드".into(),
            },
            OptionLine {
                column: 0,
                section: None,
                label: "해상도",
                value: "1280 x 720".into(),
            },
            OptionLine {
                column: 1,
                section: Some("레이아웃"),
                label: "플레이필드",
                value: "가운데".into(),
            },
            OptionLine {
                column: 1,
                section: None,
                label: "키 배치",
                value: "7K  HomeRow".into(),
            },
        ]
    }

    #[test]
    fn every_row_and_footer_action_has_a_hit_region() {
        let vp = Viewport::new(1280, 720);
        let mut ui = Ui::new(vp.scale);
        let lines = lines();
        ui.begin(1280, 720, vp.scale);
        draw_settings(
            &mut ui,
            &SettingsFrame {
                viewport: &vp,
                lines: &lines,
                selected: 2,
                help: "help",
            },
        );
        for i in 0..lines.len() {
            let row = ui
                .hits
                .iter()
                .find(|h| h.id == HitId::OptionRow(i))
                .expect("row")
                .rect;
            let (x, y) = (row.x + row.w / 2.0, row.y + row.h / 2.0);
            assert_eq!(hit_at(&ui.hits, x, y), Some(HitId::OptionRow(i)));
        }
        let back = ui
            .hits
            .iter()
            .find(|h| h.id == HitId::SettingsBack)
            .expect("back")
            .rect;
        assert!(back.y >= vp.y + vp.height - FOOTER_H * vp.scale);
        let (x, y) = (back.x + back.w / 2.0, back.y + back.h / 2.0);
        assert_eq!(hit_at(&ui.hits, x, y), Some(HitId::SettingsBack));
    }

    #[test]
    fn screen_is_one_batch() {
        let vp = Viewport::new(1280, 720);
        let mut ui = Ui::new(vp.scale);
        let lines = lines();
        for selected in 0..lines.len() {
            ui.begin(1280, 720, vp.scale);
            draw_settings(
                &mut ui,
                &SettingsFrame {
                    viewport: &vp,
                    lines: &lines,
                    selected,
                    help: "help",
                },
            );
            assert_eq!(ui.canvas.debug_batches().len(), 1, "selected {selected}");
        }
    }

    fn long_column(n: usize) -> Vec<OptionLine<'static>> {
        (0..n)
            .map(|i| OptionLine {
                column: 0,
                section: (i % 5 == 0).then_some("분류"),
                label: "항목",
                value: "값".into(),
            })
            .collect()
    }

    #[test]
    fn scroll_is_zero_while_the_column_fits_and_the_selection_is_the_bottom_row_otherwise() {
        let s = 1.0;
        let short = column_slots(&long_column(4), 0, s);
        assert_eq!(scroll_for(&short, 3, 464.0, s), 0.0);

        let long = column_slots(&long_column(14), 0, s);
        let bottom = long[13].top + (ROW_H - 4.0) * s;
        let scroll = scroll_for(&long, 13, 464.0, s);
        assert!(scroll > 0.0);
        assert!(
            (bottom - scroll - 464.0).abs() < 1e-3,
            "bottom row ends at the column bottom"
        );
        // Moving up keeps the selected row inside the window.
        for i in 0..14 {
            let scroll = scroll_for(&long, i, 464.0, s);
            assert!(long[i].top - scroll >= 0.0, "row {i} above the window");
            assert!(
                long[i].top + (ROW_H - 4.0) * s - scroll <= 464.0 + 1e-3,
                "row {i} below"
            );
        }
    }

    #[test]
    fn a_long_column_draws_and_hits_only_rows_inside_the_column() {
        let vp = Viewport::new(1280, 720);
        let lines = long_column(14);
        let mut ui = Ui::new(vp.scale);
        let s = vp.scale;
        let top = vp.y + (widgets::TOPBAR_H + 24.0) * s;
        let bottom =
            vp.y + vp.height - (widgets::FOOTER_H + 24.0) * s - widgets::HELP_CARD_H * s - 16.0 * s;
        for selected in 0..lines.len() {
            ui.begin(1280, 720, vp.scale);
            draw_settings(
                &mut ui,
                &SettingsFrame {
                    viewport: &vp,
                    lines: &lines,
                    selected,
                    help: "help",
                },
            );
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
                    h.rect.y >= top - 0.5 && h.rect.bottom() <= bottom + 0.5,
                    "{:?}",
                    h.id
                );
            }
        }
    }
}

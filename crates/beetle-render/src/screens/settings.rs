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

pub fn draw_settings(ui: &mut Ui, f: &SettingsFrame) {
    let sk = ui.skin;
    let lite = ui.lite;
    let mut hs = HitSink::new(&mut ui.hits, ui.pointer);
    let (c, t) = (&mut ui.canvas, &mut ui.text);
    let vp = f.viewport;
    let s = vp.scale;

    widgets::backdrop(c, &sk, vp, theme::BLUE, lite);
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
        let mut y = col.y;
        for (i, line) in f
            .lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.column == column)
        {
            if let Some(section) = line.section {
                widgets::section_header(c, t, section, col.x, y, col.w, s);
                y += SECTION_H * s;
            }
            let row = Rect::new(col.x, y, col.w, (ROW_H - 4.0) * s);
            widgets::option_row(
                c,
                t,
                &sk,
                &mut hs,
                row,
                i,
                line,
                i == f.selected,
                LABEL_W * s,
                s,
            );
            y += ROW_H * s;
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
}

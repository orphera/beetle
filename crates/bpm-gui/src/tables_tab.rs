//! The `[3] Tables` tab: the entries of the installed difficulty tables that the
//! collection lacks. The data and the actions come from
//! `bms_package_manager::table_ops`; this module holds the view state and draws it.

use crate::bitmap_font::BitmapFont;
use crate::ui::GuiRenderer;
use beetle_core::{DifficultyTable, TableEntry};
use beetle_render::skin::ColorRgba;
use bms_package_manager::collection::{self, Index};
use bms_package_manager::table_ops::{self, MissingRow};
use bms_package_manager::TableStore;
use std::fs;
use std::path::Path;

/// Height of one list row in px.
const ROW_H: f32 = 18.0;
/// Height reserved under the list for the selected entry's links.
const DETAIL_H: f32 = 52.0;

pub struct TablesTab {
    pub tables: Vec<DifficultyTable>,
    pub table_idx: usize,
    pub index: Option<Index>,
    pub rows: Vec<MissingRow>,
    pub selected: usize,
    first_visible: usize,
    /// One line under the header: what the tab is showing or needs.
    pub note: String,
}

impl TablesTab {
    /// Reads the installed tables and the collection index as the CLI does.
    pub fn load() -> Self {
        let dir = std::env::var("BEETLE_TABLES_DIR").unwrap_or_else(|_| "tables".to_string());
        let tables = TableStore::new(dir)
            .list()
            .into_iter()
            .map(|(_, table)| table)
            .collect();
        let mut tab = Self {
            tables,
            table_idx: 0,
            index: None,
            rows: Vec::new(),
            selected: 0,
            first_visible: 0,
            note: String::new(),
        };
        tab.reload_index();
        tab
    }

    /// Reads the collection index again, after a scan or an import.
    pub fn reload_index(&mut self) {
        let text = fs::read_to_string(collection::index_file()).unwrap_or_default();
        self.index = Index::parse(&text);
        self.rebuild_rows();
    }

    fn rebuild_rows(&mut self) {
        self.rows.clear();
        self.selected = 0;
        self.first_visible = 0;
        if self.tables.is_empty() {
            self.note = "No tables installed. Add one with `bpm table add <address>`.".into();
            return;
        }
        let Some(index) = &self.index else {
            self.note = "No collection index yet. Press [S] to scan.".into();
            return;
        };
        let table = &self.tables[self.table_idx];
        self.rows = table_ops::missing_rows(table, index);
        let body = self.rows.iter().filter(|row| row.body_needed).count();
        self.note = format!(
            "{} of {} charts missing ({} need their body)",
            self.rows.len(),
            table.entries.len(),
            body
        );
    }

    pub fn table(&self) -> Option<&DifficultyTable> {
        self.tables.get(self.table_idx)
    }

    /// Moves to the next or previous installed table.
    pub fn switch_table(&mut self, forward: bool) {
        let count = self.tables.len();
        if count == 0 {
            return;
        }
        self.table_idx = if forward {
            (self.table_idx + 1) % count
        } else {
            (self.table_idx + count - 1) % count
        };
        self.rebuild_rows();
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let last = self.rows.len() as isize - 1;
        self.selected = (self.selected as isize + delta).clamp(0, last) as usize;
    }

    pub fn selected_row(&self) -> Option<&MissingRow> {
        self.rows.get(self.selected)
    }

    /// The table entry behind the selected row.
    pub fn selected_entry(&self) -> Option<&TableEntry> {
        let row = self.selected_row()?;
        self.table()?.entries.get(row.number.checked_sub(1)?)
    }

    /// Scrolls so the selected row is inside the list, given how many rows fit.
    fn keep_selected_visible(&mut self, visible: usize) {
        if visible == 0 {
            return;
        }
        if self.selected < self.first_visible {
            self.first_visible = self.selected;
        } else if self.selected >= self.first_visible + visible {
            self.first_visible = self.selected + 1 - visible;
        }
    }

    /// Draws the tab in the content area at (`x`, `y`) with size `w` by `h`.
    pub fn draw(&mut self, r: &mut GuiRenderer, x: f32, y: f32, w: f32, h: f32) {
        let title = match self.table() {
            Some(table) => format!("{} ({})", table.name, table.symbol),
            None => "Tables".to_string(),
        };
        BitmapFont::draw_text(
            &mut r.pixmap.as_mut(),
            &title,
            x as i32,
            y as i32,
            1,
            ColorRgba::new(255, 235, 120, 255),
        );
        BitmapFont::draw_text(
            &mut r.pixmap.as_mut(),
            &self.note,
            x as i32,
            y as i32 + 18,
            1,
            ColorRgba::new(170, 170, 190, 255),
        );

        let list_y = y + 42.0;
        let list_h = (h - 42.0 - DETAIL_H).max(0.0);
        let visible = (list_h / ROW_H) as usize;
        self.keep_selected_visible(visible);
        r.draw_rect(x, list_y, w, list_h, ColorRgba::new(18, 18, 26, 255));

        for (slot, row_index) in (self.first_visible..self.rows.len())
            .take(visible)
            .enumerate()
        {
            let row = &self.rows[row_index];
            let row_y = list_y + slot as f32 * ROW_H;
            let selected = row_index == self.selected;
            if selected {
                r.draw_rect(x, row_y, w, ROW_H, ColorRgba::new(35, 45, 70, 255));
            }
            let fg = if selected {
                ColorRgba::new(255, 255, 255, 255)
            } else {
                ColorRgba::new(200, 200, 215, 255)
            };
            let tag = if row.body_needed { "[body] " } else { "" };
            let text = format!(
                "#{:<5} {:<6} {tag}{} / {}",
                row.number, row.level, row.title, row.artist
            );
            let text = fit(&text, w - 12.0);
            BitmapFont::draw_text(
                &mut r.pixmap.as_mut(),
                &text,
                x as i32 + 6,
                row_y as i32 + 3,
                1,
                fg,
            );
        }

        let detail_y = list_y + list_h + 8.0;
        let link_col = ColorRgba::new(120, 200, 255, 255);
        if let Some(row) = self.selected_row() {
            let lines = [
                format!("body: {}", row.url),
                format!("diff: {}", row.url_diff),
            ];
            for (i, line) in lines.iter().enumerate() {
                BitmapFont::draw_text(
                    &mut r.pixmap.as_mut(),
                    &fit(line, w - 12.0),
                    x as i32 + 6,
                    detail_y as i32 + i as i32 * 18,
                    1,
                    link_col,
                );
            }
        }
    }
}

/// Cuts `text` to fit `max_w` px, ending with "..." when it is cut.
fn fit(text: &str, max_w: f32) -> String {
    if BitmapFont::text_width(text, 1) <= max_w {
        return text.to_string();
    }
    let mut out: String = text.to_string();
    while !out.is_empty() && BitmapFont::text_width(&format!("{out}..."), 1) > max_w {
        out.pop();
    }
    format!("{out}...")
}

#[cfg(test)]
mod snapshot {
    use super::*;

    /// Renders one frame of the Tables tab to the BMP file named by
    /// `BPM_GUI_SNAPSHOT`, for a visual check without opening a window.
    /// Run it with `cargo test -p bpm-gui snapshot -- --ignored`.
    #[test]
    #[ignore]
    fn snapshot_tables_tab() {
        let Ok(path) = std::env::var("BPM_GUI_SNAPSHOT") else {
            return;
        };
        let mut renderer = GuiRenderer::new(960, 680).expect("pixmap");
        let mut tables = TablesTab::load();
        renderer.render_frame(
            crate::ui::ActiveTab::Tables,
            &[],
            0,
            0,
            None,
            &[],
            0,
            0,
            "",
            false,
            "",
            None,
            None,
            &mut tables,
        );
        write_bmp(&renderer, Path::new(&path));
    }

    fn write_bmp(renderer: &GuiRenderer, path: &Path) {
        let (w, h) = (renderer.pixmap.width(), renderer.pixmap.height());
        let data = renderer.pixmap.data();
        let mut out = Vec::with_capacity(54 + data.len());
        let image_size = w * h * 4;
        out.extend_from_slice(b"BM");
        out.extend_from_slice(&(54 + image_size).to_le_bytes());
        out.extend_from_slice(&[0u8; 4]);
        out.extend_from_slice(&54u32.to_le_bytes());
        out.extend_from_slice(&40u32.to_le_bytes());
        out.extend_from_slice(&w.to_le_bytes());
        out.extend_from_slice(&(-(h as i32)).to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&[0u8; 24]);
        for px in data.chunks_exact(4) {
            out.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
        fs::write(path, out).expect("write snapshot");
    }
}

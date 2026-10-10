//! State of the "난이도표" (difficulty tables) tab: the entries of the installed
//! tables that the collection lacks. The data and the actions come from
//! `bms_package_manager::table_ops`; `ui.rs` draws it.

use crate::widgets::ListView;
use beetle_core::{DifficultyTable, TableEntry};
use bms_package_manager::collection::{self, Index};
use bms_package_manager::table_ops::{self, MissingRow};
use bms_package_manager::TableStore;
use std::fs;

/// What the tab can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TablesState {
    NoTables,
    /// Tables are installed but the collection was never scanned.
    NoIndex,
    Ready,
}

/// The folder the tables are installed in, as the CLI uses it.
pub fn store() -> TableStore {
    let dir = std::env::var("BEETLE_TABLES_DIR").unwrap_or_else(|_| "tables".to_string());
    TableStore::new(dir)
}

pub struct TablesTab {
    pub tables: Vec<DifficultyTable>,
    pub table_idx: usize,
    pub index: Option<Index>,
    pub rows: Vec<MissingRow>,
    pub view: ListView,
    /// A chart was added since the index was last scanned: the list may be out of date.
    pub stale: bool,
}

impl TablesTab {
    /// Reads the installed tables and the collection index as the CLI does.
    pub fn load() -> Self {
        let mut tab = Self {
            tables: Vec::new(),
            table_idx: 0,
            index: None,
            rows: Vec::new(),
            view: ListView::default(),
            stale: false,
        };
        tab.reload_tables();
        tab.reload_index();
        tab
    }

    /// Reads the installed tables again, keeping the shown table when it is
    /// still there.
    pub fn reload_tables(&mut self) {
        let shown = self.table().map(|t| t.name.clone());
        self.tables = store().list().into_iter().map(|(_, table)| table).collect();
        self.table_idx = shown
            .and_then(|name| self.tables.iter().position(|t| t.name == name))
            .unwrap_or(0);
        self.rebuild_rows();
    }

    /// Shows the table named `name`, if installed.
    pub fn show_table(&mut self, name: &str) {
        if let Some(i) = self.tables.iter().position(|t| t.name == name) {
            self.table_idx = i;
            self.rebuild_rows();
        }
    }

    /// Reads the collection index again, after a scan or an import.
    pub fn reload_index(&mut self) {
        let text = fs::read_to_string(collection::index_file()).unwrap_or_default();
        self.index = Index::parse(&text);
        self.rebuild_rows();
    }

    fn rebuild_rows(&mut self) {
        self.rows.clear();
        self.view.reset();
        if let (Some(table), Some(index)) = (self.tables.get(self.table_idx), &self.index) {
            self.rows = table_ops::missing_rows(table, index);
        }
    }

    pub fn state(&self) -> TablesState {
        if self.tables.is_empty() {
            TablesState::NoTables
        } else if self.index.is_none() {
            TablesState::NoIndex
        } else {
            TablesState::Ready
        }
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

    pub fn selected_row(&self) -> Option<&MissingRow> {
        self.rows.get(self.view.selected)
    }

    /// The table entry behind the selected row.
    pub fn selected_entry(&self) -> Option<&TableEntry> {
        let row = self.selected_row()?;
        self.table()?.entries.get(row.number.checked_sub(1)?)
    }
}

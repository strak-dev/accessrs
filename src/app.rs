use crate::db::table_view::TableView;
use crate::ui::create_dialog::CreateTableDialog;
use crate::ui::popover::CellPopover;
use rusqlite::Connection;

pub struct App {
    pub conn: Option<Connection>,
    pub db_path: Option<String>,
    pub status: String,
    pub tables: Vec<String>,
    pub selected_table: Option<String>,
    pub create_dialog: CreateTableDialog,
    pub cell_popover: Option<CellPopover>,
    pub table_view: Option<TableView>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            conn: None,
            db_path: None,
            status: "No database open.".to_string(),
            tables: vec![],
            selected_table: None,
            create_dialog: CreateTableDialog::default(),
            cell_popover: None,
            table_view: None,
        }
    }
}

impl App {
    pub fn open_db(&mut self, path: std::path::PathBuf) {
        match Connection::open(&path) {
            Ok(conn) => {
                let _ = conn.execute_batch("PRAGMA foreign_keys = ON;");
                self.db_path = Some(path.display().to_string());
                self.status = format!("Opened: {}", path.display());
                self.conn = Some(conn);
                self.selected_table = None;
                self.table_view = None;
                self.refresh_tables();
            }
            Err(e) => {
                self.status = format!("Error opening DB: {e}");
            }
        }
    }

    pub fn refresh_tables(&mut self) {
        self.tables.clear();
        if let Some(conn) = &self.conn {
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
                .unwrap();
            self.tables = stmt
                .query_map([], |row| row.get(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect();
        }
    }

    pub fn select_table(&mut self, name: &str) {
        self.selected_table = Some(name.to_string());
        if let Some(conn) = &self.conn {
            let view = TableView::load(conn, name);
            self.status = format!("Table: {} — {} rows", name, view.rows.len());
            self.table_view = Some(view);
        }
    }

    pub fn create_table(&mut self) {
        match self.create_dialog.to_sql() {
            Err(e) => {
                self.create_dialog.error = Some(e);
            }
            Ok(sql) => {
                if let Some(conn) = &self.conn {
                    match conn.execute(&sql, []) {
                        Ok(_) => {
                            let name = self.create_dialog.table_name.trim().to_string();
                            self.status = format!("Created table: {name}");
                            self.create_dialog.reset();
                            self.refresh_tables();
                            self.select_table(&name.clone());
                        }
                        Err(e) => {
                            self.create_dialog.error = Some(format!("SQL error: {e}"));
                        }
                    }
                }
            }
        }
    }

    pub fn commit_edit(&mut self) {
        let (row_idx, col_idx, value, table_name, columns, row_data) = {
            let view = match &self.table_view {
                Some(v) => v,
                None => return,
            };
            let (row_idx, col_idx) = match view.editing_cell {
                Some(c) => c,
                None => return,
            };
            (
                row_idx,
                col_idx,
                view.edit_buffer.clone(),
                view.table_name.clone(),
                view.columns.clone(),
                view.rows[row_idx].clone(),
            )
        };

        let where_clause: String = columns
            .iter()
            .enumerate()
            .map(|(i, col)| format!("{} = ?{}", col, i + 1))
            .collect::<Vec<_>>()
            .join(" AND ");

        let sql = format!(
            "UPDATE {} SET {} = ?{} WHERE {}",
            table_name,
            columns[col_idx],
            columns.len() + 1,
            where_clause,
        );

        let mut params: Vec<String> = row_data.clone();
        params.push(value.clone());

        let result = self
            .conn
            .as_ref()
            .unwrap()
            .execute(&sql, rusqlite::params_from_iter(params.iter()));

        match result {
            Ok(_) => {
                if let Some(view) = &mut self.table_view {
                    view.rows[row_idx][col_idx] = value;
                    view.editing_cell = None;
                    view.edit_buffer.clear();
                }
                self.status = "Row updated.".to_string();
            }
            Err(e) => {
                self.status = format!("Update failed: {e}");
                if let Some(view) = &mut self.table_view {
                    view.editing_cell = None;
                }
            }
        }
    }

    pub fn commit_insert(&mut self) {
        let (table_name, columns, new_row) = {
            let view = match &self.table_view {
                Some(v) => v,
                None => return,
            };
            (
                view.table_name.clone(),
                view.columns.clone(),
                view.new_row.clone(),
            )
        };

        let (cols, vals): (Vec<&str>, Vec<&str>) = columns
            .iter()
            .zip(new_row.iter())
            .filter(|(_, v)| !v.trim().is_empty())
            .map(|(c, v)| (c.as_str(), v.as_str()))
            .unzip();

        if cols.is_empty() {
            if let Some(view) = &mut self.table_view {
                view.new_row_error = Some("Enter at least one value.".to_string());
            }
            return;
        }

        let placeholders = (1..=cols.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");

        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            table_name,
            cols.join(", "),
            placeholders,
        );

        let result = self
            .conn
            .as_ref()
            .unwrap()
            .execute(&sql, rusqlite::params_from_iter(vals.iter()));

        match result {
            Ok(_) => {
                self.status = "Row inserted.".to_string();
                if let Some(view) = &mut self.table_view {
                    view.new_row = vec![String::new(); view.columns.len()];
                    view.new_row_error = None;
                }
                if let Some(conn) = &self.conn {
                    if let Some(view) = &mut self.table_view {
                        view.reload_rows(conn);
                    }
                }
            }
            Err(e) => {
                if let Some(view) = &mut self.table_view {
                    view.new_row_error = Some(format!("Insert failed: {e}"));
                }
            }
        }
    }
}
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
mod ui;
mod easy_mark;

use db::schema::SortDir;
use db::table_view::TableView;

use ui::create_dialog::CreateTableDialog;
use ui::empty_state;
use ui::popover::CellPopover;
use ui::sidebar;
use ui::table_grid;
use ui::toolbar;

use eframe::egui;
use rfd::FileDialog;
use rusqlite::Connection;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0])
            .with_title("AccessRS"),
        ..Default::default()
    };

    eframe::run_native(
        "AccessRS",
        options,
        Box::new(|cc| {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "fira_nerd".to_owned(),
                egui::FontData::from_static(include_bytes!("../fonts/FiraCodeNerdFont-Regular.ttf")).into(),
            );
            // Set as first priority for proportional and monospace
            fonts.families.entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "fira_nerd".to_owned());
            fonts.families.entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "fira_nerd".to_owned());
            cc.egui_ctx.set_fonts(fonts);
            Ok(Box::new(App::default()))
        }),
    )
}

// ── App ───────────────────────────────────────────────────────────────────────

struct App {
    conn: Option<Connection>,
    db_path: Option<String>,
    status: String,
    tables: Vec<String>,
    selected_table: Option<String>,
    create_dialog: CreateTableDialog,
    cell_popover: Option<CellPopover>,
    table_view: Option<TableView>,
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
    fn open_db(&mut self, path: std::path::PathBuf) {
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

    fn refresh_tables(&mut self) {
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

    fn select_table(&mut self, name: &str) {
        self.selected_table = Some(name.to_string());
        if let Some(conn) = &self.conn {
            let view = TableView::load(conn, name);
            self.status = format!("Table: {} — {} rows", name, view.rows.len());
            self.table_view = Some(view);
        }
    }

    fn create_table(&mut self) {
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

    fn commit_edit(&mut self) {
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

    fn commit_insert(&mut self) {
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

// ── UI ────────────────────────────────────────────────────────────────────────

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open database…").clicked() {
                        ui.close_menu();
                        if let Some(path) = FileDialog::new()
                            .add_filter("SQLite", &["db", "sqlite", "sqlite3"])
                            .pick_file()
                        {
                            self.open_db(path);
                        }
                    }
                    if ui.button("New database…").clicked() {
                        ui.close_menu();
                        if let Some(path) = FileDialog::new()
                            .add_filter("SQLite", &["db", "sqlite", "sqlite3"])
                            .set_file_name("new_database.db")
                            .save_file()
                        {
                            self.open_db(path);
                        }
                    }
                    if self.conn.is_some() {
                        ui.separator();
                        if ui.button("Close database").clicked() {
                            ui.close_menu();
                            self.conn = None;
                            self.db_path = None;
                            self.tables.clear();
                            self.selected_table = None;
                            self.table_view = None;
                            self.status = "Database closed.".to_string();
                        }
                    }
                });
            });
        });

        // Status bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.label(&self.status);
        });

        // Create table modal
        let tables = self.tables.clone();
        if self.create_dialog.show(ctx, &tables) {
            self.create_table();
        }

        // ── Cell popover editor ───────────────────────────────────────────────
        let (popover_commit, popover_cancel) = match &mut self.cell_popover {
            Some(popover) => popover.show(ctx),
            None => (false, false),
        };

        if popover_commit {
            if let Some(popover) = &self.cell_popover {
                if let Some(view) = &mut self.table_view {
                    view.editing_cell = Some((popover.row_idx, popover.col_idx));
                    view.edit_buffer = popover.buffer.clone();
                }
            }
            self.commit_edit();
            self.cell_popover = None;
        }
        if popover_cancel {
            self.cell_popover = None;
        }

        // Sidebar
        sidebar::show(self, ctx);

        // Main panel
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.conn.is_none() {
                empty_state::show_no_db(self, ui);
                return;
            }

            if self.selected_table.is_none() {
                empty_state::show_no_table(ui);
                return;
            }

            if let Some(err) = self.table_view.as_ref().and_then(|v| v.error.clone()) {
                ui.colored_label(egui::Color32::RED, err);
                return;
            }

            let table_name = match &self.table_view {
                Some(v) => v.table_name.clone(),
                None => return,
            };

            // Toolbar
            toolbar::show(self, ui, &table_name);

            let mut actions = table_grid::GridActions::default();
            table_grid::show(self, ui, &mut actions);

            let table_grid::GridActions {
                do_commit_edit,
                do_cancel_edit,
                new_editing_cell,
                new_edit_buffer,
                mut do_commit_insert,
                new_row_updates,
                new_popover,
                sort_click,
                navigate_to,
            } = actions;

            // ── Below-table controls ──────────────────────────────────────
            if let Some(err) = self.table_view.as_ref().and_then(|v| v.new_row_error.clone()) {
                ui.colored_label(egui::Color32::RED, err);
            }
            ui.horizontal(|ui| {
                if ui.button("+ Insert row").clicked() {
                    do_commit_insert = true;
                }
            });

            // ── Apply all deferred mutations ──────────────────────────────
            if let Some(buf) = new_edit_buffer {
                if let Some(view) = &mut self.table_view {
                    view.edit_buffer = buf;
                }
            }
            if let Some(cell) = new_editing_cell {
                if let Some(view) = &mut self.table_view {
                    view.editing_cell = cell;
                }
            }
            for (col_idx, val) in new_row_updates {
                if let Some(view) = &mut self.table_view {
                    if col_idx < view.new_row.len() {
                        view.new_row[col_idx] = val;
                    }
                }
            }
            if do_cancel_edit {
                if let Some(view) = &mut self.table_view {
                    view.editing_cell = None;
                    view.edit_buffer.clear();
                }
            }
            if do_commit_edit {
                self.commit_edit();
            }
            if do_commit_insert {
                self.commit_insert();
            }
            // Apply new popover last so it doesn't clobber an open one mid-edit
            if let Some(p) = new_popover {
                self.cell_popover = Some(p);
            }

            if let Some(col_idx) = sort_click {
                if let Some(view) = &mut self.table_view {
                    if view.sort_col == Some(col_idx) {
                        // same column — flip direction
                        view.sort_dir = if view.sort_dir == SortDir::Asc {
                            SortDir::Desc
                        } else {
                            SortDir::Asc
                        };
                    } else {
                        view.sort_col = Some(col_idx);
                        view.sort_dir = SortDir::Asc;
                    }
                    view.apply_sort();
                }
            }

            if let Some((ref_table, ref_id)) = navigate_to {
                self.select_table(&ref_table.clone());
                // find the row in the newly loaded view and highlight it
                if let Some(view) = &mut self.table_view {
                    let id_col = view.columns.iter().position(|c| c == "id").unwrap_or(0);
                    view.highlighted_row = view.rows.iter().position(|r| {
                        r.get(id_col).map(|v| v == &ref_id).unwrap_or(false)
                    });
                }
                // update sidebar selection
                self.selected_table = Some(ref_table);
            }
        });
    }
}
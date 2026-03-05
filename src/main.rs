#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use egui_extras::{Column, TableBuilder};
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
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
}

// ── Column / table-creation types ────────────────────────────────────────────

#[derive(Clone)]
struct ColumnDef {
    name: String,
    col_type: ColType,
    primary_key: bool,
    not_null: bool,
}

#[derive(Clone, PartialEq)]
enum ColType {
    Text,
    Integer,
    Real,
    Blob,
}

impl ColType {
    fn label(&self) -> &str {
        match self {
            ColType::Text => "TEXT",
            ColType::Integer => "INTEGER",
            ColType::Real => "REAL",
            ColType::Blob => "BLOB",
        }
    }
    fn all() -> &'static [ColType] {
        &[ColType::Text, ColType::Integer, ColType::Real, ColType::Blob]
    }
}

impl Default for ColumnDef {
    fn default() -> Self {
        Self {
            name: String::new(),
            col_type: ColType::Text,
            primary_key: false,
            not_null: false,
        }
    }
}

// ── Create-table dialog ───────────────────────────────────────────────────────

struct CreateTableDialog {
    open: bool,
    table_name: String,
    columns: Vec<ColumnDef>,
    error: Option<String>,
}

impl Default for CreateTableDialog {
    fn default() -> Self {
        Self {
            open: false,
            table_name: String::new(),
            columns: vec![
                ColumnDef {
                    name: "id".to_string(),
                    col_type: ColType::Integer,
                    primary_key: true,
                    not_null: true,
                },
                ColumnDef::default(),
            ],
            error: None,
        }
    }
}

impl CreateTableDialog {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn to_sql(&self) -> Result<String, String> {
        if self.table_name.trim().is_empty() {
            return Err("Table name is required.".to_string());
        }

        // First col is always the locked autoincrement id
        let mut col_defs = vec!["id INTEGER PRIMARY KEY AUTOINCREMENT".to_string()];

        let rest: Vec<String> = self
            .columns
            .iter()
            .skip(1) // skip the locked id row
            .filter(|c| !c.name.trim().is_empty())
            .map(|c| {
                let mut def = format!("{} {}", c.name.trim(), c.col_type.label());
                if c.primary_key {
                    def.push_str(" PRIMARY KEY");
                }
                if c.not_null && !c.primary_key {
                    def.push_str(" NOT NULL");
                }
                def
            })
            .collect();

        col_defs.extend(rest);

        Ok(format!(
            "CREATE TABLE {} ({})",
            self.table_name.trim(),
            col_defs.join(", ")
        ))
    }
}

// ── Cell popover ──────────────────────────────────────────────────────────────

struct CellPopover {
    open: bool,
    row_idx: usize,
    col_idx: usize,
    buffer: String,
    pos: egui::Pos2,
}

// ── Table view (loaded data + edit state) ────────────────────────────────────

#[derive(Clone, PartialEq)]
enum SortDir {
    Asc,
    Desc,
}

struct TableView {
    table_name: String,
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    error: Option<String>,
    editing_cell: Option<(usize, usize)>,
    edit_buffer: String,
    new_row: Vec<String>,
    new_row_error: Option<String>,
    sort_col: Option<usize>,
    sort_dir: SortDir,
}

impl TableView {
    fn load(conn: &Connection, table_name: &str) -> Self {
        let mut view = Self {
            table_name: table_name.to_string(),
            columns: vec![],
            rows: vec![],
            error: None,
            editing_cell: None,
            edit_buffer: String::new(),
            new_row: vec![],
            new_row_error: None,
            sort_col: None,
            sort_dir: SortDir::Asc,
        };

        match conn.prepare(&format!("PRAGMA table_info({})", table_name)) {
            Err(e) => {
                view.error = Some(format!("Failed to load schema: {e}"));
                return view;
            }
            Ok(mut stmt) => {
                view.columns = stmt
                    .query_map([], |row| row.get::<_, String>(1))
                    .unwrap()
                    .filter_map(|r| r.ok())
                    .collect();
            }
        }

        view.new_row = vec![String::new(); view.columns.len()];
        view.reload_rows(conn);
        view
    }

    fn reload_rows(&mut self, conn: &Connection) {
        self.rows.clear();
        let col_count = self.columns.len();
        let sql = format!("SELECT * FROM {} LIMIT 1000", self.table_name);
        match conn.prepare(&sql) {
            Err(e) => {
                self.error = Some(format!("Failed to query table: {e}"));
            }
            Ok(mut stmt) => {
                self.rows = stmt
                    .query_map([], |row| {
                        let cells = (0..col_count)
                            .map(|i| {
                                row.get_ref(i)
                                    .map(|v| match v {
                                        rusqlite::types::ValueRef::Null => String::new(),
                                        rusqlite::types::ValueRef::Integer(n) => n.to_string(),
                                        rusqlite::types::ValueRef::Real(f) => f.to_string(),
                                        rusqlite::types::ValueRef::Text(t) => {
                                            String::from_utf8_lossy(t).to_string()
                                        }
                                        rusqlite::types::ValueRef::Blob(_) => "[BLOB]".to_string(),
                                    })
                                    .unwrap_or_else(|_| "ERR".to_string())
                            })
                            .collect();
                        Ok(cells)
                    })
                    .unwrap()
                    .filter_map(|r| r.ok())
                    .collect();
            }
        }
    }

    fn apply_sort(&mut self) {
        if let Some(col) = self.sort_col {
            let dir = self.sort_dir.clone();
            self.rows.sort_by(|a, b| {
                let av = a.get(col).map(|s| s.as_str()).unwrap_or("");
                let bv = b.get(col).map(|s| s.as_str()).unwrap_or("");
                // try numeric sort first
                match (av.parse::<f64>(), bv.parse::<f64>()) {
                    (Ok(an), Ok(bn)) => {
                        let ord = an.partial_cmp(&bn).unwrap_or(std::cmp::Ordering::Equal);
                        if dir == SortDir::Asc { ord } else { ord.reverse() }
                    }
                    _ => {
                        let ord = av.cmp(bv);
                        if dir == SortDir::Asc { ord } else { ord.reverse() }
                    }
                }
            });
        }
    }
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
        if self.create_dialog.open {
            egui::Window::new("Create Table")
                .collapsible(false)
                .resizable(true)
                .min_width(500.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Table name:");
                        ui.text_edit_singleline(&mut self.create_dialog.table_name);
                    });
                    ui.add_space(8.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Column Name").strong());
                        ui.add_space(80.0);
                        ui.label(egui::RichText::new("Type").strong());
                        ui.add_space(40.0);
                        ui.label(egui::RichText::new("PK").strong());
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("Not Null").strong());
                    });
                    ui.separator();
                    let mut to_delete: Option<usize> = None;
                    for (i, col) in self.create_dialog.columns.iter_mut().enumerate() {
                        let is_id = i == 0;
                        ui.horizontal(|ui| {
                            if is_id {
                                ui.add_enabled(
                                    false,
                                    egui::TextEdit::singleline(&mut col.name).desired_width(140.0),
                                );
                                ui.add_enabled(
                                    false,
                                    egui::Button::new("INTEGER  •  AUTOINCREMENT  •  PK"),
                                );
                            } else {
                                ui.add(
                                    egui::TextEdit::singleline(&mut col.name).desired_width(140.0),
                                );
                                egui::ComboBox::from_id_salt(format!("col_type_{i}"))
                                    .selected_text(col.col_type.label())
                                    .width(90.0)
                                    .show_ui(ui, |ui| {
                                        for t in ColType::all() {
                                            ui.selectable_value(
                                                &mut col.col_type,
                                                t.clone(),
                                                t.label(),
                                            );
                                        }
                                    });
                                ui.checkbox(&mut col.primary_key, "");
                                ui.add_space(16.0);
                                ui.checkbox(&mut col.not_null, "");
                                ui.add_space(16.0);
                                if ui.small_button("✕").clicked() {
                                    to_delete = Some(i);
                                }
                            }
                        });
                    }
                    if let Some(i) = to_delete {
                        self.create_dialog.columns.remove(i);
                    }
                    if let Some(i) = to_delete {
                        self.create_dialog.columns.remove(i);
                    }
                    ui.add_space(4.0);
                    if ui.button("+ Add column").clicked() {
                        self.create_dialog.columns.push(ColumnDef::default());
                    }
                    ui.add_space(8.0);
                    ui.separator();
                    if let Ok(sql) = self.create_dialog.to_sql() {
                        ui.label(egui::RichText::new(&sql).monospace().weak());
                    }
                    if let Some(err) = self.create_dialog.error.clone() {
                        ui.colored_label(egui::Color32::RED, err);
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() {
                            self.create_table();
                        }
                        if ui.button("Cancel").clicked() {
                            self.create_dialog.reset();
                        }
                    });
                });
        }

        // ── Cell popover editor ───────────────────────────────────────────────
        // Rendered at the top level so it floats over the table cleanly.
        let mut popover_commit = false;
        let mut popover_cancel = false;

        if let Some(popover) = &mut self.cell_popover {
            let mut win_open = popover.open;
            egui::Window::new("Edit Cell")
                .open(&mut win_open)
                .default_pos(popover.pos)  // moveable — default_pos not fixed_pos
                .default_size([320.0, 240.0])
                .min_size([200.0, 120.0])
                .collapsible(false)
                .resizable(true)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(160.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut popover.buffer)
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(6),
                            );
                        });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            popover_commit = true;
                        }
                        if ui.button("Cancel").clicked() {
                            popover_cancel = true;
                        }
                        ui.weak("Ctrl+Enter  •  Esc to cancel");
                    });
                    if ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Enter)) {
                        popover_commit = true;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        popover_cancel = true;
                    }
                });
            if !win_open {
                popover_cancel = true;
            }
        }

        if popover_commit {
            // Inject into table_view edit state then reuse commit_edit()
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
        if self.conn.is_some() {
            egui::SidePanel::left("table_list")
                .resizable(true)
                .default_width(200.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading("Tables");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("⟳").clicked() {
                                self.refresh_tables();
                            }
                            if ui.small_button("+").clicked() {
                                self.create_dialog.open = true;
                            }
                        });
                    });
                    ui.separator();
                    if self.tables.is_empty() {
                        ui.label("No tables yet.");
                    } else {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            let tables = self.tables.clone();
                            for table in &tables {
                                let selected =
                                    self.selected_table.as_deref() == Some(table.as_str());
                                if ui.selectable_label(selected, table).clicked() {
                                    self.select_table(table);
                                }
                            }
                        });
                    }
                });
        }

        // Main panel
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.conn.is_none() {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading("No database open");
                        ui.add_space(8.0);
                        if ui.button("Open database…").clicked() {
                            if let Some(path) = FileDialog::new()
                                .add_filter("SQLite", &["db", "sqlite", "sqlite3"])
                                .pick_file()
                            {
                                self.open_db(path);
                            }
                        }
                        if ui.button("New database…").clicked() {
                            if let Some(path) = FileDialog::new()
                                .add_filter("SQLite", &["db", "sqlite", "sqlite3"])
                                .set_file_name("new_database.db")
                                .save_file()
                            {
                                self.open_db(path);
                            }
                        }
                    });
                });
                return;
            }

            if self.selected_table.is_none() {
                ui.centered_and_justified(|ui| {
                    ui.label("Select a table from the sidebar.");
                });
                return;
            }

            // Clone everything needed before any mutable borrows
            let (table_name, columns, rows, error, editing_cell, new_row, new_row_error, sort_col, sort_dir) =
                match &self.table_view {
                    None => return,
                    Some(v) => (
                        v.table_name.clone(),
                        v.columns.clone(),
                        v.rows.clone(),
                        v.error.clone(),
                        v.editing_cell,
                        v.new_row.clone(),
                        v.new_row_error.clone(),
                        v.sort_col,
                        v.sort_dir.clone(),
                    ),
                };

            if let Some(err) = error {
                ui.colored_label(egui::Color32::RED, err);
                return;
            }

            // Toolbar
            ui.horizontal(|ui| {
                ui.heading(&table_name);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("⟳ Refresh").clicked() {
                        if let Some(conn) = &self.conn {
                            if let Some(view) = &mut self.table_view {
                                view.reload_rows(conn);
                            }
                        }
                    }
                });
            });
            ui.separator();

            // Deferred action flags — collected during table render, applied after
            let mut do_commit_edit = false;
            let mut do_cancel_edit = false;
            let mut new_editing_cell: Option<Option<(usize, usize)>> = None;
            let mut new_edit_buffer: Option<String> = None;
            let mut do_commit_insert = false;
            let mut new_row_updates: Vec<(usize, String)> = vec![];
            let mut new_popover: Option<CellPopover> = None;
            let mut sort_click: Option<usize> = None;

            egui::ScrollArea::both().show(ui, |ui| {
                TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::exact(36.0))
                    .columns(Column::auto().resizable(true).clip(true), columns.len())
                    .header(24.0, |mut header| {
                        header.col(|ui| {
                            ui.weak("#");
                        });
                        for (col_idx, col_name) in columns.iter().enumerate() {
                            header.col(|ui| {
                                let arrow = match sort_col {
                                    Some(i) if i == col_idx => {
                                        if sort_dir == SortDir::Asc { " ▲" } else { " ▼" }
                                    }
                                    _ => "",
                                };
                                let label = format!("{}{}", col_name, arrow);
                                if ui.button(egui::RichText::new(label).strong()).clicked() {
                                    sort_click = Some(col_idx);
                                }
                            });
                        }
                    })
                    .body(|body| {
                        let total_rows = rows.len() + 1;
                        body.rows(24.0, total_rows, |mut row| {
                            let row_idx = row.index();

                            // ── New-row insert strip ──────────────────────
                            if row_idx == rows.len() {
                                row.col(|ui| {
                                    ui.weak("*");
                                });
                                for (col_idx, val) in new_row.iter().enumerate() {
                                    row.col(|ui| {
                                        let mut buf = val.clone();
                                        let response = ui.add(
                                            egui::TextEdit::singleline(&mut buf)
                                                .desired_width(f32::INFINITY)
                                                .hint_text("…"),
                                        );
                                        if buf != *val {
                                            new_row_updates.push((col_idx, buf));
                                        }
                                        if response.lost_focus()
                                            && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                        {
                                            do_commit_insert = true;
                                        }
                                    });
                                }
                                return;
                            }

                            // ── Existing rows ─────────────────────────────
                            row.col(|ui| {
                                ui.weak((row_idx + 1).to_string());
                            });

                            for (col_idx, cell) in rows[row_idx].iter().enumerate() {
                                row.col(|ui| {
                                    let is_editing = editing_cell == Some((row_idx, col_idx));

                                    if is_editing {
                                        let mut buf = match &new_edit_buffer {
                                            Some(b) => b.clone(),
                                            None => {
                                                if let Some(view) = self.table_view.as_ref() {
                                                    view.edit_buffer.clone()
                                                } else {
                                                    cell.clone()
                                                }
                                            }
                                        };

                                        let response = ui.add(
                                            egui::TextEdit::singleline(&mut buf)
                                                .desired_width(f32::INFINITY),
                                        );
                                        response.request_focus();
                                        new_edit_buffer = Some(buf);

                                        let enter =
                                            ui.input(|i| i.key_pressed(egui::Key::Enter));
                                        let escape =
                                            ui.input(|i| i.key_pressed(egui::Key::Escape));

                                        if enter {
                                            do_commit_edit = true;
                                        } else if escape {
                                            do_cancel_edit = true;
                                        } else if response.lost_focus() {
                                            do_commit_edit = true;
                                        }
                                    } else {
                                        // ── Display cell ──────────────────
                                        let is_long = cell.len() > 60;
                                        let display = if is_long {
                                            format!("{}…", &cell[..60])
                                        } else {
                                            cell.clone()
                                        };

                                    let response = ui.label(&display);
                                    let rect_min = response.rect.min;
                                    let double_clicked = response.double_clicked();

                                    if double_clicked {
                                            if is_long {
                                                // Long cell → floating multiline popover
                                                new_popover = Some(CellPopover {
                                                    open: true,
                                                    row_idx,
                                                    col_idx,
                                                    buffer: cell.clone(),
                                                    pos: rect_min,
                                                });
                                            } else {
                                                // Short cell → inline single-line edit
                                                new_editing_cell =
                                                    Some(Some((row_idx, col_idx)));
                                                new_edit_buffer = Some(cell.clone());
                                            }
                                        }
                                    }
                                });
                            }
                        });
                    });
            }); // end ScrollArea

            // ── Below-table controls ──────────────────────────────────────
            if let Some(err) = new_row_error {
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
        });
    }
}
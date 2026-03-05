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
        let col_defs: Vec<String> = self
            .columns
            .iter()
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

        if col_defs.is_empty() {
            return Err("At least one column is required.".to_string());
        }

        Ok(format!(
            "CREATE TABLE {} ({})",
            self.table_name.trim(),
            col_defs.join(", ")
        ))
    }
}

// Holds a loaded table's data for display
struct TableView {
    table_name: String,
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    error: Option<String>,
}

impl TableView {
    fn load(conn: &Connection, table_name: &str) -> Self {
        let mut view = Self {
            table_name: table_name.to_string(),
            columns: vec![],
            rows: vec![],
            error: None,
        };

        // Get column names via PRAGMA
        match conn.prepare(&format!("PRAGMA table_info({})", table_name)) {
            Err(e) => {
                view.error = Some(format!("Failed to load schema: {e}"));
                return view;
            }
            Ok(mut stmt) => {
                view.columns = stmt
                    .query_map([], |row| row.get::<_, String>(1)) // col 1 = name
                    .unwrap()
                    .filter_map(|r| r.ok())
                    .collect();
            }
        }

        // Load rows — everything as string for display
        let col_count = view.columns.len();
        let sql = format!("SELECT * FROM {} LIMIT 1000", table_name);
        match conn.prepare(&sql) {
            Err(e) => {
                view.error = Some(format!("Failed to query table: {e}"));
            }
            Ok(mut stmt) => {
                let rows: Vec<Vec<String>> = stmt
                    .query_map([], |row| {
                        let cells = (0..col_count)
                            .map(|i| {
                                row.get_ref(i)
                                    .map(|v| match v {
                                        rusqlite::types::ValueRef::Null => "NULL".to_string(),
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
                view.rows = rows;
            }
        }

        view
    }
}

struct App {
    conn: Option<Connection>,
    db_path: Option<String>,
    status: String,
    tables: Vec<String>,
    selected_table: Option<String>,
    create_dialog: CreateTableDialog,
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
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .unwrap();
            let names: Vec<String> = stmt
                .query_map([], |row| row.get(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect();
            self.tables = names;
        }
    }

    fn select_table(&mut self, name: &str) {
        self.selected_table = Some(name.to_string());
        if let Some(conn) = &self.conn {
            self.table_view = Some(TableView::load(conn, name));
            self.status = format!(
                "Table: {} — {} rows",
                name,
                self.table_view.as_ref().map(|v| v.rows.len()).unwrap_or(0)
            );
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
}

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
            ui.horizontal(|ui| {
                ui.label(&self.status);
            });
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
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut col.name).desired_width(140.0));
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
                        });
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
                    if let Some(err) = &self.create_dialog.error.clone() {
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

        // Left sidebar
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
                            // collect first to avoid borrow conflict
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

            match &self.selected_table {
                None => {
                    ui.centered_and_justified(|ui| {
                        ui.label("Select a table from the sidebar.");
                    });
                }
                Some(_) => {
                    // Clone out what we need so we don't hold a borrow into self
                    let (table_name, columns, rows, error) = match &self.table_view {
                        None => return,
                        Some(view) => (
                            view.table_name.clone(),
                            view.columns.clone(),
                            view.rows.clone(),
                            view.error.clone(),
                        ),
                    };

                    if let Some(err) = &error {
                        ui.colored_label(egui::Color32::RED, err);
                        return;
                    }

                    ui.horizontal(|ui| {
                        ui.heading(&table_name);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("⟳ Refresh").clicked() {
                                if let Some(conn) = &self.conn {
                                    self.table_view = Some(TableView::load(conn, &table_name));
                                }
                            }
                        });
                    });
                    ui.separator();

                    egui::ScrollArea::horizontal().show(ui, |ui| {
                        TableBuilder::new(ui)
                            .striped(true)
                            .resizable(true)
                            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                            .columns(Column::auto().resizable(true), columns.len())
                            .header(24.0, |mut header| {
                                for col_name in &columns {
                                    header.col(|ui| {
                                        ui.strong(col_name);
                                    });
                                }
                            })
                            .body(|body| {
                                body.rows(20.0, rows.len(), |mut row| {
                                    let row_data = &rows[row.index()];
                                    for cell in row_data {
                                        row.col(|ui| {
                                            ui.label(cell);
                                        });
                                    }
                                });
                            });
                    });
                }
            }
        });
    }
}
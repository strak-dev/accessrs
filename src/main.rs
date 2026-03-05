#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
mod ui;
mod easy_mark;

use db::schema::SortDir;
use db::table_view::TableView;
use ui::create_dialog::CreateTableDialog;
use ui::popover::{CellPopover, PopoverMode};

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
            let (table_name, columns, rows, error, editing_cell, new_row, new_row_error, sort_col, sort_dir, date_columns, note_columns, foreign_keys, highlighted_row) =
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
                        v.date_columns.clone(),
                        v.note_columns.clone(),
                        v.foreign_keys.clone(),
                        v.highlighted_row,
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
                    if ui.button("↺ Refresh").clicked() {
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
            let mut navigate_to: Option<(String, String)> = None; // (table, id value)

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
                                        let is_fk = foreign_keys.iter().find(|fk| fk.col_idx == col_idx);
                                        let is_highlighted = highlighted_row == Some(row_idx);

                                        let is_note = note_columns.contains(&col_idx);
                                        let display = if is_note {
                                            // show first line only with a note icon
                                            let first_line = cell.lines().next().unwrap_or("").trim();
                                            if first_line.is_empty() {
                                                " Empty note".to_string()
                                            } else if first_line.len() > 50 {
                                                format!(" {}…", &first_line[..50])
                                            } else {
                                                format!(" {}", first_line)
                                            }
                                        } else if is_long {
                                            format!("{}…", &cell[..60])
                                        } else {
                                            cell.clone()
                                        };

                                        // highlight the navigated-to row
                                        if is_highlighted {
                                            ui.painter().rect_filled(
                                                ui.available_rect_before_wrap(),
                                                0.0,
                                                egui::Color32::from_rgba_unmultiplied(255, 255, 0, 5),
                                            );
                                        }

                                        let response = if let Some(fk) = is_fk {
                                            // FK cell — render as a clickable link
                                            let link = ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(format!("→ {}", display))
                                                        .color(egui::Color32::from_rgb(100, 160, 255))
                                                        .underline(),
                                                )
                                                .sense(egui::Sense::click()),
                                            );
                                            if link.clicked() {
                                                navigate_to = Some((fk.ref_table.clone(), cell.clone()));
                                            }
                                            link
                                        } else {
                                            ui.label(&display)
                                        };
                                    let rect_min = response.rect.min;
                                    let double_clicked = response.double_clicked();

                                        if double_clicked {
                                            let is_date = date_columns.contains(&col_idx);
                                            let is_note = note_columns.contains(&col_idx);
                                            if is_note {
                                                new_popover = Some(CellPopover {
                                                    open: true,
                                                    row_idx,
                                                    col_idx,
                                                    buffer: cell.clone(),
                                                    pos: rect_min,
                                                    mode: PopoverMode::Note { editing: false },
                                                });
                                            } else if is_date {
                                                let parsed = chrono::NaiveDate::parse_from_str(cell, "%Y-%m-%d")
                                                    .unwrap_or_else(|_| chrono::Local::now().date_naive());
                                                new_popover = Some(CellPopover {
                                                    open: true,
                                                    row_idx,
                                                    col_idx,
                                                    buffer: cell.clone(),
                                                    pos: rect_min,
                                                    mode: PopoverMode::Date(parsed),
                                                });
                                            } else if is_long {
                                                new_popover = Some(CellPopover {
                                                    open: true,
                                                    row_idx,
                                                    col_idx,
                                                    buffer: cell.clone(),
                                                    pos: rect_min,
                                                    mode: PopoverMode::Text,
                                                });
                                            } else {
                                                new_editing_cell = Some(Some((row_idx, col_idx)));
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
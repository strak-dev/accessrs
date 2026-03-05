
use crate::app::App;
use crate::ui::{empty_state, sidebar, table_view, toolbar};
use eframe::egui;
use rfd::FileDialog;

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.label(&self.status);
        });

        let tables = self.tables.clone();
        if self.create_dialog.show(ctx, &tables) {
            self.create_table();
        }

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

        sidebar::show(self, ctx);

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
            toolbar::show(self, ui, &table_name);
            table_view::show(self, ui);
        });
    }
}
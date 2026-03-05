use crate::App;
use eframe::egui;
use rfd::FileDialog;

pub fn show_no_db(app: &mut App, ui: &mut egui::Ui) {
    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            ui.heading("No database open");
            ui.add_space(8.0);
            if ui.button("Open database…").clicked() {
                if let Some(path) = FileDialog::new()
                    .add_filter("SQLite", &["db", "sqlite", "sqlite3"])
                    .pick_file()
                {
                    app.open_db(path);
                }
            }
            if ui.button("New database…").clicked() {
                if let Some(path) = FileDialog::new()
                    .add_filter("SQLite", &["db", "sqlite", "sqlite3"])
                    .set_file_name("new_database.db")
                    .save_file()
                {
                    app.open_db(path);
                }
            }
        });
    });
}

pub fn show_no_table(ui: &mut egui::Ui) {
    ui.centered_and_justified(|ui| {
        ui.label("Select a table from the sidebar.");
    });
}